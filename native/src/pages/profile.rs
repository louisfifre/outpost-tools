//! A profile's page (boards B1 to B3): its title line with Play at the right, then two parts, the game's settings (a fold:
//! where the game opens, its version, Fabric, the memory bar, the Java arguments, the folder) and the mods (a section
//! with "Update all" and "Add a mod" at the right of its head, and a line for each mod). Side by side up to the cap, one
//! under the other in a narrow window (B2), where the game's settings start folded.
//!
//! The page asks the api for what Home does not hold (`GET /api/mods/<profile>`: game version, Fabric, mods) and sends
//! its own changes; the build only records what was asked (`Ask`), and `act` sends it after the build. A mod that is
//! removed is said at the top of the list with its Undo (B3), until another thing is done.

use std::sync::OnceLock;

use mere_base::{Cmd, Cursor, Rect, Vec2, Weight};
use mere_layout::{Align, Dim, Display, Margin, Side, Style, Track, Tracks};
use mere_ui::tokens::{CONTROL, GAP, GAP_TIGHT, GAP_WIDE};
use mere_ui::{ActionLook, ActionOpts, FieldOpts, Icon, IconLook, IconOpts, Id, LineAction, LineEnd, ListLine, SelectOpts, TextLook, Tone, Ui, text_width};
use serde_json::json;

use super::Page;
use crate::bridge::{Done, Tag};
use crate::home::{COLUMNS_GAP, COLUMNS_MAX, LauncherHome, STACK_BELOW, left};
use crate::model::{Details, ModLine, Open, memory_limit_gb};
use crate::text;
use preview::bar::icons;

