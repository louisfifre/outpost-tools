//! Home's SERVERS section (board A1) over the fake api: each server the api lists on a line of its own, its page opened
//! and its start, stop and EULA asked by its id, and the one dev server kept for an api that lists none.

mod common;

use common::*;
use launcher_native::fake::{board_servers, with_accounts};
use launcher_native::model::{ServerLine, parse_servers};
use launcher_native::pages::Page;
use launcher_native::text;
use mere_base::Vec2;
use serde_json::json;

/// The app on the board's profiles, accounts and two servers.
fn board_rig(w: u32, h: u32) -> Rig {
    let api = board_api();
    board_servers(&api);
    with_accounts(&api);
    Rig::with_api(api, w, h, true, true)
}

fn row_of(rig: &Rig, id: &str) -> launcher_native::home::RowIds {
    rig.home().ids().server_rows.iter().find(|r| r.profile == id).expect("the server is listed").clone()
}

#[test]
fn a1_each_server_the_api_lists_has_its_line() {
    let mut rig = board_rig(1400, 940);
    rig.hover_at(Vec2::new(5.0, 930.0));
    for words in ["Friday world", "running \u{b7} 2 players \u{b7} localhost:25565", "Test server", "stopped \u{b7} made from Exploration"] {
        assert!(rig.says(words), "Home says {words:?}");
    }
    assert!(!rig.says("Dev server"), "the one dev server gives way to the list");
    let ids: Vec<String> = rig.home().ids().server_rows.iter().map(|r| r.profile.clone()).collect();
    assert_eq!(ids, ["Friday world", "Test server"]);
    // One above the other, the running one first as the api listed it.
    let (a, b) = (rig.rect(row_of(&rig, "Friday world").row), rig.rect(row_of(&rig, "Test server").row));
    assert!(b.y > a.y + a.h - 1.0 && (a.x - b.x).abs() < 0.01);
    // A running server shows Stop and a stopped one Start.
    assert!(rig.rect(row_of(&rig, "Friday world").play).w > 0.0);
    rig.write("home_a1_servers_1400.png");
}

#[test]
fn a1_the_list_is_asked_with_the_server_and_asked_again() {
    let mut rig = board_rig(1400, 940);
    assert_eq!(rig.api.count("GET", "/api/servers"), 1);
    rig.wait(4.0);
    assert!(rig.api.count("GET", "/api/servers") >= 2, "the list follows the servers' states");
}

#[test]
fn a1_a_server_that_stops_or_starts_is_read_from_the_next_answer() {
    let mut rig = board_rig(1400, 940);
    let mut list = api_get(&rig.api, "/api/servers");
    list["servers"][0]["state"] = json!("stopped");
    list["servers"][0]["players"] = json!(null);
    list["servers"][1]["state"] = json!("starting");
    rig.api.set_get("/api/servers", list);
    rig.wait(4.0);
    rig.hover_at(Vec2::new(5.0, 930.0));
    assert!(rig.says("stopped \u{b7} made from Exploration") && rig.says("Starting the world\u{2026}"));
    assert!(!rig.says("running \u{b7} 2 players \u{b7} localhost:25565"));
}

#[test]
fn a1_a_press_on_a_line_opens_that_servers_page_and_asks_its_description() {
    let mut rig = board_rig(1400, 940);
    let row = row_of(&rig, "Test server").row;
    click_line(&mut rig, row);
    assert_eq!(rig.home().page(), Some(&Page::Server));
    assert!(rig.api.count("GET", "/api/server?id=Test%20server") >= 1, "{:?}", rig.api.asked());
    rig.frame();
    // The page is the board's C1: the EULA to accept, and what it was made from.
    for words in ["To start this server, accept the ", "Accept", "Tectonic islands \u{b7} seed 48213", "3 mods not needed on a server"] {
        assert!(rig.says(words), "the page says {words:?}");
    }
    // Accept and the folder name that server.
    let accept = rig.home().pages.server.accept;
    rig.click(accept);
    assert_eq!(rig.api.last_body("POST", "/api/server/eula"), Some(json!({"id": "Test server"})));
    let open = rig.home().pages.server.open_folder;
    rig.click(open);
    assert_eq!(rig.api.last_body("POST", "/api/server/folder"), Some(json!({"id": "Test server"})));
}

#[test]
fn a1_the_buttons_of_a_line_start_and_stop_that_server() {
    let mut rig = board_rig(1400, 940);
    let stop = row_of(&rig, "Friday world").play;
    rig.click(stop);
    assert_eq!(rig.api.last_body("POST", "/api/server/stop"), Some(json!({"id": "Friday world"})));
    assert_eq!(rig.api.count("POST", "/api/server/start"), 0);
    let mut rig = board_rig(1400, 940);
    let start = row_of(&rig, "Test server").play;
    rig.click(start);
    assert_eq!(rig.api.last_body("POST", "/api/server/start"), Some(json!({"id": "Test server"})));
    assert_eq!(rig.home().page(), None, "a button opens no page");
}

