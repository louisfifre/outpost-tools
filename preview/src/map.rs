//! The 2D map's frame: the look (zoom and pan, preview.html lines 1779-1818), what hangs on the frame (the grid,
//! the cross at 0,0 and the ticks of the section's cut, CSS 228-256), and the points under the pointer.

use std::collections::BTreeSet;

use mere_base::{Cmd, Color, ImageId, Rect, Vec2};

use crate::paint::{View, marks};
use crate::world::World;

/// The map's picture, uploaded again whenever the colouring changes.
pub const MAP_IMAGE: ImageId = ImageId(14);

/// How the map is looked at: `k` times closer, its corner moved to (`x`, `y`) in widths of the frame.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Look {
    pub k: f32,
    pub x: f32,
    pub y: f32,
}

impl Look {
    pub const REST: Look = Look { k: 1.0, x: 0.0, y: 0.0 };

    /// The map may not leave the frame: its corner stays within [1 - k, 0] (line 1782).
    pub fn clamped(self) -> Look {
        Look { x: self.x.clamp(1.0 - self.k, 0.0), y: self.y.clamp(1.0 - self.k, 0.0), ..self }
    }

    /// The wheel (lines 1796-1803): a step a notch, 1.25 closer for a positive delta (away from the user) and 0.8
    /// for a negative one, the point under the pointer (`p`, as a fraction of the frame) staying where it is.
    pub fn wheel(self, p: Vec2, notches: f32) -> Look {
        if notches == 0.0 {
            return self;
        }
        // A touchpad's fraction of a notch is still a step, as one `wheel` event is for the page.
        let steps = if notches.abs() < 1.0 { notches.signum() } else { notches.round() };
        let factor = if notches > 0.0 { 1.25 } else { 0.8 };
        let mut look = self;
        for _ in 0..steps.abs() as u32 {
            let k = (look.k * factor).clamp(1.0, 16.0);
            look = Look { k, x: p.x - (p.x - look.x) * k / look.k, y: p.y - (p.y - look.y) * k / look.k }.clamped();
        }
        look
    }

    /// The carried map (lines 1814-1816): the look at the start of the press moved by `d` DIP of the frame.
    pub fn dragged(start: Look, d: Vec2, side: f32) -> Look {
        Look { x: start.x + d.x / side, y: start.y + d.y / side, ..start }.clamped()
    }
}

/// A press on the map: where it began, the look then, and whether it has travelled far enough to be a carry
/// and no longer a click (`carry` and `carried`, lines 1804-1818).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Carry {
    pub from: Vec2,
    pub look: Look,
    pub carried: bool,
}

/// `pointUnder` (lines 1770-1775): the column, row and index of the point under `local`, a place in the frame.
pub fn point_under(world: &World, look: Look, side: f32, local: Vec2) -> (usize, usize, usize) {
    let n = world.samples;
    let at = |v: f32, offset: f32| {
        let t = (v - offset * side) / (side * look.k) * n as f32;
        (t.floor().max(0.0) as usize).min(n - 1)
    };
    let (col, row) = (at(local.x, look.x), at(local.y, look.y));
    (col, row, row * n + col)
}

/// `structureNear` (lines 1484-1496): the nearest drawn structure to `local`, if one is close enough to be the
/// one pointed at: (kind, x, z).
pub fn structure_near(world: &World, chosen: &BTreeSet<String>, look: Look, side: f32, local: Vec2) -> Option<(usize, f64, f64)> {
    let s = world.structures.as_ref()?;
    let span = world.span();
    let box_w = side as f64 * look.k as f64;
    let x = world.origin_x + (local.x as f64 - look.x as f64 * side as f64) / box_w * span;
    let z = world.origin_z + (local.y as f64 - look.y as f64 * side as f64) / box_w * span;
    // Within about the mark's own radius, a little more.
    let mut reach = (6.0 * span / box_w).powi(2);
    let mut best = None;
    for &kind in &s.order {
        if !chosen.contains(&s.ids[kind]) {
            continue;
        }
        for &(px, pz) in &s.by_kind[kind] {
            let d = (px - x).powi(2) + (pz - z).powi(2);
            if d < reach {
                reach = d;
                best = Some((kind, px, pz));
            }
        }
    }
    best
}

/// Where the tick of cut `cut` sits as a fraction of the frame, when the page shows it (`showCutLine`, 1789-1795):
/// the world has a section and the tick lies within the frame after the look.
pub fn tick_at(world: &World, look: Look, cut: usize) -> Option<f32> {
    let at = (world.section.as_ref()?.cuts.get(cut)?.z - world.origin_z) / world.span();
    let y = look.y as f64 + at * look.k as f64;
    (at >= 0.0 && (0.0..=1.0).contains(&y)).then_some(y as f32)
}

