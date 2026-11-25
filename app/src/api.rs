// Everything the page asks for, by path, as scripts/launcher.ps1 answered it before. This file routes each request
// and holds what the answers share; each part answers in a file of its own: the profiles (Prism Launcher's
// instances) in api/profiles.rs, the dev server in api/dev_server.rs, the world preview in api/preview.rs, and the
// key=value files both profiles and server keep their settings in, in api/key_value.rs. The page is index.html in
// Outpost Tools' own folder, read from the disk at each load: a change to it shows on a reload of the window, with
// nothing to build. Everything else (the profiles, the dev server's scripts, the pack) is read from the pack
// repository in `repo`.

mod dev_server;
pub mod instance;
pub mod java;
mod java_setup;
mod key_value;
mod memory;
pub mod modrinth;
pub mod mods;
pub mod mrpack;
mod preview;
mod prism_portable;
mod profiles;
mod screens;
pub mod servers;
pub mod skin;

use std::{
    fs,
    path::PathBuf,
    process::Child,
    sync::{Mutex, MutexGuard, OnceLock, PoisonError},
};

use serde_json::{Value, json};
use sysinfo::System;

use crate::system;
use dev_server::ServerLaunch;
use preview::PREVIEW_URL;
use profiles::Launch;

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

// Why a request got no answer of its own. Unavailable: the pack's folders are not known (scripts/common.ps1 could not
// be read), answered 503 and asked again at the next request. Internal: anything else, answered 500.
enum Failure {
    Unavailable(String),
    Internal(String),
}

impl Failure {
    fn reply(self) -> Reply {
        match self {
            Failure::Unavailable(why) => Reply::error(503, why),
            Failure::Internal(why) => Reply::error(500, why),
        }
    }

    fn why(&self) -> &str {
        match self {
            Failure::Unavailable(why) | Failure::Internal(why) => why,
        }
    }
}

impl From<String> for Failure {
    fn from(why: String) -> Failure {
        Failure::Internal(why)
    }
}

impl From<&str> for Failure {
    fn from(why: &str) -> Failure {
        Failure::Internal(why.to_string())
    }
}

// The folders the pack's scripts agree on, asked from scripts/common.ps1 so they are written in one place.
struct Config {
    dev_server: PathBuf,
    preview_dir: PathBuf,
    preview_exports: PathBuf,
    // Prism Launcher's program, or why it was not found.
    prism: Result<PathBuf, String>,
}

// What the launcher started and still follows: the last launch, the dev server's console, the preview's server.
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
    // What the page that adds a mod asked of Modrinth lately, and the order of its searches (api/mods/search.rs).
    searches: mods::Searches,
}

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
            searches: mods::Searches::default(),
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

    // A request that panicked while it held the lock leaves it poisoned. What it guards (the processes followed, the
    // last launch) stays whole enough to read, so the requests after it go on instead of panicking in turn.
    fn live(&self) -> MutexGuard<'_, Live> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // Only a success is kept: a failure is asked again at the next request, so a pack fixed while the launcher runs
    // is seen without a restart, and no request works on empty paths.
    fn config(&self) -> Result<&Config, Failure> {
        self.config_from(|| self.read_config())
    }

    fn config_from(&self, read: impl FnOnce() -> Result<Config, String>) -> Result<&Config, Failure> {
        if let Some(config) = self.config.get() {
            return Ok(config);
        }
        let config = read().map_err(Failure::Unavailable)?;
        Ok(self.config.get_or_init(|| config))
    }

    fn read_config(&self) -> Result<Config, String> {
        let common = self.repo.join("scripts").join("common.ps1");
        let script = format!(
            ". '{}'; $prism = try {{ Find-Prism }} catch {{ \"!$($_.Exception.Message)\" }}; \
             ConvertTo-Json -Compress @{{ devServer = $DevServerDir; previewDir = $PreviewDir; previewExports = $PreviewExports; prism = $prism }}",
            common.display()
        );
        let out = system::hidden("pwsh")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .map_err(|e| format!("pwsh could not be started: {e}"))?;
        parse_config(&out.stdout, &out.stderr)
    }

    // One request, whatever brought it: the window's own protocol or the local server for captures.
    pub fn handle(&self, method: &str, path: &str, origin: Option<&str>, body: &[u8]) -> Reply {
        match self.route(method, path, origin, body) {
            Ok(reply) => reply,
            Err(failure) => {
                self.write_log(&format!("{method} {path} failed: {}", failure.why()));
                failure.reply()
            }
        }
    }

    fn route(&self, method: &str, path: &str, origin: Option<&str>, body: &[u8]) -> Result<Reply, Failure> {
        // The routes of the native screens (servers, accounts, skin, links) answer first, from api/screens.rs.
        if let Some(reply) = self.screens_route(method, path, origin, body)? {
            return Ok(reply);
        }
        let path = path.split('?').next().unwrap_or(path);
        if method == "GET" {
            return Ok(match path {
                "/" | "/launcher" => {
                    let home = crate::find_home().ok_or("index.html was not found above the program.")?;
                    let page = fs::read(home.join("index.html")).map_err(|e| e.to_string())?;
                    Reply { status: 200, kind: "text/html; charset=utf-8", body: page }
                }
                "/api/profiles" => self.profiles()?,
                "/api/launch" => {
                    let mut live = self.live();
                    let state = self.launch_state(&mut live);
                    Reply::json(200, json!({ "id": live.launch.as_ref().map(|l| l.id.clone()), "state": state }))
                }
                "/api/server" => {
                    let mut live = self.live();
                    Reply::json(200, self.server_info(&mut live)?)
                }
                "/api/java" => self.java_state(),
                "/api/ping" => Reply::empty(200),
                // The mods of a profile (api/mods.rs).
                mods if mods.starts_with("/api/mods/") => self.mods_get(mods)?,
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
                Err(Failure::Internal(why)) => {
                    self.write_log(&why);
                    Reply::error(404, why)
                }
                Err(unavailable) => return Err(unavailable),
            }),
            "/api/java/download" => Ok(self.java_download()),
            "/api/java/stop" => Ok(self.java_stop()),
            "/api/server/start" => self.server_start(),
            "/api/server/stop" => self.server_stop(),
            "/api/server/folder" => {
                let dir = &self.config()?.dev_server;
                fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                system::start("explorer.exe", &[&dir.to_string_lossy()]).map_err(|e| e.to_string())?;
                Ok(Reply::empty(200))
            }
            "/api/server/settings" => self.server_settings(&body),
            // The mods of a profile, and a page opened in the browser (api/mods.rs).
            mods if mods.starts_with("/api/mods/") || mods == "/api/open-url" => self.mods_post(mods, &body),
            _ => {
                let mut parts = path.trim_start_matches("/api/").splitn(2, '/');
                let (action, id) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                // Without the pack's folders no instance is found: say why, rather than 404.
                self.config()?;
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
}

// ---------- small helpers the parts share ----------

// What scripts/common.ps1 printed: the folders, and Prism's program or "!" and why it was not found. A folder left
// empty means the script did not run to its end, and its error says why.
fn parse_config(stdout: &[u8], stderr: &[u8]) -> Result<Config, String> {
    let value: Value = serde_json::from_slice(stdout).unwrap_or(Value::Null);
    let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
    let (dev_server, preview_dir, preview_exports) = (text("devServer"), text("previewDir"), text("previewExports"));
    if dev_server.is_empty() || preview_dir.is_empty() || preview_exports.is_empty() {
        return Err(match first_line(stderr) {
            Some(line) => format!("scripts/common.ps1 could not be read: {line}"),
            None => "scripts/common.ps1 could not be read.".into(),
        });
    }
    let prism = text("prism");
    Ok(Config {
        dev_server: PathBuf::from(dev_server),
        preview_dir: PathBuf::from(preview_dir),
        preview_exports: PathBuf::from(preview_exports),
        prism: match prism.strip_prefix('!') {
            Some(why) => Err(why.to_string()),
            None if prism.is_empty() => Err("Prism Launcher was not found.".into()),
            None => Ok(PathBuf::from(prism)),
        },
    })
}

// The first line of a program's output that says something, trimmed.
fn first_line(output: &[u8]) -> Option<String> {
    String::from_utf8_lossy(output).lines().map(str::trim).find(|l| !l.is_empty()).map(String::from)
}

// A number from the page, given as a number or as text.
fn as_int(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64)).or_else(|| value.as_str()?.trim().parse().ok())
}

