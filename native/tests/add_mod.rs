//! The page that adds a mod to a profile (board B4): its lines, what each press asks of the api, and its picture.
//! Everything answers from the fake; the picture goes to target\tmp.

mod common;

use std::sync::Arc;

use common::*;
use launcher_native::fake::{FakeApi, fixture, found, mod_menu_added, mods_fixture};
use launcher_native::pages::Page;
use mere_base::{App, Event, Vec2};
use mere_ui::Theme;
use mere_win32::Renderer;
use preview::world::World;
use serde_json::json;

/// The app at `w` x `h` with the page that adds a mod to Exploration already open over Home, as the profile's own
/// action will open it.
fn on_the_page(api: Arc<FakeApi>, w: u32, h: u32) -> Rig {
    let theme = Theme::dark();
    let (pw, ph) = preview::min_size_px((w, h));
    let mut r = Renderer::offscreen(pw, ph, 1.0).expect("offscreen renderer");
    let mut home = home_over(api.clone());
    preview::Home::poll(&mut home, 0.0, true);
    home.open(Page::Profile("exploration".into()));
    home.open(Page::AddMod("exploration".into()));
    let mut app = launcher_native::app(theme, true, Vec2::new(pw as f32, ph as f32), None::<World>, None, home, true);
    app.start(&mut r);
    app.event(Event::Resized { width_px: pw, height_px: ph, scale: 1.0 });
    app.event(Event::Window { maximized: false });
    app.set_clock(Some(1.0));
    let mut rig = Rig { app, r, api, theme, w: pw, h: ph, clock: 1.0 };
    rig.frame();
    rig.frame();
    rig
}

fn api() -> Arc<FakeApi> {
    let api = Arc::new(fixture());
    mods_fixture(&api, "add");
    api
}

fn said(rig: &Rig) -> String {
    rig.texts().iter().map(|t| t.1.as_str()).collect::<Vec<_>>().join("|")
}

/// Where a word is drawn, the first time it is.
fn at(rig: &Rig, words: &str) -> Vec2 {
    rig.texts().into_iter().find(|t| t.1 == words).unwrap_or_else(|| panic!("{words:?} is not drawn: {}", said(rig))).0
}

/// The word `words` that sits on the line of `line` (the same row), for the words every line has.
fn on_line_of(rig: &Rig, line: &str, words: &str) -> Vec2 {
    let y = at(rig, line).y;
    rig.texts().into_iter().find(|t| t.1 == words && (t.0.y - y).abs() < 20.0).unwrap_or_else(|| panic!("{words:?} is not on the line of {line:?}")).0
}

fn press(rig: &mut Rig, at: Vec2) {
    rig.click_at(Vec2::new(at.x + 6.0, at.y + 6.0));
}

/// B4: the list as the board shows it, then Add on Mod Menu names what came with it.
#[test]
fn b4_the_list_and_what_came_with_a_mod() {
    let api = api();
    mod_menu_added(&api);
    let mut rig = on_the_page(api.clone(), 1400, 940);
    assert_eq!(api.last_body("POST", "/api/mods/search").unwrap(), json!({"profile": "exploration", "query": ""}));
    for words in ["Search Modrinth", "Add a file", "Sodium", "0.7.1", "\u{2192} 0.7.3", "Update", "Iris Shaders", "Added", "JourneyMap", "Chat Heads", "Continuity", "Faster game logic, the same game."] {
        assert!(rig.says(words), "the page says {words:?}: {}", said(&rig));
    }
    // The title is drawn letter by letter (the heading look).
    assert!(rig.texts().iter().map(|t| t.1.as_str()).collect::<String>().contains("Add a mod to Exploration"));
    // The list is capped at 760 wide and centred: 1400 wide, so it starts at 320.
    let x = at(&rig, "Sodium").x;
    assert!((x - 320.0).abs() < 1.0, "the column starts at 320: {x}");
    // Press Add on Mod Menu: it asks, the mod becomes added, and Fabric API is named under it.
    mods_fixture(&api, "added");
    let add = on_line_of(&rig, "Mod Menu", "Add");
    press(&mut rig, add);
    assert_eq!(api.last_body("POST", "/api/mods/add").unwrap(), json!({"profile": "exploration", "mod": "modmenu"}));
    rig.frame();
    assert!(rig.says("Fabric API came with it: Mod Menu needs it."), "{}", said(&rig));
    assert_eq!(rig.texts().iter().filter(|t| t.1 == "Added").count(), 2, "Iris and Mod Menu are added");
    rig.write("add_mod_b4_1400.png");
}

/// Typing filters: the search asks with the query and shows its answer.
#[test]
fn typing_searches_and_the_list_shows_its_answer() {
    let api = api();
    let mut rig = on_the_page(api.clone(), 1400, 940);
    assert!(rig.says("Continuity"));
    api.set_post("/api/mods/search", 200, json!({"mods": [found("sodium", "Sodium", "A faster renderer for the game.", "0.7.1", "update", "0.7.3")]}));
    // The field has the keyboard when the page opens.
    for ch in "sod".chars() {
        rig.app.event(Event::Char { ch });
        rig.frame();
    }
    assert_eq!(api.last_body("POST", "/api/mods/search").unwrap(), json!({"profile": "exploration", "query": "sod"}));
    assert!(rig.says("Sodium") && !rig.says("Continuity"), "{}", said(&rig));
}

/// Nothing found, and a failure of Modrinth, say one sentence and show no line.
#[test]
fn an_empty_answer_and_a_failure_say_one_sentence() {
    let api = api();
    api.set_post("/api/mods/search", 200, json!({"mods": []}));
    let rig = on_the_page(api, 1400, 940);
    assert!(rig.says("No mod matches."));
    let api = Arc::new(fixture());
    api.set_post("/api/mods/search", 503, json!({"error": "Modrinth cannot be reached (offline)"}));
    let rig = on_the_page(api, 1400, 940);
    assert!(rig.says("Modrinth cannot be reached (offline)"), "{}", said(&rig));
    assert!(!rig.says("Sodium"));
}

/// The page's other actions: Update, the file and the Modrinth page each ask the api.
#[test]
fn update_the_file_and_the_modrinth_page_each_ask_the_api() {
    let api = api();
    let mut rig = on_the_page(api.clone(), 1400, 940);
    let update = on_line_of(&rig, "Sodium", "Update");
    let file = at(&rig, "Add a file");
    press(&mut rig, update);
    assert_eq!(api.last_body("POST", "/api/mods/update").unwrap(), json!({"profile": "exploration", "mod": "sodium"}));
    press(&mut rig, file);
    assert_eq!(api.last_body("POST", "/api/mods/file").unwrap(), json!({"profile": "exploration"}));
    // The icon at the end of Sodium's line: the last 32 px of the 760.
    let end = Vec2::new(320.0 + 760.0 - 16.0, update.y + 8.0);
    rig.click_at(end);
    assert_eq!(api.last_body("POST", "/api/mods/open").unwrap(), json!({"url": "https://modrinth.com/mod/sodium"}));
}
