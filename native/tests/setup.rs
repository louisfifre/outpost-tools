//! The first start's Java step (boards E1 and E2) over the fake api: nothing is downloaded, nothing is started.

mod common;

use std::sync::Arc;

use common::*;
use launcher_native::fake::{FakeApi, fixture, java_downloading, java_missing};
use launcher_native::pages::Page;
use mere_base::{App, Event, Vec2};
use mere_ui::Theme;
use mere_win32::Renderer;
use preview::world::World;
use preview::Home;

const MB: u64 = 1024 * 1024;

/// The whole app (the top bar is the board's) with the set-up shown over Home, at `w` x `h`.
fn setup_rig(api: Arc<FakeApi>, w: u32, h: u32) -> Rig {
    let theme = Theme::dark();
    let mut home = home_over(api.clone());
    home.poll(0.0, true);
    home.open(Page::Setup);
    let mut r = Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
    let mut app = launcher_native::app(theme, true, Vec2::new(w as f32, h as f32), None::<World>, None, home, true);
    app.start(&mut r);
    app.event(Event::Resized { width_px: w, height_px: h, scale: 1.0 });
    app.set_clock(Some(1.0));
    let mut rig = Rig { app, r, api, theme, w, h, clock: 1.0 };
    rig.frame();
    rig.wait(1.0);
    rig
}

fn api_with_java(size_mb: u64) -> Arc<FakeApi> {
    let api = Arc::new(fixture());
    java_missing(&api, size_mb * MB);
    api
}

/// E1: Java is missing: the sentence, the download with its size, where it comes from, and Later.
#[test]
fn e1_java_missing() {
    let mut rig = setup_rig(api_with_java(51), 1400, 940);
    let all: String = rig.texts().iter().map(|t| t.1.as_str()).collect();
    assert!(all.contains("1 JAVA2 PRISM3 SIGN IN"), "the three steps: {all}");
    for words in ["Outpost Tools needs Java 25 to run the game.", "Download Java 25 \u{b7} 51 MB", "from Adoptium, into Outpost Tools' folder", "Later"] {
        assert!(rig.says(words), "E1 says {words:?}: {:?}", rig.texts().iter().map(|t| t.1.as_str()).collect::<Vec<_>>());
    }
    rig.hover_at(Vec2::new(5.0, 930.0));
    rig.write("setup_e1_1400.png");
}

/// E1: the Download button asks the api to download, and the page shows the bar at once (E2).
#[test]
fn e1_download_asks_the_api() {
    let mut rig = setup_rig(api_with_java(51), 1400, 940);
    let at = rig.texts().into_iter().find(|t| t.1.starts_with("Download Java 25")).unwrap().0;
    // From the POST on, the api answers that it runs.
    java_downloading(&rig.api, 0, 51 * MB);
    rig.click_at(Vec2::new(at.x + 4.0, at.y + 4.0));
    assert_eq!(rig.api.count("POST", "/api/java/download"), 1);
    assert!(rig.says("Downloading Java 25."));
    // The bar follows the api.
    java_downloading(&rig.api, 20 * MB, 51 * MB);
    rig.wait(1.0);
    assert!(rig.says("20 of 51 MB"), "{:?}", rig.texts().iter().map(|t| t.1.as_str()).collect::<Vec<_>>());
}

/// E1: Later leaves the set-up and downloads nothing.
#[test]
fn e1_later_leaves() {
    let mut rig = setup_rig(api_with_java(51), 1400, 940);
    let at = rig.texts().into_iter().find(|t| t.1 == "Later").unwrap().0;
    rig.click_at(Vec2::new(at.x + 4.0, at.y + 4.0));
    assert_eq!(rig.home().page(), None);
    assert_eq!(rig.api.count("POST", "/api/java/download"), 0);
}

/// E2: while it downloads: the sentence, the bar with its figures, and Stop alone, which asks the api to stop.
#[test]
fn e2_java_downloading() {
    let api = api_with_java(195);
    java_downloading(&api, 82 * MB, 195 * MB);
    let mut rig = setup_rig(api, 1400, 940);
    for words in ["Downloading Java 25.", "82 of 195 MB", "Stop"] {
        assert!(rig.says(words), "E2 says {words:?}: {:?}", rig.texts().iter().map(|t| t.1.as_str()).collect::<Vec<_>>());
    }
    assert!(!rig.says("Later") && !rig.says("Nothing is kept."), "Stop is alone, its note is its tip");
    rig.hover_at(Vec2::new(5.0, 930.0));
    rig.write("setup_e2_1400.png");
    let at = rig.texts().into_iter().find(|t| t.1 == "Stop").unwrap().0;
    rig.click_at(Vec2::new(at.x + 4.0, at.y + 4.0));
    assert_eq!(rig.api.count("POST", "/api/java/stop"), 1);
}

/// A Java already on the PC is said, and the page asks no more.
#[test]
fn a_found_java_is_said() {
    let api = Arc::new(fixture());
    api.set_get("/api/java", serde_json::json!({"state": "found", "version": "25.0.5"}));
    let mut rig = setup_rig(api, 1400, 940);
    assert!(rig.says("Java 25.0.5 is ready."));
    let asked = rig.api.count("GET", "/api/java");
    rig.wait(20.0);
    assert_eq!(rig.api.count("GET", "/api/java"), asked);
}

/// Adoptium out of reach: the reason is said, and the download stays on offer without a size.
#[test]
fn an_unreachable_adoptium_is_said() {
    let api = Arc::new(fixture());
    api.set_get("/api/java", serde_json::json!({"state": "missing", "error": "Adoptium could not be reached: offline"}));
    let rig = setup_rig(api, 1400, 940);
    assert!(rig.says("Adoptium could not be reached: offline") && rig.says("Download Java 25"));
}
