//! The side panels' widths and the form of the page, from a handful of numbers set by hand and from the panels'
//! content. Everything else (the width at which the panels stack, the window's smallest size, the bar's centred
//! threshold) is derived from them here and in `overflow.rs`, and never typed again.
//!
//! Two forms. Beside: the settings panel, the stage and the figures panel in a row, the stage's square taking
//! what the panels leave. Stacked, when the panels' minimums and the map's minimum no longer fit side by side:
//! the stage on top, the open panels below it in equal columns.

use std::sync::OnceLock;

use mere_ui::tokens;
use mere_ui::{GUTTER, ROW_GAP, SHARE_MIN, SWATCH_W, TextLook, Tone, WIDTH_MIN};

use crate::bands::{HEIGHT_STEPS, SEA, SEA_DEPTHS, band_names};
use crate::overflow::NARROW_BAR;
use crate::settings::{SETTINGS, widest_number_text};
use crate::text;

// ---- the values set by hand ----

/// The least a panel is wide, and the most its natural width reaches (a dragged panel may pass it).
pub const PANEL_MIN: f32 = 220.0;
pub const PANEL_MAX: f32 = 330.0;
/// The least the stage's column is wide while the panels are beside it.
pub const MAP_MIN: f32 = 238.0;
/// The space between the window's edge, the panels and the stage: beside, and stacked.
pub const GAP: f32 = 28.0;
pub const GAP_STACKED: f32 = 16.0;
/// The window's least height.
pub const MIN_HEIGHT: f32 = 600.0;
/// The hit zone of the grip between a panel and the stage, centred in the gap.
pub const GRIP_W: f32 = 8.0;

// The content's own measures: the rows are built with these same names (shell.rs, mere-ui's list), so the natural widths follow them.
/// `.setting`: the gap between its three columns. The field's width is `setting_field()`.
pub const SETTING_GAP: f32 = 10.0;
/// The gap between the columns of the figures grid, and the least widths of its value and change columns (the
/// list rows' own, so that a figures row and a list row of one panel read alike).
pub const FIG_GAP: f32 = 14.0;
pub const FIG_VALUE: f32 = WIDTH_MIN;
pub const FIG_CHANGE: f32 = SHARE_MIN;
/// A list row: the swatch, the gaps, and the share column (6 ch of 12 px); the width column is the widest width a
/// list shows, `LIST_WIDTH`, which is wider than the column's 8 ch minimum.
const LIST_CHROME: f32 = SWATCH_W + 3.0 * ROW_GAP + SHARE_MIN;
const LIST_WIDTH: &str = "10,000 blocks";
/// The value and the change of a figures row.
const FIG_CELLS: f32 = FIG_VALUE + FIG_CHANGE;

// ---- what is derived ----

const fn max(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

const fn ceil(x: f32) -> f32 {
    let t = x as i32 as f32;
    if t < x { t + 1.0 } else { t }
}

/// How many gaps the page holds: the two edges and one between each open panel and the stage.
pub const fn gaps(open: usize) -> usize {
    open + 2
}

/// The narrowest window the panels fit beside the stage in, for this many open panels.
pub const fn beside_min(open: usize) -> f32 {
    gaps(open) as f32 * GAP + open as f32 * PANEL_MIN + MAP_MIN
}

/// Below this width, with both panels open, the panels stack.
pub const STACK_AT: f32 = beside_min(2);
/// The narrowest the stacked panels fit: two columns and the three gaps around them.
pub const STACKED_MIN: f32 = 2.0 * PANEL_MIN + 3.0 * GAP_STACKED;
/// The window's least width: the narrow bar, or the stacked panels, whichever needs more.
pub const MIN_WIDTH: f32 = ceil(max(NARROW_BAR, STACKED_MIN));

fn body() -> TextLook {
    TextLook::body(Tone::Text)
}

/// The settings' field column: the widest text a number field shows (`widest_number_text`) plus the field's
/// border and padding on both sides, so that no value of the 43 settings is cut. Worked out from the data, not
/// typed: a setting with a longer range widens it.
pub fn setting_field() -> f32 {
    static FIELD: OnceLock<f32> = OnceLock::new();
    *FIELD.get_or_init(|| body().width(&widest_number_text()) + 2.0 * (tokens::FRAME + tokens::INSET))
}

/// The widest row that fits under the maximum, rounded up to a whole pixel, and at least the minimum. A row wider
/// than the maximum does not count: its label wraps instead.
fn natural_of(rows: impl Iterator<Item = f32>) -> f32 {
    let widest = rows.map(|w| w + GUTTER).filter(|w| *w <= PANEL_MAX).fold(PANEL_MIN, f32::max);
    // A thousandth off, so that the float error of a sum that is a whole number does not round up a pixel.
    (widest - 1e-3).ceil()
}

/// What each panel is wide when nobody dragged it: the widest of its rows laid out on one line (the label, the
/// field or value, the gaps between, and the scrollbar's gutter), the settings first and the figures second.
/// The rows are the panel's fixed words, never the data of a run, so that a panel does not change width when the
/// view or the world does.
pub fn natural() -> [f32; 2] {
    static NATURAL: OnceLock<[f32; 2]> = OnceLock::new();
    *NATURAL.get_or_init(|| {
        let b = body();
        let settings = SETTINGS.iter().map(|s| b.width(s.label) + 2.0 * SETTING_GAP + setting_field());
        let figures = text::FIGURE_NAMES.iter().map(|n| b.width(n) + 2.0 * FIG_GAP + FIG_CELLS);
        let bands = band_names(SEA, &HEIGHT_STEPS, &SEA_DEPTHS).into_iter().map(|n| b.width(&n) + b.width(LIST_WIDTH) + LIST_CHROME);
        [natural_of(settings), natural_of(figures.chain(bands))]
    })
}

/// How the page is laid out at a window width.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Plan {
    /// The panels sit under the stage instead of beside it.
    pub stacked: bool,
    /// The settings and the figures panel's widths; 0 for a closed one.
    pub widths: [f32; 2],
}

