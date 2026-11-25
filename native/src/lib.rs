//! The native launcher: the world preview's window (`preview::PreviewApp`) with the launcher's Home given to it,
//! talking to the same api as the page's launcher, in process.

pub mod bridge;
pub mod fake;
pub mod home;
pub mod icons;
pub mod model;
pub mod pages;
pub mod text;

use mere_base::Vec2;
use mere_ui::{Slide, Theme};
use preview::run::Runs;
use preview::world::World;
use preview::{Mode, PreviewApp, PreviewState};

pub use home::{HomeIds, LauncherHome};

/// The origin the api is given and is called with, so that its own check of who asks passes as it does for the page.
pub const ORIGIN: &str = "native://launcher";

/// The runs of the preview tool on this pack: the engine runs in this process (world_preview::run), with the mod of
/// the world-preview repository found beside the pack or at WORLD_PREVIEW_DIR, the glue mod's jar on top of the
/// pack's mods (the section then shows the pack's own ores), and the folders serve.ps1 is given. Without the
/// repository, or when the pack's scripts/common.ps1 could not be read (the reason goes to the log), Generate says
/// it failed.
pub fn runs(api: &launcher::api::Api, repo: &std::path::Path) -> Runs {
    let paths = api.preview_paths().map_err(|why| api.write_log(&why)).ok();
    let mut mods = Vec::new();
    match api.glue_mod() {
        Ok(jar) => mods.push(jar),
        Err(why) => api.write_log(&why),
    }
    let root = repo.join("run").join("app").join("preview");
    let _ = std::fs::create_dir_all(&root);
    Runs {
        engine_dir: paths.as_ref().map(|p| p.engine.clone()).filter(|engine| engine.join("mod").join("build.gradle").exists()),
        engine: preview::run::engine(),
        pack: Some(paths.as_ref().map_or_else(|| api.repo.join("pack"), |p| p.pack.clone())),
        apply_pack: paths.as_ref().and_then(|p| p.source_pack.clone()),
        root,
        mods,
        launcher: Box::new(mere_win32::SystemLauncher),
        server_dir: paths.as_ref().map(|p| p.dir.clone()),
        out_dir: paths.as_ref().map(|p| p.out.clone()),
        exports_dir: paths.as_ref().map(|p| p.exports.clone()),
    }
}

/// Where the preview's Apply looks for places on this machine: Prism's instances as the launcher lists them, the
/// official launcher's installations, and the pack's test server. What cannot be read is left out, the reason in the
/// log.
pub fn sources(api: &launcher::api::Api) -> preview::places::Sources {
    let mut sources = preview::places::Sources::system();
    match api.prism_games() {
        Ok(games) => sources.prism = games.into_iter().map(|(name, dir)| preview::places::Game { name, dir }).collect(),
        Err(why) => api.write_log(&format!("apply: Prism's instances are not known: {why}")),
    }
    sources.dev_server = api.preview_paths().ok().map(|p| p.dev_server);
    sources
}

/// The app: the preview's, in Home, with the launcher's Home and, when given, the runs the preview tool starts.
pub fn app(theme: Theme, dark: bool, size: Vec2, world: Option<World>, runs: Option<Runs>, home: LauncherHome, reduce_motion: bool) -> PreviewApp {
    let mut state = PreviewState::for_world(world.as_ref());
    state.mode = Mode::Home;
    state.options = Slide::settled(false);
    state.reduce_motion = reduce_motion;
    let mut app = PreviewApp::new(theme, dark, size, state, world);
    if let Some(runs) = runs {
        app = app.with_runs(runs);
    }
    app.with_home(Box::new(home))
}
