//! preview [--data DIR] [--theme dark|light|system] [--light] [--size WxH] [--shot out.png] [--view k[,k]] [--level slopes|blocks|fine] [--view-only] [--biome id,..]
//!         [--structure surface|under|id,..] [--look k,x,y] [--camera cx,cy,cz] [--cut N] [--ore kind,..] [--cut-look k,x,y]
//!         [--runs DIR] [--engine DIR] [--pack DIR]
//!
//! Without --shot, opens the world preview's window. With it, draws one frame offscreen and writes the PNG: the
//! same frame the window would show. The out folder of --data (by default the folder PREVIEW_DATA_DIR names, and without a
//! preview.json there the page shows no preview) fills the stage (step 14).
//! A run (step 17): Generate runs the engine (world_preview::run) on the world-preview repository --engine names (by
//! default WORLD_PREVIEW_DIR; without either, Generate says the run failed), with --pack if given. Everything a run or an export writes (its out
//! folder, its log, the learned pace, the exports) goes under --runs, which must lie inside this crate's
//! target\tmp for now: the rule is lifted once a real run has been exercised. The data of --data is never written.
//! The colours (step 18): the window follows the system's light or dark setting and switches live, as the page does
//! with `prefers-color-scheme`; `--theme dark` or `light` fixes them (`--light` is `--theme light`). A picture is dark
//! unless asked, so that it does not depend on the machine; `--theme system` reads the setting once.
//! With --pack, the settings panel's Apply to pack writes its values into the pack's config/tectonic.json (only the\n//! values that differ, comments kept); nothing else is written there.
//! The history and the run before (`history.json`, `before.json`) are kept in --runs, the page's localStorage as files.
//! The view keys, --biome and --structure are the page's own `?view=`, `&biome=` and `&structure=`; --camera is its
//! `?camera=`, the 3D view's start in widths of the area (step 15); --level is how finely the 3D view draws the land
//! (`scene::Level`), which the page does not have. In the section view (`--view section`, step 16),
//! --cut is the cut shown (0-based, kept within the cuts), --ore the ores chosen in the list (their kind names, as
//! the cut then draws them alone) and --cut-look the look at the cut: k pixels a block, and the corner at x, y pixels
//! (settled on the first frame, so a place outside the cut is brought back into it).

use std::path::{Component, Path, PathBuf};

use mere_base::{Vec2, Vec3, log};
use mere_ui::Theme;
use preview::cut::CutLook;
use preview::map::Look;
use preview::paint::View;
use preview::run::Runs;
use preview::scene;
use preview::world::World;
use preview::{MIN_SIZE, PreviewApp, PreviewState, text};

fn usage() -> ! {
    eprintln!(
        "usage: preview [--data DIR] [--theme dark|light|system] [--light] [--size WxH] [--shot out.png] [--view k[,k]] [--level slopes|blocks|fine] [--view-only] [--biome id,..] [--structure surface|under|id,..] [--look k,x,y] [--camera cx,cy,cz] [--cut N] [--ore kind,..] [--cut-look k,x,y] [--runs DIR] [--engine DIR] [--pack DIR] [--setting-help KEY]"
    );
    std::process::exit(2);
}

/// Which colours: the page's own two, or the system's setting.
#[derive(Copy, Clone)]
enum Scheme {
    Dark,
    Light,
    System,
}

fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

fn parse_look(s: &str) -> Option<Look> {
    let v: Vec<f32> = s.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let &[k, x, y] = v.as_slice() else { return None };
    // The page keeps k within 1 and 16 and the corner within the frame, whatever it is asked.
    Some(Look { k: k.clamp(1.0, 16.0), x, y }.clamped())
}

fn parse_cut_look(s: &str) -> Option<CutLook> {
    let v: Vec<f64> = s.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let &[k, x, y] = v.as_slice() else { return None };
    (k.is_finite() && x.is_finite() && y.is_finite()).then_some(CutLook { k, x, y })
}

fn parse_camera(s: &str) -> Option<Vec3> {
    let v: Vec<f32> = s.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let &[x, y, z] = v.as_slice() else { return None };
    (x.is_finite() && y.is_finite() && z.is_finite()).then(|| Vec3::new(x, y, z))
}

/// Applies the page's `?view=` keys, one after the other as clicks on its tabs do.
fn apply_view(state: &mut PreviewState, key: &str) {
    match key {
        "" => {}
        "section" => state.view = preview::paint::SECTION,
        "flat2d" => state.in3d = false,
        "relief3d" => state.in3d = true,
        _ => match View::from_key(key) {
            Some(v) => state.view = v.index(),
            None => eprintln!("preview: unknown view '{key}'"),
        },
    }
}

