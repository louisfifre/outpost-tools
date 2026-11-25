//! The bar when the window is too narrow for all of it: what it gives up, in which order, and that what it keeps
//! stays whole. Layout is judged on the real rects of a built frame, from the window's least width to 2560 px.
//!
//! The world the tests load (the out folder `PREVIEW_DATA_DIR` names) was stopped at a coarse detail, so Flat, Structures and Section
//! are greyed in it; `None` stands for a world with every view on.

mod common;

use mere_base::{App, Button, Cmd, Event, Rect, Vec2};
use mere_ui::{Id, LIST_PAD, LIST_ROW, Theme, Ui, tool_action_width};
use mere_win32::Renderer;
use preview::overflow::{self, Form, Inputs};
use preview::world::World;
use preview::{Mode, PreviewApp, PreviewState, ShellIds, picture_with, text};

const SECTION: usize = preview::paint::SECTION;
/// Relief and Temperature, the views the tests press: with eight map views they are not at the strip's start.
const RELIEF: usize = 5;
const TEMPERATURE: usize = 6;
const STRUCTURES: usize = 7;
const VIEW_COUNT: usize = preview::overflow::VIEW_COUNT;
/// What a coarse world greys: Flat, Steepness, Cliffs, Structures and Section.
const GREYED: [bool; 9] = [false, true, true, true, false, false, false, true, true];
const WIDTHS: [f32; 7] = [600.0, 700.0, 800.0, 1000.0, 1280.0, 1920.0, 2560.0];
const LONG: &str = "AVeryLongAccountNameThatCannotFit";

fn rect_of(ui: &Ui, id: Id) -> Rect {
    assert_ne!(id, Id::ROOT);
    ui.rect(id).expect("built")
}

fn state(account: Option<&str>, view: usize) -> PreviewState {
    PreviewState { account: account.map(String::from), view, reduce_motion: true, ..PreviewState::default() }
}

fn built(w: f32, st: PreviewState, world: Option<&World>) -> (mere_base::DrawList, Ui, ShellIds) {
    picture_with(Theme::dark(), Vec2::new(w, 800.0), world, st)
}

fn drawn(list: &mere_base::DrawList) -> Vec<String> {
    list.cmds.iter().filter_map(|c| if let Cmd::Text { text, .. } = c { Some(text.clone()) } else { None }).collect()
}

fn inputs(w: f32, account: Option<&str>, pressed: usize, disabled: [bool; 9]) -> Inputs<'_> {
    Inputs { width: w, account, account_mark: 0.0, preview_open: true, pressed, disabled, shapes: pressed != SECTION }
}

/// Every distinct form met while the window narrows from 2560 to 500, widest first.
fn forms_while_narrowing(account: Option<&str>, pressed: usize, disabled: [bool; 9]) -> Vec<Form> {
    let mut seen: Vec<Form> = Vec::new();
    for w in (500..=2560).rev() {
        let f = overflow::plan(&inputs(w as f32, account, pressed, disabled));
        if seen.last() != Some(&f) {
            seen.push(f);
        }
    }
    seen
}

/// A form of the preview tool's bar: the brand is not in it, so its parts are off.
fn form(more: &[usize], centred: bool, shapes_in_more: bool) -> Form {
    Form { account: false, word: false, more: more.to_vec(), centred, shapes_in_more }
}

/// The widths the plan counts are the widths the layout gives: the strip, for every form it can take.
#[test]
fn the_measured_strip_is_the_laid_out_strip() {
    let coarse = common::world();
    let mut w = 600.0;
    let mut checked = 0;
    while w <= 2560.0 {
        for world in [coarse.as_ref(), None] {
            for view in [0, 5, SECTION] {
                for account in [None, Some(LONG)] {
                    if world.is_some_and(World::coarse) && preview::paint::needs_full(view) {
                        continue;
                    }
                    let (_, ui, ids) = built(w, state(account, view), world);
                    let disabled: [bool; 9] = std::array::from_fn(|i| world.is_some_and(World::coarse) && preview::paint::needs_full(i));
                    let predicted = overflow::strip_width(&inputs(w, account, view, disabled), &ids.form);
                    let got = rect_of(&ui, ids.strip).w;
                    assert!((predicted - got).abs() < 0.01, "w {w} view {view} {account:?}: predicted {predicted}, laid out {got}");
                    checked += 1;
                }
            }
        }
        w += 53.0;
    }
    assert!(checked > 100, "{checked}");
}

