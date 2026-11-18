//! The world preview's shell (build plan step 13): the bar and the three columns of `#page`
//! (preview.html lines 63-88, 370-449), built with `mere-ui` and filled with the page's real settings
//! sections and static values. The side panels are fluid (`panels.rs`): each is as wide as its content wants, the
//! middle column is what they leave, its map a square of one rule, and when the window cannot hold both panels
//! and the map's minimum side by side the panels stack under the stage.
//!
//! The right column follows the view with the page's own figures, lists and history (step 18). Each setting has its
//! help: a "?" after its label opens a panel over the row with its hint (`text::SETTING_HINTS`), until closed. A
//! field that differs from the pack's value has that value beside it. Not built: the select's menu, the ores group; in 3D, the structure marks, the right-drag pan, keys
//! and touch, and the no3d note.

use std::collections::BTreeSet;

use mere_base::{Cmd, Color, Cursor, DrawList, Orbit, Rect, Vec2, Weight};
use mere_layout::{Align, Dim, Display, Edges, Insets, Justify, Margin, Position, Side, Style, Track, Tracks};
use mere_ui::{
    ButtonLook, ButtonOpts, FieldLook, FieldOpts, Id, ListRow, PartOpts, RowOpts, ScrollOpts, SelectOpts, Slide, TabLook, TabOpts, TextLook, Theme, Tone, Ui,
    choice_style, details_style, scroll_style, wrap_lines, HelpText, Icon,
};
use mere_ui::tokens::GAP_TIGHT;

use crate::home::Home;
use crate::cut::{self, CUT_IMAGE, CutLook, NAV_H, NAV_IMAGE};
use crate::colour::biome_colour;
use crate::figures::{self, CellKind, Figures, List, blocks, figure_cells, js_round, percent};
use crate::history::{Entry, PLOT_H, here, trend};
use crate::map::{Carry, Look, frame_cmds, point_under, structure_near, tick_at};
use crate::paint::{View, rows};
use crate::panels::{self, Plan};
use crate::run::{self, Ask, DETAIL_BLOCKS, Drawn, Follow, Pace, Steps, duration};
use crate::scene::{self, Level};
use crate::section::{ORE_COLOURS, ORE_KINDS, Shown};
use crate::world::World;
use crate::settings::{Kind, SETTINGS, pack_values};
use crate::text;
use crate::view_filter;

/// Height of the top bar. The launcher's bar is 40 px (launcher line 77); natively it has no hairline (step 19.5),
/// the 40 still holds.
pub const BAR_H: f32 = 40.0;
/// The space between the bar and the content under it, for the world preview and for Home. The page has 18 (preview.html line 77, `padding: 18px 28px 22px`); the maintainer asked for less once the bar floats (step 19.5).
pub const CONTENT_TOP: f32 = 10.0;
/// The scanned area's choices, in points of 16 blocks (preview.html lines 806-809): 256 to 4096 by 256, so 4,096
/// to 65,536 blocks.
pub const AREA_POINTS: [u32; 16] = {
    let mut a = [0; 16];
    let mut i = 0;
    while i < 16 {
        a[i] = 256 * (i as u32 + 1);
        i += 1;
    }
    a
};
/// `widest` (line 808): past this many points the sixteen-block detail is out of reach.
pub const AREA_FINE_MAX: u32 = 2048;
/// `--under` (line 227): what the stage keeps free under the map for `#under`.
const UNDER: f32 = 64.0;
/// The smallest window, in device-independent pixels: what the narrow bar needs, or the two stacked panels with the
/// gaps around them, whichever is more (`panels::MIN_WIDTH`), by 600. Derived, never typed.
pub const MIN_SIZE: (f32, f32) = (panels::MIN_WIDTH, panels::MIN_HEIGHT);

/// The command line's rule for `--size`, in pixels: raised to `MIN_SIZE`, with a warning when it was.
pub fn min_size_px(size: (u32, u32)) -> (u32, u32) {
    let r = raise_to_min(Vec2::new(size.0 as f32, size.1 as f32));
    (r.x as u32, r.y as u32)
}

/// `size` raised to `MIN_SIZE` on each side, with a warning when it was: the one rule of the window's smallest
/// size, for the window (which the system holds at it), the command line and the application alike.
pub fn raise_to_min(size: Vec2) -> Vec2 {
    let raised = Vec2::new(size.x.max(MIN_SIZE.0), size.y.max(MIN_SIZE.1));
    if raised != size {
        mere_base::observe!(Warn, "a window of {}x{} is smaller than the least the layout holds: using {}x{}", size.x, size.y, raised.x, raised.y);
    }
    raised
}

/// What the page keeps of the original's wide layout (lines 75-81, 297, 303), apart from the panels' widths, which
/// are `panels.rs`'s: the padding (the top is `CONTENT_TOP`, 10, instead of the page's 18, step 19.5), the gap
/// between the columns of the figures grid, and the size of the goal figures.
/// Beside, column_h = H - 40 - 10 - 22 = H - 72 and the square's side is min(middle_w, column_h - 64).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Columns {
    pub pad: Edges,
    /// The gap between the columns of the figures grid.
    pub fig_gap: f32,
    /// The font size of the three goal figures.
    pub goal: f32,
}

pub const PAGE: Columns = Columns { pad: Edges::new(CONTENT_TOP, panels::GAP, 22.0, panels::GAP), fig_gap: panels::FIG_GAP, goal: 14.0 };
/// A press the shell recorded and the application acts on after the build (a frame may build up to three times,
/// so the shell never acts itself: recording is idempotent).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Want {
    Generate,
    Stop,
    Export,
    OpenFolder,
    /// Write the panel's values into the pack's config.
    Apply,
    /// The window's buttons: the app turns them into requests to the window.
    Minimize,
    Maximize,
    Close,
}

/// The launcher's two tools (lines 228-236): the Home and the world preview. One is on at a time.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Home,
    Preview,
}

/// How the last run ended, which the page tells its host (`tell`, line 2673): the marker on the preview tool.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RunEnd {
    /// No run ended yet, or it was stopped.
    Idle,
    Ready,
    Failed,
}

/// A press on the grip between a panel and the stage: which panel, where the pointer was, and how wide the panel
/// and the other one were then.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Grip {
    /// 0 for the settings panel, 1 for the figures panel.
    pub side: usize,
    pub from: f32,
    pub start: f32,
    pub other: f32,
}

/// What the application owns; the widgets read it and a click or a keystroke changes it.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewState {
    /// The two side panels, settings then figures: open on the bar's buttons, and the width a hand gave each (None:
    /// its natural width). The widths live for the session and are not kept on disk.
    pub panel_open: [bool; 2],
    pub panel_w: [Option<f32>; 2],
    /// The press on a grip being dragged.
    pub grip: Option<Grip>,
    /// The scroll of each panel as one whole, when the panels are stacked.
    pub panel_y: [f32; 2],
    /// The strip's pressed view: 0 to 4 are `text::VIEWS`, 5 is the section.
    pub view: usize,
    pub in3d: bool,
    /// How finely the 3D view draws the land: the pressed one of `text::LEVELS`.
    pub level: Level,
    pub seed: String,
    pub world_open: bool,
    pub settings_open: bool,
    pub figures_open: bool,
    pub history_open: bool,
    /// The settings' values, in `SETTINGS` order.
    pub values: [f64; 43],
    /// What the pack's config holds: the values `pack` puts back, and what Apply is compared to. The pack's own,
    /// copied from `settings`, until an Apply writes the panel's.
    pub baseline: [f64; 43],
    /// A pack is known (`--pack`), so Apply can write.
    pub pack_known: bool,
    /// What the last Apply said.
    pub apply: crate::apply::Notice,
    /// The pressed detail: an index in `text::DETAILS`.
    pub detail: usize,
    pub settings_y: f32,
    /// The filter above the settings: on, only the groups related to the pressed view are listed (iew_filter).
    /// It stays on when the view changes.
    pub view_only: bool,
    /// What the settings list was cut to at the last frame (iew_filter::list_key), to reset the scroll when it changes.
    pub settings_list: Option<usize>,
    pub list_y: f32,
    /// How the 2D map is looked at, and the press that may become a carry or a click.
    pub look: Look,
    pub carry: Option<Carry>,
    /// What is chosen in each of the five views (by id): what the map shows alone.
    pub chosen: [BTreeSet<String>; 5],
    /// The 3D camera: made when the 3D view is first built, and kept across view and shape switches (the page
    /// resets it only when the area's width changes, line 1873).
    pub orbit: Option<Orbit>,
    /// Where the pointer was at the last frame of a press that began on the 3D view.
    pub turn_from: Option<Vec2>,
    /// The cut of the section being looked at (0-based).
    pub cut_at: usize,
    /// How the cut is looked at; `CutLook::FRESH` until its first settling.
    pub cut_look: CutLook,
    /// A press on the cut: where it began and the look then.
    pub cut_carry: Option<(Vec2, CutLook)>,
    /// A press on the strip: how far from the frame's left edge it took hold.
    pub nav_carry: Option<f64>,
    /// The ores chosen in the section's list, by kind name (`chosen.section`): the cut draws these alone.
    pub chosen_ores: BTreeSet<String>,
    /// The scanned area in 16-block points, the page's `samples` (what the area select holds).
    pub samples: u32,
    /// What the map on screen was made from (the page's `shown`): a fact of the world, whatever the form says.
    pub made: Option<Ask>,
    /// The run being followed, if one is under way.
    pub follow: Option<Follow>,
    /// The status line's two sentences that outlive a run: the trouble (in the warning tone) and what was said.
    pub trouble: String,
    pub said: String,
    /// `#steps`: the last run's steps, or an export's notice.
    pub steps: Steps,
    /// The press to act on after this build.
    pub want: Option<Want>,
    /// A copy of the learned pace the shell reads for the detail tabs' waits; the application owns the file.
    pub pace: Pace,
    /// The history of the figures, every seed: the shell follows the runs of the map on screen (history::here).
    pub history: Vec<Entry>,
    /// The figures of the run before the one on screen (the page's preview.before).
    pub before: Option<Figures>,
    /// What the list's filter holds.
    pub filter: String,
    /// The tool on screen. The preview's state, its run and its timers go on while Home is shown: the launcher
    /// only hides the preview's frame.
    pub mode: Mode,
    pub maximized: bool,
    /// The preview was on screen since its last run ended (the launcher's `previewSeen`).
    pub preview_seen: bool,
    pub ended: RunEnd,
    /// The bar's `#account` (launcher line 84): who plays, said once beside the game's name. None while unknown, and
    /// in the standalone preview, which has no account.
    pub account: Option<String>,
    /// The tools' options sliding open and shut (launcher line 124), 1 being open. Kept in step with `mode`: a
    /// state that names a mode without it is put right when the bar is built.
    pub options: Slide,
    /// The width the options take when open, as the last frame measured it: what the slide scales.
    pub options_w: f32,
    /// The system asks for no animation (read once at start): the slide then jumps.
    pub reduce_motion: bool,
    /// The setting whose help is open over its row, as an index in `SETTINGS`. It stays open until it is closed.
    pub help: Option<usize>,
    /// The settings list showed its scrollbar last frame: the filter above it keeps the same gap on the right.
    pub settings_bar: bool,
}

impl PreviewState {
    /// The page after `loadWorld`: the form holds the settings of the run that made the map.
    pub fn for_world(world: Option<&World>) -> PreviewState {
        let mut s = PreviewState::default();
        if let Some(ask) = world.and_then(run::ask_of) {
            run::adopt(&mut s, &ask);
            s.made = Some(ask);
        }
        s
    }
}

impl Default for PreviewState {
    /// The page after loading a preview in the relief view, in a fresh browser (the seed is `preview.json`'s).
    fn default() -> PreviewState {
        PreviewState {
            panel_open: [true, true],
            panel_w: [None, None],
            grip: None,
            panel_y: [0.0, 0.0],
            view: 2,
            in3d: false,
            level: Level::default(),
            seed: "94427382".to_string(),
            world_open: true,
            settings_open: true,
            figures_open: true,
            history_open: false,
            values: pack_values(),
            baseline: pack_values(),
            pack_known: false,
            apply: Default::default(),
            detail: 2,
            settings_y: 0.0,
            view_only: false,
            settings_list: None,
            list_y: 0.0,
            look: Look::REST,
            carry: None,
            chosen: Default::default(),
            orbit: None,
            turn_from: None,
            cut_at: 0,
            cut_look: CutLook::FRESH,
            cut_carry: None,
            nav_carry: None,
            chosen_ores: BTreeSet::new(),
            samples: 256,
            made: None,
            follow: None,
            trouble: String::new(),
            said: String::new(),
            steps: Steps::Empty,
            want: None,
            pace: Pace::default(),
            history: Vec::new(),
            before: None,
            filter: String::new(),
            mode: Mode::Preview,
            maximized: false,
            preview_seen: true,
            ended: RunEnd::Idle,
            account: None,
            options: Slide::settled(true),
            options_w: 0.0,
            reduce_motion: false,
            help: None,
            settings_bar: false,
        }
    }
}