/// The only place a run may write for now: this crate's target\tmp.
const TMP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/target/tmp");

/// A canonical path in the plain form PowerShell and the generator take (without the `\\?\` prefix).
fn plain(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
}

/// `p` made absolute, with `.` and `..` resolved by name and no look at the disk.
fn lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()).components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            part => out.push(part),
        }
    }
    out
}

/// Whether `inside` is strictly under `outside`: Windows names are not case-sensitive.
fn under(inside: &Path, outside: &Path) -> bool {
    let (a, b) = (inside.to_string_lossy().to_lowercase(), outside.to_string_lossy().to_lowercase());
    a.len() > b.len() && a.starts_with(&b) && matches!(a.as_bytes()[b.len()], b'\\' | b'/')
}

/// The runs' folder, made, and refused (exit 2) when it is not inside target\tmp. Judged by its name first, so that
/// nothing is created outside, and again once made, in case a link leads elsewhere.
fn runs_root(asked: Option<PathBuf>) -> PathBuf {
    let tmp = Path::new(TMP);
    if let Err(e) = std::fs::create_dir_all(tmp) {
        eprintln!("preview: {}: {e}", tmp.display());
        std::process::exit(1);
    }
    let tmp = plain(&tmp.canonicalize().expect("target/tmp was just made"));
    let root = lexical(&asked.unwrap_or_else(|| tmp.join("runs")));
    if !under(&root, &tmp) {
        eprintln!("preview: --runs {} is not inside {}: runs write only there until a real run has been exercised", root.display(), tmp.display());
        std::process::exit(2);
    }
    let made = std::fs::create_dir_all(&root).and_then(|_| root.canonicalize());
    match made {
        Ok(real) if under(&plain(&real), &tmp) => plain(&real),
        Ok(real) => {
            eprintln!("preview: --runs {} leads outside {}", real.display(), tmp.display());
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("preview: --runs: {e}");
            std::process::exit(1);
        }
    }
}
fn main() {
    log::add_sink(log::stderr_sink(log::Level::Info));
    let (mut scheme, mut shot, mut size) = (None::<Scheme>, None, (1400u32, 940u32));
    let mut view_only = false;
    let (mut data, mut views, mut biomes, mut structures, mut look) = (None::<PathBuf>, Vec::new(), String::new(), String::new(), None);
    let (mut camera, mut cut, mut ores, mut cut_look) = (None, None, String::new(), None);
    let (mut runs, mut engine_dir, mut pack) = (None::<PathBuf>, std::env::var_os("WORLD_PREVIEW_DIR").map(PathBuf::from), None::<PathBuf>);
    let mut help = None::<String>;
    let mut level = None::<scene::Level>;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| usage());
        match a.as_str() {
            "--light" => scheme = Some(Scheme::Light),
            "--theme" => {
                scheme = Some(match value().as_str() {
                    "dark" => Scheme::Dark,
                    "light" => Scheme::Light,
                    "system" => Scheme::System,
                    _ => usage(),
                })
            }
            "--size" => size = parse_size(&value()).unwrap_or_else(|| usage()),
            "--shot" => shot = Some(value()),
            "--data" => data = Some(PathBuf::from(value())),
            "--view" => views.extend(value().split(',').map(str::to_string)),
            "--view-only" => view_only = true,
            "--level" => level = Some(scene::Level::from_key(&value()).unwrap_or_else(|| usage())),
            "--biome" => biomes = value(),
            "--structure" => structures = value(),
            "--look" => look = Some(parse_look(&value()).unwrap_or_else(|| usage())),
            "--camera" => camera = Some(parse_camera(&value()).unwrap_or_else(|| usage())),
            "--cut" => cut = Some(value().parse::<usize>().unwrap_or_else(|_| usage())),
            "--ore" => ores = value(),
            "--cut-look" => cut_look = Some(parse_cut_look(&value()).unwrap_or_else(|| usage())),
            "--runs" => runs = Some(PathBuf::from(value())),
            "--engine" => engine_dir = Some(PathBuf::from(value())),
            "--pack" => pack = Some(PathBuf::from(value())),
            // The help of one setting open under its row, by its key (`settings::SETTINGS`).
            "--setting-help" => help = Some(value()),
            _ => usage(),
        }
    }
    // A folder that is missing or unreadable is not a reason to close the window: the page then shows its `noPreview`
    // note, and a folder that is there but cannot be read says so in the status line, where the page puts a trouble.
    let data = data.or_else(|| {
        let dir = preview::world::data_dir();
        dir.join("preview.json").exists().then_some(dir)
    });
    let mut trouble = String::new();
    let world = data.and_then(|dir| match World::load(&dir) {
        Ok(w) => Some(w),
        Err(e) => {
            mere_base::observe!(Warn, "{}: {e}", dir.display());
            if dir.join("preview.json").exists() {
                trouble = text::UNREADABLE.to_string();
            }
            None
        }
    });

    let mut state = PreviewState::for_world(world.as_ref());
    state.trouble = trouble;
    state.view_only = view_only;
    if let Some(level) = level {
        state.level = level;
    }
    for key in &views {
        apply_view(&mut state, key);
    }
    for id in biomes.split(',').filter(|s| !s.is_empty()) {
        state.chosen[0].insert(id.to_string());
    }
    // "surface" and "under" stand for every structure of that part, whatever the pack calls them (lines 2866-2870).
    for id in structures.split(',').filter(|s| !s.is_empty()) {
        let part = match id {
            "surface" => Some(false),
            "under" => Some(true),
            _ => None,
        };
        match (part, world.as_ref().and_then(|w| w.structures.as_ref())) {
            (None, _) => {
                state.chosen[View::Structures.index()].insert(id.to_string());
            }
            (Some(under), Some(s)) => {
                for (i, kind) in s.ids.iter().enumerate() {
                    if s.under[i] == under && s.count[i] > 0 {
                        state.chosen[View::Structures.index()].insert(kind.clone());
                    }
                }
            }
            (Some(_), None) => {}
        }
    }
    if let Some(l) = look {
        state.look = l;
    }
    for kind in ores.split(',').filter(|s| !s.is_empty()) {
        state.chosen_ores.insert(kind.to_string());
    }
    // A cut past the last is the last, once the world says how many there are; without a section there is none to name.
    if let Some(n) = cut {
        let cuts = world.as_ref().and_then(|w| w.section.as_ref()).map_or(0, |s| s.cuts.len());
        state.cut_at = n.min(cuts.saturating_sub(1));
    }
    if let Some(l) = cut_look {
        state.cut_look = l;
    }
    if let Some(key) = help {
        // `view_only` opens the filter's own help.
        let at = if key == "view_only" {
            preview::FILTER_HELP
        } else {
            preview::settings::SETTINGS.iter().position(|s| s.key == key).unwrap_or_else(|| usage())
        };
        state.help = Some(at);
    }
    // Without a world there is nothing to look at: the page ignores it too.
    if let (Some(c), Some(w)) = (camera, world.as_ref()) {
        state.orbit = Some(scene::orbit(w, Some(c)));
    }

    // The window follows the system, as the page does; a picture is dark unless asked otherwise, so that the
    // pictures made by a command line do not depend on the machine. `--theme system` with `--shot` reads the system once.
    let scheme = scheme.unwrap_or(if shot.is_some() { Scheme::Dark } else { Scheme::System });
    let (dark, follows) = match scheme {
        Scheme::Dark => (true, false),
        Scheme::Light => (false, false),
        Scheme::System => (mere_win32::system_dark(), shot.is_none()),
    };
    let theme = if dark { Theme::dark() } else { Theme::light() };
    // The window holds its minimum itself; a picture is raised to it too, so that it shows the layout the app has.
    let (w, h) = preview::min_size_px(size);
    let runs = Runs { engine_dir, engine: preview::run::engine(), pack, root: runs_root(runs), mods: Vec::new(), launcher: Box::new(mere_win32::SystemLauncher), server_dir: None, out_dir: None, exports_dir: None };
    let mut state = state;
    state.reduce_motion = mere_win32::reduced_motion();
    let mut app = PreviewApp::new(theme, dark, Vec2::new(w as f32, h as f32), state, world).follow_scheme(follows).with_runs(runs);
    app.ui_mut().set_clipboard(Box::new(mere_win32::SystemClipboard));
    match shot {
        Some(path) => {
            let rgba = mere_win32::shot(&mut app, w, h, 1.0).unwrap_or_else(|e| {
                eprintln!("preview: {e}");
                std::process::exit(1);
            });
            if let Err(e) = std::fs::write(&path, mere_png::encode(w, h, &rgba)) {
                eprintln!("preview: {path}: {e}");
                std::process::exit(1);
            }
            mere_base::observe!(Info, "wrote {path}");
        }
        None => {
            // The window draws its own bar (src/bar.rs) and holds the minimum size the layout needs.
            let frame = mere_win32::Frame { own: true, min: MIN_SIZE, ..Default::default() };
            if let Err(e) = mere_win32::run_with(text::BAR_TITLE, &mut app, frame) {
                eprintln!("preview: {e}");
                std::process::exit(1);
            }
        }
    }
}
