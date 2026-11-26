// A profile from a profile file, a `.mrpack` (specs/play-a-profile.md, From a file and Rules). Prism's own import
// always shows its window, so Outpost Tools writes the instance itself: the instance writer (`instance.rs`) makes
// `instance.cfg` and `mmc-pack.json` from the file's `dependencies`, the mods download from the `files` addresses
// with their hashes checked, and the overrides folders are copied into the game folder.
//
// Entry points: `read_info` (name, version and mods, installs nothing: the confirmation screen), `install` (a new
// profile) and `update` (a newer version of the same pack over an instance made by `install`). Nothing here starts
// Prism, Java or the game, and Prism must not be running while an instance is written. Downloads go through
// `instance::Fetch`, so tests fake them.
//
// What the pack owns is recorded in `outpost-pack.json`, in the instance folder (not the game folder), with the
// sha1 of each file as written. An update compares the disk with that record: a file still as the pack wrote it is
// replaced or removed; a file the user changed, and every file the pack does not know (added mods, worlds), stays.
//
// An update stages every download and every override before it touches the game folder, then renames the staged files
// in and removes the ones the new version drops. A failure while staging leaves the instance exactly as it was. A
// failure while replacing (a file locked by a running game, a full disk) leaves the files already replaced in place:
// the record is then rewritten to list the pack's files as they are on disk now, under the old version number, so
// running the update again finishes the work and never takes a half-replaced file for the user's.
//
// Format: https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack (read 2026-11-25).

use std::{
    collections::BTreeMap,
    fmt, fs,
    io::{self, Read, Seek},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use zip::ZipArchive;

use super::instance::{Fetch, InstanceError, NewInstance, WrittenInstance, write_instance};

const INDEX: &str = "modrinth.index.json";
const MANIFEST: &str = "outpost-pack.json";
const STAGING: &str = "outpost-staging";
// Where the format lets a file come from (Modrinth's own list for packs it hosts).
const HOSTS: &[&str] = &["cdn.modrinth.com", "github.com", "raw.githubusercontent.com", "gitlab.com"];
const MAX_ENTRY: u64 = 2 << 30;
// All the overrides together, as unpacked: a settings pack is far below it, and a zip bomb is far above.
#[cfg(not(test))]
const MAX_OVERRIDES: u64 = 1 << 30;
// Small in tests, so the one that checks the cap does not write a gigabyte.
#[cfg(test)]
const MAX_OVERRIDES: u64 = 1 << 20;

/// Why a profile file could not be read, installed or updated, each one a thing the page can say in a sentence.
#[derive(Debug)]
pub enum MrpackError {
    /// Not a zip, no `modrinth.index.json`, or an index that is not the format's.
    Unreadable(String),
    /// The pack needs a loader Outpost Tools' profiles do not run (Forge, NeoForge, Quilt).
    Unsupported(String),
    /// A path in the file would land outside the game folder.
    UnsafePath(String),
    /// A download address is not https or not on the format's list of hosts.
    BadHost(String),
    /// Every address of a mod failed: the mod and what went wrong.
    Download(String, String),
    /// A download arrived but is not the file the pack names.
    HashMismatch(String),
    /// The instance has no pack record: it was not made from a profile file.
    NotManaged(String),
    Instance(InstanceError),
    Io(io::Error),
}

impl fmt::Display for MrpackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MrpackError::Unreadable(why) => write!(f, "This profile file cannot be read: {why}"),
            MrpackError::Unsupported(l) => write!(f, "This profile needs {l}, which Outpost Tools' profiles do not run."),
            MrpackError::UnsafePath(p) => write!(f, "This profile file names a place outside its game folder: {p}"),
            MrpackError::BadHost(u) => write!(f, "This profile file downloads from a place it should not: {u}"),
            MrpackError::Download(m, why) => write!(f, "{m} could not be downloaded: {why}"),
            MrpackError::HashMismatch(m) => write!(f, "{m} downloaded, but it is not the file the profile names."),
            MrpackError::NotManaged(id) => write!(f, "The profile {id} was not made from a profile file."),
            MrpackError::Instance(e) => e.fmt(f),
            MrpackError::Io(e) => write!(f, "The profile's folder could not be written: {e}"),
        }
    }
}

impl std::error::Error for MrpackError {}

impl From<io::Error> for MrpackError {
    fn from(e: io::Error) -> Self {
        MrpackError::Io(e)
    }
}

impl From<InstanceError> for MrpackError {
    fn from(e: InstanceError) -> Self {
        MrpackError::Instance(e)
    }
}

impl From<zip::result::ZipError> for MrpackError {
    fn from(e: zip::result::ZipError) -> Self {
        MrpackError::Unreadable(e.to_string())
    }
}

/// One mod of a pack, as the confirmation screen lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackMod {
    /// Where it goes in the game folder, `mods/sodium.jar`.
    pub path: String,
    /// The file's name: the pack's index carries no other name.
    pub file_name: String,
    /// The pack marks it optional on the client. It is installed all the same (to validate).
    pub optional: bool,
    pub size: Option<u64>,
}

/// What a profile file says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackInfo {
    pub name: String,
    pub version: String,
    pub summary: Option<String>,
    pub game_version: String,
    /// The Fabric loader's version; None is the plain game.
    pub fabric_loader: Option<String>,
    /// The files that go in the profile, by path. Those the pack marks unsupported on the client are not here.
    pub mods: Vec<PackMod>,
    /// How many files the pack keeps for servers only.
    pub server_only: usize,
    /// How many settings and other files the overrides folders carry.
    pub override_files: usize,
}

