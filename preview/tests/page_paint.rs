//! paint() against the page's own code. `tools/page_paint.mjs` cuts the colours, the measuring and paint() out of
//! preview.html and runs them with Node on the out folder; our paint() must give the same bytes, exactly, for
//! the 12 cases. Skipped (with a line saying why) when Node, the page or the data is missing.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use preview::json::Json;
use preview::paint::{View, paint};
use preview::world::World;

fn data_dir() -> PathBuf {
    preview::world::data_dir()
}

fn page() -> PathBuf {
    match std::env::var_os("WORLD_PREVIEW_HTML") {
        Some(page) => PathBuf::from(page),
        // The world-preview repository beside this one, unless WORLD_PREVIEW_DIR names it.
        None => std::env::var_os("WORLD_PREVIEW_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../world-preview")))
            .join("page")
            .join("preview.html"),
    }
}

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

fn numbers(j: &Json) -> Vec<f64> {
    j.array().expect("an array").iter().map(|v| v.num().expect("a number")).collect()
}

#[test]
fn p1_paint_is_byte_identical_to_the_pages() {
    let data = data_dir();
    if !data.join("preview.json").exists() {
        println!("skipped: no {}", data.display());
        return;
    }
    if !page().exists() {
        println!("skipped: no {}", page().display());
        return;
    }
    if Command::new("node").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        println!("skipped: node is not on PATH");
        return;
    }
    let dest = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("page_paint");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools").join("page_paint.mjs");
    let out = Command::new("node").arg(&script).arg(page()).arg(&data).arg(&dest).output().expect("node runs");
    println!("node: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "page_paint.mjs failed");

    let world = World::load(&data).expect("the data loads");
    let meta = Json::parse(&std::fs::read_to_string(dest.join("meta.json")).unwrap()).unwrap();

    // The same numbers first: colours, structures, flat land.
    let theirs: Vec<[u8; 3]> = meta.get("biomeColours").unwrap().array().unwrap().iter().map(|c| numbers(c).iter().map(|v| *v as u8).collect::<Vec<_>>().try_into().unwrap()).collect();
    assert_eq!(world.colours, theirs, "biome colours");
    let s = world.structures.as_ref().expect("structures.json is there");
    let st = meta.get("structures").unwrap();
    let ids: Vec<String> = st.get("ids").unwrap().array().unwrap().iter().map(|v| v.text().unwrap()).collect();
    assert_eq!(s.ids, ids, "structure ids");
    let colours: Vec<[u8; 3]> = st.get("colours").unwrap().array().unwrap().iter().map(|c| numbers(c).iter().map(|v| *v as u8).collect::<Vec<_>>().try_into().unwrap()).collect();
    assert_eq!(s.colours, colours, "structure colours");
    let under: Vec<bool> = st.get("under").unwrap().array().unwrap().iter().map(|v| v.boolean().unwrap()).collect();
    assert_eq!(s.under, under, "structure under");
    let order: Vec<usize> = numbers(st.get("order").unwrap()).iter().map(|v| *v as usize).collect();
    assert_eq!(s.order, order, "structure order");
    let count: Vec<u32> = numbers(st.get("count").unwrap()).iter().map(|v| *v as u32).collect();
    assert_eq!(s.count, count, "structure count");
    let flat = meta.get("flat").unwrap().num().unwrap() as usize;
    assert_eq!(world.flat_at.iter().filter(|&&f| f == 1).count(), flat, "flat count");
    println!("colours, structures ({} kinds) and flat land ({flat} points) equal", s.ids.len());

    let cases: Vec<String> = meta.get("cases").unwrap().array().unwrap().iter().map(|v| v.text().unwrap()).collect();
    assert_eq!(cases.len(), 12, "twelve cases");
    for name in &cases {
        let (view, dark, chosen) = match name.as_str() {
            "biomes_chosen_dark" => (View::Biomes, true, set(&["minecraft:badlands", "minecraft:savanna"])),
            "height_chosen_light" => (View::Height, false, set(&["3", "4"])),
            other => {
                let (key, scheme) = other.rsplit_once('_').unwrap();
                (View::from_key(key).unwrap_or_else(|| panic!("view {key}")), scheme == "dark", BTreeSet::new())
            }
        };
        let page = std::fs::read(dest.join(format!("{name}.rgba"))).unwrap();
        let ours = paint(&world, view, dark, &chosen);
        assert_eq!(ours.len(), page.len(), "{name}: length");
        let differing: Vec<usize> = (0..ours.len() / 4).filter(|&i| ours[i * 4..i * 4 + 4] != page[i * 4..i * 4 + 4]).collect();
        let first: Vec<String> =
            differing.iter().take(5).map(|&i| format!("#{i}: ours {:?}, page's {:?}", &ours[i * 4..i * 4 + 4], &page[i * 4..i * 4 + 4])).collect();
        assert!(differing.is_empty(), "{name}: {} pixels differ; first: {first:?}", differing.len());
        println!("{name}: {} pixels identical", ours.len() / 4);
    }
}
