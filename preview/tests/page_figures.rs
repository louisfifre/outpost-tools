//! The figures, the lists and the history against the page's own code. `tools/page_figures.mjs` cuts measure(), the
//! lists, showFigures(), remember() and showHistory() out of preview.html and runs them with Node on the out folder;
//! ours must give the same numbers bit for bit and the same words letter for letter. Skipped (with a line saying
//! why) when Node, the page or the data is missing.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use preview::figures::{Cell, Figures, blocks, figure_cells, percent};
use preview::history::{self, Entry, FOLLOWED};
use preview::json::Json;
use preview::text;
use preview::paint::View;
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
            .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../world-preview")))
            .join("page")
            .join("preview.html"),
    }
}

/// The page's answers, or None (with the reason printed) when they cannot be had. Node runs once for all the
/// tests: they share its output folder.
fn page_answers() -> Option<&'static (World, Json)> {
    static ANSWERS: OnceLock<Option<(World, Json)>> = OnceLock::new();
    ANSWERS.get_or_init(ask_the_page).as_ref()
}

fn ask_the_page() -> Option<(World, Json)> {
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
    let dest = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("page_figures");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools").join("page_figures.mjs");
    let out = Command::new("node").arg(&script).arg(page()).arg(&data).arg(&dest).output().expect("node runs");
    println!("node: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "page_figures.mjs failed");
    let world = World::load(&data).expect("the data loads");
    let meta = Json::parse(&std::fs::read_to_string(dest.join("meta.json")).unwrap()).unwrap();
    Some((world, meta))
}

fn field<'a>(j: &'a Json, key: &str) -> &'a Json {
    j.get(key).unwrap_or_else(|| panic!("the page's answer has no \"{key}\""))
}

fn numbers(j: &Json) -> Vec<f64> {
    j.array().expect("an array").iter().map(|v| v.num().expect("a number")).collect()
}

fn texts(j: &Json) -> Vec<String> {
    j.array().expect("an array").iter().map(|v| v.text().expect("a text")).collect()
}

/// A number or null, as the page's JSON has them.
fn option(j: &Json) -> Option<f64> {
    j.num()
}

fn bits(v: f64) -> u64 {
    v.to_bits()
}

fn their_figures(meta: &Json) -> Figures {
    let f = field(meta, "figures");
    let plain = |k: &str| field(f, k).num().unwrap_or(f64::NAN);
    Figures {
        ocean: plain("ocean"),
        flat: option(field(f, "flat")),
        high: plain("high"),
        top: plain("top"),
        expanse: option(field(f, "expanse")),
        count: plain("count") as usize,
        total: option(field(f, "total")).map(|t| t as usize),
        typical: plain("typical"),
        span: plain("span"),
        step: plain("step"),
    }
}

#[test]
fn f1_the_figures_are_the_pages_to_the_bit() {
    let Some((world, meta)) = page_answers() else { return };
    let (ours, theirs) = (world.measured.figures, their_figures(&meta));
    let same = |name: &str, a: f64, b: f64| {
        assert_eq!(bits(a), bits(b), "{name}: ours {a}, the page's {b}");
        println!("figure {name:8} {a}");
    };
    same("ocean", ours.ocean, theirs.ocean);
    assert_eq!(ours.flat.map(bits), theirs.flat.map(bits), "flat");
    println!("figure flat     {:?}", ours.flat);
    same("high", ours.high, theirs.high);
    same("top", ours.top, theirs.top);
    assert_eq!(ours.expanse.map(bits), theirs.expanse.map(bits), "expanse");
    println!("figure expanse  {:?}", ours.expanse);
    assert_eq!(ours.count, theirs.count);
    println!("figure count    {}", ours.count);
    assert_eq!(ours.total, theirs.total);
    println!("figure total    {:?}", ours.total);
    same("typical", ours.typical, theirs.typical);
    same("span", ours.span, theirs.span);
    same("step", ours.step, theirs.step);
    assert_eq!(world.biome_total, ours.total, "the page's total is the figure's");
    println!("F1: the 10 figure fields equal");
}

