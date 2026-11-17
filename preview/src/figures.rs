//! The page's `measure` (preview.html 1255-1322) and the lists it feeds (`listOf`, `structureList`, `bands`,
//! 1326-1370), and the cells `showFigures` makes of the figures (1498-1538). Numbers are f64 and accumulate in
//! the page's order, so that every digit the page shows is the one shown here: `tests/page_figures.rs` runs the
//! page's own code with Node and compares bit for bit.

use crate::bands::{HEIGHT_STEPS, SEA_DEPTHS, band_colours, band_names};
use crate::colour::{self, flat_colour, flat_sea_colour, steep_colour, temperature_colour};
use crate::json::Json;
use crate::paint::{View, rows};
use crate::text;
use crate::world::World;

/// Sizes of the zones made of touching points that share a value (`zones`, 1235-1253): scan order, a flood fill
/// over the four neighbours with an explicit stack. A zone's size does not depend on the order, but the sums
/// that follow do, in floating point, so the page's order is kept.
pub fn zones<T: Copy + PartialEq>(value_at: &[T], n: usize, mut each: impl FnMut(T, usize)) {
    let total = n * n;
    let mut seen = vec![false; total];
    let mut stack = vec![0usize; total];
    for start in 0..total {
        if seen[start] {
            continue;
        }
        let value = value_at[start];
        let (mut size, mut top) = (0usize, 0usize);
        stack[top] = start;
        top += 1;
        seen[start] = true;
        while top > 0 {
            top -= 1;
            let i = stack[top];
            size += 1;
            let x = i % n;
            let mut push = |j: usize, stack: &mut Vec<usize>, top: &mut usize| {
                if !seen[j] && value_at[j] == value {
                    seen[j] = true;
                    stack[*top] = j;
                    *top += 1;
                }
            };
            if x > 0 {
                push(i - 1, &mut stack, &mut top);
            }
            if x < n - 1 {
                push(i + 1, &mut stack, &mut top);
            }
            if i >= n {
                push(i - n, &mut stack, &mut top);
            }
            if i < total - n {
                push(i + n, &mut stack, &mut top);
            }
        }
        each(value, size);
    }
}

/// `world.figures` (1310-1320). A figure the map's detail cannot give is None; one that is not a number
/// (an area with no land has no typical biome) is read as missing wherever it is asked for by key.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Figures {
    pub ocean: f64,
    pub flat: Option<f64>,
    pub high: f64,
    pub top: f64,
    pub expanse: Option<f64>,
    pub count: usize,
    pub total: Option<usize>,
    pub typical: f64,
    pub span: f64,
    pub step: f64,
}

impl Figures {
    /// `f[key]` for the keys the figures and the history name: None where the page has null (or, here, NaN).
    pub fn get(&self, key: &str) -> Option<f64> {
        let v = match key {
            "flat" => self.flat,
            "expanse" => self.expanse,
            "typical" => Some(self.typical),
            "ocean" => Some(self.ocean),
            "high" => Some(self.high),
            "top" => Some(self.top),
            "count" => Some(self.count as f64),
            _ => None,
        };
        v.filter(|v| v.is_finite())
    }

    /// `JSON.stringify(figures)`: the page's key order, `null` for a missing or non-finite number.
    pub fn to_json(&self) -> String {
        let n = |v: f64| json_number(Some(v));
        format!(
            "{{\"ocean\":{},\"flat\":{},\"high\":{},\"top\":{},\"expanse\":{},\"count\":{},\"total\":{},\"typical\":{},\"span\":{},\"step\":{}}}",
            n(self.ocean),
            json_number(self.flat),
            n(self.high),
            n(self.top),
            json_number(self.expanse),
            self.count,
            self.total.map_or("null".to_string(), |t| t.to_string()),
            n(self.typical),
            n(self.span),
            n(self.step)
        )
    }

    /// The figures of `preview.before`. A missing step reads 16 (line 1499: `before.step ?? 16`); a null in
    /// a plain number reads NaN, which compares as unequal and shows no change.
    pub fn from_json(j: &Json) -> Option<Figures> {
        let Json::Obj(_) = j else { return None };
        let plain = |k: &str| j.get(k).and_then(Json::num).unwrap_or(f64::NAN);
        let option = |k: &str| j.get(k).and_then(Json::num);
        Some(Figures {
            ocean: plain("ocean"),
            flat: option("flat"),
            high: plain("high"),
            top: plain("top"),
            expanse: option("expanse"),
            count: option("count").map_or(0, |c| c as usize),
            total: option("total").map(|t| t as usize),
            typical: plain("typical"),
            span: plain("span"),
            step: option("step").unwrap_or(16.0),
        })
    }
}

