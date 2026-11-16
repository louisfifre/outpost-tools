//! The map in the stage, drawn offscreen through the whole application and checked pixel by pixel against
//! `paint()`. The pictures go to the target's tmp folder; the ones worth keeping are copied to docs/shots.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use mere_base::{Rect, Vec2};
use mere_ui::Theme;
use preview::map::{Look, tick_at};
use preview::paint::{View, paint};
use preview::world::World;
use preview::{PreviewApp, PreviewState};

const W: u32 = 1400;
const H: u32 = 940;

fn load() -> Option<World> {
    let dir = &preview::world::data_dir();
    if !dir.join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no out folder");
        return None;
    }
    Some(World::load(dir).expect("the data loads"))
}

struct Shot {
    rgba: Vec<u8>,
    frame: Rect,
    side: f32,
    look: Look,
}

impl Shot {
    fn px(&self, x: i32, y: i32) -> [u8; 3] {
        let i = (y as usize * W as usize + x as usize) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }
    /// The pixel at (rx, ry) of the frame.
    fn at(&self, rx: i32, ry: i32) -> [u8; 3] {
        self.px(self.frame.x as i32 + rx, self.frame.y as i32 + ry)
    }
}

fn surface_kinds(world: &World) -> BTreeSet<String> {
    let s = world.structures.as_ref().expect("structures");
    (0..s.ids.len()).filter(|&i| !s.under[i] && s.count[i] > 0).map(|i| s.ids[i].clone()).collect()
}

fn take(world: &World, light: bool, state: PreviewState, name: &str) -> Shot {
    let theme = if light { Theme::light() } else { Theme::dark() };
    let look = state.look;
    let mut app = PreviewApp::new(theme, !light, Vec2::new(W as f32, H as f32), state, Some(world.clone()));
    let rgba = mere_win32::shot(&mut app, W, H, 1.0).expect("offscreen frame");
    let path: PathBuf = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, mere_png::encode(W, H, &rgba)).unwrap();
    println!("wrote {}", path.display());
    let frame = app.ui().rect(app.ids().frame).expect("the frame was built");
    assert_eq!(frame.x.fract(), 0.0, "an integer frame keeps the pixel arithmetic exact");
    Shot { rgba, frame, side: frame.w, look }
}

fn state(view: usize) -> PreviewState {
    PreviewState { view, ..PreviewState::default() }
}

/// The texel (col, row) of `img` as RGB.
fn texel(img: &[u8], n: usize, col: usize, row: usize) -> [u8; 3] {
    let i = (row * n + col) * 4;
    [img[i], img[i + 1], img[i + 2]]
}

fn near(a: [u8; 3], b: [u8; 3], tolerance: i32) -> bool {
    a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= tolerance)
}

#[test]
fn m1_the_five_views_and_the_themes() {
    let Some(world) = load() else { return };
    take(&world, true, state(0), "map_biomes_light.png");
    take(&world, true, state(2), "map_relief_light.png");
    take(&world, false, state(0), "map_biomes.png");
    take(&world, false, state(1), "map_flat.png");
    take(&world, false, state(3), "map_temperature.png");
    let mut s = state(4);
    s.chosen[4] = surface_kinds(&world);
    take(&world, false, s, "map_structures.png");
    let mut s = state(0);
    s.chosen[0] = ["minecraft:badlands", "minecraft:savanna"].iter().map(|s| s.to_string()).collect();
    let shot = take(&world, false, s, "map_biomes_chosen.png");
    // A chosen biome keeps its colours and the rest steps back to grey: some pixel of the frame is a pure grey.
    let greys = (0..shot.side as i32).step_by(7).flat_map(|x| (0..shot.side as i32).step_by(7).map(move |y| (x, y))).filter(|&(x, y)| {
        let c = shot.at(x, y);
        c[0] == c[1] && c[1] == c[2] && c[0] > 10
    });
    assert!(greys.count() > 200, "the biomes that are not chosen step back to grey");
}

