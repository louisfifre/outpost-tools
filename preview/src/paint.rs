//! The map's colours: the page's `paint()` (preview.html lines 1379-1449) for the five 2D views, point for
//! point, and the structures' marks (`drawMarks`, 1459-1482) as screen-space commands.

use std::borrow::Cow;
use std::collections::BTreeSet;

use mere_base::{Cmd, Color, Vec2};

use crate::bands::{HEIGHT_STEPS, SEA_DEPTHS};
use crate::ground;
use crate::colour::{SEA_VEIL, climate, height_colour, temperature_colour};
use crate::map::Look;
use crate::world::World;

/// What the map shows: the strip's first six tabs, in order.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum View {
    Height,
    Slope,
    Biomes,
    Vegetation,
    Temperature,
    Structures,
}

/// How many map views there are. The strip's pressed index goes from 0 to `MAP_VIEWS - 1` for them, and
/// `SECTION` is the one after: the section is a view of the strip but not of the map.
pub const MAP_VIEWS: usize = 6;
pub const SECTION: usize = MAP_VIEWS;

impl View {
    pub const ALL: [View; MAP_VIEWS] =
        [View::Height, View::Slope, View::Biomes, View::Vegetation, View::Temperature, View::Structures];

    pub fn from_index(i: usize) -> Option<View> {
        View::ALL.get(i).copied()
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The page's key for the view (`data-view`, `?view=`).
    pub fn key(self) -> &'static str {
        ["height", "slope", "biomes", "vegetation", "temperature", "structures"][self as usize]
    }

    pub fn from_key(key: &str) -> Option<View> {
        View::ALL.into_iter().find(|v| v.key() == key)
    }

    /// The view that tells the height gap from one point to the next, 16 blocks away: a coarser map cannot give it.
    pub fn needs_full(self) -> bool {
        self == View::Slope
    }
}

/// Whether the strip's tab `i` needs the map at the full 16-block detail (the page's `needsFull`): the views that
/// tell the steepness, the structures, and the section.
pub fn needs_full(i: usize) -> bool {
    i == SECTION || View::from_index(i).is_some_and(|v| v.needs_full() || v == View::Structures)
}

/// Whether the strip's tab `i` is made after the full map only: the structures and the section.
pub fn comes_at_the_end(i: usize) -> bool {
    i == SECTION || i == View::Structures.index()
}

/// `Math.max(low, Math.min(high, v))`.
fn clamp(v: f64, low: f64, high: f64) -> f64 {
    low.max(high.min(v))
}

/// Per point, the row of the list beside the map (`listOf`, 1326-1370) and the rows' ids. None for the
/// structures, which are counted in a list of their own and have no row per point.
pub fn rows(world: &World, view: View) -> Option<(Vec<u16>, Vec<String>)> {
    let ids = row_ids(world, view)?;
    let at = match view {
        View::Biomes => world.biome_at.clone(),
        View::Height => world.heights.iter().map(|&y| height_row(y, world.sea_level)).collect(),
        View::Temperature => {
            // The class read once per biome, not once per point.
            let band: Vec<u16> = world.biomes.iter().map(temperature_row).collect();
            world.biome_at.iter().map(|&b| band[b as usize]).collect()
        }
        View::Slope | View::Vegetation => ground::rows(world, view)?,
        View::Structures => return None,
    };
    Some((at, ids))
}

/// The row of point `i` alone and the rows' ids: what `rows(world, view)` gives at `i`, without the rest of the
/// map. A click that chooses needs only the point under it.
pub fn row_at(world: &World, view: View, i: usize) -> Option<(u16, Vec<String>)> {
    let ids = row_ids(world, view)?;
    let at = match view {
        View::Biomes => world.biome_at[i],
        View::Height => height_row(world.heights[i], world.sea_level),
        View::Temperature => temperature_row(&world.biomes[world.biome_at[i] as usize]),
        View::Slope | View::Vegetation => ground::row_at(world, view, i)?,
        View::Structures => return None,
    };
    Some((at, ids))
}

/// The ids of `view`'s rows, or None for the structures.
fn row_ids(world: &World, view: View) -> Option<Vec<String>> {
    Some(match view {
        View::Biomes => world.biomes.iter().map(|b| b.id.clone()).collect(),
        View::Height => ids(SEA_DEPTHS.len() + 1 + HEIGHT_STEPS.len() + 1),
        View::Temperature => ids(5),
        View::Slope | View::Vegetation => ids(ground::colours(view).len()),
        View::Structures => return None,
    })
}

