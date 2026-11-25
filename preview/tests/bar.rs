//! The launcher's bar on the preview's own frame: its form at every width, where a press on the window goes, the
//! window's requests, the tool switch, and its pixels at the scales a screen has. The pictures go to target/tmp;
//! the ones worth keeping are copied to docs/shots.

mod common;

use std::path::Path;

use mere_base::{App, Button, Event, Hit, Rect, Vec2, WindowRequest, Edge};
use mere_ui::tokens::{ICON_SQUARE, palette};
use mere_ui::{Id, TOOL_PAD_L, Theme, Ui};
use preview::overflow::{ENDS_W, TOGGLE_EDGE};
use mere_win32::Renderer;
use preview::world::World;
use preview::{BAR_H, CONTENT_TOP, Mode, PreviewApp, PreviewState, ShellIds, picture};

fn load() -> Option<World> {
    let dir = &preview::world::data_dir();
    if !dir.join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no out folder");
        return None;
    }
    Some(World::load(dir).expect("the data loads"))
}

/// The app and its offscreen renderer at a size in DIP and a scale, as the window would drive them.
struct Rig {
    app: PreviewApp,
    r: Renderer,
    w: u32,
    h: u32,
    scale: f32,
    theme: Theme,
}

impl Rig {
    fn new(world: Option<&World>, mut state: PreviewState, w: u32, h: u32, scale: f32, dark: bool) -> Rig {
        // These tests switch tools to look at the state, not the slide: the slide has its own tests.
        state.reduce_motion = true;
        let theme = if dark { Theme::dark() } else { Theme::light() };
        let (pw, ph) = ((w as f32 * scale) as u32, (h as f32 * scale) as u32);
        let mut r = Renderer::offscreen(pw, ph, scale).expect("offscreen renderer");
        let mut app = PreviewApp::new(theme, dark, Vec2::new(w as f32, h as f32), state, world.cloned());
        app.start(&mut r);
        app.event(Event::Resized { width_px: pw, height_px: ph, scale });
        app.event(Event::Window { maximized: false });
        app.frame(&mut r, 0.0);
        app.frame(&mut r, 0.0);
        Rig { app, r, w, h, scale, theme }
    }

    fn standard(world: &World) -> Rig {
        Rig::new(Some(world), PreviewState::for_world(Some(world)), 1400, 940, 1.0, true)
    }

    fn frame(&mut self) {
        self.app.frame(&mut self.r, 0.0);
        self.app.frame(&mut self.r, 0.0);
    }

    fn rgba(&mut self) -> Vec<u8> {
        self.r.read_rgba().unwrap()
    }

    fn write(&mut self, name: &str) -> Vec<u8> {
        let rgba = self.rgba();
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        std::fs::write(&path, mere_png::encode((self.w as f32 * self.scale) as u32, (self.h as f32 * self.scale) as u32, &rgba)).unwrap();
        println!("wrote {}", path.display());
        rgba
    }

    fn rect(&self, id: Id) -> Rect {
        assert_ne!(id, Id::ROOT, "the widget is not built");
        self.app.ui().rect(id).expect("the widget was built")
    }

    fn click(&mut self, at: Vec2) {
        self.app.event(Event::PointerMoved { pos: at });
        self.app.event(Event::PointerDown { pos: at, button: Button::Left, clicks: 1 });
        self.frame();
        self.app.event(Event::PointerUp { pos: at, button: Button::Left });
        self.frame();
    }

    fn click_on(&mut self, id: Id) {
        let c = centre(self.rect(id));
        self.click(c);
    }

    fn hover(&mut self, at: Vec2) {
        self.app.event(Event::PointerMoved { pos: at });
        self.frame();
    }

    /// The device pixel at (x, y) of the picture (physical coordinates).
    fn px(&self, rgba: &[u8], x: u32, y: u32) -> [u8; 3] {
        let pw = (self.w as f32 * self.scale) as usize;
        let i = (y as usize * pw + x as usize) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    }

    fn drain(&mut self) -> Vec<WindowRequest> {
        std::iter::from_fn(|| self.app.take_request()).collect()
    }
}

fn centre(r: Rect) -> Vec2 {
    Vec2::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn rgb(c: mere_base::Color) -> [u8; 3] {
    [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8)
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 0.01, "{what}: {a} != {b}");
}

fn rect_of(ui: &Ui, id: Id) -> Rect {
    assert_ne!(id, Id::ROOT);
    ui.rect(id).expect("built")
}

