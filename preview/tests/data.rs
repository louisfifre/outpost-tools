//! The catalog, the settings table and the height bands, without a GPU.

use preview::bands::{HEIGHT_STEPS, SEA, SEA_DEPTHS, band_colours, band_names};
use preview::settings::{Kind, SETTINGS, grouped};
use preview::text::area_probe;

#[test]
fn d1_settings_are_grouped_as_the_page_groups_them() {
    let g = grouped();
    assert_eq!(g.iter().map(|v| v.len()).collect::<Vec<_>>(), [6, 12, 9, 11, 5]);
    let first: Vec<&str> = g.iter().map(|v| SETTINGS[v[0]].label).collect();
    assert_eq!(first, ["Temperature scale", "Vertical scale", "Continent scale", "Cheese caves", "Snow start"]);
    assert_eq!(SETTINGS[*g[1].last().unwrap()].label, "Tectonic");
    assert_eq!(SETTINGS.iter().filter(|s| matches!(s.kind, Kind::Switch)).count(), 17);
    let mut keys: Vec<&str> = SETTINGS.iter().map(|s| s.key).collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 43);
}

#[test]
fn d2_band_names() {
    assert_eq!(
        band_names(SEA, &HEIGHT_STEPS, &SEA_DEPTHS),
        [
            "Sea, deeper than 45",
            "Sea, 20 to 45 deep",
            "Sea, 0 to 20 deep",
            "63 to 100",
            "100 to 150",
            "150 to 200",
            "200 to 250",
            "Above 250"
        ]
    );
}

#[test]
fn d3_band_colours() {
    let want = [[23, 49, 79], [37, 78, 126], [48, 102, 165], [107, 163, 70], [170, 174, 76], [157, 125, 82], [148, 121, 107], [188, 188, 188]];
    assert_eq!(band_colours(SEA, &HEIGHT_STEPS, &SEA_DEPTHS), want);
}

#[test]
fn d4_area_probe() {
    assert_eq!(area_probe("4,096", "512", false), "4,096 \u{d7} 4,096 blocks, north up. One square: 512 blocks. Cross: 0,0.");
    assert!(area_probe("4,096", "512", true).ends_with(" Ticks: the cut."));
}

#[test]
fn d5_run_json_gives_the_ask_of_the_loaded_world() {
    use preview::run::ask_of;
    use preview::settings::pack_values;
    use preview::world::World;
    let dir = &preview::world::data_dir();
    if !dir.join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no out folder");
        return;
    }
    let world = World::load(dir).unwrap();
    assert_eq!(world.made.as_ref().expect("run.json is read").seed, "cap-1");
    let ask = ask_of(&world).unwrap();
    assert_eq!((ask.seed.as_str(), ask.samples, ask.detail), ("cap-1", 256, 16));
    assert_eq!(ask.values, pack_values());
}
