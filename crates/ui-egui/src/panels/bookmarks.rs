//! Bookmarks (Alt+B): the PDF outline. Click an entry to go to its page; add (Ctrl+B), rename,
//! re-target to the current page, delete, reorder and nest, expand and collapse (saved in the
//! file), create one per page from the page labels, or clear them all. Every edit is an engine
//! call and undoable.

use egui::RichText;
use markupcraft_engine::bookmarks::{BookmarkItem, BookmarkTitles};

use super::{PanelDef, Slot};
use crate::commands::alt;
use crate::{AppState, actions};

pub static PANEL: PanelDef = PanelDef {
    id: "bookmarks",
    title: "Bookmarks",
    icon: "bookmark",
    slot: Slot::Left,
    keys: alt(egui::Key::B),
    ui,
};

/// The panel's selection and rename field (kept in `AppState::features`).
#[derive(Default)]
pub struct Fields {
    pub selected: Option<Vec<usize>>,
    pub rename: String,
}

enum Act {
    Go(usize),
    Toggle(Vec<usize>, bool),
    Add,
    Rename(Vec<usize>, String),
    SetPage(Vec<usize>),
    Delete(Vec<usize>),
    Move(Vec<usize>, Vec<usize>, Option<usize>),
    FromPages,
    Clear,
}

