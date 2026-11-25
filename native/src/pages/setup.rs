//! The first start's set-up (boards E1 and E2): three steps, Java, Prism, the sign-in. The set-up is not a page of
//! Home, it stands before it, so it has no title line. Only the Java step is here: it says that the game needs Java 25
//! and offers its download with the size (E1), then shows how far the download is, with Stop (E2).

use mere_base::Vec2;
use mere_layout::{Align, Dim, Edges, Margin, Side, Style};
use mere_ui::tokens::{GAP, GAP_WIDE, INSET};
use preview::CONTENT_TOP;
use mere_ui::{ActionLook, ActionOpts, Rule, TextLook, Tone, Ui};

use crate::bridge::{Done, Tag};
use crate::home::LauncherHome;
use crate::text;

use super::Page;

/// The page's width.
pub const WIDTH: f32 = 560.0;
/// Between the steps and what the step says, and between the steps themselves (24 on the kit's grid).
const SPACE: f32 = 24.0;
/// Where the steps start under the top bar (board E1); the shell already keeps `CONTENT_TOP` of it.
const TOP: f32 = 48.0;
const MB: u64 = 1024 * 1024;
/// How often the state is asked while a download runs, and while nothing moves.
const FAST: f64 = 0.5;
const SLOW: f64 = 5.0;

/// How far the api says the Java step is.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Java {
    /// Not answered yet: nothing is said under the steps.
    #[default]
    Unknown,
    /// A Java 25 is on the PC, by its version.
    Found(String),
    /// None is: with why the download cannot be offered with its size, when Adoptium did not answer.
    Missing { error: Option<String> },
    /// A download is under way: the api's stage and its bytes.
    Downloading { stage: String, done: u64, total: u64 },
    /// The last download failed, with the api's reason.
    Failed(String),
}

/// What the page keeps between frames.
#[derive(Default)]
pub struct State {
    pub java: Java,
    /// What the download weighs, in bytes, once the api said (it stays while a download runs).
    pub size: Option<u64>,
    /// The poll's clock at the last call.
    now: f64,
    /// When the state is asked again.
    next: f64,
    /// A state request is asked and not answered.
    asked: bool,
    /// The download was asked and the api has not answered: a state read meanwhile is stale.
    starting: bool,
}

fn mb(bytes: u64) -> u64 {
    (bytes + MB / 2) / MB
}

/// Asks the api for the Java step's state when it is due, while the page is shown. True when it asked.
pub(crate) fn poll(home: &mut LauncherHome, now: f64) -> bool {
    home.pages.setup.now = now;
    let s = &home.pages.setup;
    if home.page() != Some(&Page::Setup) || s.asked || now < s.next || matches!(s.java, Java::Found(_)) {
        return false;
    }
    home.pages.setup.asked = true;
    home.send(Tag::Java, "GET", "/api/java", None);
    true
}

/// When the page next asks, on the poll's clock; None when it has nothing to ask.
pub(crate) fn due(home: &LauncherHome) -> Option<f64> {
    let s = &home.pages.setup;
    (home.page() == Some(&Page::Setup) && !s.asked && !matches!(s.java, Java::Found(_))).then_some(s.next)
}

/// What the api answered to the page's requests.
pub(crate) fn done(home: &mut LauncherHome, d: Done) {
    let s = &mut home.pages.setup;
    let said = |d: &Done| d.body["error"].as_str().map(str::to_string);
    let failed = |d: &Done| said(d).unwrap_or_else(|| text::FAILED.to_string());
    match d.tag {
        Tag::Java => {
            s.asked = false;
            if s.starting {
                return;
            }
            if d.status != 200 {
                s.java = Java::Missing { error: Some(failed(&d)) };
                s.next = s.now + SLOW;
                return;
            }
            let b = &d.body;
            if let Some(size) = b["size"].as_u64() {
                s.size = Some(size);
            }
            let word = |key: &str| b[key].as_str().unwrap_or_default().to_string();
            s.java = match b["state"].as_str() {
                Some("found") => Java::Found(word("version")),
                Some("downloading") => Java::Downloading { stage: word("stage"), done: b["done"].as_u64().unwrap_or(0), total: b["total"].as_u64().unwrap_or(0) },
                Some("failed") => Java::Failed(failed(&d)),
                _ => Java::Missing { error: said(&d) },
            };
            s.next = s.now + if matches!(s.java, Java::Downloading { .. }) { FAST } else { SLOW };
        }
        Tag::JavaStart => {
            s.starting = false;
            if d.status != 200 {
                s.java = Java::Failed(failed(&d));
            }
            s.next = s.now;
        }
        // The download ends at its next chunk: the state says when.
        Tag::JavaStop => s.next = s.now,
        _ => {}
    }
}