fn fill(x: f32, y: f32, w: f32, h: f32, color: Color) -> Cmd {
    Cmd::Fill { rect: Rect::new(x, y, w, h), color }
}

/// Everything the frame holds, in its own coordinates, in painting order: the picture, the marks, the grid and
/// the cross, all cut to the frame (`#flat` hides its overflow), then the ticks, which hang outside it.
pub fn frame_cmds(world: &World, view: View, chosen: &BTreeSet<String>, look: Look, side: f32, grid: Color, muted: Color, cut: usize) -> Vec<Cmd> {
    let mut out = vec![Cmd::PushClip { rect: Rect::new(0.0, 0.0, side, side) }];
    let whole = side * look.k;
    out.push(Cmd::Image { image: MAP_IMAGE, rect: Rect::new(look.x * side, look.y * side, whole, whole) });
    if view == View::Structures {
        out.extend(marks(world, chosen, look, side));
    }

    // #grid (CSS 240-246): one pixel lines, a square every 1/8 of the map, following the look. Which pixel a
    // fractional position lands on is UNVERIFIED against Chrome's background tiling.
    let line_at = |origin: f32, j: usize| (origin * side + j as f32 * whole / 8.0).round();
    for j in 0.. {
        let x = line_at(look.x, j);
        if x >= side {
            break;
        }
        if x >= 0.0 {
            out.push(fill(x, 0.0, 1.0, side, grid));
        }
    }
    for j in 0.. {
        let y = line_at(look.y, j);
        if y >= side {
            break;
        }
        if y >= 0.0 {
            out.push(fill(0.0, y, side, 1.0, grid));
        }
    }

    // The cross at 0,0 (CSS 247-251): 15 px white lines with a black halo. The page's two drop-shadow(0 0 1px
    // #000) are approximated by a translucent black border of one pixel: UNVERIFIED.
    let (cx, cy) = ((look.x + 0.5 * look.k) * side, (look.y + 0.5 * look.k) * side);
    let (ox, oy) = ((cx - 7.5).round(), (cy - 7.5).round());
    let halo = Color::hex(0).with_alpha(0.5);
    out.push(fill(ox - 1.0, oy + 6.0, 17.0, 3.0, halo));
    out.push(fill(ox + 6.0, oy - 1.0, 3.0, 17.0, halo));
    let white = Color::hex(0xffffff);
    out.push(fill(ox, oy + 7.0, 15.0, 1.0, white));
    out.push(fill(ox + 7.0, oy, 1.0, 15.0, white));
    out.push(Cmd::PopClip);

    // #cutLine (CSS 253-256): a tick of 6 x 1 px on each side, 9 px outside the frame.
    if let Some(y) = tick_at(world, look, cut) {
        let row = (y * side - 0.5).round();
        out.push(fill(-9.0, row, 6.0, 1.0, muted));
        out.push(fill(side + 3.0, row, 6.0, 1.0, muted));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_in_from_rest_keeps_the_point_under_the_pointer() {
        let l = Look::REST.wheel(Vec2::new(0.5, 0.5), 1.0);
        assert_eq!(l.k, 1.25);
        assert!((l.x + 0.125).abs() < 1e-6 && (l.y + 0.125).abs() < 1e-6, "{l:?}");
    }

    #[test]
    fn wheel_out_at_rest_stays_at_rest() {
        assert_eq!(Look::REST.wheel(Vec2::new(0.5, 0.5), -1.0), Look::REST);
    }

    #[test]
    fn the_zoom_never_passes_16() {
        let mut l = Look::REST;
        for _ in 0..20 {
            l = l.wheel(Vec2::new(0.3, 0.7), 1.0);
        }
        assert_eq!(l.k, 16.0);
        assert!(l.x >= 1.0 - 16.0 && l.x <= 0.0);
    }

    #[test]
    fn dragging_moves_the_corner_by_the_travel_in_frame_widths_within_the_clamp() {
        let start = Look { k: 2.0, x: -0.5, y: -0.5 };
        let l = Look::dragged(start, Vec2::new(-63.8, 0.0), 638.0);
        assert!((l.x + 0.6).abs() < 1e-5 && l.y == -0.5, "{l:?}");
        // At k = 1 there is no room: a positive drag is clamped to 0.
        assert_eq!(Look::dragged(Look::REST, Vec2::new(50.0, 50.0), 638.0), Look::REST);
    }
}