fn split(path: &[usize]) -> (Vec<usize>, usize) {
    match path.split_last() {
        Some((last, parent)) => (parent.to_vec(), *last),
        None => (Vec::new(), 0),
    }
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let Some(d) = app.doc() else {
        super::empty(ui, "No document open.");
        return;
    };
    let items: Vec<BookmarkItem> = d.session.bookmarks();
    let current = d.view.current;
    let mut f = std::mem::take(&mut app.features.bookmarks);
    let mut act = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Add")
            .on_hover_text("Bookmark the current page (Ctrl+B)")
            .clicked()
        {
            act = Some(Act::Add);
        }
        if ui
            .button("From Pages")
            .on_hover_text("One bookmark per page, titled by its label")
            .clicked()
        {
            act = Some(Act::FromPages);
        }
        ui.add_enabled_ui(!items.is_empty(), |ui| {
            if ui.button("Clear").clicked() {
                act = Some(Act::Clear);
            }
        });
    });
    if let Some(sel) = f.selected.clone().filter(|s| items.iter().any(|i| &i.path == s)) {
        let (parent, idx) = split(&sel);
        let siblings = items
            .iter()
            .filter(|i| i.path.len() == sel.len() && i.path.get(..i.path.len() - 1) == Some(&parent[..]))
            .count();
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Up").clicked() && idx > 0 {
                act = Some(Act::Move(sel.clone(), parent.clone(), Some(idx - 1)));
            }
            if ui.small_button("Down").clicked() && idx + 1 < siblings {
                act = Some(Act::Move(sel.clone(), parent.clone(), Some(idx + 1)));
            }
            if ui.small_button("Indent").clicked() && idx > 0 {
                let mut new_parent = parent.clone();
                new_parent.push(idx - 1);
                act = Some(Act::Move(sel.clone(), new_parent, None));
            }
            if ui.small_button("Outdent").clicked() && !parent.is_empty() {
                let (gp, pi) = split(&parent);
                act = Some(Act::Move(sel.clone(), gp, Some(pi + 1)));
            }
            if ui
                .small_button("Set Page")
                .on_hover_text("Go to the current page")
                .clicked()
            {
                act = Some(Act::SetPage(sel.clone()));
            }
            if ui.small_button("Delete").clicked() {
                act = Some(Act::Delete(sel.clone()));
            }
        });
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut f.rename).desired_width(150.0));
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.small_button("Rename").clicked() || enter) && !f.rename.trim().is_empty() {
                act = Some(Act::Rename(sel.clone(), f.rename.trim().to_string()));
            }
        });
    }
    ui.separator();
    if items.is_empty() {
        super::empty(ui, "This document has no bookmarks.");
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // Children of a collapsed entry are hidden.
        let mut hide_below: Option<usize> = None;
        for e in &items {
            let depth = e.path.len().saturating_sub(1);
            if let Some(h) = hide_below {
                if depth > h {
                    continue;
                }
                hide_below = None;
            }
            ui.horizontal(|ui| {
                ui.add_space(4.0 + 14.0 * depth.min(12) as f32);
                if e.children > 0 {
                    let icon = if e.open { "−" } else { "+" };
                    if ui.small_button(icon).clicked() {
                        act = Some(Act::Toggle(e.path.clone(), !e.open));
                    }
                } else {
                    ui.add_space(18.0);
                }
                let label = if e.title.trim().is_empty() {
                    "(untitled)"
                } else {
                    e.title.as_str()
                };
                let sel = f.selected.as_ref() == Some(&e.path);
                let text = if e.page == Some(current) {
                    RichText::new(label).strong()
                } else {
                    RichText::new(label)
                };
                let r = ui.selectable_label(sel, text);
                if r.clicked() {
                    f.selected = Some(e.path.clone());
                    f.rename = e.title.clone();
                    if let Some(p) = e.page {
                        act = Some(Act::Go(p));
                    }
                }
                if let Some(p) = e.page {
                    r.on_hover_text(format!("Page {}", p + 1));
                }
            });
            if e.children > 0 && !e.open {
                hide_below = Some(depth);
            }
        }
    });
    let Some(act) = act else {
        app.features.bookmarks = f;
        return;
    };
    let Some(d) = app.doc_mut() else { return };
    let count = d.session.page_count();
    let s = &mut d.session;
    let status = match act {
        Act::Go(p) => {
            d.view.go_to_page(p, count);
            None
        }
        Act::Toggle(p, open) => Some(actions::report(s.set_bookmark_open(&p, open), |_| String::new())),
        Act::Add => {
            let label = s.page_labels().get(current).cloned().unwrap_or_default();
            let title = if label.is_empty() {
                format!("Page {}", current + 1)
            } else {
                label
            };
            // A new bookmark goes after the selected one (else at the end).
            let (parent, index) = f.selected.as_ref().map_or((Vec::new(), None), |p| {
                let (pa, i) = split(p);
                (pa, Some(i + 1))
            });
            Some(actions::report(s.add_bookmark(&parent, index, &title, current), |p| {
                f.selected = Some(p);
                f.rename = title.clone();
                format!("Bookmark added: {title}")
            }))
        }
        Act::Rename(p, t) => Some(actions::report(s.rename_bookmark(&p, &t), |_| {
            format!("Renamed to {t}")
        })),
        Act::SetPage(p) => Some(actions::report(s.set_bookmark_page(&p, current), |_| {
            format!("Bookmark goes to page {}", current + 1)
        })),
        Act::Delete(p) => {
            f.selected = None;
            Some(actions::report(s.delete_bookmark(&p), |_| "Bookmark deleted".into()))
        }
        Act::Move(from, parent, index) => Some(actions::report(s.move_bookmark(&from, &parent, index), |p| {
            f.selected = Some(p);
            "Bookmark moved".into()
        })),
        Act::FromPages => {
            let all: Vec<usize> = (0..count).collect();
            Some(actions::report(
                s.bookmarks_from_pages(&all, BookmarkTitles::Labels, true),
                |n| format!("Made {}", actions::plural(n, "bookmark")),
            ))
        }
        Act::Clear => {
            f.selected = None;
            Some(actions::report(s.clear_bookmarks(), |n| {
                format!("Removed {}", actions::plural(n, "bookmark"))
            }))
        }
    };
    app.features.bookmarks = f;
    if let Some(s) = status.filter(|s| !s.is_empty()) {
        app.status = s;
    }
}