/// B1: the bar's parts keep their sizes. From the least width to 1920 the two panels' buttons, each tool, each tab and
/// each window button are the same size; the tools are centred while the bar can centre them and sit left, after the
/// settings button, when it cannot; the window's buttons end at the right edge; and nothing overlaps.
#[test]
fn b1_the_bar_keeps_its_form_at_every_width() {
    // The buttons, the tools and the window's buttons never change size; a tab keeps its size too, but the bar may
    // hold fewer of them at a narrow width (the rest are in "More", tests/bar_overflow.rs): the ones it holds are
    // those of the full bar, less the ones in More.
    let sizes = |ids: &ShellIds, ui: &Ui| -> Vec<(f32, f32)> {
        let parts = [ids.toggles[0], ids.toggles[1], ids.tools[0], ids.tools[1], ids.ends[0], ids.ends[1], ids.ends[2]];
        parts.iter().map(|&p| (rect_of(ui, p).w, rect_of(ui, p).h)).collect()
    };
    let tab_sizes = |ids: &ShellIds, ui: &Ui| -> Vec<(f32, f32)> { ids.tabs.iter().map(|&p| (rect_of(ui, p).w, rect_of(ui, p).h)).collect() };
    let (_, ui14, ids14) = picture(Theme::dark(), Vec2::new(1400.0, 940.0));
    let reference = sizes(&ids14, &ui14);
    let tab_reference = tab_sizes(&ids14, &ui14);
    assert_eq!(ids14.tabs.len(), 11, "nine views, 2D and 3D, always as words");
    near(rect_of(&ui14, ids14.bar).h, 40.0, "bar height");
    let mut w = preview::MIN_SIZE.0;
    while w <= 1920.0 {
        let (_, ui, ids) = picture(Theme::dark(), Vec2::new(w, 940.0));
        assert_eq!(sizes(&ids, &ui), reference, "the parts' sizes at {w}");
        let kept: Vec<(f32, f32)> = (0..11).filter(|i| !ids.form.more.contains(i) && !(*i >= 9 && ids.form.shapes_in_more)).map(|i| tab_reference[i]).collect();
        assert_eq!(tab_sizes(&ids, &ui), kept, "the tabs' sizes at {w}");
        let (lead, strip, trail) = (rect_of(&ui, ids.toggles[0]), rect_of(&ui, ids.strip), rect_of(&ui, ids.toggles[1]));
        if ids.form.centred {
            assert!((strip.x + strip.w / 2.0 - w / 2.0).abs() <= 0.5, "tools centred at {w}: {strip:?}");
        } else {
            assert!((strip.x - (lead.x + lead.w + 12.0)).abs() <= 1.01, "tools sat left at {w}: {strip:?}");
        }
        for (i, e) in ids.ends.iter().enumerate() {
            near(rect_of(&ui, *e).x, w - ENDS_W + ICON_SQUARE * i as f32, &format!("end {i} at {w}"));
        }
        near(trail.x + trail.w + 8.0, w - ENDS_W, &format!("the figures button before the window's buttons at {w}"));
        near(lead.x, TOGGLE_EDGE, &format!("the settings button at {w}"));
        assert!(lead.x + lead.w < strip.x, "the settings button ends before the tools at {w}: {lead:?} {strip:?}");
        assert!(strip.x + strip.w < trail.x, "the tools end before the figures button at {w}: {strip:?}");
        let tabs: Vec<Rect> = ids.tabs.iter().map(|&t| rect_of(&ui, t)).collect();
        assert!(tabs.windows(2).all(|p| p[0].x + p[0].w <= p[1].x + 1.01), "tabs do not overlap at {w}");
        w += if w < 650.0 || w > 1900.0 { 1.0 } else { 7.0 };
    }
}
/// B2: what a press at a point does, at 1400 x 940.
#[test]
fn b2_hit_regions() {
    let Some(world) = load() else { return };
    let mut rig = Rig::standard(&world);
    let hit = |rig: &Rig, x: f32, y: f32| rig.app.hit(Vec2::new(x, y));
    for ((x, y), edge) in [
        ((1.0, 1.0), Edge::NW),
        ((7.0, 7.0), Edge::NW),
        ((9.0, 3.0), Edge::N),
        ((700.0, 2.0), Edge::N),
        ((2.0, 470.0), Edge::W),
        ((1398.0, 470.0), Edge::E),
        ((700.0, 938.0), Edge::S),
        ((1392.0, 932.0), Edge::SE),
        ((1398.0, 1.0), Edge::NE),
        ((1.0, 938.0), Edge::SW),
        // The grips lie over the close button's edge.
        ((1395.0, 20.0), Edge::E),
        ((1380.0, 2.0), Edge::N),
    ] {
        assert_eq!(hit(&rig, x, y), Hit::Resize(edge), "({x}, {y})");
    }
    assert_eq!(hit(&rig, 1380.0, 20.0), Hit::Client, "the close button");
    assert_eq!(hit(&rig, 700.0, 500.0), Hit::Client, "the page");
    let ids = rig.app.ids().clone();
    let (lead_r, strip_r, trail_r) = (rig.rect(ids.toggles[0]), rig.rect(ids.strip), rig.rect(ids.toggles[1]));
    assert_eq!(hit(&rig, lead_r.x / 2.0, 20.0), Hit::Caption, "left of the settings button");
    let between = ((lead_r.x + lead_r.w) + strip_r.x) / 2.0;
    assert_eq!(hit(&rig, between, 20.0), Hit::Caption, "between the settings button and the tools");
    let free = (strip_r.x + strip_r.w + trail_r.x) / 2.0;
    assert_eq!(hit(&rig, free, 20.0), Hit::Caption, "between the tools and the figures button");
    let mut buttons: Vec<Id> = ids.tabs.clone();
    buttons.extend(ids.tools);
    buttons.extend(ids.toggles);
    buttons.extend(ids.ends);
    for b in buttons {
        let c = centre(rig.rect(b));
        assert_eq!(hit(&rig, c.x, c.y), Hit::Client, "button at {c:?}");
    }
    // Maximised: no edge to pull, so the corner and the sides belong to what is under them.
    rig.app.event(Event::Window { maximized: true });
    rig.frame();
    assert_eq!(hit(&rig, 1.0, 1.0), Hit::Caption);
    assert_eq!(hit(&rig, 1398.0, 470.0), Hit::Client);
    assert_eq!(hit(&rig, 1395.0, 20.0), Hit::Client);
}

