//! The settings panel's bottom row: Generate and Apply side by side, each a split action whose word acts and whose
//! chevron opens above the row what it needs (the details; the places), closed by a pick or Escape. And
//! the settings' own head: the section folds, "Back to" is the first line of its body with its two choices at the
//! far right, and the status no longer says that the map shows the settings. Built as the application builds it,
//! with events fed to the `Ui` between frames.

use mere_base::{Button, Event, Key, Mods, Rect, Vec2};
use mere_ui::{Id, Theme, Ui};
use preview::run::Ask;
use preview::settings::pack_values;
use preview::shell::Want;
use preview::{PreviewState, ShellIds, build, text};

const SIZE: Vec2 = Vec2::new(1400.0, 940.0);

struct Rig {
    ui: Ui,
    s: PreviewState,
    ids: ShellIds,
}

impl Rig {
    fn new(s: PreviewState) -> Rig {
        let mut r = Rig { ui: Ui::new(Theme::dark()), s, ids: ShellIds::default() };
        r.frame();
        r.frame();
        r
    }

    fn frame(&mut self) {
        for _ in 0..4 {
            let before = self.s.clone();
            self.ui.begin_frame(SIZE);
            self.ids = build(&mut self.ui, &mut self.s, SIZE, None, None, None);
            self.ui.end_frame();
            if !self.ui.want_redraw() && self.s == before {
                break;
            }
        }
    }

    fn rect(&self, id: Id) -> Rect {
        assert_ne!(id, Id::ROOT, "the part is not built");
        self.ui.rect(id).expect("the part was laid out")
    }

    /// A press on the widget's left part: a field of the settings list, scrolled out of sight under the bottom row,
    /// takes a press that lands on it (a fault of the kit's field, which judges it by the unclipped rect).
    fn click(&mut self, id: Id) {
        let r = self.rect(id);
        self.click_at(Vec2::new(r.x + 8.0, r.y + r.h / 2.0));
    }

    fn click_at(&mut self, pos: Vec2) {
        self.ui.event(&Event::PointerMoved { pos });
        self.frame();
        self.ui.event(&Event::PointerDown { pos, button: Button::Left, clicks: 1 });
        self.frame();
        self.ui.event(&Event::PointerUp { pos, button: Button::Left });
        self.frame();
    }

    /// Escape, as the application hears it: the shell's own check first, then the kit's.
    fn escape(&mut self) {
        let e = Event::Key { key: Key::Escape, mods: Mods::default() };
        if !preview::shell::help_escape(&mut self.s, &self.ui, &e) {
            self.ui.event(&e);
        }
        self.frame();
    }
}

#[test]
fn b1_each_chevron_opens_its_choices_above_the_row_and_the_words_do_nothing_but_act() {
    let mut r = Rig::new(PreviewState::default());
    let row = r.rect(r.ids.generate);
    assert_eq!(r.rect(r.ids.apply).y, row.y, "side by side");
    assert_eq!((r.ids.details, r.ids.place_list), (Id::ROOT, Id::ROOT), "nothing open");

    r.click(r.ids.generate_more);
    assert!(r.s.details_open);
    let (d, g) = (r.rect(r.ids.details), r.rect(r.ids.generate));
    assert!(d.y + d.h <= g.y + 0.01, "the details are above the row");
    assert_eq!(r.s.want, None, "opening starts nothing");
    assert_eq!(r.ids.place_list, Id::ROOT, "none of Apply's");

    // A press on the other chevron is a press outside the open list: it closes it and reaches nothing else; the next
    // one opens the places.
    r.click(r.ids.place_line);
    assert!(!r.s.details_open && !r.s.places_open);
    r.click(r.ids.place_line);
    assert!(r.s.places_open);
    let l = r.rect(r.ids.place_list);
    assert!(l.y + l.h <= r.rect(r.ids.apply).y + 0.01, "the places are above the row");
    assert_eq!((r.ids.details, r.s.want), (Id::ROOT, None));
}

#[test]
fn b2_escape_closes_them() {
    let mut r = Rig::new(PreviewState::default());
    r.click(r.ids.generate_more);
    assert!(r.s.details_open);
    r.escape();
    assert!(!r.s.details_open);
    assert_eq!(r.ids.details, Id::ROOT);
    r.click(r.ids.place_line);
    assert!(r.s.places_open);
    r.escape();
    assert!(!r.s.places_open);
}

