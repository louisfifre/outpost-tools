//! Apply, through the panel: the place line and its list of places, the button, the writes, the words that follow,
//! with a fake machine and folders of the test's own under target\tmp (games, a launcher's profiles, a pack). No
//! world is needed; with one at PREVIEW_DATA_DIR the pictures show its map, as the boards do.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mere_base::{App, Event, Key, Mods, Vec2};
use mere_ui::{Id, Theme};
use mere_win32::Renderer;
use preview::apply::{Host, Notice, Proc, Refusal, Running, Snapshot};
use preview::places::{Game, Kind, Sources};
use preview::settings::{Kind as SetKind, SETTINGS, pack_values};
use preview::{PreviewApp, PreviewState, text};

use common::{H, Rig, W};

struct Machine(Vec<Proc>);

impl Host for Machine {
    fn processes(&self) -> Result<Vec<Proc>, String> {
        Ok(self.0.clone())
    }
}

const SERVER: &str = r#"C:\bin\java.exe -Xmx6144M -jar C:\server\fabric-server-26.3-0.19.2.jar nogui"#;

/// A config that holds every key of the panel, with a comment above each.
fn config_text() -> String {
    let mut s = String::from("{\n");
    for set in SETTINGS.iter() {
        let v = if matches!(set.kind, SetKind::Switch) { (set.pack != 0.0).to_string() } else { set.pack.to_string() };
        s.push_str(&format!("  // About {}.\n  \"{}\": {v},\n", set.label, set.key));
    }
    s.push_str("  \"minor_version\": 1\n}\n");
    s
}

fn file_of(dir: &Path) -> PathBuf {
    dir.join("config").join("tectonic.json")
}

/// A folder with a mods folder and the config: a game, or a pack.
fn holder(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir.join("mods")).unwrap();
    std::fs::create_dir_all(dir.join("config")).unwrap();
    std::fs::write(file_of(dir), config_text()).unwrap();
    dir.to_path_buf()
}

/// A pack folder whose config holds every key of the panel: `<root>\pack`.
fn pack(name: &str) -> PathBuf {
    let root = common::root(name);
    holder(&root.join("pack"));
    root
}

/// The official launcher's folder under `root`, its profiles declaring these game folders.
fn launcher(root: &Path, games: &[(&str, &Path)]) -> PathBuf {
    let mc = root.join(".minecraft");
    std::fs::create_dir_all(&mc).unwrap();
    let profiles: Vec<String> = games
        .iter()
        .enumerate()
        .map(|(i, (name, dir))| format!("\"p{i}\":{{\"name\":\"{name}\",\"type\":\"custom\",\"gameDir\":\"{}\"}}", dir.to_string_lossy().replace('\\', "\\\\")))
        .collect();
    std::fs::write(mc.join("launcher_profiles.json"), format!("{{\"profiles\":{{{}}},\"version\":3}}", profiles.join(","))).unwrap();
    mc
}

/// How a test opens the window.
struct Open<'a> {
    pack: bool,
    sources: Option<Sources>,
    procs: Vec<Proc>,
    world: bool,
    size: (u32, u32),
    root: &'a Path,
}

