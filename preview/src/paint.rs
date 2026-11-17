//! The map's colours: the page's `paint()` (preview.html lines 1379-1449) for the five 2D views, point for
//! point, and the structures' marks (`drawMarks`, 1459-1482) as screen-space commands.

use std::collections::BTreeSet;

use mere_base::{Cmd, Color, Vec2};

use crate::bands::{HEIGHT_STEPS, SEA_DEPTHS};
use crate::colour::{SEA_VEIL, climate, flat_colour, flat_sea_colour, height_colour, steep_colour, temperature_colour};
use crate::map::Look;
use crate::world::World;

/// What the map shows: the strip's first five tabs, in order.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum View {
    Biomes,
    Flat,
    Height,
    Temperature,
    Structures,
}

impl View {
    pub fn from_index(i: usize) -> Option<View> {
        Some(match i {
            0 => View::Biomes,
            1 => View::Flat,
            2 => View::Height,
            3 => View::Temperature,
            4 => View::Structures,
            _ => return None,
        })
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The page's key for the view (`data-view`, `?view=`).
    pub fn key(self) -> &'static str {
        ["biomes", "flat", "height", "temperature", "structures"][self as usize]
    }

    pub fn from_key(key: &str) -> Option<View> {
        (0..5).filter_map(View::from_index).find(|v| v.key() == key)
    }
}

/// `Math.max(low, Math.min(high, v))`.
fn clamp(v: f64, low: f64, high: f64) -> f64 {
    low.max(high.min(v))
}

/// Per point, the row of the list beside the map (`listOf`, 1326-1370) and the rows' ids. None for the
/// structures, which are counted in a list of their own and have no row per point.
pub fn rows(world: &World, view: View) -> Option<(Vec<u16>, Vec<String>)> {
    let sea = world.sea_level;
    match view {
        View::Biomes => Some((world.biome_at.clone(), world.biomes.iter().map(|b| b.id.clone()).collect())),
        View::Height => {
            let at = world
                .heights
                .iter()
                .map(|&y| {
                    let y = y as f64;
                    // The rows go up from the deepest floor to the highest ground.
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
                })
                .collect();
            Some((at, ids(SEA_DEPTHS.len() + 1 + HEIGHT_STEPS.len() + 1)))
        }
        View::Temperature => {
            // Five classes over the game's range, from -0.7 to 2, the sea in them like the land.
            let band: Vec<u16> =
                world.biomes.iter().map(|b| (clamp((climate(b) + 0.7) / 2.7, 0.0, 1.0) * 4.0 + 0.5).floor() as u16).collect();
            Some((world.biome_at.iter().map(|&b| band[b as usize]).collect(), ids(5)))
        }
        View::Flat => {
            let at = (0..world.heights.len())
                .map(|i| if (world.heights[i] as f64) < sea { 2 } else if world.flat_at[i] == 1 { 0 } else { 1 })
                .collect();
            Some((at, ids(3)))
        }
        View::Structures => None,
    }
}

fn ids(n: usize) -> Vec<String> {
    (0..n).map(|i| i.to_string()).collect()
}

/// The map as `samples` x `samples` RGBA bytes. `chosen` holds the ids chosen in this view: when it holds one
/// of the rows' ids, what is not chosen steps back towards the ground. `dark` picks the theme's step-back greys.
pub fn paint(world: &World, view: View, dark: bool, chosen: &BTreeSet<String>) -> Vec<u8> {
    let n = world.samples;
    let total = n * n;
    let h = &world.heights;
    let sea = world.sea_level;
    let at = &world.biome_at;
    let marks = view == View::Structures;
    // What is not a chosen biome steps back towards the ground: water, then land.
    let step_back: [f64; 2] = if dark { [16.0, 40.0] } else { [200.0, 226.0] };
    // The line between two zones that stepped back: a little towards the text's side of the ground.
    let faded_line = if dark { 1.3 } else { 0.92 };
    // The choice of this view only. A chosen row this world does not have dims nothing.
    let list = if marks { None } else { rows(world, view) };
    let lit: Vec<bool> = list.as_ref().map_or(Vec::new(), |(_, ids)| ids.iter().map(|id| chosen.contains(id)).collect());
    let choosing = lit.contains(&true);
    let row_at = list.as_ref().map(|(at, _)| at);

    let mut px = vec![0u8; total * 4];
    for i in 0..total {
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
        } else if view == View::Flat {
            c = as_f64(if water {
                flat_sea_colour()
            } else if world.flat_at[i] == 1 {
                flat_colour()
            } else {
                steep_colour()
            });
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
        for k in 0..3 {
            // A canvas pixel array (Uint8ClampedArray) clamps, then rounds half to even.
            px[i * 4 + k] = (c[k] * shade).round_ties_even().clamp(0.0, 255.0) as u8;
        }
        px[i * 4 + 3] = 255;
    }
    px
}

fn as_f64(c: [u8; 3]) -> [f64; 3] {
    c.map(|v| v as f64)
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
