//! Sets: many PDFs navigated as one drawing set (a `.pcset` file listing them). The Sets panel
//! lists every sheet by its label; clicking one opens its file at that page.

use std::path::{Path, PathBuf};

use markupcraft_engine::batch::{DrawingSet, SetSheet, SetSort, load_set, save_set as write_set, set_sheets};

use crate::{AppState, actions};

pub struct SetsState {
    pub set: DrawingSet,
    pub path: Option<PathBuf>,
    pub sort: SetSort,
    /// Sheets of the set (rebuilt when the set changes).
    pub sheets: Vec<SetSheet>,
    pub errors: Vec<String>,
    pub stale: bool,
    pub filter: String,
    pub message: String,
}

impl Default for SetsState {
    fn default() -> Self {
        Self {
            set: DrawingSet {
                name: "New Set".into(),
                files: Vec::new(),
            },
            path: None,
            sort: SetSort::FileOrder,
            sheets: Vec::new(),
            errors: Vec::new(),
            stale: false,
            filter: String::new(),
            message: String::new(),
        }
    }
}

impl SetsState {
    /// Re-read the sheets when the set changed.
    pub fn sheets(&mut self) -> &[SetSheet] {
        if self.stale {
            let (s, e) = set_sheets(&self.set, self.sort);
            self.sheets = s;
            self.errors = e;
            self.stale = false;
        }
        &self.sheets
    }
}

pub fn open_set(app: &mut AppState, path: &Path) {
    let s = &mut app.features.sets;
    match load_set(path) {
        Ok(set) => {
            s.message = format!("{}: {}", set.name, actions::plural(set.files.len(), "file"));
            s.set = set;
            s.path = Some(path.to_path_buf());
            s.stale = true;
        }
        Err(e) => s.message = e.to_string(),
    }
    app.show_panel("sets");
}

pub fn save_set(app: &mut AppState, path: &Path) {
    let s = &mut app.features.sets;
    s.message = actions::report(save_set_file(path, &s.set), |_| format!("Saved {}", path.display()));
    s.path = Some(path.to_path_buf());
}

fn save_set_file(path: &Path, set: &DrawingSet) -> markupcraft_engine::Result<()> {
    write_set(path, set)
}

pub fn add_files(app: &mut AppState, paths: &[PathBuf]) {
    let s = &mut app.features.sets;
    for p in paths {
        if !s.set.files.contains(p) {
            s.set.files.push(p.clone());
        }
    }
    s.stale = true;
}

/// Open a sheet: its file (or its tab) at its page.
pub fn open_sheet(app: &mut AppState, sheet: &SetSheet) {
    let Some(file) = app.features.sets.set.files.get(sheet.file).cloned() else {
        return;
    };
    app.open_path(&file);
    if let Some(d) = app.doc_mut()
        && d.path.as_deref() == Some(file.as_path())
    {
        let n = d.session.page_count();
        d.view.go_to_page(sheet.page, n);
    }
}
