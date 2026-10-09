//! Customisable keyboard shortcuts (Revu's Preferences > Shortcuts): the user's key for any
//! command, tool or panel, kept in `<config folder>/MarkupCraft/keyboard.json` as
//! `{ "format": "markupcraft-keys", "version": 1, "keys": { "file.save_all": "Ctrl+Alt+S",
//! "tool.cloud": "" } }` ("" = no key). The dispatcher and the menus read the effective map;
//! Tools > Customize Keyboard edits it (press the new keys, Clear, Reset, Import, Export).
//! The file is untrusted input: unknown commands and unreadable keys are dropped.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::Key;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::commands::Keys;

/// Largest keyboard file read.
const MAX_FILE: u64 = 1 << 20;

/// The user's changes to the default keys: command id -> keys (`None` = no key).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyPrefs {
    pub overrides: BTreeMap<String, Option<Keys>>,
    /// Where they are saved; `None` = memory only.
    pub path: Option<PathBuf>,
    /// The command waiting for its new keys (the editor window).
    pub capturing: Option<String>,
    pub show: bool,
    pub filter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileFormat {
    format: String,
    version: u32,
    keys: BTreeMap<String, String>,
}

/// `Ctrl+Shift+S` (Cmd/Option names accepted too) to keys.
pub fn parse_keys(text: &str) -> Option<Keys> {
    let mut k = Keys::new(false, false, false, Key::A);
    let mut key = None;
    for part in text.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" | "control" => k.ctrl = true,
            "shift" => k.shift = true,
            "alt" | "option" => k.alt = true,
            "plus" => key = Some(Key::Plus),
            "minus" => key = Some(Key::Minus),
            "left" => key = Some(Key::ArrowLeft),
            "right" => key = Some(Key::ArrowRight),
            "up" => key = Some(Key::ArrowUp),
            "down" => key = Some(Key::ArrowDown),
            "del" => key = Some(Key::Delete),
            "esc" => key = Some(Key::Escape),
            _ => key = Some(Key::from_name(part)?),
        }
    }
    k.key = key?;
    Some(k)
}

/// Every command, tool and panel id with a name, for the editor.
pub fn all_ids() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = crate::commands::all()
        .filter(|c| c.built)
        .map(|c| (c.id.to_string(), c.label.to_string()))
        .collect();
    out.extend(
        crate::tools::TOOLS
            .iter()
            .map(|t| (format!("tool.{}", t.id), t.label.to_string())),
    );
    out.extend(
        crate::panels::PANELS
            .iter()
            .map(|p| (format!("panel.{}", p.id), format!("{} panel", p.title))),
    );
    out
}

impl KeyPrefs {
    /// Read `path` (missing = no changes).
    pub fn load(path: &Path) -> Self {
        let mut p = KeyPrefs {
            path: Some(path.to_path_buf()),
            ..Default::default()
        };
        if std::fs::metadata(path).is_ok_and(|m| m.len() <= MAX_FILE)
            && let Ok(text) = std::fs::read_to_string(path)
        {
            let _ = p.read_text(&text);
        }
        p
    }

    /// Replace the overrides with those in `text`. Returns how many were read.
    pub fn read_text(&mut self, text: &str) -> Result<usize, String> {
        if text.len() as u64 > MAX_FILE {
            return Err("the file is too large".into());
        }
        let f: FileFormat = serde_json::from_str(text).map_err(|e| format!("not a keyboard file: {e}"))?;
        if f.format != "markupcraft-keys" {
            return Err("not a MarkupCraft keyboard file".into());
        }
        let known: Vec<String> = all_ids().into_iter().map(|(id, _)| id).collect();
        self.overrides = f
            .keys
            .into_iter()
            .filter(|(id, _)| known.contains(id))
            .filter_map(|(id, k)| {
                if k.trim().is_empty() {
                    Some((id, None))
                } else {
                    parse_keys(&k).map(|k| (id, Some(k)))
                }
            })
            .collect();
        Ok(self.overrides.len())
    }