/// The order of giving way in the preview tool: the greyed views, then the others from the strip's end (the pressed
/// view, Relief here, never among them), then the tools stop being centred, then 2D and 3D join the views in "More".
#[test]
fn the_bar_gives_way_in_its_order() {
    let coarse = GREYED;
    assert_eq!(
        forms_while_narrowing(Some("Louis"), RELIEF, coarse),
        vec![
            form(&[], true, false),
            form(&[1, 2, 3, 7, 8], true, false),
            form(&[1, 2, 3, 6, 7, 8], true, false),
            form(&[1, 2, 3, 4, 6, 7, 8], true, false),
            form(&[0, 1, 2, 3, 4, 6, 7, 8], true, false),
            form(&[0, 1, 2, 3, 4, 6, 7, 8], false, false),
            form(&[0, 1, 2, 3, 4, 6, 7, 8], false, true),
        ],
        "greyed world"
    );
    // With every view on there is nothing greyed to move: the views go from the end.
    let all_on = forms_while_narrowing(Some("Louis"), RELIEF, [false; 9]);
    let expected = vec![
        form(&[], true, false),
        form(&[8], true, false),
        form(&[7, 8], true, false),
        form(&[6, 7, 8], true, false),
        form(&[4, 6, 7, 8], true, false),
        form(&[3, 4, 6, 7, 8], true, false),
        form(&[2, 3, 4, 6, 7, 8], true, false),
        form(&[1, 2, 3, 4, 6, 7, 8], true, false),
        form(&[0, 1, 2, 3, 4, 6, 7, 8], true, false),
        form(&[0, 1, 2, 3, 4, 6, 7, 8], false, false),
        form(&[0, 1, 2, 3, 4, 6, 7, 8], false, true),
    ];
    assert_eq!(all_on, expected, "every view on");
    // The pressed view is never moved: Section pressed, the others go from Structures back, and it has no 2D or 3D.
    let section = forms_while_narrowing(None, SECTION, [false; 9]);
    assert!(section.iter().all(|f| !f.more.contains(&SECTION) && !f.shapes_in_more), "{section:?}");
    assert_eq!(section.last(), Some(&form(&[0, 1, 2, 3, 4, 5, 6, 7], false, false)));
    // Structures pressed: the last view goes first, then the one before the pressed one is passed over.
    let structures = forms_while_narrowing(None, STRUCTURES, [false; 9]);
    assert!(structures.iter().all(|f| !f.more.contains(&STRUCTURES)), "{structures:?}");
    assert_eq!(structures.iter().find(|f| !f.more.is_empty()).map(|f| f.more.clone()), Some(vec![SECTION]));
}

/// Home keeps its brand: it gives up the account first, and the word stays whatever the width, because it buys no
/// room next to the window's buttons. At the window's least width it gives up nothing.
#[test]
fn home_gives_up_the_account_and_never_the_word() {
    let home = |w: f32| Inputs { width: w, account: Some("Louis"), account_mark: 0.0, preview_open: false, pressed: 2, disabled: [false; 9], shapes: true };
    assert_eq!(overflow::plan(&home(preview::MIN_SIZE.0)), Form::default());
    assert_eq!(overflow::plan(&home(2560.0)), Form::default());
    let mut seen: Vec<Form> = Vec::new();
    for w in (300..=2560).rev() {
        let f = overflow::plan(&home(w as f32));
        if seen.last() != Some(&f) {
            seen.push(f);
        }
    }
    assert_eq!(seen[..2], [Form::default(), Form { account: false, ..Form::default() }]);
}

