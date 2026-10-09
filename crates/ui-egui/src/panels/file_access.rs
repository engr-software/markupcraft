//! File Access (Alt+A): recent files (newest first, by folder, most used, by name, or the
//! history grouped by day) with their folder on hover; click opens, Ctrl+click opens behind the
//! current tab; pin a file so it never drops off, in a named category (categories collapse, and
//! rename or unpin from their right-click menu); remove one or clear the list. The Explorer tab
//! browses folders: a path box (Enter goes there), the drives, back / forward / up, sort by
//! name, type, size or date, PDFs only or PDFs and images, a new folder, pin the folder's files;
//! right-click a file to open it, open its folder, rename it, delete it from disk (asks first)
//! or see its properties.

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

/// How the Explorer sorts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExSort {
    #[default]
    Name,
    Type,
    Size,
    Date,
}

/// The panel's view state (kept in egui memory; the Explorer's folder history too).
#[derive(Clone, Default)]
pub struct View {
    pub sort: Sort,
    pub explorer: bool,
    pub folder: Option<PathBuf>,
    pub category: String,
    /// Explorer: folders visited before / after this one.
    pub back: Vec<PathBuf>,
    pub forward: Vec<PathBuf>,
    pub path_text: String,
    pub ex_sort: ExSort,
    /// Explorer: list images as well as PDFs.
    pub all_types: bool,
    /// A pinned category being renamed: (old, new).
    pub renaming_category: Option<(String, String)>,
    /// A file being renamed in the Explorer: (path, new name).
    pub renaming_file: Option<(PathBuf, String)>,
    /// A file waiting for "Delete from disk?" to be confirmed.
    pub confirm_delete: Option<PathBuf>,
    /// A file whose properties show.
    pub properties: Option<PathBuf>,
}

pub fn view_id() -> egui::Id {
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

fn row(app: &mut AppState, ui: &mut egui::Ui, path: &Path, detail: &str, view: &mut View, explorer: bool) {
    let t = Tokens::get(ui.ctx());
    let exists = path.is_file();
    ui.horizontal(|ui| {
        let pinned = app.shell.recent.is_pinned(path);
        if crate::icons::button(ui, "pin", 16.0, pinned, if pinned { "Unpin" } else { "Pin" }).clicked() {
            let cat = view.category.clone();
            app.shell.recent.toggle_pin(path, &cat);
            crate::shell::recent::persist(app);
        }
        if let Some((p, text)) = &mut view.renaming_file
            && p == path
        {
            let r = ui.add(egui::TextEdit::singleline(text).desired_width(160.0));
            r.request_focus();
            let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if enter {
                let to = path.with_file_name(text.trim());
                let res = rename_file(path, &to);
                app.status = match res {
                    Ok(()) => format!("Renamed to {}", name_of(&to)),
                    Err(e) => e,
                };
                view.renaming_file = None;
            } else if esc {
                view.renaming_file = None;
            }
            return;
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
            if ui.button("Open Folder").clicked() {
                if let Some(dir) = path.parent() {
                    if app.shell.extra.launch {
                        crate::shell::files::launch(dir);
                    }
                    app.status = format!("Opened {}", dir.display());
                }
                ui.close();
            }
            if ui.button("Copy Path").clicked() {
                ui.ctx().copy_text(path.display().to_string());
                ui.close();
            }
            if explorer {
                ui.separator();
                if ui.button("Rename").clicked() {
                    view.renaming_file = Some((path.to_path_buf(), name_of(path)));
                    ui.close();
                }
                if ui.button("Delete from Disk...").clicked() {
                    view.confirm_delete = Some(path.to_path_buf());
                    ui.close();
                }
                if ui.button("Properties").clicked() {
                    view.properties = Some(path.to_path_buf());
                    ui.close();
                }
            } else if ui.button("Remove from List").clicked() {
                app.shell.recent.files.retain(|f| f.path != path);
                app.shell.recent.pinned.retain(|f| f.path != path);
                crate::shell::recent::persist(app);
                ui.close();
            }
        });
    });
}

/// Rename a file on disk (refuses to replace another file or to rename an open document).
pub fn rename_file(from: &Path, to: &Path) -> Result<(), String> {
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.is_empty() || name.contains(['/', '\\']) {
        return Err("Type a file name".into());
    }
    if to.exists() {
        return Err(format!("{name} already exists"));
    }
    std::fs::rename(from, to).map_err(|e| format!("Could not rename: {e}"))
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
    dialogs(app, ui, &mut view);
    ui.data_mut(|d| d.insert_temp(view_id(), view));
}

