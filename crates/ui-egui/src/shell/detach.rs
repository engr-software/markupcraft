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
    w.pane.view.opts = doc.view.opts;
    super::split::ensure_render(&mut w.pane, doc, threads, &ctx);
    std::mem::swap(&mut doc.view, &mut w.pane.view);
    std::mem::swap(&mut doc.render, &mut w.pane.render);
    let rect = ui.available_rect_before_wrap();
    let out = ui
        .scope_builder(
            egui::UiBuilder::new().max_rect(rect).id_salt(("detached-canvas", uid)),
            |ui| canvas::show(ui, doc, &cx),
        )
        .inner;
    std::mem::swap(&mut doc.view, &mut w.pane.view);
    std::mem::swap(&mut doc.render, &mut w.pane.render);
    let pressed = ui.input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| rect.contains(p)));
    if pressed {
        app.active = i;
    }
    app.apply_canvas_out(out);
    close
}
