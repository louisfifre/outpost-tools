//! The right column through the application and the offscreen renderer, with the real preview: the figures with their
//! change, the list that follows the view and mirrors the choice, the filter, the structures' parts, the history and
//! the theme that follows the system. Everything is written under target\tmp. A test says "skipped" without the data.

mod common;

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use common::*;
use mere_base::{App, Cmd, Event, Vec2, Weight};
use mere_ui::Theme;
use mere_win32::Renderer;
use preview::figures::{CellKind, Figures, figure_cells, percent};
use preview::history::{self, Entry};
use preview::paint::View;
use preview::run::Runs;
use preview::world::World;
use preview::{PreviewApp, PreviewState, text};

/// The application at any size and in either theme, as `Rig` is at 1400 in the dark.
struct Mini {
    app: PreviewApp,
    r: Renderer,
    w: u32,
    h: u32,
}

impl Mini {
    fn new(theme: Theme, dark: bool, size: (u32, u32), world: &World, state: PreviewState, follow: bool) -> Mini {
        let (w, h) = size;
        let mut r = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
        let mut app = PreviewApp::new(theme, dark, Vec2::new(w as f32, h as f32), state, Some(world.clone())).follow_scheme(follow);
        app.start(&mut r);
        app.event(Event::Resized { width_px: w, height_px: h, scale: 1.0 });
        app.frame(&mut r, 0.0);
        Mini { app, r, w, h }
    }

    fn frame(&mut self) {
        self.app.frame(&mut self.r, 0.0);
    }

    fn rgba(&mut self) -> Vec<u8> {
        self.frame();
        self.r.read_rgba().unwrap()
    }

    fn shot(&mut self, name: &str) -> Vec<u8> {
        let rgba = self.rgba();
        let path = tmp().join(name);
        std::fs::write(&path, mere_png::encode(self.w, self.h, &rgba)).unwrap();
        println!("wrote {}", path.display());
        rgba
    }

    fn px(&self, rgba: &[u8], x: f32, y: f32) -> [u8; 3] {
        let i = (y as usize * self.w as usize + x as usize) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    }
}

fn state_for(world: &World, view: usize) -> PreviewState {
    PreviewState { view, ..PreviewState::for_world(Some(world)) }
}

/// The texts the last frame drew, in order.
fn drawn(rig: &Rig) -> Vec<(String, f32, Weight)> {
    rig.app.list().cmds.iter().filter_map(|c| if let Cmd::Text { text, size, weight, .. } = c { Some((text.clone(), *size, *weight)) } else { None }).collect()
}

/// The run before of G7 and the page's own check: it differs in every way a figure can.
fn before_of(f: &Figures) -> Figures {
    Figures {
        ocean: f.ocean + 0.34,
        flat: f.flat.map(|v| v - 1.06),
        top: f.top - 3.0,
        typical: f.typical + 49.0,
        expanse: f.expanse.map(|v| v + 160.0),
        count: f.count - 2,
        ..*f
    }
}

fn list_of(world: &World, view: View) -> &preview::figures::List {
    world.measured.lists[view.index()].as_ref().expect("the list")
}

/// The ids of a list that have something in them, in the order the page shows them (by size for biomes).
fn shown_ids(world: &World, view: View) -> Vec<String> {
    let l = list_of(world, view);
    let mut order: Vec<usize> = (0..l.ids.len()).filter(|&i| l.cells[i] > 0.0).collect();
    if matches!(view, View::Biomes | View::Structures) {
        order.sort_by(|&a, &b| l.cells[b].partial_cmp(&l.cells[a]).unwrap());
    }
    order.into_iter().map(|i| l.ids[i].clone()).collect()
}

