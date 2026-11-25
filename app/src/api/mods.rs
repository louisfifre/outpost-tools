// The mods of a profile: what the profile's page (game version, Fabric, the mods and their newer versions, remove and
// undo, update, update all) and the page that adds a mod (search, add with what it needs, a file, a link) ask. A
// profile is a Prism instance: its game version and Fabric are in `mmc-pack.json` (instance.rs), its mods are the
// jars of its game folder's `mods` (a link followed, profiles.rs), and Modrinth names them by the SHA-1 of the file
// (modrinth.rs). Everything outside the process (Modrinth, Fabric's list, the browser, the file dialog) is behind
// `Outside`, so the tests answer from a table and nothing reaches the network.
//
// Routes (GET answers any origin, POST only the page's own, as in api.rs):
//   GET  /api/mods/<profile>                  -> {gameVersion, gameVersions, fabric: {version, latest} | null,
//                                                 mods: [{id, slug, name, version, latest}]}
//   POST /api/mods/<profile>/remove  {mod}    -> {}   the jar is kept aside, for `restore`, until another change
//   POST /api/mods/<profile>/restore {mod}    -> {}
//   POST /api/mods/<profile>/update  {mod}    -> {}
//   POST /api/mods/<profile>/update-all       -> {}
//   POST /api/mods/<profile>/fabric           -> {}   installs Fabric, or moves it to the newest stable loader
//   POST /api/mods/search {profile, query}    -> {mods: [{id, title, description, version, url, state, newVersion}]}
//   POST /api/mods/add    {profile, mod}      -> {added: [{id, title, version, neededBy}]}
//   POST /api/mods/update {profile, mod}      -> {}
//   POST /api/mods/file   {profile}           -> {}   asks for a jar and adds it as it is; nothing when none is chosen
//   POST /api/mods/open   {url}, /api/open-url {url} -> {}   a page opened in the browser
// A mod of the profile's page is named by its file (`id`): the file is all that is known of a jar Modrinth does not
// know. The page that adds a mod names a mod by its Modrinth project, and `update` takes either.
// Failures answer a sentence in `error`: 409 when Minecraft runs (the files are held), 503 when the network cannot be
// reached, 422 when the request cannot be done (no version fits, a file is there already), 404 when nothing is there.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use serde_json::{Value, json};
use sha1::{Digest, Sha1};

use super::instance::{self, Fetch, InstanceError};
use super::modrinth::{self, Http, HttpError, Modrinth, UpdateState};
use super::{Api, Failure, Reply};
use crate::system;

/// What the mods' routes ask of the world outside the process.
pub trait Outside: Http + Send + Sync {
    /// Opens an address in the person's browser.
    fn open(&self, url: &str) -> Result<(), String>;
    /// Asks the person for a jar; None when they chose none.
    fn pick_file(&self) -> Option<PathBuf>;
    /// Whether Minecraft's files are held; None to look at the processes.
    fn locked(&self) -> Option<bool> {
        None
    }
}

impl Http for Arc<dyn Outside> {
    fn get(&self, url: &str) -> Result<Vec<u8>, HttpError> {
        (**self).get(url)
    }
    fn post_json(&self, url: &str, body: &str) -> Result<Vec<u8>, HttpError> {
        (**self).post_json(url, body)
    }
}

// The lists of Mojang and Fabric come through the same door as Modrinth's answers.
struct Wire(Arc<dyn Outside>);

impl Fetch for Wire {
    fn get(&self, url: &str) -> Result<Vec<u8>, InstanceError> {
        self.0.get(url).map_err(|e| InstanceError::Network(format!("{url}: {}", reason(&e))))
    }
}

fn reason(error: &HttpError) -> String {
    match error {
        HttpError::Unreachable(why) => why.clone(),
        HttpError::Status(code) => format!("status {code}"),
    }
}

// The real world: Modrinth over the network, the browser through the shell, a Windows file dialog.
struct Real(modrinth::UreqHttp);