impl<'a> Open<'a> {
    fn on(root: &'a Path) -> Open<'a> {
        Open { pack: false, sources: None, procs: Vec::new(), world: false, size: (W, H), root }
    }
}

fn open(o: Open) -> Rig {
    let theme = Theme::dark();
    let (w, h) = o.size;
    let mut r = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
    let mut runs = common::runs(o.root, &[]);
    if o.pack {
        runs.pack = Some(o.root.join("pack"));
    }
    let world = if o.world { common::world() } else { None };
    let state = PreviewState::for_world(world.as_ref());
    let mut app = PreviewApp::new(theme, true, Vec2::new(w as f32, h as f32), state, world).with_runs(runs).with_apply_host(Arc::new(Machine(o.procs)));
    if let Some(s) = o.sources {
        app = app.with_places(s);
    }
    app.start(&mut r);
    app.event(Event::Resized { width_px: w, height_px: h, scale: 1.0 });
    app.frame(&mut r, 0.0);
    app.frame(&mut r, 0.0);
    Rig { app, r, theme }
}

/// The old way in: the window on the root's pack (or none), no game.
fn rig(root: &Path, with_pack: bool, procs: Vec<Proc>) -> Rig {
    open(Open { pack: with_pack, procs, ..Open::on(root) })
}

fn index(key: &str) -> usize {
    SETTINGS.iter().position(|s| s.key == key).unwrap()
}

fn key(rig: &mut Rig, k: Key) {
    rig.app.event(Event::Key { key: k, mods: Mods::default() });
    rig.frame();
}

/// Opens the list above the place line, if it is closed.
fn open_list(rig: &mut Rig) {
    if !rig.app.state().places_open {
        let id = rig.app.ids().place_line;
        rig.click_on(id);
    }
    assert!(rig.app.state().places_open, "the list opened");
}

/// Ticks (or unticks) these rows of the list, then closes it with Escape.
fn tick(rig: &mut Rig, rows: &[usize]) {
    open_list(rig);
    for &i in rows {
        let id = rig.app.ids().place_rows[i];
        rig.click_on(id);
    }
    key(rig, Key::Escape);
    assert!(!rig.app.state().places_open, "Escape closed the list");
}

/// A press on an empty spot of the middle column: the focus leaves the line, as at rest.
fn rest(rig: &mut Rig, at: Vec2) {
    rig.click(at);
    assert_eq!(rig.app.ui().focused(), None, "nothing has the focus");
    rig.park();
}

fn apply_now(rig: &mut Rig) {
    let id = rig.app.ids().apply;
    rig.click_on(id);
    rig.until(20, "the write to end", |r| !r.app.applying());
    rig.frame();
}

/// The form as on the boards: two values moved from the pack's.
fn moved(rig: &mut Rig) {
    let base = rig.app.state().baseline;
    let s = rig.app.state_mut();
    s.values = base;
    s.values[index("vertical_scale")] = 1.75;
    s.values[index("erosion_scale")] = 0.15;
    rig.frame();
}

fn done_report(rig: &Rig) -> preview::apply::Report {
    match &rig.app.state().apply {
        Notice::Done(r) => r.clone(),
        other => panic!("no apply ended: {other:?}"),
    }
}

#[test]
fn a1_without_a_place_or_a_change_the_button_does_nothing() {
    let root = pack("apply_a1");
    let mut none = rig(&root, false, vec![]);
    none.app.state_mut().values[index("vertical_scale")] = 2.0;
    none.frame();
    let id = none.app.ids().apply;
    none.click_on(id);
    assert!(!none.app.applying());
    assert_eq!(none.app.state().apply, Notice::None);

    // Another folder: the first window kept the value typed in its own (`draft.json`).
    let root = pack("apply_a1_same");
    let mut same = rig(&root, true, vec![]);
    let id = same.app.ids().apply;
    same.click_on(id);
    assert!(!same.app.applying(), "nothing is ticked");
    tick(&mut same, &[0]);
    let id = same.app.ids().apply;
    same.click_on(id);
    assert!(!same.app.applying(), "no value differs from the pack's");
    assert_eq!(std::fs::read_to_string(root.join("pack/config/tectonic.json")).unwrap().matches("About ").count(), 43);
}

#[test]
fn a2_a_click_writes_the_value_keeps_the_comments_and_moves_the_baseline() {
    let root = pack("apply_a2");
    let file = root.join("pack/config/tectonic.json");
    let before = std::fs::read_to_string(&file).unwrap();
    let mut rig = rig(&root, true, vec![Proc { name: "java.exe".into(), cmd: SERVER.into() }]);
    tick(&mut rig, &[0]);
    assert_eq!(rig.app.ids().place_text, text::place_to(text::SOURCE_SHORT));
    rig.app.state_mut().values[index("vertical_scale")] = 2.0;
    rig.frame();
    apply_now(&mut rig);
    let after = std::fs::read_to_string(&file).unwrap();
    assert_eq!(after.replace("\"vertical_scale\": 2.0,", "\"vertical_scale\": 1.5,"), before, "only that value changed");
    assert_eq!(rig.app.state().baseline[index("vertical_scale")], 2.0);
    assert_eq!(rig.app.state().baseline, rig.app.state().values);
    let report = done_report(&rig);
    assert_eq!(report.running, vec![Running::Server(PathBuf::from(r"C:\server"))]);
    assert_eq!(report.places.iter().map(|w| (w.kind, w.result, w.snapshot)).collect::<Vec<_>>(), vec![(Kind::Source, Ok(1), Snapshot::Absent)]);
    let said = format!("{} {} {}", text::saved_for(text::SOURCE_SHORT), text::APPLY_SERVER, text::APPLY_LAND);
    assert_eq!(rig.app.ids().apply_text, said, "the panel says when the running server takes it");
    // The "pack" tab now puts back what was applied.
    assert_ne!(pack_values(), rig.app.state().baseline);
    rig.shot("apply_done.png");
}

#[test]
fn a3_pictures_of_the_block() {
    let root = pack("apply_a3");
    let mut off = rig(&root, false, vec![]);
    off.shot("apply_no_pack.png");
    let mut on = rig(&root, true, vec![]);
    tick(&mut on, &[0]);
    on.app.state_mut().values[index("ocean_offset")] = -0.5;
    on.shot("apply_enabled.png");
}

#[test]
fn a4_the_pack_s_values_are_the_ones_its_file_holds() {
    // The file was edited since the copy in the code was made: the form and "Pack" follow the file.
    let root = pack("apply_a4");
    let file = root.join("pack/config/tectonic.json");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"vertical_scale\": 1.5,"));
    std::fs::write(&file, text.replace("\"vertical_scale\": 1.5,", "\"vertical_scale\": 1.75,")).unwrap();
    let rig = rig(&root, true, vec![]);
    let i = index("vertical_scale");
    assert_eq!(rig.app.state().baseline[i], 1.75, "the pack's value is the file's");
    assert_eq!(rig.app.state().values, rig.app.state().baseline, "a form with no run behind it opens on the pack");
    assert_eq!(rig.app.ids().pack_marks[i], Id::ROOT, "nothing differs, nothing is marked");
}

