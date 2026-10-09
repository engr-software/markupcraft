//! Markups List settings kept between sessions: saved views (named filter, sort, grouping,
//! search and column configurations, Revu's saved filters) and column widths, in
//! `<config folder>/MarkupCraft/markups-list.json`; and Copy Rows (the selected markups' visible
//! cells as tab-separated text for a spreadsheet).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use markupcraft_model::{MarkupTable, Scope, View};
use serde::{Deserialize, Serialize};

/// Largest settings file read.
const MAX_FILE: u64 = 4 << 20;
/// Most saved views kept.
const MAX_VIEWS: usize = 200;

/// A named list configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedView {
    pub name: String,
    pub visible: Vec<String>,
    #[serde(default)]
    pub sort_column: String,
    #[serde(default)]
    pub sort_descending: bool,
    #[serde(default)]
    pub group_by: Vec<String>,
    #[serde(default)]
    pub filters: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub measurements_only: bool,
}

impl SavedView {
    pub fn of(name: &str, v: &View) -> Self {
        Self {
            name: name.to_string(),
            visible: v.visible.clone(),
            sort_column: v.sort_column.clone(),
            sort_descending: v.sort_descending,
            group_by: v.group_by.clone(),
            filters: v.filters.clone(),
            search: v.search.clone(),
            measurements_only: v.measurements_only,
        }
    }

    /// Apply to `v` (its page scope stays).
    pub fn apply(&self, v: &mut View) {
        v.visible = self.visible.clone();
        v.sort_column = self.sort_column.clone();
        v.sort_descending = self.sort_descending;
        v.group_by = self.group_by.clone();
        v.filters = self.filters.clone();
        v.search = self.search.clone();
        v.measurements_only = self.measurements_only;
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FileFormat {
    format: String,
    version: u32,
    #[serde(default)]
    views: Vec<SavedView>,
    #[serde(default)]
    widths: BTreeMap<String, f32>,
}

/// The list's saved settings.
#[derive(Debug, Clone, Default)]
pub struct ListPrefs {
    pub views: Vec<SavedView>,
    /// column id -> width (points)
    pub widths: BTreeMap<String, f32>,
    pub path: Option<PathBuf>,
    /// The name typed for Save View.
    pub new_name: String,
    /// Widths changed since the last save.
    widths_dirty: bool,
}

impl ListPrefs {
    pub fn load(path: &Path) -> Self {
        let mut p = ListPrefs {
            path: Some(path.to_path_buf()),
            ..Default::default()
        };
        if std::fs::metadata(path).is_ok_and(|m| m.len() <= MAX_FILE)
            && let Ok(text) = std::fs::read_to_string(path)
            && let Ok(f) = serde_json::from_str::<FileFormat>(&text)
            && f.format == "markupcraft-markups-list"
        {
            p.views = f.views.into_iter().take(MAX_VIEWS).collect();
            p.widths = f
                .widths
                .into_iter()
                .filter(|(_, w)| w.is_finite() && (10.0..=2000.0).contains(w))
                .collect();
        }
        p
    }

    pub fn save(&mut self) {
        self.widths_dirty = false;
        let Some(path) = self.path.clone() else { return };
        let f = FileFormat {
            format: "markupcraft-markups-list".into(),
            version: 1,
            views: self.views.clone(),
            widths: self.widths.clone(),
        };
        if let Ok(text) = serde_json::to_string_pretty(&f)
            && let Err(e) = crate::chest::write_atomic(&path, text.as_bytes())
        {
            log::warn!("markups list settings: {e}");
        }
    }

    /// Save (or replace) the view `name`.
    pub fn save_view(&mut self, name: &str, v: &View) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        self.views.retain(|s| s.name != name);
        self.views.push(SavedView::of(name, v));
        self.views.truncate(MAX_VIEWS);
        self.save();
    }

    pub fn delete_view(&mut self, name: &str) {
        self.views.retain(|s| s.name != name);
        self.save();
    }

    pub fn width(&self, column: &str) -> Option<f32> {
        self.widths.get(column).copied()
    }

    /// The widths the table drew this frame; saved once the pointer is up.
    pub fn note_widths(&mut self, widths: &[(String, f32)], pointer_down: bool) {
        for (id, w) in widths {
            if w.is_finite() && *w >= 10.0 && self.widths.get(id).is_none_or(|old| (old - w).abs() > 0.5) {
                self.widths.insert(id.clone(), w.round());
                self.widths_dirty = true;
            }
        }
        if self.widths_dirty && !pointer_down {
            self.save();
        }
    }
}

/// The selected markups' visible cells, tab-separated, with a header line.
pub fn rows_tsv(table: &MarkupTable<'_>, view: &View, selected: &[usize]) -> String {
    let cols: Vec<usize> = view.visible.iter().filter_map(|id| table.column_index(id)).collect();
    let clean = |s: &str| s.replace(['\t', '\r', '\n'], " ");
    let mut out: Vec<String> = vec![
        cols.iter()
            .filter_map(|c| table.columns().get(*c).map(|col| clean(&col.header)))
            .collect::<Vec<_>>()
            .join("\t"),
    ];
    let mut v = view.clone();
    v.scope = Scope::Selected(selected.iter().copied().collect());
    let root = table.build(&v);
    let mut rows = Vec::new();
    collect(&root, &mut rows);
    for i in rows {
        out.push(
            cols.iter()
                .map(|c| clean(&table.cell(i, *c).text))
                .collect::<Vec<_>>()
                .join("\t"),
        );
    }
    out.join("\n")
}

fn collect(g: &markupcraft_model::Group, out: &mut Vec<usize>) {
    out.extend(g.rows.iter().copied());
    for c in &g.children {
        collect(c, out);
    }
}

/// The Views menu: saved views to apply or delete, and Save Current View.
pub fn views_menu(ui: &mut egui::Ui, view: &mut View, prefs: &mut ListPrefs) {
    ui.menu_button("Views", |ui| {
        if prefs.views.is_empty() {
            ui.label(egui::RichText::new("No saved views yet").weak());
        }
        let mut delete = None;
        for s in &prefs.views {
            ui.horizontal(|ui| {
                if ui.button(&s.name).on_hover_text("Apply this view").clicked() {
                    s.apply(view);
                    ui.close();
                }
                if crate::icons::button(ui, "x", 16.0, false, &format!("Delete view {}", s.name)).clicked() {
                    delete = Some(s.name.clone());
                }
            });
        }
        if let Some(n) = delete {
            prefs.delete_view(&n);
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut prefs.new_name)
                    .hint_text("View name")
                    .desired_width(120.0),
            );
            if ui.button("Save View").clicked() && !prefs.new_name.trim().is_empty() {
                let name = std::mem::take(&mut prefs.new_name);
                prefs.save_view(&name, view);
                ui.close();
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_and_widths_persist() {
        let dir = std::env::temp_dir().join(format!("markupcraft-listprefs-{}", std::process::id()));
        let path = dir.join("markups-list.json");
        let mut p = ListPrefs::load(&path);
        let v = View {
            visible: vec!["subject".into(), "measurement".into()],
            search: "duct".into(),
            ..Default::default()
        };
        p.save_view("Ducts", &v);
        p.note_widths(&[("subject".into(), 140.0)], false);
        let q = ListPrefs::load(&path);
        assert_eq!(q.views.len(), 1);
        assert_eq!(q.width("subject"), Some(140.0));
        let mut w = View::default();
        q.views[0].apply(&mut w);
        assert_eq!(w.search, "duct");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
