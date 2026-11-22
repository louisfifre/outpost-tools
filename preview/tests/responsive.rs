//! The side panels' layout rule, checked over a sweep of windows. Beside the stage the panels are as wide as their
//! content wants (`panels::natural`, 319 and 321 px) and the middle column is what is left, its square
//! `min(middle - gaps, H - 40 - 10 - 22 - 64)`; the panels shrink only once the map is at its minimum; below the
//! width where the minimums no longer fit they stack. The pictures go to target/tmp; the ones worth keeping are
//! copied to docs/shots.

use std::path::Path;

use mere_base::{App, Event, Rect, Vec2};
use mere_ui::{Id, Theme, Ui};
use mere_win32::Renderer;
use preview::panels::{self, GAP, MAP_MIN, PANEL_MIN, STACK_AT};
use preview::world::World;
use preview::{BAR_H, CONTENT_TOP, MIN_SIZE, PreviewApp, PreviewState, ShellIds, build, picture_with};

const H: f32 = 940.0;
const TOP: f32 = BAR_H + CONTENT_TOP;
const COL_H: f32 = H - BAR_H - CONTENT_TOP - 22.0;
/// Wide enough that the panels are at their natural width: 4 gaps, the two panels and the map's minimum.
const SWEEP: [f32; 9] = [1000.0, 1100.0, 1200.0, 1250.0, 1251.0, 1400.0, 1480.0, 1481.0, 1920.0];

fn load() -> Option<World> {
    let dir = &preview::world::data_dir();
    if !dir.join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no out folder");
        return None;
    }
    Some(World::load(dir).expect("the data loads"))
}

fn rect(ui: &Ui, id: Id) -> Rect {
    assert_ne!(id, Id::ROOT, "the node was not built");
    ui.rect(id).expect("the node was built")
}

fn same(a: Rect, b: Rect, what: &str) {
    let d = [(a.x, b.x), (a.y, b.y), (a.w, b.w), (a.h, b.h)];
    assert!(d.iter().all(|(p, q)| (p - q).abs() < 0.01), "{what}: {a:?} != {b:?}");
}

fn laid_out(world: &World, view: usize, w: f32, h: f32) -> (Ui, ShellIds) {
    let state = PreviewState { view, ..PreviewState::for_world(Some(world)) };
    let (_, ui, ids) = picture_with(Theme::dark(), Vec2::new(w, h), Some(world), state);
    (ui, ids)
}

/// The natural widths, and what the middle column is at a window width when both panels are at them.
fn natural() -> [f32; 2] {
    panels::natural()
}

fn middle_at(w: f32) -> f32 {
    let [l, r] = natural();
    w - 4.0 * GAP - l - r
}

/// What sits inside the left and right columns, as the shell reports it: each is checked against its column.
fn left_parts(ids: &ShellIds) -> Vec<(String, Id)> {
    let mut v = vec![
        ("world".to_string(), ids.world_head),
        ("settings".into(), ids.settings_head),
        ("seed".into(), ids.seed),
        ("select".into(), ids.select),
        ("profiles".into(), ids.profiles),
        ("settings scroll".into(), ids.settings_scroll),
        ("reset".into(), ids.reset),
        ("run".into(), ids.run),
        ("details".into(), ids.details),
        ("generate".into(), ids.generate),
    ];
    v.extend(ids.group_heads.iter().enumerate().map(|(i, id)| (format!("group head {i}"), *id)));
    v.extend(ids.rows.iter().enumerate().map(|(i, id)| (format!("row {i}"), *id)));
    v
}

fn right_parts(ids: &ShellIds) -> Vec<(String, Id)> {
    let mut v = vec![
        ("figures".to_string(), ids.figures_head),
        ("history".into(), ids.history_head),
        ("figures grid".into(), ids.figures_grid),
        ("list block".into(), ids.list_block),
        ("pick".into(), ids.pick),
        ("filter".into(), ids.filter),
        ("list scroll".into(), ids.list_scroll),
    ];
    v.extend(ids.list_rows.iter().enumerate().map(|(i, id)| (format!("list row {i}"), *id)));
    // The filter is not built in every view.
    v.retain(|(_, id)| *id != Id::ROOT);
    v
}