#[test]
fn a5_a_value_typed_and_not_generated_is_there_at_the_next_start() {
    let root = pack("apply_a5");
    let i = index("ocean_depth");
    {
        let mut first = rig(&root, true, vec![]);
        first.app.state_mut().values[i] = -0.3;
        first.frame();
    }
    let second = rig(&root, true, vec![]);
    assert_eq!(second.app.state().values[i], -0.3, "what was typed came back");
    assert_eq!(second.app.state().baseline[i], SETTINGS[i].pack, "and the pack's value is still the pack's");
    assert_ne!(second.app.ids().pack_marks[i], Id::ROOT, "the field says it differs from the pack");
}

/// A prod copy: the source pack (`apply_pack`, the checkout's, which a deploy keeps) and the snapshot the runs and the
/// game's sync use (`pack`). Ticks the pack's source, applies max_y = 448 and returns the rig.
fn prod_copy(name: &str, snapshot: &Path, source: &Path) -> Rig {
    let mut rig = open_prod(snapshot, source);
    tick(&mut rig, &[0]);
    rig.app.state_mut().values[index("max_y")] = 448.0;
    rig.frame();
    let id = rig.app.ids().apply;
    rig.click_on(id);
    rig.until(20, &format!("the write of {name} to end"), |r| !r.app.applying());
    rig.frame();
    rig
}

