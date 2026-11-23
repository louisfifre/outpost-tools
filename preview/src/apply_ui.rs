//! The Apply block under the settings: the two choices that put every value back (to the pack's, to Tectonic's own;
//! each is pressed while the form is there, both when the pack is Tectonic's own), the place line that opens the
//! list of places above itself (`places.rs`, the kit's menu with multiple selectors, ending on "Choose a folder…"),
//! Apply (the only control that writes), what the last apply said, what to do when no game was found or a folder was
//! refused, and the warning about seams. The shell only records the press (`Want::Apply`); the application
//! writes (`apply.rs`).

use mere_layout::{Edges, Style};
use mere_ui::tokens::{GAP, GAP_WIDE};
use mere_ui::{ButtonLook, ButtonOpts, MenuOpts, SelectionRow, Selector, TabLook, TabOpts, TextLook, Tone, Trigger, Ui, choice_style};

use crate::apply::{Notice, Refusal, Report, Running, Snapshot};
use crate::places::{self, Kind, Place};
use crate::settings::{original_values, same_values};
use crate::shell::{ShellIds, Want};
use crate::text;

/// Why a place was refused, as its sentence ends.
fn why(r: Refusal) -> &'static str {
    match r {
        Refusal::Missing => text::REFUSED_MISSING,
        Refusal::InUse => text::REFUSED_IN_USE,
        Refusal::Read => text::REFUSED_READ,
        Refusal::Write => text::REFUSED_WRITE,
        Refusal::Key => text::REFUSED_KEY,
    }
}

/// What a finished apply says: one quiet sentence for what was saved (the places, what makes them take the values,
/// what changes in a world), then one amber sentence per place refused, and the snapshot a prod copy could not write.
pub fn report_words(r: &Report) -> (String, Vec<String>) {
    let saved: Vec<_> = r.saved().collect();
    let mut quiet: Vec<&str> = Vec::new();
    let names: Vec<&str> = saved.iter().map(|w| w.short.as_str()).collect();
    let saved_for = text::saved_for(&text::and_list(&names));
    if !saved.is_empty() {
        quiet.push(&saved_for);
    }
    // A game reads its file at its start: restarting it is all there is to do.
    let game = saved.iter().any(|w| w.kind == Kind::Game);
    if game {
        quiet.push(text::APPLY_RESTART);
    }
    // The pack's source and the test server reach a game through the pack's sync, at its next start: what runs then.
    if saved.iter().any(|w| w.kind != Kind::Game) {
        let server = r.running.iter().any(|x| matches!(x, Running::Server(_)));
        let client = r.running.contains(&Running::Game) && !game;
        match (client, server) {
            (true, true) => quiet.push(text::APPLY_BOTH),
            (true, false) => quiet.push(text::APPLY_GAME),
            (false, true) => quiet.push(text::APPLY_SERVER),
            (false, false) => {}
        }
    }
    // Only a prod copy has a snapshot, and only there do the values outlast a deploy by being committed.
    if saved.iter().any(|w| w.snapshot == Snapshot::Written) {
        quiet.push(text::APPLY_PROD_LIMIT);
    }
    if !saved.is_empty() {
        quiet.push(text::APPLY_LAND);
    }
    let mut warn: Vec<String> = r.places.iter().filter_map(|w| w.result.err().map(|e| text::not_saved(&w.short, why(e)))).collect();
    if saved.iter().any(|w| w.snapshot == Snapshot::Failed) {
        warn.push(text::APPLY_FAIL_SNAPSHOT.to_string());
    }
    (quiet.join(" "), warn)
}

/// The fields of the state it reads and writes, one by one: it is built inside the settings fold, which holds another
/// of the state's fields.
pub struct Apply<'a> {
    pub values: &'a mut [f64; 43],
    pub baseline: &'a [f64; 43],
    pub places: &'a mut Vec<Place>,
    pub open: &'a mut bool,
    /// The last folder chosen was no game.
    pub refused: bool,
    pub notice: &'a Notice,
    pub want: &'a mut Option<Want>,
}