/// The rows go up from the deepest floor to the highest ground.
fn height_row(y: i16, sea: f64) -> u16 {
    let y = y as f64;
    let mut band = 0;
    if y >= sea {
        while band < HEIGHT_STEPS.len() && y >= HEIGHT_STEPS[band] {
            band += 1;
        }
        (SEA_DEPTHS.len() + 1 + band) as u16
    } else {
        while band < SEA_DEPTHS.len() && sea - y > SEA_DEPTHS[band] {
            band += 1;
        }
        (SEA_DEPTHS.len() - band) as u16
    }
}

/// Five classes over the game's range, from -0.7 to 2, the sea in them like the land.
fn temperature_row(b: &crate::world::Biome) -> u16 {
    (clamp((climate(b) + 0.7) / 2.7, 0.0, 1.0) * 4.0 + 0.5).floor() as u16
}

fn ids(n: usize) -> Vec<String> {
    (0..n).map(|i| i.to_string()).collect()
}

/// The map as `samples` x `samples` RGBA bytes. `chosen` holds the ids chosen in this view: when it holds one
/// of the rows' ids, what is not chosen steps back towards the ground. `dark` picks the theme's step-back greys.
pub fn paint(world: &World, view: View, dark: bool, chosen: &BTreeSet<String>) -> Vec<u8> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8);
    paint_in(world, view, dark, chosen, threads)
}

/// `paint` on `threads` threads, each painting a band of whole rows. Each point is painted from the world alone, by
/// the same code whatever its band, so the picture is the same bytes on any number of threads.
fn paint_in(world: &World, view: View, dark: bool, chosen: &BTreeSet<String>, threads: usize) -> Vec<u8> {
    let n = world.samples;
    let total = n * n;
    let marks = view == View::Structures;
    // The choice of this view only. A chosen row this world does not have dims nothing. The biomes' rows are the
    // world's own list, read where it is.
    let list: Option<(Cow<[u16]>, Vec<String>)> = match view {
        View::Structures => None,
        View::Biomes => row_ids(world, view).map(|ids| (Cow::Borrowed(world.biome_at.as_slice()), ids)),
        _ => rows(world, view).map(|(at, ids)| (Cow::Owned(at), ids)),
    };
    let lit: Vec<bool> = list.as_ref().map_or(Vec::new(), |(_, ids)| ids.iter().map(|id| chosen.contains(id)).collect());
    let row_at = list.as_ref().map(|(at, _)| at.as_ref());
    let ground_colours = ground::colours(view);
    let point = Point { world, view, dark, marks, choosing: lit.contains(&true), lit: &lit, row_at, ground_colours: &ground_colours };

    let mut px = vec![0u8; total * 4];
    if n == 0 {
        return px;
    }
    let band_rows = n.div_ceil(threads.max(1));
    std::thread::scope(|scope| {
        for (b, band) in px.chunks_mut(band_rows * n * 4).enumerate() {
            let point = &point;
            scope.spawn(move || {
                let first = b * band_rows * n;
                for (k, out) in band.chunks_exact_mut(4).enumerate() {
                    out.copy_from_slice(&point.paint(first + k));
                }
            });
        }
    });
    px
}

/// What every point of one picture is painted from.
struct Point<'a> {
    world: &'a World,
    view: View,
    dark: bool,
    marks: bool,
    choosing: bool,
    lit: &'a [bool],
    row_at: Option<&'a [u16]>,
    ground_colours: &'a [[u8; 3]],
}

