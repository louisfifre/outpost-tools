//! The server's page before its first start (board C1) and the bar's account menu (board A2), over the fake api. Nothing
//! here writes a file: Accept is a request to the api, and the fake only remembers it was asked.

mod common;

use common::*;
use launcher_native::fake::{board_server, board_servers, with_accounts};
use launcher_native::pages::Page;
use mere_base::{Cmd, Key, Vec2};
use mere_ui::Id;
use serde_json::json;

/// The app on the board's profiles with the board's server, its page open.
fn server_page(w: u32, h: u32) -> Rig {
    let api = board_api();
    board_server(&api);
    with_accounts(&api);
    let mut rig = Rig::with_api(api, w, h, true, true);
    open_server(&mut rig);
    rig
}

#[test]
fn c1_the_page_asks_for_the_eula_before_anything() {
    let rig = server_page(1400, 940);
    assert_eq!(rig.home().page(), Some(&Page::Server));
    // The heading looks are drawn a letter at a time: the title and the section's head are read spelled out.
    let spelled: String = rig.texts().iter().filter(|t| t.1.chars().count() == 1).map(|t| t.1.as_str()).collect();
    assert!(spelled.contains("Test server") && spelled.contains("MADE FROM"), "{spelled}");
    for words in ["To start this server, accept the ", "Minecraft EULA", ". ", "It writes eula=true in its eula.txt.", "Accept", "Start"] {
        assert!(rig.says(words), "the page says {words:?}");
    }
    for words in ["Profile", "Exploration", "3 mods not needed on a server", "World", "Tectonic islands \u{b7} seed 48213", "Folder", "\u{2026}\\servers\\Test server", "Open"] {
        assert!(rig.says(words), "the made-from section says {words:?}");
    }
    // Its words are muted where the board mutes them.
    let t = rig.theme;
    assert_eq!(rig.colour_of("It writes eula=true in its eula.txt."), Some(t.muted));
    assert_eq!(rig.colour_of("To start this server, accept the "), Some(t.text));
    // The settings of the old placeholder are not there before the EULA.
    assert!(!rig.says("View distance (chunks)"));
    assert!(!rig.says("Distances, players, port and line appear after the first start."), "the board shows no note under the section");
}

#[test]
fn c1_start_waits_for_the_eula() {
    let mut rig = server_page(1400, 940);
    let start = rig.home().pages.server.start;
    // C1: the play icon before "Start", framed (disabled) until the EULA is accepted.
    let (r, word) = (rig.rect(start), rig.texts().into_iter().find(|t| t.1 == "Start").expect("the word is drawn").0);
    let icon = rig.app.list().cmds.iter().find_map(|c| if let Cmd::Shape { rect, .. } = c { Some(*rect).filter(|s| r.contains(Vec2::new(s.x + 1.0, s.y + 1.0))) } else { None }).expect("the play icon is in the action");
    assert!(icon.x + icon.w < word.x, "the icon stands before the word");
    rig.click(start);
    assert_eq!(rig.api.count("POST", "/api/server/start"), 0, "a disabled Start asks nothing");
    // The head's end: Start is at the right edge of the page's column, Accept after the sentence.
    let (s, a) = (rig.rect(start), rig.rect(rig.home().pages.server.accept));
    assert!(s.x > a.x + a.w, "Start is at the right, Accept after the sentence");
    assert!(s.y < a.y, "Start is on the title's line");
}

#[test]
fn c1_accept_asks_the_api_and_then_start_can_go() {
    let mut rig = server_page(1400, 940);
    let accept = rig.home().pages.server.accept;
    rig.click(accept);
    assert_eq!(rig.api.count("POST", "/api/server/eula"), 1);
    // The fake wrote nothing: the server is read again, as the api would now describe it.
    let mut v = api_get(&rig.api, "/api/server");
    assert_eq!(v["eula"], json!(false), "nothing in the test accepted it");
    v["eula"] = json!(true);
    rig.api.set_get("/api/server", v);
    rig.wait(4.0);
    assert!(!rig.says("Accept") && !rig.says("It writes eula=true in its eula.txt."), "the sentence is gone once it is accepted");
    let start = rig.home().pages.server.start;
    rig.click(start);
    assert_eq!(rig.api.count("POST", "/api/server/start"), 1);
}

