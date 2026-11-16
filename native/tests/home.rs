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
    let p = Vec2::new(b.x + 30.0, b.y + b.h + 1.0 + row as f32 * mere_ui::MENU_ROW + 10.0);
    rig.click_at(p);
}

fn settings_of_exploration(rig: &mut Rig) {
    let id = rig.home().ids().rows.iter().find(|r| r.profile == "exploration").unwrap().settings;
    rig.click(id);
}

/// H1: the first frame asks for the profiles and the server; the profile played last is above.
#[test]
fn h1_the_first_frame_asks_and_orders() {
    let mut rig = Rig::with_api(std::sync::Arc::new(launcher_native::fake::fixture()), 1400, 940, true, false);
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
    assert!(said.contains("EXPLORATION"), "the h2");
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
    assert_eq!(body, json!({"javaArgs": "-XX:+UseZGC", "memoryMb": 6144, "open": {"kind": "server", "target": "localhost"}}));
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
    let api = std::sync::Arc::new(launcher_native::fake::fixture());
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

/// H7: the memory is held within 2 and three quarters of the machine, and rounded.
#[test]
fn h7_memory() {
    let mut rig = Rig::new(1400, 940, true);
    settings_of_exploration(&mut rig);
    let id = rig.home().ids().memory;
    rig.type_into(id, "99");
    rig.key(Key::Enter);
    assert_eq!(rig.api.last_body("POST", "/api/settings/exploration").unwrap()["memoryMb"], json!(24 * 1024));
    let id = rig.home().ids().memory;
    rig.type_into(id, "3.6");
    rig.key(Key::Enter);
    assert_eq!(rig.api.last_body("POST", "/api/settings/exploration").unwrap()["memoryMb"], json!(4096));
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
    assert!(rig.says("Running \u{b7} localhost:25565"));
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

/// H10: the server's fields: the icon opens them reversed; without props, or while it runs, they are locked.
#[test]
fn h10_server_fields_and_locks() {
    let mut rig = Rig::new(1400, 940, true);
    let gear = rig.home().ids().server_settings;
    assert_eq!(rig.home().ids().s_memory, mere_ui::Id::ROOT, "folded at first");
    rig.click(gear);
    assert_ne!(rig.home().ids().s_memory, mere_ui::Id::ROOT);
    let r = rig.rect(rig.home().ids().server_settings);
    assert!(rig.app.list().cmds.iter().any(|c| matches!(c, mere_base::Cmd::Fill { rect, color } if *color == rig.theme.fill && (rect.x - r.x).abs() < 0.01 && (rect.w - mere_ui::tokens::ICON_SQUARE).abs() < 0.01)), "drawn in reverse");
    // No props: the first start has not happened.
    rig.api.set_get("/api/server", json!({"dir": "C:\\Games\\server", "state": "stopped", "eula": true, "memoryMb": 6144, "javaArgs": "", "props": null}));
    rig.wait(3.0);
    assert_eq!(rig.home().notes().1, "Distances, players, port and line appear after the first start.");
    let view = rig.home().ids().s_view;
    rig.click(view);
    assert_eq!(rig.app.ui().focused(), None);
    // Starting: the memory is locked too.
    rig.api.set_get("/api/server", json!({"dir": "C:\\Games\\server", "state": "starting", "eula": true, "memoryMb": 6144, "javaArgs": "", "props": null}));
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
    let gear = rig.home().ids().server_settings;
    rig.click(gear);
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
    let gear = rig.home().ids().server_settings;
    rig.click(gear);
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
