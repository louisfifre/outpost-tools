// What the server's page (board C1), Home's servers (A1) and the account menu (A2) of the native launcher ask beyond the
// dev server's own routes: the servers of the servers folder (servers.rs) beside the dev server, a server's EULA, a
// link opened in the browser, the accounts Prism keeps, and an account's skin. One hook, `screens_route`, answers
// before api.rs's own routes and passes every request that is not its own on (`Ok(None)`).
//
// Starting and stopping a server of the folder is server_run.rs's; the choice of the folder is made here.
//
// The dev server stays what dev_server.rs makes of it: a request with no `id`, or with the id "dev", is its own. The
// servers of the servers folder are named by their folder, which is their id.
//
// Accounts: the player names of Prism's accounts.json and nothing else of it. The sign-in tokens beside them are never
// read into an answer, kept or sent, and nothing here writes the file. Using an account is Outpost Tools' own choice,
// kept in `run/app/account.txt` and given to Prism at the launch (`--profile`); adding one and signing out wait for
// the Microsoft sign-in, which is not built, and answer so.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde_json::{Value, json};

use super::key_value::{Props, strip_bom};
use super::servers::{self, Server};
use super::skin::{self, Fetch};
use super::{Api, Failure, Reply, as_int};
use crate::system::{self, Proc};

/// The id the dev server answers to.
const DEV: &str = "dev";
const SERVERS_FOLDER_FILE: &str = "servers-folder.txt";
const ACCOUNT_FILE: &str = "account.txt";

// ---------- pure parts ----------

/// The servers folder: the one named in `servers-folder.txt` of Outpost Tools' data folder, else `servers` in it.
pub fn servers_folder(data: &Path) -> PathBuf {
    fs::read_to_string(data.join(SERVERS_FOLDER_FILE))
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| data.join("servers"))
}

/// One value of a query string, with its `%xx` and `+` undone.
pub fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| url_decode(v))
    })
}

fn url_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() => {
                out.push(u8::from_str_radix(&text[i + 1..i + 3], 16).unwrap_or(b'?'));
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Where a server of the servers folder stands, from the processes: one that runs this server's launcher jar (by its
/// whole path) is starting until the log says players can join. Nothing started by hand outside Outpost Tools is told
/// apart: it is the same jar in the same folder.
pub fn state_in(processes: &[Proc], dir: &Path) -> &'static str {
    let marker = servers::process_marker(dir);
    match processes.iter().find(|p| p.name == "java.exe" && p.cmd.contains(&marker)) {
        None => "stopped",
        Some(java) if servers::is_ready(dir, SystemTime::UNIX_EPOCH + Duration::from_secs(java.started)) => "running",
        Some(_) => "starting",
    }
}

/// Where a server stands once the servers this program started itself are counted: those it holds a handle on
/// (`known`, with the time each was started) are starting until the log says players can join; any other is read from
/// the processes.
pub fn state_of(known: &[(PathBuf, SystemTime)], processes: &[Proc], dir: &Path) -> &'static str {
    match known.iter().find(|(d, _)| d == dir) {
        Some((_, since)) if servers::is_ready(dir, *since) => "running",
        Some(_) => "starting",
        None => state_in(processes, dir),
    }
}

fn players_in(dir: &Path) -> u32 {
    fs::read_to_string(dir.join("logs").join("latest.log")).map(|log| servers::players_online(&log)).unwrap_or(0)
}

/// How the game was set up for a server: memory and Java's arguments, kept in `launch.json` of its folder as the dev
/// server keeps them.
pub(super) fn read_launch(dir: &Path) -> (i64, String) {
    let saved: Value = fs::read(dir.join("launch.json")).ok().and_then(|b| serde_json::from_slice(strip_bom(&b)).ok()).unwrap_or(Value::Null);
    (as_int(&saved["memoryMb"]).unwrap_or(6144), saved["javaArgs"].as_str().map(String::from).unwrap_or_else(|| "-XX:+UseZGC".into()))
}

/// What a server was made from, as board C1 says it: its profile, the mods it leaves out, its world and the seed (the
/// one written when it was made, else the one the game wrote in server.properties).
pub fn made_from(server: &Server) -> Value {
    let seed = server.seed.or_else(|| Props::read(&servers::props_file(&server.dir))?.get("level-seed")?.trim().parse().ok());
    let mut made = json!({ "profile": server.profile, "world": server.world_project, "seed": seed });
    if let Some(n) = server.left_out {
        made["leftOut"] = json!(n);
    }
    made
}

fn props_json(dir: &Path) -> Value {
    match servers::read_settings(dir) {
        Some(s) => json!({ "motd": s.motd, "whitelist": s.whitelist, "viewDistance": s.view_distance, "simulationDistance": s.simulation_distance, "maxPlayers": s.max_players, "port": s.port }),
        None => Value::Null,
    }
}

fn port_of(dir: &Path) -> u16 {
    servers::read_settings(dir).map_or(servers::DEFAULT_PORT, |s| s.port)
}

/// One server of the folder as Home's line and the server's page read it.
pub fn server_json(server: &Server, known: &[(PathBuf, SystemTime)], processes: &[Proc]) -> Value {
    let id = server.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let state = state_of(known, processes, &server.dir);
    let (memory, java_args) = read_launch(&server.dir);
    json!({
        "id": id,
        "name": server.name,
        "dir": server.dir.to_string_lossy(),
        "state": state,
        "eula": servers::eula_accepted(&server.dir),
        "players": (state == "running").then(|| players_in(&server.dir)),
        "port": port_of(&server.dir),
        "memoryMb": memory,
        "javaArgs": java_args,
        "props": props_json(&server.dir),
        "madeFrom": made_from(server),
    })
}

