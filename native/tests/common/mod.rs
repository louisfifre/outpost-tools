//! What every test of the native launcher shares: Home over the preview's app, answered by a fake through an
//! inline bridge. Nothing here builds the real api: a test never starts Prism, Java, a server or a script.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use launcher_native::bridge::{Backend, Bridge};
use launcher_native::fake::{FakeApi, fixture};
use launcher_native::home::{Calendar, LauncherHome};
use mere_base::{App, Button, Cmd, Color, Event, Key, Mods, Rect, Vec2};
use mere_ui::{Id, Theme};
use mere_win32::Renderer;
use preview::PreviewApp;
use preview::world::World;
use serde_json::{Value, json};

pub fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
}

/// The profile of the fixture that has a server address to join, for the tests of the locked fields.
pub fn with_server_profile(api: &FakeApi) {
    let mut v = serde_json::from_str::<Value>(&serde_json::to_string(&api_get(api, "/api/profiles")).unwrap()).unwrap();
    v["profiles"][1]["open"] = json!({"kind": "server", "target": "localhost"});
    api.set_get("/api/profiles", v);
}

pub fn api_get(api: &FakeApi, path: &str) -> Value {
    // The fake answers a GET like any backend: ask it.
    use launcher_native::bridge::Backend;
    serde_json::from_slice(&api.ask("GET", path, b"").body).unwrap()
}

pub fn home_over(api: Arc<FakeApi>) -> LauncherHome {
    LauncherHome::new(Bridge::inline(api), Calendar::fixed())
}

/// A Home that has taken its first answers, for the pictures that are not of the full app.
pub fn loaded_home(api: Arc<FakeApi>) -> LauncherHome {
    use preview::Home;
    let mut h = home_over(api);
    h.poll(0.0, true);
    h
}

pub struct Rig {
    pub app: PreviewApp,
    pub r: Renderer,
    pub api: Arc<FakeApi>,
    pub theme: Theme,
    pub w: u32,
    pub h: u32,
    pub clock: f64,
}

impl Rig {
    /// The app at `w` x `h` (raised to the window's least), the clock at 1.0, its first two frames built.
    pub fn new(w: u32, h: u32, dark: bool) -> Rig {
        Rig::with_api(Arc::new(fixture()), w, h, dark, true)
    }

    pub fn with_api(api: Arc<FakeApi>, w: u32, h: u32, dark: bool, frames: bool) -> Rig {
        Rig::with_backend(api.clone(), api, w, h, dark, frames)
    }

    /// The same over any backend, the real api for one: `api` is then only what the rig's fake-reading helpers see.
    pub fn with_backend(backend: Arc<dyn Backend>, api: Arc<FakeApi>, w: u32, h: u32, dark: bool, frames: bool) -> Rig {
        let theme = if dark { Theme::dark() } else { Theme::light() };
        let (pw, ph) = preview::min_size_px((w, h));
        let mut r = Renderer::offscreen(pw, ph, 1.0).expect("offscreen renderer");
        let mut app = launcher_native::app(theme, dark, Vec2::new(pw as f32, ph as f32), None::<World>, None, LauncherHome::new(Bridge::inline(backend), Calendar::fixed()), true);
        app.start(&mut r);
        app.event(Event::Resized { width_px: pw, height_px: ph, scale: 1.0 });
        app.event(Event::Window { maximized: false });
        app.set_clock(Some(1.0));
        let mut rig = Rig { app, r, api, theme, w: pw, h: ph, clock: 1.0 };
        if frames {
            rig.frame();
        }
        rig
    }

    pub fn set_clock(&mut self, seconds: f64) {
        self.clock = seconds;
        self.app.set_clock(Some(seconds));
    }

    pub fn frame(&mut self) {
        self.app.frame(&mut self.r, 0.0);
        self.app.frame(&mut self.r, 0.0);
    }

    /// Moves the clock on by `seconds`, one frame at each second of it, so that every poll that falls due is taken.
    pub fn wait(&mut self, seconds: f64) {
        let to = self.clock + seconds;
        while self.clock < to {
            self.set_clock((self.clock + 1.0).min(to));
            self.frame();
        }
    }

    pub fn home(&self) -> &LauncherHome {
        self.app.home().expect("a home").as_any().downcast_ref::<LauncherHome>().expect("the launcher's home")
    }