/// The const arithmetic of the narrowest bar is the runtime arithmetic of the plan: the least width of the preview
/// tool's bar is the width at which its last form stops fitting.
#[test]
fn the_narrowest_bar_is_where_the_last_form_stops_fitting() {
    // Temperature is the widest view's tab, which the const counts for the pressed one.
    let i = |w: f32| inputs(w, None, TEMPERATURE, [false; 9]);
    let last = |w: f32| overflow::plan(&i(w));
    let narrowest = last(500.0);
    assert_eq!(narrowest, form(&[0, 1, 2, 3, 4, 5, 7, 8], false, true));
    assert!(overflow::fits(&i(overflow::NARROW_BAR), &narrowest), "fits at the const's width");
    assert!(!overflow::fits(&i(overflow::NARROW_BAR - 1.0), &narrowest), "and not a pixel less");
    assert!(preview::MIN_SIZE.0 >= overflow::NARROW_BAR);
}
/// The widths at which the bar changes form, printed for the record (`cargo test -- --ignored --nocapture`).
/// A measure, not a check: it asserts nothing, so it does not run with the suite.
#[test]
#[ignore = "prints the thresholds; run on demand"]
fn the_thresholds_are_printed() {
    for (name, disabled) in [("greyed world", GREYED), ("every view on", [false; 9])] {
        let mut last: Option<Form> = None;
        for w in (500..=2560).rev() {
            let f = overflow::plan(&inputs(w as f32, Some("Louis"), RELIEF, disabled));
            if last.as_ref() != Some(&f) {
                println!("{name}: from {w} px down: {f:?}");
                last = Some(f);
            }
        }
    }
}

/// A form is only taken once it fits: at every width the first form of the order that fits is the one chosen, so
/// the bar never gives up more than it must.
#[test]
fn the_bar_takes_the_first_form_that_fits() {
    let disabled = GREYED;
    for w in (500..=2560).step_by(3) {
        let i = inputs(w as f32, Some("Louis"), RELIEF, disabled);
        let chosen = overflow::plan(&i);
        let order = forms_while_narrowing(Some("Louis"), RELIEF, disabled);
        let at = order.iter().position(|f| *f == chosen).expect("a form of the order");
        assert!(order[..at].iter().all(|f| !overflow::fits(&i, f)), "{w}: a form before {chosen:?} fits");
    }
}

/// At the widths of the sweep: nothing overlaps, both tools say their names, the panels' buttons are at the bar's
/// ends, the tools are centred while the form says so and sit left, after the settings button, when it does not.
#[test]
fn the_bar_fits_at_every_width_with_the_tools_named() {
    let world = common::world();
    for w in WIDTHS {
        for (name, world) in [("coarse", world.as_ref()), ("all views", None)] {
            let (list, ui, ids) = built(w, state(None, RELIEF), world);
            let (strip, bar) = (rect_of(&ui, ids.strip), rect_of(&ui, ids.bar));
            let (left, right) = (rect_of(&ui, ids.toggles[0]), rect_of(&ui, ids.toggles[1]));
            assert_eq!(ids.brand, Id::ROOT, "{name} {w}: the brand is Home's");
            assert!(left.x + left.w <= strip.x + 0.01, "{name} {w}: the settings button overlaps the tools {left:?} {strip:?}");
            assert!(strip.x + strip.w <= right.x + 0.01, "{name} {w}: the tools overlap the figures button");
            assert!(right.x + right.w <= w - overflow::ENDS_W + 0.01, "{name} {w}: the figures button overlaps the window's buttons");
            assert!((left.x - overflow::TOGGLE_EDGE).abs() < 0.01, "{name} {w}: the settings button is TOGGLE_EDGE in from the edge: {left:?}");
            assert!((right.x + right.w + 8.0 - (w - overflow::ENDS_W)).abs() < 0.01, "{name} {w}: the figures button is 8 before the window's buttons: {right:?}");
            if ids.form.centred {
                assert!((strip.x + strip.w / 2.0 - w / 2.0).abs() <= 0.5, "{name} {w}: the tools are centred");
            } else {
                assert!((strip.x - (left.x + left.w + 12.0)).abs() < 1.01, "{name} {w}: the tools start 12 px after the settings button: {strip:?}");
            }
            let mut parts = ids.tabs.clone();
            parts.extend(ids.tools);
            if ids.more != Id::ROOT {
                parts.push(ids.more);
            }
            let mut rects: Vec<Rect> = parts.iter().map(|&p| rect_of(&ui, p)).collect();
            rects.sort_by(|a, b| a.x.total_cmp(&b.x));
            assert!(rects.windows(2).all(|p| p[0].x + p[0].w <= p[1].x + 1.01), "{name} {w}: parts overlap");
            assert!(rects.iter().all(|r| r.x >= strip.x - 0.01 && r.x + r.w <= strip.x + strip.w + 0.01 && r.y >= bar.y && r.y + r.h <= bar.y + bar.h), "{name} {w}: a part is outside");
            // The tools say their names, always, and are as wide as the word and the icon need.
            let words = drawn(&list);
            for (i, tool) in ids.tools.iter().enumerate() {
                assert!(words.iter().any(|t| t == text::TOOL_NAMES[i]), "{name} {w}: '{}' is not drawn", text::TOOL_NAMES[i]);
                let r = rect_of(&ui, *tool);
                assert!((r.w - tool_action_width(text::TOOL_NAMES[i])).abs() < 0.01, "{name} {w}: tool {i} is {} wide", r.w);
            }
            // 2D and 3D are tabs or rows of "More", as the form says.
            assert_eq!(words.iter().any(|t| t == "2D"), !ids.form.shapes_in_more, "{name} {w}: 2D");
        }
    }
}

