//! The history of the figures (preview.html 1540-1637): one entry per map that came to the screen, kept in a file
//! of the page's own JSON shape (its localStorage `preview.history`), and the lines drawn from them, worked out
//! here as numbers and path text so that a test can hold them against the page's.

use std::io;
use std::path::Path;

use mere_base::observe;

use crate::figures::{Figures, KEYS, blocks, json_number, percent, to_fixed1};
use crate::json::Json;
use crate::run::Ask;
use crate::settings::{SETTINGS, same_value};
use crate::text;

/// The figures followed run after run (line 494): those of the goal, then the two that pay for them.
pub const FOLLOWED: [&str; 5] = ["flat", "expanse", "typical", "ocean", "high"];

/// One map that came to the screen. `values` keep the order they were written in; a value that was not a number
/// (null in the file) is NaN, which equals nothing, itself included.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Milliseconds since 1970 (`Date.now()`).
    pub at: f64,
    pub seed: String,
    pub span: f64,
    pub step: f64,
    pub values: Vec<(String, f64)>,
    /// In `FOLLOWED` order; None where the map's detail could not give the figure.
    pub figures: [Option<f64>; 5],
}

/// The entry of a map made from `made`, with its figures (line 1550). The settings go in `SETTINGS` order.
pub fn entry(made: &Ask, f: &Figures, at_ms: f64) -> Entry {
    Entry {
        at: at_ms,
        seed: made.seed.clone(),
        span: f.span,
        step: f.step,
        values: SETTINGS.iter().zip(made.values).map(|(s, v)| (s.key.to_string(), v)).collect(),
        figures: FOLLOWED.map(|k| f.get(k)),
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn values_json(values: &[(String, f64)]) -> String {
    let fields: Vec<String> = values.iter().map(|(k, v)| format!("{}:{}", json_string(k), json_number(Some(*v)))).collect();
    format!("{{{}}}", fields.join(","))
}

/// What two entries must share to be the same map (line 1551): `JSON.stringify([seed, span, step, values])`.
pub fn key(e: &Entry) -> String {
    format!("[{},{},{},{}]", json_string(&e.seed), json_number(Some(e.span)), json_number(Some(e.step)), values_json(&e.values))
}

/// A map is kept once: a reload that finds the same seed, area, detail and settings adds nothing (line 1552).
/// Returns whether the entry was kept.
pub fn remember(history: &mut Vec<Entry>, e: Entry) -> bool {
    if history.last().is_some_and(|last| key(last) == key(&e)) {
        return false;
    }
    history.push(e);
    true
}

/// The runs a line follows (`runsHere`, 1563): this seed, over this area, at this detail.
pub fn here<'a>(history: &'a [Entry], seed: &str, span: f64, step: f64) -> Vec<&'a Entry> {
    history.iter().filter(|e| e.seed == seed && e.span == span && e.step == step).collect()
}

