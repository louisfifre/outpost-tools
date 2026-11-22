//! What a person does to the map: the wheel, the carry, the click that chooses, the probe line, and the tab that
//! changes the colouring. The first four need no GPU: the shell is built as the application builds it, with
//! events fed to the `Ui` between frames; the last goes through `PreviewApp` and an offscreen renderer.

use std::collections::BTreeSet;

use mere_base::{App, Button, Event, Rect, Vec2};
use mere_ui::{Theme, Ui};
use mere_win32::Renderer;
use preview::map::{Look, point_under};
use preview::paint::{View, paint, rows};
use preview::text::pretty;
use preview::world::World;
use preview::scene;
use preview::{PreviewApp, PreviewState, ShellIds, build};

const SIZE: Vec2 = Vec2::new(1400.0, 940.0);

fn load() -> Option<World> {
    let dir = &preview::world::data_dir();
    if !dir.join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no out folder");
        return None;
    }
    Some(World::load(dir).expect("the data loads"))
}

/// The shell with a world, built as `PreviewApp::frame` builds it.
struct Rig {
    ui: Ui,
    s: PreviewState,
    world: World,
    ids: ShellIds,
}

impl Rig {
    fn new(world: World, s: PreviewState) -> Rig {
        let mut r = Rig { ui: Ui::new(Theme::dark()), s, world, ids: ShellIds::default() };
        // The first frame learns where everything is: the pointer is judged against the last one.
        r.frame();
        r.frame();
        r
    }

    fn frame(&mut self) {
        for _ in 0..3 {
            let before = self.s.clone();
            self.ui.begin_frame(SIZE);
            self.ids = build(&mut self.ui, &mut self.s, SIZE, Some(&self.world), None, None);
            self.ui.end_frame();
            if !self.ui.want_redraw() && self.s == before {
                break;
            }
        }
    }

    fn rect(&self) -> Rect {
        self.ui.rect(self.ids.map).expect("the map is built")
    }

    /// The 3D view's square.
    fn relief(&self) -> Rect {
        self.ui.rect(self.ids.relief).expect("the 3D view is built")
    }

    fn centre(&self) -> Vec2 {
        let r = self.rect();
        Vec2::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
    }