// Java's arguments on one line, with single spaces.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn common_ps1_that_did_not_run_gives_its_first_error_line() {
        let why = parse_config(b"{\"devServer\":null,\"previewDir\":null,\"previewExports\":null,\"prism\":\"!x\"}", b"\n  The term 'Find-Prism' is not recognized.\r\nAt line:1\n")
            .err()
            .unwrap();
        assert_eq!(why, "scripts/common.ps1 could not be read: The term 'Find-Prism' is not recognized.");
        assert_eq!(parse_config(b"", b"").err().unwrap(), "scripts/common.ps1 could not be read.");
    }

    #[test]
    fn common_ps1_that_ran_gives_the_folders_and_prism() {
        let out = br#"{"devServer":"C:\\pack\\run\\server","previewDir":"C:\\pack\\run\\preview","previewExports":"C:\\pack\\run\\exports","prism":"!Prism is not installed."}"#;
        let config = parse_config(out, b"").ok().unwrap();
        assert_eq!(config.dev_server, PathBuf::from(r"C:\pack\run\server"));
        assert_eq!(config.preview_exports, PathBuf::from(r"C:\pack\run\exports"));
        assert_eq!(config.prism.err().unwrap(), "Prism is not installed.");
    }

    #[test]
    fn unknown_folders_answer_503_with_the_reason() {
        let reply = Failure::Unavailable("scripts/common.ps1 could not be read.".into()).reply();
        assert_eq!(reply.status, 503);
        let body: Value = serde_json::from_slice(&reply.body).unwrap();
        assert_eq!(body["error"], "scripts/common.ps1 could not be read.");
        assert_eq!(Failure::from("other").reply().status, 500);
    }

    #[test]
    fn a_failed_read_is_asked_again_and_a_success_is_kept() {
        let api = Api::new(PathBuf::from(r"C:\no-such-pack"), "native://test".into());
        let asked = Cell::new(0);
        let read = |ok: bool| {
            asked.set(asked.get() + 1);
            if ok { parse_config(br#"{"devServer":"a","previewDir":"b","previewExports":"c","prism":"p"}"#, b"") } else { Err("no".into()) }
        };
        assert!(api.config_from(|| read(false)).is_err());
        assert!(api.config_from(|| read(true)).is_ok());
        assert!(api.config_from(|| read(false)).is_ok());
        assert_eq!(asked.get(), 2);
    }

    #[test]
    fn a_request_that_panicked_holding_the_lock_does_not_stop_the_next() {
        let api = Api::new(PathBuf::from(r"C:\no-such-pack"), "native://test".into());
        std::thread::scope(|s| {
            let held = s.spawn(|| {
                let _live = api.live.lock();
                panic!("a request panicked");
            });
            assert!(held.join().is_err());
        });
        assert!(api.live.is_poisoned());
        let reply = api.handle("GET", "/api/launch", None, b"");
        assert_eq!(reply.status, 200);
    }
}