/// How far a download has come: files finished out of files to fetch, and the one starting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub file: String,
}

/// What an update did, by game-folder path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateReport {
    pub written: Vec<String>,
    pub removed: Vec<String>,
    /// Files the pack wanted to change that the user had changed: left as the user has them.
    pub kept: Vec<String>,
}

// ---------- the index ----------

#[derive(Deserialize)]
struct RawIndex {
    #[serde(rename = "formatVersion")]
    format_version: u32,
    game: String,
    #[serde(rename = "versionId")]
    version_id: String,
    name: String,
    #[serde(default)]
    summary: Option<String>,
    files: Vec<RawFile>,
    dependencies: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct RawFile {
    path: String,
    hashes: BTreeMap<String, String>,
    #[serde(default)]
    env: Option<RawEnv>,
    downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    file_size: Option<u64>,
}

#[derive(Deserialize)]
struct RawEnv {
    #[serde(default)]
    client: Option<String>,
    #[serde(default)]
    server: Option<String>,
}

struct PackFile {
    path: String,
    sha1: String,
    sha512: Option<String>,
    downloads: Vec<String>,
}

struct Pack {
    info: PackInfo,
    files: Vec<PackFile>,
}

/// A path as the format wants it: relative, `/`-separated, nothing that climbs out, names a drive, or that Windows
/// would read as a device (con, nul, com1 ...) or write under another name (a trailing dot or space).
fn safe_path(path: &str) -> Result<String, MrpackError> {
    let bad = || MrpackError::UnsafePath(path.to_string());
    if path.is_empty() || path.contains(['\\', ':', '<', '>', '"', '|', '?', '*']) || path.starts_with('/') {
        return Err(bad());
    }
    if path.chars().any(char::is_control) {
        return Err(bad());
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.ends_with(['.', ' ']) {
            return Err(bad());
        }
        // The device name counts with any extension: con.txt is still the console.
        let stem = segment.split('.').next().unwrap_or_default().trim_end().to_ascii_lowercase();
        let numbered = stem.len() == 4 && (stem.starts_with("com") || stem.starts_with("lpt")) && matches!(stem.as_bytes()[3], b'1'..=b'9');
        if matches!(stem.as_str(), "con" | "prn" | "aux" | "nul" | "conin$" | "conout$") || numbered {
            return Err(bad());
        }
    }
    Ok(path.to_string())
}

fn check_host(url: &str) -> Result<(), MrpackError> {
    let rest = url.strip_prefix("https://").ok_or_else(|| MrpackError::BadHost(url.to_string()))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default().to_ascii_lowercase();
    if HOSTS.contains(&host.as_str()) { Ok(()) } else { Err(MrpackError::BadHost(url.to_string())) }
}

fn read_entry<R: Read + Seek>(archive: &mut ZipArchive<R>, index: usize) -> Result<Vec<u8>, MrpackError> {
    let mut entry = archive.by_index(index)?;
    if entry.size() > MAX_ENTRY {
        return Err(MrpackError::Unreadable(format!("{} is too large", entry.name())));
    }
    let mut bytes = Vec::new();
    // The declared size can lie: the read itself is capped.
    (&mut entry).take(MAX_ENTRY + 1).read_to_end(&mut bytes).map_err(|e| MrpackError::Unreadable(e.to_string()))?;
    if bytes.len() as u64 > MAX_ENTRY {
        return Err(MrpackError::Unreadable(format!("{} is too large", entry.name())));
    }
    Ok(bytes)
}

/// The override files the client gets, by game-folder path to the zip entry's index: `overrides/` first, then
/// `client-overrides/` on top. `server-overrides/` is not for the client.
fn override_entries<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<BTreeMap<String, usize>, MrpackError> {
    let mut common = BTreeMap::new();
    let mut client = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i)?;
        let name = entry.name().to_string();
        if entry.is_dir() || name.ends_with('/') {
            continue;
        }
        let kept = if let Some(rest) = name.strip_prefix("overrides/") {
            common.insert(safe_path(rest)?, i);
            true
        } else if let Some(rest) = name.strip_prefix("client-overrides/") {
            client.insert(safe_path(rest)?, i);
            true
        } else {
            false
        };
        if kept {
            total = total.saturating_add(entry.size());
            if total > MAX_OVERRIDES {
                return Err(MrpackError::Unreadable("its settings and other files unpack to more than 1 GB".into()));
            }
        }
    }
    common.extend(client);
    Ok(common)
}

