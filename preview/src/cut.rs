//! The cut as it is looked at (preview.html lines 2206-2332 and 2467-2496): the look (scale and corner), the
//! picture of the part in sight, the whole cut drawn small above it with its frame, and what lies under the pointer.
//!
//! The arithmetic is the page's in f64, in its order. `box` below is the `#cutView` border box as
//! `getBoundingClientRect` gives it: the look is always judged against it. The page's canvases are shown in the
//! padding box, one pixel shorter than the border box (the box has a border-bottom), so a picture made `box_h`
//! rows high is squeezed by `(box_h - 1) / box_h` in height: `sheet_rect` does it. Images are made at DIP
//! resolution (the page's `devicePixelRatio` is 1 here).

use std::collections::BTreeSet;

use mere_base::{ImageId, Rect};

use crate::section::{CAVE, DEEP, EARTH, EARTH_DEPTH, Ground, LAVA, ORE_COLOURS, ORE_KINDS, ROCK, Section, Shown, WATER};
use crate::text;
use crate::world::World;

/// The part of the cut in sight, uploaded again whenever it changes.
pub const CUT_IMAGE: ImageId = ImageId(18);
/// The whole cut drawn small.
pub const NAV_IMAGE: ImageId = ImageId(17);
/// `#nav` (line 285): the border box; its canvas shows in the 55 px of the padding box.
pub const NAV_H: f32 = 56.0;
/// What `#nav`'s `clientHeight` is: the border box less its border-bottom.
const NAV_CLIENT_H: f64 = 55.0;
/// Line 2471: the closest the cut may be looked at.
const MOST_CLOSE: f64 = 12.0;

/// How the cut is looked at: `k` screen pixels per block, and where the cut's corner went (screen pixels). A fresh
/// look has `k == 0`: the first settling puts the highest ground in the middle at the scale that fits.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CutLook {
    pub k: f64,
    pub x: f64,
    pub y: f64,
}

impl CutLook {
    pub const FRESH: CutLook = CutLook { k: 0.0, x: 0.0, y: 0.0 };
}

/// Line 1376.
fn clamp(v: f64, low: f64, high: f64) -> f64 {
    low.max(v.min(high))
}

/// `Math.round`.
fn round(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// `cutScales` (2230-2233): the scale that fills the window with the whole height of the cut, and the least one:
/// half of that, never smaller than the whole length in the window.
pub fn scales(box_w: f64, box_h: f64, ground: &Ground, length: usize) -> (f64, f64) {
    let fit = box_h / ground.rows as f64;
    (fit, fit.min((box_w / length as f64).max(fit / 2.0)))
}

/// `showCutLook` (2211-2226) without its drawing: a new cut is first looked at around its highest ground, the
/// scale and the corner are kept within the cut, and a cut lower than its window sits on the window's bottom line.
/// Settling again changes nothing.
pub fn settled(look: CutLook, box_w: f64, box_h: f64, ground: &Ground, length: usize) -> CutLook {
    if !(box_w > 0.0 && box_h > 0.0) {
        return look;
    }
    let (fit, least) = scales(box_w, box_h, ground, length);
    let mut l = look;
    if l.k == 0.0 || l.k.is_nan() {
        l.x = box_w / 2.0 - ground.peak as f64 * fit;
    }
    l.k = clamp(if l.k != 0.0 && !l.k.is_nan() { l.k } else { fit }, least, MOST_CLOSE);
    let (wide, tall) = (length as f64 * l.k, ground.rows as f64 * l.k);
    l.x = clamp(l.x, (box_w - wide).min(0.0), 0.0);
    l.y = if tall <= box_h { box_h - tall } else { clamp(l.y, box_h - tall, 0.0) };
    l
}

/// The wheel (2467-2474): a step a notch, 1.25 closer for a positive delta and 0.8 for a negative one, the point
/// under the pointer (`p`, local to the box) staying where it is. A fraction of a notch is one step, as a touchpad's
/// `wheel` event is for the page, and each step is settled.
pub fn wheel(look: CutLook, p: (f64, f64), notches: f32, box_w: f64, box_h: f64, ground: &Ground, length: usize) -> CutLook {
    if notches == 0.0 {
        return look;
    }
    let steps = if notches.abs() < 1.0 { notches.signum() } else { notches.round() };
    let factor = if notches > 0.0 { 1.25 } else { 0.8 };
    let least = scales(box_w, box_h, ground, length).1;
    let mut look = look;
    for _ in 0..steps.abs() as u32 {
        let k = clamp(look.k * factor, least, MOST_CLOSE);
        look = CutLook { k, x: p.0 - (p.0 - look.x) * k / look.k, y: p.1 - (p.1 - look.y) * k / look.k };
        look = settled(look, box_w, box_h, ground, length);
    }
    look
}

/// The carried cut (2483): the look at the start of the press moved by the travel, then settled.
pub fn dragged(start: CutLook, dx: f64, dy: f64, box_w: f64, box_h: f64, ground: &Ground, length: usize) -> CutLook {
    settled(CutLook { x: start.x + dx, y: start.y + dy, ..start }, box_w, box_h, ground, length)
}

/// The blocks in sight (2242-2245): columns `x0..x1` and rows `r0..r1` of the cut once its sky is cropped.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sight {
    pub x0: usize,
    pub x1: usize,
    pub r0: usize,
    pub r1: usize,
}

