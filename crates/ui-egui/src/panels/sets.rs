//! Sets (Alt+2): a drawing set of many PDFs, listed sheet by sheet (page labels), sorted by
//! file order or sheet number; click a sheet to open it.

use egui::RichText;
use markupcraft_engine::batch::SetSort;

use super::{PanelDef, Slot};
use crate::AppState;
use crate::commands::alt;
use crate::dialogs::{PDF, Purpose};
use crate::features::{self, Ask, sets};

pub static PANEL: PanelDef = PanelDef {
    id: "sets",
    title: "Sets",
    icon: "files",
    slot: Slot::Left,
    keys: alt(egui::Key::Num2),
    ui,
};

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let active_path = app.doc().and_then(|d| d.path.clone());
    let active_page = app.doc().map_or(0, |d| d.view.current);
    let (mut ask, mut open_sheet, mut remove, mut new) = (None, None, None, false);
    {
        let s = &mut app.features.sets;
        ui.horizontal_wrapped(|ui| {
            if ui.button("New").clicked() {
                new = true;
            }
            if ui.button("Open...").clicked() {
                ask = Some(Ask::SetOpen);
            }
            if ui.button("Save...").clicked() {
                ask = Some(Ask::SetSave);
            }
            if ui.button("Add Files...").clicked() {
                ask = Some(Ask::SetAddFiles);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut s.set.name);
        });
        ui.horizontal(|ui| {
            let before = s.sort;
            ui.label("Sort");
            ui.selectable_value(&mut s.sort, SetSort::FileOrder, "File order");
            ui.selectable_value(&mut s.sort, SetSort::Label, "Sheet");
            ui.selectable_value(&mut s.sort, SetSort::FileThenLabel, "File, sheet");
            if s.sort != before {
                s.stale = true;
            }
        });
        ui.add(egui::TextEdit::singleline(&mut s.filter).hint_text("Filter sheets"));
        ui.separator();
        if s.set.files.is_empty() {
            super::empty(ui, "Add PDFs to make a set: their sheets are listed here as one.");
        }
        let files = s.set.files.clone();
        let filter = s.filter.to_lowercase();
        let sheets = s.sheets().to_vec();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut last = None;
            for sh in sheets
                .iter()
                .filter(|sh| filter.is_empty() || sh.label.to_lowercase().contains(&filter))
            {
                if last != Some(sh.file) {
                    last = Some(sh.file);
                    ui.horizontal(|ui| {
                        let name = files
                            .get(sh.file)
                            .and_then(|f| f.file_name())
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                        ui.label(RichText::new(name).strong().small());
                        if ui.small_button("x").on_hover_text("Remove from the set").clicked() {
                            remove = Some(sh.file);
                        }
                    });
                }
                let current =
                    files.get(sh.file).is_some_and(|f| Some(f) == active_path.as_ref()) && sh.page == active_page;
                let label = if sh.label.is_empty() {
                    format!("Page {}", sh.page + 1)
                } else {
                    sh.label.clone()
                };
                if ui.selectable_label(current, label).clicked() {
                    open_sheet = Some(sh.clone());
                }
            }
        });
        for e in &s.errors {
            ui.label(RichText::new(e).small().weak());
        }
        if !s.message.is_empty() {
            ui.label(RichText::new(&s.message).small());
        }
    }
    if new {
        app.features.sets = Default::default();
    }
    if let Some(i) = remove {
        let s = &mut app.features.sets;
        if i < s.set.files.len() {
            s.set.files.remove(i);
            s.stale = true;
        }
    }
    if let Some(sh) = open_sheet {
        sets::open_sheet(app, &sh);
    }
    match ask {
        Some(Ask::SetOpen) => app.dialogs.open(Purpose::Feature(Ask::SetOpen), features::SET, false),
        Some(Ask::SetSave) => {
            let name = format!("{}.pcset", app.features.sets.set.name.trim());
            app.dialogs.save(Purpose::Feature(Ask::SetSave), features::SET, &name);
        }
        Some(Ask::SetAddFiles) => app.dialogs.open(Purpose::Feature(Ask::SetAddFiles), PDF, true),
        _ => {}
    }
}