/// The width of the labels' column (15 characters of the body, on the 4 px grid).
pub const LABEL_W: f32 = 108.0;
/// The memory bar's height: the bar (20), the gap (4) and its words (16).
pub const BAR_H: f32 = 40.0;
/// The grip of an end of the memory bar: 12 wide, 16 tall, on the bar's 20 px.
const GRIP_W: f32 = 12.0;
const GRIP_H: f32 = 16.0;
/// The words under the bar keep this much between them.
const WORDS_GAP: f32 = GAP_TIGHT;
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
#[derive(Copy, Clone, Debug, PartialEq)]
enum End {
    Min,
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
    min_gb: i64,
    max_gb: i64,
    /// The amounts last sent, so that a drag that ends where it began sends nothing.
    saved_gb: (i64, i64),
    drag: Option<End>,
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
    let (min_gb, max_gb) = ((p.min_memory_mb as f64 / 1024.0).round() as i64, (p.memory_mb as f64 / 1024.0).round() as i64);
    home.pages.profile = State {
        id: id.to_string(),
        options,
        open_on,
        address: if let Open::Server(a) = &p.open { a.clone() } else { String::new() },
        java: p.java_args.clone(),
        min_gb,
        max_gb,
        saved_gb: (min_gb, max_gb),
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
                let (id, min, max, java) = (st.id.clone(), st.min_gb, st.max_gb, st.java.trim().to_string());
                let open = st.options.get(st.open_on).map(|(o, _)| match o {
                    Open::Server(_) => Open::Server(st.address.trim().to_string()),
                    other => other.clone(),
                });
                if let Some(p) = home.profiles.iter_mut().find(|p| p.id == id) {
                    p.min_memory_mb = min * 1024;
                    p.memory_mb = max * 1024;
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
    st.saved_gb = (st.min_gb, st.max_gb);
    let body = json!({
        "memoryMb": st.max_gb * 1024,
        "minMemoryMb": st.min_gb * 1024,
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
        COLUMNS_MAX,
        &name,
        |ui| {
            let r = ui.icon_action("play", &icons().play, IconLook::Filled, IconOpts { disabled: blocked, ..IconOpts::default() });
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

/// The memory bar (B1): as long as the PC's memory, the part the PC uses now hatched at its right end, and over the
/// bar the profile's minimum and maximum, each an end the user carries in whole gigabytes. The maximum never reaches into
/// the hatched part. Under it, the words: each end's amount, what the PC uses, the whole.
fn memory_bar(home: &mut LauncherHome, ui: &mut Ui, machine_mb: i64, used_mb: i64, locked: bool) {
    let total = ((machine_mb as f64 / 1024.0).round() as i64).max(1);
    let limit = memory_limit_gb(machine_mb, used_mb).min(total);
    let st = &mut home.pages.profile;
    let free = if used_mb > 0 { ((machine_mb - used_mb).max(0) as f32 / machine_mb.max(1) as f32).clamp(0.0, 1.0) } else { 1.0 };
    let resp = ui.canvas("bar", Style { grow: 1.0, h: Dim::Px(BAR_H), min_w: Dim::Px(0.0), ..Style::default() }, |ui, resp| {
        let t = *ui.theme();
        let off = |c| t.disabled_if(c, locked);
        let w = resp.rect.map_or(0.0, |r| r.w);
        if let (Some(rect), false) = (resp.rect, locked) {
            let pointer = ui.pointer();
            let value = |p: Vec2| (p.x - rect.x) / rect.w * total as f32;
            if let (true, Some(p)) = (resp.pressed, pointer) {
                // A press on the bar itself (not on its words) takes the nearer end; on a tie, the one the press is on the side of.
                if p.y - rect.y < GRIP_H + 4.0 {
                    let v = value(p);
                    let (to_min, to_max) = ((v - st.min_gb as f32).abs(), (v - st.max_gb as f32).abs());
                    st.drag = Some(if to_min < to_max || (to_min == to_max && v < st.min_gb as f32) { End::Min } else { End::Max });
                }
            }
            if let (Some(end), Some(p)) = (st.drag, pointer) {
                let gb = value(p).round() as i64;
                match end {
                    End::Min => st.min_gb = gb.clamp(1, st.max_gb),
                    End::Max => st.max_gb = gb.clamp(st.min_gb, limit.max(st.min_gb)),
                }
            }
            if !resp.held && st.drag.take().is_some() && (st.min_gb, st.max_gb) != st.saved_gb {
                st.asks.push(Ask::Save);
            }
            if resp.hovered || resp.held {
                ui.want_cursor(Cursor::ResizeEW);
            }
        }
        let at = |gb: i64| gb as f32 / total as f32 * w;
        let (min_x, max_x) = (at(st.min_gb), at(st.max_gb));
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
        // The words: 11 px, under the bar. Both ends' words keep clear of each other.
        let word = |x: f32, s: &str, color| Cmd::Text { at: Vec2::new(x, 25.0), text: s.to_string(), size: 11.0, color: off(color), weight: Weight::Regular };
        let (min_s, max_s, total_s) = (text::memory_min(st.min_gb), text::memory_max(st.max_gb), text::memory_total(total));
        let w_of = |s: &str| text_width(s, 11.0);
        let max_at = (max_x - 4.0).max(0.0);
        let mut min_at = (min_x - 12.0).max(0.0);
        if min_at + w_of(&min_s) + WORDS_GAP > max_at {
            min_at = (max_at - WORDS_GAP - w_of(&min_s)).max(0.0);
        }
        let total_at = w - w_of(&total_s);
        cmds.push(word(min_at, &min_s, t.text));
        cmds.push(word(max_at, &max_s, t.text));
        // What the PC uses starts where its hatching does, or ends where the whole's word begins when that is nearer.
        let used_at = used_x.min(total_at - WORDS_GAP - w_of(text::USED_BY_PC));
        if used_mb > 0 && used_at > max_at + w_of(&max_s) + WORDS_GAP {
            cmds.push(word(used_at, text::USED_BY_PC, t.muted));
        }
        cmds.push(word(total_at, &total_s, t.muted));
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
