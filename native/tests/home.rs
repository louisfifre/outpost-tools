//! What Home does, driven by a fake api through an inline bridge and a clock the tests hold. The fake is the
//! only thing asked: the helper builds no `Api` (H14).

mod common;

use common::*;
use mere_base::{App, Key, Vec2};
use preview::Mode;
use serde_json::json;

const EULA: &str = "The Minecraft EULA is not accepted yet: set eula=true in the server's folder.";

fn open_menu_row(rig: &mut Rig, row: usize) {
    let b = rig.rect(rig.home().ids().open_on);
    rig.click_at(centre(b));
    let p = Vec2::new(b.x + 30.0, b.y + b.h + mere_ui::tokens::GAP_TIGHT + mere_ui::LIST_PAD + (row as f32 + 0.5) * mere_ui::LIST_ROW);
    rig.click_at(p);
}

fn settings_of_exploration(rig: &mut Rig) {
    open_profile(rig, "exploration");
}

/// H1: the first frame asks for the profiles and the server; the profile played last is above.
#[test]
fn h1_the_first_frame_asks_and_orders() {
    let mut rig = Rig::with_api(std::sync::Arc::new(outpost_tools::fake::fixture()), 1400, 940, true, false);
    rig.frame();
    assert_eq!(rig.api.count("GET", "/api/profiles"), 1);
    assert_eq!(rig.api.count("GET", "/api/server"), 1);
    let ids = rig.home().ids().clone();
    assert_eq!(ids.rows.len(), 2);
    assert_eq!(ids.rows[0].profile, "exploration");
    assert!(rig.rect(ids.rows[0].row).y < rig.rect(ids.rows[1].row).y);
}

/// H2: the line under a profile.
#[test]
fn h2_the_lines() {
    let rig = Rig::new(1400, 940, true);
    assert!(rig.says("yesterday \u{b7} 1.7 h played \u{b7} opens New World"));
    assert!(rig.says("not played yet"));
}

/// H3: Play posts, says what the launch does in the text colour, locks both Plays, follows, and stops following.
#[test]
fn h3_play_and_the_follow() {
    let mut rig = Rig::new(1400, 940, true);
    let ids = rig.home().ids().rows.clone();
    rig.click(ids[0].play);
    assert_eq!(rig.api.count("POST", "/api/launch/exploration"), 1);
    assert_eq!(rig.colour_of("Starting\u{2026}"), Some(rig.theme.text), "the news of the moment is in the text colour");
    // One launch at a time: the other Play does nothing.
    rig.click(ids[1].play);
    assert_eq!(rig.api.count("POST", "/api/launch/atelier"), 0);
    // A press on the disabled Play falls to the line, which opens the profile's page: back to Home.
    assert_eq!(rig.home().page(), Some(&outpost_tools::pages::Page::Profile("atelier".into())));
    let back = rig.home().ids().back;
    rig.click(back);
    // The page's every second, from the click (clock 1.0): 12 s on, the game is loading.
    rig.api.set_get("/api/launch", json!({"id": "exploration", "state": "loading"}));
    rig.set_clock(13.0);
    rig.frame();
    assert!(rig.says("Loading the game\u{2026} 12 s"), "{:?}", rig.texts().iter().map(|t| t.1.clone()).collect::<Vec<_>>());
    assert_eq!(rig.colour_of("Loading the game\u{2026} 12 s"), Some(rig.theme.text));
    rig.api.set_get("/api/launch", json!({"id": "exploration", "state": "running"}));
    rig.wait(2.0);
    assert!(rig.says("Running"));
    let asked = rig.api.count("GET", "/api/launch");
    rig.wait(3.0);
    assert_eq!(rig.api.count("GET", "/api/launch"), asked, "running ends the follow");
}