#[test]
fn f2_the_lists_and_the_absent_biomes_are_the_pages() {
    let Some((world, meta)) = page_answers() else { return };
    let absent = texts(field(&meta, "absent"));
    assert_eq!(world.absent, absent, "absent ids");
    assert_eq!(world.measured.absent, absent);
    println!("F2: {} absent biomes equal", absent.len());

    let lists = field(&meta, "lists");
    let samples2 = (world.samples * world.samples) as f64;
    let mut strings = 0;
    for view in View::ALL {
        let kind = view.key();
        let theirs = field(lists, kind);
        let ours = world.measured.lists[view.index()].as_ref();
        if matches!(theirs, Json::Null) {
            assert!(ours.is_none(), "{kind}: the page has no list");
            continue;
        }
        let ours = ours.unwrap_or_else(|| panic!("{kind}: the page has a list, we have none"));
        assert_eq!(ours.ids, texts(field(theirs, "ids")), "{kind}: ids");
        assert_eq!(ours.names, texts(field(theirs, "names")), "{kind}: names");
        let colours: Vec<[u8; 3]> = field(theirs, "colours").array().unwrap().iter().map(|c| numbers(c).iter().map(|v| *v as u8).collect::<Vec<_>>().try_into().unwrap()).collect();
        assert_eq!(ours.colours, colours, "{kind}: colours");
        let (cells, widths) = (numbers(field(theirs, "cells")), numbers(field(theirs, "widths")));
        assert_eq!(ours.cells.iter().copied().map(bits).collect::<Vec<_>>(), cells.iter().copied().map(bits).collect::<Vec<_>>(), "{kind}: cells");
        assert_eq!(ours.widths.iter().copied().map(bits).collect::<Vec<_>>(), widths.iter().copied().map(bits).collect::<Vec<_>>(), "{kind}: widths");
        assert_eq!(ours.counted, field(theirs, "counted").boolean().unwrap(), "{kind}: counted");
        if let Some(total) = option(field(theirs, "total")) {
            assert_eq!(ours.total, total, "{kind}: total");
        }
        let (width_text, share_text) = (texts(field(theirs, "widthText")), texts(field(theirs, "shareText")));
        for r in 0..ours.ids.len() {
            assert_eq!(blocks(ours.widths[r]), width_text[r], "{kind} row {r}: width text");
            let share = if ours.counted { text::num(ours.cells[r]) } else { percent(100.0 * ours.cells[r] / samples2) };
            assert_eq!(share, share_text[r], "{kind} row {r}: share text");
            strings += 2;
        }
        println!("F2: list {kind}: {} rows, ids, names, colours, cells and widths bit-exact", ours.ids.len());
    }
    println!("F2: {strings} width and share strings equal");
}

fn their_cells(j: &Json) -> Vec<(String, String)> {
    j.array().unwrap().iter().map(|c| (c.array().unwrap()[0].text().unwrap(), c.array().unwrap()[1].text().unwrap())).collect()
}

fn ours(cells: &[Cell]) -> Vec<(String, String)> {
    cells.iter().map(|c| (c.text.clone(), c.kind.class())).collect()
}

#[test]
fn f3_the_figure_cells_are_the_pages() {
    let Some((world, meta)) = page_answers() else { return };
    let f = world.measured.figures;
    // The page's own test run: a run before that differs in every way a figure can.
    let b1 = Figures {
        ocean: f.ocean + 0.34,
        flat: f.flat.map(|v| v - 1.06),
        top: f.top - 3.0,
        typical: f.typical + 49.0,
        expanse: f.expanse.map(|v| v + 160.0),
        count: f.count - 2,
        ..f
    };
    let other_area = Figures { span: b1.span * 2.0, ..b1 };
    let cases = field(&meta, "showFigures").array().unwrap();
    assert_eq!(cases.len(), 3);
    for (name, before, theirs) in [("no run before", None, &cases[0]), ("a run before", Some(&b1), &cases[1]), ("another area", Some(&other_area), &cases[2])] {
        let mine = ours(&figure_cells(&f, before));
        let page = their_cells(theirs);
        assert_eq!(mine, page, "{name}");
        println!("F3: {name}: {} cells equal", mine.len());
        if before == Some(&b1) {
            for (text, class) in &mine {
                if !text.is_empty() {
                    println!("  [{class}] {text}");
                }
            }
            let changes: Vec<&String> = mine.iter().filter(|c| c.1.starts_with("before")).map(|c| &c.0).collect();
            for sign in ["=", "+", "\u{2212}", " pt"] {
                assert!(changes.iter().any(|c| c.contains(sign)), "no change form {sign:?} among {changes:?}");
            }
        }
    }
    assert_eq!(figure_cells(&f, None).len(), 22);
    assert_eq!(figure_cells(&f, Some(&b1)).len(), 25);
}