    /// The overrides as a file's text.
    pub fn to_text(&self) -> String {
        let keys = self
            .overrides
            .iter()
            .map(|(id, k)| (id.clone(), k.map(|k| k.label()).unwrap_or_default()))
            .collect();
        serde_json::to_string_pretty(&FileFormat {
            format: "markupcraft-keys".into(),
            version: 1,
            keys,
        })
        .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        match &self.path {
            Some(p) => crate::chest::write_atomic(p, self.to_text().as_bytes()),
            None => Ok(()),
        }
    }

    /// The keys of command `id` now.
    pub fn keys_for(&self, id: &str) -> Option<Keys> {
        match self.overrides.get(id) {
            Some(k) => *k,
            None => default_keys(id),
        }
    }

    /// Give `id` new keys (`None` clears); another command with the same keys loses them.
    /// Returns the id that lost them.
    pub fn assign(&mut self, id: &str, keys: Option<Keys>) -> Option<String> {
        let mut lost = None;
        if let Some(k) = keys {
            for (other, _) in all_ids() {
                if other != id && self.keys_for(&other) == Some(k) {
                    self.overrides.insert(other.clone(), None);
                    lost = Some(other);
                }
            }
        }
        if keys == default_keys(id) {
            self.overrides.remove(id);
        } else {
            self.overrides.insert(id.to_string(), keys);
        }
        lost
    }

    /// Back to the default key of `id`.
    pub fn reset(&mut self, id: &str) {
        self.overrides.remove(id);
    }

    /// Every binding with the user's changes, most specific first (the dispatcher's list).
    pub fn bindings(&self) -> Vec<(Keys, String)> {
        let mut out: Vec<(Keys, String)> = crate::commands::bindings()
            .into_iter()
            .filter(|(k, id)| match self.overrides.get(id) {
                // a changed command keeps its alias only when the main key is unchanged
                Some(_) => false,
                None => !self.overrides.values().any(|o| *o == Some(*k)),
            })
            .collect();
        for (id, k) in &self.overrides {
            if let Some(k) = k {
                out.push((*k, id.clone()));
            }
        }
        out.sort_by_key(|(k, _)| std::cmp::Reverse(k.specificity()));
        out
    }
}

/// The default (first) key of any id.
pub fn default_keys(id: &str) -> Option<Keys> {
    crate::commands::describe(id).and_then(|(_, _, k)| k)
}