impl Http for Real {
    fn get(&self, url: &str) -> Result<Vec<u8>, HttpError> {
        self.0.get(url)
    }
    fn post_json(&self, url: &str, body: &str) -> Result<Vec<u8>, HttpError> {
        self.0.post_json(url, body)
    }
}

impl Outside for Real {
    fn open(&self, url: &str) -> Result<(), String> {
        // The shell opens an address with the person's browser; the address is one argument, never a command line.
        system::start("explorer.exe", &[url]).map_err(|e| e.to_string())
    }

    fn pick_file(&self) -> Option<PathBuf> {
        let script = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; \
            $owner = New-Object System.Windows.Forms.Form -Property @{ TopMost = $true }; \
            $dialog = New-Object System.Windows.Forms.OpenFileDialog; \
            $dialog.Filter = 'Mods (*.jar)|*.jar'; $dialog.Title = 'Add a mod'; \
            if ($dialog.ShowDialog($owner) -eq 'OK') { [Console]::Out.Write($dialog.FileName) }";
        let out = system::hidden("pwsh").args(["-NoProfile", "-STA", "-Command", script]).output().ok()?;
        let chosen = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!chosen.is_empty()).then(|| PathBuf::from(chosen))
    }
}

// What a test or a picture put in place of the real world, by the pack repository an `Api` was made for.
static INSTALLED: Mutex<Vec<(PathBuf, Arc<dyn Outside>)>> = Mutex::new(Vec::new());

/// Answers the mods' routes of the `Api` made for `repo` from `outside` instead of the real world. For tests and
/// pictures: nothing in the program calls it.
pub fn install(repo: &Path, outside: Arc<dyn Outside>) {
    let mut all = INSTALLED.lock().unwrap_or_else(PoisonError::into_inner);
    all.retain(|(r, _)| r != repo);
    all.push((repo.to_path_buf(), outside));
}

fn outside_of(repo: &Path) -> Arc<dyn Outside> {
    let all = INSTALLED.lock().unwrap_or_else(PoisonError::into_inner);
    all.iter().find(|(r, _)| r == repo).map(|(_, o)| o.clone()).unwrap_or_else(|| Arc::new(Real(modrinth::UreqHttp::new())))
}

// Why a request is not done: the reply that says so.
type Stop = Reply;
type Done<T> = Result<T, Stop>;

fn said(status: u16, sentence: impl Into<String>) -> Stop {
    Reply::error(status, sentence)
}

fn from_modrinth(error: modrinth::Error) -> Stop {
    match error {
        modrinth::Error::Http(_) => said(503, error.to_string()),
        modrinth::Error::Parse(_) => said(502, error.to_string()),
        modrinth::Error::NoVersion { .. } | modrinth::Error::Incompatible { .. } => said(422, format!("{error}.")),
    }
}

fn from_instance(error: InstanceError) -> Stop {
    match error {
        InstanceError::Network(_) => said(503, error.to_string()),
        InstanceError::NoFabric(_) => said(422, error.to_string()),
        other => said(500, other.to_string()),
    }
}

fn io(what: &str, error: std::io::Error) -> Stop {
    said(500, format!("{what}: {error}"))
}

const RUNNING: &str = "Minecraft is running: mods can change once it is closed.";
// How many results the page that adds a mod shows.
const FOUND: u32 = 20;
// How many versions are asked of Modrinth at once for a list of results.
const AT_ONCE: usize = 6;

// A profile: its folder (Prism's instance), the game folder inside it and where its jars are.
struct Profile {
    dir: PathBuf,
    mods: PathBuf,
}

impl Profile {
    // Kept in the instance's folder, not the game's: a link there can lead to a folder the pack shares.
    fn aside(&self) -> PathBuf {
        self.dir.join("outpost-removed")
    }

    fn pack_file(&self) -> PathBuf {
        self.dir.join("mmc-pack.json")
    }

    fn pack(&self) -> Done<(String, String, Option<String>)> {
        let text = fs::read_to_string(self.pack_file()).map_err(|e| io("The profile's mmc-pack.json could not be read", e))?;
        let (game, fabric) = instance::pack_versions(&text).ok_or_else(|| said(500, "The profile's mmc-pack.json names no game version."))?;
        Ok((text, game, fabric))
    }
}

