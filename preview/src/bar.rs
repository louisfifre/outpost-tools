//! The launcher's `#titlebar` (launcher/index.html lines 75-129 and 221-245), built by the preview as the
//! window's host: the game's name on the left, the tools in the middle with the preview's own tabs after
//! the preview tool, the account and the window's buttons on the right. Step 21 moves this to the launcher; until then the
//! preview is the only application, so it hosts its own bar.
//!
//! The icon paths below come from the launcher page (the line numbers are its), which draws them after
//! Pixelarticons (MIT): see THIRD-PARTY.md.
//!
//! The bar has no fixed breakpoint, and it never turns a word into an icon (the page's `condense()` is not built).
//! When it lacks room it gives parts up in a set order, worked out from the real widths in `overflow.rs`: the
//! account name, the greyed views (into "More"), the brand's name, then the other views one by one. The account is
//! given up before the views in the preview tool.

use std::sync::OnceLock;

use mere_base::{Cmd, Color, Rect, Vec2};
use std::sync::Arc;

use mere_layout::{Align, Dim, Display, Edges, Insets, Justify, Margin, Overflow, Position, Side, Style, Track, Tracks};
use mere_ui::tokens::{CONTROL, FRAME, GAP_TIGHT, INSET, INSET_TIGHT};
use mere_ui::{Icon, IconLook, IconOpts, Rule, TabMenuRow, Slide, TabLook, TabOpts, TextLook, Tone, Ui, strip_style};

use crate::home::{AccountAsk, AccountMenu, Pixels};
use crate::overflow::{self, Form, VIEW_COUNT};
use crate::run::Drawn;
use crate::shell::{BAR_H, Mode, PreviewState, RunEnd, ShellIds, Want};
use crate::text;
use crate::world::World;

