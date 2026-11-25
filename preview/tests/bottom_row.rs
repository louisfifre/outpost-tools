//! The settings panel's bottom row: Generate and Apply side by side, each opening above the row only what it needs
//! (the details and a confirming Generate; the places and a confirming Apply), closed by a second press or Escape. And
//! the settings' own head: the section folds, "Back to" is the first line of its body with its two choices at the
//! far right, and the status no longer says that the map shows the settings. Built as the application builds it,
//! with events fed to the `Ui` between frames.

use mere_base::{Button, Event, Key, Mods, Rect, Vec2};
use mere_ui::{Id, Theme, Ui};
use preview::run::Ask;
use preview::settings::pack_values;
use preview::shell::Want;
use preview::{Action, PreviewState, ShellIds, build, text};

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
fn b1_a_press_opens_what_the_action_needs_above_the_row_and_a_second_one_closes_it() {
    let mut r = Rig::new(PreviewState::default());
    assert_eq!(r.s.action, None);
    let row = r.rect(r.ids.generate);
    assert_eq!(r.rect(r.ids.apply_open).y, row.y, "side by side");
    assert_eq!((r.ids.details, r.ids.generate_go, r.ids.apply, r.ids.place_line), (Id::ROOT, Id::ROOT, Id::ROOT, Id::ROOT), "nothing open");

    r.click(r.ids.generate);
    assert_eq!(r.s.action, Some(Action::Generate));
    assert!(r.rect(r.ids.details).y + r.rect(r.ids.details).h <= r.rect(r.ids.generate_go).y, "the details, then the button");
    assert!(r.rect(r.ids.generate_go).y + r.rect(r.ids.generate_go).h <= r.rect(r.ids.generate).y, "above the row");
    assert_eq!((r.ids.apply, r.ids.place_line), (Id::ROOT, Id::ROOT), "none of Apply's");
    assert_eq!(r.s.want, None, "opening starts nothing");

    r.click(r.ids.generate);
    assert_eq!(r.s.action, None, "pressed again, it closes");
    assert_eq!((r.ids.details, r.ids.generate_go), (Id::ROOT, Id::ROOT));

    r.click(r.ids.apply_open);
    assert_eq!(r.s.action, Some(Action::Apply));
    assert!(r.rect(r.ids.place_line).y + r.rect(r.ids.place_line).h <= r.rect(r.ids.apply).y, "the place line, then Apply");
    assert!(r.rect(r.ids.apply).y + r.rect(r.ids.apply).h <= r.rect(r.ids.apply_open).y, "above the row");
    assert_eq!((r.ids.details, r.ids.generate_go), (Id::ROOT, Id::ROOT), "none of Generate's");
    assert_eq!(r.s.want, None);

    // The other action's press swaps what is open.
    r.click(r.ids.generate);
    assert_eq!(r.s.action, Some(Action::Generate));
    assert_eq!(r.ids.apply, Id::ROOT);
}

#[test]
fn b2_escape_closes_them() {
    let mut r = Rig::new(PreviewState::default());
    r.click(r.ids.generate);
    assert_eq!(r.s.action, Some(Action::Generate));
    r.escape();
    assert_eq!(r.s.action, None);
    assert_eq!(r.ids.details, Id::ROOT);
    r.click(r.ids.apply_open);
    r.escape();
    assert_eq!(r.s.action, None);
}

#[test]
fn b3_generate_takes_the_detail_chosen_and_the_confirming_button_starts_the_run() {
    let mut r = Rig::new(PreviewState::default());
    r.click(r.ids.generate);
    assert_eq!(r.s.detail, 2);
    // Detail tabs are the `d0`.. keys of the details node: press the first (64 blocks), then confirm.
    let d = r.rect(r.ids.details);
    r.click_at(Vec2::new(d.x + 20.0, d.y + d.h / 2.0));
    assert_eq!(r.s.detail, 0, "64 blocks");
    assert_eq!(r.s.action, Some(Action::Generate), "choosing a detail leaves it open");
    r.click(r.ids.generate_go);
    assert_eq!(r.s.want, Some(Want::Generate));
    assert_eq!(r.s.action, None, "it closes once it asked");
}

#[test]
fn b4_the_confirming_generate_is_off_while_the_map_shows_the_form_and_the_status_says_nothing_of_it() {
    let s = PreviewState::default();
    let ask = Ask { seed: s.seed.clone(), samples: s.samples, detail: 16, values: pack_values() };
    let mut r = Rig::new(PreviewState { made: Some(ask), action: Some(Action::Generate), ..s });
    assert_eq!(r.ids.status_text, "", "no sentence for a map that is up to date");
    assert_eq!(r.ids.status, Id::ROOT);
    r.click(r.ids.generate_go);
    assert_eq!(r.s.want, None, "disabled: the map shows these settings");
    assert_ne!(r.ids.generate, Id::ROOT, "the row's Generate stays enabled: it opens the choices");
    // A detail moved away from the map's makes a new run to ask for.
    let d = r.rect(r.ids.details);
    r.click_at(Vec2::new(d.x + 20.0, d.y + d.h / 2.0));
    r.click(r.ids.generate_go);
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
    assert_ne!(r.ids.apply_open, Id::ROOT);
    r.click(r.ids.settings_head);
    assert!(r.s.settings_open);
    assert_ne!(r.ids.profiles, Id::ROOT);
    let _ = text::BACK_TO;
}

/// Pictures of the panel: at rest, with Generate open, with the settings folded. Apply's open is in tests/apply.rs
/// (`a3_pictures_of_the_block`), where its places are.
#[test]
fn b6_pictures() {
    use mere_base::Gfx;
    use mere_win32::Renderer;
    let (w, h) = (1400u32, 940u32);
    let shots = [
        ("bottom_closed", PreviewState::default()),
        ("bottom_generate", PreviewState { action: Some(Action::Generate), ..PreviewState::default() }),
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