/// A dialog answered for the keyboard file.
pub fn dialog_answer(app: &mut AppState, tag: &str, path: &Path) -> String {
    if tag == "export_keys" {
        return match crate::chest::write_atomic(path, app.keys.to_text().as_bytes()) {
            Ok(()) => format!("Exported the keyboard shortcuts to {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        };
    }
    let text = match std::fs::metadata(path) {
        Ok(m) if m.len() > MAX_FILE => return "The file is too large".into(),
        _ => std::fs::read_to_string(path),
    };
    match text.map_err(|e| e.to_string()).and_then(|t| app.keys.read_text(&t)) {
        Ok(n) => {
            let _ = app.keys.save();
            format!("Imported {n} keyboard shortcuts")
        }
        Err(e) => format!("Import failed: {e}"),
    }
}

/// Tools > Customize Keyboard.
pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.keys.show {
        return;
    }
    // Waiting for keys: the next key press (with its modifiers) is the new shortcut.
    if let Some(id) = app.keys.capturing.clone() {
        let pressed = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        if let Some((key, m)) = pressed {
            ctx.input_mut(|i| i.events.clear());
            app.keys.capturing = None;
            if key == Key::Escape && m.is_none() {
                app.status = "Kept the shortcut".into();
            } else {
                let k = Keys::new(m.command, m.shift, m.alt, key);
                let lost = app.keys.assign(&id, Some(k));
                let _ = app.keys.save();
                app.status = match lost {
                    Some(o) => format!("{} now runs {id} (taken from {o})", k.label()),
                    None => format!("{} now runs {id}", k.label()),
                };
            }
        }
    }
    let mut open = true;
    let mut import = false;
    let mut export = false;
    egui::Window::new("Customize Keyboard")
        .open(&mut open)
        .default_size([460.0, 520.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Find");
                ui.text_edit_singleline(&mut app.keys.filter);
                if ui.button("Reset All").clicked() {
                    app.keys.overrides.clear();
                    let _ = app.keys.save();
                }
                import = ui.button("Import...").clicked();
                export = ui.button("Export...").clicked();
            });
            ui.separator();
            let f = app.keys.filter.to_lowercase();
            egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| {
                egui::Grid::new("keys-grid")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        for (id, label) in all_ids() {
                            if !f.is_empty() && !label.to_lowercase().contains(&f) && !id.contains(&f) {
                                continue;
                            }
                            ui.label(&label);
                            let cur = app.keys.keys_for(&id).map(|k| k.label()).unwrap_or_default();
                            let changed = app.keys.overrides.contains_key(&id);
                            let text = if app.keys.capturing.as_deref() == Some(id.as_str()) {
                                "Press keys...".to_string()
                            } else if cur.is_empty() {
                                "-".to_string()
                            } else {
                                cur
                            };
                            let rt = if changed {
                                egui::RichText::new(text).strong()
                            } else {
                                egui::RichText::new(text)
                            };
                            if ui
                                .button(rt)
                                .on_hover_text(format!("{id}: click, then press the new keys"))
                                .clicked()
                            {
                                app.keys.capturing = Some(id.clone());
                            }
                            if ui
                                .small_button("Clear")
                                .on_hover_text(format!("No key for {label}"))
                                .clicked()
                            {
                                app.keys.assign(&id, None);
                                let _ = app.keys.save();
                            }
                            if ui
                                .add_enabled(changed, egui::Button::new("Reset").small())
                                .on_hover_text(format!("Back to the default key of {label}"))
                                .clicked()
                            {
                                app.keys.reset(&id);
                                let _ = app.keys.save();
                            }
                            ui.end_row();
                        }
                    });
            });
        });
    if import {
        app.dialogs.open(
            crate::dialogs::Purpose::Edit("import_keys", String::new()),
            crate::dialogs::KEYS,
            false,
        );
    }
    if export {
        app.dialogs.save(
            crate::dialogs::Purpose::Edit("export_keys", String::new()),
            crate::dialogs::KEYS,
            "keyboard.json",
        );
    }
    if !open {
        app.keys.show = false;
        app.keys.capturing = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_assign_and_round_trip() {
        let k = parse_keys("Ctrl+Shift+S").unwrap();
        assert!(k.ctrl && k.shift && !k.alt && k.key == Key::S);
        assert_eq!(parse_keys(&k.label()), Some(k));
        assert_eq!(parse_keys("Shift+Plus").map(|k| k.key), Some(Key::Plus));
        assert!(parse_keys("Ctrl+Nope").is_none());
        let mut p = KeyPrefs::default();
        // Give Save All the Save key: Save loses it.
        let save = default_keys("file.save").unwrap();
        assert_eq!(p.assign("file.save_all", Some(save)).as_deref(), Some("file.save"));
        assert_eq!(p.keys_for("file.save"), None);
        let b = p.bindings();
        assert!(b.iter().any(|(k, id)| *k == save && id == "file.save_all"));
        assert!(!b.iter().any(|(_, id)| id == "file.save"));
        let text = p.to_text();
        let mut q = KeyPrefs::default();
        assert_eq!(q.read_text(&text).unwrap(), 2);
        assert_eq!(q.overrides, p.overrides);
        p.reset("file.save");
        assert_eq!(p.keys_for("file.save"), Some(save));
        assert!(q.read_text("{\"format\":\"x\",\"version\":1,\"keys\":{}}").is_err());
    }
}
