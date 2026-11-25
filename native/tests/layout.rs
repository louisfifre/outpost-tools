//! The layout of Home against the page's numbers (every one names its index.html rule): the two parts fill the
//! window continuously, with no switch, and nothing is cut or overlaps.

mod common;

use std::sync::Arc;

use common::*;
use launcher_native::fake::fixture;
use mere_base::{App, Rect, Vec2};
use mere_ui::Theme;
use preview::{BAR_H, Mode, PreviewState, picture_with_home};

/// Home alone in its pane (no raise to the window's least), `w` x `h` as the window.
fn home_at(w: f32, h: f32) -> (mere_ui::Ui, launcher_native::HomeIds, mere_base::DrawList) {
    let mut home = loaded_home(Arc::new(fixture()));
    let state = PreviewState { mode: Mode::Home, reduce_motion: true, ..PreviewState::default() };
    let (list, ui, _) = picture_with_home(Theme::dark(), Vec2::new(w, h), None, state, &mut home);
    (ui, home.ids().clone(), list)
}

/// L1: side by side, every width from 1000 (the window's least) to 2560, one rule and no switch: `.columns` (two minmax(0, 1fr) and a gap of 48), the page's
/// padding of 28 (`.page`, side), the top of the preview's page (10) under the bar (40), and the page's cap of 1120
/// for the two together, centred beyond it.
#[test]
fn l1_side_by_side_up_to_the_cap() {
    for w in 1000..=2560 {
        let (ui, ids, _) = home_at(w as f32, 940.0);
        let inner = w as f32 - 56.0;
        let used = inner.min(launcher_native::home::COLUMNS_MAX);
        let left = 28.0 + (inner - used) / 2.0;
        let half = (used - launcher_native::home::COLUMNS_GAP) / 2.0;
        let (a, b) = (ui.rect(ids.profiles).unwrap(), ui.rect(ids.server).unwrap());
        assert!((a.x - left).abs() < 0.01 && (a.y - 50.0).abs() < 0.01 && (a.w - half).abs() < 0.01, "profiles at {w}: {a:?}");
        assert!((b.x - (left + half + launcher_native::home::COLUMNS_GAP)).abs() < 0.01 && (b.w - half).abs() < 0.01, "server at {w}: {b:?}");
    }
}

/// L4: at 1400 x 940, the page's rows in the kit's measures: a header as tall as an icon square and its hairline,
/// a row with `inset` all round pulled `inset` out on each side, its two lines (the name's 19.5 and the small
/// line's 16.5) taller than the squares, and the second square pulled a frame over the first. At 1400 the columns
/// are at their cap of 1120, centred: 112 px of room on each side beyond the padding of 28, two parts of 536.
#[test]
fn l4_the_numbers_at_1400() {
    use launcher_native::home::COLUMNS_GAP;
    use mere_ui::tokens::{FRAME, ICON_SQUARE, INSET};
    let (ui, ids, _) = home_at(1400.0, 940.0);
    let (left, part, top) = (140.0, (1120.0 - COLUMNS_GAP) / 2.0, 50.0);
    rect_is(ui.rect(ids.profiles_header).unwrap(), left, top, part, ICON_SQUARE + FRAME, "header");
    let r = |id| ui.rect(id).unwrap();
    // The hairline is the header's last pixel row.
    let below = top + ICON_SQUARE + FRAME;
    assert_eq!(r(ids.profiles_header).y + r(ids.profiles_header).h - FRAME, below - FRAME);
    let first = &ids.rows[0];
    assert_eq!(first.profile, "exploration", "the profile played last comes first");
    let row_h = 2.0 * INSET + 19.5 + 16.5;
    let square_y = below + (row_h - ICON_SQUARE) / 2.0;
    // The squares sit at the row's right, inside its inset, the second over the first by a frame.
    let squares = |row_x: f32| {
        let settings = row_x + part + 2.0 * INSET - INSET - ICON_SQUARE;
        (settings - ICON_SQUARE + FRAME, settings)
    };
    let row_x = left - INSET;
    rect_is(r(first.row), row_x, below, part + 2.0 * INSET, row_h, "first row");
    let (play, settings) = squares(row_x);
    rect_is(r(first.play), play, square_y, ICON_SQUARE, ICON_SQUARE, "play");
    rect_is(r(first.settings), settings, square_y, ICON_SQUARE, ICON_SQUARE, "settings");
    let row_x = left + part + COLUMNS_GAP - INSET;
    rect_is(r(ids.server_row), row_x, below, part + 2.0 * INSET, row_h, "server row");
    let (start, settings) = squares(row_x);
    rect_is(r(ids.server_play), start, square_y, ICON_SQUARE, ICON_SQUARE, "start");
    rect_is(r(ids.server_settings), settings, square_y, ICON_SQUARE, ICON_SQUARE, "server settings");
}

