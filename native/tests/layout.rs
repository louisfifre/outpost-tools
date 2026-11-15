//! The layout of Home against the page's numbers (every one cites its index.html line): the two parts fill the
//! window continuously, with no switch, and nothing is cut or overlaps.

mod common;

use std::sync::Arc;

use common::*;
use launcher_native::fake::fixture;
use nui_base::{App, Rect, Vec2};
use nui_ui::Theme;
use preview::{BAR_H, Mode, PreviewState, picture_with_home};

/// Home alone in its pane (no raise to the window's least), `w` x `h` as the window.
fn home_at(w: f32, h: f32) -> (nui_ui::Ui, launcher_native::HomeIds, nui_base::DrawList) {
    let mut home = loaded_home(Arc::new(fixture()));
    let state = PreviewState { mode: Mode::Home, reduce_motion: true, ..PreviewState::default() };
    let (list, ui, _) = picture_with_home(Theme::dark(), Vec2::new(w, h), None, state, &mut home);
    (ui, home.ids().clone(), list)
}

/// L1: side by side, every width from 1000 (the window's least) to 1920, one rule and no switch: line 148 (two minmax(0, 1fr) and a gap of 48), the page's
/// padding of 28 (line 145, side), the top of the preview's page (10) under the bar (40).
#[test]
fn l1_side_by_side_fills_the_width() {
    for w in 1000..=1920 {
        let (ui, ids, _) = home_at(w as f32, 940.0);
        let half = (w as f32 - 104.0) / 2.0;
        let (a, b) = (ui.rect(ids.profiles).unwrap(), ui.rect(ids.server).unwrap());
        assert!((a.x - 28.0).abs() < 0.01 && (a.y - 50.0).abs() < 0.01 && (a.w - half).abs() < 0.01, "profiles at {w}: {a:?}");
        assert!((b.x - (28.0 + half + 48.0)).abs() < 0.01 && (b.w - half).abs() < 0.01, "server at {w}: {b:?}");
    }
}

/// L4: at 1400 x 940, the page's rows: lines 154 (header of 34), 172-176 (a row of 56 pulled 8 px out on each
/// side), 160 (30 px squares) and 162 (the second pulled 1 px over the first).
#[test]
fn l4_the_numbers_at_1400() {
    let (ui, ids, _) = home_at(1400.0, 940.0);
    rect_is(ui.rect(ids.profiles_header).unwrap(), 28.0, 50.0, 648.0, 34.0, "header");
    let r = |id| ui.rect(id).unwrap();
    // The hairline is the header's last pixel row.
    assert_eq!(r(ids.profiles_header).y + r(ids.profiles_header).h - 1.0, 83.0);
    let first = &ids.rows[0];
    assert_eq!(first.profile, "exploration", "the profile played last comes first");
    rect_is(r(first.row), 20.0, 84.0, 664.0, 56.0, "first row");
    rect_is(r(first.play), 617.0, 97.0, 30.0, 30.0, "play");
    rect_is(r(first.settings), 646.0, 97.0, 30.0, 30.0, "settings");
    rect_is(r(ids.server_row), 716.0, 84.0, 664.0, 56.0, "server row");
    rect_is(r(ids.server_play), 1313.0, 97.0, 30.0, 30.0, "start");
    rect_is(r(ids.server_settings), 1342.0, 97.0, 30.0, 30.0, "server settings");
}

fn texts(list: &nui_base::DrawList) -> Vec<(Vec2, String)> {
    list.cmds.iter().filter_map(|c| if let nui_base::Cmd::Text { at, text, .. } = c { Some((*at, text.clone())) } else { None }).collect()
}

/// L4 (cont.): the name starts at x 28.
#[test]
fn l4_the_name_starts_at_the_column() {
    let (_, _, list) = home_at(1400.0, 940.0);
    let name = texts(&list).into_iter().find(|(_, t)| t == "Exploration").expect("the name is drawn whole");
    assert!((name.0.x - 28.0).abs() < 0.01, "{:?}", name.0);
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
    let thumb = rig.app.list().cmds.iter().any(|c| matches!(c, nui_base::Cmd::Fill { rect, color } if *color == rig.theme.line && rect.w < 8.0 && rect.h > 10.0));
    assert!(thumb, "the scrollbar's thumb is drawn");
    for _ in 0..20 {
        rig.app.event(nui_base::Event::Wheel { pos: Vec2::new(700.0, 400.0), delta: -3.0 });
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
        assert!((at.0.x - 119.28).abs() < 0.5, "{w}: {}", at.0.x);
        assert_eq!(rig.colour_of("PlayerOne"), Some(rig.theme.muted));
    }
    // The preview tool at 1000: the name ends where the tools begin.
    let mut rig = Rig::new(1000, 940, true);
    rig.app.state_mut().mode = Mode::Preview;
    rig.frame();
    let strip = rig.rect(rig.app.ids().strip);
    let shown = rig.texts().into_iter().find(|(_, t, _)| t.starts_with("Player")).expect("the account");
    assert!(shown.0.x + shown.1.chars().count() as f32 * 7.2 <= strip.x + 0.01, "{shown:?} runs into the strip at {}", strip.x);
}

/// L9: a window asked at 800 is the least, 1000 (the app's own rule).
#[test]
fn l9_a_small_window_is_raised_to_the_least() {
    let rig = Rig::new(800, 500, true);
    assert_eq!((rig.w, rig.h), (1000, 600));
    let ids = rig.app.ids();
    assert_eq!(rig.rect(ids.bar).w, 1000.0);
}

/// L8: the preview tool in the launcher keeps the panels where the preview has them. Skipped without the data.
#[test]
fn l8_the_preview_tool_keeps_its_panels() {
    let dir = std::path::Path::new(r"C:\Games\preview\out");
    if !dir.join("preview.json").exists() {
        println!("skipped: no C:\\Games\\preview\\out");
        return;
    }
    let world = preview::world::World::load(dir).expect("the data loads");
    for w in [1000u32, 1200, 1400, 1920] {
        let theme = Theme::dark();
        let h = 940u32;
        let mut r = nui_win32::Renderer::offscreen(w, h, 1.0).unwrap();
        let mut app = launcher_native::app(theme, true, Vec2::new(w as f32, h as f32), Some(world.clone()), None, home_over(Arc::new(fixture())), true);
        app.start(&mut r);
        app.set_clock(Some(1.0));
        app.state_mut().mode = Mode::Preview;
        app.frame(&mut r, 0.0);
        app.frame(&mut r, 0.0);
        let ids = app.ids().clone();
        let (l, rt) = (app.ui().rect(ids.left).unwrap(), app.ui().rect(ids.right).unwrap());
        rect_is(l, 28.0, 50.0, 320.0, h as f32 - 72.0, &format!("left at {w}"));
        rect_is(rt, w as f32 - 358.0, 50.0, 330.0, h as f32 - 72.0, &format!("right at {w}"));
        let stage = app.ui().rect(ids.frame).unwrap();
        let side = (w as f32 - 762.0).min(h as f32 - 136.0);
        assert!((stage.w - side).abs() < 0.01 || (stage.h - side).abs() < 0.01, "the map's square at {w}: {stage:?} against {side}");
        assert!(app.list().cmds.iter().any(|c| matches!(c, nui_base::Cmd::Text { text, .. } if text.starts_with("Player"))), "the account at {w}");
    }
}
