//! Rich text and spell check in the in-place text editor: Ctrl+B / Ctrl+I / Ctrl+U and a small
//! toolbar (bold, italic, underline, colour) style the selected characters; the editor shows the
//! styles as typed; misspelled words get a red underline and the toolbar offers suggestions for
//! the word at the cursor. The dictionary (`markupcraft_engine::spell`) loads on a worker thread
//! the first time an editor opens.

use std::sync::{Arc, OnceLock};

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{Color32, FontId, Stroke};
use markupcraft_engine::spell::{Dictionary, SpellOptions, check_text};
use markupcraft_model::rich::{self, StyleChange, TextRun};
use markupcraft_model::{Color, TextStyle};

static DICT: OnceLock<Option<Arc<Dictionary>>> = OnceLock::new();
static STARTED: std::sync::Once = std::sync::Once::new();

/// Start loading the spelling dictionary (once per process, off the UI thread).
pub fn load_dictionary() {
    STARTED.call_once(|| {
        let spawned = std::thread::Builder::new().name("markupcraft-spell".into()).spawn(|| {
            let _ = DICT.set(markupcraft_engine::spell::dictionary("en_US").ok());
        });
        if spawned.is_err() {
            let _ = DICT.set(None);
        }
    });
}

/// Load the spelling dictionary on this thread and wait until it is there (tests and the
/// headless screenshots: the spell-check underline and suggestions then show from the first
/// frame instead of whenever the worker thread finishes).
pub fn load_dictionary_blocking() {
    STARTED.call_once(|| {
        let _ = DICT.set(markupcraft_engine::spell::dictionary("en_US").ok());
    });
    // A worker started earlier may still be loading: wait for it (bounded).
    for _ in 0..12_000 {
        if DICT.get().is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// The dictionary, once loaded.
pub fn dictionary() -> Option<Arc<Dictionary>> {
    DICT.get().cloned().flatten()
}

/// Whether the dictionary finished loading (or failed to).
pub fn dictionary_settled() -> bool {
    DICT.get().is_some()
}

/// Misspelled words of `text` as char ranges (no suggestions: cheap enough per frame).
pub fn misspelled(text: &str) -> Vec<(usize, usize, String)> {
    let Some(d) = dictionary() else { return Vec::new() };
    let p = crate::spell_prefs::get();
    if text.len() > 20_000 || !p.live {
        return Vec::new();
    }
    let opts = SpellOptions {
        suggestions: 0,
        ignore_uppercase: p.ignore_caps,
        british: p.british,
        accept: p.words,
    };
    check_text(&d, text, &opts)
        .into_iter()
        .map(|m| {
            let n = m.word.chars().count();
            (m.start, m.start + n, m.word)
        })
        .collect()
}

/// Suggestions for `word`.
pub fn suggestions(word: &str) -> Vec<String> {
    dictionary().map(|d| d.suggest(word, 4)).unwrap_or_default()
}

fn c32(c: &Color) -> Color32 {
    crate::theme::color32(c, 1.0)
}

/// The editor's layout: the text in its runs' styles, misspellings underlined in red.
pub fn layout_job(text: &str, base: &TextStyle, runs: &[TextRun], size: f32, wrap: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let chars: Vec<char> = text.chars().collect();
    let bad = misspelled(text);
    let uc = crate::spell_prefs::get().color;
    let n = chars.len();
    // Cut points: run boundaries and misspelling boundaries.
    let mut cuts: Vec<usize> = vec![0, n];
    for (s, e, _) in rich::segments(base, runs, 0, n) {
        cuts.push(s);
        cuts.push(e);
    }
    for (s, e, _) in &bad {
        cuts.push(*s);
        cuts.push(*e);
    }
    cuts.retain(|c| *c <= n);
    cuts.sort_unstable();
    cuts.dedup();
    for w in cuts.windows(2) {
        let (s, e) = (w[0], w[1]);
        if s >= e {
            continue;
        }
        let st = rich::style_at(base, runs, s);
        let wrong = bad.iter().any(|(bs, be, _)| *bs <= s && e <= *be);
        let t: String = chars.get(s..e).unwrap_or_default().iter().collect();
        let underline = if wrong {
            Stroke::new(1.5, Color32::from_rgb(uc[0], uc[1], uc[2]))
        } else if st.underline {
            Stroke::new((size / 14.0).max(1.0), c32(&st.color))
        } else {
            Stroke::NONE
        };
        job.append(
            &t,
            0.0,
            TextFormat {
                font_id: FontId::proportional(if st.bold { size * 1.04 } else { size }),
                color: c32(&st.color),
                italics: st.italic,
                underline,
                ..Default::default()
            },
        );
    }
    job
}

/// The selected chars of the editor `id` (start, end), when there is a selection.
pub fn selection(ctx: &egui::Context, id: egui::Id) -> Option<(usize, usize)> {
    let state = egui::TextEdit::load_state(ctx, id)?;
    let r = state.cursor.char_range()?;
    let (a, b): (usize, usize) = (r.primary.index.into(), r.secondary.index.into());
    (a != b).then_some((a.min(b), a.max(b)))
}

/// The cursor position of the editor `id`.
pub fn cursor(ctx: &egui::Context, id: egui::Id) -> Option<usize> {
    egui::TextEdit::load_state(ctx, id)?
        .cursor
        .char_range()
        .map(|r| r.primary.index.into())
}

/// Style the selection of editor `id`: returns the new runs (unchanged without a selection).
pub fn restyle(
    ctx: &egui::Context,
    id: egui::Id,
    text: &str,
    base: &TextStyle,
    runs: &[TextRun],
    change: StyleChange,
) -> Vec<TextRun> {
    match selection(ctx, id) {
        Some((s, e)) => rich::apply(base, runs, text.chars().count(), s, e, change),
        None => runs.to_vec(),
    }
}

/// Put the cursor of editor `id` at `at` (after a replacement).
pub fn set_cursor(ctx: &egui::Context, id: egui::Id, at: usize) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(at))));
        state.store(ctx, id);
    }
}

/// The misspelled word at char `at`: (start, end, word).
pub fn word_at(text: &str, at: usize) -> Option<(usize, usize, String)> {
    misspelled(text).into_iter().find(|(s, e, _)| *s <= at && at <= *e)
}

/// Replace chars `s..e` of `text` with `with`; runs follow.
pub fn replace(text: &str, runs: &[TextRun], s: usize, e: usize, with: &str) -> (String, Vec<TextRun>) {
    let chars: Vec<char> = text.chars().collect();
    let mut out: String = chars.get(..s).unwrap_or_default().iter().collect();
    out.push_str(with);
    out.extend(chars.get(e..).unwrap_or_default());
    let runs = rich::rebase(runs, text, &out);
    (out, runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_a_word_keeps_runs() {
        let base = TextStyle::default();
        let runs = rich::apply(&base, &[], 11, 0, 5, StyleChange::Bold);
        let (t, r) = replace("hello wrold", &runs, 6, 11, "world");
        assert_eq!(t, "hello world");
        assert_eq!((r[0].start, r[0].end), (0, 5));
        let job = layout_job("hello world", &base, &r, 12.0, 100.0);
        assert!(job.sections.len() >= 2);
    }
}
