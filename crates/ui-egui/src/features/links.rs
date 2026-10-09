//! Links: the Hyperlink tool (drag the link area, then choose where it goes: a page, a web
//! address or a file), the links drawn on the pages, and following a link.

use std::path::PathBuf;

use egui::{Color32, RichText};
use markupcraft_engine::links::{LinkInfo, LinkLook, LinkTarget};
use markupcraft_geom::{Point, Rect};

use super::Mark;
use crate::{AppState, DocTab};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetKind {
    #[default]
    Page,
    Url,
    File,
}

pub struct LinksState {
    pub highlight: bool,
    pub selected: Option<String>,
    /// A dragged link area waiting for its target.
    pub pending: Option<(usize, Rect)>,
    pub kind: TargetKind,
    pub page: usize,
    pub url: String,
    pub file: String,
    pub look: LinkLook,
    pub message: String,
}

impl Default for LinksState {
    fn default() -> Self {
        Self {
            highlight: false,
            selected: None,
            pending: None,
            kind: TargetKind::Page,
            page: 1,
            url: "https://".into(),
            file: String::new(),
            look: LinkLook::default(),
            message: String::new(),
        }
    }
}

impl LinksState {
    pub fn marks(&self, d: &DocTab, out: &mut Vec<Mark>) {
        if self.highlight || self.selected.is_some() {
            for l in d.session.links() {
                let sel = self.selected.as_deref() == Some(l.id.as_str());
                if self.highlight || sel {
                    out.push(Mark::rect(
                        l.page,
                        l.rect,
                        Color32::from_rgba_premultiplied(0, 0, 60, 40),
                        if sel {
                            super::canvas::CURRENT_STROKE
                        } else {
                            Color32::from_rgb(40, 90, 220)
                        },
                    ));
                }
            }
        }
        if let Some((page, r)) = self.pending {
            out.push(Mark::rect(
                page,
                r,
                super::canvas::CURRENT_FILL,
                super::canvas::CURRENT_STROKE,
            ));
        }
    }
}

/// What a link goes to, as text.
pub fn describe(t: &LinkTarget) -> String {
    match t {
        LinkTarget::Page(p) => format!("Page {}", p + 1),
        LinkTarget::Url(u) => u.clone(),
        LinkTarget::File { path, page } => match page {
            Some(p) => format!("{path} (page {})", p + 1),
            None => path.clone(),
        },
        LinkTarget::Other(s) => s.clone(),
    }
}

pub fn rect_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    match super::rect_of(pts) {
        Some(r) => {
            app.features.links.pending = Some((page, r));
            app.features.links.message.clear();
        }
        None => app.status = "Drag a box over the link area".into(),
    }
}

/// Follow a link: go to its page, open its file, or open its web address.
pub fn follow(app: &mut AppState, l: &LinkInfo, ctx: &egui::Context) {
    match &l.target {
        LinkTarget::Page(p) => {
            if let Some(d) = app.doc_mut() {
                let n = d.session.page_count();
                d.view.go_to_page(*p, n);
            }
        }
        LinkTarget::Url(u) => ctx.open_url(egui::OpenUrl::new_tab(u)),
        LinkTarget::File { path, page } => {
            let base = app
                .doc()
                .and_then(|d| d.path.as_ref().and_then(|p| p.parent().map(|p| p.to_path_buf())));
            let p = PathBuf::from(path);
            let p = if p.is_relative() {
                base.map_or(p.clone(), |b| b.join(&p))
            } else {
                p
            };
            if p.is_file() {
                app.open_path(&p);
                if let (Some(pg), Some(d)) = (page, app.doc_mut()) {
                    let n = d.session.page_count();
                    d.view.go_to_page(*pg, n);
                }
            } else {
                app.status = format!("{} is not found", p.display());
            }
        }
        LinkTarget::Other(s) => app.status = format!("This link runs {s}"),
    }
}

/// The link-target dialog after a link area was dragged.
pub fn window(app: &mut AppState, ctx: &egui::Context) {
    let Some((page, rect)) = app.features.links.pending else {
        return;
    };
    let count = app.doc().map_or(0, |d| d.session.page_count());
    let (mut open, mut ok) = (true, false);
    super::window("Hyperlink").open(&mut open).show(ctx, |ui| {
        let l = &mut app.features.links;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut l.kind, TargetKind::Page, "Page");
            ui.selectable_value(&mut l.kind, TargetKind::Url, "Web address");
            ui.selectable_value(&mut l.kind, TargetKind::File, "File");
        });
        match l.kind {
            TargetKind::Page => {
                ui.add(
                    egui::DragValue::new(&mut l.page)
                        .range(1..=count.max(1))
                        .prefix("Page "),
                );
            }
            TargetKind::Url => {
                ui.text_edit_singleline(&mut l.url);
            }
            TargetKind::File => {
                ui.add(egui::TextEdit::singleline(&mut l.file).hint_text("path to a file (relative to this PDF)"));
            }
        }
        ui.horizontal(|ui| {
            ui.label("Border");
            ui.add(egui::DragValue::new(&mut l.look.width).range(0.0..=12.0));
            super::color_edit(ui, &mut l.look.color);
        });
        if !l.message.is_empty() {
            ui.label(RichText::new(&l.message).small());
        }
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                ok = true;
            }
            if ui.button("Cancel").clicked() {
                l.pending = None;
            }
        });
    });
    if !open {
        app.features.links.pending = None;
    }
    if ok {
        let l = &app.features.links;
        let target = match l.kind {
            TargetKind::Page => LinkTarget::Page(l.page.saturating_sub(1)),
            TargetKind::Url => LinkTarget::Url(l.url.trim().to_string()),
            TargetKind::File => LinkTarget::File {
                path: l.file.trim().to_string(),
                page: None,
            },
        };
        let look = l.look;
        let Some(d) = app.doc_mut() else { return };
        match d.session.add_link(page, rect, &target, look) {
            Ok(id) => {
                app.features.links.pending = None;
                app.features.links.selected = Some(id);
                app.status = format!("Link to {} added", describe(&target));
                app.show_panel("links");
            }
            Err(e) => app.features.links.message = e.to_string(),
        }
    }
}