/// The window opened on a prod copy's two packs.
fn open_prod(snapshot: &Path, source: &Path) -> Rig {
    let theme = Theme::dark();
    let mut r = Renderer::offscreen(W, H, 1.0).expect("offscreen renderer");
    let mut runs = common::runs(snapshot, &[]);
    runs.pack = Some(snapshot.join("pack"));
    runs.apply_pack = Some(source.join("pack"));
    let mut app = PreviewApp::new(theme, true, Vec2::new(W as f32, H as f32), PreviewState::default(), None).with_runs(runs).with_apply_host(Arc::new(Machine(vec![])));
    app.start(&mut r);
    app.event(Event::Resized { width_px: W, height_px: H, scale: 1.0 });
    app.frame(&mut r, 0.0);
    app.frame(&mut r, 0.0);
    Rig { app, r, theme }
}

/// A prod copy runs on a snapshot of the pack that each deploy replaces, and the game's sync serves it: Apply writes
/// the source pack (which survives a deploy once committed) and the snapshot (which the game reads at its next start).
#[test]
fn a7_from_a_prod_copy_both_the_source_and_the_snapshot_are_written() {
    let source = pack("apply_a7_source");
    let snapshot = pack("apply_a7_snapshot");
    let (source_file, snapshot_file) = (source.join("pack/config/tectonic.json"), snapshot.join("pack/config/tectonic.json"));
    let rig = prod_copy("a7", &snapshot, &source);
    assert!(std::fs::read_to_string(&source_file).unwrap().contains("\"max_y\": 448"), "the source pack holds the value");
    assert!(std::fs::read_to_string(&snapshot_file).unwrap().contains("\"max_y\": 448"), "the snapshot, which the game's sync serves, holds it too");
    let report = done_report(&rig);
    assert_eq!((report.places[0].result, report.places[0].snapshot), (Ok(1), Snapshot::Written));
    assert!(rig.app.ids().apply_text.contains(text::APPLY_PROD_LIMIT), "the one limit left is said");
}

#[test]
fn a8_a_snapshot_that_cannot_be_written_is_said_and_the_source_is_still_written() {
    let source = pack("apply_a8_source");
    let snapshot = pack("apply_a8_snapshot");
    std::fs::remove_file(snapshot.join("pack/config/tectonic.json")).unwrap();
    let rig = prod_copy("a8", &snapshot, &source);
    assert!(std::fs::read_to_string(source.join("pack/config/tectonic.json")).unwrap().contains("\"max_y\": 448"));
    let report = done_report(&rig);
    assert_eq!((report.places[0].result, report.places[0].snapshot), (Ok(1), Snapshot::Failed));
    assert!(!snapshot.join("pack/config/tectonic.json").exists(), "the snapshot's file is not created");
    assert!(rig.app.ids().apply_text.ends_with(text::APPLY_FAIL_SNAPSHOT), "the notice says which file was not written");
}

/// From a checkout there is no snapshot: one file, and no word about a deploy.
#[test]
fn a9_from_a_checkout_one_file_is_written_and_no_limit_is_said() {
    let root = pack("apply_a9");
    let mut rig = rig(&root, true, vec![]);
    tick(&mut rig, &[0]);
    rig.app.state_mut().values[index("max_y")] = 448.0;
    rig.frame();
    apply_now(&mut rig);
    let report = done_report(&rig);
    assert_eq!((report.places[0].result, report.places[0].snapshot), (Ok(1), Snapshot::Absent));
    assert_eq!(rig.app.ids().apply_text, format!("{} {}", text::saved_for(text::SOURCE_SHORT), text::APPLY_LAND));
}

