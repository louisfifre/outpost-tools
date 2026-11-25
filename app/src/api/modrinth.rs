// Modrinth's public API (v2) for the mods of a profile: the Discover list and the search for a game version and Fabric,
// the version of a mod that fits, its required dependencies, the mods that have a newer version, and where each mod
// runs (game only, server only, both). Nothing here touches the page: the network sits behind `Http`, so the tests run
// on fixtures (tests/fixtures/modrinth, real answers of 2026-11-25, trimmed) and the live call is #[ignore].
// Docs: https://docs.modrinth.com/api/

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    io::{self, Read},
    path::Path,
    time::Duration,
};

use serde::Deserialize;
use serde_json::json;
use sha1::{Digest, Sha1};

const BASE: &str = "https://api.modrinth.com/v2";
const LOADER: &str = "fabric";
const MAX_LIMIT: u32 = 100;

// What the client needs of the network: one GET, one POST with a JSON body, the answer's bytes.
pub trait Http {
    fn get(&self, url: &str) -> Result<Vec<u8>, HttpError>;
    fn post_json(&self, url: &str, body: &str) -> Result<Vec<u8>, HttpError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    // No answer: offline, DNS, timeout.
    Unreachable(String),
    // An answer with a failing status.
    Status(u16),
}

// The real network. Modrinth asks every client for a User-Agent that names it, with no personal data.
pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new() -> UreqHttp {
        let agent = ureq::Agent::config_builder()
            .user_agent(concat!("outpost-tools/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        UreqHttp { agent }
    }
}

impl Default for UreqHttp {
    fn default() -> Self {
        UreqHttp::new()
    }
}

fn from_ureq(error: ureq::Error) -> HttpError {
    match error {
        ureq::Error::StatusCode(code) => HttpError::Status(code),
        other => HttpError::Unreachable(other.to_string()),
    }
}

impl Http for UreqHttp {
    fn get(&self, url: &str) -> Result<Vec<u8>, HttpError> {
        let mut response = self.agent.get(url).call().map_err(from_ureq)?;
        response.body_mut().read_to_vec().map_err(from_ureq)
    }
    fn post_json(&self, url: &str, body: &str) -> Result<Vec<u8>, HttpError> {
        let mut response =
            self.agent.post(url).header("Content-Type", "application/json").send(body).map_err(from_ureq)?;
        response.body_mut().read_to_vec().map_err(from_ureq)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    // Modrinth cannot be reached, or answered with a failing status.
    Http(HttpError),
    // An answer that is not the shape the docs promise.
    Parse(String),
    // No version of `project` (its title when known, else its id) fits the game version with Fabric. `game_versions`
    // lists those it exists for with Fabric, newest first, empty when it has none.
    NoVersion { project: String, game_versions: Vec<String> },
    // A mod to add declares itself incompatible with another one of the set.
    Incompatible { project: String, with: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Http(HttpError::Unreachable(why)) => write!(f, "Modrinth cannot be reached ({why})"),
            Error::Http(HttpError::Status(code)) => write!(f, "Modrinth answered with status {code}"),
            Error::Parse(why) => write!(f, "Modrinth's answer could not be read ({why})"),
            Error::NoVersion { project, game_versions } if game_versions.is_empty() => {
                write!(f, "No version of {project} runs with Fabric")
            }
            Error::NoVersion { project, game_versions } => {
                write!(f, "No version of {project} fits this game version; it exists for {}", game_versions.join(", "))
            }
            Error::Incompatible { project, with } => write!(f, "{project} does not work with {with}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<HttpError> for Error {
    fn from(error: HttpError) -> Error {
        Error::Http(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Required,
    Optional,
    Unsupported,
    #[serde(other)]
    Unknown,
}

// Where a mod runs, from its client_side and server_side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runs {
    // A server must not get it.
    GameOnly,
    ServerOnly,
    Both,
    // Modrinth does not say.
    Unknown,
}

pub fn runs(client: Side, server: Side) -> Runs {
    match (client, server) {
        (Side::Unsupported, Side::Unsupported) => Runs::Unknown,
        (_, Side::Unsupported) => Runs::GameOnly,
        (Side::Unsupported, _) => Runs::ServerOnly,
        (Side::Unknown, _) | (_, Side::Unknown) => Runs::Unknown,
        _ => Runs::Both,
    }
}

// A mod's page, as the search and the project list give it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Mod {
    // A search hit calls it project_id, the project list id.
    #[serde(alias = "project_id")]
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub downloads: u64,
    pub client_side: Side,
    pub server_side: Side,
}

impl Mod {
    pub fn runs(&self) -> Runs {
        runs(self.client_side, self.server_side)
    }
    pub fn url(&self) -> String {
        format!("https://modrinth.com/mod/{}", self.slug)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Page {
    pub hits: Vec<Mod>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub limit: u32,
    #[serde(default)]
    pub total_hits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionType {
    Release,
    Beta,
    Alpha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyType {
    Required,
    Optional,
    Incompatible,
    Embedded,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Dependency {
    #[serde(default)]
    pub project_id: Option<String>,
    // A dependency mostly names a project only, any version of it.
    #[serde(default)]
    pub version_id: Option<String>,
    pub dependency_type: DependencyType,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Hashes {
    pub sha1: String,
    pub sha512: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct File {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub size: u64,
    pub hashes: Hashes,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: VersionType,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    // ISO 8601 in UTC: the strings compare in date order.
    pub date_published: String,
    #[serde(default)]
    pub status: Option<String>,
    pub files: Vec<File>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

impl ModVersion {
    // The file to download: the one marked primary, else the first.
    pub fn primary_file(&self) -> Option<&File> {
        self.files.iter().find(|f| f.primary).or_else(|| self.files.first())
    }
    fn fits(&self, game_version: &str) -> bool {
        self.game_versions.iter().any(|g| g == game_version)
            && self.loaders.iter().any(|l| l == LOADER)
            && self.status.as_deref().is_none_or(|s| s == "listed")
    }
}

// One mod to install: the version chosen, and for a dependency, the mod that needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub info: Mod,
    pub version: ModVersion,
    // The project id of the mod that asked for it first; None for the mod the user chose.
    pub needed_by: Option<String>,
}

// What choosing a mod adds: the mod first, then what it needs, each project once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub picks: Vec<Pick>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    UpToDate,
    Newer { current: ModVersion, latest: ModVersion },
    // Modrinth knows the file, but no version of its mod fits the game version asked.
    NoVersionForGame { current: ModVersion },
    // Modrinth does not know the file (a mod dropped in by hand): it gets no updates.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheck {
    pub sha1: String,
    pub state: UpdateState,
    // The version of the installed file when Modrinth knows it, whatever the state: its mod, its number.
    pub current: Option<ModVersion>,
}

// The SHA-1 of a file, in lower-case hex, which is how Modrinth looks a file up.
pub fn file_sha1(path: &Path) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn parse<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, Error> {
    serde_json::from_slice(bytes).map_err(|e| Error::Parse(e.to_string()))
}

fn rank(kind: VersionType) -> u8 {
    match kind {
        VersionType::Release => 0,
        VersionType::Beta => 1,
        VersionType::Alpha => 2,
    }
}

// What the resolution looks up next: a project (id or slug), or the exact version a dependency pins, with its project
// when the dependency says it.
enum Want {
    Project(String),
    Version(String, Option<String>),
}

pub struct Modrinth<H: Http> {
    http: H,
    base: String,
}

impl<H: Http> Modrinth<H> {
    pub fn new(http: H) -> Modrinth<H> {
        Modrinth { http, base: BASE.to_string() }
    }

    // The Add a mod list: mods for `game_version` with Fabric, in Modrinth's "relevance" order, which with an empty
    // `query` is, as far as read (unverified against the site), the order of its Discover page. A query filters by name.
    pub fn discover(&self, game_version: &str, query: &str, offset: u32, limit: u32) -> Result<Page, Error> {
        // Modrinth answers an error above 100 rather than a shorter page.
        let limit = limit.clamp(1, MAX_LIMIT);
        let facets = json!([["project_type:mod"], [format!("versions:{game_version}")], [format!("categories:{LOADER}")]]);
        let mut url = format!(
            "{}/search?facets={}&index=relevance&offset={offset}&limit={limit}",
            self.base,
            encode(&facets.to_string())
        );
        if !query.trim().is_empty() {
            url.push_str(&format!("&query={}", encode(query.trim())));
        }
        parse(&self.http.get(&url)?)
    }

    // The version of a mod (its id or slug) to install: the newest release that runs with Fabric on `game_version`,
    // else the newest beta, else the newest alpha. None when no version fits.
    pub fn compatible_version(&self, project: &str, game_version: &str) -> Result<Option<ModVersion>, Error> {
        let url = format!(
            "{}/project/{}/version?loaders={}&game_versions={}",
            self.base,
            encode(project),
            encode(&json!([LOADER]).to_string()),
            encode(&json!([game_version]).to_string())
        );
        let versions: Vec<ModVersion> = parse(&self.http.get(&url)?)?;
        Ok(versions.into_iter().filter(|v| v.fits(game_version)).min_by(|a, b| {
            rank(a.version_type).cmp(&rank(b.version_type)).then_with(|| b.date_published.cmp(&a.date_published))
        }))
    }

    // The game versions a mod exists for with Fabric, newest first.
    pub fn game_versions_with_fabric(&self, project: &str) -> Result<Vec<String>, Error> {
        let url = format!("{}/project/{}/version?loaders={}", self.base, encode(project), encode(&json!([LOADER]).to_string()));
        let versions: Vec<ModVersion> = parse(&self.http.get(&url)?)?;
        let mut seen = HashSet::new();
        Ok(versions.into_iter().flat_map(|v| v.game_versions).filter(|g| seen.insert(g.clone())).collect())
    }

    // The mods by project id, in no promised order. Ids that do not exist are left out.
    pub fn mods(&self, ids: &[String]) -> Result<Vec<Mod>, Error> {
        let mut out = Vec::new();
        for chunk in ids.chunks(50) {
            let url = format!("{}/projects?ids={}", self.base, encode(&json!(chunk).to_string()));
            out.extend(parse::<Vec<Mod>>(&self.http.get(&url)?)?);
        }
        Ok(out)
    }

    // What adding `project` takes: its fitting version, then the required dependencies, recursively, each project
    // once. `installed` holds the project ids the profile already has: they are not picked again, nor looked into.
    // Optional and embedded dependencies are left out. Fails on the first mod with no fitting version, or on a declared
    // incompatibility with another mod of the set or with an installed one.
    pub fn resolve(&self, project: &str, game_version: &str, installed: &[String]) -> Result<Resolution, Error> {
        let mut have: HashSet<String> = installed.iter().cloned().collect();
        let mut chosen: Vec<(ModVersion, Option<String>)> = Vec::new();
        let mut queue = VecDeque::from([(Want::Project(project.to_string()), None::<String>)]);
        while let Some((want, by)) = queue.pop_front() {
            // Saves a request when the project is already known by its id.
            if matches!(&want, Want::Project(p) if have.contains(p)) {
                continue;
            }
            let version = self.pick(&want, game_version)?;
            if !have.insert(version.project_id.clone()) {
                continue;
            }
            for dep in version.dependencies.iter().filter(|d| d.dependency_type == DependencyType::Required) {
                let next = match (&dep.version_id, &dep.project_id) {
                    (Some(v), _) => Want::Version(v.clone(), dep.project_id.clone()),
                    (None, Some(p)) => Want::Project(p.clone()),
                    (None, None) => continue,
                };
                queue.push_back((next, Some(version.project_id.clone())));
            }
            chosen.push((version, by));
        }
        let ids: Vec<String> = chosen.iter().map(|(v, _)| v.project_id.clone()).collect();
        let mut infos: HashMap<String, Mod> = self.mods(&ids)?.into_iter().map(|m| (m.id.clone(), m)).collect();
        let mut picks = Vec::new();
        for (version, needed_by) in chosen {
            let info = infos
                .remove(&version.project_id)
                .ok_or_else(|| Error::Parse(format!("project {} is missing from the list", version.project_id)))?;
            picks.push(Pick { info, version, needed_by });
        }
        for pick in &picks {
            let clashes = pick.version.dependencies.iter().filter(|d| d.dependency_type == DependencyType::Incompatible);
            for dep in clashes {
                if let Some(other) = picks.iter().find(|p| Some(&p.info.id) == dep.project_id.as_ref()) {
                    return Err(Error::Incompatible { project: pick.info.title.clone(), with: other.info.title.clone() });
                }
                // A mod the profile already has. Only the new mods' declarations are known here: an installed mod
                // that declares itself incompatible with a new one is not found.
                if let Some(id) = dep.project_id.as_ref().filter(|id| installed.contains(id)) {
                    let with = self.mods(std::slice::from_ref(id)).ok().and_then(|m| m.into_iter().find(|m| m.id == *id)).map_or_else(|| id.clone(), |m| m.title);
                    return Err(Error::Incompatible { project: pick.info.title.clone(), with });
                }
            }
        }
        Ok(Resolution { picks })
    }

    fn pick(&self, want: &Want, game_version: &str) -> Result<ModVersion, Error> {
        let project = match want {
            Want::Version(id, project) => {
                let version: ModVersion = parse(&self.http.get(&format!("{}/version/{}", self.base, encode(id)))?)?;
                if version.fits(game_version) {
                    return Ok(version);
                }
                // A pinned version that does not fit: another version of its project may.
                project.clone().unwrap_or(version.project_id)
            }
            Want::Project(p) => p.clone(),
        };
        if let Some(version) = self.compatible_version(&project, game_version)? {
            return Ok(version);
        }
        let title = self.mods(std::slice::from_ref(&project)).ok().and_then(|m| m.into_iter().next()).map(|m| m.title);
        Err(Error::NoVersion { project: title.unwrap_or_else(|| project.clone()), game_versions: self.game_versions_with_fabric(&project)? })
    }

    // For each file hash (SHA-1, lower-case hex) of the installed mods: is there a newer version of its mod for
    // `game_version` with Fabric. Asked with another game version than the profile's, NoVersionForGame names the mods
    // that cannot follow. The answers come in the order of `hashes`.
    pub fn updates(&self, hashes: &[String], game_version: &str) -> Result<Vec<UpdateCheck>, Error> {
        if hashes.is_empty() {
            return Ok(Vec::new());
        }
        // Modrinth's keys are lower-case hex: a hash given in capitals is asked and matched all the same.
        let asked: Vec<String> = hashes.iter().map(|h| h.to_lowercase()).collect();
        let current_body = json!({ "hashes": asked, "algorithm": "sha1" }).to_string();
        let latest_body =
            json!({ "hashes": asked, "algorithm": "sha1", "loaders": [LOADER], "game_versions": [game_version] }).to_string();
        let lower = |m: HashMap<String, ModVersion>| -> HashMap<String, ModVersion> {
            m.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect()
        };
        let current = lower(parse(&self.http.post_json(&format!("{}/version_files", self.base), &current_body)?)?);
        let latest = lower(parse(&self.http.post_json(&format!("{}/version_files/update", self.base), &latest_body)?)?);
        let mut checks = Vec::new();
        for (hash, key) in hashes.iter().zip(&asked) {
            let state = match (current.get(key), latest.get(key)) {
                (None, _) => UpdateState::Unknown,
                (Some(current), None) => UpdateState::NoVersionForGame { current: current.clone() },
                // The endpoint offers the newest of any kind. A mod installed as a release is only moved to a
                // release: a beta or an alpha is offered to a mod that is already one.
                (Some(current), Some(latest))
                    if current.version_type == VersionType::Release && latest.version_type != VersionType::Release =>
                {
                    match self.compatible_version(&current.project_id, game_version)?.filter(|v| v.version_type == VersionType::Release) {
                        Some(release) if is_newer(&release, current) => UpdateState::Newer { current: current.clone(), latest: release },
                        Some(_) => UpdateState::UpToDate,
                        None if current.fits(game_version) => UpdateState::UpToDate,
                        None => UpdateState::NoVersionForGame { current: current.clone() },
                    }
                }
                (Some(current), Some(latest)) if is_newer(latest, current) => {
                    UpdateState::Newer { current: current.clone(), latest: latest.clone() }
                }
                _ => UpdateState::UpToDate,
            };
            checks.push(UpdateCheck { sha1: hash.clone(), state, current: current.get(key).cloned() });
        }
        Ok(checks)
    }
}

fn is_newer(candidate: &ModVersion, current: &ModVersion) -> bool {
    candidate.id != current.id && candidate.date_published > current.date_published
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/modrinth").join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    // Answers by rule: the first (method, url fragment) that matches. Every request is kept for the asserts.
    struct Fake {
        rules: Vec<(&'static str, String, Result<String, HttpError>)>,
        log: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new() -> Fake {
            Fake { rules: Vec::new(), log: RefCell::new(Vec::new()) }
        }
        fn get(mut self, fragment: &str, body: impl Into<String>) -> Fake {
            self.rules.push(("GET", fragment.to_string(), Ok(body.into())));
            self
        }
        fn post(mut self, fragment: &str, body: impl Into<String>) -> Fake {
            self.rules.push(("POST", fragment.to_string(), Ok(body.into())));
            self
        }
        fn fail(mut self, fragment: &str, error: HttpError) -> Fake {
            self.rules.push(("GET", fragment.to_string(), Err(error)));
            self
        }
        fn answer(&self, method: &str, url: &str, body: &str) -> Result<Vec<u8>, HttpError> {
            self.log.borrow_mut().push(format!("{method} {url} {body}"));
            self.rules
                .iter()
                .find(|(m, fragment, _)| *m == method && url.contains(fragment.as_str()))
                .unwrap_or_else(|| panic!("no rule for {method} {url}"))
                .2
                .clone()
                .map(String::into_bytes)
        }
    }

    impl Http for Fake {
        fn get(&self, url: &str) -> Result<Vec<u8>, HttpError> {
            self.answer("GET", url, "")
        }
        fn post_json(&self, url: &str, body: &str) -> Result<Vec<u8>, HttpError> {
            self.answer("POST", url, body)
        }
    }

    fn client(fake: Fake) -> Modrinth<Fake> {
        Modrinth::new(fake)
    }

    const GAME: &str = "1.21.1";
    const FABRIC_API: &str = "P7dR8mSH";
    const PLACEHOLDER_API: &str = "eXts2L7r";
    const MODMENU: &str = "mOgUt4GM";

    #[test]
    fn discover_without_a_query_asks_for_game_version_fabric_and_relevance() {
        let api = client(Fake::new().get("/search", fixture("search.json")));
        let page = api.discover(GAME, "", 0, 4).unwrap();
        let log = api.http.log.borrow();
        let url = &log[0];
        assert!(url.contains(&encode(r#"["versions:1.21.1"]"#)), "{url}");
        assert!(url.contains(&encode(r#"["categories:fabric"]"#)), "{url}");
        assert!(url.contains(&encode(r#"["project_type:mod"]"#)), "{url}");
        assert!(url.contains("index=relevance") && url.contains("limit=4") && !url.contains("query="), "{url}");
        assert_eq!(page.hits.len(), 4);
        assert_eq!(page.total_hits, 20562);
        // The order of the answer is kept.
        assert_eq!(page.hits[0].slug, "fabric-api");
        assert_eq!(page.hits[0].id, FABRIC_API);
    }

    #[test]
    fn a_search_by_name_sends_the_query_encoded() {
        let api = client(Fake::new().get("/search", fixture("search_sodium.json")));
        let page = api.discover(GAME, " sodium & co ", 0, 2).unwrap();
        assert!(api.http.log.borrow()[0].contains("&query=sodium%20%26%20co"));
        assert_eq!(page.hits[0].title, "Sodium");
    }

    #[test]
    fn each_hit_says_where_it_runs() {
        let api = client(Fake::new().get("/search", fixture("search_sodium.json")));
        let sodium = &api.discover(GAME, "sodium", 0, 2).unwrap().hits[0];
        assert_eq!((sodium.client_side, sodium.server_side), (Side::Required, Side::Unsupported));
        assert_eq!(sodium.runs(), Runs::GameOnly);
        assert_eq!(sodium.url(), "https://modrinth.com/mod/sodium");
        let api = client(Fake::new().get("/search", fixture("search.json")));
        assert_eq!(api.discover(GAME, "", 0, 4).unwrap().hits[0].runs(), Runs::Both);
    }

    #[test]
    fn runs_from_the_two_sides() {
        use Side::*;
        assert_eq!(runs(Required, Unsupported), Runs::GameOnly);
        assert_eq!(runs(Optional, Unsupported), Runs::GameOnly);
        assert_eq!(runs(Unknown, Unsupported), Runs::GameOnly);
        assert_eq!(runs(Unsupported, Required), Runs::ServerOnly);
        assert_eq!(runs(Required, Required), Runs::Both);
        assert_eq!(runs(Optional, Optional), Runs::Both);
        assert_eq!(runs(Required, Optional), Runs::Both);
        assert_eq!(runs(Unknown, Required), Runs::Unknown);
        assert_eq!(runs(Unsupported, Unsupported), Runs::Unknown);
    }

    #[test]
    fn a_side_not_in_the_docs_is_unknown_not_an_error() {
        let page: Page = serde_json::from_str(
            r#"{"hits":[{"project_id":"x","slug":"x","title":"X","client_side":"sometimes","server_side":"required"}]}"#,
        )
        .unwrap();
        assert_eq!(page.hits[0].runs(), Runs::Unknown);
    }

    #[test]
    fn the_version_is_the_newest_release_over_a_newer_beta() {
        let api = client(Fake::new().get("/project/sodium/version", fixture("versions_sodium.json")));
        let version = api.compatible_version("sodium", GAME).unwrap().unwrap();
        // The fixture holds two newer betas ahead of this release.
        assert_eq!(version.version_number, "mc1.21.1-0.8.13-fabric");
        assert_eq!(version.version_type, VersionType::Release);
        let log = api.http.log.borrow();
        assert!(log[0].contains(&encode(r#"["fabric"]"#)) && log[0].contains(&encode(r#"["1.21.1"]"#)), "{}", log[0]);
        assert!(version.primary_file().unwrap().filename.ends_with(".jar"));
    }

    #[test]
    fn a_beta_is_taken_when_there_is_no_release() {
        let mut versions: Vec<serde_json::Value> = serde_json::from_str(&fixture("versions_sodium.json")).unwrap();
        versions.retain(|v| v["version_type"] == "beta");
        let api = client(Fake::new().get("/version", serde_json::to_string(&versions).unwrap()));
        let version = api.compatible_version("sodium", GAME).unwrap().unwrap();
        assert_eq!(version.version_number, "mc1.21.1-0.8.14-beta.2-fabric");
    }

    #[test]
    fn a_version_for_another_game_version_does_not_fit() {
        let api = client(Fake::new().get("/version", fixture("versions_sodium.json")));
        assert_eq!(api.compatible_version("sodium", "1.20.1").unwrap(), None);
    }

    fn real_modrinth() -> Modrinth<Fake> {
        client(
            Fake::new()
                .get("/project/modmenu/version", fixture("versions_modmenu.json"))
                .get(&format!("/project/{FABRIC_API}/version"), fixture("versions_fabric_api.json"))
                .get(&format!("/project/{PLACEHOLDER_API}/version"), fixture("versions_placeholder_api.json"))
                .get("/projects?ids=", fixture("projects.json")),
        )
    }

    #[test]
    fn a_mod_comes_with_its_required_dependencies_named() {
        let resolution = real_modrinth().resolve("modmenu", GAME, &[]).unwrap();
        let slugs: Vec<_> = resolution.picks.iter().map(|p| p.info.slug.as_str()).collect();
        assert_eq!(slugs, ["modmenu", "fabric-api", "placeholder-api"]);
        let needed: Vec<_> = resolution.picks.iter().map(|p| p.needed_by.as_deref()).collect();
        assert_eq!(needed, [None, Some(MODMENU), Some(MODMENU)]);
        assert!(resolution.picks.iter().all(|p| p.version.primary_file().is_some()));
    }

    #[test]
    fn an_installed_dependency_is_not_added_again() {
        let api = real_modrinth();
        let resolution = api.resolve("modmenu", GAME, &[FABRIC_API.to_string()]).unwrap();
        let slugs: Vec<_> = resolution.picks.iter().map(|p| p.info.slug.as_str()).collect();
        assert_eq!(slugs, ["modmenu", "placeholder-api"]);
        assert!(!api.http.log.borrow().iter().any(|r| r.contains(&format!("/project/{FABRIC_API}/version"))));
    }

    // Versions built by hand: `deps` are (project, type) pairs.
    fn version(project: &str, deps: &[(&str, &str)]) -> serde_json::Value {
        json!({
            "id": format!("v-{project}"), "project_id": project, "name": project, "version_number": "1",
            "version_type": "release", "game_versions": [GAME], "loaders": ["fabric"],
            "date_published": "2026-01-01T00:00:00Z", "status": "listed",
            "files": [{ "url": format!("https://cdn.example/{project}.jar"), "filename": format!("{project}.jar"),
                        "primary": true, "size": 1, "hashes": { "sha1": "a", "sha512": "b" } }],
            "dependencies": deps.iter().map(|(p, t)| json!({ "project_id": p, "version_id": null, "dependency_type": t })).collect::<Vec<_>>()
        })
    }

    fn projects(ids: &[&str]) -> String {
        let all: Vec<_> = ids
            .iter()
            .map(|id| json!({ "id": id, "slug": id, "title": id.to_uppercase(), "client_side": "required", "server_side": "required" }))
            .collect();
        serde_json::Value::Array(all).to_string()
    }

    fn listing(version: serde_json::Value) -> String {
        json!([version]).to_string()
    }

    #[test]
    fn a_shared_dependency_and_a_cycle_give_each_project_once() {
        // a needs b and c, b needs c, c needs a again.
        let api = client(
            Fake::new()
                .get("/project/a/version", listing(version("a", &[("b", "required"), ("c", "required")])))
                .get("/project/b/version", listing(version("b", &[("c", "required")])))
                .get("/project/c/version", listing(version("c", &[("a", "required"), ("d", "optional"), ("e", "embedded")])))
                .get("/projects?ids=", projects(&["a", "b", "c"])),
        );
        let resolution = api.resolve("a", GAME, &[]).unwrap();
        let ids: Vec<_> = resolution.picks.iter().map(|p| p.info.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
        // c was asked for by a first.
        assert_eq!(resolution.picks[2].needed_by.as_deref(), Some("a"));
        let projects_asked = api.http.log.borrow().iter().filter(|r| r.contains("/projects?ids=")).count();
        assert_eq!(projects_asked, 1);
    }

    #[test]
    fn a_dependency_that_pins_a_version_uses_it() {
        let pinned = json!({ "project_id": "b", "version_id": "v-b", "dependency_type": "required" });
        let mut a = version("a", &[]);
        a["dependencies"] = json!([pinned]);
        let api = client(
            Fake::new()
                .get("/project/a/version", listing(a))
                .get("/version/v-b", version("b", &[]).to_string())
                .get("/projects?ids=", projects(&["a", "b"])),
        );
        let resolution = api.resolve("a", GAME, &[]).unwrap();
        assert_eq!(resolution.picks.len(), 2);
        assert_eq!(resolution.picks[1].version.id, "v-b");
    }

    #[test]
    fn a_dependency_without_a_fitting_version_names_the_mod_and_the_versions_that_exist() {
        let api = client(
            Fake::new()
                .get("/project/a/version", listing(version("a", &[("b", "required")])))
                // Asked with the game version: nothing. Asked without: what exists.
                .get("game_versions=", "[]")
                .get("/project/b/version", fixture("versions_sodium_any.json"))
                .get("/projects?ids=", projects(&["b"])),
        );
        match api.resolve("a", GAME, &[]) {
            Err(Error::NoVersion { project, game_versions }) => {
                assert_eq!(project, "B");
                assert!(game_versions.iter().any(|g| g == "1.20.1"), "{game_versions:?}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_mod_incompatible_with_another_of_the_set_is_refused() {
        let api = client(
            Fake::new()
                .get("/project/a/version", listing(version("a", &[("b", "required"), ("c", "required")])))
                .get("/project/b/version", listing(version("b", &[("c", "incompatible")])))
                .get("/project/c/version", listing(version("c", &[])))
                .get("/projects?ids=", projects(&["a", "b", "c"])),
        );
        assert_eq!(api.resolve("a", GAME, &[]), Err(Error::Incompatible { project: "B".into(), with: "C".into() }));
    }

    #[test]
    fn modrinth_unreachable_is_one_error() {
        let api = client(Fake::new().fail("/search", HttpError::Unreachable("dns".into())).fail("/project", HttpError::Status(404)));
        assert_eq!(api.discover(GAME, "", 0, 4), Err(Error::Http(HttpError::Unreachable("dns".into()))));
        assert_eq!(api.compatible_version("nope", GAME), Err(Error::Http(HttpError::Status(404))));
        let sentence = Error::Http(HttpError::Unreachable("dns".into())).to_string();
        assert!(sentence.starts_with("Modrinth cannot be reached"));
    }

    const OLD_SODIUM: &str = "d67e66ea4bb2409997b636dae4203d33764cdcc8";
    const UNKNOWN_FILE: &str = "0000000000000000000000000000000000000000";

    fn updates_with(latest: String) -> Vec<UpdateCheck> {
        updates_with_current(latest, fixture("current.json"))
    }

    fn updates_with_current(latest: String, current: String) -> Vec<UpdateCheck> {
        let api = client(
            Fake::new()
                .post("/version_files/update", latest)
                .post("/version_files", current)
                // What the client asks when the newest version is not a release.
                .get("/project/AANobbMI/version", fixture("versions_sodium.json")),
        );
        let checks = api.updates(&[OLD_SODIUM.to_string(), UNKNOWN_FILE.to_string()], GAME).unwrap();
        let log = api.http.log.borrow();
        let latest_request = log.iter().find(|r| r.contains("/version_files/update")).unwrap();
        assert!(latest_request.contains(r#""loaders":["fabric"]"#) && latest_request.contains(r#""game_versions":["1.21.1"]"#));
        assert!(latest_request.contains(r#""algorithm":"sha1""#));
        checks
    }

    #[test]
    fn an_installed_mod_with_a_newer_version_is_named() {
        let checks = updates_with(fixture("update.json"));
        assert_eq!(checks[0].sha1, OLD_SODIUM);
        match &checks[0].state {
            UpdateState::Newer { current, latest } => {
                assert_eq!(current.version_number, "mc1.21-0.5.11");
                // The installed one is a release: the newest beta is not offered, the newest release is.
                assert_eq!(latest.version_number, "mc1.21.1-0.8.13-fabric");
            }
            other => panic!("{other:?}"),
        }
        // A file Modrinth does not know gets no updates.
        assert_eq!(checks[1].state, UpdateState::Unknown);
    }

    #[test]
    fn an_installed_beta_is_offered_the_newer_beta() {
        let mut current: serde_json::Value = serde_json::from_str(&fixture("current.json")).unwrap();
        current.as_object_mut().unwrap().values_mut().for_each(|v| v["version_type"] = json!("beta"));
        let checks = updates_with_current(fixture("update.json"), current.to_string());
        match &checks[0].state {
            UpdateState::Newer { latest, .. } => assert_eq!(latest.version_number, "mc1.21.1-0.8.14-beta.2-fabric"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_release_with_only_a_newer_beta_is_up_to_date_or_cannot_follow() {
        let only_betas = |keep: &str| -> Fake {
            let mut versions: Vec<serde_json::Value> = serde_json::from_str(&fixture("versions_sodium.json")).unwrap();
            versions.retain(|v| v["version_type"] == "beta");
            let mut current: serde_json::Value = serde_json::from_str(&fixture("current.json")).unwrap();
            current.as_object_mut().unwrap().values_mut().for_each(|v| v["game_versions"] = json!([keep]));
            Fake::new()
                .post("/version_files/update", fixture("update.json"))
                .post("/version_files", current.to_string())
                .get("/project/AANobbMI/version", serde_json::to_string(&versions).unwrap())
        };
        let hash = [OLD_SODIUM.to_string()];
        // The installed release already runs on the game version asked: nothing to offer.
        let state = client(only_betas(GAME)).updates(&hash, GAME).unwrap().remove(0).state;
        assert_eq!(state, UpdateState::UpToDate);
        // It runs on another one, and only betas exist for this one: it cannot follow with a release.
        let state = client(only_betas("1.20.1")).updates(&hash, GAME).unwrap().remove(0).state;
        assert!(matches!(state, UpdateState::NoVersionForGame { .. }), "{state:?}");
    }

    #[test]
    fn a_hash_in_capitals_is_asked_and_matched_in_lower_case() {
        let api = client(Fake::new().post("/version_files/update", fixture("current.json")).post("/version_files", fixture("current.json")));
        let checks = api.updates(&[OLD_SODIUM.to_uppercase()], GAME).unwrap();
        assert_eq!(checks[0].state, UpdateState::UpToDate);
        assert_eq!(checks[0].sha1, OLD_SODIUM.to_uppercase());
        assert!(api.http.log.borrow()[0].contains(OLD_SODIUM) && !api.http.log.borrow()[0].contains(&OLD_SODIUM.to_uppercase()));
    }

    #[test]
    fn a_search_asks_for_at_most_100() {
        let api = client(Fake::new().get("/search", fixture("search.json")));
        api.discover(GAME, "", 0, 5000).unwrap();
        api.discover(GAME, "", 0, 0).unwrap();
        let log = api.http.log.borrow();
        assert!(log[0].contains("limit=100 "), "{}", log[0]);
        assert!(log[1].contains("limit=1 "), "{}", log[1]);
    }

    #[test]
    fn a_mod_incompatible_with_an_installed_one_is_refused() {
        let api = client(
            Fake::new()
                .get("/project/a/version", listing(version("a", &[("x", "incompatible")])))
                .get("/projects?ids=", projects(&["a", "x"])),
        );
        assert_eq!(
            api.resolve("a", GAME, &["x".to_string()]),
            Err(Error::Incompatible { project: "A".into(), with: "X".into() })
        );
        // Not installed: no clash.
        assert!(api.resolve("a", GAME, &[]).is_ok());
    }

    #[test]
    fn an_installed_mod_that_is_the_latest_is_up_to_date() {
        let checks = updates_with(fixture("current.json"));
        assert_eq!(checks[0].state, UpdateState::UpToDate);
        // Whatever the state, the version Modrinth knows the file as comes with it; a file it does not know has none.
        assert_eq!(checks[0].current.as_ref().unwrap().project_id, "AANobbMI");
        assert_eq!(checks[1].current, None);
    }

    #[test]
    fn an_installed_mod_with_no_version_for_the_game_cannot_follow() {
        let checks = updates_with("{}".to_string());
        assert!(matches!(checks[0].state, UpdateState::NoVersionForGame { .. }));
        assert_eq!(checks[1].state, UpdateState::Unknown);
    }

    #[test]
    fn no_hash_means_no_request() {
        let api = client(Fake::new());
        assert_eq!(api.updates(&[], GAME), Ok(Vec::new()));
        assert!(api.http.log.borrow().is_empty());
    }

    #[test]
    fn the_hash_of_a_file_is_its_sha1() {
        let dir = std::env::temp_dir().join(format!("outpost-modrinth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("abc.jar");
        std::fs::write(&file, b"abc").unwrap();
        let hash = file_sha1(&file);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(hash.unwrap(), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    // Live: public metadata only, with the program's User-Agent.
    #[test]
    #[ignore = "uses the network"]
    fn live_discover_for_a_game_version() {
        let api = Modrinth::new(UreqHttp::new());
        let page = api.discover(GAME, "", 0, 5).unwrap();
        assert!(!page.hits.is_empty());
        let fabric_api = api.compatible_version("fabric-api", GAME).unwrap().unwrap();
        assert!(fabric_api.primary_file().is_some());
    }
}
