//! The section against the page's own code. `tools/page_section.mjs` cuts the ore rules, `groundOf`, `orePlay`, the
//! cut's look and its drawing and the strip out of preview.html and runs them with Node on the out folder; the port
//! must give the same numbers and the same bytes. Skipped (with a line saying why) when Node, the page or the data
//! is missing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use preview::cut::{self, CutLook};
use preview::json::Json;
use preview::section::{ORE_KINDS, Shown, ground_of};
use preview::text;
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

/// Runs the script once for the whole file; None (and a line) when something is missing.
fn fixture() -> Option<&'static (PathBuf, World, Json)> {
    static F: OnceLock<Option<(PathBuf, World, Json)>> = OnceLock::new();
    F.get_or_init(|| {
        let data = data_dir();
        if !data.join("preview.json").exists() {
            println!("skipped: no {}", data.display());
            return None;
        }
        if !page().exists() {
            println!("skipped: no {}", page().display());
            return None;
        }
        if Command::new("node").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
            println!("skipped: node is not on PATH");
            return None;
        }
        let dest = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("page_section");
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools").join("page_section.mjs");
        let out = Command::new("node").arg(&script).arg(page()).arg(&data).arg(&dest).output().expect("node runs");
        println!("node: {}", String::from_utf8_lossy(&out.stdout));
        assert!(out.status.success(), "page_section.mjs failed: {}", String::from_utf8_lossy(&out.stderr));
        let world = World::load(&data).expect("the data loads");
        assert!(world.section.is_some(), "the data has a section");
        let meta = Json::parse(&std::fs::read_to_string(dest.join("meta.json")).unwrap()).unwrap();
        Some((dest, world, meta))
    })
    .as_ref()
}

fn f(j: &Json, key: &str) -> f64 {
    j.get(key).unwrap_or_else(|| panic!("no {key}")).num().unwrap_or_else(|| panic!("{key} is not a number"))
}

fn opt(j: Option<&Json>) -> Option<f64> {
    j.and_then(Json::num)
}

fn f64s(path: &Path) -> Vec<f64> {
    std::fs::read(path).unwrap().chunks_exact(8).map(|b| f64::from_le_bytes(b.try_into().unwrap())).collect()
}

fn kind_index(name: &str) -> usize {
    ORE_KINDS.iter().position(|k| *k == name).unwrap_or_else(|| panic!("kind {name}"))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}

#[test]
fn s1_rules_rows_and_the_unplayed_count_equal() {
    let Some((_, world, meta)) = fixture() else { return };
    let section = world.section.as_ref().unwrap();
    let rules = meta.get("rules").unwrap().array().unwrap();
    assert_eq!(section.rules.len(), rules.len(), "rules");
    for (ours, theirs) in section.rules.iter().zip(rules) {
        assert_eq!(Some(ours.id.clone()), theirs.get("id").and_then(Json::text));
        assert_eq!(ours.index as f64, f(theirs, "index"));
        assert_eq!(ours.kind, kind_index(&theirs.get("kind").unwrap().text().unwrap()), "{}", ours.id);
        assert_eq!(Some(ours.playable), theirs.get("playable").and_then(Json::boolean), "{} playable", ours.id);
        let a = theirs.get("anchored").unwrap();
        match (ours.anchored, a) {
            (None, Json::Null) => {}
            (Some(x), a) => assert_eq!((x.count, x.from, x.to), (f(a, "count"), f(a, "from"), f(a, "to")), "{} anchored", ours.id),
            (None, other) => panic!("{}: the page has {other:?}", ours.id),
        }
    }
    let rows = meta.get("rows").unwrap().array().unwrap();
    let ours = section.rows_of_mods();
    assert_eq!(ours.len(), rows.len(), "rows of mods");
    for (o, t) in ours.iter().zip(rows) {
        assert_eq!(Some(o.id.clone()), t.get("id").and_then(Json::text));
        assert_eq!(o.kind, kind_index(&t.get("kind").unwrap().text().unwrap()));
        assert_eq!((o.count, o.from, o.to), (f(t, "count"), f(t, "from"), f(t, "to")));
    }
    assert_eq!(section.unplayed() as f64, f(meta, "unplayed"));
    let playable = section.rules.iter().filter(|r| r.playable).count();
    println!("S1: {} rules ({playable} playable, {} unplayed), {} rows of mods: all equal", rules.len(), section.unplayed(), rows.len());
}

