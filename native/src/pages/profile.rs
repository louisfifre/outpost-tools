//! A profile's page (boards B1 to B3): its title line with Play at the right, then two parts, the game's settings (a fold:
//! where the game opens, its version, Fabric, the memory bar, the Java arguments, the folder) and the mods (a section
//! with "Update all" and "Add a mod" at the right of its head, and a line for each mod). Side by side up to the cap, one
//! under the other in a narrow window (B2), where the game's settings start folded.
//!
//! The page asks the api for what Home does not hold (`GET /api/mods/<profile>`: game version, Fabric, mods) and sends
//! its own changes; the build only records what was asked (`Ask`), and `act` sends it after the build. A mod that is
//! removed is said at the top of the list with its Undo (B3), until another thing is done.

use std::sync::OnceLock;

use mere_base::{Cmd, Cursor, Key, Rect, Vec2, Weight};
use mere_layout::{Align, Dim, Display, Margin, Side, Style, Track, Tracks};
use mere_ui::tokens::{CONTROL, GAP, GAP_WIDE};
use mere_ui::{ActionLook, ActionOpts, FieldOpts, Icon, Id, LineAction, LineEnd, ListLine, SelectOpts, TextLook, Tone, Ui, text_width};
use serde_json::json;

use super::Page;
use crate::bridge::{Done, Tag};
use crate::home::{COLUMNS_GAP, LauncherHome, STACK_BELOW, left};
use crate::model::{Details, ModLine, Open, memory_limit_gb};
use crate::text;
use preview::bar::icons;

/// The page's width (boards B1 and B3: 140 to 1260 in a 1400 window), the server page's too.
pub const WIDTH: f32 = 1120.0;
/// The width of the labels' column (15 characters of the body, on the 4 px grid).
pub const LABEL_W: f32 = 108.0;
/// The memory bar's height: the bar (20), the gap (4) and its words (16).
pub const BAR_H: f32 = 40.0;
/// The grip of an end of the memory bar: 12 wide, 16 tall, on the bar's 20 px.
const GRIP_W: f32 = 12.0;
const GRIP_H: f32 = 16.0;
/// The words under the bar keep this much between them.
const WORDS_GAP: f32 = GAP;
/// The keyboard and the pointer move an end by this much, in MB: half a gigabyte.
pub const STEP_MB: i64 = 512;
/// The least either end holds (memory.rs: a profile asks for 1 GB at the least).
const MIN_MB: i64 = 1024;
const WORD_SIZE: f32 = 11.0;
const TRASH: &str = "M9 2h6v2h5v2H4V4h5zM5 8h2v12h10V8h2v14H5zm4 2h2v8H9zm4 0h2v8h-2z";

fn trash() -> &'static Icon {
    static TRASH_ICON: OnceLock<Icon> = OnceLock::new();
    TRASH_ICON.get_or_init(|| {
        Icon::parse(TRASH, 24.0).unwrap_or_else(|e| {
            mere_base::observe!(Error, "icon path '{TRASH}': {e}");
            Icon { grid: 24.0, figures: Vec::new() }
        })
    })
}

/// Which end of the memory bar is carried.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
enum End {
    Min,
    #[default]
    Max,
}

/// What the build asked for.
#[derive(Clone, Debug, PartialEq)]
enum Ask {
    Save,
    Folder,
    Play,
    AddMod,
    Remove(String),
    Undo,
    Update(String),
    UpdateAll,
    Fabric,
    Link(String),
}

/// One line of the mods' list as it stood in the last build.
#[derive(Clone, Debug)]
pub struct LineIds {
    pub mod_id: String,
    pub line: Id,
    pub name: Id,
    pub action: Id,
    pub remove: Id,
}

/// Where the parts of the page stood in the last build, for tests.
#[derive(Clone, Debug)]
pub struct Ids {
    pub play: Id,
    pub game: Id,
    pub mods: Id,
    pub memory: Id,
    pub version: Id,
    pub fabric_update: Id,
    pub update_all: Id,
    pub add_mod: Id,
    pub undo: Id,
    pub lines: Vec<LineIds>,
}

impl Default for Ids {
    fn default() -> Ids {
        let o = Id::ROOT;
        Ids { play: o, game: o, mods: o, memory: o, version: o, fabric_update: o, update_all: o, add_mod: o, undo: o, lines: Vec::new() }
    }
}

