//! The launcher's Home, which the host of the preview gives. The preview crate owns the window, the bar and the
//! preview tool; what Home shows (profiles, a server) is the launcher's, so Home is a trait the host fills.
//! Without one the pane says that Home is for the launcher (the standalone preview's PLACEHOLDER).

use mere_base::Vec2;
use mere_ui::Ui;

pub trait Home {
    /// Builds Home in the pane under the bar; `size` is the pane's (window width, window height - BAR_H).
    fn build(&mut self, ui: &mut Ui, size: Vec2);
    /// Acts on what the last build recorded; true when something changed and the page must be built again.
    fn act(&mut self) -> bool;
    /// Takes the answers that came back and starts the polls that are due; `now` is the app's clock in seconds,
    /// `on_screen` whether Home is the tool shown and the window visible. True when a frame is wanted.
    fn poll(&mut self, now: f64, on_screen: bool) -> bool;
    /// Seconds until Home next needs a frame with no event; None for never.
    fn wake_in(&self, now: f64) -> Option<f32>;
    /// The tool was switched to Home (the page's showMode: load() and loadServer()).
    fn shown(&mut self);
    /// The bar's #account: None while unknown.
    fn account(&self) -> Option<String>;
    fn as_any(&self) -> &dyn std::any::Any;
}