fn dialogs(app: &mut AppState, ui: &mut egui::Ui, view: &mut View) {
    if let Some(p) = view.confirm_delete.clone() {
        let mut answer = None;
        egui::Window::new("Delete File")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label(format!("Delete {} from the disk? This cannot be undone.", name_of(&p)));
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        answer = Some(false);
                    }
                });
            });
        if let Some(a) = answer {
            view.confirm_delete = None;
            if a {
                if app.docs.iter().any(|d| d.path.as_deref() == Some(p.as_path())) {
                    app.status = format!("{} is open: close it first", name_of(&p));
                } else {
                    app.status = match std::fs::remove_file(&p) {
                        Ok(()) => format!("Deleted {}", name_of(&p)),
                        Err(e) => format!("Could not delete {}: {e}", name_of(&p)),
                    };
                }
            }
        }
    }
    if let Some(p) = view.properties.clone() {
        let mut open = true;
        let meta = std::fs::metadata(&p).ok();
        egui::Window::new("File Properties")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                egui::Grid::new("file-props").num_columns(2).show(ui, |ui| {
                    ui.label("Name");
                    ui.label(name_of(&p));
                    ui.end_row();
                    ui.label("Folder");
                    ui.label(p.parent().map(|d| d.display().to_string()).unwrap_or_default());
                    ui.end_row();
                    if let Some(m) = &meta {
                        ui.label("Size");
                        ui.label(format!("{} KB", m.len().div_ceil(1024)));
                        ui.end_row();
                        let secs = m
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map_or(0, |d| d.as_secs());
                        ui.label("Modified");
                        ui.label(crate::shell::recent::day_label(secs, crate::shell::recent::now_secs()));
                        ui.end_row();
                        ui.label("Read-only");
                        ui.label(if m.permissions().readonly() { "Yes" } else { "No" });
                        ui.end_row();
                    }
                });
            });
        if !open {
            view.properties = None;
        }
    }
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
                Sort::History => "History",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut view.sort, Sort::Date, "Date");
                ui.selectable_value(&mut view.sort, Sort::Folder, "Folder");
                ui.selectable_value(&mut view.sort, Sort::MostUsed, "Most used");
                ui.selectable_value(&mut view.sort, Sort::Name, "Name");
                ui.selectable_value(&mut view.sort, Sort::History, "History");
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
        // Pinned, by category (each one collapses; right-click to rename or unpin it).
        let mut pinned = app.shell.recent.pinned.clone();
        if !pinned.is_empty() {
            pinned.sort_by(|a, b| {
                a.category
                    .cmp(&b.category)
                    .then(name_of(&a.path).cmp(&name_of(&b.path)))
            });
            ui.label(RichText::new("Pinned").strong());
            let mut cats: Vec<String> = pinned.iter().map(|p| p.category.clone()).collect();
            cats.dedup();
            for cat in cats {
                let files: Vec<PathBuf> = pinned
                    .iter()
                    .filter(|p| p.category == cat)
                    .map(|p| p.path.clone())
                    .collect();
                if let Some((old, new)) = &mut view.renaming_category
                    && *old == cat
                {
                    let r = ui.add(egui::TextEdit::singleline(new).desired_width(140.0));
                    r.request_focus();
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let (o, n) = (old.clone(), new.clone());
                        app.shell.recent.rename_category(&o, &n);
                        crate::shell::recent::persist(app);
                        view.renaming_category = None;
                    } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        view.renaming_category = None;
                    }
                    continue;
                }
                let title = if cat.is_empty() {
                    "Ungrouped".to_string()
                } else {
                    cat.clone()
                };
                let h = egui::CollapsingHeader::new(RichText::new(title).italics())
                    .id_salt(("fa-cat", &cat))
                    .default_open(true)
                    .show(ui, |ui| {
                        for p in &files {
                            row(app, ui, p, "Pinned", view, false);
                        }
                    });
                h.header_response.context_menu(|ui| {
                    if ui.button("Rename Category").clicked() {
                        view.renaming_category = Some((cat.clone(), cat.clone()));
                        ui.close();
                    }
                    if ui.button("Unpin All").clicked() {
                        app.shell.recent.remove_category(&cat);
                        crate::shell::recent::persist(app);
                        ui.close();
                    }
                });
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
        let now = crate::shell::recent::now_secs();
        let mut folder: Option<PathBuf> = None;
        let mut day: Option<String> = None;
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
            if view.sort == Sort::History {
                let d = crate::shell::recent::day_label(f.opened, now);
                if day.as_ref() != Some(&d) {
                    ui.label(RichText::new(&d).size(11.0).strong());
                    day = Some(d);
                }
            }
            let detail = format!("Opened {} time(s)", f.count);
            row(app, ui, &f.path, &detail, view, false);
        }
    });
}