/// The least width of the preview tool's bar is the window's least width: the bar fits there, in its last form,
/// with the two tools named and nothing overlapping.
#[test]
fn at_the_least_width_the_bar_is_in_its_last_form_and_fits() {
    let w = preview::MIN_SIZE.0;
    let (list, ui, ids) = built(w, state(None, TEMPERATURE), None);
    assert_eq!(ids.form, form(&[0, 1, 2, 3, 4, 5, 7, 8], false, true));
    let (strip, right) = (rect_of(&ui, ids.strip), rect_of(&ui, ids.toggles[1]));
    assert!(strip.x + strip.w <= right.x + 0.01, "the tools reach the figures button: {strip:?} {right:?}");
    let words = drawn(&list);
    for kept in ["Home", "Preview", "Temperature", "More"] {
        assert!(words.iter().any(|t| t == kept), "{kept} is drawn: {words:?}");
    }
    for gone in ["Biomes", "Relief", "2D", "3D"] {
        assert!(!words.iter().any(|t| t == gone), "{gone} is in More: {words:?}");
    }
}

/// At 1000 the greyed views are in "More" and Biomes, Relief and Temperature stay, the bar still centred.
#[test]
fn at_1000_the_greyed_views_are_in_more() {
    let Some(world) = common::world() else { return };
    assert!(world.coarse(), "the loaded world is a coarse one");
    let (list, ui, ids) = built(1000.0, state(Some("Louis"), RELIEF), Some(&world));
    assert_eq!(ids.form, form(&[1, 2, 3, 7, 8], true, false));
    assert_eq!(ids.tabs.len(), 4 + 2, "Biomes, Vegetation, Relief, Temperature, then 2D and 3D");
    assert_ne!(ids.more, Id::ROOT);
    let words = drawn(&list);
    for kept in ["Biomes", "Vegetation", "Relief", "Temperature", "More", "Home", "Preview", "2D", "3D"] {
        assert!(words.iter().any(|t| t == kept), "{kept} is drawn: {words:?}");
    }
    // The preview tool has no brand, so no account either.
    for gone in ["Flat", "Steepness", "Cliffs", "Structures", "Section", "Louis"] {
        assert!(!words.iter().any(|t| t == gone), "{gone} is not drawn");
    }
    // "More" is the last of the views, before the 2D and 3D tabs.
    let (more, relief, d2) = (rect_of(&ui, ids.more), rect_of(&ui, ids.tabs[2]), rect_of(&ui, ids.tabs[4]));
    assert!(relief.x < more.x && more.x + more.w <= d2.x);
}

