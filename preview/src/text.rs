//! The page's wording (preview.html, line numbers in the comments). Nothing is written here that the page
//! does not say.

pub const WINDOW_TITLE: &str = "World preview";
/// Line 455.
pub const WORLD: &str = "World";
/// Line 456.
pub const SEED: &str = "Seed";
/// Line 457.
pub const SEED_RANDOM: &str = "random";
/// Line 458.
pub const SPAN: &str = "Scanned area";
/// Line 459: one choice of the scanned area, in blocks.
pub fn span_value(blocks: f64) -> String {
    format!("{} blocks", num(blocks))
}
/// Line 581.
pub const PROFILES: &str = "Settings";
/// Line 582.
pub const PACK: &str = "Pack";
/// The filter at the top of the settings panel: only the groups that relate to the pressed map. Not the page's.
/// It names the map, `view` as `PreviewState::view` (0 to 4 the strip's maps, 5 the section).
pub fn view_only(view: usize) -> String {
    match VIEWS.get(view) {
        Some(name) => format!("Only for the {name} map"),
        None => "Only for the section".to_string(),
    }
}
/// The filter's help: its title, then its text.
pub const VIEW_ONLY_TITLE: &str = "Only the settings for this map";
pub const VIEW_ONLY_HINT: &str = "Lists only the settings that change what the pressed map draws. The others are hidden, not reset: they keep their values.";
/// How many settings the filter leaves on show, out of all of them.
pub fn shown_of(shown: usize, all: usize) -> String {
    format!("{shown} of {all}")
}
/// Line 467.
pub const UP_TO_DATE: &str = "The map shows these settings.";
/// Line 464, `detailValue` of 64, 32 and 16.
pub const DETAILS: [&str; 3] = ["64 blocks", "32 blocks", "16 blocks"];
/// Line 462.
pub const GENERATE: &str = "Generate preview";
/// Line 480.
pub const EXPORT: &str = "Export";
/// Line 491.
pub const FIGURES: &str = "In figures";
/// Line 492.
pub const HISTORY: &str = "History";
/// Line 588.
pub const VIEWS: [&str; 5] = ["Biomes", "Flat", "Relief", "Temperature", "Structures"];
/// Line 619.
pub const SECTION: &str = "Section";
/// Line 589.
pub const SHAPES: [&str; 2] = ["2D", "3D"];
/// How finely the 3D view draws the land, in `scene::Level::ALL`'s order, and what each one's title says.
pub const LEVELS: [&str; 3] = ["Slopes", "Blocks", "Fine blocks"];
pub const LEVEL_HINTS: [&str; 3] = [
    "A sloped surface: the lightest, for any machine.",
    "One block per point of the land, 512 a side at most.",
    "One block per square of the 2D map: the finest and the heaviest.",
];
/// Line 508, the height view: its title and the name of its first column.
pub const LIST_HEIGHT: [&str; 2] = ["Heights", "height"];
/// Line 520.
pub const COL_WIDTH: &str = "typical width";
/// Line 521.
pub const COL_SHARE: &str = "share";
/// Line 510.
pub const PICK_HINT: &str = "Click rows, here or on the map.";
/// Under the settings: what the grey value beside a field is. Not the page's (it had a "pack…" to click).
pub const PACK_NOTE: &str = "A grey value beside a field is the pack\u{2019}s: the field differs from it.";
/// The tip of that grey value.
pub const PACK_VALUE_HINT: &str = "The pack\u{2019}s value";
/// The arrow in a group's title, shown while one of its settings differs from the pack's.
pub const GROUP_RESET_HINT: &str = "Put this group back to the pack\u{2019}s values";
/// The second fixed set of values, beside the pack's: Tectonic's own defaults.
pub const ORIGINAL: &str = "Original";
pub const ORIGINAL_HINT: &str = "Put everything back to Tectonic\u{2019}s own values";
/// What stands before the two choices that put everything back.
pub const BACK_TO: &str = "Back to";
/// The last line of a setting's help: Tectonic's own value, the pack's, and the range the field takes (a switch
/// has none). The values come as the fields write them.
pub fn help_facts(original: &str, pack: &str, range: Option<(&str, &str)>) -> String {
    match range {
        Some((min, max)) => format!("Original {original} \u{b7} Pack {pack} \u{b7} from {min} to {max}"),
        None => format!("Original {original} \u{b7} Pack {pack}"),
    }
}
/// Lines 528-529.
pub const ON: &str = "on";
pub const OFF: &str = "off";
/// Line 525, in `groupOrder` order (the ores group is not built: it shows in the section view only).
pub const GROUPS: [&str; 5] = ["Biome size and climate", "Relief", "Continents, oceans and islands", "Caves", "Not drawn here"];
/// Lines 605-611, in key order.
pub const FIGURE_NAMES: [&str; 7] = ["Flat land", "Typical flat zone", "Typical biome", "Sea", "High land", "Highest peak", "Biomes present"];