/// The checkout already holds the form's values (an Apply made before the snapshot was written too, or a deploy of an
/// older commit) but the snapshot is behind: nothing differs from the pack, yet Apply is lit, and it repairs the snapshot.
#[test]
fn a10_a_snapshot_behind_the_pack_lights_apply_and_is_brought_up_to_date() {
    let source = pack("apply_a10_source");
    let snapshot = pack("apply_a10_snapshot");
    let (source_file, snapshot_file) = (source.join("pack/config/tectonic.json"), snapshot.join("pack/config/tectonic.json"));
    let held = std::fs::read_to_string(&source_file).unwrap();
    std::fs::write(&source_file, held.replace("\"max_y\": 320,", "\"max_y\": 448,")).unwrap();
    let mut rig = open_prod(&snapshot, &source);
    tick(&mut rig, &[0]);
    assert_eq!(rig.app.state().values, rig.app.state().baseline, "the form is the checkout's");
    assert!(std::fs::read_to_string(&snapshot_file).unwrap().contains("\"max_y\": 320"));
    let id = rig.app.ids().apply;
    rig.click_on(id);
    assert!(rig.app.applying(), "Apply is enabled though the form equals the pack");
    rig.until(20, "the write to end", |r| !r.app.applying());
    rig.frame();
    assert!(std::fs::read_to_string(&snapshot_file).unwrap().contains("\"max_y\": 448"), "the snapshot holds the form's values");
    assert_eq!(done_report(&rig).places[0].snapshot, Snapshot::Written);
    let id = rig.app.ids().apply;
    rig.click_on(id);
    assert!(!rig.app.applying(), "both files hold the values: Apply is disabled again");
}

/// A field's arithmetic leaves float noise far under the millionth the file holds: that is no change, so Apply stays
/// off, the field is not marked and "Pack" stays pressed, as Apply itself would write nothing.
#[test]
fn a11_float_noise_is_not_a_change_from_the_pack() {
    let root = pack("apply_a11");
    let mut rig = rig(&root, true, vec![]);
    tick(&mut rig, &[0]);
    let i = index("vertical_scale");
    rig.app.state_mut().values[i] = rig.app.state().baseline[i] + 1e-12;
    rig.frame();
    assert_eq!(rig.app.ids().pack_marks[i], Id::ROOT, "the field is not marked");
    let id = rig.app.ids().apply;
    rig.click_on(id);
    assert!(!rig.app.applying(), "Apply is disabled: the pack already holds the value");
}

// The boards of "Apply: after review", state by state. A friend's machine: games of the official launcher and of
// Prism, no pack repository.

/// One game, in the official launcher only: `<root>\.minecraft-exploration`.
fn one_game(root: &Path) -> (Sources, PathBuf) {
    let game = holder(&root.join(".minecraft-exploration"));
    let mc = launcher(root, &[("Exploration", &game)]);
    (Sources { minecraft: Some(mc), ..Sources::default() }, game)
}

/// The same pack installed twice: in Prism and in the official launcher.
fn two_games(root: &Path) -> (Sources, PathBuf, PathBuf) {
    let prism = holder(&root.join(r"PrismLauncher\instances\Exploration\minecraft"));
    let vanilla = holder(&root.join(".minecraft-exploration"));
    let mc = launcher(root, &[("Exploration", &vanilla)]);
    (Sources { prism: vec![Game { name: "Exploration".into(), dir: prism.clone() }], minecraft: Some(mc), dev_server: None }, prism, vanilla)
}

