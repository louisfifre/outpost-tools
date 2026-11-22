//! The Apply to pack block under the settings: the button, beside it the two choices that put every value back (to the
//! pack's, to Tectonic's own; each is pressed while the form is there, both when the pack is Tectonic's own), what the
//! last apply said, and the one-time warning about seams. The shell only records the press (`Want::Apply`); the application writes (`apply.rs`).

use mere_layout::{Align, Edges, Margin, Side, Style};
use mere_ui::{ButtonLook, ButtonOpts, TabLook, TabOpts, TextLook, Tone, Ui, choice_style};

use crate::apply::{Fail, Notice, Running, Snapshot};
use crate::settings::{original_values, same_values};
use crate::shell::{ShellIds, Want};
use crate::text;

/// What a finished apply says: that it was written, and which running program only reads it at its next start.
pub fn notice_words(n: &Notice) -> String {
    let said = match n {
        Notice::None => "",
        Notice::Working => text::APPLY_WORKING,
        Notice::Done { snapshot: Snapshot::Failed, .. } => text::APPLY_FAIL_SNAPSHOT,
        Notice::Done { running, .. } => match (running.contains(&Running::Game), running.iter().any(|r| matches!(r, Running::Server(_)))) {
            (true, true) => text::APPLY_BOTH,
            (true, false) => text::APPLY_GAME,
            (false, true) => text::APPLY_SERVER,
            (false, false) => text::APPLIED,
        },
        Notice::Failed(Fail::Read) => text::APPLY_FAIL_READ,
        Notice::Failed(Fail::Write) => text::APPLY_FAIL_WRITE,
        Notice::Failed(Fail::Key) => text::APPLY_FAIL_KEY,
    };
    // Only a prod copy has a snapshot, and only there do the values outlast a deploy by being committed.
    if matches!(n, Notice::Done { snapshot: Snapshot::Written, .. }) { format!("{said} {}", text::APPLY_PROD_LIMIT) } else { said.to_string() }
}

/// The fields of the state it reads and writes, one by one: it is built inside the settings fold, which holds another
/// of the state's fields.
pub struct Apply<'a> {
    pub values: &'a mut [f64; 43],
    pub baseline: &'a [f64; 43],
    pub snapshot: Option<[f64; 43]>,
    pub pack_known: bool,
    pub notice: &'a Notice,
    pub want: &'a mut Option<Want>,
}

pub fn apply_block(ui: &mut Ui, s: Apply, ids: &mut ShellIds) {
    ids.apply = mere_ui::Id::ROOT;
    ids.apply_note = mere_ui::Id::ROOT;
    ids.apply_text.clear();
    let style = Style { column: true, shrink: 0.0, pad: Edges::new(10.0, 0.0, 0.0, 0.0), gap: mere_base::Vec2::new(0.0, 6.0), ..Style::default() };
    ui.node("applyBlock", style, |ui| {
        let differs = !same_values(s.values, s.baseline);
        // The pack holds the form but the launcher's copy of it is behind: Apply is lit with no field marked.
        let behind = !differs && s.snapshot.is_some_and(|x| !same_values(&x, s.values));
        let working = *s.notice == Notice::Working;
        let enabled = s.pack_known && (differs || behind) && !working;
        let line = Style { wrap: true, align_items: Align::Baseline, gap: mere_base::Vec2::new(10.0, 4.0), ..Style::default() };
        ui.node("applyLine", line, |ui| {
            let r = ui.button_with("applyPack", text::APPLY_TO_PACK, ButtonLook::Link, ButtonOpts { disabled: !enabled, grow: 0.0 });
            ids.apply = r.id;
            let hint = if !s.pack_known { text::APPLY_NO_PACK } else if behind { text::APPLY_BEHIND_HINT } else if !differs { text::APPLY_SAME } else { text::APPLY_HINT };
            ui.title(r.id, hint);
            if r.clicked && enabled {
                *s.want = Some(Want::Apply);
            }
            // Pushed to the right end of the line, or of the next one when the panel is too narrow for both.
            let back = Style { margin: Margin { left: Side::Auto, ..Margin::default() }, ..choice_style() };
            ids.profiles = ui.node("profiles", back, |ui| {
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
        });
        // A "done" that the values have since moved away from is not about what is shown any more.
        let words = if matches!(*s.notice, Notice::Done { .. }) && differs { String::new() } else { notice_words(s.notice) };
        if !words.is_empty() {
            let tone = if matches!(*s.notice, Notice::Failed(_) | Notice::Done { snapshot: Snapshot::Failed, .. }) { Tone::Warn } else { Tone::Muted };
            ids.apply_text = words.clone();
            ids.apply_note = ui.node("applyNote", Style::default(), |ui| ui.paragraph(&words, TextLook::small(tone)));
        }
        if behind && !working {
            ui.paragraph(text::APPLY_BEHIND, TextLook::small(Tone::Muted));
        }
        // Said once, here, whatever happens: the seams are a fact of the world already generated.
        if s.pack_known {
            ui.paragraph(text::APPLY_SEAMS, TextLook::small(Tone::Muted));
        }
    });
}