#[test]
fn m2_rest_look_shows_each_point_as_its_texel() {
    let Some(world) = load() else { return };
    let shot_dark = take(&world, false, state(2), "map_relief.png");
    let img = paint(&world, View::Height, true, &BTreeSet::new());
    let n = world.samples;
    let side = shot_dark.side;
    let mut checked = 0;
    let mut seed = 12345u64;
    for _ in 0..300 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let rx = ((seed >> 33) % side as u64) as i32;
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let ry = ((seed >> 33) % side as u64) as i32;
        let near_line = |v: i32| (0..=8).any(|j| (v - (j as f32 * side / 8.0).round() as i32).abs() <= 1);
        let c = (side / 2.0).round() as i32;
        if near_line(rx) || near_line(ry) || (rx - c).abs() <= 9 && (ry - c).abs() <= 9 || rx < 1 || ry < 1 || rx > side as i32 - 2 || ry > side as i32 - 2 {
            continue;
        }
        let u = ((rx as f32 + 0.5) / side * n as f32) as i32;
        let v = ((ry as f32 + 0.5) / side * n as f32) as i32;
        let got = shot_dark.at(rx, ry);
        let around = (-1..=1).flat_map(|dv| (-1..=1).map(move |du| (du, dv)));
        let found = around.clone().any(|(du, dv)| {
            let (col, row) = ((u + du).clamp(0, n as i32 - 1) as usize, (v + dv).clamp(0, n as i32 - 1) as usize);
            texel(&img, n, col, row) == got
        });
        assert!(found, "pixel ({rx},{ry}) is {got:?}, not one of the 3 x 3 texels around ({u},{v})");
        checked += 1;
    }
    assert!(checked >= 200, "only {checked} pixels were checked");
    println!("rest look: {checked} pixels each equal a texel around their place");

    // The ticks of the cut: 9 px left of the frame at the row of the first cut, in the muted colour.
    let at = tick_at(&world, Look::REST, 0).expect("the cut is there");
    let row = (at * side - 0.5).round() as i32;
    let t = Theme::dark();
    let muted = [t.muted.r, t.muted.g, t.muted.b].map(|v| (v * 255.0).round() as u8);
    for (x, which) in [(-6, "left"), (side as i32 + 6, "right")] {
        let got = shot_dark.px(shot_dark.frame.x as i32 + x, shot_dark.frame.y as i32 + row);
        assert!(near(got, muted, 1), "{which} tick: {got:?}, want {muted:?}");
    }
    // And only there: a row away is the ground.
    assert!(!near(shot_dark.px(shot_dark.frame.x as i32 - 6, shot_dark.frame.y as i32 + row + 3), muted, 1));

    // The grid: the first vertical line is 1/8 across, and the texel under it is not what shows there.
    let (gx, gy) = ((side / 8.0).round() as i32, (side * 0.3) as i32);
    let u = ((gx as f32 + 0.5) / side * n as f32) as usize;
    let v = ((gy as f32 + 0.5) / side * n as f32) as usize;
    assert_ne!(shot_dark.at(gx, gy), texel(&img, n, u, v), "a grid line lies over the map");
    // The cross: its white centre pixel.
    let c = (side / 2.0 - 7.5).round() as i32 + 7;
    assert_eq!(shot_dark.at(c, c + 4), [255, 255, 255], "the cross is white");
}