/// What JSON.stringify writes for a number: nothing finite is `null`, and -0 is 0.
pub fn json_number(v: Option<f64>) -> String {
    match v {
        Some(v) if v.is_finite() => if v == 0.0 { "0".to_string() } else { format!("{v}") },
        _ => "null".to_string(),
    }
}

/// One list beside the map: the rows' ids, names, colours and sizes (`listOf`).
#[derive(Clone, Debug, Default)]
pub struct List {
    pub ids: Vec<String>,
    pub names: Vec<String>,
    pub colours: Vec<[u8; 3]>,
    pub cells: Vec<f64>,
    pub widths: Vec<f64>,
    /// Structures are counted, not measured in area.
    pub counted: bool,
    /// What the shares are of: the points of the map, or the structures there are.
    pub total: f64,
}

/// What `measure` gives (`cells`, `widths`, `absent`, `figures`), and the five lists, by `View::index`.
#[derive(Clone, Debug, Default)]
pub struct Measured {
    pub cells: Vec<f64>,
    pub widths: Vec<f64>,
    pub absent: Vec<String>,
    pub figures: Figures,
    pub lists: [Option<List>; 5],
}

/// `measure` (1255-1322). `listed` says `preview.json` has `possible`: only then is there a total of biomes.
pub fn measure(world: &World, absent: Vec<String>, listed: bool) -> Measured {
    let n = world.samples;
    let total = n * n;
    let sea = world.sea_level;
    let step = world.step;
    let coarse = step > 16.0;
    let biomes = world.biomes.len();
    let mut cells = vec![0.0f64; biomes];
    // Sum of (zone area)^2 per biome: divided by the biome's area, the size of the zone a random point of it
    // sits in. Small scattered patches weigh little, as they should.
    let mut weighted = vec![0.0f64; biomes];
    zones(&world.biome_at, n, |b, size| {
        cells[b as usize] += size as f64;
        weighted[b as usize] += size as f64 * size as f64;
    });
    let widths: Vec<f64> = cells.iter().zip(&weighted).map(|(&c, &w)| if c != 0.0 { (w / c).sqrt() * step } else { 0.0 }).collect();

    let (mut ocean, mut land, mut high, mut top) = (0usize, 0usize, 0usize, -999.0f64);
    for &h in &world.heights {
        let y = h as f64;
        if y > top {
            top = y;
        }
        if y < sea {
            ocean += 1;
            continue;
        }
        land += 1;
        if y > 150.0 {
            high += 1;
        }
    }
    let flat = world.flat_at.iter().filter(|&&f| f == 1).count();
    // A river runs through a plain without ending it, so river water joins the zones on its two banks.
    let river: Vec<bool> = world.biomes.iter().map(|b| b.id.contains("river")).collect();
    let mut plain = world.flat_at.clone();
    for i in 0..total {
        if (world.heights[i] as f64) < sea && river[world.biome_at[i] as usize] {
            plain[i] = 1;
        }
    }
    let (mut plain_cells, mut plain_weighted) = (0.0f64, 0.0f64);
    zones(&plain, n, |is_plain, size| {
        if is_plain == 1 {
            plain_cells += size as f64;
            plain_weighted += size as f64 * size as f64;
        }
    });
    let (mut land_cells, mut land_weighted) = (0.0f64, 0.0f64);
    for (i, b) in world.biomes.iter().enumerate() {
        if !colour::is_water(&b.id) {
            land_cells += cells[i];
            land_weighted += cells[i] * widths[i];
        }
    }
    let (total_f, land_f) = (total as f64, land as f64);
    let figures = Figures {
        ocean: 100.0 * ocean as f64 / total_f,
        flat: if coarse { None } else { Some(100.0 * flat as f64 / land_f) },
        high: 100.0 * high as f64 / land_f,
        top,
        expanse: if coarse {
            None
        } else if plain_cells != 0.0 {
            Some((plain_weighted / plain_cells).sqrt() * step)
        } else {
            Some(0.0)
        },
        count: biomes,
        total: listed.then_some(biomes + absent.len()),
        typical: land_weighted / land_cells,
        span: n as f64 * step,
        step,
    };

    let mut lists: [Option<List>; 5] = Default::default();
    lists[View::Biomes.index()] = Some(List {
        ids: world.biomes.iter().map(|b| b.id.clone()).collect(),
        names: world.biomes.iter().map(|b| text::pretty(&b.id)).collect(),
        colours: world.colours.clone(),
        cells: cells.clone(),
        widths: widths.clone(),
        counted: false,
        total: total_f,
    });
    for view in [View::Flat, View::Height, View::Temperature] {
        let Some((at, ids)) = rows(world, view) else { continue };
        let (names, colours): (Vec<String>, Vec<[u8; 3]>) = match view {
            View::Height => (band_names(sea, &HEIGHT_STEPS, &SEA_DEPTHS), band_colours(sea, &HEIGHT_STEPS, &SEA_DEPTHS)),
            View::Temperature => (
                text::LEGEND_TEMPERATURE.iter().map(|s| s.to_string()).collect(),
                (0..5).map(|i| temperature_colour(-0.7 + i as f64 * 2.7 / 4.0)).collect(),
            ),
            _ => (text::LEGEND_FLAT.iter().map(|s| s.to_string()).collect(), vec![flat_colour(), steep_colour(), flat_sea_colour()]),
        };
        let mut band_cells = vec![0.0f64; names.len()];
        let mut band_weighted = vec![0.0f64; names.len()];
        zones(&at, n, |band, size| {
            band_cells[band as usize] += size as f64;
            band_weighted[band as usize] += size as f64 * size as f64;
        });
        let band_widths = band_cells.iter().zip(&band_weighted).map(|(&c, &w)| if c != 0.0 { (w / c).sqrt() * step } else { 0.0 }).collect();
        lists[view.index()] = Some(List { ids, names, colours, cells: band_cells, widths: band_widths, counted: false, total: total_f });
    }
    // Structures are counted, not measured in area: a row gives how many there are, and about how far apart.
    if let Some(s) = &world.structures {
        let area = (n as f64 * step) * (n as f64 * step);
        let cells: Vec<f64> = s.count.iter().map(|&c| c as f64).collect();
        let widths = cells.iter().map(|&c| if c > 0.0 { (area / c).sqrt() } else { 0.0 }).collect();
        lists[View::Structures.index()] = Some(List {
            ids: s.ids.clone(),
            names: s.ids.iter().map(|id| text::pretty(id)).collect(),
            colours: s.colours.clone(),
            cells,
            widths,
            counted: true,
            total: s.points.len() as f64,
        });
    }
    Measured { cells, widths, absent, figures, lists }
}

