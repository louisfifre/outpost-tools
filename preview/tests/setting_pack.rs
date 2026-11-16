//! What the settings say about the pack: a field that differs from the pack's value has that value beside it, a
//! group that differs has an arrow in its title that puts it back, and beside Apply two choices put everything back,
//! to the pack's values or to Tectonic's own, each pressed while the form is there. Built as the application builds
//! it, with events fed to the `Ui` between frames.

use mere_base::{Button, Cmd, DrawList, Event, Gfx, Rect, Vec2};
use mere_ui::{Id, Theme, Ui};
use mere_win32::Renderer;
use preview::settings::{SETTINGS, original_values, pack_values};
use preview::{PreviewState, ShellIds, build, text};

const SIZE: Vec2 = Vec2::new(1400.0, 940.0);

fn index(key: &str) -> usize {
    SETTINGS.iter().position(|s| s.key == key).expect("a setting of that key")
}

struct Rig {
    ui: Ui,
    s: PreviewState,
    ids: ShellIds,
    list: DrawList,
}

impl Rig {
    fn new(s: PreviewState) -> Rig {
        let mut r = Rig { ui: Ui::new(Theme::dark()), s, ids: ShellIds::default(), list: DrawList::default() };
        r.frame();
        r.frame();
        r
    }

    fn frame(&mut self) {
        for _ in 0..4 {
            let before = self.s.clone();
            self.ui.begin_frame(SIZE);
            self.ids = build(&mut self.ui, &mut self.s, SIZE, None, None, None);
            self.list = self.ui.end_frame().clone();
            if !self.ui.want_redraw() && self.s == before {
                break;
            }
        }
    }

    fn rect(&self, id: Id) -> Rect {
        assert_ne!(id, Id::ROOT, "the part is not built");
        self.ui.rect(id).expect("the part was laid out")
    }

    fn click(&mut self, id: Id) {
        let r = self.rect(id);
        let pos = Vec2::new(r.x + r.w / 2.0, r.y + r.h / 2.0);
        self.ui.event(&Event::PointerMoved { pos });
        self.frame();
        self.ui.event(&Event::PointerDown { pos, button: Button::Left, clicks: 1 });
        self.frame();
        self.ui.event(&Event::PointerUp { pos, button: Button::Left });
        self.frame();
    }

    /// The texts drawn inside `r`, in drawing order.
    fn texts_in(&self, r: Rect) -> Vec<String> {
        self.list.cmds.iter().filter_map(|c| match c {
            Cmd::Text { at, text, .. } if r.contains(*at) => Some(text.clone()),
            _ => None,
        }).collect()
    }
}

fn with(changes: &[(&str, f64)]) -> PreviewState {
    let mut s = PreviewState::default();
    for (key, v) in changes {
        s.values[index(key)] = *v;
    }
    s
}

#[test]
fn p1_a_field_that_differs_from_the_pack_has_the_pack_s_value_beside_it() {
    let r = Rig::new(PreviewState::default());
    assert!(r.ids.pack_marks.iter().all(|id| *id == Id::ROOT), "at the pack's values nothing is marked");

    let (number, switch) = (index("continents_scale"), index("rolling_hills"));
    let r = Rig::new(with(&[("continents_scale", 0.1), ("rolling_hills", 0.0)]));
    for (i, id) in r.ids.pack_marks.iter().enumerate() {
        assert_eq!(*id != Id::ROOT, i == number || i == switch, "{}", SETTINGS[i].key);
    }
    // The pack's value as its field would write it, between the label and the field, on the row.
    let (mark, row) = (r.rect(r.ids.pack_marks[number]), r.rect(r.ids.rows[number]));
    // (Other texts may lie there too, cut away by a scroll area: the list of commands is not what is seen.)
    assert!(r.texts_in(mark).contains(&SETTINGS[number].pack.to_string()));
    assert!(mark.x >= row.x && mark.x + mark.w <= row.x + row.w && mark.y >= row.y && mark.y + mark.h <= row.y + row.h);
    assert!(r.texts_in(r.rect(r.ids.pack_marks[switch])).contains(&text::ON.to_string()), "a switch says on or off");
}

