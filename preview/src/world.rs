//! A run's output folder, read the way the page reads it (preview.html `loadWorld` 1142-1167, `loadStructures`
//! 1171-1185, `measure` 1255-1322 for the level and flat land, `loadSection` 1975-1982 for the cut).

use std::path::{Path, PathBuf};

use crate::colour::{biome_colour, structure_colour, underground};
use crate::figures::{Measured, measure};
use crate::json::Json;
use crate::section::Section;

/// The environment variable that names a world preview's out folder: the default of `--data` and the world the
/// tests read. A folder of one machine never lives in the code.
pub const DATA_VAR: &str = "PREVIEW_DATA_DIR";

/// The out folder `PREVIEW_DATA_DIR` names, or an empty path, which holds no `preview.json`, when it is not set.
pub fn data_dir() -> PathBuf {
    std::env::var_os(DATA_VAR).map(PathBuf::from).unwrap_or_default()
}

#[derive(Clone, Debug)]
pub struct Biome {
    pub id: String,
    pub temperature: f64,
}

/// Where each structure would start, as the generator places them (structures.json).
#[derive(Clone, Debug)]
pub struct Structures {
    pub ids: Vec<String>,
    pub colours: Vec<[u8; 3]>,
    /// Built under the ground rather than on the surface.
    pub under: Vec<bool>,
    /// How many points each kind has.
    pub count: Vec<u32>,
    /// (kind, x, z) of every start whose kind is in range.
    pub points: Vec<(usize, f64, f64)>,
    pub by_kind: Vec<Vec<(f64, f64)>>,
    /// The kinds that have points, the most frequent first: the rare ones are drawn over them.
    pub order: Vec<usize>,
}

/// What a run was asked for, read from `run.json` (`loadWorld`, lines 1150-1163): the seed and the Tectonic
/// values that made the map. Missing for a folder the generator made before it wrote the file.
#[derive(Clone, Debug, PartialEq)]
pub struct Made {
    pub seed: String,
    pub tectonic: Vec<(String, f64)>,
}

#[derive(Clone, Debug)]
pub struct World {
    pub seed: String,
    /// Points on a side of the square grid.
    pub samples: usize,
    /// Blocks between two points.
    pub step: f64,
    pub origin_x: f64,
    pub origin_z: f64,
    pub sea_level: f64,
    pub heights: Vec<i16>,
    pub biome_at: Vec<u16>,
    pub biomes: Vec<Biome>,
    /// One colour per biome (`colour::biome_colour`).
    pub colours: Vec<[u8; 3]>,
    /// 1 where the land is flat, 0 elsewhere.
    pub flat_at: Vec<u8>,
    pub structures: Option<Structures>,
    /// The section: its cuts and ore rules, when section.json lists cuts and ores.json parses (`Section::load`).
    pub section: Option<Section>,
    /// The page's total of biomes: those met plus the surface ones the generator can place that this area
    /// lacks. None when `preview.json` does not list what the generator can place.
    pub biome_total: Option<usize>,
    /// The surface biomes the generator can place that this area does not hold, in the file's order (bsent,
    /// line 1307).
    pub absent: Vec<String>,
    /// What measure gives: the figures, and the lists beside the map.
    pub measured: Measured,
    /// The run that made this folder, when `run.json` says.
    pub made: Option<Made>,
}

impl World {
    /// Blocks across the whole area.
    pub fn span(&self) -> f64 {
        self.samples as f64 * self.step
    }

    /// A map stopped at a coarser detail: level land is told 16 blocks apart, so some views are not offered.
    pub fn coarse(&self) -> bool {
        self.step > 16.0
    }

    pub fn load(dir: &Path) -> Result<World, String> {
        let text = String::from_utf8(std::fs::read(dir.join("preview.json")).map_err(|e| format!("{}: {e}", dir.join("preview.json").display()))?)
            .map_err(|_| "preview.json is not UTF-8".to_string())?;
        World::load_grid(dir, &text, "", true, None)
    }