/// Line 229.
pub const HOME: &str = "M4 20h16v2H4zm16-10h2v10h-2zM2 10h2v10H2zm2-2h2v2H4zm2-2h2v2H6zm2-2h2v2H8zm2-2h4v2h-4zm4 2h2v2h-2zm2 2h2v2h-2zm2 2h2v2h-2zM8 14h2v6H8zm2-2h4v2h-4zm4 2h2v6h-2z";
/// Line 234.
pub const PREVIEW: &str = "M4 20h2v2H2V6h2v14Zm12 0h2v2h-4v-2h-2v-2h2V8h-2V6h4v14Zm-8 0H6v-2h2v2Zm12 0h-2v-2h2v2ZM10 4h2v2h-2v10h2v2H8V4H6V2h4v2Zm12 14h-2V4h-2V2h4v16ZM6 6H4V4h2v2Zm12 0h-2V4h2v2Z";
/// Line 240.
pub const MINIMIZE: &str = "M5 11h14v2H5z";
/// Line 735.
pub const MAXIMIZE: &str = "M4 4h16v2H4zm0 14h16v2H4zM4 6h2v12H4zm14 0h2v12h-2z";
/// Line 736.
pub const RESTORE: &str = "M8 4h12v2H8zm10 2h2v8h-2zm-2 6h2v2h-2zM8 6h2v2H8zM4 8h12v2H4zm0 10h12v2H4zM4 10h2v8H4zm10 0h2v8h-2z";
/// Line 242.
pub const CLOSE: &str = "M5 5h2v2H5zm2 2h2v2H7zm2 2h2v2H9zm2 2h2v2h-2zm2-2h2v2h-2zm2-2h2v2h-2zm2-2h2v2h-2zM9 13h2v2H9zm-2 2h2v2H7zm-2 2h2v2H5zm8-4h2v2h-2zm2 2h2v2h-2zm2 2h2v2h-2z";
/// Lines 427-432: the views, by key (biomes, height, temperature, structures, section).
pub const VIEW_ICONS: [(&str, &str); 5] = [
    ("biomes", "M8 2h8v2H8zM6 4h12v8H6zM8 12h8v2H8zM11 14h2v6h-2zM5 20h14v2H5z"),
    ("height", "M10 4h4v2h-4zM8 6h2v2H8zm6 0h2v2h-2zM6 8h2v4H6zm10 0h2v4h-2zM4 12h2v4H4zm14 0h2v4h-2zM2 16h2v4H2zm18 0h2v4h-2zM2 20h20v2H2z"),
    ("temperature", "M10 2h4v2h-4zM8 4h2v10H8zm6 0h2v10h-2zM6 14h2v6H6zm10 0h2v6h-2zM8 20h8v2H8zM11 8h2v8h-2zM9 16h6v2H9z"),
    ("structures", "M4 2h4v2h2V2h4v2h2V2h4v6H4zM6 8h2v12H6zm10 0h2v12h-2zM10 14h4v6h-4zM2 20h20v2H2z"),
    ("section", "M2 4h20v2H2zM2 18h20v2H2zM2 6h2v12H2zm18 0h2v12h-2zM4 12h4v2H4zm4-2h4v2H8zm4 2h4v2h-4zm4-4h4v2h-4z"),
];
/// The account button's chevron (an 8 by 5 grid), the check beside the account in use and the plus of "Add an account".
pub const CHEVRON: &str = "M0 0h2v1H0zM6 0h2v1H6zM0 1h3v1H0zM5 1h3v1H5zM1 2h6v1H1zM2 3h4v1H2zM3 4h2v1H3z";
pub const CHECK: &str = "M4 12h2v2H4zM6 14h2v2H6zM8 16h2v2H8zM10 14h2v2h-2zM12 12h2v2h-2zM14 10h2v2h-2zM16 8h2v2h-2zM18 6h2v2h-2z";
pub const PLUS: &str = "M11 4h2v7h7v2h-7v7h-2v-7H4v-2h7z";
/// Line 418.
pub const PLAY: &str = "M15 11h-2V9h2zm0 4h-2v-2h2zm-2 2h-2v-2h2zm0-8h-2V7h2zm-2-2H9V5h2zM9 21H7V3h2zm6-8h2v-2h-2zm-6 4h2v2H9z";
/// Line 419.
pub const STOP: &str = "M6 6h12v12H6z";
/// Line 420.
pub const SETTINGS: &str = "M8 14H7v6H5v-6H2v-2h6v2Zm5 6h-2V10h2v10Zm9-2h-3v2h-2v-2h-1v-2h6v2Zm-3-4h-2V4h2v10ZM7 10H5V4h2v6Zm6-4h2v2H9V6h2V4h2v2Z";
/// The two panels' buttons, in the same 24 unit grid and 2 px strokes as the icons above: a frame (3 to 21 across,
/// 5 to 19 down) split by a bar near its left edge (the settings panel) or near its right edge (the figures panel):
/// the panel's place drawn empty while it is hidden, and filled while it is shown, so the icon says the state.
pub const PANEL_LEFT: &str = "M3 5h18v2H3zm0 12h18v2H3zM3 7h2v10H3zm16 0h2v10h-2zM8 7h2v10H8z";
pub const PANEL_RIGHT: &str = "M3 5h18v2H3zm0 12h18v2H3zM3 7h2v10H3zm16 0h2v10h-2zM14 7h2v10h-2z";
pub const PANEL_LEFT_SHOWN: &str = "M3 5h18v2H3zm0 12h18v2H3zM3 7h2v10H3zm16 0h2v10h-2zM5 7h5v10H5z";
pub const PANEL_RIGHT_SHOWN: &str = "M3 5h18v2H3zm0 12h18v2H3zM3 7h2v10H3zm16 0h2v10h-2zM14 7h5v10h-5z";
/// Line 223: the pack's cube, in a 32 unit grid, each face in its own colour (`fill`).
pub const PACK: [(&str, u32); 3] = [("M16 2l14 7-14 7L2 9z", 0x7fb850), ("M2 9l14 7v14L2 23z", 0x8b6344), ("M30 9l-14 7v14l14-7z", 0x6b4b33)];

/// The icons, parsed once.
pub struct Icons {
    pub home: Icon,
    pub preview: Icon,
    pub minimize: Icon,
    pub maximize: Icon,
    pub restore: Icon,
    pub close: Icon,
    pub views: Vec<Icon>,
    pub play: Icon,
    pub stop: Icon,
    pub settings: Icon,
    pub chevron: Icon,
    pub check: Icon,
    pub plus: Icon,
    /// The buttons of the settings panel and of the figures panel, each hidden then shown.
    pub panels: [[Icon; 2]; 2],
    /// The three faces of the pack's cube with their colours.
    pub pack: Vec<(Icon, Color)>,
}