#[test]
fn g1_the_relief_column_has_the_real_figures_and_bands() {
    let Some(world) = world() else { return };
    let mut rig = Rig::new(&world, state_for(&world, 5), None);
    rig.park();
    let rgba = rig.shot("figures_1400.png");
    let ids = rig.app.ids().clone();
    let f = world.measured.figures;
    let want: Vec<String> = figure_cells(&f, None).iter().map(|c| c.text.clone()).collect();
    assert_eq!(ids.figures_text, want);
    println!("G1 figures: {:?}", ids.figures_text.iter().filter(|t| !t.is_empty()).collect::<Vec<_>>());
    // The three of the goal are semibold at 14 px.
    let texts = drawn(&rig);
    for i in [1, 4, 7] {
        let t = &ids.figures_text[i];
        assert!(texts.iter().any(|(s, size, w)| s == t && *size == 14.0 && *w == Weight::SemiBold), "goal value {t:?} is not semibold 14");
    }
    assert_eq!(ids.goal_values.len(), 3);
    assert_eq!(ids.list_title, "HEIGHTS");
    // The rule before Sea.
    let sea = rig.rect(ids.goal_values[2]);
    let line = (sea.y as i32 + 20..sea.y as i32 + 60).any(|y| rig.px(&rgba, sea.x + 5.0, y as f32) == rgb(rig.theme.line));
    assert!(line, "no --line rule under the goal figures");

    let want_ids = shown_ids(&world, View::Height);
    assert_eq!(ids.list_keys, want_ids);
    let l = list_of(&world, View::Height);
    let shares: Vec<String> = want_ids.iter().map(|id| percent(100.0 * l.cells[id.parse::<usize>().unwrap()] / 65536.0)).collect();
    for (id, share) in want_ids.iter().zip(&shares) {
        assert!(texts.iter().any(|(s, ..)| s == share), "band {id}: no {share} drawn");
    }
    let sum: f64 = shares.iter().map(|s| s.trim_end_matches('%').parse::<f64>().unwrap()).sum();
    assert!((sum - 100.0).abs() <= 0.1 * shares.len() as f64, "the shares sum to {sum}");
    println!("G1: {} bands, shares {:?} (sum {sum:.1})", shares.len(), shares);
}

#[test]
fn g2_biomes_by_share_then_the_absent_ones_and_the_choice() {
    let Some(world) = world() else { return };
    let mut rig = Rig::new(&world, state_for(&world, 0), None);
    rig.park();
    let ids = rig.app.ids().clone();
    let l = list_of(&world, View::Biomes);
    assert_eq!(ids.list_rows.len(), 25);
    let cells: Vec<f64> = ids.list_keys.iter().map(|id| l.cells[l.ids.iter().position(|x| x == id).unwrap()]).collect();
    assert!(cells.windows(2).all(|p| p[0] >= p[1]), "ordered by share: {cells:?}");
    assert_eq!(ids.list_part_words, vec![(text::ABSENT.to_string(), text::num(world.absent.len() as f64))]);
    assert_eq!(ids.absent_rows.len(), world.absent.len());
    println!("G2: {} biomes, then {:?} with {} absent rows", ids.list_rows.len(), ids.list_part_words[0], ids.absent_rows.len());
    assert_eq!(ids.pick_text, text::PICK_HINT);

    // A click on the first row chooses it.
    let first = ids.list_keys[0].clone();
    let at = l.ids.iter().position(|x| *x == first).unwrap();
    rig.click_on(ids.list_rows[0]);
    rig.park();
    assert!(rig.app.state().chosen[0].contains(&first) && rig.app.state().chosen[0].len() == 1);
    assert_eq!(rig.app.painted_view(), Some(View::Biomes));
    assert_eq!(rig.app.painted_choice(), Some(&BTreeSet::from([first.clone()])), "the map repaints with the choice");
    let ids = rig.app.ids().clone();
    let want = format!("{} \u{b7} {} of the area{}", l.names[at], percent(100.0 * l.cells[at] / 65536.0), text::PICK_NONE);
    assert_eq!(ids.pick_text, want);
    let rgba = rig.rgba();
    let swatch = |rig: &Rig, row: usize| {
        let r = rig.rect(ids.list_rows[row]);
        rig.px(&rgba, r.x + 6.0, r.y + 12.0)
    };
    let colour_of = |key: &str| {
        let c = l.colours[l.ids.iter().position(|x| x == key).unwrap()];
        mere_base::Color::rgb8(c[0], c[1], c[2])
    };
    assert!(near(swatch(&rig, 0), rgb(colour_of(&ids.list_keys[0])), 1), "the chosen row is lit");
    assert!(near(swatch(&rig, 1), rgb(rig.theme.faded(colour_of(&ids.list_keys[1]), 0.5)), 2), "the others step back: {:?}", swatch(&rig, 1));

    // "Show all" clears the choice.
    rig.click_on(ids.pick_none);
    rig.park();
    assert!(rig.app.state().chosen[0].is_empty());
    assert_eq!(rig.app.painted_choice(), Some(&BTreeSet::new()));
    assert_eq!(rig.app.ids().pick_text, text::PICK_HINT);
}

