//! What the bar gives up when the window is too narrow for all of it, worked out from the real widths of its
//! parts before they are built. The widths are arithmetic (the font is monospaced: a word is a count of cells
//! times a constant, see `mere_ui::TextLook::width`), so the thresholds are measured, not typed in: a longer
//! word, another account name or one more view moves them by itself.
//!
//! The bar is a grid of three cells. In the preview tool the left cell holds the settings panel's button and the
//! right cell the figures panel's button and the window's buttons; in Home the left cell holds the brand and the
//! right one the window's buttons. While the bar is centred the side cells are equal, `(width - tools) / 2`, so
//! the tools stay in the middle, and each side has to hold what its cell holds. The bar gives way in this order,
//! taking the first step that fits:
//!
//! 1. in Home, the account name (cut with an ellipsis down to a short stub, then gone whole), then the word
//!    the brand's name if that buys room;
//! 2. in the preview tool, the views that are greyed (they need the 16-block detail) move into "More", then the
//!    other views, one at a time from the strip's end, the pressed one staying;
//! 3. in the preview tool, when even that bar cannot be centred, the tools stop being centred and sit left after
//!    the settings button;
//! 4. then 2D and 3D move into "More" with the views.
//!
//! The two tools keep their names, always.

use mere_ui::tokens::{GAP, GAP_TIGHT, ICON, ICON_SQUARE};
use mere_ui::{ADVANCE_EM, TOOL_GAP, TOOL_PAD_L, TOOL_PAD_R, TextLook, Tone, tool_button_width};

use crate::text;

/// From the window's edge to an end button's square: the icon in it then sits 12 px in, where the brand's cube does.
pub const EDGE: f32 = 12.0 - (ICON_SQUARE - ICON) / 2.0;
/// The window's three buttons, `icon-square` each, and the room after them: what the right cell holds at least.
pub const ENDS_W: f32 = 3.0 * ICON_SQUARE + EDGE;
/// A panel's button: the window buttons' unframed square.
pub const TOGGLE_W: f32 = ICON_SQUARE;
/// Before the settings button, so that its icon starts where the brand's cube does.
pub const TOGGLE_EDGE: f32 = EDGE;
/// Between the figures button and the window's buttons.
pub const TOGGLE_GAP: f32 = GAP;
/// Between the preview tool and its views: both can be filled at once, and must not touch.
pub const TOOL_VIEWS_GAP: f32 = GAP_TIGHT;
/// What the left cell holds in the preview tool, and the right one, with the gap the tools keep from the buttons.
pub const LEFT_NEED: f32 = TOGGLE_EDGE + TOGGLE_W;
pub const RIGHT_NEED: f32 = TOGGLE_W + TOGGLE_GAP + ENDS_W + TOOLS_GAP;
/// The least space between the tools and the panels' buttons: after the settings button when they sit left, and
/// before the figures button always.
pub const TOOLS_GAP: f32 = 12.0;
/// `.brand`'s padding before the cube, the cube, and the gap between the brand's parts.
const BRAND_PAD: f32 = 12.0;
const CUBE: f32 = 16.0;
const BRAND_GAP: f32 = 10.0;
/// A strip tab: 13 px each side of its word, overlapping its neighbour's hairline by 1.
const TAB_PAD: f32 = 13.0;
/// The gap between two groups of the strip.
const SEP: f32 = 12.0;
/// The chevron of the "More" tab, and the space before it.
const CHEVRON: f32 = 8.0;
const CHEVRON_GAP: f32 = GAP;
/// An account cut shorter than this many letters (and the ellipsis) is not worth showing: it goes whole.
pub const ACCOUNT_STUB: usize = 3;

