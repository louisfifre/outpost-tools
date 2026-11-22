//! The Steepness, Cliffs and Vegetation maps: ported from preview.html's `steepness()` and the `slope`, `cliffs` and
//! `vegetation` branches of `bands()` (world-preview's page), so that the list and the map give what the page's do.
//! Each gives, per point, the row of its list; the names and the hovers are `text::LEGEND_*`.
//!
//! They read the map's own grid, one point every 16 blocks, so none of them is told on a coarser map.

use crate::colour::{flat_sea_colour, hsl_to_rgb, is_water};
use crate::paint::View;
use crate::world::World;

/// Where one row of steepness ends and the next starts, in blocks of height between two points 16 apart.
pub const SLOPE_EDGES: [f64; 4] = [2.0, 5.0, 10.0, 16.0];
/// A drop of more than this between two points 16 apart is a wall.
pub const WALL: f64 = 10.0;
/// How many points away a wall still counts as beside a flat point (32 blocks): the ground right at its foot is
/// itself tilted, so it is rarely flat by the Flat view's test.
pub const REACH: isize = 2;

/// The biggest height gap, in blocks, from a land point to the four points around it. A neighbour under water does
/// not count: a river bed would make every bank a slope.
pub fn steepness(heights: &[i16], n: usize, sea: f64, i: usize) -> f64 {
    let total = n * n;
    let (x, y) = (i % n, heights[i] as f64);
    let gap = |j: usize| if heights[j] as f64 >= sea { (y - heights[j] as f64).abs() } else { 0.0 };
    [
        if x > 0 { gap(i - 1) } else { 0.0 },
        if x < n - 1 { gap(i + 1) } else { 0.0 },
        if i >= n { gap(i - n) } else { 0.0 },
        if i < total - n { gap(i + n) } else { 0.0 },
    ]
    .into_iter()
    .fold(0.0, f64::max)
}

/// The row of each point in `view` (Slope, Cliffs or Vegetation), or None for another view.
pub fn rows(world: &World, view: View) -> Option<Vec<u16>> {
    let n = world.samples;
    let total = n * n;
    let (h, sea) = (&world.heights, world.sea_level);
    let water = |i: usize| (h[i] as f64) < sea;
    Some(match view {
        // Level, gentle, sloping, steep, wall, then water.
        View::Slope => (0..total)
            .map(|i| {
                if water(i) {
                    return 5;
                }
                let gap = steepness(h, n, sea, i);
                SLOPE_EDGES.iter().position(|&edge| gap <= edge).unwrap_or(SLOPE_EDGES.len()) as u16
            })
            .collect(),
        // Flat at the foot of a wall, wall, flat on top of a wall, other land, water.
        View::Cliffs => (0..total)
            .map(|i| {
                if water(i) {
                    return 4;
                }
                let flat = world.flat_at[i] == 1;
                if !flat && steepness(h, n, sea, i) <= WALL {
                    return 3;
                }
                let x = (i % n) as isize;
                let (mut below, mut above) = (false, false);
                for dz in -REACH..=REACH {
                    for dx in -REACH..=REACH {
                        let j = i as isize + dz * n as isize + dx;
                        if (dz != 0 || dx != 0) && j >= 0 && (j as usize) < total && x + dx >= 0 && x + dx < n as isize && !water(j as usize) {
                            let rise = h[j as usize] as f64 - h[i] as f64;
                            above |= rise >= WALL;
                            below |= -rise >= WALL;
                        }
                    }
                }
                if flat {
                    if above { 0 } else if below { 2 } else { 3 }
                } else {
                    1
                }
            })
            .collect(),
        // Bare, open, forest, dense, water: read from the biome's name, so a modded biome counts without a table.
        View::Vegetation => {
            let cover: Vec<u16> = world.biomes.iter().map(|b| cover_of(&b.id)).collect();
            (0..total).map(|i| if water(i) { 4 } else if cover[world.biome_at[i] as usize] == 4 { 1 } else { cover[world.biome_at[i] as usize] }).collect()
        }
        _ => return None,
    })
}