/// Home's servers: those of the servers folder by their folder's order, then the dev server's line when the pack's
/// folders are known (`dev`). A folder whose Outpost Tools file cannot be read is listed in `broken`, with why. `folder`
/// is the servers folder itself, for the page to say where servers live.
pub fn servers_json(folder: &Path, known: &[(PathBuf, SystemTime)], processes: &[Proc], dev: Option<Value>) -> Value {
    let list = servers::list_servers(folder);
    let mut lines: Vec<Value> = list.servers.iter().map(|s| server_json(s, known, processes)).collect();
    lines.extend(dev);
    let broken: Vec<Value> = list.errors.iter().map(|(dir, why)| json!({ "folder": dir.to_string_lossy(), "why": why })).collect();
    json!({ "servers": lines, "broken": broken, "folder": folder.to_string_lossy() })
}

// ---- accounts ----

#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub name: String,
    /// The account Prism itself would play with.
    pub active: bool,
}

/// The player names of Prism's accounts.json. Only the name and the active flag are read.
pub fn parse_prism_accounts(bytes: &[u8]) -> Vec<Account> {
    let value: Value = serde_json::from_slice(strip_bom(bytes)).unwrap_or(Value::Null);
    value["accounts"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|a| Some(Account { name: a["profile"]["name"].as_str().filter(|n| !n.is_empty())?.to_string(), active: a["active"].as_bool() == Some(true) }))
                .collect()
        })
        .unwrap_or_default()
}

/// The account in use: the one Outpost Tools' own choice names when it is still among the accounts, else Prism's
/// active one, else the first.
pub fn in_use<'a>(accounts: &'a [Account], chosen: Option<&str>) -> Option<&'a Account> {
    chosen.and_then(|c| accounts.iter().find(|a| a.name == c)).or_else(|| accounts.iter().find(|a| a.active)).or(accounts.first())
}

pub fn accounts_json(accounts: &[Account], chosen: Option<&str>) -> Value {
    let used = in_use(accounts, chosen).map(|a| a.name.clone());
    json!({ "accounts": accounts.iter().map(|a| json!({ "name": a.name, "inUse": Some(&a.name) == used.as_ref() })).collect::<Vec<_>>() })
}

/// The arguments that start an instance in Prism, with the account Outpost Tools chose when it did.
pub fn launch_args<'a>(instance: &'a str, chosen: Option<&'a str>) -> Vec<&'a str> {
    let mut args = vec!["--launch", instance];
    args.extend(chosen.into_iter().flat_map(|name| ["--profile", name]));
    args
}

// ---- the skin ----

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding, for the skin's picture in a JSON answer.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= chunk.len() { B64[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// The answer for the skin of `name`, read through `fetch` and kept under `root`.
pub fn skin_reply(root: &Path, name: &str, fetch: &dyn Fetch, client_jar: Option<&Path>) -> Reply {
    match skin::load(root, name, fetch, client_jar) {
        Ok(s) => Reply::json(200, json!({ "name": s.name, "slim": s.slim, "png": base64_encode(&s.png), "default": s.is_default })),
        Err(skin::SkinError::InvalidName) => Reply::error(400, "That is not an account name."),
        Err(skin::SkinError::NoSuchAccount) => Reply::error(404, "No account has this name."),
        Err(skin::SkinError::Offline(_)) => Reply::error(502, "Mojang could not be reached."),
        Err(other) => Reply::error(500, other.to_string()),
    }
}

// ---------- the routes ----------

fn not_built(sentence: &str) -> Reply {
    Reply::error(501, sentence)
}

impl Api {
    fn data_dir(&self) -> PathBuf {
        self.repo.join("run").join("app")
    }

    fn servers_dir(&self) -> PathBuf {
        servers_folder(&self.data_dir())
    }

    /// A server of the servers folder by its id (its folder's name): only one the folder lists, so an id is never a path.
    fn folder_server(&self, id: &str) -> Option<Server> {
        servers::list_servers(&self.servers_dir()).servers.into_iter().find(|s| s.dir.file_name().is_some_and(|n| n.to_string_lossy() == id))
    }

    fn prism_accounts(&self) -> Vec<Account> {
        let Some(file) = self.config().ok().and_then(|c| c.prism.as_ref().ok()).and_then(|p| Some(p.parent()?.join("accounts.json"))) else { return Vec::new() };
        fs::read(file).map(|b| parse_prism_accounts(&b)).unwrap_or_default()
    }

    fn chosen_account(&self) -> Option<String> {
        fs::read_to_string(self.data_dir().join(ACCOUNT_FILE)).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
    }

    /// The name of the account the game is played with, for the bar.
    pub(super) fn account(&self) -> Option<String> {
        let accounts = self.prism_accounts();
        in_use(&accounts, self.chosen_account().as_deref()).map(|a| a.name.clone())
    }

    /// The account Outpost Tools chose, while it is still one of Prism's: what `--profile` is given at a launch.
    pub(super) fn launch_account(&self) -> Option<String> {
        let chosen = self.chosen_account()?;
        self.prism_accounts().iter().any(|a| a.name == chosen).then_some(chosen)
    }

