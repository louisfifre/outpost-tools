//! The section's model, read and played the way the page does (preview.html lines 1909-2204): the cuts and the
//! ore rules the server loaded, the ground of a cut, and the ores played on it with the game's own way of drawing a
//! height and of shaping a vein.
//!
//! Every number of the play is an f64 and every step is in the page's order of evaluation (each `draw()` included),
//! because the page's own code run by Node is the test: the figures and the drawn layer must agree byte for byte.
//! Integers appear only where an array is indexed. `Math.round` is `(v + 0.5).floor()`.
//!
//! Not built: `veinOf` and the `everywhere` rows (they belong to the ores settings group, which has no editor yet).

use std::collections::BTreeMap;
use std::path::Path;

use crate::colour::hash_of;
use crate::json::Json;
use crate::world::World;

/// Line 1918: the order of the ores, in the list and in the drawn layer (a cell holds the kind plus one).
pub const ORE_KINDS: [&str; 8] = ["coal", "copper", "iron", "gold", "lapis", "redstone", "diamond", "emerald"];
/// Lines 1916-1917.
pub const ORE_COLOURS: [[u8; 3]; 8] =
    [[24, 24, 24], [231, 124, 86], [227, 192, 170], [252, 238, 75], [42, 95, 208], [255, 31, 31], [93, 236, 245], [23, 221, 98]];
/// Line 1919.
pub const WATER: [u8; 3] = [47, 92, 158];
pub const LAVA: [u8; 3] = [226, 102, 26];
pub const ROCK: [u8; 3] = [128, 128, 128];
pub const DEEP: [u8; 3] = [80, 80, 86];
pub const EARTH: [u8; 3] = [139, 99, 68];
/// Line 1930: a cave is a hollow, darker than the rock around it.
pub const CAVE: [u8; 3] = [46, 42, 40];
/// Line 1921: the top layers no ore replaces.
pub const EARTH_DEPTH: f64 = 3.0;
/// Line 1923: ore near the ground is counted down to this depth, apart under low and under high ground.
pub const NEAR_DEPTH: f64 = 64.0;
pub const HIGH_GROUND: f64 = 80.0;
/// Line 1927.
pub const COUNTED_CHUNKS: usize = 512;
/// Line 1929: the ores drawn are those of a slice this thick, in blocks across the cut.
pub const DRAWN_SLICE: (f64, f64) = (6.0, 10.0);

/// One cut of the section: the line it follows and what the generator answered along it.
#[derive(Clone, Debug)]
pub struct Cut {
    pub file: String,
    pub x: f64,
    pub z: f64,
    /// Read when the section is loaded; None when the file is missing or is not `length * height` bytes long.
    pub cells: Option<Vec<u8>>,
}

/// A rule that counts its depth from the ground (line 1950): tries per chunk, nearest and deepest depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchored {
    pub count: f64,
    pub from: f64,
    pub to: f64,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub id: String,
    /// Its place in `ores.placements`, which is what `Section::biomes` lists.
    pub index: usize,
    /// An index in `ORE_KINDS`.
    pub kind: usize,
    pub config: Json,
    pub placement: Vec<Json>,
    pub playable: bool,
    pub anchored: Option<Anchored>,
}

/// A row of what is added under the ground (line 1990): one per anchored rule of the mods.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: String,
    pub kind: usize,
    pub count: f64,
    pub from: f64,
    pub to: f64,
}

#[derive(Clone, Debug)]
pub struct Section {
    pub length: usize,
    pub height: usize,
    pub min_y: f64,
    pub cuts: Vec<Cut>,
    pub rules: Vec<Rule>,
    /// Surface biome id to the placements (by index) it lists.
    pub biomes: BTreeMap<String, Vec<usize>>,
}

/// Line 1932.
fn bare(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}

fn string(j: Option<&Json>) -> Option<&str> {
    match j {
        Some(Json::Str(s)) => Some(s),
        _ => None,
    }
}

/// `bare(step.type)`; a step without a type reads "undefined", as the page's template string does.
fn step_type(step: &Json) -> &str {
    bare(string(step.get("type")).unwrap_or("undefined"))
}

/// Lines 1933-1937 without a regular expression: the text after the last ':' must be an optional "deepslate_",
/// then one of the kinds, then "_ore".
pub fn ore_kind(state_id: &str) -> Option<usize> {
    let (_, name) = state_id.rsplit_once(':')?;
    let name = name.strip_prefix("deepslate_").unwrap_or(name);
    let kind = name.strip_suffix("_ore")?;
    ORE_KINDS.iter().position(|k| *k == kind)
}

fn rule_kind(config: &Json) -> Option<usize> {
    let state = config.get("targets")?.array()?.first()?.get("state")?;
    match state {
        Json::Str(s) => ore_kind(s),
        other => ore_kind(string(other.get("Name"))?),
    }
}

