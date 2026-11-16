//! The height bands of the relief view: their names and colours, ported from preview.html so that the list
//! shows what the page's does (lines 509, 1040-1045, 1091-1102, 1134, 1137, 1353-1355).

/// `seaLevel` of the out folder's `preview.json`.
pub const SEA: f64 = 63.0;
/// Line 1134: where one band of height ends and the next starts, above the sea.
pub const HEIGHT_STEPS: [f64; 4] = [100.0, 150.0, 200.0, 250.0];
/// Line 1137: the same under the sea, in blocks of water over the floor.
pub const SEA_DEPTHS: [f64; 2] = [20.0, 45.0];

pub use crate::colour::{height_colour, hsl_to_rgb};

/// Line 509.
pub fn band_names(sea: f64, steps: &[f64], depths: &[f64]) -> Vec<String> {
    let deepest = depths[depths.len() - 1];
    let mut out = vec![format!("Sea, deeper than {deepest}")];
    let shelves: Vec<String> =
        depths.iter().enumerate().map(|(i, deep)| format!("Sea, {} to {deep} deep", if i > 0 { depths[i - 1] } else { 0.0 })).collect();
    out.extend(shelves.into_iter().rev());
    for (i, top) in steps.iter().enumerate() {
        out.push(format!("{} to {top}", if i > 0 { steps[i - 1] } else { sea }));
    }
    out.push(format!("Above {}", steps[steps.len() - 1]));
    out
}

/// Lines 1353-1355: each band takes the colour of its middle.
pub fn band_colours(sea: f64, steps: &[f64], depths: &[f64]) -> Vec<[u8; 3]> {
    let mut out = vec![height_colour(sea - depths[depths.len() - 1] - 15.0, sea)];
    let shelves: Vec<[u8; 3]> = depths
        .iter()
        .enumerate()
        .map(|(i, deep)| height_colour(sea - ((if i > 0 { depths[i - 1] } else { 0.0 }) + deep) / 2.0, sea))
        .collect();
    out.extend(shelves.into_iter().rev());
    let lows: Vec<f64> = std::iter::once(sea).chain(steps.iter().copied()).collect();
    for (i, low) in lows.iter().enumerate() {
        out.push(height_colour(if i < steps.len() { (low + steps[i]) / 2.0 } else { low + 30.0 }, sea));
    }
    out
}