#[test]
fn s2_the_ground_of_the_four_cuts_equals() {
    let Some((dest, world, meta)) = fixture() else { return };
    let section = world.section.as_ref().unwrap();
    let grounds = meta.get("ground").unwrap().array().unwrap();
    assert_eq!(grounds.len(), section.cuts.len());
    for (i, theirs) in grounds.iter().enumerate() {
        let cut = &section.cuts[i];
        let g = ground_of(world, section, cut.cells.as_ref().unwrap(), cut);
        assert_eq!(g.surface, f64s(&dest.join(format!("ground_{i}_surface.f64"))), "cut {i} surface");
        assert_eq!(g.floor, f64s(&dest.join(format!("ground_{i}_floor.f64"))), "cut {i} floor");
        assert_eq!(g.chunks as f64, f(theirs, "chunks"));
        let high: Vec<u8> = theirs.get("high").unwrap().array().unwrap().iter().map(|v| v.num().unwrap() as u8).collect();
        assert_eq!(g.high, high, "cut {i} high");
        let biomes: Vec<Option<String>> = theirs.get("biomes").unwrap().array().unwrap().iter().map(Json::text).collect();
        assert_eq!(g.biomes, biomes, "cut {i} biomes");
        assert_eq!((g.top, g.bottom), (f(theirs, "top"), f(theirs, "bottom")), "cut {i} top, bottom");
        assert_eq!((g.peak as f64, g.crop as f64, g.rows as f64), (f(theirs, "peak"), f(theirs, "crop"), f(theirs, "rows")), "cut {i} peak, crop, rows");
        println!("S2: cut {i}: {} columns, {} chunks ({} high), top {}, bottom {}, peak {}, crop {}, rows {}: equal", g.surface.len(), g.chunks, g.high.iter().filter(|&&h| h == 1).count(), g.top, g.bottom, g.peak, g.crop, g.rows);
    }
}

#[test]
fn s3_the_play_of_cuts_0_and_2_equals() {
    let Some((dest, world, meta)) = fixture() else { return };
    let section = world.section.as_ref().unwrap();
    for at in [0usize, 2] {
        let mut shown = Shown::new(world, at).expect("the cut is there");
        let theirs = meta.get("play").unwrap().get(&at.to_string()).unwrap();
        assert_eq!(shown.play.counted as f64, f(theirs, "counted"), "cut {at} counted");
        let per = theirs.get("perChunk").unwrap();
        for (kind, name) in ORE_KINDS.iter().enumerate() {
            match (&shown.play.per_chunk[kind], per.get(name)) {
                (None, None) => {}
                (Some(o), Some(t)) => {
                    assert_eq!(o.low, opt(t.get("low")), "cut {at} {name} low");
                    assert_eq!(o.high, opt(t.get("high")), "cut {at} {name} high");
                    let from: Vec<String> = t.get("from").unwrap().array().unwrap().iter().map(|v| v.text().unwrap()).collect();
                    assert_eq!(o.from, from, "cut {at} {name} from");
                    println!("S3: cut {at} {name:9} low {:?} high {:?} from {} plays", o.low, o.high, o.from.len());
                }
                (o, t) => panic!("cut {at} {name}: ours {o:?}, the page's {t:?}"),
            }
        }
        shown.draw_chunks(world, 0, shown.ground.chunks as i64 - 1);
        let layer = std::fs::read(dest.join(format!("layer_{at}.u8"))).unwrap();
        assert_eq!(shown.play.layer.len(), layer.len());
        let differing: Vec<usize> = (0..layer.len()).filter(|&i| shown.play.layer[i] != layer[i]).collect();
        let ores = layer.iter().filter(|&&c| c != 0).count();
        let l = section.length;
        let first: Vec<String> = differing
            .iter()
            .take(5)
            .map(|&i| format!("cell x {} y {} chunk {}: ours {}, the page's {}", i % l, section.top_y() - (i / l) as f64, (i % l) >> 4, shown.play.layer[i], layer[i]))
            .collect();
        assert!(differing.is_empty(), "cut {at}: {} layer cells differ; first: {first:?}", differing.len());
        assert!(ores > 0, "the layer holds no ore");
        println!("S3: cut {at}: counted {}, layer of {} cells byte-identical, {ores} ore cells", shown.play.counted, layer.len());
    }
}