fn read_pack<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<Pack, MrpackError> {
    let unreadable = |why: String| MrpackError::Unreadable(why);
    let index = archive.index_for_name(INDEX).ok_or_else(|| unreadable(format!("it has no {INDEX}")))?;
    let bytes = read_entry(archive, index)?;
    let raw: RawIndex = serde_json::from_slice(super::key_value::strip_bom(&bytes)).map_err(|e| unreadable(format!("{INDEX}: {e}")))?;
    if raw.format_version != 1 || raw.game != "minecraft" {
        return Err(unreadable(format!("format {} for {}", raw.format_version, raw.game)));
    }
    for (key, name) in [("forge", "Forge"), ("neoforge", "NeoForge"), ("quilt-loader", "Quilt")] {
        if raw.dependencies.contains_key(key) {
            return Err(MrpackError::Unsupported(name.to_string()));
        }
    }
    let game_version = raw.dependencies.get("minecraft").ok_or_else(|| unreadable("it names no game version".into()))?.clone();
    let fabric_loader = raw.dependencies.get("fabric-loader").cloned();
    // These end up in files and in Prism's JSON.
    for v in std::iter::once(&game_version).chain(fabric_loader.iter()) {
        if v.is_empty() || !v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+')) {
            return Err(unreadable(format!("{v:?} is not a version")));
        }
    }

    let mut files = Vec::new();
    let mut mods = Vec::new();
    let mut server_only = 0;
    for f in raw.files {
        let path = safe_path(&f.path)?;
        let (client, server) = f.env.map(|e| (e.client, e.server)).unwrap_or_default();
        // A missing env means required on both sides.
        if client.as_deref() == Some("unsupported") {
            if server.as_deref() != Some("unsupported") {
                server_only += 1;
            }
            continue;
        }
        for url in &f.downloads {
            check_host(url)?;
        }
        let sha1 = f.hashes.get("sha1").ok_or_else(|| unreadable(format!("{path} has no sha1")))?.to_ascii_lowercase();
        let file_name = path.rsplit('/').next().unwrap_or(&path).to_string();
        mods.push(PackMod { path: path.clone(), file_name, optional: client.as_deref() == Some("optional"), size: f.file_size });
        files.push(PackFile { path, sha1, sha512: f.hashes.get("sha512").map(|h| h.to_ascii_lowercase()), downloads: f.downloads });
    }
    mods.sort_by(|a, b| a.path.cmp(&b.path));
    let override_files = override_entries(archive)?.len();
    let info = PackInfo { name: raw.name, version: raw.version_id, summary: raw.summary, game_version, fabric_loader, mods, server_only, override_files };
    Ok(Pack { info, files })
}

/// Reads a profile file's name, version and mods, and installs nothing.
pub fn read_info(file: &Path) -> Result<PackInfo, MrpackError> {
    read_info_from(fs::File::open(file)?)
}

/// `read_info` over any reader: the bytes of a file already in memory, for instance.
pub fn read_info_from<R: Read + Seek>(reader: R) -> Result<PackInfo, MrpackError> {
    Ok(read_pack(&mut ZipArchive::new(reader)?)?.info)
}

// ---------- what the pack owns ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Owned {
    path: String,
    sha1: String,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    format_version: u32,
    pack_name: String,
    version_id: String,
    game_version: String,
    fabric_loader: Option<String>,
    files: Vec<Owned>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha1_hex(bytes: &[u8]) -> String {
    hex(&Sha1::digest(bytes))
}

fn sha1_of_file(path: &Path) -> io::Result<Option<String>> {
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut hasher = Sha1::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(Some(hex(&hasher.finalize())))
}

// Worlds are the user's: an update writes there only where nothing exists, and never removes.
fn protected(path: &str) -> bool {
    path.starts_with("saves/")
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Write,
    /// Already as the pack wants it.
    Same,
    /// The user changed it and the pack wants to change it too: left alone, and reported.
    KeepUser,
    /// The user removed it and the pack has not changed it: not brought back.
    StayRemoved,
}

// `disk` is the sha1 of what is in the game folder now, `old` what the pack wrote last time, `new` what it wants.
fn decide(path: &str, disk: Option<&str>, old: Option<&str>, new: &str) -> Action {
    match disk {
        None if old == Some(new) => Action::StayRemoved,
        None => Action::Write,
        Some(d) if d == new => Action::Same,
        // The pack did not change it: whatever the user made of it is theirs, and not worth a word.
        Some(_) if old == Some(new) => Action::Same,
        Some(_) if protected(path) => Action::KeepUser,
        Some(d) if Some(d) == old => Action::Write,
        Some(_) => Action::KeepUser,
    }
}

fn fetch_checked(fetch: &dyn Fetch, file: &PackFile) -> Result<Vec<u8>, MrpackError> {
    let mut last = MrpackError::Download(file.path.clone(), "the pack gives no address".into());
    for url in &file.downloads {
        match fetch.get(url) {
            Ok(bytes) => {
                let sha512_ok = file.sha512.as_ref().is_none_or(|h| *h == hex(&Sha512::digest(&bytes)));
                if sha1_hex(&bytes) == file.sha1 && sha512_ok {
                    return Ok(bytes);
                }
                last = MrpackError::HashMismatch(file.path.clone());
            }
            Err(e) => last = MrpackError::Download(file.path.clone(), e.to_string()),
        }
    }
    Err(last)
}

