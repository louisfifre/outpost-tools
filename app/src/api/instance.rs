// A new profile: the Prism instance folder Outpost Tools writes itself, from a game version and a With Fabric choice
// (specs/play-a-profile.md, New profile and Rules). Nothing here starts Prism, Java or the game, and nothing signs
// in: it reads Mojang's version list and Fabric's list of loaders, then writes two files and an empty game folder.
// The game itself downloads at the profile's first Play.
//
// The public entry points are `create_instance` (everything), and its parts for a caller that has its own source of
// versions, as the .mrpack import has: `write_instance` writes a `NewInstance`, `fabric_loader` and `game_releases`
// ask the network, `performance_mods` names what Fabric profiles bring.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Serialize;
use serde_json::{Value, json};

use super::key_value::Cfg;

pub const VERSION_MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC_META: &str = "https://meta.fabricmc.net/v2/versions/loader";

// Prism's own uids for the components of a Fabric profile (its meta service names them so).
const MINECRAFT: &str = "net.minecraft";
const INTERMEDIARY: &str = "net.fabricmc.intermediary";
const FABRIC_LOADER: &str = "net.fabricmc.fabric-loader";

/// Why a profile could not be made, each one a thing the page can say in a sentence.
#[derive(Debug)]
pub enum InstanceError {
    /// A list could not be fetched or read: the address and what went wrong.
    Network(String),
    /// The game version is not in Mojang's list of releases.
    UnknownVersion(String),
    /// Fabric has no stable loader for this game version.
    NoFabric(String),
    /// The name or the version cannot make a folder or a file.
    Invalid(String),
    Io(io::Error),
}

impl fmt::Display for InstanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstanceError::Network(why) => write!(f, "A list could not be fetched: {why}"),
            InstanceError::UnknownVersion(v) => write!(f, "{v} is not a game version Mojang lists."),
            InstanceError::NoFabric(v) => write!(f, "Fabric has no version for {v} yet."),
            InstanceError::Invalid(why) => f.write_str(why),
            InstanceError::Io(e) => write!(f, "The profile's folder could not be written: {e}"),
        }
    }
}

impl std::error::Error for InstanceError {}

impl From<io::Error> for InstanceError {
    fn from(e: io::Error) -> Self {
        InstanceError::Io(e)
    }
}

/// Gets the bytes of an address. The network sits behind this so tests read fixtures.
pub trait Fetch {
    fn get(&self, url: &str) -> Result<Vec<u8>, InstanceError>;
}

/// The real network: blocking, with a User-Agent that names the program and carries nothing personal.
pub struct HttpFetch {
    agent: ureq::Agent,
}