/// What the page keeps between frames.
#[derive(Default)]
pub struct State {
    pub ids: Ids,
    /// The profile the page is about.
    id: String,
    /// Whether the game's settings are open: until the user folds or opens them, open in a wide window and folded in a
    /// narrow one (B2).
    game_open: Option<bool>,
    details: Option<Details>,
    /// The options of "Opens on", with the open they stand for.
    options: Vec<(Open, String)>,
    open_on: usize,
    address: String,
    java: String,
    focus_address: bool,
    /// The memory the bar holds, in MB (the api's unit), always a whole step of `STEP_MB`.
    min_mb: i64,
    max_mb: i64,
    /// The amounts last sent, so that a drag that ends where it began sends nothing.
    saved_mb: (i64, i64),
    drag: Option<End>,
    /// The end the keyboard moves: the one last carried, the maximum at first. Up and Down switch it.
    end: End,
    /// What a field held when it got the keyboard (`left`).
    address_was: Option<String>,
    java_was: Option<String>,
    /// The mod whose removal was asked, and the one just removed (the line "<name> removed." with its Undo).
    removing: Option<ModLine>,
    removed: Option<ModLine>,
    asks: Vec<Ask>,
}

/// The page is pushed: fills the form from the profile and asks for what it lists.
pub fn open(home: &mut LauncherHome, id: &str) {
    let Some(p) = home.profiles.iter().find(|p| p.id == id).cloned() else { return };
    let mut options = vec![(Open::Menu, text::ON_MENU.to_string())];
    options.extend(p.worlds.iter().map(|w| (Open::World(w.clone()), text::on_world(w))));
    options.push((Open::Server(String::new()), text::ON_SERVER.to_string()));
    let open_on = options.iter().position(|(o, _)| o.kind() == p.open.kind() && (p.open.kind() != "world" || o.target() == p.open.target())).unwrap_or(0);
    let (min_mb, max_mb) = (snap(p.min_memory_mb).max(MIN_MB), snap(p.memory_mb).max(MIN_MB));
    home.pages.profile = State {
        id: id.to_string(),
        options,
        open_on,
        address: if let Open::Server(a) = &p.open { a.clone() } else { String::new() },
        java: p.java_args.clone(),
        min_mb,
        max_mb,
        saved_mb: (min_mb, max_mb),
        ..State::default()
    };
    home.say("");
    home.lock();
    home.send(Tag::Instance(id.to_string()), "GET", &format!("/api/mods/{id}"), None);
}

/// What the api answered to the page's requests.
pub fn done(home: &mut LauncherHome, d: Done) {
    let ok = d.status == 200;
    match d.tag.clone() {
        Tag::Instance(id) => {
            if ok && id == home.pages.profile.id {
                home.pages.profile.details = Some(Details::parse(&d.body));
            } else if !ok {
                let why = home.failure(&d);
                home.say(&why);
            }
        }
        Tag::SaveProfile => match d.status {
            200 => {
                let st = &home.pages.profile;
                let (id, min, max, java) = (st.id.clone(), st.min_mb, st.max_mb, st.java.trim().to_string());
                let open = st.options.get(st.open_on).map(|(o, _)| match o {
                    Open::Server(_) => Open::Server(st.address.trim().to_string()),
                    other => other.clone(),
                });
                if let Some(p) = home.profiles.iter_mut().find(|p| p.id == id) {
                    p.min_memory_mb = min;
                    p.memory_mb = max;
                    p.java_args = java;
                    if let Some(open) = open {
                        p.open = open;
                    }
                }
                home.say(text::SAVED);
            }
            400 => home.say(text::REFUSED),
            409 => home.say(text::LOCKED),
            _ => {
                let why = home.failure(&d);
                home.say(&why);
            }
        },
        Tag::ModAction(kind) => {
            let st = &mut home.pages.profile;
            let removing = st.removing.take();
            if ok {
                // Only a removal opens the line with its Undo; any other change, and the Undo itself, closes it.
                st.removed = if kind == "remove" { removing } else { None };
                let id = st.id.clone();
                home.send(Tag::Instance(id.clone()), "GET", &format!("/api/mods/{id}"), None);
            } else if d.status == 409 {
                home.say(text::LOCKED);
            } else {
                let why = home.failure(&d);
                home.say(&why);
            }
        }
        Tag::Folder | Tag::OpenUrl => {
            if !ok {
                let why = home.failure(&d);
                home.say(&why);
            }
        }
        _ => {}
    }
}