/// Lines 1939-1941: a number a data pack may write as a plain value or as a draw between two bounds. A uniform
/// with a bound missing gives None where the page's array would hold `undefined` and go on to NaN: the Node
/// comparison of `playable` shows the data does not reach it.
pub fn bounds(v: &Json) -> Option<(f64, f64)> {
    match v {
        Json::Num(n) => Some((*n, *n)),
        Json::Obj(_) => {
            let kind = string(v.get("type")).unwrap_or("");
            if kind.ends_with("constant") {
                bounds(v.get("value")?)
            } else if kind.ends_with("uniform") {
                // `a ?? b`: the second only when the first is absent or null.
                let pick = |key: &str| {
                    let first = v.get(key).filter(|j| **j != Json::Null);
                    first.or_else(|| v.get("value").and_then(|x| x.get(key))).and_then(Json::num)
                };
                Some((pick("min_inclusive")?, pick("max_inclusive")?))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Lines 1942-1947.
fn playable(placement: &[Json]) -> bool {
    const STEPS: [&str; 7] = ["count", "rarity_filter", "in_square", "height_range", "heightmap", "offset", "biome"];
    const HEIGHTS: [&str; 5] = ["constant", "uniform", "trapezoid", "biased_to_bottom", "very_biased_to_bottom"];
    placement.iter().all(|step| {
        let t = step_type(step);
        STEPS.contains(&t)
            && (t != "count" || step.get("count").and_then(bounds).is_some())
            && (t != "offset" || ["x", "y", "z"].iter().all(|k| step.get(k).and_then(bounds).is_some()))
            && (t != "height_range" || step.get("height").and_then(|h| string(h.get("type"))).is_some_and(|h| HEIGHTS.contains(&bare(h))))
    })
}

/// Lines 1950-1963: the ground's height, then steps down.
fn anchored(placement: &[Json]) -> Option<Anchored> {
    let at = placement.iter().position(|s| step_type(s) == "heightmap")?;
    if placement.iter().any(|s| step_type(s) == "height_range") {
        return None;
    }
    let (mut from, mut to) = (0.0, 0.0);
    for step in &placement[at + 1..] {
        if step_type(step) != "offset" {
            continue;
        }
        let y = bounds(step.get("y")?)?;
        from -= y.1;
        to -= y.0;
    }
    // `find(...)?.count ?? 1`: the first count step; a missing or null count is 1.
    let count = placement.iter().find(|s| step_type(s) == "count").and_then(|s| s.get("count")).filter(|j| **j != Json::Null);
    let count = bounds(count.unwrap_or(&Json::Num(1.0)))?;
    (from >= 0.0).then_some(Anchored { count: count.1, from, to })
}

/// `Math.round`.
fn round(v: f64) -> f64 {
    (v + 0.5).floor()
}

fn obj(fields: Vec<(&str, Json)>) -> Json {
    Json::Obj(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Lines 1966-1973: the same depth written the way a data pack must, several steps of 16 blocks at most, each
/// drawn between its share of the two bounds.
pub fn steps_down(from: f64, to: f64) -> Vec<Json> {
    let parts = (to / 16.0).ceil().max(1.0);
    (0..parts as usize)
        .map(|i| {
            let i = i as f64;
            let share = |v: f64| round(v * (i + 1.0) / parts) - round(v * i / parts);
            let y = obj(vec![
                ("type", Json::Str("uniform".into())),
                ("min_inclusive", Json::Num(-share(to))),
                ("max_inclusive", Json::Num(-share(from))),
            ]);
            obj(vec![("type", Json::Str("offset".into())), ("x", Json::Num(0.0)), ("y", y), ("z", Json::Num(0.0))])
        })
        .collect()
}

fn read_json(path: &Path) -> Option<Json> {
    let bytes = std::fs::read(path).ok()?;
    Json::parse(std::str::from_utf8(&bytes).ok()?).ok()
}

impl Section {
    /// `loadSection` (1975-1982): None unless section.json lists cuts and ores.json parses.
    pub fn load(dir: &Path) -> Option<Section> {
        let meta = read_json(&dir.join("section.json"))?;
        let ores = read_json(&dir.join("ores.json"))?;
        let list = meta.get("cuts")?.array().filter(|l| !l.is_empty())?;
        let length = meta.get("length")?.num()? as usize;
        let height = meta.get("height")?.num()? as usize;
        let min_y = meta.get("minY")?.num()?;
        let mut cuts = Vec::new();
        for cut in list {
            let file = cut.get("file").and_then(Json::text).unwrap_or_default();
            // The page reads a file by its name in the data folder and nothing else.
            let name = Path::new(&file).file_name().map(|n| n.to_owned());
            let cells = name.and_then(|n| std::fs::read(dir.join(n)).ok()).filter(|b| b.len() == length * height);
            cuts.push(Cut { file, x: cut.get("x")?.num()?, z: cut.get("z")?.num()?, cells });
        }
        let mut rules = Vec::new();
        for (index, rule) in ores.get("placements")?.array()?.iter().enumerate() {
            let config = rule.get("config").cloned().unwrap_or(Json::Null);
            let Some(kind) = rule_kind(&config) else { continue };
            let placement = rule.get("placement").and_then(Json::array).map(<[Json]>::to_vec).unwrap_or_default();
            let playable = playable(&placement);
            let anchored = if playable { anchored(&placement) } else { None };
            rules.push(Rule { id: rule.get("id").and_then(Json::text).unwrap_or_default(), index, kind, config, placement, playable, anchored });
        }
        let mut biomes = BTreeMap::new();
        if let Some(Json::Obj(fields)) = ores.get("biomes") {
            for (id, list) in fields {
                let indexes = list.array().map(|a| a.iter().filter_map(Json::num).map(|n| n as usize).collect()).unwrap_or_default();
                biomes.insert(id.clone(), indexes);
            }
        }
        Some(Section { length, height, min_y, cuts, rules, biomes })
    }

    /// The y of the top row of a cut.
    pub fn top_y(&self) -> f64 {
        self.min_y + self.height as f64 - 1.0
    }

    /// Line 1990: what the mods already add under the ground, one row per anchored rule.
    pub fn rows_of_mods(&self) -> Vec<Row> {
        self.rules
            .iter()
            .filter_map(|r| r.anchored.map(|a| Row { id: r.id.clone(), kind: r.kind, count: a.count, from: a.from, to: a.to }))
            .collect()
    }

    /// Line 2389: the rules that use a placement step this page cannot play.
    pub fn unplayed(&self) -> usize {
        self.rules.iter().filter(|r| !r.playable).count()
    }
}

/// The ground of a cut, once per cut (lines 2008-2034).
#[derive(Clone, Debug)]
pub struct Ground {
    /// The first block that is not air, and the first that is neither air nor water (the sea floor under a sea),
    /// per column, plus one.
    pub surface: Vec<f64>,
    pub floor: Vec<f64>,
    pub chunks: usize,
    /// 1 where the chunk lies under high ground.
    pub high: Vec<u8>,
    /// The surface biome of each chunk, which says which rules it plays.
    pub biomes: Vec<Option<String>>,
    pub top: f64,
    pub bottom: f64,
    pub peak: usize,
    /// Rows of empty sky above the highest ground, left out of the drawing.
    pub crop: usize,
    pub rows: usize,
}

pub fn ground_of(world: &World, section: &Section, cells: &[u8], cut: &Cut) -> Ground {
    let (l, h) = (section.length, section.height);
    let min_y = section.min_y;
    let top_y = section.top_y();
    let mut surface = vec![min_y; l];
    let mut floor = vec![min_y; l];
    for x in 0..l {
        let mut seen = false;
        // y from the top down: row r is y = top_y - r.
        for r in 0..h {
            let cell = cells[r * l + x];
            let y = top_y - r as f64;
            if cell != 0 && !seen {
                surface[x] = y + 1.0;
                seen = true;
            }
            if cell == 1 || cell == 3 {
                floor[x] = y + 1.0;
                break;
            }
        }
    }
    let chunks = l >> 4;
    let (mut high, mut biomes) = (vec![0u8; chunks], Vec::new());
    let last = (world.samples - 1) as f64;
    let row = round((cut.z - world.origin_z) / world.step).clamp(0.0, last) as usize;
    for c in 0..chunks {
        let mut sixteen: Vec<f64> = floor[c * 16..c * 16 + 16].to_vec();
        sixteen.sort_by(|a, b| a.total_cmp(b));
        high[c] = (sixteen[8] >= HIGH_GROUND) as u8;
        let col = ((cut.x + (c * 16) as f64 - world.origin_x) / world.step).floor().clamp(0.0, last) as usize;
        biomes.push(world.biomes.get(world.biome_at[row * world.samples + col] as usize).map(|b| b.id.clone()));
    }
    let (mut top, mut bottom, mut peak) = (min_y, top_y, 0);
    for x in 0..l {
        if surface[x] > top {
            top = surface[x];
            peak = x;
        }
        bottom = bottom.min(floor[x]);
    }
    // The empty sky above the highest ground is left out of the drawing.
    let crop = (top_y - top - 24.0).max(0.0) as usize;
    Ground { surface, floor, chunks, high, biomes, top, bottom, peak, crop, rows: h - crop }
}

/// The page's `drawOf` (1998-2006): a mulberry32 stream that depends on its name alone, so the same rule gives the
/// same veins from one play to the next.
pub struct Draw(u32);

impl Draw {
    pub fn new(name: &str) -> Draw {
        // `Math.floor(hashOf(name) * 4294967296) | 0`: the hash is a 32-bit number divided by 2^32, so this is it back.
        Draw((hash_of(name) * 4294967296.0) as u32)
    }

    /// Math.imul gives the low 32 bits of the product, which is what a wrapping multiply of u32 gives.
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let a = self.0;
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        // `t + imul(..) ^ t` is `(t + imul(..)) ^ t`.
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        (t ^ (t >> 14)) as f64 / 4294967296.0
    }
}

/// What one ore shows in the list: blocks per chunk near the ground, under low and under high ground.
#[derive(Clone, Debug, PartialEq)]
pub struct OreFigure {
    pub low: Option<f64>,
    pub high: Option<f64>,
    /// The plays (rule or row names) that make it, in play order.
    pub from: Vec<String>,
}

/// A height draw of the game (lines 2049-2064), its anchors already turned into y.
#[derive(Clone, Copy, Debug)]
enum Height {
    Constant(f64),
    Uniform(f64, f64),
    Trapezoid { low: f64, top: f64, plateau: f64 },
    Biased { low: f64, top: f64, inner: f64 },
    VeryBiased { low: f64, top: f64, inner: f64 },
}

/// One placement step, read from its JSON once per play instead of once per try.
#[derive(Clone, Copy, Debug)]
enum Step {
    Count(f64, f64),
    Rarity(f64),
    InSquare,
    Height(Height),
    /// True for a heightmap name that holds OCEAN_FLOOR.
    Heightmap(bool),
    Offset([(f64, f64); 3]),
    Pass,
}

fn number(j: Option<&Json>) -> f64 {
    j.and_then(Json::num).unwrap_or(f64::NAN)
}

impl Step {
    fn compile(step: &Json, min_y: f64, top_y: f64) -> Step {
        // Line 2046.
        let anchor = |v: Option<&Json>| match v {
            Some(v) if v.get("absolute").is_some() => number(v.get("absolute")),
            Some(v) if v.get("above_bottom").is_some() => min_y + number(v.get("above_bottom")),
            Some(v) => top_y - number(v.get("below_top")),
            None => f64::NAN,
        };
        match step_type(step) {
            "count" => {
                let (lo, hi) = step.get("count").and_then(bounds).unwrap_or((f64::NAN, f64::NAN));
                Step::Count(lo, hi)
            }
            "rarity_filter" => Step::Rarity(number(step.get("chance"))),
            "in_square" => Step::InSquare,
            "height_range" => {
                let h = step.get("height");
                let get = |key: &str| h.and_then(|h| h.get(key));
                let (low, top) = (anchor(get("min_inclusive")), anchor(get("max_inclusive")));
                let kind = string(get("type")).map(bare).unwrap_or("");
                Step::Height(match kind {
                    "constant" => Height::Constant(anchor(get("value"))),
                    "uniform" => Height::Uniform(low, top),
                    "trapezoid" => Height::Trapezoid { low, top, plateau: get("plateau").and_then(Json::num).unwrap_or(0.0) },
                    "biased_to_bottom" => Height::Biased { low, top, inner: get("inner").and_then(Json::num).unwrap_or(1.0) },
                    _ => Height::VeryBiased { low, top, inner: get("inner").and_then(Json::num).unwrap_or(1.0) },
                })
            }
            "heightmap" => Step::Heightmap(string(step.get("heightmap")).is_some_and(|h| h.contains("OCEAN_FLOOR"))),
            "offset" => {
                let b = |k: &str| step.get(k).and_then(bounds).unwrap_or((f64::NAN, f64::NAN));
                Step::Offset([b("x"), b("y"), b("z")])
            }
            _ => Step::Pass,
        }
    }
}

/// One rule, or one row, to play.
#[derive(Clone, Debug)]
struct OnePlay {
    name: String,
    kind: usize,
    /// The placement the surface biome must list.
    rule: usize,
    size: f64,
    skip: f64,
    steps: Vec<Step>,
}

/// What a play keeps across the veins: the box of blocks met (line 2135) and the balls of a vein.
struct Scratch {
    met: Vec<i32>,
    stamp: i32,
    balls: Vec<f64>,
}

impl Scratch {
    fn new() -> Scratch {
        #[cfg(test)]
        tests::SCRATCHES.with(|n| n.set(n.get() + 1));
        Scratch { met: vec![0; 32768], stamp: 0, balls: vec![0.0; 128] }
    }
}

struct Ctx<'a> {
    seed: &'a str,
    at: usize,
    section: &'a Section,
    cells: &'a [u8],
    ground: &'a Ground,
    l: f64,
    min_y: f64,
    top_y: f64,
}

impl Ctx<'_> {
    /// Line 2044.
    fn cell_at(&self, x: f64, y: f64) -> u8 {
        if x < 0.0 || x >= self.l || y < self.min_y || y > self.top_y { 0 } else { self.cells[((self.top_y - y) * self.l + x) as usize] }
    }
}

/// `clamp(v, 0, L - 1)` as an index.
fn index(v: f64, l: f64) -> usize {
    v.min(l - 1.0).max(0.0) as usize
}

/// Line 2047.
fn between(low: f64, top: f64, draw: &mut Draw) -> f64 {
    if top < low { low } else { low + (draw.next() * (top - low + 1.0)).floor() }
}

/// Line 2045: a draw between two bounds, one number of the stream whatever the bounds.
fn int(low: f64, top: f64, draw: &mut Draw) -> f64 {
    low + (draw.next() * (top - low + 1.0)).floor()
}

fn height(h: Height, draw: &mut Draw) -> f64 {
    match h {
        Height::Constant(v) => v,
        Height::Uniform(low, top) => between(low, top, draw),
        Height::Trapezoid { low, top, plateau } => {
            if plateau >= top - low {
                return between(low, top, draw);
            }
            let side = ((top - low - plateau) / 2.0).floor();
            let a = (draw.next() * (top - low - side + 1.0)).floor();
            let b = (draw.next() * (side + 1.0)).floor();
            low + a + b
        }
        Height::Biased { low, top, inner } => {
            // The outer draw is read first, then the inner one inside its factor.
            let outer = draw.next();
            let inner_draw = draw.next();
            low + (outer * ((inner_draw * (top - low - inner + 1.0).max(1.0)).floor() + inner)).floor()
        }
        Height::VeryBiased { low, top, inner } => {
            let first = between(low + inner, top, draw);
            let second = between(low, first - 1.0, draw);
            between(low, second - 1.0 + inner, draw)
        }
    }
}

/// Lines 2066-2088: where a rule tries, in one chunk.
fn tries(steps: &[Step], x0: f64, draw: &mut Draw, ctx: &Ctx) -> Vec<[f64; 3]> {
    let mut places = vec![[x0, 0.0, 0.0]];
    for step in steps {
        let mut next = Vec::new();
        for &[x, y, z] in &places {
            match *step {
                Step::Count(lo, hi) => {
                    let mut n = int(lo, hi, draw);
                    while n > 0.0 {
                        next.push([x, y, z]);
                        n -= 1.0;
                    }
                }
                Step::Rarity(chance) => {
                    if draw.next() < 1.0 / chance {
                        next.push([x, y, z]);
                    }
                }
                Step::InSquare => {
                    let a = x + (draw.next() * 16.0).floor();
                    let b = z + (draw.next() * 16.0).floor();
                    next.push([a, y, b]);
                }
                Step::Height(h) => next.push([x, height(h, draw), z]),
                Step::Heightmap(floor) => {
                    let ground = if floor { &ctx.ground.floor } else { &ctx.ground.surface }[index(x, ctx.l)];
                    if ground > ctx.min_y {
                        next.push([x, ground, z]);
                    }
                }
                Step::Offset([sx, sy, sz]) => {
                    let a = x + int(sx.0, sx.1, draw);
                    let b = y + int(sy.0, sy.1, draw);
                    let c = z + int(sz.0, sz.1, draw);
                    next.push([a, b, c]);
                }
                Step::Pass => next.push([x, y, z]),
            }
        }
        places = next;
    }
    places
}

/// Lines 2090-2134: a vein, the way the game's ore feature shapes it: balls along a short slanted line. `put` is
/// called with every block, and may draw from the same stream.
fn vein(ox: f64, oy: f64, oz: f64, size: f64, draw: &mut Draw, s: &mut Scratch, put: &mut dyn FnMut(&mut Draw, f64, f64, f64)) {
    let angle = draw.next() * std::f64::consts::PI;
    let reach = size / 8.0;
    let x0 = ox + angle.sin() * reach;
    let x1 = ox - angle.sin() * reach;
    let z0 = oz + angle.cos() * reach;
    let z1 = oz - angle.cos() * reach;
    let y0 = oy + (draw.next() * 3.0).floor() - 2.0;
    let y1 = oy + (draw.next() * 3.0).floor() - 2.0;
    // An integer size is the only one a pack writes; anything else makes no balls.
    let n = if size.is_finite() && size > 0.0 { size as usize } else { 0 };
    if s.balls.len() < n * 4 {
        s.balls = vec![0.0; n * 4];
    }
    let balls = &mut s.balls;
    // Each ball is four numbers in a row: its centre, then its radius.
    for i in 0..n {
        let t = i as f64 / size;
        let r = draw.next() * size / 16.0;
        balls[i * 4] = x0 + (x1 - x0) * t;
        balls[i * 4 + 1] = y0 + (y1 - y0) * t;
        balls[i * 4 + 2] = z0 + (z1 - z0) * t;
        balls[i * 4 + 3] = (((std::f64::consts::PI * t).sin() + 1.0) * r + 1.0) / 2.0;
    }
    // A ball inside another one adds nothing.
    for i in 0..n.saturating_sub(1) {
        for j in i + 1..n {
            let (a, b) = (i * 4, j * 4);
            if balls[a + 3] <= 0.0 || balls[b + 3] <= 0.0 {
                continue;
            }
            let gap = balls[a + 3] - balls[b + 3];
            let (dx, dy, dz) = (balls[a] - balls[b], balls[a + 1] - balls[b + 1], balls[a + 2] - balls[b + 2]);
            if gap * gap > dx * dx + dy * dy + dz * dz {
                if gap > 0.0 {
                    balls[b + 3] = -1.0;
                } else {
                    balls[a + 3] = -1.0;
                }
            }
        }
    }
    // A block two balls share is placed once: the blocks met are marked in a box around the try.
    s.stamp += 1;
    let stamp = s.stamp;
    let (bx, by, bz) = (ox.floor() - 16.0, oy - 16.0, oz.floor() - 16.0);
    for b in (0..n * 4).step_by(4) {
        let (cx, cy, cz, r) = (balls[b], balls[b + 1], balls[b + 2], balls[b + 3]);
        if r < 0.0 {
            continue;
        }
        let mut x = (cx - r).floor();
        while x <= (cx + r).floor() {
            let dx = (x + 0.5 - cx) / r;
            let mut y = (cy - r).floor();
            while y <= (cy + r).floor() {
                let dy = (y + 0.5 - cy) / r;
                let mut z = (cz - r).floor();
                while z <= (cz + r).floor() {
                    let dz = (z + 0.5 - cz) / r;
                    if dx * dx + dy * dy + dz * dz < 1.0 {
                        let (i, j, l) = (x - bx, y - by, z - bz);
                        let inside = i >= 0.0 && i < 32.0 && j >= 0.0 && j < 32.0 && l >= 0.0 && l < 32.0;
                        let mut fresh = true;
                        if inside {
                            let at = ((i as i32) << 10) | ((j as i32) << 5) | l as i32;
                            if s.met[at as usize] == stamp {
                                fresh = false;
                            } else {
                                s.met[at as usize] = stamp;
                            }
                        }
                        if fresh {
                            put(draw, x, y, z);
                        }
                    }
                    z += 1.0;
                }
                y += 1.0;
            }
            x += 1.0;
        }
    }
}

/// Lines 2150-2167: one rule played on one chunk, `out` called with every block of ore it places. `near_only`
/// leaves out the tries too deep to reach the counted depth.
fn play_chunk(ctx: &Ctx, play: &OnePlay, c: usize, replay: usize, near_only: bool, s: &mut Scratch, out: &mut dyn FnMut(f64, f64, f64)) {
    let listed = ctx.ground.biomes[c].as_ref().and_then(|b| ctx.section.biomes.get(b)).is_some_and(|l| l.contains(&play.rule));
    if !listed {
        return;
    }
    let mut draw = Draw::new(&format!("{}/{}/{}/{}/{}", ctx.seed, ctx.at, play.name, c, replay));
    let skip = play.skip;
    let floor = &ctx.ground.floor;
    let mut keep = |draw: &mut Draw, x: f64, y: f64, z: f64| {
        if ctx.cell_at(x, y) != 1 {
            return;
        }
        let f = floor[x as usize];
        if y < f && y >= f - EARTH_DEPTH {
            return;
        }
        // A vein in the open is often thrown away, block by block: the game looks at the six neighbours, the cut has four.
        let closed = ctx.cell_at(x - 1.0, y) != 0 && ctx.cell_at(x + 1.0, y) != 0 && ctx.cell_at(x, y - 1.0) != 0 && ctx.cell_at(x, y + 1.0) != 0;
        if skip > 0.0 && (skip >= 1.0 || draw.next() < skip) && !closed {
            return;
        }
        out(x, y, z);
    };
    for [x, y, z] in tries(&play.steps, (c * 16) as f64, &mut draw, ctx) {
        // A try far above the ground meets only air; a vein reaches 12 blocks at most from its try.
        let a = floor[index(x - 8.0, ctx.l)];
        let b = floor[index(x, ctx.l)];
        let d = floor[index(x + 8.0, ctx.l)];
        let highest = a.max(b).max(d);
        if y <= ctx.min_y || y - 12.0 > highest {
            continue;
        }
        if near_only && y + 12.0 < a.min(b).min(d) - NEAR_DEPTH {
            continue;
        }
        vein(x, y, z, play.size, &mut draw, s, &mut keep);
    }
}

/// The ores played on one cut: the figures, and the layer drawn chunk by chunk as the cut comes into sight.
#[derive(Clone, Debug)]
pub struct Play {
    pub per_chunk: [Option<OreFigure>; 8],
    /// `length * height` bytes, row-major from the top: 0 none, else the ore kind plus one.
    pub layer: Vec<u8>,
    pub drawn: Vec<bool>,
    /// The chunk plays the figures are counted over.
    pub counted: usize,
    /// Rises whenever a chunk is newly drawn, so a picture knows its layer changed.
    pub version: u32,
    plays: Vec<OnePlay>,
}

/// Lines 2041-2204. `rows` come from `rows_of_mods` only, so each has its rule; one without is skipped.
pub fn ore_play(world: &World, section: &Section, at: usize, ground: &Ground, rows: &[Row]) -> Play {
    let cells = section.cuts[at].cells.as_deref().expect("a cut that is played has its cells");
    let ctx = Ctx { seed: &world.seed, at, section, cells, ground, l: section.length as f64, min_y: section.min_y, top_y: section.top_y() };
    let compile = |placement: &[Json]| placement.iter().map(|s| Step::compile(s, ctx.min_y, ctx.top_y)).collect::<Vec<_>>();
    let config_of = |config: &Json| (number(config.get("size")), config.get("discard_chance_on_air_exposure").and_then(Json::num).unwrap_or(0.0));

    let mut plays = Vec::new();
    for rule in &section.rules {
        if rule.playable && rule.anchored.is_none() {
            let (size, skip) = config_of(&rule.config);
            plays.push(OnePlay { name: rule.id.clone(), kind: rule.kind, rule: rule.index, size, skip, steps: compile(&rule.placement) });
        }
    }
    for added in rows {
        let Some(rule) = section.rules.iter().find(|r| r.id == added.id) else { continue };
        if !(added.count > 0.0) || added.to < added.from {
            continue;
        }
        let (size, skip) = config_of(&rule.config);
        let mut steps = vec![Step::Count(added.count, added.count), Step::InSquare, Step::Heightmap(true)];
        steps.extend(compile(&steps_down(added.from, added.to)));
        plays.push(OnePlay { name: added.id.clone(), kind: added.kind, rule: rule.index, size, skip, steps });
    }

    // The figures: blocks per chunk near the ground, under low and under high ground, counted over a sample of the
    // same size whatever the length of the cut (every chunk several times over, or one in so many).
    let chunks = ground.chunks;
    let replays = round(COUNTED_CHUNKS as f64 / chunks as f64).max(1.0) as usize;
    let sample: Vec<usize> = if chunks <= COUNTED_CHUNKS {
        (0..chunks).collect()
    } else {
        (0..COUNTED_CHUNKS).map(|i| ((i as f64 + 0.5) * chunks as f64 / COUNTED_CHUNKS as f64).floor() as usize).collect()
    };
    let mut played_in = [0.0f64; 2];
    for &c in &sample {
        played_in[ground.high[c] as usize] += replays as f64;
    }
    let mut scratch = Scratch::new();
    let mut per_chunk: [Option<OreFigure>; 8] = std::array::from_fn(|_| None);
    let mut sums = [(0.0f64, 0.0f64); 8];
    for play in &plays {
        let mut near = [0.0f64; 2];
        for &c in &sample {
            for replay in 0..replays {
                play_chunk(&ctx, play, c, replay, true, &mut scratch, &mut |x, y, _| {
                    let depth = ground.floor[x as usize] - y;
                    if (1.0..=NEAR_DEPTH).contains(&depth) {
                        near[ground.high[c] as usize] += 1.0;
                    }
                });
            }
        }
        let fig = per_chunk[play.kind].get_or_insert(OreFigure { low: Some(0.0), high: Some(0.0), from: Vec::new() });
        let sum = &mut sums[play.kind];
        sum.0 += if played_in[0] != 0.0 { near[0] / played_in[0] } else { 0.0 };
        sum.1 += if played_in[1] != 0.0 { near[1] / played_in[1] } else { 0.0 };
        fig.from.push(play.name.clone());
    }
    for (kind, fig) in per_chunk.iter_mut().enumerate() {
        if let Some(f) = fig {
            f.low = (played_in[0] != 0.0).then_some(sums[kind].0);
            f.high = (played_in[1] != 0.0).then_some(sums[kind].1);
        }
    }
    Play {
        per_chunk,
        layer: vec![0; section.length * section.height],
        drawn: vec![false; chunks],
        counted: sample.len() * replays,
        version: 0,
        plays,
    }
}

impl Play {
    /// Lines 2193-2202: the ores of a thin slice across the cut, played on the chunks looked at only, as they come
    /// into sight, once each.
    pub fn draw_chunks(&mut self, world: &World, section: &Section, at: usize, ground: &Ground, from: i64, to: i64) {
        let cells = section.cuts[at].cells.as_deref().expect("a cut that is drawn has its cells");
        let ctx = Ctx { seed: &world.seed, at, section, cells, ground, l: section.length as f64, min_y: section.min_y, top_y: section.top_y() };
        let Play { layer, drawn, plays, version, .. } = self;
        // Made at the first chunk to draw: the cut is drawn at each frame, and most frames draw no new chunk.
        let mut scratch: Option<Scratch> = None;
        let (first, last) = (from.max(0), to.min(ground.chunks as i64 - 1));
        for c in first..=last {
            let c = c as usize;
            if drawn[c] {
                continue;
            }
            drawn[c] = true;
            *version += 1;
            let scratch = scratch.get_or_insert_with(Scratch::new);
            for play in plays.iter() {
                let kind = (play.kind + 1) as u8;
                play_chunk(&ctx, play, c, 0, false, scratch, &mut |x, y, z| {
                    if z >= DRAWN_SLICE.0 && z < DRAWN_SLICE.1 {
                        // A typed array drops a write whose index is not an integer inside it.
                        let i = (ctx.top_y - y) * ctx.l + x;
                        if i >= 0.0 && i < layer.len() as f64 && i.fract() == 0.0 {
                            layer[i as usize] = kind;
                        }
                    }
                });
            }
        }
    }
}

/// A cut as it is looked at: its ground and the ores played on it.
pub struct Shown {
    pub at: usize,
    pub ground: Ground,
    pub play: Play,
}

impl Shown {
    /// None without a section or without that cut's cells.
    pub fn new(world: &World, at: usize) -> Option<Shown> {
        let section = world.section.as_ref()?;
        let cut = section.cuts.get(at)?;
        let cells = cut.cells.as_ref()?;
        let ground = ground_of(world, section, cells, cut);
        let play = ore_play(world, section, at, &ground, &section.rows_of_mods());
        Some(Shown { at, ground, play })
    }

    pub fn draw_chunks(&mut self, world: &World, from: i64, to: i64) {
        let Some(section) = world.section.as_ref() else { return };
        self.play.draw_chunks(world, section, self.at, &self.ground, from, to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Biome;

    fn parse(s: &str) -> Json {
        Json::parse(s).unwrap()
    }

    #[test]
    fn the_ore_is_read_from_its_state() {
        assert_eq!(ore_kind("minecraft:deepslate_iron_ore"), Some(2));
        assert_eq!(ore_kind("minecraft:coal_ore"), Some(0));
        assert_eq!(ore_kind("minecraft:iron_ore_x"), None);
        assert_eq!(ore_kind("minecraft:stone"), None);
        assert_eq!(ore_kind("coal_ore"), None, "the regular expression wants the colon");
    }

    #[test]
    fn bounds_read_a_number_a_constant_and_a_uniform() {
        assert_eq!(bounds(&parse("5")), Some((5.0, 5.0)));
        assert_eq!(bounds(&parse(r#"{"type":"minecraft:constant","value":3}"#)), Some((3.0, 3.0)));
        assert_eq!(bounds(&parse(r#"{"type":"minecraft:uniform","min_inclusive":1,"max_inclusive":4}"#)), Some((1.0, 4.0)));
        assert_eq!(bounds(&parse(r#"{"type":"minecraft:uniform","value":{"min_inclusive":2,"max_inclusive":6}}"#)), Some((2.0, 6.0)));
        assert_eq!(bounds(&parse(r#"{"type":"minecraft:weighted_list"}"#)), None);
        assert_eq!(bounds(&parse(r#"{"type":"minecraft:uniform","min_inclusive":1}"#)), None);
        assert_eq!(bounds(&parse("null")), None);
    }

    #[test]
    fn a_depth_is_written_as_steps_of_16_at_most() {
        let steps = steps_down(8.0, 32.0);
        assert_eq!(steps.len(), 2);
        for s in &steps {
            assert_eq!(bounds(s.get("y").unwrap()), Some((-16.0, -4.0)));
            assert_eq!(s.get("x").and_then(Json::num), Some(0.0));
        }
        // A shallow depth is one step.
        assert_eq!(steps_down(0.0, 10.0).len(), 1);
        assert_eq!(steps_down(0.0, 0.0).len(), 1);
    }

    #[test]
    fn a_rule_counted_from_the_ground_is_anchored() {
        // The shape of exploration:ore/iron_near_ground: 4 tries, the ground, then 9 to 40 blocks under it.
        let placement = parse(
            r#"[{"type":"minecraft:count","count":{"type":"minecraft:uniform","min_inclusive":1,"max_inclusive":4}},
                {"type":"minecraft:in_square"},
                {"type":"minecraft:heightmap","heightmap":"OCEAN_FLOOR_WG"},
                {"type":"minecraft:offset","x":0,"y":{"type":"minecraft:uniform","min_inclusive":-16,"max_inclusive":-4},"z":0},
                {"type":"minecraft:offset","x":0,"y":{"type":"minecraft:uniform","min_inclusive":-16,"max_inclusive":-5},"z":0},
                {"type":"minecraft:biome"}]"#,
        );
        let list = placement.array().unwrap();
        assert!(playable(list));
        // Two steps down: nearest 4 + 5 = 9, deepest 16 + 16 = 32.
        assert_eq!(anchored(list), Some(Anchored { count: 4.0, from: 9.0, to: 32.0 }));
        // A height range makes it a rule of its own height.
        let ranged = parse(r#"[{"type":"minecraft:height_range","height":{"type":"minecraft:uniform","min_inclusive":{"absolute":0},"max_inclusive":{"absolute":9}}}]"#);
        assert!(playable(ranged.array().unwrap()));
        assert_eq!(anchored(ranged.array().unwrap()), None);
        // A step the page cannot play.
        assert!(!playable(parse(r#"[{"type":"minecraft:noise_based_count"}]"#).array().unwrap()));
    }

    #[test]
    fn the_draw_depends_on_its_name_alone() {
        let (mut a, mut b, mut c) = (Draw::new("seed/0/rule/3/0"), Draw::new("seed/0/rule/3/0"), Draw::new("seed/0/rule/3/1"));
        let (va, vb, vc): (Vec<f64>, Vec<f64>, Vec<f64>) = ((0..50).map(|_| a.next()).collect(), (0..50).map(|_| b.next()).collect(), (0..50).map(|_| c.next()).collect());
        assert_eq!(va, vb);
        assert_ne!(va, vc);
        assert!(va.iter().all(|v| (0.0..1.0).contains(v)));
    }

    fn world() -> World {
        World {
            seed: "1".into(),
            samples: 4,
            step: 16.0,
            origin_x: 0.0,
            origin_z: 0.0,
            sea_level: 63.0,
            heights: vec![0; 16],
            biome_at: vec![0; 16],
            biomes: vec![Biome { id: "minecraft:plains".into(), temperature: 0.8 }],
            colours: vec![[0, 0, 0]],
            flat_at: vec![0; 16],
            structures: None,
            section: None,
            biome_total: None,
            absent: Vec::new(),
            measured: Default::default(),
            made: None,
        }
    }

    #[test]
    fn the_ground_of_a_small_cut() {
        // 32 x 8, y from 0 to 7: rock under y 4 everywhere, a pool two deep over column 5, a cave at y 2 in column 9.
        let (l, h) = (32usize, 8usize);
        let mut cells = vec![0u8; l * h];
        let mut set = |x: usize, y: usize, v: u8| cells[(7 - y) * l + x] = v;
        for x in 0..l {
            for y in 0..4 {
                set(x, y, 1);
            }
        }
        set(5, 4, 2);
        set(5, 5, 2);
        set(9, 2, 0);
        set(9, 4, 1);
        let section = Section { length: l, height: h, min_y: 0.0, cuts: vec![], rules: vec![], biomes: BTreeMap::new() };
        let cut = Cut { file: String::new(), x: 0.0, z: 0.0, cells: None };
        let g = ground_of(&world(), &section, &cells, &cut);
        assert_eq!((g.surface[5], g.floor[5]), (6.0, 4.0), "water over the floor");
        assert_eq!((g.surface[9], g.floor[9]), (5.0, 5.0), "a cave does not change the floor");
        assert_eq!((g.surface[0], g.floor[0]), (4.0, 4.0));
        assert_eq!((g.top, g.bottom, g.peak), (6.0, 4.0, 5));
        assert_eq!((g.chunks, g.crop, g.rows), (2, 0, 8));
        assert_eq!(g.high, vec![0, 0]);
        assert_eq!(g.biomes, vec![Some("minecraft:plains".to_string()); 2]);
    }

    #[test]
    fn a_tall_empty_sky_is_cropped() {
        // 16 x 64: ground at y 3, topY 63: the sky above the top is 60 rows less 24 kept.
        let (l, h) = (16usize, 64usize);
        let mut cells = vec![0u8; l * h];
        for x in 0..l {
            cells[(63 - 2) * l + x] = 1;
        }
        let section = Section { length: l, height: h, min_y: 0.0, cuts: vec![], rules: vec![], biomes: BTreeMap::new() };
        let cut = Cut { file: String::new(), x: 0.0, z: 0.0, cells: None };
        let g = ground_of(&world(), &section, &cells, &cut);
        assert_eq!(g.top, 3.0);
        assert_eq!((g.crop, g.rows), (36, 28));
    }

    thread_local! {
        /// How many `Scratch` this thread made.
        pub(super) static SCRATCHES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    #[test]
    fn a_frame_that_draws_no_new_chunk_makes_no_scratch() {
        // 32 x 8, rock under y 4: two chunks, no rule to play.
        let (l, h) = (32usize, 8usize);
        let mut cells = vec![0u8; l * h];
        for x in 0..l {
            for y in 0..4 {
                cells[(7 - y) * l + x] = 1;
            }
        }
        let cut = Cut { file: String::new(), x: 0.0, z: 0.0, cells: Some(cells.clone()) };
        let section = Section { length: l, height: h, min_y: 0.0, cuts: vec![cut], rules: vec![], biomes: BTreeMap::new() };
        let ground = ground_of(&world(), &section, &cells, &section.cuts[0]);
        let mut play = Play { per_chunk: std::array::from_fn(|_| None), layer: vec![0; l * h], drawn: vec![false; ground.chunks], counted: 0, version: 0, plays: vec![] };
        let made = || SCRATCHES.with(|n| n.get());
        let start = made();
        play.draw_chunks(&world(), &section, 0, &ground, 0, 1);
        assert_eq!(made() - start, 1, "one scratch for the chunks drawn in one call");
        assert_eq!(play.drawn, vec![true; ground.chunks]);
        play.draw_chunks(&world(), &section, 0, &ground, 0, 1);
        assert_eq!(made() - start, 1, "every chunk was drawn: no scratch");
    }
}