pub fn apply_block(ui: &mut Ui, s: Apply, ids: &mut ShellIds) {
    ids.apply = mere_ui::Id::ROOT;
    ids.apply_note = mere_ui::Id::ROOT;
    ids.apply_text.clear();
    ids.place_line = mere_ui::Id::ROOT;
    ids.place_list = mere_ui::Id::ROOT;
    ids.place_rows.clear();
    ids.place_action = mere_ui::Id::ROOT;
    ids.place_note = "";
    ids.place_text.clear();
    let style = Style { column: true, shrink: 0.0, pad: Edges::new(GAP_WIDE, 0.0, 0.0, 0.0), gap: mere_base::Vec2::new(0.0, GAP), ..Style::default() };
    ui.node("applyBlock", style, |ui| {
        ids.profiles = ui.node("profiles", choice_style(), |ui| {
            ui.text(text::BACK_TO, TextLook::body(Tone::Muted));
            let original = original_values();
            for (key, name, hint, to) in [("pack", text::PACK, text::PROFILE_PACK_HINT, *s.baseline), ("original", text::ORIGINAL, text::ORIGINAL_HINT, original)] {
                let r = ui.tab(key, name, TabLook::Choice, TabOpts { pressed: same_values(s.values, &to), disabled: false });
                ui.title(r.id, hint);
                if r.clicked {
                    *s.values = to;
                }
                if key == "original" {
                    ids.original = r.id;
                } else {
                    ids.back_pack = r.id;
                }
            }
        });

        let working = *s.notice == Notice::Working;
        let ticked: Vec<&Place> = s.places.iter().filter(|p| p.ticked).collect();
        let none_ticked = ticked.is_empty();
        // A place ticked whose file does not hold the form yet: there is something to write.
        let stale = ticked.iter().any(|p| !p.holds(s.values));
        // The pack's source holds the form, but the launcher's copy of it is behind: Apply is lit with no field marked.
        let behind = ticked.iter().any(|p| p.kind == Kind::Source && p.held.len() > 1 && p.held[0].is_some_and(|h| same_values(&h, s.values)) && !p.holds(s.values));
        let enabled = stale && !working;
        let no_game = !s.places.iter().any(|p| p.kind == Kind::Game);

        // The place line, and the list of places hung above it: the corner over Apply, never a dialog. It opens even
        // with no place found, for its last row: "Choose a folder…".
        let line = places::line(s.places);
        ids.place_text = line.clone();
        let opts = MenuOpts { up: true, ..MenuOpts::default() };
        let mut rows = Vec::new();
        let (mut action, mut choose) = (mere_ui::Id::ROOT, false);
        let places = &mut *s.places;
        let r = ui.menu("place", Trigger::Line(&line), s.open, opts, |ui| {
            // The games, then the repository's places, then the action: three groups, each parted from the one before
            // by its own separator. A group with no place is left out.
            let (games, others): (Vec<_>, Vec<_>) = places.iter_mut().enumerate().partition(|(_, p)| p.kind == Kind::Game);
            for (key, group) in [("games", games), ("repository", others)] {
                if group.is_empty() {
                    continue;
                }
                ui.group(key, |ui| {
                    for (i, p) in group {
                        // Several places at once: each row has its multiple selector, and a press flips that place alone.
                        let row = SelectionRow { label: &p.name, detail: &p.how, detail_tail: &p.path, selector: Selector::Multiple, selected: p.ticked, disabled: false };
                        let o = ui.selection_row(&format!("p{i}"), row);
                        if o.clicked {
                            p.ticked = !p.ticked;
                        }
                        rows.push(o.id);
                    }
                });
            }
            ui.group("choose", |ui| {
                let a = ui.action_row("choose", text::CHOOSE_FOLDER);
                (action, choose) = (a.id, a.clicked);
            });
        });
        ids.place_line = r.line.id;
        ids.place_list = r.list.unwrap_or(mere_ui::Id::ROOT);
        ids.place_rows = rows;
        ids.place_action = action;
        // The system's dialog takes the screen: the list closes under it and the line keeps the keyboard.
        if choose {
            *s.open = false;
            *s.want = Some(Want::ChooseFolder);
        }
        if r.enter && enabled {
            *s.want = Some(Want::Apply);
        }

        ui.node("go", Style::default(), |ui| {
            let r = ui.button_with("apply", text::APPLY, ButtonLook::Primary, ButtonOpts { disabled: !enabled, grow: 1.0 });
            ids.apply = r.id;
            let hint = if none_ticked { text::APPLY_NO_PLACE } else if behind { text::APPLY_BEHIND_HINT } else if !stale { text::APPLY_SAME } else { text::APPLY_HINT };
            ui.title(r.id, hint);
            if r.clicked && enabled {
                *s.want = Some(Want::Apply);
            }
        });

        // What the last apply said, while the form still holds what it wrote; or that it is under way.
        let (quiet, warn) = match s.notice {
            Notice::Working => (text::APPLY_WORKING.to_string(), Vec::new()),
            Notice::Done(report) if same_values(&report.values, s.values) => report_words(report),
            _ => (String::new(), Vec::new()),
        };
        let said = !quiet.is_empty() || !warn.is_empty();
        if said {
            let all: Vec<&str> = std::iter::once(quiet.as_str()).filter(|q| !q.is_empty()).chain(warn.iter().map(String::as_str)).collect();
            ids.apply_text = all.join("\n");
            ids.apply_note = ui.node("applyNote", Style { column: true, gap: mere_base::Vec2::new(0.0, GAP), ..Style::default() }, |ui| {
                if !quiet.is_empty() {
                    ui.paragraph(&quiet, TextLook::small(Tone::Muted));
                }
                for w in &warn {
                    ui.paragraph(w, TextLook::small(Tone::Warn));
                }
            });
        }
        if behind && !working {
            ui.paragraph(text::APPLY_BEHIND, TextLook::small(Tone::Muted));
        }
        // A folder refused says why in amber, in the place of the sentence that asked for it.
        if s.refused {
            ui.paragraph(text::NOT_A_GAME, TextLook::small(Tone::Warn));
            ids.place_note = text::NOT_A_GAME;
        } else if no_game {
            ui.paragraph(text::NO_GAME_FOUND, TextLook::small(Tone::Muted));
            ids.place_note = text::NO_GAME_FOUND;
        }
        // Said at rest: the sentence after an Apply says what changes in a world in its place.
        if !matches!(s.notice, Notice::Done(_)) || !said {
            ui.paragraph(text::APPLY_SEAMS, TextLook::small(Tone::Muted));
        }
    });
}
