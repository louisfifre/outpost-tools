// The profiles: Prism Launcher's instances, their settings read from and written to each instance.cfg so the page
// and Prism always agree, and the launch of one of them, followed through Prism's processes.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::key_value::Cfg;
use super::{Api, Live, Reply, as_int, squeeze};
use crate::system::{self, Proc};

// The last launch asked from the page, and what it was last seen doing.
pub(super) struct Launch {
    pub(super) id: String,
    at: Instant,
    saw_game: bool,
    prepared: Option<Instant>,
}

impl Api {
    fn prism(&self) -> Result<&PathBuf, String> {
        self.config().prism.as_ref().map_err(|why| why.clone())
    }

    fn instances(&self) -> Result<PathBuf, String> {
        Ok(self.prism()?.parent().unwrap().join("instances"))
    }

    // The id comes from the page: only a plain folder name that is a real instance.
    pub(super) fn instance_cfg(&self, id: &str) -> Option<PathBuf> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return None;
        }
        let cfg = self.instances().ok()?.join(id).join("instance.cfg");
        cfg.exists().then_some(cfg)
    }

    pub(super) fn profiles(&self) -> Result<Reply, String> {
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

    pub(super) fn launch(&self, id: &str) -> Result<Reply, String> {
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
    pub(super) fn launch_state(&self, live: &mut Live) -> &'static str {
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

    pub(super) fn profile_settings(&self, cfg_file: &Path, body: &Value) -> Result<Reply, String> {
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

// A link's target without the \\?\ prefix Windows gives it.
fn plain_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}