fn texts(list: &mere_base::DrawList) -> Vec<(Vec2, String)> {
    list.cmds.iter().filter_map(|c| if let mere_base::Cmd::Text { at, text, .. } = c { Some((*at, text.clone())) } else { None }).collect()
}

/// L4 (cont.): the name starts at the column, x 140 at 1400.
#[test]
fn l4_the_name_starts_at_the_column() {
    let (_, _, list) = home_at(1400.0, 940.0);
    let name = texts(&list).into_iter().find(|(_, t)| t == "Exploration").expect("the name is drawn whole");
    assert!((name.0.x - 140.0).abs() < 0.01, "{:?}", name.0);
}

/// L5: nothing clipped or overlapped, at every width the pictures use and a short height.
#[test]
fn l5_nothing_is_clipped_or_overlapped() {
    for (w, h) in [(1000.0, 940.0), (1200.0, 940.0), (1400.0, 940.0), (1920.0, 940.0), (1400.0, 600.0)] {
        let (ui, ids, list) = home_at(w, h);
        let (a, b) = (ui.rect(ids.profiles).unwrap(), ui.rect(ids.server).unwrap());
        assert!(a.intersect(&b).w <= 0.0 || a.intersect(&b).h <= 0.0, "{w}x{h}: the parts overlap: {a:?} {b:?}");
        let mut rects: Vec<Rect> = ids.rows.iter().flat_map(|r| [r.row, r.play, r.settings]).map(|i| ui.rect(i).unwrap()).collect();
        rects.extend([ui.rect(ids.server_row).unwrap(), ui.rect(ids.server_play).unwrap(), ui.rect(ids.server_settings).unwrap()]);
        for r in rects {
            assert!(r.x >= 20.0 - 0.01 && r.x + r.w <= w - 20.0 + 0.01, "{w}x{h}: {r:?} leaves the page's margins");
        }
        let said = texts(&list);
        // A title is drawn one glyph at a time (its letter spacing).
        let all: String = said.iter().map(|(_, t)| t.as_str()).collect();
        for name in ["Exploration", "Atelier", "Dev server", "PROFILES", "SERVER"] {
            assert!(all.contains(name), "{w}x{h}: {name} is cut or missing");
        }
    }
}

/// L6: a short window with a profile's settings and the server's fields open: the content scrolls, and its end
/// brings the server folder's "Open" wholly inside the pane.
#[test]
fn l6_a_short_window_scrolls_to_its_end() {
    let mut rig = Rig::new(1400, 600, true);
    // At this size the two parts just fit (the server's ends at 582 of 600, the preview's top being 10 and not the page's 24):
    // the server's note, which says why a start was refused, is what makes the content taller than the window.
    let why = "The Minecraft EULA is not accepted: read it, then set eula=true in the server's folder, and start the server again. ".repeat(3);
    rig.api.set_post("/api/server/start", 412, serde_json::json!({ "error": why }));
    let start = rig.home().ids().server_play;
    rig.click(start);
    let settings = rig.home().ids().rows[0].settings;
    rig.click(settings);
    let gear = rig.home().ids().server_settings;
    rig.click(gear);
    let thumb = rig.app.list().cmds.iter().any(|c| matches!(c, mere_base::Cmd::Fill { rect, color } if *color == rig.theme.line && rect.w < 8.0 && rect.h > 10.0));
    assert!(thumb, "the scrollbar's thumb is drawn");
    for _ in 0..20 {
        rig.app.event(mere_base::Event::Wheel { pos: Vec2::new(700.0, 400.0), delta: -3.0 });
        rig.frame();
    }
    let open = rig.rect(rig.home().ids().s_open_folder);
    assert!(open.y >= BAR_H && open.y + open.h <= rig.h as f32, "{open:?} is not inside the pane");
    rig.write("home_1400x600_scrolled.png");
}