/// H4: a profile's settings, the menu, a server address that is typed and saved.
#[test]
fn h4_settings_and_the_address() {
    let mut rig = Rig::new(1400, 940, true);
    settings_of_exploration(&mut rig);
    let said: String = rig.texts().iter().map(|t| t.1.as_str()).collect();
    assert!(said.contains("Exploration") && !said.contains("EXPLORATION"), "the title line keeps the name's case");
    assert!(rig.says("world: New World"));
    open_menu_row(&mut rig, 2);
    assert_eq!(rig.api.count("POST", "/api/settings/exploration"), 0, "a server without its address is not a choice yet");
    let ids = rig.home().ids().clone();
    assert_eq!(rig.app.ui().focused(), Some(ids.address), "the address has the keyboard");
    for ch in "localhost".chars() {
        rig.app.event(mere_base::Event::Char { ch });
    }
    rig.key(Key::Enter);
    let body = rig.api.last_body("POST", "/api/settings/exploration").expect("saved");
    assert_eq!(body, json!({"javaArgs": "-XX:+UseZGC", "memoryMb": 6144, "minMemoryMb": 1024, "open": {"kind": "server", "target": "localhost"}}));
    assert!(rig.says("Saved."));
}

/// H5: a refusal and a lock, said as the page says them.
#[test]
fn h5_refused_then_locked() {
    let mut rig = Rig::new(1400, 940, true);
    settings_of_exploration(&mut rig);
    rig.api.set_post("/api/settings/exploration", 400, json!({}));
    open_menu_row(&mut rig, 0);
    assert!(rig.says("Setting refused, nothing changed."));
    rig.api.set_post("/api/settings/exploration", 409, json!({}));
    open_menu_row(&mut rig, 1);
    assert!(rig.says("Minecraft is running: settings can change once it is closed."));
}

/// H6: Prism running: the four fields are disabled and the note says why.
#[test]
fn h6_locked_fields() {
    let api = std::sync::Arc::new(outpost_tools::fake::fixture());
    with_server_profile(&api);
    let mut v = api_get(&api, "/api/profiles");
    v["locked"] = json!(true);
    api.set_get("/api/profiles", v);
    let mut rig = Rig::with_api(api, 1400, 940, true, true);
    settings_of_exploration(&mut rig);
    assert!(rig.says("Minecraft is running: settings can change once it is closed."));
    let ids = rig.home().ids().clone();
    for (what, id) in [("address", ids.address), ("memory", ids.memory), ("java", ids.java)] {
        rig.click(id);
        assert_eq!(rig.app.ui().focused(), None, "{what} takes no keyboard");
    }
    let b = rig.rect(ids.open_on);
    rig.click_at(centre(b));
    rig.click_at(Vec2::new(b.x + 30.0, b.y + b.h + 1.0 + 10.0));
    assert_eq!(rig.api.count("POST", "/api/settings/exploration"), 0, "the select does not open");
}

/// H8: Back returns to the list, says nothing, and asks the profiles again.
#[test]
fn h8_back() {
    let mut rig = Rig::new(1400, 940, true);
    settings_of_exploration(&mut rig);
    rig.api.set_post("/api/settings/exploration", 400, json!({}));
    open_menu_row(&mut rig, 0);
    assert!(rig.says("Setting refused, nothing changed."));
    let asked = rig.api.count("GET", "/api/profiles");
    let back = rig.home().ids().back;
    rig.click(back);
    assert_eq!(rig.home().ids().rows.len(), 2, "the list is back");
    assert_eq!(rig.home().notes().0, "");
    assert!(rig.api.count("GET", "/api/profiles") > asked);
}