#[test]
fn c1_the_link_goes_to_the_api() {
    let mut rig = server_page(1400, 940);
    let link = rig.home().pages.server.eula_link;
    rig.click(link);
    assert_eq!(rig.api.last_body("POST", "/api/open-link"), Some(json!({"url": "https://aka.ms/MinecraftEULA"})));
}

#[test]
fn c1_the_section_opens_the_profile_and_the_folder_and_folds() {
    let mut rig = server_page(1400, 940);
    let open = rig.home().pages.server.open_folder;
    rig.click(open);
    assert_eq!(rig.api.count("POST", "/api/server/folder"), 1);
    // The fold closes on a press on its head.
    let made = rig.home().pages.server.made;
    let head = rig.rect(made);
    rig.click_at(Vec2::new(head.x + 20.0, head.y + 14.0));
    assert!(!rig.says("Tectonic islands \u{b7} seed 48213"), "a closed section holds nothing");
    rig.click_at(Vec2::new(head.x + 20.0, head.y + 14.0));
    assert!(rig.says("Tectonic islands \u{b7} seed 48213"));
    let link = rig.home().pages.server.profile_link;
    rig.click(link);
    assert_eq!(rig.home().page(), Some(&Page::Profile("exploration".into())), "the profile link opens the profile's page");
}

#[test]
fn c1_one_mod_and_none_are_said_each_its_way() {
    assert_eq!(launcher_native::text::left_out(1).as_deref(), Some("1 mod not needed on a server"));
    assert_eq!(launcher_native::text::left_out(0), None);
}

#[test]
fn c1_picture() {
    let mut rig = server_page(1400, 940);
    rig.write("server_c1_1400.png");
    let mut narrow = server_page(700, 600);
    narrow.write("server_c1_700.png");
}

// ---- A2 ----

fn with_menu(w: u32, h: u32) -> Rig {
    let api = board_api();
    board_servers(&api);
    with_accounts(&api);
    Rig::with_api(api, w, h, true, true)
}

fn open_menu(rig: &mut Rig) {
    let button = rig.app.ids().account;
    rig.click(button);
}

/// A1: the bar at rest shows the account in use as its face, its name and a chevron, with the menu shut: the face is read
/// once with the first answer that names the account, and stands in the 16 px before the name.
#[test]
fn a1_the_bar_shows_the_face_before_the_name_at_rest() {
    let mut rig = with_menu(1400, 940);
    assert_eq!(rig.app.ids().account_list, Id::ROOT, "the menu is shut");
    assert_eq!(rig.api.count("GET", "/api/skin?name=PlayerOne"), 1, "the face was read without opening the menu");
    let name = rig.texts().into_iter().find(|t| t.1 == "PlayerOne").expect("the name is drawn").0;
    let button = rig.rect(rig.app.ids().account);
    let (w, h) = (rig.w as usize, rig.h as usize);
    let rgba = rig.r.read_rgba().unwrap();
    let at = |x: usize, y: usize| &rgba[(y * w + x) * 4..(y * w + x) * 4 + 3];
    // The sample skin's face is the skin tone 0xc69a73, filled over the 16 px square that ends GAP before the name.
    let (x0, x1) = ((name.x - 8.0 - 16.0) as usize, (name.x - 8.0) as usize);
    let faced = (0..h.min(40)).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| at(x, y) == [0xc6, 0x9a, 0x73]).count();
    assert!(faced >= 16 * 16 / 4, "the face stands before the name: {faced} pixels of its skin tone");
    assert!(x0 as f32 >= button.x && (x1 as f32) < name.x, "the face is inside the account's button, before the name");
    rig.write("account_a1_rest_1400.png");
}