/// `Math.round`: halves go up, also below zero.
pub fn js_round(v: f64) -> f64 {
    let f = v.floor();
    if v - f >= 0.5 { f + 1.0 } else { f }
}

/// `percent` (653).
pub fn percent(v: f64) -> String {
    format!("{}%", text::num_digits(v, 1))
}

/// `blocks` (655): a width read on a 16-block grid is not known to the block, so it is rounded to 50.
pub fn blocks(v: f64) -> String {
    format!("{} blocks", text::num(js_round(v / 50.0) * 50.0))
}

/// `Number.prototype.toFixed(1)`. The one place a double is not simply rounded to nearest is an exact tie, which
/// only a multiple of a quarter can be (82.25, 0.75): the spec takes the larger multiple of a tenth, so away from
/// zero. Anything else is nearest, which Rust's exact formatting also gives.
pub fn to_fixed1(v: f64) -> String {
    if v == 0.0 {
        return "0.0".to_string();
    }
    let a = v.abs();
    let tie = (a * 4.0).fract() == 0.0 && (a * 10.0).fract() == 0.5;
    let out = if tie {
        let tenths = (a * 10.0 + 0.5).floor() as u64;
        format!("{}.{}", tenths / 10, tenths % 10)
    } else {
        format!("{a:.1}")
    };
    if v < 0.0 { format!("-{out}") } else { out }
}

/// What a cell of the figures grid is (the `className` the page gives its span).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum CellKind {
    Blank,
    Head,
    Rule,
    Name,
    Value { goal: bool, muted: bool },
    Before { goal: bool },
}

