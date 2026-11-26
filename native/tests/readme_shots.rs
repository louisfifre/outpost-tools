//! The README's pictures (docs\shots): each screen, dark and light, from the fake api and the fixed date. They go to
//! target\tmp as `readme-<screen>-<theme>.png`; nothing here reads the machine, the network or a real profile.

mod common;

use std::sync::Arc;

use common::*;
use mere_base::{App, Event, Vec2};
use mere_ui::Theme;
use mere_win32::Renderer;
use outpost_tools::fake::{FakeApi, board_server, fixture, java_missing, mod_menu_added, mods_fixture, with_accounts};
use outpost_tools::pages::Page;
use preview::Home;
use preview::world::World;
use serde_json::json;

/// The whole app at 1400 x 940 with `pages` opened over Home one after the other, drawn in `dark` or light.
fn screen(api: Arc<FakeApi>, pages: &[Page], dark: bool) -> Rig {
    let theme = if dark { Theme::dark() } else { Theme::light() };
    let (w, h) = preview::min_size_px((1400, 940));
    let mut r = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
    let mut home = home_over(api.clone());
    home.poll(0.0, true);
    for p in pages {
        home.open(p.clone());
    }
    let mut app = outpost_tools::app(theme, dark, Vec2::new(w as f32, h as f32), None::<World>, None, home, true);
    app.start(&mut r);
    app.event(Event::Resized { width_px: w, height_px: h, scale: 1.0 });
    app.event(Event::Window { maximized: false });
    app.set_clock(Some(1.0));
    let mut rig = Rig { app, r, api, theme, w, h, clock: 1.0 };
    rig.frame();
    rig.frame();
    rig.wait(1.0);
    // Out of the way of every widget, so that nothing is lit.
    rig.hover_at(Vec2::new(5.0, 930.0));
    rig
}

fn board_pc() -> Arc<FakeApi> {
    let api = Arc::new(fixture());
    let mut v = api_get(&api, "/api/profiles");
    v["machineMb"] = json!(16384);
    v["usedMb"] = json!(5222);
    v["profiles"][1]["minMemoryMb"] = json!(2048);
    api.set_get("/api/profiles", v);
    with_accounts(&api);
    api
}

#[test]
fn the_readme_pictures() {
    for dark in [true, false] {
        let theme = if dark { "dark" } else { "light" };
        let exploration = Page::Profile("exploration".into());

        let api = board_pc();
        board_server(&api);
        screen(api, &[], dark).write(&format!("readme-home-{theme}.png"));

        screen(board_pc(), &[exploration.clone()], dark).write(&format!("readme-profile-{theme}.png"));

        let api = board_pc();
        mods_fixture(&api, "add");
        mod_menu_added(&api);
        screen(api, &[exploration.clone(), Page::AddMod("exploration".into())], dark).write(&format!("readme-add-mod-{theme}.png"));

        let api = board_pc();
        board_server(&api);
        screen(api, &[Page::Server], dark).write(&format!("readme-server-{theme}.png"));

        let api = Arc::new(fixture());
        with_accounts(&api);
        java_missing(&api, 51 * 1024 * 1024);
        screen(api, &[Page::Setup], dark).write(&format!("readme-setup-{theme}.png"));
    }
}