#[test]
fn m3_zoomed_in_every_texel_is_a_hard_block() {
    let Some(world) = load() else { return };
    let mut s = state(2);
    s.look = Look { k: 8.0, x: -2.0, y: -2.0 };
    let shot = take(&world, false, s, "map_relief_zoom8.png");
    let img = paint(&world, View::Height, true, &BTreeSet::new());
    let (n, side) = (world.samples, shot.side);
    let b = side * 8.0 / n as f32;
    let at_texel = |u: i32, v: i32| texel(&img, n, u as usize, v as usize);
    // The screen pixel (rx, ry) lies in texel floor(u) of the picture, u in texels.
    let place = |r: i32, origin: f32| (r as f32 + 0.5 - origin * side) / b;

    let mut interior = 0;
    for ry in 1..side as i32 - 1 {
        for rx in 1..side as i32 - 1 {
            let (u, v) = (place(rx, shot.look.x), place(ry, shot.look.y));
            let (fu, fv) = (u.fract() * b, v.fract() * b);
            // At least 1 px inside its block, whatever the side of the edge.
            if fu < 1.0 || b - fu < 1.0 || fv < 1.0 || b - fv < 1.0 {
                continue;
            }
            let want = at_texel(u.floor() as i32, v.floor() as i32);
            assert_eq!(shot.at(rx, ry), want, "pixel ({rx},{ry}) in texel ({},{})", u.floor(), v.floor());
            interior += 1;
        }
    }
    assert!(interior > 100_000, "{interior} interior pixels");

    // Sharp edges: the two pixels nearest a boundary between two different texels are those texels, no blend.
    let mut sharp = 0;
    for tv in 0..n as i32 {
        let y = shot.look.y * side + (tv as f32 + 0.5) * b;
        if y < 2.0 || y > side - 2.0 {
            continue;
        }
        for tu in 0..n as i32 - 1 {
            let edge = shot.look.x * side + (tu as f32 + 1.0) * b;
            if edge < 3.0 || edge > side - 3.0 || at_texel(tu, tv) == at_texel(tu + 1, tv) {
                continue;
            }
            let left = (edge - 0.5).ceil() as i32 - 1;
            assert_eq!(shot.at(left, y as i32), at_texel(tu, tv), "left of the edge after texel ({tu},{tv})");
            assert_eq!(shot.at(left + 1, y as i32), at_texel(tu + 1, tv), "right of the edge after texel ({tu},{tv})");
            sharp += 1;
        }
    }
    assert!(sharp >= 25, "only {sharp} sharp boundaries");
    println!("zoom 8x: {interior} interior pixels equal their texel, {sharp} sharp boundaries");
}

#[test]
fn m4_a_structure_mark_is_a_disc_of_its_colour() {
    let Some(world) = load() else { return };
    let s = world.structures.as_ref().unwrap();
    let mut st = state(4);
    st.chosen[4] = surface_kinds(&world);
    let shot = take(&world, false, st, "map_structures_marks.png");
    let side = shot.side as f64;
    let span = world.span();
    let screen = |x: f64, z: f64| ((x - world.origin_x) / span * side, (z - world.origin_z) / span * side);
    let drawn: Vec<(usize, f64, f64)> = s.order.iter().filter(|&&k| !s.under[k]).flat_map(|&k| s.by_kind[k].iter().map(move |&(x, z)| (k, x, z))).collect();
    assert!(!drawn.is_empty(), "there are surface structures");
    let near_line = |v: f64| (0..=8).any(|j| (v - (j as f64 * side / 8.0).round()).abs() <= 2.0);
    let c = side / 2.0;
    // The first surface point with nothing else drawn near it, off the grid and the cross.
    let first = drawn
        .iter()
        .find(|&&(_, x, z)| {
            let (sx, sy) = screen(x, z);
            let alone = drawn.iter().filter(|&&(_, ox, oz)| (ox, oz) != (x, z)).all(|&(_, ox, oz)| {
                let (a, b) = screen(ox, oz);
                (a - sx).hypot(b - sy) > 12.0
            });
            alone && !near_line(sx) && !near_line(sy) && (sx - c).abs() > 12.0 && sx > 8.0 && sy > 8.0 && sx < side - 8.0 && sy < side - 8.0
        })
        .expect("an isolated surface structure");
    let (sx, sy) = screen(first.1, first.2);
    let got = shot.at(sx as i32, sy as i32);
    let want = s.colours[first.0];
    assert!(near(got, want, 2), "the mark of {} at ({sx:.1},{sy:.1}) is {got:?}, want {want:?}", s.ids[first.0]);
    // The land under it is grey: a pixel well away from any mark is a stepped-back grey.
    let corner = shot.at(2, 2);
    assert!(corner[0] == corner[1] && corner[1] == corner[2] || (corner[0] as i32 - corner[2] as i32).abs() < 12, "land is grey: {corner:?}");
    println!("structure mark of {} is {got:?}", s.ids[first.0]);
}