/// The path constants are the launcher's own and parse, which a test checks; a failure here would be a typo made
/// in copying, so it is a logged empty icon rather than a panic.
fn parse(d: &str, grid: f32) -> Icon {
    Icon::parse(d, grid).unwrap_or_else(|e| {
        mere_base::observe!(Error, "icon path '{d}': {e}");
        Icon { grid, figures: Vec::new() }
    })
}

pub fn icons() -> &'static Icons {
    static ICONS: OnceLock<Icons> = OnceLock::new();
    ICONS.get_or_init(|| Icons {
        home: parse(HOME, 24.0),
        preview: parse(PREVIEW, 24.0),
        minimize: parse(MINIMIZE, 24.0),
        maximize: parse(MAXIMIZE, 24.0),
        restore: parse(RESTORE, 24.0),
        close: parse(CLOSE, 24.0),
        views: VIEW_ICONS.iter().map(|(_, d)| parse(d, 24.0)).collect(),
        play: parse(PLAY, 24.0),
        stop: parse(STOP, 24.0),
        settings: parse(SETTINGS, 24.0),
        chevron: parse(CHEVRON, 8.0),
        check: parse(CHECK, 24.0),
        plus: parse(PLUS, 24.0),
        panels: [[parse(PANEL_LEFT, 24.0), parse(PANEL_LEFT_SHOWN, 24.0)], [parse(PANEL_RIGHT, 24.0), parse(PANEL_RIGHT_SHOWN, 24.0)]],
        pack: PACK.iter().map(|(d, c)| (parse(d, 32.0), Color::hex(*c))).collect(),
    })
}

/// Which marker the preview tool carries (renderModes, lines 784-787): running while a run is under way, else the
/// way the last one ended while the person has not looked at the preview since.
pub fn marker(s: &PreviewState) -> Option<usize> {
    if s.follow.is_some() {
        Some(0)
    } else if s.preview_seen {
        None
    } else {
        match s.ended {
            RunEnd::Ready => Some(1),
            RunEnd::Failed => Some(2),
            RunEnd::Idle => None,
        }
    }
}

/// What the window's title says (line 794).
pub fn window_title(s: &PreviewState) -> String {
    match marker(s) {
        Some(m) => text::title_with(text::PREVIEW_STATE[m]),
        None => text::BAR_TITLE.to_string(),
    }
}

/// The views that cannot be shown for this world: Steepness, Structures and Section come after the full map only (line 2506).
/// While the stage shows a run's own picture, Structures and Section are greyed until the run has made them.
fn greyed(s: &PreviewState, world: Option<&World>) -> [bool; VIEW_COUNT] {
    let coarse = world.is_some_and(World::coarse);
    let pending = pending(s);
    std::array::from_fn(|i| (coarse && crate::paint::needs_full(i)) || (pending && crate::paint::comes_at_the_end(i)))
}

/// The stage shows the run's sketch or map, which have no structures and no cuts yet.
fn pending(s: &PreviewState) -> bool {
    s.follow.as_ref().is_some_and(|f| f.drawn != Drawn::Nothing)
}

/// What the bar shows at a window `width` wide: see `overflow.rs`. The views' arrangement is the one of the preview
/// tool open, whatever tool is on, so the views do not shuffle while they slide; the brand's parts follow the
/// tool on show, so Home, which has the room, keeps the account and the word.
pub fn form_for(s: &PreviewState, world: Option<&World>, width: f32) -> (Form, Form) {
    let inputs = |preview_open| overflow::Inputs { width, account: s.account.as_deref(), account_mark: if s.account_menu.accounts.is_empty() { 0.0 } else { overflow::ACCOUNT_MARK }, preview_open, pressed: s.view, disabled: greyed(s, world) };
    let open = overflow::plan(&inputs(true));
    let on_show = if s.mode == Mode::Preview { open.clone() } else { overflow::plan(&inputs(false)) };
    (open, on_show)
}