#[test]
fn b3_generate_takes_the_detail_chosen_and_the_word_starts_the_run() {
    let mut r = Rig::new(PreviewState::default());
    r.click(r.ids.generate_more);
    assert_eq!(r.s.detail, 2);
    // The first row of the list is 64 blocks: a press chooses it and closes the list, starting nothing.
    let d = r.rect(r.ids.details);
    r.click_at(Vec2::new(d.x + 40.0, d.y + 20.0));
    assert_eq!(r.s.detail, 0, "64 blocks");
    assert!(!r.s.details_open, "choosing closes the list");
    assert_eq!(r.s.want, None, "choosing starts nothing");
    r.click(r.ids.generate);
    assert_eq!(r.s.want, Some(Want::Generate));
}

#[test]
fn b4_the_word_of_generate_is_off_while_the_map_shows_the_form_and_the_status_says_nothing_of_it() {
    let s = PreviewState::default();
    let ask = Ask { seed: s.seed.clone(), samples: s.samples, detail: 16, values: pack_values() };
    let mut r = Rig::new(PreviewState { made: Some(ask), ..s });
    assert_eq!(r.ids.status_text, "", "no sentence for a map that is up to date");
    assert_eq!(r.ids.status, Id::ROOT);
    r.click(r.ids.generate);
    assert_eq!(r.s.want, None, "disabled: the map shows these settings");
    // Its chevron stays enabled: a detail moved away from the map's makes a new run to ask for.
    r.click(r.ids.generate_more);
    assert!(r.s.details_open);
    let d = r.rect(r.ids.details);
    r.click_at(Vec2::new(d.x + 40.0, d.y + 20.0));
    r.click(r.ids.generate);
    assert_eq!(r.s.want, Some(Want::Generate));
}

#[test]
fn b5_the_settings_fold_and_back_to_is_the_first_line_of_its_body_with_its_choices_at_the_far_right() {
    let mut r = Rig::new(PreviewState::default());
    let (back, filter) = (r.rect(r.ids.profiles), r.rect(r.ids.view_only));
    assert!(back.y < filter.y, "before the filter and the list");
    assert!(back.y > r.rect(r.ids.settings_head).y + r.rect(r.ids.settings_head).h - 0.01, "in the body, under the head");
    // "Back to" stays at the left as plain text; Pack and Original end the line.
    let (pack, original) = (r.rect(r.ids.back_pack), r.rect(r.ids.original));
    assert!(pack.x > back.x + 60.0 && pack.x + pack.w < original.x, "Pack then Original");
    assert!((original.x + original.w - (back.x + back.w)).abs() < 1.0, "at the line's far end");

    // The head folds the body, bottom row and all that stays.
    assert!(r.s.settings_open);
    r.click(r.ids.settings_head);
    assert!(!r.s.settings_open, "the head's press folds the section");
    assert_eq!(r.ids.profiles, Id::ROOT, "the body is gone, Back to with it");
    assert_ne!(r.ids.generate, Id::ROOT);
    assert_ne!(r.ids.apply, Id::ROOT);
    r.click(r.ids.settings_head);
    assert!(r.s.settings_open);
    assert_ne!(r.ids.profiles, Id::ROOT);
    let _ = text::BACK_TO;
}

/// Pictures of the panel: at rest, with each list open and with the settings folded. The lists open are in tests/apply.rs
/// (`a3_pictures_of_the_block`) and below.
#[test]
fn b6_pictures() {
    use mere_base::Gfx;
    use mere_win32::Renderer;
    let (w, h) = (1400u32, 940u32);
    let shots = [
        ("bottom_closed", PreviewState::default()),
                ("bottom_generate", PreviewState { details_open: true, ..PreviewState::default() }),
        ("bottom_places", PreviewState { places_open: true, ..PreviewState::default() }),
        ("bottom_folded", PreviewState { settings_open: false, ..PreviewState::default() }),
    ];
    for (name, state) in shots {
        let theme = Theme::dark();
        let (list, _, _) = preview::picture_with(theme, Vec2::new(w as f32, h as f32), None, state);
        let mut r = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
        r.begin(theme.ground);
        r.draw_list(&list);
        r.end();
        let rgba = r.read_rgba().unwrap();
        let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.png"));
        std::fs::write(&path, mere_png::encode(w, h, &rgba)).unwrap();
        println!("wrote {}", path.display());
    }
}