/// Where the shell's parts are, for tests and for the steps that draw into them. Filled while building.
#[derive(Clone, Debug)]
pub struct ShellIds {
    pub page: Id,
    pub left: Id,
    pub middle: Id,
    pub right: Id,
    /// The bar of the window, its brand, the two tools' buttons (Home, preview) and the window's three buttons.
    pub bar: Id,
    pub brand: Id,
    /// The bar's two buttons that open and close the settings and the figures panel (Id::ROOT in Home).
    pub toggles: [Id; 2],
    /// The grips between each panel and the stage (Id::ROOT when stacked or the panel is closed).
    pub grips: [Id; 2],
    /// How the page was laid out: the panels' widths and whether they are stacked.
    pub plan: Plan,
    /// The scroll area of each panel as one whole (stacked only), and every scroll area built in the page.
    pub panel_scrolls: [Id; 2],
    pub scroll_areas: Vec<Id>,
    pub tools: [Id; 2],
    pub ends: [Id; 3],
    /// Every button of the bar (the tools', the tabs' and the window's): what a press on the bar does not drag.
    pub bar_buttons: Vec<Id>,
    /// The pane under the bar while Home is on.
    pub home_pane: Id,
    /// The pane's content box under Home, inset like the preview's page.
    pub home_body: Id,
    /// The tools' strip in the middle of the bar, and the preview's tabs in it.
    pub strip: Id,
    /// The box that slides the preview's tabs open (Id::ROOT while they are not built).
    pub options: Id,
    pub tabs: Vec<Id>,
    /// The strip's "More" tab (Id::ROOT unless the bar lacks room for every view) and the form the bar took at this
    /// width: what it shows and what "More" holds.
    pub more: Id,
    pub form: crate::overflow::Form,
    pub world_head: Id,
    pub settings_head: Id,
    pub figures_head: Id,
    pub history_head: Id,
    pub seed: Id,
    pub select: Id,
    /// `#areaHint`, built only past the widest fine area.
    pub area_hint: Id,
    /// The two choices beside Apply that put every value back, then each of them: the pack's, Tectonic's own.
    pub profiles: Id,
    pub back_pack: Id,
    pub original: Id,
    /// The arrow in each group's title that puts the group back to the pack's (Id::ROOT while the group is the pack's),
    /// and the pack's value beside each field that differs from it (Id::ROOT beside the others), in `SETTINGS` order.
    pub group_resets: [Id; 5],
    pub pack_marks: Vec<Id>,
    pub settings_scroll: Id,
    pub group_heads: Vec<Id>,
    /// In `SETTINGS` order.
    pub rows: Vec<Id>,
    /// The "?" after each setting's label, in `SETTINGS` order.
    pub help_marks: Vec<Id>,
    /// The open help under its row, its title, its text and its close cross (Id::ROOT while none is open).
    pub help: Id,
    pub help_title: Id,
    pub help_text: Id,
    pub help_close: Id,
    /// The help's last line, a setting's values and range (Id::ROOT for the filter's help).
    pub help_facts: Id,
    /// The filter's row, its label, its count of settings on show (Id::ROOT while the filter is off) and its "?".
    pub view_only: Id,
    pub filter_label: Id,
    pub filter_count: Id,
    pub filter_mark: Id,
    pub reset: Id,
    /// The Apply button and the words under it (Id::ROOT when not built), and what they say.
    pub apply: Id,
    pub apply_note: Id,
    pub apply_text: String,
    pub run: Id,
    pub status: Id,
    pub details: Id,
    pub generate: Id,
    pub stage: Id,
    pub frame: Id,
    /// The map itself (a canvas), when the frame holds one.
    pub map: Id,
    /// The 3D view's square (a canvas), when the frame holds one; the app draws the scene into its rect.
    pub relief: Id,
    /// The 3D view's levels, under it beside Export; `ROOT` outside the 3D view.
    pub levels: [Id; 3],
    pub under: Id,
    pub probe: Id,
    /// What the probe line says, whole (the line itself is cut with an ellipsis when it does not fit).
    pub probe_text: String,
    /// What the note over the frame says (empty when there is none).
    pub note_text: String,
    pub export: Id,
    pub figures_grid: Id,
    /// Every cell's text of the figures grid, in order (the rule's and the blanks' are empty).
    pub figures_text: Vec<String>,
    pub goal_values: Vec<Id>,
    /// #history's column (the hover area), the plot slot of each line, and what the probe line says.
    pub history_body: Id,
    pub trends: Vec<Id>,
    pub history_probe: String,
    pub list_block: Id,
    /// The list's title as shown: "HEIGHTS", or "ORES" in the section.
    pub list_title: String,
    pub pick: Id,
    /// What #pick says, whole: the hint, or the names, the share and the word that clears.
    pub pick_text: String,
    /// The chosen rows' names (buttons) and the one that clears the choice.
    pub pick_names: Vec<Id>,
    pub pick_none: Id,
    /// The text field that filters the rows.
    pub filter: Id,
    pub list_scroll: Id,
    /// The rows shown, in order, and the list's own id of each (a biome's, a structure's, a band's number).
    pub list_rows: Vec<Id>,
    pub list_keys: Vec<String>,
    /// The part heads shown, with their name and their count, and the rows of the absent biomes.
    pub list_parts: Vec<Id>,
    pub list_part_words: Vec<(String, String)>,
    pub absent_rows: Vec<Id>,
    /// The cut of the section (a column inside the frame), its strip, the strip's note and the cut at true scale
    /// (canvases): the ids are Id::ROOT unless a cut is shown.
    pub cut: Id,
    pub nav: Id,
    pub nav_note: Id,
    pub cut_view: Id,
    /// The cut view's border box as the look arithmetic takes it (before layout): width and height.
    pub cut_box: (f64, f64),
    /// "Another cut", when the section view has a section.
    pub next_cut: Id,
    /// The ore rows of the section's list, in `ORE_KINDS` order.
    pub ores: Vec<Id>,
    /// The square beside the generate button, while a run is under way (Id::ROOT otherwise).
    pub stop: Id,
    /// `#steps`, its hairline, and the export notice with its two links (Id::ROOT unless shown).
    pub steps: Id,
    pub steps_bar: Id,
    pub saved: Id,
    pub open_folder: Id,
    pub close_notice: Id,
    /// What the status line says, whole ("" when it is not built), and the word on the generate button.
    pub status_text: String,
    pub generate_text: String,
    /// The steps as the line shows them: "✓ label 12 s" for a done one, "› label" for the one under way.
    pub steps_items: Vec<String>,
}

impl Default for ShellIds {
    fn default() -> ShellIds {
        let o = Id::ROOT;
        ShellIds {
            page: o,
            left: o,
            middle: o,
            right: o,
            bar: o,
            brand: o,
            toggles: [o; 2],
            grips: [o; 2],
            plan: Plan { stacked: false, widths: [0.0; 2] },
            panel_scrolls: [o; 2],
            scroll_areas: Vec::new(),
            tools: [o; 2],
            ends: [o; 3],
            bar_buttons: Vec::new(),
            home_pane: o,
            home_body: o,
            strip: o,
            options: o,
            tabs: Vec::new(),
            more: o,
            form: crate::overflow::Form::default(),
            world_head: o,
            settings_head: o,
            figures_head: o,
            history_head: o,
            seed: o,
            select: o,
            area_hint: o,
            profiles: o,
            back_pack: o,
            original: o,
            group_resets: [o; 5],
            pack_marks: vec![o; SETTINGS.len()],
            settings_scroll: o,
            group_heads: Vec::new(),
            rows: vec![o; SETTINGS.len()],
            help_marks: vec![o; SETTINGS.len()],
            help: o,
            help_title: o,
            help_text: o,
            help_close: o,
            help_facts: o,
            view_only: o,
            filter_label: o,
            filter_count: o,
            filter_mark: o,
            reset: o,
            apply: o,
            apply_note: o,
            apply_text: String::new(),
            run: o,
            status: o,
            details: o,
            generate: o,
            stage: o,
            frame: o,
            map: o,
            relief: o,
            levels: [o; 3],
            under: o,
            probe: o,
            probe_text: String::new(),
            note_text: String::new(),
            export: o,
            figures_grid: o,
            figures_text: Vec::new(),
            goal_values: Vec::new(),
            history_body: o,
            trends: Vec::new(),
            history_probe: String::new(),
            list_block: o,
            list_title: String::new(),
            pick: o,
            pick_text: String::new(),
            pick_names: Vec::new(),
            pick_none: o,
            filter: o,
            list_scroll: o,
            list_rows: Vec::new(),
            list_keys: Vec::new(),
            list_parts: Vec::new(),
            list_part_words: Vec::new(),
            absent_rows: Vec::new(),
            cut: o,
            nav: o,
            nav_note: o,
            cut_view: o,
            cut_box: (0.0, 0.0),
            next_cut: o,
            ores: Vec::new(),
            stop: o,
            steps: o,
            steps_bar: o,
            saved: o,
            open_folder: o,
            close_notice: o,
            status_text: String::new(),
            generate_text: String::new(),
            steps_items: Vec::new(),
        }
    }
}

/// `.column` (line 106): a flex column that may shrink below its content.
fn column() -> Style {
    Style { column: true, min_h: Dim::Px(0.0), ..Style::default() }
}

/// `flex: 1; min-height: 0`.
fn fills() -> Style {
    Style { grow: 1.0, shrink: 1.0, basis: Dim::Px(0.0), min_h: Dim::Px(0.0), ..Style::default() }
}

/// A horizontal rule of one pixel: the `border-top` or `border-bottom` of a block.
fn rule() -> Style {
    Style { h: Dim::Px(1.0), shrink: 0.0, ..Style::default() }
}

/// `.row` (line 204): a label column of 9em and the control.
fn world_row() -> Style {
    Style {
        display: Display::Grid,
        cols: Tracks::new(&[Track::Px(108.0), Track::MinMaxFr0(1.0)]),
        gap: Vec2::new(10.0, 0.0),
        align_items: Align::Center,
        ..Style::default()
    }
}

/// `.setting` (line 164): the label, the empty output, and the field in 8ch.
fn setting_row() -> Style {
    Style {
        display: Display::Grid,
        cols: Tracks::new(&[Track::MinMaxFr0(1.0), Track::Auto, Track::Px(panels::setting_field())]),
        gap: Vec2::new(panels::SETTING_GAP, 0.0),
        align_items: Align::Center,
        pad: Edges::vh(2.0, 0.0),
        ..Style::default()
    }
}