impl Point<'_> {
    /// The RGBA of point `i`.
    fn paint(&self, i: usize) -> [u8; 4] {
        let Point { world, view, dark, marks, choosing, lit, row_at, ground_colours } = *self;
        let n = world.samples;
        let total = n * n;
        let h = &world.heights;
        let sea = world.sea_level;
        let at = &world.biome_at;
        // What is not a chosen biome steps back towards the ground: water, then land.
        let step_back: [f64; 2] = if dark { [16.0, 40.0] } else { [200.0, 226.0] };
        // The line between two zones that stepped back: a little towards the text's side of the ground.
        let faded_line = if dark { 1.3 } else { 0.92 };
        let biome = at[i] as usize;
        let y = h[i] as f64;
        let x = i % n;
        let water = y < sea;
        // Light from the north-west: slopes facing it are brighter. Water stays flat, but in the Relief view,
        // where the sea floor is ground like the rest.
        let mut slope = 0.0;
        if !water || view == View::Height {
            let west = if x > 0 { h[i - 1] as f64 } else { y };
            let east = if x < n - 1 { h[i + 1] as f64 } else { y };
            let north = if i >= n { h[i - n] as f64 } else { y };
            let south = if i < total - n { h[i + n] as f64 } else { y };
            slope = (east - west) + (south - north);
        }
        // Where the biome changes. Not between two kinds of sea, which would only add noise.
        let right = if x < n - 1 { i + 1 } else { i };
        let below = if i < total - n { i + n } else { i };
        let border = (at[right] != at[i] && !(water && (h[right] as f64) < sea)) || (at[below] != at[i] && !(water && (h[below] as f64) < sea));
        let mut shade = 1.0;
        let c: [f64; 3];
        if choosing && !lit[row_at.expect("a view with rows")[i] as usize] {
            let g = step_back[if water { 0 } else { 1 }];
            c = [g, g, g];
            shade = clamp(1.0 + slope * 0.004, 0.9, 1.1);
            // What stepped back keeps a trace of its zones, so a chosen biome is still read among its neighbours.
            if border && view == View::Biomes {
                shade *= faded_line;
            }
        } else if marks {
            // The land stepped back, its relief a little stronger than a choice leaves it, to place the marks on.
            let g = step_back[if water { 0 } else { 1 }];
            c = [g, g, g];
            shade = if water { 1.0 } else { clamp(1.0 + slope * 0.008, 0.82, 1.18) };
        } else if view == View::Height {
            c = as_f64(height_colour(y, sea));
            shade = clamp(1.0 + slope * 0.012, 0.75, 1.25);
        } else if view == View::Temperature {
            let t = as_f64(temperature_colour(climate(&world.biomes[biome])));
            c = if water { [0, 1, 2].map(|k| (t[k] + SEA_VEIL[k] as f64) / 2.0) } else { t };
            shade = clamp(1.0 + slope * 0.006, 0.85, 1.15);
        } else if matches!(view, View::Slope | View::Vegetation) {
            c = as_f64(ground_colours[row_at.expect("a view with rows")[i] as usize]);
            // Vegetation keeps a trace of the relief; Steepness is a flat colour.
            if view == View::Vegetation && !water {
                shade = clamp(1.0 + slope * 0.006, 0.85, 1.15);
            }
        } else {
            c = as_f64(world.colours[biome]);
            // Here the lightness says which biome it is, so the relief may only touch it lightly.
            shade = clamp(1.0 + slope * 0.006, 0.85, 1.15);
            // A dark line where the biome changes: two neighbours of one colour family stay two zones. Lighter
            // on a pale biome, where it would turn into a heavy grey maze; stronger on a dark one.
            if border {
                shade *= if c[0] + c[1] + c[2] > 420.0 { 0.75 } else { 0.5 };
            }
        }
        // A canvas pixel array (Uint8ClampedArray) clamps, then rounds half to even.
        let byte = |k: usize| (c[k] * shade).round_ties_even().clamp(0.0, 255.0) as u8;
        [byte(0), byte(1), byte(2), 255]
    }
}

