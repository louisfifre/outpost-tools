// launcher-native [--repo DIR] [--theme dark|light|system] [--shot out.png --size WxH --tool home|preview --data DIR]
//
// The native launcher: the world preview's window with the launcher's Home, answered by the same api as the page's
// launcher (app), called in process. Without --shot, opens the window, which draws its own bar and holds
// the preview's least size. With it, draws one frame offscreen and writes the PNG, from a fake api and a fixed date:
// a picture never asks the real one and does not depend on the machine.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use launcher::{api::Api, find_repo, system};
use launcher_native::bridge::Bridge;
use launcher_native::fake;
use launcher_native::home::{Calendar, LauncherHome};
use launcher_native::{ORIGIN, app};
use mere_base::{Vec2, log};
use mere_ui::{Slide, Theme};
use preview::world::World;
use preview::{MIN_SIZE, Mode, text};

fn usage() -> ! {
    eprintln!("usage: launcher-native [--repo DIR] [--theme dark|light|system] [--shot out.png --size WxH --tool home|preview --data DIR]");
    std::process::exit(2);
}

fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// The world preview's out folder a picture draws when `--data` is not given, if set.
const DATA_VAR: &str = "PREVIEW_DATA_DIR";

/// The world of an out folder, and whether the folder is there and cannot be read (the preview's own main shows
/// its trouble line then).
fn load_world(dir: &Path) -> (Option<World>, bool) {
    if !dir.join("preview.json").exists() {
        return (None, false);
    }
    match World::load(dir) {
        Ok(w) => (Some(w), false),
        Err(e) => {
            mere_base::observe!(Warn, "{}: {e}", dir.display());
            (None, true)
        }
    }
}

fn main() {
    log::add_sink(log::stderr_sink(log::Level::Info));
    let (mut repo, mut theme, mut shot, mut size, mut tool, mut data) = (None::<PathBuf>, None::<String>, None::<String>, (1400u32, 940u32), String::from("home"), None::<PathBuf>);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| usage());
        match a.as_str() {
            "--repo" => repo = Some(PathBuf::from(value())),
            "--theme" => theme = Some(value()),
            "--shot" => shot = Some(value()),
            "--size" => size = parse_size(&value()).unwrap_or_else(|| usage()),
            "--tool" => tool = value(),
            "--data" => data = Some(PathBuf::from(value())),
            _ => usage(),
        }
    }
    if !matches!(tool.as_str(), "home" | "preview") || theme.as_deref().is_some_and(|t| !matches!(t, "dark" | "light" | "system")) {
        usage();
    }
    match shot {
        Some(path) => picture(&path, size, theme.as_deref().unwrap_or("dark") == "light", &tool, data),
        None => window(repo, theme.as_deref().unwrap_or("system")),
    }
}

/// One frame offscreen, from the fixture.
fn picture(path: &str, size: (u32, u32), light: bool, tool: &str, data: Option<PathBuf>) {
    let dir = data.or_else(|| std::env::var_os(DATA_VAR).map(PathBuf::from).filter(|d| d.join("preview.json").exists()));
    let (world, unreadable) = dir.map_or((None, false), |d| load_world(&d));
    let dark = !light;
    let theme = if dark { Theme::dark() } else { Theme::light() };
    let (w, h) = preview::min_size_px(size);
    let backend = Arc::new(fake::fixture());
    let home = LauncherHome::new(Bridge::inline(backend), Calendar::fixed());
    let mut app = app(theme, dark, Vec2::new(w as f32, h as f32), world, None, home, true);
    if unreadable {
        app.state_mut().trouble = text::UNREADABLE.to_string();
    }
    if tool == "preview" {
        app.state_mut().mode = Mode::Preview;
        app.state_mut().options = Slide::settled(true);
    }
    app.ui_mut().set_clipboard(Box::new(mere_win32::SystemClipboard));
    let rgba = mere_win32::shot(&mut app, w, h, 1.0).unwrap_or_else(|e| {
        eprintln!("launcher-native: {e}");
        std::process::exit(1);
    });
    if let Err(e) = std::fs::write(path, mere_png::encode(w, h, &rgba)) {
        eprintln!("launcher-native: {path}: {e}");
        std::process::exit(1);
    }
    mere_base::observe!(Info, "wrote {path}");
}

fn window(repo: Option<PathBuf>, scheme: &str) {
    let repo = repo.or_else(find_repo).unwrap_or_else(|| PathBuf::from("."));
    let data = repo.join("run").join("app");
    let _ = std::fs::create_dir_all(&data);
    let api = Arc::new(Api::new(repo.clone(), ORIGIN.into()));
    // A second start is refused: `only_one` raises the running window, whose handle the toolkit keeps in `hwnd.txt`.
    if !system::only_one(&data) {
        api.write_log("start refused: the launcher already runs");
        return;
    }
    // Holding the lock, whatever `hwnd.txt` says is from a dead session (the toolkit removes it when its window
    // closes, a crash leaves it); handles are recycled, so a later start must not raise it.
    let _ = std::fs::remove_file(data.join("hwnd.txt"));
    api.write_log("started (native)");
    // What the toolkit warns of goes to the launcher's log: with no console it is the only trace.
    let sink_api = api.clone();
    log::add_sink(move |r| {
        if r.level >= log::Level::Warn {
            sink_api.write_log(&format!("{:?} {}: {}", r.level, r.source, r.message));
        }
    });

    let runs = launcher_native::runs(&api, &repo);
    let (world, unreadable) = load_world(&runs.out());

    let dark = match scheme {
        "dark" => true,
        "light" => false,
        _ => mere_win32::system_dark(),
    };
    let home = LauncherHome::new(Bridge::threads(api.clone()), Calendar::real());
    let theme = if dark { Theme::dark() } else { Theme::light() };
    let (w, h) = preview::min_size_px((1280, 880));
    let mut app = app(theme, dark, Vec2::new(w as f32, h as f32), world, Some(runs), home, mere_win32::reduced_motion()).follow_scheme(scheme == "system");
    if unreadable {
        app.state_mut().trouble = text::UNREADABLE.to_string();
    }
    app.ui_mut().set_clipboard(Box::new(mere_win32::SystemClipboard));
    // The same cube as the page launcher's. The place goes in a file of its own, beside the page launcher's
    // `window.txt`, because the two files are not written the same way.
    let (rgba, side) = launcher::icon::cube_rgba();
    let frame = mere_win32::Frame {
        own: true,
        min: MIN_SIZE,
        icon: Some(mere_win32::Icon { rgba, side }),
        place: Some(data.join("native-window.txt")),
        hwnd_file: Some(data.join("hwnd.txt")),
    };
    if let Err(e) = mere_win32::run_with(preview::text::BAR_TITLE, &mut app, frame) {
        api.write_log(&format!("the window failed: {e}"));
    }
    api.write_log("stopped: the window was closed");
}
