// Everything the page asks for, by path, as scripts/launcher.ps1 answered it before: the profiles are Prism
// Launcher's instances, their settings are read from and written to each instance.cfg so the page and Prism always
// agree, the dev server is scripts/dev-server.ps1 in a console of its own, and the world preview is the sibling
// repository's own server. The page is launcher/index.html, read from the disk at each load: a change to it shows
// on a reload of the window, with nothing to build.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Child,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use sysinfo::System;

use crate::system::{self, Proc};

pub const PREVIEW_URL: &str = "http://localhost:25590/";

pub struct Reply {
    pub status: u16,
    pub kind: &'static str,
    pub body: Vec<u8>,
}

impl Reply {
    fn json(status: u16, value: Value) -> Reply {
        Reply { status, kind: "application/json; charset=utf-8", body: value.to_string().into_bytes() }
    }
    fn empty(status: u16) -> Reply {
        Reply::json(status, json!({}))
    }
    fn error(status: u16, sentence: impl Into<String>) -> Reply {
        Reply::json(status, json!({ "error": sentence.into() }))
    }
}

// The folders the pack's scripts agree on, asked once from scripts/common.ps1 so they are written in one place.
struct Config {
    dev_server: PathBuf,
    preview_dir: PathBuf,
    preview_exports: PathBuf,
    // Prism Launcher's program, or why it was not found.
    prism: Result<PathBuf, String>,
}

// The last launch asked from the page, and what it was last seen doing.
struct Launch {
    id: String,
    at: Instant,
    saw_game: bool,
    prepared: Option<Instant>,
}

// The last start of the dev server asked from the page: the console it runs in.
struct ServerLaunch {
    console: Child,
    saw_java: bool,
    busy: Instant,
}

struct Live {
    launch: Option<Launch>,
    server: Option<ServerLaunch>,
    preview: Option<Child>,
    system: System,
}

pub struct Api {
    pub repo: PathBuf,
    // The only origin allowed to start or change anything: the page's own.
    origin: String,
    config: OnceLock<Config>,
    live: Mutex<Live>,
    machine_mb: u64,
    log: PathBuf,
}

// The server.properties keys the page shows, with the bounds a value must keep.
const SERVER_KEYS: [(&str, &str, i64, i64); 4] = [
    ("viewDistance", "view-distance", 3, 32),
    ("simulationDistance", "simulation-distance", 5, 32),
    ("maxPlayers", "max-players", 1, 100),
    ("port", "server-port", 1024, 65535),
];

impl Api {
    pub fn new(repo: PathBuf, origin: String) -> Api {
        let mut system = System::new();
        system.refresh_memory();
        let machine_mb = system.total_memory() / (1024 * 1024);
        let log = repo.join("run").join("launcher.log");
        Api {
            repo,
            origin,
            config: OnceLock::new(),
            live: Mutex::new(Live { launch: None, server: None, preview: None, system }),
            machine_mb,
            log,
        }
    }