/// Sends what the last build asked. True when something was.
pub fn act(home: &mut LauncherHome) -> bool {
    let asks = std::mem::take(&mut home.pages.profile.asks);
    if asks.is_empty() {
        return false;
    }
    let id = home.pages.profile.id.clone();
    // The same thing asked twice in one build is asked once.
    let mut seen: Vec<Ask> = Vec::new();
    for ask in asks {
        if seen.contains(&ask) {
            continue;
        }
        seen.push(ask.clone());
        let mod_do = |home: &mut LauncherHome, kind: &str, body: Option<serde_json::Value>| home.send(Tag::ModAction(kind.into()), "POST", &format!("/api/mods/{id}/{kind}"), body);
        match ask {
            Ask::Save => save(home),
            Ask::Folder => home.send(Tag::Folder, "POST", &format!("/api/folder/{id}"), None),
            Ask::Play => home.play(id.clone()),
            Ask::AddMod => home.open(Page::AddMod(id.clone())),
            Ask::Remove(m) => {
                home.pages.profile.removing = home.pages.profile.details.as_ref().and_then(|d| d.mods.iter().find(|x| x.id == m).cloned());
                mod_do(home, "remove", Some(json!({ "mod": m })));
            }
            Ask::Undo => {
                if let Some(m) = home.pages.profile.removed.clone() {
                    mod_do(home, "restore", Some(json!({ "mod": m.id })));
                }
            }
            Ask::Update(m) => mod_do(home, "update", Some(json!({ "mod": m }))),
            Ask::UpdateAll => mod_do(home, "update-all", None),
            Ask::Fabric => mod_do(home, "fabric", None),
            Ask::Link(url) => home.send(Tag::OpenUrl, "POST", "/api/open-url", Some(json!({ "url": url }))),
        }
    }
    true
}

/// `save`: a server without its address is not a choice yet, so nothing is sent.
fn save(home: &mut LauncherHome) {
    let st = &mut home.pages.profile;
    let Some((open, _)) = st.options.get(st.open_on).cloned() else { return };
    let open = match open {
        Open::Server(_) => {
            let address = st.address.trim().to_string();
            if address.is_empty() {
                return;
            }
            Open::Server(address)
        }
        other => other,
    };
    st.saved_mb = (st.min_mb, st.max_mb);
    let body = json!({
        "memoryMb": st.max_mb,
        "minMemoryMb": st.min_mb,
        "javaArgs": st.java.trim(),
        "open": { "kind": open.kind(), "target": open.target() },
    });
    let id = st.id.clone();
    home.send(Tag::SaveProfile, "POST", &format!("/api/settings/{id}"), Some(body));
}

pub fn build(home: &mut LauncherHome, ui: &mut Ui) {
    let id = home.pages.profile.id.clone();
    let name = home.profile_name();
    let blocked = home.play_blocked(&id);
    let stacked = ui.size().x < STACK_BELOW;
    let mut play = None;
    home.page_frame(
        ui,
        WIDTH,
        &name,
        |ui| {
            let r = ui.action_icon("play", &icons().play, text::PLAY, ActionLook::Primary, ActionOpts { disabled: blocked, ..ActionOpts::default() });
            ui.title(r.id, &text::play(&name));
            play = Some(r);
        },
        |home, ui| {
            let note = home.notes().0.to_string();
            if !note.is_empty() {
                ui.node("note", Style { margin: Margin { bottom: Side::Px(GAP_WIDE), ..Margin::default() }, ..Style::default() }, |ui| ui.paragraph(&note, TextLook::small(Tone::Muted)));
            }
            let columns = if stacked {
                Style { column: true, gap: Vec2::new(0.0, GAP_WIDE), w: Dim::Pct(100.0), ..Style::default() }
            } else {
                Style {
                    display: Display::Grid,
                    cols: Tracks::new(&[Track::MinMaxFr0(1.0), Track::MinMaxFr0(1.0)]),
                    gap: Vec2::new(COLUMNS_GAP, 0.0),
                    align_items: Align::Start,
                    w: Dim::Pct(100.0),
                    ..Style::default()
                }
            };
            ui.node("columns", columns, |ui| {
                game_part(home, ui, stacked);
                mods_part(home, ui);
            });
        },
    );
    let st = &mut home.pages.profile;
    if let Some(r) = play {
        st.ids.play = r.id;
        if r.clicked {
            st.asks.push(Ask::Play);
        }
    }
}

/// A line of the game's settings: its label in the labels' column, then what it holds.
fn row(ui: &mut Ui, key: &str, label: &str, body: impl FnOnce(&mut Ui)) {
    let grid = Style {
        display: Display::Grid,
        cols: Tracks::new(&[Track::Px(LABEL_W), Track::MinMaxFr0(1.0)]),
        gap: Vec2::new(GAP, 0.0),
        align_items: Align::Center,
        min_h: Dim::Px(CONTROL),
        ..Style::default()
    };
    ui.node(key, grid, |ui| {
        ui.text(label, TextLook::body(Tone::Muted));
        ui.node("value", Style { align_items: Align::Center, gap: Vec2::new(GAP_WIDE, 0.0), min_w: Dim::Px(0.0), ..Style::default() }, body);
    });
}

