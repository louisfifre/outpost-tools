//! The page that adds a mod to a profile (board B4): a search of Modrinth for the profile's game version and Fabric, an
//! action to add a file, and the results, one line each in the kit's list line (the same widget as the profile's mods,
//! so a mod looks the same here and there): its name and version, its description under it, the version it would be
//! updated to with Update, a muted check with Added for a mod the profile has, Add for the others, and an icon that
//! opens its page on Modrinth. A mod added with another says so under the line of the mod that was asked for.
//!
//! What the page asks of the api (the real one answers in `app/src/api/mods.rs`, the fake in `fake.rs` answers the same
//! for the tests).
//!   POST /api/mods/search {profile, query}  -> {mods: [{id, title, description, version, url, state, newVersion}]}
//!        `state` is "add", "added" or "update"; `version` is the profile's when it has the mod, else the one to add.
//!   POST /api/mods/add {profile, mod}       -> {added: [{id, title, version, neededBy}]}  (the mod, then what it needs)
//!   POST /api/mods/update {profile, mod}    -> {}
//!   POST /api/mods/file {profile}           -> {}  (the api asks for the file, adds it as it is; {} too when none is chosen)
//!   POST /api/mods/open {url}               -> {}  (opens a page of Modrinth in the browser)
//! A failure is an `error` sentence with its status, shown under the search.

use std::sync::OnceLock;

use mere_base::Vec2;
use mere_layout::{Align, Dim, Edges, Style};
use mere_ui::tokens::{GAP, GAP_WIDE};
use mere_ui::{ActionLook, FieldOpts, Icon, LineAction, LineEnd, ListLine, Rule, TextLook, Tone, Ui};
use serde_json::{Value, json};

use crate::bridge::{Done, Tag};
use crate::home::LauncherHome;
use crate::text;

/// The page's width.
pub const WIDTH: f32 = 760.0;
/// The search field's width: 48 characters of the body text (board B4).
pub const FIELD_W: f32 = 346.0;

const CHECK: &str = "M4 12h2v2H4zM6 14h2v2H6zM8 16h2v2H8zM10 14h2v2h-2zM12 12h2v2h-2zM14 10h2v2h-2zM16 8h2v2h-2zM18 6h2v2h-2z";
const EXTERNAL: &str = "M13 3h8v8h-2V7h-2v2h-2v2h-2v2h-2v-2h2V9h2V7h2V5h-4zM3 5h8v2H5v12h12v-6h2v8H3z";

fn icon(cell: &'static OnceLock<Icon>, path: &str) -> &'static Icon {
    cell.get_or_init(|| {
        Icon::parse(path, 24.0).unwrap_or_else(|e| {
            mere_base::observe!(Error, "icon path '{path}': {e}");
            Icon { grid: 24.0, figures: Vec::new() }
        })
    })
}

/// What a result line offers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Add,
    Added,
    Update,
}

/// One result of the search.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub id: String,
    pub title: String,
    pub description: String,
    pub version: String,
    pub url: String,
    pub mark: Mark,
    pub new_version: String,
}

impl Found {
    fn parse(v: &Value) -> Found {
        let s = |key: &str| v[key].as_str().unwrap_or_default().to_string();
        let mark = match v["state"].as_str() {
            Some("added") => Mark::Added,
            Some("update") => Mark::Update,
            _ => Mark::Add,
        };
        Found { id: s("id"), title: s("title"), description: s("description"), version: s("version"), url: s("url"), mark, new_version: s("newVersion") }
    }
}

/// What the page asks of the api; it comes back with the answer as a `Tag`.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// The search for this query.
    Search(String),
    /// Add the mod with this id, and what it needs.
    Add(String),
    Update(String),
    /// A page of Modrinth, by its address.
    Open(String),
    /// A mod as a file.
    File,
}

/// What the page keeps between frames.
#[derive(Default)]
pub struct State {
    pub query: String,
    pub found: Vec<Found>,
    /// What was added with a mod, by the mod's id: said under its line.
    pub came_with: Vec<(String, String)>,
    /// The mods an add or an update is under way for.
    pub busy: Vec<String>,
    /// One sentence under the search: why there is no list.
    pub note: String,
    focus: bool,
}

fn said(d: &Done) -> String {
    d.body["error"].as_str().map(String::from).unwrap_or_else(|| text::FAILED.to_string())
}

/// The page is pushed: a clean state, the field ready to type in, and Modrinth's list asked.
pub fn opened(home: &mut LauncherHome) {
    home.pages.add_mod = State { focus: true, ..State::default() };
    home.ask_mods(Ask::Search(String::new()));
}

/// Does what the build asked, with the profile the page is about.
pub fn act(home: &mut LauncherHome, ask: Ask) {
    let Some(profile) = home.current() else { return };
    let (path, body) = match &ask {
        Ask::Search(query) => ("/api/mods/search", json!({"profile": profile, "query": query})),
        Ask::Add(id) => ("/api/mods/add", json!({"profile": profile, "mod": id})),
        Ask::Update(id) => ("/api/mods/update", json!({"profile": profile, "mod": id})),
        Ask::Open(url) => ("/api/mods/open", json!({"url": url})),
        Ask::File => ("/api/mods/file", json!({"profile": profile})),
    };
    home.send(Tag::Mods(ask), "POST", path, Some(body));
}