    // Why something failed, kept in run\launcher.log: the only trace once the window is gone.
    pub fn write_log(&self, line: &str) {
        use std::io::Write;
        let _ = fs::create_dir_all(self.log.parent().unwrap());
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&self.log) {
            let _ = writeln!(file, "{} {line}", now_text());
        }
    }

    fn config(&self) -> &Config {
        self.config.get_or_init(|| {
            let common = self.repo.join("scripts").join("common.ps1");
            let script = format!(
                ". '{}'; $prism = try {{ Find-Prism }} catch {{ \"!$($_.Exception.Message)\" }}; \
                 ConvertTo-Json -Compress @{{ devServer = $DevServerDir; previewDir = $PreviewDir; previewExports = $PreviewExports; prism = $prism }}",
                common.display()
            );
            let out = system::hidden("pwsh").args(["-NoProfile", "-Command", &script]).output();
            let value: Value = out.ok().and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or(Value::Null);
            let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
            let prism = text("prism");
            Config {
                dev_server: PathBuf::from(text("devServer")),
                preview_dir: PathBuf::from(text("previewDir")),
                preview_exports: PathBuf::from(text("previewExports")),
                prism: match prism.strip_prefix('!') {
                    Some(why) => Err(why.to_string()),
                    None if prism.is_empty() => Err("scripts/common.ps1 could not be read.".into()),
                    None => Ok(PathBuf::from(prism)),
                },
            }
        })
    }

    // One request, whatever brought it: the window's own protocol or the local server for captures.
    pub fn handle(&self, method: &str, path: &str, origin: Option<&str>, body: &[u8]) -> Reply {
        match self.route(method, path, origin, body) {
            Ok(reply) => reply,
            Err(why) => {
                self.write_log(&format!("{method} {path} failed: {why}"));
                Reply::error(500, why)
            }
        }
    }

    fn route(&self, method: &str, path: &str, origin: Option<&str>, body: &[u8]) -> Result<Reply, String> {
        let path = path.split('?').next().unwrap_or(path);
        if method == "GET" {
            return Ok(match path {
                "/" | "/launcher" => {
                    let page = fs::read(self.repo.join("launcher").join("index.html")).map_err(|e| e.to_string())?;
                    Reply { status: 200, kind: "text/html; charset=utf-8", body: page }
                }
                "/api/profiles" => self.profiles()?,
                "/api/launch" => {
                    let mut live = self.live.lock().unwrap();
                    let state = self.launch_state(&mut live);
                    Reply::json(200, json!({ "id": live.launch.as_ref().map(|l| l.id.clone()), "state": state }))
                }
                "/api/server" => {
                    let mut live = self.live.lock().unwrap();
                    Reply::json(200, self.server_info(&mut live))
                }
                "/api/ping" => Reply::empty(200),
                "/api/preview" => Reply::json(200, json!({ "available": self.preview_serve().exists(), "url": PREVIEW_URL })),
                _ => Reply::empty(404),
            });
        }
        // Only the page itself may start or change anything, never another page.
        if method != "POST" || origin != Some(self.origin.as_str()) {
            return Ok(Reply::empty(403));
        }
        let body: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        match path {
            "/api/preview/open" => Ok(match self.start_preview() {
                Ok(()) => Reply::json(200, json!({ "url": PREVIEW_URL })),
                Err(why) => {
                    self.write_log(&why);
                    Reply::error(404, why)
                }
            }),
            "/api/server/start" => self.server_start(),
            "/api/server/stop" => self.server_stop(),
            "/api/server/folder" => {
                let dir = &self.config().dev_server;
                fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                system::start("explorer.exe", &[&dir.to_string_lossy()]).map_err(|e| e.to_string())?;
                Ok(Reply::empty(200))
            }
            "/api/server/settings" => self.server_settings(&body),
            _ => {
                let mut parts = path.trim_start_matches("/api/").splitn(2, '/');
                let (action, id) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                let Some(cfg) = self.instance_cfg(id) else { return Ok(Reply::empty(404)) };
                match action {
                    "launch" => self.launch(id),
                    "folder" => {
                        let dir = cfg.parent().unwrap().join("minecraft");
                        system::start("explorer.exe", &[&dir.to_string_lossy()]).map_err(|e| e.to_string())?;
                        Ok(Reply::empty(200))
                    }
                    "settings" => self.profile_settings(&cfg, &body),
                    _ => Ok(Reply::empty(404)),
                }
            }
        }
    }

    // ---------- profiles ----------

    fn prism(&self) -> Result<&PathBuf, String> {
        self.config().prism.as_ref().map_err(|why| why.clone())
    }

    fn instances(&self) -> Result<PathBuf, String> {
        Ok(self.prism()?.parent().unwrap().join("instances"))
    }

    // The id comes from the page: only a plain folder name that is a real instance.
    fn instance_cfg(&self, id: &str) -> Option<PathBuf> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return None;
        }
        let cfg = self.instances().ok()?.join(id).join("instance.cfg");
        cfg.exists().then_some(cfg)
    }

    fn profiles(&self) -> Result<Reply, String> {
        let instances = self.instances()?;
        let state = {
            let mut live = self.live.lock().unwrap();
            prism_state(&system::processes(&mut live.system))
        };
        let mut profiles = Vec::new();
        let mut dirs: Vec<_> = fs::read_dir(&instances).map_err(|e| e.to_string())?.flatten().collect();
        dirs.sort_by_key(|d| d.file_name());
        for dir in dirs {
            let id = dir.file_name().to_string_lossy().to_string();
            let Some(cfg) = self.instance_cfg(&id) else { continue };
            let cfg = Cfg::read(&cfg);
            let game = dir.path().join("minecraft");
            let game_dir = fs::read_link(&game).map(plain_path).unwrap_or(game);
            // Prism's own setting for what the game opens on: one switch, then a server address or a world.
            let server = cfg.get("JoinServerOnLaunchAddress").unwrap_or_default();
            let world = cfg.get("JoinWorldOnLaunch").unwrap_or_default();
            let joins = cfg.get("JoinServerOnLaunch").as_deref() == Some("true");
            let open = if joins && !server.is_empty() {
                json!({ "kind": "server", "target": server })
            } else if joins && !world.is_empty() {
                json!({ "kind": "world", "target": world })
            } else {
                json!({ "kind": "menu", "target": "" })
            };
            profiles.push(json!({
                "id": id,
                "name": cfg.get("name"),
                "memoryMb": cfg.number("MaxMemAlloc"),
                "javaArgs": cfg.get("JvmArgs").unwrap_or_default(),
                "gameDir": game_dir.to_string_lossy(),
                // Milliseconds since 1970 and seconds, as Prism counts them; 0 when never played.
                "lastPlayed": cfg.number("lastLaunchTime"),
                "playedSeconds": cfg.number("totalTimePlayed"),
                "running": state.games.iter().any(|(game, _)| *game == id),
                "worlds": worlds(&game_dir),
                "open": open,
            }));
        }
        // locked: Prism keeps each instance's settings in memory and writes them back while it runs, so a setting
        // saved now would be lost.
        Ok(Reply::json(200, json!({
            "machineMb": self.machine_mb,
            "account": self.account(),
            "locked": state.running,
            "profiles": profiles,
        })))
    }

    // Only the player name of the account Prism will use. The file also holds the sign-in tokens: they are never
    // kept nor sent.
    fn account(&self) -> Option<String> {
        let file = self.prism().ok()?.parent()?.join("accounts.json");
        let value: Value = serde_json::from_slice(&fs::read(file).ok()?).ok()?;
        let accounts = value["accounts"].as_array()?;
        let active = accounts.iter().find(|a| a["active"].as_bool() == Some(true)).or(accounts.first())?;
        active["profile"]["name"].as_str().map(String::from)
    }

    fn launch(&self, id: &str) -> Result<Reply, String> {
        let prism = self.prism()?.clone();
        let mut live = self.live.lock().unwrap();
        if prism_state(&system::processes(&mut live.system)).games.iter().any(|(game, _)| game == id) {
            return Ok(Reply::empty(409));
        }
        // Starting Prism says nothing about the game: the page then asks /api/launch how it goes.
        system::start(&prism, &["--launch", id]).map_err(|e| e.to_string())?;
        live.launch = Some(Launch { id: id.to_string(), at: Instant::now(), saw_game: false, prepared: None });
        Ok(Reply::empty(200))
    }

    // Where the last launch stands, as one word the page turns into a sentence.
    fn launch_state(&self, live: &mut Live) -> &'static str {
        if live.launch.is_none() {
            return "none";
        }
        let state = prism_state(&system::processes(&mut live.system));
        let launch = live.launch.as_mut().unwrap();
        if let Some((_, pid)) = state.games.iter().find(|(game, _)| *game == launch.id) {
            launch.saw_game = true;
            return if system::has_window(*pid) { "running" } else { "loading" };
        }
        if launch.saw_game {
            return "stopped";
        }
        if state.preparing {
            launch.prepared = Some(Instant::now());
            return "preparing";
        }
        let elapsed = launch.at.elapsed();
        if !state.running {
            return if elapsed < Duration::from_secs(5) { "starting" } else { "failed" };
        }
        // Prism is still there with nothing running under it: it is showing a window of its own. After the step
        // before the game, that window is the error of that step; the game follows within a second otherwise.
        if launch.prepared.is_some_and(|at| at.elapsed() > Duration::from_secs(8)) {
            return "prepareFailed";
        }
        if elapsed > Duration::from_secs(25) { "waiting" } else { "starting" }
    }

    fn profile_settings(&self, cfg_file: &Path, body: &Value) -> Result<Reply, String> {
        let memory = as_int(&body["memoryMb"]);
        let java_args = squeeze(body["javaArgs"].as_str().unwrap_or_default());
        let kind = body["open"]["kind"].as_str().unwrap_or_default();
        let target = body["open"]["target"].as_str().unwrap_or_default().trim().to_string();
        let game_dir = cfg_file.parent().unwrap().join("minecraft");
        let open_valid = match kind {
            "menu" => true,
            "world" => worlds(&game_dir).contains(&target),
            "server" => {
                (1..=255).contains(&target.len())
                    && target.chars().all(|c| c.is_ascii_alphanumeric() || ".:[]_-".contains(c))
            }
            _ => false,
        };
        let Some(memory) = memory.filter(|m| *m >= 1024 && *m <= self.machine_mb as i64) else {
            return Ok(Reply::empty(400));
        };
        if java_args.len() > 2000 || !open_valid {
            return Ok(Reply::empty(400));
        }
        {
            let mut live = self.live.lock().unwrap();
            // Prism would write its own copy of the settings over ours when the game closes.
            if prism_state(&system::processes(&mut live.system)).running {
                return Ok(Reply::empty(409));
            }
        }
        let mut cfg = Cfg::read(cfg_file);
        // Prism only applies these two when their Override switch is on.
        cfg.set("OverrideMemory", "true");
        cfg.set("MaxMemAlloc", &memory.to_string());
        // The least memory must not exceed the most. A missing key stays missing: Prism has its own default.
        if cfg.get("MinMemAlloc").and_then(|v| v.parse::<i64>().ok()).is_some_and(|least| least > memory) {
            cfg.set("MinMemAlloc", &memory.to_string());
        }
        cfg.set("OverrideJavaArgs", "true");
        cfg.set("JvmArgs", &java_args);
        cfg.set("JoinServerOnLaunch", if kind == "menu" { "false" } else { "true" });
        cfg.set("JoinServerOnLaunchAddress", if kind == "server" { &target } else { "" });
        cfg.set("JoinWorldOnLaunch", if kind == "world" { &target } else { "" });
        cfg.write(cfg_file).map_err(|e| e.to_string())?;
        Ok(Reply::empty(200))
    }

    // ---------- the dev server ----------

    fn server_props(&self) -> PathBuf {
        self.config().dev_server.join("server.properties")
    }

    // One word for where the server stands, read from the processes and the server's log.
    fn server_state(&self, live: &mut Live) -> &'static str {
        let dir = &self.config().dev_server;
        let marker = format!("{}\\fabric-server-", dir.display()).to_lowercase();
        let processes = system::processes(&mut live.system);
        if let Some(java) = processes.iter().find(|p| p.name == "java.exe" && p.cmd.contains(&marker)) {
            if let Some(l) = live.server.as_mut() {
                l.saw_java = true;
            }
            // latest.log is rewritten at each start; "Done (" is the line Minecraft writes once players can join.
            let log = dir.join("logs").join("latest.log");
            let fresh = fs::metadata(&log)
                .and_then(|m| m.modified())
                .is_ok_and(|t| t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) >= java.started);
            if fresh && fs::read_to_string(&log).is_ok_and(|text| text.contains("Done (")) {
                return "running";
            }
            return "starting";
        }
        let Some(l) = live.server.as_mut() else { return "stopped" };
        if l.console.try_wait().ok().flatten().is_some() || l.saw_java {
            return "stopped";
        }
        // The console is still open with no server in it: either a step before the server (the download, the mod
        // sync) is running under it, or the script has ended and the window waits to be read.
        let console = l.console.id();
        if processes.iter().any(|p| p.parent == Some(console)) {
            l.busy = Instant::now();
            return "preparing";
        }
        // Between two steps there is no child for a moment: only a long quiet means the script has ended.
        if l.busy.elapsed() < Duration::from_secs(4) { "preparing" } else { "failed" }
    }

    fn server_launch_settings(&self) -> (i64, String) {
        // How dev-server.ps1 starts Java, saved by the page in launch.json of the server folder.
        let saved: Value = fs::read(self.config().dev_server.join("launch.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(strip_bom(&b)).ok())
            .unwrap_or(Value::Null);
        let memory = as_int(&saved["memoryMb"]).unwrap_or(6144);
        let java_args = saved["javaArgs"].as_str().map(String::from).unwrap_or_else(|| "-XX:+UseZGC".into());
        (memory, java_args)
    }

    fn eula(&self) -> bool {
        fs::read_to_string(self.config().dev_server.join("eula.txt"))
            .is_ok_and(|text| text.lines().any(|line| line.trim_end() == "eula=true"))
    }

    fn server_info(&self, live: &mut Live) -> Value {
        let (memory, java_args) = self.server_launch_settings();
        // server.properties is written by dev-server.ps1 on its first run (with the RCON password): until then, nothing to show.
        let props = Props::read(&self.server_props()).map(|props| {
            let mut out = json!({
                "motd": props.get("motd").unwrap_or_default(),
                "whitelist": props.get("white-list").as_deref() == Some("true"),
            });
            for (name, key, _, _) in SERVER_KEYS {
                out[name] = json!(props.get(key).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0));
            }
            out
        });
        json!({
            "dir": self.config().dev_server.to_string_lossy(),
            "state": self.server_state(live),
            "eula": self.eula(),
            "memoryMb": memory,
            "javaArgs": java_args,
            "props": props,
        })
    }

    fn server_start(&self) -> Result<Reply, String> {
        let mut live = self.live.lock().unwrap();
        if matches!(self.server_state(&mut live), "preparing" | "starting" | "running") {
            return Ok(Reply::empty(409));
        }
        let dir = &self.config().dev_server;
        if !self.eula() {
            // Accepting the EULA is the owner's decision: the page only says where.
            let file = dir.join("eula.txt");
            return Ok(Reply::error(
                412,
                format!("The Minecraft EULA is not accepted. Read https://aka.ms/MinecraftEULA, then write eula=true in {}.", file.display()),
            ));
        }
        let script = self.repo.join("scripts").join("dev-server.ps1");
        let command = format!(
            "& '{}' -ServerDir '{}'; Write-Host ''; Read-Host 'The server script has ended. Press Enter to close this window'",
            script.display(),
            dir.display()
        );
        let console = system::in_console("pwsh", &["-NoProfile", "-Command", &command], &self.repo).map_err(|e| e.to_string())?;
        live.server = Some(ServerLaunch { console, saw_java: false, busy: Instant::now() });
        Ok(Reply::empty(200))
    }

    fn server_stop(&self) -> Result<Reply, String> {
        {
            let mut live = self.live.lock().unwrap();
            if self.server_state(&mut live) != "running" {
                return Ok(Reply::empty(409));
            }
        }
        // The server saves the worlds and ends on its own "stop" command, sent over RCON as scripts/rcon.ps1 does.
        let port = Props::read(&self.server_props())
            .and_then(|p| p.get("rcon.port"))
            .and_then(|v| v.trim().parse::<u16>().ok())
            .unwrap_or(25575);
        let rcon = self.repo.join("scripts").join("rcon.ps1");
        system::hidden("pwsh")
            .args(["-NoProfile", "-File"])
            .arg(&rcon)
            .arg("-ServerDir")
            .arg(&self.config().dev_server)
            .args(["-Port", &port.to_string(), "stop"])
            .output()
            .map_err(|e| e.to_string())?;
        Ok(Reply::empty(200))
    }

    fn server_settings(&self, body: &Value) -> Result<Reply, String> {
        let memory = as_int(&body["memoryMb"]);
        let java_args = squeeze(body["javaArgs"].as_str().unwrap_or_default());
        let mut valid = memory.is_some_and(|m| m >= 1024 && m <= self.machine_mb as i64) && java_args.len() <= 2000;
        let props = &body["props"];
        let has_props = !props.is_null();
        if has_props {
            if !self.server_props().exists() {
                valid = false;
            }
            for (name, _, min, max) in SERVER_KEYS {
                if !as_int(&props[name]).is_some_and(|n| n >= min && n <= max) {
                    valid = false;
                }
            }
            // The line under the server's name in the game's list. server.properties escapes anything beyond plain
            // text, so plain text only.
            let motd = props["motd"].as_str().unwrap_or("\\");
            if motd.chars().count() > 59 || !motd.chars().all(|c| (' '..='~').contains(&c) && c != '\\') {
                valid = false;
            }
            if !props["whitelist"].is_boolean() {
                valid = false;
            }
        }
        if !valid {
            return Ok(Reply::empty(400));
        }
        {
            let mut live = self.live.lock().unwrap();
            // The server reads its settings when it starts: a change now would say something that is not so.
            if matches!(self.server_state(&mut live), "preparing" | "starting" | "running") {
                return Ok(Reply::empty(409));
            }
        }
        let dir = &self.config().dev_server;
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let launch = json!({ "memoryMb": memory, "javaArgs": java_args });
        fs::write(dir.join("launch.json"), serde_json::to_string_pretty(&launch).unwrap()).map_err(|e| e.to_string())?;
        if has_props {
            let file = self.server_props();
            let mut p = Props::read(&file).unwrap_or_default();
            for (name, key, _, _) in SERVER_KEYS {
                p.set(key, &as_int(&props[name]).unwrap().to_string());
            }
            p.set("motd", props["motd"].as_str().unwrap().trim());
            p.set("white-list", if props["whitelist"].as_bool() == Some(true) { "true" } else { "false" });
            p.write(&file).map_err(|e| e.to_string())?;
        }
        Ok(Reply::empty(200))
    }

    // ---------- the world preview ----------

    // Its own repository, the sibling folder ..\world-preview or WORLD_PREVIEW_DIR when set.
    fn preview_serve(&self) -> PathBuf {
        let dir = std::env::var_os("WORLD_PREVIEW_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.repo.parent().unwrap().join("world-preview"));
        dir.join("scripts").join("serve.ps1")
    }

    // Starts the preview's server on this pack, with no window of its own: the page holds it. Its server watches
    // this process and stops with it. The glue mod is not in the pack: the preview's server takes its jar on top,
    // so the section shows the pack's own ores. A preview from before -Exports keeps its exports in <Dir>\exports;
    // one from before -ParentPid stops once its pings stop instead.
    fn start_preview(&self) -> Result<(), String> {
        let mut live = self.live.lock().unwrap();
        if live.preview.as_mut().is_some_and(|p| p.try_wait().ok().flatten().is_none()) {
            return Ok(());
        }
        let serve = self.preview_serve();
        if !serve.exists() {
            return Err(format!(
                "The world preview was not found: no scripts\\serve.ps1 in {}. Put the world-preview repository there, or set WORLD_PREVIEW_DIR to its folder.",
                serve.parent().unwrap().parent().unwrap().display()
            ));
        }
        let config = self.config();
        let script = format!(
            ". '{common}'; $env:WORLD_PREVIEW_MODS = Build-GlueMod; \
             $serve = '{serve}'; $known = (Get-Command $serve).Parameters; \
             $a = @{{ Pack = '{pack}'; Dir = '{dir}'; NoWindow = $true }}; \
             if ($known.ContainsKey('Exports')) {{ $a.Exports = '{exports}' }}; \
             if ($known.ContainsKey('ParentPid')) {{ $a.ParentPid = {pid} }}; \
             & $serve @a",
            common = self.repo.join("scripts").join("common.ps1").display(),
            serve = serve.display(),
            pack = self.repo.join("pack").display(),
            dir = config.preview_dir.display(),
            exports = config.preview_exports.display(),
            pid = std::process::id(),
        );
        let child = system::hidden("pwsh").args(["-NoProfile", "-Command", &script]).spawn().map_err(|e| e.to_string())?;
        live.preview = Some(child);
        Ok(())
    }
}

// ---------- Prism's processes ----------

struct PrismState {
    running: bool,
    // The games running, by instance id, with the game's process.
    games: Vec<(String, u32)>,
    preparing: bool,
}

// Prism runs the step before the game (the pack sync) and then the game itself as its own child processes, and
// exits once the game has closed (seen on Prism 11.1.1). So the list of processes tells what a launch really did,
// where "Prism was started" tells nothing.
fn prism_state(processes: &[Proc]) -> PrismState {
    let prisms: Vec<u32> = processes.iter().filter(|p| p.name == "prismlauncher.exe").map(|p| p.pid).collect();
    let mut state = PrismState { running: !prisms.is_empty(), games: Vec::new(), preparing: false };
    for child in processes.iter().filter(|p| p.parent.is_some_and(|parent| prisms.contains(&parent))) {
        if child.name == "prismlauncher.exe" {
            continue;
        }
        // The game is Java started by Prism itself, with the instance's folder in its command line.
        if child.name != "java.exe" && child.name != "javaw.exe" {
            state.preparing = true;
        } else if let Some(id) = instance_in(&child.cmd) {
            state.games.push((id, child.pid));
        }
    }
    state
}

// The instance id in a command line: the folder after "instances\" or "instances/".
fn instance_in(cmd: &str) -> Option<String> {
    let mut rest = cmd;
    while let Some(at) = rest.find("instances") {
        rest = &rest[at + "instances".len()..];
        let Some(after) = rest.strip_prefix('\\').or_else(|| rest.strip_prefix('/')) else { continue };
        let id: String = after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
        if !id.is_empty() && after[id.len()..].starts_with(['\\', '/']) {
            return Some(id);
        }
    }
    None
}

// The worlds of a profile, by folder name: the name Prism wants to open one directly.
fn worlds(game_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(game_dir.join("saves"))
        .map(|dirs| {
            dirs.flatten()
                .filter(|d| d.path().join("level.dat").exists())
                .map(|d| d.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

// ---------- files of key=value lines ----------

// Prism's instance.cfg, one key=value per line. A value that holds quotes, commas or backslashes must be wrapped in
// quotes and escaped, or Prism reads it wrong (it silently drops bare quotes, which glues a command line together).
struct Cfg {
    lines: Vec<String>,
}

impl Cfg {
    fn read(file: &Path) -> Cfg {
        Cfg { lines: read_lines(file) }
    }
    fn get(&self, key: &str) -> Option<String> {
        let raw = find(&self.lines, key)?;
        let raw = &self.lines[raw][key.len() + 1..];
        if raw.len() < 2 || !(raw.starts_with('"') && raw.ends_with('"')) {
            return Some(raw.to_string());
        }
        let mut out = String::new();
        let mut chars = raw[1..raw.len() - 1].chars();
        while let Some(c) = chars.next() {
            out.push(if c == '\\' { chars.next().unwrap_or('\\') } else { c });
        }
        Some(out)
    }
    fn number(&self, key: &str) -> i64 {
        self.get(key).and_then(|v| v.trim().parse().ok()).unwrap_or(0)
    }
    fn set(&mut self, key: &str, value: &str) {
        let plain = !value.contains(['"', ',', ';', '\\'])
            && !value.starts_with(char::is_whitespace)
            && !value.ends_with(char::is_whitespace);
        let value = if plain { value.to_string() } else { format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")) };
        let line = format!("{key}={value}");
        match find(&self.lines, key) {
            Some(at) => self.lines[at] = line,
            None => {
                let at = self.lines.iter().position(|l| l == "[General]").map(|i| i + 1).unwrap_or(0);
                self.lines.insert(at, line);
            }
        }
    }
    fn write(&self, file: &Path) -> std::io::Result<()> {
        write_lines(file, &self.lines)
    }
}

// The dev server's server.properties: plain key=value lines.
#[derive(Default)]
struct Props {
    lines: Vec<String>,
}

impl Props {
    fn read(file: &Path) -> Option<Props> {
        file.exists().then(|| Props { lines: read_lines(file) })
    }
    fn get(&self, key: &str) -> Option<String> {
        find(&self.lines, key).map(|at| self.lines[at][key.len() + 1..].to_string())
    }
    fn set(&mut self, key: &str, value: &str) {
        let line = format!("{key}={value}");
        match find(&self.lines, key) {
            Some(at) => self.lines[at] = line,
            None => self.lines.push(line),
        }
    }
    fn write(&self, file: &Path) -> std::io::Result<()> {
        write_lines(file, &self.lines)
    }
}

fn find(lines: &[String], key: &str) -> Option<usize> {
    let prefix = format!("{key}=");
    lines.iter().position(|l| l.starts_with(&prefix))
}

fn read_lines(file: &Path) -> Vec<String> {
    let bytes = fs::read(file).unwrap_or_default();
    String::from_utf8_lossy(strip_bom(&bytes)).lines().map(String::from).collect()
}

// Lines as Windows writes them, as the scripts did.
fn write_lines(file: &Path, lines: &[String]) -> std::io::Result<()> {
    let mut text = lines.join("\r\n");
    text.push_str("\r\n");
    fs::write(file, text)
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

// ---------- small helpers ----------

// A number from the page, given as a number or as text.
fn as_int(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64)).or_else(|| value.as_str()?.trim().parse().ok())
}

// Java's arguments on one line, with single spaces.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// A link's target without the \\?\ prefix Windows gives it.
fn plain_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

fn now_text() -> String {
    // The log only needs an order and a rough time: seconds since 1970, in UTC.
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rest) = (secs / 86400, secs % 86400);
    // Days since 1970 to a calendar date (Howard Hinnant's algorithm).
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}