#[test]
fn b1_to_b3_one_game_found_is_ticked_listed_and_written() {
    let root = common::root("apply_b1");
    let (sources, game) = one_game(&root);
    let mut rig = open(Open { sources: Some(sources), world: true, ..Open::on(&root) });
    // 1: at rest. The only place, a game: ticked.
    moved(&mut rig);
    assert_eq!(rig.app.ids().place_text, "To: Exploration");
    assert!(rig.app.state().places[0].ticked);
    assert!(std::fs::read_to_string(root.join("places.json")).unwrap().contains("minecraft-exploration"), "the tick is kept");
    rig.park();
    rig.shot("apply3_one_rest.png");
    // 2: the list open above the line, exactly as wide, its first row focused.
    open_list(&mut rig);
    let (line, list) = (rig.rect(rig.app.ids().place_line), rig.rect(rig.app.ids().place_list));
    assert_eq!((list.x, list.w), (line.x, line.w), "the list is the line's width");
    assert!(list.y + list.h <= line.y, "the list hangs above the line");
    assert_eq!(rig.app.ids().place_rows.len(), 1);
    assert_eq!(rig.app.ui().focused(), Some(rig.app.ids().place_rows[0]), "the first row has the focus");
    rig.park();
    rig.shot("apply3_one_open.png");
    // Enter, from the list, applies.
    key(&mut rig, Key::Enter);
    assert!(rig.app.applying() || matches!(rig.app.state().apply, Notice::Done(_)), "Enter applied");
    rig.until(20, "the write to end", |r| !r.app.applying());
    key(&mut rig, Key::Escape);
    // 3: just after. One sentence; Apply is dim until a value changes; the seams sentence gave way.
    let written = std::fs::read_to_string(file_of(&game)).unwrap();
    assert!(written.contains("\"vertical_scale\": 1.75,") && written.contains("\"erosion_scale\": 0.15,"));
    assert_eq!(rig.app.ids().apply_text, "Saved for Exploration. Restart the game to use it. Only land nobody has visited yet will change.");
    assert_eq!(rig.app.state().baseline, pack_values(), "a game is not the pack: Pack still puts the pack's values back");
    rest(&mut rig, Vec2::new(700.0, 820.0));
    let shot = rig.shot("apply3_done.png");
    assert!(!shot.is_empty());
    let id = rig.app.ids().apply;
    rig.click_on(id);
    assert!(!rig.app.applying(), "Apply is dim after a full success");
    rig.app.state_mut().values[index("ocean_offset")] = -0.5;
    rig.frame();
    assert!(rig.app.ids().apply_text.is_empty(), "a value moved: the sentence is about what is shown no more");
    let id = rig.app.ids().apply;
    rig.click_on(id);
    assert!(rig.app.applying(), "a value changed: Apply is lit again");
    rig.until(20, "the write to end", |r| !r.app.applying());
}

#[test]
fn b4_b5_two_games_nothing_ticked_then_one() {
    let root = common::root("apply_b4");
    let (sources, prism, vanilla) = two_games(&root);
    let mut rig = open(Open { sources: Some(sources), world: true, ..Open::on(&root) });
    moved(&mut rig);
    // 4: two places, nothing ticked: say what to do, Apply is dim.
    assert_eq!(rig.app.ids().place_text, text::PLACE_CHOOSE_GAME);
    assert!(rig.app.state().places.iter().all(|p| !p.ticked));
    let id = rig.app.ids().apply;
    rig.click_on(id);
    assert!(!rig.app.applying(), "nothing ticked, nothing written");
    rig.park();
    rig.shot("apply3_two_first.png");
    // 5: the second ticked, the list stays open; the line names it with its launcher.
    open_list(&mut rig);
    let id = rig.app.ids().place_rows[1];
    rig.click_on(id);
    assert!(rig.app.state().places_open, "the list stays open while ticking");
    assert_eq!(rig.app.ids().place_text, "To: Exploration (Minecraft Launcher)");
    rig.park();
    rig.shot("apply3_two_open.png");
    // A press outside closes the list and reaches nothing.
    let seed = rig.app.ids().seed;
    rig.click_on(seed);
    assert!(!rig.app.state().places_open);
    assert_ne!(rig.app.ui().focused(), Some(seed), "the press went to no field");
    apply_now(&mut rig);
    assert!(std::fs::read_to_string(file_of(&vanilla)).unwrap().contains("\"vertical_scale\": 1.75,"));
    assert!(std::fs::read_to_string(file_of(&prism)).unwrap().contains("\"vertical_scale\": 1.5,"), "an unticked game is not written");
    // The next session keeps the tick; a place gone is dropped from the set.
    drop(rig);
    let mut again = open(Open { sources: Some(two_games_again(&root)), ..Open::on(&root) });
    assert_eq!(again.app.ids().place_text, "To: Exploration (Minecraft Launcher)");
    std::fs::remove_file(file_of(&vanilla)).unwrap();
    drop(again);
    again = open(Open { sources: Some(two_games_again(&root)), ..Open::on(&root) });
    assert_eq!(again.app.state().places.len(), 1);
    assert_eq!(again.app.ids().place_text, text::PLACE_CHOOSE_GAME, "the place gone is no longer ticked; the one left was not ticked");
    assert!(!again.app.state().places[0].ticked, "a single game left is not ticked against a kept choice");
    assert_eq!(std::fs::read_to_string(root.join("places.json")).unwrap(), "{\"ticked\":[]}", "dropped from the set");
}