/// Wide enough, nothing is given up.
#[test]
fn a_wide_window_keeps_everything() {
    let (_, _, ids) = built(1920.0, state(Some("Louis"), RELIEF), None);
    assert_eq!(ids.form, form(&[], true, false));
    assert_eq!(ids.more, Id::ROOT);
    assert_eq!(ids.tabs.len(), VIEW_COUNT + 2);
}

/// Home has the brand: it keeps the account and the word at the window's least width, and the preview tool has the
/// panels' buttons in their place.
#[test]
fn home_keeps_its_brand_and_the_preview_tool_has_the_panels_buttons() {
    let w = preview::MIN_SIZE.0;
    let mut st = state(Some("Louis"), RELIEF);
    st.mode = Mode::Home;
    let (list, _, ids) = built(w, st, None);
    assert!(ids.form.account && ids.form.word, "{:?}", ids.form);
    assert_ne!(ids.brand, Id::ROOT);
    assert_eq!(ids.toggles, [Id::ROOT; 2], "no panel buttons in Home");
    let words = drawn(&list);
    assert!(words.iter().any(|t| t == "Louis"), "{words:?}");
    let (list, _, ids) = built(w, state(Some("Louis"), RELIEF), None);
    assert_eq!(ids.brand, Id::ROOT);
    assert_ne!(ids.toggles, [Id::ROOT; 2]);
    let words = drawn(&list);
    assert!(!words.iter().any(|t| t == "Louis"), "{words:?}");
}

/// The pressed view stays among the tabs, at every width and for every view.
#[test]
fn the_pressed_view_is_never_in_more() {
    for view in 0..=SECTION {
        for w in (600..=1500).step_by(25) {
            let (_, _, ids) = built(w as f32, state(Some("Louis"), view), None);
            assert!(!ids.form.more.contains(&view), "view {view} at {w}: {:?}", ids.form);
            let shapes = if view == SECTION || ids.form.shapes_in_more { 0 } else { 2 };
            assert_eq!(ids.tabs.len(), VIEW_COUNT - ids.form.more.len() + shapes, "view {view} at {w}");
        }
    }
}

/// In Home, an account that fits is whole; one that is a little too long is cut with an ellipsis; one that would be
/// cut to nothing is gone.
#[test]
fn the_account_is_whole_then_cut_then_gone() {
    let mut cut = None;
    let mut gone_at = None;
    for w in (400..=700).rev() {
        let st = PreviewState { mode: Mode::Home, ..state(Some("Alexandrina"), RELIEF) };
        let (list, _, ids) = built(w as f32, st, None);
        let words = drawn(&list);
        let shown = words.iter().find(|t| t.starts_with("Al") && !t.contains(' '));
        match shown {
            Some(t) if t == "Alexandrina" => {}
            Some(t) => {
                assert!(t.ends_with('\u{2026}') && t.chars().count() >= 4, "{t}");
                cut.get_or_insert(w);
            }
            None => {
                assert!(!ids.form.account);
                gone_at.get_or_insert(w);
            }
        }
    }
    let (cut, gone_at) = (cut.expect("a width where it is cut"), gone_at.expect("a width where it goes"));
    assert!(cut > gone_at, "cut at {cut}, gone at {gone_at}");
}
struct Rig {
    app: PreviewApp,
    r: Renderer,
}

impl Rig {
    fn new(w: u32, st: PreviewState, world: Option<&World>) -> Rig {
        let mut r = Renderer::offscreen(w, 700, 1.0).expect("offscreen renderer");
        let mut app = PreviewApp::new(Theme::dark(), true, Vec2::new(w as f32, 700.0), st, world.cloned());
        app.start(&mut r);
        app.event(Event::Resized { width_px: w, height_px: 700, scale: 1.0 });
        app.frame(&mut r, 0.0);
        app.frame(&mut r, 0.0);
        Rig { app, r }
    }

    fn frame(&mut self) {
        self.app.frame(&mut self.r, 0.0);
        self.app.frame(&mut self.r, 0.0);
    }

