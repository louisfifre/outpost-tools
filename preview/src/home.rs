//! The launcher's Home, which the host of the preview gives. The preview crate owns the window, the bar and the
//! preview tool; what Home shows (profiles, a server) is the launcher's, so Home is a trait the host fills.
//! Without one the pane says that Home is for the launcher (the standalone preview's PLACEHOLDER).

use mere_base::Vec2;
use mere_ui::Ui;

/// A picture of pixels, 4 bytes each (RGBA, rows from the top): an account's face and its player, as the skin gives them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pixels {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// One account of the bar's menu.
#[derive(Clone, Debug, PartialEq)]
pub struct AccountEntry {
    pub name: String,
    /// The one the game is played with: marked with a check.
    pub in_use: bool,
    /// Its head, 8 x 8 pixels; None while its skin is not read.
    pub face: Option<Pixels>,
}

/// What the bar's account menu shows: the accounts, and the player of the one in use. No account: no menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountMenu {
    pub accounts: Vec<AccountEntry>,
    /// The player of the account in use, from the front, 24 x 40 pixels.
    pub figure: Option<Pixels>,
}

/// What the person asked of the account menu.
#[derive(Clone, Debug, PartialEq)]
pub enum AccountAsk {
    /// The menu was opened: read the accounts again.
    Opened,
    /// Play with this account.
    Use(String),
    /// Add an account.
    Add,
    /// Sign this account out.
    SignOut(String),
}

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
    /// The accounts the bar's menu lists; none by default (the standalone preview has no account).
    fn account_menu(&self) -> std::sync::Arc<AccountMenu> {
        std::sync::Arc::default()
    }
    /// What the person asked of the account menu; acted on by the next `act`.
    fn account_ask(&mut self, _ask: AccountAsk) {}
    /// The first-start set-up shows (boards E1 and E2): the bar then holds only the brand and the window's buttons, no
    /// tools and no account, since there is nothing yet to switch to or to sign in as.
    fn bare_bar(&self) -> bool {
        false
    }
    fn as_any(&self) -> &dyn std::any::Any;
}