#[test]
fn g3_the_light_column() {
    let Some(world) = world() else { return };
    let top: Vec<String> = shown_ids(&world, View::Biomes).into_iter().take(2).collect();
    let mut state = state_for(&world, 0);
    state.chosen[0].extend(top.iter().cloned());
    let mut m = Mini::new(Theme::light(), false, (1400, 940), &world, state, false);
    let rgba = m.shot("figures_light.png");
    let ids = m.app.ids().clone();
    assert_eq!(m.px(&rgba, 2.0, 2.0), [0xfa, 0xfa, 0xf8], "the ground");
    // The first row's name: its darkest pixel is the text colour, near #171717.
    let row = m.app.ui().rect(ids.list_rows[0]).unwrap();
    let darkest = (row.y as i32..(row.y + row.h) as i32)
        .flat_map(|y| (row.x as i32 + 20..row.x as i32 + 120).map(move |x| (x, y)))
        .map(|(x, y)| m.px(&rgba, x as f32, y as f32))
        .min_by_key(|p| p.iter().map(|v| *v as u32).sum::<u32>())
        .unwrap();
    assert!(darkest.iter().all(|&v| v <= 0x30), "the name is not drawn in the text colour: {darkest:?}");
    assert_eq!(ids.pick_names.len(), 2, "the two chosen rows are named");
}

#[test]
fn g4_the_filter_keeps_what_matches_and_what_is_chosen() {
    let Some(world) = world() else { return };
    let mut rig = Rig::new(&world, state_for(&world, 0), None);
    rig.park();
    let first = rig.app.ids().list_keys[0].clone();
    assert!(!first.contains("savanna"), "the first biome is not a savanna: {first}");
    rig.click_on(rig.app.ids().list_rows[0]);
    let filter = rig.app.ids().filter;
    rig.click_on(filter);
    for ch in "savanna".chars() {
        rig.app.event(Event::Char { ch });
    }
    rig.frame();
    rig.frame();
    rig.park();
    assert_eq!(rig.app.state().filter, "savanna");
    let ids = rig.app.ids().clone();
    assert!(ids.list_keys.len() > 1);
    assert!(ids.list_keys.iter().all(|k| k.replace('_', " ").contains("savanna") || *k == first), "{:?}", ids.list_keys);
    assert!(ids.list_keys.contains(&first), "the chosen row stays in sight");
    let absent_match = world.absent.iter().filter(|id| id.replace('_', " ").contains("savanna")).count();
    assert_eq!(ids.absent_rows.len(), absent_match);
    assert_eq!(ids.list_parts.len(), usize::from(absent_match > 0), "a part with no row shown has no head");
    println!("G4: filter 'savanna' shows {:?} and {} absent rows", ids.list_keys, absent_match);

    // A filter nothing matches leaves the chosen row only, and no head.
    rig.app.state_mut().filter = "zzzz".to_string();
    rig.frame();
    rig.park();
    let ids = rig.app.ids();
    assert_eq!(ids.list_keys, vec![first]);
    assert!(ids.list_parts.is_empty() && ids.absent_rows.is_empty());
}

