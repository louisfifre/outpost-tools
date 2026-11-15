//! A real run of the world preview from the native launcher, as its window makes one: the runs `main` gives the
//! preview tool (`launcher_native::runs`), the engine in this process (world_preview::run), Generate pressed on the
//! form. Seed cap-1 over 65 536 blocks with a point every 64 blocks, the widest run the pack needs.
//!
//! Ignored by default: it starts a game server, takes minutes and writes into the preview folders of the launcher's
//! config. `OUTPOST_REPO` names the pack repository (its `pack/` and `scripts/common.ps1`); without it, "skipped".
//! `cargo test --release --test real_run -- --ignored --nocapture` prints the steps as they come.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use launcher::api::Api;
use launcher_native::bridge::Bridge;
use launcher_native::fake::fixture;
use launcher_native::home::{Calendar, LauncherHome};
use launcher_native::{ORIGIN, app};
use nui_base::{App, Button, Event, Vec2};
use nui_ui::{Slide, Theme};
use preview::Mode;

#[test]
#[ignore]
fn a_real_run_cap_1_over_65536_blocks_every_64() {
    let Some(repo) = std::env::var_os("OUTPOST_REPO").map(PathBuf::from) else {
        println!("skipped: OUTPOST_REPO names no pack repository");
        return;
    };
    let api = Arc::new(Api::new(repo.clone(), ORIGIN.into()));
    let runs = launcher_native::runs(&api, &repo);
    assert!(runs.engine_dir.is_some(), "the world-preview repository is found beside the pack or at WORLD_PREVIEW_DIR");
    let (w, h) = (1400u32, 940u32);
    let home = LauncherHome::new(Bridge::inline(Arc::new(fixture())), Calendar::fixed());
    let mut app = app(Theme::dark(), true, Vec2::new(w as f32, h as f32), None, Some(runs), home, true);
    {
        let s = app.state_mut();
        s.mode = Mode::Preview;
        s.options = Slide::settled(true);
        s.seed = "cap-1".to_string();
        // 65 536 blocks in 16-block points, at the detail of 64 blocks (the first of the detail tabs).
        s.samples = 4096;
        s.detail = 0;
    }
    let mut r = nui_win32::Renderer::offscreen(w, h, 1.0).expect("offscreen renderer");
    app.start(&mut r);
    app.event(Event::Resized { width_px: w, height_px: h, scale: 1.0 });
    app.frame(&mut r, 0.0);

    let g = app.ui().rect(app.ids().generate).expect("the Generate button is built");
    let at = Vec2::new(g.x + g.w / 2.0, g.y + g.h / 2.0);
    app.event(Event::PointerMoved { pos: at });
    app.event(Event::PointerDown { pos: at, button: Button::Left, clicks: 1 });
    app.frame(&mut r, 0.0);
    app.event(Event::PointerUp { pos: at, button: Button::Left });
    app.frame(&mut r, 0.0);
    assert!(app.running(), "Generate started the engine; trouble: {}", app.state().trouble);

    let started = Instant::now();
    let mut said = String::new();
    while app.running() {
        assert!(started.elapsed() < Duration::from_secs(3600), "the run took more than an hour");
        std::thread::sleep(Duration::from_millis(250));
        app.frame(&mut r, 0.0);
        if let Some(now) = app.state().follow.as_ref().map(|f| f.current()) {
            if now != said {
                println!("{:>4} s  {now}", started.elapsed().as_secs());
                said = now;
            }
        }
    }
    let state = app.state();
    assert!(state.trouble.is_empty(), "the run failed: {} (see the run log under run\\app\\preview)", state.trouble);
    println!("{}", state.said);
    let world = app.world().expect("the finished run became the map");
    assert_eq!((world.samples, world.step, world.span()), (1024, 64.0, 65536.0));
    assert_eq!(world.made.as_ref().map(|m| m.seed.as_str()), Some("cap-1"));
}
