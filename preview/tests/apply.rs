//! Apply to pack, through the panel: the button, the write, the words that follow, with a fake machine and a pack
//! folder of the test's own under target\tmp. No world is needed.

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use mere_base::{App, Event, Vec2};
use mere_ui::Theme;
use mere_win32::Renderer;
use preview::apply::{Host, Notice, Proc, Running, Snapshot};
use preview::settings::{Kind, SETTINGS, pack_values};
use preview::{PreviewApp, PreviewState, text};

use common::{H, Rig, W};

struct Machine(Vec<Proc>);

impl Host for Machine {
    fn processes(&self) -> Result<Vec<Proc>, String> {
        Ok(self.0.clone())
    }
}

const SERVER: &str = r#"C:\bin\java.exe -Xmx6144M -jar C:\server\fabric-server-26.3-0.19.2.jar nogui"#;

/// A pack folder whose config holds every key of the panel, with a comment above each.
fn pack(name: &str) -> PathBuf {
    let root = common::root(name);
    let dir = root.join("pack").join("config");
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = String::from("{\n");
    for set in SETTINGS.iter() {
        let v = if matches!(set.kind, Kind::Switch) { (set.pack != 0.0).to_string() } else { set.pack.to_string() };
        s.push_str(&format!("  // About {}.\n  \"{}\": {v},\n", set.label, set.key));
    }
    s.push_str("  \"minor_version\": 1\n}\n");
    std::fs::write(dir.join("tectonic.json"), s).unwrap();
    root
}

fn rig(root: &std::path::Path, with_pack: bool, procs: Vec<Proc>) -> Rig {
    let theme = Theme::dark();
    let mut r = Renderer::offscreen(W, H, 1.0).expect("offscreen renderer");
    let mut runs = common::runs(root, &[]);
    if with_pack {
        runs.pack = Some(root.join("pack"));
    }
    let mut app = PreviewApp::new(theme, true, Vec2::new(W as f32, H as f32), PreviewState::default(), None).with_runs(runs).with_apply_host(Arc::new(Machine(procs)));
    app.start(&mut r);
    app.event(Event::Resized { width_px: W, height_px: H, scale: 1.0 });
    app.frame(&mut r, 0.0);
    app.frame(&mut r, 0.0);
    Rig { app, r, theme }
}

fn index(key: &str) -> usize {
    SETTINGS.iter().position(|s| s.key == key).unwrap()
}

#[test]
fn a1_without_a_pack_or_a_change_the_button_does_nothing() {
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
    assert!(!same.app.applying(), "no value differs from the pack's");
    assert_eq!(std::fs::read_to_string(root.join("pack/config/tectonic.json")).unwrap().matches("About ").count(), 43);
}

#[test]
fn a2_a_click_writes_the_value_keeps_the_comments_and_moves_the_baseline() {
    let root = pack("apply_a2");
    let file = root.join("pack/config/tectonic.json");
    let before = std::fs::read_to_string(&file).unwrap();
    let mut rig = rig(&root, true, vec![Proc { name: "java.exe".into(), cmd: SERVER.into() }]);
    rig.app.state_mut().values[index("vertical_scale")] = 2.0;
    rig.frame();
    let id = rig.app.ids().apply;
    rig.click_on(id);
    rig.until(20, "the write to end", |r| !r.app.applying());
    rig.frame();
    let after = std::fs::read_to_string(&file).unwrap();
    assert_eq!(after.replace("\"vertical_scale\": 2.0,", "\"vertical_scale\": 1.5,"), before, "only that value changed");
    assert_eq!(rig.app.state().baseline[index("vertical_scale")], 2.0);
    assert_eq!(rig.app.state().baseline, rig.app.state().values);
    assert_eq!(rig.app.state().apply, Notice::Done { running: vec![Running::Server(PathBuf::from(r"C:\server"))], snapshot: Snapshot::Absent });
    assert_eq!(rig.app.ids().apply_text, text::APPLY_SERVER, "the panel says when the running server takes it");
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
    assert_eq!(rig.app.ids().pack_marks[i], mere_ui::Id::ROOT, "nothing differs, nothing is marked");
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
    assert_ne!(second.app.ids().pack_marks[i], mere_ui::Id::ROOT, "the field says it differs from the pack");
}