/// B4: the window's buttons and its title.
#[test]
fn b4_the_buttons_and_the_title_are_requests() {
    let Some(world) = common::world() else { return };
    let root = common::root("bar_requests");
    let mut rig = Rig::new(Some(&world), PreviewState::for_world(Some(&world)), 1400, 940, 1.0, true);
    // The first frame names the window.
    assert_eq!(rig.drain(), vec![WindowRequest::Title("Outpost Tools".into())]);
    assert_eq!(rig.drain(), vec![], "a request is taken once");
    let ids = rig.app.ids().clone();
    for (i, want) in [WindowRequest::Minimize, WindowRequest::ToggleMaximize, WindowRequest::Close].into_iter().enumerate() {
        rig.click_on(ids.ends[i]);
        assert_eq!(rig.drain(), vec![want], "end button {i}");
    }
    // Maximised, the middle button restores: a request all the same, and its icon and title change.
    rig.app.event(Event::Window { maximized: true });
    rig.frame();
    rig.click_on(ids.ends[1]);
    assert_eq!(rig.drain(), vec![WindowRequest::ToggleMaximize]);
    rig.app.event(Event::Window { maximized: false });

    // A run under way: the preview tool carries a marker and the title says so, whichever tool is on.
    drop(rig);
    let mut state = PreviewState::for_world(Some(&world));
    state.detail = 2;
    state.reduce_motion = true;
    let runs = common::runs(&root, &[("MERE_STUB", "hold")]);
    let theme = Theme::dark();
    let mut r = Renderer::offscreen(1400, 940, 1.0).unwrap();
    let mut app = PreviewApp::new(theme, true, Vec2::new(1400.0, 940.0), state, Some(world.clone())).with_runs(runs);
    app.start(&mut r);
    app.event(Event::Resized { width_px: 1400, height_px: 940, scale: 1.0 });
    app.frame(&mut r, 0.0);
    while app.take_request().is_some() {}
    // A changed setting enables Generate.
    app.state_mut().values[0] += 0.01;
    app.frame(&mut r, 0.0);
    let generate = app.ui().rect(app.ids().generate).unwrap();
    let at = centre(generate);
    app.event(Event::PointerMoved { pos: at });
    app.event(Event::PointerDown { pos: at, button: Button::Left, clicks: 1 });
    app.frame(&mut r, 0.0);
    app.event(Event::PointerUp { pos: at, button: Button::Left });
    app.frame(&mut r, 0.0);
    assert!(app.running(), "the stub runs");
    let mut titles = Vec::new();
    while let Some(q) = app.take_request() {
        titles.push(q);
    }
    assert_eq!(titles, vec![WindowRequest::Title("Outpost Tools \u{b7} preview computing".into())]);
    // Home on: the marker is still there, and it is the square on the preview tool.
    let home = app.ui().rect(app.ids().tools[0]).unwrap();
    let c = centre(home);
    app.event(Event::PointerMoved { pos: c });
    app.event(Event::PointerDown { pos: c, button: Button::Left, clicks: 1 });
    app.frame(&mut r, 0.0);
    app.event(Event::PointerUp { pos: c, button: Button::Left });
    app.frame(&mut r, 0.0);
    app.frame(&mut r, 0.0);
    assert_eq!(app.state().mode, Mode::Home);
    let tool = app.ui().rect(app.ids().tools[1]).unwrap();
    let rgba = r.read_rgba().unwrap();
    let at = |x: f32, y: f32| {
        let i = (y as usize * 1400 + x as usize) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    };
    // 5 x 5, on the icon's top right corner: 27 px in from the tool's left edge and 4 px down.
    let (mx, my) = (tool.x + 27.0, tool.y + 4.0);
    assert_eq!(at(mx + 2.0, my + 2.0), rgb(theme.text), "the marker, steady");
    assert_eq!(at(mx - 2.0, my + 2.0), rgb(theme.ground), "and nothing around it");
}