// A jar of the profile with its SHA-1.
#[derive(Clone)]
struct Jar {
    file: String,
    path: PathBuf,
    sha1: String,
}

// A jar and what Modrinth says of it.
struct Known {
    jar: Jar,
    current: Option<modrinth::ModVersion>,
    state: UpdateState,
}

fn jars(dir: &Path) -> Vec<Jar> {
    let mut found: Vec<Jar> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_file() && e.file_name().to_string_lossy().to_lowercase().ends_with(".jar"))
                .filter_map(|e| {
                    let path = e.path();
                    Some(Jar { file: e.file_name().to_string_lossy().to_string(), sha1: modrinth::file_sha1(&path).ok()?, path })
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort_by(|a, b| a.file.to_lowercase().cmp(&b.file.to_lowercase()));
    found
}

fn inspect(outside: &Arc<dyn Outside>, jars: &[Jar], game: &str) -> Result<Vec<Known>, modrinth::Error> {
    let hashes: Vec<String> = jars.iter().map(|j| j.sha1.clone()).collect();
    let checks = Modrinth::new(outside.clone()).updates(&hashes, game)?;
    Ok(jars.iter().cloned().zip(checks).map(|(jar, check)| Known { jar, current: check.current, state: check.state }).collect())
}

// A file name the page names: one plain name, never a path.
fn plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0']) && name.len() < 256
}

fn is_jar(name: &str) -> bool {
    plain_name(name) && name.to_lowercase().ends_with(".jar")
}

// Only these are opened in the browser: a web address, written as one argument.
fn is_web_address(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://")) && url.len() < 2048 && !url.chars().any(|c| c.is_whitespace() || c.is_control() || c == '"')
}

// Whole dotted numbers compare as numbers ("0.18.10" is after "0.18.9"); anything else is only compared for difference.
fn is_newer_loader(latest: &str, current: &str) -> bool {
    let numbers = |v: &str| v.split('.').map(|p| p.parse::<u64>().ok()).collect::<Option<Vec<u64>>>();
    match (numbers(latest), numbers(current)) {
        (Some(l), Some(c)) => l > c,
        _ => latest != current,
    }
}

fn sha1_of(bytes: &[u8]) -> String {
    Sha1::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

// The file of a version, fetched and checked against the checksum Modrinth gives.
fn download(outside: &Arc<dyn Outside>, title: &str, file: &modrinth::File) -> Done<Vec<u8>> {
    if !is_jar(&file.filename) {
        return Err(said(502, format!("Modrinth names a file of {title} that is no jar.")));
    }
    if !file.url.starts_with("https://") {
        return Err(said(502, format!("Modrinth gave no safe address for {title}.")));
    }
    let bytes = outside.get(&file.url).map_err(|e| from_modrinth(modrinth::Error::Http(e)))?;
    if sha1_of(&bytes) != file.hashes.sha1.to_lowercase() {
        return Err(said(502, format!("The file of {title} did not match Modrinth's checksum, nothing was changed.")));
    }
    Ok(bytes)
}

// Writes a jar beside its place and moves it there in one step, so a jar is never half written.
fn place(mods: &Path, name: &str, bytes: &[u8]) -> Done<()> {
    fs::create_dir_all(mods).map_err(|e| io("The mods folder could not be made", e))?;
    let part = mods.join(format!("{name}.part"));
    fs::write(&part, bytes).map_err(|e| io(&format!("{name} could not be written"), e))?;
    fs::rename(&part, mods.join(name)).map_err(|e| {
        let _ = fs::remove_file(&part);
        io(&format!("{name} could not be put in the mods folder"), e)
    })
}

// A move that also crosses disks, where a plain rename cannot.
fn relocate(from: &Path, to: &Path) -> std::io::Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::remove_file(from)
}