    pub fn rect(&self, id: Id) -> Rect {
        assert_ne!(id, Id::ROOT, "the widget is not built");
        self.app.ui().rect(id).expect("the widget was built")
    }

    pub fn click_at(&mut self, p: Vec2) {
        self.click_n(p, 1);
    }

    pub fn click_n(&mut self, p: Vec2, clicks: u8) {
        self.app.event(Event::PointerMoved { pos: p });
        self.app.event(Event::PointerDown { pos: p, button: Button::Left, clicks });
        self.frame();
        self.app.event(Event::PointerUp { pos: p, button: Button::Left });
        self.frame();
    }

    pub fn click(&mut self, id: Id) {
        let r = self.rect(id);
        self.click_at(centre(r));
    }

    pub fn hover_at(&mut self, p: Vec2) {
        self.app.event(Event::PointerMoved { pos: p });
        self.frame();
    }

    /// Selects everything in the field under the pointer (a triple click), then types over it.
    pub fn type_into(&mut self, id: Id, words: &str) {
        let r = self.rect(id);
        self.click_n(centre(r), 3);
        for ch in words.chars() {
            self.app.event(Event::Char { ch });
        }
        self.frame();
    }

    pub fn key(&mut self, key: Key) {
        self.app.event(Event::Key { key, mods: Mods::default() });
        self.frame();
    }

    pub fn texts(&self) -> Vec<(Vec2, String, Color)> {
        self.app.list().cmds.iter().filter_map(|c| if let Cmd::Text { at, text, color, .. } = c { Some((*at, text.clone(), *color)) } else { None }).collect()
    }

    pub fn says(&self, words: &str) -> bool {
        self.texts().iter().any(|(_, t, _)| t == words)
    }

    pub fn colour_of(&self, words: &str) -> Option<Color> {
        self.texts().iter().find(|(_, t, _)| t == words).map(|(_, _, c)| *c)
    }

    pub fn write(&mut self, name: &str) {
        let rgba = self.r.read_rgba().unwrap();
        let path = out_dir().join(name);
        std::fs::write(&path, mere_png::encode(self.w, self.h, &rgba)).unwrap();
        println!("wrote {}", path.display());
    }
}

/// A press on a line of Home, on its name's side: away from its button, which has a press of its own.
pub fn click_line(rig: &mut Rig, line: Id) {
    let r = rig.rect(line);
    rig.click_at(Vec2::new(r.x + 24.0, r.y + r.h / 2.0));
}

/// Opens the page of the profile `id` by a press on its line.
pub fn open_profile(rig: &mut Rig, id: &str) {
    let row = rig.home().ids().rows.iter().find(|r| r.profile == id).expect("the profile is listed").row;
    click_line(rig, row);
}

/// Opens the server's page by a press on its line.
pub fn open_server(rig: &mut Rig) {
    let row = rig.home().ids().server_row;
    click_line(rig, row);
}

pub fn centre(r: Rect) -> Vec2 {
    Vec2::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

pub fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 0.01, "{what}: {a} != {b}");
}

pub fn rect_is(r: Rect, x: f32, y: f32, w: f32, h: f32, what: &str) {
    assert!((r.x - x).abs() < 0.01 && (r.y - y).abs() < 0.01 && (r.w - w).abs() < 0.01 && (r.h - h).abs() < 0.01, "{what}: {r:?} != ({x}, {y}, {w}, {h})");
}

/// The fixture with the board's other two profiles (A1): one a fortnight ago with no mods, and one with a name too long
/// for its line, played last month.
pub fn board_api() -> Arc<FakeApi> {
    let api = Arc::new(fixture());
    let mut v = api_get(&api, "/api/profiles");
    let day = 86_400_000i64;
    for (id, name, days) in [("plain", "Plain 26.3", 14), ("friday", "Friday server pack from L\u{e9}a, with shaders and a minimap", 31)] {
        v["profiles"].as_array_mut().unwrap().push(json!({
            "id": id, "name": name, "memoryMb": 4096, "javaArgs": "", "gameDir": format!("C:/Code/{id}"),
            "lastPlayed": launcher_native::fake::FIXED_NOW_MS - days * day, "playedSeconds": 0, "running": false, "worlds": [],
            "open": {"kind": "menu", "target": ""}
        }));
    }
    api.set_get("/api/profiles", v);
    api
}