/// L7: the account in the bar, whole in Home from 1000 to 1920 (a window raised to 1000 at the least).
#[test]
fn l7_the_account_after_the_name() {
    for w in [1000, 1200, 1400, 1920] {
        let rig = Rig::new(w, 940, true);
        let at = rig.texts().into_iter().find(|(_, t, _)| t == "PlayerOne").unwrap_or_else(|| panic!("no account at {w}"));
        // The brand's padding (12), the cube (16) and two gaps (10 each), then the brand's name, whatever it says.
        let after_name = 48.0 + preview::overflow::brand_look().width(&preview::text::BAR_TITLE.to_uppercase());
        assert!((at.0.x - after_name).abs() < 0.5, "{w}: {} for {after_name}", at.0.x);
        assert_eq!(rig.colour_of("PlayerOne"), Some(rig.theme.muted));
    }
    // The preview tool's bar holds the panels' buttons where Home has the brand and the account (the preview's
    // overflow.rs holds the rule), so the account is Home's only, at any width.
    for w in [1000, 1920] {
        let mut rig = Rig::new(w, 940, true);
        rig.app.state_mut().mode = Mode::Preview;
        rig.frame();
        assert!(rig.texts().into_iter().all(|(_, t, _)| !t.starts_with("Player")), "the account is shown in the preview tool at {w}");
    }
}

/// L9: a window asked smaller than the least is raised to it (the preview's MIN_SIZE, derived from its panels and bar).
#[test]
fn l9_a_small_window_is_raised_to_the_least() {
    let least = (preview::MIN_SIZE.0.ceil() as u32, preview::MIN_SIZE.1.ceil() as u32);
    let rig = Rig::new(400, 400, true);
    assert_eq!((rig.w, rig.h), least);
    let ids = rig.app.ids();
    assert_eq!(rig.rect(ids.bar).w, least.0 as f32);
}

/// L8: the preview tool in the launcher keeps the panels where the preview has them. Skipped without the data.
#[test]
fn l8_the_preview_tool_keeps_its_panels() {
    let dir = std::path::PathBuf::from(std::env::var_os("PREVIEW_DATA_DIR").unwrap_or_default());
    let dir = dir.as_path();
    if !dir.join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no world preview out folder");
        return;
    }
    let world = preview::world::World::load(dir).expect("the data loads");
    for w in [1000u32, 1200, 1400, 1920] {
        let theme = Theme::dark();
        let h = 940u32;
        let mut r = mere_win32::Renderer::offscreen(w, h, 1.0).unwrap();
        let mut app = launcher_native::app(theme, true, Vec2::new(w as f32, h as f32), Some(world.clone()), None, home_over(Arc::new(fixture())), true);
        app.start(&mut r);
        app.set_clock(Some(1.0));
        app.state_mut().mode = Mode::Preview;
        app.frame(&mut r, 0.0);
        app.frame(&mut r, 0.0);
        let ids = app.ids().clone();
        let (l, rt) = (app.ui().rect(ids.left).unwrap(), app.ui().rect(ids.right).unwrap());
        // The widths are the preview's own plan (natural widths, shrunk once the map is at its least).
        let plan = preview::panels::plan(w as f32, [true, true], [None, None]);
        let gap = preview::panels::GAP;
        rect_is(l, gap, 50.0, plan.widths[0], h as f32 - 72.0, &format!("left at {w}"));
        rect_is(rt, w as f32 - gap - plan.widths[1], 50.0, plan.widths[1], h as f32 - 72.0, &format!("right at {w}"));
        let stage = app.ui().rect(ids.frame).unwrap();
        let middle = w as f32 - preview::panels::gaps(2) as f32 * gap - plan.widths[0] - plan.widths[1];
        let side = middle.min(h as f32 - 136.0);
        assert!((stage.w - side).abs() < 0.01 || (stage.h - side).abs() < 0.01, "the map's square at {w}: {stage:?} against {side}");
        let account = app.list().cmds.iter().any(|c| matches!(c, mere_base::Cmd::Text { text, .. } if text.starts_with("Player")));
        assert!(!account, "the account is Home's only, at {w}");
    }
}
