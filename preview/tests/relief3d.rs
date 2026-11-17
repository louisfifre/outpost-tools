//! The 3D view in the stage, drawn offscreen through the whole application. The scene is a mesh drawn with the
//! D3D viewport set to the stage's square, so the proofs are about pixels: nothing outside the square differs
//! from the interface list drawn alone, and inside it the land is there. The pictures go to the target's tmp
//! folder; the ones worth keeping are copied to docs/shots.

use std::path::{Path, PathBuf};

use mere_base::{App, Button, Event, Rect, Vec2};
use mere_ui::Theme;
use mere_win32::Renderer;
use preview::paint::View;
use preview::world::World;
use preview::{PreviewApp, PreviewState, scene, text};

const W: u32 = 1400;
const H: u32 = 940;
const DARK_GROUND: [u8; 3] = [0x00, 0x00, 0x00];
const LIGHT_GROUND: [u8; 3] = [0xfa, 0xfa, 0xf8];
const DARK_TEXT: [u8; 3] = [0xec, 0xec, 0xec];

fn load() -> Option<World> {
    let dir = &preview::world::data_dir();
    if !dir.join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no out folder");
        return None;
    }
    Some(World::load(dir).expect("the data loads"))
}

fn state_3d(view: usize) -> PreviewState {
    PreviewState { view, in3d: true, ..PreviewState::default() }
}

fn write(rgba: &[u8], name: &str) {
    let path: PathBuf = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, mere_png::encode(W, H, rgba)).unwrap();
    println!("wrote {}", path.display());
}

fn px(rgba: &[u8], x: usize, y: usize) -> [u8; 3] {
    let i = (y * W as usize + x) * 4;
    [rgba[i], rgba[i + 1], rgba[i + 2]]
}

/// A: the frame as the window shows it. B: the interface list of that frame drawn alone on the ground.
/// Where the two agree, the scene covered nothing.
fn take(world: &World, light: bool, state: PreviewState, name: &str) -> (Vec<u8>, Vec<u8>, PreviewApp, Rect, Renderer) {
    let theme = if light { Theme::light() } else { Theme::dark() };
    let mut r = Renderer::offscreen(W, H, 1.0).expect("offscreen renderer");
    let mut app = PreviewApp::new(theme, !light, Vec2::new(W as f32, H as f32), state, Some(world.clone()));
    app.start(&mut r);
    app.event(Event::Resized { width_px: W, height_px: H, scale: 1.0 });
    app.frame(&mut r, 0.0);
    let a = r.read_rgba().unwrap();
    let b = reference(&mut r, &app, &theme);
    write(&a, name);
    let rect = app.ui().rect(app.ids().relief).expect("the relief canvas was built");
    (a, b, app, rect, r)
}

fn reference(r: &mut Renderer, app: &PreviewApp, theme: &Theme) -> Vec<u8> {
    use mere_base::Gfx;
    r.begin(theme.ground);
    r.draw_list(app.list());
    r.end();
    r.read_rgba().unwrap()
}

fn inside(rect: Rect, x: usize, y: usize) -> bool {
    (x as f32) >= rect.x && (x as f32) < rect.x + rect.w && (y as f32) >= rect.y && (y as f32) < rect.y + rect.h
}

/// How many pixels outside `rect` differ between the two pictures.
fn leaks(a: &[u8], b: &[u8], rect: Rect) -> usize {
    let mut n = 0;
    for y in 0..H as usize {
        for x in 0..W as usize {
            if !inside(rect, x, y) && px(a, x, y) != px(b, x, y) {
                n += 1;
            }
        }
    }
    n
}

fn share_not(a: &[u8], rect: Rect, ground: [u8; 3]) -> f32 {
    let (mut n, mut all) = (0, 0);
    for y in rect.y as usize..(rect.y + rect.h) as usize {
        for x in rect.x as usize..(rect.x + rect.w) as usize {
            all += 1;
            if px(a, x, y) != ground {
                n += 1;
            }
        }
    }
    n as f32 / all as f32
}

fn tab_pixel(a: &[u8], app: &PreviewApp, index: usize) -> [u8; 3] {
    let t = app.ui().rect(app.ids().tabs[index]).expect("the tab was built");
    px(a, t.x as usize + 3, t.y as usize + 3)
}

#[test]
fn s1_stage_3d_relief_is_drawn_inside_the_frame_only() {
    let Some(world) = load() else { return };
    let (a, b, app, rect, _r) = take(&world, false, state_3d(2), "relief3d_1400.png");
    // 28 + the settings panel (319) + 28 from the left; the middle column is 648 wide at 1400.\n    assert_eq!(rect, Rect::new(375.0, 50.0, 648.0, 648.0));
    assert_eq!(app.ui().rect(app.ids().frame), Some(rect));
    assert_eq!(leaks(&a, &b, rect), 0, "the scene stays inside the square");
    let share = share_not(&a, rect, DARK_GROUND);
    assert!(share >= 0.30, "the land fills {share} of the square");
    assert_eq!(tab_pixel(&a, &app, 2), DARK_TEXT, "Relief is pressed");
    assert_eq!(tab_pixel(&a, &app, 7), DARK_TEXT, "3D is pressed");
    assert_ne!(tab_pixel(&a, &app, 6), DARK_TEXT, "2D is not");
    assert_eq!(px(&a, rect.x as usize + 2, rect.y as usize + 2), DARK_GROUND, "the scene's background is --ground");
    assert_eq!(app.ids().probe_text, text::area_probe("4,096", "512", false), "no Ticks in 3D");
    assert_eq!(app.painted_view(), Some(View::Height));
    println!("S1: land share of the square {share:.3}");
}