/// Builds the bar and fills `ids` with where its parts are. `width` is the window's.
pub fn build(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, world: Option<&World>, width: f32) {
    let t = *ui.theme();
    let icons = icons();
    let marker = marker(s);
    // A state that names a tool without the slide that goes with it (a test, a host) is put right: the options
    // are open only for the preview.
    if s.mode == Mode::Home && s.options == Slide::settled(true) {
        s.options = Slide::settled(false);
    } else if s.mode == Mode::Preview && s.options == Slide::settled(false) {
        s.options = Slide::settled(true);
    }
    let now = ui.clock();
    let value = s.options.value(now, s.reduce_motion);
    let (views_form, form) = form_for(s, world, width);
    ids.form = Form { more: views_form.more.clone(), shapes_in_more: views_form.shapes_in_more, ..form.clone() };
    let preview = s.mode == Mode::Preview;
    // The bar is a grid of three columns with a free space on each side of the tools, so the tools stay in the
    // middle whatever the width (line 76); when the bar is too narrow to centre them they sit left, after the
    // settings button, and the middle column takes what is left. The launcher page has a 1 px border-bottom on
    // #titlebar (line 77); the native bar drops it by the maintainer's choice (2026-11-15: the header should float,
    // no separating line), a deliberate departure from the page (the launcher's own page is not changed).
    // The 1 px bottom padding is kept so the parts stay where the page centres them above its border: removing it
    // would move them 0.5 px.
    let cols = if form.centred { [Track::MinMaxFr0(1.0), Track::Auto, Track::MinMaxFr0(1.0)] } else { [Track::Auto, Track::MinMaxFr0(1.0), Track::Auto] };
    let bar = Style {
        display: Display::Grid,
        cols: Tracks::new(&cols),
        align_items: Align::Center,
        h: Dim::Px(BAR_H),
        shrink: 0.0,
        pad: Edges::new(0.0, 0.0, 1.0, 0.0),
        position: Position::Relative,
        ..Style::default()
    };
    ids.bar = ui.node("bar", bar, |ui| {
        if preview {
            // In the preview tool the settings panel's button is where the brand is in Home. When the tools sit left
            // the cell keeps `TOOLS_GAP` after the button, and the tools start there.
            let lead = Style { pad: Edges::new(0.0, if form.centred { 0.0 } else { overflow::TOOLS_GAP }, 0.0, overflow::TOGGLE_EDGE), justify_self: Some(Align::Start), ..Style::default() };
            ui.node("lead", lead, |ui| panel_toggle(ui, s, ids, 0));
        }
        // `.brand` (lines 81-82): the cube and the name in capitals, 12 px in from the left, 10 px apart.
        // Its own width, not the column's: the page's stretches it over the free space, which nothing can see.
        let brand = Style { align_items: Align::Center, gap: Vec2::new(10.0, 0.0), pad: Edges::new(0.0, 0.0, 0.0, 12.0), min_w: Dim::Px(0.0), max_w: Dim::Pct(100.0), justify_self: Some(Align::Start), ..Style::default() };
        if !preview {
            ids.brand = ui.node("brand", brand, |ui| {
                let cube = Style { w: Dim::Px(16.0), h: Dim::Px(16.0), shrink: 0.0, ..Style::default() };
                ui.canvas("pack", cube, |_, _| {
                    icons.pack.iter().map(|(i, c)| Cmd::Shape { rect: Rect::new(0.0, 0.0, 16.0, 16.0), grid: i.grid, figures: i.figures.clone(), color: *c, crisp: false }).collect()
                });
                if form.word {
                    ui.text(&text::BAR_TITLE.to_uppercase(), overflow::brand_look());
                }
            });
        }

        // `.strip` (line 109): the tools, one box wide each, then the preview's tabs after its own tool. Sat left
        // when the bar cannot centre them.
        let strip = Style { justify_self: if form.centred { None } else { Some(Align::Start) }, ..strip_style() };
        ids.strip = ui.node("tools", strip, |ui| {
            // Set-up: the cell stays, so that the buttons keep their column, and holds nothing.
            if s.bare {
                return;
            }
            let tool = |ui: &mut Ui, ids: &mut ShellIds, s: &mut PreviewState, which: usize| {
                let (mode, icon) = if which == 0 { (Mode::Home, &icons.home) } else { (Mode::Preview, &icons.preview) };
                let word = if which == 1 { marker.map(|m| text::PREVIEW_STATE[m]) } else { None };
                let dot = word.map(|_| if marker == Some(2) { t.warn } else { t.text });
                let r = ui.tool_action(&format!("tool{which}"), icon, text::TOOL_NAMES[which], IconOpts { pressed: s.mode == mode, disabled: false, joined: false, marker: dot });
                ids.tools[which] = r.id;
                ids.bar_buttons.push(r.id);
                let name = text::MODES[which];
                ui.title(r.id, &word.map_or(name.to_string(), |w| text::mode_with(name, w)));
                // `showMode` (line 797): the same mode does nothing; the preview, once shown, has been seen.
                if r.clicked && s.mode != mode {
                    s.mode = mode;
                    s.options.toward(mode == Mode::Preview, now, s.reduce_motion);
                    if mode == Mode::Preview {
                        s.preview_seen = true;
                    }
                }
            };
            ui.node("home", Style::default(), |ui| tool(ui, ids, s, 0));
            ui.strip_gap();
            ui.node("preview", Style::default(), |ui| {
                tool(ui, ids, s, 1);
                // A closed tool's choices are 0 wide and not built; while they slide they are built at their own width
                // inside a box that is cut to the share the slide has reached (`.options`, lines 124-127).
                if s.mode == Mode::Preview || value > 0.0 {
                    let settled = s.reduce_motion || (value >= 1.0 && !s.options.moving(now, false));
                    let outer = if settled {
                        Style::default()
                    } else {
                        Style { overflow: Overflow::Hidden, min_w: Dim::Px(0.0), shrink: 0.0, w: Dim::Px(value * s.options_w), ..Style::default() }
                    };
                    let mut inner = None;
                    ids.options = ui.node("options", outer, |ui| {
                        // `gap-tight` after the tool: the tool and a pressed first view are both filled, and touching
                        // they would read as one box with the view's word off its centre.
                        let pad = Edges::new(0.0, 0.0, 0.0, 1.0 + overflow::TOOL_VIEWS_GAP);
                        inner = Some(ui.node("inner", Style { pad, shrink: 0.0, ..Style::default() }, |ui| tabs(ui, s, ids, world, &views_form)));
                    });
                    if let Some(r) = inner.and_then(|id| ui.rect(id)) {
                        s.options_w = r.w;
                    }
                }
            });
        });

        // The right cell: in the preview tool the figures panel's button, then the account, each 8 px before what follows,
        // and last the window's buttons, which are `.ends` (line 89), as tall as the bar and flush with its corner.
        // Like the brand's, its width is its own, capped at the column's, so that a long account name is cut.
        let right = Style { justify_self: Some(Align::End), align_self: Some(Align::Stretch), align_items: Align::Center, gap: Vec2::new(overflow::TOGGLE_GAP, 0.0), min_w: Dim::Px(0.0), max_w: Dim::Pct(100.0), ..Style::default() };
        ui.node("right", right, |ui| {
            if preview {
                panel_toggle(ui, s, ids, 1);
            }
            // `#account` (line 84): muted, cut with an ellipsis when the free space beside the tools is narrower than it,
            // and gone whole when even a short stub no longer fits (`overflow.rs`). Set-up shows none.
            if let (true, Some(a)) = (form.account && !s.bare, s.account.clone()) {
                if s.account_menu.accounts.is_empty() {
                    ui.node("accountName", Style { min_w: Dim::Px(0.0), ..Style::default() }, |ui| ui.ellipsis(&a, TextLook::body(Tone::Muted)));
                } else {
                    account_button(ui, s, ids, &a);
                }
            }
            let ends = Style { align_items: Align::Center, pad: Edges::new(0.0, overflow::EDGE, 0.0, 0.0), ..Style::default() };
            ui.node("ends", ends, |ui| {
                let r = ui.icon_action("minimize", &icons.minimize, IconLook::Caption, IconOpts::default());
                ids.ends[0] = r.id;
                ui.title(r.id, text::MINIMIZE);
                if r.clicked {
                    s.want = Some(Want::Minimize);
                }
                let (icon, word) = if s.maximized { (&icons.restore, text::RESTORE) } else { (&icons.maximize, text::MAXIMIZE) };
                let r = ui.icon_action("maximize", icon, IconLook::Caption, IconOpts::default());
                ids.ends[1] = r.id;
                ui.title(r.id, word);
                if r.clicked {
                    s.want = Some(Want::Maximize);
                }
                let r = ui.icon_action("close", &icons.close, IconLook::Close, IconOpts::default());
                ids.ends[2] = r.id;
                ui.title(r.id, text::CLOSE_WINDOW);
                if r.clicked {
                    s.want = Some(Want::Close);
                }
                ids.bar_buttons.extend(ids.ends);
            });
        });
    });
}