fn rgb(c: [u8; 3]) -> Color {
    Color::hex(((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32)
}

/// Builds the shell into the frame the caller began (`begin_frame(size)` before, `end_frame` after). `size` is
/// the window's: the map's side is arithmetic from it, because the square needs the column's height before
/// layout. `world` is the preview on the map, if there is one.
pub fn build(ui: &mut Ui, s: &mut PreviewState, size: Vec2, world: Option<&World>, shown: Option<&Shown>, home: Option<&mut dyn Home>) -> ShellIds {
    build_run(ui, s, size, world, world, shown, home)
}

/// `build` while a run draws on the stage: `stage` is the run's picture (its sketch, then its map), which the frame
/// shows, while the bar and the figures stay those of `world`, the map before (lines 2618-2619).
pub fn build_run(ui: &mut Ui, s: &mut PreviewState, size: Vec2, world: Option<&World>, stage: Option<&World>, shown: Option<&Shown>, home: Option<&mut dyn Home>) -> ShellIds {
    let t = *ui.theme();
    let cols = PAGE;
    let mut ids = ShellIds::default();
    // A map stopped at a coarser detail cannot give these views (line 2509): the page goes back to the biomes.
    if world.is_some_and(World::coarse) && matches!(s.view, 1 | 4 | 5) {
        s.view = 0;
    }

    // The panels' widths come first: the stage is what they leave. A hand's drag is applied before, so that the
    // frame it happens in already shows it.
    let open = s.panel_open;
    let plan = panels::plan(size.x, open, s.panel_w);
    if plan.stacked || s.mode != Mode::Preview {
        s.grip = None;
    }
    drag_panel(ui, s, size.x);
    let plan = panels::plan(size.x, open, s.panel_w);
    ids.plan = plan;
    let stacked = plan.stacked;
    let (middle_w, column_h) = if stacked {
        // The stage takes the half of the height under the bar, and its content starts under the bar's gap.
        let gap = panels::GAP_STACKED;
        (size.x - 2.0 * gap, ((size.y - BAR_H) / 2.0).floor() - cols.pad.top)
    } else {
        let beside = open.iter().filter(|o| **o).count();
        (size.x - panels::gaps(beside) as f32 * panels::GAP - plan.widths[0] - plan.widths[1], size.y - BAR_H - cols.pad.top - cols.pad.bottom)
    };
    // Line 227: `side = min(the column's width, its height - --under)`.
    let side = middle_w.min(column_h - UNDER).max(0.0);
    // The section takes the square's place (line 283): the column's whole width, and its height less `--under`.
    let frame_size = if s.view == 5 { (middle_w.max(0.0), (column_h - UNDER).max(0.0)) } else { (side, side) };

    // body (lines 63-72): a column filling the window.
    ui.node("body", Style { column: true, ..fills() }, |ui| {
        crate::bar::build(ui, s, &mut ids, world, size.x);
        // Home: the page's columns are not built. The launcher's home is step 21, so its pane says so.
        if s.mode == Mode::Home {
            // A host that gives a Home fills the whole pane (it pads inside its own scroll area).
            if let Some(home) = home {
                ids.home_pane = ui.node("home", fills(), |ui| home.build(ui, Vec2::new(size.x, size.y - BAR_H)));
                return;
            }
            let pane = Style { pad: cols.pad, ..fills() };
            let body = Style { align_items: Align::Center, justify: Justify::Center, ..fills() };
            let mut home_body = ids.home_body;
            ids.home_pane = ui.node("home", pane, |ui| {
                home_body = ui.node("homeBody", body, |ui| ui.text(text::HOME_LATER, TextLook::body(Tone::Muted)));
            });
            ids.home_body = home_body;
            return;
        }
        let panel = |w: f32| Style { w: Dim::Px(w), shrink: 0.0, ..column() };
        if !stacked {
            // Beside: the settings panel, the stage and the figures panel in a row. A closed panel is not built and
            // takes no room; the stage grows into it. Each open panel has a grip in the gap between it and the stage.
            let page = Style { pad: cols.pad, w: Dim::Pct(100.0), ..fills() };
            let stage_style = Style { grow: 1.0, shrink: 1.0, basis: Dim::Px(0.0), min_w: Dim::Px(0.0), ..column() };
            ids.page = ui.node("page", page, |ui| {
                if open[0] {
                    ids.left = ui.node("left", panel(plan.widths[0]), |ui| left_column(ui, s, &mut ids, &t, false));
                    grip_gap(ui, s, &mut ids, &t, 0);
                }
                ids.middle = ui.node("middle", stage_style, |ui| middle_column(ui, s, &mut ids, &t, frame_size, stage, shown));
                if open[1] {
                    grip_gap(ui, s, &mut ids, &t, 1);
                    ids.right = ui.node("right", panel(plan.widths[1]), |ui| right_column(ui, s, &mut ids, &t, &cols, world, shown, false));
                }
            });
        } else {
            // Stacked: the stage on top, the open panels under it in equal columns, no line between any of them.
            let gap = panels::GAP_STACKED;
            let page = Style { column: true, pad: Edges::new(cols.pad.top, gap, cols.pad.bottom, gap), gap: Vec2::new(0.0, gap), w: Dim::Pct(100.0), ..fills() };
            let stage_style = Style { h: Dim::Px(column_h), shrink: 0.0, ..column() };
            ids.page = ui.node("page", page, |ui| {
                ids.middle = ui.node("middle", stage_style, |ui| middle_column(ui, s, &mut ids, &t, frame_size, stage, shown));
                ui.node("panels", Style { gap: Vec2::new(gap, 0.0), ..fills() }, |ui| {
                    if open[0] {
                        ids.left = ui.node("left", panel(plan.widths[0]), |ui| {
                            let mut y = s.panel_y[0];
                            let id = panel_scroll(ui, &mut y, |ui| left_column(ui, s, &mut ids, &t, true));
                            s.panel_y[0] = y;
                            ids.panel_scrolls[0] = id;
                            ids.scroll_areas.push(id);
                        });
                    }
                    if open[1] {
                        ids.right = ui.node("right", panel(plan.widths[1]), |ui| {
                            let mut y = s.panel_y[1];
                            let id = panel_scroll(ui, &mut y, |ui| right_column(ui, s, &mut ids, &t, &cols, world, shown, true));
                            s.panel_y[1] = y;
                            ids.panel_scrolls[1] = id;
                            ids.scroll_areas.push(id);
                        });
                    }
                });
            });
        }
    });
    ids
}

/// A panel as one whole that scrolls, when the panels are stacked: the kit's scrollbar in a gutter of its own at the
/// panel's right edge, no fade, and nothing inside it scrolls by itself.
fn panel_scroll(ui: &mut Ui, y: &mut f32, body: impl FnOnce(&mut Ui)) -> Id {
    let style = Style { margin: Margin::default(), pad: Edges::new(0.0, 0.0, 12.0, 0.0), ..scroll_style() };
    ui.scroll("panel", y, style, ScrollOpts { fade: 0.0 }, body).id
}

/// Moves a panel with the grip being dragged: its width is where the press began plus the pointer's travel, between
/// the panel's minimum and what leaves the stage its own. The press is read until it is over, not only while held,
/// so the last movement before the release is not lost.
fn drag_panel(ui: &Ui, s: &mut PreviewState, width: f32) {
    let (Some(g), Some(p)) = (s.grip, ui.pointer()) else { return };
    if p.x == g.from {
        return;
    }
    let travel = if g.side == 0 { p.x - g.from } else { g.from - p.x };
    s.panel_w[g.side] = Some(panels::clamp_drag(width, s.panel_open, g.other, g.start + travel));
}

/// The gap between a panel and the stage, with the grip in its middle: `GRIP_W` wide and as tall as the panel. Nothing
/// is drawn at rest; the pointer over it draws a line of 1 px in the muted colour, and a press held draws it in the
/// text colour. A double click puts the panel back to its natural width.
fn grip_gap(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, t: &Theme, side: usize) {
    let plan = ids.plan;
    let gap = Style { w: Dim::Px(panels::GAP), shrink: 0.0, justify: Justify::Center, ..Style::default() };
    let (muted, text_colour) = (t.muted, t.text);
    ui.node(&format!("gap{side}"), gap, |ui| {
        let grip = Style { w: Dim::Px(panels::GRIP_W), shrink: 0.0, ..Style::default() };
        let resp = ui.canvas(&format!("grip{side}"), grip, |ui, resp| {
            if resp.pressed {
                if ui.clicks() >= 2 {
                    s.panel_w[side] = None;
                    s.grip = None;
                } else if let Some(p) = ui.pointer() {
                    s.grip = Some(Grip { side, from: p.x, start: plan.widths[side], other: plan.widths[1 - side] });
                }
            }
            if !resp.held && s.grip.is_some_and(|g| g.side == side) {
                s.grip = None;
            }
            if resp.hovered || resp.held {
                ui.want_cursor(Cursor::ResizeEW);
            }
            let colour = if resp.held { Some(text_colour) } else if resp.hovered { Some(muted) } else { None };
            match (colour, resp.rect) {
                (Some(color), Some(r)) => vec![Cmd::Fill { rect: Rect::new((r.w / 2.0).floor(), 0.0, 1.0, r.h), color }],
                _ => Vec::new(),
            }
        });
        ui.title(resp.id, text::RESIZE_PANEL);
        ids.grips[side] = resp.id;
    });
}

/// The left column (lines 370-400): the world, the settings, and the run block under them. `stacked`: the whole
/// column is one scroll area (the caller's), so the settings list shows its rows in it instead of scrolling by itself.
fn left_column(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, t: &Theme, stacked: bool) {
    let muted = TextLook::body(Tone::Muted);

    let r = ui.fold_with("world", text::WORLD, &mut s.world_open, mere_ui::FoldOpts { bare: stacked, ..Default::default() }, |ui| {
        ui.node("seedRow", world_row(), |ui| {
            ui.text(text::SEED, muted);
            let opts = FieldOpts { look: FieldLook::Text, placeholder: text::SEED_RANDOM, max_chars: 64, width: Dim::Pct(100.0), focus: false, ..FieldOpts::default() };
            ids.seed = ui.text_field("seed", &mut s.seed, opts).resp.id;
        });
        ui.node("areaRow", world_row(), |ui| {
            ui.text(text::SPAN, muted);
            // The page's sixteen widths (lines 806-809): 256 to 4096 points of 16 blocks, by 256.
            let labels: Vec<String> = AREA_POINTS.iter().map(|&p| text::span_value(p as f64 * 16.0)).collect();
            let mut at = AREA_POINTS.iter().position(|&p| p == s.samples).unwrap_or(0);
            let r = ui.select_menu("area", &labels, &mut at, SelectOpts { height: Some(mere_ui::tokens::CONTROL_COMPACT), ..SelectOpts::default() });
            ids.select = r.resp.id;
            if r.changed {
                s.samples = AREA_POINTS.get(at).copied().unwrap_or(s.samples);
                // Past 32,768 blocks the sixteen-block map is out of reach: the next coarser one (line 812).
                if s.samples > AREA_FINE_MAX && DETAIL_BLOCKS[s.detail] == 16 {
                    s.detail = DETAIL_BLOCKS.iter().position(|&d| d == 32).unwrap_or(s.detail);
                }
            }
        });
        // `#areaHint` (lines 382, 814), shown only past the widest fine area.
        if s.samples > AREA_FINE_MAX {
            ids.area_hint = ui.node("areaHint", Style::default(), |ui| ui.paragraph(text::SPAN_COARSE, TextLook::small(Tone::Muted)));
        }
    });
    ids.world_head = r.id;

    let r = ui.fold_with("settings", text::PROFILES, &mut s.settings_open, mere_ui::FoldOpts { grow: !stacked, ..Default::default() }, |ui| {
        let mut help = Help { open: &mut s.help };
        // Stacked, the list has no bar of its own: the column's scroll serves the filter and the list alike.
        let gutter = if s.settings_bar && !stacked { mere_ui::GUTTER } else { 0.0 };
        view_only_row(ui, (&mut s.view_only, s.view), gutter, &mut help, ids);
        s.settings_bar = settings_scroll(ui, (s.view_only, s.view), &mut s.settings_list, &mut s.settings_y, (&mut s.values, &s.baseline), &mut help, ids, t, stacked);
        crate::apply_ui::apply_block(ui, crate::apply_ui::Apply { values: &mut s.values, baseline: &s.baseline, pack_known: s.pack_known, notice: &s.apply, want: &mut s.want }, ids);
    });
    ids.settings_head = r.id;

    run_block(ui, s, ids, t);
}

/// The filter's help in `PreviewState::help`, past the settings' indices.
pub const FILTER_HELP: usize = SETTINGS.len();

/// What a setting's label has beside its "?": the row (the panel less the scrollbar's gutter), less the gaps, the
/// field and the mark. Read from the plan, as the layout does not give it before it is laid out.
fn label_room(ids: &ShellIds) -> f32 {
    ids.plan.widths[0] - mere_ui::GUTTER - 2.0 * panels::SETTING_GAP - panels::setting_field() - GAP_TIGHT - mere_ui::HELP_MARK_W
}

/// The filter, on the grid of the settings' rows so that its toggle sits above their fields: a label that names the
/// pressed map with its "?" as the settings have, while on how many settings are left on show, then the toggle.
/// `filter` is the toggle's value and the pressed view; `gutter` the gap the list's scrollbar keeps on the right.
fn view_only_row(ui: &mut Ui, filter: (&mut bool, usize), gutter: f32, help: &mut Help, ids: &mut ShellIds) {
    let (only, view) = filter;
    let body = TextLook::body(Tone::Text);
    let muted = TextLook::body(Tone::Muted);
    let label = text::view_only(view);
    let count = only.then(|| {
        let shown = view_filter::visible(true, view).iter().map(Vec::len).sum();
        text::shown_of(shown, SETTINGS.len())
    });
    // The count takes its own column, so the label has that much less room.
    let room = label_room(ids) - count.as_ref().map_or(0.0, |c| muted.width(c) + panels::SETTING_GAP);
    // The rows below sit in a scroll that keeps a gutter for its bar on the right while it shows; the same gap here
    // lines the toggle up with their fields.
    let mut row = setting_row();
    row.pad.right = gutter;
    let open = *help.open == Some(FILTER_HELP);
    ui.node("view_only_place", mere_ui::help_anchor(), |ui| {
    let row_id = ui.hover_area("view_only", row, |ui, over| {
        let name = Style { position: Position::Relative, min_w: Dim::Px(0.0), ..Style::default() };
        let (place, mark) = mere_ui::help_label(&label, room);
        ui.node("name", name, |ui| {
            ids.filter_label = ui.node("label", place, |ui| ui.paragraph(&label, body));
            let r = ui.help_mark(text::HELP_MARK, mark, over.is_some(), open);
            ids.filter_mark = r.id;
            if r.clicked {
                *help.open = if open { None } else { Some(FILTER_HELP) };
            }
        });
        // Off, the column keeps an empty text, as a setting's row does.
        let r = ui.node("count", Style::default(), |ui| ui.text(count.as_deref().unwrap_or(""), muted));
        ids.filter_count = if count.is_some() { r } else { Id::ROOT };
        ui.toggle("field", only, text::ON, text::OFF);
    });
    ids.view_only = row_id;
    if *help.open == Some(FILTER_HELP) {
        help_panel(ui, ids, row_id, ("view_only", text::VIEW_ONLY_TITLE, text::VIEW_ONLY_HINT, ""), help.open);
    }
    });
}

/// A value as its field shows it: a number as the number field writes it, a switch as on or off.
fn value_words(kind: Kind, v: f64) -> String {
    match kind {
        Kind::Number(_) => crate::settings::shown(v),
        Kind::Switch => if v != 0.0 { text::ON } else { text::OFF }.to_string(),
    }
}

/// Two values of a setting are the same one (the float noise of a field's arithmetic apart).
fn same_value(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The last line of a setting's help: Tectonic's own value, the pack's, and the range of a number.
fn setting_facts(i: usize, baseline: &[f64; 43]) -> String {
    let set = &SETTINGS[i];
    let (original, pack) = (value_words(set.kind, set.original), value_words(set.kind, baseline[i]));
    match set.kind {
        Kind::Number(spec) => text::help_facts(&original, &pack, Some((&crate::settings::shown(spec.min), &crate::settings::shown(spec.max)))),
        Kind::Switch => text::help_facts(&original, &pack, None),
    }
}

/// `#settings` (lines 157-164): five groups, each a title over its rows, then the reset hint.
/// cut is the filter and the pressed view; list what the list was cut to last frame. True when the list shows its
/// scrollbar.
#[allow(clippy::too_many_arguments)]
fn settings_scroll(ui: &mut Ui, cut: (bool, usize), list: &mut Option<usize>, y: &mut f32, held: (&mut [f64; 43], &[f64; 43]), help: &mut Help, ids: &mut ShellIds, t: &Theme, stacked: bool) -> bool {
    let (values, baseline) = held;
    // The list is cut to the pressed view when the filter is on; a new cut starts at the top, where the first group is.
    let key = view_filter::list_key(cut.0, cut.1);
    if *list != key {
        *list = key;
        *y = 0.0;
    }
    let groups = view_filter::visible(cut.0, cut.1);
    let body = TextLook::body(Tone::Text);
    let label_room = label_room(ids);
    let mut rows = |ui: &mut Ui, ids: &mut ShellIds| {
        // The first group on show gets the short top pad, not the one that separates two groups.
        let first = groups.iter().position(|m| !m.is_empty());
        for (g, members) in groups.iter().enumerate().filter(|(_, m)| !m.is_empty()) {
            ui.node(&format!("g{g}"), Style { column: true, shrink: 0.0, ..Style::default() }, |ui| {
                // The group title (lines 105, 157): 4 px above (14 px between groups), 4 px under, then the
                // hairline of `--line`.
                let head = Style { column: true, ..Style::default() };
                // The whole group, shown or filtered out: the arrow puts back what the title names.
                let whole: Vec<usize> = (0..SETTINGS.len()).filter(|&i| SETTINGS[i].group == g).collect();
                let moved = whole.iter().any(|&i| !same_value(values[i], baseline[i]));
                ids.group_resets[g] = Id::ROOT;
                let head_id = ui.node("head", head, |ui| {
                    let row = Style {
                        // Centred: the arrow is as tall as the title's line, so it adds no height.
                        align_items: Align::Center,
                        gap: Vec2::new(10.0, 0.0),
                        pad: Edges::new(if Some(g) == first { 4.0 } else { 14.0 }, 0.0, 4.0, 0.0),
                        ..Style::default()
                    };
                    ui.node("title", row, |ui| {
                        ui.text(&text::GROUPS[g].to_uppercase(), TextLook::label());
                        if moved {
                            let r = ui.small_icon("reset", back_icon(), TextLook::label().line, Margin { left: Side::Auto, ..Margin::default() });
                            ui.title(r.id, text::GROUP_RESET_HINT);
                            ids.group_resets[g] = r.id;
                            if r.clicked {
                                for &i in &whole {
                                    values[i] = baseline[i];
                                }
                            }
                        }
                    });
                    ui.fill(rule(), t.line);
                });
                ids.group_heads.push(head_id);
                ui.node("body", Style { column: true, pad: Edges::new(4.0, 0.0, 0.0, 0.0), ..Style::default() }, |ui| {
                    for &i in members {
                        let set = &SETTINGS[i];
                        // The pack's value, said beside a field that differs from it; the label has that much less room.
                        let muted = TextLook::body(Tone::Muted);
                        let pack = (!same_value(values[i], baseline[i])).then(|| value_words(set.kind, baseline[i]));
                        let label_room = label_room - pack.as_ref().map_or(0.0, |p| muted.width(p));
                        ui.node(&format!("{}Place", set.key), mere_ui::help_anchor(), |ui| {
                        ids.rows[i] = ui.hover_area(set.key, setting_row(), |ui, over| {
                            // The label, then the mark that opens its help, shown while the pointer is on the row.
                            // A label wider than its cell wraps onto more lines instead of being cut, and the mark
                            // follows its last word.
                            let name = Style { position: Position::Relative, min_w: Dim::Px(0.0), ..Style::default() };
                            let (label, mark) = mere_ui::help_label(set.label, label_room);
                            ui.node("name", name, |ui| {
                                ui.node("label", label, |ui| ui.paragraph(set.label, body));
                                let r = ui.help_mark(text::HELP_MARK, mark, over.is_some(), *help.open == Some(i));
                                ids.help_marks[i] = r.id;
                                if r.clicked {
                                    // The same mark again closes it; another one moves it to its own setting.
                                    *help.open = if *help.open == Some(i) { None } else { Some(i) };
                                }
                            });
                            // The output: empty until the value differs from the pack's, then the pack's.
                            let out = ui.node("pack", Style::default(), |ui| ui.text(pack.as_deref().unwrap_or(""), muted));
                            ids.pack_marks[i] = if pack.is_some() { out } else { Id::ROOT };
                            if pack.is_some() {
                                ui.title(out, text::PACK_VALUE_HINT);
                            }
                            match set.kind {
                                Kind::Number(spec) => {
                                    ui.number("field", &mut values[i], spec);
                                }
                                Kind::Switch => {
                                    let mut on = values[i] != 0.0;
                                    if ui.toggle("field", &mut on, text::ON, text::OFF).clicked {
                                        values[i] = if on { 1.0 } else { 0.0 };
                                    }
                                }
                            }
                        });
                        if *help.open == Some(i) {
                            help_panel(ui, ids, ids.rows[i], (set.key, set.label, text::setting_hint(set.key), &setting_facts(i, baseline)), help.open);
                        }
                        });
                    }
                });
            });
        }
        // Line 161.
        ids.reset = ui.node("reset", Style { column: true, shrink: 0.0, pad: Edges::new(12.0, 0.0, 0.0, 0.0), ..Style::default() }, |ui| {
            ui.paragraph(text::PACK_NOTE, TextLook::small(Tone::Muted));
        });
    };
    if stacked {
        // The column's one scroll holds the rows: a plain box here, so that no scroll area is inside another.
        ids.settings_scroll = ui.node("settings", Style { column: true, shrink: 0.0, ..Style::default() }, |ui| rows(ui, ids));
        false
    } else {
        let r = ui.scroll("settings", y, scroll_style(), ScrollOpts::LIST, |ui| rows(ui, ids));
        ids.settings_scroll = r.id;
        ids.scroll_areas.push(r.id);
        r.overflow
    }
}

/// The setting whose help is open.
struct Help<'a> {
    open: &'a mut Option<usize>,
}

/// The arrow that puts a group back: a stroke that comes round and points left, in the kit's small icon square.
const BACK: &str = "M4 2h1v1h-1zM3 3h1v1h-1zM2 4h7v1h-7zM3 5h1v1h-1zM4 6h1v1h-1zM9 5h1v3h-1zM4 8h5v1h-5z";

fn back_icon() -> &'static Icon {
    static ICON: std::sync::OnceLock<Icon> = std::sync::OnceLock::new();
    ICON.get_or_init(|| {
        Icon::parse(BACK, mere_ui::SMALL_ICON).unwrap_or_else(|e| {
            mere_base::observe!(Error, "the group's arrow: {e}");
            Icon { grid: mere_ui::SMALL_ICON, figures: Vec::new() }
        })
    })
}

/// Opens the help of a row over it and keeps the ids the tests read; the cross closes it.
fn help_panel(ui: &mut Ui, ids: &mut ShellIds, row: Id, about: (&str, &str, &str, &str), open: &mut Option<usize>) {
    let (key, title, hint, facts) = about;
    let r = ui.help_panel(&format!("{key}Help"), row, HelpText { title, hint, facts });
    ids.help = r.panel;
    ids.help_title = r.title;
    ids.help_close = r.close;
    ids.help_text = r.hint;
    ids.help_facts = r.facts.unwrap_or(Id::ROOT);
    if r.closed {
        *open = None;
    }
}

/// Escape closes the open help, after an open menu and a focused field have had theirs (they take it first). True
/// when it did, and the event goes no further.
pub fn help_escape(s: &mut PreviewState, ui: &Ui, e: &mere_base::Event) -> bool {
    let escape = matches!(e, mere_base::Event::Key { key: mere_base::Key::Escape, .. });
    if escape && s.help.is_some() && !ui.menu_open() && ui.focused().is_none() {
        s.help = None;
        return true;
    }
    false
}

/// What `#status` says (`showButton`, lines 799-802, and `tick`, 2610-2615): the clock while a run is followed, else
/// the trouble, what was said, or that the map already shows these settings.
fn status_words(s: &PreviewState) -> String {
    if let Some(f) = &s.follow {
        let elapsed = duration(f.seconds);
        let clock = match f.expected {
            Some(e) => text::running(&elapsed, &duration(e)),
            None => text::measuring(&elapsed),
        };
        return if matches!(f.drawn, Drawn::Sketch(_)) { format!("{clock} {}", text::SKETCH_READABLE) } else { clock };
    }
    if !s.trouble.is_empty() {
        return s.trouble.clone();
    }
    if !s.said.is_empty() {
        return s.said.clone();
    }
    if made_shows_form(s) { text::UP_TO_DATE.to_string() } else { String::new() }
}

/// `sameAs(shown)`: the map on screen was made from what the form holds.
fn made_shows_form(s: &PreviewState) -> bool {
    s.made.as_ref().is_some_and(|m| m.same_as(&s.seed, s.samples, DETAIL_BLOCKS[s.detail], &s.values))
}

/// `#run` (line 192): a hairline, the status, the details and the generate button with the stop square beside it.
fn run_block(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, t: &Theme) {
    let style = Style { column: true, margin: Margin { top: Side::Auto, ..Margin::default() }, shrink: 0.0, ..Style::default() };
    ids.run = ui.node("run", style, |ui| {
        ui.fill(rule(), t.edge);
        let inner = Style { column: true, pad: Edges::new(12.0, 0.0, 0.0, 0.0), gap: Vec2::new(0.0, 8.0), ..Style::default() };
        ui.node("runBody", inner, |ui| {
            // `#status:empty { display: none }`: nothing to say, no box.
            ids.status_text = status_words(s);
            if !ids.status_text.is_empty() {
                let tone = if s.follow.is_none() && !s.trouble.is_empty() { Tone::Warn } else { Tone::Muted };
                let words = ids.status_text.clone();
                ids.status = ui.node("status", Style::default(), |ui| ui.paragraph(&words, TextLook::status(tone)));
            }
            ids.details = ui.node("details", details_style(), |ui| {
                for (i, word) in text::DETAILS.iter().enumerate() {
                    // The wait this machine took for this detail on this area, under its name (showDetails, 819-830).
                    let wait = s.pace.about(s.samples, DETAIL_BLOCKS[i]).map(duration).unwrap_or_default();
                    // The sixteen-block map of a wide area is out of reach (line 823).
                    let disabled = DETAIL_BLOCKS[i] == 16 && s.samples > AREA_FINE_MAX;
                    let r = ui.detail_tab(&format!("d{i}"), word, &wait, TabOpts { pressed: i == s.detail, disabled });
                    ui.title(r.id, if disabled { text::SPAN_COARSE } else { text::DETAIL_HINT });
                    if r.clicked {
                        s.detail = i;
                    }
                }
            });
            let same = made_shows_form(s);
            let busy = s.follow.as_ref().map(|f| (f.ask.same_as(&s.seed, s.samples, DETAIL_BLOCKS[s.detail], &s.values), f.share(), f.stopping));
            ui.node("go", Style { gap: Vec2::new(6.0, 0.0), ..Style::default() }, |ui| {
                // While a run is under way the button says so; once a setting moved it starts the new run in its
                // place (lines 789-798). The shell only records the press: the application acts on it.
                let (word, look, disabled) = match busy {
                    Some((true, share, stopping)) => (text::GENERATING, ButtonLook::Running { share }, stopping),
                    Some((false, _, stopping)) => (text::GENERATE, ButtonLook::Primary, stopping),
                    // Disabled: the map already shows these settings (line 467).
                    None => (text::GENERATE, ButtonLook::Primary, same),
                };
                ids.generate_text = word.to_string();
                let r = ui.button_with("generate", word, look, ButtonOpts { disabled, grow: 1.0 });
                ids.generate = r.id;
                if r.clicked {
                    s.want = Some(Want::Generate);
                }
                if let Some((_, _, stopping)) = busy {
                    let r = ui.stop_button("stopRun", stopping);
                    ids.stop = r.id;
                    ui.title(r.id, text::STOP);
                    if r.clicked {
                        s.want = Some(Want::Stop);
                    }
                }
            });
        });
    });
}
/// The middle column: the stage holds the map's square and the strip under it (line 226).
fn middle_column(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, t: &Theme, frame_size: (f32, f32), world: Option<&World>, shown: Option<&Shown>) {
    // The square's side in the 2D and 3D views; the section's frame is the column's width by the height left.
    let (side, frame_h) = frame_size;
    let stage = Style { align_items: Align::Center, ..Style { column: true, ..fills() } };
    ids.stage = ui.node("stage", stage, |ui| {
        let frame = Style { position: Position::Relative, w: Dim::Px(side), h: Dim::Px(frame_h), shrink: 0.0, ..Style::default() };
        ids.frame = ui.node("frame", frame, |ui| {
            let view = View::from_index(s.view);
            // The note comes after the veil, which dims the content and not the note (line 238).
            let mut words = None;
            // The run's own picture is on the stage: what it lacks yet comes at the end of the run.
            let drawn = s.follow.as_ref().map_or(Drawn::Nothing, |f| f.drawn);
            match (world, view) {
                (None, _) => words = Some(text::NO_PREVIEW),
                (Some(w), None) if drawn != Drawn::Nothing && w.section.is_none() => words = Some(text::SECTION_PENDING),
                (Some(w), Some(View::Structures)) if drawn != Drawn::Nothing && w.structures.is_none() => words = Some(text::STRUCTURES_PENDING),
                // The section (view 5): a world without one says so; one whose cut is not read yet shows nothing.
                (Some(w), None) if w.section.is_none() => words = Some(text::NO_SECTION),
                (Some(w), None) => {
                    if let Some(shown) = shown {
                        cut_frame(ui, s, ids, t, w, shown, (side, frame_h));
                    }
                }
                // Before the 3D view: a world without structures has nothing to drape (line 2521).
                (Some(w), Some(View::Structures)) if w.structures.is_none() => words = Some(text::NO_STRUCTURES),
                (Some(w), Some(_)) if s.in3d => ids.relief = relief_canvas(ui, s, side, w),
                (Some(w), Some(view)) => ids.map = map_canvas(ui, s, t, side, w, view),
            }
            // `#frame.busy` (line 238): while a run is under way the map, the 3D view and the cut are at opacity
            // 0.3, which over the ground is the ground at 0.7 laid over them. The run's first sketch lifts it (line 2624).
            if s.follow.is_some() && drawn == Drawn::Nothing {
                let inset = Insets { left: Dim::Px(0.0), right: Dim::Px(0.0), top: Dim::Px(0.0), bottom: Dim::Px(0.0) };
                ui.fill(Style { position: Position::Absolute, inset, ..Style::default() }, t.ground.with_alpha(0.7));
            }
            if let Some(words) = words {
                note(ui, words);
                ids.note_text = words.to_string();
            }
            // `#under` (line 260): 10 px below the map, as wide as it.
            let under = Style {
                position: Position::Absolute,
                inset: Insets { left: Dim::Px(0.0), right: Dim::Px(0.0), top: Dim::Px(frame_h + 10.0), ..Insets::default() },
                column: true,
                gap: Vec2::new(0.0, 6.0),
                ..Style::default()
            };
            let probe_text = if view.is_none() { cut_probe_text(ui, s, ids, world, shown) } else { probe_text(ui, s, ids, side, world) };
            ids.probe_text = probe_text.clone();
            // "Another cut" is there in the section view only, when there is a section (line 2527).
            let cuts = if view.is_none() { world.and_then(|w| w.section.as_ref()).map_or(0, |sec| sec.cuts.len()) } else { 0 };
            ids.under = ui.node("under", under, |ui| {
                ui.node("probeRow", Style { gap: Vec2::new(18.0, 0.0), align_items: Align::Baseline, ..Style::default() }, |ui| {
                    let probe = Style { grow: 1.0, basis: Dim::Px(0.0), min_w: Dim::Px(0.0), ..Style::default() };
                    ids.probe = ui.node("probe", probe, |ui| ui.ellipsis(&probe_text, TextLook::body(Tone::Muted)));
                    if cuts > 0 {
                        let r = ui.button("nextCut", text::NEXT_CUT, ButtonLook::Link);
                        ids.next_cut = r.id;
                        ui.title(r.id, text::NEXT_CUT_HINT);
                        if r.clicked {
                            s.cut_at = (s.cut_at + 1) % cuts;
                        }
                    }
                    // The 3D view's levels: in this row, which is there in 2D as well, so that showing them moves
                    // nothing else. They take their room from the probe, which is cut with an ellipsis.
                    if ids.relief != Id::ROOT {
                        ui.node("levels", choice_style(), |ui| {
                            for level in Level::ALL {
                                let i = level.index();
                                let r = ui.tab(&format!("level{i}"), text::LEVELS[i], TabLook::Choice, TabOpts { pressed: s.level == level, disabled: false });
                                ids.levels[i] = r.id;
                                ui.title(r.id, text::LEVEL_HINTS[i]);
                                if r.clicked {
                                    s.level = level;
                                }
                            }
                        });
                    }
                    let r = ui.button("export", text::EXPORT, ButtonLook::Link);
                    ids.export = r.id;
                    ui.title(r.id, text::EXPORT_HINT);
                    if r.clicked {
                        s.want = Some(Want::Export);
                    }
                });
                steps_block(ui, s, ids, t, side);
            });
        });
    });
}

/// `#steps` (lines 263-273, 2563-2581, 2800-2813): a run's steps under a hairline that fills as the wait goes, or the
/// notice of where an export went. While a run is followed they are read from it; after it, from the state.
/// `width` is the frame's: `#under` is as wide.
fn steps_block(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, t: &Theme, width: f32) {
    let shown = match &s.follow {
        Some(f) => Steps::Run { done: f.done.clone(), now: f.current(), share: f.share() },
        None => s.steps.clone(),
    };
    let muted = TextLook::body(Tone::Muted);
    let now_look = TextLook::body(Tone::Text);
    let column = Style { column: true, gap: Vec2::new(0.0, 4.0), ..Style::default() };
    ids.steps = ui.node("steps", column, |ui| match &shown {
        Steps::Empty => {}
        Steps::Run { done, now, share } => {
            let (line, text_colour, share) = (t.line, t.text, *share);
            // `.bar` (line 268): a 1 px hairline of `--line`, filled from the left with `--text`.
            ids.steps_bar = ui
                .canvas("bar", Style { h: Dim::Px(1.0), shrink: 0.0, ..Style::default() }, |_, _| {
                    vec![
                        Cmd::Fill { rect: Rect::new(0.0, 0.0, width, 1.0), color: line },
                        Cmd::Fill { rect: Rect::new(0.0, 0.0, share.clamp(0.0, 1.0) * width, 1.0), color: text_colour },
                    ]
                })
                .id;
            // `.line` (line 269): one line, cut at the edge; the names of the done steps shorten first (`.step`,
            // 270-273), their seconds and the step under way stay whole.
            let row = Style { gap: Vec2::new(14.0, 0.0), min_w: Dim::Px(0.0), overflow: mere_layout::Overflow::Hidden, ..Style::default() };
            ui.node("line", row, |ui| {
                for (i, (label, seconds)) in done.iter().enumerate() {
                    let took = duration(*seconds);
                    ids.steps_items.push(format!("{} {label} {took}", text::DONE_MARK));
                    let step = Style { gap: Vec2::new(6.0, 0.0), min_w: Dim::Px(0.0), ..Style::default() };
                    ui.node(&format!("step{i}"), step, |ui| {
                        ui.text(text::DONE_MARK, muted);
                        ui.ellipsis(label, muted);
                        ui.text(&took, muted);
                    });
                }
                if !now.is_empty() {
                    ids.steps_items.push(format!("{} {now}", text::NOW_MARK));
                    ui.node("now", Style { gap: Vec2::new(6.0, 0.0), shrink: 0.0, ..Style::default() }, |ui| {
                        ui.text(text::NOW_MARK, now_look);
                        ui.text(now, now_look);
                    });
                }
            });
        }
        Steps::Saved { folder } => {
            // `.saved` (lines 277-281): the path takes a whole line, then the two links, wrapping. The path is one
            // flowing run: the label with its 6 px, then the folder wrapped on from it, back to the left edge.
            let notice = Style { wrap: true, align_items: Align::Baseline, gap: Vec2::new(16.0, 2.0), ..Style::default() };
            ids.saved = ui.node("saved", notice, |ui| {
                let path = Style { basis: Dim::Pct(100.0), column: true, min_w: Dim::Px(0.0), ..Style::default() };
                ui.node("path", path, |ui| ui.flow(text::EXPORTED_TO, Tone::Muted, 6.0, folder, now_look));
                let r = ui.button("openFolder", text::OPEN_FOLDER, ButtonLook::Link);
                ids.open_folder = r.id;
                if r.clicked {
                    s.want = Some(Want::OpenFolder);
                }
                let r = ui.button("closeNotice", text::CLOSE_NOTICE, ButtonLook::Link);
                ids.close_notice = r.id;
                if r.clicked {
                    s.steps = Steps::Empty;
                }
            });
        }
    });
}

/// The section in the frame (`#cut`, lines 284-291 and 2205-2332): over the whole frame a column of the strip
/// (the whole cut drawn small, with the frame of the part looked at), its note, and the cut at true scale. The
/// pictures are uploaded by the app (`cut::nav_image`, `cut::sheet`); here they are placed. The cut view's box is
/// arithmetic before layout, as `side` is, because the look is settled against it before anything reads the look:
/// the frame less the strip (56), the note (its wrapped lines of 18 px and 8 px under) and two gaps of 8.
fn cut_frame(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, t: &Theme, world: &World, shown: &Shown, size: (f32, f32)) {
    let section = world.section.as_ref().expect("a cut is shown only for a world with a section");
    let (w, h) = size;
    let length = section.length;
    let ground = &shown.ground;
    let range = cut::nav_range(section, ground);
    let note_text = text::nav_note(&text::times(cut::nav_times(length, w as f64, range)));
    let muted = TextLook::body(Tone::Muted);
    let lines = wrap_lines(&note_text, muted, w).len() as f32;
    let box_h = (h - NAV_H - 8.0 - 18.0 * lines - 8.0 - 8.0).max(0.0);
    let (bw, bh) = (w as f64, box_h as f64);
    s.cut_look = cut::settled(s.cut_look, bw, bh, ground, length);
    ids.cut_box = (bw, bh);
    let (line, text_colour) = (t.line, t.text);

    let column = Style {
        position: Position::Absolute,
        inset: Insets { left: Dim::Px(0.0), right: Dim::Px(0.0), top: Dim::Px(0.0), bottom: Dim::Px(0.0) },
        column: true,
        gap: Vec2::new(0.0, 8.0),
        ..Style::default()
    };
    ids.cut = ui.node("cut", column, |ui| {
        // `#nav` (line 285): 56 px with a hairline under it; its canvas shows in the 55 px above the hairline.
        let nav_style = Style { h: Dim::Px(NAV_H), shrink: 0.0, ..Style::default() };
        ids.nav = ui
            .canvas("nav", nav_style, |ui, resp| {
                if let Some(rect) = resp.rect {
                    let pointer = ui.pointer();
                    // A click brings the window there; a press on the frame (3 px of grace) carries it along (2319-2332).
                    if let (true, Some(p)) = (resp.pressed, pointer) {
                        let x = (p.x - rect.x) as f64;
                        let frame = cut::nav_frame(s.cut_look, bw, bh, length, ground, section, range, bw).rect();
                        let (left, right) = (frame.x as f64, (frame.x + frame.w) as f64);
                        let inside = x >= left - 3.0 && x <= right + 3.0;
                        s.nav_carry = Some(if inside { x - left } else { frame.w as f64 / 2.0 });
                    }
                    if let (Some(grip), Some(p)) = (s.nav_carry, pointer) {
                        let go = cut::nav_go(s.cut_look, grip, (p.x - rect.x) as f64, bw, length);
                        s.cut_look = cut::settled(go, bw, bh, ground, length);
                    }
                    if !resp.held {
                        s.nav_carry = None;
                    }
                }
                let frame = cut::nav_frame(s.cut_look, bw, bh, length, ground, section, range, bw).rect();
                // `#nav` is a pointer, its frame a hand that grabs, and both grab while carried (lines 285, 288, 2329).
                if resp.hovered || resp.held {
                    let on_frame = resp.rect.zip(ui.pointer()).is_some_and(|(r, p)| frame.contains(Vec2::new(p.x - r.x, p.y - r.y)));
                    ui.want_cursor(if s.nav_carry.is_some() || on_frame { Cursor::Grab } else { Cursor::Hand });
                }
                vec![
                    Cmd::Image { image: NAV_IMAGE, rect: Rect::new(0.0, 0.0, w, NAV_H - 1.0) },
                    // `outline: 1px solid var(--text)` inside the frame's box.
                    Cmd::Frame { rect: frame, width_px: 1.0, color: text_colour },
                    Cmd::Fill { rect: Rect::new(0.0, NAV_H - 1.0, w, 1.0), color: line },
                ]
            })
            .id;
        // `#navNote` (line 289): 12 px muted, 8 px under it.
        let note_style = Style { shrink: 0.0, margin: Margin { bottom: Side::Px(8.0), ..Margin::default() }, ..Style::default() };
        ids.nav_note = ui.node("navNote", note_style, |ui| ui.paragraph(&note_text, muted));
        // `#cutView` (line 290): takes what is left; its canvas shows in the 1 px less above the hairline.
        let view_style = Style { grow: 1.0, basis: Dim::Px(0.0), min_h: Dim::Px(0.0), ..Style::default() };
        ids.cut_view = ui
            .canvas("cutView", view_style, |ui, resp| {
                if let Some(rect) = resp.rect {
                    // The wheel comes closer to the point under it (2467-2474).
                    if let Some((notches, p)) = ui.take_wheel(resp.id) {
                        let at = ((p.x - rect.x) as f64, (p.y - rect.y) as f64);
                        s.cut_look = cut::wheel(s.cut_look, at, notches, bw, bh, ground, length);
                    }
                    let pointer = ui.pointer();
                    if let (true, Some(p)) = (resp.pressed, pointer) {
                        s.cut_carry = Some((p, s.cut_look));
                    }
                    // Read until the press is over, not only while held, so the last movement before the release counts.
                    if let (Some((from, start)), Some(p)) = (s.cut_carry, pointer) {
                        s.cut_look = cut::dragged(start, (p.x - from.x) as f64, (p.y - from.y) as f64, bw, bh, ground, length);
                    }
                    if !resp.held {
                        s.cut_carry = None;
                    }
                }
                // `cursor: crosshair` (line 290); the page sets no other while the cut is carried.
                if resp.hovered || resp.held {
                    ui.want_cursor(Cursor::Crosshair);
                }
                let mut cmds = vec![Cmd::PushClip { rect: Rect::new(0.0, 0.0, w, box_h - 1.0) }];
                if let Some(sight) = cut::sight(s.cut_look, bw, bh, length, ground.rows) {
                    cmds.push(Cmd::Image { image: CUT_IMAGE, rect: cut::sheet_rect(s.cut_look, sight, bh) });
                }
                cmds.push(Cmd::PopClip);
                cmds.push(Cmd::Fill { rect: Rect::new(0.0, box_h - 1.0, w, 1.0), color: line });
                cmds
            })
            .id;
    });
}

/// What `#probe` says in the section: the block under the pointer when it is over the cut, else the cut's line
/// (`restCut`, 2455-2459). Nothing until a cut is shown. While a drag is held the page freezes the line; here it
/// follows the pointer: UNVERIFIED small difference.
fn cut_probe_text(ui: &Ui, s: &PreviewState, ids: &ShellIds, world: Option<&World>, shown: Option<&Shown>) -> String {
    let (Some(w), Some(shown)) = (world, shown) else { return String::new() };
    let Some(section) = w.section.as_ref() else { return String::new() };
    let cut = &section.cuts[shown.at];
    let rest = || text::cut_rest(shown.at + 1, section.cuts.len(), &text::num(cut.z), &text::num(cut.x), &text::num(cut.x + section.length as f64));
    let (Some(p), Some(rect)) = (ui.pointer(), ui.rect(ids.cut_view)) else { return rest() };
    if !rect.contains(p) {
        return rest();
    }
    cut::probe(w, shown, s.cut_look, ((p.x - rect.x) as f64, (p.y - rect.y) as f64)).unwrap_or_else(rest)
}

/// `#note` (line 257): over the whole frame, its words centred in 40 px of padding, in the muted colour. The
/// lines of a wrapped note are left-aligned in a block that is centred, not centred one by one: UNVERIFIED
/// against the page, which centres each line.
fn note(ui: &mut Ui, words: &str) {
    let style = Style {
        position: Position::Absolute,
        inset: Insets { left: Dim::Px(0.0), right: Dim::Px(0.0), top: Dim::Px(0.0), bottom: Dim::Px(0.0) },
        pad: Edges::all(40.0),
        align_items: Align::Center,
        justify: Justify::Center,
        ..Style::default()
    };
    ui.node("note", style, |ui| ui.paragraph(words, TextLook::body(Tone::Muted)));
}

/// What `#probe` says: the point or structure under the pointer when it is over the map, else the area
/// (`restProbe`, 1761-1765, and the `mousemove` of 1828-1835). The pointer is judged against the last frame.
fn probe_text(ui: &Ui, s: &PreviewState, ids: &ShellIds, side: f32, world: Option<&World>) -> String {
    // The page leaves the line empty until a map exists (restProbe returns at `!world`).
    let Some(w) = world else { return String::new() };
    let map_shown = ids.map != Id::ROOT;
    let rest = || text::area_probe(&text::num(w.span()), &text::num(w.span() / 8.0), map_shown && tick_at(w, s.look, s.cut_at).is_some());
    let (Some(p), Some(rect), Some(view)) = (ui.pointer(), ui.rect(ids.map), View::from_index(s.view)) else { return rest() };
    if !map_shown || !rect.contains(p) {
        return rest();
    }
    let local = Vec2::new(p.x - rect.x, p.y - rect.y);
    if view == View::Structures {
        if let Some((kind, x, z)) = structure_near(w, &s.chosen[4], s.look, side, local) {
            let name = text::pretty(&w.structures.as_ref().expect("a near structure needs structures").ids[kind]);
            return text::structure_probe(&text::num(x), &text::num(z), &name);
        }
    }
    let (col, row, i) = point_under(w, s.look, side, local);
    text::probe(
        &text::num(w.origin_x + col as f64 * w.step),
        &text::num(w.origin_z + row as f64 * w.step),
        &text::num(w.heights[i] as f64),
        &text::pretty(&w.biomes[w.biome_at[i] as usize].id),
    )
}

/// The 2D map: a canvas that takes the wheel, the carry and the click, and draws the picture, the marks, the
/// grid, the cross and the ticks (`map::frame_cmds`). Returns its id.
fn map_canvas(ui: &mut Ui, s: &mut PreviewState, t: &Theme, side: f32, world: &World, view: View) -> Id {
    let style = Style { w: Dim::Px(side), h: Dim::Px(side), shrink: 0.0, ..Style::default() };
    let (grid, muted) = (t.grid, t.muted);
    let resp = ui.canvas("map", style, |ui, resp| {
        if let Some(rect) = resp.rect {
            // The wheel comes closer to the point under it (lines 1796-1803).
            if let Some((notches, p)) = ui.take_wheel(resp.id) {
                s.look = s.look.wheel(Vec2::new((p.x - rect.x) / rect.w, (p.y - rect.y) / rect.h), notches);
            }
            let pointer = ui.pointer();
            if resp.pressed {
                if let Some(from) = pointer {
                    s.carry = Some(Carry { from, look: s.look, carried: false });
                }
            }
            // A press that travels carries the map. Read until the press is over, not only while held, so the
            // last movement before the release is not lost.
            if let (Some(c), Some(p)) = (s.carry.as_mut(), pointer) {
                let d = Vec2::new(p.x - c.from.x, p.y - c.from.y);
                // A hand is never still: a few pixels are not yet a move.
                if !c.carried && d.x.hypot(d.y) >= 4.0 {
                    c.carried = true;
                }
                if c.carried {
                    s.look = Look::dragged(c.look, d, side);
                }
            }
            // A press that stays in place is the click that chooses (lines 1821-1827); the structures are
            // chosen in the list only.
            if resp.clicked && view != View::Structures && s.carry.is_some_and(|c| !c.carried) {
                if let (Some(p), Some((at, ids))) = (pointer, rows(world, view)) {
                    let (_, _, i) = point_under(world, s.look, side, Vec2::new(p.x - rect.x, p.y - rect.y));
                    let id = ids[at[i] as usize].clone();
                    let set = &mut s.chosen[view.index()];
                    if !set.remove(&id) {
                        set.insert(id);
                    }
                }
            }
            // `cursor: crosshair`, and `grabbing` once the press has travelled (lines 233, 1813).
            if resp.hovered || resp.held {
                let carried = s.carry.is_some_and(|c| c.carried) && resp.held;
                ui.want_cursor(if carried { Cursor::Grab } else { Cursor::Crosshair });
            }
            if !resp.held {
                s.carry = None;
            }
        }
        frame_cmds(world, view, &s.chosen[view.index()], s.look, side, grid, muted, s.cut_at)
    });
    resp.id
}
/// The 3D view: a canvas that takes the orbit's drag and wheel. It draws nothing itself, because the scene is a mesh
/// the app draws into this rect before the interface list (the page's #relief, absolutely over the frame).
/// Returns its id.
fn relief_canvas(ui: &mut Ui, s: &mut PreviewState, side: f32, world: &World) -> Id {
    let style = Style { w: Dim::Px(side), h: Dim::Px(side), shrink: 0.0, ..Style::default() };
    let resp = ui.canvas("relief", style, |ui, resp| {
        let orbit = s.orbit.get_or_insert_with(|| scene::orbit(world, None));
        if let Some(rect) = resp.rect {
            if let Some((notches, _)) = ui.take_wheel(resp.id) {
                orbit.zoom(notches);
            }
            let pointer = ui.pointer();
            if resp.pressed {
                s.turn_from = pointer;
            }
            // Read until the press is over, not only while held, so the last movement before the release counts
            // (as the map does). A drag that leaves the square keeps turning: OrbitControls captures the pointer.
            if let (Some(from), Some(p)) = (s.turn_from, pointer) {
                // OrbitControls turns a full circle per clientHeight of its canvas: the square's side.
                orbit.drag(p.x - from.x, p.y - from.y, rect.h);
                s.turn_from = Some(p);
            }
            if !resp.held {
                s.turn_from = None;
            }
        }
        Vec::new()
    });
    resp.id
}

/// What a press in the list asks of the choice (applied once the list is built, so that the build only reads).
enum Pick {
    /// `choose(id)`: the id joins the choice or leaves it.
    Toggle(String),
    /// A part's line: the choice becomes exactly these ids, or none when it already was (1703-1708).
    Part(Vec<String>),
    /// `picked.clear()`.
    Clear,
}

/// The right column (lines 429-449): the figures and the history, then the list of what the view shows. In the
/// section the figures are not built (line 2533: they would describe the map) and the list is the ores of the cut.
fn right_column(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, t: &Theme, cols: &Columns, world: Option<&World>, shown: Option<&Shown>, stacked: bool) {
    let cut = s.view == 5;
    // The history's plots are as wide as the panel; stacked, the panel's scrollbar keeps its gutter.
    let plot_w = ids.plan.widths[1] - if stacked { mere_ui::GUTTER } else { 0.0 };
    if !cut {
        figures_block(ui, s, ids, t, cols, world, plot_w, stacked);
    }

    // `#listBlock.rule` (lines 295-296, 440): a hairline on top, 9 px of padding, a gap of 6. In the section
    // the rule and the padding go (line 294). Stacked, the panel is one scroll and the block is as tall as its
    // rows: it takes no free height.
    let block = if stacked { Style { column: true, shrink: 0.0, ..Style::default() } } else { Style { column: true, ..fills() } };
    ids.list_block = ui.node("listBlock", block, |ui| {
        if !cut {
            ui.fill(rule(), t.edge);
        }
        let pad = Edges::new(if cut { 0.0 } else { 9.0 }, 0.0, 0.0, 0.0);
        let inner = if stacked { Style { column: true, pad, gap: Vec2::new(0.0, 6.0), shrink: 0.0, ..Style::default() } } else { Style { column: true, pad, gap: Vec2::new(0.0, 6.0), ..fills() } };
        ui.node("listBody", inner, |ui| {
            if cut {
                ore_list(ui, s, ids, world, shown, stacked);
            } else {
                view_list(ui, s, ids, world, stacked);
            }
        });
    });
}

/// A list's rows: in a scroll area of their own beside the stage, or, stacked, as plain content of the panel's one
/// scroll area. Returns the box's id and whether it scrolls.
fn list_area(ui: &mut Ui, stacked: bool, y: &mut f32, body: impl FnOnce(&mut Ui)) -> (Id, bool) {
    if stacked {
        (ui.node("list", Style { column: true, shrink: 0.0, ..Style::default() }, body), false)
    } else {
        (ui.scroll("list", y, scroll_style(), ScrollOpts::LIST, body).id, true)
    }
}

/// The section's list: its ores, with what the cut played of each (lines 2370-2385).
fn ore_list(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, world: Option<&World>, shown: Option<&Shown>, stacked: bool) {
    let title = text::ORE_LIST[0].to_uppercase();
    let section = world.and_then(|w| w.section.as_ref());
    // Names and figures of the ore rows, in `ORE_KINDS` order; empty without a cut.
    let figures: Vec<(usize, String, String, String)> = match shown {
        Some(shown) => (0..ORE_KINDS.len())
            .filter_map(|k| {
                let f = shown.play.per_chunk[k].as_ref()?;
                let figure = |v: Option<f64>| v.map_or("\u{2013}".to_string(), |v| text::num_digits(v, 1));
                Some((k, figure(f.low), figure(f.high), f.from.join(", ")))
            })
            .collect(),
        None => Vec::new(),
    };
    ui.node("listHead", column(), |ui| {
        ui.node("title", Style { pad: Edges::new(0.0, 0.0, 2.0, 0.0), ..Style::default() }, |ui| {
            ui.text(&title, TextLook::heading());
        });
        // `#pick` (line 341): two lines of the small size at most.
        let pick = Style { h: Dim::Px(34.0), overflow: mere_layout::Overflow::Hidden, ..Style::default() };
        let words = match (shown, section) {
            (Some(_), Some(sec)) if sec.unplayed() > 0 => text::ore_unplayed(sec.unplayed()),
            (Some(_), _) => text::ORE_PICK_HINT.to_string(),
            (None, _) => String::new(),
        };
        ids.pick = ui.node("pick", pick, |ui| ui.paragraph(&words, TextLook::small(Tone::Muted)));
    });
    ids.list_title = title;
    // Line 2370-2372: the legend names the columns; a list without a cut has no legend.
    if shown.is_some() {
        let head = ui.list_head("legend", text::ORE_LIST[1], text::ORE_COLUMNS[0], text::ORE_COLUMNS[1]);
        let hint = text::ore_columns_hint(&text::num(shown.map_or(0, |s| s.play.counted) as f64));
        ui.title(head[0], &hint);
        ui.title(head[1], &hint);
    }
    let (list, scrolls) = list_area(ui, stacked, &mut s.list_y, |ui| {
        for (k, low, high, from) in &figures {
            let name = ORE_KINDS[*k];
            let row = ListRow { swatch: rgb(ORE_COLOURS[*k]), name: text::ORES[*k], width: low, share: high };
            let opts = RowOpts { pressed: s.chosen_ores.contains(name), choosing: !s.chosen_ores.is_empty(), ..RowOpts::default() };
            let r = ui.list_row(&format!("ore{k}"), &row, opts);
            ui.title(r.id, &text::ore_from(from));
            ids.ores.push(r.id);
            // A click picks the ore, or lets it go (2385).
            if r.clicked && !s.chosen_ores.remove(name) {
                s.chosen_ores.insert(name.to_string());
            }
        }
    });
    ids.list_scroll = list;
    if scrolls {
        ids.scroll_areas.push(list);
    }
}

/// The list beside the map in the five map views (`showList` and `showChoice`, lines 1639-1758): its title, the
/// filter, what the choice weighs, the legend and the rows.
fn view_list(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, world: Option<&World>, stacked: bool) {
    let view = View::from_index(s.view).expect("the map views are 0 to 4");
    let counted = view == View::Structures;
    let list: Option<&List> = world.and_then(|w| w.measured.lists[view.index()].as_ref());
    // Before any list was shown the page's markup holds the filter and the legend with no column name.
    let fresh = world.is_none();
    let title = if fresh { String::new() } else { text::LISTS[view.index()][0].to_uppercase() };
    let column_word = if fresh { "" } else { text::LISTS[view.index()][1] };
    let filter_shown = fresh || (list.is_some() && (view == View::Biomes || counted));
    let legend_shown = fresh || list.is_some();
    let asked = if filter_shown { s.filter.trim().to_lowercase() } else { String::new() };
    let chosen: BTreeSet<String> = s.chosen[view.index()].clone();
    let small = TextLook::small(Tone::Muted);

    // Rows with something in them: biomes by share, structures by count, bands in their own order, lowest first.
    let mut order: Vec<usize> = list.map_or(Vec::new(), |l| (0..l.ids.len()).filter(|&i| l.cells[i] != 0.0 && !l.cells[i].is_nan()).collect());
    if let (Some(l), true) = (list, view == View::Biomes || counted) {
        order.sort_by(|&a, &b| l.cells[b].partial_cmp(&l.cells[a]).unwrap_or(std::cmp::Ordering::Equal));
    }
    // What the choice weighs: its rows in list order, those the area has.
    let lit: Vec<usize> = list.map_or(Vec::new(), |l| (0..l.ids.len()).filter(|&i| chosen.contains(&l.ids[i]) && l.cells[i] != 0.0 && !l.cells[i].is_nan()).collect());
    let mut presses: Vec<Pick> = Vec::new();

    ids.list_title = title.clone();
    ui.node("listHead", column(), |ui| {
        ui.node("title", Style { pad: Edges::new(0.0, 0.0, 2.0, 0.0), ..Style::default() }, |ui| {
            ui.text(&title, TextLook::heading());
        });
        if filter_shown {
            // `#filter { margin: 4px 0 6px }` (line 339).
            ui.node("filterBox", Style { column: true, pad: Edges::new(4.0, 0.0, 6.0, 0.0), ..Style::default() }, |ui| {
                let opts = FieldOpts { look: FieldLook::Text, placeholder: text::FILTER, max_chars: 200, width: Dim::Pct(100.0), focus: false, ..FieldOpts::default() };
                ids.filter = ui.text_field("filter", &mut s.filter, opts).resp.id;
            });
        }
        // `#pick` (line 341): two lines whatever it says, so the first choice does not move the rows under the pointer.
        let line = Style { h: Dim::Px(34.0), overflow: mere_layout::Overflow::Hidden, ..Style::default() };
        let Some(l) = list else {
            ids.pick = ui.node("pick", line, |_| {});
            return;
        };
        if lit.is_empty() {
            let hint = if counted { text::PICK_HINT_MARKS } else { text::PICK_HINT };
            ids.pick_text = hint.to_string();
            ids.pick = ui.node("pick", line, |ui| ui.paragraph(hint, small));
            return;
        }
        // Up to three, the choice is named, since its rows may be far apart in the list. A name takes its row out.
        let row = Style { wrap: true, align_items: Align::Baseline, ..line };
        let cells: f64 = lit.iter().map(|&i| l.cells[i]).sum();
        ids.pick = ui.node("pick", row, |ui| {
            let mut said = String::new();
            if lit.len() > 3 {
                let many = text::picked_many(lit.len());
                ui.text(&many, small);
                said.push_str(&many);
            } else {
                for (at, &i) in lit.iter().enumerate() {
                    if at > 0 {
                        ui.text(", ", small);
                        said.push_str(", ");
                    }
                    let r = ui.button(&format!("pick{i}"), &l.names[i], ButtonLook::Pick);
                    ui.title(r.id, &text::pick_remove(&l.names[i]));
                    ids.pick_names.push(r.id);
                    said.push_str(&l.names[i]);
                    if r.clicked {
                        presses.push(Pick::Toggle(l.ids[i].clone()));
                    }
                }
            }
            let tail = match world {
                Some(w) if !l.counted => text::picked_share(&percent(100.0 * cells / (w.samples * w.samples) as f64)),
                _ => text::picked_count(&text::num(cells), &text::num(l.total)),
            };
            ui.text(&tail, small);
            said.push_str(&tail);
            // `#pick button.none { margin-left: 8px }`.
            ui.node("gap", Style { w: Dim::Px(8.0), shrink: 0.0, ..Style::default() }, |_| {});
            let none = if counted { text::PICK_NONE_MARKS } else { text::PICK_NONE };
            let r = ui.button("none", none, ButtonLook::PickNone);
            ids.pick_none = r.id;
            said.push_str(none);
            if r.clicked {
                presses.push(Pick::Clear);
            }
            ids.pick_text = said;
        });
    });

    if legend_shown {
        let (width, share) = if counted { (text::COL_SPACING, text::COL_COUNT) } else { (text::COL_WIDTH, text::COL_SHARE) };
        let head = ui.list_head("legend", column_word, width, share);
        // Only the spacing column of the structures says what it is (line 1643).
        if counted {
            ui.title(head[0], text::COL_SPACING_HINT);
        }
    }

    // The filter reads the id too, so the name of a mod finds its biomes. A chosen row stays in sight.
    let visible = |id: &str| chosen.contains(id) || asked.is_empty() || id.replace('_', " ").contains(&asked);
    let (list_box, scrolls) = list_area(ui, stacked, &mut s.list_y, |ui| {
        let Some(l) = list else { return };
        let total = world.map_or(1.0, |w| (w.samples * w.samples) as f64);
        let choosing = !lit.is_empty();
        let row = |ui: &mut Ui, i: usize, ids: &mut ShellIds, presses: &mut Vec<Pick>| {
            let share = if counted { text::num(l.cells[i]) } else { percent(100.0 * l.cells[i] / total) };
            let width = blocks(l.widths[i]);
            let item = ListRow { swatch: rgb(l.colours[i]), name: &l.names[i], width: &width, share: &share };
            let opts = RowOpts { pressed: chosen.contains(&l.ids[i]), choosing, mark: counted, ..RowOpts::default() };
            let r = ui.list_row(&format!("row{i}"), &item, opts);
            // The biomes and the structures are named by their id on hover (line 1660).
            if view == View::Biomes || counted {
                ui.title(r.id, &l.ids[i]);
            }
            ids.list_rows.push(r.id);
            ids.list_keys.push(l.ids[i].clone());
            if r.clicked {
                presses.push(Pick::Toggle(l.ids[i].clone()));
            }
        };
        if !counted {
            for &i in order.iter().filter(|&&i| visible(&l.ids[i])) {
                row(ui, i, ids, &mut presses);
            }
            // After the biomes of the area, those it does not hold: named, since a count alone does not say which.
            // They are not rows to choose: there is nothing of them to show on the map.
            let absent: Vec<&String> = world.filter(|_| view == View::Biomes).map_or(Vec::new(), |w| w.absent.iter().filter(|id| visible(id)).collect());
            if !absent.is_empty() {
                let count = text::num(world.map_or(0, |w| w.absent.len()) as f64);
                let opts = PartOpts { pressed: false, first: order.is_empty(), button: false };
                let r = ui.list_part("absentHead", text::ABSENT, &count, opts);
                ui.title(r.id, text::ABSENT_HINT);
                ids.list_parts.push(r.id);
                ids.list_part_words.push((text::ABSENT.to_string(), count));
                for (k, id) in absent.iter().enumerate() {
                    let name = text::pretty(id);
                    let item = ListRow { swatch: rgb(biome_colour(id)), name: &name, width: "", share: "" };
                    let r = ui.list_row(&format!("absent{k}"), &item, RowOpts { absent: true, ..RowOpts::default() });
                    ui.title(r.id, id);
                    ids.absent_rows.push(r.id);
                }
            }
            return;
        }
        // The structures come in two parts, what is built on the surface and what is built under the ground, each
        // under a quiet line of its own that shows the whole part alone.
        let under = world.and_then(|w| w.structures.as_ref()).map(|st| st.under.clone()).unwrap_or_default();
        let mut parts_made = 0;
        for part in [false, true] {
            let members: Vec<usize> = order.iter().copied().filter(|&i| under.get(i) == Some(&part)).collect();
            if members.is_empty() {
                continue;
            }
            let first = parts_made == 0;
            parts_made += 1;
            let part_ids: Vec<String> = members.iter().map(|&i| l.ids[i].clone()).collect();
            if !members.iter().any(|&i| visible(&l.ids[i])) {
                continue;
            }
            let sum: f64 = members.iter().map(|&i| l.cells[i]).sum();
            let (name, count) = (text::PARTS[part as usize], text::num(sum));
            let pressed = chosen.len() == part_ids.len() && part_ids.iter().all(|id| chosen.contains(id));
            let r = ui.list_part(&format!("part{}", part as u8), name, &count, PartOpts { pressed, first, button: true });
            ui.title(r.id, text::PART_HINT);
            ids.list_parts.push(r.id);
            ids.list_part_words.push((name.to_string(), count));
            if r.clicked {
                presses.push(Pick::Part(part_ids));
            }
            for &i in members.iter().filter(|&&i| visible(&l.ids[i])) {
                row(ui, i, ids, &mut presses);
            }
        }
    });
    ids.list_scroll = list_box;
    if scrolls {
        ids.scroll_areas.push(list_box);
    }

    let picked = &mut s.chosen[view.index()];
    for press in presses {
        match press {
            Pick::Toggle(id) => {
                if !picked.remove(&id) {
                    picked.insert(id);
                }
            }
            Pick::Part(part) => {
                let alone = picked.len() == part.len() && part.iter().all(|id| picked.contains(id));
                picked.clear();
                if !alone {
                    picked.extend(part);
                }
            }
            Pick::Clear => picked.clear(),
        }
    }
}

/// `#figuresBlock` (lines 430-439): the figures of the map and the history.
fn figures_block(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, t: &Theme, cols: &Columns, world: Option<&World>, plot_w: f32, stacked: bool) {
    let body = TextLook::body(Tone::Text);
    let muted = TextLook::body(Tone::Muted);
    // It keeps its height whatever the room: stacked, the panel is a scroll area and what does not fit scrolls.
    ui.node("figuresBlock", Style { shrink: 0.0, ..column() }, |ui| {
        let r = ui.fold_with("figures", text::FIGURES, &mut s.figures_open, mere_ui::FoldOpts { bare: stacked, ..Default::default() }, |ui| {
            // Line 297: name, value and change; the rule before "Sea" spans all three.
            let grid = Style {
                display: Display::Grid,
                cols: Tracks::new(&[Track::MinMaxFr0(1.0), Track::Auto, Track::Auto]),
                gap: Vec2::new(cols.fig_gap, 4.0),
                align_items: Align::Baseline,
                ..Style::default()
            };
            let cells = world.map_or(Vec::new(), |w| figure_cells(&w.measured.figures, s.before.as_ref()));
            ids.figures_text = cells.iter().map(|c| c.text.clone()).collect();
            // Which figure the cells are at (a name starts one), and the last name's id, for the titles.
            let (mut at, mut name_id) = (usize::MAX, Id::ROOT);
            ids.figures_grid = ui.node("grid", grid, |ui| {
                for (n, cell) in cells.iter().enumerate() {
                    let key = format!("cell{n}");
                    match cell.kind {
                        CellKind::Blank => {
                            ui.text("", muted);
                        }
                        // Line 300.
                        CellKind::Head => {
                            ui.node(&key, Style { justify: Justify::End, ..Style::default() }, |ui| ui.text(&cell.text, TextLook::small(Tone::Muted)));
                        }
                        CellKind::Rule => {
                            let span = Style { col_span_all: true, pad: Edges::vh(4.0, 0.0), ..Style::default() };
                            ui.node(&key, span, |ui| ui.fill(Style { h: Dim::Px(1.0), grow: 1.0, shrink: 0.0, ..Style::default() }, t.line));
                        }
                        // Line 299: a name is read whole, so it may take a second line.
                        CellKind::Name => {
                            name_id = ui.node(&key, Style { min_w: Dim::Px(0.0), ..Style::default() }, |ui| ui.paragraph(&cell.text, body));
                            at = at.wrapping_add(1);
                            // The definition is shown on hover (line 1527).
                            ui.title(name_id, text::FIGURE_DEFS[at]);
                        }
                        // Lines 301-303: the three figures of the goal are larger and semibold.
                        CellKind::Value { goal, muted: grey } => {
                            let tone = if grey { Tone::Muted } else { Tone::Text };
                            let look = if goal { TextLook { size: cols.goal, line: cols.goal * 1.5, weight: Weight::SemiBold, tone, ..body } } else { TextLook { tone, ..body } };
                            let v = ui.node(&key, Style { min_w: Dim::Px(panels::FIG_VALUE), justify: Justify::End, ..Style::default() }, |ui| ui.text(&cell.text, look));
                            if goal {
                                ids.goal_values.push(v);
                            }
                            // A figure the detail cannot give says why, on its name and its value (line 1533).
                            if grey {
                                ui.title(name_id, text::NEEDS_FULL);
                                ui.title(v, text::NEEDS_FULL);
                            }
                        }
                        // Lines 304-306: what moved reads as strongly as the value on the figures of the goal.
                        CellKind::Before { goal } => {
                            let look = TextLook { tone: if goal { Tone::Text } else { Tone::Muted }, ..body };
                            let v = ui.node(&key, Style { min_w: Dim::Px(panels::FIG_CHANGE), justify: Justify::End, ..Style::default() }, |ui| ui.text(&cell.text, look));
                            // What the figure was in the run before (line 1530).
                            if let (false, Some(b)) = (cell.text.is_empty(), s.before.as_ref()) {
                                let k = figures::KEYS[at];
                                let was = if k == "count" { format!("{}", b.count) } else { b.get(k).map_or(String::new(), |v| figures::format_value(b, k, v)) };
                                ui.title(v, &text::was(&was));
                            }
                        }
                    }
                }
            });
        });
        ids.figures_head = r.id;
        // Closed at the start (the page's `data-closed`): the body is drawn to its width, so it is built when open.
        let r = ui.fold("history", text::HISTORY, &mut s.history_open, |ui| history_body(ui, s.history.as_slice(), ids, t, plot_w, world, s.made.as_ref()));
        ids.history_head = r.id;
    });
}

/// `#history` (lines 1569-1637): one small line per figure, oldest run left, and under them what the run pointed at
/// changed. The page only draws with a map on screen.
fn history_body(ui: &mut Ui, history: &[Entry], ids: &mut ShellIds, t: &Theme, plot_w: f32, world: Option<&World>, made: Option<&Ask>) {
    let (Some(world), Some(made)) = (world, made) else { return };
    let f = &world.measured.figures;
    let runs = here(history, &made.seed, f.span, f.step);
    let len = runs.len();
    ids.history_body = ui.hover_area("history", Style { column: true, gap: Vec2::new(0.0, 8.0), ..Style::default() }, |ui, at| {
        // The pointer picks the nearest run, across every line at once (line 1630).
        // `#history { cursor: crosshair }` (line 310).
        if at.is_some() {
            ui.want_cursor(Cursor::Crosshair);
        }
        let pointed = at.filter(|_| len >= 2).map(|a| (js_round(a as f64 * (len - 1) as f64).max(0.0) as usize).min(len - 1));
        let trend = trend(&runs, pointed, plot_w as f64);
        for (n, row) in trend.rows.iter().enumerate() {
            let p = |x: f64, y: f64| Vec2::new(x as f32, y as f32);
            // Lines 314-316: the mark of the run pointed at, the line, and the dot on it.
            let mut plot = vec![Cmd::Polyline { points: vec![p(row.now_x, 0.0), p(row.now_x, PLOT_H)], width: 1.0, color: t.muted }];
            for line in &row.lines {
                plot.push(Cmd::Polyline { points: line.iter().map(|&(x, y)| p(x, y)).collect(), width: 1.5, color: t.text });
            }
            if let Some((x, y)) = row.dot {
                plot.push(Cmd::Disc { center: p(x, y), radius: 3.5, color: t.text });
                plot.push(Cmd::Ring { center: p(x, y), radius: 3.5, width: 2.0, color: t.ground });
            }
            ids.trends.push(ui.trend_row(&format!("trend{n}"), row.name, &row.value, plot));
        }
        // Line 317.
        ui.node("probe", Style { min_h: Dim::Px(32.0), ..Style::default() }, |ui| ui.paragraph(&trend.probe, TextLook::small(Tone::Muted)));
        ids.history_probe = trend.probe;
    });
}

/// The shell as a picture of the page with no preview: builds until the scrollbar's gutter has settled (it comes
/// one frame late), at most three times, and returns the last draw list with the `Ui` (for the rects) and the ids.
pub fn picture(theme: Theme, size: Vec2) -> (DrawList, Ui, ShellIds) {
    picture_with(theme, size, None, PreviewState::default())
}

/// A picture of the shell with a Home given by the host, not raised to the window's minimum: for pictures of Home
/// below it, which no window shows. Builds until the gutter has settled, as `picture_with` does.
pub fn picture_with_home(theme: Theme, size: Vec2, world: Option<&World>, state: PreviewState, home: &mut dyn Home) -> (DrawList, Ui, ShellIds) {
    let mut ui = Ui::new(theme);
    let mut s = state;
    let mut last = None;
    for _ in 0..4 {
        ui.begin_frame(size);
        let ids = build(&mut ui, &mut s, size, world, None, Some(&mut *home));
        let list = ui.end_frame().clone();
        let again = ui.want_redraw() || home.act();
        last = Some((list, ids));
        if !again {
            break;
        }
    }
    let (list, ids) = last.expect("the loop runs at least once");
    (list, ui, ids)
}

/// The same with a preview on the map and a state of the caller's.
pub fn picture_with(theme: Theme, size: Vec2, world: Option<&World>, state: PreviewState) -> (DrawList, Ui, ShellIds) {
    let mut ui = Ui::new(theme);
    let mut s = state;
    let mut last = None;
    for _ in 0..3 {
        ui.begin_frame(size);
        let ids = build(&mut ui, &mut s, size, world, None, None);
        let list = ui.end_frame().clone();
        last = Some((list, ids));
        if !ui.want_redraw() {
            break;
        }
    }
    let (list, ids) = last.expect("the loop runs at least once");
    (list, ui, ids)
}