    fn moved(&mut self, pos: Vec2) {
        self.ui.event(&Event::PointerMoved { pos });
    }
    fn down(&mut self, pos: Vec2) {
        self.ui.event(&Event::PointerDown { pos, button: Button::Left, clicks: 1 });
    }
    fn up(&mut self, pos: Vec2) {
        self.ui.event(&Event::PointerUp { pos, button: Button::Left });
    }
    /// A whole click: the pointer arrives, goes down, a frame, goes up, a frame.
    fn click(&mut self, pos: Vec2) {
        self.moved(pos);
        self.down(pos);
        self.frame();
        self.up(pos);
        self.frame();
    }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn i1_the_wheel_comes_closer_to_the_point_under_it() {
    let Some(world) = load() else { return };
    let mut r = Rig::new(world, PreviewState::default());
    let c = r.centre();
    r.moved(c);
    r.ui.event(&Event::Wheel { pos: c, delta: 1.0 });
    r.frame();
    let l = r.s.look;
    assert!(near(l.k, 1.25) && near(l.x, -0.125) && near(l.y, -0.125), "{l:?}");
    // Away from the user is the other way, and one notch is one step.
    r.ui.event(&Event::Wheel { pos: c, delta: -1.0 });
    r.frame();
    assert_eq!(r.s.look, Look::REST);
}

#[test]
fn i2_a_press_that_travels_carries_the_map_and_chooses_nothing() {
    let Some(world) = load() else { return };
    let start = Look { k: 2.0, x: -0.5, y: -0.5 };
    let mut r = Rig::new(world, PreviewState { look: start, ..PreviewState::default() });
    let (c, side) = (r.centre(), r.rect().w);
    r.moved(c);
    r.down(c);
    r.frame();
    let to = Vec2::new(c.x + 100.0, c.y);
    r.moved(to);
    r.frame();
    r.up(to);
    r.frame();
    let l = r.s.look;
    assert!(near(l.x, -0.5 + 100.0 / side) && near(l.y, -0.5) && near(l.k, 2.0), "{l:?}");
    assert!(r.s.chosen.iter().all(BTreeSet::is_empty), "a carry is not a click");
    assert!(r.s.carry.is_none(), "the press is over");
}

#[test]
fn i3_a_still_click_chooses_the_biome_under_it_and_a_second_unchooses() {
    let Some(world) = load() else { return };
    let mut r = Rig::new(world, PreviewState { view: 0, ..PreviewState::default() });
    // A little off the centre, so that the cross is not what is clicked.
    let c = r.centre();
    let p = Vec2::new(c.x + 60.0, c.y + 40.0);
    let rect = r.rect();
    let (_, _, i) = point_under(&r.world, Look::REST, rect.w, Vec2::new(p.x - rect.x, p.y - rect.y));
    let want = r.world.biomes[r.world.biome_at[i] as usize].id.clone();
    r.click(p);
    assert_eq!(r.s.chosen[0], BTreeSet::from([want.clone()]));
    assert!(r.s.chosen[1..].iter().all(BTreeSet::is_empty));
    // The next paint dims the others: the choice is what paint() reads.
    let img = paint(&r.world, View::Biomes, true, &r.s.chosen[0]);
    let plain = paint(&r.world, View::Biomes, true, &BTreeSet::new());
    assert_ne!(img, plain);
    r.click(p);
    assert!(r.s.chosen[0].is_empty(), "the second click takes it away");
    let (_, ids) = rows(&r.world, View::Biomes).unwrap();
    assert!(ids.contains(&want));
}

#[test]
fn i4_the_probe_names_the_point_under_the_pointer_and_rests_on_the_area() {
    let Some(world) = load() else { return };
    let mut r = Rig::new(world, PreviewState::default());
    // At rest: the area, with the ticks of the cut.
    assert!(r.ids.probe_text.starts_with("4,096 \u{d7} 4,096 blocks"), "{}", r.ids.probe_text);
    assert!(r.ids.probe_text.ends_with(" Ticks: the cut."), "{}", r.ids.probe_text);
    let rect = r.rect();
    let p = Vec2::new(rect.x + 100.0, rect.y + 200.0);
    r.moved(p);
    r.frame();
    let (col, row, i) = point_under(&r.world, Look::REST, rect.w, Vec2::new(100.0, 200.0));
    let biome = pretty(&r.world.biomes[r.world.biome_at[i] as usize].id);
    let t = r.ids.probe_text.clone();
    assert!(t.starts_with("x "), "{t}");
    assert!(t.ends_with(&biome), "{t} should end with {biome}");
    assert!(t.contains(&format!("height {}", r.world.heights[i])), "{t}");
    println!("probe over ({col},{row}): {t}");
    // Leaving the map brings the area back.
    r.moved(Vec2::new(5.0, 5.0));
    r.frame();
    assert!(r.ids.probe_text.ends_with(" Ticks: the cut."), "{}", r.ids.probe_text);
}

#[test]
fn i5_the_tab_changes_the_colouring_through_the_whole_application() {
    let Some(world) = load() else { return };
    let (w, h) = (1400u32, 940u32);
    let mut app = PreviewApp::new(Theme::dark(), true, SIZE, PreviewState::default(), Some(world.clone()));
    let mut r = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
    app.start(&mut r);
    app.event(Event::Resized { width_px: w, height_px: h, scale: 1.0 });
    let picture = |app: &mut PreviewApp, r: &mut Renderer| {
        app.frame(r, 0.0);
        r.read_rgba().unwrap()
    };
    let first = picture(&mut app, &mut r);
    assert_eq!(app.state().view, View::Height.index(), "the page opens on the relief");

    // A pixel well inside a texel where the two colourings differ.
    let frame = app.ui().rect(app.ids().frame).unwrap();
    let n = world.samples;
    let heights = paint(&world, View::Height, true, &BTreeSet::new());
    let biomes = paint(&world, View::Biomes, true, &BTreeSet::new());
    let tex = |img: &[u8], col: usize, row: usize| [img[(row * n + col) * 4], img[(row * n + col) * 4 + 1], img[(row * n + col) * 4 + 2]];
    let (mut at, mut found) = ((0, 0), false);
    'search: for row in (100..200).step_by(3) {
        for col in (60..200).step_by(3) {
            // The screen pixel at the middle of the texel, off the grid's lines and the cross.
            let (x, y) = (((col as f32 + 0.5) / n as f32 * frame.w) as i32, ((row as f32 + 0.5) / n as f32 * frame.h) as i32);
            let off_line = (0..=8).all(|j| (x - (j as f32 * frame.w / 8.0).round() as i32).abs() > 1 && (y - (j as f32 * frame.h / 8.0).round() as i32).abs() > 1);
            if off_line && tex(&heights, col, row) != tex(&biomes, col, row) {
                (at, found) = ((x, y), true);
                let (u, v) = (col, row);
                // The pixel must lie in that texel, not the next one.
                if ((x as f32 + 0.5) / frame.w * n as f32) as usize == u && ((y as f32 + 0.5) / frame.h * n as f32) as usize == v {
                    break 'search;
                }
                found = false;
            }
        }
    }
    assert!(found, "no pixel tells the views apart");
    let px = |rgba: &[u8]| {
        let i = (((frame.y as i32 + at.1) as usize) * w as usize + (frame.x as i32 + at.0) as usize) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    };
    let (col, row) = ((((at.0 as f32 + 0.5) / frame.w * n as f32) as usize), (((at.1 as f32 + 0.5) / frame.h * n as f32) as usize));
    assert_eq!(px(&first), tex(&heights, col, row), "the relief to begin with");