/// B5: switching tools changes what is on screen and nothing else.
#[test]
fn b5_the_tool_switch_keeps_the_previews_state() {
    let Some(world) = load() else { return };
    let mut rig = Rig::standard(&world);
    // Something to keep: another view, a closer look, the 3D camera.
    let tabs = rig.app.ids().tabs.clone();
    rig.click_on(tabs[6]);
    rig.click_on(tabs[10]);
    {
        let s = rig.app.state_mut();
        s.look.k = 4.0;
        s.settings_y = 10.0;
    }
    rig.frame();
    let before = rig.app.state().clone();
    assert!(before.orbit.is_some() && before.in3d && before.view == 6);
    let (home, preview) = (rig.app.ids().tools[0], rig.app.ids().tools[1]);
    rig.click_on(home);
    assert_eq!(rig.app.state().mode, Mode::Home);
    let ids = rig.app.ids().clone();
    assert_ne!(ids.home_pane, Id::ROOT);
    assert_eq!(rig.rect(ids.home_pane), Rect::new(0.0, BAR_H, 1400.0, 940.0 - BAR_H));
    assert_eq!(rig.rect(ids.home_body), Rect::new(28.0, BAR_H + CONTENT_TOP, 1400.0 - 56.0, 940.0 - BAR_H - CONTENT_TOP - 22.0));
    assert!(ids.tabs.is_empty(), "the preview's tabs are not built when its tool is closed");
    assert_eq!((ids.left, ids.middle, ids.right), (Id::ROOT, Id::ROOT, Id::ROOT), "the page's columns are not built");
    // Home again does nothing.
    rig.click_on(home);
    assert_eq!(rig.app.state().mode, Mode::Home);
    rig.click_on(preview);
    let after = rig.app.state();
    assert_eq!(after.mode, Mode::Preview);
    assert_eq!((after.view, after.in3d, after.look, after.settings_y), (before.view, before.in3d, before.look, before.settings_y));
    assert_eq!(after.orbit, before.orbit, "the camera is the same");
    assert!(after.preview_seen);
}