/// The account menu's width: 232, on the 4 px grid.
const ACCOUNT_MENU_W: f32 = 232.0;
/// The player in the menu: the 24 by 40 picture, four times over.
const FIGURE_SCALE: f32 = 4.0;

/// The pixels of a picture as filled boxes, one per run of a colour in a row, `scale` times over.
fn pixel_cmds(p: &Pixels, scale: f32) -> Vec<Cmd> {
    let mut cmds = Vec::new();
    let colour = |x: u32, y: u32| {
        let at = ((y * p.w + x) * 4) as usize;
        p.rgba.get(at..at + 4).filter(|px| px[3] > 0).map(|px| Color { r: px[0] as f32 / 255.0, g: px[1] as f32 / 255.0, b: px[2] as f32 / 255.0, a: px[3] as f32 / 255.0 })
    };
    for y in 0..p.h {
        let mut x = 0;
        while x < p.w {
            let Some(c) = colour(x, y) else {
                x += 1;
                continue;
            };
            let mut end = x + 1;
            while end < p.w && colour(end, y) == Some(c) {
                end += 1;
            }
            cmds.push(Cmd::Fill { rect: Rect::new(x as f32 * scale, y as f32 * scale, (end - x) as f32 * scale, scale), color: c });
            x = end;
        }
    }
    cmds
}

