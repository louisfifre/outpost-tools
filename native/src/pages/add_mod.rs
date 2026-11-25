//! The page that adds a mod to a profile (board B4): a search of Modrinth and the lines of its results. A placeholder.

use mere_ui::Ui;

use crate::home::LauncherHome;
use crate::text;

/// The page's width.
pub const WIDTH: f32 = 760.0;

/// What the page keeps between frames.
#[derive(Default)]
pub struct State {}

pub fn build(home: &mut LauncherHome, ui: &mut Ui) {
    let title = text::add_mod_title(&home.profile_name());
    home.page_frame(ui, WIDTH, &title, |_| {}, |_, _| {});
}