fn entries(j: &Json) -> Vec<Json> {
    j.array().expect("the stored history is a list").to_vec()
}

#[test]
fn f4_the_stored_history_is_read_and_written_as_the_pages() {
    let Some((_, meta)) = page_answers() else { return };
    let stored = field(field(&meta, "history"), "stored").text().expect("stored text");
    let parsed = history::parse(&stored).expect("the page's file reads");
    let json = Json::parse(&stored).unwrap();
    let items = entries(&json);
    assert_eq!(parsed.len(), 5);
    for (e, item) in parsed.iter().zip(&items) {
        assert_eq!(e.seed, field(item, "seed").text().unwrap());
        assert_eq!(e.span, field(item, "span").num().unwrap());
        assert_eq!(e.step, field(item, "step").num().unwrap());
        assert_eq!(e.at, field(item, "at").num().unwrap());
        let Json::Obj(values) = field(item, "values") else { panic!("values is an object") };
        let keys: Vec<&String> = e.values.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, values.iter().map(|(k, _)| k).collect::<Vec<_>>(), "the settings keep the file's order");
        for ((_, ours), (_, theirs)) in e.values.iter().zip(values) {
            assert_eq!(*ours, theirs.num().expect("a number"));
        }
        for (slot, key) in FOLLOWED.iter().enumerate() {
            assert_eq!(e.figures[slot].map(bits), option(field(field(item, "figures"), key)).map(bits), "{key}");
        }
    }
    let again = history::parse(&history::to_json(&parsed)).expect("what we write reads back");
    assert_eq!(again, parsed);
    println!("F4: {} stored entries parsed ({} settings each), written and read back equal", parsed.len(), parsed[0].values.len());
}

#[test]
fn f5_the_trend_is_the_pages() {
    let Some((world, meta)) = page_answers() else { return };
    let h = field(&meta, "history");
    let parsed: Vec<Entry> = history::parse(&field(h, "stored").text().unwrap()).unwrap();
    let f = world.measured.figures;
    let seed = parsed[0].seed.clone();
    let runs = history::here(&parsed, &seed, f.span, f.step);
    assert_eq!(runs.len(), 4, "four runs of the seed over this area");
    let mut paths = 0;
    for (name, pointed) in [("pointednull", None), ("pointed0", Some(0)), ("pointed2", Some(2))] {
        let theirs = field(h, name);
        let mine = history::trend(&runs, pointed, 330.0);
        let rows = field(theirs, "rows").array().unwrap();
        assert_eq!(mine.rows.len(), rows.len(), "{name}: rows");
        for (row, page) in mine.rows.iter().zip(rows) {
            let p = page.array().unwrap();
            assert_eq!(row.name, p[0].text().unwrap(), "{name}: label");
            assert_eq!(row.value, p[1].text().unwrap(), "{name} {}: value", row.key);
            assert_eq!(bits(row.now_x), bits(p[2].num().unwrap()), "{name} {}: now x", row.key);
            assert_eq!(row.d, p[3].text().unwrap(), "{name} {}: path", row.key);
            let dot = p[4].array().map(|d| (d[0].num().unwrap(), d[1].num().unwrap()));
            assert_eq!(row.dot.map(|(x, y)| (bits(x), bits(y))), dot.map(|(x, y)| (bits(x), bits(y))), "{name} {}: dot", row.key);
            paths += 1;
        }
        assert_eq!(mine.probe, field(theirs, "probe").text().unwrap(), "{name}: probe");
        println!("F5: {name}: {} rows, probe {:?}", rows.len(), mine.probe);
    }
    println!("F5: {paths} path strings equal in 3 cases");
}