/// A picture of pixels in a box of `w` by `h`: the face of an account, or the player.
fn picture(ui: &mut Ui, key: &str, p: Option<&Pixels>, w: f32, h: f32, scale: f32) {
    let style = Style { w: Dim::Px(w), h: Dim::Px(h), shrink: 0.0, ..Style::default() };
    ui.canvas(key, style, |_, _| p.map(|p| pixel_cmds(p, scale)).unwrap_or_default());
}

/// A pixel icon in a box of `w` by `h`, `w` wide on its grid.
fn glyph(ui: &mut Ui, key: &str, icon: &Icon, w: f32, h: f32, color: Color) {
    let style = Style { w: Dim::Px(w), h: Dim::Px(h), shrink: 0.0, ..Style::default() };
    ui.canvas(key, style, |_, _| vec![Cmd::Shape { rect: Rect::new(0.0, 0.0, w, w), grid: icon.grid, figures: icon.figures.clone(), color, crisp: true }]);
}

/// A line of the account menu: 32 px tall, its parts `GAP` apart.
fn account_line() -> Style {
    Style { align_items: Align::Center, gap: Vec2::new(mere_ui::tokens::GAP, 0.0), h: Dim::Px(32.0), pad: Edges::vh(0.0, INSET), ..Style::default() }
}

