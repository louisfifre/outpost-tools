//! The page's colours (preview.html lines 1007-1123), ported expression by expression so that `paint` gives the
//! bytes the page's own `paint()` gives. Everything is f64, in the page's order of operations: `Math.round` is
//! `(x + 0.5).floor()`, and JS's `%` on floats is Rust's.

use crate::world::Biome;

/// One alternative of a family's pattern. The page writes them as regular expressions; the only shapes they
/// take are a plain word, two words in order (`deep.*ocean`) and a word that must end a word (`mines\b`), so
/// this is all a matcher needs. The parts must occur in order, each found after the end of the one before
/// (`a.*b`); a part spelled `"mines\\b"` is the literal "mines" followed by the end of the text or a character
/// outside [A-Za-z0-9_].
pub struct Alt(pub &'static [&'static str]);

impl Alt {
    pub fn matches(&self, text: &str) -> bool {
        in_order(self.0, text)
    }
}

fn in_order(parts: &[&str], text: &str) -> bool {
    let Some((first, rest)) = parts.split_first() else { return true };
    let (word, boundary) = match first.strip_suffix("\\b") {
        Some(w) => (w, true),
        None => (*first, false),
    };
    // Every place the word occurs, because a later one may be the one that lets the rest match (or the
    // boundary hold): that is what a regular expression tries too.
    let mut from = 0;
    while let Some(at) = text[from..].find(word) {
        let start = from + at;
        let end = start + word.len();
        let ends_word = text[end..].chars().next().is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
        if (!boundary || ends_word) && in_order(rest, &text[end..]) {
            return true;
        }
        // Step one character on, not one byte, so the next search starts on a boundary.
        from = start + text[start..].chars().next().map_or(1, char::len_utf8);
        if from > text.len() {
            break;
        }
    }
    false
}

/// Whether any alternative matches: a pattern `a|b|c`.
pub fn any(alts: &[Alt], text: &str) -> bool {
    alts.iter().any(|a| a.matches(text))
}

macro_rules! alt {
    ($l:literal) => {
        Alt(&[$l])
    };
    ([$($l:literal),+]) => {
        Alt(&[$($l),+])
    };
}

/// `alts!["a", ["b", "c"]]` is the pattern `a|b.*c`.
macro_rules! alts {
    ($($p:tt),+ $(,)?) => {
        &[$(alt!($p)),+]
    };
}

