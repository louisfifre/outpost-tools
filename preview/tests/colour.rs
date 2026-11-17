//! The pieces of the map that need no data: the regex-free matcher, the number and name formats, the look.

use mere_base::Vec2;
use preview::colour::{Alt, biome_colour, hash_of, hsl_to_rgb, structure_colour, underground};
use preview::map::Look;
use preview::paint::View;
use preview::text::{num, pretty};

#[test]
fn c1_in_order_parts_match_like_a_dot_star() {
    let deep_ocean = Alt(&["deep", "ocean"]);
    assert!(deep_ocean.matches("deep_lukewarm_ocean"));
    assert!(!deep_ocean.matches("ocean_deep"));
    assert!(!deep_ocean.matches("deep"));
    // The word after the first one may be found later in the text, and the first may occur twice.
    assert!(Alt(&["a", "b"]).matches("xaxaxb"));
    assert!(!Alt(&["aa", "b"]).matches("xaxbxa"));
}

#[test]
fn c2_the_word_boundary_of_mines() {
    let mines = Alt(&["mines\\b"]);
    assert!(mines.matches("abandoned_mines"));
    assert!(mines.matches("mines x"));
    assert!(!mines.matches("mines_x"));
    // A later occurrence may be the one that ends a word, as a regular expression would find.
    assert!(mines.matches("mines_x mines"));
}

#[test]
fn c3_numbers_and_names() {
    assert_eq!(num(-2048.0), "-2,048");
    assert_eq!(num(4096.0), "4,096");
    assert_eq!(num(512.0), "512");
    assert_eq!(num(0.0), "0");
    assert_eq!(num(1234567.0), "1,234,567");
    assert_eq!(num(2.5), "3");
    assert_eq!(num(-2.5), "-3");
    assert_eq!(pretty("terralith:cave/fungal_caves"), "Cave fungal caves");
    assert_eq!(pretty("minecraft:deep_lukewarm_ocean"), "Deep lukewarm ocean");
    assert_eq!(pretty("plains"), "Plains");
}

#[test]
fn c4_look_wheel_and_drag() {
    let l = Look::REST.wheel(Vec2::new(0.5, 0.5), 1.0);
    assert_eq!(l.k, 1.25);
    assert!((l.x + 0.125).abs() < 1e-6 && (l.y + 0.125).abs() < 1e-6, "{l:?}");
    assert_eq!(Look::REST.wheel(Vec2::new(0.5, 0.5), -1.0), Look::REST);
    let mut l = Look::REST;
    for _ in 0..20 {
        l = l.wheel(Vec2::new(0.9, 0.1), 1.0);
        assert!(l.k <= 16.0 && l.x >= 1.0 - l.k && l.x <= 0.0 && l.y >= 1.0 - l.k && l.y <= 0.0, "{l:?}");
    }
    assert_eq!(l.k, 16.0);
    let d = Look::dragged(Look { k: 2.0, x: -0.5, y: -0.5 }, Vec2::new(-63.8, 0.0), 638.0);
    assert!((d.x + 0.6).abs() < 1e-5, "{d:?}");
    assert_eq!(Look::dragged(Look::REST, Vec2::new(40.0, 40.0), 638.0), Look::REST);
}

#[test]
fn c5_views_and_keys() {
    for i in 0..5 {
        let v = View::from_index(i).unwrap();
        assert_eq!(View::from_key(v.key()), Some(v));
        assert_eq!(v.index(), i);
    }
    assert_eq!(View::from_index(5), None);
    assert_eq!(View::from_key("height"), Some(View::Height));
    assert_eq!(View::from_key("section"), None);
}

#[test]
fn c6_colour_primitives() {
    assert_eq!(hash_of(""), 2166136261.0 / 4294967296.0);
    assert!(hash_of("minecraft:plains") != hash_of("minecraft:plains!"));
    assert_eq!(hsl_to_rgb(0.0, 100.0, 50.0), [255, 0, 0]);
    assert!(underground("terralith:underground/x", None) && underground("a:b", Some("underground_structures")));
    assert!(underground("minecraft:stronghold", None) && !underground("minecraft:village_plains", Some("surface_structures")));
    // Villages are green, mines are the underground family, whatever the set says of the rest.
    let village = structure_colour("minecraft:village_plains", Some("minecraft:villages"));
    assert!(village[1] > village[0] && village[1] > village[2], "{village:?}");
    // An id with no colon has no name to read: the page tests "undefined", which no family matches.
    let _ = biome_colour("nocolon");
}