#[test]
fn a1_a_stop_is_said_on_the_line_that_was_stopped_only() {
    let mut rig = board_rig(1400, 940);
    let stop = row_of(&rig, "Friday world").play;
    rig.click(stop);
    rig.hover_at(Vec2::new(5.0, 930.0));
    assert!(rig.says("Saving and stopping\u{2026}"));
    assert!(rig.says("stopped \u{b7} made from Exploration"), "the other server's line is its own");
}

#[test]
fn a1_an_api_that_lists_no_servers_leaves_the_one_server() {
    let mut rig = Rig::new(1400, 940, true);
    assert_eq!(rig.api.count("GET", "/api/servers"), 1);
    rig.hover_at(Vec2::new(5.0, 930.0));
    assert!(rig.says("Dev server") && rig.says("stopped \u{b7} joins at localhost:25565"));
    // Its routes are the plain ones, with no id.
    let play = rig.home().ids().server_play;
    rig.click(play);
    assert_eq!(rig.api.last_body("POST", "/api/server/start"), Some(serde_json::Value::Null));
}

#[test]
fn a1_the_dev_server_listed_beside_the_others_keeps_the_plain_routes() {
    let api = board_api();
    board_servers(&api);
    let mut list = api_get(&api, "/api/servers");
    list["servers"].as_array_mut().unwrap().push(json!({"id": "dev", "name": "", "state": "stopped", "eula": true, "players": null, "port": 25565}));
    api.set_get("/api/servers", list);
    let mut rig = Rig::with_api(api, 1400, 940, true, true);
    rig.hover_at(Vec2::new(5.0, 930.0));
    assert!(rig.says("Dev server") && rig.says("stopped \u{b7} joins at localhost:25565"));
    let start = row_of(&rig, "dev").play;
    rig.click(start);
    assert_eq!(rig.api.last_body("POST", "/api/server/start"), Some(serde_json::Value::Null), "the dev server is not named");
    rig.write("home_a1_servers_dev_1400.png");
}

#[test]
fn the_servers_the_api_listed_are_read_and_a_missing_list_is_none() {
    let line = |id: &str, state: &str, players: serde_json::Value, made: serde_json::Value| json!({"id": id, "name": id, "state": state, "eula": true, "players": players, "port": 25570, "madeFrom": made});
    let v = json!({"servers": [line("a", "running", json!(1), json!({"profile": "p"})), json!({"id": "dev", "state": "stopped"})]});
    let parsed = parse_servers(&v).unwrap();
    assert_eq!(
        parsed[0],
        ServerLine { id: "a".into(), name: "a".into(), state: "running".into(), eula: true, players: Some(1), port: 25570, made_from: Some("p".into()) }
    );
    assert_eq!((parsed[1].port, parsed[1].made_from.clone(), parsed[1].request_id()), (25565, None, String::new()));
    assert_eq!(parse_servers(&json!({})), None);
    assert_eq!(parse_servers(&json!({"servers": []})), Some(vec![]));
}

#[test]
fn the_players_and_the_profile_are_said_where_the_board_says_them() {
    assert_eq!(text::players(1), "1 player");
    assert_eq!(text::players(0), "0 players");
    assert_eq!(text::server_line("running", 25565, Some(2), None).as_deref(), Some("running \u{b7} 2 players \u{b7} localhost:25565"));
    assert_eq!(text::server_line("running", 25565, None, Some("Exploration")).as_deref(), Some("running \u{b7} localhost:25565"));
    assert_eq!(text::server_line("stopped", 25565, None, Some("Exploration")).as_deref(), Some("stopped \u{b7} made from Exploration"));
    assert_eq!(text::server_line("stopped", 25565, None, None).as_deref(), Some("stopped \u{b7} joins at localhost:25565"));
    assert_eq!(text::server_line("starting", 25565, None, Some("Exploration")).as_deref(), Some("Starting the world\u{2026}"));
}

#[test]
fn a2_an_account_the_api_cannot_add_yet_says_why_on_home() {
    let api = board_api();
    board_servers(&api);
    with_accounts(&api);
    let sentence = "Adding an account needs the Microsoft sign-in, which is not built yet.";
    api.set_post("/api/accounts/add", 501, json!({"error": sentence}));
    let mut rig = Rig::with_api(api, 1400, 940, true, true);
    let button = rig.app.ids().account;
    rig.click(button);
    rig.frame();
    let at = rig.texts().iter().find(|(_, t, _)| t == "Add an account").map(|(p, _, _)| *p).expect("the row is drawn");
    rig.click_at(Vec2::new(at.x + 8.0, at.y + 8.0));
    rig.frame();
    // The note under the profiles wraps: its two lines are the sentence.
    assert!(rig.says("Adding an account needs the Microsoft sign-in, which is not") && rig.says("built yet."));
}