/// B6: the bar floats: no line under it, the gap under it is the page's ground; the crisp icons are exactly the
/// ground or their colour; the tabs' frames are whole device pixels. At 1, 1.5 and 2.
#[test]
fn b6_the_bar_is_whole_device_pixels_at_every_scale() {
    let Some(world) = load() else { return };
    for (scale, name) in [(1.0f32, "bar_1400_dark.png"), (1.5, "bar_1400_x15.png"), (2.0, "bar_1400_x2.png")] {
        let mut rig = Rig::new(Some(&world), PreviewState::for_world(Some(&world)), 1400, 940, scale, true);
        let rgba = rig.write(name);
        let t = rig.theme;
        let (pw, ph) = ((1400.0 * scale) as u32, (940.0 * scale) as u32);
        let ids = rig.app.ids().clone();

        // The bar floats. (a) No row of the top 60 DIP is one full-width line.
        let rows = (0..ph.min((60.0 * scale).ceil() as u32))
            .filter(|&y| (0..pw).all(|x| rig.px(&rgba, x, y) == rgb(t.line)))
            .collect::<Vec<_>>();
        assert!(rows.is_empty(), "scale {scale}: a full-width line at rows {rows:?}");
        // (b) The bar's foot and the gap under it (39 to 50 DIP) are the page's ground over the whole width.
        for y in (39.0 * scale).ceil() as u32..(50.0 * scale).floor() as u32 {
            for x in 0..pw {
                assert_eq!(rig.px(&rgba, x, y), rgb(t.ground), "scale {scale}: not the ground at ({x}, {y})");
            }
        }

        // The icons: each is its colour or the ground, nothing between.
        for (what, id, side) in [("home", ids.tools[0], 36.0), ("minimise", ids.ends[0], ICON_SQUARE), ("maximise", ids.ends[1], ICON_SQUARE), ("close", ids.ends[2], ICON_SQUARE)] {
            let r = rig.rect(id);
            // The home tool is its icon, TOOL_PAD_L in, and its name; the others are the icon alone.
            let c = if what == "home" { Vec2::new(r.x + TOOL_PAD_L + 8.0, r.y + r.h / 2.0) } else { centre(r) };
            let _ = side;
            let (x0, y0) = (((c.x - 8.0) * scale).floor() as u32, ((c.y - 8.0) * scale).floor() as u32);
            let (x1, y1) = (((c.x + 8.0) * scale).ceil() as u32, ((c.y + 8.0) * scale).ceil() as u32);
            let mut inked = 0;
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = rig.px(&rgba, x, y);
                    assert!(p == rgb(t.ground) || p == rgb(t.muted), "scale {scale}: the {what} icon has {p:?} at ({x}, {y})");
                    inked += (p == rgb(t.muted)) as usize;
                }
            }
            assert!(inked >= 8, "scale {scale}: the {what} icon is drawn ({inked} pixels)");
        }

        // The panels' buttons are pressed (both panels are open): the icon is the ground on the text colour's fill, and
        // each pixel of it is one or the other.
        for (what, id) in [("settings panel", ids.toggles[0]), ("figures panel", ids.toggles[1])] {
            let c = centre(rig.rect(id));
            let (x0, y0) = (((c.x - 8.0) * scale).floor() as u32, ((c.y - 8.0) * scale).floor() as u32);
            let (x1, y1) = (((c.x + 8.0) * scale).ceil() as u32, ((c.y + 8.0) * scale).ceil() as u32);
            let mut inked = 0;
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = rig.px(&rgba, x, y);
                    assert!(p == rgb(t.ground) || p == rgb(t.text), "scale {scale}: the {what} icon has {p:?} at ({x}, {y})");
                    inked += (p == rgb(t.ground)) as usize;
                }
            }
            assert!(inked >= 8, "scale {scale}: the {what} icon is drawn ({inked} pixels)");
        }

        // The tabs' frames: along the top edge of each, from just inside its corners, the pixels are exactly the
        // frame's colour (a blurred line would have a mix).
        for (i, tab) in ids.tabs.iter().enumerate() {
            // A greyed tab (the loaded world is a coarse one) is drawn faded: its frame is not the plain edge.
            if world.coarse() && matches!(i, 1 | 4 | 5) {
                continue;
            }
            let r = rig.rect(*tab);
            let want = if i == 2 || i == 6 { rgb(t.text) } else { rgb(t.edge) };
            let y = (r.y * scale).round() as u32;
            let (x0, x1) = ((r.x * scale).round() as u32 + 2, ((r.x + r.w) * scale).round() as u32 - 2);
            for x in x0..x1 {
                assert_eq!(rig.px(&rgba, x, y), want, "scale {scale}: tab {i} top edge at ({x}, {y})");
            }
            // Only the first tab of a group has a left edge nobody paints over (the others share it, the pressed one on top).
            if i != 0 && i != 6 {
                continue;
            }
            let x = (r.x * scale).round() as u32;
            let (y0, y1) = (((r.y * scale).round() as u32) + 2, (((r.y + r.h) * scale).round() as u32) - 2);
            for y in y0..y1 {
                assert_eq!(rig.px(&rgba, x, y), want, "scale {scale}: tab {i} left edge at ({x}, {y})");
            }
        }
    }
}

