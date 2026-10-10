//! The shared batch file list's further sources and actions: the open documents, the current
//! Set, a folder with or without its subfolders, drag to reorder, and saving / loading the list
//! (JSON). Also the batch processes Split (every file split into parts) and Run Script (tool
//! steps run on every file), and Create PDF's one-PDF-per-file output.

use std::path::{Path, PathBuf};

use markupcraft_engine::{EngineError, Result};

fn invalid(m: impl Into<String>) -> EngineError {
    EngineError::Invalid(m.into())
}
use serde_json::{Value, json};

/// Options of the list and the extra batch kinds.
#[derive(Debug, Clone, PartialEq)]
pub struct ListExtras {
    /// Add Folder takes subfolders too.
    pub recursive: bool,
    /// Create PDF: one PDF per file; beside each source (else in a chosen folder).
    pub each: bool,
    pub beside_source: bool,
    /// Split: pages per part (0 = at top-level bookmarks).
    pub split_pages: u32,
    /// Run Script: the script file (`[{"tool", "params"}]`).
    pub script: Option<PathBuf>,
}

impl Default for ListExtras {
    fn default() -> Self {
        Self {
            recursive: true,
            each: false,
            beside_source: true,
            split_pages: 1,
            script: None,
        }
    }
}

/// What the list's buttons ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
    Folder,
    SaveList,
    LoadList,
    Script,
}

/// A file row's name as a drag handle; a row dropped here reports (from, to).
pub fn row(ui: &mut egui::Ui, i: usize, f: &Path, moved: &mut Option<(usize, usize)>) {
    let name = f
        .file_name()
        .map_or_else(|| f.display().to_string(), |n| n.to_string_lossy().into_owned());
    let r = ui
        .dnd_drag_source(egui::Id::new(("batch-row", i)), i, |ui| {
            ui.label(format!("\u{2195} {name}"));
        })
        .response
        .on_hover_text(f.display().to_string());
    if let Some(from) = r.dnd_release_payload::<usize>() {
        *moved = Some((*from, i));
    }
}

/// Move a row from `from` to `to`.
pub fn move_row(files: &mut Vec<PathBuf>, from: usize, to: usize) {
    if from < files.len() && to < files.len() && from != to {
        let f = files.remove(from);
        files.insert(to, f);
    }
}

/// The extra sources under the list.
pub fn sources_ui(
    ui: &mut egui::Ui,
    files: &mut Vec<PathBuf>,
    open_docs: &[PathBuf],
    set_files: &[PathBuf],
    ex: &mut ListExtras,
) -> Option<Want> {
    let mut want = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(!open_docs.is_empty(), egui::Button::new("Add Open Files"))
            .on_hover_text("Every open document that is saved to a file")
            .clicked()
        {
            add(files, open_docs);
        }
        if ui
            .add_enabled(!set_files.is_empty(), egui::Button::new("Add Current Set"))
            .clicked()
        {
            add(files, set_files);
        }
        if ui.button("Add Folder...").clicked() {
            want = Some(Want::Folder);
        }
        ui.checkbox(&mut ex.recursive, "Subfolders");
        if ui.button("Save List...").clicked() {
            want = Some(Want::SaveList);
        }
        if ui.button("Load List...").clicked() {
            want = Some(Want::LoadList);
        }
        if !files.is_empty() && ui.button("Clear").clicked() {
            files.clear();
        }
    });
    want
}

fn add(files: &mut Vec<PathBuf>, more: &[PathBuf]) {
    for p in more {
        if !files.contains(p) && files.len() < markupcraft_engine::batch::MAX_FILES {
            files.push(p.clone());
        }
    }
}

/// Save a file list (JSON: {"files": [...]}).
pub fn save_list(path: &Path, files: &[PathBuf]) -> Result<()> {
    let v = json!({ "format": "markupcraft-file-list", "files": files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>() });
    let b = serde_json::to_vec_pretty(&v).map_err(|e| invalid(e.to_string()))?;
    markupcraft_engine::finish::write_file(path, &b)
}

/// Load a file list written by [`save_list`] (or a plain JSON array of paths).
pub fn load_list(path: &Path) -> Result<Vec<PathBuf>> {
    let len = std::fs::metadata(path)
        .map_err(|e| invalid(format!("{}: {e}", path.display())))?
        .len();
    if len > 4 << 20 {
        return Err(invalid("the list file is too large"));
    }
    let b = std::fs::read(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
    let v: Value = serde_json::from_slice(&b).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
    let arr = v
        .get("files")
        .unwrap_or(&v)
        .as_array()
        .ok_or_else(|| invalid("not a file list"))?;
    Ok(arr
        .iter()
        .filter_map(Value::as_str)
        .take(markupcraft_engine::batch::MAX_FILES)
        .map(PathBuf::from)
        .collect())
}

/// Run Script on every file: the script's steps (`[{"tool", "params" | "args"}]`) as one
/// batch_apply, each file saved in place. Returns (files done, errors).
pub fn run_script(files: &[PathBuf], script: &Path) -> std::result::Result<(usize, Vec<String>), String> {
    let text = std::fs::read_to_string(script).map_err(|e| format!("{}: {e}", script.display()))?;
    let steps: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", script.display()))?;
    let ops: Vec<Value> = steps
        .as_array()
        .ok_or("a script is a list of {\"tool\", \"params\"} steps")?
        .iter()
        .map(|s| {
            let args = s
                .get("params")
                .or_else(|| s.get("args"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            json!({ "tool": s.get("tool").cloned().unwrap_or(Value::Null), "args": args })
        })
        .collect();
    let mut a = markupcraft_automation::Automation::new();
    let list: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
    let v = a
        .call("batch_apply", &json!({ "files": list, "operations": ops }))
        .map_err(|e| e.to_string())?;
    let done = v.get("done").and_then(Value::as_u64).unwrap_or(0) as usize;
    let errors = v
        .get("files")
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .filter_map(|f| f.get("error").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok((done, errors))
}
