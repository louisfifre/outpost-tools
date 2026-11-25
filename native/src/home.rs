//! The launcher's Home (index.html's `#launcher` and its script, `load` to `saveServer`): the profiles and the dev server, in two
//! parts that fill the window, drawn with mere-ui and answered by the api through a `Bridge`.
//!
//! Layout (boards A1 and A3): two columns that widen continuously up to a cap of 880 px (`COLUMNS_MAX`), then stay
//! centred; under `STACK_BELOW` the two parts are stacked in one column. Each part is a section with its action at
//! the right end of its head (it does not fold), and its lines open its page: a press on a profile's line or on the
//! server's opens that page (`pages`), the Play and Start buttons keeping their own press.
//!
//! Home is the floor of a stack of pages (`pages::Page`): `open` pushes one, the back action pops it. A page is drawn
//! inside `page_frame`, which gives it the title line and its width.
//!
//! The build only reads the pointer and the keys, and records what was asked (`Want`); `act` does it, after the
//! build, so that a build never waits on the api. Answers are taken by `poll`.

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use mere_base::Vec2;
use mere_layout::{Align, Dim, Display, Edges, Margin, Overflow, Side, Style, Track, Tracks};
use mere_ui::tokens::{CONTROL, FRAME, GAP, GAP_TIGHT, GAP_WIDE, INSET, INSET_TIGHT, REGION};
use mere_ui::{ActionLook, ActionOpts, FieldOpts, FieldResp, IconLook, IconOpts, Id, NumberOpts, NumberSpec, TextLook, ToggleOpts, Tone, Ui};

/// The widest the two columns grow together (board A1): a line read across a whole wide screen loses the link between
/// its name and its button.
pub const COLUMNS_MAX: f32 = 880.0;
/// Under this width of the window the two parts are stacked (board A3 is 800 px wide, board A1 1400). The boards do not
/// give the width where it changes: to validate.
pub const STACK_BELOW: f32 = 860.0;
/// The gap between the two columns.
pub const COLUMNS_GAP: f32 = REGION;
/// The width of the memory field and of the whitelist toggle, which line up: two digits and a unit fit, on the 4 px grid.
pub const SHORT_W: f32 = 56.0;
/// The height of the Java arguments' field: room for two lines, on the 4 px grid.
pub const JAVA_H: f32 = 52.0;
/// Every control of Home is a control's height, the select's own: a field's and the toggle's padding count the
/// frame, around the 18 px line (`FRAME + INSET_TIGHT` above and below makes `CONTROL`).
const PAD_V: f32 = FRAME + INSET_TIGHT;
const _: () = assert!(18.0 + 2.0 * PAD_V == CONTROL);
/// How long a server still running after its stop was taken holds the stop button, in seconds.
const STOP_WAIT: f64 = 30.0;
use preview::{AccountAsk, AccountEntry, AccountMenu, Home};
use preview::PAGE;
use preview::bar::icons;
use serde_json::{Value, json};

use crate::bridge::{Bridge, Done, Tag};
use crate::icons as own_icons;
use crate::model::{Profile, Profiles, Server, ServerLine, about, memory_max, ordered, parse_servers, round_clamp};
use crate::pages::{self, Page};
use crate::skin_view::{self, SkinView};
use crate::text;

/// What time is, for the line under a profile: the real one, or a fixed one for pictures.
#[derive(Copy, Clone)]
pub struct Calendar {
    pub now_ms: fn() -> i64,
    /// The local calendar day, in days since 1970, of a time in ms.
    pub day: fn(i64) -> i64,
}

impl Calendar {
    pub fn real() -> Calendar {
        fn now() -> i64 {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
        }
        Calendar { now_ms: now, day: launcher::system::local_day }
    }

    /// 2026-11-15 12:00 UTC (see `fake::FIXED_NOW_MS`), the days counted in UTC.
    pub fn fixed() -> Calendar {
        Calendar { now_ms: || crate::fake::FIXED_NOW_MS, day: |ms| ms.div_euclid(86_400_000) }
    }
}

/// Where the parts of Home stood in the last build, for tests.
#[derive(Clone, Debug)]
pub struct HomeIds {
    pub scroll: Id,
    pub page: Id,
    pub columns: Id,
    pub profiles: Id,
    pub server: Id,
    pub profiles_header: Id,
    pub server_header: Id,
    pub rows: Vec<RowIds>,
    pub back: Id,
    pub settings_name: Id,
    pub open_on: Id,
    pub address: Id,
    pub memory: Id,
    pub java: Id,
    pub open_folder: Id,
    pub note: Id,
    pub server_row: Id,
    pub server_play: Id,
    /// Each server line of Home, in the order of the list (`profile` holds the server's id).
    pub server_rows: Vec<RowIds>,
    pub new_profile: Id,
    pub new_server: Id,
    pub s_memory: Id,
    pub s_java: Id,
    pub s_view: Id,
    pub s_sim: Id,
    pub s_players: Id,
    pub s_port: Id,
    pub s_whitelist: Id,
    pub s_motd: Id,
    pub s_open_folder: Id,
    pub server_note: Id,
}

