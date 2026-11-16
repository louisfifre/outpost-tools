//! The settings' field column is as wide as the widest value any of the 43 settings can show, so that no number is
//! cut ("Noodle caves size" is -0.075, six cells, which the old 8 ch column of 57.6 px did not hold).

use mere_ui::{TextLook, Tone, tokens};
use preview::panels::setting_field;
use preview::settings::{Kind, SETTINGS, widest_number_text};

/// What a field leaves for its text: the column less the border and the padding on both sides.
fn inner() -> f32 {
    setting_field() - 2.0 * (tokens::FRAME + tokens::INSET)
}

#[test]
fn the_widest_value_is_the_noodle_size_of_the_pack() {
    assert_eq!(widest_number_text(), "-0.075");
}

#[test]
fn every_value_a_setting_starts_with_fits_its_field() {
    let body = TextLook::body(Tone::Text);
    for s in SETTINGS.iter().filter(|s| matches!(s.kind, Kind::Number(_))) {
        let shown = format!("{}", s.pack);
        assert!(body.width(&shown) <= inner() + 1e-3, "{}: {shown} is {} px in {} px", s.key, body.width(&shown), inner());
    }
}

#[test]
fn the_column_holds_the_widest_value_and_nothing_more() {
    let body = TextLook::body(Tone::Text);
    let widest = body.width(&widest_number_text());
    assert!(widest <= inner() + 1e-3, "{widest} in {}", inner());
    // Not wider than that: the 1 px border and the 8 px padding on each side are all it adds.
    assert!((setting_field() - (widest + 18.0)).abs() < 1e-3, "{}", setting_field());
}

#[test]
fn the_ends_of_every_range_fit_too() {
    let body = TextLook::body(Tone::Text);
    for s in SETTINGS.iter() {
        if let Kind::Number(spec) = s.kind {
            for v in [spec.min, spec.max] {
                let shown = format!("{v}");
                assert!(body.width(&shown) <= inner() + 1e-3, "{}: {shown}", s.key);
            }
        }
    }
}