fn count(open: [bool; 2]) -> usize {
    open.iter().filter(|o| **o).count()
}

/// Whether the open panels no longer fit beside the stage at this width. With none open there is nothing to stack.
pub fn is_stacked(width: f32, open: [bool; 2]) -> bool {
    let n = count(open);
    n > 0 && width < beside_min(n)
}

/// The panels' widths at a window width, `dragged` being what a hand set. Beside: each open panel is its width
/// (natural, or dragged); when the window narrows the stage's column shrinks first, and once it is at its minimum
/// the panels shrink toward their own, each in proportion to what it has above its minimum. Stacked: equal columns
/// filling the width.
pub fn plan(width: f32, open: [bool; 2], dragged: [Option<f32>; 2]) -> Plan {
    let n = count(open);
    if is_stacked(width, open) {
        let w = if n == 2 { ((width - 3.0 * GAP_STACKED) / 2.0 + 1e-3).floor() } else { width - 2.0 * GAP_STACKED };
        return Plan { stacked: true, widths: [if open[0] { w } else { 0.0 }, if open[1] { w } else { 0.0 }] };
    }
    let nat = natural();
    let mut w = [0, 1].map(|i| if open[i] { dragged[i].unwrap_or(nat[i]) } else { 0.0 });
    let room = width - gaps(n) as f32 * GAP - MAP_MIN;
    let over = w[0] + w[1] - room;
    // Whether the panels are held to what the stage's minimum leaves them (rather than at their wanted width, or all
    // at their minimum already).
    let mut held = false;
    if over > 0.0 {
        let slack = [0, 1].map(|i| if open[i] { (w[i] - PANEL_MIN).max(0.0) } else { 0.0 });
        let total = slack[0] + slack[1];
        if total > 0.0 {
            let k = (over / total).min(1.0);
            for i in 0..2 {
                w[i] -= slack[i] * k;
            }
            held = k < 1.0;
        }
    }
    // Whole pixels, rounded down so that the rounding never takes from the stage's minimum.
    let mut out = w.map(|v| (v + 1e-3).floor());
    if held {
        // What the rounding down left over goes back, a pixel at a time, to the panel that lost the most to it: the
        // stage then stays at its minimum to the pixel instead of rising by one as the panels shrink.
        let mut spare = (room - out[0] - out[1] + 1e-3).floor();
        let mut order = [0, 1];
        order.sort_by(|&a, &b| (w[b] - out[b]).total_cmp(&(w[a] - out[a])));
        for i in order {
            if spare >= 1.0 && open[i] {
                out[i] += 1.0;
                spare -= 1.0;
            }
        }
    }
    Plan { stacked: false, widths: out }
}

/// The width a drag may give panel `side` at a window width: from the minimum to what leaves the stage its minimum
/// next to the other panel as it is. There is no maximum: the natural width is a starting point.
pub fn clamp_drag(width: f32, open: [bool; 2], other: f32, wanted: f32) -> f32 {
    let most = width - gaps(count(open)) as f32 * GAP - MAP_MIN - other;
    (wanted.clamp(PANEL_MIN, most.max(PANEL_MIN)) + 1e-3).floor()
}