impl HttpFetch {
    pub fn new() -> HttpFetch {
        let agent = ureq::Agent::config_builder()
            .user_agent(concat!("outpost-tools/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        HttpFetch { agent }
    }
}

impl Default for HttpFetch {
    fn default() -> Self {
        HttpFetch::new()
    }
}

impl Fetch for HttpFetch {
    fn get(&self, url: &str) -> Result<Vec<u8>, InstanceError> {
        let mut response = self.agent.get(url).call().map_err(|e| InstanceError::Network(format!("{url}: {e}")))?;
        response.body_mut().read_to_vec().map_err(|e| InstanceError::Network(format!("{url}: {e}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionKind {
    Release,
    Snapshot,
    /// Mojang's old_alpha and old_beta.
    Old,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameVersion {
    pub id: String,
    pub kind: VersionKind,
    /// ISO 8601, as Mojang writes it: the list sorts by it, newest first.
    pub release_time: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionManifest {
    /// The id Mojang names the latest release: the one a new profile proposes.
    pub latest_release: String,
    /// Every version, newest first.
    pub versions: Vec<GameVersion>,
}

impl VersionManifest {
    /// The releases only, newest first: what the version choice of a new profile lists.
    pub fn releases(&self) -> impl Iterator<Item = &GameVersion> {
        self.versions.iter().filter(|v| v.kind == VersionKind::Release)
    }

    pub fn is_release(&self, id: &str) -> bool {
        self.releases().any(|v| v.id == id)
    }
}

/// Reads Mojang's version manifest (version_manifest_v2.json).
pub fn parse_version_manifest(bytes: &[u8]) -> Result<VersionManifest, InstanceError> {
    let bad = |what: &str| InstanceError::Network(format!("Mojang's version list has no {what}"));
    let value: Value = serde_json::from_slice(super::key_value::strip_bom(bytes))
        .map_err(|e| InstanceError::Network(format!("Mojang's version list is not JSON: {e}")))?;
    let latest_release = value["latest"]["release"].as_str().ok_or_else(|| bad("latest release"))?.to_string();
    let mut versions = Vec::new();
    for v in value["versions"].as_array().ok_or_else(|| bad("versions"))? {
        let kind = match v["type"].as_str() {
            Some("release") => VersionKind::Release,
            Some("snapshot") => VersionKind::Snapshot,
            Some(_) => VersionKind::Old,
            None => return Err(bad("type on a version")),
        };
        let id = v["id"].as_str().ok_or_else(|| bad("id on a version"))?.to_string();
        let release_time = v["releaseTime"].as_str().unwrap_or_default().to_string();
        versions.push(GameVersion { id, kind, release_time });
    }
    // The file is already newest first; sorting by the ISO time keeps that if it ever is not.
    versions.sort_by(|a, b| b.release_time.cmp(&a.release_time));
    Ok(VersionManifest { latest_release, versions })
}

/// The list of game versions, from Mojang's manifest.
pub fn game_releases(fetch: &dyn Fetch) -> Result<VersionManifest, InstanceError> {
    parse_version_manifest(&fetch.get(VERSION_MANIFEST_URL)?)
}

/// The newest stable Fabric loader in Fabric's answer for one game version
/// (`/v2/versions/loader/<game>`, newest first), None when it lists no stable one.
pub fn parse_stable_loader(bytes: &[u8]) -> Result<Option<String>, InstanceError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|e| InstanceError::Network(format!("Fabric's list is not JSON: {e}")))?;
    let entries = value.as_array().ok_or_else(|| InstanceError::Network("Fabric's list is not a list".into()))?;
    Ok(entries
        .iter()
        .map(|e| &e["loader"])
        .find(|l| l["stable"].as_bool() == Some(true))
        .and_then(|l| l["version"].as_str())
        .map(String::from))
}

/// The latest stable Fabric loader for a game version, from Fabric's meta API. NoFabric when there is none.
pub fn fabric_loader(fetch: &dyn Fetch, game_version: &str) -> Result<String, InstanceError> {
    check_version_id(game_version)?;
    let bytes = fetch.get(&format!("{FABRIC_META}/{game_version}"))?;
    parse_stable_loader(&bytes)?.ok_or_else(|| InstanceError::NoFabric(game_version.to_string()))
}

/// A Modrinth project, named by its id: the id never changes, the slug can.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModrinthProject {
    pub id: &'static str,
    pub name: &'static str,
}

/// The performance mods a Fabric profile brings, Sodium and Lithium first. Only the projects are named: choosing and
/// downloading a version for the profile's game is the Modrinth client's work. Which other mods stay kept up is still
/// to check (specs/play-a-profile.md, New profile).
pub fn performance_mods() -> &'static [ModrinthProject] {
    &[ModrinthProject { id: "AANobbMI", name: "Sodium" }, ModrinthProject { id: "gvQqBUqZ", name: "Lithium" }]
}

/// The memory a profile gets, in megabytes, written as Prism's own override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub min_mb: u32,
    pub max_mb: u32,
}

impl Memory {
    /// A new profile's start: a 2 GB maximum for the plain game (the maintainer, 2026-11-25), 3 GB with the
    /// performance mods, and a 1 GB minimum for both. The 3 GB and the 1 GB are to validate.
    pub fn starting(fabric: bool) -> Memory {
        Memory { min_mb: 1024, max_mb: if fabric { 3072 } else { 2048 } }
    }
}

/// Everything that makes the files of a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstance {
    /// The name the user reads, kept as typed in `instance.cfg`.
    pub name: String,
    /// A Mojang version id, `26.3`.
    pub game_version: String,
    /// The Fabric loader's version; None is the plain game.
    pub fabric_loader: Option<String>,
    pub memory: Memory,
}

impl NewInstance {
    pub fn new(name: &str, game_version: &str, fabric_loader: Option<&str>) -> NewInstance {
        NewInstance {
            name: name.trim().to_string(),
            game_version: game_version.to_string(),
            fabric_loader: fabric_loader.map(String::from),
            memory: Memory::starting(fabric_loader.is_some()),
        }
    }

    /// The text of `mmc-pack.json`: the components Prism resolves into the game, in Prism's own layout (4 spaces,
    /// keys sorted). Prism adds the LWJGL component and what Minecraft requires when it first reads the file.
    pub fn mmc_pack_json(&self) -> String {
        let game = &self.game_version;
        let mut components = vec![json!({
            "cachedName": "Minecraft",
            "cachedVersion": game,
            "important": true,
            "uid": MINECRAFT,
            "version": game,
        })];
        if let Some(loader) = &self.fabric_loader {
            components.push(json!({
                "cachedName": "Intermediary Mappings",
                "cachedRequires": [{ "equals": game, "uid": MINECRAFT }],
                "cachedVersion": game,
                "dependencyOnly": true,
                "uid": INTERMEDIARY,
                "version": game,
            }));
            components.push(json!({
                "cachedName": "Fabric Loader",
                "cachedRequires": [{ "uid": INTERMEDIARY }],
                "cachedVersion": loader,
                "uid": FABRIC_LOADER,
                "version": loader,
            }));
        }
        let pack = json!({ "components": components, "formatVersion": 1 });
        let mut out = Vec::new();
        let format = serde_json::ser::PrettyFormatter::with_indent(b"    ");
        pack.serialize(&mut serde_json::Serializer::with_formatter(&mut out, format)).expect("a Value always serializes");
        let mut text = String::from_utf8(out).expect("JSON is UTF-8");
        text.push('\n');
        text
    }

    /// The settings of `instance.cfg`, written by the same `Cfg` that changes them later, so the quoting is one.
    pub(super) fn cfg(&self) -> Cfg {
        let mut cfg = Cfg::new();
        cfg.set("ConfigVersion", "1.3");
        cfg.set("InstanceType", "OneSix");
        cfg.set("name", &self.name);
        cfg.set("iconKey", "default");
        cfg.set("OverrideMemory", "true");
        cfg.set("MinMemAlloc", &self.memory.min_mb.to_string());
        cfg.set("MaxMemAlloc", &self.memory.max_mb.to_string());
        cfg
    }
}

/// A profile written to the disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenInstance {
    /// The folder's name under Prism's `instances`: the id Prism and `--launch` know it by.
    pub id: String,
    pub dir: PathBuf,
    /// The game folder, `minecraft`: where mods, worlds and options go.
    pub game_dir: PathBuf,
}

/// A folder name for a profile: lowercase letters, digits and dashes, the characters the rest of the api accepts as
/// an instance id.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() { "profile".to_string() } else { out.to_string() }
}