// The same widths as const arithmetic, so that the window's smallest size is a constant. The words are ASCII,
// so a word is as many cells as it has bytes; a test holds these against the runtime widths.
const BODY_CELL: f32 = ADVANCE_EM * 12.0;
const fn body_w(s: &str) -> f32 {
    s.len() as f32 * BODY_CELL
}
const fn tool_w(label: &str) -> f32 {
    TOOL_PAD_L + 16.0 + TOOL_GAP + body_w(label) + TOOL_PAD_R
}
const fn tab_w(word: &str) -> f32 {
    2.0 * TAB_PAD + body_w(word) - 1.0
}
const MORE_W: f32 = 2.0 * TAB_PAD + body_w(text::MORE) + CHEVRON_GAP + CHEVRON - 1.0;
/// The widest tab a view can have (the pressed view always stays in the strip).
const fn widest_view_tab() -> f32 {
    let mut widest = tab_w(text::SECTION);
    let mut i = 0;
    while i < text::VIEWS.len() {
        let w = tab_w(text::VIEWS[i]);
        if w > widest {
            widest = w;
        }
        i += 1;
    }
    widest
}
/// The strip of the narrowest bar: the two tools, one view, "More".
pub const NARROW_STRIP: f32 = 1.0 + (tool_w(text::TOOL_NAMES[0]) - 1.0) + SEP + (tool_w(text::TOOL_NAMES[1]) - 1.0) + 1.0 + TOOL_VIEWS_GAP + widest_view_tab() + MORE_W;
/// The narrowest the preview tool's bar can be: the settings button, the tools sat left, the figures button and the
/// window's buttons.
pub const NARROW_BAR: f32 = LEFT_NEED + TOOLS_GAP + NARROW_STRIP + RIGHT_NEED;

/// What the bar shows, as worked out for one width.
#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    /// The account name is shown (cut with an ellipsis if need be). Home only.
    pub account: bool,
    /// The brand's name (OUTPOST TOOLS) is shown. Home only.
    pub word: bool,
    /// The views the "More" tab holds, in the strip's order.
    pub more: Vec<usize>,
    /// The tools are centred in the bar; otherwise they sit left after the settings button.
    pub centred: bool,
    /// 2D and 3D are rows of "More" instead of tabs.
    pub shapes_in_more: bool,
}

impl Default for Form {
    fn default() -> Form {
        Form { account: true, word: true, more: Vec::new(), centred: true, shapes_in_more: false }
    }
}

impl Form {
    /// Whether the strip has a "More" tab: views or shapes are held back.
    pub fn has_more(&self) -> bool {
        !self.more.is_empty() || self.shapes_in_more
    }
}

/// What the plan reads.
#[derive(Clone, Debug)]
pub struct Inputs<'a> {
    /// The window's width.
    pub width: f32,
    pub account: Option<&'a str>,
    /// The preview tool is the one on show: its views take room in the strip, and its panel buttons are on the
    /// bar's ends instead of the brand.
    pub preview_open: bool,
    /// The pressed view, which always stays in the strip.
    pub pressed: usize,
    /// Which of the six views are greyed.
    pub disabled: [bool; VIEW_COUNT],
    /// 2D and 3D after the views (not built in the section).
    pub shapes: bool,
}

pub const VIEW_COUNT: usize = 6;

/// The word of view `i` (the five views, then the section).
pub fn view_word(i: usize) -> &'static str {
    if i < text::VIEWS.len() { text::VIEWS[i] } else { text::SECTION }
}

fn body() -> TextLook {
    TextLook::body(Tone::Text)
}

/// A strip tab's width less the 1 px it is pulled over its neighbour.
pub fn tab_step(word: &str) -> f32 {
    2.0 * TAB_PAD + body().width(word) - 1.0
}

/// The "More" tab's, the same way.
pub fn more_step() -> f32 {
    2.0 * TAB_PAD + body().width(text::MORE) + CHEVRON_GAP + CHEVRON - 1.0
}

/// The strip's width in a form.
pub fn strip_width(i: &Inputs, form: &Form) -> f32 {
    // The strip's own 1 px of padding, then the tools: each pulled 1 px over what is before it.
    let mut w = 1.0 + (tool_button_width(text::TOOL_NAMES[0]) - 1.0) + SEP + (tool_button_width(text::TOOL_NAMES[1]) - 1.0);
    if i.preview_open {
        // The options' box pads its tabs by 1 px on the left, which the first tab's pull takes back, and by the
        // gap that keeps them off the tool.
        w += 1.0 + TOOL_VIEWS_GAP;
        w += (0..VIEW_COUNT).filter(|v| !form.more.contains(v)).map(|v| tab_step(view_word(v))).sum::<f32>();
        if form.has_more() {
            w += more_step();
        }
        if i.shapes && !form.shapes_in_more {
            w += SEP + text::SHAPES.iter().map(|s| tab_step(s)).sum::<f32>();
        }
    }
    w
}

