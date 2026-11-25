//! The shell drawn offscreen in both themes (the widths are swept by tests/responsive.rs), and checked pixel by pixel. The
//! pictures are written to the target's tmp folder and copied to docs/shots by hand: they are a milestone.

use mere_base::{Color, Gfx, Rect, Vec2};
use mere_ui::{GUTTER, Theme};
use mere_win32::Renderer;
use preview::{ShellIds, picture};

const H: u32 = 940;

fn draw(theme: Theme, w: u32, name: &str) -> (Vec<u8>, mere_ui::Ui, ShellIds) {
    let (list, ui, ids) = picture(theme, Vec2::new(w as f32, H as f32));
    let mut r = Renderer::offscreen(w, H, 1.0).expect("offscreen renderer");
    r.begin(theme.ground);
    r.draw_list(&list);
    r.end();
    let rgba = r.read_rgba().unwrap();
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, mere_png::encode(w, H, &rgba)).unwrap();
    println!("wrote {}", path.display());
    (rgba, ui, ids)
}

fn px(rgba: &[u8], w: u32, x: f32, y: f32) -> [u8; 3] {
    let i = (y as usize * w as usize + x as usize) * 4;
    [rgba[i], rgba[i + 1], rgba[i + 2]]
}

fn same(got: [u8; 3], want: Color, what: &str) {
    let w = [want.r, want.g, want.b].map(|v| (v * 255.0).round() as i32);
    assert!(got.iter().zip(w).all(|(a, b)| (*a as i32 - b).abs() <= 1), "{what}: got {got:?}, want {w:?}");
}

fn check(theme: Theme, w: u32, name: &str) {
    let (rgba, ui, ids) = draw(theme, w, name);
    let at = |x: f32, y: f32| px(&rgba, w, x, y);
    let rect = |id| -> Rect { ui.rect(id).expect("built") };

    // P1: the ground, and the gap between the left and middle columns.
    same(at(2.0, 2.0), theme.ground, "ground");
    let (left, middle) = (rect(ids.left), rect(ids.middle));
    same(at((left.x + left.w + middle.x) / 2.0, 500.0), theme.ground, "column gap");

    // P2: no world, so the frame is empty ground; its note is centred, far from the corner.
    let f = rect(ids.frame);
    same(at(f.x + 5.0, f.y + 5.0), theme.ground, "empty frame");

    // P3: the pressed Relief tab is filled with --fill (inside its 1 px frame).
    let relief = rect(ids.tabs[0]);
    same(at(relief.x + 3.0, relief.y + 3.0), theme.fill, "pressed tab");

    // P5: with no map the page decides Generate (nothing shows these settings): it is enabled, filled with --text
    // to its frame, and the status line has nothing to say, so it is not built.
    let g = rect(ids.generate);
    same(at(g.x + g.w / 2.0, g.y), theme.fill, "generate top border");
    same(at(g.x + 3.0, g.y + 3.0), theme.fill, "generate inside");
    assert!(ids.status_text.is_empty() && ids.status == mere_ui::Id::ROOT, "an empty status is not built");

    // P6: the select's arrow: its third row of ink (the first full one), 13 px from the right edge plus 4. The five
    // rows are centred in the content box, on the whole pixel above: floor(pad + (content - 5) / 2).
    let sel = rect(ids.select);
    let pad = mere_ui::tokens::FRAME + mere_ui::tokens::INSET_HAIR;
    let top = (pad + (sel.h - 2.0 * pad - 5.0) / 2.0).floor();
    same(at(sel.x + sel.w - 13.0 + 4.0, sel.y + top + 2.0), theme.text, "select chevron");

    // P7: the settings thumb, 3 px wide in the middle of the scroll box's gutter, at the top of the track (10 px
    // under the box's top, the room of the arrow), muted. The box is 10 px wider than the column (the page's
    // margin-right: -10px), so the bar sits just outside the column's edge.
    let sc = rect(ids.settings_scroll);
    same(at(sc.x + sc.w - GUTTER / 2.0, sc.y + 15.0), theme.muted, "settings thumb");

    assert!(rgba.chunks(4).any(|p| p[..3] != rgba[..3]), "the picture is one colour");
}

/// P4: the first row of the relief list is the lowest band the area has, and its swatch is that band's colour.
#[test]
fn p4_the_first_row_swatch_is_its_bands_colour() {
    let dir = &preview::world::data_dir();
    if !dir.join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no out folder");
        return;
    }
    let world = preview::world::World::load(dir).expect("the data loads");
    let theme = Theme::dark();
    let (list, ui, ids) = preview::picture_with(theme, Vec2::new(1400.0, H as f32), Some(&world), preview::PreviewState::for_world(Some(&world)));
    let mut r = Renderer::offscreen(1400, H, 1.0).expect("offscreen renderer");
    r.begin(theme.ground);
    r.draw_list(&list);
    r.end();
    let rgba = r.read_rgba().unwrap();
    let bands = world.measured.lists[5].as_ref().expect("the height list");
    let first = (0..bands.cells.len()).find(|&i| bands.cells[i] > 0.0).unwrap();
    let c = bands.colours[first];
    let row = ui.rect(ids.list_rows[0]).expect("built");
    // The row's top + 3 padding + 6 to the swatch's middle, 6 px in.
    same(px(&rgba, 1400, row.x + 6.0, row.y + 3.0 + 3.0 + 6.0), Color::rgb8(c[0], c[1], c[2]), "first band swatch");
}

#[test]
fn preview_1400_dark() {
    check(Theme::dark(), 1400, "preview_1400.png");
}

#[test]
fn preview_1400_light() {
    check(Theme::light(), 1400, "preview_1400_light.png");
}