fn check_version_id(id: &str) -> Result<(), InstanceError> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ')) {
        return Err(InstanceError::Invalid(format!("{id:?} is not a game version.")));
    }
    Ok(())
}

/// Writes the profile's instance folder under Prism's `instances` folder: `instance.cfg`, `mmc-pack.json` and an
/// empty `minecraft` game folder (with `mods` for a Fabric profile). The folder is the name's slug, with `-2`, `-3`
/// after it when taken; it is claimed with a plain create, so a folder that exists is never touched. Prism must not
/// be running: it keeps its instances in memory and would not see the new one until it reads the folder again.
pub fn write_instance(instances: &Path, new: &NewInstance) -> Result<WrittenInstance, InstanceError> {
    check_version_id(&new.game_version)?;
    if new.name.is_empty() {
        return Err(InstanceError::Invalid("A profile needs a name.".into()));
    }
    if new.memory.min_mb == 0 || new.memory.min_mb > new.memory.max_mb {
        return Err(InstanceError::Invalid("The minimum memory cannot be above the maximum.".into()));
    }
    fs::create_dir_all(instances)?;
    let base = slug(&new.name);
    let (id, dir) = (1u32..)
        .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
        .find_map(|id| {
            let dir = instances.join(&id);
            match fs::create_dir(&dir) {
                Ok(()) => Some(Ok((id, dir))),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                Err(e) => Some(Err(e)),
            }
        })
        .expect("an endless range")?;
    let game_dir = dir.join("minecraft");
    let written = (|| -> io::Result<()> {
        fs::create_dir(&game_dir)?;
        if new.fabric_loader.is_some() {
            fs::create_dir(game_dir.join("mods"))?;
        }
        fs::write(dir.join("mmc-pack.json"), new.mmc_pack_json())?;
        new.cfg().write(&dir.join("instance.cfg"))
    })();
    if let Err(e) = written {
        // Our own folder, made a moment ago: nothing of the user's is in it.
        let _ = fs::remove_dir_all(&dir);
        return Err(e.into());
    }
    Ok(WrittenInstance { id, dir, game_dir })
}