/// The account's narrowest showing: a stub and the ellipsis, or the whole name when that is shorter.
pub fn account_stub_width(account: &str) -> f32 {
    let look = TextLook::body(Tone::Muted);
    look.width(account).min((ACCOUNT_STUB + 1) as f32 * look.width("m"))
}

/// What the brand needs in its cell for a form (Home).
pub fn brand_need(i: &Inputs, form: &Form) -> f32 {
    let mut w = BRAND_PAD + CUBE;
    if form.word {
        w += BRAND_GAP + TextLook::heading().width(&text::BAR_TITLE.to_uppercase());
    }
    if let (true, Some(a)) = (form.account, i.account) {
        w += BRAND_GAP + account_stub_width(a);
    }
    w
}

/// What the left and the right cell hold at least, in a form.
pub fn sides(i: &Inputs, form: &Form) -> (f32, f32) {
    if i.preview_open { (LEFT_NEED, RIGHT_NEED) } else { (brand_need(i, form), ENDS_W) }
}

/// Whether the form fits: centred, each side cell `(width - strip) / 2` holds what it must; otherwise the left
/// cell, the gap, the strip and the right cell side by side.
pub fn fits(i: &Inputs, form: &Form) -> bool {
    let strip = strip_width(i, form);
    let (left, right) = sides(i, form);
    if form.centred {
        (i.width - strip) / 2.0 + 0.01 >= left.max(right)
    } else {
        i.width + 0.01 >= left + TOOLS_GAP + strip + right
    }
}

/// The first form in the order of giving way that fits, or the last one (nothing left to give) when none does.
pub fn plan(i: &Inputs) -> Form {
    let mut steps: Vec<Form> = Vec::new();
    if !i.preview_open {
        // Home: the brand gives way, and nothing else is in the bar to move.
        steps.push(Form::default());
        if i.account.is_some() {
            steps.push(Form { account: false, ..Form::default() });
        }
        steps.push(Form { account: false, word: false, ..Form::default() });
    } else {
        // The greyed views leave first, then the others from the strip's end. The pressed view stays.
        let greyed: Vec<usize> = (0..VIEW_COUNT).filter(|&v| i.disabled[v] && v != i.pressed).collect();
        let from_end: Vec<usize> = (0..VIEW_COUNT).rev().filter(|&v| !i.disabled[v] && v != i.pressed).collect();
        let held = |k: usize| -> Vec<usize> {
            let mut m = greyed.clone();
            m.extend(from_end.iter().take(k));
            m.sort_unstable();
            m
        };
        // The brand is not in the bar of this tool: its parts are off.
        let base = Form { account: false, word: false, ..Form::default() };
        steps.push(Form { more: Vec::new(), ..base.clone() });
        if !greyed.is_empty() {
            steps.push(Form { more: held(0), ..base.clone() });
        }
        for k in 1..=from_end.len() {
            steps.push(Form { more: held(k), ..base.clone() });
        }
        // The tools stop being centred, then the shapes join the views in "More".
        let all = held(from_end.len());
        steps.push(Form { more: all.clone(), centred: false, ..base.clone() });
        if i.shapes {
            steps.push(Form { more: all, centred: false, shapes_in_more: true, ..base });
        }
    }
    let last = steps.len() - 1;
    let mut chosen = steps.into_iter().enumerate().find(|(n, f)| *n == last || fits(i, f)).map(|(_, f)| f).unwrap_or_default();
    // The steps are cumulative, so a Home that gave up the word keeps it gone; but the word is only worth giving
    // up when it buys room. The brand is narrower than the window's buttons it shares the side with, so with the
    // real widths it never does, and the word stays unless the brand is the cell's widest part.
    if !i.preview_open && !chosen.word && fits(i, &Form { word: true, ..chosen.clone() }) {
        chosen.word = true;
    }
    chosen
}
