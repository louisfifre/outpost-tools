//! The section view in the stage, drawn offscreen through the whole application: its layout, its pixels, and what
//! the wheel, a drag, a press on the strip, "Another cut", the ore rows and the pointer do. The pictures go to the
//! target's tmp folder; the ones worth keeping are copied to docs/shots. Skipped (with a line) without the data.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use mere_base::{App, Button, Cmd, Event, Rect, Vec2};
use mere_ui::{Id, Theme, wrap_lines};
use mere_win32::Renderer;
use preview::cut::{self, CutLook, NAV_H};
use preview::map::tick_at;
use preview::section::{ORE_COLOURS, ORE_KINDS};
use preview::world::World;
use preview::{BAR_H, CONTENT_TOP, PreviewApp, PreviewState, text};

const W: u32 = 1400;
const H: u32 = 940;
const DARK_GROUND: [u8; 3] = [0x00, 0x00, 0x00];
const LIGHT_GROUND: [u8; 3] = [0xfa, 0xfa, 0xf8];
const DARK_TEXT: [u8; 3] = [0xec, 0xec, 0xec];
const LINE: [u8; 3] = [0x2a, 0x2a, 0x2a];

fn load() -> Option<World> {
    let dir = &preview::world::data_dir();
    if !dir.join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no out folder");
        return None;
    }
    Some(World::load(dir).expect("the data loads"))
}

fn state() -> PreviewState {
    PreviewState { view: preview::paint::SECTION, ..PreviewState::default() }
}

