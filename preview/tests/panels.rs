//! The side panels' behaviour: where the page stacks them, how they shrink, what a hand does to their width, what a
//! closed panel gives back, that a stacked panel scrolls as one whole, and the bar's two buttons. Layout is judged
//! on the real rects of a built page, and the hand through the events a window sends.


use mere_base::{App, Button, Cmd, Color, Cursor, Event, Rect, Vec2};
use mere_ui::{Id, Theme};
use mere_win32::Renderer;
use preview::panels::{self, GAP, GAP_STACKED, MAP_MIN, PANEL_MAX, PANEL_MIN, STACK_AT, beside_min};
use preview::world::World;
use preview::{BAR_H, CONTENT_TOP, PreviewApp, PreviewState, ShellIds, picture_with};

const H: f32 = 800.0;

fn world() -> Option<World> {
    let dir = &preview::world::data_dir();
    dir.join("preview.json").exists().then(|| World::load(dir).expect("the data loads"))
}

/// The app and its offscreen renderer at a size, driven by events and frames as the window would.
struct Rig {
    app: PreviewApp,
    r: Renderer,
    theme: Theme,
    w: u32,
    h: u32,
}

impl Rig {
    fn new(w: u32, h: u32, mut state: PreviewState, world: Option<&World>) -> Rig {
        state.reduce_motion = true;
        let theme = Theme::dark();
        let mut r = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
        let mut app = PreviewApp::new(theme, true, Vec2::new(w as f32, h as f32), state, world.cloned());
        app.start(&mut r);
        app.event(Event::Resized { width_px: w, height_px: h, scale: 1.0 });
        let mut rig = Rig { app, r, theme, w, h };
        rig.frame();
        rig
    }

    fn frame(&mut self) {
        self.app.frame(&mut self.r, 0.0);
        self.app.frame(&mut self.r, 0.0);
    }

    fn resize(&mut self, w: u32) {
        self.w = w;
        self.r = Renderer::offscreen(w, self.h, 1.0).expect("offscreen renderer");
        self.app.start(&mut self.r);
        self.app.event(Event::Resized { width_px: w, height_px: self.h, scale: 1.0 });
        self.frame();
    }

    fn ids(&self) -> ShellIds {
        self.app.ids().clone()
    }

    fn rect(&self, id: Id) -> Rect {
        assert_ne!(id, Id::ROOT, "the widget is not built");
        self.app.ui().rect(id).expect("the widget was built")
    }

    fn state(&self) -> &PreviewState {
        self.app.state()
    }

    fn move_to(&mut self, at: Vec2) {
        self.app.event(Event::PointerMoved { pos: at });
        self.frame();
    }

    fn down(&mut self, at: Vec2, clicks: u8) {
        self.app.event(Event::PointerMoved { pos: at });
        self.app.event(Event::PointerDown { pos: at, button: Button::Left, clicks });
        self.frame();
    }

    fn up(&mut self, at: Vec2) {
        self.app.event(Event::PointerUp { pos: at, button: Button::Left });
        self.frame();
    }

    fn click_on(&mut self, id: Id) {
        let c = centre(self.rect(id));
        self.down(c, 1);
        self.up(c);
    }

    /// A drag of a panel's grip by `dx`, ending with the button up; returns the panel's width then.
    fn drag(&mut self, side: usize, dx: f32) -> f32 {
        let from = centre(self.rect(self.ids().grips[side]));
        self.down(from, 1);
        self.move_to(Vec2::new(from.x + dx, from.y));
        self.up(Vec2::new(from.x + dx, from.y));
        self.panel_w(side)
    }

    fn panel_w(&self, side: usize) -> f32 {
        let ids = self.ids();
        self.rect(if side == 0 { ids.left } else { ids.right }).w
    }

    /// The pixel at (x, y) of the last frame.
    fn px(&mut self, x: f32, y: f32) -> [u8; 3] {
        let rgba = self.r.read_rgba().unwrap();
        let i = (y as usize * self.w as usize + x as usize) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    }

    /// How many pixels of a rect differ from the ground: the button is unframed, so this is its icon's ink, and a
    /// filled icon has more than an empty one.
    fn ink(&mut self, r: Rect) -> usize {
        let rgba = self.r.read_rgba().unwrap();
        let ground = rgb(self.theme.ground);
        let mut n = 0;
        for y in r.y as usize..(r.y + r.h) as usize {
            for x in r.x as usize..(r.x + r.w) as usize {
                let i = (y * self.w as usize + x) * 4;
                if [rgba[i], rgba[i + 1], rgba[i + 2]] != ground {
                    n += 1;
                }
            }
        }
        n
    }
}

