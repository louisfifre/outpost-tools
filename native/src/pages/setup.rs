//! The first start's set-up (boards E1 and E2): Java, Prism, the sign-in. A placeholder, with no title line: the
//! set-up is not a page of Home, it stands before it.

use mere_ui::Ui;

use crate::home::LauncherHome;

/// The page's width.
pub const WIDTH: f32 = 560.0;

/// What the page keeps between frames.
#[derive(Default)]
pub struct State {}

pub fn build(_home: &mut LauncherHome, _ui: &mut Ui) {}