fn tmp(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

fn rgb(c: mere_base::Color) -> [u8; 3] {
    [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8)
}

fn centre(r: Rect) -> Vec2 {
    Vec2::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// The application and its offscreen renderer, driven by events and frames as the window would.
struct Rig {
    app: PreviewApp,
    r: Renderer,
    w: u32,
    h: u32,
    theme: Theme,
}

impl Rig {
    fn new(world: &World, light: bool, state: PreviewState, w: u32, h: u32) -> Rig {
        let theme = if light { Theme::light() } else { Theme::dark() };
        let mut r = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
        let mut app = PreviewApp::new(theme, !light, Vec2::new(w as f32, h as f32), state, Some(world.clone()));
        app.start(&mut r);
        app.event(Event::Resized { width_px: w, height_px: h, scale: 1.0 });
        app.frame(&mut r, 0.0);
        Rig { app, r, w, h, theme }
    }

    fn frame(&mut self) {
        self.app.frame(&mut self.r, 0.0);
    }

    fn rgba(&mut self) -> Vec<u8> {
        self.r.read_rgba().unwrap()
    }

    fn write(&mut self, name: &str) -> Vec<u8> {
        let rgba = self.rgba();
        std::fs::write(tmp(name), mere_png::encode(self.w, self.h, &rgba)).unwrap();
        println!("wrote {}", tmp(name).display());
        rgba
    }

    fn rect(&self, id: Id) -> Rect {
        self.app.ui().rect(id).expect("the widget was built")
    }

    fn moved(&mut self, pos: Vec2) {
        self.app.event(Event::PointerMoved { pos });
    }

    /// A click as the tests of the map make it: the pointer arrives, goes down, a frame, goes up, a frame.
    fn click(&mut self, pos: Vec2) {
        self.moved(pos);
        self.app.event(Event::PointerDown { pos, button: Button::Left, clicks: 1 });
        self.frame();
        self.app.event(Event::PointerUp { pos, button: Button::Left });
        self.frame();
    }

    /// A press that travels by `d` before it is released.
    fn drag(&mut self, from: Vec2, d: Vec2) {
        self.moved(from);
        self.app.event(Event::PointerDown { pos: from, button: Button::Left, clicks: 1 });
        self.frame();
        let to = Vec2::new(from.x + d.x, from.y + d.y);
        self.moved(to);
        self.frame();
        self.app.event(Event::PointerUp { pos: to, button: Button::Left });
        self.frame();
    }

    fn wheel(&mut self, pos: Vec2, delta: f32) {
        self.moved(pos);
        self.app.event(Event::Wheel { pos, delta });
        self.frame();
    }

    fn look(&self) -> CutLook {
        self.app.state().cut_look
    }

    /// The cut view's border box as the look arithmetic takes it.
    fn cut_box(&self) -> (f64, f64) {
        self.app.ids().cut_box
    }

    fn px(&self, rgba: &[u8], x: usize, y: usize) -> [u8; 3] {
        let i = (y * self.w as usize + x) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    }
}

fn texel(rgba: &[u8], w: usize, h: usize, x: i64, y: i64, ground: [u8; 3]) -> [u8; 3] {
    if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
        return ground;
    }
    let i = (y as usize * w + x as usize) * 4;
    if rgba[i + 3] == 0 { ground } else { [rgba[i], rgba[i + 1], rgba[i + 2]] }
}

/// A fixed pseudo-random walk, so the samples are the same at every run.
fn samples(n: usize, x0: usize, x1: usize, y0: usize, y1: usize) -> Vec<(usize, usize)> {
    let mut s = 0x2545_f491u32;
    let mut next = move |m: usize| {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        (s >> 8) as usize % m
    };
    (0..n).map(|_| (x0 + next(x1 - x0), y0 + next(y1 - y0))).collect()
}

const ARITH_TOP: f32 = BAR_H + CONTENT_TOP + 56.0 + 8.0 + 18.0 + 8.0 + 8.0;

#[test]
fn c1_layout_of_the_section_at_1400() {
    let Some(world) = load() else { return };
    let mut rig = Rig::new(&world, false, state(), W, H);
    let rgba = rig.write("section_1400.png");
    let ids = rig.app.ids().clone();
    assert_eq!(rig.rect(ids.frame), Rect::new(375.0, BAR_H + CONTENT_TOP, 648.0, 804.0));
    assert_eq!(rig.rect(ids.nav), Rect::new(375.0, BAR_H + CONTENT_TOP, 648.0, 56.0));
    let cv = rig.rect(ids.cut_view);
    assert_eq!((cv.x, cv.w, cv.y), (375.0, 648.0, ARITH_TOP));
    assert_eq!((cv.w as f64, cv.h as f64), ids.cut_box, "the laid-out box is the arithmetic one");
    assert_eq!(cv.y + cv.h, BAR_H + CONTENT_TOP + 804.0, "the cut view ends with the frame");
    assert_eq!(rig.rect(ids.under).y, 864.0);
    assert_eq!(ids.tabs.len(), 9, "the section has no 2D and 3D tabs");
    let section = world.section.as_ref().unwrap();
    // Section is pressed: its tab is filled in the colour of the text.
    let t = rig.rect(ids.tabs[8]);
    assert_eq!(rig.px(&rgba, t.x as usize + 3, t.y as usize + 3), DARK_TEXT);
    assert_ne!(ids.next_cut, Id::ROOT);
    assert_ne!(ids.export, Id::ROOT);
    assert_eq!(ids.figures_head, Id::ROOT, "the figures are not built in the section");
    assert_eq!(ids.history_head, Id::ROOT);
    assert_eq!(ids.list_title, "ORES");
    let shown = rig.app.shown().expect("the cut is shown");
    let kinds = shown.play.per_chunk.iter().filter(|f| f.is_some()).count();
    assert_eq!(ids.ores.len(), kinds);
    assert_eq!(kinds, 8);
    assert_eq!(ids.probe_text, "Cut 1 of 4, west to east at z -1,504, x -2,048 to 2,048. True scale. Wheel: closer.");
    assert_eq!(ids.probe_text, text::cut_rest(1, section.cuts.len(), "-1,504", "-2,048", "2,048"));
    println!("C1: frame {:?}, nav {:?}, cut view {:?}, {} ore rows", rig.rect(ids.frame), rig.rect(ids.nav), cv, ids.ores.len());
}

#[test]
fn c2_the_pixels_are_the_pictures() {
    let Some(world) = load() else { return };
    let mut rig = Rig::new(&world, false, state(), W, H);
    let rgba = rig.rgba();
    let ids = rig.app.ids().clone();
    let section = world.section.as_ref().unwrap();
    let shown = rig.app.shown().unwrap();
    let (bw, bh) = rig.cut_box();
    let look = rig.look();

    // The cut view: each pixel is the texel under it or one of its 3 x 3 neighbours; the sky is the page's ground.
    let cv = rig.rect(ids.cut_view);
    let sight = cut::sight(look, bw, bh, section.length, shown.ground.rows).unwrap();
    let sheet = cut::sheet(section, section.cuts[0].cells.as_ref().unwrap(), &shown.ground, &shown.play.layer, sight, look.k, &BTreeSet::new());
    let r = cut::sheet_rect(look, sight, bh);
    let (sw, sh) = (sight.w(), sight.h());
    let mut sky = 0;
    for (x, y) in samples(300, cv.x as usize, (cv.x + cv.w) as usize, cv.y as usize, (cv.y + cv.h) as usize - 1) {
        let (lx, ly) = (x as f32 + 0.5 - cv.x, y as f32 + 0.5 - cv.y);
        let col = ((lx - r.x) / (r.w / sw as f32)).floor() as i64;
        let row = ((ly - r.y) / (r.h / sh as f32)).floor() as i64;
        let got = rig.px(&rgba, x, y);
        let mut allowed = BTreeSet::new();
        for dr in -1..=1 {
            for dc in -1..=1 {
                allowed.insert(texel(&sheet, sw, sh, col + dc, row + dr, DARK_GROUND));
            }
        }
        assert!(allowed.contains(&got), "cut pixel ({x}, {y}) is {got:?}, texel ({col}, {row}) and its neighbours give {allowed:?}");
        if texel(&sheet, sw, sh, col, row, DARK_GROUND) == DARK_GROUND && got == DARK_GROUND {
            sky += 1;
        }
    }
    println!("C2: 300 cut pixels each a texel or its neighbour ({sky} of them sky, the page's ground)");

    // The strip, away from the frame: a texel of the small picture or a neighbour.
    let nav = rig.rect(ids.nav);
    let (nav_rgba, range) = cut::nav_image(section, &shown.ground, 648);
    let frame = cut::nav_frame(look, bw, bh, section.length, &shown.ground, section, range, bw);
    let fr = frame.rect();
    let mut checked = 0;
    for (x, y) in samples(600, nav.x as usize, (nav.x + nav.w) as usize, nav.y as usize, (nav.y + 55.0) as usize) {
        let (lx, ly) = (x as f32 - nav.x, y as f32 - nav.y);
        if lx > fr.x - 4.0 && lx < fr.x + fr.w + 4.0 {
            continue;
        }
        let (col, row) = (lx.floor() as i64, (((ly + 0.5) * NAV_H / 55.0) - 0.5).round() as i64);
        let got = rig.px(&rgba, x, y);
        let allowed: BTreeSet<[u8; 3]> = (-1..=1).flat_map(|dr| (-1..=1).map(move |dc| (dc, dr))).map(|(dc, dr)| texel(&nav_rgba, 648, 56, col + dc, row + dr, DARK_GROUND)).collect();
        assert!(allowed.contains(&got), "strip pixel ({x}, {y}) is {got:?}, allowed {allowed:?}");
        checked += 1;
    }
    assert!(checked >= 150, "only {checked} strip pixels were checked");
    println!("C2: {checked} strip pixels each a texel or its neighbour");

    // The frame: its left and right edges are one pixel of --text on its rows (a fractional edge may take two).
    let mid = (nav.y + fr.y + fr.h / 2.0) as usize;
    let near = |x: f32| (x.floor() as usize - 1..=x.ceil() as usize + 1).filter(|&c| rig.px(&rgba, nav.x as usize + c, mid) == DARK_TEXT).count();
    let (left_hits, right_hits) = (near(fr.x), near(fr.x + fr.w - 1.0));
    assert!(left_hits >= 1 && right_hits >= 1, "the frame's edges: {left_hits} and {right_hits} text-coloured pixels on row {mid}, frame {fr:?}");
    println!("C2: frame {fr:?}: {left_hits} and {right_hits} --text pixels at its edges");

    // The hairlines: under the strip, and the cut view's last row.
    for x in (375..1023).step_by(7) {
        assert_eq!(rig.px(&rgba, x, (BAR_H + CONTENT_TOP) as usize + 55), LINE, "the strip's hairline at x {x}");
        assert_eq!(rig.px(&rgba, x, (cv.y + cv.h) as usize - 1), LINE, "the cut view's hairline at x {x}");
    }
}

#[test]
fn c3_the_wheel_comes_closer_and_goes_away() {
    let Some(world) = load() else { return };
    let section = world.section.as_ref().unwrap();
    let mut rig = Rig::new(&world, false, state(), W, H);
    let cv = rig.rect(rig.app.ids().cut_view);
    let c = centre(cv);
    let (bw, bh) = rig.cut_box();
    let start = rig.look();
    let ground = rig.app.shown().unwrap().ground.clone();
    rig.wheel(c, 1.0);
    let want = cut::wheel(start, ((c.x - cv.x) as f64, (c.y - cv.y) as f64), 1.0, bw, bh, &ground, section.length);
    assert_eq!(rig.look(), want, "one notch in at the centre");
    assert!(rig.look().k > start.k);
    // Far out the scale stops at the least one.
    rig.wheel(c, -30.0);
    let least = cut::scales(bw, bh, &ground, section.length).1;
    assert_eq!(rig.look().k, least);
    println!("C3: k {} -> {} -> {least}", start.k, want.k);

    // The picture of a closer look: four notches in, then the cut carried 150 px.
    let mut rig = Rig::new(&world, false, state(), W, H);
    for _ in 0..4 {
        rig.wheel(c, 1.0);
    }
    let closer = rig.look();
    assert!(closer.k > start.k * 2.0);
    rig.drag(c, Vec2::new(-150.0, 0.0));
    rig.write("section_zoom.png");
    println!("C3: zoom picture at k {}", rig.look().k);
}

#[test]
fn c4_a_drag_carries_the_cut() {
    let Some(world) = load() else { return };
    let section = world.section.as_ref().unwrap();
    let mut rig = Rig::new(&world, false, state(), W, H);
    let c = centre(rig.rect(rig.app.ids().cut_view));
    let (bw, bh) = rig.cut_box();
    let start = rig.look();
    let ground = rig.app.shown().unwrap().ground.clone();
    rig.drag(c, Vec2::new(-100.0, 0.0));
    let after = rig.look();
    assert_eq!(after, cut::dragged(start, -100.0, 0.0, bw, bh, &ground, section.length));
    assert_eq!(after.x, start.x - 100.0, "not clamped here");
    // The cut is as tall as its window: it sits on the bottom line whatever the drag says.
    assert_eq!(after.y, cut::settled(CutLook { y: 55.0, ..after }, bw, bh, &ground, section.length).y);
    assert!(rig.app.state().cut_carry.is_none(), "the release ends the carry");
    // A drag the other way, far, is stopped by the cut's start.
    rig.drag(c, Vec2::new(600.0, 0.0));
    rig.drag(c, Vec2::new(600.0, 0.0));
    assert!(rig.look().x <= 0.0);
    println!("C4: x {} -> {} -> {}", start.x, after.x, rig.look().x);
}

#[test]
fn c5_a_click_on_the_strip_brings_the_window_there() {
    let Some(world) = load() else { return };
    let section = world.section.as_ref().unwrap();
    let mut rig = Rig::new(&world, false, state(), W, H);
    let nav = rig.rect(rig.app.ids().nav);
    let (bw, bh) = rig.cut_box();
    let start = rig.look();
    let ground = rig.app.shown().unwrap().ground.clone();
    let range = cut::nav_range(section, &ground);
    let before = cut::nav_frame(start, bw, bh, section.length, &ground, section, range, bw);
    assert!(before.left > 120.0, "the frame starts well right of the click");
    let p = Vec2::new(nav.x + 100.0, nav.y + 28.0);
    rig.click(p);
    let want = cut::settled(cut::nav_go(start, before.rect().w as f64 / 2.0, 100.0, bw, section.length), bw, bh, &ground, section.length);
    assert_eq!(rig.look(), want);
    let after = cut::nav_frame(rig.look(), bw, bh, section.length, &ground, section, range, bw);
    // The grip is half the frame, so the frame's middle is under the pointer.
    let middle = after.left + after.width.max(5.0) / 2.0;
    assert!((middle - 100.0).abs() < 1.0, "the frame's middle is at {middle}, the click at 100");
    assert!(rig.app.state().nav_carry.is_none());
    // The picture shows it: text-coloured pixels of the frame around the new place on a row of it.
    let rgba = rig.rgba();
    let fr = after.rect();
    let mid = (nav.y + fr.y + fr.h / 2.0) as usize;
    let hits = (nav.x as usize + fr.x as usize - 1..=nav.x as usize + (fr.x + fr.w) as usize + 1).filter(|&x| rig.px(&rgba, x, mid) == DARK_TEXT).count();
    assert!(hits >= 2, "the frame's edges are drawn at the new place ({hits} pixels)");

    // A press on the frame carries it: the same grip, moved 50 px.
    let fr = cut::nav_frame(rig.look(), bw, bh, section.length, &ground, section, range, bw).rect();
    let on = Vec2::new(nav.x + fr.x + fr.w / 2.0, nav.y + 28.0);
    let looked = rig.look();
    rig.drag(on, Vec2::new(50.0, 0.0));
    let moved = cut::nav_frame(rig.look(), bw, bh, section.length, &ground, section, range, bw).left - cut::nav_frame(looked, bw, bh, section.length, &ground, section, range, bw).left;
    assert!((moved - 50.0).abs() < 1e-3, "the frame moved {moved}");
    println!("C5: a click at 100 puts the frame's middle at {middle:.2}; a drag of 50 moves it {moved:.3}");
}

#[test]
fn c6_another_cut() {
    let Some(world) = load() else { return };
    let section = world.section.as_ref().unwrap();
    let mut rig = Rig::new(&world, false, state(), W, H);
    let first = rig.look();
    let button = centre(rig.rect(rig.app.ids().next_cut));
    rig.click(button);
    assert_eq!(rig.app.state().cut_at, 1);
    assert_eq!(rig.app.ids().probe_text, "Cut 2 of 4, west to east at z -992, x -2,048 to 2,048. True scale. Wheel: closer.");
    let shown = rig.app.shown().unwrap();
    assert_eq!(shown.at, 1);
    let (bw, bh) = rig.cut_box();
    assert_eq!(rig.look(), cut::settled(CutLook::FRESH, bw, bh, &shown.ground, section.length), "a new cut starts fresh");
    for _ in 0..2 {
        rig.click(button);
    }
    assert_eq!(rig.app.state().cut_at, 3);
    rig.click(button);
    assert_eq!(rig.app.state().cut_at, 0, "four clicks wrap to the first");
    assert_eq!(rig.look(), first);
    assert!(rig.app.ids().probe_text.starts_with("Cut 1 of 4"));

    // After two clicks, the 2D map's ticks follow the cut: z -192.
    rig.click(button);
    rig.click(button);
    assert_eq!(rig.app.state().cut_at, 2);
    let tab = centre(rig.rect(rig.app.ids().tabs[0]));
    rig.click(tab);
    assert_eq!(rig.app.state().view, 0);
    let look = rig.app.state().look;
    let at = tick_at(&world, look, 2).expect("the cut is on the map");
    assert_ne!(Some(at), tick_at(&world, look, 0), "cut 3's tick is not cut 1's");
    assert_eq!(rig.app.ids().probe_text, text::area_probe("4,096", "512", true));
    // z -192 over 4096 from -2048: (-192 + 2048) / 4096 of the square's side.
    let side = rig.rect(rig.app.ids().frame).w;
    assert!((at - (1856.0 / 4096.0) as f32).abs() < 1e-6);
    let row = (at * side - 0.5).round() as i32;
    let frame = rig.rect(rig.app.ids().frame);
    let muted = rgb(rig.theme.muted);
    let rgba = rig.rgba();
    let got = rig.px(&rgba, frame.x as usize - 6, (frame.y as i32 + row) as usize);
    let close = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= 1);
    assert!(close(got, muted), "the tick at z -192 is {got:?}, want {muted:?}");
    println!("C6: cuts walk 1 -> 2 -> 3 -> 4 -> 1; the 2D tick of cut 3 is at {at:.4} of the square (row {row})");
}

