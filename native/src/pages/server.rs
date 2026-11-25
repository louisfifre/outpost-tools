//! A server's page (board C1): where it was made from, its settings and its start. A placeholder: its title line
//! and, in the left half, the settings Home used to open in place (`server_form`), until the screen replaces them.

use mere_ui::Ui;

use crate::home::{COLUMNS_GAP, LauncherHome};
use crate::text;

/// The page's width: two columns of a half each.
pub const WIDTH: f32 = 1120.0;

/// What the page keeps between frames.
#[derive(Default)]
pub struct State {}

pub fn build(home: &mut LauncherHome, ui: &mut Ui) {
    home.page_frame(ui, WIDTH, text::SERVER_NAME, |_| {}, |home, ui| {
        home.half(ui, (WIDTH - COLUMNS_GAP) / 2.0, |home, ui| home.server_form(ui));
    });
}