impl CellKind {
    pub fn class(&self) -> String {
        match *self {
            CellKind::Blank | CellKind::Name => String::new(),
            CellKind::Head => "head".to_string(),
            CellKind::Rule => "rule".to_string(),
            CellKind::Value { goal, muted } => format!("{}{}", if goal { "value goal" } else { "value" }, if muted { " muted" } else { "" }),
            CellKind::Before { goal } => (if goal { "before goal-change" } else { "before" }).to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub text: String,
    pub kind: CellKind,
}

/// The figures in the order the page lists them (`text.figureNames`, 604-612) and the three of the goal (616).
pub const KEYS: [&str; 7] = ["flat", "expanse", "typical", "ocean", "high", "top", "count"];
const GOAL: [&str; 3] = ["flat", "expanse", "typical"];

/// How a figure is shown (`format`, 1500).
pub fn format_value(f: &Figures, key: &str, v: f64) -> String {
    match key {
        "ocean" | "flat" | "high" => percent(v),
        "top" => text::altitude(v),
        "count" => match f.total.filter(|t| *t > 0) {
            Some(t) => text::count_of(f.count, t),
            None => format!("{}", f.count),
        },
        _ => blocks(v),
    }
}

/// What moved since the run before, counted on the values as they are shown (1502-1511): [rounding step, decimals].
fn change(key: &str, f: f64, b: f64) -> String {
    let (step, digits) = match key {
        "ocean" | "flat" | "high" => (0.1, 1),
        "top" | "count" => (1.0, 0),
        _ => (50.0, 0),
    };
    let steps = js_round(f / step) - js_round(b / step);
    if steps == 0.0 {
        return text::SAME.to_string();
    }
    // A width is rounded to 50 blocks: one step of it is not a change worth a number.
    if step == 50.0 && steps.abs() == 1.0 {
        return text::CLOSE.to_string();
    }
    // Two shares differ by points, not by percent.
    format!("{}{}{}", if steps > 0.0 { "+" } else { "\u{2212}" }, text::num_digits(steps.abs() * step, digits), if step == 0.1 { text::POINTS } else { "" })
}

/// The cells `showFigures` appends to `#figures`, in order. `before` is the run before; it counts only over the
/// same area at the same detail.
pub fn figure_cells(f: &Figures, before: Option<&Figures>) -> Vec<Cell> {
    let b = before.filter(|b| b.span == f.span && b.step == f.step);
    let cell = |text: &str, kind| Cell { text: text.to_string(), kind };
    let mut rows = Vec::new();
    // The second column needs a name only once there is something in it.
    if b.is_some() {
        rows.push(cell("", CellKind::Blank));
        rows.push(cell("", CellKind::Blank));
        rows.push(cell(text::CHANGE, CellKind::Head));
    }
    for (i, key) in KEYS.iter().enumerate() {
        let goal = GOAL.contains(key);
        // The figures that answer the goal come first and larger; a line sets the others apart.
        if *key == "ocean" {
            rows.push(cell("", CellKind::Rule));
        }
        let label = cell(text::FIGURE_NAMES[i], CellKind::Name);
        // The count of biomes is read against all there are: a change beside it was taken for the missing ones.
        let moved = match (b.and_then(|b| b.get(key)), f.get(key)) {
            (Some(bv), Some(fv)) if !(*key == "count" && f.total.is_some_and(|t| t > 0)) => change(key, fv, bv),
            _ => String::new(),
        };
        let moved = cell(&moved, CellKind::Before { goal });
        // A figure the map's detail cannot give stays in its place, greyed.
        let value = match f.get(key) {
            Some(v) => cell(&format_value(f, key, v), CellKind::Value { goal, muted: false }),
            None => cell("\u{2014}", CellKind::Value { goal, muted: true }),
        };
        rows.push(label);
        rows.push(value);
        rows.push(moved);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_find_touching_points_of_one_value() {
        // 1 1 2
        // 1 2 2
        // 3 3 2
        let grid = [1u8, 1, 2, 1, 2, 2, 3, 3, 2];
        let mut got = Vec::new();
        zones(&grid, 3, |v, size| got.push((v, size)));
        assert_eq!(got, vec![(1, 3), (2, 4), (3, 2)]);
        // Points that meet at a corner do not touch: four zones of one point.
        let mut got = Vec::new();
        zones(&[1u8, 0, 0, 1], 2, |v, size| got.push((v, size)));
        assert_eq!(got, vec![(1, 1), (0, 1), (0, 1), (1, 1)]);
    }

    #[test]
    fn javascript_rounding() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(0.49999999999999994), 0.0);
        assert_eq!(to_fixed1(82.25), "82.3");
        assert_eq!(to_fixed1(0.75), "0.8");
        assert_eq!(to_fixed1(1.04), "1.0");
        assert_eq!(to_fixed1(15.0), "15.0");
        assert_eq!(to_fixed1(-0.0), "0.0");
        assert_eq!(to_fixed1(-82.25), "-82.3");
        assert_eq!(blocks(1274.9), "1,250 blocks");
        assert_eq!(blocks(1275.0), "1,300 blocks");
        assert_eq!(percent(21.44), "21.4%");
    }

    fn figures() -> Figures {
        Figures { ocean: 41.5, flat: Some(38.2), high: 6.3, top: 287.0, expanse: Some(1250.0), count: 25, total: Some(146), typical: 900.0, span: 4096.0, step: 16.0 }
    }

    #[test]
    fn the_cells_of_the_figures() {
        let f = figures();
        let plain = figure_cells(&f, None);
        // 7 names, 7 values, 7 changes and the rule.
        assert_eq!(plain.len(), 22);
        assert_eq!(plain[0], Cell { text: "Flat land".into(), kind: CellKind::Name });
        assert_eq!(plain[1].text, "38.2%");
        assert_eq!(plain[1].kind.class(), "value goal");
        assert_eq!(plain[9].kind, CellKind::Rule);
        assert_eq!(plain[3].text, "Typical flat zone");
        assert_eq!(plain[4].text, "1,250 blocks");
        let top = plain.iter().position(|c| c.text == "Highest peak").unwrap();
        assert_eq!(plain[top + 1].text, "height 287");
        assert_eq!(plain.last().unwrap().kind, CellKind::Before { goal: false });
        assert_eq!(plain[plain.len() - 2].text, "25 of 146");

        let before = Figures { ocean: 41.5 + 0.34, flat: Some(38.2 - 1.06), top: 284.0, ..f };
        let with = figure_cells(&f, Some(&before));
        assert_eq!(with.len(), 25);
        assert_eq!(with[2], Cell { text: "change".into(), kind: CellKind::Head });
        let change_of = |name: &str| {
            let i = with.iter().position(|c| c.text == name).unwrap();
            with[i + 2].text.clone()
        };
        assert_eq!(change_of("Flat land"), "+1.1 pt");
        assert_eq!(change_of("Sea"), "\u{2212}0.3 pt");
        assert_eq!(change_of("Highest peak"), "+3");
        assert_eq!(change_of("High land"), "=");
        assert_eq!(change_of("Biomes present"), "", "the count shows no change beside a total");
        // Another area: nothing to compare with.
        assert_eq!(figure_cells(&f, Some(&Figures { span: 8192.0, ..before })).len(), 22);
    }

    #[test]
    fn a_missing_figure_is_a_dash_and_greyed() {
        let f = Figures { flat: None, expanse: None, ..figures() };
        let cells = figure_cells(&f, None);
        assert_eq!(cells[1], Cell { text: "\u{2014}".into(), kind: CellKind::Value { goal: true, muted: true } });
        assert_eq!(cells[1].kind.class(), "value goal muted");
    }

    #[test]
    fn figures_travel_as_json() {
        let f = Figures { flat: None, typical: f64::NAN, ..figures() };
        let text = f.to_json();
        assert_eq!(
            text,
            "{\"ocean\":41.5,\"flat\":null,\"high\":6.3,\"top\":287,\"expanse\":1250,\"count\":25,\"total\":146,\"typical\":null,\"span\":4096,\"step\":16}"
        );
        let back = Figures::from_json(&Json::parse(&text).unwrap()).unwrap();
        assert_eq!((back.flat, back.total, back.count, back.step), (None, Some(146), 25, 16.0));
        assert!(back.typical.is_nan());
        // A file from before the step was kept reads 16.
        let old = Figures::from_json(&Json::parse("{\"ocean\":1,\"span\":4096}").unwrap()).unwrap();
        assert_eq!(old.step, 16.0);
    }
}