/// The pictures of the bar: light, Home, maximised, the pointer over Close.
#[test]
fn bar_pictures() {
    let Some(world) = load() else { return };
    let state = || PreviewState::for_world(Some(&world));
    let mut rig = Rig::new(Some(&world), state(), 1400, 940, 1.0, false);
    rig.write("bar_1400_light.png");

    let mut rig = Rig::standard(&world);
    let home = rig.app.ids().tools[0];
    rig.click_on(home);
    rig.hover(Vec2::new(700.0, 500.0));
    rig.write("bar_home.png");

    let mut rig = Rig::standard(&world);
    rig.app.event(Event::Window { maximized: true });
    rig.frame();
    rig.write("bar_maximized.png");

    let mut rig = Rig::standard(&world);
    let close = rig.rect(rig.app.ids().ends[2]);
    rig.hover(Vec2::new(close.x + ICON_SQUARE / 2.0, close.y + 20.0));
    let rgba = rig.write("bar_hover_close.png");
    assert_eq!(rig.px(&rgba, (close.x + 3.0) as u32, (close.y + 3.0) as u32), [(palette::RED_600 >> 16) as u8, (palette::RED_600 >> 8) as u8, palette::RED_600 as u8], "Close is red under the pointer");
    // Beside it the others stay quiet.
    let min = rig.rect(rig.app.ids().ends[0]);
    assert_eq!(rig.px(&rgba, (min.x + 3.0) as u32, (min.y + 3.0) as u32), rgb(rig.theme.ground));
}

fn says(rig: &Rig, words: &str) -> bool {
    rig.app.list().cmds.iter().any(|c| matches!(c, mere_base::Cmd::Text { text, .. } if text == words))
}

/// B3: the pointer's shape follows what is under it: a hand on a tab, the arrow on the bare shell, a crosshair on
/// the map, and the sizing arrows only where the window's frame is (an edge of the bar).
#[test]
fn b3_the_cursor_follows_what_is_under_the_pointer() {
    use mere_base::Cursor;
    let Some(world) = load() else { return };
    let mut rig = Rig::standard(&world);
    // Biomes: a greyed tab (the loaded world is a coarse one) asks for no hand.
    let tab = centre(rig.rect(rig.app.ids().tabs[0]));
    rig.hover(tab);
    assert_eq!(rig.app.cursor(), Cursor::Hand, "over a tab");
    let map = centre(rig.rect(rig.app.ids().stage));
    rig.hover(map);
    assert_eq!(rig.app.cursor(), Cursor::Crosshair, "over the map");
    rig.hover(Vec2::new(5.0, 500.0));
    assert_eq!(rig.app.cursor(), Cursor::Arrow, "over the bare shell");
    assert_eq!(rig.app.hit(Vec2::new(1.0, 500.0)), Hit::Resize(Edge::W), "the frame's edge is the system's");
}

/// T1: a tooltip waits half a second under the pointer, asks the window to wake then, and is painted last.
#[test]
fn t1_a_tooltip_comes_after_its_delay_and_goes_with_the_pointer() {
    let Some(world) = load() else { return };
    let mut rig = Rig::standard(&world);
    rig.app.set_clock(Some(10.0));
    let min = centre(rig.rect(rig.app.ids().ends[0]));
    rig.hover(min);
    let wait = rig.app.wake_in().expect("a tip is due");
    assert!(wait > 0.0 && wait <= 0.5, "wakes within the delay: {wait}");
    assert!(!says(&rig, preview::text::MINIMIZE), "not before the delay");
    rig.app.set_clock(Some(10.6));
    rig.frame();
    assert!(says(&rig, preview::text::MINIMIZE), "after the delay");
    assert_eq!(rig.app.wake_in(), None, "nothing more to wait for");
    rig.write("tooltip_minimise.png");
    rig.hover(Vec2::new(5.0, 500.0));
    assert!(!says(&rig, preview::text::MINIMIZE), "gone with the pointer");

    // A view tab carries its label as its title: the picture kept in docs/shots.
    rig.app.set_clock(Some(20.0));
    let tab = centre(rig.rect(rig.app.ids().tabs[5]));
    rig.hover(tab);
    rig.app.set_clock(Some(20.6));
    rig.frame();
    let tips = rig.app.list().cmds.iter().filter(|c| matches!(c, mere_base::Cmd::Text { text, .. } if text == "Relief")).count();
    assert!(tips >= 2, "the tab's own label and the tip that repeats it: {tips}");
    rig.write("tooltip.png");
}