#[test]
fn c7_picking_an_ore() {
    let Some(world) = load() else { return };
    let section = world.section.as_ref().unwrap();
    let mut rig = Rig::new(&world, false, state(), W, H);
    let before = rig.rgba();
    let ids = rig.app.ids().clone();
    let swatch = |rig: &Rig, rgba: &[u8], k: usize| {
        let r = rig.rect(ids.ores[k]);
        rig.px(rgba, r.x as usize + 6, (r.y + r.h / 2.0) as usize)
    };
    assert_eq!(swatch(&rig, &before, 1), ORE_COLOURS[1], "copper is plain before anything is chosen");
    rig.click(centre(rig.rect(ids.ores[2])));
    let chosen: BTreeSet<String> = ["iron".to_string()].into();
    assert_eq!(rig.app.state().chosen_ores, chosen);
    let rgba = rig.rgba();
    assert_eq!(swatch(&rig, &rgba, 2), ORE_COLOURS[2], "iron stays plain");
    assert_ne!(swatch(&rig, &rgba, 1), ORE_COLOURS[1], "copper fades while choosing");
    assert_ne!(swatch(&rig, &rgba, 0), ORE_COLOURS[0]);

    // The cut draws iron only.
    let shown = rig.app.shown().unwrap();
    let (bw, bh) = rig.cut_box();
    let look = rig.look();
    let sight = cut::sight(look, bw, bh, section.length, shown.ground.rows).unwrap();
    let sheet = cut::sheet(section, section.cuts[0].cells.as_ref().unwrap(), &shown.ground, &shown.play.layer, sight, look.k, &chosen);
    let mut found = [0usize; 8];
    for p in sheet.chunks_exact(4) {
        if let Some(k) = ORE_COLOURS.iter().position(|c| p[..3] == c[..] && p[3] == 255) {
            found[k] += 1;
        }
    }
    for (k, n) in found.iter().enumerate() {
        if k == 2 {
            assert!(*n > 0, "iron is drawn");
        } else {
            assert_eq!(*n, 0, "{} is not drawn while iron is chosen", ORE_KINDS[k]);
        }
    }
    // The picture agrees: no other ore colour in the cut view's pixels either.
    let cv = rig.rect(rig.app.ids().cut_view);
    let mut stray = 0;
    for y in cv.y as usize..(cv.y + cv.h) as usize - 1 {
        for x in cv.x as usize..(cv.x + cv.w) as usize {
            let p = rig.px(&rgba, x, y);
            if (0..8).any(|k| k != 2 && p == ORE_COLOURS[k]) {
                stray += 1;
            }
        }
    }
    assert_eq!(stray, 0, "{stray} pixels of another ore");
    println!("C7: iron chosen: {} iron texels, none of the other ores", found[2]);

    // A second click lets it go.
    rig.click(centre(rig.rect(ids.ores[2])));
    assert!(rig.app.state().chosen_ores.is_empty());
    let rgba = rig.rgba();
    assert_eq!(swatch(&rig, &rgba, 1), ORE_COLOURS[1]);

    // The picture to keep: cut 3 with iron chosen.
    let mut rig = Rig::new(&world, false, state(), W, H);
    let button = centre(rig.rect(rig.app.ids().next_cut));
    rig.click(button);
    rig.click(button);
    assert_eq!(rig.app.state().cut_at, 2);
    rig.click(centre(rig.rect(rig.app.ids().ores[2])));
    assert_eq!(rig.app.state().chosen_ores, chosen);
    assert!(rig.app.ids().probe_text.starts_with("Cut 3 of 4"));
    rig.write("section_iron.png");
}