/// H9: the server's start, its refusal, and its stop.
#[test]
fn h9_start_and_stop() {
    let mut rig = Rig::new(1400, 940, true);
    rig.api.set_post("/api/server/start", 412, json!({"error": EULA}));
    let start = rig.home().ids().server_play;
    rig.click(start);
    assert_eq!(rig.api.count("POST", "/api/server/start"), 1);
    assert_eq!(rig.home().notes().1, EULA);
    // Running: the toggle is the stop square.
    let mut v = api_get(&rig.api, "/api/server");
    v["state"] = json!("running");
    rig.api.set_get("/api/server", v);
    rig.wait(3.0);
    assert!(rig.says("running \u{b7} localhost:25565"));
    let toggle = rig.home().ids().server_play;
    let c = centre(rig.rect(toggle));
    rig.hover_at(c);
    rig.set_clock(rig.clock + 0.6);
    rig.frame();
    assert!(rig.says("Stop the server"), "the tip names the stop");
    rig.click(toggle);
    assert!(rig.says("Saving and stopping\u{2026}"));
    assert_eq!(rig.api.count("POST", "/api/server/stop"), 1);
}

/// A running server, its stop button found.
fn running_server(rig: &mut Rig) -> mere_ui::Id {
    let mut v = api_get(&rig.api, "/api/server");
    v["state"] = json!("running");
    rig.api.set_get("/api/server", v);
    rig.wait(3.0);
    rig.home().ids().server_play
}

/// H9b: a stop the server did not take says the server's reason and gives the button back.
#[test]
fn h9b_a_refused_stop_says_why_and_releases_the_button() {
    let mut rig = Rig::new(1400, 940, true);
    let stop = running_server(&mut rig);
    rig.api.set_post("/api/server/stop", 502, json!({"error": "Exception: RCON login refused"}));
    rig.click(stop);
    assert_eq!(rig.home().notes().1, "Exception: RCON login refused");
    rig.wait(3.0);
    assert_eq!(rig.home().notes().1, "Exception: RCON login refused", "the next poll keeps the reason");
    assert!(rig.says("running \u{b7} localhost:25565"));
    rig.click(stop);
    assert_eq!(rig.api.count("POST", "/api/server/stop"), 2, "the button is free again");
}

/// H9c: a stop taken by a server that is still running 30 s later gives the button back.
#[test]
fn h9c_a_stop_that_does_not_end_the_server_releases_the_button_after_30_s() {
    let mut rig = Rig::new(1400, 940, true);
    let stop = running_server(&mut rig);
    rig.click(stop);
    rig.wait(27.0);
    assert!(rig.says("Saving and stopping\u{2026}"));
    rig.wait(4.0);
    assert!(rig.says("running \u{b7} localhost:25565"));
    rig.click(stop);
    assert_eq!(rig.api.count("POST", "/api/server/stop"), 2);
}

/// H9d: an api that cannot read the pack (503) says why under both parts, and the reason goes once it answers.
#[test]
fn h9d_an_unreadable_pack_says_why_until_it_is_read() {
    const WHY: &str = "scripts/common.ps1 could not be read: The term 'Find-Prism' is not recognized.";
    let api = std::sync::Arc::new(outpost_tools::fake::fixture());
    let (profiles, server) = (api_get(&api, "/api/profiles"), api_get(&api, "/api/server"));
    api.set_get_status("/api/profiles", 503, json!({"error": WHY}));
    api.set_get_status("/api/server", 503, json!({"error": WHY}));
    let mut rig = Rig::with_api(api, 1400, 940, true, true);
    assert_eq!(rig.home().notes(), (WHY, WHY));
    assert!(rig.texts().iter().any(|t| t.1.starts_with("scripts/common.ps1 could not")), "the reason is said, wrapped in the part's width");
    rig.api.set_get("/api/profiles", profiles);
    rig.api.set_get("/api/server", server);
    rig.wait(10.0);
    assert_eq!(rig.home().notes(), ("", ""));
}

