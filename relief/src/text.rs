//! Every sentence the demo shows, in one place, so wording changes touch one file.

use preview::world::World;

pub const WINDOW_TITLE: &str = "Relief";
pub const TITLE: &str = "Relief";
pub const HINT: &str = "Drag to turn, wheel to zoom";

/// A whole number as the page writes it: thousands separated by commas, the minus sign in front.
pub fn num(v: i64) -> String {
    let digits = v.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(d);
    }
    if v < 0 {
        out.insert(0, '-');
    }
    out
}

/// The page's caption under its map, minus the grid's square and the cross: the area first, north up.
pub fn area(w: &World) -> String {
    let blocks = num(w.span() as i64);
    format!("{blocks} \u{d7} {blocks} blocks, north up.")
}

pub fn seed(w: &World) -> String {
    format!("Seed {}", w.seed)
}

pub fn grid(w: &World) -> String {
    format!("One point every {} blocks", num(w.step as i64))
}

pub fn heights(w: &World) -> String {
    let (lo, hi) = w.height_range();
    format!("Heights {} to {}, sea at {}", num(lo.into()), num(hi.into()), num(w.sea_level as i64))
}

/// The page's figure: biomes met, "of" all those the generator can place when the file says so.
pub fn biomes(w: &World) -> String {
    match w.biome_total {
        Some(total) => format!("Biomes present {} of {}", num(w.biomes.len() as i64), num(total as i64)),
        None => format!("Biomes present {}", num(w.biomes.len() as i64)),
    }
}

pub fn usage() -> &'static str {
    "usage: relief <data-dir> [--shot out.png --size WxH --scale S]"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_as_the_page_writes_them() {
        assert_eq!(num(4096), "4,096");
        assert_eq!(num(512), "512");
        assert_eq!(num(-64), "-64");
        assert_eq!(num(1_234_567), "1,234,567");
        assert_eq!(num(0), "0");
    }

    #[test]
    fn the_overlay_speaks_like_the_pages_caption() {
        let dir = &preview::world::data_dir();
        if !dir.join("preview.json").exists() {
            return;
        }
        let w = World::load(dir).unwrap();
        assert_eq!(area(&w), "4,096 \u{d7} 4,096 blocks, north up.");
        assert_eq!(grid(&w), "One point every 16 blocks");
        assert_eq!(biomes(&w), "Biomes present 25 of 136");
    }
}