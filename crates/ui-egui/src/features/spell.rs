//! Check Spelling (F7): the markup comments against the bundled dictionary, one word at a
//! time: change it to a suggestion (or typed text), ignore it, or ignore it everywhere.

use egui::RichText;
use markupcraft_engine::MarkupPatch;
use markupcraft_engine::spell::{Misspelling, SpellOptions, dictionary};

use crate::{AppState, actions};

pub const LANG: &str = "en_US";

#[derive(Default)]
pub struct SpellState {
    pub open: bool,
    pub words: Vec<Misspelling>,
    pub current: usize,
    pub change_to: String,
    pub ignored: Vec<String>,
    pub ignore_caps: bool,
    pub message: String,
}

/// Markup > Check Spelling.
pub fn start(app: &mut AppState) {
    app.features.spell.open = true;
    app.features.spell.ignore_caps = true;
    check(app);
}

/// Check every markup again (skipping ignored words).
pub fn check(app: &mut AppState) {
    let Some(d) = app.docs.get(app.active) else { return };
    let s = &mut app.features.spell;
    let opts = SpellOptions {
        ignore_uppercase: s.ignore_caps,
        accept: s.ignored.clone(),
        suggestions: 6,
    };
    match dictionary(LANG).and_then(|dict| d.session.spell_check(&dict, None, &opts)) {
        Ok(w) => {
            s.words = w;
            s.current = 0;
            s.change_to = s
                .words
                .first()
                .and_then(|m| m.suggestions.first().cloned())
                .unwrap_or_default();
            s.message = if s.words.is_empty() {
                "No spelling errors in the markups.".into()
            } else {
                actions::plural(s.words.len(), "word").to_string() + " to check"
            };
        }
        Err(e) => {
            s.words.clear();
            s.message = e.to_string();
        }
    }
    go_to_current(app);
}

fn go_to_current(app: &mut AppState) {
    let Some(m) = app.features.spell.words.get(app.features.spell.current).cloned() else {
        return;
    };
    if let Some(d) = app.doc_mut() {
        if let Some(p) = m.page {
            let n = d.session.page_count();
            d.view.go_to_page(p, n);
        }
        actions::select(&mut d.session, vec![m.markup.clone()]);
    }
}

fn advance(app: &mut AppState) {
    let s = &mut app.features.spell;
    s.current += 1;
    s.change_to = s
        .words
        .get(s.current)
        .and_then(|m| m.suggestions.first().cloned())
        .unwrap_or_default();
    go_to_current(app);
}

/// Replace the current word with `to` in its markup's comment.
pub fn change(app: &mut AppState, to: &str) {
    let Some(m) = app.features.spell.words.get(app.features.spell.current).cloned() else {
        return;
    };
    let Some(d) = app.doc_mut() else { return };
    let Some(markup) = d.session.doc().find(&m.markup) else {
        return;
    };
    let text: Vec<char> = markup.contents.chars().collect();
    let len = m.word.chars().count();
    let ok = text
        .get(m.start..m.start + len)
        .is_some_and(|w| w.iter().collect::<String>() == m.word);
    if !ok {
        app.features.spell.message = "The comment changed; check again".into();
        return;
    }
    let mut new: String = text.iter().take(m.start).collect();
    new.push_str(to);
    new.extend(text.iter().skip(m.start + len));
    let patch = MarkupPatch {
        contents: Some(new),
        ..Default::default()
    };
    let r = d.session.set_properties(std::slice::from_ref(&m.markup), &patch);
    app.status = actions::report(r, |_| format!("Changed {} to {to}", m.word));
    // Offsets of later words in the same comment move by the change in length.
    let delta = to.chars().count() as isize - len as isize;
    let cur = app.features.spell.current;
    for w in app.features.spell.words.iter_mut().skip(cur + 1) {
        if w.markup == m.markup && w.start > m.start {
            w.start = (w.start as isize + delta).max(0) as usize;
        }
    }
    advance(app);
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.spell.open {
        return;
    }
    let (mut open, mut act) = (true, None::<u8>);
    super::window("Check Spelling").open(&mut open).show(ctx, |ui| {
        let s = &mut app.features.spell;
        match s.words.get(s.current) {
            Some(m) => {
                ui.label(format!("Word {} of {}", s.current + 1, s.words.len()));
                ui.label(RichText::new(&m.word).strong().size(16.0));
                ui.horizontal(|ui| {
                    ui.label("Change to:");
                    ui.text_edit_singleline(&mut s.change_to);
                });
                ui.label("Suggestions:");
                let sugg = m.suggestions.clone();
                ui.horizontal_wrapped(|ui| {
                    for w in &sugg {
                        if ui.selectable_label(*w == s.change_to, w).clicked() {
                            s.change_to = w.clone();
                        }
                    }
                    if sugg.is_empty() {
                        ui.label(RichText::new("(none)").weak());
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Change").clicked() {
                        act = Some(0);
                    }
                    if ui.button("Ignore").clicked() {
                        act = Some(1);
                    }
                    if ui.button("Ignore All").clicked() {
                        act = Some(2);
                    }
                });
            }
            None => {
                ui.label(if s.message.is_empty() { "Done." } else { &s.message });
            }
        }
        ui.checkbox(&mut s.ignore_caps, "Ignore words in capitals");
        ui.horizontal(|ui| {
            if ui.button("Check Again").clicked() {
                act = Some(3);
            }
            if ui.button("Close").clicked() {
                s.open = false;
            }
        });
    });
    if !open {
        app.features.spell.open = false;
    }
    match act {
        Some(0) => {
            let to = app.features.spell.change_to.clone();
            if !to.trim().is_empty() {
                change(app, &to);
            }
        }
        Some(1) => advance(app),
        Some(2) => {
            if let Some(w) = app
                .features
                .spell
                .words
                .get(app.features.spell.current)
                .map(|m| m.word.clone())
            {
                app.features.spell.ignored.push(w.clone());
                let cur = app.features.spell.current;
                let s = &mut app.features.spell;
                let keep: Vec<Misspelling> = s
                    .words
                    .iter()
                    .enumerate()
                    .filter(|(i, m)| *i <= cur || m.word != w)
                    .map(|(_, m)| m.clone())
                    .collect();
                s.words = keep;
                advance(app);
            }
        }
        Some(3) => check(app),
        _ => {}
    }
}
