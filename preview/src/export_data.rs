//! `export.json` (page lines 2779-2786): the settings, the figures and the lists of a try in one file, beside its
//! pictures. Written by hand in the page's key order, as `JSON.stringify(.., null, 2)` lays it out, because the crate
//! has a reader for JSON and no writer, and one file does not call for one.

use std::fmt::Write as _;

use crate::figures::{Figures, List, json_number};
use crate::paint::View;
use crate::section::{ORE_KINDS, OreFigure};
use crate::settings::SETTINGS;
use crate::world::World;

/// A JSON string: quotes, backslashes and control characters escaped, the rest as it is.
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The rows of one list as the page's export gives them (line 2771-2774): a counted list keeps the rows that have
/// a count, biggest first; a measured one the rows that have a share, biggest first only for the biomes.
fn rows(list: &List, samples: usize, sort_by_share: bool) -> String {
    let total = (samples * samples) as f64;
    let mut found: Vec<(usize, f64)> = (0..list.ids.len())
        .map(|i| (i, if list.counted { list.cells[i] } else { 100.0 * list.cells[i] / total }))
        .filter(|(_, v)| *v != 0.0)
        .collect();
    // Array.prototype.sort is stable, and so is sort_by.
    if list.counted || sort_by_share {
        found.sort_by(|a, b| b.1.total_cmp(&a.1));
    }
    let round = |v: f64| (v + 0.5).floor();
    let items: Vec<String> = found
        .iter()
        .map(|&(i, v)| {
            let (id, name, width) = (quote(&list.ids[i]), quote(&list.names[i]), json_number(Some(round(list.widths[i]))));
            if list.counted {
                format!("{{\"id\":{id},\"name\":{name},\"count\":{},\"spacing\":{width}}}", json_number(Some(v)))
            } else {
                format!("{{\"id\":{id},\"name\":{name},\"share\":{},\"width\":{width}}}", json_number(Some(v)))
            }
        })
        .collect();
    format!("[{}]", items.join(","))
}

fn ore_figures(per_chunk: &[Option<OreFigure>; 8]) -> String {
    let items: Vec<String> = (0..ORE_KINDS.len())
        .filter_map(|k| {
            let f = per_chunk[k].as_ref()?;
            let from: Vec<String> = f.from.iter().map(|s| quote(s)).collect();
            Some(format!("{}:{{\"low\":{},\"high\":{},\"from\":[{}]}}", quote(ORE_KINDS[k]), json_number(f.low), json_number(f.high), from.join(",")))
        })
        .collect();
    format!("{{{}}}", items.join(","))
}

/// What is known of the try: `kinds` are the views that were exported, `values` the tectonic settings it was made with, `before` the figures of the run before, kept only when it was the same span.
/// `per_chunk` is the cut's ore figures, None when no cut is shown. The added rows the page lets one edit have no
/// native editor yet, so `added` is always null.
pub fn export_json(world: &World, seed: &str, values: &[f64; 43], kinds: &[View], before: Option<&Figures>, per_chunk: Option<&[Option<OreFigure>; 8]>) -> String {
    let figures = &world.measured.figures;
    let tect: Vec<String> = SETTINGS.iter().zip(values).map(|(s, v)| format!("{}:{}", quote(s.key), json_number(Some(*v)))).collect();
    let pack: Vec<String> = SETTINGS.iter().map(|s| format!("{}:{}", quote(s.key), json_number(Some(s.pack)))).collect();
    let same_span = before.filter(|b| b.span == figures.span);
    let lists: Vec<String> = kinds
        .iter()
        .filter_map(|k| {
            let list = world.measured.lists[k.index()].as_ref()?;
            Some(format!("{}:{}", quote(k.key()), rows(list, world.samples, *k == View::Biomes)))
        })
        .collect();
    let absent: Vec<String> = world.absent.iter().map(|s| quote(s)).collect();
    let ores = per_chunk.map_or("null".to_string(), |p| format!("{{\"added\":null,\"perChunk\":{}}}", ore_figures(p)));
    format!(
        "{{\n  \"seed\": {},\n  \"blocks\": {},\n  \"seaLevel\": {},\n  \"tectonic\": {{{}}},\n  \"pack\": {{{}}},\n  \"figures\": {},\n  \"figuresBefore\": {},\n  \"lists\": {{{}}},\n  \"biomesAbsent\": [{}],\n  \"ores\": {}\n}}\n",
        quote(seed),
        json_number(Some(world.samples as f64 * world.step)),
        json_number(Some(world.sea_level)),
        tect.join(","),
        pack.join(","),
        figures.to_json(),
        same_span.map_or("null".to_string(), Figures::to_json),
        lists.join(","),
        absent.join(","),
        ores
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped() {
        assert_eq!(quote("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
    }

    #[test]
    fn a_counted_list_keeps_its_counted_rows_biggest_first() {
        let list = List {
            ids: vec!["a".into(), "b".into(), "c".into()],
            names: vec!["A".into(), "B".into(), "C".into()],
            cells: vec![2.0, 0.0, 5.0],
            widths: vec![10.4, 0.0, 20.5],
            counted: true,
            ..List::default()
        };
        assert_eq!(rows(&list, 4, false), "[{\"id\":\"c\",\"name\":\"C\",\"count\":5,\"spacing\":21},{\"id\":\"a\",\"name\":\"A\",\"count\":2,\"spacing\":10}]");
    }

    #[test]
    fn a_measured_list_gives_shares_of_the_whole_map() {
        let list = List { ids: vec!["a".into(), "b".into()], names: vec!["A".into(), "B".into()], cells: vec![4.0, 12.0], widths: vec![7.0, 9.0], ..List::default() };
        // 4 x 4 samples: 16 cells; the biomes' list is sorted, the others keep their order.
        assert_eq!(rows(&list, 4, true), "[{\"id\":\"b\",\"name\":\"B\",\"share\":75,\"width\":9},{\"id\":\"a\",\"name\":\"A\",\"share\":25,\"width\":7}]");
        assert_eq!(rows(&list, 4, false), "[{\"id\":\"a\",\"name\":\"A\",\"share\":25,\"width\":7},{\"id\":\"b\",\"name\":\"B\",\"share\":75,\"width\":9}]");
    }
}