/// T2: with no run under way the app asks for no frames and has nothing to look at while hidden.
#[test]
fn t2_an_idle_app_asks_for_nothing() {
    let Some(world) = load() else { return };
    let mut rig = Rig::standard(&world);
    rig.hover(Vec2::new(5.0, 500.0));
    assert_eq!(rig.app.wake_in(), None);
    assert!(!rig.app.tick());
    assert!(!rig.app.frame(&mut rig.r, 0.0), "a settled page is not drawn again");
}

/// T3: a preview folder that is missing shows the page's note and nothing else; one that is there and cannot be
/// read says so in the status line, in the warning tone. Neither closes the window.
#[test]
fn t3_errors_are_the_pages_note_and_the_status_line() {
    let mut rig = Rig::new(None, PreviewState::for_world(None), 1400, 940, 1.0, true);
    assert!(says(&rig, preview::text::NO_PREVIEW));
    assert!(!says(&rig, preview::text::UNREADABLE));
    assert_eq!(rig.app.ids().probe_text, "", "no map, no probe line (the page's restProbe)");
    rig.write("error_missing.png");

    let mut state = PreviewState::for_world(None);
    state.trouble = preview::text::UNREADABLE.to_string();
    let mut rig = Rig::new(None, state, 1400, 940, 1.0, true);
    assert!(says(&rig, preview::text::NO_PREVIEW));
    assert_eq!(rig.app.ids().status_text, preview::text::UNREADABLE, "the status line holds the trouble");
    rig.write("error_unreadable.png");
}
// ---- the bar's slide and account (step 21) ----

/// A rig on the preview with no world, at 1400 x 940 in Home or in the preview tool, the clock held at `at`.
fn slide_rig(mode: Mode, reduce: bool, account: Option<&str>, w: u32, at: f64) -> Rig {
    let state = PreviewState { mode, account: account.map(String::from), ..PreviewState::default() };
    let mut rig = Rig::new(None, state, w, 940, 1.0, true);
    rig.app.state_mut().reduce_motion = reduce;
    rig.app.set_clock(Some(at));
    rig.frame();
    rig
}

fn options_w(rig: &Rig) -> f32 {
    rig.rect(rig.app.ids().options).w
}

/// A1: Home to the preview tool, 110 ms in: the options are at the ease's value of their width.
#[test]
fn a1_the_options_slide_open_in_220_ms() {
    let mut rig = slide_rig(Mode::Home, false, None, 1400, 10.0);
    assert_eq!(rig.app.ids().options, Id::ROOT, "closed: not built");
    let preview = rig.app.ids().tools[1];
    rig.click_on(preview);
    assert_eq!(rig.app.state().mode, Mode::Preview);
    rig.app.set_clock(Some(10.11));
    rig.app.frame(&mut rig.r, 0.0);
    let natural = rig.app.state().options_w;
    assert!(natural > 300.0, "the tabs' width is known: {natural}");
    let w = options_w(&rig);
    let want = mere_ui::ease(0.5) * natural;
    assert!((w - want).abs() < 1.5, "{w} against {want}");
    assert!(rig.app.frame(&mut rig.r, 0.0), "frames go on while it moves");
    rig.app.set_clock(Some(11.0));
    rig.app.frame(&mut rig.r, 0.0);
    assert!((options_w(&rig) - natural).abs() < 0.01);
    assert!(!rig.app.frame(&mut rig.r, 0.0), "and stop when it has settled");
    rig.write("bar_slide_done.png");
}

/// A2: with the system's reduced motion the options are whole on the first frame after the click.
#[test]
fn a2_reduced_motion_switches_at_once() {
    let mut rig = slide_rig(Mode::Home, true, None, 1400, 10.0);
    let preview = rig.app.ids().tools[1];
    rig.click_on(preview);
    let natural = rig.app.state().options_w;
    assert!(natural > 300.0);
    assert!((options_w(&rig) - natural).abs() < 0.01);
    assert!(!rig.app.frame(&mut rig.r, 0.0));
}