#[test]
fn p2_a_group_that_differs_has_an_arrow_that_puts_it_back_alone() {
    let r = Rig::new(PreviewState::default());
    assert_eq!(r.ids.group_resets, [Id::ROOT; 5], "no arrow while every group is the pack's");

    // One setting of the first group and one of the third.
    let mut r = Rig::new(with(&[("temperature_scale", 0.2), ("ocean_depth", -0.3)]));
    let (first, third) = (SETTINGS[index("temperature_scale")].group, SETTINGS[index("ocean_depth")].group);
    for g in 0..5 {
        assert_eq!(r.ids.group_resets[g] != Id::ROOT, g == first || g == third, "group {g}");
    }
    // The arrow sits on its group's title line, at the right.
    let (arrow, head) = (r.rect(r.ids.group_resets[first]), r.rect(r.ids.group_heads[0]));
    assert!(arrow.y >= head.y && arrow.y + arrow.h <= head.y + head.h + 0.01, "on the title's line");
    assert!(arrow.x + arrow.w > head.x + head.w - 1.0, "at the right end");

    // And it adds no height: the rows are where they are with no arrow.
    let plain = Rig::new(PreviewState::default());
    assert_eq!(r.rect(r.ids.rows[index("vertical_scale")]).y, plain.rect(plain.ids.rows[index("vertical_scale")]).y, "the arrow moves no row");

    r.click(r.ids.group_resets[first]);
    assert_eq!(r.s.values[index("temperature_scale")], pack_values()[index("temperature_scale")], "the group is back");
    assert_eq!(r.s.values[index("ocean_depth")], -0.3, "the other group kept its value");
    assert_eq!(r.ids.group_resets[first], Id::ROOT, "its arrow is gone");
    assert_ne!(r.ids.group_resets[third], Id::ROOT);
}

#[test]
fn p3_pack_and_original_put_everything_back_and_say_where_the_form_is() {
    // The pack differs from Tectonic's own values, so only one of the two can be pressed.
    assert_ne!(pack_values(), original_values());
    let mut r = Rig::new(with(&[("vertical_scale", 2.0)]));
    assert_ne!(r.ids.original, Id::ROOT);
    r.click(r.ids.original);
    assert_eq!(r.s.values, original_values(), "Original: Tectonic's own values");
    assert_eq!(r.s.baseline, pack_values(), "the pack's values did not move");
    assert_ne!(r.ids.pack_marks[index("vertical_scale")], Id::ROOT, "the fields now differ from the pack");

    // The two are named on one line, the pack's first.
    let said = r.texts_in(r.rect(r.ids.profiles));
    let places: Vec<Option<usize>> = [text::BACK_TO, text::PACK, text::ORIGINAL].iter().map(|w| said.iter().position(|s| s == w)).collect();
    assert!(places.iter().all(Option::is_some) && places.windows(2).all(|p| p[0] < p[1]), "{said:?}");
    r.click(r.ids.back_pack);
    assert_eq!(r.s.values, pack_values(), "Pack: the pack's values");
    assert!(r.ids.pack_marks.iter().all(|id| *id == Id::ROOT));
}

#[test]
fn p4_pictures() {
    let s = with(&[("continents_scale", 0.1), ("max_y", 448.0), ("snow_start_offset", 192.0), ("rolling_hills", 0.0)]);
    let r = Rig::new(s);
    let (w, h) = (SIZE.x as u32, SIZE.y as u32);
    let theme = Theme::dark();
    let mut out = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
    out.begin(theme.ground);
    out.draw_list(&r.list);
    out.end();
    let rgba = out.read_rgba().unwrap();
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("setting_pack_dark.png");
    std::fs::write(&path, mere_png::encode(w, h, &rgba)).unwrap();
    println!("wrote {}", path.display());
}