/// R1: while the panels are at their natural width they are where the plan puts them, and what is inside each is
/// where it is at 1400, relative to its column. The frame is centred in the middle column and follows the one rule.
#[test]
fn r1_the_panels_keep_their_width_and_the_square_follows_one_rule() {
    let Some(world) = load() else { return };
    let [lw, rw] = natural();
    let (ui14, ids14) = laid_out(&world, 5, 1400.0, H);
    let (l14, r14) = (rect(&ui14, ids14.left), rect(&ui14, ids14.right));
    for w in SWEEP {
        assert!(w >= 4.0 * GAP + lw + rw + MAP_MIN, "{w} holds the panels at their natural width");
        let (ui, ids) = laid_out(&world, 5, w, H);
        let middle = middle_at(w);
        same(rect(&ui, ids.left), Rect::new(GAP, TOP, lw, COL_H), &format!("left at {w}"));
        same(rect(&ui, ids.right), Rect::new(w - GAP - rw, TOP, rw, COL_H), &format!("right at {w}"));
        same(rect(&ui, ids.middle), Rect::new(2.0 * GAP + lw, TOP, middle, COL_H), &format!("middle at {w}"));
        let side = middle.min(COL_H - 64.0);
        let f = rect(&ui, ids.frame);
        assert!((f.w - side).abs() < 0.01 && (f.h - side).abs() < 0.01, "frame at {w}: {f:?}, side {side}");
        assert!((f.x + f.w / 2.0 - (2.0 * GAP + lw + middle / 2.0)).abs() <= 0.5, "frame centred at {w}: {f:?}");
        // The left panel inside: identical to 1400's.
        let left = rect(&ui, ids.left);
        for ((name, id), (_, id14)) in left_parts(&ids).iter().zip(left_parts(&ids14)) {
            let (a, b) = (rect(&ui, *id), rect(&ui14, id14));
            same(Rect::new(a.x - left.x, a.y - left.y, a.w, a.h), Rect::new(b.x - l14.x, b.y - l14.y, b.w, b.h), &format!("{name} at {w}"));
        }
        // The right panel inside, relative to its own column.
        let right = rect(&ui, ids.right);
        let (rp, rp14) = (right_parts(&ids), right_parts(&ids14));
        assert_eq!(rp.len(), rp14.len(), "the same rows at {w}");
        for ((name, id), (_, id14)) in rp.iter().zip(rp14) {
            let (a, b) = (rect(&ui, *id), rect(&ui14, id14));
            same(Rect::new(a.x - right.x, a.y - right.y, a.w, a.h), Rect::new(b.x - r14.x, b.y - r14.y, b.w, b.h), &format!("{name} at {w}"));
        }
        println!("R1: {w}: side {side}, frame {f:?}");
    }
}

/// The frame's side from one build, which is all a size needs (the scrollbar's gutter does not move the frame).
fn side_at(world: Option<&World>, w: f32) -> f32 {
    let mut ui = Ui::new(Theme::dark());
    let mut s = PreviewState::default();
    let size = Vec2::new(w, H);
    ui.begin_frame(size);
    let ids = build(&mut ui, &mut s, size, world, None, None);
    ui.end_frame();
    rect(&ui, ids.frame).w
}

/// R2: from the width where the panels stack to 1920, one pixel of window moves the square by at most one pixel, it
/// never shrinks as the window grows, and it is the rule everywhere: the map's column less nothing, at most what the
/// height gives. Below the width where the panels are at their natural width they shrink to hold the map at its
/// minimum, so the square stays there until the panels are at theirs.
#[test]
fn r2_the_centre_is_one_continuous_function_of_the_window() {
    let [lw, rw] = natural();
    let mut last = side_at(None, STACK_AT);
    for w in (STACK_AT as u32 + 1)..=1920 {
        let w = w as f32;
        let side = side_at(None, w);
        assert!(side >= last, "the square shrank from {last} to {side} at {w}");
        assert!(side - last <= 1.0 + 1e-4, "the square jumped from {last} to {side} at {w}");
        // The panels at the width they have: natural, or shrunk to hold the map's minimum.
        let panels_w = (w - 4.0 * GAP - MAP_MIN).min(lw + rw);
        let middle = w - 4.0 * GAP - panels_w;
        assert!((side - middle.min(COL_H - 64.0)).abs() < 0.01, "side {side} at {w}: the column is {middle}");
        last = side;
    }
}

