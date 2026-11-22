//! What the run and export tests share: the real data they need (a test says "skipped" without it), a stand-in for
//! the engine, a root folder of their own under the crate's target\tmp, and the application driven offscreen
//! by events and frames as the window would.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use mere_base::{App, Button, Color, Event, Rect, Vec2};
use mere_ui::{Id, Theme};
use mere_win32::{Renderer, SystemLauncher};
use preview::run::{Engine, Runs};
use preview::world::World;
use preview::{PreviewApp, PreviewState};

pub const W: u32 = 1400;
pub const H: u32 = 940;

/// The world of the real out folder, or None (with the reason printed) when the data is missing.
pub fn world() -> Option<World> {
    if !&preview::world::data_dir().join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no out folder");
        return None;
    }
    Some(World::load(&preview::world::data_dir()).expect("the data loads"))
}

/// A plain (not `\\?\`) absolute path.
pub fn plain(p: &Path) -> PathBuf {
    let c = p.canonicalize().expect("the path exists");
    let s = c.to_string_lossy();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
}

/// `<crate>/target/tmp`, made.
pub fn tmp() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp");
    std::fs::create_dir_all(&dir).unwrap();
    plain(&dir)
}

/// A root of the test's own under target\tmp, emptied first.
pub fn root(name: &str) -> PathBuf {
    let dir = tmp().join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A stand-in for the engine, so that the app's run code is tested without the game. It follows the engine's
/// protocol: the files it deletes first, the progress.json the mod writes (atomically, by a rename), the files a
/// finished run leaves, timing.json, run.json, an error when the run fails, and the end of the server when the stop
/// flag is set. It downloads nothing, starts no game and writes only under target\tmp (it refuses any other out or
/// work folder before it creates anything).
///
/// `MERE_STUB` is the mode: "finish" (default) copies the finished out folder `MERE_STUB_FROM` names, "hold" stays in
/// the map pass with a child process of its own, as the game server is, until it is stopped, "fail" fails after
/// its first progress, and "stages" lands that folder's files stage by stage, each held until the test lets it go.
pub fn stub(env: &[(&str, &str)]) -> Engine {
    let get = |k: &str| env.iter().find(|(key, _)| *key == k).map(|(_, v)| v.to_string());
    let mode = get("MERE_STUB").unwrap_or_else(|| "finish".to_string());
    let from = get("MERE_STUB_FROM").map(PathBuf::from);
    Arc::new(move |o: &world_preview::Options, log: &mut dyn FnMut(&str)| stub_run(o, &mode, from.as_deref(), log))
}

fn stub_run(o: &world_preview::Options, mode: &str, from: Option<&Path>, log: &mut dyn FnMut(&str)) -> Result<PathBuf, String> {
    let tmp = tmp();
    let out = o.out.clone().ok_or("stub: an out folder is required")?;
    for path in [Some(&out), o.dir.as_ref()].into_iter().flatten() {
        let full = std::path::absolute(path).map_err(|e| e.to_string())?;
        if !full.starts_with(&tmp) {
            return Err(format!("stub: refusing {}, it is not under {}", path.display(), tmp.display()));
        }
    }
    let step = o.step;
    let detail = o.detail.unwrap_or(step);
    if ![step, 2 * step, 4 * step].contains(&detail) {
        return Err(format!("Detail must be {step}, {} or {} blocks.", 2 * step, 4 * step));
    }
    if let Some(dir) = &o.dir {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    for name in ["preview.json", "coarse.json", "section.json", "ores.json", "structures.json", "timing.json", "progress.json", "partial.json"] {
        let _ = std::fs::remove_file(out.join(name));
    }
    let stopped = || o.stop.as_ref().is_some_and(|s| s.load(Ordering::Relaxed));
    let pause = |ms: u64| -> Result<(), String> {
        std::thread::sleep(Duration::from_millis(ms));
        if stopped() { Err(world_preview::STOPPED.to_string()) } else { Ok(()) }
    };
    let mut written = 0;
    let mut progress = |pass: &str, at: u32, done: u32, total: u32| -> Result<(), String> {
        let part = out.join("progress.json.tmp");
        std::fs::write(&part, format!(r#"{{"pass":"{pass}","step":{at},"done":{done},"total":{total}}}"#)).map_err(|e| e.to_string())?;
        let _ = std::fs::rename(&part, out.join("progress.json"));
        written += 1;
        if mode == "fail" && written == 1 {
            return Err("stub: the run failed on purpose.".to_string());
        }
        Ok(())
    };

    // The game starts: no progress file yet, only the engine's lines.
    log("Starting the server");
    pause(1200)?;
    if mode == "stages" {
        return stages(o, &out, from, &mut progress, &stopped, log);
    }
    for at in [4 * step, 2 * step, step].into_iter().filter(|at| *at >= detail) {
        if at > detail {
            for done in 0..=4 {
                progress("sketch", at, done, 4)?;
                pause(200)?;
            }
            continue;
        }
        for done in 0..=10 {
            progress("map", at, done, 10)?;
            if mode == "hold" && done == 4 {
                // Stands for the game server: a process of its own, which only the stop ends.
                let mut server = Command::new("ping.exe").args(["-n", "600", "127.0.0.1"]).stdin(Stdio::null()).stdout(Stdio::null()).spawn().map_err(|e| e.to_string())?;
                std::fs::write(out.join("stub-child.pid"), server.id().to_string()).map_err(|e| e.to_string())?;
                while !stopped() {
                    std::thread::sleep(Duration::from_millis(50));
                }
                let _ = server.kill();
                let _ = server.wait();
                return Err(world_preview::STOPPED.to_string());
            }
            pause(200)?;
        }
    }
    progress("extras", detail, 0, 0)?;
    pause(300)?;

    if mode != "finish" {
        return Err(format!("stub: unknown mode {mode}"));
    }
    let from = from.filter(|f| f.join("preview.json").exists()).ok_or("stub: MERE_STUB_FROM must name a folder with a preview.json.")?;
    copy_from(from, &out, |name| {
        ["preview.json", "heights.bin", "biomes.bin", "section.json", "ores.json", "structures.json"].contains(&name) || (name.starts_with("section-") && name.ends_with(".bin"))
    })?;
    finish_run(o, &out, log)
}

/// The files of `from` that `kept` names, copied into `out`.
fn copy_from(from: &Path, out: &Path, kept: impl Fn(&str) -> bool) -> Result<(), String> {
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if kept(&name) {
            std::fs::copy(entry.path(), out.join(&name)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// "stages": the run's files land as the real engine writes them, and the run waits at each landing until the test
/// writes a gate file into the out folder, so that a test sees each stage for as long as it looks. The sketch
/// (`coarse.json` and the files its step names) during the sketch pass, then `go-map`; the map's pass at 4 tiles of
/// 10 with its points known so far (`partial.json` and its two files), then `go-full`; the map (`preview.json` and
/// its two files) and the extras pass, then `go-end`; the cuts, the ores and the structures, and the run's end.
fn stages(
    o: &world_preview::Options,
    out: &Path,
    from: Option<&Path>,
    progress: &mut dyn FnMut(&str, u32, u32, u32) -> Result<(), String>,
    stopped: &dyn Fn() -> bool,
    log: &mut dyn FnMut(&str),
) -> Result<PathBuf, String> {
    let from = from.filter(|f| f.join("preview.json").exists() && f.join("coarse.json").exists()).ok_or("stub: MERE_STUB_FROM must name a folder with a preview.json and a coarse.json.")?;
    let gate = |name: &str| -> Result<(), String> {
        while !out.join(name).exists() {
            if stopped() {
                return Err(world_preview::STOPPED.to_string());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    };
    let step = o.step;
    progress("sketch", 2 * step, 0, 4)?;
    // As the mod writes them: the two files first, the description that names them last.
    copy_from(from, out, |name| name.starts_with("coarse-") && name.ends_with(".bin"))?;
    copy_from(from, out, |name| name == "coarse.json")?;
    progress("sketch", 2 * step, 4, 4)?;
    gate("go-map")?;
    progress("map", step, 4, 10)?;
    partial(from, out, 4, 10)?;
    gate("go-full")?;
    progress("map", step, 10, 10)?;
    copy_from(from, out, |name| ["heights.bin", "biomes.bin"].contains(&name))?;
    copy_from(from, out, |name| name == "preview.json")?;
    progress("extras", step, 0, 0)?;
    gate("go-end")?;
    copy_from(from, out, |name| ["section.json", "ores.json", "structures.json"].contains(&name) || (name.starts_with("section-") && name.ends_with(".bin")))?;
    finish_run(o, out, log)
}

/// The map's points known so far, as the engine writes them during the map's pass (Progress.java `writePartial`):
/// the north half of the rows of `from`'s map, the others not sampled yet (biome 0xFFFF, height 0), the two files
/// each written beside its place and moved there, the description last. Its list is the map's, where the engine's
/// is every biome the generator can place: the loader takes any list.
fn partial(from: &Path, out: &Path, done: u32, total: u32) -> Result<(), String> {
    let text = std::fs::read_to_string(from.join("preview.json")).map_err(|e| e.to_string())?;
    let samples = preview::json::Json::parse(&text)?.get("samples").and_then(preview::json::Json::num).ok_or("stub: preview.json has no samples")? as usize;
    let half = samples / 2 * samples;
    let mut heights = std::fs::read(from.join("heights.bin")).map_err(|e| e.to_string())?;
    let mut biomes = std::fs::read(from.join("biomes.bin")).map_err(|e| e.to_string())?;
    heights[half * 2..].fill(0);
    biomes[half * 2..].fill(0xFF);
    let json = text.trim_start().replacen('{', &format!(r#"{{"done":{done},"total":{total},"#), 1);
    for (name, bytes) in [("partial-heights.bin", heights), ("partial-biomes.bin", biomes), ("partial.json", json.into_bytes())] {
        let part = out.join(format!("{name}.tmp"));
        std::fs::write(&part, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&part, out.join(name)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// What every finished run leaves last: the mod's timing, then the run record the engine writes.
fn finish_run(o: &world_preview::Options, out: &Path, log: &mut dyn FnMut(&str)) -> Result<PathBuf, String> {
    std::fs::write(out.join("timing.json"), r#"{"workMillis":1500}"#).map_err(|e| e.to_string())?;
    // As the engine's run record: a switch is kept as 1 or 0.
    let tectonic: Vec<String> = o
        .tectonic
        .iter()
        .map(|(k, v)| format!("\"{k}\":{}", match v { world_preview::Value::Bool(b) => (*b as i32).to_string(), other => other.to_string() }))
        .collect();
    let seed = o.seed.replace('\\', "\\\\").replace('"', "\\\"");
    std::fs::write(out.join("run.json"), format!(r#"{{"seed":"{seed}","tectonic":{{{}}}}}"#, tectonic.join(","))).map_err(|e| e.to_string())?;
    log(&format!("Preview ready in {}", out.display()));
    Ok(out.to_path_buf())
}

/// The runs a test gives the application: the stub engine, with its mode in `env`.
pub fn runs(root: &Path, env: &[(&str, &str)]) -> Runs {
    Runs {
        engine_dir: Some(root.to_path_buf()),
        engine: stub(env),
        pack: None,
        apply_pack: None,
        root: root.to_path_buf(),
        mods: Vec::new(),
        launcher: Box::new(SystemLauncher),
        server_dir: None,
        out_dir: None,
        exports_dir: None,
    }
}

pub fn rgb(c: Color) -> [u8; 3] {
    [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8)
}

pub fn near(a: [u8; 3], b: [u8; 3], tolerance: i32) -> bool {
    a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= tolerance)
}

pub fn centre(r: Rect) -> Vec2 {
    Vec2::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// Whether a process is still listed by the system.
pub fn alive(pid: u32) -> bool {
    let out = Command::new("tasklist.exe").args(["/FI", &format!("PID eq {pid}"), "/NH"]).output().expect("tasklist runs");
    String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
}

/// Waits (up to `secs`) for the process to be gone.
pub fn gone_within(pid: u32, secs: u64) -> bool {
    let started = Instant::now();
    while alive(pid) {
        if started.elapsed() > Duration::from_secs(secs) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

/// The application and its offscreen renderer, the page's dark theme.
pub struct Rig {
    pub app: PreviewApp,
    pub r: Renderer,
    pub theme: Theme,
}

impl Rig {
    /// The app with the world loaded as the command line loads it (the form holds the run's settings).
    pub fn new(world: &World, state: PreviewState, runs: Option<Runs>) -> Rig {
        let theme = Theme::dark();
        let mut r = Renderer::offscreen(W, H, 1.0).expect("offscreen renderer");
        let mut app = PreviewApp::new(theme, true, Vec2::new(W as f32, H as f32), state, Some(world.clone()));
        if let Some(runs) = runs {
            app = app.with_runs(runs);
        }
        app.start(&mut r);
        app.event(Event::Resized { width_px: W, height_px: H, scale: 1.0 });
        app.frame(&mut r, 0.0);
        Rig { app, r, theme }
    }

    pub fn frame(&mut self) {
        self.app.frame(&mut self.r, 0.0);
    }

    /// Frames every 100 ms until `done`, or panics with `what` after `secs`.
    pub fn until(&mut self, secs: u64, what: &str, mut done: impl FnMut(&mut Rig) -> bool) {
        let started = Instant::now();
        loop {
            self.frame();
            if done(self) {
                return;
            }
            assert!(started.elapsed() < Duration::from_secs(secs), "timed out after {secs} s waiting for: {what}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn rgba(&mut self) -> Vec<u8> {
        self.r.read_rgba().unwrap()
    }

    /// Draws the frame, keeps it as a PNG under target\tmp and returns the pixels.
    pub fn shot(&mut self, name: &str) -> Vec<u8> {
        self.frame();
        let rgba = self.rgba();
        let path = tmp().join(name);
        std::fs::write(&path, mere_png::encode(W, H, &rgba)).unwrap();
        println!("wrote {}", path.display());
        rgba
    }

    pub fn rect(&self, id: Id) -> Rect {
        assert_ne!(id, Id::ROOT, "the widget is not built");
        self.app.ui().rect(id).expect("the widget was built")
    }

    pub fn px(&self, rgba: &[u8], x: f32, y: f32) -> [u8; 3] {
        let i = (y as usize * W as usize + x as usize) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    }

    /// A click as the other tests make it: the pointer arrives, goes down, a frame, goes up, a frame.
    pub fn click(&mut self, pos: Vec2) {
        self.app.event(Event::PointerMoved { pos });
        self.app.event(Event::PointerDown { pos, button: Button::Left, clicks: 1 });
        self.frame();
        self.app.event(Event::PointerUp { pos, button: Button::Left });
        self.frame();
    }

    pub fn click_on(&mut self, id: Id) {
        let at = centre(self.rect(id));
        self.click(at);
    }

    /// Moves the pointer away from everything, so that nothing is hovered.
    pub fn park(&mut self) {
        self.app.event(Event::PointerMoved { pos: Vec2::new(2.0, 2.0) });
        self.frame();
    }
}