/// What takes the room of a row that a control fills.
fn fill() -> Style {
    Style { grow: 1.0, min_w: Dim::Px(0.0), ..Style::default() }
}

fn game_part(home: &mut LauncherHome, ui: &mut Ui, stacked: bool) {
    let locked = home.locked;
    let (machine_mb, used_mb) = (home.machine_mb, home.used_mb);
    let dir = home.profiles.iter().find(|p| p.id == home.pages.profile.id).map(|p| p.game_dir.clone()).unwrap_or_default();
    let mut open = home.pages.profile.game_open.unwrap_or(!stacked);
    let r = ui.fold("game", text::GAME_SECTION, &mut open, |ui| {
        ui.node("settings", Style { column: true, gap: Vec2::new(0.0, GAP), ..Style::default() }, |ui| {
            opens_on(home, ui, locked);
            game_version(home, ui);
            fabric(home, ui, locked);
            row(ui, "memory", text::MEMORY, |ui| {
                ui.node("bar", fill(), |ui| memory_bar(home, ui, machine_mb, used_mb, locked));
            });
            java_and_folder(home, ui, locked, &dir);
        });
    });
    let st = &mut home.pages.profile;
    if r.clicked {
        st.game_open = Some(open);
    }
    st.ids.game = r.id;
}

fn opens_on(home: &mut LauncherHome, ui: &mut Ui, locked: bool) {
    let labels: Vec<String> = home.pages.profile.options.iter().map(|(_, l)| l.clone()).collect();
    let mut at = home.pages.profile.open_on;
    let mut changed = false;
    let mut select = Id::ROOT;
    row(ui, "openOn", text::OPEN_ON, |ui| {
        ui.node("select", fill(), |ui| {
            let r = ui.select_menu("select", &labels, &mut at, SelectOpts { disabled: locked, ..SelectOpts::default() });
            select = r.resp.id;
            changed = r.changed;
        });
    });
    home.ids.open_on = select;
    let st = &mut home.pages.profile;
    st.open_on = at;
    let is_server = matches!(st.options.get(at), Some((Open::Server(_), _)));
    if changed {
        // Choosing a server without an address puts the keyboard in its field.
        if is_server && st.address.trim().is_empty() {
            st.focus_address = true;
        }
        st.asks.push(Ask::Save);
    }
    if is_server {
        let before = st.address.clone();
        let mut field = Id::ROOT;
        let mut left_it = false;
        row(ui, "address", "", |ui| {
            ui.node("field", fill(), |ui| {
                let opts = FieldOpts { placeholder: text::SERVER_ADDRESS, width: Dim::Pct(100.0), focus: std::mem::take(&mut st.focus_address), disabled: locked, ..FieldOpts::default() };
                let r = ui.text_field("field", &mut st.address, opts);
                field = r.resp.id;
                left_it = left(&mut st.address_was, &before, &st.address, &r);
            });
        });
        if left_it {
            st.asks.push(Ask::Save);
        }
        home.ids.address = field;
    }
}

/// The game version: its choice is shown, not offered yet. A change has to list first the mods that have no version for the
/// new one (play-a-profile.md), and that step is not drawn: until it is, the select does not open.
fn game_version(home: &mut LauncherHome, ui: &mut Ui) {
    let details = home.pages.profile.details.clone().unwrap_or_default();
    let mut versions = details.game_versions.clone();
    if !versions.contains(&details.game_version) {
        versions.insert(0, details.game_version.clone());
    }
    let mut at = versions.iter().position(|v| *v == details.game_version).unwrap_or(0);
    let mut id = Id::ROOT;
    row(ui, "version", text::GAME_VERSION, |ui| {
        ui.node("select", Style { w: Dim::Px(88.0), shrink: 0.0, ..Style::default() }, |ui| {
            id = ui.select_menu("select", &versions, &mut at, SelectOpts { disabled: true, ..SelectOpts::default() }).resp.id;
        });
    });
    home.pages.profile.ids.version = id;
}