#[test]
fn a2_the_menu_lists_the_accounts_the_player_and_the_sign_out() {
    let mut rig = with_menu(1400, 940);
    assert_ne!(rig.app.ids().account, Id::ROOT, "the account is a button once the accounts are known");
    assert_eq!(rig.api.count("GET", "/api/accounts"), 0, "the accounts are read when the menu opens");
    open_menu(&mut rig);
    assert_eq!(rig.api.count("GET", "/api/accounts"), 1);
    assert_eq!(rig.api.count("GET", "/api/skin?name=PlayerOne"), 1);
    assert_eq!(rig.api.count("GET", "/api/skin?name=SecondAccount"), 1);
    rig.frame();
    for words in ["PlayerOne", "SecondAccount", "Add an account", "Sign out of PlayerOne"] {
        assert!(rig.says(words), "the menu says {words:?}");
    }
    assert_ne!(rig.app.ids().account_list, Id::ROOT);
    rig.write("account_a2_1400.png");
}

#[test]
fn a2_a_pick_asks_the_api_and_shuts_the_menu() {
    for (row, path, body) in [
        ("SecondAccount", "/api/accounts/use", Some(json!({"name": "SecondAccount"}))),
        ("Add an account", "/api/accounts/add", None),
        ("Sign out of PlayerOne", "/api/accounts/sign-out", Some(json!({"name": "PlayerOne"}))),
    ] {
        let mut rig = with_menu(1400, 940);
        open_menu(&mut rig);
        rig.frame();
        let at = rig.texts().iter().find(|(_, t, _)| t == row).map(|(p, _, _)| *p).expect("the row is drawn");
        rig.click_at(Vec2::new(at.x + 8.0, at.y + 8.0));
        assert_eq!(rig.api.count("POST", path), 1, "{row}");
        assert_eq!(rig.api.last_body("POST", path), body.or(Some(serde_json::Value::Null)), "{row}");
        assert_eq!(rig.app.ids().account_list, Id::ROOT, "the menu shuts after {row}");
    }
}

#[test]
fn a2_the_account_in_use_is_marked_and_picking_it_changes_nothing() {
    let mut rig = with_menu(1400, 940);
    open_menu(&mut rig);
    rig.frame();
    let at = rig.texts().iter().find(|(_, t, _)| t == "PlayerOne").map(|(p, _, _)| *p).unwrap();
    // Two names say PlayerOne now: the bar's and the menu's, the second under the first.
    let all: Vec<Vec2> = rig.texts().iter().filter(|(_, t, _)| t == "PlayerOne").map(|(p, _, _)| *p).collect();
    assert_eq!(all.len(), 2);
    let menu_row = all.iter().copied().max_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
    assert!(menu_row.y > at.y - 1.0);
    rig.click_at(Vec2::new(menu_row.x + 8.0, menu_row.y + 8.0));
    assert_eq!(rig.api.count("POST", "/api/accounts/use"), 0);
}

#[test]
fn a2_escape_and_a_press_outside_shut_it() {
    let mut rig = with_menu(1400, 940);
    open_menu(&mut rig);
    assert_ne!(rig.app.ids().account_list, Id::ROOT);
    rig.key(Key::Escape);
    assert_eq!(rig.app.ids().account_list, Id::ROOT);
    open_menu(&mut rig);
    rig.click_at(Vec2::new(1000.0, 700.0));
    assert_eq!(rig.app.ids().account_list, Id::ROOT);
    // The press that shuts it opens nothing under it.
    assert_eq!(rig.home().page(), None);
}

#[test]
fn a2_no_account_is_a_muted_name_and_no_menu() {
    let api = board_api();
    let mut v = api_get(&api, "/api/profiles");
    v["account"] = serde_json::Value::Null;
    api.set_get("/api/profiles", v);
    let rig = Rig::with_api(api, 1400, 940, true, true);
    assert_eq!(rig.app.ids().account, Id::ROOT);
    assert!(rig.says("No account: sign in once in Prism"));
}