/// The row of a biome's name; 4 for a water biome (a land point under one reads as open). Water goes first, so a
/// river biome is never read as a swamp.
fn cover_of(id: &str) -> u16 {
    let has = |words: &[&str]| words.iter().any(|w| id.contains(w));
    if is_water(id) {
        4
    } else if has(&["desert", "badlands", "mesa", "peaks", "ice_spikes", "stony", "_slopes", "dunes", "wasteland"]) {
        0
    } else if has(&["jungle", "dark_forest", "swamp", "mangrove", "bamboo", "rainforest"]) {
        3
    } else if has(&["forest", "birch", "taiga", "grove", "cherry", "woods", "woodland"]) {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Biome, World};

    /// `n` a side, sea at 63; `heights` row by row. Flat is what `flat_at` says, so a test picks it by hand.
    fn world(n: usize, heights: Vec<i16>, flat_at: Vec<u8>, biomes: &[&str], biome_at: Vec<u16>) -> World {
        World {
            seed: String::new(),
            samples: n,
            step: 16.0,
            origin_x: 0.0,
            origin_z: 0.0,
            sea_level: 63.0,
            heights,
            biome_at,
            biomes: biomes.iter().map(|id| Biome { id: id.to_string(), temperature: 0.5 }).collect(),
            colours: Vec::new(),
            flat_at,
            structures: None,
            section: None,
            biome_total: None,
            absent: Vec::new(),
            measured: Default::default(),
            made: None,
        }
    }

    #[test]
    fn steepness_is_the_biggest_gap_to_the_four_neighbours_and_ignores_water() {
        // 3 x 3: the middle at 100, its neighbours at 100, 95, 60 (water) and 120.
        let h: Vec<i16> = vec![0, 95, 0, 100, 100, 120, 0, 60, 0];
        assert_eq!(steepness(&h, 3, 63.0, 4), 20.0, "the 120 to the east");
        let h: Vec<i16> = vec![0, 95, 0, 100, 100, 100, 0, 60, 0];
        assert_eq!(steepness(&h, 3, 63.0, 4), 5.0, "the water to the south does not count");
    }

    #[test]
    fn slope_rows_follow_the_edges() {
        // One row of land climbing by 0, 3, 8, 14, 30, a sea point at the end: rows 0 to 4, then water.
        let row: Vec<i16> = vec![70, 70, 73, 81, 95, 125, 10];
        // A 7 x 7 grid of copies of the row: the north and south gaps are 0, so the row's own gaps decide.
        let all: Vec<i16> = (0..7).flat_map(|_| row.clone()).collect();
        let w = world(7, all, vec![0; 49], &["x"], vec![0; 49]);
        let at = rows(&w, View::Slope).unwrap();
        // Gaps to the widest neighbour: 0, 3, 8, 14, 30, 30 (the 125 against 95), then the sea point.
        assert_eq!(&at[0..7], &[0, 1, 2, 3, 4, 4, 5]);
    }

    #[test]
    fn a_flat_point_beside_a_wall_is_at_its_foot_or_on_top() {
        // 8 x 1 strip of 8 points (a grid of 8 a side, only the middle row matters): low ground at 70, then a
        // wall up to 100. The flat low points within 2 of the wall are at its foot; the flat high ones on top.
        let n = 8;
        let row: Vec<i16> = vec![70, 70, 70, 70, 100, 100, 100, 100];
        let mut h = Vec::new();
        for _ in 0..n {
            h.extend(row.clone());
        }
        let flat = vec![1u8; n * n];
        let w = world(n, h, flat, &["x"], vec![0; n * n]);
        let at = rows(&w, View::Cliffs).unwrap();
        let row_at = &at[3 * n..4 * n];
        // x = 0 is 4 points from the wall: other land is not what flat gives, but it is flat here, so no wall near.
        assert_eq!(row_at[0], 3, "too far from the wall");
        assert_eq!(row_at[2], 0, "two points from the wall: at its foot");
        assert_eq!(row_at[5], 2, "two points past the wall: on top");
        assert_eq!(row_at[7], 3, "too far from the wall");
    }

    #[test]
    fn vegetation_reads_the_biome_name_and_water_wins() {
        let ids = ["minecraft:desert", "minecraft:plains", "minecraft:birch_forest", "minecraft:jungle", "minecraft:river", "minecraft:swamp_river"];
        let n = 6;
        // One biome per column, all land but for the last row.
        let mut h = vec![80i16; n * n];
        for x in 0..n {
            h[(n - 1) * n + x] = 10;
        }
        let biome_at: Vec<u16> = (0..n * n).map(|i| (i % n) as u16).collect();
        let w = world(n, h, vec![0; n * n], &ids, biome_at);
        let at = rows(&w, View::Vegetation).unwrap();
        assert_eq!(&at[0..6], &[0, 1, 2, 3, 1, 1], "a land point under a river biome reads as open");
        assert_eq!(&at[(n - 1) * n..], &[4; 6], "below the sea is water");
    }
}

/// The colour of each row of `view`'s list.
pub fn colours(view: View) -> Vec<[u8; 3]> {
    match view {
        View::Slope => vec![
            hsl_to_rgb(100.0, 48.0, 62.0),
            hsl_to_rgb(68.0, 52.0, 58.0),
            hsl_to_rgb(42.0, 62.0, 55.0),
            hsl_to_rgb(18.0, 66.0, 48.0),
            hsl_to_rgb(350.0, 55.0, 30.0),
            flat_sea_colour(),
        ],
        View::Cliffs => vec![hsl_to_rgb(18.0, 78.0, 55.0), hsl_to_rgb(0.0, 0.0, 28.0), hsl_to_rgb(215.0, 70.0, 62.0), hsl_to_rgb(30.0, 8.0, 62.0), flat_sea_colour()],
        View::Vegetation => vec![hsl_to_rgb(38.0, 28.0, 66.0), hsl_to_rgb(78.0, 52.0, 62.0), hsl_to_rgb(120.0, 45.0, 42.0), hsl_to_rgb(140.0, 60.0, 24.0), flat_sea_colour()],
        _ => Vec::new(),
    }
}