impl Default for HomeIds {
    fn default() -> HomeIds {
        let o = Id::ROOT;
        HomeIds {
            scroll: o,
            page: o,
            columns: o,
            profiles: o,
            server: o,
            profiles_header: o,
            server_header: o,
            back: o,
            settings_name: o,
            open_on: o,
            address: o,
            memory: o,
            java: o,
            open_folder: o,
            note: o,
            server_row: o,
            server_play: o,
            server_rows: Vec::new(),
            new_profile: o,
            new_server: o,
            s_memory: o,
            s_java: o,
            s_view: o,
            s_sim: o,
            s_players: o,
            s_port: o,
            s_whitelist: o,
            s_motd: o,
            s_open_folder: o,
            server_note: o,
            rows: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RowIds {
    pub profile: String,
    pub row: Id,
    pub play: Id,
}

/// A launch this window follows (the page's `launching`).
#[derive(Clone, Debug)]
struct Launching {
    id: String,
    state: String,
    since: f64,
    over: bool,
}

#[derive(Clone, Debug, Default)]
struct ServerForm {
    memory: f64,
    java: String,
    view: f64,
    sim: f64,
    players: f64,
    port: f64,
    whitelist: bool,
    motd: String,
}

/// What a field held when it got the keyboard: it saves only when left with another value (the page's `change`).
#[derive(Clone, Debug, Default)]
struct Trackers {
    s_memory: Option<f64>,
    s_java: Option<String>,
    s_view: Option<f64>,
    s_sim: Option<f64>,
    s_players: Option<f64>,
    s_port: Option<f64>,
    s_motd: Option<String>,
}

/// True when the field was left this frame holding another value than the one it had when it got the keyboard.
pub(crate) fn left<T: PartialEq + Clone>(slot: &mut Option<T>, before: &T, now: &T, resp: &FieldResp) -> bool {
    if slot.is_none() && (resp.focused || resp.blurred) {
        *slot = Some(before.clone());
    }
    if resp.blurred {
        return slot.take().is_some_and(|v| &v != now);
    }
    false
}

/// What a failed answer says: the api's own sentence, or that the request failed.
fn said(d: &Done) -> String {
    d.body["error"].as_str().map(String::from).unwrap_or_else(|| text::FAILED.to_string())
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Want {
    Play(String),
    OpenProfile(String),
    /// Open a server's page: the id the api names it by (empty for the dev server).
    OpenServer(String),
    Back,
    ServerStart,
    ServerStop,
    ServerSave,
    ServerFolder,
    Mods(pages::add_mod::Ask),
    /// Accept the Minecraft EULA: the api writes it, on the person's press.
    Eula,
    OpenLink(String),
    Accounts,
    AccountUse(String),
    AccountAdd,
    AccountSignOut(String),
}

pub struct LauncherHome {
    bridge: Bridge,
    cal: Calendar,
    pub(crate) ids: HomeIds,
    pub(crate) profiles: Vec<Profile>,
    pub(crate) locked: bool,
    account: Option<String>,
    /// The account the api named, when it did: `account` says "No account" otherwise, for the bar.
    signed: Option<String>,
    /// The accounts the api listed, with the one in use; the skins read for them, and the menu drawn from both.
    accounts: Vec<(String, bool)>,
    skins: BTreeMap<String, Option<SkinView>>,
    skin_asked: BTreeSet<String>,
    menu: Arc<AccountMenu>,
    pub(crate) machine_mb: i64,
    /// What the PC uses now, in MiB.
    pub(crate) used_mb: i64,
    /// The server the page, the settings and the server requests are about, as the api last described it.
    server: Option<Server>,
    /// Which server that is: its id for the api, empty for the dev server.
    selected: String,
    /// Every server of Home's SERVERS section as the api listed them; None while it has not, or when it does not know
    /// the route (the section then shows the one server above, as the page did).
    servers: Option<Vec<ServerLine>>,
    launching: Option<Launching>,
    /// The pages above Home, the one shown last.
    stack: Vec<Page>,
    /// What each page keeps between frames.
    pub pages: pages::Pages,
    sform: ServerForm,
    stopping: bool,
    /// When the last stop was asked, on the poll's clock.
    stop_asked: f64,
    /// Why the last stop failed, while the server still runs.
    stop_refused: Option<String>,
    /// The api's reason for its last 503, while it may still be shown.
    unavailable: Option<String>,
    filled_for: Option<String>,
    status: String,
    server_note: String,
    scroll_y: f32,
    tracks: Trackers,
    wants: Vec<Want>,
    started: bool,
    pending_shown: bool,
    on_screen: bool,
    now: f64,
    next_server: f64,
    next_profiles: f64,
    next_launch: f64,
    /// The polls asked and not answered yet: the timer does not ask again over a slow answer.
    in_flight: Vec<Tag>,
}

impl LauncherHome {
    pub fn new(bridge: Bridge, cal: Calendar) -> LauncherHome {
        LauncherHome {
            bridge,
            cal,
            ids: HomeIds::default(),
            profiles: Vec::new(),
            locked: false,
            account: None,
            signed: None,
            accounts: Vec::new(),
            skins: BTreeMap::new(),
            skin_asked: BTreeSet::new(),
            menu: Arc::default(),
            machine_mb: 0,
            used_mb: 0,
            server: None,
            selected: String::new(),
            servers: None,
            launching: None,
            stack: Vec::new(),
            pages: pages::Pages::default(),
            sform: ServerForm::default(),
            stopping: false,
            stop_asked: 0.0,
            stop_refused: None,
            unavailable: None,
            filled_for: None,
            status: String::new(),
            server_note: String::new(),
            scroll_y: 0.0,
            tracks: Trackers::default(),
            wants: Vec::new(),
            started: false,
            pending_shown: false,
            on_screen: false,
            now: 0.0,
            next_server: 0.0,
            next_profiles: 0.0,
            next_launch: 0.0,
            in_flight: Vec::new(),
        }
    }

    /// Where the parts stood in the last build.
    pub fn ids(&self) -> &HomeIds {
        &self.ids
    }

    /// What the notes under the two parts say now (the profiles', the server's).
    pub fn notes(&self) -> (&str, &str) {
        (&self.status, &self.server_note)
    }

    /// The page shown above Home; None while Home itself is.
    pub fn page(&self) -> Option<&Page> {
        self.stack.last()
    }

    /// Pushes a page above the one shown, as a press on a line (or on a page's own action) does.
    pub fn open(&mut self, page: Page) {
        match &page {
            Page::Profile(id) => pages::profile::open(self, id),
            Page::AddMod(_) => pages::add_mod::opened(self),
            Page::Server | Page::Setup => {}
        }
        self.stack.push(page);
    }

    /// Leaves the page shown, as its back action does: says nothing more, and asks the profiles again.
    pub fn back(&mut self) {
        self.stack.pop();
        self.say("");
        self.load_profiles();
    }

    /// The id of the profile the page shown is about (its page, or the page that adds a mod to it).
    pub(crate) fn current(&self) -> Option<String> {
        match self.stack.last() {
            Some(Page::Profile(id) | Page::AddMod(id)) => Some(id.clone()),
            _ => None,
        }
    }

    /// The name of that profile, empty when it is gone.
    pub(crate) fn profile_name(&self) -> String {
        let id = self.current();
        self.profiles.iter().find(|p| Some(&p.id) == id.as_ref()).map(|p| p.name.clone()).unwrap_or_default()
    }

    /// The poll's clock, in seconds.
    pub(crate) fn now(&self) -> f64 {
        self.now
    }

    /// What the page that adds a mod asks of the api, done after the build like the rest.
    pub(crate) fn ask_mods(&mut self, ask: pages::add_mod::Ask) {
        self.wants.push(Want::Mods(ask));
    }

    /// What a page asks of Home: done after its build, as Home's own presses are.
    pub(crate) fn want(&mut self, want: Want) {
        self.wants.push(want);
    }

    /// The server as the api last described it; None until it has.
    pub(crate) fn server_data(&self) -> Option<&Server> {
        self.server.as_ref()
    }

    /// A start or a stop is asked and not over: the server takes no other.
    pub(crate) fn server_working(&self) -> bool {
        self.stopping || self.server.as_ref().is_some_and(Server::busy)
    }

    /// A profile's name by its id; the id itself when the profile is gone.
    pub(crate) fn profile_label(&self, id: &str) -> String {
        self.profiles.iter().find(|p| p.id == id).map_or_else(|| id.to_string(), |p| p.name.clone())
    }

    /// Asks the skin of an account, once for a name: a face does not change while the launcher runs. The name goes into the
    /// request's path, so only what Mojang allows in one is asked.
    fn ask_skin(&mut self, name: &str) {
        if name.len() <= 16 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') && self.skin_asked.insert(name.to_string()) {
            self.send(Tag::Skin(name.to_string()), "GET", &format!("/api/skin?name={name}"), None);
        }
    }

    /// The menu of the accounts: the ones the api listed, or the one the profiles named while it has not listed.
    fn rebuild_menu(&mut self) {
        let listed: Vec<(String, bool)> = if self.accounts.is_empty() { self.signed.iter().map(|a| (a.clone(), true)).collect() } else { self.accounts.clone() };
        let accounts: Vec<AccountEntry> = listed
            .iter()
            .map(|(name, in_use)| AccountEntry { name: name.clone(), in_use: *in_use, face: self.skins.get(name).and_then(|s| s.as_ref()).map(|s| s.face.clone()) })
            .collect();
        let figure = listed.iter().find(|(_, in_use)| *in_use).and_then(|(name, _)| self.skins.get(name)).and_then(|s| s.as_ref()).map(|s| s.figure.clone());
        let menu = AccountMenu { accounts, figure };
        if *self.menu != menu {
            self.menu = Arc::new(menu);
        }
    }

    fn mem_max(&self) -> f64 {
        memory_max(self.machine_mb)
    }

    // ---------- what the api said ----------

    pub(crate) fn send(&mut self, tag: Tag, method: &str, path: &str, body: Option<Value>) {
        if matches!(tag, Tag::Profiles | Tag::Server | Tag::Servers | Tag::Launch) {
            self.in_flight.push(tag.clone());
        }
        self.bridge.send(tag, method, path, body);
    }

    /// A request about the selected server: the dev server's own route as it is, another server's with its id (in the
    /// query of a GET, in the body of a POST), so the api tells them apart.
    fn send_server(&mut self, tag: Tag, method: &str, path: &str, body: Option<Value>) {
        if self.selected.is_empty() {
            return self.send(tag, method, path, body);
        }
        if method == "GET" {
            let id: String = self.selected.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
            return self.send(tag, method, &format!("{path}?id={id}"), body);
        }
        let mut body = body.unwrap_or_else(|| json!({}));
        body["id"] = json!(self.selected);
        self.send(tag, method, path, Some(body));
    }

    /// Makes `id` the server the page and the requests are about. What was known of the one before is dropped: the
    /// next answer fills the page again.
    fn select(&mut self, id: &str) {
        if self.selected == id {
            return;
        }
        self.selected = id.to_string();
        self.server = None;
        self.stopping = false;
        self.stop_refused = None;
        self.filled_for = None;
        self.server_note.clear();
        self.next_server = self.now;
    }

    /// The servers Home lists: the api's list, else the one server it described (the dev server of an api with no list).
    fn server_lines(&self) -> Vec<ServerLine> {
        if let Some(list) = &self.servers {
            return list.clone();
        }
        self.server
            .iter()
            .map(|s| ServerLine {
                id: self.selected.clone(),
                name: s.name.clone(),
                state: s.state.clone(),
                eula: s.eula,
                players: None,
                port: s.props.as_ref().map_or(25565, |p| p.port),
                made_from: s.made.as_ref().map(|m| m.profile.clone()),
            })
            .collect()
    }

    /// True while a poll of this kind is asked and not answered: the timer waits for it rather than asking again.
    fn asking(&self, tag: &Tag) -> bool {
        self.in_flight.contains(tag)
    }

    fn load_profiles(&mut self) {
        self.next_profiles = self.now + 10.0;
        self.send(Tag::Profiles, "GET", "/api/profiles", None);
    }

    fn load_server(&mut self) {
        self.next_server = self.now + 3.0;
        self.send(Tag::Servers, "GET", "/api/servers", None);
        self.send_server(Tag::Server, "GET", "/api/server", None);
    }

    pub(crate) fn say(&mut self, sentence: &str) {
        self.status = sentence.to_string();
    }

    fn say_server(&mut self, sentence: &str) {
        self.server_note = sentence.to_string();
    }

    fn done(&mut self, d: Done) {
        if let Some(at) = self.in_flight.iter().position(|t| *t == d.tag) {
            self.in_flight.remove(at);
        }
        let ok = d.status == 200;
        match d.tag {
            // `load`.
            Tag::Profiles => {
                if !ok {
                    // A departure: the page's first load says nothing.
                    let why = self.failure(&d);
                    self.say(&why);
                    return;
                }
                self.available();
                let p = Profiles::parse(&d.body);
                self.profiles = p.profiles;
                self.locked = p.locked;
                self.machine_mb = p.machine_mb;
                self.used_mb = p.used_mb;
                self.signed = p.account.clone();
                if let Some(name) = p.account.clone() {
                    // The bar's face: read once, with the first answer that names the account.
                    self.ask_skin(&name);
                }
                self.account = Some(p.account.unwrap_or_else(|| text::NO_ACCOUNT.to_string()));
                self.rebuild_menu();
                if matches!(self.stack.last(), Some(Page::Profile(_))) {
                    self.lock();
                }
            }
            // `loadServer`.
            Tag::Server => {
                if !ok {
                    // The page says nothing; the api's reason when it could not read the pack.
                    if d.status == 503 {
                        let why = self.failure(&d);
                        self.say_server(&why);
                    }
                    return;
                }
                // An answer about a server that is no longer the selected one is not for the page.
                if d.body["id"].as_str().unwrap_or_default() != self.selected {
                    return;
                }
                self.available();
                let server = Server::parse(&d.body);
                if matches!(server.state.as_str(), "stopped" | "failed") {
                    self.stopping = false;
                    self.stop_refused = None;
                }
                // A stop that was taken but left the server running does not hold the button for ever.
                if self.stopping && server.state == "running" && self.now - self.stop_asked >= STOP_WAIT {
                    self.stopping = false;
                }
                let key = format!("{} {}", server.state, server.props.is_some());
                self.server = Some(server);
                if self.filled_for.as_deref() != Some(key.as_str()) {
                    self.filled_for = Some(key);
                    self.fill_server();
                }
                self.lock_server();
            }
            // Home's list of servers: not an api that lists them leaves the one server's line.
            Tag::Servers => {
                if !ok {
                    self.servers = None;
                    if d.status == 503 {
                        let why = self.failure(&d);
                        self.say_server(&why);
                    }
                    return;
                }
                self.available();
                self.servers = parse_servers(&d.body);
            }
            // play's follow.
            Tag::Launch => {
                if !ok {
                    return;
                }
                if let Some(l) = self.launching.as_mut() {
                    l.state = d.body["state"].as_str().unwrap_or_default().to_string();
                    l.over = matches!(l.state.as_str(), "running" | "prepareFailed" | "stopped" | "failed");
                }
            }
            // play.
            Tag::Play(_) => {
                if ok {
                    self.next_launch = self.now + 1.0;
                } else {
                    self.launching = None;
                    let why = self.failure(&d);
                    self.say(&why);
                    self.load_profiles();
                }
            }
            // `startServer`.
            Tag::ServerStart => {
                if !ok {
                    self.say_server(&said(&d));
                }
                self.load_server();
            }
            // `stopServer`, with the server's own reason (the page says only that it failed).
            Tag::ServerStop => {
                if !ok {
                    self.stopping = false;
                    let why = said(&d);
                    self.say_server(&why);
                    self.stop_refused = Some(why);
                }
                self.load_server();
            }
            // The profile page's own requests.
            Tag::Instance(_) | Tag::SaveProfile | Tag::Folder | Tag::ModAction(_) | Tag::OpenUrl => pages::profile::done(self, d),
            // saveServer.
            Tag::SaveServer => match d.status {
                200 => {
                    self.say_server(text::SAVED);
                }
                400 => self.say_server(text::REFUSED),
                409 => self.say_server(text::SERVER_LOCKED),
                _ => {
                    let why = self.failure(&d);
                    self.say_server(&why);
                }
            },
            Tag::Mods(ref ask) => pages::add_mod::done(self, ask.clone(), &d),
            Tag::ServerFolder => {
                if !ok {
                    let why = self.failure(&d);
                    self.say_server(&why);
                }
            }
            // Accept: the api wrote the line, or says why not; either way the server is read again.
            Tag::Eula => {
                if !ok {
                    self.say_server(&said(&d));
                }
                self.load_server();
            }
            Tag::OpenLink => {
                if !ok {
                    self.say_server(&said(&d));
                }
            }
            Tag::Accounts => {
                if !ok {
                    return;
                }
                self.accounts = crate::model::parse_accounts(&d.body);
                let names: Vec<String> = self.accounts.iter().map(|(n, _)| n.clone()).collect();
                for name in names {
                    self.ask_skin(&name);
                }
                self.rebuild_menu();
            }
            Tag::Skin(name) => {
                if ok {
                    self.skins.insert(name, skin_view::read(&d.body));
                    self.rebuild_menu();
                }
            }
            // A change of account: the profiles and the list are read again, whose answers redraw the bar.
            Tag::AccountUse | Tag::AccountAdd | Tag::AccountSignOut => {
                if !ok {
                    // "Not built": the api says what waits for the Microsoft sign-in.
                    let why = if d.status == 501 { said(&d) } else { self.failure(&d) };
                    self.say(&why);
                }
                self.load_profiles();
                self.send(Tag::Accounts, "GET", "/api/accounts", None);
            }
            Tag::Java | Tag::JavaStart | Tag::JavaStop => pages::setup::done(self, d),
            // The first start: no Java 25 puts the set-up before Home (a person who got somewhere meanwhile is left
            // where they are). An answer that is not understood leaves Home: the set-up is reachable by what it needs.
            Tag::JavaFound => {
                if d.status == 200 && d.body["found"] == json!(false) && self.stack.is_empty() {
                    self.open(Page::Setup);
                }
            }
        }
    }

    /// What a failed request says: the api's reason when it answered 503 (the pack's scripts/common.ps1 could not
    /// be read), which is kept to be taken back once the api answers again; that it failed otherwise.
    pub(crate) fn failure(&mut self, d: &Done) -> String {
        if d.status != 503 {
            return text::FAILED.to_string();
        }
        let why = said(d);
        self.unavailable = Some(why.clone());
        why
    }

    /// The api answers again: a 503 reason still shown is no longer so.
    fn available(&mut self) {
        let Some(why) = self.unavailable.take() else { return };
        if self.status == why {
            self.say("");
        }
        if self.server_note == why {
            self.say_server("");
        }
    }

    /// `lock`.
    pub(crate) fn lock(&mut self) {
        if self.locked {
            self.say(text::LOCKED);
        } else if self.status == text::LOCKED {
            self.say("");
        }
    }

    /// `lockServer`.
    fn lock_server(&mut self) {
        let Some(server) = &self.server else { return };
        if server.busy() {
            // Why the last stop failed stays in place of the lock's sentence: it is what the user needs then.
            if self.stop_refused.as_deref() != Some(self.server_note.as_str()) {
                self.say_server(text::SERVER_LOCKED);
            }
        } else if server.props.is_none() {
            self.say_server(text::FIRST_START);
        } else if self.server_note == text::SERVER_LOCKED || self.server_note == text::FIRST_START {
            self.say_server("");
        }
    }

    /// `fillServer`.
    fn fill_server(&mut self) {
        let Some(s) = &self.server else { return };
        let p = s.props.clone();
        self.sform = ServerForm {
            memory: (s.memory_mb as f64 / 1024.0).round(),
            java: s.java_args.clone(),
            view: p.as_ref().map_or(0.0, |p| p.view as f64),
            sim: p.as_ref().map_or(0.0, |p| p.sim as f64),
            players: p.as_ref().map_or(0.0, |p| p.players as f64),
            port: p.as_ref().map_or(0.0, |p| p.port as f64),
            whitelist: p.as_ref().is_some_and(|p| p.whitelist),
            motd: p.map(|p| p.motd).unwrap_or_default(),
        };
    }

    /// True while this profile cannot be played: it runs, or a launch is under way (one at a time).
    pub(crate) fn play_blocked(&self, id: &str) -> bool {
        self.launching.as_ref().is_some_and(|l| !l.over) || self.profiles.iter().any(|p| p.id == id && p.running)
    }

    /// play.
    pub(crate) fn play(&mut self, id: String) {
        self.launching = Some(Launching { id: id.clone(), state: "starting".into(), since: self.now, over: false });
        // The page awaits the POST before it starts its 1 s follow (`play`): a GET sent now would race it and could read the previous launch.
        self.next_launch = f64::INFINITY;
        self.send(Tag::Play(id.clone()), "POST", &format!("/api/launch/{id}"), None);
    }

    /// `saveServer`.
    fn save_server(&mut self) {
        let f = self.sform.clone();
        let mut body = json!({ "memoryMb": (f.memory * 1024.0).round() as i64, "javaArgs": f.java.trim() });
        if self.server.as_ref().is_some_and(|s| s.props.is_some()) {
            body["props"] = json!({
                "motd": f.motd.trim(),
                "whitelist": f.whitelist,
                "viewDistance": f.view as i64,
                "simulationDistance": f.sim as i64,
                "maxPlayers": f.players as i64,
                "port": f.port as i64,
            });
        }
        self.send_server(Tag::SaveServer, "POST", "/api/server/settings", Some(body));
    }

    // ---------- the page ----------

    fn lines_of(&self, p: &Profile, clock: f64) -> (String, bool) {
        let followed = self.launching.as_ref().filter(|l| l.id == p.id);
        let now = match followed {
            Some(l) => text::launch(&l.state, ((clock - l.since) + 0.5).floor() as i64),
            None if p.running => text::launch("running", 0),
            None => None,
        };
        match now {
            Some(sentence) => (sentence, true),
            None => {
                let days = (self.cal.day)(p.last_played) - (self.cal.day)((self.cal.now_ms)());
                (about(p, days), false)
            }
        }
    }

    /// A server's line under its name (board A1), and whether it says what is happening now rather than where it stands.
    fn line_of_server(&self, l: &ServerLine) -> (String, bool) {
        let stopping = self.stopping && l.request_id() == self.selected;
        let state = if stopping { "stopping" } else { l.state.as_str() };
        let made = l.made_from.as_ref().map(|p| self.profile_label(p));
        let mut line = text::server_line(state, l.port, l.players, made.as_deref()).unwrap_or_default();
        // A server made from a profile says so; its page says the EULA, which the dev server's line has to.
        if state == "stopped" && !l.eula && made.is_none() {
            line = text::NO_EULA.to_string();
        }
        (line, !matches!(state, "stopped" | "running"))
    }

    fn col() -> Style {
        Style { column: true, min_w: Dim::Px(0.0), ..Style::default() }
    }

    /// What every page above Home is drawn in: a column `max_w` wide at most, centred, with the title line over the page
    /// (the back action, `title`, `end` at the right end), then what `body` builds. The back action asks to leave the page.
    pub(crate) fn page_frame(&mut self, ui: &mut Ui, max_w: f32, title: &str, end: impl FnOnce(&mut Ui), body: impl FnOnce(&mut LauncherHome, &mut Ui)) {
        let style = Style { column: true, w: Dim::Pct(100.0), max_w: Dim::Px(max_w), margin: Margin::horizontal_auto(), ..Style::default() };
        ui.node("pageColumn", style, |ui| {
            let head = ui.page_head("head", own_icons::back(), title, end);
            ui.title(head.back.id, text::BACK);
            self.ids.back = head.back.id;
            if head.back.clicked {
                self.wants.push(Want::Back);
            }
            body(self, ui);
        });
    }

    /// Home itself: the profiles and the servers, side by side up to the cap, stacked in a narrow window.
    fn home_page(&mut self, ui: &mut Ui, stacked: bool) {
        let columns = if stacked {
            Style { column: true, gap: Vec2::new(0.0, GAP_WIDE), w: Dim::Pct(100.0), ..Style::default() }
        } else {
            Style {
                display: Display::Grid,
                cols: Tracks::new(&[Track::MinMaxFr0(1.0), Track::MinMaxFr0(1.0)]),
                gap: Vec2::new(COLUMNS_GAP, 0.0),
                align_items: Align::Start,
                w: Dim::Pct(100.0),
                max_w: Dim::Px(COLUMNS_MAX),
                margin: Margin::horizontal_auto(),
                ..Style::default()
            }
        };
        self.ids.columns = ui.node("columns", columns, |ui| {
            self.ids.profiles = ui.node("profilesPart", Self::col(), |ui| self.profiles_part(ui));
            self.ids.server = ui.node("serverPart", Self::col(), |ui| self.server_part(ui));
        });
    }

    /// A column `w` wide at most that holds one half of a page's two columns.
    pub(crate) fn half(&mut self, ui: &mut Ui, w: f32, body: impl FnOnce(&mut LauncherHome, &mut Ui)) {
        ui.node("half", Style { column: true, w: Dim::Pct(100.0), max_w: Dim::Px(w), min_w: Dim::Px(0.0), ..Style::default() }, |ui| body(self, ui));
    }

    /// A `.profile` row's look: `inset` all round, pulled out by as much on each side so that its
    /// words line up with the column while its lit box overhangs it.
    fn row_style() -> Style {
        Style {
            align_items: Align::Center,
            pad: Edges::vh(INSET, INSET),
            margin: Margin { left: Side::Px(-INSET), right: Side::Px(-INSET), ..Margin::default() },
            ..Style::default()
        }
    }

    fn who(ui: &mut Ui, name: &str, line: &str, now: bool) {
        ui.node("who", Style { column: true, grow: 1.0, min_w: Dim::Px(0.0), pad: Edges::new(0.0, GAP_WIDE, 0.0, 0.0), ..Style::default() }, |ui| {
            ui.ellipsis(name, TextLook { size: 13.0, line: 19.5, ..TextLook::body(Tone::Text) });
            ui.paragraph(line, TextLook::small(if now { Tone::Text } else { Tone::Muted }));
        });
    }

    /// A label above its field (`label, .label`): muted, `gap-tight` over it.
    fn label(ui: &mut Ui, words: &str) {
        ui.node("label", Style { margin: Margin { bottom: Side::Px(GAP_TIGHT), ..Margin::default() }, ..Style::default() }, |ui| ui.text(words, TextLook::body(Tone::Muted)));
    }

    fn field(ui: &mut Ui, key: &str, words: &str, body: impl FnOnce(&mut Ui)) {
        ui.node(key, Style { column: true, ..Style::default() }, |ui| {
            Self::label(ui, words);
            body(ui);
        });
    }

    fn profiles_part(&mut self, ui: &mut Ui) {
        let clock = ui.clock();
        let lit = ui.theme().tint();
        let busy = self.launching.as_ref().is_some_and(|l| !l.over);
        let mut new = (Id::ROOT, false);
        let section = ui.section(
            "profiles",
            text::PROFILES_GROUP,
            |ui| {
                let r = ui.action("new", text::NEW_PROFILE, ActionLook::Underlined);
                new = (r.id, r.clicked);
            },
            |ui| {
                for p in ordered(&self.profiles) {
                    let (line, now) = self.lines_of(&p, clock);
                    let mut row_ids = RowIds { profile: p.id.clone(), row: Id::ROOT, play: Id::ROOT };
                    let row = ui.hover_action(&format!("row-{}", p.id), Self::row_style(), lit, |ui| {
                        Self::who(ui, &p.name, &line, now);
                        let play = ui.icon_action("play", &icons().play, IconLook::Filled, IconOpts { disabled: p.running || busy, ..IconOpts::default() });
                        ui.title(play.id, &text::play(&p.name));
                        row_ids.play = play.id;
                        if play.clicked {
                            self.wants.push(Want::Play(p.id.clone()));
                        }
                    });
                    row_ids.row = row.id;
                    if row.clicked {
                        self.wants.push(Want::OpenProfile(p.id.clone()));
                    }
                    self.ids.rows.push(row_ids);
                }
            },
        );
        self.ids.profiles_header = section;
        self.ids.new_profile = new.0;
        if !self.status.is_empty() {
            let note = self.status.clone();
            self.ids.note = ui.node("note", Style { margin: Margin { top: Side::Px(GAP_WIDE), ..Margin::default() }, ..Style::default() }, |ui| ui.paragraph(&note, TextLook::small(Tone::Muted)));
        }
    }

    fn folder_row(ui: &mut Ui, path: &str) -> mere_ui::Resp {
        let mut resp = None;
        ui.node("folder", Style { align_items: Align::Center, gap: Vec2::new(GAP, 0.0), ..Style::default() }, |ui| {
            ui.node("path", Style { grow: 1.0, min_w: Dim::Px(0.0), ..Style::default() }, |ui| ui.ellipsis(path, TextLook::body(Tone::Text)));
            resp = Some(ui.action_with("open", text::OPEN, ActionLook::Underlined, ActionOpts::default()));
        });
        resp.expect("the row was built")
    }

    /// The server's part (the second `section.col`): one line for each server, its name, where it stands and its start
    /// or stop button; a press on the line opens its page.
    fn server_part(&mut self, ui: &mut Ui) {
        let lit = ui.theme().tint();
        let lines = self.server_lines();
        let mut new = (Id::ROOT, false);
        self.ids.server_rows.clear();
        let section = ui.section(
            "servers",
            text::SERVER_GROUP,
            |ui| {
                let r = ui.action("new", text::NEW_SERVER, ActionLook::Underlined);
                new = (r.id, r.clicked);
            },
            |ui| {
                // Before the api has answered, the one line a server has: its name and nothing to press.
                if lines.is_empty() {
                    ui.node("serverRow", Self::row_style(), |ui| Self::who(ui, text::SERVER_NAME, "", false));
                }
                for l in &lines {
                    let (words, now) = self.line_of_server(l);
                    let (running, stopping) = (l.state == "running", self.stopping && l.request_id() == self.selected);
                    let off = stopping || (!running && l.busy());
                    let name = if l.name.is_empty() { text::SERVER_NAME } else { l.name.as_str() };
                    let mut ids = RowIds { profile: l.id.clone(), row: Id::ROOT, play: Id::ROOT };
                    let row = ui.hover_action(&format!("server-{}", l.id), Self::row_style(), lit, |ui| {
                        Self::who(ui, name, &words, now);
                        // Start once at a time; stop only a server that is ready to hear it (`renderServer`).
                        let r = if running {
                            let r = ui.icon_action("stop", &icons().stop, IconLook::Square, IconOpts { disabled: off, ..IconOpts::default() });
                            ui.title(r.id, text::STOP_SERVER);
                            if r.clicked {
                                self.select(&l.request_id());
                                self.wants.push(Want::ServerStop);
                            }
                            r
                        } else {
                            let r = ui.icon_action("start", &icons().play, IconLook::Square, IconOpts { disabled: off, ..IconOpts::default() });
                            ui.title(r.id, text::START_SERVER);
                            if r.clicked {
                                self.select(&l.request_id());
                                self.wants.push(Want::ServerStart);
                            }
                            r
                        };
                        ids.play = r.id;
                    });
                    ids.row = row.id;
                    if row.clicked {
                        self.wants.push(Want::OpenServer(l.request_id()));
                    }
                    if self.ids.server_rows.is_empty() {
                        self.ids.server_row = ids.row;
                        self.ids.server_play = ids.play;
                    }
                    self.ids.server_rows.push(ids);
                }
            },
        );
        self.ids.server_header = section;
        self.ids.new_server = new.0;
        self.server_note_under(ui);
    }

    /// What the server's last request said, under its part.
    pub(crate) fn server_note_under(&mut self, ui: &mut Ui) {
        if !self.server_note.is_empty() {
            let note = self.server_note.clone();
            self.ids.server_note = ui.node("note", Style { margin: Margin { top: Side::Px(GAP_WIDE), ..Margin::default() }, ..Style::default() }, |ui| ui.paragraph(&note, TextLook::small(Tone::Muted)));
        }
    }

    /// The server's settings, as Home showed them in place: the form of the server's page until its screen replaces it,
    /// and the note under it.
    pub(crate) fn server_form(&mut self, ui: &mut Ui) {
        if self.server.is_some() {
            self.server_fields(ui);
        }
        self.server_note_under(ui);
    }

    /// `#server-fields`.
    fn server_fields(&mut self, ui: &mut Ui) {
        let max = self.mem_max();
        let (busy, props) = self.server.as_ref().map_or((false, false), |s| (s.busy(), s.props.is_some()));
        let off = busy || !props;
        let dir = self.server.as_ref().map(|s| s.dir.clone()).unwrap_or_default();
        let fields = Style { column: true, gap: Vec2::new(0.0, GAP_WIDE), pad: Edges::new(GAP_WIDE, 0.0, 0.0, 0.0), ..Style::default() };
        let half = Style { column: true, grow: 1.0, shrink: 1.0, basis: Dim::Px(0.0), min_w: Dim::Px(0.0), ..Style::default() };
        // The pairs keep the fields' own vertical gap across, so the fields stand on one even grid.
        let pair = Style { gap: Vec2::new(GAP_WIDE, 0.0), ..Style::default() };
        ui.node("fields", fields, |ui| {
            Self::field(ui, "memory", text::MEMORY, |ui| {
                ui.node("memory", Style { align_items: Align::Center, gap: Vec2::new(GAP, 0.0), ..Style::default() }, |ui| {
                    let before = self.sform.memory;
                    let spec = NumberSpec { min: 2.0, max, step: 1.0 };
                    let r = ui.number_with("field", &mut self.sform.memory, spec, NumberOpts { width: Dim::Px(SHORT_W), right: true, pad_v: PAD_V, disabled: busy });
                    self.ids.s_memory = r.resp.id;
                    if r.blurred {
                        self.sform.memory = round_clamp(self.sform.memory, 2.0, max);
                    }
                    if left(&mut self.tracks.s_memory, &before, &self.sform.memory, &r) {
                        self.wants.push(Want::ServerSave);
                    }
                    ui.text(text::GIGABYTES, TextLook::body(Tone::Muted));
                });
            });
            Self::field(ui, "java", text::JAVA_ARGS, |ui| {
                let before = self.sform.java.clone();
                let opts = FieldOpts { width: Dim::Pct(100.0), height: Dim::Px(JAVA_H), disabled: busy, ..FieldOpts::default() };
                let r = ui.text_field("field", &mut self.sform.java, opts);
                self.ids.s_java = r.resp.id;
                if left(&mut self.tracks.s_java, &before, &self.sform.java, &r) {
                    self.wants.push(Want::ServerSave);
                }
            });
            // One number of a pair: its words, its range, and where it is kept.
            macro_rules! number {
                ($ui:expr, $key:expr, $words:expr, $min:expr, $max:expr, $value:ident, $track:ident, $id:ident) => {
                    $ui.node($key, half, |ui| {
                        Self::label(ui, $words);
                        let before = self.sform.$value;
                        let spec = NumberSpec { min: $min, max: $max, step: 1.0 };
                        let r = ui.number_with("field", &mut self.sform.$value, spec, NumberOpts { width: Dim::Pct(100.0), right: false, pad_v: PAD_V, disabled: off });
                        self.ids.$id = r.resp.id;
                        if r.blurred {
                            self.sform.$value = round_clamp(self.sform.$value, $min, $max);
                        }
                        if left(&mut self.tracks.$track, &before, &self.sform.$value, &r) {
                            self.wants.push(Want::ServerSave);
                        }
                    });
                };
            }
            ui.node("pair1", pair, |ui| {
                number!(ui, "view", text::VIEW_DISTANCE, 3.0, 32.0, view, s_view, s_view);
                number!(ui, "sim", text::SIMULATION_DISTANCE, 5.0, 32.0, sim, s_sim, s_sim);
            });
            ui.node("pair2", pair, |ui| {
                number!(ui, "players", text::MAX_PLAYERS, 1.0, 100.0, players, s_players, s_players);
                number!(ui, "port", text::PORT, 1024.0, 65535.0, port, s_port, s_port);
            });
            // `.toggle`.
            ui.node("toggle", Style { align_items: Align::Center, gap: Vec2::new(GAP, 0.0), ..Style::default() }, |ui| {
                ui.node("label", Style { grow: 1.0, ..Style::default() }, |ui| ui.text(text::WHITELIST, TextLook::body(Tone::Muted)));
                let opts = ToggleOpts { min_w: Dim::Px(SHORT_W), pad: Edges::vh(PAD_V, FRAME + INSET), disabled: off };
                let r = ui.toggle_with("whitelist", &mut self.sform.whitelist, text::ON, text::OFF, opts);
                self.ids.s_whitelist = r.id;
                if r.clicked {
                    self.wants.push(Want::ServerSave);
                }
            });
            Self::field(ui, "motd", text::MOTD, |ui| {
                let before = self.sform.motd.clone();
                let opts = FieldOpts { width: Dim::Pct(100.0), max_chars: 59, disabled: off, ..FieldOpts::default() };
                let r = ui.text_field("field", &mut self.sform.motd, opts);
                self.ids.s_motd = r.resp.id;
                if left(&mut self.tracks.s_motd, &before, &self.sform.motd, &r) {
                    self.wants.push(Want::ServerSave);
                }
            });
            Self::field(ui, "folder", text::SERVER_FOLDER, |ui| {
                let r = Self::folder_row(ui, &dir);
                self.ids.s_open_folder = r.id;
                if r.clicked {
                    self.wants.push(Want::ServerFolder);
                }
            });
        });
    }
}

impl Home for LauncherHome {
    fn build(&mut self, ui: &mut Ui, size: Vec2) {
        self.ids = HomeIds::default();
        // The page's `#launcher`: a column that scrolls when it is taller than the window.
        let scroll = Style {
            column: true,
            grow: 1.0,
            shrink: 1.0,
            basis: Dim::Px(0.0),
            min_h: Dim::Px(0.0),
            overflow: Overflow::ScrollY,
            ..Style::default()
        };
        let mut y = self.scroll_y;
        let r = ui.scroll("home", &mut y, scroll, mere_ui::ScrollOpts { fade: 0.0 }, |ui| {
            // `.page` without its cap and its 860 padding change; the top is the preview's.
            let page = Style { column: true, pad: PAGE.pad, ..Style::default() };
            self.ids.page = ui.node("homePage", page, |ui| match self.stack.last().cloned() {
                None => self.home_page(ui, size.x < STACK_BELOW),
                Some(Page::Profile(_)) => pages::profile::build(self, ui),
                Some(Page::AddMod(_)) => pages::add_mod::build(self, ui),
                Some(Page::Server) => pages::server::build(self, ui),
                Some(Page::Setup) => pages::setup::build(self, ui),
            });
        });
        self.ids.scroll = r.id;
        self.scroll_y = y;
    }

    fn act(&mut self) -> bool {
        let paged = pages::profile::act(self);
        if self.wants.is_empty() {
            return paged;
        }
        let mut wants = std::mem::take(&mut self.wants);
        // The same thing asked twice in one build is asked once.
        let mut seen: Vec<Want> = Vec::new();
        wants.retain(|w| {
            let dup = seen.contains(w);
            seen.push(w.clone());
            !dup
        });
        // A save goes before any navigation: the page's change event fires on mousedown, before the click, and a press and release in one frame would otherwise let Back drop the edit.
        wants.sort_by_key(|w| !matches!(w, Want::ServerSave));
        for want in wants {
            match want {
                // play.
                Want::Play(id) => self.play(id),
                Want::OpenProfile(id) => self.open(Page::Profile(id)),
                Want::OpenServer(id) => {
                    self.select(&id);
                    self.open(Page::Server);
                }
                // The page's `back` listener.
                Want::Back => self.back(),
                Want::ServerStart => self.send_server(Tag::ServerStart, "POST", "/api/server/start", None),
                // stopServer.
                Want::ServerStop => {
                    self.stopping = true;
                    self.stop_asked = self.now;
                    self.stop_refused = None;
                    self.send_server(Tag::ServerStop, "POST", "/api/server/stop", None);
                }
                Want::ServerSave => self.save_server(),
                Want::ServerFolder => self.send_server(Tag::ServerFolder, "POST", "/api/server/folder", None),
                Want::Mods(ask) => pages::add_mod::act(self, ask),
                Want::Eula => self.send_server(Tag::Eula, "POST", "/api/server/eula", None),
                Want::OpenLink(url) => self.send(Tag::OpenLink, "POST", "/api/open-link", Some(json!({ "url": url }))),
                Want::Accounts => self.send(Tag::Accounts, "GET", "/api/accounts", None),
                Want::AccountUse(name) => self.send(Tag::AccountUse, "POST", "/api/accounts/use", Some(json!({ "name": name }))),
                Want::AccountAdd => self.send(Tag::AccountAdd, "POST", "/api/accounts/add", None),
                Want::AccountSignOut(name) => self.send(Tag::AccountSignOut, "POST", "/api/accounts/sign-out", Some(json!({ "name": name }))),
            }
        }
        true
    }

    fn poll(&mut self, now: f64, on_screen: bool) -> bool {
        self.now = now;
        self.on_screen = on_screen;
        let mut wanted = false;
        // The first look at both, whatever tool shows (the page's `Promise.all` of `load`, `loadServer` and `loadModes`).
        if !self.started {
            self.started = true;
            self.send(Tag::JavaFound, "GET", "/api/java/found", None);
            self.load_profiles();
            self.load_server();
            wanted = true;
        }
        if std::mem::take(&mut self.pending_shown) {
            self.load_profiles();
            self.load_server();
            wanted = true;
        }
        for d in self.bridge.take() {
            self.done(d);
            wanted = true;
        }
        if on_screen {
            if now >= self.next_server && !self.asking(&Tag::Server) {
                self.load_server();
                wanted = true;
            }
            wanted |= pages::setup::poll(self, now);
            wanted |= pages::add_mod::poll(self, now);
            // A stand-in for the page's reload on window focus (its `focus` listener): mere_base has no such event.
            if now >= self.next_profiles && !self.asking(&Tag::Profiles) {
                self.load_profiles();
                wanted = true;
            }
        }
        // The launch is followed whatever the tool (`play`'s follow).
        if self.launching.as_ref().is_some_and(|l| !l.over) && now >= self.next_launch && !self.asking(&Tag::Launch) {
            self.next_launch = now + 1.0;
            self.send(Tag::Launch, "GET", "/api/launch", None);
            wanted = true;
        }
        // Answers that came back at once (Inline) are taken now.
        for d in self.bridge.take() {
            self.done(d);
            wanted = true;
        }
        wanted
    }

    fn wake_in(&self, now: f64) -> Option<f32> {
        if self.bridge.busy() {
            return Some(0.05);
        }
        let mut due: Vec<f64> = Vec::new();
        if self.on_screen {
            due.push(self.next_server);
            due.push(self.next_profiles);
            due.extend(pages::setup::due(self));
            due.extend(pages::add_mod::due(self));
        }
        if self.launching.as_ref().is_some_and(|l| !l.over) {
            if self.next_launch.is_finite() {
                due.push(self.next_launch);
            }
        }
        due.into_iter().reduce(f64::min).map(|d| (d - now).max(0.0) as f32)
    }

    fn shown(&mut self) {
        self.pending_shown = true;
    }

    fn account(&self) -> Option<String> {
        self.account.clone()
    }

    /// The set-up stands before Home: no tools and no account in the bar (boards E1 and E2).
    fn bare_bar(&self) -> bool {
        self.page() == Some(&Page::Setup)
    }

    fn account_menu(&self) -> Arc<AccountMenu> {
        self.menu.clone()
    }

    fn account_ask(&mut self, ask: AccountAsk) {
        self.wants.push(match ask {
            AccountAsk::Opened => Want::Accounts,
            AccountAsk::Use(name) => Want::AccountUse(name),
            AccountAsk::Add => Want::AccountAdd,
            AccountAsk::SignOut(name) => Want::AccountSignOut(name),
        });
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