impl Api {
    fn mods_profile(&self, id: &str) -> Result<Option<Profile>, Failure> {
        // Without the pack's folders no instance is found: say why, rather than 404.
        self.config()?;
        let Some(cfg) = self.instance_cfg(id) else { return Ok(None) };
        let dir = cfg.parent().expect("a file has a folder").to_path_buf();
        Ok(Some(Profile { mods: super::profiles::game_dir_of(&dir).join("mods"), dir }))
    }

    // Prism keeps its instances in memory and the game holds its jars: nothing of a profile changes while it runs.
    fn mods_locked(&self, outside: &Arc<dyn Outside>) -> bool {
        outside.locked().unwrap_or_else(|| {
            let mut live = self.live();
            super::profiles::prism_running(&mut live)
        })
    }

    pub(super) fn mods_get(&self, path: &str) -> Result<Reply, Failure> {
        let id = path.trim_start_matches("/api/mods/");
        let Some(profile) = self.mods_profile(id)? else { return Ok(Reply::empty(404)) };
        let outside = outside_of(&self.repo);
        Ok(self.page_of(&profile, &outside).unwrap_or_else(|stop| stop))
    }

    pub(super) fn mods_post(&self, path: &str, body: &Value) -> Result<Reply, Failure> {
        let rest = path.trim_start_matches("/api/");
        let outside = outside_of(&self.repo);
        let text = |key: &str| body[key].as_str().unwrap_or_default().to_string();
        let (profile_id, action) = match rest.strip_prefix("mods/").map(|r| r.split_once('/')) {
            Some(Some((profile, action))) => (profile.to_string(), action),
            // The page that adds a mod names its profile in the body.
            Some(None) => (text("profile"), rest.trim_start_matches("mods/")),
            None => (String::new(), rest),
        };
        if matches!(action, "open" | "open-url") {
            return Ok(Self::open_address(&outside, &text("url")));
        }
        let Some(profile) = self.mods_profile(&profile_id)? else { return Ok(Reply::empty(404)) };
        let handled = (|| -> Done<Reply> {
            // Looking is allowed while the game runs; changing is not.
            if action != "search" && self.mods_locked(&outside) {
                return Err(said(409, RUNNING));
            }
            match action {
                "remove" => remove(&profile, &text("mod")),
                "restore" => restore(&profile, &text("mod")),
                "update" => update_one(&profile, &outside, &text("mod")),
                "update-all" => update_all(&profile, &outside),
                "fabric" => fabric(&profile, &outside),
                "add" => add(&profile, &outside, &text("mod")),
                "file" => add_file(&profile, &outside),
                "search" => search(&profile, &outside, &text("query")),
                _ => Ok(Reply::empty(404)),
            }
        })();
        Ok(handled.unwrap_or_else(|stop| stop))
    }

    fn open_address(outside: &Arc<dyn Outside>, url: &str) -> Reply {
        if !is_web_address(url) {
            return said(400, "Only a web address can be opened.");
        }
        match outside.open(url) {
            Ok(()) => Reply::empty(200),
            Err(why) => said(500, format!("The browser could not be opened: {why}")),
        }
    }