/// A new profile from a name, a game version and the With Fabric choice: checks the version against Mojang's list,
/// takes the latest stable Fabric loader when asked, and writes the instance.
pub fn create_instance(fetch: &dyn Fetch, instances: &Path, name: &str, game_version: &str, with_fabric: bool) -> Result<WrittenInstance, InstanceError> {
    if !game_releases(fetch)?.is_release(game_version) {
        return Err(InstanceError::UnknownVersion(game_version.to_string()));
    }
    let loader = if with_fabric { Some(fabric_loader(fetch, game_version)?) } else { None };
    write_instance(instances, &NewInstance::new(name, game_version, loader.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &[u8] = include_bytes!("../../fixtures/instance/version_manifest.json");
    const LOADERS: &[u8] = include_bytes!("../../fixtures/instance/fabric_loaders_26.3.json");
    const UNSTABLE: &[u8] = include_bytes!("../../fixtures/instance/fabric_loaders_unstable.json");
    // What Prism wrote for a Fabric profile on 26.3 (versions only: nothing personal).
    const PRISM_PACK: &str = include_str!("../../fixtures/instance/prism-mmc-pack.json");
    const PRISM_CFG: &str = include_str!("../../fixtures/instance/prism-instance.cfg");

    struct Fixtures;

    impl Fetch for Fixtures {
        fn get(&self, url: &str) -> Result<Vec<u8>, InstanceError> {
            match url {
                VERSION_MANIFEST_URL => Ok(MANIFEST.to_vec()),
                "https://meta.fabricmc.net/v2/versions/loader/26.3" => Ok(LOADERS.to_vec()),
                "https://meta.fabricmc.net/v2/versions/loader/1.20.1" => Ok(UNSTABLE.to_vec()),
                other => Err(InstanceError::Network(format!("no fixture for {other}"))),
            }
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("instance_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn manifest_lists_releases_newest_first_and_names_the_latest() {
        let manifest = parse_version_manifest(MANIFEST).unwrap();
        assert_eq!(manifest.latest_release, "26.3");
        let releases: Vec<_> = manifest.releases().map(|v| v.id.as_str()).collect();
        assert_eq!(releases, ["26.3", "26.2", "1.21.11", "1.20.1"]);
        assert!(manifest.is_release("26.3"));
        assert!(!manifest.is_release("26.4-pre-1"));
        assert!(!manifest.is_release("b1.7.3"));
        assert_eq!(manifest.versions.iter().filter(|v| v.kind == VersionKind::Old).count(), 2);
    }

    #[test]
    fn manifest_that_is_not_mojangs_is_a_network_error() {
        assert!(matches!(parse_version_manifest(b"{}"), Err(InstanceError::Network(_))));
        assert!(matches!(parse_version_manifest(b"<html>"), Err(InstanceError::Network(_))));
    }

    #[test]
    fn the_stable_loader_is_the_newest_stable_one() {
        assert_eq!(parse_stable_loader(LOADERS).unwrap().as_deref(), Some("0.20.1"));
        assert_eq!(parse_stable_loader(UNSTABLE).unwrap(), None);
        assert_eq!(parse_stable_loader(b"[]").unwrap(), None);
        assert!(matches!(fabric_loader(&Fixtures, "1.20.1"), Err(InstanceError::NoFabric(_))));
        assert_eq!(fabric_loader(&Fixtures, "26.3").unwrap(), "0.20.1");
        // A version is a path segment of an address: nothing else gets through.
        assert!(matches!(fabric_loader(&Fixtures, "26.3/../x"), Err(InstanceError::Invalid(_))));
    }

    #[test]
    fn performance_mods_are_sodium_and_lithium_by_modrinth_id() {
        let ids: Vec<_> = performance_mods().iter().map(|m| (m.name, m.id)).collect();
        assert_eq!(ids, [("Sodium", "AANobbMI"), ("Lithium", "gvQqBUqZ")]);
    }

    #[test]
    fn mmc_pack_matches_what_prism_wrote_but_for_what_prism_adds_itself() {
        let mut prism: Value = serde_json::from_str(PRISM_PACK).unwrap();
        // Prism adds LWJGL and Minecraft's requirement on it when it resolves the game's files.
        prism["components"].as_array_mut().unwrap().retain(|c| c["uid"] != "org.lwjgl3");
        prism["components"][0].as_object_mut().unwrap().remove("cachedRequires");
        let ours: Value = serde_json::from_str(&NewInstance::new("x", "26.3", Some("0.20.1")).mmc_pack_json()).unwrap();
        assert_eq!(ours, prism);
    }

    #[test]
    fn mmc_pack_is_laid_out_as_prism_lays_it_out() {
        let text = NewInstance::new("x", "26.3", None).mmc_pack_json();
        assert_eq!(
            text,
            "{\n    \"components\": [\n        {\n            \"cachedName\": \"Minecraft\",\n            \"cachedVersion\": \"26.3\",\n            \"important\": true,\n            \"uid\": \"net.minecraft\",\n            \"version\": \"26.3\"\n        }\n    ],\n    \"formatVersion\": 1\n}\n"
        );
    }

    #[test]
    fn cfg_has_the_keys_prism_writes_for_a_new_instance() {
        let ours = NewInstance { memory: Memory { min_mb: 2048, max_mb: 6144 }, ..NewInstance::new("Example", "26.3", Some("0.20.1")) }.cfg();
        let path = temp("prism_cfg");
        fs::write(&path, PRISM_CFG).unwrap();
        let prism = Cfg::read(&path);
        let _ = fs::remove_file(path);
        for key in ["ConfigVersion", "InstanceType", "name", "iconKey", "OverrideMemory", "MinMemAlloc", "MaxMemAlloc"] {
            assert_eq!(ours.get(key), prism.get(key), "{key}");
            assert!(ours.get(key).is_some(), "{key}");
        }
    }

    #[test]
    fn a_name_with_quotes_is_quoted_and_read_back() {
        let dir = temp("quoted");
        let written = write_instance(&dir, &NewInstance::new(r#"Bob's "pack", v2"#, "26.3", None)).unwrap();
        let cfg = Cfg::read(&written.dir.join("instance.cfg"));
        assert_eq!(cfg.get("name").as_deref(), Some(r#"Bob's "pack", v2"#));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn slugs_are_instance_ids_the_api_accepts() {
        assert_eq!(slug("My Pack!"), "my-pack");
        assert_eq!(slug("  --  "), "profile");
        assert_eq!(slug("Été 2026"), "t-2026");
        assert!(slug("../x").chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }

    #[test]
    fn create_writes_a_fabric_instance_and_never_overwrites_a_folder() {
        let dir = temp("create");
        let first = create_instance(&Fixtures, &dir, "My Pack", "26.3", true).unwrap();
        assert_eq!(first.id, "my-pack");
        let pack: Value = serde_json::from_slice(&fs::read(first.dir.join("mmc-pack.json")).unwrap()).unwrap();
        let uids: Vec<_> = pack["components"].as_array().unwrap().iter().map(|c| c["uid"].as_str().unwrap()).collect();
        assert_eq!(uids, ["net.minecraft", "net.fabricmc.intermediary", "net.fabricmc.fabric-loader"]);
        assert_eq!(pack["components"][2]["version"], "0.20.1");
        assert!(first.game_dir.join("mods").is_dir());
        let cfg = Cfg::read(&first.dir.join("instance.cfg"));
        assert_eq!((cfg.get("name").as_deref(), cfg.number("MaxMemAlloc")), (Some("My Pack"), 3072));
        let text = fs::read_to_string(first.dir.join("instance.cfg")).unwrap();
        assert!(text.starts_with("[General]\r\n"));

        let second = create_instance(&Fixtures, &dir, "My Pack", "26.3", true).unwrap();
        assert_eq!(second.id, "my-pack-2");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn create_plain_game_has_only_minecraft_and_two_gigabytes() {
        let dir = temp("plain");
        let plain = create_instance(&Fixtures, &dir, "Vanilla", "26.2", false).unwrap();
        let pack: Value = serde_json::from_slice(&fs::read(plain.dir.join("mmc-pack.json")).unwrap()).unwrap();
        assert_eq!(pack["components"].as_array().unwrap().len(), 1);
        assert_eq!(Cfg::read(&plain.dir.join("instance.cfg")).number("MaxMemAlloc"), 2048);
        assert!(!plain.game_dir.join("mods").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn create_refuses_what_it_cannot_make_and_leaves_nothing_behind() {
        let dir = temp("refuse");
        assert!(matches!(create_instance(&Fixtures, &dir, "A", "26.4-pre-1", false), Err(InstanceError::UnknownVersion(_))));
        assert!(matches!(create_instance(&Fixtures, &dir, "A", "1.20.1", true), Err(InstanceError::NoFabric(_))));
        assert!(matches!(create_instance(&Fixtures, &dir, "  ", "26.3", false), Err(InstanceError::Invalid(_))));
        assert!(!dir.exists() || fs::read_dir(&dir).unwrap().next().is_none());
        let _ = fs::remove_dir_all(dir);
    }

    // The real network: Mojang's and Fabric's public lists, no account, nothing downloaded. Run on request.
    #[test]
    #[ignore]
    fn live_lists_have_the_shape_we_read() {
        let fetch = HttpFetch::new();
        let manifest = game_releases(&fetch).unwrap();
        assert!(manifest.is_release(&manifest.latest_release));
        assert!(!fabric_loader(&fetch, &manifest.latest_release).unwrap().is_empty());
    }
}