/// The account's button before the window's buttons (board A2): the face, the name and a chevron. It opens the menu of
/// the accounts, the player and the sign-out: a framed box under the button, aligned to its right edge, drawn after the
/// tree, that holds the kit's lines
/// (`hover_action`). The application shuts it (a pick, Escape, a press outside: `account_closes`).
fn account_button(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, name: &str) {
    let t = *ui.theme();
    let icons = icons();
    let menu: Arc<AccountMenu> = s.account_menu.clone();
    let open = s.account_open;
    let face = menu.accounts.iter().find(|a| a.in_use).and_then(|a| a.face.as_ref());
    let place = Style { position: Position::Relative, min_w: Dim::Px(0.0), ..Style::default() };
    ui.node("accountPlace", place, |ui| {
        let line = Style { align_items: Align::Center, gap: Vec2::new(mere_ui::tokens::GAP, 0.0), min_w: Dim::Px(0.0), h: Dim::Px(CONTROL), pad: Edges::vh(0.0, INSET), ..Style::default() };
        let button = ui.hover_action("account", line, t.tint(), |ui| {
            picture(ui, "face", face, 16.0, 16.0, 2.0);
            ui.ellipsis(name, TextLook::body(if open { Tone::Text } else { Tone::Muted }));
            glyph(ui, "chevron", &icons.chevron, 8.0, 5.0, if open { t.text } else { t.muted });
        });
        ids.account = button.id;
        ids.bar_buttons.push(button.id);
        ui.title(button.id, text::ACCOUNT_MENU);
        if button.clicked {
            s.account_open = !open;
            if s.account_open {
                s.account_ask = Some(AccountAsk::Opened);
            }
        }
        if !s.account_open {
            return;
        }
        let zero = Dim::Px(0.0);
        let style = Style {
            position: Position::Absolute,
            inset: Insets { right: zero, top: Dim::Pct(100.0), ..Insets::default() },
            margin: Margin { top: Side::Px(GAP_TIGHT), ..Margin::default() },
            w: Dim::Px(ACCOUNT_MENU_W),
            column: true,
            pad: Edges::vh(FRAME + INSET_TIGHT, FRAME),
            ..Style::default()
        };
        let mut ask = None;
        let mut close = false;
        ids.account_list = ui.overlay("accountList", style, |ui| {
            for (i, a) in menu.accounts.iter().enumerate() {
                let r = ui.hover_action(&format!("account{i}"), account_line(), t.tint(), |ui| {
                    picture(ui, "face", a.face.as_ref(), 16.0, 16.0, 2.0);
                    ui.node("name", Style { grow: 1.0, min_w: Dim::Px(0.0), ..Style::default() }, |ui| ui.ellipsis(&a.name, TextLook::body(Tone::Text)));
                    if a.in_use {
                        let mark = ui.icon("inUse", &icons.check, 16.0, t.text, true);
                        ui.title(mark, text::IN_USE);
                    }
                });
                if r.clicked {
                    close = true;
                    if !a.in_use {
                        ask = Some(AccountAsk::Use(a.name.clone()));
                    }
                }
            }
            ui.rule(Rule::Separator, Edges::vh(INSET_TIGHT, 0.0));
            let r = ui.hover_action("add", account_line(), t.tint(), |ui| {
                ui.icon("plus", &icons.plus, 16.0, t.text, true);
                ui.text(text::ADD_ACCOUNT, TextLook::body(Tone::Text));
            });
            if r.clicked {
                close = true;
                ask = Some(AccountAsk::Add);
            }
            ui.rule(Rule::Separator, Edges::vh(INSET_TIGHT, 0.0));
            if let Some(figure) = &menu.figure {
                let (w, h) = (figure.w as f32 * FIGURE_SCALE, figure.h as f32 * FIGURE_SCALE);
                let around = Style { justify: Justify::Center, pad: Edges::vh(mere_ui::tokens::GAP_WIDE, 0.0), ..Style::default() };
                ui.node("player", around, |ui| picture(ui, "figure", Some(figure), w, h, FIGURE_SCALE));
                ui.rule(Rule::Separator, Edges::vh(0.0, INSET_TIGHT));
            }
            if let Some(a) = menu.accounts.iter().find(|a| a.in_use) {
                let r = ui.hover_action("signOut", account_line(), t.tint(), |ui| ui.text(&text::sign_out(&a.name), TextLook::body(Tone::Muted)));
                if r.clicked {
                    close = true;
                    ask = Some(AccountAsk::SignOut(a.name.clone()));
                }
            }
        });
        ids.bar_buttons.push(ids.account_list);
        if close {
            s.account_open = false;
        }
        if ask.is_some() {
            s.account_ask = ask;
        }
    });
}

