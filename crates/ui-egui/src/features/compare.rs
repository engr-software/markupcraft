//! Compare Documents: the dialog (older and newer revision, what to compare, how to mark the
//! changes) and the results the Compare panel lists, each change clouded on the newer document.

use std::path::PathBuf;

use egui::RichText;
use markupcraft_engine::compare::{CompareMode, CompareOptions, CompareRegion};
use markupcraft_geom::Point;
use markupcraft_model::Color;

use super::{Ask, Mark, canvas};
use crate::dialogs::{PDF, Purpose};
use crate::{AppState, DocTab, actions};

/// What became of a change in review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Review {
    Open,
    Accepted,
    Rejected,
}

pub struct CompareState {
    pub open: bool,
    /// Older revision: an open document (uid) or a file.
    pub old_doc: Option<u64>,
    pub old_file: Option<PathBuf>,
    /// Newer revision: an open document.
    pub new_doc: Option<u64>,
    pub mode: CompareMode,
    pub sensitivity: f64,
    pub include_markups: bool,
    pub color: Color,
    pub clouds: bool,
    /// The document the results are on and the changes found.
    pub doc: u64,
    pub regions: Vec<(CompareRegion, Review)>,
    pub current: Option<usize>,
    pub message: String,
}

impl Default for CompareState {
    fn default() -> Self {
        let o = CompareOptions::default();
        Self {
            open: false,
            old_doc: None,
            old_file: None,
            new_doc: None,
            mode: o.mode,
            sensitivity: o.sensitivity,
            include_markups: o.include_markups,
            color: o.color,
            clouds: true,
            doc: 0,
            regions: Vec::new(),
            current: None,
            message: String::new(),
        }
    }
}

impl CompareState {
    pub fn marks(&self, d: &DocTab, out: &mut Vec<Mark>) {
        if d.uid != self.doc {
            return;
        }
        if let Some((r, _)) = self.current.and_then(|i| self.regions.get(i)) {
            out.push(Mark::rect(
                r.page,
                r.rect.padded(6.0),
                canvas::CURRENT_FILL,
                canvas::CURRENT_STROKE,
            ));
        }
    }
}

/// Tools > Compare Documents: the active document is the newer revision; the older defaults to
/// another open document.
pub fn open(app: &mut AppState) {
    let active = app.doc().map(|d| d.uid);
    let other = app.docs.iter().map(|d| d.uid).find(|u| Some(*u) != active);
    let c = &mut app.features.compare;
    c.open = true;
    c.new_doc = active;
    if c.old_doc.is_none_or(|u| Some(u) == active) {
        c.old_doc = other;
    }
}

fn doc_name(app: &AppState, uid: Option<u64>) -> String {
    uid.and_then(|u| app.docs.iter().find(|d| d.uid == u))
        .map_or_else(|| "(choose)".to_string(), |d| d.name.clone())
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.compare.open {
        return;
    }
    let docs: Vec<(u64, String)> = app.docs.iter().map(|d| (d.uid, d.name.clone())).collect();
    let (mut open, mut run, mut browse) = (true, false, false);
    let old_name = match (&app.features.compare.old_file, app.features.compare.old_doc) {
        (Some(p), _) => p
            .file_name()
            .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned()),
        (None, u) => doc_name(app, u),
    };
    let new_name = doc_name(app, app.features.compare.new_doc);
    super::window("Compare Documents").open(&mut open).show(ctx, |ui| {
        let c = &mut app.features.compare;
        egui::Grid::new("compare-docs").num_columns(2).show(ui, |ui| {
            ui.label("Older (A)");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("compare-old")
                    .selected_text(&old_name)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for (u, n) in &docs {
                            if ui
                                .selectable_label(c.old_doc == Some(*u) && c.old_file.is_none(), n)
                                .clicked()
                            {
                                c.old_doc = Some(*u);
                                c.old_file = None;
                            }
                        }
                    });
                if ui.button("Browse...").clicked() {
                    browse = true;
                }
            });
            ui.end_row();
            ui.label("Newer (B)");
            egui::ComboBox::from_id_salt("compare-new")
                .selected_text(&new_name)
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (u, n) in &docs {
                        ui.selectable_value(&mut c.new_doc, Some(*u), n);
                    }
                });
            ui.end_row();
            ui.label("Compare");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut c.mode, CompareMode::Both, "Text and graphics");
                ui.selectable_value(&mut c.mode, CompareMode::Text, "Text");
                ui.selectable_value(&mut c.mode, CompareMode::Graphics, "Graphics");
            });
            ui.end_row();
            ui.label("Sensitivity");
            ui.add(egui::Slider::new(&mut c.sensitivity, 0.0..=1.0).fixed_decimals(2));
            ui.end_row();
            ui.label("Markups");
            ui.checkbox(&mut c.include_markups, "Include existing markups");
            ui.end_row();
            ui.label("Changes");
            ui.horizontal(|ui| {
                super::color_edit(ui, &mut c.color);
                ui.checkbox(&mut c.clouds, "Clouds");
            });
            ui.end_row();
        });
        if !c.message.is_empty() {
            ui.label(RichText::new(&c.message).small());
        }
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                run = true;
            }
            if ui.button("Cancel").clicked() {
                c.open = false;
            }
        });
    });
    if !open {
        app.features.compare.open = false;
    }
    if browse {
        app.dialogs.open(Purpose::Feature(Ask::CompareOld), PDF, false);
    }
    if run {
        run_compare(app);
    }
}

