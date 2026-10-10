//! Dialog windows: the unsaved-changes prompt (closing a document, exiting) and Calibrate.

use egui::RichText;
use markupcraft_measure::units::{LengthUnit, real_units};

use crate::{AppState, Prompt, actions, pages_from_text};

/// Show every open dialog.
pub fn show(app: &mut AppState, ctx: &egui::Context) {
    prompt(app, ctx);
    calibrate(app, ctx);
}

fn prompt(app: &mut AppState, ctx: &egui::Context) {
    let Some(p) = app.prompts.first().cloned() else { return };
    let names = |uids: &[u64]| -> Vec<String> {
        app.docs
            .iter()
            .filter(|d| uids.contains(&d.uid))
            .map(|d| d.name.clone())
            .collect()
    };
    let (title, list) = match &p {
        Prompt::Close(uid) => ("Save changes?", names(&[*uid])),
        Prompt::Exit(uids) => ("Save changes before exiting?", names(uids)),
    };
    if list.is_empty() {
        app.prompts.remove(0);
        return;
    }
    let mut answer = None;
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("These documents have changes that are not saved:");
            for n in &list {
                ui.label(RichText::new(n).strong());
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(0);
                }
                if ui.button("Don't Save").clicked() {
                    answer = Some(1);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(2);
                }
            });
        });
    let Some(a) = answer else { return };
    app.prompts.remove(0);
    match (p, a) {
        (Prompt::Close(uid), 0) => app.save_doc(uid, false, true),
        (Prompt::Close(uid), 1) => app.force_close(uid),
        (Prompt::Exit(uids), 0) => {
            app.prompts.insert(0, Prompt::Exit(uids.clone()));
            for uid in uids {
                app.save_doc(uid, false, true);
            }
        }
        (Prompt::Exit(_), 1) => app.close_now = true,
        _ => app.status = "Cancelled".into(),
    }
}

fn calibrate(app: &mut AppState, ctx: &egui::Context) {
    let Some(mut c) = app.calibrate.clone() else { return };
    let mut open = true;
    let mut apply = false;
    let count = app
        .docs
        .iter()
        .find(|d| d.uid == c.doc)
        .map_or(0, |d| d.session.page_count());
    crate::i18n::window("Calibrate")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let paper = c.a.dist(c.b) / 72.0;
            ui.label(format!("The two points are {paper:.3} in apart on the sheet."));
            ui.label("What is that distance on the drawing?");
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut c.length)
                        .desired_width(80.0)
                        .hint_text("e.g. 20"),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
                egui::ComboBox::from_id_salt("cal-unit")
                    .selected_text(c.unit.label())
                    .show_ui(ui, |ui| {
                        for u in real_units() {
                            ui.selectable_value(&mut c.unit, u, u.name());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Pages:");
                ui.add(
                    egui::TextEdit::singleline(&mut c.pages)
                        .desired_width(120.0)
                        .hint_text(format!("this page ({})", c.page + 1)),
                );
                if ui.small_button("All").clicked() {
                    c.pages = "all".into();
                }
            });
            ui.checkbox(&mut c.apply_to_markups, "Update measurements on those pages");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    apply = true;
                }
                if ui.button("Cancel").clicked() {
                    app.calibrate = None;
                }
            });
        });
    if !open {
        app.calibrate = None;
        return;
    }
    if app.calibrate.is_none() {
        return;
    }
    if !apply {
        app.calibrate = Some(c);
        return;
    }
    let Some(length) = markupcraft_measure::parse_label(&c.length).or_else(|| c.length.trim().parse().ok()) else {
        app.status = "Type the real distance, e.g. 20 or 12' 6\"".into();
        app.calibrate = Some(c);
        return;
    };
    let Some(pages) = pages_from_text(&c.pages, c.page, count) else {
        app.status = "Pages: leave empty for this page, type all, or a range like 1-3, 7".into();
        app.calibrate = Some(c);
        return;
    };
    // A feet-and-inches entry (12' 6") parses to feet.
    let unit = if c.length.contains('\'') || c.length.contains('"') {
        LengthUnit::Foot
    } else {
        c.unit
    };
    let Some(d) = app.docs.iter_mut().find(|d| d.uid == c.doc) else {
        app.calibrate = None;
        return;
    };
    let r = d.session.calibrate(&pages, c.a, c.b, length, unit, c.apply_to_markups);
    app.status = actions::report(r, |s| {
        format!("Scale set: {} ({})", s.ratio, actions::plural(pages.len(), "page"))
    });
    app.calibrate = None;
}