/// A prod copy: the source pack (`apply_pack`, the checkout's, which a deploy keeps) and the snapshot the runs and the
/// game's sync use (`pack`). Applies max_y = 448 and returns the rig.
fn prod_copy(name: &str, snapshot: &PathBuf, source: &PathBuf) -> Rig {
    let mut rig = open_prod(snapshot, source);
    rig.app.state_mut().values[index("max_y")] = 448.0;
    rig.frame();
    let id = rig.app.ids().apply;
    rig.click_on(id);
    rig.until(20, &format!("the write of {name} to end"), |r| !r.app.applying());
    rig.frame();
    rig
}

/// The window opened on a prod copy's two packs.
fn open_prod(snapshot: &PathBuf, source: &PathBuf) -> Rig {
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
    assert_eq!(rig.app.state().apply, Notice::Done { running: vec![], snapshot: Snapshot::Written });
    assert!(rig.app.ids().apply_text.ends_with(text::APPLY_PROD_LIMIT), "the one limit left is said");
}

#[test]
fn a8_a_snapshot_that_cannot_be_written_is_said_and_the_source_is_still_written() {
    let source = pack("apply_a8_source");
    let snapshot = pack("apply_a8_snapshot");
    std::fs::remove_file(snapshot.join("pack/config/tectonic.json")).unwrap();
    let rig = prod_copy("a8", &snapshot, &source);
    assert!(std::fs::read_to_string(source.join("pack/config/tectonic.json")).unwrap().contains("\"max_y\": 448"));
    assert_eq!(rig.app.state().apply, Notice::Done { running: vec![], snapshot: Snapshot::Failed });
    assert_eq!(rig.app.ids().apply_text, text::APPLY_FAIL_SNAPSHOT, "the notice says which file was not written");
}

/// From a checkout there is no snapshot: one file, and no word about a deploy.
#[test]
fn a9_from_a_checkout_one_file_is_written_and_no_limit_is_said() {
    let root = pack("apply_a9");
    let mut rig = rig(&root, true, vec![]);
    rig.app.state_mut().values[index("max_y")] = 448.0;
    rig.frame();
    let id = rig.app.ids().apply;
    rig.click_on(id);
    rig.until(20, "the write to end", |r| !r.app.applying());
    rig.frame();
    assert_eq!(rig.app.state().apply, Notice::Done { running: vec![], snapshot: Snapshot::Absent });
    assert_eq!(rig.app.ids().apply_text, text::APPLIED);
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
    assert_eq!(rig.app.state().values, rig.app.state().baseline, "the form is the checkout's");
    assert!(std::fs::read_to_string(&snapshot_file).unwrap().contains("\"max_y\": 320"));
    let id = rig.app.ids().apply;
    rig.click_on(id);
    assert!(rig.app.applying(), "Apply is enabled though the form equals the pack");
    rig.until(20, "the write to end", |r| !r.app.applying());
    rig.frame();
    assert!(std::fs::read_to_string(&snapshot_file).unwrap().contains("\"max_y\": 448"), "the snapshot holds the form's values");
    assert_eq!(rig.app.state().apply, Notice::Done { running: vec![], snapshot: Snapshot::Written });
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
    let i = index("vertical_scale");
    rig.app.state_mut().values[i] = rig.app.state().baseline[i] + 1e-12;
    rig.frame();
    assert_eq!(rig.app.ids().pack_marks[i], mere_ui::Id::ROOT, "the field is not marked");
    let id = rig.app.ids().apply;
    rig.click_on(id);
    assert!(!rig.app.applying(), "Apply is disabled: the pack already holds the value");
}