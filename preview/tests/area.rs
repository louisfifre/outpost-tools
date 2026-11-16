//! The scanned area's select (preview.html lines 378-382, 806-831): a click opens the page's sixteen widths, a row
//! sets the area, and past 32,768 blocks the sixteen-block detail gives way to the next coarser one, with the hint
//! under the row. Built as `PreviewApp::frame` builds the shell, without a world and without a GPU.

use mere_base::{Button, Cmd, Event, Vec2};
use mere_ui::{MENU_ROW, Theme, Ui};
use preview::run::DETAIL_BLOCKS;
use preview::{PreviewState, ShellIds, build};

const SIZE: Vec2 = Vec2::new(1400.0, 940.0);

struct Rig {
    ui: Ui,
    s: PreviewState,
    ids: ShellIds,
    texts: Vec<String>,
}

impl Rig {
    fn new(s: PreviewState) -> Rig {
        let mut r = Rig { ui: Ui::new(Theme::dark()), s, ids: ShellIds::default(), texts: Vec::new() };
        // The first frame learns where everything is: the pointer is judged against the last one.
        r.frame();
        r.frame();
        r
    }

    fn frame(&mut self) {
        self.ui.begin_frame(SIZE);
        self.ids = build(&mut self.ui, &mut self.s, SIZE, None, None, None);
        let list = self.ui.end_frame();
        self.texts = list.cmds.iter().filter_map(|c| if let Cmd::Text { text, .. } = c { Some(text.clone()) } else { None }).collect();
    }

    fn click(&mut self, pos: Vec2) {
        self.ui.event(&Event::PointerMoved { pos });
        self.ui.event(&Event::PointerDown { pos, button: Button::Left, clicks: 1 });
        self.frame();
        self.ui.event(&Event::PointerUp { pos, button: Button::Left });
        self.frame();
    }

    fn says(&self, s: &str) -> bool {
        self.texts.iter().any(|t| t == s)
    }
}

#[test]
fn a1_the_area_opens_its_sixteen_widths_and_a_row_sets_it() {
    let mut r = Rig::new(PreviewState { detail: 2, ..PreviewState::default() });
    assert_eq!(DETAIL_BLOCKS[2], 16, "the test starts on the finest detail");
    assert_eq!(r.s.samples, 256);
    assert!(r.says("4,096 blocks"), "the box shows the area as the page opens");
    assert!(!r.says("65,536 blocks"), "the menu is closed");
    let b = r.ui.rect(r.ids.select).expect("the select is built");
    assert!((b.h - mere_ui::tokens::CONTROL_COMPACT).abs() < 0.01, "the preview's select is a compact one: {b:?}");

    r.click(Vec2::new(b.x + b.w / 2.0, b.y + b.h / 2.0));
    for blocks in ["4,096 blocks", "32,768 blocks", "36,864 blocks", "65,536 blocks"] {
        assert!(r.says(blocks), "the open menu lists {blocks}");
    }

    // The ninth row: 2304 points, 36,864 blocks, past the widest fine area.
    let row = b.y + b.h + 1.0 + 8.0 * MENU_ROW + MENU_ROW / 2.0;
    r.click(Vec2::new(b.x + b.w / 2.0, row));
    assert_eq!(r.s.samples, 2304);
    assert!(!r.says("65,536 blocks"), "the menu closes on its choice");
    assert!(r.says("36,864 blocks"), "the box shows the choice");
    assert_eq!(DETAIL_BLOCKS[r.s.detail], 32, "the sixteen-block detail gives way to 32");
    assert_ne!(r.ids.area_hint, mere_ui::Id::ROOT, "the hint is under the row");
    // The paragraph is drawn one line at a time.
    assert!(r.texts.join(" ").contains("Past 32,768 blocks"), "the hint says why");
}

#[test]
fn a2_a_fine_area_keeps_the_detail_and_shows_no_hint() {
    let mut r = Rig::new(PreviewState { detail: 2, ..PreviewState::default() });
    let b = r.ui.rect(r.ids.select).unwrap();
    r.click(Vec2::new(b.x + b.w / 2.0, b.y + b.h / 2.0));
    // The eighth row: 2048 points, 32,768 blocks, the widest fine area.
    r.click(Vec2::new(b.x + b.w / 2.0, b.y + b.h + 1.0 + 7.0 * MENU_ROW + MENU_ROW / 2.0));
    assert_eq!(r.s.samples, 2048);
    assert_eq!(DETAIL_BLOCKS[r.s.detail], 16);
    assert_eq!(r.ids.area_hint, mere_ui::Id::ROOT, "no hint");
}