#[test]
fn c8_the_probe_follows_the_pointer() {
    let Some(world) = load() else { return };
    let mut rig = Rig::new(&world, false, state(), W, H);
    let cv = rig.rect(rig.app.ids().cut_view);
    let look = rig.look();
    let at = |lx: f32, ly: f32| Vec2::new(cv.x + lx, cv.y + ly);

    rig.moved(at(319.0, 500.0));
    rig.frame();
    let got = rig.app.ids().probe_text.clone();
    let want = cut::probe(&world, rig.app.shown().unwrap(), look, (319.0, 500.0)).expect("inside the cut");
    assert_eq!(got, want);
    assert!(got.starts_with("x "), "{got}");
    println!("C8: rock: {got}");

    rig.moved(at(5.0, 5.0));
    rig.frame();
    let got = rig.app.ids().probe_text.clone();
    assert!(got.contains("above the ground") && got.contains("Air"), "{got}");
    println!("C8: sky: {got}");

    // A press on the ore of the layer names it.
    let shown = rig.app.shown().unwrap();
    let section = world.section.as_ref().unwrap();
    let (bw, bh) = rig.cut_box();
    let sight = cut::sight(look, bw, bh, section.length, shown.ground.rows).unwrap();
    let l = section.length;
    let ore = (sight.r0..sight.r1)
        .flat_map(|r| (sight.x0..sight.x1).map(move |x| (r, x)))
        .find(|&(r, x)| shown.play.layer[(shown.ground.crop + r) * l + x] != 0)
        .expect("an ore is in sight");
    let (lx, ly) = (look.x + (ore.1 as f64 + 0.5) * look.k, look.y + (ore.0 as f64 + 0.5) * look.k);
    rig.moved(at(lx as f32, ly as f32));
    rig.frame();
    let name = text::ORES[rig.app.shown().unwrap().play.layer[(rig.app.shown().unwrap().ground.crop + ore.0) * l + ore.1] as usize - 1];
    assert!(rig.app.ids().probe_text.contains(name), "{} should name {name}", rig.app.ids().probe_text);

    // Outside the cut: the rest text.
    rig.moved(Vec2::new(100.0, 900.0));
    rig.frame();
    assert_eq!(rig.app.ids().probe_text, "Cut 1 of 4, west to east at z -1,504, x -2,048 to 2,048. True scale. Wheel: closer.");
    // On the strip too.
    let nav = rig.rect(rig.app.ids().nav);
    rig.moved(centre(nav));
    rig.frame();
    assert!(rig.app.ids().probe_text.starts_with("Cut 1 of 4"));
}