fn centre(r: Rect) -> Vec2 {
    Vec2::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn rgb(c: Color) -> [u8; 3] {
    [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8)
}

fn state() -> PreviewState {
    PreviewState::default()
}

fn plan_of(w: f32, open: [bool; 2]) -> panels::Plan {
    panels::plan(w, open, [None, None])
}

/// The panels at their natural width: the widest row of each laid out on one line, under the maximum.
#[test]
fn the_natural_widths_are_the_widest_rows_under_the_maximum() {
    let [left, right] = panels::natural();
    assert!((PANEL_MIN..=PANEL_MAX).contains(&left) && (PANEL_MIN..=PANEL_MAX).contains(&right), "{left} {right}");
    // "Lanterns over underground rivers": 32 cells of 7.2 px, the two gaps and the field, and the scrollbar's gutter.
    let widest = mere_ui::TextLook::body(mere_ui::Tone::Text).width("Lanterns over underground rivers") + 2.0 * panels::SETTING_GAP + panels::setting_field() + mere_ui::GUTTER;
    assert_eq!(left, widest.ceil());
    // That row is the widest label of the settings.
    for s in preview::settings::SETTINGS.iter() {
        assert!(s.label.chars().count() <= "Lanterns over underground rivers".chars().count(), "{}", s.label);
    }
}

/// A label wider than the cell it has wraps onto more lines instead of being cut: at the panel's minimum, the long
/// labels take two lines and keep every word.
#[test]
fn a_label_wider_than_its_cell_wraps() {
    let (list, ui, ids) = {
        let st = PreviewState { panel_w: [Some(PANEL_MIN), None], ..state() };
        picture_with(Theme::dark(), Vec2::new(1400.0, H), None, st)
    };
    let long = preview::settings::SETTINGS.iter().position(|s| s.label == "Lanterns over underground rivers").unwrap();
    let row = ui.rect(ids.rows[long]).expect("the row is built");
    let one_line = ui.rect(ids.rows[0]).expect("a short row").h;
    assert!(row.h > one_line + 10.0, "the long label took more lines: {} against {}", row.h, one_line);
    let words: Vec<String> = list.cmds.iter().filter_map(|c| if let Cmd::Text { text, .. } = c { Some(text.clone()) } else { None }).collect();
    for word in ["Lanterns", "over", "underground", "rivers"] {
        assert!(words.iter().any(|t| t.contains(word)), "{word} is drawn: no ellipsis ate it");
    }
}

/// The threshold is the arithmetic of the numbers set by hand, for two panels, one, or none, and the page switches
/// there: side by side from the threshold, stacked a pixel under it.
#[test]
fn the_page_stacks_exactly_where_the_arithmetic_says() {
    assert_eq!(STACK_AT, 4.0 * GAP + 2.0 * PANEL_MIN + MAP_MIN);
    assert_eq!(beside_min(1), 3.0 * GAP + PANEL_MIN + MAP_MIN);
    assert_eq!(beside_min(0), 2.0 * GAP + MAP_MIN);
    for (open, n) in [([true, true], 2), ([true, false], 1), ([false, true], 1)] {
        let at = beside_min(n);
        assert!(!plan_of(at, open).stacked, "{open:?} at {at}");
        assert!(plan_of(at - 1.0, open).stacked, "{open:?} at {}", at - 1.0);
        // The page agrees: beside, the panel is left of the stage; stacked, it is under it.
        for (w, stacked) in [(at, false), (at - 1.0, true)] {
            let st = PreviewState { panel_open: open, ..state() };
            let (_, ui, ids) = picture_with(Theme::dark(), Vec2::new(w, H), None, st);
            assert_eq!(ids.plan.stacked, stacked, "{open:?} at {w}");
            let (panel, middle) = if open[0] { (ids.left, ids.middle) } else { (ids.right, ids.middle) };
            let (p, m) = (ui.rect(panel).unwrap(), ui.rect(middle).unwrap());
            if stacked {
                assert!(p.y >= m.y + m.h, "the panel is under the stage: {p:?} {m:?}");
            } else {
                assert!((p.y - m.y).abs() < 0.01 && (p.x + p.w <= m.x || m.x + m.w <= p.x), "the panel is beside the stage: {p:?} {m:?}");
            }
        }
    }
    // With no panel open there is nothing to stack, at any width.
    assert!(!plan_of(panels::MIN_WIDTH, [false, false]).stacked);
}

/// The map's column shrinks first; only once it is at its minimum do the panels shrink, each toward its own minimum
/// in proportion to what it has above it.
#[test]
fn panels_shrink_only_once_the_map_is_at_its_minimum() {
    let [nl, nr] = panels::natural();
    let fits = 4.0 * GAP + nl + nr + MAP_MIN;
    for w in (STACK_AT as u32)..=1500 {
        let w = w as f32;
        let p = plan_of(w, [true, true]);
        assert!(!p.stacked);
        let [l, r] = p.widths;
        let map = w - 4.0 * GAP - l - r;
        if w >= fits {
            assert_eq!((l, r), (nl, nr), "natural at {w}");
            assert!(map >= MAP_MIN - 0.01, "the map has its minimum at {w}");
        } else {
            assert!((map - MAP_MIN).abs() < 0.01, "the map is held at its minimum at {w}: {map}");
            assert!(l >= PANEL_MIN && r >= PANEL_MIN && l <= nl && r <= nr, "panels between their minimum and natural at {w}: {l} {r}");
            // In proportion to the slack above each minimum, to the pixel.
            let (sl, sr) = (nl - PANEL_MIN, nr - PANEL_MIN);
            let (lost_l, lost_r) = (nl - l, nr - r);
            assert!((lost_l / sl - lost_r / sr).abs() <= 1.0 / sl.min(sr) + 1e-3, "in proportion at {w}: lost {lost_l} of {sl} and {lost_r} of {sr}");
        }
    }
    // At the stacking threshold both panels are at their minimum.
    assert_eq!(plan_of(STACK_AT, [true, true]).widths, [PANEL_MIN, PANEL_MIN]);
    // The page shows it: the middle column is the map's minimum at a width between the two thresholds.
    let w = (STACK_AT + fits) / 2.0;
    let (_, ui, ids) = picture_with(Theme::dark(), Vec2::new(w.floor(), H), None, state());
    assert!((ui.rect(ids.middle).unwrap().w - MAP_MIN).abs() < 0.01);
}

/// A hand changes a panel's width between its minimum and what leaves the map its own, with no maximum: past the
/// natural width, and cut at both ends. The width is the state's, for the session.
#[test]
fn dragging_is_clamped_at_both_ends() {
    let mut rig = Rig::new(1400, H as u32, state(), None);
    let [nl, nr] = panels::natural();
    assert_eq!(rig.panel_w(0), nl);
    // Past the natural width, and past PANEL_MAX: no maximum.
    let w = rig.drag(0, 100.0);
    assert_eq!(w, nl + 100.0);
    assert!(w > PANEL_MAX, "dragged past the natural cap");
    assert_eq!(rig.state().panel_w, [Some(nl + 100.0), None], "the state holds it");
    // Far to the right: what leaves the map its minimum next to the other panel.
    let most = 1400.0 - 4.0 * GAP - MAP_MIN - nr;
    assert_eq!(rig.drag(0, 2000.0), most);
    let map = rig.rect(rig.ids().middle).w;
    assert!((map - MAP_MIN).abs() < 0.01, "the map is at its minimum: {map}");
    // Far to the left: the minimum.
    assert_eq!(rig.drag(0, -2000.0), PANEL_MIN);
    // The figures panel is dragged from its own side: the pointer going left makes it wider.
    assert_eq!(rig.drag(1, -60.0), nr + 60.0);
    // Each panel is clamped against the other as it is: the settings panel is at its minimum, so the figures panel
    // may take what is left of the map's minimum.
    let most_right = 1400.0 - 4.0 * GAP - MAP_MIN - PANEL_MIN;
    assert_eq!(rig.drag(1, -2000.0), most_right);
    // A window that narrows takes the dragged panel down with the map's minimum: the wanted width is a wish.
    rig.resize(1100);
    let (l, r) = (rig.panel_w(0), rig.panel_w(1));
    assert!(l >= PANEL_MIN && r >= PANEL_MIN && (rig.rect(rig.ids().middle).w - MAP_MIN).abs() < 0.01, "{l} {r}");
}

/// A double click on the grip puts the panel back to its natural width, and only that panel.
#[test]
fn a_double_click_returns_the_panel_to_its_natural_width() {
    let mut rig = Rig::new(1400, H as u32, state(), None);
    let [nl, nr] = panels::natural();
    rig.drag(0, -50.0);
    rig.drag(1, -40.0);
    assert_eq!((rig.panel_w(0), rig.panel_w(1)), (nl - 50.0, nr + 40.0));
    let at = centre(rig.rect(rig.ids().grips[0]));
    rig.down(at, 1);
    rig.up(at);
    rig.down(at, 2);
    rig.up(at);
    assert_eq!(rig.state().panel_w, [None, Some(nr + 40.0)]);
    assert_eq!((rig.panel_w(0), rig.panel_w(1)), (nl, nr + 40.0));
    assert!(rig.state().grip.is_none(), "no drag is left under way");
    // A single press that does not move leaves the width alone.
    let at = centre(rig.rect(rig.ids().grips[1]));
    rig.down(at, 1);
    rig.up(at);
    assert_eq!(rig.state().panel_w[1], Some(nr + 40.0));
}

/// The grip: at rest nothing is drawn, the pointer over it draws a line of 1 px in the muted colour and asks for the
/// horizontal-resize cursor, and a press held draws it in the text colour.
#[test]
fn the_grip_draws_a_line_only_under_the_pointer() {
    let mut rig = Rig::new(1400, H as u32, state(), None);
    let g = rig.rect(rig.ids().grips[0]);
    assert_eq!(g.w, panels::GRIP_W);
    let (x, y) = (g.x + (g.w / 2.0).floor(), g.y + g.h / 2.0);
    let t = rig.theme;
    rig.move_to(Vec2::new(5.0, 400.0));
    assert_eq!(rig.px(x, y), rgb(t.ground), "nothing at rest");
    assert_eq!(rig.px(x - 1.0, y), rgb(t.ground));
    assert_ne!(rig.app.cursor(), Cursor::ResizeEW);
    rig.move_to(centre(g));
    assert_eq!(rig.app.cursor(), Cursor::ResizeEW, "the horizontal-resize cursor");
    assert_eq!(rig.px(x, y), rgb(t.muted), "a line in the muted colour on hover");
    assert_eq!(rig.px(x - 1.0, y), rgb(t.ground), "1 px wide");
    assert_eq!(rig.px(x + 1.0, y), rgb(t.ground), "1 px wide");
    rig.down(centre(g), 1);
    assert_eq!(rig.px(x, y), rgb(t.text), "the text colour while it is dragged");
    rig.up(centre(g));
}

/// The hit zone is 8 px wide, in the middle of the gap between the panel and the map.
#[test]
fn the_grip_is_8_px_in_the_middle_of_the_gap() {
    let rig = Rig::new(1400, H as u32, state(), None);
    let ids = rig.ids();
    for (side, gap_start) in [(0, rig.rect(ids.left).x + rig.rect(ids.left).w), (1, rig.rect(ids.middle).x + rig.rect(ids.middle).w)] {
        let g = rig.rect(ids.grips[side]);
        assert_eq!((g.w, g.x - gap_start), (8.0, (GAP - 8.0) / 2.0), "side {side}: {g:?}");
        assert!((g.y - (BAR_H + CONTENT_TOP)).abs() < 0.01 && g.h > 500.0, "as tall as the panels: {g:?}");
    }
}

/// A closed panel takes no room: the map's column grows by its width and its gap, and its grip is gone.
#[test]
fn a_closed_panel_gives_its_room_to_the_map() {
    let mut rig = Rig::new(1400, H as u32, state(), None);
    let ids = rig.ids();
    let [nl, nr] = panels::natural();
    let before = rig.rect(ids.middle);
    rig.click_on(ids.toggles[1]);
    let ids = rig.ids();
    assert_eq!(rig.state().panel_open, [true, false]);
    assert_eq!((ids.right, ids.grips[1]), (Id::ROOT, Id::ROOT), "the figures panel is not built");
    let after = rig.rect(ids.middle);
    assert_eq!(after.w, before.w + nr + GAP, "the map's column took the panel and its gap");
    assert_eq!(after.x, before.x);
    // Closing the other too: the stage takes the whole width between the edges.
    rig.click_on(ids.toggles[0]);
    let ids = rig.ids();
    assert_eq!(rig.state().panel_open, [false, false]);
    assert_eq!(rig.rect(ids.middle).w, 1400.0 - 2.0 * GAP);
    assert_eq!(rig.rect(ids.middle).x, GAP);
    // The settings panel alone, with the figures panel closed, opens again where it was.
    rig.click_on(ids.toggles[0]);
    assert_eq!(rig.rect(rig.ids().left).w, nl);
}

/// The bar's two buttons: each toggles its own panel, shows its pressed state while its panel is open, and names it in
/// its tooltip. Both panels start open. Home has neither.
#[test]
fn the_bars_buttons_toggle_their_own_panel() {
    let mut rig = Rig::new(1400, H as u32, state(), None);
    let ids = rig.ids();
    assert_eq!(rig.state().panel_open, [true, true], "both start open");
    rig.move_to(Vec2::new(5.0, 400.0));
    // The buttons are unframed (`IconLook::Caption`): an open panel shows a filled icon, a hidden one an empty icon.
    let open: Vec<usize> = (0..2).map(|i| { let b = rig.rect(ids.toggles[i]); rig.ink(b) }).collect();
    assert!(open.iter().all(|&n| n > 0), "both icons are drawn");
    rig.click_on(ids.toggles[0]);
    assert_eq!(rig.state().panel_open, [false, true], "the settings button closes the settings panel only");
    let b0 = rig.rect(ids.toggles[0]);
    rig.move_to(Vec2::new(5.0, 400.0));
    assert!(rig.ink(b0) < open[0], "the icon empties when its panel is closed");
    let b1 = rig.rect(ids.toggles[1]);
    assert_eq!(rig.ink(b1), open[1], "the other one stays filled");
    rig.click_on(ids.toggles[1]);
    assert_eq!(rig.state().panel_open, [false, false]);
    rig.click_on(ids.toggles[0]);
    assert_eq!(rig.state().panel_open, [true, false]);

    // The tooltips name the panels.
    for (i, name) in [(0, preview::text::SETTINGS_PANEL), (1, preview::text::FIGURES_PANEL)] {
        rig.move_to(Vec2::new(5.0, 400.0));
        rig.app.set_clock(Some(10.0 + i as f64 * 10.0));
        rig.move_to(centre(rig.rect(ids.toggles[i])));
        rig.app.set_clock(Some(10.6 + i as f64 * 10.0));
        rig.frame();
        let says = rig.app.list().cmds.iter().any(|c| matches!(c, Cmd::Text { text, .. } if text == name));
        assert!(says, "the tip says {name}");
    }

    // Home keeps its brand and has no panel buttons.
    let mut home = Rig::new(1400, H as u32, PreviewState { mode: preview::Mode::Home, ..state() }, None);
    home.frame();
    assert_eq!(home.ids().toggles, [Id::ROOT; 2]);
    assert_ne!(home.ids().brand, Id::ROOT);
}

/// Stacked, each panel is one scroll area with the kit's scrollbar in its own gutter, and nothing scrolls inside it:
/// the settings list and the lists show their rows in the panel's one scroll.
#[test]
fn stacked_panels_scroll_as_one_whole_with_no_nested_scroll() {
    let world = world();
    let state = PreviewState { view: 2, ..PreviewState::for_world(world.as_ref()) };
    let mut rig = Rig::new(700, 700, state, world.as_ref());
    let ids = rig.ids();
    assert!(ids.plan.stacked);
    // Two scroll areas, one per panel, and no other.
    assert_eq!(ids.scroll_areas, ids.panel_scrolls.to_vec(), "only the panels' own scroll areas");
    let (left, right) = (rig.rect(ids.left), rig.rect(ids.right));
    assert_eq!(rig.rect(ids.panel_scrolls[0]), left, "the left panel is its scroll area");
    assert_eq!(rig.rect(ids.panel_scrolls[1]), right);
    // None is inside another, and the lists are plain boxes inside the panel's scroll.
    let areas: Vec<Rect> = ids.scroll_areas.iter().map(|&a| rig.rect(a)).collect();
    for (i, a) in areas.iter().enumerate() {
        for (j, b) in areas.iter().enumerate() {
            if i != j {
                assert!(!(a.x >= b.x && a.y >= b.y && a.x + a.w <= b.x + b.w && a.y + a.h <= b.y + b.h), "scroll area {i} is inside {j}");
            }
        }
    }
    assert!(!ids.scroll_areas.contains(&ids.settings_scroll) && !ids.scroll_areas.contains(&ids.list_scroll));
    // The settings list shows every group in the one scroll: its box is as tall as its rows, taller than the panel.
    let list = rig.rect(ids.settings_scroll);
    assert!(list.h > left.h, "the rows are in the panel's scroll, not cut by a scroll of their own: {list:?} in {left:?}");
    // The panel scrolls: the wheel over it moves everything, the settings with the world above them.
    let head_before = rig.rect(ids.world_head).y;
    rig.app.event(Event::Wheel { pos: centre(left), delta: -1.0 });
    rig.frame();
    rig.frame();
    assert!(rig.rect(rig.ids().world_head).y < head_before, "the whole panel moved");
    assert!(rig.state().panel_y[0] > 0.0 && rig.state().panel_y[1] == 0.0, "and only that panel: {:?}", rig.state().panel_y);
    // Beside the stage, the settings and the list do scroll by themselves.
    rig.resize(1400);
    let ids = rig.ids();
    assert!(!ids.plan.stacked);
    assert!(ids.scroll_areas.contains(&ids.settings_scroll) && ids.scroll_areas.contains(&ids.list_scroll));
    assert_eq!(ids.panel_scrolls, [Id::ROOT; 2]);
}

/// Stacked: the stage on top takes the half of the height under the bar with its square centred; the open panels are
/// under it in equal columns that fill the width, 16 px apart; one open panel takes the whole width.
#[test]
fn the_stacked_page_is_the_stage_over_equal_columns() {
    let rig = Rig::new(700, 700, state(), None);
    let ids = rig.ids();
    let (m, l, r) = (rig.rect(ids.middle), rig.rect(ids.left), rig.rect(ids.right));
    assert_eq!(m.y, BAR_H + CONTENT_TOP);
    assert_eq!(m.h, ((700.0 - BAR_H) / 2.0).floor() - CONTENT_TOP, "the half of the height under the bar");
    assert_eq!((m.x, m.w), (GAP_STACKED, 700.0 - 2.0 * GAP_STACKED));
    let f = rig.rect(ids.frame);
    assert!((f.x + f.w / 2.0 - 350.0).abs() <= 0.5 && f.w == f.h, "the square is centred: {f:?}");
    assert_eq!((l.x, l.w, r.w), (GAP_STACKED, ((700.0 - 3.0 * GAP_STACKED) / 2.0).floor(), ((700.0 - 3.0 * GAP_STACKED) / 2.0).floor()));
    assert_eq!(r.x - (l.x + l.w), GAP_STACKED, "16 px between the columns");
    assert_eq!(l.y, r.y);
    assert!(l.y >= m.y + m.h + GAP_STACKED - 0.01, "16 px under the stage: {l:?} {m:?}");
    assert!((l.y + l.h - (700.0 - 22.0)).abs() < 0.01, "down to the bottom padding");
    // No grip when stacked.
    assert_eq!(ids.grips, [Id::ROOT; 2]);
    // One open panel is stacked only under 542 px (3 gaps, the panel and the map's minimum), below the window's
    // least width, so a picture shows it: the panel fills the width.
    let st = PreviewState { panel_open: [true, false], ..state() };
    let (_, ui, ids) = picture_with(Theme::dark(), Vec2::new(500.0, 700.0), None, st);
    assert!(ids.plan.stacked && ids.right == Id::ROOT);
    let l = ui.rect(ids.left).unwrap();
    assert_eq!((l.x, l.w), (GAP_STACKED, 500.0 - 2.0 * GAP_STACKED));
}

/// No line or rule between the stage and the panels, or between the panels, when they are stacked: the dark space
/// separates them.
#[test]
fn the_stacked_page_has_no_rule_between_its_parts() {
    let rig = Rig::new(700, 700, state(), None);
    let ids = rig.ids();
    let (m, l, r) = (rig.rect(ids.middle), rig.rect(ids.left), rig.rect(ids.right));
    let (top, bottom) = (m.y + m.h, l.y + 2.0);
    for c in &rig.app.list().cmds {
        if let Cmd::Fill { rect, .. } = c {
            let thin = rect.h <= 1.01 && rect.w >= 40.0;
            assert!(!(thin && rect.y >= top - 0.5 && rect.y <= bottom), "a rule between the stage and the panels: {rect:?}");
        }
    }
    // Between the two columns nothing is drawn in the gap, the whole height of the panels.
    let (gx0, gx1) = (l.x + l.w + 1.0, r.x - 1.0);
    for c in &rig.app.list().cmds {
        if let Cmd::Fill { rect, .. } = c {
            assert!(!(rect.x >= gx0 && rect.x + rect.w <= gx1 && rect.y >= l.y && rect.h > 0.0 && rect.w > 0.0), "something in the gap between the panels: {rect:?}");
        }
    }
}