/// The drives (Windows: the lettered drives that exist; elsewhere the root).
fn drives() -> Vec<PathBuf> {
    if cfg!(windows) {
        (b'A'..=b'Z')
            .map(|c| PathBuf::from(format!("{}:\\", char::from(c))))
            .filter(|p| p.is_dir())
            .collect()
    } else {
        vec![PathBuf::from("/")]
    }
}

/// Go to `to` in the Explorer (the folder it leaves goes on the back list).
pub fn go(view: &mut View, from: &Path, to: PathBuf) {
    if to.as_path() != from {
        view.back.push(from.to_path_buf());
        view.back.truncate(100);
        view.forward.clear();
    }
    view.path_text = to.display().to_string();
    view.folder = Some(to);
}

/// The Explorer's entries of `folder`: folders first, then files, sorted.
pub fn entries(folder: &Path, sort: ExSort, all_types: bool) -> Vec<(bool, PathBuf)> {
    let wanted = |p: &Path| {
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        ext == "pdf" || (all_types && markupcraft_engine::docfile::IMAGE_EXTS.contains(&ext.as_str()))
    };
    let mut v: Vec<(bool, PathBuf)> = std::fs::read_dir(folder)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .take(MAX_LIST)
                .map(|e| e.path())
                .filter(|p| p.is_dir() || wanted(p))
                .map(|p| (p.is_dir(), p))
                .collect()
        })
        .unwrap_or_default();
    let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
    let date = |p: &Path| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs())
    };
    let ext = |p: &Path| {
        p.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    };
    v.sort_by(|a, b| {
        let by = match sort {
            ExSort::Name => std::cmp::Ordering::Equal,
            ExSort::Type => ext(&a.1).cmp(&ext(&b.1)),
            ExSort::Size => size(&b.1).cmp(&size(&a.1)),
            ExSort::Date => date(&b.1).cmp(&date(&a.1)),
        };
        b.0.cmp(&a.0)
            .then(by)
            .then(name_of(&a.1).to_lowercase().cmp(&name_of(&b.1).to_lowercase()))
    });
    v
}

