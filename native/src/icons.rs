//! The back arrow of `.icon.back` (index.html line 258): Pixelarticons' arrow-left, drawn in two paths. The play,
//! stop and settings icons are the preview's bar's own (`preview::bar::icons`), the same paths of lines 418-420.

use std::sync::OnceLock;

use nui_ui::Icon;

pub const BACK: &str = "M20 11v2H4v-2zM8 13v2H6v-2zm2 2v2H8v-2zm2 2v2h-2v-2zm-4-6V9H6v2zM10 15V7H8v8zm2 2V5h-2v12z";

pub fn back() -> &'static Icon {
    static BACK_ICON: OnceLock<Icon> = OnceLock::new();
    BACK_ICON.get_or_init(|| {
        Icon::parse(BACK, 24.0).unwrap_or_else(|e| {
            nui_base::observe!(Error, "icon path '{BACK}': {e}");
            Icon { grid: 24.0, figures: Vec::new() }
        })
    })
}
