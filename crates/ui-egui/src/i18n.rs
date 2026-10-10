//! Interface language (General > Options > Language): string tables that map the English
//! text of the interface to another language. English is the default and the key: [`tr`]
//! returns the translation of a string in the current language, or the string itself when the
//! table has none. Tables live in `crates/ui-egui/i18n/<code>.tsv` (one `English<TAB>Translation`
//! line per string, `#` comments), so a translator edits a text file, not code.
//!
//! The language is per thread (the interface runs on one thread; each headless test drives its
//! own app on its own thread), set each frame from the preferences.
//!
//! Translated: the menu bar and every menu command, the drawing and measuring tools, the panel
//! titles, the Preferences dialog's pages and the titles of the dialogs.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The interface languages: code and name in that language.
pub use markupcraft_engine::prefs::LANGUAGES;

/// Spanish (written for MarkupCraft).
const ES: &str = include_str!("../i18n/es.tsv");

thread_local! {
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::En) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Es,
}

impl Lang {
    pub fn from_code(code: &str) -> Self {
        match code {
            "es" => Self::Es,
            _ => Self::En,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
        }
    }
}

/// Set the interface language of this thread (`en`, `es`; anything else is English).
pub fn set_language(code: &str) {
    CURRENT.with(|c| c.set(Lang::from_code(code)));
}

/// This thread's interface language.
pub fn language() -> Lang {
    CURRENT.with(Cell::get)
}

/// Parse a table: `English<TAB>Translation` lines; blank lines and `#` comments are skipped,
/// and so is a line without a tab or with an empty side.
fn parse(text: &'static str) -> HashMap<&'static str, &'static str> {
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .filter(|(k, v)| !k.is_empty() && !v.trim().is_empty())
        .map(|(k, v)| (k, v.trim_end_matches('\r')))
        .collect()
}

fn table(lang: Lang) -> Option<&'static HashMap<&'static str, &'static str>> {
    static TABLE_ES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    match lang {
        Lang::En => None,
        Lang::Es => Some(TABLE_ES.get_or_init(|| parse(ES))),
    }
}

/// `s` in `lang` (itself when the table has no translation).
pub fn tr_in(lang: Lang, s: &str) -> &str {
    match table(lang).and_then(|t| t.get(s)) {
        Some(t) => t,
        None => s,
    }
}

/// `s` in the current language.
pub fn tr(s: &str) -> &str {
    tr_in(language(), s)
}

/// `s` in the current language, as an owned string (for `format!` callers).
pub fn tr_owned(s: &str) -> String {
    tr(s).to_string()
}

/// A dialog window titled `title` in the current language. Its id stays the English title, so
/// its place and size survive a change of language.
pub fn window<'a>(title: &str) -> egui::Window<'a> {
    egui::Window::new(tr(title).to_string()).id(egui::Id::new(title))
}

/// The titles of the dialogs (each window's title goes through [`window`]).
#[rustfmt::skip]
pub const DIALOG_TITLES: &[&str] = &[
    "About MarkupCraft", "Add Viewport", "Apply Redactions", "Apply to Pages", "Batch Sign & Seal",
    "Calibrate", "Check Spelling", "Compare Documents", "Count Status Report", "Create Digital ID",
    "Create PDF Package", "Customize Keyboard", "Customize Toolbars", "Delete File", "Deskew",
    "Digital IDs", "Document", "Document Properties", "Document Recovery", "Dynamic Fill",
    "Edit Action", "Email Templates", "File Manager Integration", "File Properties", "File in Use",
    "Form Fields", "Insert Layered Pages", "Insert Pages", "Interactive Stamp", "JavaScript Console",
    "Jobs", "Keyboard Shortcuts", "Legend", "Line Styles", "Link Search Results", "Manage Columns",
    "Markup Summary", "MarkupCraft Help", "New PDF from Template", "Number Pages", "OCR",
    "Overlay Pages", "Page Labels from Region", "Preferences", "Print", "Profile Columns",
    "Quantity Link", "Redaction Properties", "Reply", "Revert As", "Review Text", "Set Scale",
    "Sign Document", "Signed Document", "Sketch to Scale", "Smart Overlay", "Stamp Fields",
    "Stamp Settings", "Stamps", "Stitching", "Web Tab",
];

/// Labels of the menus and dialogs that are not command, tool or panel names.
#[rustfmt::skip]
pub const OTHER: &[&str] = &[
    "Panels", "Toolbars", "not yet", "Language", "Theme", "System", "Light", "Dark", "Options",
    "User name (author of new markups)",
];

/// Every string of the menus, tools, panels and Preferences pages, for the completeness check.
pub fn interface_strings() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    v.extend(crate::commands::MENUS.iter().copied());
    v.extend(crate::commands::all().filter(|c| !c.label.is_empty()).map(|c| c.label));
    v.extend(crate::tools::TOOLS.iter().map(|t| t.label));
    v.extend(crate::panels::PANELS.iter().map(|p| p.title));
    v.extend(crate::prefs_ui::PAGES.iter().copied());
    v.extend(crate::features::more6::prefs::PAGES.iter().map(|(p, _)| *p));
    v.extend(DIALOG_TITLES.iter().copied());
    v.extend(OTHER.iter().copied());
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_covers_every_menu_tool_panel_and_page() {
        let missing: Vec<&str> = interface_strings()
            .into_iter()
            .filter(|s| table(Lang::Es).is_some_and(|t| !t.contains_key(s)))
            .collect();
        assert!(
            missing.is_empty(),
            "{} untranslated:\n{}",
            missing.len(),
            missing.join("\n")
        );
    }

    #[test]
    fn english_is_the_default_and_the_key() {
        assert_eq!(language(), Lang::En);
        assert_eq!(tr("File"), "File");
        set_language("es");
        assert_eq!(tr("File"), "Archivo");
        assert_eq!(tr("no such text"), "no such text");
        set_language("xx");
        assert_eq!(tr("File"), "File");
    }

    #[test]
    fn tables_parse_tabs_and_skip_comments() {
        let t = parse("# c\nA\tB\nbad line\nC\t\nD\tE\r\n");
        assert_eq!(t.get("A"), Some(&"B"));
        assert_eq!(t.get("D"), Some(&"E"));
        assert_eq!(t.len(), 2);
    }
}