    /// The last sketch a run under way wrote (`loadSketch`, lines 1187-1200): `coarse.json` and the two files its step
    /// names, so a finer sketch landing meanwhile is not paired with them. Only drawn: it has no cuts, no structures
    /// and no run record, and its figures, measured on a coarser grid, are not shown.
    pub fn load_sketch(dir: &Path) -> Result<World, String> {
        let text = String::from_utf8(std::fs::read(dir.join("coarse.json")).map_err(|e| format!("{}: {e}", dir.join("coarse.json").display()))?)
            .map_err(|_| "coarse.json is not UTF-8".to_string())?;
        let step = Json::parse(&text)?.get("step").and_then(Json::num).ok_or("coarse.json has no step")?;
        World::load_grid(dir, &text, &format!("coarse-{step}-"), false, None)
    }

    /// The map's own points known so far, over the sketch `base` (`loadDraft`, lines 1202-1231): `partial.json` and
    /// its two files, which the engine writes during the map's pass. A point the pass has not reached yet (biome
    /// 0xFFFF) takes the sketch point it falls in. Only drawn, like a sketch. Gives the tiles done with it.
    pub fn load_partial(dir: &Path, base: &World) -> Result<(World, u32), String> {
        let text = String::from_utf8(std::fs::read(dir.join("partial.json")).map_err(|e| format!("{}: {e}", dir.join("partial.json").display()))?)
            .map_err(|_| "partial.json is not UTF-8".to_string())?;
        let done = Json::parse(&text)?.get("done").and_then(Json::num).ok_or("partial.json has no done")?;
        Ok((World::load_grid(dir, &text, "partial-", false, Some(base))?, done as u32))
    }

    /// A grid's description (`text`) and its two files, `<prefix>heights.bin` and `<prefix>biomes.bin`; with `full`,
    /// what comes beside a map as well: the structures, the section and the run record. With `under`, the grid is a
    /// partial map whose points not sampled yet are taken from that sketch.
    fn load_grid(dir: &Path, text: &str, prefix: &str, full: bool, under: Option<&World>) -> Result<World, String> {
        let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| format!("{}: {e}", dir.join(name).display()));
        let json = Json::parse(text)?;
        let number = |key: &str| json.get(key).and_then(Json::num).ok_or_else(|| format!("the grid has no number \"{key}\""));

        let samples = number("samples")? as usize;
        let step = number("step")?;
        let n = samples * samples;
        let heights_raw = read(&format!("{prefix}heights.bin"))?;
        let biomes_raw = read(&format!("{prefix}biomes.bin"))?;
        if samples < 2 || heights_raw.len() != n * 2 || biomes_raw.len() != n * 2 {
            return Err(format!(
                "{samples} samples a side need {} bytes in heights.bin and biomes.bin, found {} and {}",
                n * 2,
                heights_raw.len(),
                biomes_raw.len()
            ));
        }
        let mut biomes = Vec::new();
        for b in json.get("biomes").and_then(Json::array).ok_or("preview.json has no \"biomes\" list")? {
            let id = b.get("id").and_then(Json::text).ok_or("a biome of preview.json has no id")?;
            biomes.push(Biome { id, temperature: b.get("temperature").and_then(Json::num).unwrap_or(0.0) });
        }
        // As the page counts them: a biome that lives under the ground only would be called missing for ever.
        let possible = json.get("possible").and_then(Json::array);
        let absent: Vec<String> = possible
            .map(|list| {
                list.iter()
                    .filter(|b| !b.get("underground").and_then(Json::boolean).unwrap_or(false))
                    .filter_map(|b| b.get("id").and_then(Json::text))
                    .filter(|id| !biomes.iter().any(|m| &m.id == id))
                    .collect()
            })
            .unwrap_or_default();
        let biome_total = possible.map(|_| biomes.len() + absent.len());
        let mut heights: Vec<i16> = heights_raw.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        let mut biome_at: Vec<u16> = biomes_raw.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        if let Some(base) = under {
            // Lines 1212-1226: the sketch must cover the same square, a whole number of the map's points to each of its own.
            let ratio = base.step / step;
            if base.samples as f64 * ratio != samples as f64 {
                return Err(format!("the sketch of {} points every {} blocks does not cover the partial map of {samples} every {step}", base.samples, base.step));
            }
            // The sketch's biomes renumbered into the partial map's list (every biome the generator can place).
            let renumber: Vec<u16> = base.biomes.iter().map(|b| biomes.iter().position(|m| m.id == b.id).unwrap_or(0) as u16).collect();
            for row in 0..samples {
                for col in 0..samples {
                    let i = row * samples + col;
                    // 0xFFFF: a point not sampled yet.
                    if biome_at[i] != 0xFFFF {
                        continue;
                    }
                    let j = (row as f64 / ratio).floor() as usize * base.samples + (col as f64 / ratio).floor() as usize;
                    heights[i] = base.heights[j];
                    biome_at[i] = renumber[base.biome_at[j] as usize];
                }
            }
        }
        if let Some((i, &b)) = biome_at.iter().enumerate().find(|(_, b)| **b as usize >= biomes.len()) {
            return Err(format!("biomes.bin point {i} names biome {b}, but preview.json lists {}", biomes.len()));
        }
        let sea_level = number("seaLevel")?;
        let colours = biomes.iter().map(|b| biome_colour(&b.id)).collect();
        let flat_at = flat_land(&heights, samples, sea_level);
        let mut world = World {
            seed: json.get("seed").and_then(Json::text).unwrap_or_default(),
            samples,
            step,
            origin_x: number("originX")?,
            origin_z: number("originZ")?,
            sea_level,
            heights,
            biome_at,
            biomes,
            colours,
            flat_at,
            structures: if full { load_structures(dir) } else { None },
            section: if full { Section::load(dir) } else { None },
            biome_total,
            absent: Vec::new(),
            measured: Measured::default(),
            made: if full { load_made(dir) } else { None },
        };
        world.measured = measure(&world, absent.clone(), possible.is_some());
        world.absent = absent;
        Ok(world)
    }

    /// Lowest and highest ground; (0, 0) for an empty grid.
    pub fn height_range(&self) -> (i16, i16) {
        let lo = self.heights.iter().copied().min().unwrap_or(0);
        let hi = self.heights.iter().copied().max().unwrap_or(0);
        (lo, hi)
    }
}