#[test]
fn s4_the_strip_equals() {
    let Some((dest, world, meta)) = fixture() else { return };
    let section = world.section.as_ref().unwrap();
    let a = meta.get("cases").unwrap().get("A").unwrap();
    let cut = &section.cuts[0];
    let g = ground_of(world, section, cut.cells.as_ref().unwrap(), cut);
    let (rgba, range) = cut::nav_image(section, &g, 638);
    let theirs = std::fs::read(dest.join("nav.rgba")).unwrap();
    assert_eq!(rgba.len(), theirs.len());
    let differing = (0..rgba.len() / 4).filter(|&i| rgba[i * 4..i * 4 + 4] != theirs[i * 4..i * 4 + 4]).count();
    assert_eq!(differing, 0, "{differing} strip pixels differ");
    let nav = a.get("nav").unwrap();
    assert_eq!(range, (f(nav.get("range").unwrap(), "top"), f(nav.get("range").unwrap(), "bottom")));
    let note = text::times(cut::nav_times(section.length, 638.0, range));
    assert_eq!(Some(note.clone()), nav.get("text").and_then(Json::text));
    println!("S4: the strip is {} pixels identical, range {range:?}, note \"{}\"", rgba.len() / 4, text::nav_note(&note));
}

#[test]
fn s5_the_pictures_of_the_cut_equal() {
    let Some((dest, world, meta)) = fixture() else { return };
    let section = world.section.as_ref().unwrap();
    for (name, start) in [("A", CutLook::FRESH), ("B", CutLook { k: 0.6, x: -1000.0, y: 0.0 }), ("C", CutLook { k: 4.0, x: -3000.0, y: -500.0 })] {
        let theirs = meta.get("cases").unwrap().get(name).unwrap();
        let at = f(theirs, "cut") as usize;
        let size = theirs.get("box").unwrap().array().unwrap();
        let (bw, bh) = (size[0].num().unwrap(), size[1].num().unwrap());
        let picked: BTreeSet<String> = theirs.get("picked").unwrap().array().unwrap().iter().map(|v| v.text().unwrap()).collect();
        let mut shown = Shown::new(world, at).unwrap();
        let cut = &section.cuts[at];

        let look = cut::settled(start, bw, bh, &shown.ground, section.length);
        let want = theirs.get("look").unwrap();
        assert!(near(look.k, f(want, "k")) && near(look.x, f(want, "x")) && near(look.y, f(want, "y")), "case {name}: look {look:?}, the page's {want:?}");
        assert_eq!(cut::settled(look, bw, bh, &shown.ground, section.length), look, "settling twice");

        let s = cut::sight(look, bw, bh, section.length, shown.ground.rows).expect("something is in sight");
        shown.draw_chunks(world, (s.x0 >> 4) as i64 - 1, (s.x1 >> 4) as i64 + 1);
        let sheet = cut::sheet(section, cut.cells.as_ref().unwrap(), &shown.ground, &shown.play.layer, s, look.k, &picked);
        let size = theirs.get("size").unwrap().array().unwrap();
        assert_eq!((s.w() as f64, s.h() as f64), (size[0].num().unwrap(), size[1].num().unwrap()), "case {name}: size");
        let page = std::fs::read(dest.join(format!("sheet_{name}.rgba"))).unwrap();
        assert_eq!(sheet.len(), page.len());
        let differing = (0..sheet.len() / 4).filter(|&i| sheet[i * 4..i * 4 + 4] != page[i * 4..i * 4 + 4]).count();
        assert_eq!(differing, 0, "case {name}: {differing} sheet texels differ");
        let ores = (0..sheet.len() / 4).filter(|&i| ![[128, 128, 128, 255], [80, 80, 86, 255], [47, 92, 158, 255], [0, 0, 0, 0]].contains(&[sheet[i * 4], sheet[i * 4 + 1], sheet[i * 4 + 2], sheet[i * 4 + 3]])).count();

        let args = cut::sheet_args(look, s);
        let image = theirs.get("image").unwrap().array().unwrap();
        for (i, a) in args.iter().enumerate() {
            assert!(near(*a, image[i].num().unwrap()), "case {name}: drawImage argument {i}: {a} against {}", image[i].num().unwrap());
        }

        let range = cut::nav_image(section, &shown.ground, 638).1;
        let frame = cut::nav_frame(look, bw, bh, section.length, &shown.ground, section, range, 638.0);
        let style = theirs.get("frame").unwrap();
        for (key, v) in [("left", frame.left), ("width", frame.width), ("top", frame.top), ("bottom", frame.bottom)] {
            assert!(near(v, f(style, key)), "case {name}: frame {key}: {v} against {}", f(style, key));
        }
        println!(
            "S5: case {name} (cut {at}, {bw}x{bh}, k {:.4}): look and drawImage equal, {} texels identical ({ores} not terrain), frame {frame:?}",
            look.k,
            sheet.len() / 4
        );
        if name == "B" {
            assert!(look.k < 0.75, "case B exercises the two-block ores");
            assert!(!picked.is_empty());
        }
    }
}