#[test]
fn c9_a_world_without_a_section_says_so() {
    let Some(mut world) = load() else { return };
    world.section = None;
    let mut rig = Rig::new(&world, false, state(), W, H);
    let ids = rig.app.ids().clone();
    assert_eq!(text::NO_SECTION, "No section for this preview. Generate it again.");
    let says = |app: &PreviewApp, words: &str| app.list().cmds.iter().any(|c| matches!(c, Cmd::Text { text, .. } if text == words));
    assert!(says(&rig.app, text::NO_SECTION), "the frame says there is no section");
    assert_eq!(ids.next_cut, Id::ROOT);
    assert_eq!(ids.cut, Id::ROOT);
    assert_eq!(ids.list_title, "ORES");
    assert!(ids.ores.is_empty());
    assert!(!says(&rig.app, text::ORE_PICK_HINT), "the pick line is empty");
    assert!(!says(&rig.app, text::ORE_COLUMNS[0]), "no legend");
    assert!(rig.app.shown().is_none());
    assert_eq!(ids.probe_text, "");
    assert_ne!(ids.export, Id::ROOT);
    rig.write("section_none.png");
}

#[test]
fn c10_light_and_narrow() {
    let Some(world) = load() else { return };
    let mut rig = Rig::new(&world, true, state(), W, H);
    let rgba = rig.write("section_1400_light.png");
    let cv = rig.rect(rig.app.ids().cut_view);
    // The top left corner of the cut view is sky: the page's ground.
    assert_eq!(rig.px(&rgba, cv.x as usize + 3, cv.y as usize + 3), LIGHT_GROUND);
    let text = rgb(Theme::light().text);
    assert_eq!(text, [0x17, 0x17, 0x17]);
    let nav = rig.rect(rig.app.ids().nav);
    let (bw, bh) = rig.cut_box();
    let section = world.section.as_ref().unwrap();
    let shown = rig.app.shown().unwrap();
    let range = cut::nav_range(section, &shown.ground);
    let fr = cut::nav_frame(rig.look(), bw, bh, section.length, &shown.ground, section, range, bw).rect();
    let mid = (nav.y + fr.y + fr.h / 2.0) as usize;
    let hits = (nav.x as usize + fr.x as usize - 1..=nav.x as usize + (fr.x + fr.w) as usize + 1).filter(|&x| rig.px(&rgba, x, mid) == text).count();
    assert!(hits >= 2, "the frame is #171717 in the light theme ({hits})");

    // 1000 px: the middle column is 248 wide (the panels at their natural width), so the note wraps on three lines and the cut view starts 36 px lower.
    let mut narrow = Rig::new(&world, false, state(), 1000, 800);
    narrow.write("section_1000.png");
    let ids = narrow.app.ids().clone();
    let nav = narrow.rect(ids.nav);
    let cv = narrow.rect(ids.cut_view);
    let note = text::nav_note(&text::times(cut::nav_times(section.length, nav.w as f64, cut::nav_range(section, &narrow.app.shown().unwrap().ground))));
    assert_eq!(wrap_lines(&note, mere_ui::TextLook::body(mere_ui::Tone::Muted), nav.w).len(), 3);
    assert_eq!(cv.y - nav.y, 98.0 + 36.0, "the cut view moved down by two lines");
    assert_eq!((cv.w as f64, cv.h as f64), ids.cut_box);
    assert_eq!(narrow.rect(ids.frame).w, nav.w);
    println!("C10: 1000 px: nav {nav:?}, cut view {cv:?}");
}

