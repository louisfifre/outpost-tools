// The profiles: Prism Launcher's instances, their settings read from and written to each instance.cfg so the page
// and Prism always agree, and the launch of one of them, followed through Prism's processes.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::key_value::Cfg;
use super::{Api, Failure, Live, Reply, as_int, squeeze};
use crate::system::{self, Proc};

// The last launch asked from the page, and what it was last seen doing.
pub(super) struct Launch {
    pub(super) id: String,
    at: Instant,
    saw_game: bool,
    prepared: Option<Instant>,
}

impl Api {
    fn prism(&self) -> Result<&PathBuf, Failure> {
        self.config()?.prism.as_ref().map_err(|why| Failure::Internal(why.clone()))
    }

    fn instances(&self) -> Result<PathBuf, Failure> {
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

    // Each instance in the order of its folder: its id, its settings and its game folder (a link followed).
    fn instance_list(&self) -> Result<Vec<(String, Cfg, PathBuf)>, Failure> {
        let instances = self.instances()?;
        let mut dirs: Vec<_> = fs::read_dir(&instances).map_err(|e| e.to_string())?.flatten().collect();
        dirs.sort_by_key(|d| d.file_name());
        Ok(dirs
            .into_iter()
            .filter_map(|dir| {
                let id = dir.file_name().to_string_lossy().to_string();
                let cfg = Cfg::read(&self.instance_cfg(&id)?);
                let game_dir = game_dir_of(&dir.path());
                Some((id, cfg, game_dir))
            })
            .collect())
    }

    /// The game of each Prism instance, by its name in Prism (its id when it has none) and its game folder: where the
    /// world preview's Apply looks for a Tectonic config. Err says why Prism's instances are not known.
    pub fn prism_games(&self) -> Result<Vec<(String, PathBuf)>, String> {
        let list = self.instance_list().map_err(|failure| failure.why().to_string())?;
        Ok(list.into_iter().map(|(id, cfg, game_dir)| (cfg.get("name").filter(|n| !n.trim().is_empty()).unwrap_or(id), game_dir)).collect())
    }

    pub(super) fn profiles(&self) -> Result<Reply, Failure> {
        let state = {
            let mut live = self.live();
            prism_state(&system::processes(&mut live.system))
        };
        let mut profiles = Vec::new();
        for (id, cfg, game_dir) in self.instance_list()? {
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
                // The least memory; null when Prism has none to say, which the page reads as its starting 1 GB.
                "minMemoryMb": cfg.get("MinMemAlloc").and_then(|v| v.trim().parse::<i64>().ok()),
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
            // What the PC uses now, for the memory bar's hatched end (0 when it cannot be read).
            "usedMb": used_mb(),
            "account": self.account(),
            "locked": state.running,
            "profiles": profiles,
        })))
    }

    pub(super) fn launch(&self, id: &str) -> Result<Reply, Failure> {
        let prism = self.prism()?.clone();
        let mut live = self.live();
        if prism_state(&system::processes(&mut live.system)).games.iter().any(|(game, _)| game == id) {
            return Ok(Reply::empty(409));
        }
        // Starting Prism says nothing about the game: the page then asks /api/launch how it goes.
        let account = self.launch_account();
        system::start(&prism, &super::screens::launch_args(id, account.as_deref())).map_err(|e| e.to_string())?;
        live.launch = Some(Launch { id: id.to_string(), at: Instant::now(), saw_game: false, prepared: None });
        Ok(Reply::empty(200))
    }

    // Where the last launch stands, as one word the page turns into a sentence.
    pub(super) fn launch_state(&self, live: &mut Live) -> &'static str {
        if live.launch.is_none() {
            return "none";
        }
        let state = prism_state(&system::processes(&mut live.system));
        launch_word(live.launch.as_mut().unwrap(), &state, Instant::now(), system::has_window)
    }

    pub(super) fn profile_settings(&self, cfg_file: &Path, body: &Value) -> Result<Reply, Failure> {
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
        // The least memory is optional (the page of older forms sends none): when given it is at least 1 GB and
        // never above the most.
        let least = match body.get("minMemoryMb").filter(|v| !v.is_null()) {
            None => None,
            Some(v) => match as_int(v).filter(|m| *m >= 1024 && *m <= memory) {
                Some(m) => Some(m),
                None => return Ok(Reply::empty(400)),
            },
        };
        if java_args.len() > 2000 || !open_valid {
            return Ok(Reply::empty(400));
        }
        {
            let mut live = self.live();
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
        if let Some(least) = least {
            cfg.set("MinMemAlloc", &least.to_string());
        } else if cfg.get("MinMemAlloc").and_then(|v| v.parse::<i64>().ok()).is_some_and(|least| least > memory) {
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

// Whether Prism runs: while it does, a profile's files are held and its settings would be written over.
pub(super) fn prism_running(live: &mut Live) -> bool {
    prism_state(&system::processes(&mut live.system)).running
}

// Where a launch stands at `now`, from Prism's processes; `has_window` tells whether the game's process shows its
// window yet.
fn launch_word(launch: &mut Launch, state: &PrismState, now: Instant, has_window: impl Fn(u32) -> bool) -> &'static str {
    if let Some((_, pid)) = state.games.iter().find(|(game, _)| *game == launch.id) {
        launch.saw_game = true;
        return if has_window(*pid) { "running" } else { "loading" };
    }
    if launch.saw_game {
        return "stopped";
    }
    if state.preparing {
        launch.prepared = Some(now);
        return "preparing";
    }
    let elapsed = now.duration_since(launch.at);
    if !state.running {
        return if elapsed < Duration::from_secs(5) { "starting" } else { "failed" };
    }
    // Prism is still there with nothing running under it: it is showing a window of its own. After the step
    // before the game, that window is the error of that step; the game follows within a second otherwise.
    if launch.prepared.is_some_and(|at| now.duration_since(at) > Duration::from_secs(8)) {
        return "prepareFailed";
    }
    if elapsed > Duration::from_secs(25) { "waiting" } else { "starting" }
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

// The game folder of an instance's folder: `minecraft`, a link followed (a profile can share the folder of a pack).
pub(super) fn game_dir_of(instance: &Path) -> PathBuf {
    let game = instance.join("minecraft");
    fs::read_link(&game).map(plain_path).unwrap_or(game)
}

// What the PC uses of its memory now, in MiB.
fn used_mb() -> i64 {
    i64::try_from(super::memory::PcMemory::read().used / (1024 * 1024)).unwrap_or(0)
}

// A link's target without the \\?\ prefix Windows gives it.
fn plain_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_instance_id_is_the_folder_after_instances() {
        let cmd = r#""C:\Java\bin\javaw.exe" -Djava.library.path=C:\Prism\instances\Pack-1_a\natives -cp C:\Prism\libraries\a.jar"#;
        assert_eq!(instance_in(cmd).as_deref(), Some("Pack-1_a"));
        assert_eq!(instance_in("java -Dx=C:/Prism/instances/other/minecraft").as_deref(), Some("other"));
        // A word "instances" that is no folder of them, then the real one.
        assert_eq!(instance_in(r"java --instances C:\Prism\instances\real\natives").as_deref(), Some("real"));
    }

    #[test]
    fn a_command_line_without_an_instance_folder_has_no_id() {
        assert_eq!(instance_in(r"java -jar C:\server\fabric-server-launch.jar"), None);
        // The id must be a whole folder name, not the end of the line nor one with other characters.
        assert_eq!(instance_in(r"java C:\Prism\instances\last"), None);
        assert_eq!(instance_in(r"java C:\Prism\instances\with space\natives"), None);
        assert_eq!(instance_in(r"java C:\Prism\instances\\natives"), None);
    }

    fn launch(at: Instant) -> Launch {
        Launch { id: "pack".into(), at, saw_game: false, prepared: None }
    }

    fn prism(running: bool, preparing: bool, game: Option<u32>) -> PrismState {
        PrismState { running, preparing, games: game.map(|pid| ("pack".to_string(), pid)).into_iter().collect() }
    }

    const NO_WINDOW: fn(u32) -> bool = |_| false;

    #[test]
    fn a_launch_follows_prism_through_the_pack_sync_to_the_game_window() {
        let t0 = Instant::now();
        let s = |secs: u64| t0 + Duration::from_secs(secs);
        let mut l = launch(t0);
        assert_eq!(launch_word(&mut l, &prism(false, false, None), s(1), NO_WINDOW), "starting");
        assert_eq!(launch_word(&mut l, &prism(true, true, None), s(2), NO_WINDOW), "preparing");
        assert_eq!(launch_word(&mut l, &prism(true, false, None), s(3), NO_WINDOW), "starting");
        assert_eq!(launch_word(&mut l, &prism(true, false, Some(7)), s(4), NO_WINDOW), "loading");
        assert_eq!(launch_word(&mut l, &prism(true, false, Some(7)), s(30), |pid| pid == 7), "running");
        // Once the game was seen, its end is a stop, whatever Prism does.
        assert_eq!(launch_word(&mut l, &prism(true, false, None), s(31), NO_WINDOW), "stopped");
    }

    #[test]
    fn prism_gone_is_a_start_for_5_s_then_a_failure() {
        let t0 = Instant::now();
        let mut l = launch(t0);
        assert_eq!(launch_word(&mut l, &prism(false, false, None), t0 + Duration::from_secs(4), NO_WINDOW), "starting");
        assert_eq!(launch_word(&mut l, &prism(false, false, None), t0 + Duration::from_secs(6), NO_WINDOW), "failed");
    }

    #[test]
    fn prism_alone_after_the_sync_is_its_error_and_alone_long_is_a_wait() {
        let t0 = Instant::now();
        let s = |secs: u64| t0 + Duration::from_secs(secs);
        let mut synced = launch(t0);
        assert_eq!(launch_word(&mut synced, &prism(true, true, None), s(2), NO_WINDOW), "preparing");
        assert_eq!(launch_word(&mut synced, &prism(true, false, None), s(10), NO_WINDOW), "starting");
        assert_eq!(launch_word(&mut synced, &prism(true, false, None), s(11), NO_WINDOW), "prepareFailed");
        let mut asked = launch(t0);
        assert_eq!(launch_word(&mut asked, &prism(true, false, None), s(25), NO_WINDOW), "starting");
        assert_eq!(launch_word(&mut asked, &prism(true, false, None), s(26), NO_WINDOW), "waiting");
    }
}