fn fabric(home: &mut LauncherHome, ui: &mut Ui, locked: bool) {
    let fabric = home.pages.profile.details.as_ref().and_then(|d| d.fabric.clone());
    let loaded = home.pages.profile.details.is_some();
    let mut act = None;
    row(ui, "fabric", text::FABRIC, |ui| {
        let Some(f) = &fabric else {
            if loaded {
                ui.node("state", fill(), |ui| ui.text(text::NOT_INSTALLED, TextLook::body(Tone::Muted)));
                act = Some(ui.action_with("fabric", text::INSTALL, ActionLook::Underlined, ActionOpts { disabled: locked, ..ActionOpts::default() }));
            }
            return;
        };
        ui.node("state", Style { align_items: Align::Baseline, gap: Vec2::new(GAP_WIDE, 0.0), ..fill() }, |ui| {
            ui.text(&f.version, TextLook::body(Tone::Text));
            if let Some(latest) = &f.latest {
                ui.text(&text::newer(latest), TextLook::small(Tone::Muted));
            }
        });
        if f.latest.is_some() {
            act = Some(ui.action_with("fabric", text::UPDATE, ActionLook::Underlined, ActionOpts { disabled: locked, ..ActionOpts::default() }));
        }
    });
    let st = &mut home.pages.profile;
    st.ids.fabric_update = act.map_or(Id::ROOT, |r| r.id);
    if act.is_some_and(|r| r.clicked) {
        st.asks.push(Ask::Fabric);
    }
}

fn java_and_folder(home: &mut LauncherHome, ui: &mut Ui, locked: bool, dir: &str) {
    let st = &mut home.pages.profile;
    let before = st.java.clone();
    let mut java = (Id::ROOT, false);
    row(ui, "java", text::JAVA_ARGS, |ui| {
        ui.node("field", fill(), |ui| {
            let r = ui.text_field("field", &mut st.java, FieldOpts { width: Dim::Pct(100.0), disabled: locked, ..FieldOpts::default() });
            java = (r.resp.id, left(&mut st.java_was, &before, &st.java, &r));
        });
    });
    if java.1 {
        st.asks.push(Ask::Save);
    }
    let mut open = None;
    row(ui, "folder", text::FOLDER, |ui| {
        ui.node("path", fill(), |ui| ui.ellipsis_start(dir, TextLook::body(Tone::Muted)));
        open = Some(ui.action_with("open", text::OPEN, ActionLook::Underlined, ActionOpts::default()));
    });
    home.ids.java = java.0;
    if let Some(r) = open {
        home.ids.open_folder = r.id;
        if r.clicked {
            home.pages.profile.asks.push(Ask::Folder);
        }
    }
}

/// An amount of memory held to a whole step of `STEP_MB`.
fn snap(mb: i64) -> i64 {
    (mb as f64 / STEP_MB as f64).round() as i64 * STEP_MB
}

/// Where the words under the bar start (x, from the bar's left) and what they say; None for a word left out.
#[derive(Clone, Debug, PartialEq)]
pub struct Words {
    pub min: (f32, String),
    pub max: (f32, String),
    pub used: Option<f32>,
    pub total: Option<(f32, String)>,
}

/// The words under a bar `w` long, whose ends stand at `min_x` and `max_x` and whose used part starts at `used_x`
/// (None when the PC's use is not known). No two words ever overlap, at any amount and any width: the ends' words come
/// first and keep each other clear (the minimum's slides left of its grip, then the maximum's right of it), then the
/// whole at the bar's end, then the PC's use where the hatching starts. When they cannot all stand, the least needed
/// goes first: the PC's use, then the unit of the minimum, then the whole's, then the words "min" and "max".
pub fn memory_words(w: f32, min_mb: i64, max_mb: i64, total_gb: i64, min_x: f32, max_x: f32, used_x: Option<f32>) -> Words {
    let width = |s: &str| text_width(s, WORD_SIZE);
    let total_s = text::memory_total(total_gb);
    let total_w = width(&total_s);
    // From the fullest reading to the barest; the first that fits is the one shown. (the whole's word, the minimum's
    // unit, the words "min" and "max")
    let forms: [(bool, bool, bool); 5] = [(true, true, true), (true, false, true), (false, true, true), (false, false, true), (false, false, false)];
    let mut last = None;
    for (level, &(with_total, with_unit, with_words)) in forms.iter().enumerate() {
        let (min_s, max_s) = (text::memory_min(min_mb, with_unit, with_words), text::memory_max(max_mb, with_words));
        let (min_w, max_w) = (width(&min_s), width(&max_s));
        let right = if with_total { w - total_w - WORDS_GAP } else { w };
        let mut max_at = (max_x - 4.0).max(0.0);
        let mut min_at = (min_x - 12.0).max(0.0);
        // The minimum's word slides left to stay clear of the maximum's; at the bar's start the maximum's goes right.
        if min_at + min_w + WORDS_GAP > max_at {
            min_at = (max_at - WORDS_GAP - min_w).max(0.0);
            max_at = max_at.max(min_at + min_w + WORDS_GAP);
        }
        // Near the bar's end the maximum's word slides left, and the minimum's with it.
        if max_at + max_w > right {
            max_at = right - max_w;
            min_at = min_at.min(max_at - WORDS_GAP - min_w);
        }
        let mut words = Words { min: (min_at, min_s), max: (max_at, max_s), used: None, total: with_total.then(|| (w - total_w, total_s.clone())) };
        if min_at < 0.0 || max_at < 0.0 {
            last = Some(words);
            continue;
        }
        if level == 0 {
            if let Some(x) = used_x {
                let used_w = width(text::USED_BY_PC);
                let (lo, hi) = (max_at + max_w + WORDS_GAP, right - used_w);
                if lo <= hi {
                    words.used = Some(x.clamp(lo, hi));
                }
            }
        }
        return words;
    }
    // Nothing stands clear: the bar is narrower than two bare amounts. The maximum's alone is the one kept.
    let mut words = last.unwrap_or(Words { min: (0.0, String::new()), max: (0.0, String::new()), used: None, total: None });
    words.min.1.clear();
    words.max.0 = 0.0;
    words
}

