//! Where the input method may write in the preview: on the text fields (the seed, the filter), never on a
//! setting's number field. Built with the shell's own harness, no GPU.

use mere_base::{Button, Event, Rect, Vec2};
use mere_ui::{Id, Theme, Ui};
use preview::{PreviewState, ShellIds, build, picture};

const SIZE: Vec2 = Vec2::new(1400.0, 940.0);

/// A press at `p`, then one frame so the field takes the focus and paints its spot.
fn press(ui: &mut Ui, s: &mut PreviewState, p: Vec2) -> ShellIds {
    ui.event(&Event::PointerMoved { pos: p });
    ui.event(&Event::PointerDown { pos: p, button: Button::Left, clicks: 1 });
    ui.event(&Event::PointerUp { pos: p, button: Button::Left });
    let mut ids = None;
    for _ in 0..2 {
        ui.begin_frame(SIZE);
        ids = Some(build(ui, s, SIZE, None, None, None));
        ui.end_frame();
    }
    ids.unwrap()
}

fn rect(ui: &Ui, id: Id) -> Rect {
    ui.rect(id).expect("the node was built")
}

fn mid(r: Rect) -> Vec2 {
    Vec2::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

#[test]
fn a_text_field_gets_the_input_method_and_a_number_field_does_not() {
    let (_, mut ui, ids) = picture(Theme::dark(), SIZE);
    let mut s = PreviewState::default();
    // Nothing has the keyboard yet.
    assert_eq!(ui.ime_spot(), None);

    // The seed: the spot is inside its rect.
    let seed = rect(&ui, ids.seed);
    let ids = press(&mut ui, &mut s, mid(seed));
    let spot = ui.ime_spot().expect("the seed is a text field");
    let seed = rect(&ui, ids.seed);
    assert!(seed.contains(Vec2::new(spot.x, spot.y)), "{spot:?} not inside {seed:?}");

    // The first setting's number field (the last 57.6 of its row): off.
    let row = rect(&ui, ids.rows[0]);
    let ids = press(&mut ui, &mut s, Vec2::new(row.x + row.w - 20.0, row.y + row.h / 2.0));
    assert_eq!(ui.focused().is_some(), true, "the number field has the keyboard");
    assert_eq!(ui.ime_spot(), None);

    // The filter: on again.
    let filter = rect(&ui, ids.filter);
    let ids = press(&mut ui, &mut s, mid(filter));
    let spot = ui.ime_spot().expect("the filter is a text field");
    let filter = rect(&ui, ids.filter);
    assert!(filter.contains(Vec2::new(spot.x, spot.y)), "{spot:?} not inside {filter:?}");
}
