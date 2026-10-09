//! Detached document windows (MultiView on a second monitor): Window > Detach Tab to New Window
//! (or a tab's right-click Detach) shows the active document in a window of its own, with its
//! own view and renderer (another view of the same file: edits show in both). Drawing and
//! selecting in it work as in the main window; clicking in it makes its document the active
//! one, so the panels, menus and keys follow it. Reattach (or closing the window) ends it.

use egui::{RichText, ViewportBuilder, ViewportId};

use super::split::Pane;
use crate::AppState;
use crate::canvas::{self, CanvasCx, DocView};

pub struct Detached {
    pub pane: Pane,
    /// Split inside the window: a second view of the document, side by side.
    pub second: Option<Pane>,
    /// The second view follows the first (page for page, or by as much as it pans and zooms).
    pub sync: super::split::Sync,
    /// The window follows the main window's view of the document (page for page).
    pub follow_main: bool,
}

impl Detached {
    pub fn uid(&self) -> u64 {
        self.pane.uid
    }
}

/// Detach the active document into a window.
pub fn detach(app: &mut AppState) {
    let Some(d) = app.doc() else { return };
    if app.shell.extra.detached.iter().any(|w| w.uid() == d.uid) {
        app.status = format!("{} already has a window", d.name);
        return;
    }
    let mut view = DocView::default();
    view.mode = d.view.mode;
    view.fit = d.view.fit;
    view.go_to_page(d.view.current, d.session.page_count());
    let (uid, name) = (d.uid, d.name.clone());
    app.shell.extra.detached.push(Detached {
        pane: Pane::new(uid, view),
        second: None,
        sync: super::split::Sync::Off,
        follow_main: false,
    });
    app.status = format!("{name} detached to a new window");
}

/// Draw every detached window.
pub fn show(app: &mut AppState, ctx: &egui::Context) {
    // Windows of documents that closed go away.
    let open: Vec<u64> = app.docs.iter().map(|d| d.uid).collect();
    app.shell.extra.detached.retain(|w| open.contains(&w.uid()));
    let uids: Vec<u64> = app.shell.extra.detached.iter().map(Detached::uid).collect();
    for uid in uids {
        let Some(name) = app.docs.iter().find(|d| d.uid == uid).map(|d| d.name.clone()) else {
            continue;
        };
        let builder = ViewportBuilder::default()
            .with_title(format!("{name} - MarkupCraft"))
            .with_inner_size([900.0, 700.0]);
        let close = ctx.show_viewport_immediate(ViewportId::from_hash_of(("detached", uid)), builder, |ui, _class| {
            window_ui(app, ui, uid)
        });
        if close {
            app.shell.extra.detached.retain(|w| w.uid() != uid);
        }
    }
}

/// One detached window's contents; true when it should close.
fn window_ui(app: &mut AppState, ui: &mut egui::Ui, uid: u64) -> bool {
    let mut close = ui.input(|i| i.viewport().close_requested());
    ui.horizontal(|ui| {
        let name = app
            .docs
            .iter()
            .find(|d| d.uid == uid)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        ui.label(RichText::new(name).strong());
        if ui.button("Reattach").clicked() {
            close = true;
        }
        if let Some(w) = app.shell.extra.detached.iter_mut().find(|w| w.uid() == uid) {
            let mut split = w.second.is_some();
            if ui.checkbox(&mut split, "Split").changed() {
                w.second = split.then(|| {
                    let mut v = DocView::default();
                    v.mode = w.pane.view.mode;
                    v.fit = w.pane.view.fit;
                    v.current = w.pane.view.current;
                    Pane::new(uid, v)
                });
            }
            use super::split::Sync;
            egui::ComboBox::from_id_salt(("detached-sync", uid))
                .selected_text(match w.sync {
                    Sync::Off => "Sync: Off",
                    Sync::Document => "Sync: Document",
                    Sync::Page => "Sync: Page",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut w.sync, Sync::Off, "Sync: Off");
                    ui.selectable_value(&mut w.sync, Sync::Document, "Sync: Document");
                    ui.selectable_value(&mut w.sync, Sync::Page, "Sync: Page");
                });
            ui.checkbox(&mut w.follow_main, "Follow the main window");
        }
    });
    let threads = app.threads;
    let ctx = ui.ctx().clone();
    let tool = crate::tools::find(app.tool).unwrap_or(&crate::tools::select::TOOL);
    let author = app.author.clone();
    let edit = app.edit.clone();
    let template = app.template().0.cloned();
    let cx = CanvasCx {
        tool,
        wheel_zooms: app.wheel_zooms,
        hide_markups: app.hide_markups,
        want_thumbs: false,
        snaps: app.snaps,
        show_grid: app.show_grid,
        template: template.as_ref(),
        drawing_mode: false,
        stamp: app.stamp,
        author: &author,
        edit: &edit,
    };
    let Some(w) = app.shell.extra.detached.iter_mut().find(|w| w.uid() == uid) else {
        return true;
    };
    let Some(i) = app.docs.iter().position(|d| d.uid == uid) else {
        return true;
    };
    let Some(doc) = app.docs.get_mut(i) else { return true };
    if w.follow_main {
        let src = &doc.view;
        w.pane.view.mode = src.mode;
        w.pane.view.place(src.current, src.zoom, src.fit, src.offset);
    }
    let full = ui.available_rect_before_wrap();
    let rects = if w.second.is_some() {
        let mid = full.center().x;
        vec![
            egui::Rect::from_min_max(full.min, egui::pos2(mid - 2.0, full.max.y)),
            egui::Rect::from_min_max(egui::pos2(mid + 2.0, full.min.y), full.max),
        ]
    } else {
        vec![full]
    };
    let before = (w.pane.view.zoom, w.pane.view.offset);
    let mut outs = Vec::new();
    for (k, rect) in rects.iter().enumerate() {
        let pane = if k == 0 {
            &mut w.pane
        } else {
            match w.second.as_mut() {
                Some(p) => p,
                None => break,
            }
        };
        pane.view.opts = doc.view.opts;
        super::split::ensure_render(pane, doc, threads, &ctx);
        std::mem::swap(&mut doc.view, &mut pane.view);
        std::mem::swap(&mut doc.render, &mut pane.render);
        let out = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(*rect)
                    .id_salt(("detached-canvas", uid, k)),
                |ui| canvas::show(ui, doc, &cx),
            )
            .inner;
        std::mem::swap(&mut doc.view, &mut pane.view);
        std::mem::swap(&mut doc.render, &mut pane.render);
        outs.push(out);
    }
    // the second view follows the first
    if let Some(second) = w.second.as_mut() {
        use super::split::Sync;
        let src = &w.pane.view;
        match w.sync {
            Sync::Off => {}
            Sync::Document => {
                second.view.mode = src.mode;
                second.view.place(src.current, src.zoom, src.fit, src.offset);
            }
            Sync::Page => {
                let k = src.zoom / before.0.max(1e-6);
                let d_off = src.offset - before.1;
                if (k - 1.0).abs() > 1e-4 || d_off != egui::Vec2::ZERO {
                    let (cur, fit) = (second.view.current, second.view.fit);
                    let zoom = second.view.zoom * k;
                    let off = second.view.offset * k + d_off;
                    second.view.place(cur, zoom, fit, off);
                }
            }
        }
    }
    let pressed = ui.input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| full.contains(p)));
    if pressed {
        app.active = i;
    }
    for out in outs {
        app.apply_canvas_out(out);
    }
    close
}