/// R3: a short window. The panels keep their x and width, the columns are as tall as the window less the bar and the
/// padding, and nothing hangs out of its column.
#[test]
fn r3_short_windows_keep_the_panels_and_fit() {
    let Some(world) = load() else { return };
    let [lw, rw] = natural();
    for h in [600.0, 700.0, 940.0] {
        let (ui, ids) = laid_out(&world, 5, 1400.0, h);
        let colh = h - BAR_H - CONTENT_TOP - 22.0;
        same(rect(&ui, ids.left), Rect::new(GAP, TOP, lw, colh), &format!("left at {h}"));
        same(rect(&ui, ids.right), Rect::new(1400.0 - GAP - rw, TOP, rw, colh), &format!("right at {h}"));
        same(rect(&ui, ids.middle), Rect::new(2.0 * GAP + lw, TOP, middle_at(1400.0), colh), &format!("middle at {h}"));
        let inside = |col: Id, parts: &[(&str, Id)]| {
            let c = rect(&ui, col);
            for (name, id) in parts {
                let r = rect(&ui, *id);
                assert!(r.y >= c.y - 0.5 && r.y + r.h <= c.y + c.h + 0.5, "{name} at {h} hangs out of its column: {r:?} in {c:?}");
                assert!(r.x >= c.x - 0.5 && r.x + r.w <= c.x + c.w + 0.5 + 10.0, "{name} at {h} is outside its column: {r:?} in {c:?}");
            }
        };
        inside(ids.left, &[("world", ids.world_head), ("settings", ids.settings_head), ("settings scroll", ids.settings_scroll), ("run", ids.run)]);
        inside(ids.right, &[("figures", ids.figures_head), ("history", ids.history_head), ("list block", ids.list_block)]);
        inside(ids.middle, &[("frame", ids.frame), ("under", ids.under)]);
        assert!(rect(&ui, ids.settings_scroll).h >= 0.0 && rect(&ui, ids.list_scroll).h >= 0.0);
        let (left, run) = (rect(&ui, ids.left), rect(&ui, ids.run));
        assert!((run.y + run.h - (left.y + left.h)).abs() < 0.01, "the run block rests on the bottom of the left column at {h}");
        let side = middle_at(1400.0).min(h - BAR_H - CONTENT_TOP - 22.0 - 64.0);
        assert!((rect(&ui, ids.frame).w - side).abs() < 0.01, "frame at {h}");
        println!("R3: {h}: settings {:?}, list {:?}, frame {side}", rect(&ui, ids.settings_scroll), rect(&ui, ids.list_scroll));
    }
}

/// R4: a size below the minimum is raised, in the app and in the command line's rule; the minimum is derived.
#[test]
fn r4_sizes_below_the_minimum_are_raised() {
    let least = MIN_SIZE.0;
    assert_eq!(MIN_SIZE, (panels::MIN_WIDTH, 600.0));
    assert!(least >= preview::overflow::NARROW_BAR && least >= panels::STACKED_MIN, "{least}");
    assert!(least - 1.0 < preview::overflow::NARROW_BAR.max(panels::STACKED_MIN), "it is the larger of the two, rounded up");
    assert_eq!(preview::min_size_px((400, 940)), (least as u32, 940));
    assert_eq!(preview::min_size_px((1400, 400)), (1400, 600));
    assert_eq!(preview::min_size_px((1920, 1080)), (1920, 1080));
    // The application lays out 400 x 940 as the least width by 940: stacked, the panels in two columns.
    let mut r = Renderer::offscreen(400, 940, 1.0).expect("offscreen renderer");
    let mut app = PreviewApp::new(Theme::dark(), true, Vec2::new(400.0, 940.0), PreviewState::default(), None);
    app.start(&mut r);
    app.event(Event::Resized { width_px: 400, height_px: 940, scale: 1.0 });
    app.frame(&mut r, 0.0);
    let ui = app.ui();
    let gap = panels::GAP_STACKED;
    same(rect(ui, app.ids().middle), Rect::new(gap, TOP, least - 2.0 * gap, ((940.0 - BAR_H) / 2.0).floor() - CONTENT_TOP), "middle of 400 x 940");
    let col = ((least - 3.0 * gap) / 2.0).floor();
    assert!((rect(ui, app.ids().left).w - col).abs() < 0.01 && (rect(ui, app.ids().right).w - col).abs() < 0.01, "two equal columns of {col}");
}