/// A3: closing: 110 ms after a click on Home the tabs are at 1 - ease(0.5) of their width, and inert.
#[test]
fn a3_the_options_slide_shut_and_are_inert() {
    let mut rig = slide_rig(Mode::Preview, false, None, 1400, 10.0);
    let natural = rig.app.state().options_w;
    assert!(natural > 300.0);
    let home = rig.app.ids().tools[0];
    rig.click_on(home);
    assert_eq!(rig.app.state().mode, Mode::Home);
    rig.app.set_clock(Some(10.11));
    rig.app.frame(&mut rig.r, 0.0);
    let w = options_w(&rig);
    let want = (1.0 - mere_ui::ease(0.5)) * natural;
    assert!((w - want).abs() < 1.5, "{w} against {want}");
    let (view, tab) = (rig.app.state().view, rig.app.ids().tabs[0]);
    let c = centre(rig.rect(tab));
    // The first tab is under the pointer only if the slide has not cut it off: click it, whatever it is.
    rig.click(c);
    assert_eq!(rig.app.state().view, view, "a tab does nothing while its tool closes");
    rig.app.set_clock(Some(11.0));
    rig.frame();
    assert_eq!(rig.app.ids().options, Id::ROOT, "gone when shut");
}

/// A4: in Home the account sits 10 px after the name, muted; a long one is cut before the tools. The preview tool has
/// no brand, so no account: its left end is the settings panel's button.
#[test]
fn a4_the_account_follows_the_name() {
    for w in [1000, 1920] {
        let rig = slide_rig(Mode::Home, true, Some("PlayerOne"), w, 1.0);
        let shown: Vec<(String, Vec2)> = rig.app.list().cmds.iter().filter_map(|c| if let mere_base::Cmd::Text { text, at, .. } = c { text.starts_with("Player").then(|| (text.clone(), *at)) } else { None }).collect();
        assert_eq!(shown.len(), 1, "Home {w}");
        let (text, at) = &shown[0];
        // The brand's padding (12), the cube (16) and two gaps (10 each), then the brand's name, whatever it says.
        let after_name = 48.0 + preview::overflow::brand_look().width(&preview::text::BAR_TITLE.to_uppercase());
        assert!((at.x - after_name).abs() < 0.5, "Home {w}: {} for {after_name}", at.x);
        let brand = rig.rect(rig.app.ids().brand);
        assert!((at.y + 9.0 - (brand.y + brand.h / 2.0)).abs() < 3.0, "on the bar's centre line");
        assert_eq!(text, "PlayerOne", "whole whenever the free space left of the tools holds it: always in Home, {w}");
        let strip = rig.rect(rig.app.ids().strip);
        assert!(at.x + text.chars().count() as f32 * 7.2 <= strip.x + 0.01, "Home {w}: runs into the tools");
    }
    for w in [1000, 1400, 1920] {
        let rig = slide_rig(Mode::Preview, true, Some("PlayerOne"), w, 1.0);
        let shown = rig.app.list().cmds.iter().filter(|c| matches!(c, mere_base::Cmd::Text { text, .. } if text.starts_with("Player"))).count();
        assert_eq!(shown, 0, "the preview tool shows no account at {w}");
        assert_eq!(rig.app.ids().brand, Id::ROOT);
    }
    let long = "A".repeat(40);
    let rig = slide_rig(Mode::Home, true, Some(&long), 600, 1.0);
    let cut = rig.app.list().cmds.iter().find_map(|c| if let mere_base::Cmd::Text { text, at, .. } = c { (text.ends_with('\u{2026}') && text.starts_with("AAA")).then(|| (text.clone(), *at)) } else { None });
    let (text, at) = cut.expect("a 40 char account does not fit before the tools at 600 and ends in an ellipsis");
    let right = at.x + text.chars().count() as f32 * 7.2;
    let strip = rig.rect(rig.app.ids().strip);
    assert!(right <= strip.x + 0.01, "{right} runs into the strip at {}", strip.x);
}
/// The picture of the half-open tabs.
#[test]
fn a5_the_half_open_bar_picture() {
    let mut rig = slide_rig(Mode::Home, false, Some("PlayerOne"), 1400, 10.0);
    let preview = rig.app.ids().tools[1];
    rig.click_on(preview);
    rig.app.set_clock(Some(10.11));
    rig.frame();
    rig.write("bar_slide_mid.png");
}