#[test]
fn s2_zoomed_terrain_is_cut_at_the_frame_edges() {
    let Some(world) = load() else { return };
    // The notches asked for: the wheel stops at the zoom range's near end, so any larger number gives the same.
    const NOTCHES: f32 = 30.0;
    let mut orbit = scene::orbit(&world, None);
    orbit.zoom(NOTCHES);
    let mut frames = 0;
    while scene::settle(&mut orbit, &world, 1.0 / 60.0) {
        frames += 1;
        assert!(frames < 600, "never came to rest");
    }
    let state = PreviewState { orbit: Some(orbit), ..state_3d(2) };
    let (a, b, _app, rect, _r) = take(&world, false, state, "relief3d_zoom.png");
    let leaked = leaks(&a, &b, rect);
    assert_eq!(leaked, 0, "{leaked} pixels outside the square differ: a scissor rect would be needed");
    let (x0, y0) = (rect.x as usize, rect.y as usize);
    let (x1, y1) = ((rect.x + rect.w) as usize - 1, (rect.y + rect.h) as usize - 1);
    let lines: [(&str, Vec<(usize, usize)>); 4] = [
        ("top", (x0..=x1).map(|x| (x, y0 + 1)).collect()),
        ("bottom", (x0..=x1).map(|x| (x, y1 - 1)).collect()),
        ("left", (y0..=y1).map(|y| (x0 + 1, y)).collect()),
        ("right", (y0..=y1).map(|y| (x1 - 1, y)).collect()),
    ];
    for (name, points) in lines {
        let land = points.iter().filter(|&&(x, y)| px(&a, x, y) != DARK_GROUND).count() as f32 / points.len() as f32;
        assert!(land >= 0.90, "{name} edge line is {land} terrain");
        println!("S2: {name} edge line {land:.3} terrain");
    }
}

#[test]
fn s3_light_theme_ground_and_drape() {
    let Some(world) = load() else { return };
    let (a, b, _app, rect, _r) = take(&world, true, state_3d(2), "relief3d_light.png");
    assert_eq!(px(&a, rect.x as usize + 2, rect.y as usize + 2), LIGHT_GROUND);
    assert_eq!(leaks(&a, &b, rect), 0);
    assert!(share_not(&a, rect, LIGHT_GROUND) >= 0.30);
}

/// A click as the tests of the map make it: the pointer arrives, goes down, a frame, goes up, a frame.
fn click(app: &mut PreviewApp, r: &mut Renderer, tab: usize) {
    let t = app.ui().rect(app.ids().tabs[tab]).expect("the tab was built");
    let p = Vec2::new(t.x + t.w / 2.0, t.y + t.h / 2.0);
    app.event(Event::PointerMoved { pos: p });
    app.event(Event::PointerDown { pos: p, button: Button::Left, clicks: 1 });
    app.frame(r, 0.0);
    app.event(Event::PointerUp { pos: p, button: Button::Left });
    app.frame(r, 0.0);
}

#[test]
fn s4_a_view_tab_repaints_the_drape_and_2d_comes_back() {
    let Some(world) = load() else { return };
    let (a, _b, mut app, rect, mut r) = take(&world, false, state_3d(2), "relief3d_s4_start.png");
    click(&mut app, &mut r, 0);
    assert_eq!(app.painted_view(), Some(View::Biomes));
    let c = r.read_rgba().unwrap();
    write(&c, "relief3d_biomes.png");
    let (mut changed, mut all) = (0, 0);
    for y in rect.y as usize..(rect.y + rect.h) as usize {
        for x in rect.x as usize..(rect.x + rect.w) as usize {
            all += 1;
            if px(&a, x, y) != px(&c, x, y) {
                changed += 1;
            }
        }
    }
    let share = changed as f32 / all as f32;
    assert!(share >= 0.20, "only {share} of the square changed");
    println!("S4: {share:.3} of the square changed with the biomes drape");

    let kept = app.state().orbit.clone();
    assert!(kept.is_some());
    click(&mut app, &mut r, 6);
    assert_ne!(app.ids().map, mere_ui::Id::ROOT);
    assert_eq!(app.ids().relief, mere_ui::Id::ROOT);
    click(&mut app, &mut r, 7);
    assert_ne!(app.ids().relief, mere_ui::Id::ROOT);
    assert_eq!(app.state().orbit, kept, "the camera is kept across 2D and 3D");
    click(&mut app, &mut r, 5);
    assert_eq!(app.ids().tabs.len(), 6, "the section has no 2D and 3D tabs");
    assert_eq!(app.ids().relief, mere_ui::Id::ROOT);
}
