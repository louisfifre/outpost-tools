//! Runs against the stub engine (tests/common), which follows the real engine's protocol without the game: the
//! button's running look, the progress read once a second, the stop that ends the server, the finished run's world,
//! a failed or refused run, a run asked for while another is under way, and quitting. The real engine is never run
//! here (it downloads, needs a pack, the EULA and Java). Everything is written under target\tmp.
//! A test says "skipped" without the out folder PREVIEW_DATA_DIR names, but r10, which makes a small one of its own.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::*;
use preview::paint::{View, paint};
use preview::run::{Drawn, Pace, Steps, duration};
use preview::world::World;
use preview::{PreviewState, text};

fn pid_of(root: &Path) -> Option<u32> {
    std::fs::read_to_string(root.join("out/stub-child.pid")).ok()?.trim().parse().ok()
}

fn start(world: &preview::world::World, root: &Path, env: &[(&str, &str)], seed: &str) -> Rig {
    let mut state = PreviewState::for_world(Some(world));
    state.seed = seed.to_string();
    let mut rig = Rig::new(world, state, Some(runs(root, env)));
    rig.park();
    rig
}

#[test]
fn r1_a_run_under_way_and_its_stop() {
    let Some(world) = world() else { return };
    let root = root("runs-r1");
    // What this machine measured: 10 s besides the work, and 40 s of work for 256 points at 16 blocks: 50 s in all.
    std::fs::write(root.join("pace.json"), r#"{"fixed":10,"work":{"16":{"span":256,"seconds":40}}}"#).unwrap();
    let mut rig = start(&world, &root, &[("MERE_STUB", "hold")], "r1-seed");
    assert_eq!(rig.app.state().pace.about(256, 16), Some(50), "the detail tab's wait is read from the pace file");
    let ids = rig.app.ids().clone();
    rig.click_on(ids.generate);
    assert!(rig.app.running(), "the click started the generator");

    rig.until(30, "the map pass at 40 % and the server's pid", |r| {
        let s = r.app.state();
        s.follow.as_ref().is_some_and(|f| f.part == Some(40) && f.stage == "Map, every 16 blocks") && pid_of(&root).is_some()
    });
    let grandchild = pid_of(&root).unwrap();

    // Export while busy does nothing.
    let export = rig.app.ids().export;
    rig.click_on(export);
    assert!(!root.join("exports").exists(), "no export while a run is under way");

    // The page's `already`: the run is 20 s old, so the wait is 40 % of the 50 s expected.
    rig.app.set_run_age(20);
    rig.park();
    let rgba = rig.shot("run_40.png");
    let ids = rig.app.ids().clone();
    let t = rig.theme;
    assert_eq!(ids.status_text, "20 s of about 50 s.");
    assert_eq!(ids.generate_text, "Generating\u{2026}");

    let g = rig.rect(ids.generate);
    // The fill is the padding box's 40 %; read above the word, whose glyphs would be in the way at mid height.
    let (inner, y) = (g.w - 2.0, g.y + 4.0);
    assert_eq!(rig.px(&rgba, g.x + 1.0 + 0.2 * inner, y), rgb(t.line), "filled at 20 %");
    assert_eq!(rig.px(&rgba, g.x + 1.0 + 0.6 * inner, y), rgb(t.ground), "not filled at 60 %");
    assert_eq!(rig.px(&rgba, g.x + g.w / 2.0, g.y), rgb(t.text), "framed in the text colour");

    let s = rig.rect(ids.stop);
    assert_eq!((s.w, s.h), (34.0, 34.0));
    assert!((s.x - (g.x + g.w + 6.0)).abs() < 0.01, "6 px right of the button");
    assert_eq!(rig.px(&rgba, s.x + 17.0, s.y), rgb(t.edge), "the stop's frame");
    assert_eq!(rig.px(&rgba, s.x + 17.0, s.y + 17.0), rgb(t.muted), "the stop's small square");

    let bar = rig.rect(ids.steps_bar);
    assert_eq!(rig.px(&rgba, bar.x + 0.2 * bar.w, bar.y), rgb(t.text), "the hairline is filled at 20 %");
    assert_eq!(rig.px(&rgba, bar.x + 0.6 * bar.w, bar.y), rgb(t.line), "and not at 60 %");

    let follow = rig.app.state().follow.clone().unwrap();
    let took = |i: usize| duration(follow.done[i].1);
    assert_eq!(
        ids.steps_items,
        vec![
            format!("\u{2713} Starting the game {}", took(0)),
            format!("\u{2713} Sketch, every 64 blocks {}", took(1)),
            format!("\u{2713} Sketch, every 32 blocks {}", took(2)),
            "\u{203a} Map, every 16 blocks \u{b7} 40 %".to_string(),
        ]
    );

    // The map is veiled: one point away from the grid lines is the texel at 0.3 over the ground.
    let map = rig.rect(ids.map);
    let img = paint(&world, View::Height, true, &BTreeSet::new());
    let n = world.samples as i32;
    let (rx, ry) = ((0.1 * map.w) as i32, (0.1 * map.h) as i32);
    let got = rig.px(&rgba, map.x + rx as f32, map.y + ry as f32);
    let (u, v) = (((rx as f32 + 0.5) / map.w * n as f32) as i32, ((ry as f32 + 0.5) / map.h * n as f32) as i32);
    let veiled = (-1..=1).flat_map(|dv| (-1..=1).map(move |du| (du, dv))).any(|(du, dv)| {
        let i = (((v + dv) * n + u + du) as usize) * 4;
        let ground = rgb(t.ground);
        let want = [0, 1, 2].map(|c| (img[i + c] as f32 * 0.3 + ground[c] as f32 * 0.7).round() as u8);
        near(got, want, 2)
    });
    assert!(veiled, "the map's pixel {got:?} is not a texel at opacity 0.3 over the ground");

    // Stop: both buttons grey while the process tree is ended.
    let stop = ids.stop;
    rig.click_on(stop);
    assert!(rig.app.state().follow.as_ref().is_some_and(|f| f.stopping));
    let rgba = rig.rgba();
    let ids = rig.app.ids().clone();
    let (g, s) = (rig.rect(ids.generate), rig.rect(ids.stop));
    assert_eq!(rig.px(&rgba, g.x + g.w / 2.0, g.y), rgb(t.edge), "the running generate is disabled: its frame goes to --edge");
    assert_eq!(rig.px(&rgba, s.x + 17.0, s.y), rgb(t.faded(t.edge, mere_ui::tokens::FADE_DISABLED)), "the stop's frame is faded");

    rig.until(15, "the run to end", |r| !r.app.running());
    rig.park();
    assert_eq!(rig.app.state().said, text::STOPPED);
    assert_eq!(rig.app.ids().status_text, text::STOPPED);
    assert!(gone_within(grandchild, 10), "the server the generator started was ended with it");
    assert_eq!(rig.app.world().unwrap().made.as_ref().unwrap().seed, "cap-1", "the world is still the one from before");
    assert_eq!(rig.app.state().seed, "r1-seed", "the form keeps what was typed");
    let rgba = rig.rgba();
    let ids = rig.app.ids().clone();
    let g = rig.rect(ids.generate);
    assert_eq!(rig.px(&rgba, g.x + g.w / 2.0, g.y), rgb(t.text), "Generate is enabled again: the map is not this form's");
    assert_eq!(ids.stop, mere_ui::Id::ROOT, "the stop square is gone");
    assert!(!ids.steps_items.is_empty() && ids.steps_items.iter().all(|i| i.starts_with('\u{2713}')), "{:?}", ids.steps_items);
    assert!(matches!(rig.app.state().steps, Steps::Run { ref now, .. } if now.is_empty()));
}

#[test]
fn r2_a_finished_run_becomes_the_map() {
    let Some(world) = world() else { return };
    let root = root("runs-r2");
    let from = preview::world::data_dir();
    let mut rig = start(&world, &root, &[("MERE_STUB", "finish"), ("MERE_STUB_FROM", from.to_str().expect("a UTF-8 path"))], "r2-seed");
    let g = rig.app.ids().generate;
    rig.click_on(g);
    assert!(rig.app.running());
    rig.until(60, "the run to end", |r| !r.app.running());
    rig.park();
    let t = rig.theme;

    let said = rig.app.state().said.clone();
    assert!(said.starts_with("Preview ready in "), "{said}");
    assert_eq!(rig.app.state().made.as_ref().unwrap().seed, "r2-seed");
    assert_eq!(rig.app.world().unwrap().made.as_ref().unwrap().seed, "r2-seed", "the world was read again from the run's folder");
    assert!(rig.app.state().trouble.is_empty());
    let ids = rig.app.ids().clone();
    assert_eq!(ids.status_text, said);
    let rgba = rig.rgba();
    let g = rig.rect(ids.generate);
    assert_eq!(rig.px(&rgba, g.x + g.w / 2.0, g.y), rgb(t.edge), "Generate is disabled: the map shows these settings");

    // The pace learned from it: 1.5 s of work, the rest of the run is the game's.
    let pace = Pace::load(&root.join("pace.json"));
    assert_eq!(pace.work.get(&16), Some(&(256, 1.5)));
    let fixed = pace.fixed.expect("a fixed part was learned");
    assert!(fixed >= 0.0 && ((fixed + 1.5).fract()).abs() < 1e-9, "fixed {fixed} is the whole seconds of the run less 1.5");

    // The hairline is whole, and the last step done is the map (or the extras, when the look caught that pass).
    let bar = rig.rect(ids.steps_bar);
    assert_eq!(rig.px(&rgba, bar.x + 0.99 * bar.w, bar.y), rgb(t.text));
    let last = ids.steps_items.last().unwrap();
    assert!(last.starts_with("\u{2713} Map, every 16 blocks ") || last.starts_with("\u{2713} Cuts, ores and structures "), "{last}");
    assert!(ids.steps_items.iter().all(|i| !i.starts_with('\u{203a}')));

    rig.frame();
    assert!(rig.app.painted_view().is_some(), "the next frame draws the new map");
}

#[test]
fn r3_a_failed_run_says_so_in_the_warning_colour() {
    let Some(world) = world() else { return };
    let root = root("runs-r3");
    let mut rig = start(&world, &root, &[("MERE_STUB", "fail")], "r3-seed");
    let g = rig.app.ids().generate;
    rig.click_on(g);
    rig.until(30, "the run to end", |r| !r.app.running());
    rig.park();
    assert_eq!(rig.app.state().trouble, text::FAILED);
    let ids = rig.app.ids().clone();
    assert_eq!(ids.status_text, text::FAILED);
    let rgba = rig.rgba();
    let status = rig.rect(ids.status);
    let warn = rgb(rig.theme.warn);
    let mut found = false;
    for y in status.y as i32..(status.y + status.h) as i32 {
        for x in status.x as i32..(status.x + status.w) as i32 {
            found |= near(rig.px(&rgba, x as f32, y as f32), warn, 8);
        }
    }
    assert!(found, "no pixel of the status line is in --warn");
    assert_eq!(rig.app.world().unwrap().made.as_ref().unwrap().seed, "cap-1", "the map from before is kept");
    assert!(!rig.app.running());
}

#[test]
fn r4_a_refused_seed_starts_nothing() {
    let Some(world) = world() else { return };
    let root = root("runs-r4");
    let mut rig = start(&world, &root, &[("MERE_STUB", "hold")], "a/b");
    let g = rig.app.ids().generate;
    rig.click_on(g);
    rig.park();
    assert_eq!(rig.app.state().trouble, text::REFUSED);
    assert_eq!(rig.app.ids().status_text, text::REFUSED);
    assert!(!rig.app.running() && rig.app.run_id().is_none());
    assert!(!root.join("out/progress.json").exists(), "no process ran");
    assert!(!root.join("run.log").exists());
}

#[test]
fn r5_a_moved_setting_queues_the_next_run() {
    let Some(world) = world() else { return };
    let root = root("runs-r5");
    let mut rig = start(&world, &root, &[("MERE_STUB", "hold")], "r5-seed");
    let g = rig.app.ids().generate;
    rig.click_on(g);
    assert!(rig.app.running());
    let (first_run, first_ask) = (rig.app.run_id().unwrap(), rig.app.state().follow.as_ref().unwrap().ask.clone());

    // A setting moved: the button offers the new run in the place of the one under way.
    rig.app.state_mut().values[0] += 0.01;
    rig.park();
    assert_eq!(rig.app.ids().generate_text, text::GENERATE);
    let g = rig.app.ids().generate;
    rig.click_on(g);
    rig.until(30, "the next run to start by itself", |r| {
        r.app.running() && r.app.run_id() != Some(first_run) && r.app.state().follow.as_ref().is_some_and(|f| f.ask.values[0] != first_ask.values[0])
    });
    assert!(rig.app.state().said.is_empty());

    let stop = rig.app.ids().stop;
    rig.click_on(stop);
    rig.until(15, "the second run to end", |r| !r.app.running());
}

#[test]
fn r6_quitting_stops_the_run_under_way() {
    let Some(world) = world() else { return };
    let root = root("runs-r6");
    let mut rig = start(&world, &root, &[("MERE_STUB", "hold")], "r6-seed");
    let g = rig.app.ids().generate;
    rig.click_on(g);
    rig.until(30, "the server's pid", |_| pid_of(&root).is_some());
    let grandchild = pid_of(&root).unwrap();
    assert!(alive(grandchild));
    drop(rig.app);
    assert!(gone_within(grandchild, 10), "the server outlived the application");
}

/// Whether the map's pixel a tenth of the way in is a texel of `img` (a world of `n` points a side) at full opacity:
/// the picture is that world's, and nothing veils it. One point away from the grid lines, as in r1.
fn shows(rig: &Rig, rgba: &[u8], img: &[u8], n: usize) -> bool {
    let map = rig.rect(rig.app.ids().map);
    let n = n as i32;
    let (rx, ry) = ((0.1 * map.w) as i32, (0.1 * map.h) as i32);
    let got = rig.px(rgba, map.x + rx as f32, map.y + ry as f32);
    let (u, v) = (((rx as f32 + 0.5) / map.w * n as f32) as i32, ((ry as f32 + 0.5) / map.h * n as f32) as i32);
    (-1..=1).flat_map(|dv| (-1..=1).map(move |du| (du, dv))).any(|(du, dv)| {
        let i = (((v + dv) * n + u + du) as usize) * 4;
        near(got, [img[i], img[i + 1], img[i + 2]], 2)
    })
}

/// Puts the stage on a view (5 the height, 7 the structures, 8 the section) and says what its note says.
fn note_on(rig: &mut Rig, view: usize) -> String {
    rig.app.state_mut().view = view;
    rig.park();
    rig.app.ids().note_text.clone()
}

/// Clicks the bar's tab of a view and says which view is on after it.
fn click_view(rig: &mut Rig, view: usize) -> usize {
    let tab = rig.app.ids().tabs[view];
    rig.click_on(tab);
    rig.park();
    rig.app.state().view
}

#[test]
fn r8_the_stage_shows_the_sketch_then_the_map_then_everything() {
    let Some(world) = world() else { return };
    let from = preview::world::data_dir();
    if !from.join("coarse.json").exists() {
        println!("skipped: the out folder PREVIEW_DATA_DIR names holds no sketch");
        return;
    }
    let root = root("runs-r8");
    let out = root.join("out");
    let mut rig = start(&world, &root, &[("MERE_STUB", "stages"), ("MERE_STUB_FROM", from.to_str().expect("a UTF-8 path"))], "r8-seed");
    let figures_before = rig.app.ids().figures_text.clone();
    let g = rig.app.ids().generate;
    rig.click_on(g);
    assert!(rig.app.running());
    let drawn = |r: &Rig| r.app.state().follow.as_ref().map(|f| f.drawn);

    // The run has just started: its sketch is on the stage, unveiled, and the status says it can be read.
    rig.until(30, "the run's sketch on the stage", |r| matches!(drawn(r), Some(Drawn::Sketch(_))));
    let sketch = World::load_sketch(&out).unwrap();
    rig.park();
    let rgba = rig.shot("run_started.png");
    assert!(shows(&rig, &rgba, &paint(&sketch, View::Height, true, &BTreeSet::new()), sketch.samples), "the stage shows the sketch, not veiled");
    assert!(rig.app.ids().status_text.ends_with(text::SKETCH_READABLE), "{}", rig.app.ids().status_text);
    assert_eq!(rig.app.ids().figures_text, figures_before, "the figures stay those of the map before");
    assert_eq!(note_on(&mut rig, 5), text::STRUCTURES_PENDING);
    assert_eq!(note_on(&mut rig, 6), text::SECTION_PENDING);
    assert_eq!(note_on(&mut rig, 0), "");
    assert_eq!(click_view(&mut rig, 5), 0, "Structures is greyed while the run has none");
    assert_eq!(click_view(&mut rig, 6), 0, "Section is greyed while the run has none");

    // The map is there while the cuts, the ores and the structures are still being made (its partial map, between,
    // is r10's).
    std::fs::write(out.join("go-map"), "").unwrap();
    std::fs::write(out.join("go-full"), "").unwrap();
    rig.until(30, "the run's map on the stage", |r| drawn(r) == Some(Drawn::Map));
    let map = World::load(&out).unwrap();
    rig.park();
    let rgba = rig.shot("run_map_ready.png");
    assert!(shows(&rig, &rgba, &paint(&map, View::Height, true, &BTreeSet::new()), map.samples), "the stage shows the run's map, not veiled");
    assert!(rig.app.running(), "the run goes on");
    assert!(!rig.app.ids().status_text.contains(text::SKETCH_READABLE), "a map is no sketch");
    assert_eq!(rig.app.ids().steps_items.last().map(String::as_str), Some(format!("{} {}", text::NOW_MARK, text::STEP_EXTRAS).as_str()));
    assert_eq!(note_on(&mut rig, 6), text::SECTION_PENDING);
    assert_eq!(note_on(&mut rig, 5), text::STRUCTURES_PENDING);
    rig.shot("run_map_ready_structures.png");
    assert_eq!(note_on(&mut rig, 0), "");
    assert_eq!(click_view(&mut rig, 5), 0, "Structures is still greyed beside the run's map");
    assert_eq!(click_view(&mut rig, 6), 0);

    // The run's end: everything is there, the structures and the cuts included.
    std::fs::write(out.join("go-end"), "").unwrap();
    rig.until(30, "the run to end", |r| !r.app.running());
    rig.park();
    let rgba = rig.shot("run_finished.png");
    assert!(shows(&rig, &rgba, &paint(&map, View::Height, true, &BTreeSet::new()), map.samples));
    assert_eq!(rig.app.world().unwrap().made.as_ref().unwrap().seed, "r8-seed");
    assert_eq!(click_view(&mut rig, 5), 5, "Structures can be chosen again");
    assert_eq!(click_view(&mut rig, 0), 0);
    assert_eq!(note_on(&mut rig, 5), "");
    rig.shot("run_finished_structures.png");
    assert_eq!(rig.app.painted_view(), Some(View::Structures), "the structures are drawn");
    assert_eq!(note_on(&mut rig, 6), "");
    rig.frame();
    assert!(rig.app.shown().is_some(), "the section shows a cut");
}

#[test]
fn r9_a_stopped_run_gives_the_stage_back_to_the_map_before() {
    let Some(world) = world() else { return };
    let from = preview::world::data_dir();
    if !from.join("coarse.json").exists() {
        println!("skipped: the out folder PREVIEW_DATA_DIR names holds no sketch");
        return;
    }
    let root = root("runs-r9");
    let mut rig = start(&world, &root, &[("MERE_STUB", "stages"), ("MERE_STUB_FROM", from.to_str().expect("a UTF-8 path"))], "r9-seed");
    let g = rig.app.ids().generate;
    rig.click_on(g);
    rig.until(30, "the run's sketch on the stage", |r| r.app.state().follow.as_ref().is_some_and(|f| matches!(f.drawn, Drawn::Sketch(_))));
    let stop = rig.app.ids().stop;
    rig.click_on(stop);
    rig.until(15, "the run to end", |r| !r.app.running());
    rig.park();
    let rgba = rig.shot("run_stopped.png");
    assert!(shows(&rig, &rgba, &paint(&world, View::Height, true, &BTreeSet::new()), world.samples), "the stage shows the map before again");
    assert_eq!(rig.app.ids().status_text, text::STOPPED);
}

/// A small finished out folder of its own, so that r10 needs no PREVIEW_DATA_DIR: a map of 16 x 16 points every 16
/// blocks, all desert at 100 (its list holds the plains too), and the sketch of 8 x 8 every 32 before it, all
/// plains at 70.
fn small_out(dir: &Path) {
    let grid = |samples: usize, step: u32, biomes: &str| {
        format!(r#"{{"seed":"r10-seed","samples":{samples},"step":{step},"originX":-128,"originZ":-128,"seaLevel":63,"biomes":[{biomes}]}}"#)
    };
    let bins = |prefix: &str, samples: usize, height: i16, biome: u16| {
        std::fs::write(dir.join(format!("{prefix}heights.bin")), height.to_le_bytes().repeat(samples * samples)).unwrap();
        std::fs::write(dir.join(format!("{prefix}biomes.bin")), biome.to_le_bytes().repeat(samples * samples)).unwrap();
    };
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("preview.json"), grid(16, 16, r#"{"id":"minecraft:desert","temperature":2.0},{"id":"minecraft:plains","temperature":0.8}"#)).unwrap();
    bins("", 16, 100, 0);
    std::fs::write(dir.join("coarse.json"), grid(8, 32, r#"{"id":"minecraft:plains","temperature":0.8}"#)).unwrap();
    bins("coarse-32-", 8, 70, 0);
}

/// Whether the map's pixel at `fx`, `fy` of its square is a texel of `img` (a world of `n` points a side) at full
/// opacity, as `shows` checks a tenth of the way in.
fn shows_at(rig: &Rig, rgba: &[u8], img: &[u8], n: usize, fx: f32, fy: f32) -> bool {
    let map = rig.rect(rig.app.ids().map);
    let n = n as i32;
    let (rx, ry) = ((fx * map.w) as i32, (fy * map.h) as i32);
    let got = rig.px(rgba, map.x + rx as f32, map.y + ry as f32);
    let (u, v) = (((rx as f32 + 0.5) / map.w * n as f32) as i32, ((ry as f32 + 0.5) / map.h * n as f32) as i32);
    (-1..=1).flat_map(|dv| (-1..=1).map(move |du| (du, dv))).filter(|(du, dv)| (0..n).contains(&(u + du)) && (0..n).contains(&(v + dv))).any(|(du, dv)| {
        let i = (((v + dv) * n + u + du) as usize) * 4;
        near(got, [img[i], img[i + 1], img[i + 2]], 2)
    })
}

#[test]
fn r10_the_map_fills_in_over_the_sketch_during_its_pass() {
    let root = root("runs-r10");
    let from = root.join("data");
    small_out(&from);
    let world = World::load(&from).unwrap();
    let out = root.join("out");
    let mut state = PreviewState::for_world(Some(&world));
    state.seed = "r10-seed-2".to_string();
    state.view = View::Biomes.index();
    let mut rig = Rig::new(&world, state, Some(runs(&root, &[("MERE_STUB", "stages"), ("MERE_STUB_FROM", from.to_str().expect("a UTF-8 path"))])));
    rig.park();
    let g = rig.app.ids().generate;
    rig.click_on(g);
    assert!(rig.app.running());
    let drawn = |r: &Rig| r.app.state().follow.as_ref().map(|f| f.drawn);
    rig.until(30, "the run's sketch on the stage", |r| matches!(drawn(r), Some(Drawn::Sketch(32))));

    // The map's pass at 4 tiles of 10: its north half is sampled, its south half not yet.
    std::fs::write(out.join("go-map"), "").unwrap();
    rig.until(30, "the run's partial map on the stage", |r| drawn(r) == Some(Drawn::Partial(4)));
    let sketch = World::load_sketch(&out).unwrap();
    let (partial, done) = World::load_partial(&out, &sketch).unwrap();
    assert_eq!(done, 4);
    let id = |w: &World, i: usize| w.biomes[w.biome_at[i] as usize].id.clone();
    assert_eq!((partial.heights[0], id(&partial, 0).as_str()), (100, "minecraft:desert"), "a sampled point is the map's");
    assert_eq!((partial.heights[255], id(&partial, 255).as_str()), (70, "minecraft:plains"), "a point not sampled yet is the sketch's");
    rig.park();
    let rgba = rig.shot("run_partial.png");
    let img = paint(&partial, View::Biomes, true, &BTreeSet::new());
    let (desert, plains) = (&img[..4], &img[255 * 4..256 * 4]);
    assert_ne!(desert, plains, "the two halves are told apart in the biomes' view");
    assert!(shows_at(&rig, &rgba, &img, 16, 0.1, 0.1), "the north of the stage shows the map's own points, not veiled");
    assert!(shows_at(&rig, &rgba, &img, 16, 0.1, 0.9), "the south of the stage shows the sketch under them");
    assert!(rig.app.ids().status_text.ends_with(text::SKETCH_READABLE), "{}", rig.app.ids().status_text);
    assert!(rig.app.running(), "the run goes on");

    // The whole map replaces it, then the run ends.
    std::fs::write(out.join("go-full"), "").unwrap();
    rig.until(30, "the run's map on the stage", |r| drawn(r) == Some(Drawn::Map));
    std::fs::write(out.join("go-end"), "").unwrap();
    rig.until(30, "the run to end", |r| !r.app.running());
    rig.park();
    let rgba = rig.shot("run_partial_finished.png");
    let map = World::load(&out).unwrap();
    assert!(shows_at(&rig, &rgba, &paint(&map, View::Biomes, true, &BTreeSet::new()), 16, 0.1, 0.9), "the south is the map's own at the end");
}

#[test]
fn r7_the_stub_writes_nowhere_but_target_tmp() {
    let Some(_) = world() else { return };
    let outside = plain(Path::new(env!("CARGO_MANIFEST_DIR")).join("target").as_path()).join("guard-outside");
    let _ = std::fs::remove_dir_all(&outside);
    let mut options = world_preview::Options::new(outside.clone());
    options.out = Some(outside.clone());
    let result = stub(&[])(&options, &mut |_| {});
    assert!(result.is_err_and(|e| e.starts_with("stub: refusing")), "the stub accepted an out folder outside target\\tmp");
    assert!(!outside.exists(), "the stub created {}", outside.display());
}
