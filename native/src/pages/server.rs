//! A server's page (board C1): its name with the Start action at the right, the Minecraft EULA to accept before the first
//! start, and what the server was made from (profile, world, folder). Once the EULA is accepted the page goes on with the
//! settings Home used to open in place (`server_form`), until their screen replaces them.
//!
//! The page asks the api and nothing else: Accept is a request to the api, which writes the line in the server's
//! `eula.txt` when the person presses it; the link goes to the api too, which opens it in the browser.

use mere_base::Vec2;
use mere_layout::{Align, Dim, Display, Edges, Style, Track, Tracks};
use mere_ui::tokens::{CONTROL, GAP, GAP_TIGHT, GAP_WIDE, SPACE_24, SPACE_32};
use mere_ui::{ActionLook, ActionOpts, Id, TextLook, Tone, Ui};

use crate::home::{COLUMNS_GAP, LauncherHome, Want};
use crate::model::{MadeFrom, folder_tail};
use crate::text;

/// The page's width: two columns of a half each.
pub const WIDTH: f32 = 1120.0;

/// The width of the label column of the made-from rows, in characters of the body (board C1: 15ch).
const LABEL_CHARS: usize = 15;

/// What the page keeps between frames: whether its section is open, and where its parts stood in the last build.
pub struct State {
    pub made_open: bool,
    pub start: Id,
    pub eula_link: Id,
    pub accept: Id,
    pub made: Id,
    pub profile_link: Id,
    pub open_folder: Id,
}

impl Default for State {
    fn default() -> State {
        let o = Id::ROOT;
        State { made_open: true, start: o, eula_link: o, accept: o, made: o, profile_link: o, open_folder: o }
    }
}

/// One line of the made-from section: its label in a column of a fixed width, then what it says, `GAP_WIDE` between the
/// parts of that.
fn fact(ui: &mut Ui, key: &str, label: &str, label_w: f32, value: impl FnOnce(&mut Ui)) {
    let row = Style {
        display: Display::Grid,
        cols: Tracks::new(&[Track::Px(label_w), Track::MinMaxFr0(1.0)]),
        gap: Vec2::new(GAP, 0.0),
        align_items: Align::Center,
        min_h: Dim::Px(CONTROL),
        ..Style::default()
    };
    ui.node(key, row, |ui| {
        ui.text(label, TextLook::body(Tone::Muted));
        ui.node("value", Style { align_items: Align::Center, gap: Vec2::new(GAP_WIDE, 0.0), min_w: Dim::Px(0.0), ..Style::default() }, value);
    });
}

/// The sentence of the EULA with its link, and Accept facing it.
fn eula_line(ui: &mut Ui, st: &mut State, wants: &mut Vec<Want>) {
    let row = Style { align_items: Align::Center, gap: Vec2::new(SPACE_24, 0.0), pad: Edges::new(GAP_TIGHT, 0.0, SPACE_32, 0.0), ..Style::default() };
    ui.node("eula", row, |ui| {
        let sentence = Style { wrap: true, align_items: Align::Center, min_w: Dim::Px(0.0), ..Style::default() };
        ui.node("sentence", sentence, |ui| {
            ui.text(text::EULA_BEFORE, TextLook::body(Tone::Text));
            let link = ui.action("link", text::EULA_LINK, ActionLook::Underlined);
            st.eula_link = link.id;
            if link.clicked {
                wants.push(Want::OpenLink(text::EULA_URL.to_string()));
            }
            ui.text(text::EULA_AFTER, TextLook::body(Tone::Text));
            ui.text(text::EULA_WRITES, TextLook::body(Tone::Muted));
        });
        let accept = ui.action("accept", text::ACCEPT, ActionLook::Primary);
        st.accept = accept.id;
        if accept.clicked {
            wants.push(Want::Eula);
        }
    });
}

/// MADE FROM: the profile (a link to its page), the world and the folder (with Open). Folds, as a section with no action does.
fn made_from(ui: &mut Ui, st: &mut State, wants: &mut Vec<Want>, made: &MadeFrom, profile: &str, dir: &str) {
    let label_w = TextLook::body(Tone::Muted).width(&"0".repeat(LABEL_CHARS));
    let mut open = st.made_open;
    let (mut profile_link, mut open_folder) = (None, None);
    let section = ui.fold("madeFrom", text::MADE_FROM, &mut open, |ui| {
        fact(ui, "profile", text::MADE_PROFILE, label_w, |ui| {
            profile_link = Some(ui.action("link", profile, ActionLook::Underlined));
            if let Some(left_out) = text::left_out(made.left_out) {
                ui.text(&left_out, TextLook::small(Tone::Muted));
            }
        });
        fact(ui, "world", text::MADE_WORLD, label_w, |ui| {
            let words = match (&made.world, made.seed) {
                (Some(world), Some(seed)) => text::world_seed(world, seed),
                (Some(world), None) => world.clone(),
                (None, _) => text::NO_WORLD.to_string(),
            };
            ui.ellipsis(&words, TextLook::body(Tone::Text));
        });
        fact(ui, "folder", text::MADE_FOLDER, label_w, |ui| {
            ui.node("path", Style { grow: 1.0, min_w: Dim::Px(0.0), ..Style::default() }, |ui| ui.ellipsis_start(&folder_tail(dir), TextLook::body(Tone::Muted)));
            open_folder = Some(ui.action("open", text::OPEN, ActionLook::Underlined));
        });
    });
    st.made_open = open;
    st.made = section.id;
    if let Some(r) = profile_link {
        st.profile_link = r.id;
        if r.clicked {
            wants.push(Want::OpenProfile(made.profile.clone()));
        }
    }
    if let Some(r) = open_folder {
        st.open_folder = r.id;
        if r.clicked {
            wants.push(Want::ServerFolder);
        }
    }
}

pub fn build(home: &mut LauncherHome, ui: &mut Ui) {
    let mut st = std::mem::take(&mut home.pages.server);
    let server = home.server_data().cloned();
    let title = server.as_ref().map(|s| s.name.as_str()).filter(|n| !n.is_empty()).unwrap_or(text::SERVER_NAME).to_string();
    let (loaded, eula) = (server.is_some(), server.as_ref().is_some_and(|s| s.eula));
    // Start waits for the EULA, and for a start or a stop that is not over.
    let off = !eula || home.server_working();
    let profile = server.as_ref().and_then(|s| s.made.as_ref()).map(|m| home.profile_label(&m.profile)).unwrap_or_default();
    let mut start = None;
    let mut wants: Vec<Want> = Vec::new();
    home.page_frame(
        ui,
        WIDTH,
        &title,
        |ui| start = Some(ui.action_with("start", text::START, ActionLook::Primary, ActionOpts { disabled: off, ..ActionOpts::default() })),
        |home, ui| {
            if loaded && !eula {
                eula_line(ui, &mut st, &mut wants);
            }
            home.half(ui, (WIDTH - COLUMNS_GAP) / 2.0, |home, ui| {
                if let Some(s) = &server {
                    if let Some(made) = &s.made {
                        made_from(ui, &mut st, &mut wants, made, &profile, &s.dir);
                    }
                }
                // Before the first start there is nothing to set yet: only what the api last said of the server.
                if eula {
                    home.server_form(ui);
                } else if home.notes().1 != text::FIRST_START {
                    home.server_note_under(ui);
                }
            });
        },
    );
    if let Some(r) = start {
        st.start = r.id;
        if r.clicked {
            wants.push(Want::ServerStart);
        }
    }
    for want in wants {
        home.want(want);
    }
    home.pages.server = st;
}
