//! General > Spelling: check spelling as you type (on or off), ignore words in capitals, the
//! underline colour, the user dictionary (words always accepted; Check Spelling's Add to
//! Dictionary adds to it) and auto-complete from a managed list of phrases (the text editor
//! offers the phrases that start with the word being typed).

use egui::RichText;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpellPrefs {
    /// Underline misspelled words while typing.
    pub live: bool,
    pub ignore_caps: bool,
    pub color: [u8; 3],
    /// The user dictionary.
    pub words: Vec<String>,
    /// Auto-complete from `phrases`.
    pub complete: bool,
    pub phrases: Vec<String>,
    /// The dictionary: English (United Kingdom) instead of English (United States).
    pub british: bool,
}

impl Default for SpellPrefs {
    fn default() -> Self {
        Self {
            live: true,
            ignore_caps: false,
            color: [220, 30, 30],
            words: Vec::new(),
            complete: true,
            phrases: Vec::new(),
            british: false,
        }
    }
}

thread_local! {
    /// The settings of this (UI) thread's app.
    static CURRENT: std::cell::RefCell<Option<SpellPrefs>> = const { std::cell::RefCell::new(None) };
}

/// Make `p` the settings the editor uses (once a frame).
pub fn set(p: &SpellPrefs) {
    CURRENT.with(|c| {
        if let Ok(mut g) = c.try_borrow_mut()
            && g.as_ref() != Some(p)
        {
            *g = Some(p.clone());
        }
    });
}

/// The settings in force.
pub fn get() -> SpellPrefs {
    CURRENT
        .with(|c| c.try_borrow().ok().and_then(|g| g.clone()))
        .unwrap_or_default()
}

/// Phrases of the auto-complete list that start with `word` (case-insensitive), at most 5.
pub fn completions(word: &str) -> Vec<String> {
    let p = get();
    let w = word.trim().to_lowercase();
    if !p.complete || w.chars().count() < 2 {
        return Vec::new();
    }
    p.phrases
        .iter()
        .filter(|ph| {
            let l = ph.to_lowercase();
            l.starts_with(&w) && l != w
        })
        .take(5)
        .cloned()
        .collect()
}

/// The word being typed before char `at`: (start, end, word).
pub fn word_before(text: &str, at: usize) -> Option<(usize, usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let at = at.min(chars.len());
    let mut s = at;
    while s > 0
        && chars
            .get(s - 1)
            .is_some_and(|c| c.is_alphanumeric() || *c == '-' || *c == '\'')
    {
        s -= 1;
    }
    (s < at).then(|| (s, at, chars.get(s..at).unwrap_or_default().iter().collect()))
}

fn list_ui(ui: &mut egui::Ui, id: &str, hint: &str, list: &mut Vec<String>) {
    let mut remove = None;
    egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(90.0)
        .show(ui, |ui| {
            for (i, w) in list.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(w);
                    if ui.small_button("Remove").on_hover_text(format!("Remove {w}")).clicked() {
                        remove = Some(i);
                    }
                });
            }
        });
    if let Some(i) = remove
        && i < list.len()
    {
        list.remove(i);
    }
    ui.horizontal(|ui| {
        let key = egui::Id::new(("spell-add", id));
        let mut t: String = ui.data(|d| d.get_temp(key)).unwrap_or_default();
        ui.add(egui::TextEdit::singleline(&mut t).hint_text(hint).desired_width(160.0));
        if ui.button(format!("Add to {id}")).clicked() && !t.trim().is_empty() && list.len() < 5_000 {
            let w = t.trim().to_string();
            if !list.contains(&w) {
                list.push(w);
            }
            t.clear();
        }
        ui.data_mut(|d| d.insert_temp(key, t));
    });
}

/// The Spelling rows of Preferences.
pub fn page_ui(ui: &mut egui::Ui, p: &mut SpellPrefs) {
    ui.add_space(6.0);
    ui.label(RichText::new("Spelling").strong());
    ui.checkbox(&mut p.live, "Check spelling as you type");
    ui.checkbox(&mut p.ignore_caps, "Ignore words in capitals");
    ui.horizontal(|ui| {
        ui.label("Underline colour");
        let mut c = egui::Color32::from_rgb(p.color[0], p.color[1], p.color[2]);
        if ui.color_edit_button_srgba(&mut c).changed() {
            p.color = [c.r(), c.g(), c.b()];
        }
    });
    ui.horizontal(|ui| {
        ui.label("Dictionary");
        ui.radio_value(&mut p.british, false, "English (United States)");
        ui.radio_value(&mut p.british, true, "English (United Kingdom)");
    });
    ui.label("User dictionary");
    list_ui(ui, "Dictionary", "a word", &mut p.words);
    ui.checkbox(&mut p.complete, "Auto-complete from this list");
    list_ui(ui, "List", "a phrase", &mut p.phrases);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completions_and_the_word_being_typed() {
        set(&SpellPrefs {
            phrases: vec![
                "Verify in field".into(),
                "Verify with architect".into(),
                "Install".into(),
            ],
            ..Default::default()
        });
        assert_eq!(completions("ver").len(), 2);
        assert!(completions("v").is_empty(), "two letters at least");
        assert_eq!(word_before("please ver", 10), Some((7, 10, "ver".into())));
        assert_eq!(word_before("x ", 2), None);
        set(&SpellPrefs::default());
    }
}
