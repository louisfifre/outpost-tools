//! Export: the 2D pictures of the whole map written to a folder of the runs' own, the section's last sheet, and
//! the notice of where they went. Everything is written under target\tmp. Skipped (with a line) without the data.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use common::*;
use mere_base::Launcher;
use preview::paint::{View, paint};
use preview::json::Json;
use preview::run::{Runs, Steps};
use preview::{PreviewState, text};

fn saved(rig: &Rig) -> PathBuf {
    match &rig.app.state().steps {
        Steps::Saved { folder } => PathBuf::from(folder),
        other => panic!("no export notice: {other:?}"),
    }
}

fn decoded(file: &Path) -> mere_png::Decoded {
    mere_png::decode(&std::fs::read(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))).expect("a PNG this crate wrote")
}

/// The runs of an export: no engine is needed.
fn export_runs(root: &Path) -> Runs {
    Runs { engine_dir: None, ..runs(root, &[]) }
}

#[test]
fn e1_the_exported_picture_is_the_stage_content() {
    let Some(world) = world() else { return };
    let root = root("export-e1");
    let state = PreviewState { view: 0, ..PreviewState::for_world(Some(&world)) };
    let mut rig = Rig::new(&world, state, Some(export_runs(&root)));
    rig.park();
    let export = rig.app.ids().export;
    rig.click_on(export);
    rig.park();

    let folder = saved(&rig);
    assert!(folder.starts_with(root.join("exports")), "{}", folder.display());
    assert!(folder.file_name().unwrap().to_string_lossy().starts_with("cap-1_"), "{}", folder.display());
    let n = world.samples as u32;
    for view in [View::Biomes, View::Flat, View::Height, View::Temperature] {
        let png = decoded(&folder.join(format!("{}.png", view.key())));
        assert_eq!((png.width, png.height), (n, n));
        assert!(png.rgba == paint(&world, view, true, &BTreeSet::new()), "{} differs from paint()", view.key());
    }
    for absent in ["structures.png", "section.png"] {
        assert!(!folder.join(absent).exists(), "{absent} is not exported yet");
    }
    assert_eq!(rig.app.state().said, text::EXPORTED);

    // The picture of the notice: and the stage shows what the file holds, point for point.
    let rgba = rig.shot("export_done.png");
    let ids = rig.app.ids().clone();
    assert_eq!(ids.status_text, text::EXPORTED);
    assert_ne!(ids.saved, mere_ui::Id::ROOT);
    let file = decoded(&folder.join("biomes.png"));
    let map = rig.rect(ids.map);
    let side = map.w;
    let texel = |col: i32, row: i32| {
        let i = ((row.clamp(0, n as i32 - 1) as usize) * n as usize + col.clamp(0, n as i32 - 1) as usize) * 4;
        [file.rgba[i], file.rgba[i + 1], file.rgba[i + 2]]
    };
    let (mut checked, mut seed) = (0, 12345u64);
    for _ in 0..600 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let rx = ((seed >> 33) % side as u64) as i32;
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let ry = ((seed >> 33) % side as u64) as i32;
        // Away from the grid, the cross and the border, which lie over the picture.
        let near_line = |v: i32| (0..=8).any(|j| (v - (j as f32 * side / 8.0).round() as i32).abs() <= 1);
        let c = (side / 2.0).round() as i32;
        if near_line(rx) || near_line(ry) || (rx - c).abs() <= 9 && (ry - c).abs() <= 9 || rx < 1 || ry < 1 || rx > side as i32 - 2 || ry > side as i32 - 2 {
            continue;
        }
        let (u, v) = (((rx as f32 + 0.5) / side * n as f32) as i32, ((ry as f32 + 0.5) / side * n as f32) as i32);
        let got = rig.px(&rgba, map.x + rx as f32, map.y + ry as f32);
        let found = (-1..=1).flat_map(|dv| (-1..=1).map(move |du| (du, dv))).any(|(du, dv)| texel(u + du, v + dv) == got);
        assert!(found, "stage pixel ({rx},{ry}) is {got:?}, not one of the texels of biomes.png around ({u},{v})");
        checked += 1;
        if checked == 200 {
            break;
        }
    }
    assert_eq!(checked, 200, "only {checked} stage pixels were checked");
}

#[test]
fn e2_a_chosen_biome_does_not_change_the_export() {
    let Some(world) = world() else { return };
    let root = root("export-e2");
    let mut state = PreviewState { view: 0, ..PreviewState::for_world(Some(&world)) };
    state.chosen[0].insert("minecraft:badlands".to_string());
    let mut rig = Rig::new(&world, state, Some(export_runs(&root)));
    rig.park();
    let export = rig.app.ids().export;
    rig.click_on(export);
    let file = decoded(&saved(&rig).join("biomes.png"));
    let whole = paint(&world, View::Biomes, true, &BTreeSet::new());
    let chosen = paint(&world, View::Biomes, true, &BTreeSet::from(["minecraft:badlands".to_string()]));
    assert!(file.rgba == whole, "the page exports the whole map");
    assert!(file.rgba != chosen, "the choice's picture differs, so the test can tell");
}