#[test]
fn c11_a_small_window_draws_ores_two_blocks_wide() {
    let Some(world) = load() else { return };
    let mut rig = Rig::new(&world, false, state(), W, 600);
    let cv = rig.rect(rig.app.ids().cut_view);
    assert_eq!(cv.h, 366.0);
    let c = centre(cv);
    rig.wheel(c, -30.0);
    let (bw, bh) = rig.cut_box();
    let section = world.section.as_ref().unwrap();
    let least = cut::scales(bw, bh, &rig.app.shown().unwrap().ground, section.length).1;
    assert_eq!(rig.look().k, least);
    assert!(least < 0.75);
    assert_eq!(rig.app.cut_reach(), Some(true), "the sheet is made with the wide ores");
    // And closer again, the ores are single blocks.
    rig.wheel(c, 40.0);
    assert!(rig.look().k >= 0.75);
    assert_eq!(rig.app.cut_reach(), Some(false));
    println!("C11: least scale {least:.4}");
}

#[test]
fn cli_shot_is_byte_identical_to_the_test_shot() {
    let Some(world) = load() else { return };
    // The command line builds its state from the world (the page's loadWorld): the form holds the run's settings.
    let mut rig = Rig::new(&world, false, PreviewState { view: preview::paint::SECTION, ..PreviewState::for_world(Some(&world)) }, W, H);
    let mine = mere_png::encode(W, H, &rig.rgba());
    let out = tmp("cli_section.png");
    let _ = std::fs::remove_file(&out);
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_preview"))
        .arg("--data")
        .arg(preview::world::data_dir())
        .args(["--view", "section", "--shot"])
        .arg(&out)
        .output()
        .expect("the preview runs");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let a = std::fs::read(&out).unwrap();
    assert_eq!(a.len(), mine.len());
    assert!(a == mine, "the command line's picture differs from the test's");
    println!("CLI: {} bytes, identical", a.len());
}