    // The profile's page: its game version, Fabric and mods. What Modrinth and Fabric know is asked, and what cannot
    // be reached is left out: the mods are then named by their files and none has a newer version.
    fn page_of(&self, profile: &Profile, outside: &Arc<dyn Outside>) -> Done<Reply> {
        let (_, game, fabric) = profile.pack()?;
        let on_disk = jars(&profile.mods);
        let known = match inspect(outside, &on_disk, &game) {
            Ok(known) => known,
            Err(why) => {
                self.write_log(&format!("The mods of {} could not be looked up: {why}", profile.dir.display()));
                on_disk.into_iter().map(|jar| Known { jar, current: None, state: UpdateState::Unknown }).collect()
            }
        };
        let ids: Vec<String> = known.iter().filter_map(|k| k.current.as_ref().map(|c| c.project_id.clone())).collect::<HashSet<_>>().into_iter().collect();
        let projects = Modrinth::new(outside.clone()).mods(&ids).unwrap_or_default();
        let mut lines: Vec<(bool, bool, String, Value)> = known
            .iter()
            .map(|k| {
                let project = k.current.as_ref().and_then(|c| projects.iter().find(|p| p.id == c.project_id));
                let latest = match &k.state {
                    UpdateState::Newer { latest, .. } => Some(latest.version_number.clone()),
                    _ => None,
                };
                let name = project.map_or_else(|| k.jar.file.clone(), |p| p.title.clone());
                let line = json!({
                    "id": k.jar.file,
                    "slug": project.map(|p| p.slug.clone()),
                    "name": name,
                    "version": k.current.as_ref().map(|c| c.version_number.clone()).unwrap_or_default(),
                    "latest": latest,
                });
                // The ones to update first, then the others by name, the files Modrinth does not name last.
                (latest.is_none(), project.is_none(), name.to_lowercase(), line)
            })
            .collect();
        lines.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
        let fabric = fabric.map(|version| {
            let latest = instance::fabric_loader(&Wire(outside.clone()), &game).ok().filter(|l| is_newer_loader(l, &version));
            json!({ "version": version, "latest": latest })
        });
        Ok(Reply::json(200, json!({
            "gameVersion": game,
            // Changing the game version is not offered yet: the page shows the one it has.
            "gameVersions": [game],
            "fabric": fabric,
            "mods": lines.into_iter().map(|l| l.3).collect::<Vec<_>>(),
        })))
    }
}

// What an earlier remove kept aside stays until another change of the mods: then it is let go.
fn forget_removed(profile: &Profile) {
    let _ = fs::remove_dir_all(profile.aside());
}

fn remove(profile: &Profile, file: &str) -> Done<Reply> {
    let path = profile.mods.join(file);
    if !is_jar(file) || !path.is_file() {
        return Err(said(404, "That mod is not in the profile."));
    }
    forget_removed(profile);
    let aside = profile.aside();
    fs::create_dir_all(&aside).map_err(|e| io("The removed mod could not be kept", e))?;
    relocate(&path, &aside.join(file)).map_err(|e| io(&format!("{file} could not be removed"), e))?;
    Ok(Reply::empty(200))
}

fn restore(profile: &Profile, file: &str) -> Done<Reply> {
    let kept = profile.aside().join(file);
    if !is_jar(file) || !kept.is_file() {
        return Err(said(404, "There is nothing to put back."));
    }
    let back = profile.mods.join(file);
    if back.exists() {
        return Err(said(422, format!("A file named {file} is in the profile again, nothing was put back.")));
    }
    fs::create_dir_all(&profile.mods).map_err(|e| io("The mods folder could not be made", e))?;
    relocate(&kept, &back).map_err(|e| io(&format!("{file} could not be put back"), e))?;
    let _ = fs::remove_dir(profile.aside());
    Ok(Reply::empty(200))
}

// Moves one jar to the version Modrinth offers: the new file is whole before the old one goes.
fn bring_up_to_date(profile: &Profile, outside: &Arc<dyn Outside>, known: &Known) -> Done<()> {
    let UpdateState::Newer { latest, .. } = &known.state else { return Ok(()) };
    let file = latest.primary_file().ok_or_else(|| said(502, "Modrinth lists no file for the newer version."))?;
    let bytes = download(outside, &known.jar.file, file)?;
    place(&profile.mods, &file.filename, &bytes)?;
    if file.filename != known.jar.file {
        fs::remove_file(&known.jar.path).map_err(|e| io(&format!("{} could not be replaced", known.jar.file), e))?;
    }
    Ok(())
}

fn inspected(profile: &Profile, outside: &Arc<dyn Outside>) -> Done<Vec<Known>> {
    let (_, game, _) = profile.pack()?;
    inspect(outside, &jars(&profile.mods), &game).map_err(from_modrinth)
}

fn update_one(profile: &Profile, outside: &Arc<dyn Outside>, which: &str) -> Done<Reply> {
    let known = inspected(profile, outside)?;
    // The profile's page names a file, the page that adds a mod a project.
    let Some(mine) = known.iter().find(|k| k.jar.file == which || k.current.as_ref().is_some_and(|c| c.project_id == which)) else {
        return Err(said(404, "That mod is not in the profile."));
    };
    forget_removed(profile);
    bring_up_to_date(profile, outside, mine)?;
    Ok(Reply::empty(200))
}