/// H10: the server's fields: its page shows them; without props, or while it runs, they are locked.
#[test]
fn h10_server_fields_and_locks() {
    let mut rig = Rig::new(1400, 940, true);
    assert_eq!(rig.home().ids().s_memory, mere_ui::Id::ROOT, "Home does not show them");
    open_server(&mut rig);
    assert_ne!(rig.home().ids().s_memory, mere_ui::Id::ROOT);
    // No props: the first start has not happened.
    rig.api.set_get("/api/server", json!({"dir": "C:\\Code\\server", "state": "stopped", "eula": true, "memoryMb": 6144, "javaArgs": "", "props": null}));
    rig.wait(3.0);
    assert_eq!(rig.home().notes().1, "Distances, players, port and line appear after the first start.");
    let view = rig.home().ids().s_view;
    rig.click(view);
    assert_eq!(rig.app.ui().focused(), None);
    // Starting: the memory is locked too.
    rig.api.set_get("/api/server", json!({"dir": "C:\\Code\\server", "state": "starting", "eula": true, "memoryMb": 6144, "javaArgs": "", "props": null}));
    rig.wait(3.0);
    assert_eq!(rig.home().notes().1, "The server is running: settings can change once it is stopped.");
    let mem = rig.home().ids().s_memory;
    rig.click(mem);
    assert_eq!(rig.app.ui().focused(), None);
}

/// H11: the whitelist switch saves at once.
#[test]
fn h11_whitelist() {
    let mut rig = Rig::new(1400, 940, true);
    open_server(&mut rig);
    let w = rig.home().ids().s_whitelist;
    rig.click(w);
    let body = rig.api.last_body("POST", "/api/server/settings").expect("saved");
    assert_eq!(body["props"]["whitelist"], json!(false));
    assert_eq!(body["props"]["viewDistance"], json!(12));
    assert_eq!(body["memoryMb"], json!(6144));
    assert!(rig.says("Saved."));
}

/// H12: the server is asked every 3 s while Home shows, never while the preview does, and both again on the way back.
#[test]
fn h12_polls() {
    let mut rig = Rig::new(1400, 940, true);
    let n = rig.api.count("GET", "/api/server");
    rig.wait(3.0);
    assert_eq!(rig.api.count("GET", "/api/server"), n + 1);
    // To the preview tool: nothing is asked.
    let preview = rig.app.ids().tools[1];
    rig.click(preview);
    assert_eq!(rig.app.state().mode, Mode::Preview);
    let n = rig.api.count("GET", "/api/server");
    let p = rig.api.count("GET", "/api/profiles");
    rig.wait(12.0);
    assert_eq!((rig.api.count("GET", "/api/server"), rig.api.count("GET", "/api/profiles")), (n, p));
    let home = rig.app.ids().tools[0];
    rig.click(home);
    assert!(rig.api.count("GET", "/api/server") > n && rig.api.count("GET", "/api/profiles") > p, "back on Home it asks again");
}

/// H13: the folders.
#[test]
fn h13_folders() {
    let mut rig = Rig::new(1400, 940, true);
    settings_of_exploration(&mut rig);
    let open = rig.home().ids().open_folder;
    rig.click(open);
    assert_eq!(rig.api.count("POST", "/api/folder/exploration"), 1);
    let back = rig.home().ids().back;
    rig.click(back);
    open_server(&mut rig);
    let open = rig.home().ids().s_open_folder;
    rig.click(open);
    assert_eq!(rig.api.count("POST", "/api/server/folder"), 1);
}

/// H14: everything asked went to the fake, and the helper builds no real api.
#[test]
fn h14_only_the_fake_is_asked() {
    let mut rig = Rig::new(1400, 940, true);
    settings_of_exploration(&mut rig);
    rig.wait(4.0);
    let asked = rig.api.asked();
    assert!(!asked.is_empty() && asked.iter().all(|(_, path, _)| path.starts_with("/api/")));
    let helper = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/common/mod.rs")).unwrap();
    assert!(!helper.contains(&["Api", "::new"].concat()), "the helper never builds the real api");
}

