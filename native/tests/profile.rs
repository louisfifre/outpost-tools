//! The profile page (boards B1 to B3): drawn from the fake api, its pictures to `target/tmp`, and what each part asks.

mod common;

use std::sync::Arc;

use common::*;
use launcher_native::fake::{FakeApi, fixture};
use launcher_native::pages::Page;
use mere_base::{App, Button, Cmd, Event, Vec2};
use serde_json::{Value, json};

/// The board's PC: 16 GB with a third used, and Exploration at 2 to 6 GB.
fn board_pc() -> Arc<FakeApi> {
    let api = Arc::new(fixture());
    let mut v = api_get(&api, "/api/profiles");
    v["machineMb"] = json!(16384);
    v["usedMb"] = json!(5222);
    v["profiles"][1]["minMemoryMb"] = json!(2048);
    api.set_get("/api/profiles", v);
    // The bar shows the account's face before its name (the boards).
    launcher_native::fake::with_accounts(&api);
    api
}

fn page(w: u32, h: u32) -> Rig {
    let mut rig = Rig::with_api(board_pc(), w, h, true, true);
    open_profile(&mut rig, "exploration");
    // The page's own request is answered at once by the inline bridge; one frame draws it.
    rig.frame();
    rig
}

fn ids(rig: &Rig) -> launcher_native::pages::profile::Ids {
    rig.home().pages.profile.ids.clone()
}

/// Presses the bar at `from` gigabytes, carries to `to`, releases.
fn drag(rig: &mut Rig, from: f32, to: f32) {
    let (from, to) = (at_gb(rig, from), at_gb(rig, to));
    rig.app.event(Event::PointerMoved { pos: from });
    rig.app.event(Event::PointerDown { pos: from, button: Button::Left, clicks: 1 });
    rig.frame();
    rig.app.event(Event::PointerMoved { pos: to });
    rig.frame();
    rig.app.event(Event::PointerUp { pos: to, button: Button::Left });
    rig.frame();
}

/// Where an amount stands on the memory bar, in the window's pixels (the bar is as long as the PC's 16 GB).
fn at_gb(rig: &Rig, gb: f32) -> Vec2 {
    let r = rig.rect(ids(rig).memory);
    Vec2::new(r.x + gb / 16.0 * r.w, r.y + 10.0)
}

fn saved(rig: &Rig) -> Value {
    rig.api.last_body("POST", "/api/settings/exploration").expect("saved")
}

fn said(rig: &Rig) -> Vec<String> {
    rig.texts().iter().map(|t| t.1.clone()).collect()
}

/// The words are drawn: whole, or letter by letter as a heading's letter spacing draws them.
fn has(rig: &Rig, words: &str) -> bool {
    rig.says(words) || said(rig).concat().contains(words)
}

/// B1: the page at 1400: Play at the right of the title, the game's settings and the mods side by side.
#[test]
fn b1_the_profile_page() {
    let mut rig = page(1400, 940);
    rig.hover_at(Vec2::new(5.0, 930.0));
    rig.write("profile_b1_1400.png");
    for words in [
        "Exploration", "GAME", "Opens on", "Game version", "Fabric", "0.18.2", "\u{2192} 0.18.4", "Memory", "min 2 GB", "max 6 GB", "used by the PC", "16 GB", "Java arguments",
        "Game folder", "Open", "MODS \u{b7} 14", "Update all (3)", "Add a mod", "Sodium", "0.7.1", "\u{2192} 0.7.3", "my-local-mod.jar",
    ] {
        assert!(has(&rig, words), "the page says {words}: {:?}", said(&rig));
    }
    let i = ids(&rig);
    let (game, mods, play) = (rig.rect(i.game), rig.rect(i.mods), rig.rect(i.play));
    assert!(mods.x > game.x + game.w - 1.0 && (mods.y - game.y).abs() < 8.0, "two columns, side by side");
    assert!(play.x + play.w > mods.x + mods.w - 1.0 && play.y < mods.y, "Play at the right of the title line");
    assert_eq!(i.lines.len(), 14);
    // The slots keep their columns: every trash on one line.
    let trash: Vec<f32> = i.lines.iter().map(|l| rig.rect(l.remove).x).collect();
    assert!(trash.iter().all(|x| (x - trash[0]).abs() < 0.01));
}