#[test]
fn e3_the_section_is_exported_as_last_looked_at() {
    let Some(world) = world() else { return };
    let root = root("export-e3");
    let state = PreviewState { view: preview::paint::SECTION, ..PreviewState::for_world(Some(&world)) };
    let mut rig = Rig::new(&world, state, Some(export_runs(&root)));
    rig.until(20, "the cut's sheet to be uploaded", |r| r.app.last_sheet().is_some());
    rig.park();
    let export = rig.app.ids().export;
    rig.click_on(export);
    let (w, h, rgba) = rig.app.last_sheet().cloned().unwrap();
    let file = decoded(&saved(&rig).join("section.png"));
    assert_eq!((file.width, file.height), (w, h));
    assert!(file.rgba == rgba, "section.png is the sheet, alpha kept");
    assert!(file.rgba.chunks(4).any(|p| p[3] == 0), "the sheet has transparent parts, which the file keeps");
}

/// A launcher that records the folder it was asked to show and starts nothing.
struct Recorder(Arc<Mutex<Vec<PathBuf>>>);

impl Launcher for Recorder {
    fn spawn(&self, _: &mut std::process::Command) -> std::io::Result<std::process::Child> {
        Err(std::io::Error::other("a recorder starts nothing"))
    }
    fn stop(&self, _: &mut std::process::Child) -> std::io::Result<()> {
        Ok(())
    }
    fn open_folder(&self, path: &Path) -> std::io::Result<()> {
        self.0.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }
}

#[test]
fn e4_the_notice_opens_the_folder_and_closes() {
    let Some(world) = world() else { return };
    let root = root("export-e4");
    let opened = Arc::new(Mutex::new(Vec::new()));
    let runs = Runs { launcher: Box::new(Recorder(opened.clone())), ..export_runs(&root) };
    let mut rig = Rig::new(&world, PreviewState::for_world(Some(&world)), Some(runs));
    rig.park();
    let export = rig.app.ids().export;
    rig.click_on(export);
    rig.park();
    let folder = saved(&rig);
    let ids = rig.app.ids().clone();
    rig.click_on(ids.open_folder);
    assert_eq!(*opened.lock().unwrap(), vec![folder], "the folder of that export was opened");
    assert!(matches!(rig.app.state().steps, Steps::Saved { .. }), "opening leaves the notice");
    rig.click_on(ids.close_notice);
    assert_eq!(rig.app.state().steps, Steps::Empty);
}

/// E5: export.json holds what the page's does: the settings of the try, the pack, the figures, a list per
/// exported view with the page's rows, the biomes the pack could place and did not, and no ores outside a cut.
#[test]
fn e5_the_data_file_holds_the_settings_and_the_figures() {
    let Some(world) = world() else { return };
    let root = root("export-e5");
    let mut rig = Rig::new(&world, PreviewState { view: 0, ..PreviewState::for_world(Some(&world)) }, Some(export_runs(&root)));
    rig.park();
    let export = rig.app.ids().export;
    rig.click_on(export);
    let file = saved(&rig).join("export.json");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let json = Json::parse(&text).expect("the file is JSON");

    assert_eq!(json.get("seed").and_then(Json::text).as_deref(), Some("cap-1"));
    assert_eq!(json.get("blocks").and_then(Json::num), Some(world.samples as f64 * world.step));
    assert_eq!(json.get("seaLevel").and_then(Json::num), Some(world.sea_level));
    let figures = preview::figures::Figures::from_json(json.get("figures").unwrap()).unwrap();
    assert_eq!(figures, world.measured.figures, "the figures come back whole");
    assert!(matches!(json.get("figuresBefore"), Some(Json::Null)), "no run before, no figures before");
    assert!(matches!(json.get("ores"), Some(Json::Null)), "no cut shown, no ores");
    let Some(Json::Obj(pack)) = json.get("pack") else { panic!("no pack") };
    let Some(Json::Obj(tectonic)) = json.get("tectonic") else { panic!("no tectonic") };
    assert_eq!((pack.len(), tectonic.len()), (43, 43), "every setting is named in both");
    let absent = json.get("biomesAbsent").and_then(Json::array).unwrap();
    assert_eq!(absent.len(), world.absent.len());

    let lists = json.get("lists").expect("lists");
    let biomes = lists.get("biomes").and_then(Json::array).expect("the biomes' rows");
    let shares: Vec<f64> = biomes.iter().map(|r| r.get("share").and_then(Json::num).unwrap()).collect();
    assert!(shares.windows(2).all(|p| p[0] >= p[1]), "the biomes come biggest first");
    assert!((shares.iter().sum::<f64>() - 100.0).abs() < 1e-6, "the shares are of the whole map: {}", shares.iter().sum::<f64>());
    assert!(lists.get("structures").is_none(), "structures are not exported yet");
    assert!(text.ends_with("}\n") && text.contains("\n  \"seed\": "), "laid out as the page's two-space JSON");

    // The notice's steps list ends with the data step, as the page's does.
    assert_eq!(rig.app.state().said, text::EXPORTED);
    println!("E5: {} bytes, {} biome rows", text.len(), biomes.len());
}