/// `measure`, lines 1265-1293: the level land (the ground moves by 2 blocks at most to each neighbour point,
/// a neighbour under water not counting), then the flat land, level ground among level ground: at least 5 of
/// the 9 points around.
fn flat_land(heights: &[i16], n: usize, sea: f64) -> Vec<u8> {
    let total = n * n;
    let mut level = vec![0u8; total];
    for i in 0..total {
        let y = heights[i] as f64;
        if y < sea {
            continue;
        }
        let x = i % n;
        let gap = |j: usize| if heights[j] as f64 >= sea { (y - heights[j] as f64).abs() } else { 0.0 };
        let d = [
            if x > 0 { gap(i - 1) } else { 0.0 },
            if x < n - 1 { gap(i + 1) } else { 0.0 },
            if i >= n { gap(i - n) } else { 0.0 },
            if i < total - n { gap(i + n) } else { 0.0 },
        ]
        .into_iter()
        .fold(0.0, f64::max);
        if d <= 2.0 {
            level[i] = 1;
        }
    }
    let mut flat = vec![0u8; total];
    for i in 0..total {
        if (heights[i] as f64) < sea {
            continue;
        }
        let x = i % n;
        let mut around = 0;
        for dz in [-(n as isize), 0, n as isize] {
            for dx in -1isize..=1 {
                let j = i as isize + dz + dx;
                if j >= 0 && (j as usize) < total && x as isize + dx >= 0 && (x as isize + dx) < n as isize {
                    around += level[j as usize] as u32;
                }
            }
        }
        if around >= 5 {
            flat[i] = 1;
        }
    }
    flat
}

fn read_json(dir: &Path, name: &str) -> Option<Json> {
    let text = std::fs::read(dir.join(name)).ok()?;
    Json::parse(std::str::from_utf8(&text).ok()?).ok()
}

/// `run.json`: the seed (a text, or a number if the generator wrote one) and the values as numbers. A file that is
/// missing or does not parse is no run to speak of, not an error: the form then keeps what it has.
fn load_made(dir: &Path) -> Option<Made> {
    let data = read_json(dir, "run.json")?;
    let seed = data.get("seed")?.text()?;
    let Json::Obj(fields) = data.get("tectonic")? else { return None };
    let tectonic = fields.iter().filter_map(|(k, v)| Some((k.clone(), v.num()?))).collect();
    Some(Made { seed, tectonic })
}

