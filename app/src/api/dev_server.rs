// The dev server: scripts/dev-server.ps1 started in a console of its own, its state read from the processes and its
// log, stopped over RCON, and the settings the page shows, kept in launch.json and server.properties of its folder.

use std::{
    fs,
    path::PathBuf,
    process::{Child, Output},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::key_value::{Props, strip_bom};
use super::{Api, Failure, Live, Reply, as_int, first_line, squeeze};
use crate::system;

// The last start of the dev server asked from the page: the console it runs in.
pub(super) struct ServerLaunch {
    console: Child,
    saw_java: bool,
    busy: Instant,
}

// The server.properties keys the page shows, with the bounds a value must keep.
const SERVER_KEYS: [(&str, &str, i64, i64); 4] = [
    ("viewDistance", "view-distance", 3, 32),
    ("simulationDistance", "simulation-distance", 5, 32),
    ("maxPlayers", "max-players", 1, 100),
    ("port", "server-port", 1024, 65535),
];

impl Api {
    fn server_props(&self) -> Result<PathBuf, Failure> {
        Ok(self.config()?.dev_server.join("server.properties"))
    }

    // One word for where the server stands, read from the processes and the server's log.
    fn server_state(&self, live: &mut Live) -> Result<&'static str, Failure> {
        let dir = &self.config()?.dev_server;
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
                return Ok("running");
            }
            return Ok("starting");
        }
        let Some(l) = live.server.as_mut() else { return Ok("stopped") };
        if l.console.try_wait().ok().flatten().is_some() || l.saw_java {
            return Ok("stopped");
        }
        // The console is still open with no server in it: either a step before the server (the download, the mod
        // sync) is running under it, or the script has ended and the window waits to be read.
        let console = l.console.id();
        if processes.iter().any(|p| p.parent == Some(console)) {
            l.busy = Instant::now();
            return Ok("preparing");
        }
        // Between two steps there is no child for a moment: only a long quiet means the script has ended.
        Ok(if l.busy.elapsed() < Duration::from_secs(4) { "preparing" } else { "failed" })
    }

    fn server_launch_settings(&self) -> Result<(i64, String), Failure> {
        // How dev-server.ps1 starts Java, saved by the page in launch.json of the server folder.
        let saved: Value = fs::read(self.config()?.dev_server.join("launch.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(strip_bom(&b)).ok())
            .unwrap_or(Value::Null);
        let memory = as_int(&saved["memoryMb"]).unwrap_or(6144);
        let java_args = saved["javaArgs"].as_str().map(String::from).unwrap_or_else(|| "-XX:+UseZGC".into());
        Ok((memory, java_args))
    }

    fn eula(&self) -> Result<bool, Failure> {
        Ok(fs::read_to_string(self.config()?.dev_server.join("eula.txt"))
            .is_ok_and(|text| text.lines().any(|line| line.trim_end() == "eula=true")))
    }

    pub(super) fn server_info(&self, live: &mut Live) -> Result<Value, Failure> {
        let (memory, java_args) = self.server_launch_settings()?;
        // server.properties is written by dev-server.ps1 on its first run (with the RCON password): until then, nothing to show.
        let props = Props::read(&self.server_props()?).map(|props| {
            let mut out = json!({
                "motd": props.get("motd").unwrap_or_default(),
                "whitelist": props.get("white-list").as_deref() == Some("true"),
            });
            for (name, key, _, _) in SERVER_KEYS {
                out[name] = json!(props.get(key).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0));
            }
            out
        });
        Ok(json!({
            "dir": self.config()?.dev_server.to_string_lossy(),
            "state": self.server_state(live)?,
            "eula": self.eula()?,
            "memoryMb": memory,
            "javaArgs": java_args,
            "props": props,
        }))
    }

    pub(super) fn server_start(&self) -> Result<Reply, Failure> {
        let mut live = self.live();
        if matches!(self.server_state(&mut live)?, "preparing" | "starting" | "running") {
            return Ok(Reply::empty(409));
        }
        let dir = &self.config()?.dev_server;
        if !self.eula()? {
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

    pub(super) fn server_stop(&self) -> Result<Reply, Failure> {
        {
            let mut live = self.live();
            if self.server_state(&mut live)? != "running" {
                return Ok(Reply::empty(409));
            }
        }
        // The server saves the worlds and ends on its own "stop" command, sent over RCON as scripts/rcon.ps1 does.
        let port = Props::read(&self.server_props()?)
            .and_then(|p| p.get("rcon.port"))
            .and_then(|v| v.trim().parse::<u16>().ok())
            .unwrap_or(25575);
        let rcon = self.repo.join("scripts").join("rcon.ps1");
        let out = system::hidden("pwsh")
            .args(["-NoProfile", "-File"])
            .arg(&rcon)
            .arg("-ServerDir")
            .arg(&self.config()?.dev_server)
            .args(["-Port", &port.to_string(), "stop"])
            .output()
            .map_err(|e| e.to_string())?;
        if let Some(why) = stop_refused(&out) {
            self.write_log(&format!("The server's stop failed: {why}"));
            return Ok(Reply::error(502, why));
        }
        Ok(Reply::empty(200))
    }

    pub(super) fn server_settings(&self, body: &Value) -> Result<Reply, Failure> {
        let memory = as_int(&body["memoryMb"]);
        let java_args = squeeze(body["javaArgs"].as_str().unwrap_or_default());
        let props = &body["props"];
        let has_props = !props.is_null();
        let props_exist = has_props && self.server_props()?.exists();
        if !settings_valid(body, self.machine_mb, props_exist) {
            return Ok(Reply::empty(400));
        }
        {
            let mut live = self.live();
            // The server reads its settings when it starts: a change now would say something that is not so.
            if matches!(self.server_state(&mut live)?, "preparing" | "starting" | "running") {
                return Ok(Reply::empty(409));
            }
        }
        let dir = &self.config()?.dev_server;
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let launch = json!({ "memoryMb": memory, "javaArgs": java_args });
        fs::write(dir.join("launch.json"), serde_json::to_string_pretty(&launch).unwrap()).map_err(|e| e.to_string())?;
        if has_props {
            let file = self.server_props()?;
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
}

// The server's settings the page sent are ones the server can take. Its properties are checked only when sent, and
// then server.properties must exist (`props_exist`): dev-server.ps1 writes it on the first start.
pub(super) fn settings_valid(body: &Value, machine_mb: u64, props_exist: bool) -> bool {
    let memory = as_int(&body["memoryMb"]);
    let java_args = squeeze(body["javaArgs"].as_str().unwrap_or_default());
    if !memory.is_some_and(|m| m >= 1024 && m <= machine_mb as i64) || java_args.len() > 2000 {
        return false;
    }
    let props = &body["props"];
    if props.is_null() {
        return true;
    }
    if !props_exist {
        return false;
    }
    if !SERVER_KEYS.iter().all(|(name, _, min, max)| as_int(&props[*name]).is_some_and(|n| n >= *min && n <= *max)) {
        return false;
    }
    // The line under the server's name in the game's list. server.properties escapes anything beyond plain text, so
    // plain text only.
    let motd = props["motd"].as_str().unwrap_or("\\");
    if motd.chars().count() > 59 || !motd.chars().all(|c| (' '..='~').contains(&c) && c != '\\') {
        return false;
    }
    props["whitelist"].is_boolean()
}

// Why rcon.ps1 did not deliver "stop" (the server refused the password, or was not listening): the first line of its
// error. None when it did.
fn stop_refused(out: &Output) -> Option<String> {
    if out.status.success() {
        return None;
    }
    Some(first_line(&out.stderr).or_else(|| first_line(&out.stdout)).unwrap_or_else(|| "The server did not take the stop command.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::ExitStatusExt;
    use std::process::ExitStatus;

    fn output(code: u32, stdout: &str, stderr: &str) -> Output {
        Output { status: ExitStatus::from_raw(code), stdout: stdout.into(), stderr: stderr.into() }
    }

    #[test]
    fn a_stop_that_failed_says_the_first_line_of_its_error() {
        let out = output(1, "", "\r\nException: RCON login refused\r\nLine |\r\n");
        assert_eq!(stop_refused(&out).as_deref(), Some("Exception: RCON login refused"));
        assert_eq!(stop_refused(&output(1, "", "")).as_deref(), Some("The server did not take the stop command."));
    }

    #[test]
    fn a_stop_that_was_delivered_is_no_failure() {
        assert_eq!(stop_refused(&output(0, "> stop\r\nStopping the server\r\n", "")), None);
    }

    fn settings() -> Value {
        json!({
            "memoryMb": 4096,
            "javaArgs": "-XX:+UseZGC",
            "props": { "viewDistance": 12, "simulationDistance": 8, "maxPlayers": 10, "port": 25565, "motd": "A test server", "whitelist": true },
        })
    }

    #[test]
    fn settings_within_their_bounds_are_taken() {
        assert!(settings_valid(&settings(), 8192, true));
        // Numbers may come as text, and the properties may be left out before the first start.
        assert!(settings_valid(&json!({ "memoryMb": "2048", "javaArgs": "" }), 8192, false));
        let mut edge = settings();
        edge["props"]["viewDistance"] = json!(32);
        edge["props"]["port"] = json!(1024);
        edge["memoryMb"] = json!(8192);
        assert!(settings_valid(&edge, 8192, true));
    }

    #[test]
    fn settings_out_of_bounds_are_refused() {
        let refused = |change: &dyn Fn(&mut Value)| {
            let mut body = settings();
            change(&mut body);
            !settings_valid(&body, 8192, true)
        };
        assert!(refused(&|b| b["memoryMb"] = json!(1000)));
        assert!(refused(&|b| b["memoryMb"] = json!(9000)));
        assert!(refused(&|b| b["memoryMb"] = json!(4096.5)));
        assert!(refused(&|b| b["javaArgs"] = json!("x".repeat(2001))));
        assert!(refused(&|b| b["props"]["viewDistance"] = json!(2)));
        assert!(refused(&|b| b["props"]["simulationDistance"] = json!(33)));
        assert!(refused(&|b| b["props"]["maxPlayers"] = json!(0)));
        assert!(refused(&|b| b["props"]["port"] = json!(65536)));
        assert!(refused(&|b| b["props"]["port"] = json!(null)));
        assert!(refused(&|b| b["props"]["motd"] = json!("x".repeat(60))));
        assert!(refused(&|b| b["props"]["motd"] = json!("back\\slash")));
        assert!(refused(&|b| b["props"]["motd"] = json!("caf\u{e9}")));
        assert!(refused(&|b| b["props"]["motd"] = json!(null)));
        assert!(refused(&|b| b["props"]["whitelist"] = json!("true")));
        // Properties sent before server.properties exists.
        assert!(!settings_valid(&settings(), 8192, false));
    }
}