fn prune_empty(game_dir: &Path, mut dir: &Path) {
    while dir != game_dir && dir.starts_with(game_dir) && fs::remove_dir(dir).is_ok() {
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
}

/// What `apply` did. `failed` is set when the replacing stopped midway: `owned` is then what the pack meant to own,
/// and the game folder holds some of it.
struct Applied {
    owned: Vec<Owned>,
    report: UpdateReport,
    failed: Option<MrpackError>,
}

// Brings the game folder to what the pack says, given what the pack wrote last time. Every download and override is
// staged first and checked; only then are files renamed in and dropped ones removed. An Err means nothing was
// touched; `Applied::failed` means the replacing started and stopped.
fn apply<R: Read + Seek>(
    dir: &Path,
    game_dir: &Path,
    old: &[Owned],
    pack: &Pack,
    archive: &mut ZipArchive<R>,
    fetch: &dyn Fetch,
    progress: &mut dyn FnMut(&Progress),
) -> Result<Applied, MrpackError> {
    let old: BTreeMap<&str, &str> = old.iter().map(|o| (o.path.as_str(), o.sha1.as_str())).collect();
    let overrides = override_entries(archive)?;
    let mut report = UpdateReport::default();
    let mut owned: BTreeMap<String, String> = BTreeMap::new();

    // What to fetch: the pack's files, but for those an override replaces anyway.
    let mut to_fetch = Vec::new();
    for f in pack.files.iter().filter(|f| !overrides.contains_key(&f.path)) {
        owned.insert(f.path.clone(), f.sha1.clone());
        let disk = sha1_of_file(&game_dir.join(&f.path))?;
        match decide(&f.path, disk.as_deref(), old.get(f.path.as_str()).copied(), &f.sha1) {
            Action::Write => to_fetch.push(f),
            Action::KeepUser => report.kept.push(f.path.clone()),
            Action::Same | Action::StayRemoved => {}
        }
    }

    let staging = dir.join(STAGING);
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    // Everything the new version brings goes into the staging folder, downloads and overrides alike.
    let staged = (|| -> Result<Vec<(String, PathBuf)>, MrpackError> {
        let mut staged = Vec::new();
        for (n, f) in to_fetch.iter().enumerate() {
            progress(&Progress { done: n, total: to_fetch.len(), file: f.path.clone() });
            let bytes = fetch_checked(fetch, f)?;
            let part = staging.join(n.to_string());
            fs::write(&part, bytes)?;
            staged.push((f.path.clone(), part));
        }
        progress(&Progress { done: to_fetch.len(), total: to_fetch.len(), file: String::new() });
        let mut unpacked = 0u64;
        for (path, &index) in &overrides {
            let bytes = read_entry(archive, index)?;
            unpacked += bytes.len() as u64;
            if unpacked > MAX_OVERRIDES {
                return Err(MrpackError::Unreadable("its settings and other files unpack to more than 1 GB".into()));
            }
            let new = sha1_hex(&bytes);
            owned.insert(path.clone(), new.clone());
            let disk = sha1_of_file(&game_dir.join(path))?;
            match decide(path, disk.as_deref(), old.get(path.as_str()).copied(), &new) {
                Action::Write => {
                    let part = staging.join(format!("o{}", staged.len()));
                    fs::write(&part, bytes)?;
                    staged.push((path.clone(), part));
                }
                Action::KeepUser => report.kept.push(path.clone()),
                Action::Same | Action::StayRemoved => {}
            }
        }
        Ok(staged)
    })();
    let staged = match staged {
        Ok(s) => s,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };

    // The pack's own files that the new version drops: removed when still as it wrote them.
    let mut to_remove = Vec::new();
    for (path, recorded) in &old {
        if owned.contains_key(*path) || protected(path) {
            continue;
        }
        match sha1_of_file(&game_dir.join(path)) {
            Ok(Some(d)) if d == *recorded => to_remove.push(path.to_string()),
            Ok(Some(_)) => report.kept.push(path.to_string()),
            Ok(None) => {}
            Err(e) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(e.into());
            }
        }
    }

    // The replacing: only renames and removals now.
    let replaced = (|| -> Result<(), MrpackError> {
        for (path, part) in &staged {
            let file = game_dir.join(path);
            if let Some(parent) = file.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(part, &file)?;
            report.written.push(path.clone());
        }
        for path in &to_remove {
            let file = game_dir.join(path);
            fs::remove_file(&file)?;
            report.removed.push(path.clone());
            if let Some(parent) = file.parent() {
                prune_empty(game_dir, parent);
            }
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    let owned = owned.into_iter().map(|(path, sha1)| Owned { path, sha1 }).collect();
    Ok(Applied { owned, report, failed: replaced.err() })
}

// The pack's files as they are on disk now: each recorded path whose content is one of the hashes the pack wrote
// (the old version's or the new one's). Anything else on disk is the user's, and is not listed.
fn owned_on_disk(game_dir: &Path, records: &[Owned]) -> Vec<Owned> {
    let mut found = BTreeMap::new();
    for record in records {
        if let Ok(Some(d)) = sha1_of_file(&game_dir.join(&record.path)) {
            if d == record.sha1 {
                found.insert(record.path.clone(), d);
            }
        }
    }
    found.into_iter().map(|(path, sha1)| Owned { path, sha1 }).collect()
}

fn write_manifest(dir: &Path, info: &PackInfo, files: Vec<Owned>) -> Result<(), MrpackError> {
    save_manifest(
        dir,
        &Manifest {
            format_version: 1,
            pack_name: info.name.clone(),
            version_id: info.version.clone(),
            game_version: info.game_version.clone(),
            fabric_loader: info.fabric_loader.clone(),
            files,
        },
    )
}

// Written beside and renamed in, so a crash leaves the whole old record or the whole new one.
fn save_manifest(dir: &Path, manifest: &Manifest) -> Result<(), MrpackError> {
    let text = serde_json::to_string_pretty(manifest).expect("a manifest always serializes");
    let part = dir.join(format!("{MANIFEST}.tmp"));
    fs::write(&part, text)?;
    if let Err(e) = fs::rename(&part, dir.join(MANIFEST)) {
        let _ = fs::remove_file(&part);
        return Err(e.into());
    }
    Ok(())
}

/// Makes a new profile from a profile file: the instance's files, the mods downloaded and checked, the overrides
/// copied. `instances` is Prism's `instances` folder; the profile is named as the pack is, with `-2` after its
/// folder when taken. A failure removes the folder it made. `progress` hears each mod as it starts.
pub fn install(instances: &Path, file: &Path, fetch: &dyn Fetch, progress: &mut dyn FnMut(&Progress)) -> Result<WrittenInstance, MrpackError> {
    let mut archive = ZipArchive::new(fs::File::open(file)?)?;
    let pack = read_pack(&mut archive)?;
    let info = &pack.info;
    let written = write_instance(instances, &NewInstance::new(&info.name, &info.game_version, info.fabric_loader.as_deref()))?;
    let done = apply(&written.dir, &written.game_dir, &[], &pack, &mut archive, fetch, progress).and_then(|applied| match applied.failed {
        Some(e) => Err(e),
        None => write_manifest(&written.dir, info, applied.owned),
    });
    if let Err(e) = done {
        // The folder was made a moment ago by us: nothing of the user's is in it.
        let _ = fs::remove_dir_all(&written.dir);
        return Err(e);
    }
    Ok(written)
}

/// Updates the profile `id` (a folder of `instances` made by `install`) to the version in `file`. The pack's files
/// are replaced or removed where still as the pack wrote them; worlds, added mods and settings the user changed stay
/// (listed in `kept` when the pack wanted to change them). The profile's name and memory are not touched.
pub fn update(instances: &Path, id: &str, file: &Path, fetch: &dyn Fetch, progress: &mut dyn FnMut(&Progress)) -> Result<UpdateReport, MrpackError> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(MrpackError::NotManaged(id.to_string()));
    }
    let dir = instances.join(id);
    let manifest: Manifest = fs::read(dir.join(MANIFEST))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or_else(|| MrpackError::NotManaged(id.to_string()))?;
    let mut archive = ZipArchive::new(fs::File::open(file)?)?;
    let pack = read_pack(&mut archive)?;
    let info = &pack.info;
    let game_dir = dir.join("minecraft");
    let applied = apply(&dir, &game_dir, &manifest.files, &pack, &mut archive, fetch, progress)?;
    let finished = match applied.failed {
        Some(e) => Err(e),
        None => (|| {
            if info.game_version != manifest.game_version || info.fabric_loader != manifest.fabric_loader {
                let new = NewInstance::new(&info.name, &info.game_version, info.fabric_loader.as_deref());
                fs::write(dir.join("mmc-pack.json"), new.mmc_pack_json())?;
            }
            write_manifest(&dir, info, applied.owned.clone())
        })(),
    };
    if let Err(e) = finished {
        // Files may have changed: the record says what the pack owns on disk now, under the old version.
        let both: Vec<Owned> = manifest.files.iter().chain(applied.owned.iter()).cloned().collect();
        let files = owned_on_disk(&game_dir, &both);
        let _ = save_manifest(&dir, &Manifest { files, ..manifest });
        return Err(e);
    }
    Ok(applied.report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, io::Write};
    use zip::write::SimpleFileOptions;

    struct Fake(HashMap<String, Vec<u8>>);

    impl Fetch for Fake {
        fn get(&self, url: &str) -> Result<Vec<u8>, InstanceError> {
            self.0.get(url).cloned().ok_or_else(|| InstanceError::Network(format!("no fixture for {url}")))
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mrpack_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn url(name: &str) -> String {
        format!("https://cdn.modrinth.com/data/x/{name}")
    }

    // One mod of a test pack: its path, its bytes, its env.
    struct M(&'static str, &'static [u8], Option<(&'static str, &'static str)>);

    fn build(dir: &Path, file: &str, version: &str, deps: &str, mods: &[M], overrides: &[(&str, &str)]) -> (PathBuf, Fake) {
        let files: Vec<_> = mods
            .iter()
            .map(|M(path, bytes, env)| {
                let name = path.rsplit('/').next().unwrap();
                let mut f = serde_json::json!({
                    "path": path,
                    "hashes": { "sha1": sha1_hex(bytes), "sha512": hex(&Sha512::digest(bytes)) },
                    "downloads": [url(name)],
                    "fileSize": bytes.len(),
                });
                if let Some((c, s)) = env {
                    f["env"] = serde_json::json!({ "client": c, "server": s });
                }
                f
            })
            .collect();
        let index = format!(
            r#"{{"formatVersion":1,"game":"minecraft","versionId":"{version}","name":"Example Pack","summary":"A test","files":{},"dependencies":{deps}}}"#,
            serde_json::Value::Array(files)
        );
        let path = dir.join(file);
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default();
        zip.start_file(INDEX, opts).unwrap();
        zip.write_all(index.as_bytes()).unwrap();
        for (name, text) in overrides {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let net = mods.iter().map(|M(p, b, _)| (url(p.rsplit('/').next().unwrap()), b.to_vec())).collect();
        (path, Fake(net))
    }

    const DEPS: &str = r#"{"minecraft":"26.3","fabric-loader":"0.20.1"}"#;

    fn pack_v1(dir: &Path) -> (PathBuf, Fake) {
        build(
            dir,
            "v1.mrpack",
            "1.0.0",
            DEPS,
            &[M("mods/sodium-1.jar", b"sodium one", None), M("mods/lithium-1.jar", b"lithium one", Some(("required", "required"))), M("mods/server-only.jar", b"srv", Some(("unsupported", "required")))],
            &[("overrides/config/sodium.json", "{\"a\":1}"), ("overrides/options.txt", "fov:70\n"), ("client-overrides/config/client.txt", "c1"), ("server-overrides/server.txt", "s")],
        )
    }

    fn quiet() -> impl FnMut(&Progress) {
        |_| {}
    }

    #[test]
    fn info_reads_name_version_and_mods_without_installing() {
        let dir = temp("info");
        let (file, _) = pack_v1(&dir);
        let info = read_info(&file).unwrap();
        assert_eq!((info.name.as_str(), info.version.as_str(), info.summary.as_deref()), ("Example Pack", "1.0.0", Some("A test")));
        assert_eq!((info.game_version.as_str(), info.fabric_loader.as_deref()), ("26.3", Some("0.20.1")));
        let mods: Vec<_> = info.mods.iter().map(|m| m.file_name.as_str()).collect();
        assert_eq!(mods, ["lithium-1.jar", "sodium-1.jar"]);
        assert_eq!((info.server_only, info.override_files), (1, 3));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn install_writes_the_instance_mods_and_overrides() {
        let dir = temp("install");
        let (file, net) = pack_v1(&dir);
        let instances = dir.join("instances");
        let mut seen = Vec::new();
        let w = install(&instances, &file, &net, &mut |p| seen.push((p.done, p.total))).unwrap();
        assert_eq!(w.id, "example-pack");
        let pack: serde_json::Value = serde_json::from_slice(&fs::read(w.dir.join("mmc-pack.json")).unwrap()).unwrap();
        assert_eq!(pack["components"][2]["version"], "0.20.1");
        assert_eq!(fs::read(w.game_dir.join("mods/sodium-1.jar")).unwrap(), b"sodium one");
        assert!(!w.game_dir.join("mods/server-only.jar").exists());
        assert_eq!(fs::read_to_string(w.game_dir.join("options.txt")).unwrap(), "fov:70\n");
        assert_eq!(fs::read_to_string(w.game_dir.join("config/client.txt")).unwrap(), "c1");
        assert!(!w.game_dir.join("server.txt").exists());
        assert!(!w.dir.join(STAGING).exists());
        assert!(w.dir.join("instance.cfg").is_file());
        assert_eq!(seen.last(), Some(&(2, 2)));
        // Never an EULA: nothing of the kind is made.
        assert!(!w.dir.join("eula.txt").exists() && !w.game_dir.join("eula.txt").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_wrong_hash_or_a_failed_download_leaves_no_instance() {
        let dir = temp("badhash");
        let (file, mut net) = pack_v1(&dir);
        let instances = dir.join("instances");
        net.0.insert(url("sodium-1.jar"), b"tampered".to_vec());
        assert!(matches!(install(&instances, &file, &net, &mut quiet()), Err(MrpackError::HashMismatch(_))));
        net.0.remove(&url("sodium-1.jar"));
        assert!(matches!(install(&instances, &file, &net, &mut quiet()), Err(MrpackError::Download(..))));
        assert_eq!(fs::read_dir(&instances).unwrap().count(), 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn optional_mods_install_and_unsupported_ones_do_not() {
        let dir = temp("env");
        let (file, net) = build(&dir, "e.mrpack", "1", DEPS, &[M("mods/opt.jar", b"o", Some(("optional", "optional"))), M("mods/no.jar", b"n", Some(("unsupported", "unsupported")))], &[]);
        let info = read_info(&file).unwrap();
        assert_eq!(info.mods.len(), 1);
        assert!(info.mods[0].optional);
        let w = install(&dir.join("i"), &file, &net, &mut quiet()).unwrap();
        assert!(w.game_dir.join("mods/opt.jar").is_file() && !w.game_dir.join("mods/no.jar").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_pack_for_another_loader_or_with_unsafe_paths_is_refused() {
        let dir = temp("refuse");
        let (forge, _) = build(&dir, "f.mrpack", "1", r#"{"minecraft":"1.20.1","forge":"47.1.0"}"#, &[], &[]);
        assert!(matches!(read_info(&forge), Err(MrpackError::Unsupported(_))));
        for bad in ["../evil.jar", "/abs.jar", "C:/Code/x.jar", "mods\\x.jar", "mods/../../x.jar"] {
            let (p, _) = build(&dir, "p.mrpack", "1", DEPS, &[M(bad, b"x", None)], &[]);
            assert!(matches!(read_info(&p), Err(MrpackError::UnsafePath(_))), "{bad}");
        }
        let (p, _) = build(&dir, "o.mrpack", "1", DEPS, &[], &[("overrides/../x.txt", "x")]);
        assert!(matches!(read_info(&p), Err(MrpackError::UnsafePath(_))));
        fs::write(dir.join("junk.mrpack"), b"not a zip").unwrap();
        assert!(matches!(read_info(&dir.join("junk.mrpack")), Err(MrpackError::Unreadable(_))));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_download_from_another_host_is_refused() {
        assert!(check_host("https://cdn.modrinth.com/data/a/b.jar").is_ok());
        assert!(check_host("https://raw.githubusercontent.com/a/b").is_ok());
        for bad in ["http://cdn.modrinth.com/a", "https://cdn.modrinth.com.evil.org/a", "https://evil.org/cdn.modrinth.com", "https://cdn.modrinth.com@example.com/a", "file:///C:/Code/x"] {
            assert!(check_host(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn update_replaces_the_packs_files_and_keeps_the_users() {
        let dir = temp("update");
        let (v1, net1) = pack_v1(&dir);
        let instances = dir.join("instances");
        let w = install(&instances, &v1, &net1, &mut quiet()).unwrap();
        let game = &w.game_dir;
        // The user: changed options, added a mod, has a world, and changed a pack mod.
        fs::write(game.join("options.txt"), "fov:110\n").unwrap();
        fs::create_dir_all(game.join("saves/World")).unwrap();
        fs::write(game.join("saves/World/level.dat"), "world").unwrap();
        fs::write(game.join("mods/my-own.jar"), "mine").unwrap();
        fs::write(game.join("mods/lithium-1.jar"), "hacked").unwrap();
        // Version 2 on the next game version: sodium and its config change, lithium is replaced, options.txt is
        // new, client.txt is dropped, and the pack ships a world.
        let (v2, net2) = build(
            &dir,
            "v2.mrpack",
            "2.0.0",
            r#"{"minecraft":"26.4","fabric-loader":"0.21.0"}"#,
            &[M("mods/sodium-2.jar", b"sodium two", None), M("mods/lithium-2.jar", b"lithium two", None)],
            &[("overrides/config/sodium.json", "{\"a\":2}"), ("overrides/options.txt", "fov:80\n"), ("overrides/saves/World/level.dat", "pack world")],
        );
        let report = update(&instances, &w.id, &v2, &net2, &mut quiet()).unwrap();
        assert_eq!(fs::read(game.join("mods/sodium-2.jar")).unwrap(), b"sodium two");
        assert!(!game.join("mods/sodium-1.jar").exists());
        assert_eq!(fs::read_to_string(game.join("config/sodium.json")).unwrap(), "{\"a\":2}");
        assert!(!game.join("config/client.txt").exists());
        // Kept: the user's options, world, added mod. The changed pack mod is not deleted either.
        assert_eq!(fs::read_to_string(game.join("options.txt")).unwrap(), "fov:110\n");
        assert_eq!(fs::read_to_string(game.join("saves/World/level.dat")).unwrap(), "world");
        assert_eq!(fs::read(game.join("mods/my-own.jar")).unwrap(), b"mine");
        assert_eq!(fs::read(game.join("mods/lithium-1.jar")).unwrap(), b"hacked");
        assert!(game.join("mods/lithium-2.jar").is_file());
        assert!(report.kept.contains(&"options.txt".to_string()) && report.kept.contains(&"mods/lithium-1.jar".to_string()));
        assert!(report.removed.contains(&"mods/sodium-1.jar".to_string()));
        // The game and loader changed: Prism's component file follows.
        let pack: serde_json::Value = serde_json::from_slice(&fs::read(w.dir.join("mmc-pack.json")).unwrap()).unwrap();
        assert_eq!((pack["components"][0]["version"].as_str(), pack["components"][2]["version"].as_str()), (Some("26.4"), Some("0.21.0")));
        // Name and memory stay the instance's own.
        assert!(fs::read_to_string(w.dir.join("instance.cfg")).unwrap().contains("Example Pack"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn update_downloads_nothing_for_what_is_already_there_and_fails_before_touching_files() {
        let dir = temp("update_fail");
        let (v1, net1) = pack_v1(&dir);
        let instances = dir.join("instances");
        let w = install(&instances, &v1, &net1, &mut quiet()).unwrap();
        // The same pack again with no network at all: nothing to fetch.
        let none = Fake(HashMap::new());
        let again = update(&instances, &w.id, &v1, &none, &mut quiet()).unwrap();
        assert!(again.written.is_empty() && again.removed.is_empty() && again.kept.is_empty());
        // A new mod that cannot be fetched: the old files stay.
        let (v2, _) = build(&dir, "v2.mrpack", "2", DEPS, &[M("mods/sodium-2.jar", b"two", None)], &[]);
        assert!(update(&instances, &w.id, &v2, &none, &mut quiet()).is_err());
        assert!(w.game_dir.join("mods/sodium-1.jar").is_file());
        assert!(!w.dir.join(STAGING).exists());
        // A mod the user removed is not brought back by a pack that has not changed it.
        fs::remove_file(w.game_dir.join("mods/sodium-1.jar")).unwrap();
        let back = update(&instances, &w.id, &v1, &none, &mut quiet()).unwrap();
        assert!(back.written.is_empty() && !w.game_dir.join("mods/sodium-1.jar").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_the_user_changed_is_not_reported_when_the_pack_did_not_change_it() {
        let dir = temp("unchanged_by_pack");
        let (v1, net1) = pack_v1(&dir);
        let instances = dir.join("instances");
        let w = install(&instances, &v1, &net1, &mut quiet()).unwrap();
        fs::write(w.game_dir.join("options.txt"), "fov:110\n").unwrap();
        fs::write(w.game_dir.join("mods/lithium-1.jar"), "hacked").unwrap();
        // Version 2 changes only sodium's config.
        let (v2, net2) = build(
            &dir,
            "v2.mrpack",
            "2.0.0",
            DEPS,
            &[M("mods/sodium-1.jar", b"sodium one", None), M("mods/lithium-1.jar", b"lithium one", Some(("required", "required")))],
            &[("overrides/config/sodium.json", "{\"a\":2}"), ("overrides/options.txt", "fov:70\n"), ("client-overrides/config/client.txt", "c1")],
        );
        let report = update(&instances, &w.id, &v2, &net2, &mut quiet()).unwrap();
        assert!(report.kept.is_empty(), "{:?}", report.kept);
        assert_eq!(report.written, ["config/sodium.json"]);
        assert_eq!(fs::read_to_string(w.game_dir.join("options.txt")).unwrap(), "fov:110\n");
        assert_eq!(fs::read(w.game_dir.join("mods/lithium-1.jar")).unwrap(), b"hacked");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_replacement_that_fails_midway_leaves_a_record_that_matches_the_disk() {
        let dir = temp("midway");
        let (v1, net1) = pack_v1(&dir);
        let instances = dir.join("instances");
        let w = install(&instances, &v1, &net1, &mut quiet()).unwrap();
        // A file where a folder must be made: the second new file fails after the first was put in place.
        fs::write(w.game_dir.join("m"), "in the way").unwrap();
        let (v2, net2) = build(
            &dir,
            "v2.mrpack",
            "2.0.0",
            DEPS,
            &[M("mods/sodium-1.jar", b"sodium one", None), M("mods/lithium-1.jar", b"lithium one", Some(("required", "required")))],
            &[("overrides/a.txt", "new a"), ("overrides/config/sodium.json", "{\"a\":2}"), ("overrides/m/b.txt", "b")],
        );
        assert!(update(&instances, &w.id, &v2, &net2, &mut quiet()).is_err());
        assert_eq!(fs::read_to_string(w.game_dir.join("a.txt")).unwrap(), "new a");
        // The old version number stays, so the update can run again; the record lists what is on disk.
        let manifest: Manifest = serde_json::from_slice(&fs::read(w.dir.join(MANIFEST)).unwrap()).unwrap();
        assert_eq!(manifest.version_id, "1.0.0");
        let record: BTreeMap<_, _> = manifest.files.iter().map(|o| (o.path.as_str(), o.sha1.as_str())).collect();
        assert_eq!(record["a.txt"], sha1_hex(b"new a"));
        assert_eq!(record["mods/sodium-1.jar"], sha1_hex(b"sodium one"));
        assert!(!record.contains_key("m/b.txt"));
        assert!(!w.dir.join(STAGING).exists() && !w.dir.join(format!("{MANIFEST}.tmp")).exists());
        // With the obstacle gone, the same update finishes the job and reports nothing as kept.
        fs::remove_file(w.game_dir.join("m")).unwrap();
        let report = update(&instances, &w.id, &v2, &net2, &mut quiet()).unwrap();
        assert!(report.kept.is_empty(), "{:?}", report.kept);
        assert_eq!(fs::read_to_string(w.game_dir.join("m/b.txt")).unwrap(), "b");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn windows_device_names_and_trailing_dots_are_unsafe_paths() {
        for bad in ["mods/con.jar", "NUL", "config/aux.txt", "a/com1", "x/LPT9.cfg", "mods/a.jar.", "mods/a.jar ", "mods/a<b.jar", "mods/x?.jar"] {
            assert!(safe_path(bad).is_err(), "{bad}");
        }
        for good in ["mods/sodium.jar", "config/console.txt", "config/com10.txt", "a/b.c.d"] {
            assert!(safe_path(good).is_ok(), "{good}");
        }
    }

    #[test]
    fn overrides_that_unpack_to_too_much_are_refused() {
        let dir = temp("bomb");
        let path = dir.join("big.mrpack");
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(INDEX, opts).unwrap();
        zip.write_all(format!(r#"{{"formatVersion":1,"game":"minecraft","versionId":"1","name":"Big","files":[],"dependencies":{DEPS}}}"#).as_bytes()).unwrap();
        // Zeros compress to almost nothing: more than the cap declared, a few kilobytes on disk.
        let zeros = vec![0u8; 1 << 19];
        for n in 0..3 {
            zip.start_file(format!("overrides/big{n}.bin"), opts).unwrap();
            zip.write_all(&zeros).unwrap();
        }
        zip.finish().unwrap();
        assert!(matches!(read_info(&path), Err(MrpackError::Unreadable(why)) if why.contains("1 GB")));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn update_needs_an_instance_the_pack_made() {
        let dir = temp("unmanaged");
        let (v1, net) = pack_v1(&dir);
        fs::create_dir_all(dir.join("instances/plain")).unwrap();
        assert!(matches!(update(&dir.join("instances"), "plain", &v1, &net, &mut quiet()), Err(MrpackError::NotManaged(_))));
        assert!(matches!(update(&dir.join("instances"), "../x", &v1, &net, &mut quiet()), Err(MrpackError::NotManaged(_))));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn decisions() {
        assert_eq!(decide("a", None, None, "n"), Action::Write);
        assert_eq!(decide("a", None, Some("n"), "n"), Action::StayRemoved);
        assert_eq!(decide("a", None, Some("o"), "n"), Action::Write);
        assert_eq!(decide("a", Some("n"), Some("o"), "n"), Action::Same);
        assert_eq!(decide("a", Some("o"), Some("o"), "n"), Action::Write);
        assert_eq!(decide("a", Some("u"), Some("o"), "n"), Action::KeepUser);
        assert_eq!(decide("a", Some("u"), None, "n"), Action::KeepUser);
        assert_eq!(decide("saves/w/level.dat", Some("o"), Some("o"), "n"), Action::KeepUser);
        // The user changed it and the pack did not: nothing to do, nothing to report.
        assert_eq!(decide("a", Some("u"), Some("n"), "n"), Action::Same);
        assert_eq!(decide("saves/w/level.dat", Some("u"), Some("n"), "n"), Action::Same);
    }
}
