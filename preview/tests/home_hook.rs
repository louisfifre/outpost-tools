//! The hook a host fills to give Home: what the app calls, and when.

use std::any::Any;

use mere_base::{App, Cmd, Event, Vec2, Button};
use mere_layout::{Dim, Style};
use mere_ui::{Theme, Ui};
use mere_win32::Renderer;
use preview::{BAR_H, Home, Mode, PreviewApp, PreviewState};

#[derive(Default)]
struct Counting {
    builds: usize,
    acts: usize,
    shown: usize,
    polls: usize,
    off_screen_polls: usize,
    size: Option<Vec2>,
}

impl Home for Counting {
    fn build(&mut self, ui: &mut Ui, size: Vec2) {
        self.builds += 1;
        self.size = Some(size);
        ui.node("box", Style { w: Dim::Px(50.0), h: Dim::Px(20.0), ..Style::default() }, |_| {});
    }
    fn act(&mut self) -> bool {
        self.acts += 1;
        // The first build asks to be built again, once.
        self.acts == 1
    }
    fn poll(&mut self, _: f64, on_screen: bool) -> bool {
        self.polls += 1;
        if !on_screen {
            self.off_screen_polls += 1;
        }
        false
    }
    fn wake_in(&self, _: f64) -> Option<f32> {
        Some(0.05)
    }
    fn shown(&mut self) {
        self.shown += 1;
    }
    fn account(&self) -> Option<String> {
        Some("Zed".into())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn counts(app: &PreviewApp) -> &Counting {
    app.home().expect("a home").as_any().downcast_ref::<Counting>().expect("the test's home")
}

fn rig(mode: Mode) -> (PreviewApp, Renderer) {
    let state = PreviewState { mode, reduce_motion: true, ..PreviewState::default() };
    let mut r = Renderer::offscreen(1400, 940, 1.0).unwrap();
    let mut app = PreviewApp::new(Theme::dark(), true, Vec2::new(1400.0, 940.0), state, None).with_home(Box::new(Counting::default()));
    app.start(&mut r);
    app.event(Event::Resized { width_px: 1400, height_px: 940, scale: 1.0 });
    app.set_clock(Some(1.0));
    (app, r)
}

#[test]
fn the_pane_is_built_with_its_size_and_built_again_when_it_acts() {
    let (mut app, mut r) = rig(Mode::Home);
    app.frame(&mut r, 0.0);
    let c = counts(&app);
    assert_eq!(c.size, Some(Vec2::new(1400.0, 940.0 - BAR_H)));
    assert_eq!(c.builds, 2, "act() returned true once: a second build");
    assert_eq!(c.acts, 2);
    let pane = app.ui().rect(app.ids().home_pane).unwrap();
    assert_eq!((pane.x, pane.y, pane.w, pane.h), (0.0, BAR_H, 1400.0, 940.0 - BAR_H), "no padding of the host's own");
}

#[test]
fn shown_is_called_once_on_a_switch_to_home_and_not_for_the_first_frame() {
    let (mut app, mut r) = rig(Mode::Home);
    app.frame(&mut r, 0.0);
    assert_eq!(counts(&app).shown, 0);
    let click = |app: &mut PreviewApp, r: &mut Renderer, id| {
        let rect = app.ui().rect(id).unwrap();
        let p = Vec2::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
        app.event(Event::PointerMoved { pos: p });
        app.event(Event::PointerDown { pos: p, button: Button::Left, clicks: 1 });
        app.frame(r, 0.0);
        app.event(Event::PointerUp { pos: p, button: Button::Left });
        app.frame(r, 0.0);
    };
    let preview = app.ids().tools[1];
    click(&mut app, &mut r, preview);
    assert_eq!(app.state().mode, Mode::Preview);
    assert_eq!(counts(&app).shown, 0);
    let home = app.ids().tools[0];
    click(&mut app, &mut r, home);
    assert_eq!(app.state().mode, Mode::Home);
    assert_eq!(counts(&app).shown, 1);
}

#[test]
fn the_account_reaches_the_bar_and_the_wake_is_merged() {
    let (mut app, mut r) = rig(Mode::Home);
    app.frame(&mut r, 0.0);
    assert_eq!(app.state().account.as_deref(), Some("Zed"));
    assert!(app.list().cmds.iter().any(|c| matches!(c, Cmd::Text { text, .. } if text == "Zed")));
    assert_eq!(app.wake_in(), Some(0.05));
    // tick() polls off screen.
    let before = counts(&app).off_screen_polls;
    app.tick();
    assert_eq!(counts(&app).off_screen_polls, before + 1);
}

#[test]
fn without_a_home_the_pane_keeps_its_placeholder() {
    let state = PreviewState { mode: Mode::Home, ..PreviewState::default() };
    let mut r = Renderer::offscreen(1400, 940, 1.0).unwrap();
    let mut app = PreviewApp::new(Theme::dark(), true, Vec2::new(1400.0, 940.0), state, None);
    app.start(&mut r);
    app.event(Event::Resized { width_px: 1400, height_px: 940, scale: 1.0 });
    app.frame(&mut r, 0.0);
    assert!(app.home().is_none());
    assert!(app.list().cmds.iter().any(|c| matches!(c, Cmd::Text { text, .. } if text.contains("Home belongs to the launcher"))));
}
