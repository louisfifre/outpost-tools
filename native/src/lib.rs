//! The native launcher: the world preview's window (`preview::PreviewApp`) with the launcher's Home given to it,
//! talking to the same api as the page's launcher, in process.

pub mod bridge;
pub mod fake;
pub mod home;
pub mod icons;
pub mod model;
pub mod text;

use nui_base::Vec2;
use nui_ui::{Slide, Theme};
use preview::run::Runs;
use preview::world::World;
use preview::{Mode, PreviewApp, PreviewState};

pub use home::{HomeIds, LauncherHome};

/// The origin the api is given and is called with, so that its own check of who asks passes as it does for the page.
pub const ORIGIN: &str = "native://launcher";

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