/// Compare with the dialog's settings; the newer document gets a cloud per change.
pub fn run_compare(app: &mut AppState) {
    let c = &app.features.compare;
    let Some(new_uid) = c.new_doc else {
        app.features.compare.message = "Choose the newer document".into();
        return;
    };
    let old = match (&c.old_file, c.old_doc) {
        (Some(p), _) => markupcraft_engine::raster::read_pdf(p),
        (None, Some(u)) if u != new_uid => match app.docs.iter().find(|d| d.uid == u) {
            Some(d) => d.session.render_bytes(),
            None => {
                app.features.compare.message = "The older document is closed".into();
                return;
            }
        },
        _ => {
            app.features.compare.message = "Choose an older revision other than the newer document".into();
            return;
        }
    };
    let old = match old {
        Ok(b) => b,
        Err(e) => {
            app.features.compare.message = e.to_string();
            return;
        }
    };
    let opts = CompareOptions {
        mode: c.mode,
        sensitivity: c.sensitivity,
        include_markups: c.include_markups,
        color: c.color,
        cloud: if c.clouds { 1.0 } else { 0.0 },
        ..Default::default()
    };
    let Some(i) = app.docs.iter().position(|d| d.uid == new_uid) else {
        return;
    };
    app.active = i;
    let Some(d) = app.docs.get_mut(i) else { return };
    match d.session.compare_with(old, &opts) {
        Ok(rep) => {
            let c = &mut app.features.compare;
            c.doc = new_uid;
            c.regions = rep.regions.into_iter().map(|r| (r, Review::Open)).collect();
            c.current = None;
            c.open = false;
            c.message.clear();
            app.status = format!(
                "Compare: {} ({} text, {} graphics)",
                actions::plural(c.regions.len(), "change"),
                rep.text_changes,
                rep.graphics_changes
            );
            app.show_panel("compare");
            if !app.features.compare.regions.is_empty() {
                go_to(app, 0);
            }
        }
        Err(e) => app.features.compare.message = e.to_string(),
    }
}

/// Show change `i`.
pub fn go_to(app: &mut AppState, i: usize) {
    let c = &mut app.features.compare;
    let Some((r, _)) = c.regions.get(i).cloned() else {
        return;
    };
    c.current = Some(i);
    let uid = c.doc;
    let Some(pos) = app.docs.iter().position(|d| d.uid == uid) else {
        return;
    };
    app.active = pos;
    let Some(d) = app.docs.get_mut(pos) else { return };
    if let Some(render) = d.render.as_ref() {
        let p = Point::new((r.rect.x0 + r.rect.x1) / 2.0, (r.rect.y0 + r.rect.y1) / 2.0);
        d.view.center_on(r.page, p, render.pages());
    }
    if !r.markup.is_empty() && d.session.doc().find(&r.markup).is_some() {
        actions::select(&mut d.session, vec![r.markup.clone()]);
    }
}

/// Accept (keep the cloud) or reject (delete it) change `i`, then go to the next one.
pub fn review(app: &mut AppState, i: usize, accept: bool) {
    let uid = app.features.compare.doc;
    let Some((r, state)) = app.features.compare.regions.get_mut(i) else {
        return;
    };
    *state = if accept { Review::Accepted } else { Review::Rejected };
    let id = r.markup.clone();
    if !accept
        && !id.is_empty()
        && let Some(d) = app.docs.iter_mut().find(|d| d.uid == uid)
    {
        let r = d.session.delete_markups(&[id], false);
        app.status = actions::report(r, |_| "Change rejected; its cloud is deleted".into());
    }
    let next = app
        .features
        .compare
        .regions
        .iter()
        .enumerate()
        .skip(i + 1)
        .find(|(_, (_, s))| *s == Review::Open)
        .map(|(k, _)| k);
    if let Some(k) = next {
        go_to(app, k);
    }
}

/// Delete every cloud the comparison made (one undo step each is avoided: one delete).
pub fn delete_all(app: &mut AppState) {
    let uid = app.features.compare.doc;
    let ids: Vec<String> = app
        .features
        .compare
        .regions
        .iter()
        .filter(|(r, s)| *s != Review::Rejected && !r.markup.is_empty())
        .map(|(r, _)| r.markup.clone())
        .collect();
    if let Some(d) = app.docs.iter_mut().find(|d| d.uid == uid) {
        let live: Vec<String> = ids
            .into_iter()
            .filter(|id| d.session.doc().find(id).is_some())
            .collect();
        let r = d.session.delete_markups(&live, false);
        app.status = actions::report(r, |n| format!("Deleted {}", actions::plural(n, "cloud")));
    }
    app.features.compare.regions.clear();
    app.features.compare.current = None;
}
