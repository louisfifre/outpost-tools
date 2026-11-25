//! A profile's page (board B1): the game's settings and the mods. A placeholder: its title line and, in the left
//! half, the settings Home used to open in place (`settings_form`), until the screen replaces them.

use mere_ui::Ui;

use crate::home::{COLUMNS_GAP, LauncherHome};

/// The page's width: two columns of a half each.
pub const WIDTH: f32 = 1120.0;

/// What the page keeps between frames.
#[derive(Default)]
pub struct State {}

pub fn build(home: &mut LauncherHome, ui: &mut Ui) {
    let name = home.profile_name();
    home.page_frame(ui, WIDTH, &name, |_| {}, |home, ui| {
        home.half(ui, (WIDTH - COLUMNS_GAP) / 2.0, |home, ui| home.settings_form(ui));
    });
}