/// The memory bar (B1): as long as the PC's memory, the part the PC uses now hatched at its right end, and over the
/// bar the profile's minimum and maximum, each an end the user carries in half gigabytes. The maximum never reaches into
/// the hatched part. Under it, the words (`memory_words`): each end's amount, what the PC uses, the whole. The bar takes
/// the keyboard like a field: Left and Right move the end it holds by `STEP_MB`, Home and End take it to its least and
/// its most, Up and Down switch the end.
fn memory_bar(home: &mut LauncherHome, ui: &mut Ui, machine_mb: i64, used_mb: i64, locked: bool) {
    let total_gb = ((machine_mb as f64 / 1024.0).round() as i64).max(1);
    let total_mb = total_gb * 1024;
    let limit_mb = (memory_limit_gb(machine_mb, used_mb).min(total_gb) * 1024).max(MIN_MB);
    let st = &mut home.pages.profile;
    let free = if used_mb > 0 { ((machine_mb - used_mb).max(0) as f32 / machine_mb.max(1) as f32).clamp(0.0, 1.0) } else { 1.0 };
    let resp = ui.canvas_focus("bar", Style { grow: 1.0, h: Dim::Px(BAR_H), min_w: Dim::Px(0.0), ..Style::default() }, locked, |ui, resp, keys| {
        let t = *ui.theme();
        let off = |c| t.disabled_if(c, locked);
        let w = resp.rect.map_or(0.0, |r| r.w);
        // An end holds from 1 GB to the maximum's limit; the minimum stops where the maximum is.
        let place = |st: &mut State, end: End, mb: i64| match end {
            End::Min => st.min_mb = snap(mb).clamp(MIN_MB, st.max_mb),
            End::Max => st.max_mb = snap(mb).clamp(st.min_mb, limit_mb.max(st.min_mb)),
        };
        if let (Some(rect), false) = (resp.rect, locked) {
            let pointer = ui.pointer();
            let value = |p: Vec2| ((p.x - rect.x) / rect.w.max(1.0) * total_mb as f32) as i64;
            if let (true, Some(p)) = (resp.pressed, pointer) {
                // A press on the bar itself (not on its words) takes the nearer end; on a tie, the one the press is on the side of.
                if p.y - rect.y < GRIP_H + 4.0 {
                    let v = value(p);
                    let (to_min, to_max) = ((v - st.min_mb).abs(), (v - st.max_mb).abs());
                    st.drag = Some(if to_min < to_max || (to_min == to_max && v < st.min_mb) { End::Min } else { End::Max });
                }
            }
            if let (Some(end), Some(p)) = (st.drag, pointer) {
                st.end = end;
                place(st, end, value(p));
            }
            if !resp.held && st.drag.take().is_some() && (st.min_mb, st.max_mb) != st.saved_mb {
                st.asks.push(Ask::Save);
            }
            if resp.hovered || resp.held {
                ui.want_cursor(Cursor::ResizeEW);
            }
        }
        // The keyboard: each key that moves an end is saved at once, as a drag is when it is let go.
        for (key, _) in &keys.keys {
            let end = st.end;
            let at = if end == End::Min { st.min_mb } else { st.max_mb };
            let (least, most) = if end == End::Min { (MIN_MB, st.max_mb) } else { (st.min_mb, limit_mb.max(st.min_mb)) };
            match key {
                Key::Left => place(st, end, at - STEP_MB),
                Key::Right => place(st, end, at + STEP_MB),
                Key::Home => place(st, end, least),
                Key::End => place(st, end, most),
                _ => st.end = if end == End::Min { End::Max } else { End::Min },
            }
        }
        if !keys.keys.is_empty() && (st.min_mb, st.max_mb) != st.saved_mb {
            st.asks.push(Ask::Save);
        }
        let at = |mb: i64| mb as f32 / total_mb as f32 * w;
        let (min_x, max_x) = (at(st.min_mb), at(st.max_mb));
        let mut cmds = vec![Cmd::Fill { rect: Rect::new(0.0, 8.0, w, 4.0), color: off(t.line) }];
        // The part the PC uses: faint diagonal stripes, from where the free memory ends to the bar's end.
        let used_x = free * w;
        if used_mb > 0 && used_x < w {
            cmds.push(Cmd::PushClip { rect: Rect::new(used_x, 4.0, w - used_x, 12.0) });
            let mut x = used_x - 12.0;
            while x < w {
                cmds.push(Cmd::Polyline { points: vec![Vec2::new(x, 16.0), Vec2::new(x + 12.0, 4.0)], width: 2.0, color: off(t.edge) });
                x += 6.0;
            }
            cmds.push(Cmd::PopClip);
        }
        cmds.push(Cmd::Fill { rect: Rect::new(min_x, 8.0, (max_x - min_x).max(0.0), 4.0), color: off(t.text) });
        let grip = |x: f32| Rect::new(x - GRIP_W / 2.0, 2.0, GRIP_W, GRIP_H);
        cmds.push(Cmd::Fill { rect: grip(min_x), color: off(t.ground) });
        cmds.push(Cmd::Frame { rect: grip(min_x), width_px: 1.0, color: off(t.text) });
        cmds.push(Cmd::Fill { rect: grip(max_x), color: off(t.fill) });
        if keys.focused {
            // The kit's ring around the bar, and a second around the end the keys move.
            cmds.push(ui.focus_ring(Rect::new(0.0, 0.0, w, GRIP_H + 4.0)));
            let g = grip(if st.end == End::Min { min_x } else { max_x });
            cmds.push(ui.focus_ring(Rect::new(g.x - 2.0, g.y - 2.0, g.w + 4.0, g.h + 4.0)));
        }
        // The words: 11 px, under the bar, none over another.
        let word = |x: f32, s: &str, color| Cmd::Text { at: Vec2::new(x, 25.0), text: s.to_string(), size: WORD_SIZE, color: off(color), weight: Weight::Regular };
        let words = memory_words(w, st.min_mb, st.max_mb, total_gb, min_x, max_x, (used_mb > 0 && used_x < w).then_some(used_x));
        cmds.push(word(words.min.0, &words.min.1, t.text));
        cmds.push(word(words.max.0, &words.max.1, t.text));
        if let Some(x) = words.used {
            cmds.push(word(x, text::USED_BY_PC, t.muted));
        }
        if let Some((x, s)) = &words.total {
            cmds.push(word(*x, s, t.muted));
        }
        cmds
    });
    st.ids.memory = resp.id;
    home.ids.memory = resp.id;
}