/// [hue, saturation, lightness].
type Family = (&'static [Alt], [f64; 3]);

/// Lines 1012-1034, in the page's order: the first rule that matches wins.
static BIOME_FAMILIES: &[Family] = &[
    (alts!["frozen_ocean", "frozen_river"], [200.0, 50.0, 70.0]),
    (alts![["deep", "ocean"]], [222.0, 55.0, 30.0]),
    (alts!["warm_ocean", "lukewarm"], [190.0, 60.0, 45.0]),
    (alts!["cold_ocean"], [215.0, 35.0, 45.0]),
    (alts!["ocean", "river"], [212.0, 55.0, 42.0]),
    // snowy_taiga|snowy.*(spruce|pine)
    (alts!["snowy_taiga", ["snowy", "spruce"], ["snowy", "pine"]], [175.0, 18.0, 74.0]),
    (alts!["ice", "frozen", "glacial", "snowy", "wintry", "siberian", "frostfire"], [200.0, 25.0, 88.0]),
    (alts!["alpine", "grove", "shield", "taiga", "spruce", "pine"], [158.0, 32.0, 34.0]),
    (alts!["oasis"], [150.0, 50.0, 62.0]),
    (alts!["beach", "shore"], [48.0, 45.0, 80.0]),
    (alts!["badlands", "mesa", "canyon", "bryce", "painted", "red_", "scarlet"], [18.0, 62.0, 50.0]),
    (alts!["desert", "sands", "dune"], [44.0, 62.0, 68.0]),
    (alts!["arid", "steppe"], [38.0, 40.0, 46.0]),
    (alts!["savanna", "shrubland", "brushland", "ashen"], [62.0, 38.0, 50.0]),
    (alts!["jungle", "tropical", "rainforest", "bamboo"], [112.0, 62.0, 34.0]),
    (alts!["swamp", "marsh", "mangrove"], [150.0, 30.0, 28.0]),
    (alts!["cherry", "sakura", "lavender", "blooming", "moonlight", "amethyst", "orchid", "mushroom", "warped"], [300.0, 40.0, 66.0]),
    (alts!["volcanic", "caldera", "crater", "basalt", "yellowstone"], [8.0, 30.0, 38.0]),
    (
        alts!["peaks", "mountain", "cliffs", "spires", "stony", "rocky", "windswept", "granite", "haze", "emerald", "yosemite", "highlands"],
        [30.0, 8.0, 56.0],
    ),
    (alts!["forest", "birch", "woodland", "cloud"], [128.0, 42.0, 36.0]),
    (alts!["plains", "meadow", "valley", "lowlands", "field", "clearing", "lush"], [92.0, 45.0, 56.0]),
];

/// Lines 1062-1076.
static STRUCTURE_FAMILIES: &[Family] = &[
    (alts!["underground", "mineshaft", "ancient_city", "stronghold", "trial", "dungeon", "catacomb", "crypt", "mines\\b"], [214.0, 60.0, 66.0]),
    (alts!["treasure", "buried"], [52.0, 92.0, 56.0]),
    (alts!["village", "town", "settlement", "hamlet"], [100.0, 62.0, 48.0]),
    (alts!["monument"], [226.0, 78.0, 62.0]),
    (alts!["ocean_ruin", "underwater", "coral"], [174.0, 70.0, 42.0]),
    (alts!["portal"], [282.0, 58.0, 66.0]),
    (alts!["pyramid", "temple", "hut", "igloo", "mansion", "tower", "spire", "castle", "palace", "keep", "fort", "lodge", "mage"], [326.0, 64.0, 68.0]),
    (alts!["outpost", "pillager", "illager", "bandit", "raid"], [0.0, 76.0, 56.0]),
    (alts!["shipwreck", "ship", "boat", "typhon", "pirate"], [24.0, 82.0, 52.0]),
    (alts!["rubble"], [48.0, 70.0, 60.0]),
    // A camp left behind is neither an outpost nor a wreck: a quiet earth colour.
    (alts!["camp"], [30.0, 28.0, 58.0]),
    (alts!["ruin", "trail"], [40.0, 38.0, 64.0]),
];

fn family_of(table: &'static [Family], name: &str) -> Option<[f64; 3]> {
    table.iter().find(|(alts, _)| any(alts, name)).map(|(_, hsl)| *hsl)
}

/// Lines 1035-1039: FNV-1a over the UTF-16 units of the string, as a fraction of 2^32.
pub fn hash_of(s: &str) -> f64 {
    let mut h: u32 = 2166136261;
    for unit in s.encode_utf16() {
        // Math.imul(h ^ unit, 16777619): the same 32 bits as a wrapping product of unsigned ones.
        h = (h ^ unit as u32).wrapping_mul(16777619);
    }
    h as f64 / 4294967296.0
}

/// Lines 1040-1045.
pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    let (s, l) = (s / 100.0, l / 100.0);
    let k = |n: f64| (n + h / 30.0) % 12.0;
    let a = s * l.min(1.0 - l);
    let f = |n: f64| {
        let kn = k(n);
        let v = l - a * (-1.0f64).max((kn - 3.0).min((9.0 - kn).min(1.0)));
        (255.0 * v + 0.5).floor() as u8
    };
    [f(0.0), f(8.0), f(4.0)]
}

/// Lines 1046-1054. A name with no colon has no second part: the page then tests the word "undefined".
pub fn biome_colour(id: &str) -> [u8; 3] {
    let name = id.split(':').nth(1).unwrap_or("undefined");
    let (h, s, l) = match family_of(BIOME_FAMILIES, name) {
        Some([h, s, l]) => (h, s, l),
        None => {
            // A name no rule knows: say so, the grey-green it gets tells nothing.
            mere_base::observe!(Info, "No colour family for {id}");
            (100.0, 20.0, 50.0)
        }
    };
    let vary = hash_of(id);
    hsl_to_rgb((h + (vary - 0.5) * 22.0 + 360.0) % 360.0, s, 12.0f64.max(92.0f64.min(l + (hash_of(&format!("{id}!")) - 0.5) * 16.0)))
}