fn as_f64(c: [u8; 3]) -> [f64; 3] {
    c.map(|v| v as f64)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::world::Biome;

    /// `n` a side, sea at 63: heights from deep sea to high peaks with walls, a mix of biomes (water ones among
    /// them) and of flat points, so every row of every view is met.
    pub(crate) fn world(n: usize) -> World {
        let ids = ["minecraft:ocean", "minecraft:plains", "minecraft:desert", "minecraft:jungle", "minecraft:birch_forest", "minecraft:river", "minecraft:snowy_peaks"];
        let temperatures = [0.5, 0.8, 2.0, 0.95, 0.6, 0.5, -0.7];
        let total = n * n;
        // A deterministic scramble, so neighbours differ by a little or by a wall.
        let noise = |i: usize| ((i as u64).wrapping_mul(2654435761) >> 7) % 97;
        let heights: Vec<i16> = (0..total)
            .map(|i| {
                let (x, z) = ((i % n) as i64, (i / n) as i64);
                (x * 5 + z * 3 + noise(i) as i64 / 8 - 40 + if (x / 4) % 3 == 0 { 60 } else { 0 }) as i16
            })
            .collect();
        World {
            seed: String::new(),
            samples: n,
            step: 16.0,
            origin_x: 0.0,
            origin_z: 0.0,
            sea_level: 63.0,
            biome_at: (0..total).map(|i| (noise(i * 3 + 1) % ids.len() as u64) as u16).collect(),
            flat_at: (0..total).map(|i| (noise(i + 5) % 2) as u8).collect(),
            heights,
            biomes: ids.iter().zip(temperatures).map(|(id, t)| Biome { id: id.to_string(), temperature: t }).collect(),
            colours: (0..ids.len()).map(|k| [(30 * k) as u8, (200 - 20 * k) as u8, (90 + 15 * k) as u8]).collect(),
            structures: None,
            section: None,
            biome_total: None,
            absent: Vec::new(),
            measured: Default::default(),
            made: None,
        }
    }

    #[test]
    fn the_picture_is_the_same_bytes_on_any_number_of_threads() {
        // 37 a side: no band count divides it, so the bands' edges fall anywhere.
        let w = world(37);
        let none = BTreeSet::new();
        for view in View::ALL {
            let some: BTreeSet<String> = rows(&w, view).map_or(BTreeSet::new(), |(at, ids)| [ids[at[0] as usize].clone()].into());
            for dark in [false, true] {
                for chosen in [&none, &some] {
                    let one = paint_in(&w, view, dark, chosen, 1);
                    assert_eq!(one.len(), 37 * 37 * 4);
                    for threads in [2, 3, 4, 8, 64] {
                        assert!(paint_in(&w, view, dark, chosen, threads) == one, "{view:?} dark {dark} chosen {chosen:?} on {threads} threads");
                    }
                }
            }
        }
    }

    #[test]
    fn row_at_gives_what_rows_gives_at_every_point_of_every_view() {
        let w = world(24);
        for view in View::ALL {
            let Some((at, ids)) = rows(&w, view) else {
                assert_eq!(view, View::Structures);
                assert!(row_at(&w, view, 0).is_none());
                continue;
            };
            let met: BTreeSet<u16> = at.iter().copied().collect();
            assert!(met.len() > 1, "{view:?}: the world meets one row only, the test would prove little");
            for i in 0..at.len() {
                assert_eq!(row_at(&w, view, i), Some((at[i], ids.clone())), "{view:?} at point {i}");
            }
        }
    }
}

/// The marks of the chosen structure kinds, in the frame's own coordinates (0,0 its corner, `side` its width in
/// DIP), for the look. The page draws them into a 2048 px canvas that the look then scales; here they are drawn
/// on the screen: a mark keeps about its size however close the map is looked at, growing a little as it comes
/// closer: radius 3.5 * k^0.2, which is the page's `3.5 * pixel` with `pixel = 2048 / shown / k^0.8` seen
/// through the canvas's scale. Each kind: all its discs, then all its rings (a dark ring keeps a pale mark
/// apart from pale ground), the most frequent kinds first so that the rare ones lie over them.
pub fn marks(world: &World, chosen: &BTreeSet<String>, look: Look, side: f32) -> Vec<Cmd> {
    let Some(s) = &world.structures else { return Vec::new() };
    let k = look.k as f64;
    let radius = (3.5 * k.powf(0.2)) as f32;
    let width = k.powf(0.2) as f32;
    let span = world.span();
    let mut out = Vec::new();
    for &kind in &s.order {
        if !chosen.contains(&s.ids[kind]) {
            continue;
        }
        let [r, g, b] = s.colours[kind];
        let fill = Color::rgb8(r, g, b);
        let centre = |&(x, z): &(f64, f64)| {
            Vec2::new(
                (look.x as f64 * side as f64 + (x - world.origin_x) / span * side as f64 * k) as f32,
                (look.y as f64 * side as f64 + (z - world.origin_z) / span * side as f64 * k) as f32,
            )
        };
        for p in &s.by_kind[kind] {
            out.push(Cmd::Disc { center: centre(p), radius, color: fill });
        }
        // `rgb(0 0 0 / 0.7)`. One stroke a ring here, where the page strokes one path a kind, so two
        // overlapping rings darken twice: UNVERIFIED against a capture.
        for p in &s.by_kind[kind] {
            out.push(Cmd::Ring { center: centre(p), radius, width, color: Color::hex(0).with_alpha(0.7) });
        }
    }
    out
}