fn update_all(profile: &Profile, outside: &Arc<dyn Outside>) -> Done<Reply> {
    let known = inspected(profile, outside)?;
    forget_removed(profile);
    for k in &known {
        bring_up_to_date(profile, outside, k)?;
    }
    Ok(Reply::empty(200))
}

fn fabric(profile: &Profile, outside: &Arc<dyn Outside>) -> Done<Reply> {
    let (text, game, _) = profile.pack()?;
    let loader = instance::fabric_loader(&Wire(outside.clone()), &game).map_err(from_instance)?;
    let written = instance::with_fabric_loader(&text, &loader).map_err(from_instance)?;
    fs::write(profile.pack_file(), written).map_err(|e| io("The profile's mmc-pack.json could not be written", e))?;
    Ok(Reply::empty(200))
}

fn search(profile: &Profile, outside: &Arc<dyn Outside>, query: &str) -> Done<Reply> {
    let (_, game, _) = profile.pack()?;
    let modrinth = Modrinth::new(outside.clone());
    let page = modrinth.discover(&game, query, 0, FOUND).map_err(from_modrinth)?;
    let known = inspect(outside, &jars(&profile.mods), &game).map_err(from_modrinth)?;
    let have = |project: &str| known.iter().find(|k| k.current.as_ref().is_some_and(|c| c.project_id == project));
    // The version a mod not in the profile would bring: asked for several at once.
    let to_add: Vec<&modrinth::Mod> = page.hits.iter().filter(|h| have(&h.id).is_none()).collect();
    let mut versions: Vec<(String, String)> = Vec::new();
    for chunk in to_add.chunks(AT_ONCE) {
        let asked: Vec<(String, String)> = std::thread::scope(|scope| {
            let threads: Vec<_> = chunk
                .iter()
                .map(|hit| {
                    let (modrinth, game) = (&modrinth, &game);
                    scope.spawn(move || {
                        let version = modrinth.compatible_version(&hit.id, game).ok().flatten().map(|v| v.version_number);
                        (hit.id.clone(), version.unwrap_or_default())
                    })
                })
                .collect();
            threads.into_iter().filter_map(|t| t.join().ok()).collect()
        });
        versions.extend(asked);
    }
    let mods: Vec<Value> = page
        .hits
        .iter()
        .map(|hit| {
            let (state, version, newer) = match have(&hit.id) {
                Some(Known { current, state: UpdateState::Newer { latest, .. }, .. }) => {
                    ("update", current.as_ref().map(|c| c.version_number.clone()).unwrap_or_default(), latest.version_number.clone())
                }
                Some(k) => ("added", k.current.as_ref().map(|c| c.version_number.clone()).unwrap_or_default(), String::new()),
                None => ("add", versions.iter().find(|(id, _)| *id == hit.id).map(|(_, v)| v.clone()).unwrap_or_default(), String::new()),
            };
            json!({
                "id": hit.id, "title": hit.title, "description": hit.description, "version": version,
                "url": hit.url(), "state": state, "newVersion": newer,
            })
        })
        .collect();
    Ok(Reply::json(200, json!({ "mods": mods })))
}