fn mods_part(home: &mut LauncherHome, ui: &mut Ui) {
    let locked = home.locked;
    let details = home.pages.profile.details.clone().unwrap_or_default();
    let removed = home.pages.profile.removed.clone();
    let updates = details.updates();
    let kept = &home.pages.profile.ids;
    let mut ids = Ids { play: kept.play, game: kept.game, memory: kept.memory, version: kept.version, fabric_update: kept.fabric_update, ..Ids::default() };
    let mut asks = Vec::new();
    let mut all = None;
    let mut add = None;
    ids.mods = ui.section(
        "mods",
        &text::mods_section(details.mods.len()),
        |ui| {
            // "Update all" is for when several have a newer version: one has its own Update.
            if updates >= 2 {
                all = Some(ui.action_with("updateAll", &text::update_all(updates), ActionLook::Underlined, ActionOpts { disabled: locked, ..ActionOpts::default() }));
            }
            add = Some(ui.action("add", text::ADD_A_MOD, ActionLook::Underlined));
        },
        |ui| {
            if let Some(m) = &removed {
                ui.node("removed", Style { align_items: Align::Center, gap: Vec2::new(GAP_WIDE, 0.0), min_h: Dim::Px(CONTROL), ..Style::default() }, |ui| {
                    ui.text(&text::removed(&m.name), TextLook::body(Tone::Text));
                    let r = ui.action_with("undo", text::UNDO, ActionLook::Underlined, ActionOpts { disabled: locked, ..ActionOpts::default() });
                    ids.undo = r.id;
                    if r.clicked {
                        asks.push(Ask::Undo);
                    }
                });
            }
            for m in &details.mods {
                let new = m.latest.as_deref().map(text::newer).unwrap_or_default();
                let action = if m.latest.is_some() { LineAction::Word(text::UPDATE) } else { LineAction::None };
                let line = ListLine {
                    name: &m.name,
                    version: &m.version,
                    link: m.slug.is_some(),
                    new_version: &new,
                    action,
                    action_disabled: locked,
                    end: Some(LineEnd { icon: trash(), disabled: locked }),
                    ..ListLine::default()
                };
                let r = ui.list_line(&format!("mod-{}", m.id), line);
                ui.title(r.end.id, &text::remove_mod(&m.name));
                if let Some(slug) = &m.slug {
                    ui.title(r.name.id, &text::open_on_modrinth(&m.name));
                    if r.name.clicked {
                        asks.push(Ask::Link(format!("https://modrinth.com/mod/{slug}")));
                    }
                }
                if r.action.clicked {
                    asks.push(Ask::Update(m.id.clone()));
                }
                if r.end.clicked {
                    asks.push(Ask::Remove(m.id.clone()));
                }
                ids.lines.push(LineIds { mod_id: m.id.clone(), line: r.line, name: r.name.id, action: r.action.id, remove: r.end.id });
            }
        },
    );
    if let Some(r) = all {
        ids.update_all = r.id;
        if r.clicked {
            asks.push(Ask::UpdateAll);
        }
    }
    if let Some(r) = add {
        ids.add_mod = r.id;
        if r.clicked {
            asks.push(Ask::AddMod);
        }
    }
    let st = &mut home.pages.profile;
    st.ids = ids;
    st.asks.extend(asks);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every word the layout returns, with its start and end.
    fn spans(words: &Words) -> Vec<(f32, f32)> {
        let w = |s: &str| text_width(s, WORD_SIZE);
        let mut v = vec![(words.min.0, words.min.0 + w(&words.min.1)), (words.max.0, words.max.0 + w(&words.max.1))];
        if let Some(x) = words.used {
            v.push((x, x + w(text::USED_BY_PC)));
        }
        if let Some((x, s)) = &words.total {
            v.push((*x, x + w(s)));
        }
        v.retain(|s| s.1 > s.0);
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v
    }

    /// At every width from a narrow bar to a wide one, every PC from 4 to 128 GB, every pair of ends (in half gigabytes)
    /// and every use, no word overlaps another or leaves the bar.
    #[test]
    fn the_words_never_overlap_at_any_amount_or_width() {
        for w in (160..=900).step_by(20) {
            for total_gb in [4i64, 8, 16, 32, 64, 128] {
                let total_mb = total_gb * 1024;
                for min_mb in (1024..=total_mb).step_by(512 * 3) {
                    for max_mb in (min_mb..=total_mb).step_by(512 * 5) {
                        for used in [None, Some(0.9f32), Some(0.5), Some(0.1)] {
                            let at = |mb: i64| mb as f32 / total_mb as f32 * w as f32;
                            let words = memory_words(w as f32, min_mb, max_mb, total_gb, at(min_mb), at(max_mb), used.map(|u| u * w as f32));
                            let s = spans(&words);
                            for pair in s.windows(2) {
                                assert!(pair[0].1 < pair[1].0, "w {w}, {total_gb} GB, {min_mb}..{max_mb}, used {used:?}: {words:?}");
                            }
                            assert!(s.first().is_none_or(|f| f.0 >= -0.01) && s.last().is_none_or(|l| l.1 <= w as f32 + 0.01), "w {w}: {words:?}");
                        }
                    }
                }
            }
        }
    }

    /// The board's bar keeps its reading: the ends' words, the PC's use at its hatching, the whole at the end.
    #[test]
    fn the_boards_words_keep_their_reading() {
        let w = 420.0;
        let at = |mb: i64| mb as f32 / 16384.0 * w;
        let words = memory_words(w, 2048, 6144, 16, at(2048), at(6144), Some(0.68 * w));
        assert_eq!((words.min.1.as_str(), words.max.1.as_str()), ("min 2 GB", "max 6 GB"));
        assert!(words.used.is_some() && words.total.as_ref().is_some_and(|t| t.1 == "16 GB"));
    }

    #[test]
    fn steps_are_half_gigabytes() {
        assert_eq!(snap(6144), 6144);
        assert_eq!(snap(6400), 6656);
        assert_eq!(snap(6300), 6144);
        assert_eq!(snap(0), 0);
    }
}