impl Sight {
    pub fn w(&self) -> usize {
        self.x1 - self.x0
    }
    pub fn h(&self) -> usize {
        self.r1 - self.r0
    }
}

/// None when nothing is in sight.
pub fn sight(look: CutLook, box_w: f64, box_h: f64, length: usize, rows: usize) -> Option<Sight> {
    let k = look.k;
    let x0 = (-look.x / k).floor().max(0.0);
    let x1 = ((box_w - look.x) / k).ceil().min(length as f64);
    let r0 = (-look.y / k).floor().max(0.0);
    let r1 = ((box_h - look.y) / k).ceil().min(rows as f64);
    (x1 - x0 > 0.0 && r1 - r0 > 0.0).then(|| Sight { x0: x0 as usize, x1: x1 as usize, r0: r0 as usize, r1: r1 as usize })
}

/// The picture of what is in sight (`drawCut`, 2247-2269): RGBA, `w * h` texels, one per block. The sky (air at or
/// above the surface) is transparent, so the page's ground shows. Seen from afar (k under 0.75) an ore of one block
/// is drawn two blocks wide. An ore whose kind is not chosen is left out when something is chosen.
pub fn sheet(section: &Section, cells: &[u8], ground: &Ground, layer: &[u8], s: Sight, k: f64, chosen: &BTreeSet<String>) -> Vec<u8> {
    let (w, h) = (s.w(), s.h());
    let l = section.length;
    let top_y = section.top_y();
    let mut px = vec![0u8; w * h * 4];
    for r in 0..h {
        for x in 0..w {
            let bx = s.x0 + x;
            let row = ground.crop + s.r0 + r;
            let y = top_y - row as f64;
            let cell = cells[row * l + bx];
            if cell == 0 && y >= ground.surface[bx] {
                continue;
            }
            let floor = ground.floor[bx];
            let c = match cell {
                0 => CAVE,
                2 => WATER,
                3 => LAVA,
                _ if y < floor && y >= floor - EARTH_DEPTH => EARTH,
                _ if y < 4.0 => DEEP,
                _ => ROCK,
            };
            let i = (r * w + x) * 4;
            px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    let reach = if k < 0.75 { 1 } else { 0 };
    for r in 0..h {
        for x in 0..w {
            let ore = layer[(ground.crop + s.r0 + r) * l + s.x0 + x];
            if ore == 0 || (!chosen.is_empty() && !chosen.contains(ORE_KINDS[ore as usize - 1])) {
                continue;
            }
            let c = ORE_COLOURS[ore as usize - 1];
            for dr in 0..=reach {
                for dx in 0..=reach {
                    if r + dr < h && x + dx < w {
                        let i = ((r + dr) * w + x + dx) * 4;
                        px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
                    }
                }
            }
        }
    }
    px
}

/// The arguments the page gives `drawImage` (2272) at a ratio of 1: x, y, width, height in canvas space.
pub fn sheet_args(look: CutLook, s: Sight) -> [f64; 4] {
    let k = look.k;
    [look.x + s.x0 as f64 * k, look.y + s.r0 as f64 * k, s.w() as f64 * k, s.h() as f64 * k]
}

/// Where the sheet goes in the cut view's own coordinates: `sheet_args`, with the page's canvas of `box_h` rows
/// shown in a padding box one pixel shorter, so y and the height are squeezed by `(box_h - 1) / box_h`.
pub fn sheet_rect(look: CutLook, s: Sight, box_h: f64) -> Rect {
    let [x, y, w, h] = sheet_args(look, s);
    let squeeze = (box_h - 1.0) / box_h;
    Rect::new(x as f32, (y * squeeze) as f32, w as f32, (h * squeeze) as f32)
}

/// The heights the strip shows (line 2286): from a little under the deepest floor to a little over the highest ground. (top, bottom).
pub fn nav_range(section: &Section, ground: &Ground) -> (f64, f64) {
    (ground.top + 6.0, section.min_y.max(ground.bottom - 20.0))
}

/// The whole cut drawn small (`drawNav`, 2279-2300): its rock and its water, from a little under the deepest floor
/// to a little over the highest ground, so the heights are stretched to read. Returns RGBA of `width` by `NAV_H`
/// (transparent where nothing is drawn) and the range of heights it shows, (top, bottom).
pub fn nav_image(section: &Section, ground: &Ground, width: usize) -> (Vec<u8>, (f64, f64)) {
    let height = NAV_H as usize;
    let (l, min_y) = (section.length, section.min_y);
    let (top, bottom) = nav_range(section, ground);
    let mut px = vec![0u8; width * height * 4];
    let row_of = |y: f64| round((top - y) / (top - bottom) * height as f64);
    for col in 0..width {
        let from = ((col * l) as f64 / width as f64).floor() as usize;
        let to = (from + 1).max((((col + 1) * l) as f64 / width as f64).floor() as usize);
        let (mut rock_y, mut water_y) = (min_y, min_y);
        for x in from..to {
            rock_y = rock_y.max(ground.floor[x]);
            water_y = water_y.max(ground.surface[x]);
        }
        let rock = clamp(row_of(rock_y), 0.0, height as f64) as usize;
        let sea = clamp(row_of(water_y), 0.0, height as f64) as usize;
        for r in sea..height {
            let c = if r < rock { WATER } else { ROCK };
            let i = (r * width + col) * 4;
            px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    (px, (top, bottom))
}

/// Line 2302: how many times the heights are stretched, blocks per pixel along the cut against blocks per pixel up it.
pub fn nav_times(length: usize, width: f64, range: (f64, f64)) -> f64 {
    (length as f64 / width) / ((range.0 - range.1) / NAV_H as f64)
}

/// The frame over the strip as the page's styles give it (`showNavFrame`, 2305-2316), in px of `#nav`'s padding box.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct NavFrame {
    pub left: f64,
    pub top: f64,
    /// The `width` style, before the 5 px minimum.
    pub width: f64,
    /// The `bottom` style: the distance from the padding box's bottom edge.
    pub bottom: f64,
}

impl NavFrame {
    /// The box the page draws: `min-width: 5px`, and a height that is what the top and the bottom leave.
    pub fn rect(&self) -> Rect {
        Rect::new(self.left as f32, self.top as f32, self.width.max(5.0) as f32, (NAV_CLIENT_H - self.bottom - self.top) as f32)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn nav_frame(look: CutLook, box_w: f64, box_h: f64, length: usize, ground: &Ground, section: &Section, range: (f64, f64), width: f64) -> NavFrame {
    let (k, l) = (look.k, length as f64);
    let height = NAV_CLIENT_H;
    let top_y = section.top_y() - ground.crop as f64;
    let x0 = clamp(-look.x / k, 0.0, l);
    let x1 = clamp((box_w - look.x) / k, 0.0, l);
    // The heights in sight, placed on the strip's own range of heights.
    let row_of = |y: f64| (range.0 - y) / (range.0 - range.1) * height;
    let y_of = |row: f64| top_y - row;
    let high = if look.y >= 0.0 { 0.0 } else { clamp(row_of(y_of(-look.y / k)), 0.0, height) };
    let low = clamp(row_of(y_of((box_h - look.y) / k)), 0.0, height);
    NavFrame { left: x0 / l * width, top: high, width: (x1 - x0) / l * width, bottom: if low >= height - 0.5 { 0.0 } else { height - low } }
}

/// A press or a drag on the strip (2325): where the window's left edge goes, as a look not yet settled. `grip` is
/// how far from the frame's left edge it was taken, `local_x` the pointer in the strip.
pub fn nav_go(look: CutLook, grip: f64, local_x: f64, width: f64, length: usize) -> CutLook {
    let per = length as f64 / width;
    CutLook { x: -((local_x - grip) * per) * look.k, ..look }
}

/// What lies under the pointer (2488-2495), `local` being its place in the box: None outside the cut, where the
/// page says the rest text. The layer is read whatever is chosen.
pub fn probe(world: &World, shown: &Shown, look: CutLook, local: (f64, f64)) -> Option<String> {
    let section = world.section.as_ref()?;
    let cut = section.cuts.get(shown.at)?;
    let cells = cut.cells.as_ref()?;
    let ground = &shown.ground;
    let l = section.length;
    let x = ((local.0 - look.x) / look.k).floor();
    let r = ((local.1 - look.y) / look.k).floor();
    if x < 0.0 || x >= l as f64 || r < 0.0 || r >= ground.rows as f64 {
        return None;
    }
    let (x, row) = (x as usize, ground.crop + r as usize);
    let y = section.top_y() - row as f64;
    let (cell, ore) = (cells[row * l + x], shown.play.layer[row * l + x]);
    let what = if ore != 0 {
        text::ORES[ore as usize - 1]
    } else if cell == 1 && y >= ground.floor[x] - EARTH_DEPTH {
        text::CUT_EARTH
    } else {
        text::CUT_CELLS.get(cell as usize).copied().unwrap_or("undefined")
    };
    let biome = ground.biomes[x >> 4].as_deref().map(text::pretty).unwrap_or_default();
    Some(text::cut_probe(&text::num(cut.x + x as f64), y, &text::cut_depth(ground.floor[x] - y), what, &biome))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ground that only has the numbers the look reads.
    fn ground(rows: usize, peak: usize) -> Ground {
        Ground { surface: Vec::new(), floor: Vec::new(), chunks: 0, high: Vec::new(), biomes: Vec::new(), top: 0.0, bottom: 0.0, peak, crop: 0, rows }
    }

    #[test]
    fn the_scales_fill_the_height_and_allow_half_of_it() {
        let g = ground(300, 0);
        // A window 600 high: fit is 2; a 638 wide window over 4096 blocks wants 0.156 at least, half of fit is 1.
        let (fit, least) = scales(638.0, 600.0, &g, 4096);
        assert_eq!(fit, 2.0);
        assert_eq!(least, 1.0);
        // A wide window may go down to the whole length in it.
        let (_, least) = scales(5000.0, 600.0, &g, 4096);
        assert_eq!(least, 5000.0 / 4096.0);
    }

    #[test]
    fn a_fresh_look_puts_the_peak_in_the_middle_then_clamps() {
        let g = ground(300, 1000);
        let l = settled(CutLook::FRESH, 638.0, 600.0, &g, 4096);
        assert_eq!(l.k, 2.0);
        // 319 - 1000 * 2 is -1681, inside [638 - 8192, 0].
        assert_eq!((l.x, l.y), (319.0 - 2000.0, 0.0));
        // A peak at the very start would put the corner right of the window: clamped to 0.
        let l = settled(CutLook::FRESH, 638.0, 600.0, &ground(300, 0), 4096);
        assert_eq!(l.x, 0.0);
        // And a peak at the end cannot pull the window past the cut's end.
        let l = settled(CutLook::FRESH, 638.0, 600.0, &ground(300, 4095), 4096);
        assert_eq!(l.x, 638.0 - 4096.0 * 2.0);
        // Settling again changes nothing.
        assert_eq!(settled(l, 638.0, 600.0, &ground(300, 4095), 4096), l);
    }

    #[test]
    fn a_cut_lower_than_its_window_sits_on_its_bottom_line() {
        let g = ground(100, 10);
        // fit = 600 / 100 = 6, but least is 3 (half of it), so zooming out makes the cut 300 high in 600.
        let l = settled(CutLook { k: 3.0, x: 0.0, y: -50.0 }, 638.0, 600.0, &g, 4096);
        assert_eq!(l.k, 3.0);
        assert_eq!(l.y, 600.0 - 300.0);
        // A tall one may be moved up and down within its height.
        let l = settled(CutLook { k: 12.0, x: 0.0, y: 10.0 }, 638.0, 600.0, &g, 4096);
        assert_eq!(l.y, 0.0);
        let l = settled(CutLook { k: 12.0, x: 0.0, y: -5000.0 }, 638.0, 600.0, &g, 4096);
        assert_eq!(l.y, 600.0 - 1200.0);
    }

    #[test]
    fn the_wheel_keeps_the_point_under_the_pointer_and_stays_within_the_scales() {
        let g = ground(300, 1000);
        let start = settled(CutLook::FRESH, 638.0, 600.0, &g, 4096);
        let p = (200.0, 300.0);
        let l = wheel(start, p, 1.0, 638.0, 600.0, &g, 4096);
        assert_eq!(l.k, 2.5);
        // The block under the pointer is the same before and after.
        assert!(((p.0 - start.x) / start.k - (p.0 - l.x) / l.k).abs() < 1e-9);
        // Zoomed out for ever it stops at the least scale; in, at 12.
        assert_eq!(wheel(start, p, -30.0, 638.0, 600.0, &g, 4096).k, scales(638.0, 600.0, &g, 4096).1);
        assert_eq!(wheel(start, p, 40.0, 638.0, 600.0, &g, 4096).k, 12.0);
        // A fraction of a notch is a step.
        assert_eq!(wheel(start, p, 0.2, 638.0, 600.0, &g, 4096).k, 2.5);
        assert_eq!(wheel(start, p, 0.0, 638.0, 600.0, &g, 4096), start);
    }

    #[test]
    fn a_drag_moves_the_corner_by_the_travel_within_the_cut() {
        let g = ground(300, 1000);
        let start = settled(CutLook::FRESH, 638.0, 600.0, &g, 4096);
        let l = dragged(start, -100.0, 0.0, 638.0, 600.0, &g, 4096);
        assert_eq!(l.x, start.x - 100.0);
        // Far to the right of the cut's start: clamped at 0.
        assert_eq!(dragged(start, 1e6, 0.0, 638.0, 600.0, &g, 4096).x, 0.0);
    }

    #[test]
    fn the_sight_is_the_blocks_the_window_shows() {
        let s = sight(CutLook { k: 2.0, x: -100.0, y: 0.0 }, 638.0, 600.0, 4096, 300).unwrap();
        assert_eq!(s, Sight { x0: 50, x1: 369, r0: 0, r1: 300 });
        assert_eq!(sight(CutLook { k: 2.0, x: 10.0, y: -10.0 }, 638.0, 600.0, 4096, 300).unwrap().x0, 0);
        assert_eq!(sight(CutLook { k: 2.0, x: -9000.0, y: 0.0 }, 638.0, 600.0, 4096, 300), None);
    }

    #[test]
    fn the_sheet_goes_where_the_page_draws_it_squeezed_by_the_missing_pixel() {
        let s = Sight { x0: 50, x1: 369, r0: 3, r1: 300 };
        let look = CutLook { k: 2.0, x: -100.0, y: 0.0 };
        assert_eq!(sheet_args(look, s), [0.0, 6.0, 638.0, 594.0]);
        let r = sheet_rect(look, s, 600.0);
        assert_eq!((r.x, r.w), (0.0, 638.0));
        assert!((r.y - 6.0 * 599.0 / 600.0).abs() < 1e-5 && (r.h - 594.0 * 599.0 / 600.0).abs() < 1e-4, "{r:?}");
    }

    #[test]
    fn the_frame_on_the_strip_is_never_narrower_than_five_pixels() {
        let section = Section { length: 4096, height: 384, min_y: -64.0, cuts: vec![], rules: vec![], biomes: Default::default() };
        let g = ground(300, 0);
        // At 12 px a block, 638 px is 53 blocks of 4096: 53 / 4096 * 638 is 8 px; a wider cut makes it thinner.
        let f = nav_frame(CutLook { k: 12.0, x: -1200.0, y: -50.0 }, 638.0, 600.0, 4096, &g, &section, (100.0, 0.0), 638.0);
        assert!((f.width - 638.0 / 12.0 / 4096.0 * 638.0).abs() < 1e-9);
        assert!(f.rect().w > 8.2 && f.rect().w < 8.4, "{:?}", f.rect());
        let thin = NavFrame { left: 10.0, top: 0.0, width: 1.5, bottom: 0.0 };
        assert_eq!(thin.rect().w, 5.0);
        assert_eq!(thin.rect().h, 55.0);
    }

    #[test]
    fn a_press_on_the_strip_puts_the_window_where_the_pointer_is() {
        // 638 px for 4096 blocks, grip 0: a press at 100 px is block 641.8, at k = 2 the corner is at -1283.6.
        let l = nav_go(CutLook { k: 2.0, x: 0.0, y: 5.0 }, 0.0, 100.0, 638.0, 4096);
        assert!((l.x + 100.0 * 4096.0 / 638.0 * 2.0).abs() < 1e-9);
        assert_eq!(l.y, 5.0);
    }
}