    // Click the Biomes tab: the frame after it is painted with the biomes' colours.
    let tab: Rect = app.ui().rect(app.ids().tabs[0]).unwrap();
    let c = Vec2::new(tab.x + tab.w / 2.0, tab.y + tab.h / 2.0);
    for e in [Event::PointerMoved { pos: c }, Event::PointerDown { pos: c, button: Button::Left, clicks: 1 }, Event::PointerUp { pos: c, button: Button::Left }] {
        app.event(e);
    }
    let second = picture(&mut app, &mut r);
    assert_eq!(app.state().view, 0, "the click set the view");
    assert_eq!(px(&second), tex(&biomes, col, row), "the biomes' colour");
    assert_ne!(px(&second), tex(&heights, col, row), "and no longer the relief's");
}

// The 3D view's orbit.

fn rig_3d(world: World) -> Rig {
    Rig::new(world, PreviewState { view: View::Height.index(), in3d: true, ..PreviewState::default() })
}

/// Lets the camera glide to rest, as the frames of the window do.
fn rest(r: &mut Rig) -> mere_base::Vec3 {
    let orbit = r.s.orbit.as_mut().expect("the orbit was made");
    for _ in 0..600 {
        if !scene::settle(orbit, &r.world, scene::Level::default(), 1.0 / 60.0) {
            return orbit.eye();
        }
    }
    panic!("never came to rest");
}

fn yaw(eye: mere_base::Vec3) -> f32 {
    eye.x.atan2(eye.z)
}

/// A press at `from`, a move to `to` in ten steps with a frame each, then the release and a frame.
fn drag(r: &mut Rig, from: Vec2, to: Vec2) {
    r.moved(from);
    r.down(from);
    r.frame();
    for i in 1..=10 {
        let t = i as f32 / 10.0;
        r.moved(Vec2::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t));
        r.frame();
    }
    r.up(to);
    r.frame();
}