fn add(profile: &Profile, outside: &Arc<dyn Outside>, project: &str) -> Done<Reply> {
    if project.is_empty() {
        return Err(said(400, "No mod was named."));
    }
    let (_, game, _) = profile.pack()?;
    let known = inspect(outside, &jars(&profile.mods), &game).map_err(from_modrinth)?;
    let installed: Vec<String> = known.iter().filter_map(|k| k.current.as_ref().map(|c| c.project_id.clone())).collect();
    let resolution = Modrinth::new(outside.clone()).resolve(project, &game, &installed).map_err(from_modrinth)?;
    // Everything is fetched and checked before anything is written: a mod and what it needs come together or not at all.
    let mut fetched = Vec::new();
    for pick in &resolution.picks {
        let file = pick.version.primary_file().ok_or_else(|| said(502, format!("Modrinth lists no file for {}.", pick.info.title)))?;
        if profile.mods.join(&file.filename).exists() {
            return Err(said(422, format!("A file named {} is in the profile already, nothing was added.", file.filename)));
        }
        fetched.push((file.filename.clone(), download(outside, &pick.info.title, file)?));
    }
    forget_removed(profile);
    for (name, bytes) in &fetched {
        place(&profile.mods, name, bytes)?;
    }
    let added: Vec<Value> = resolution
        .picks
        .iter()
        .map(|pick| {
            let by = pick.needed_by.as_ref().and_then(|id| resolution.picks.iter().find(|p| p.info.id == *id)).map(|p| p.info.title.clone());
            json!({ "id": pick.info.id, "title": pick.info.title, "version": pick.version.version_number, "neededBy": by })
        })
        .collect();
    Ok(Reply::json(200, json!({ "added": added })))
}

fn add_file(profile: &Profile, outside: &Arc<dyn Outside>) -> Done<Reply> {
    let Some(chosen) = outside.pick_file() else { return Ok(Reply::empty(200)) };
    let name = chosen.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if !is_jar(&name) || !chosen.is_file() {
        return Err(said(422, "Only a .jar file can be added as a mod."));
    }
    if profile.mods.join(&name).exists() {
        return Err(said(422, format!("A file named {name} is in the profile already, nothing was added.")));
    }
    let bytes = fs::read(&chosen).map_err(|e| io(&format!("{name} could not be read"), e))?;
    forget_removed(profile);
    place(&profile.mods, &name, &bytes)?;
    Ok(Reply::empty(200))
}

/// An `Outside` that answers from a table and remembers what it was asked, for the tests and the pictures: nothing
/// is fetched, opened or shown.
pub mod canned {
    use super::*;

    #[derive(Default)]
    pub struct Canned {
        // (method, a part of the address, the answer): the first that matches.
        rules: Mutex<Vec<(String, String, Vec<u8>)>>,
        // Every request, as "METHOD address body".
        pub asked: Mutex<Vec<String>>,
        pub opened: Mutex<Vec<String>>,
        pub file: Mutex<Option<PathBuf>>,
        pub locked: Mutex<Option<bool>>,
    }

    impl Canned {
        pub fn when_get(&self, part: &str, body: impl Into<Vec<u8>>) -> &Canned {
            self.rules.lock().unwrap().push(("GET".into(), part.into(), body.into()));
            self
        }
        pub fn when_post(&self, part: &str, body: impl Into<Vec<u8>>) -> &Canned {
            self.rules.lock().unwrap().push(("POST".into(), part.into(), body.into()));
            self
        }
        pub fn requests(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }
        fn answer(&self, method: &str, url: &str, body: &str) -> Result<Vec<u8>, HttpError> {
            self.asked.lock().unwrap().push(format!("{method} {url} {body}"));
            let rules = self.rules.lock().unwrap();
            rules
                .iter()
                .find(|(m, part, _)| m == method && url.contains(part.as_str()))
                .map(|(_, _, answer)| answer.clone())
                .ok_or_else(|| HttpError::Unreachable(format!("nothing answers {method} {url}")))
        }
    }

    impl Http for Canned {
        fn get(&self, url: &str) -> Result<Vec<u8>, HttpError> {
            self.answer("GET", url, "")
        }
        fn post_json(&self, url: &str, body: &str) -> Result<Vec<u8>, HttpError> {
            self.answer("POST", url, body)
        }
    }

    impl Outside for Canned {
        fn open(&self, url: &str) -> Result<(), String> {
            self.opened.lock().unwrap().push(url.to_string());
            Ok(())
        }
        fn pick_file(&self) -> Option<PathBuf> {
            self.file.lock().unwrap().clone()
        }
        fn locked(&self) -> Option<bool> {
            // Tests never look at the processes of the machine they run on.
            Some(self.locked.lock().unwrap().unwrap_or(false))
        }
    }
}

#[cfg(test)]
mod tests;