/// Line 468.
pub const GENERATING: &str = "Generating\u{2026}";
/// Line 469, kept for the tooltips of a later step.
pub const STOP: &str = "Stop the run";
/// Line 470.
pub fn running(elapsed: &str, about: &str) -> String {
    format!("{elapsed} of about {about}.")
}
/// Line 471.
pub fn measuring(elapsed: &str) -> String {
    format!("{elapsed}. A first run at this detail: its time is kept for the next ones.")
}
/// Line 473.
pub const STEP_START: &str = "Starting the game";
/// Line 474.
pub fn step_sketch(step: u32) -> String {
    format!("Sketch, every {step} blocks")
}
/// Line 475.
pub fn step_last(step: u32) -> String {
    format!("Map, every {step} blocks")
}
/// Line 476.
pub const STEP_EXTRAS: &str = "Cuts, ores and structures";
/// Line 477: said after the clock while the stage shows a sketch.
pub const SKETCH_READABLE: &str = "The sketch on the map can be read already.";
/// Not the page's, which shows the map without marks and an empty cut: what a view says while the run that will
/// give it is still under way.
pub const STRUCTURES_PENDING: &str = "The structures come at the end of the run.";
pub const SECTION_PENDING: &str = "The cuts come at the end of the run.";
/// The title of a view greyed for that reason.
pub const COMES_AT_THE_END: &str = "Comes at the end of the run.";
/// Line 479.
pub const STOPPED: &str = "Stopped. The map is the one from before.";
/// Line 482.
pub const EXPORTING: &str = "Exporting\u{2026}";
/// Line 483.
pub fn export_step(view: &str) -> String {
    format!("Pictures: {view}")
}
/// Line 485.
pub const EXPORTED: &str = "Exported.";
/// Line 486.
pub const EXPORTED_TO: &str = "Exported to";
/// Line 487.
pub const OPEN_FOLDER: &str = "Open the folder";
/// Line 488.
pub const CLOSE_NOTICE: &str = "Close";
/// Line 489.
pub const OPEN_FAILED: &str = "The folder could not be opened: it may have been moved or deleted.";
/// Line 490.
pub const EXPORT_FAILED: &str = "The export failed.";
/// Line 590.
pub fn done(wait: &str) -> String {
    format!("Preview ready in {wait}.")
}
/// Line 591.
pub const FAILED: &str = "The run failed. Try again, then see last-run.log in the preview folder.";
/// Line 592.
pub const REFUSED: &str = "Seed refused: it takes letters, digits, spaces and hyphens.";
/// Line 2571 and 2579: the marks of a done step and of the one under way.
pub const DONE_MARK: &str = "\u{2713}";
pub const NOW_MARK: &str = "\u{203a}";

/// Line 461.
pub fn duration(s: u64) -> String {
    if s < 60 {
        format!("{s} s")
    } else if s % 60 == 0 {
        format!("{} min", s / 60)
    } else {
        format!("{} min {} s", s / 60, s % 60)
    }
}

/// Not the page's: its server never hands over a folder it cannot read. Written in the page's wording for the one
/// case a native window has, a preview folder that is there and cannot be read.
pub const UNREADABLE: &str = "The preview could not be read. Generate it again.";
/// Line 593.
pub const NO_PREVIEW: &str = "No preview yet. Choose a seed and generate one.";
/// Line 599.
pub const NO_STRUCTURES: &str = "No structures for this preview. Generate it again.";

/// Line 597: `cut` says the ticks of the section's cut are on the frame.
pub fn area_probe(blocks: &str, square: &str, cut: bool) -> String {
    format!("{blocks} \u{d7} {blocks} blocks, north up. One square: {square} blocks. Cross: 0,0.{}", if cut { " Ticks: the cut." } else { "" })
}