    fn dev_line(&self) -> Result<Value, Failure> {
        let info = {
            let mut live = self.live();
            self.server_info(&mut live)?
        };
        let state = info["state"].as_str().unwrap_or("stopped");
        let port = info["props"]["port"].as_i64().filter(|p| *p > 0).unwrap_or(25565);
        let players = (state == "running").then(|| players_in(&self.config().map(|c| c.dev_server.clone()).unwrap_or_default()));
        Ok(json!({ "id": DEV, "name": "", "state": state, "eula": info["eula"], "players": players, "port": port }))
    }

    /// What the servers of the folder stand on: those this program started and still holds, and the processes.
    pub(super) fn standing(&self) -> (Vec<(PathBuf, SystemTime)>, Vec<Proc>) {
        let mut live = self.live();
        let known = super::server_run::known_runs(&mut live);
        (known, system::processes(&mut live.system))
    }

    fn servers_list(&self) -> Result<Reply, Failure> {
        let dev = match self.dev_line() {
            Ok(line) => Some(line),
            Err(Failure::Unavailable(why)) => {
                // Without the pack's folders there is no dev server to list: only its absence, unless nothing else is there.
                let others = servers_json(&self.servers_dir(), &[], &[], None);
                if others["servers"].as_array().is_some_and(|l| l.is_empty()) && others["broken"].as_array().is_some_and(|l| l.is_empty()) {
                    return Err(Failure::Unavailable(why));
                }
                None
            }
            Err(other) => return Err(other),
        };
        let (known, processes) = self.standing();
        Ok(Reply::json(200, servers_json(&self.servers_dir(), &known, &processes, dev)))
    }

    fn server_detail(&self, id: &str) -> Reply {
        match self.folder_server(id) {
            Some(server) => {
                let (known, processes) = self.standing();
                Reply::json(200, server_json(&server, &known, &processes))
            }
            None => Reply::empty(404),
        }
    }

    fn accounts_list(&self) -> Reply {
        Reply::json(200, accounts_json(&self.prism_accounts(), self.chosen_account().as_deref()))
    }

    fn skin_of(&self, name: &str) -> Reply {
        let jar = self.config().ok().and_then(|c| c.prism.as_ref().ok()).and_then(|p| skin::find_client_jar(&p.parent()?.join("libraries")));
        skin_reply(&self.data_dir(), name, &skin::Http::new(), jar.as_deref())
    }

    fn use_account(&self, name: &str) -> Reply {
        if !self.prism_accounts().iter().any(|a| a.name == name) {
            return Reply::empty(404);
        }
        let data = self.data_dir();
        match fs::create_dir_all(&data).and_then(|()| fs::write(data.join(ACCOUNT_FILE), name)) {
            Ok(()) => Reply::empty(200),
            Err(e) => Reply::error(500, e.to_string()),
        }
    }

    /// Asks the person for the servers folder and keeps the answer where `servers_folder` reads it. Nothing is chosen
    /// while a server of the present folder runs: it would vanish from Home with its process still going.
    fn choose_servers_folder(&self) -> Reply {
        let folder = self.servers_dir();
        let (known, processes) = self.standing();
        if servers::list_servers(&folder).servers.iter().any(|s| state_of(&known, &processes, &s.dir) != "stopped") {
            return Reply::error(409, "A server is running: stop it before choosing another servers folder.");
        }
        let Some(chosen) = super::mods::outside_of(&self.repo).pick_folder() else { return Reply::empty(200) };
        if !chosen.is_dir() {
            return Reply::error(422, "That is not a folder.");
        }
        let data = self.data_dir();
        match fs::create_dir_all(&data).and_then(|()| fs::write(data.join(SERVERS_FOLDER_FILE), chosen.to_string_lossy().as_bytes())) {
            Ok(()) => Reply::json(200, json!({ "folder": chosen.to_string_lossy() })),
            Err(e) => Reply::error(500, format!("The choice could not be kept: {e}")),
        }
    }

    fn accept_eula(&self, id: &str) -> Result<Reply, Failure> {
        let dir = if id.is_empty() || id == DEV {
            self.config()?.dev_server.clone()
        } else {
            match self.folder_server(id) {
                Some(server) => server.dir,
                None => return Ok(Reply::empty(404)),
            }
        };
        if !dir.is_dir() {
            return Ok(Reply::error(404, "The server's folder is not there."));
        }
        servers::accept_eula(&dir).map_err(|e| format!("eula.txt could not be written: {e}"))?;
        Ok(Reply::empty(200))
    }

    fn folder_settings(&self, server: &Server, body: &Value) -> Result<Reply, Failure> {
        let dir = &server.dir;
        let props = &body["props"];
        let props_exist = servers::props_file(dir).exists();
        if !super::dev_server::settings_valid(body, self.machine_mb, props_exist) {
            return Ok(Reply::empty(400));
        }
        let (known, processes) = self.standing();
        if state_of(&known, &processes, dir) != "stopped" {
            return Ok(Reply::empty(409));
        }
        let launch = json!({ "memoryMb": as_int(&body["memoryMb"]), "javaArgs": super::squeeze(body["javaArgs"].as_str().unwrap_or_default()) });
        fs::write(dir.join("launch.json"), serde_json::to_string_pretty(&launch).unwrap_or_default()).map_err(|e| e.to_string())?;
        if !props.is_null() {
            let number = |key: &str| as_int(&props[key]).unwrap_or(0).max(0) as u32;
            let settings = servers::Settings {
                view_distance: number("viewDistance"),
                simulation_distance: number("simulationDistance"),
                max_players: number("maxPlayers"),
                port: u16::try_from(number("port")).unwrap_or(0),
                motd: props["motd"].as_str().unwrap_or_default().trim().to_string(),
                whitelist: props["whitelist"].as_bool() == Some(true),
            };
            if let Err(why) = servers::write_settings(dir, &settings) {
                return Ok(Reply::error(400, why));
            }
        }
        Ok(Reply::empty(200))
    }