/// What the api answered.
pub fn done(home: &mut LauncherHome, ask: Ask, d: &Done) {
    let ok = d.status == 200;
    match ask {
        Ask::Search(query) => {
            let state = &mut home.pages.add_mod;
            // An answer to a query the field no longer holds is out of date.
            if query.trim() != state.query.trim() {
                return;
            }
            if ok {
                state.found = d.body["mods"].as_array().map(|a| a.iter().map(Found::parse).collect()).unwrap_or_default();
                state.note = if state.found.is_empty() { text::NO_MODS.to_string() } else { String::new() };
            } else {
                state.found.clear();
                state.note = said(d);
            }
        }
        Ask::Add(id) | Ask::Update(id) => {
            let state = &mut home.pages.add_mod;
            state.busy.retain(|b| *b != id);
            if !ok {
                state.note = said(d);
                return;
            }
            state.note.clear();
            for dep in d.body["added"].as_array().into_iter().flatten().filter(|a| a["neededBy"].as_str().is_some_and(|by| !by.is_empty())) {
                let sentence = text::came_with(dep["title"].as_str().unwrap_or_default(), dep["neededBy"].as_str().unwrap_or_default());
                state.came_with.push((id.clone(), sentence));
            }
            // The lines say what the profile has now.
            let query = state.query.clone();
            home.ask_mods(Ask::Search(query));
        }
        Ask::File => {
            if ok {
                let query = home.pages.add_mod.query.clone();
                home.ask_mods(Ask::Search(query));
            } else {
                home.pages.add_mod.note = said(d);
            }
        }
        Ask::Open(_) => {
            if !ok {
                home.pages.add_mod.note = said(d);
            }
        }
    }
}

pub fn build(home: &mut LauncherHome, ui: &mut Ui) {
    let title = text::add_mod_title(&home.profile_name());
    home.page_frame(ui, WIDTH, &title, |_| {}, |home, ui| {
        search_bar(home, ui);
        ui.rule(Rule::Section, Edges::default());
        let note = home.pages.add_mod.note.clone();
        if !note.is_empty() {
            ui.node("note", Style { pad: Edges::vh(GAP_WIDE, 0.0), ..Style::default() }, |ui| ui.paragraph(&note, TextLook::small(Tone::Muted)));
        }
        let lines = home.pages.add_mod.found.clone();
        for line in &lines {
            result(home, ui, line);
        }
    });
}

/// The search field and, at the right end, the action that takes a file.
fn search_bar(home: &mut LauncherHome, ui: &mut Ui) {
    let bar = Style { align_items: Align::Center, gap: Vec2::new(GAP_WIDE, 0.0), pad: Edges::new(0.0, 0.0, GAP, 0.0), w: Dim::Pct(100.0), ..Style::default() };
    ui.node("bar", bar, |ui| {
        let state = &mut home.pages.add_mod;
        let opts = FieldOpts { placeholder: text::SEARCH_MODRINTH, width: Dim::Px(FIELD_W), focus: std::mem::take(&mut state.focus), ..FieldOpts::default() };
        let r = ui.text_field("search", &mut state.query, opts);
        let query = state.query.clone();
        if r.changed {
            home.ask_mods(Ask::Search(query));
        }
        ui.node("gap", Style { grow: 1.0, ..Style::default() }, |_| {});
        if ui.action("file", text::ADD_A_FILE, ActionLook::Underlined).clicked {
            home.ask_mods(Ask::File);
        }
    });
}

/// One result: the kit's list line, its parts from what the search said.
fn result(home: &mut LauncherHome, ui: &mut Ui, found: &Found) {
    static CHECK_ICON: OnceLock<Icon> = OnceLock::new();
    static EXTERNAL_ICON: OnceLock<Icon> = OnceLock::new();
    let state = &home.pages.add_mod;
    let busy = state.busy.contains(&found.id);
    let note = state.came_with.iter().filter(|(id, _)| *id == found.id).map(|(_, said)| said.as_str()).collect::<Vec<_>>().join(" ");
    let new_version = if found.mark == Mark::Update { text::new_version(&found.new_version) } else { String::new() };
    let action = match (found.mark, busy) {
        (Mark::Added, _) => LineAction::Done(icon(&CHECK_ICON, CHECK), text::ADDED),
        (Mark::Update, false) => LineAction::Word(text::UPDATE),
        (Mark::Update, true) => LineAction::Word(text::UPDATING),
        (Mark::Add, false) => LineAction::Word(text::ADD),
        (Mark::Add, true) => LineAction::Word(text::ADDING),
    };
    let line = ListLine {
        name: &found.title,
        version: &found.version,
        detail: &found.description,
        note: &note,
        new_version: &new_version,
        action,
        action_disabled: busy,
        end: Some(LineEnd { icon: icon(&EXTERNAL_ICON, EXTERNAL), disabled: found.url.is_empty() }),
        ..ListLine::default()
    };
    let r = ui.list_line(&found.id, line);
    ui.title(r.end.id, text::OPEN_ON_MODRINTH);
    if r.action.clicked {
        let ask = if found.mark == Mark::Update { Ask::Update(found.id.clone()) } else { Ask::Add(found.id.clone()) };
        home.pages.add_mod.busy.push(found.id.clone());
        home.ask_mods(ask);
    }
    if r.end.clicked {
        home.ask_mods(Ask::Open(found.url.clone()));
    }
}