/// Lines 1171-1185. Nothing when the file is missing or lacks either array: older previews have none.
fn load_structures(dir: &Path) -> Option<Structures> {
    let data = read_json(dir, "structures.json")?;
    let kinds_json = data.get("structures")?.array()?;
    let points_json = data.get("points")?.array()?;
    let kinds = kinds_json.len();
    let mut ids = Vec::new();
    let mut colours = Vec::new();
    let mut under = Vec::new();
    for s in kinds_json {
        let id = s.get("id").and_then(Json::text).unwrap_or_default();
        let set = s.get("set").and_then(Json::text);
        let step = s.get("step").and_then(Json::text);
        colours.push(structure_colour(&id, set.as_deref()));
        under.push(underground(&id, step.as_deref()));
        ids.push(id);
    }
    let mut count = vec![0u32; kinds];
    let mut points = Vec::new();
    let mut by_kind = vec![Vec::new(); kinds];
    for p in points_json {
        let Some([k, x, z]) = p.array().and_then(|a| Some([a.first()?.num()?, a.get(1)?.num()?, a.get(2)?.num()?])) else { continue };
        // Line 1175: a kind outside the list is dropped.
        if k < 0.0 || k >= kinds as f64 || k.fract() != 0.0 {
            continue;
        }
        let k = k as usize;
        count[k] += 1;
        by_kind[k].push((x, z));
        points.push((k, x, z));
    }
    let mut order: Vec<usize> = (0..kinds).filter(|&i| count[i] > 0).collect();
    // sort_by is stable, as Array.prototype.sort is.
    order.sort_by(|&a, &b| count[b].cmp(&count[a]));
    Some(Structures { ids, colours, under, count, points, by_kind, order })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_folder_is_an_error_not_a_panic() {
        assert!(World::load(Path::new(r"no\such\folder")).is_err());
    }

    #[test]
    fn reads_the_real_out_folder_when_it_is_there() {
        let dir = &data_dir();
        if !dir.join("preview.json").exists() {
            println!("skipped: PREVIEW_DATA_DIR names no out folder");
            return;
        }
        let w = World::load(dir).unwrap();
        assert_eq!((w.samples, w.step, w.origin_x, w.origin_z, w.sea_level), (256, 16.0, -2048.0, -2048.0, 63.0));
        assert_eq!(w.span(), 4096.0);
        assert!(!w.coarse());
        assert_eq!((w.biomes.len(), w.colours.len()), (25, 25));
        let s = w.structures.as_ref().expect("structures.json is there");
        assert_eq!((s.ids.len(), s.points.len()), (75, 655));
        assert!(s.order.windows(2).all(|p| s.count[p[0]] >= s.count[p[1]]));
        let section = w.section.as_ref().expect("section.json and ores.json are there");
        assert_eq!(section.cuts[0].z, -1504.0);
        assert_eq!((section.cuts.len(), section.length, section.height, section.min_y), (4, 4096, 384, -64.0));
        assert!(section.cuts.iter().all(|c| c.cells.is_some()), "every cut's cells are read");
        assert!(w.flat_at.iter().any(|&f| f == 1));
    }

    #[test]
    fn a_sketch_is_read_from_the_files_its_step_names() {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/tmp")).join("world_unit_sketch");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(World::load_sketch(&dir).is_err(), "no sketch before the first pass");
        let grid = |samples: usize, step: u32| {
            format!(r#"{{"seed":"s","samples":{samples},"step":{step},"originX":-64,"originZ":-64,"seaLevel":63,"biomes":[{{"id":"minecraft:plains","temperature":0.8}}]}}"#)
        };
        let bins = |prefix: &str, samples: usize, height: i16| {
            std::fs::write(dir.join(format!("{prefix}heights.bin")), height.to_le_bytes().repeat(samples * samples)).unwrap();
            std::fs::write(dir.join(format!("{prefix}biomes.bin")), vec![0u8; samples * samples * 2]).unwrap();
        };
        // The sketch of 64 blocks, then a full map's files and a run record beside it, which a sketch does not take.
        bins("coarse-64-", 2, 70);
        bins("", 8, 90);
        std::fs::write(dir.join("coarse.json"), grid(2, 64)).unwrap();
        std::fs::write(dir.join("run.json"), r#"{"seed":"s","tectonic":{}}"#).unwrap();
        let w = World::load_sketch(&dir).unwrap();
        assert_eq!((w.samples, w.step, w.heights[0], w.made.is_none(), w.structures.is_none(), w.section.is_none()), (2, 64.0, 70, true, true, true));
        // A finer sketch's description landed before its files: what it names is not there yet.
        std::fs::write(dir.join("coarse.json"), grid(4, 32)).unwrap();
        assert!(World::load_sketch(&dir).is_err());
        bins("coarse-32-", 4, 75);
        assert_eq!(World::load_sketch(&dir).unwrap().heights[0], 75);
    }

    #[test]
    fn a_partial_map_takes_its_unsampled_points_from_the_sketch() {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/tmp")).join("world_unit_partial");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let grid = |samples: usize, step: u32, biomes: &str, extra: &str| {
            format!(r#"{{"seed":"s","samples":{samples},"step":{step},"originX":-64,"originZ":-64,"seaLevel":63,"biomes":[{biomes}]{extra}}}"#)
        };
        // The sketch: 2 x 2 every 64 blocks, the plains on its west column and the desert on its east one.
        std::fs::write(dir.join("coarse.json"), grid(2, 64, r#"{"id":"minecraft:plains"},{"id":"minecraft:desert"}"#, "")).unwrap();
        std::fs::write(dir.join("coarse-64-heights.bin"), [70i16, 80, 70, 80].iter().flat_map(|h| h.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
        std::fs::write(dir.join("coarse-64-biomes.bin"), [0u16, 1, 0, 1].iter().flat_map(|b| b.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
        let sketch = World::load_sketch(&dir).unwrap();
        assert!(World::load_partial(&dir, &sketch).is_err(), "no partial map before the map's pass");
        // The partial map: 8 x 8 every 16 blocks, its list in another order; its first row sampled, the rest not.
        let list = r#"{"id":"minecraft:desert"},{"id":"minecraft:ocean"},{"id":"minecraft:plains"}"#;
        std::fs::write(dir.join("partial.json"), grid(8, 16, list, r#","done":1,"total":4"#)).unwrap();
        let heights: Vec<i16> = (0..64).map(|i| if i < 8 { 100 } else { 0 }).collect();
        let biomes: Vec<u16> = (0..64).map(|i| if i < 8 { 1 } else { 0xFFFF }).collect();
        std::fs::write(dir.join("partial-heights.bin"), heights.iter().flat_map(|h| h.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
        std::fs::write(dir.join("partial-biomes.bin"), biomes.iter().flat_map(|b| b.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
        let (w, done) = World::load_partial(&dir, &sketch).unwrap();
        assert_eq!((w.samples, w.step, done, w.biomes.len()), (8, 16.0, 1, 3));
        assert_eq!((w.heights[3], w.biome_at[3]), (100, 1), "a sampled point is the map's own");
        assert_eq!((w.heights[8], w.biome_at[8], w.heights[15], w.biome_at[15]), (70, 2, 80, 0), "the others are the sketch's, renumbered");
        assert_eq!((w.heights[63], w.biomes[w.biome_at[63] as usize].id.as_str()), (80, "minecraft:desert"));
        assert!(w.made.is_none() && w.structures.is_none() && w.section.is_none());
        // A sketch of another square does not fill it.
        let other = World { samples: 3, ..sketch.clone() };
        assert!(World::load_partial(&dir, &other).is_err());
        // Without a sketch under it, 0xFFFF names no biome: the map's own loader still refuses it.
        std::fs::copy(dir.join("partial.json"), dir.join("preview.json")).unwrap();
        std::fs::copy(dir.join("partial-heights.bin"), dir.join("heights.bin")).unwrap();
        std::fs::copy(dir.join("partial-biomes.bin"), dir.join("biomes.bin")).unwrap();
        assert!(World::load(&dir).is_err());
    }

    #[test]
    fn a_flat_plain_is_flat_and_a_cliff_is_not() {
        // 4 x 4: the left half at 70, a wall to 90 on the right half; the sea is 63.
        let mut h = vec![70i16; 16];
        for i in 0..16 {
            if i % 4 >= 2 {
                h[i] = 90;
            }
        }
        let f = flat_land(&h, 4, 63.0);
        // Points on the cliff's two sides have a neighbour 20 away: not level, so not flat.
        assert_eq!(f[1], 0);
        assert_eq!(f[2], 0);
        // A corner of the high ground has 4 level points around it (itself and 3), which is not 5.
        assert_eq!(f[3], 0);
        // The sea is never flat land.
        assert_eq!(flat_land(&[10; 16], 4, 63.0), vec![0; 16]);
        // Open flat ground: an inner point has all 9 around it level.
        assert_eq!(flat_land(&[70; 16], 4, 63.0)[5], 1);
    }
}