/// The account menu shuts on Escape and on a press outside it and its button, which reaches nothing else, as an open
/// list of the kit does. True when the press or the key was taken.
pub fn account_closes(s: &mut PreviewState, ui: &Ui, ids: &ShellIds, e: &mere_base::Event) -> bool {
    if !s.account_open {
        return false;
    }
    let outside = |pos: Vec2| ![ids.account, ids.account_list].iter().any(|&id| ui.rect(id).is_some_and(|r| r.contains(pos)));
    let shut = match e {
        mere_base::Event::Key { key: mere_base::Key::Escape, .. } => true,
        mere_base::Event::PointerDown { pos, .. } => outside(*pos),
        _ => false,
    };
    if shut {
        s.account_open = false;
    }
    shut
}

/// The button that opens and closes a side panel (0: settings, 1: figures): pressed while its panel is open, and
/// toggling its own panel only.
fn panel_toggle(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, which: usize) {
    let name = if which == 0 { text::SETTINGS_PANEL } else { text::FIGURES_PANEL };
    // The window buttons' look, unframed; the icon, not a reversed box, says whether the panel is shown.
    let icon = &icons().panels[which][s.panel_open[which] as usize];
    let r = ui.icon_action(&format!("panel{which}"), icon, IconLook::Caption, IconOpts::default());
    ids.toggles[which] = r.id;
    ids.bar_buttons.push(r.id);
    ui.title(r.id, name);
    if r.clicked {
        s.panel_open[which] = !s.panel_open[which];
    }
}

/// The preview's tabs as the page sends them to its host (lines 749-754): the six views, a gap before the last two
/// (the structures and the section), then a gap and 2D and 3D, disabled while the section is pressed.
/// A disabled view carries the page's `needsFull` as its title, an enabled one its label.
/// The views the form holds back are not tabs: they are the rows of "More", at the end of the views.
fn tabs(ui: &mut Ui, s: &mut PreviewState, ids: &mut ShellIds, world: Option<&World>, form: &Form) {
    let off = greyed(s, world);
    let section = s.view == crate::paint::SECTION;
    let why = if pending(s) && !world.is_some_and(World::coarse) { text::COMES_AT_THE_END } else { text::NEEDS_FULL };
    for i in (0..VIEW_COUNT).filter(|i| !form.more.contains(i)) {
        if overflow::gap_before(form, i) {
            ui.strip_gap();
        }
        let word = overflow::view_word(i);
        let disabled = off[i];
        let r = ui.tab(&format!("view{i}"), word, TabLook::Strip, TabOpts { pressed: i == s.view, disabled });
        ids.tabs.push(r.id);
        ids.bar_buttons.push(r.id);
        // A view with something to explain says it on hover, in the page's words.
        let hint = text::VIEW_HINTS.get(i).copied().filter(|h| !h.is_empty());
        ui.title(r.id, if disabled { why } else { hint.unwrap_or(word) });
        // `inert` while the tool is not the one on screen (the page's closing slide).
        if r.clicked && s.mode == Mode::Preview {
            s.view = i;
        }
    }
    if form.has_more() {
        // The views the strip lacks room for, then, when the bar is narrower still, 2D and 3D.
        let mut rows: Vec<TabMenuRow> = form.more.iter().map(|&v| TabMenuRow { text: overflow::view_word(v).to_string(), disabled: off[v] }).collect();
        if form.shapes_in_more {
            rows.extend(text::SHAPES.iter().map(|w| TabMenuRow { text: w.to_string(), disabled: section }));
        }
        let r = ui.tab_menu("more", text::MORE, &rows);
        ids.more = r.resp.id;
        ids.bar_buttons.push(r.resp.id);
        ui.title(r.resp.id, text::MORE_TITLE);
        if let (Some(row), true) = (r.picked, s.mode == Mode::Preview) {
            match form.more.get(row) {
                Some(&view) => s.view = view,
                None => s.in3d = row - form.more.len() == 1,
            }
        }
    }
    // The section has no 2D or 3D (line 2540): the tabs stay in place, disabled while it is pressed.
    if !form.shapes_in_more {
        ui.strip_gap();
        for (i, word) in text::SHAPES.iter().enumerate() {
            let r = ui.tab(&format!("shape{i}"), word, TabLook::Strip, TabOpts { pressed: (i == 1) == s.in3d, disabled: section });
            ids.tabs.push(r.id);
            ids.bar_buttons.push(r.id);
            ui.title(r.id, word);
            if r.clicked && s.mode == Mode::Preview {
                s.in3d = i == 1;
            }
        }
    }
}