/// N1: a press on a profile's line opens its page, with the title line (its name and the back action); Back returns to
/// Home. A press on the Play button is the button's alone.
#[test]
fn n1_a_profile_line_opens_its_page_and_back_returns() {
    use outpost_tools::pages::Page;
    let mut rig = Rig::new(1400, 940, true);
    assert_eq!(rig.home().page(), None);
    let play = rig.home().ids().rows[0].play;
    rig.click(play);
    assert_eq!(rig.home().page(), None, "Play does not open the page");
    assert_eq!(rig.api.count("POST", "/api/launch/exploration"), 1);
    rig.set_clock(rig.clock + 20.0);
    open_profile(&mut rig, "atelier");
    assert_eq!(rig.home().page(), Some(&Page::Profile("atelier".into())));
    let said: String = rig.texts().iter().map(|t| t.1.as_str()).collect();
    assert!(said.contains("Atelier") && !said.contains("PROFILES"), "the page replaces Home");
    let back = rig.home().ids().back;
    let tip_at = centre(rig.rect(back));
    rig.hover_at(tip_at);
    rig.set_clock(rig.clock + 0.6);
    rig.frame();
    assert!(rig.says("Back"), "the back action has its tip");
    rig.click(back);
    assert_eq!(rig.home().page(), None);
    let again: String = rig.texts().iter().map(|t| t.1.as_str()).collect();
    assert!(again.contains("PROFILES"), "Home is back");
}

/// N2: the server's line opens the server's page, and the pages stack: the page that adds a mod sits over the profile's,
/// and Back leaves one at a time. The set-up has no title line.
#[test]
fn n2_pages_stack_and_back_pops_one() {
    use outpost_tools::pages::Page;
    let mut rig = Rig::new(1400, 940, true);
    open_server(&mut rig);
    assert_eq!(rig.home().page(), Some(&Page::Server));
    let said: String = rig.texts().iter().map(|t| t.1.as_str()).collect();
    assert!(said.contains("Dev server"));
    let back = rig.home().ids().back;
    rig.click(back);
    assert_eq!(rig.home().page(), None);
    open_profile(&mut rig, "exploration");
    // The page that adds a mod is pushed over it (by the profile screen's own action, once it has one).
    let mut home = outpost_tools::home::LauncherHome::new(outpost_tools::bridge::Bridge::inline(std::sync::Arc::new(outpost_tools::fake::fixture())), outpost_tools::home::Calendar::fixed());
    preview::Home::poll(&mut home, 0.0, true);
    home.open(Page::Profile("exploration".into()));
    home.open(Page::AddMod("exploration".into()));
    assert_eq!(home.page(), Some(&Page::AddMod("exploration".into())));
    home.back();
    assert_eq!(home.page(), Some(&Page::Profile("exploration".into())));
    home.back();
    assert_eq!(home.page(), None);
    home.open(Page::Setup);
    assert_eq!(home.page(), Some(&Page::Setup));
}

/// N3: every page draws, in the shell: the add-a-mod page's title says the profile, the set-up has no title line.
#[test]
fn n3_every_page_draws_in_the_shell() {
    use outpost_tools::pages::Page;
    for (page, title) in [(Page::AddMod("exploration".into()), Some("Add a mod to Exploration")), (Page::Setup, None), (Page::Server, Some("Dev server"))] {
        let api = std::sync::Arc::new(outpost_tools::fake::fixture());
        let mut home = loaded_home(api);
        home.open(page.clone());
        let state = preview::PreviewState { mode: Mode::Home, reduce_motion: true, ..preview::PreviewState::default() };
        let (list, _, _) = preview::picture_with_home(mere_ui::Theme::dark(), Vec2::new(1400.0, 940.0), None, state, &mut home);
        let said: String = list.cmds.iter().filter_map(|c| if let mere_base::Cmd::Text { text, .. } = c { Some(text.as_str()) } else { None }).collect();
        match title {
            Some(t) => assert!(said.contains(t), "{page:?} says {t}: {said}"),
            None => assert!(!said.contains("PROFILES") && home.ids().back == mere_ui::Id::ROOT, "{page:?} has no title line"),
        }
    }
}
