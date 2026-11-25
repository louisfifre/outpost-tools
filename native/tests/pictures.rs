//! The pictures of the native launcher: Home over a sweep of widths, light, its settings and menu, a launch, a
//! short window, and the world preview tool inside it. They go to target\tmp; the ones worth keeping are copied
//! into docs\shots. Every picture comes from the fake api and the fixed date.

mod common;

use std::sync::Arc;

use common::*;
use launcher_native::fake::fixture;
use mere_base::{App, DrawList, Gfx, Vec2};
use mere_ui::Theme;
use mere_win32::Renderer;
use preview::{Mode, PreviewState, picture_with_home};
use serde_json::json;

fn draw(list: &DrawList, theme: Theme, w: u32, h: u32) -> Vec<u8> {
    let mut r = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
    r.begin(theme.ground);
    r.draw_list(list);
    r.end();
    r.read_rgba().unwrap()
}

fn save(rgba: &[u8], w: u32, h: u32, name: &str) {
    let path = out_dir().join(name);
    std::fs::write(&path, mere_png::encode(w, h, rgba)).unwrap();
    println!("wrote {}", path.display());
}

/// Home alone at any size, even under the window's least: for the widths no window has.
fn home_picture(w: u32, h: u32, dark: bool) -> Vec<u8> {
    let theme = if dark { Theme::dark() } else { Theme::light() };
    let mut home = loaded_home(Arc::new(fixture()));
    let account = preview::Home::account(&home);
    let account_menu = preview::Home::account_menu(&home);
    let state = PreviewState { mode: Mode::Home, reduce_motion: true, account, account_menu, ..PreviewState::default() };
    let (list, _, _) = picture_with_home(theme, Vec2::new(w as f32, h as f32), None, state, &mut home);
    draw(&list, theme, w, h)
}

#[test]
fn home_pictures_over_the_sweep() {
    for (w, h, name) in [(620, 940, "home_620.png"), (800, 940, "home_800.png"), (1000, 940, "home_1000.png"), (1400, 940, "home_1400.png"), (1920, 940, "home_1920.png")] {
        let rgba = home_picture(w, h, true);
        save(&rgba, w, h, name);
        assert!(rgba.chunks(4).any(|p| p[..3] != rgba[..3]), "{name} is one colour");
    }
    let rgba = home_picture(1400, 940, false);
    save(&rgba, 1400, 940, "home_1400_light.png");
    // The light ground, the light theme's `bg-ground`.
    let g = mere_ui::tokens::palette::GREY_50;
    assert_eq!(&rgba[..3], &[(g >> 16) as u8, (g >> 8) as u8, g as u8]);
}

/// P0: the full app in Home draws the same bytes as the Home alone at its size.
#[test]
fn p0_the_app_and_the_picture_agree() {
    let mut rig = Rig::new(1400, 940, true);
    rig.frame();
    let from_app = rig.r.read_rgba().unwrap();
    let alone = home_picture(1400, 940, true);
    assert_eq!(from_app.len(), alone.len());
    let differing = from_app.iter().zip(&alone).filter(|(a, b)| a != b).count();
    assert_eq!(differing, 0, "{differing} bytes differ");
}

#[test]
fn the_short_window_with_everything_open() {
    let mut rig = Rig::new(1400, 600, true);
    let why = "The Minecraft EULA is not accepted: read it, then set eula=true in the server's folder, and start the server again. ".repeat(3);
    rig.api.set_post("/api/server/start", 412, json!({ "error": why }));
    let start = rig.home().ids().server_play;
    rig.click(start);
    open_server(&mut rig);
    // Out of the way of every widget, so that nothing is lit.
    rig.hover_at(Vec2::new(5.0, 590.0));
    rig.write("home_1400x600.png");
}

#[test]
fn the_settings_with_the_menu_open() {
    let mut rig = Rig::new(1400, 940, true);
    open_profile(&mut rig, "exploration");
    let b = rig.rect(rig.home().ids().open_on);
    rig.click_at(centre(b));
    // "a server", the third row, under the pointer.
    rig.hover_at(Vec2::new(b.x + 30.0, b.y + b.h + mere_ui::tokens::GAP_TIGHT + mere_ui::LIST_PAD + (2.0 + 0.5) * mere_ui::LIST_ROW));
    assert!(rig.says("a server") && rig.says("the game menu"));
    rig.write("home_settings_1400.png");
}