/// The page's shape (line 1550), in the page's key order.
pub fn to_json(history: &[Entry]) -> String {
    let items: Vec<String> = history
        .iter()
        .map(|e| {
            let figures: Vec<String> = FOLLOWED.iter().zip(&e.figures).map(|(k, v)| format!("\"{k}\":{}", json_number(*v))).collect();
            format!(
                "{{\"at\":{},\"seed\":{},\"span\":{},\"step\":{},\"values\":{},\"figures\":{{{}}}}}",
                json_number(Some(e.at)),
                json_string(&e.seed),
                json_number(Some(e.span)),
                json_number(Some(e.step)),
                values_json(&e.values),
                figures.join(",")
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// Reads what the page wrote, and what `to_json` writes. A missing figure or a null reads None, a null value
/// reads NaN, and the settings keep the file's order.
pub fn parse(text: &str) -> Result<Vec<Entry>, String> {
    let json = Json::parse(text)?;
    let list = match &json {
        Json::Null => return Ok(Vec::new()),
        Json::Arr(list) => list,
        _ => return Err("the history is not a list".to_string()),
    };
    let mut out = Vec::new();
    for (i, item) in list.iter().enumerate() {
        let number = |k: &str| item.get(k).and_then(Json::num).ok_or_else(|| format!("entry {i} has no number \"{k}\""));
        let Some(Json::Obj(values)) = item.get("values") else { return Err(format!("entry {i} has no \"values\"")) };
        let mut figures = [None; 5];
        if let Some(f) = item.get("figures") {
            for (slot, key) in figures.iter_mut().zip(FOLLOWED) {
                *slot = f.get(key).and_then(Json::num);
            }
        }
        out.push(Entry {
            at: item.get("at").and_then(Json::num).unwrap_or(0.0),
            seed: item.get("seed").and_then(Json::text).ok_or_else(|| format!("entry {i} has no seed"))?,
            span: number("span")?,
            step: number("step")?,
            values: values.iter().map(|(k, v)| (k.clone(), v.num().unwrap_or(f64::NAN))).collect(),
            figures,
        });
    }
    Ok(out)
}

/// The kept history: none when the file is missing, none (and a warning) when it cannot be read.
pub fn load(path: &Path) -> Vec<Entry> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    match String::from_utf8(bytes).map_err(|_| "not UTF-8".to_string()).and_then(|t| parse(&t)) {
        Ok(history) => history,
        Err(e) => {
            observe!(Warn, "the history {} could not be read: {e}", path.display());
            Vec::new()
        }
    }
}

pub fn save(path: &Path, history: &[Entry]) -> io::Result<()> {
    std::fs::write(path, to_json(history))
}

/// What a run changed from the one before it (`changes`, 1617-1624): the settings by their names.
pub fn changes(was: Option<&Entry>, now: &Entry) -> String {
    let Some(was) = was else { return text::HISTORY_FIRST.to_string() };
    let moved: Vec<(&str, f64, f64)> = now
        .values
        .iter()
        .filter_map(|(k, v)| was.values.iter().find(|(wk, _)| wk == k).filter(|(_, wv)| !same_value(*wv, *v)).map(|(_, wv)| (k.as_str(), *wv, *v)))
        .collect();
    if moved.len() > 2 {
        return text::history_many(moved.len());
    }
    let said: Vec<String> = moved
        .iter()
        .map(|(k, from, to)| text::history_moved(SETTINGS.iter().find(|s| s.key == *k).map_or(*k, |s| s.label), *from, *to))
        .collect();
    if said.is_empty() { text::HISTORY_SAME.to_string() } else { said.join(", ") }
}

/// One line of the history: a figure over the runs.
#[derive(Clone, Debug, PartialEq)]
pub struct TrendRow {
    pub key: &'static str,
    pub name: &'static str,
    /// The figure of the run pointed at, formatted.
    pub value: String,
    /// Where the pointed run stands along the line.
    pub now_x: f64,
    /// The path as the page writes it, points to one decimal.
    pub d: String,
    /// The same points as numbers, one list per stretch the line is not broken in (a run whose detail could not
    /// give the figure leaves a gap).
    pub lines: Vec<Vec<(f64, f64)>>,
    pub dot: Option<(f64, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Trend {
    pub rows: Vec<TrendRow>,
    pub probe: String,
}

/// The height of a line's box (line 313).
pub const PLOT_H: f64 = mere_ui::PLOT_H as f64;

/// `showHistory` (1569-1615) as data, for the runs of `here`. `pointed` is the run the pointer is over.
pub fn trend(runs: &[&Entry], pointed: Option<usize>, width: f64) -> Trend {
    if runs.len() < 2 {
        return Trend { rows: Vec::new(), probe: text::HISTORY_FEW.to_string() };
    }
    let len = runs.len();
    let at = match pointed {
        Some(p) if p < len => p,
        _ => len - 1,
    };
    let x = |i: usize| i as f64 / (len - 1) as f64 * width;
    let mut rows = Vec::new();
    for (slot, key) in FOLLOWED.iter().enumerate() {
        let values: Vec<Option<f64>> = runs.iter().map(|e| e.figures[slot]).collect();
        let known: Vec<f64> = values.iter().flatten().copied().collect();
        if known.is_empty() {
            continue;
        }
        let mut low = known.iter().copied().fold(f64::INFINITY, f64::min);
        let mut high = known.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        // A figure that did not move lies across the middle, not along an edge.
        if high - low < 1e-9 {
            low -= 1.0;
            high += 1.0;
        }
        let y = |v: f64| 2.0 + (1.0 - (v - low) / (high - low)) * (PLOT_H - 4.0);
        let name = text::FIGURE_NAMES[KEYS.iter().position(|k| k == key).expect("a followed figure is a figure")];
        let value = match values[at] {
            None => "\u{2014}".to_string(),
            Some(v) if matches!(*key, "typical" | "expanse") => blocks(v),
            Some(v) => percent(v),
        };
        let (mut d, mut pen) = (String::new(), 'M');
        let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
        let mut open = false;
        for (i, v) in values.iter().enumerate() {
            let Some(v) = v else {
                pen = 'M';
                open = false;
                continue;
            };
            let (px, py) = (to_fixed1(x(i)), to_fixed1(y(*v)));
            d.push_str(&format!("{pen}{px},{py}"));
            pen = 'L';
            if !open {
                lines.push(Vec::new());
                open = true;
            }
            lines.last_mut().expect("a line was opened").push((px.parse().expect("a fixed number"), py.parse().expect("a fixed number")));
        }
        rows.push(TrendRow { key, name, value, now_x: x(at), d, lines, dot: values[at].map(|v| (x(at), y(v))) });
    }
    let probe = match pointed {
        None => text::history_hint(len),
        Some(_) => text::history_run(at + 1, len, &changes(at.checked_sub(1).map(|i| runs[i]), runs[at])),
    };
    Trend { rows, probe }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seed: &str, step: f64, tweak: f64, flat: Option<f64>) -> Entry {
        Entry {
            at: 1.0,
            seed: seed.to_string(),
            span: 4096.0,
            step,
            values: vec![("ridge_scale".to_string(), 0.1), ("erosion_scale".to_string(), tweak)],
            figures: [flat, Some(1250.0), Some(900.0), Some(40.0), Some(6.0)],
        }
    }

    #[test]
    fn a_map_is_kept_once() {
        let mut h = Vec::new();
        assert!(remember(&mut h, run("7", 16.0, 0.1, Some(38.0))));
        assert!(!remember(&mut h, run("7", 16.0, 0.1, Some(39.0))), "same seed, area, detail and settings");
        assert!(remember(&mut h, run("7", 16.0, 0.2, Some(38.0))));
        assert!(remember(&mut h, run("7", 16.0, 0.1, Some(38.0))), "only the last entry is compared");
        assert_eq!(h.len(), 3);
        assert_eq!(here(&h, "7", 4096.0, 16.0).len(), 3);
        assert_eq!(here(&h, "8", 4096.0, 16.0).len(), 0);
        assert_eq!(here(&h, "7", 4096.0, 32.0).len(), 0);
    }

    #[test]
    fn the_file_is_the_pages_and_reads_back() {
        let h = vec![run("7", 16.0, 0.1, Some(38.5)), run("7", 16.0, 0.2, None)];
        let text = to_json(&h);
        assert!(text.starts_with("[{\"at\":1,\"seed\":\"7\",\"span\":4096,\"step\":16,\"values\":{\"ridge_scale\":0.1,\"erosion_scale\":0.1},\"figures\":{\"flat\":38.5,"));
        assert!(text.contains("\"flat\":null"));
        assert_eq!(parse(&text).unwrap(), h);
        // The page's own file may lack a figure, hold a null setting and a numeric seed.
        let page = parse("[{\"at\":5,\"seed\":94427382,\"span\":4096,\"step\":16,\"values\":{\"a\":null,\"b\":2},\"figures\":{\"flat\":null,\"ocean\":1.5}}]").unwrap();
        assert_eq!(page[0].seed, "94427382");
        assert!(page[0].values[0].1.is_nan());
        assert_eq!(page[0].figures, [None, None, None, Some(1.5), None]);
        assert_eq!(parse("null").unwrap(), Vec::new());
        assert!(parse("{}").is_err());
    }

    #[test]
    fn a_gap_breaks_the_line_in_two() {
        let mut runs = vec![run("7", 16.0, 0.1, Some(30.0)), run("7", 16.0, 0.2, None), run("7", 16.0, 0.3, Some(40.0))];
        runs.push(run("7", 16.0, 0.4, Some(35.0)));
        let refs: Vec<&Entry> = runs.iter().collect();
        let t = trend(&refs, None, 330.0);
        let flat = &t.rows[0];
        assert_eq!(flat.d.matches('M').count(), 2, "{}", flat.d);
        assert_eq!(flat.lines.len(), 2);
        assert_eq!(flat.lines[0].len(), 1);
        assert_eq!(flat.lines[1].len(), 2);
        // 30 is the lowest: the bottom of the box, 2 + (PLOT_H - 4); 40 the highest: the top, 2; 35 halfway.
        let (bottom, middle) = (PLOT_H - 2.0, PLOT_H / 2.0);
        assert_eq!(flat.d, format!("M0.0,{bottom:.1}M220.0,2.0L330.0,{middle:.1}"));
        assert_eq!(flat.dot, Some((330.0, middle)));
        assert_eq!(t.probe, text::history_hint(4));
    }

    #[test]
    fn a_figure_that_did_not_move_lies_across_the_middle() {
        let runs = [run("7", 16.0, 0.1, Some(30.0)), run("7", 16.0, 0.2, Some(30.0))];
        let refs: Vec<&Entry> = runs.iter().collect();
        let t = trend(&refs, Some(0), 100.0);
        let middle = PLOT_H / 2.0;
        assert!(t.rows[0].d.starts_with(&format!("M0.0,{middle:.1}L100.0,{middle:.1}")), "{}", t.rows[0].d);
        assert_eq!(t.rows[0].value, "30.0%");
        assert_eq!(t.rows[1].value, "1,250 blocks");
        assert_eq!(t.probe, "Run 1 of 2: the first one kept.");
        assert_eq!(trend(&refs[..1], None, 100.0).probe, text::HISTORY_FEW);
        // A pointer past the end is the last run.
        assert_eq!(trend(&refs, Some(9), 100.0).probe, "Run 2 of 2: Erosion scale 0.1 \u{2192} 0.2.");
    }

    #[test]
    fn changes_name_what_moved() {
        let a = run("7", 16.0, 0.1, Some(30.0));
        assert_eq!(changes(None, &a), "the first one kept");
        assert_eq!(changes(Some(&a), &a), "same settings");
        let mut b = a.clone();
        b.values[1].1 = 0.2;
        assert_eq!(changes(Some(&a), &b), "Erosion scale 0.1 \u{2192} 0.2");
        b.values[0].1 = 0.3;
        assert_eq!(changes(Some(&a), &b), "Ridge scale 0.1 \u{2192} 0.3, Erosion scale 0.1 \u{2192} 0.2");
        b.values.push(("vertical_scale".to_string(), 2.0));
        let mut c = a.clone();
        c.values.push(("vertical_scale".to_string(), 1.5));
        assert_eq!(changes(Some(&c), &b), "3 settings changed");
        // A key the run before did not have is not a change; an unknown key is named as it is.
        assert_eq!(changes(Some(&a), &b), "Ridge scale 0.1 \u{2192} 0.3, Erosion scale 0.1 \u{2192} 0.2");
        let mut d = a.clone();
        d.values[0] = ("mystery".to_string(), 1.0);
        let mut e = d.clone();
        e.values[0].1 = 2.0;
        assert_eq!(changes(Some(&d), &e), "mystery 1 \u{2192} 2");
    }
}