    /// The hook of api.rs's `route`: Some(answer) for what this file answers, None for the rest.
    pub(super) fn screens_route(&self, method: &str, full_path: &str, origin: Option<&str>, body: &[u8]) -> Result<Option<Reply>, Failure> {
        let (path, query) = full_path.split_once('?').unwrap_or((full_path, ""));
        if method == "GET" {
            return Ok(Some(match path {
                "/api/servers" => self.servers_list()?,
                "/api/server" => match query_value(query, "id") {
                    Some(id) if !id.is_empty() && id != DEV => self.server_detail(&id),
                    _ => return Ok(None),
                },
                "/api/accounts" => self.accounts_list(),
                "/api/skin" => self.skin_of(&query_value(query, "name").unwrap_or_default()),
                _ => return Ok(None),
            }));
        }
        let parsed: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let id = parsed["id"].as_str().unwrap_or_default();
        let folder_request = matches!(path, "/api/server/start" | "/api/server/stop" | "/api/server/folder" | "/api/server/settings");
        let mine = match path {
            "/api/server/eula" | "/api/servers/folder" | "/api/open-link" | "/api/accounts/use" | "/api/accounts/add" | "/api/accounts/sign-out" => true,
            _ => folder_request && !id.is_empty() && id != DEV,
        };
        if !mine {
            return Ok(None);
        }
        // Only the page itself may start or change anything, never another page.
        if method != "POST" || origin != Some(self.origin.as_str()) {
            return Ok(Some(Reply::empty(403)));
        }
        Ok(Some(match path {
            "/api/server/eula" => self.accept_eula(id)?,
            "/api/servers/folder" => self.choose_servers_folder(),
            // The same opening as the mods page's (api/mods.rs): web addresses only, through `Outside`.
            "/api/open-link" => self.mods_post("/api/open-url", &parsed)?,
            "/api/accounts/use" => self.use_account(parsed["name"].as_str().unwrap_or_default()),
            "/api/accounts/add" => not_built("Adding an account needs the Microsoft sign-in, which is not built yet."),
            "/api/accounts/sign-out" => not_built("Signing out would delete the sign-in Prism keeps, which is not built yet."),
            _ => {
                let Some(server) = self.folder_server(id) else { return Ok(Some(Reply::empty(404))) };
                match path {
                    "/api/server/folder" => {
                        system::start("explorer.exe", &[&server.dir.to_string_lossy()]).map_err(|e| e.to_string())?;
                        Reply::empty(200)
                    }
                    "/api/server/settings" => self.folder_settings(&server, &parsed)?,
                    "/api/server/start" => self.folder_start(&server)?,
                    _ => self.folder_stop(&server),
                }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::skin::FetchError;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    };

    fn temp_dir(name: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("screens_{name}_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn made_server(folder: &Path, name: &str, left_out: Option<u32>) -> Server {
        let dir = folder.join(name);
        fs::create_dir_all(&dir).unwrap();
        let server = Server { dir, name: name.into(), profile: "exploration".into(), world_project: Some("Tectonic islands".into()), seed: Some(48213), left_out };
        server.write().unwrap();
        server
    }

    fn java(dir: &Path, started: u64) -> Proc {
        Proc { pid: 7, parent: None, name: "java.exe".into(), cmd: format!("java -xmx6144m -jar {} nogui", servers::process_marker(dir)), started }
    }

    #[test]
    fn the_servers_folder_is_the_one_chosen_or_servers_in_the_data_folder() {
        let data = temp_dir("folder");
        assert_eq!(servers_folder(&data), data.join("servers"));
        fs::write(data.join(SERVERS_FOLDER_FILE), "  C:\\Code\\servers \r\n").unwrap();
        assert_eq!(servers_folder(&data), PathBuf::from("C:\\Code\\servers"));
        fs::write(data.join(SERVERS_FOLDER_FILE), "\n").unwrap();
        assert_eq!(servers_folder(&data), data.join("servers"));
        let _ = fs::remove_dir_all(data);
    }

    #[test]
    fn a_query_is_read_with_its_escapes() {
        assert_eq!(query_value("id=Test%20server&x=1", "id").as_deref(), Some("Test server"));
        assert_eq!(query_value("name=a+b", "name").as_deref(), Some("a b"));
        assert_eq!(query_value("id=100%", "id").as_deref(), Some("100%"));
        assert_eq!(query_value("x=1", "id"), None);
        assert_eq!(query_value("", "id"), None);
    }

    #[test]
    fn the_server_says_what_it_was_made_from_before_its_first_start() {
        let folder = temp_dir("made");
        let server = made_server(&folder, "Test server", Some(3));
        let json = server_json(&server, &[], &[]);
        assert_eq!(json["id"], "Test server");
        assert_eq!(json["name"], "Test server");
        assert_eq!(json["madeFrom"], json!({ "profile": "exploration", "world": "Tectonic islands", "seed": 48213, "leftOut": 3 }));
        assert_eq!((json["state"].as_str(), json["eula"].as_bool(), json["props"].is_null()), (Some("stopped"), Some(false), true));
        assert!(json["players"].is_null());
        // The mods left out are unknown for a server made before they were counted, and no world gives no seed.
        let old = made_server(&folder, "Old", None);
        let bare = Server { world_project: None, seed: None, ..old };
        let made = made_from(&bare);
        assert!(made.get("leftOut").is_none() && made["world"].is_null() && made["seed"].is_null());
        // Once the game wrote the properties, a seed that was not kept is read from there.
        fs::write(servers::props_file(&bare.dir), "level-seed=77\nserver-port=25570\n").unwrap();
        assert_eq!(made_from(&bare)["seed"], 77);
        assert_eq!(server_json(&bare, &[], &[])["port"], 25570);
        let _ = fs::remove_dir_all(folder);
    }

    #[test]
    fn a_servers_process_and_its_log_say_whether_it_runs_and_how_many_play() {
        let folder = temp_dir("state");
        let one = made_server(&folder, "Friday world", Some(0));
        let two = made_server(&folder, "Test server", Some(3));
        let log = one.dir.join("logs");
        fs::create_dir_all(&log).unwrap();
        // The process is there and the log says nothing yet: starting. Then the game says it is ready.
        let procs = [java(&one.dir, 0)];
        fs::write(log.join("latest.log"), "[10:00:00] [Server thread/INFO]: Starting minecraft server\n").unwrap();
        assert_eq!(state_in(&procs, &one.dir), "starting");
        fs::write(
            log.join("latest.log"),
            "[10:00:09] [Server thread/INFO]: Done (4.2s)! For help, type \"help\"\n[10:01:00] [Server thread/INFO]: Ana joined the game\n[10:02:00] [Server thread/INFO]: Bo joined the game\n[10:03:00] [Server thread/INFO]: Ana left the game\n[10:03:30] [Server thread/INFO]: Cy joined the game\n[10:04:00] [Server thread/INFO]: <Bo> I left the game\n",
        )
        .unwrap();
        assert_eq!(state_in(&procs, &one.dir), "running");
        // The same process does not make another server's folder run.
        assert_eq!(state_in(&procs, &two.dir), "stopped");
        let list = servers_json(&folder, &[], &procs, Some(json!({ "id": "dev", "name": "", "state": "stopped" })));
        let lines = list["servers"].as_array().unwrap();
        assert_eq!(lines.iter().map(|l| l["id"].as_str().unwrap()).collect::<Vec<_>>(), ["Friday world", "Test server", "dev"]);
        assert_eq!((lines[0]["state"].as_str(), lines[0]["players"].as_u64(), lines[0]["port"].as_u64()), (Some("running"), Some(2), Some(25565)));
        assert_eq!((lines[1]["state"].as_str(), lines[1]["madeFrom"]["leftOut"].as_u64()), (Some("stopped"), Some(3)));
        let _ = fs::remove_dir_all(folder);
    }

    #[test]
    fn a_broken_server_file_is_said_beside_the_list_not_hidden() {
        let folder = temp_dir("broken");
        made_server(&folder, "Good", None);
        fs::create_dir_all(folder.join("Bad")).unwrap();
        fs::write(folder.join("Bad").join(servers::META_FILE), "{ no").unwrap();
        let list = servers_json(&folder, &[], &[], None);
        assert_eq!(list["servers"].as_array().unwrap().len(), 1);
        assert_eq!(list["broken"].as_array().unwrap().len(), 1);
        assert!(list["broken"][0]["why"].as_str().unwrap().contains("not valid"));
        let _ = fs::remove_dir_all(folder);
    }

    // The shape of Prism's accounts.json (docs/research/launcher.md in the pack), with tokens that must never come out.
    const ACCOUNTS: &str = r#"{"accounts":[
        {"active":true,"profile":{"id":"a1","name":"PlayerOne","skin":{"url":"http://x"}},"type":"MSA","ygg":{"token":"SECRET-TOKEN","extra":{"clientToken":"SECRET-CLIENT"}}},
        {"profile":{"id":"b2","name":"SecondAccount"},"type":"MSA","msa-client-id":"x","ygg":{"token":"SECRET-TOKEN-2"}},
        {"type":"Offline"}],"formatVersion":3}"#;

    #[test]
    fn accounts_are_read_by_name_and_never_with_their_tokens() {
        let accounts = parse_prism_accounts(ACCOUNTS.as_bytes());
        assert_eq!(accounts, [Account { name: "PlayerOne".into(), active: true }, Account { name: "SecondAccount".into(), active: false }]);
        let answer = accounts_json(&accounts, None).to_string();
        assert!(!answer.contains("SECRET") && !answer.contains("token"), "{answer}");
        assert_eq!(accounts_json(&accounts, None), json!({"accounts": [{"name": "PlayerOne", "inUse": true}, {"name": "SecondAccount", "inUse": false}]}));
        // Outpost Tools' own choice wins while it is still an account; a name that is gone falls back to Prism's.
        assert_eq!(accounts_json(&accounts, Some("SecondAccount"))["accounts"][1]["inUse"], true);
        assert_eq!(accounts_json(&accounts, Some("Gone"))["accounts"][0]["inUse"], true);
        assert_eq!(in_use(&[Account { name: "Only".into(), active: false }], None).map(|a| a.name.as_str()), Some("Only"));
        assert!(parse_prism_accounts(b"not json").is_empty() && parse_prism_accounts(b"{}").is_empty());
    }

    #[test]
    fn a_launch_names_the_account_only_when_one_was_chosen() {
        assert_eq!(launch_args("pack", None), ["--launch", "pack"]);
        assert_eq!(launch_args("pack", Some("SecondAccount")), ["--launch", "pack", "--profile", "SecondAccount"]);
    }

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    struct Offline;
    impl Fetch for Offline {
        fn get(&self, _: &str) -> Result<Vec<u8>, FetchError> {
            Err(FetchError::Unreachable("no connection".into()))
        }
    }

    #[test]
    fn a_skin_that_cannot_be_read_answers_with_a_sentence_and_the_right_status() {
        let root = temp_dir("skin");
        assert_eq!(skin_reply(&root, "../x", &Offline, None).status, 400);
        let offline = skin_reply(&root, "PlayerOne", &Offline, None);
        assert_eq!(offline.status, 502);
        assert_eq!(serde_json::from_slice::<Value>(&offline.body).unwrap()["error"], "Mojang could not be reached.");
        let _ = fs::remove_dir_all(root);
    }

    fn api_over(repo: &Path) -> Api {
        Api::new(repo.to_path_buf(), "native://test".into())
    }

    #[test]
    fn accepting_the_eula_writes_the_line_in_that_servers_file_only() {
        let repo = temp_dir("eula");
        let folder = repo.join("run").join("app").join("servers");
        let (one, two) = (made_server(&folder, "One", None), made_server(&folder, "Two", None));
        let api = api_over(&repo);
        let ask = |origin: Option<&str>, body: Value| api.handle("POST", "/api/server/eula", origin, body.to_string().as_bytes()).status;
        // Another page may not.
        assert_eq!(ask(Some("https://evil.example"), json!({"id": "One"})), 403);
        assert!(!servers::eula_file(&one.dir).exists());
        // An id is a listed server's folder, never a path.
        assert_eq!(ask(Some("native://test"), json!({"id": "..\\.."})), 404);
        assert_eq!(ask(Some("native://test"), json!({"id": "Three"})), 404);
        assert_eq!(ask(Some("native://test"), json!({"id": "One"})), 200);
        assert_eq!(fs::read_to_string(servers::eula_file(&one.dir)).unwrap(), "eula=true\r\n");
        assert!(servers::eula_accepted(&one.dir) && !servers::eula_file(&two.dir).exists());
        // A file the game wrote keeps its other lines.
        fs::write(servers::eula_file(&two.dir), "#By changing the setting below\r\neula=false\r\n").unwrap();
        assert_eq!(ask(Some("native://test"), json!({"id": "Two"})), 200);
        assert_eq!(fs::read_to_string(servers::eula_file(&two.dir)).unwrap(), "#By changing the setting below\r\neula=true\r\n");
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn a_server_of_the_folder_is_asked_by_its_id_and_the_dev_server_by_none() {
        let repo = temp_dir("detail");
        let folder = repo.join("run").join("app").join("servers");
        made_server(&folder, "Test server", Some(3));
        let api = api_over(&repo);
        let get = |path: &str| api.handle("GET", path, None, b"");
        let reply = get("/api/server?id=Test%20server");
        assert_eq!(reply.status, 200);
        let body: Value = serde_json::from_slice(&reply.body).unwrap();
        assert_eq!((body["name"].as_str(), body["madeFrom"]["leftOut"].as_u64(), body["eula"].as_bool()), (Some("Test server"), Some(3), Some(false)));
        assert_eq!(get("/api/server?id=Nope").status, 404);
        let _ = fs::remove_dir_all(repo);
    }

    // A server process that records what it is told, and ends on "stop" when `ends_on_stop`.
    struct Probe {
        lines: Arc<Mutex<Vec<String>>>,
        killed: Arc<Mutex<bool>>,
        ends_on_stop: bool,
        stopped: bool,
    }
    impl servers::ServerProcess for Probe {
        fn send_line(&mut self, line: &str) -> std::io::Result<()> {
            self.lines.lock().unwrap().push(line.into());
            self.stopped = line == "stop";
            Ok(())
        }
        fn has_exited(&mut self) -> std::io::Result<bool> {
            Ok(*self.killed.lock().unwrap() || (self.ends_on_stop && self.stopped))
        }
        fn kill(&mut self) -> std::io::Result<()> {
            *self.killed.lock().unwrap() = true;
            Ok(())
        }
    }

    // Starts nothing: remembers each command and hands out a Probe.
    #[derive(Default)]
    struct ProbeSpawner {
        commands: Mutex<Vec<servers::LaunchCommand>>,
        lines: Arc<Mutex<Vec<String>>>,
        killed: Arc<Mutex<bool>>,
        ends_on_stop: bool,
    }
    impl servers::Spawner for Arc<ProbeSpawner> {
        fn spawn(&self, c: &servers::LaunchCommand) -> std::io::Result<Box<dyn servers::ServerProcess>> {
            self.commands.lock().unwrap().push(servers::LaunchCommand { java: c.java.clone(), args: c.args.clone(), dir: c.dir.clone() });
            Ok(Box::new(Probe { lines: self.lines.clone(), killed: self.killed.clone(), ends_on_stop: self.ends_on_stop, stopped: false }))
        }
    }

    fn api_running(repo: &Path, spawner: &Arc<ProbeSpawner>, java: Option<PathBuf>) -> Api {
        use crate::api::Runner;
        api_over(repo).with_runner(Runner {
            spawner: Arc::new(spawner.clone()),
            java: Arc::new(move |_| java.clone()),
            stop_timeout: Duration::from_millis(60),
            poll: Duration::from_millis(5),
        })
    }

    fn ask(api: &Api, path: &str, id: &str) -> (u16, Value) {
        let reply = api.handle("POST", path, Some("native://test"), json!({ "id": id }).to_string().as_bytes());
        (reply.status, serde_json::from_slice(&reply.body).unwrap_or(Value::Null))
    }

    fn state_of_one(api: &Api, id: &str) -> (String, Option<u64>) {
        let reply = api.handle("GET", &format!("/api/server?id={id}"), None, b"");
        let body: Value = serde_json::from_slice(&reply.body).unwrap();
        (body["state"].as_str().unwrap().to_string(), body["players"].as_u64())
    }

    // A port nothing listens on, so a test never meets a real server of the PC.
    fn free_port() -> u16 {
        std::net::TcpListener::bind(("0.0.0.0", 0)).unwrap().local_addr().unwrap().port()
    }

    // A server of the folder with all it needs to start, but its EULA (and `java.exe`, a file the test names).
    fn ready_server(repo: &Path, name: &str) -> Server {
        let one = made_server(&repo.join("run").join("app").join("servers"), name, None);
        fs::write(repo.join("java.exe"), b"").unwrap();
        fs::write(one.dir.join(servers::LAUNCHER_JAR), b"").unwrap();
        fs::write(servers::props_file(&one.dir), format!("server-port={}\n", free_port())).unwrap();
        one
    }

    #[test]
    fn a_server_starts_with_its_java_folder_and_memory_and_stops_after_it_saves() {
        let repo = temp_dir("run");
        let one = made_server(&repo.join("run").join("app").join("servers"), "One", None);
        let java = repo.join("java.exe");
        let spawner = Arc::new(ProbeSpawner { ends_on_stop: true, ..ProbeSpawner::default() });
        let api = api_running(&repo, &spawner, Some(java.clone()));
        // Another page may not start anything.
        assert_eq!(api.handle("POST", "/api/server/start", Some("https://evil.example"), br#"{"id":"One"}"#).status, 403);
        assert_eq!(ask(&api, "/api/server/start", "Nope").0, 404);
        // Nothing starts before the EULA is accepted, and the refusal says so.
        let (status, body) = ask(&api, "/api/server/start", "One");
        assert_eq!(status, 412);
        assert!(body["error"].as_str().unwrap().contains("EULA"));
        assert!(!servers::eula_file(&one.dir).exists(), "a refused start writes no eula.txt");
        assert_eq!(ask(&api, "/api/server/eula", "One").0, 200);
        // Then what is still missing: the Java, the launcher file.
        let no_java = api_running(&repo, &spawner, None);
        assert_eq!(ask(&no_java, "/api/server/start", "One").0, 412);
        assert_eq!(ask(&api, "/api/server/start", "One").0, 412, "no java.exe file on disk");
        fs::write(&java, b"").unwrap();
        assert_eq!(ask(&api, "/api/server/start", "One").0, 422);
        fs::write(one.dir.join(servers::LAUNCHER_JAR), b"").unwrap();
        fs::write(servers::props_file(&one.dir), format!("server-port={}\n", free_port())).unwrap();
        fs::write(one.dir.join("launch.json"), r#"{ "memoryMb": 2048, "javaArgs": "-XX:+UseZGC -Dsample=1" }"#).unwrap();
        assert!(spawner.commands.lock().unwrap().is_empty(), "every refusal came before a process");

        assert_eq!(ask(&api, "/api/server/start", "One").0, 200);
        {
            let commands = spawner.commands.lock().unwrap();
            let c = &commands[0];
            assert_eq!((&c.java, &c.dir), (&java, &one.dir));
            assert_eq!(c.args[..3], ["-Xmx2048M", "-XX:+UseZGC", "-Dsample=1"]);
            assert!(c.args.ends_with(&["-jar".into(), one.dir.join(servers::LAUNCHER_JAR).to_string_lossy().into_owned(), "nogui".into()]));
        }
        // It is starting until its log says players can join, then running with the players the log counts.
        assert_eq!(state_of_one(&api, "One"), ("starting".into(), None));
        fs::create_dir_all(one.dir.join("logs")).unwrap();
        let log = "[12:00:01] [Server thread/INFO]: Done (3.1s)!\n[12:00:09] [Server thread/INFO]: Ann joined the game\n[12:00:10] [Server thread/INFO]: Bo joined the game\n";
        fs::write(one.dir.join("logs").join("latest.log"), log).unwrap();
        assert_eq!(state_of_one(&api, "One"), ("running".into(), Some(2)));
        // One start at a time, and settings wait for the stop.
        assert_eq!(ask(&api, "/api/server/start", "One").0, 409);
        let settings = json!({ "id": "One", "memoryMb": 2048, "javaArgs": "" });
        assert_eq!(api.handle("POST", "/api/server/settings", Some("native://test"), settings.to_string().as_bytes()).status, 409);

        let (status, body) = ask(&api, "/api/server/stop", "One");
        assert_eq!((status, body["outcome"].as_str()), (200, Some("saved")));
        assert_eq!(*spawner.lines.lock().unwrap(), ["stop"]);
        assert!(!*spawner.killed.lock().unwrap());
        assert_eq!(state_of_one(&api, "One").0, "stopped");
        assert_eq!(ask(&api, "/api/server/stop", "One").0, 409);
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn a_server_that_does_not_end_after_stop_is_killed() {
        let repo = temp_dir("kill");
        let one = ready_server(&repo, "One");
        servers::accept_eula(&one.dir).unwrap();
        let spawner = Arc::new(ProbeSpawner::default());
        let api = api_running(&repo, &spawner, Some(repo.join("java.exe")));
        assert_eq!(ask(&api, "/api/server/start", "One").0, 200);
        let (status, body) = ask(&api, "/api/server/stop", "One");
        assert_eq!((status, body["outcome"].as_str()), (200, Some("killed")));
        assert_eq!(*spawner.lines.lock().unwrap(), ["stop"]);
        assert!(*spawner.killed.lock().unwrap());
        assert_eq!(state_of_one(&api, "One").0, "stopped");
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn a_server_that_ended_by_itself_can_start_again() {
        let repo = temp_dir("ended");
        let one = ready_server(&repo, "One");
        servers::accept_eula(&one.dir).unwrap();
        let spawner = Arc::new(ProbeSpawner::default());
        let api = api_running(&repo, &spawner, Some(repo.join("java.exe")));
        assert_eq!(ask(&api, "/api/server/start", "One").0, 200);
        // Its process ended (a crash, or "stop" typed elsewhere): none is held for it any more.
        *spawner.killed.lock().unwrap() = true;
        assert_eq!(state_of_one(&api, "One").0, "stopped");
        *spawner.killed.lock().unwrap() = false;
        assert_eq!(ask(&api, "/api/server/start", "One").0, 200);
        assert_eq!(spawner.commands.lock().unwrap().len(), 2);
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn the_servers_folder_is_chosen_in_a_dialog_and_kept_where_it_is_read() {
        use crate::api::mods::{self, canned::Canned};
        let repo = temp_dir("choose");
        let data = repo.join("run").join("app");
        let elsewhere = temp_dir("elsewhere");
        made_server(&elsewhere, "Friday", None);
        let canned = Arc::new(Canned::default());
        mods::install(&repo, canned.clone());
        let api = api_over(&repo);
        let choose = |origin: &str| api.handle("POST", "/api/servers/folder", Some(origin), b"{}");
        assert_eq!(choose("https://evil.example").status, 403);
        // A dialog closed with no choice changes nothing.
        assert_eq!(choose("native://test").status, 200);
        assert!(!data.join(SERVERS_FOLDER_FILE).exists());
        // A choice that is not a folder is refused.
        *canned.folder.lock().unwrap() = Some(elsewhere.join("no-such-folder"));
        assert_eq!(choose("native://test").status, 422);
        assert!(!data.join(SERVERS_FOLDER_FILE).exists());
        *canned.folder.lock().unwrap() = Some(elsewhere.clone());
        let reply = choose("native://test");
        assert_eq!(reply.status, 200);
        assert_eq!(serde_json::from_slice::<Value>(&reply.body).unwrap()["folder"], elsewhere.to_string_lossy().as_ref());
        assert_eq!(servers_folder(&data), elsewhere);
        let listed = servers_json(&servers_folder(&data), &[], &[], None);
        assert_eq!((listed["servers"][0]["id"].as_str(), listed["folder"].as_str()), (Some("Friday"), Some(elsewhere.to_string_lossy().as_ref())));
        // The server of the new folder is the one asked by id.
        assert_eq!(api.handle("GET", "/api/server?id=Friday", None, b"").status, 200);
        let _ = fs::remove_dir_all(repo);
        let _ = fs::remove_dir_all(elsewhere);
    }

    #[test]
    fn no_other_folder_is_chosen_while_a_server_of_this_one_runs() {
        use crate::api::mods::{self, canned::Canned};
        let repo = temp_dir("choose-busy");
        let one = ready_server(&repo, "One");
        servers::accept_eula(&one.dir).unwrap();
        let canned = Arc::new(Canned::default());
        *canned.folder.lock().unwrap() = Some(repo.clone());
        mods::install(&repo, canned);
        let spawner = Arc::new(ProbeSpawner::default());
        let api = api_running(&repo, &spawner, Some(repo.join("java.exe")));
        assert_eq!(ask(&api, "/api/server/start", "One").0, 200);
        assert_eq!(api.handle("POST", "/api/servers/folder", Some("native://test"), b"{}").status, 409);
        assert!(!repo.join("run").join("app").join(SERVERS_FOLDER_FILE).exists());
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn adding_an_account_and_signing_out_say_they_are_not_built() {
        let repo = temp_dir("accounts");
        let api = api_over(&repo);
        for path in ["/api/accounts/add", "/api/accounts/sign-out"] {
            let reply = api.handle("POST", path, Some("native://test"), br#"{"name":"PlayerOne"}"#);
            assert_eq!(reply.status, 501, "{path}");
            assert!(serde_json::from_slice::<Value>(&reply.body).unwrap()["error"].as_str().unwrap().contains("not built"));
            assert_eq!(api.handle("POST", path, None, b"{}").status, 403);
        }
        let _ = fs::remove_dir_all(repo);
    }
}