#[test]
fn g5_structures_come_in_two_parts_with_round_marks() {
    let Some(world) = world() else { return };
    let mut rig = Rig::new(&world, state_for(&world, 7), None);
    rig.park();
    let ids = rig.app.ids().clone();
    let s = world.structures.as_ref().expect("structures");
    let sum = |under: bool| s.count.iter().zip(&s.under).filter(|(_, u)| **u == under).map(|(c, _)| *c as f64).sum::<f64>();
    assert_eq!(
        ids.list_part_words,
        vec![(text::PARTS[0].to_string(), text::num(sum(false))), (text::PARTS[1].to_string(), text::num(sum(true)))]
    );
    println!("G5: parts {:?}", ids.list_part_words);
    assert_eq!(ids.list_rows.len(), s.count.iter().filter(|&&c| c > 0).count());
    assert_eq!(ids.pick_text, text::PICK_HINT_MARKS);
    let discs = rig.app.list().cmds.iter().filter(|c| matches!(c, Cmd::Disc { radius, .. } if *radius == 5.0)).count();
    assert!(discs >= ids.list_rows.len().min(8), "{discs} round marks");

    let surface: BTreeSet<String> = (0..s.ids.len()).filter(|&i| !s.under[i] && s.count[i] > 0).map(|i| s.ids[i].clone()).collect();
    rig.click_on(ids.list_parts[0]);
    rig.park();
    assert_eq!(rig.app.state().chosen[View::Structures.index()], surface, "the surface head chooses exactly the surface ids");
    rig.shot("structures_list.png");
    assert!(rig.app.ids().pick_text.ends_with(text::PICK_NONE_MARKS), "{}", rig.app.ids().pick_text);
    let again = rig.app.ids().list_parts[0];
    rig.click_on(again);
    rig.park();
    assert!(rig.app.state().chosen[View::Structures.index()].is_empty(), "again clears");
    // Chosen once more for the picture.
    let again = rig.app.ids().list_parts[0];
    rig.click_on(again);
    rig.park();
    rig.shot("structures_list.png");
    assert_eq!(rig.app.state().chosen[View::Structures.index()], surface);
}

#[test]
fn g6_a_narrower_window_does_not_shrink_the_goal_figures() {
    let Some(world) = world() else { return };
    let mut m = Mini::new(Theme::dark(), true, (1000, 940), &world, state_for(&world, 6), false);
    m.shot("figures_1000.png");
    // No breakpoint since step 19: the page's wide numbers hold at 1000 px too.
    let cols = preview::PAGE;
    assert_eq!((cols.goal, cols.fig_gap), (14.0, 14.0));
    let sizes: Vec<f32> = m.app.list().cmds.iter().filter_map(|c| if let Cmd::Text { size, weight: Weight::SemiBold, text, .. } = c { Some((text, size)) } else { None }).filter(|(t, _)| m.app.ids().figures_text.contains(t)).map(|(_, s)| *s).collect();
    assert!(sizes.len() >= 3 && sizes.iter().all(|&s| s == 14.0), "{sizes:?}");
    assert_eq!(m.app.ids().list_title, "TEMPERATURES");
    assert_eq!(m.app.ids().list_rows.len(), shown_ids(&world, View::Temperature).len());
}

#[test]
fn g7_the_run_before_gives_the_change_column() {
    let Some(world) = world() else { return };
    let root = root("figures_before");
    let f = world.measured.figures;
    let b1 = before_of(&f);
    std::fs::write(root.join("before.json"), b1.to_json()).unwrap();
    let mut rig = Rig::new(&world, state_for(&world, 5), Some(runs(&root, &[])));
    rig.park();
    rig.shot("figures_change.png");
    let ids = rig.app.ids().clone();
    let want: Vec<String> = figure_cells(&f, Some(&b1)).iter().map(|c| c.text.clone()).collect();
    assert_eq!(ids.figures_text, want);
    assert_eq!(ids.figures_text[2], "change");
    assert!(ids.figures_text.iter().any(|t| t.ends_with(" pt")) && ids.figures_text.iter().any(|t| t == "="));
    let moved: Vec<String> = figure_cells(&f, Some(&b1)).into_iter().filter(|c| matches!(c.kind, CellKind::Before { .. })).map(|c| c.text).collect();
    println!("G7 changes, flat to count: {moved:?}");
}