/// R5: the section at 1000 and 1920: the frame is the middle column's whole width and its height less `--under`,
/// and the columns around it are those of the relief view.
#[test]
fn r5_the_section_takes_the_middle_column_and_leaves_the_panels_alone() {
    let Some(world) = load() else { return };
    let [lw, _] = natural();
    for w in [1000.0, 1920.0] {
        let (ui, ids) = laid_out(&world, 8, w, H);
        let (relief, rids) = laid_out(&world, 5, w, H);
        let f = rect(&ui, ids.frame);
        same(f, Rect::new(2.0 * GAP + lw, TOP, middle_at(w), COL_H - 64.0), &format!("section frame at {w}"));
        for (name, a, b) in [("left", ids.left, rids.left), ("middle", ids.middle, rids.middle), ("right", ids.right, rids.right)] {
            same(rect(&ui, a), rect(&relief, b), &format!("{name} column at {w}"));
        }
        for ((name, id), (_, id2)) in left_parts(&ids).iter().zip(left_parts(&rids)) {
            same(rect(&ui, *id), rect(&relief, id2), &format!("{name} in the section at {w}"));
        }
    }
}

/// The picture of the app at a size, with a state of the caller's.
fn picture(world: &World, state: PreviewState, w: u32, h: u32, name: &str) {
    let theme = Theme::dark();
    let mut r = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
    let mut app = PreviewApp::new(theme, true, Vec2::new(w as f32, h as f32), state, Some(world.clone()));
    app.start(&mut r);
    app.event(Event::Resized { width_px: w, height_px: h, scale: 1.0 });
    app.frame(&mut r, 0.0);
    app.frame(&mut r, 0.0);
    let rgba = r.read_rgba().unwrap();
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, mere_png::encode(w, h, &rgba)).unwrap();
    println!("wrote {}", path.display());
}

/// The pictures: both panels open at 1920 x 1040, 1280 x 880, 1000 x 700, the stacking width and ten pixels under
/// it, 700 x 700 and the least window; the settings panel dragged wider; the figures panel closed. (The section's\n/// picture needs a world with one: `--data <an out folder with a section> --view section` makes it.)
#[test]
fn pictures_over_the_sweep() {
    let Some(world) = load() else { return };
    let state = |view: usize| PreviewState { view, ..PreviewState::for_world(Some(&world)) };
    let at = STACK_AT as u32;
    let least = MIN_SIZE.0 as u32;
    for (w, h) in [(1920, 1040), (1280, 880), (1000, 700), (at, 700), (at - 10, 700), (700, 700), (least, 600)] {
        picture(&world, state(5), w, h, &format!("fluid_{w}x{h}.png"));
    }
    picture(&world, PreviewState { panel_w: [Some(420.0), None], ..state(5) }, 1400, 800, "fluid_settings_dragged.png");
    picture(&world, PreviewState { panel_open: [true, false], ..state(5) }, 1280, 880, "fluid_figures_closed.png");
    picture(&world, PreviewState { panel_open: [false, true], ..state(5) }, 1280, 880, "fluid_settings_closed.png");
}

#[test]
fn the_derived_numbers_are_the_arithmetic_of_the_values_set_by_hand() {
    assert_eq!(STACK_AT, 4.0 * 28.0 + 220.0 + 220.0 + 238.0);
    assert_eq!(panels::STACKED_MIN, 2.0 * 220.0 + 3.0 * 16.0);
    assert_eq!(PANEL_MIN, 220.0);
}