/// Lines 1077-1083. `set` is the structure's set when the file gives one.
pub fn structure_colour(id: &str, set: Option<&str>) -> [u8; 3] {
    let name = format!("{} {}", id.rsplit(':').next().unwrap_or(""), set.unwrap_or("").rsplit(':').next().unwrap_or(""));
    let (h, s, l) = match family_of(STRUCTURE_FAMILIES, &name) {
        Some([h, s, l]) => (h, s, l),
        // A family no rule knows still gets a hue of its own, from its set, but a quieter one.
        None => (hash_of(set.unwrap_or(id)) * 360.0, 34.0, 62.0),
    };
    hsl_to_rgb((h + (hash_of(id) - 0.5) * 14.0 + 360.0) % 360.0, s, l + (hash_of(&format!("{id}!")) - 0.5) * 12.0)
}

/// Line 1088: built under the ground, by the step the file declares, by the name, or a stronghold.
pub fn underground(id: &str, step: Option<&str>) -> bool {
    step.unwrap_or("").starts_with("underground") || id.contains("underground") || id == "minecraft:stronghold"
}

/// Line 1089.
pub fn is_water(id: &str) -> bool {
    id.contains("ocean") || id.contains("river")
}

/// Lines 1091-1102: lowland green, then ochre, rock, snow; below the sea, deeper is darker.
pub fn height_colour(y: f64, sea: f64) -> [u8; 3] {
    if y < sea {
        let d = ((sea - y) / 60.0).min(1.0);
        return hsl_to_rgb(212.0, 55.0, 46.0 - d * 26.0);
    }
    let stops: [(f64, [f64; 3]); 6] = [
        (0.0, [110.0, 40.0, 42.0]),
        (40.0, [80.0, 40.0, 50.0]),
        (90.0, [40.0, 38.0, 48.0]),
        (150.0, [25.0, 20.0, 45.0]),
        (210.0, [0.0, 0.0, 70.0]),
        (260.0, [0.0, 0.0, 96.0]),
    ];
    let up = y - sea;
    for i in 1..stops.len() {
        if up <= stops[i].0 {
            let (a, ca) = stops[i - 1];
            let (b, cb) = stops[i];
            let t = (up - a) / (b - a);
            return hsl_to_rgb(ca[0] + (cb[0] - ca[0]) * t, ca[1] + (cb[1] - ca[1]) * t, ca[2] + (cb[2] - ca[2]) * t);
        }
    }
    [245, 245, 245]
}

/// Lines 1105-1108: two hues only, so a step in colour is a step in temperature.
pub fn temperature_colour(t: f64) -> [u8; 3] {
    let p = 0.0f64.max(1.0f64.min((t + 0.7) / 2.7));
    if p < 0.5 { hsl_to_rgb(214.0, 62.0, 42.0 + p * 2.0 * 46.0) } else { hsl_to_rgb(10.0, 68.0, 88.0 - (p - 0.5) * 2.0 * 42.0) }
}

/// Line 1109.
pub fn flat_colour() -> [u8; 3] {
    hsl_to_rgb(92.0, 48.0, 62.0)
}
pub fn steep_colour() -> [u8; 3] {
    hsl_to_rgb(30.0, 8.0, 30.0)
}
pub fn flat_sea_colour() -> [u8; 3] {
    hsl_to_rgb(212.0, 45.0, 26.0)
}

/// Line 1116: water keeps the colour of its temperature, seen through this blue.
pub const SEA_VEIL: [u8; 3] = [28, 62, 130];

/// Lines 1112-1113: the game gives every ocean but the frozen one the same temperature, so the name is read.
pub fn sea_temperature(id: &str) -> Option<f64> {
    if !id.contains("ocean") {
        None
    } else if id.contains("frozen") {
        Some(-0.7)
    } else if id.contains("cold") {
        Some(-0.025)
    } else if id.contains("lukewarm") {
        Some(1.325)
    } else if id.contains("warm") {
        Some(2.0)
    } else {
        Some(0.65)
    }
}

/// Line 1114.
pub fn climate(b: &Biome) -> f64 {
    sea_temperature(&b.id).unwrap_or(b.temperature)
}