/// The fixture history of H1: three runs of the real seed, area and detail, each with one setting moved.
fn fixtures(world: &World) -> Vec<Entry> {
    let made = PreviewState::for_world(Some(world)).made.expect("the world says what made it");
    let f = world.measured.figures;
    let make = |key: usize, delta: f64, figures: [Option<f64>; 5], at: f64| {
        let mut e = history::entry(&made, &f, at);
        e.values[key].1 += delta;
        e.figures = figures;
        e
    };
    let (flat, expanse, typical, ocean, high) = (f.flat.unwrap(), f.expanse.unwrap(), f.typical, f.ocean, f.high);
    vec![
        make(0, 0.01, [Some(flat - 3.0), Some(expanse - 120.0), Some(typical + 60.0), Some(ocean + 1.5), Some(high - 0.8)], 1_700_000_000_000.0),
        make(1, 0.02, [Some(flat - 1.0), Some(expanse + 40.0), Some(typical + 20.0), Some(ocean + 0.5), Some(high - 0.2)], 1_700_000_100_000.0),
        make(2, 0.05, [None, Some(expanse + 200.0), Some(typical - 90.0), Some(ocean - 0.7), Some(high + 0.4)], 1_700_000_200_000.0),
    ]
}

#[test]
fn h1_h2_h3_the_history_opens_follows_the_pointer_and_dedupes() {
    let Some(world) = world() else { return };
    let root = root("history_h1");
    let on_disk = fixtures(&world);
    let file = runs(&root, &[]).history();
    history::save(&file, &on_disk).unwrap();
    let mut rig = Rig::new(&world, state_for(&world, 5), Some(runs(&root, &[])));
    rig.park();
    let kept = history::load(&file);
    assert_eq!(kept.len(), 4, "the real map is added to the three fixtures");
    let made = rig.app.state().made.clone().unwrap();
    let f = world.measured.figures;
    assert_eq!(history::here(&kept, &made.seed, f.span, f.step).len(), 4);

    // H1: closed at first; opened, five lines and the hint.
    assert!(rig.app.ids().trends.is_empty());
    rig.click_on(rig.app.ids().history_head);
    rig.park();
    let ids = rig.app.ids().clone();
    assert_eq!(ids.trends.len(), 5, "flat, expanse, typical, sea and high land");
    assert_eq!(ids.history_probe, text::history_hint(4));
    for &t in &ids.trends {
        assert_eq!(rig.rect(t).h, 30.0);
    }

    // H2: the pointer a third of the way across picks the second run.
    let body = rig.rect(ids.history_body);
    let at = Vec2::new(body.x + body.w / 3.0, body.y + 10.0);
    rig.app.event(Event::PointerMoved { pos: at });
    rig.frame();
    rig.frame();
    let runs_here = history::here(&kept, &made.seed, f.span, f.step);
    let want = text::history_run(2, 4, &history::changes(Some(runs_here[0]), runs_here[1]));
    assert_eq!(rig.app.ids().history_probe, want);
    println!("H2: {want}");
    rig.shot("history_1400.png");
    // The pointer leaves: the hint is back.
    rig.park();
    assert_eq!(rig.app.ids().history_probe, text::history_hint(4));

    // H3: a second app on the same folder finds the real map last and adds nothing.
    let second = Rig::new(&world, state_for(&world, 5), Some(runs(&root, &[])));
    assert_eq!(second.app.state().history.len(), 4);
    assert_eq!(history::load(&file).len(), 4, "the file is unchanged");
}

#[test]
fn h4_a_finished_run_keeps_the_run_before_and_the_new_map() {
    let Some(world) = world() else { return };
    let root = root("history_h4");
    let mut state = state_for(&world, 5);
    state.seed = "h4-seed".to_string();
    // One setting moved before Generate, as a person would.
    state.values[0] += 0.01;
    let from = preview::world::data_dir();
    let env = [("MERE_STUB", "finish"), ("MERE_STUB_FROM", from.to_str().expect("a UTF-8 path"))];
    let mut rig = Rig::new(&world, state, Some(runs(&root, &env)));
    rig.park();
    assert_eq!(history::load(&root.join("history.json")).len(), 1, "the map on screen is kept at the start");
    rig.generate();
    rig.until(60, "the run to end", |r| !r.app.running());
    rig.park();
    rig.shot("history_h4.png");

    let f = world.measured.figures;
    let saved = std::fs::read_to_string(root.join("before.json")).expect("before.json was written");
    assert_eq!(Figures::from_json(&preview::json::Json::parse(&saved).unwrap()), Some(f), "the old world's figures");
    let kept = history::load(&root.join("history.json"));
    assert_eq!(kept.len(), 2, "the run is added");
    assert_eq!(kept[1].seed, "h4-seed");
    let ids = rig.app.ids().clone();
    let want: Vec<String> = figure_cells(&f, Some(&f)).iter().map(|c| c.text.clone()).collect();
    assert_eq!(ids.figures_text, want);
    assert_eq!(ids.figures_text[2], "change");
    let same = figure_cells(&f, Some(&f)).iter().filter(|c| matches!(c.kind, CellKind::Before { .. }) && c.text == "=").count();
    assert_eq!(same, 6, "every figure but the count reads =");
}