pub fn build(home: &mut LauncherHome, ui: &mut Ui) {
    let column = Style { column: true, w: Dim::Pct(100.0), max_w: Dim::Px(WIDTH), margin: Margin { top: Side::Px(TOP - CONTENT_TOP), ..Margin::horizontal_auto() }, gap: Vec2::new(0.0, SPACE), ..Style::default() };
    ui.node("setup", column, |ui| {
        steps(ui, 0);
        ui.node("step", Style { column: true, ..Style::default() }, |ui| {
            ui.rule(Rule::Section, Edges::default());
            let body = Style { column: true, gap: Vec2::new(0.0, GAP), pad: Edges::new(GAP_WIDE, 0.0, 0.0, 0.0), ..Style::default() };
            ui.node("java", body, |ui| java_step(home, ui));
        });
    });
}

/// "1 JAVA  2 PRISM  3 SIGN IN": the step the guide is at in the text colour, the others muted.
fn steps(ui: &mut Ui, at: usize) {
    ui.node("steps", Style { gap: Vec2::new(SPACE, 0.0), ..Style::default() }, |ui| {
        for (i, name) in text::SETUP_STEPS.iter().enumerate() {
            let tone = if i == at { Tone::Text } else { Tone::Muted };
            ui.text(&format!("{} {}", i + 1, name.to_uppercase()), TextLook { tone, ..TextLook::heading() });
        }
    });
}

/// A word button set back by its own padding so that its word lines up with the text above it. True when pressed.
fn flush(ui: &mut Ui, key: &str, words: &str, tip: Option<&str>) -> bool {
    let style = Style { margin: Margin { left: Side::Px(-INSET), ..Margin::default() }, ..Style::default() };
    let mut clicked = false;
    ui.node(&format!("{key}Box"), style, |ui| {
        let r = ui.action(key, words, ActionLook::Plain);
        if let Some(tip) = tip {
            ui.title(r.id, tip);
        }
        clicked = r.clicked;
    });
    clicked
}

fn java_step(home: &mut LauncherHome, ui: &mut Ui) {
    let java = home.pages.setup.java.clone();
    let size = home.pages.setup.size;
    match java {
        Java::Unknown => {}
        Java::Found(version) => ui.paragraph(&text::java_ready(&version), TextLook::body(Tone::Text)),
        Java::Missing { error: why } => offer(home, ui, size, why),
        Java::Failed(why) => offer(home, ui, size, Some(why)),
        Java::Downloading { stage, done, total } => {
            ui.paragraph(text::JAVA_DOWNLOADING, TextLook::body(Tone::Text));
            let total = if total == 0 { size.unwrap_or(0) } else { total };
            let share = if total == 0 { 0.0 } else { (done as f32 / total as f32).clamp(0.0, 1.0) };
            let words = match stage.as_str() {
                "verifying" => text::JAVA_CHECKING.to_string(),
                "extracting" => text::JAVA_UNPACKING.to_string(),
                _ => text::java_progress(mb(done), mb(total)),
            };
            // The bar is a button that does nothing, as wide as the column.
            ui.node("bar", Style { w: Dim::Pct(100.0), ..Style::default() }, |ui| {
                ui.action_with("progress", &words, ActionLook::Running { share }, ActionOpts { grow: 1.0, ..ActionOpts::default() });
            });
            if flush(ui, "stop", text::JAVA_STOP, Some(text::JAVA_NOTHING_KEPT)) {
                home.send(Tag::JavaStop, "POST", "/api/java/stop", None);
            }
        }
    }
}

/// Board E1: the sentence, the download with its size and where it comes from, and Later. `why` is what went wrong
/// last (Adoptium out of reach, a download that failed), said between the sentence and the button.
fn offer(home: &mut LauncherHome, ui: &mut Ui, size: Option<u64>, why: Option<String>) {
    ui.paragraph(text::JAVA_NEEDED, TextLook::body(Tone::Text));
    if let Some(why) = why {
        ui.paragraph(&why, TextLook::body(Tone::Warn));
    }
    let mut start = false;
    ui.node("offer", Style { align_items: Align::Center, gap: Vec2::new(GAP_WIDE, 0.0), ..Style::default() }, |ui| {
        start = ui.action("download", &text::java_download(size.map(mb)), ActionLook::Primary).clicked;
        ui.text(text::JAVA_FROM, TextLook::small(Tone::Muted));
    });
    if start {
        home.pages.setup.java = Java::Downloading { stage: "downloading".into(), done: 0, total: size.unwrap_or(0) };
        home.pages.setup.starting = true;
        home.send(Tag::JavaStart, "POST", "/api/java/download", None);
    }
    if flush(ui, "later", text::JAVA_LATER, None) {
        home.back();
    }
}