/// A new folder in `parent` ("New Folder", "New Folder 2", ...).
pub fn new_folder(parent: &Path) -> Result<PathBuf, String> {
    for n in 1..100 {
        let name = if n == 1 {
            "New Folder".to_string()
        } else {
            format!("New Folder {n}")
        };
        let p = parent.join(name);
        if !p.exists() {
            return std::fs::create_dir(&p).map(|_| p).map_err(|e| e.to_string());
        }
    }
    Err("too many new folders".into())
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
    if view.path_text.is_empty() {
        view.path_text = folder.display().to_string();
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!view.back.is_empty(), egui::Button::new("<").small())
            .on_hover_text("Back")
            .clicked()
            && let Some(p) = view.back.pop()
        {
            view.forward.push(folder.clone());
            view.path_text = p.display().to_string();
            view.folder = Some(p);
        }
        if ui
            .add_enabled(!view.forward.is_empty(), egui::Button::new(">").small())
            .on_hover_text("Forward")
            .clicked()
            && let Some(p) = view.forward.pop()
        {
            view.back.push(folder.clone());
            view.path_text = p.display().to_string();
            view.folder = Some(p);
        }
        if ui.small_button("Up").clicked()
            && let Some(p) = folder.parent()
        {
            go(view, &folder, p.to_path_buf());
        }
        egui::ComboBox::from_id_salt("fa-drive")
            .selected_text("Drives")
            .width(60.0)
            .show_ui(ui, |ui| {
                for d in drives() {
                    if ui.button(d.display().to_string()).clicked() {
                        go(view, &folder, d);
                    }
                }
            });
    });
    let r = ui.add(
        egui::TextEdit::singleline(&mut view.path_text)
            .desired_width(f32::INFINITY)
            .hint_text("Folder path"),
    );
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        let p = PathBuf::from(view.path_text.trim());
        if p.is_dir() {
            go(view, &folder, p);
        } else if p.is_file() {
            open(app, &p, false);
        } else {
            app.status = format!("{} is not a folder", p.display());
        }
    }
    // Path completion: folders that start with what is typed.
    if r.has_focus() {
        let typed = PathBuf::from(&view.path_text);
        if let (Some(parent), Some(stem)) = (
            typed.parent(),
            typed.file_name().map(|s| s.to_string_lossy().to_lowercase()),
        ) {
            let hits: Vec<PathBuf> = std::fs::read_dir(parent)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .take(MAX_LIST)
                        .map(|e| e.path())
                        .filter(|p| p.is_dir() && name_of(p).to_lowercase().starts_with(&stem))
                        .take(8)
                        .collect()
                })
                .unwrap_or_default();
            for h in hits {
                if ui.small_button(name_of(&h)).clicked() {
                    go(view, &folder, h);
                }
            }
        }
    }
    let folder = view.folder.clone().unwrap_or(folder);
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("fa-ex-sort")
            .selected_text(match view.ex_sort {
                ExSort::Name => "Name",
                ExSort::Type => "Type",
                ExSort::Size => "Size",
                ExSort::Date => "Date",
            })
            .width(60.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut view.ex_sort, ExSort::Name, "Name");
                ui.selectable_value(&mut view.ex_sort, ExSort::Type, "Type");
                ui.selectable_value(&mut view.ex_sort, ExSort::Size, "Size");
                ui.selectable_value(&mut view.ex_sort, ExSort::Date, "Date");
            });
        ui.checkbox(&mut view.all_types, "Images");
        if ui.small_button("New Folder").clicked() {
            app.status = match new_folder(&folder) {
                Ok(p) => format!("Made {}", name_of(&p)),
                Err(e) => format!("Could not make a folder: {e}"),
            };
        }
        if ui
            .small_button("Pin Folder")
            .on_hover_text("Pin this folder's PDFs into a category named after it")
            .clicked()
        {
            let cat = name_of(&folder);
            for (dir, p) in entries(&folder, ExSort::Name, false) {
                if !dir && !app.shell.recent.is_pinned(&p) {
                    app.shell.recent.toggle_pin(&p, &cat);
                }
            }
            crate::shell::recent::persist(app);
        }
    });
    let list = entries(&folder, view.ex_sort, view.all_types);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (dir, p) in &list {
            if *dir {
                if ui.button(format!("[{}]", name_of(p))).clicked() {
                    go(view, &folder, p.clone());
                }
            } else {
                row(app, ui, p, "", view, true);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_sorts_filters_makes_folders_and_renames() {
        let d = std::env::temp_dir().join(format!("markupcraft-fa-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("b.pdf"), b"%PDF-1.7 bigger file").unwrap();
        std::fs::write(d.join("a.pdf"), b"%PDF").unwrap();
        std::fs::write(d.join("c.png"), b"png").unwrap();
        let names = |v: Vec<(bool, PathBuf)>| v.iter().map(|(_, p)| name_of(p)).collect::<Vec<_>>();
        assert_eq!(names(entries(&d, ExSort::Name, false)), ["a.pdf", "b.pdf"]);
        assert_eq!(names(entries(&d, ExSort::Size, false)), ["b.pdf", "a.pdf"]);
        assert_eq!(names(entries(&d, ExSort::Type, true)), ["a.pdf", "b.pdf", "c.png"]);
        let f = new_folder(&d).unwrap();
        assert_eq!(name_of(&f), "New Folder");
        assert_eq!(name_of(&new_folder(&d).unwrap()), "New Folder 2");
        assert_eq!(names(entries(&d, ExSort::Name, false))[0], "New Folder");
        assert!(rename_file(&d.join("a.pdf"), &d.join("b.pdf")).is_err());
        rename_file(&d.join("a.pdf"), &d.join("z.pdf")).unwrap();
        assert!(d.join("z.pdf").is_file());
        let mut v = View::default();
        go(&mut v, &d, f.clone());
        assert_eq!(v.back, vec![d.clone()]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