fn light_pictures_equal(a: &[u8], b: &[u8], what: &str) {
    let differing = a.iter().zip(b).filter(|(x, y)| x != y).count();
    assert!(a == b, "{what}: {differing} bytes differ");
}

#[test]
fn t1_the_theme_follows_the_system_live() {
    let Some(world) = world() else { return };
    let state = PreviewState { view: View::Height.index(), ..PreviewState::for_world(Some(&world)) };
    let mut a = Mini::new(Theme::dark(), true, (1400, 940), &world, state.clone(), true);
    let dark = a.rgba();
    assert!(a.app.event(Event::Scheme { dark: false }), "a followed change asks for a frame");
    let switched = a.shot("scheme_switched.png");
    let mut b = Mini::new(Theme::light(), false, (1400, 940), &world, state.clone(), false);
    let fresh = b.rgba();
    assert_ne!(switched, dark, "the picture changed");
    light_pictures_equal(&switched, &fresh, "the switched window and a fresh light one");
    assert_eq!(a.app.painted_view(), Some(View::Height));

    // And back.
    assert!(a.app.event(Event::Scheme { dark: true }));
    light_pictures_equal(&a.rgba(), &dark, "back to dark");

    // A fixed theme ignores the system.
    let mut c = Mini::new(Theme::dark(), true, (1400, 940), &world, state, false);
    let before = c.rgba();
    assert!(!c.app.event(Event::Scheme { dark: false }));
    light_pictures_equal(&c.rgba(), &before, "a fixed dark theme");
    light_pictures_equal(&before, &dark, "two dark windows");
    println!("T1: switched, switched back, and a fixed theme stayed");
}

#[test]
fn cli_theme_light_gives_the_light_ground_and_a_list() {
    if world().is_none() {
        return;
    }
    let out = tmp().join("cli_light.png");
    let _ = std::fs::remove_file(&out);
    let runs_root = tmp().join("cli-runs");
    let status = Command::new(env!("CARGO_BIN_EXE_preview"))
        .arg("--data")
        .arg(preview::world::data_dir())
        .args(["--theme", "light", "--view", "biomes", "--shot"])
        .arg(&out)
        .arg("--runs")
        .arg(&runs_root)
        .status()
        .expect("the preview runs");
    assert!(status.success());
    let png = mere_png::decode(&std::fs::read(&out).unwrap()).expect("a PNG");
    let px = |x: usize, y: usize| {
        let i = (y * png.width as usize + x) * 4;
        [png.rgba[i], png.rgba[i + 1], png.rgba[i + 2]]
    };
    assert_eq!(px(2, 2), [0xfa, 0xfa, 0xf8], "the light ground");
    // The right column ends 28 px from the edge of a 1400 px window and is as wide as the plan says: a swatch of the
    // list is 6 px in, in one of the rows below.
    let right_x = (1400.0 - 28.0 - preview::panels::plan(1400.0, [true, true], [None, None]).widths[1]) as usize;
    let swatch = (420..900).any(|y| px(right_x + 6, y) != [0xfa, 0xfa, 0xf8] && px(right_x + 6, y) != px(2, 2));
    assert!(swatch, "no list row in the right column");
    assert!(Path::new(&runs_root).join("history.json").exists(), "the picture's map was kept in the history, under target\\tmp");
    let _: Option<Runs> = None;
}