/// B2: a small window stacks the parts, and the game's settings start folded; a press opens them.
#[test]
fn b2_the_small_window() {
    let mut rig = page(800, 600);
    let i = ids(&rig);
    assert!(!rig.says("Opens on") && has(&rig, "GAME") && has(&rig, "MODS \u{b7} 14"));
    let (game, mods) = (rig.rect(i.game), rig.rect(i.mods));
    assert!(mods.y >= game.y + game.h && (mods.x - game.x).abs() < 1.0, "one under the other");
    rig.hover_at(Vec2::new(5.0, 590.0));
    rig.write("profile_b2_800.png");
    rig.click_at(Vec2::new(game.x + 20.0, game.y + 12.0));
    assert!(rig.says("Opens on"), "the fold opens");
    rig.write("profile_b2_open_800.png");
}

/// B3: a removed mod is said at the top of the list with its Undo; the Undo puts it back.
#[test]
fn b3_remove_and_undo() {
    let mut rig = page(1400, 940);
    let sodium = ids(&rig).lines[0].clone();
    assert_eq!(sodium.mod_id, "sodium");
    // The api's list after the removal.
    let mut after = launcher_native::fake::exploration_mods();
    after["mods"].as_array_mut().unwrap().remove(0);
    rig.api.set_get("/api/mods/exploration", after);
    rig.click(sodium.remove);
    assert_eq!(rig.api.last_body("POST", "/api/mods/exploration/remove"), Some(json!({"mod": "sodium"})));
    rig.hover_at(Vec2::new(5.0, 930.0));
    assert!(rig.says("Sodium removed.") && rig.says("Undo") && has(&rig, "MODS \u{b7} 13") && rig.says("Update all (2)"), "{:?}", said(&rig));
    assert!(!ids(&rig).lines.iter().any(|l| l.mod_id == "sodium"));
    rig.write("profile_b3_1400.png");
    rig.api.set_get("/api/mods/exploration", launcher_native::fake::exploration_mods());
    rig.click(ids(&rig).undo);
    assert_eq!(rig.api.last_body("POST", "/api/mods/exploration/restore"), Some(json!({"mod": "sodium"})));
    assert!(!rig.says("Sodium removed.") && has(&rig, "MODS \u{b7} 14"), "the line closes when the mod is back");
}

/// The updates: one mod's, all of them, Fabric's; a mod's name opens its page; Add a mod pushes its page.
#[test]
fn b4_updates_links_and_add() {
    let mut rig = page(1400, 940);
    let i = ids(&rig);
    rig.click(i.lines[1].action);
    assert_eq!(rig.api.last_body("POST", "/api/mods/exploration/update"), Some(json!({"mod": "lithium"})));
    let i = ids(&rig);
    rig.click(i.update_all);
    assert_eq!(rig.api.count("POST", "/api/mods/exploration/update-all"), 1);
    rig.click(i.fabric_update);
    assert_eq!(rig.api.count("POST", "/api/mods/exploration/fabric"), 1);
    rig.click(i.lines[3].name);
    assert_eq!(rig.api.last_body("POST", "/api/open-url"), Some(json!({"url": "https://modrinth.com/mod/tectonic"})));
    let hand_made = i.lines.last().unwrap();
    rig.click(hand_made.name);
    assert_eq!(rig.api.count("POST", "/api/open-url"), 1, "a file dropped in by hand has no page");
    rig.click(i.add_mod);
    assert_eq!(rig.home().page(), Some(&Page::AddMod("exploration".into())));
}

/// Update all is for several: one newer version alone has its own Update.
#[test]
fn b5_update_all_waits_for_several() {
    let api = board_pc();
    let mut v = launcher_native::fake::exploration_mods();
    for m in v["mods"].as_array_mut().unwrap().iter_mut().skip(1) {
        m["latest"] = Value::Null;
    }
    api.set_get("/api/mods/exploration", v);
    let mut rig = Rig::with_api(api, 1400, 940, true, true);
    open_profile(&mut rig, "exploration");
    rig.frame();
    assert!(has(&rig, "MODS \u{b7} 14") && !rig.says("Update all (1)") && ids(&rig).update_all == mere_ui::Id::ROOT);
}