#[test]
fn o1_a_drag_on_the_3d_view_turns_the_camera_by_a_full_turn_per_side() {
    let Some(world) = load() else { return };
    let mut r = rig_3d(world);
    let (c, side) = (Vec2::new(r.relief().x + r.relief().w / 2.0, r.relief().y + r.relief().h / 2.0), r.relief().w);
    let start = rest(&mut r);
    drag(&mut r, c, Vec2::new(c.x - 100.0, c.y));
    let eye = rest(&mut r);
    let want = std::f32::consts::TAU * 100.0 / side;
    assert!((yaw(eye) - want).abs() < 0.01, "yaw {} against {want}", yaw(eye));
    assert!(eye.x > 0.0);
    drag(&mut r, Vec2::new(c.x - 100.0, c.y), c);
    let back = rest(&mut r);
    assert!((back.x - start.x).abs() < 1.0, "right undoes left: {} against {}", back.x, start.x);
}

#[test]
fn o2_a_press_outside_the_stage_does_not_turn() {
    let Some(world) = load() else { return };
    let mut r = rig_3d(world);
    rest(&mut r);
    let before = r.s.orbit.clone();
    // The page's padding, then across the stage.
    let (y, far) = (r.relief().y + 100.0, r.relief().x + r.relief().w + 50.0);
    drag(&mut r, Vec2::new(10.0, y), Vec2::new(far, y));
    rest(&mut r);
    assert_eq!(r.s.orbit, before);
    assert!(r.s.turn_from.is_none());
}

#[test]
fn o3_the_wheel_over_the_stage_zooms_and_elsewhere_does_not() {
    let Some(world) = load() else { return };
    let mut r = rig_3d(world);
    let before = {
        let eye = rest(&mut r);
        eye.length()
    };
    let c = Vec2::new(r.relief().x + r.relief().w / 2.0, r.relief().y + r.relief().h / 2.0);
    r.moved(c);
    r.ui.event(&Event::Wheel { pos: c, delta: 1.0 });
    r.frame();
    let after = rest(&mut r).length();
    assert!((after / before - 0.95).abs() < 1e-3, "{after} / {before}");

    let kept = r.s.orbit.clone();
    let list = r.ui.rect(r.ids.list_scroll).expect("the list was built");
    let p = Vec2::new(list.x + list.w / 2.0, list.y + list.h / 2.0);
    r.moved(p);
    r.ui.event(&Event::Wheel { pos: p, delta: 1.0 });
    r.frame();
    rest(&mut r);
    assert_eq!(r.s.orbit, kept);
}

#[test]
fn o4_a_drag_that_leaves_the_stage_keeps_turning_until_the_release() {
    let Some(world) = load() else { return };
    let mut r = rig_3d(world);
    rest(&mut r);
    let rect = r.relief();
    // Starts 14 px inside the right edge and ends 86 px outside it.
    let from = Vec2::new(rect.x + rect.w - 14.0, rect.y + 200.0);
    drag(&mut r, from, Vec2::new(from.x + 100.0, from.y));
    let eye = rest(&mut r);
    let want = -std::f32::consts::TAU * 100.0 / rect.w;
    assert!((yaw(eye) - want).abs() < 0.01, "yaw {} against {want}", yaw(eye));
}

// The 3D view's levels.

/// The middle of the rect of `id`.
fn middle(ui: &Ui, id: mere_ui::Id) -> Vec2 {
    let r = ui.rect(id).expect("the part is built");
    Vec2::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

#[test]
fn l1_the_levels_are_under_the_3d_view_and_a_click_chooses_one() {
    let Some(world) = load() else { return };
    // In 2D there is no level to choose.
    let flat = Rig::new(world.clone(), PreviewState::default());
    assert_eq!(flat.ids.levels, [mere_ui::Id::ROOT; 3]);
    let mut r = rig_3d(world);
    // Showing them moves nothing else: the stage's square and Export are where they are in 2D.
    assert_eq!(r.ui.rect(r.ids.frame), flat.ui.rect(flat.ids.frame));
    assert_eq!(r.ui.rect(r.ids.export), flat.ui.rect(flat.ids.export));
    // They sit in the row under the view, before Export, from the lightest to the finest.
    let (view, export) = (r.relief(), r.ui.rect(r.ids.export).unwrap());
    let tabs: Vec<Rect> = r.ids.levels.iter().map(|id| r.ui.rect(*id).expect("a level's tab")).collect();
    assert!(tabs.iter().all(|t| t.y >= view.y + view.h && t.x >= view.x && t.x + t.w <= export.x), "{tabs:?} against {view:?} and {export:?}");
    assert!(tabs[0].x + tabs[0].w <= tabs[1].x && tabs[1].x + tabs[1].w <= tabs[2].x);
    // A window opens on the blocks; a click on a level chooses it.
    assert_eq!(r.s.level, scene::Level::Blocks);
    for level in [scene::Level::Fine, scene::Level::Slopes, scene::Level::Blocks] {
        let at = middle(&r.ui, r.ids.levels[level.index()]);
        r.click(at);
        assert_eq!(r.s.level, level);
    }
    // The camera is not put back by a change of level.
    rest(&mut r);
    let before = r.s.orbit.clone();
    let at = middle(&r.ui, r.ids.levels[0]);
    r.click(at);
    assert_eq!((r.s.level, &r.s.orbit), (scene::Level::Slopes, &before));
}

/// A device that counts the meshes it is sent.
struct Counting<'a> {
    inner: &'a mut Renderer,
    meshes: usize,
}