/// The sources of `two_games` on folders already made.
fn two_games_again(root: &Path) -> Sources {
    let prism = root.join(r"PrismLauncher\instances\Exploration\minecraft");
    Sources { prism: vec![Game { name: "Exploration".into(), dir: prism }], minecraft: Some(root.join(".minecraft")), dev_server: None }
}

#[test]
fn b6_one_place_saved_one_refused() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = common::root("apply_b6");
    let (sources, prism, vanilla) = two_games(&root);
    let mut rig = open(Open { sources: Some(sources), world: true, ..Open::on(&root) });
    moved(&mut rig);
    tick(&mut rig, &[0, 1]);
    assert_eq!(rig.app.ids().place_text, "To: 2 places");
    // The game holds its file without sharing it.
    let held = std::fs::OpenOptions::new().read(true).share_mode(0).open(file_of(&vanilla)).unwrap();
    apply_now(&mut rig);
    drop(held);
    let report = done_report(&rig);
    assert_eq!(report.places.iter().map(|w| w.result).collect::<Vec<_>>(), vec![Ok(2), Err(Refusal::InUse)]);
    assert!(std::fs::read_to_string(file_of(&prism)).unwrap().contains("\"vertical_scale\": 1.75,"));
    assert_eq!(
        rig.app.ids().apply_text,
        "Saved for Exploration (Prism Launcher). Restart the game to use it. Only land nobody has visited yet will change.\nNot saved for Exploration (Minecraft Launcher): the game is using the file. Close the game, then apply again."
    );
    rig.park();
    rig.shot("apply3_partial.png");
    // The refused place does not hold the values: Apply stays lit, and a second try saves it.
    apply_now(&mut rig);
    assert_eq!(done_report(&rig).places.iter().map(|w| w.result).collect::<Vec<_>>(), vec![Ok(0), Ok(2)]);
    // A place whose file went away is refused, and no file is made.
    std::fs::remove_file(file_of(&vanilla)).unwrap();
    rig.app.state_mut().values[index("ocean_offset")] = -0.5;
    rig.frame();
    apply_now(&mut rig);
    assert_eq!(done_report(&rig).places[1].result, Err(Refusal::Missing));
    assert!(!file_of(&vanilla).exists());
    assert!(rig.app.ids().apply_text.ends_with(&text::not_saved("Exploration (Minecraft Launcher)", text::REFUSED_MISSING)));
}

#[test]
fn b7_nothing_found() {
    let root = common::root("apply_b7");
    // A launcher whose only game has no Tectonic config: no place.
    std::fs::create_dir_all(root.join(r".minecraft-plain\mods")).unwrap();
    let mc = launcher(&root, &[("Plain", &root.join(".minecraft-plain"))]);
    let mut rig = open(Open { sources: Some(Sources { minecraft: Some(mc), ..Sources::default() }), world: true, ..Open::on(&root) });
    moved(&mut rig);
    assert!(rig.app.state().places.is_empty());
    assert_eq!(rig.app.ids().place_text, text::PLACE_NONE_FOUND);
    let id = rig.app.ids().place_line;
    rig.click_on(id);
    assert!(!rig.app.state().places_open, "an empty list does not open");
    let id = rig.app.ids().apply;
    rig.click_on(id);
    assert!(!rig.app.applying());
    rig.park();
    rig.shot("apply3_none.png");
}