/// Line 596: three spaces between the fields.
pub fn probe(x: &str, z: &str, y: &str, biome: &str) -> String {
    format!("x {x}   z {z}   height {y}   {biome}")
}

/// Line 602.
pub fn structure_probe(x: &str, z: &str, name: &str) -> String {
    format!("x {x}   z {z}   {name}")
}

/// Line 652 with no decimals: `toLocaleString('en-US')`, groups of three, halves away from zero.
pub fn num(v: f64) -> String {
    let r = (v.abs() + 0.5).floor() as u64;
    let digits = r.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    if v < 0.0 && r != 0 { format!("-{grouped}") } else { grouped }
}

/// Lines 1120-1123: the name alone, without the mod it comes from, in words.
pub fn pretty(id: &str) -> String {
    let name = id.rsplit(':').next().unwrap_or("");
    let mut words = String::new();
    let mut gap = false;
    for c in name.chars() {
        if c == '_' || c == '/' {
            // A run of them is one space.
            gap = true;
        } else {
            if gap {
                words.push(' ');
                gap = false;
            }
            words.push(c);
        }
    }
    if gap {
        words.push(' ');
    }
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Line 508: what the list beside the map holds in each view, by `View::index`: its title and the name of its
/// first column.
pub const LISTS: [[&str; 2]; 5] = [["Biomes", "biome"], ["Ground", "ground"], ["Heights", "height"], ["Temperatures", "climate"], ["Structures", "structure"]];
/// Line 522.
pub const COL_SPACING: &str = "one every";
/// Line 524.
pub const COL_COUNT: &str = "count";
/// Line 512: no structure is drawn until it is chosen.
pub const PICK_HINT_MARKS: &str = "Click the rows to draw on the map. None is drawn until then.";
/// Line 518.
pub const PICK_NONE: &str = "Show all";
/// Line 513.
pub const PICK_NONE_MARKS: &str = "Draw none";
/// Line 514.
pub fn picked_many(count: usize) -> String {
    format!("{count} chosen")
}
/// Line 515.
pub fn picked_share(share: &str) -> String {
    format!(" \u{b7} {share} of the area")
}
/// Line 516.
pub fn picked_count(count: &str, of: &str) -> String {
    format!(" \u{b7} {count} of {of}")
}
/// Line 519.
pub const FILTER: &str = "Filter by name\u{2026}";
/// Line 614.
pub const ABSENT: &str = "Not in this area";
/// Line 600: the structures' two parts, surface first.
pub const PARTS: [&str; 2] = ["On the surface", "Under the ground"];
/// Line 648.
pub const LEGEND_FLAT: [&str; 3] = ["Flat", "Slopes", "Water"];
/// Line 649.
pub const LEGEND_TEMPERATURE: [&str; 5] = ["Frozen", "Cold", "Temperate", "Warm", "Scorching"];
/// Line 502.
pub const CHANGE: &str = "change";
/// Line 504.
pub const SAME: &str = "=";
/// Line 505.
pub const CLOSE: &str = "\u{2248}";
/// Line 506.
pub const POINTS: &str = " pt";
/// Line 613.
pub fn count_of(count: usize, total: usize) -> String {
    format!("{count} of {total}")
}
/// Line 617: a whole number of blocks as JavaScript writes a number.
pub fn altitude(v: f64) -> String {
    format!("height {v}")
}
/// Line 495.
pub const HISTORY_FEW: &str = "Generate again with this seed, over this area, at this detail, and each figure gets a line from run to run.";
/// Line 496.
pub fn history_hint(n: usize) -> String {
    format!("The {n} runs of this seed over this area, at this detail, oldest left. Point at one to see what it changed.")
}
/// Line 497.
pub fn history_run(at: usize, of: usize, what: &str) -> String {
    format!("Run {at} of {of}: {what}.")
}
/// Line 498.
pub const HISTORY_FIRST: &str = "the first one kept";
/// Line 499.
pub const HISTORY_SAME: &str = "same settings";
/// Line 500: `from` and `to` as JavaScript writes a number.
pub fn history_moved(name: &str, from: f64, to: f64) -> String {
    format!("{name} {from} \u{2192} {to}")
}
/// Line 501.
pub fn history_many(count: usize) -> String {
    format!("{count} settings changed")
}

/// Line 620.
pub const NO_SECTION: &str = "No section for this preview. Generate it again.";
/// Line 621.
pub const NEXT_CUT: &str = "Another cut";

/// Line 623: `at` and `of` are printed as they are, `z`, `from` and `to` already formatted.
pub fn cut_rest(at: usize, of: usize, z: &str, from: &str, to: &str) -> String {
    format!("Cut {at} of {of}, west to east at z {z}, x {from} to {to}. True scale. Wheel: closer.")
}

/// Line 624.
pub fn nav_note(times: &str) -> String {
    format!("The whole cut, heights \u{d7} {times}. Click or drag the frame to look along it.")
}

/// Line 625: three spaces between the fields. `y` is a whole number of blocks.
pub fn cut_probe(x: &str, y: f64, depth: &str, what: &str, biome: &str) -> String {
    format!("x {x}   height {y}   {depth}   {what}   {biome}")
}

/// Line 626: `depth` is a whole number of blocks.
pub fn cut_depth(depth: f64) -> String {
    if depth > 0.0 { format!("{depth} under the ground") } else { "above the ground".to_string() }
}

/// Line 627, indexed by the cell: air, rock, water, lava.
pub const CUT_CELLS: [&str; 4] = ["Air", "Rock", "Water", "Lava"];
/// Line 628.
pub const CUT_EARTH: &str = "Top layers";
/// Line 629, in `section::ORE_KINDS` order.
pub const ORES: [&str; 8] = ["Coal", "Copper", "Iron", "Gold", "Lapis", "Redstone", "Diamond", "Emerald"];
/// Line 630: the list's title and the name of its first column.
pub const ORE_LIST: [&str; 2] = ["Ores", "ore"];
/// Line 631.
pub const ORE_COLUMNS: [&str; 2] = ["low ground", "high ground"];
/// Line 633.
pub const ORE_PICK_HINT: &str = "Click an ore to see it alone.";

/// Line 635.
pub fn ore_unplayed(count: usize) -> String {
    format!("{count} ore rules use a placement step this page cannot play: they are left out.")
}

/// Line 652 with `digits` fraction digits: `toLocaleString('en-US', { minimumFractionDigits: digits,
/// maximumFractionDigits: digits })`, halves away from zero, computed on the double as it is. UNVERIFIED for the
/// exact binary halves (Intl may round on the shortest decimal of the double instead); the Node test compares
/// the strings of the real data.
pub fn num_digits(v: f64, digits: usize) -> String {
    let scale = 10f64.powi(digits as i32);
    let r = (v.abs() * scale + 0.5).floor() as u64;
    let unit = scale as u64;
    let whole = (r / unit).to_string();
    let mut out = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if digits > 0 {
        out.push_str(&format!(".{:0width$}", r % unit, width = digits));
    }
    if v < 0.0 && r != 0 { format!("-{out}") } else { out }
}

/// Line 2303: how many times the heights are stretched, with a decimal below 1.95 and whole above.
pub fn times(v: f64) -> String {
    if v < 1.95 { num_digits(v, 1) } else { num((v + 0.5).floor()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numbers_are_written_as_the_page_writes_them() {
        assert_eq!(num_digits(1234.56, 1), "1,234.6");
        assert_eq!(num_digits(0.04, 1), "0.0");
        assert_eq!(num_digits(-0.04, 1), "0.0", "no minus sign on a zero");
        assert_eq!(num_digits(-1500.25, 1), "-1,500.3");
        assert_eq!(num_digits(7.0, 0), "7");
        assert_eq!(times(1.3), "1.3");
        assert_eq!(times(1.94), "1.9");
        assert_eq!(times(12.4), "12");
        assert_eq!(times(1.95), "2");
        assert_eq!(nav_note("12"), "The whole cut, heights \u{d7} 12. Click or drag the frame to look along it.");
    }

    #[test]
    fn the_runs_speak_as_the_page_does() {
        assert_eq!(GENERATING, "Generating…");
        assert_eq!(running("20 s", "50 s"), "20 s of about 50 s.");
        assert_eq!(measuring("5 s"), "5 s. A first run at this detail: its time is kept for the next ones.");
        assert_eq!((step_sketch(64).as_str(), step_last(16).as_str()), ("Sketch, every 64 blocks", "Map, every 16 blocks"));
        assert_eq!(STOPPED, "Stopped. The map is the one from before.");
        assert_eq!(done("1 min 5 s"), "Preview ready in 1 min 5 s.");
        assert_eq!(export_step("Biomes"), "Pictures: Biomes");
        assert_eq!((EXPORTED, EXPORTED_TO, OPEN_FOLDER, CLOSE_NOTICE), ("Exported.", "Exported to", "Open the folder", "Close"));
        assert_eq!(REFUSED, "Seed refused: it takes letters, digits, spaces and hyphens.");
        assert_eq!((DONE_MARK, NOW_MARK), ("✓", "›"));
    }

    #[test]
    fn the_cut_speaks_as_the_page_does() {
        assert_eq!(cut_rest(1, 4, "-1,504", "-2,048", "2,048"), "Cut 1 of 4, west to east at z -1,504, x -2,048 to 2,048. True scale. Wheel: closer.");
        assert_eq!(cut_probe("-100", 12.0, &cut_depth(5.0), "Rock", "Plains"), "x -100   height 12   5 under the ground   Rock   Plains");
        assert_eq!(cut_depth(0.0), "above the ground");
        assert_eq!(cut_depth(-3.0), "above the ground");
    }
}

// ---- the settings' help (lines 530-575, `text.settings`, the hint of each) ----

/// The mark after a setting's label that opens its help.
pub const HELP_MARK: &str = "?";

/// What each setting shapes in the world, then what lower does and what higher does, always in that order, keyed by the setting's key (`settings::SETTINGS`).
/// Ported from the page's hints, which say what changes in the direction that grows it, then checked against the
/// comments of the pack's `tectonic.json` and reworded where they were vague or said too little. A switch says what on
/// (or off) does, in one sentence.
pub const SETTING_HINTS: [(&str, &str); 43] = [
    ("temperature_scale", "How wide the warm and cold zones are. Lower: wider zones, larger biomes. Higher: narrower zones, smaller biomes."),
    ("vegetation_scale", "How wide the dry and wet zones are. Lower: wider zones, larger biomes. Higher: narrower zones, smaller biomes."),
    ("temperature_multiplier", "How far the climate swings between hot and cold. Lower: fewer deserts and frozen lands. Higher: more of both."),
    ("vegetation_multiplier", "How far the land swings between dry and wet. Lower: fewer very dry and very wet biomes. Higher: more of both."),
    ("temperature_offset", "Shifts the whole world's climate. Lower: a colder world, more snowy biomes. Higher: a warmer world, more hot biomes."),
    ("vegetation_offset", "Shifts the whole world's humidity. Lower: a drier world. Higher: a wetter world, with more forests and jungles."),
    ("vertical_scale", "Stretches the land above sea level. Lower: flatter relief. Higher: taller relief; doubling it doubles every height above the sea."),
    ("elevation_boost", "Adds to the vertical scale on mountains and plateaus only. Lower: they rise like the lowlands. Higher: they rise faster than the lowlands."),
    ("flat_terrain_skew", "Where plateaus give way to flat ground. Lower: more plateaus. Higher: more flat land."),
    ("erosion_scale", "How wide the mountain ranges are. Lower: thicker ranges, wider land between them. Higher: thinner ranges, closer together."),
    ("ridge_scale", "How wide rivers and the ground along them are. Lower: wider rivers, broader valleys. Higher: narrower rivers, closer together."),
    ("rolling_hills", "On: some plains get very smooth, rolling hills."),
    ("jungle_pillars", "On: some jungles get huge pillars, up to a hundred blocks tall."),
    ("ultrasmooth", "On: much smoother ground with no staircase steps, but deep oceans and windswept biomes come out broken."),
    ("alternate_erosion_scaling", "Experimental: another way for Tectonic to apply the erosion scale, which shapes mountain ranges."),
    ("min_y", "The lowest height of the world, in steps of 16 blocks. Lower: a deeper world, more room under the ground. Higher: a shallower one."),
    ("max_y", "The highest height of the world, in steps of 16 blocks. Lower: less room for mountains. Higher: room for taller mountains."),
    ("continents_scale", "How large continents are. Lower: larger continents, larger oceans between them. Higher: smaller ones, closer together."),
    ("ocean_offset", "Lower: more ocean. Higher: more land. Above \u{2212}0.45 there is no deep ocean, so no islands."),
    ("alternate_continents_scaling", "Experimental: another way for Tectonic to apply the continent scale."),
    ("ocean_depth", "The height of the ocean floor; 0 is sea level, \u{2212}0.5 is 64 blocks under it. Lower: deeper oceans. Higher: shallower ones."),
    ("deep_ocean_depth", "The height of the deep ocean floor; 0 is sea level, \u{2212}0.5 is 64 blocks under it. Lower: deeper deep oceans. Higher: shallower ones."),
    ("enabled", "On: Tectonic raises islands out of the deep ocean, and nowhere else."),
    ("noise_scale", "How large islands are. Lower: larger islands, further apart. Higher: smaller islands, closer together."),
    ("noise_multiplier", "How strongly the island land swings up and down. Lower: fewer, lower islands. Higher: more of the deep ocean rises into islands."),
    ("noise_offset", "Raises the island land everywhere in the deep ocean. Lower: fewer islands. Higher: more islands."),
    ("cheese_enabled", "On: the large, open caves, with wide halls and pillars of stone."),
    ("cheese_additive", "How much rock fills the cheese caves. Lower: bigger cheese caves. Higher: smaller ones."),
    ("spaghetti_enabled", "On: the long, winding tunnels of middle width."),
    ("noodle_enabled", "On: the narrow, cramped tunnels."),
    ("noodle_additive", "How much rock fills the noodle caves. Lower: bigger noodle caves. Higher: smaller ones."),
    ("depth_cutoff_start", "How far under the ground caves stop; 0.1 is about 12 blocks, 0.5 about 64. Lower: caves nearer the surface. Higher: a thicker roof."),
    ("depth_cutoff_size", "Over how many blocks caves fade out under that roof; 0.1 is about 12 blocks. Lower: a sharper cut. Higher: a slower close."),
    ("lava_tunnels", "On: rare, deep tunnels of lava near the bottom of the world."),
    ("underground_rivers", "On: rivers that reach a mountain range go on under it, so a boat can follow them."),
    ("carvers_enabled", "On: the old caves and ravines, carved once the ground is made. The section does not show them."),
    ("ore_fix", "On: ores are placed at better heights when the bottom of the world is lowered."),
    ("mod_enabled", "Off: the world is made without Tectonic, to compare against."),
    ("snow_start_offset", "Moves the height where snow starts in cold biomes such as taigas. Lower: snow starts lower down. Higher: snow starts higher up."),
    ("monument_offset", "Moves ocean monuments down to sit on deeper ocean floors. Lower: deeper monuments. Higher: monuments nearer the surface."),
    ("river_ice", "On: underground rivers under snowy biomes are covered with ice."),
    ("river_lanterns", "On: lanterns hang, rarely, over underground rivers."),
    ("remove_frozen_ocean_ice", "On: frozen oceans lose their ice, so they are as easy to sail as deep frozen oceans."),
];

/// The help of the setting `key`, or "" for a key with none.
pub fn setting_hint(key: &str) -> &'static str {
    SETTING_HINTS.iter().find(|(k, _)| *k == key).map_or("", |(_, h)| *h)
}

// ---- the launcher's bar (outpost-tools\index.html, `text`, lines 336-415) ----

/// Line 337: the application's name, Outpost Tools (the launcher's spec), which the bar shows in
/// capitals and the window's title says. No Mojang trademark: the game's name stays in sentences about the game.
pub const BAR_TITLE: &str = "Outpost Tools";
/// Line 341: the two tools, Home first.
pub const MODES: [&str; 2] = ["Home", "World preview"];
/// What the bar's two tools say on their buttons, beside their icons. The longer `MODES` stay their tooltips.
pub const TOOL_NAMES: [&str; 2] = ["Home", "Preview"];
/// The strip's last tab when the bar lacks room: it holds the views that no longer fit, and its tooltip.
pub const MORE: &str = "More";
pub const MORE_TITLE: &str = "More views";
/// The tooltips of the bar's two buttons that open and close the side panels (preview tool only), and of the grip
/// between a panel and the stage. Not the page's: the page has fixed panels.
pub const SETTINGS_PANEL: &str = "Settings panel";
pub const FIGURES_PANEL: &str = "Figures panel";
pub const RESIZE_PANEL: &str = "Drag to resize, double-click to reset";
/// Line 342: where a run of the preview stands, by marker.
pub const PREVIEW_STATE: [&str; 3] = ["computing", "ready", "failed"];
/// Line 343.
pub fn mode_with(name: &str, marker: &str) -> String {
    format!("{name} \u{b7} {marker}")
}
/// Line 344.
pub fn title_with(marker: &str) -> String {
    format!("{BAR_TITLE} \u{b7} preview {marker}")
}
/// Lines 410-413: the window's own buttons.
pub const MINIMIZE: &str = "Minimise";
pub const MAXIMIZE: &str = "Maximise";
pub const RESTORE: &str = "Restore";
pub const CLOSE_WINDOW: &str = "Close";
/// Line 466 of the preview page: the title of a view that needs the full detail, as the page sends it to the host.
pub const NEEDS_FULL: &str = "Needs the 16-block detail: generate the map again at 16 blocks.";
/// PLACEHOLDER: the launcher's Home is step 21; until then its pane says so. Not the launcher's wording.
pub const HOME_LATER: &str = "Home belongs to the launcher, not to this window.";

// ---- the titles (the page's `title` attributes) ----

/// Line 460.
pub const SPAN_COARSE: &str = "Past 32,768 blocks, a point every 32 blocks at the finest.";
/// Line 465.
pub const DETAIL_HINT: &str = "Blocks between two points of the map. A coarser map comes sooner, without the flat land, the structures and the section.";
/// Line 481.
pub const EXPORT_HINT: &str = "Keeps the settings, the figures, and a picture of each view, flat and in 3D as it is looked at now, in a folder.";
/// Line 484.
pub const EXPORT_DATA: &str = "Settings and figures";
/// Line 503.
pub fn was(v: &str) -> String {
    format!("before: {v}")
}
/// Line 517.
pub fn pick_remove(name: &str) -> String {
    format!("Remove {name}")
}
/// Line 523.
pub const COL_SPACING_HINT: &str = "About how far apart two of them are: the side of the square each has to itself on this area.";
/// Line 583.
pub const PROFILE_PACK_HINT: &str = "Put everything back to the pack\u{2019}s values";
/// Line 601.
pub const PART_HINT: &str = "Click to draw this whole part; again to draw none.";
/// Line 615.
pub const ABSENT_HINT: &str = "Biomes the generator can place at the surface, that this area does not hold.";
/// Line 622.
pub const NEXT_CUT_HINT: &str = "The cuts are the lines of this map that cross the most different ground: a height, some sea, some level land.";
/// Line 632.
pub fn ore_columns_hint(plays: &str) -> String {
    format!("Blocks of ore per chunk within 64 blocks under the ground: where the ground is under height 80, and from 80 up. Counted over {plays} chunks played along this cut.")
}
/// Line 634.
pub fn ore_from(rules: &str) -> String {
    format!("From: {rules}")
}
/// Lines 605-611: what each figure is, shown on hover, in the order of `FIGURE_NAMES`.
pub const FIGURE_DEFS: [&str; 7] = [
    "Share of the land. Flat: the ground moves by 2 blocks at most to the neighbour points, 16 blocks away. A lone flat point does not count.",
    "Width of the flat zone a flat point taken at random falls in. A river does not cut a zone.",
    "Width of the biome zone a point taken at random on land falls in.",
    "Share of the whole area below sea level, rivers and lakes included.",
    "Share of the land higher than 150.",
    "",
    "Surface biomes met in this area, of all those the generator can place. The others are named at the end of the list of biomes.",
];
/// Apply to pack: the settings' values written into the pack's Tectonic config.
pub const APPLY_TO_PACK: &str = "Apply to pack";
pub const APPLY_HINT: &str = "Write these values into the pack\u{2019}s Tectonic config, for the game to use";
pub const APPLY_NO_PACK: &str = "No pack is known: start with --pack";
pub const APPLY_SAME: &str = "The pack already holds these values";
pub const APPLY_WORKING: &str = "Applying\u{2026}";
pub const APPLIED: &str = "Applied to the pack.";
pub const APPLY_GAME: &str = "The game is running: it takes them at its next start.";
pub const APPLY_SERVER: &str = "The dev server is running: it takes them at its next start.";
pub const APPLY_BOTH: &str = "The game and the dev server are running: they take them at their next start.";
pub const APPLY_FAIL_READ: &str = "The pack\u{2019}s config could not be read.";
pub const APPLY_FAIL_WRITE: &str = "The pack\u{2019}s config could not be written.";
pub const APPLY_FAIL_KEY: &str = "The pack\u{2019}s config does not look as expected. Nothing was written.";
pub const APPLY_SEAMS: &str = "On a world that already exists, new values leave visible seams between old and new chunks. They take full effect on a new world.";
