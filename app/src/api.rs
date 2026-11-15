// Everything the page asks for, by path, as scripts/launcher.ps1 answered it before. This file routes each request
// and holds what the answers share; each part answers in a file of its own: the profiles (Prism Launcher's
// instances) in api/profiles.rs, the dev server in api/dev_server.rs, the world preview in api/preview.rs, and the
// key=value files both profiles and server keep their settings in, in api/key_value.rs. The page is index.html in
// Outpost Tools' own folder, read from the disk at each load: a change to it shows on a reload of the window, with
// nothing to build. Everything else (the profiles, the dev server's scripts, the pack) is read from the pack
// repository in `repo`.

mod dev_server;
mod key_value;
mod preview;
mod profiles;

use std::{
    fs,
    path::PathBuf,
    process::Child,
    sync::{Mutex, OnceLock},
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

// The folders the pack's scripts agree on, asked once from scripts/common.ps1 so they are written in one place.
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
                    let home = crate::find_home().ok_or("index.html was not found above the program.")?;
                    let page = fs::read(home.join("index.html")).map_err(|e| e.to_string())?;
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
}

// ---------- small helpers the parts share ----------

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
