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
    /// Categories: off, by file name, by sheet number.
    pub categories: markupcraft_engine::sets_more::CategoryMode,
    /// Revisions: how earlier versions show (0 shown, 1 hidden, 2 greyed, 3 crossed out).
    pub previous: u8,
    pub revision_filter: String,
    pub show_tags: bool,
    /// Edit Tags: the sheet key, tag name and value.
    pub tag_sheet: String,
    pub tag_name: String,
    pub tag_value: String,
    /// Publish: latest versions only.
    pub latest_only: bool,
}

impl Default for SetsState {
    fn default() -> Self {
        Self {
            set: DrawingSet {
                name: "New Set".into(),
                files: Vec::new(),
                ..Default::default()
            },
            path: None,
            sort: SetSort::FileOrder,
            sheets: Vec::new(),
            errors: Vec::new(),
            stale: false,
            filter: String::new(),
            message: String::new(),
            categories: Default::default(),
            previous: 2,
            revision_filter: String::new(),
            show_tags: false,
            tag_sheet: String::new(),
            tag_name: String::new(),
            tag_value: String::new(),
            latest_only: true,
        }
    }
}

impl SetsState {
    /// The files of the open Set.
    pub fn files(&self) -> Vec<std::path::PathBuf> {
        self.set.files.clone()
    }

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

/// Publish, package and print a Set (the file or folder was chosen).
pub fn publish_file(app: &mut AppState, ask: &super::Ask, path: &Path) {
    use markupcraft_engine::sets_more::{print_set, publish_combined, publish_package};
    let s = &mut app.features.sets;
    s.message = match ask {
        super::Ask::SetPublishPdf => {
            actions::report(publish_combined(&s.set, path, s.latest_only, &s.revision_filter), |r| {
                format!("Published {} to {}", actions::plural(r.pages, "sheet"), path.display())
            })
        }
        super::Ask::SetPackageDir => actions::report(publish_package(&s.set, path), |r| {
            format!(
                "Packaged {} with a drawing log of {} rows",
                actions::plural(r.files.len(), "file"),
                r.log_rows
            )
        }),
        super::Ask::SetPrintOut => actions::report(
            print_set(&s.set, &markupcraft_engine::printing::PrintJob::default(), path),
            |n| format!("Set printed: {} in {}", actions::plural(n, "sheet"), path.display()),
        ),
        _ => return,
    };
    app.status = app.features.sets.message.clone();
}

/// Edit Tags: set a custom tag on a sheet (saved when the set is saved).
pub fn set_tag(app: &mut AppState) {
    let s = &mut app.features.sets;
    let (k, n, v) = (
        s.tag_sheet.trim().to_string(),
        s.tag_name.trim().to_string(),
        s.tag_value.trim().to_string(),
    );
    if k.is_empty() || n.is_empty() {
        s.message = "Choose a sheet and a tag name".into();
        return;
    }
    let entry = s.set.tags.entry(k.clone()).or_default();
    if v.is_empty() {
        entry.remove(&n);
    } else {
        entry.insert(n.clone(), v);
    }
    if entry.is_empty() {
        s.set.tags.remove(&k);
    }
    s.message = format!("Tag {n} set on {k}; save the set to keep it");
}