    fn click(&mut self, p: Vec2) {
        self.app.event(Event::PointerMoved { pos: p });
        self.app.event(Event::PointerDown { pos: p, button: Button::Left, clicks: 1 });
        self.frame();
        self.app.event(Event::PointerUp { pos: p, button: Button::Left });
        self.frame();
    }

    fn more(&self) -> Rect {
        rect_of(self.app.ui(), self.app.ids().more)
    }

    fn row(&self, i: usize) -> Vec2 {
        let b = self.more();
        Vec2::new(b.x + 20.0, b.y + b.h + mere_ui::tokens::GAP_TIGHT + LIST_PAD + (i as f32 + 0.5) * LIST_ROW)
    }

    fn words(&mut self) -> Vec<String> {
        drawn(self.app.list())
    }
}

/// "More" lists the views it holds; choosing one presses it, and it takes its place in the strip.
#[test]
fn choosing_a_view_in_more_presses_it() {
    let mut rig = Rig::new(1000, state(None, 0), None);
    assert_eq!(rig.app.ids().form.more, vec![4, 5, 6, 7, 8], "Vegetation to Section are in More at 1000");
    let at = rig.more();
    rig.click(Vec2::new(at.x + 10.0, at.y + 10.0));
    let words = rig.words();
    for row in ["Structures", "Section"] {
        assert!(words.iter().any(|t| t == row), "{row} is listed: {words:?}");
    }
    let at = rig.row(4);
    rig.click(at);
    assert_eq!(rig.app.state().view, SECTION, "Section is pressed");
    assert!(!rig.app.ids().form.more.contains(&SECTION), "and it is in the strip: {:?}", rig.app.ids().form);
    assert_eq!(rig.app.ids().tabs.len(), VIEW_COUNT - rig.app.ids().form.more.len(), "the section has no 2D and 3D, so the strip has room for more views");
}

/// A greyed view is listed, shown disabled, and a press on it presses nothing.
#[test]
fn a_greyed_view_in_more_cannot_be_chosen() {
    let Some(world) = common::world() else { return };
    let mut rig = Rig::new(1000, PreviewState { reduce_motion: true, ..PreviewState::for_world(Some(&world)) }, Some(&world));
    let before = rig.app.state().view;
    let at = rig.more();
    rig.click(Vec2::new(at.x + 10.0, at.y + 10.0));
    let words = rig.words();
    for row in ["Flat", "Structures", "Section"] {
        assert!(words.iter().any(|t| t == row), "{row} is listed");
    }
    let at = rig.row(0);
    rig.click(at);
    assert_eq!(rig.app.state().view, before, "Flat did nothing");
    assert!(rig.words().iter().any(|t| t == "Flat"), "the list stays open: Flat is in it and in no tab");
}

/// Narrower still, 2D and 3D are rows of "More" after the views, and choosing one presses it like a tab does.
#[test]
fn shapes_in_more_are_chosen_like_views() {
    // The least width the window can have: the bar has given up everything it can.
    let mut rig = Rig::new(preview::MIN_SIZE.0.ceil() as u32, state(None, 0), None);
    let ids = rig.app.ids().clone();
    assert!(ids.form.shapes_in_more && !ids.form.centred, "{:?}", ids.form);
    assert_eq!(ids.tabs.len(), 1, "only the pressed view is a tab");
    let at = rig.more();
    rig.click(Vec2::new(at.x + 10.0, at.y + 10.0));
    let words = rig.words();
    for row in ["Flat", "Section", "2D", "3D"] {
        assert!(words.iter().any(|t| t == row), "{row} is listed: {words:?}");
    }
    let n = ids.form.more.len();
    let at = rig.row(n + 1);
    rig.click(at);
    assert!(rig.app.state().in3d, "3D is pressed");
    assert_eq!(rig.app.state().view, 0, "and the view is the same");
    let at = rig.more();
    rig.click(Vec2::new(at.x + 10.0, at.y + 10.0));
    let at = rig.row(n);
    rig.click(at);
    assert!(!rig.app.state().in3d, "2D is back");
}