#[test]
fn a_launch_and_a_running_server() {
    let mut rig = Rig::new(1400, 940, true);
    let mut server = api_get(&rig.api, "/api/server");
    server["state"] = json!("running");
    rig.api.set_get("/api/server", server);
    let play = rig.home().ids().rows[0].play;
    rig.click(play);
    rig.api.set_get("/api/launch", json!({"id": "exploration", "state": "loading"}));
    rig.set_clock(13.0);
    rig.frame();
    rig.hover_at(Vec2::new(5.0, 930.0));
    assert!(rig.says("Loading the game\u{2026} 12 s") && rig.says("running \u{b7} localhost:25565"));
    rig.write("home_launch_1400.png");
}

fn preview_rig(w: u32, h: u32, dark: bool) -> Option<Rig> {
    let dir = std::path::PathBuf::from(std::env::var_os("PREVIEW_DATA_DIR").unwrap_or_default());
    let dir = dir.as_path();
    if !dir.join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no world preview out folder");
        return None;
    }
    let world = preview::world::World::load(dir).expect("the data loads");
    let theme = if dark { Theme::dark() } else { Theme::light() };
    let (pw, ph) = preview::min_size_px((w, h));
    let api = Arc::new(fixture());
    let mut r = Renderer::offscreen(pw, ph, 1.0).unwrap();
    let mut app = launcher_native::app(theme, dark, Vec2::new(pw as f32, ph as f32), Some(world), None, home_over(api.clone()), true);
    app.start(&mut r);
    app.event(mere_base::Event::Resized { width_px: pw, height_px: ph, scale: 1.0 });
    app.set_clock(Some(1.0));
    app.state_mut().mode = Mode::Preview;
    let mut rig = Rig { app, r, api, theme, w: pw, h: ph, clock: 1.0 };
    rig.frame();
    rig.frame();
    Some(rig)
}

#[test]
fn the_preview_tool_in_the_launcher() {
    for (w, name, dark) in [(1000, "launcher_preview_1000.png", true), (1400, "launcher_preview_1400.png", true), (1920, "launcher_preview_1920.png", true), (1400, "launcher_preview_1400_light.png", false)] {
        let Some(mut rig) = preview_rig(w, 940, dark) else { return };
        // The preview tool's bar holds the panels' buttons where Home has the brand and the account (preview overflow.rs).
        assert!(!rig.texts().iter().any(|t| t.1.starts_with("Player")), "the account is Home's only, at {w}");
        rig.write(name);
    }
}

/// A1: Home at 1400, the board's four profiles (the last one cut with an ellipsis) and the server, stopped then running.
#[test]
fn a1_home_full() {
    // The bar at rest shows the account's face before its name (A1): its skin is read with the first answer.
    let api = board_api();
    launcher_native::fake::with_accounts(&api);
    let mut rig = Rig::with_api(api, 1400, 940, true, true);
    rig.hover_at(Vec2::new(5.0, 930.0));
    assert!(rig.says("stopped \u{b7} joins at localhost:25565"));
    // The profiles' lines end with what the profile holds (the board): the updates, else the mods.
    for line in ["yesterday \u{b7} 1.7 h played \u{b7} 3 updates", "not played yet", "2 weeks ago \u{b7} no mods", "last month \u{b7} 41 mods"] {
        assert!(rig.says(line), "{line}: {:?}", rig.texts().iter().map(|t| t.1.clone()).collect::<Vec<_>>());
    }
    let long = rig.texts().into_iter().find(|t| t.1.starts_with("Friday server pack")).expect("the long name is drawn");
    assert!(long.1.ends_with('\u{2026}'), "the long name is cut with an ellipsis: {:?}", long.1);
    rig.write("home_a1_1400.png");
    let mut server = api_get(&rig.api, "/api/server");
    server["state"] = json!("running");
    rig.api.set_get("/api/server", server);
    rig.wait(3.0);
    rig.hover_at(Vec2::new(5.0, 930.0));
    assert!(rig.says("running \u{b7} localhost:25565"));
    rig.write("home_a1_running_1400.png");
}

/// A3: Home at 800 x 600, the two parts stacked.
#[test]
fn a3_home_small() {
    let mut rig = Rig::with_api(board_api(), 800, 600, true, true);
    rig.hover_at(Vec2::new(5.0, 590.0));
    let profiles = rig.rect(rig.home().ids().profiles);
    let servers = rig.rect(rig.home().ids().server);
    assert!(servers.y > profiles.y + profiles.h, "stacked");
    rig.write("home_a3_800.png");
}