/// Play sits on the title line and starts the profile.
#[test]
fn b6_play() {
    let mut rig = page(1400, 940);
    let play = ids(&rig).play;
    // B1: the icon, then the word, inside the one filled box.
    let (r, word) = (rig.rect(play), rig.texts().into_iter().find(|t| t.1 == "Play").expect("the word is drawn").0);
    let icon = rig.app.list().cmds.iter().find_map(|c| if let Cmd::Shape { rect, .. } = c { Some(*rect).filter(|s| r.contains(Vec2::new(s.x + 1.0, s.y + 1.0))) } else { None }).expect("the play icon is in the action");
    assert!(icon.x + icon.w < word.x && word.x + 20.0 < r.x + r.w, "the icon stands before the word, both inside the box");
    assert!((r.h - 36.0).abs() < 0.01 && (r.w - 79.0).abs() < 3.0, "the board's Play is 36 tall and about 79 wide: {r:?}");
    rig.click(play);
    assert_eq!(rig.api.count("POST", "/api/launch/exploration"), 1);
}

/// On a PC with 32 GB the grips of 2 and 6 GB are close: their words stand side by side, never over each other.
#[test]
fn m2_the_words_of_a_big_pc_keep_clear() {
    let api = board_pc();
    let mut v = api_get(&api, "/api/profiles");
    v["machineMb"] = json!(32768);
    api.set_get("/api/profiles", v);
    let mut rig = Rig::with_api(api, 1400, 940, true, true);
    open_profile(&mut rig, "exploration");
    rig.frame();
    let at = |words: &str| rig.texts().into_iter().find(|t| t.1 == words).unwrap_or_else(|| panic!("{words:?}: {:?}", said(&rig))).0;
    let (min, max) = (at("min 2 GB"), at("max 6 GB"));
    assert!(min.x + mere_ui::text_width("min 2 GB", 11.0) < max.x, "the words stand clear: {min:?} {max:?}");
    rig.write("profile_m2_32gb_1400.png");
}

/// The memory bar: either end is carried in whole gigabytes and saved when let go; the maximum stops where the PC's
/// own use begins, the minimum never passes the maximum.
#[test]
fn m1_the_memory_bar() {
    let mut rig = page(1400, 940);
    drag(&mut rig, 6.0, 8.2);
    assert_eq!(saved(&rig)["memoryMb"], json!(8 * 1024));
    assert_eq!(saved(&rig)["minMemoryMb"], json!(2 * 1024));
    assert!(rig.says("max 8 GB") && rig.says("min 2 GB"));
    // Past the grey part: held at the 10 whole gigabytes the PC leaves free.
    drag(&mut rig, 8.0, 15.0);
    assert_eq!(saved(&rig)["memoryMb"], json!(10 * 1024));
    // The minimum is carried too, and held under the maximum.
    drag(&mut rig, 2.0, 4.0);
    assert_eq!(saved(&rig)["minMemoryMb"], json!(4 * 1024));
    drag(&mut rig, 4.0, 14.0);
    assert_eq!(saved(&rig)["minMemoryMb"], json!(10 * 1024));
    assert!(rig.says("min 10 GB") && rig.says("max 10 GB"));
    rig.write("profile_memory_1400.png");
    // A press that moves nothing sends nothing.
    let asked = rig.api.count("POST", "/api/settings/exploration");
    drag(&mut rig, 10.0, 10.0);
    assert_eq!(rig.api.count("POST", "/api/settings/exploration"), asked);
}

/// Minecraft running: the bar, the mods' changes and Update all take nothing.
#[test]
fn l1_locked() {
    let api = board_pc();
    let mut v = api_get(&api, "/api/profiles");
    v["locked"] = json!(true);
    api.set_get("/api/profiles", v);
    let mut rig = Rig::with_api(api, 1400, 940, true, true);
    open_profile(&mut rig, "exploration");
    rig.frame();
    assert!(rig.says("Minecraft is running: settings can change once it is closed."));
    drag(&mut rig, 6.0, 8.0);
    let i = ids(&rig);
    rig.click(i.lines[0].remove);
    rig.click(i.lines[0].action);
    rig.click(i.update_all);
    rig.click(i.fabric_update);
    assert_eq!(rig.api.count("POST", "/api/settings/exploration"), 0);
    assert!(rig.api.asked().iter().all(|(m, p, _)| m != "POST" || !p.starts_with("/api/mods/")));
    rig.write("profile_locked_1400.png");
}

/// A profile with no Fabric says so and offers to install it.
#[test]
fn f1_no_fabric() {
    let mut rig = Rig::with_api(board_pc(), 1400, 940, true, true);
    open_profile(&mut rig, "atelier");
    rig.frame();
    assert!(rig.says("not installed") && rig.says("Install") && has(&rig, "MODS \u{b7} 0"));
}
