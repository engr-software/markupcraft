//! Sets (Alt+2): a drawing set of many PDFs, listed sheet by sheet (page labels), sorted by
//! file order or sheet number; click a sheet to open it.

use egui::RichText;
use markupcraft_engine::batch::SetSort;
use markupcraft_engine::sets_more::{CategoryMode, category_of, revisions, tagged_sheets_with};

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
    // Preferences > Sets: category templates, auto-tagging, stacked revisions.
    let sp = app.shell.prefs.more.sets.clone();
    let (mut ask, mut open_sheet, mut remove, mut new) = (None, None, None, false);
    let (mut publish, mut package, mut print_set, mut set_tag) = (false, false, false, false);
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
        ui.horizontal_wrapped(|ui| {
            ui.label("Categories");
            ui.selectable_value(&mut s.categories, CategoryMode::Off, "Off");
            ui.selectable_value(&mut s.categories, CategoryMode::FileName, "File name");
            ui.selectable_value(&mut s.categories, CategoryMode::SheetNumber, "Sheet number");
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Earlier revisions");
            ui.selectable_value(&mut s.previous, 0, "Shown");
            ui.selectable_value(&mut s.previous, 1, "Hidden");
            ui.selectable_value(&mut s.previous, 2, "Greyed");
            ui.selectable_value(&mut s.previous, 3, "Crossed out");
            ui.add(
                egui::TextEdit::singleline(&mut s.revision_filter)
                    .hint_text("filter e.g. @?#")
                    .desired_width(70.0),
            )
            .on_hover_text("Wildcard for the sheet key in file names; empty = by sheet number");
            ui.checkbox(&mut s.show_tags, "Tags");
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut s.latest_only, "Latest only");
            if ui
                .small_button("Publish...")
                .on_hover_text("One PDF with a bookmark per sheet")
                .clicked()
            {
                publish = true;
            }
            if ui
                .small_button("Package...")
                .on_hover_text("Copy the files with a drawing log")
                .clicked()
            {
                package = true;
            }
            if ui.small_button("Print Set...").clicked() {
                print_set = true;
            }
        });
        if s.show_tags {
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut s.tag_sheet)
                        .hint_text("sheet (file#page)")
                        .desired_width(110.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut s.tag_name)
                        .hint_text("tag")
                        .desired_width(60.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut s.tag_value)
                        .hint_text("value")
                        .desired_width(70.0),
                );
                if ui.small_button("Set Tag").clicked() {
                    set_tag = true;
                }
            });
        }
        ui.separator();
        if s.set.files.is_empty() {
            super::empty(ui, "Add PDFs to make a set: their sheets are listed here as one.");
        }
        let files = s.set.files.clone();
        let filter = s.filter.to_lowercase();
        let sheets = s.sheets().to_vec();
        // Tags, categories and revisions of the sheets (in the listed order).
        let rules = sp.rules();
        let tagged = tagged_sheets_with(&s.set, s.sort, &rules, sp.auto_tags).0;
        let groups = revisions(&tagged, &s.revision_filter);
        // Stacked: each latest version holds its earlier ones.
        let mut older_of: std::collections::HashMap<(usize, usize), Vec<markupcraft_engine::batch::SetSheet>> =
            std::collections::HashMap::new();
        for g in &groups {
            if let Some(latest) = g.latest().and_then(|i| tagged.get(i)) {
                let olds: Vec<_> = g
                    .versions
                    .iter()
                    .take(g.versions.len().saturating_sub(1))
                    .filter_map(|i| tagged.get(*i).map(|t| t.sheet.clone()))
                    .collect();
                if !olds.is_empty() {
                    older_of.insert((latest.sheet.file, latest.sheet.page), olds);
                }
            }
        }
        let earlier: std::collections::HashSet<(usize, usize)> = groups
            .iter()
            .flat_map(|g| g.versions.iter().take(g.versions.len().saturating_sub(1)))
            .filter_map(|i| tagged.get(*i).map(|t| (t.sheet.file, t.sheet.page)))
            .collect();
        let tags_of = |sh: &markupcraft_engine::batch::SetSheet| {
            tagged
                .iter()
                .find(|t| t.sheet.file == sh.file && t.sheet.page == sh.page)
                .map(|t| t.tags.clone())
                .unwrap_or_default()
        };
        let category = |sh: &markupcraft_engine::batch::SetSheet| match s.categories {
            CategoryMode::Off => String::new(),
            CategoryMode::FileName => category_of(
                &files
                    .get(sh.file)
                    .and_then(|f| f.file_stem())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                &rules,
            ),
            CategoryMode::SheetNumber => {
                category_of(&tags_of(sh).get("Sheet Number").cloned().unwrap_or_default(), &rules)
            }
        };
        let mut sheets = sheets;
        if s.categories != CategoryMode::Off {
            sheets.sort_by_key(|sh| rules.iter().position(|r| r.name == category(sh)).unwrap_or(usize::MAX));
        }
        let previous = s.previous;
        let show_tags = s.show_tags;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut last = None;
            let mut last_cat: Option<String> = None;
            for sh in sheets
                .iter()
                .filter(|sh| filter.is_empty() || sh.label.to_lowercase().contains(&filter))
            {
                let old = earlier.contains(&(sh.file, sh.page));
                if old && (previous == 1 || sp.stack_revisions) {
                    continue;
                }
                if s.categories != CategoryMode::Off {
                    let c = category(sh);
                    if last_cat.as_ref() != Some(&c) {
                        ui.label(RichText::new(&c).strong());
                        last_cat = Some(c);
                    }
                }
                if s.categories == CategoryMode::Off && last != Some(sh.file) {
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
                let mut text = RichText::new(label);
                if old && previous == 2 {
                    text = text.weak();
                }
                if old && previous == 3 {
                    text = text.strikethrough();
                }
                let r = ui.selectable_label(current, text);
                if show_tags {
                    let t = tags_of(sh);
                    let line: Vec<String> = t
                        .iter()
                        .filter(|(k, _)| k.as_str() != "Sheet Number")
                        .map(|(k, v)| format!("{k}: {v}"))
                        .collect();
                    if !line.is_empty() {
                        ui.label(RichText::new(line.join("  ")).small().weak());
                    }
                }
                if r.clicked() {
                    open_sheet = Some(sh.clone());
                    s.tag_sheet = markupcraft_engine::sets_more::tag_key(sh, &files);
                }
                if sp.stack_revisions
                    && previous != 1
                    && let Some(olds) = older_of.get(&(sh.file, sh.page))
                {
                    let n = olds.len();
                    egui::CollapsingHeader::new(
                        RichText::new(format!("{n} earlier revision{}", if n == 1 { "" } else { "s" })).small(),
                    )
                    .id_salt(("set-stack", sh.file, sh.page))
                    .show(ui, |ui| {
                        for o in olds {
                            let name = files
                                .get(o.file)
                                .and_then(|f| f.file_stem())
                                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                            if ui.selectable_label(false, RichText::new(name).weak()).clicked() {
                                open_sheet = Some(o.clone());
                            }
                        }
                    });
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
    if set_tag {
        sets::set_tag(app);
    }
    if publish {
        app.dialogs
            .save(Purpose::Feature(Ask::SetPublishPdf), PDF, "Published Set.pdf");
    }
    if package {
        app.dialogs.folder(Purpose::Feature(Ask::SetPackageDir));
    }
    if print_set {
        app.dialogs
            .save(Purpose::Feature(Ask::SetPrintOut), PDF, "Set Print.pdf");
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
