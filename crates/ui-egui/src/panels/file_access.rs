//! File Access (Alt+A): recent files (newest first, by folder, most used or by name) with
//! their folder on hover; click opens, Ctrl+click opens behind the current tab; pin a file so it
//! never drops off, in a named category; remove one or clear the list. An Explorer tab lists a
//! folder's PDFs and subfolders (up, open folder, pin the folder's files).

use std::path::{Path, PathBuf};

use egui::RichText;

use super::{PanelDef, Slot};
use crate::AppState;
use crate::commands::alt;
use crate::shell::recent::Sort;
use crate::theme::Tokens;

pub static PANEL: PanelDef = PanelDef {
    id: "file_access",
    title: "File Access",
    icon: "folder-open",
    slot: Slot::Left,
    keys: alt(egui::Key::A),
    ui,
};

/// Most folder entries listed.
const MAX_LIST: usize = 2000;

#[derive(Clone, Default)]
struct View {
    sort: Sort,
    explorer: bool,
    folder: Option<PathBuf>,
    category: String,
}

fn view_id() -> egui::Id {
    egui::Id::new("file-access-view")
}

/// A file to open, and whether it opens in the background.
fn open(app: &mut AppState, path: &Path, background: bool) {
    let keep = app.active;
    let had = app.docs.len();
    app.open_path(path);
    if background && app.docs.len() > had {
        app.active = keep.min(app.docs.len().saturating_sub(1));
    }
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())
}

fn row(app: &mut AppState, ui: &mut egui::Ui, path: &Path, detail: &str, view: &View) {
    let t = Tokens::get(ui.ctx());
    let exists = path.is_file();
    ui.horizontal(|ui| {
        let pinned = app.shell.recent.is_pinned(path);
        if crate::icons::button(ui, "pin", 16.0, pinned, if pinned { "Unpin" } else { "Pin" }).clicked() {
            let cat = view.category.clone();
            app.shell.recent.toggle_pin(path, &cat);
            crate::shell::recent::persist(app);
        }
        let text = RichText::new(name_of(path)).color(if exists { t.text } else { t.text_faint });
        let r = ui
            .add(egui::Button::new(text).frame(false))
            .on_hover_text(format!("{}\n{detail}", path.display()));
        if r.clicked() && exists {
            let bg = ui.input(|i| i.modifiers.command);
            open(app, path, bg);
        }
        r.context_menu(|ui| {
            if ui.button("Open").clicked() {
                open(app, path, false);
                ui.close();
            }
            if ui.button("Open in Background").clicked() {
                open(app, path, true);
                ui.close();
            }
            if ui.button("Copy Path").clicked() {
                ui.ctx().copy_text(path.display().to_string());
                ui.close();
            }
            if ui.button("Remove from List").clicked() {
                app.shell.recent.files.retain(|f| f.path != path);
                app.shell.recent.pinned.retain(|f| f.path != path);
                crate::shell::recent::persist(app);
                ui.close();
            }
        });
    });
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let mut view: View = ui.data(|d| d.get_temp(view_id())).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.selectable_value(&mut view.explorer, false, "Recents");
        ui.selectable_value(&mut view.explorer, true, "Explorer");
    });
    ui.separator();
    if view.explorer {
        explorer(app, ui, &mut view);
    } else {
        recents(app, ui, &mut view);
    }
    ui.data_mut(|d| d.insert_temp(view_id(), view));
}

fn recents(app: &mut AppState, ui: &mut egui::Ui, view: &mut View) {
    ui.horizontal(|ui| {
        ui.label("Sort");
        egui::ComboBox::from_id_salt("fa-sort")
            .selected_text(match view.sort {
                Sort::Date => "Date",
                Sort::Folder => "Folder",
                Sort::MostUsed => "Most used",
                Sort::Name => "Name",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut view.sort, Sort::Date, "Date");
                ui.selectable_value(&mut view.sort, Sort::Folder, "Folder");
                ui.selectable_value(&mut view.sort, Sort::MostUsed, "Most used");
                ui.selectable_value(&mut view.sort, Sort::Name, "Name");
            });
        if ui
            .small_button("Clear")
            .on_hover_text("Clear the recent files")
            .clicked()
        {
            app.queue("file.clear_recent");
        }
    });
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // Pinned, by category.
        let mut pinned = app.shell.recent.pinned.clone();
        if !pinned.is_empty() {
            pinned.sort_by(|a, b| {
                a.category
                    .cmp(&b.category)
                    .then(name_of(&a.path).cmp(&name_of(&b.path)))
            });
            ui.label(RichText::new("Pinned").strong());
            let mut cat: Option<String> = None;
            for p in &pinned {
                if cat.as_deref() != Some(p.category.as_str()) && !p.category.is_empty() {
                    ui.label(RichText::new(&p.category).italics());
                }
                cat = Some(p.category.clone());
                row(app, ui, &p.path, "Pinned", view);
            }
            ui.separator();
        }
        ui.horizontal(|ui| {
            ui.label("Pin into category");
            ui.add(egui::TextEdit::singleline(&mut view.category).desired_width(100.0));
        });
        let files = app.shell.recent.sorted(view.sort);
        if files.is_empty() {
            super::empty(ui, "Files you open are listed here.");
        }
        let mut folder: Option<PathBuf> = None;
        for f in &files {
            if view.sort == Sort::Folder {
                let parent = f.path.parent().map(Path::to_path_buf);
                if parent != folder {
                    ui.label(
                        RichText::new(parent.as_ref().map_or_else(String::new, |p| p.display().to_string()))
                            .size(11.0)
                            .weak(),
                    );
                    folder = parent;
                }
            }
            let detail = format!("Opened {} time(s)", f.count);
            row(app, ui, &f.path, &detail, view);
        }
    });
}

fn explorer(app: &mut AppState, ui: &mut egui::Ui, view: &mut View) {
    let folder = view
        .folder
        .clone()
        .or_else(|| app.doc().and_then(|d| d.path.as_ref()?.parent().map(Path::to_path_buf)))
        .or_else(|| {
            app.shell
                .recent
                .files
                .first()
                .and_then(|f| f.path.parent().map(Path::to_path_buf))
        })
        .or_else(|| std::env::current_dir().ok());
    let Some(folder) = folder else {
        super::empty(ui, "No folder.");
        return;
    };
    ui.horizontal(|ui| {
        if ui.small_button("Up").clicked()
            && let Some(p) = folder.parent()
        {
            view.folder = Some(p.to_path_buf());
        }
        ui.label(RichText::new(folder.display().to_string()).size(11.0));
    });
    let mut entries: Vec<(bool, PathBuf)> = std::fs::read_dir(&folder)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .take(MAX_LIST)
                .map(|e| e.path())
                .filter(|p| p.is_dir() || p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")))
                .map(|p| (p.is_dir(), p))
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(name_of(&a.1).to_lowercase().cmp(&name_of(&b.1).to_lowercase()))
    });
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (dir, p) in &entries {
            if *dir {
                if ui.button(format!("[{}]", name_of(p))).clicked() {
                    view.folder = Some(p.clone());
                }
            } else {
                row(app, ui, p, "", view);
            }
        }
    });
}