impl mere_base::Gfx for Counting<'_> {
    fn size_px(&self) -> (u32, u32) {
        self.inner.size_px()
    }
    fn scale(&self) -> f32 {
        self.inner.scale()
    }
    fn upload_image(&mut self, id: mere_base::ImageId, width: u32, height: u32, rgba: &[u8]) {
        self.inner.upload_image(id, width, height, rgba)
    }
    fn upload_mesh(&mut self, id: mere_base::MeshId, vertices: &[mere_base::Vertex], indices: &[u32]) {
        self.meshes += 1;
        self.inner.upload_mesh(id, vertices, indices)
    }
    fn begin(&mut self, ground: mere_base::Color) {
        self.inner.begin(ground)
    }
    fn draw_mesh(&mut self, viewport: Rect, mesh: mere_base::MeshId, view_proj: &mere_base::Mat4, material: mere_base::Material) {
        self.inner.draw_mesh(viewport, mesh, view_proj, material)
    }
    fn draw_list(&mut self, list: &mere_base::DrawList) {
        self.inner.draw_list(list)
    }
    fn end(&mut self) {
        self.inner.end()
    }
}

#[test]
fn l2_a_level_is_built_when_the_3d_view_shows_it_and_draws_its_own_land() {
    let Some(world) = load() else { return };
    let (w, h) = (1400u32, 940u32);
    let mut app = PreviewApp::new(Theme::dark(), true, SIZE, PreviewState::default(), Some(world.clone()));
    let mut r = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
    let mut device = Counting { inner: &mut r, meshes: 0 };
    app.start(&mut device);
    app.event(Event::Resized { width_px: w, height_px: h, scale: 1.0 });
    app.frame(&mut device, 0.0);
    assert_eq!(device.meshes, 0, "a window that stays in 2D builds no mesh");

    app.state_mut().in3d = true;
    let mut pictures = Vec::new();
    for level in scene::Level::ALL {
        // The fine blocks of a wide map are too heavy for a test: the scene's own tests cover them.
        if level == scene::Level::Fine && world.samples > 1024 {
            continue;
        }
        app.state_mut().level = level;
        let before = device.meshes;
        app.frame(&mut device, 0.0);
        assert!(device.meshes > before, "{level:?} is built when the 3D view shows it");
        let built = device.meshes;
        app.frame(&mut device, 0.0);
        assert_eq!(device.meshes, built, "{level:?} is built once");
        pictures.push(device.inner.read_rgba().unwrap());
    }
    assert_ne!(pictures[0], pictures[1], "the slopes and the blocks are not the same land");
    // The fine blocks are the blocks on a map the blocks do not thin.
    if let Some(fine) = pictures.get(2) {
        assert_eq!(*fine != pictures[1], world.samples > scene::COARSE_MOST);
    }

    // To 2D and back: the device still holds the level's meshes.
    let built = device.meshes;
    app.state_mut().in3d = false;
    app.frame(&mut device, 0.0);
    app.state_mut().in3d = true;
    app.frame(&mut device, 0.0);
    assert_eq!(device.meshes, built);
}