#[test]
fn b9_the_maintainer_s_machine_with_the_list_open() {
    let root = common::root("apply_b9");
    let exploration = holder(&root.join(r"PrismLauncher\instances\Exploration\minecraft"));
    let atelier = holder(&root.join(r"PrismLauncher\instances\Atelier\minecraft"));
    let server = holder(&root.join("server"));
    holder(&root.join("pack"));
    let sources = Sources {
        prism: vec![Game { name: "Exploration".into(), dir: exploration.clone() }, Game { name: "Atelier".into(), dir: atelier }],
        minecraft: None,
        dev_server: Some(server),
    };
    // Exploration was ticked in an earlier session.
    let key = exploration.to_string_lossy().replace('/', "\\").to_lowercase().replace('\\', "\\\\");
    std::fs::write(root.join("places.json"), format!("{{\"ticked\":[\"{key}\"]}}")).unwrap();
    let mut rig = open(Open { pack: true, sources: Some(sources), world: true, ..Open::on(&root) });
    moved(&mut rig);
    let kinds: Vec<(Kind, bool)> = rig.app.state().places.iter().map(|p| (p.kind, p.ticked)).collect();
    assert_eq!(kinds, vec![(Kind::Game, true), (Kind::Game, false), (Kind::Server, false), (Kind::Source, false)], "the repository's places last, not ticked");
    assert_eq!(rig.app.ids().place_text, "To: Exploration");
    open_list(&mut rig);
    rig.park();
    rig.shot("apply3_maint.png");
    // No scroll bar: every row is inside the window.
    let list = rig.rect(rig.app.ids().place_list);
    assert!(list.y >= 0.0, "the list fits above the line: {list:?}");
}

#[test]
fn b10_the_small_window_holds_the_line_and_apply() {
    let root = common::root("apply_b10");
    let (sources, _, _) = two_games(&root);
    let (w, h) = preview::min_size_px((800, 600));
    let mut rig = open(Open { sources: Some(sources), world: true, size: (w, h), ..Open::on(&root) });
    moved(&mut rig);
    tick(&mut rig, &[1]);
    let (line, apply, left) = (rig.rect(rig.app.ids().place_line), rig.rect(rig.app.ids().apply), rig.rect(rig.app.ids().left));
    assert!(line.x >= left.x && line.x + line.w <= left.x + left.w + 0.01, "the line holds in the panel: {line:?} in {left:?}");
    assert_eq!((apply.x, apply.w), (line.x, line.w), "Apply is as wide as the line");
    rest(&mut rig, Vec2::new(400.0, 450.0));
    let rgba = rig.rgba();
    let path = common::tmp().join("apply3_small.png");
    std::fs::write(&path, mere_png::encode(w, h, &rgba)).unwrap();
    println!("wrote {}", path.display());
    // And the list opens above it inside the window.
    open_list(&mut rig);
    let list = rig.rect(rig.app.ids().place_list);
    assert!(list.y >= 0.0 && list.x + list.w <= w as f32, "{list:?}");
}

/// Space on the focused line opens the list, Escape closes it and gives the focus back to the line.
#[test]
fn b11_the_keyboard_opens_ticks_and_closes() {
    let root = common::root("apply_b11");
    let (sources, ..) = two_games(&root);
    let mut rig = open(Open { sources: Some(sources), ..Open::on(&root) });
    open_list(&mut rig);
    rig.app.event(Event::Char { ch: ' ' });
    rig.frame();
    assert!(rig.app.state().places[0].ticked, "Space ticks the focused row");
    key(&mut rig, Key::Escape);
    assert!(!rig.app.state().places_open);
    assert_eq!(rig.app.ui().focused(), Some(rig.app.ids().place_line), "the focus is back on the line");
    rig.app.event(Event::Char { ch: ' ' });
    rig.frame();
    assert!(rig.app.state().places_open, "Space on the line opens the list again");
}
