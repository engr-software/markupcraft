//! Split views (MultiView): the document area in two panes, side by side (Split Vertical,
//! Ctrl+2) or one above the other (Split Horizontal, Ctrl+H), each showing any open document,
//! the same one included. Toggle Split (Ctrl+I) flips the orientation, Switch (Ctrl+1) swaps
//! what the panes show, Balance (Shift+F12) evens their sizes, Unsplit (Ctrl+Shift+2) goes
//! back to one pane. Synchronize: Document (the panes follow each other page for page) or Page
//! (they pan and zoom together whatever page each shows).
//!
//! The pane that has focus shows the active document (menus and keys act on it); clicking in
//! the other pane moves the focus there. The second pane keeps its own view and renderer, so
//! one document can show in both panes at different places.

use egui::{Rect, RichText, Vec2, vec2};
use markupcraft_render::{RenderDoc, RenderOptions};

use crate::canvas::{self, CanvasCx, DocView};
use crate::theme::Tokens;
use crate::{AppState, DocTab, actions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sync {
    #[default]
    Off,
    /// Page for page: both panes show the same page number, zoom and position.
    Document,
    /// Relative: panning and zooming one pane moves the other by as much.
    Page,
}

/// The second pane.
pub struct Pane {
    /// The document it shows.
    pub uid: u64,
    pub view: DocView,
    pub render: Option<RenderDoc>,
    /// The document bytes its renderer was made from (rebuilt when they change).
    bytes: Option<std::sync::Arc<Vec<u8>>>,
}

pub struct Split {
    /// Side by side (else one above the other).
    pub vertical: bool,
    /// Share of the first pane.
    pub frac: f32,
    /// The active document shows in the first pane (else in the second).
    pub active_first: bool,
    pub pane: Pane,
    pub sync: Sync,
    /// The focused pane's (zoom, offset) last frame, for Page sync.
    pub last: Option<(f32, Vec2)>,
}

/// Split the document area; the new pane shows the active document.
pub fn split(app: &mut AppState, vertical: bool) {
    if let Some(s) = &mut app.shell.split {
        s.vertical = vertical;
        return;
    }
    let Some(d) = app.doc() else { return };
    let mut view = DocView::default();
    view.mode = d.view.mode;
    view.fit = d.view.fit;
    view.go_to_page(d.view.current, d.session.page_count());
    app.shell.split = Some(Split {
        vertical,
        frac: 0.5,
        active_first: true,
        pane: Pane {
            uid: d.uid,
            view,
            render: None,
            bytes: None,
        },
        sync: match app.shell.ui.extra.sync_default.as_str() {
            "document" => Sync::Document,
            "page" => Sync::Page,
            _ => Sync::Off,
        },
        last: None,
    });
}

pub fn unsplit(app: &mut AppState) {
    // The focused (active) document stays on screen.
    app.shell.split = None;
}

/// Swap what the two panes show.
pub fn switch(app: &mut AppState) {
    let Some(active) = app.doc().map(|d| d.uid) else { return };
    let Some(s) = &mut app.shell.split else { return };
    let other = s.pane.uid;
    if other == active {
        return;
    }
    s.pane.uid = active;
    s.pane.render = None;
    s.pane.bytes = None;
    if let Some(i) = app.docs.iter().position(|d| d.uid == other) {
        app.active = i;
    }
}

/// Show document `uid` in the second pane (a tab dropped on it).
pub fn show_in_pane(app: &mut AppState, uid: u64) {
    let Some(s) = &mut app.shell.split else { return };
    if s.pane.uid != uid {
        s.pane.uid = uid;
        s.pane.render = None;
        s.pane.bytes = None;
        s.pane.view = DocView::default();
    }
    if let Some(name) = app.docs.iter().find(|d| d.uid == uid).map(|d| d.name.clone()) {
        app.status = format!("{name} shows in the second pane");
    }
}

/// Give the focus to the second pane: it now shows the active document, the first pane the
/// document that was active.
fn focus_pane(app: &mut AppState) {
    let Some(s) = &mut app.shell.split else { return };
    let Some(active_uid) = app.docs.get(app.active).map(|d| d.uid) else {
        return;
    };
    let target = s.pane.uid;
    if target == active_uid {
        // Same document in both panes: exchange the views.
        if let Some(d) = app.docs.get_mut(app.active) {
            std::mem::swap(&mut d.view, &mut s.pane.view);
            s.pane.render = None;
            s.pane.bytes = None;
        }
    } else if let Some(i) = app.docs.iter().position(|d| d.uid == target) {
        // The pane's view goes to its document; the pane takes the old active one.
        if let Some(d) = app.docs.get_mut(i) {
            std::mem::swap(&mut d.view, &mut s.pane.view);
        }
        s.pane.uid = active_uid;
        if let Some(d) = app.docs.iter_mut().find(|d| d.uid == active_uid) {
            std::mem::swap(&mut d.view, &mut s.pane.view);
        }
        s.pane.render = None;
        s.pane.bytes = None;
        app.active = i;
    }
    s.active_first = !s.active_first;
    s.last = None;
    for d in &mut app.docs {
        d.view.invalidate();
    }
    if let Some(s) = &mut app.shell.split {
        s.pane.view.invalidate();
    }
}

impl Pane {
    /// A pane showing document `uid` with `view` (its renderer is made on first draw).
    pub fn new(uid: u64, view: DocView) -> Self {
        Self {
            uid,
            view,
            render: None,
            bytes: None,
        }
    }
}

/// The second pane's renderer, rebuilt when the document's bytes changed.
pub(super) fn ensure_render(pane: &mut Pane, doc: &DocTab, threads: usize, ctx: &egui::Context) {
    let same = pane
        .bytes
        .as_ref()
        .is_some_and(|b| std::sync::Arc::ptr_eq(b, &doc.bytes));
    if !same || pane.render.is_none() {
        let opts = RenderOptions {
            hide: actions::drawn_objects(doc.session.doc()),
            threads,
            hide_all_markups: false,
        };
        pane.render = RenderDoc::open(doc.bytes.clone(), &opts).ok();
        pane.bytes = Some(doc.bytes.clone());
        pane.view.invalidate();
    }
    if let Some(r) = &pane.render {
        pane.view.receive(ctx, r);
    }
}

/// Draw the document area: one canvas, or two panes.
pub fn show(app: &mut AppState, ui: &mut egui::Ui) {
    app.shell.extra.pane_rect = None;
    if app.shell.split.is_none() {
        let rect = ui.available_rect_before_wrap();
        pane_ui(app, ui, rect);
        return;
    }
    // A pane showing a document that closed falls back to the active one.
    let active_uid = app.doc().map(|d| d.uid);
    if let Some(s) = &mut app.shell.split
        && !app.docs.iter().any(|d| d.uid == s.pane.uid)
    {
        match active_uid {
            Some(u) => {
                s.pane.uid = u;
                s.pane.render = None;
                s.pane.bytes = None;
            }
            None => app.shell.split = None,
        }
    }
    let Some((vertical, frac, active_first)) = app.shell.split.as_ref().map(|s| (s.vertical, s.frac, s.active_first))
    else {
        return;
    };
    let full = ui.available_rect_before_wrap();
    let bar = 4.0;
    let (a, b) = if vertical {
        let x = full.left() + full.width() * frac;
        (
            Rect::from_min_max(full.min, egui::pos2(x - bar / 2.0, full.bottom())),
            Rect::from_min_max(egui::pos2(x + bar / 2.0, full.top()), full.max),
        )
    } else {
        let y = full.top() + full.height() * frac;
        (
            Rect::from_min_max(full.min, egui::pos2(full.right(), y - bar / 2.0)),
            Rect::from_min_max(egui::pos2(full.left(), y + bar / 2.0), full.max),
        )
    };
    // The divider drags.
    let div = if vertical {
        Rect::from_min_max(egui::pos2(a.right(), full.top()), egui::pos2(b.left(), full.bottom()))
    } else {
        Rect::from_min_max(egui::pos2(full.left(), a.bottom()), egui::pos2(full.right(), b.top()))
    };
    let t = Tokens::get(ui.ctx());
    let dr = ui.interact(div, egui::Id::new("split-divider"), egui::Sense::drag());
    ui.painter()
        .rect_filled(div, 0.0, if dr.hovered() { t.accent } else { t.border });
    if dr.dragged()
        && let Some(p) = dr.interact_pointer_pos()
        && let Some(s) = &mut app.shell.split
    {
        let f = if vertical {
            (p.x - full.left()) / full.width().max(1.0)
        } else {
            (p.y - full.top()) / full.height().max(1.0)
        };
        s.frac = f.clamp(0.1, 0.9);
    }
    if dr.hovered() || dr.dragged() {
        ui.ctx().set_cursor_icon(if vertical {
            egui::CursorIcon::ResizeHorizontal
        } else {
            egui::CursorIcon::ResizeVertical
        });
    }
    let (primary, secondary) = if active_first { (a, b) } else { (b, a) };
    pane_ui(app, ui, primary);
    second_pane(app, ui, secondary);
    app.shell.extra.pane_rect = Some(secondary);
    // A document tab dragged over the second pane: mark the drop spot.
    if app.shell.extra.dragging_tab
        && ui
            .input(|i| i.pointer.latest_pos())
            .is_some_and(|p| secondary.contains(p))
    {
        ui.painter().rect_stroke(
            secondary.shrink(2.0),
            0.0,
            egui::Stroke::new(3.0, t.accent),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            secondary.center(),
            egui::Align2::CENTER_CENTER,
            "\u{2193} Show here",
            egui::FontId::proportional(18.0),
            t.accent,
        );
    }
    // Focus follows a press in the other pane.
    let pressed = ui.input(|i| {
        i.pointer
            .press_origin()
            .filter(|_| i.pointer.any_pressed())
            .is_some_and(|p| secondary.contains(p))
    });
    if pressed {
        focus_pane(app);
    }
    sync(app);
}

fn sync(app: &mut AppState) {
    let Some(i) = Some(app.active) else { return };
    let Some(s) = &mut app.shell.split else { return };
    let Some(d) = app.docs.get(i) else { return };
    let src = &d.view;
    match s.sync {
        Sync::Off => {}
        Sync::Document => {
            let dst = &mut s.pane.view;
            dst.mode = src.mode;
            dst.rotation = src.rotation;
            dst.place(src.current, src.zoom, src.fit, src.offset);
        }
        Sync::Page => {
            let now = (src.zoom, src.offset);
            if let Some((z0, o0)) = s.last {
                let dst = &mut s.pane.view;
                let k = src.zoom / z0.max(1e-6);
                let d_off = src.offset - o0;
                if (k - 1.0).abs() > 1e-4 || d_off != Vec2::ZERO {
                    let (cur, fit) = (dst.current, dst.fit);
                    let zoom = dst.zoom * k;
                    let off = dst.offset * k + d_off;
                    dst.place(
                        cur,
                        zoom,
                        if (k - 1.0).abs() > 1e-4 { canvas::Fit::None } else { fit },
                        off,
                    );
                }
            }
            s.last = Some(now);
        }
    }
}

/// The primary pane: the active document (tabs are above the whole area).
fn pane_ui(app: &mut AppState, ui: &mut egui::Ui, rect: Rect) {
    let out = ui
        .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            let canvas_rect = super::overlay::rulers_frame(app, ui, rect);
            ui.scope_builder(egui::UiBuilder::new().max_rect(canvas_rect), |ui| {
                let tool = crate::tools::find(app.tool).unwrap_or(&crate::tools::select::TOOL);
                let (template, drawing_mode) = {
                    let (t, d) = app.template();
                    (t.cloned(), d)
                };
                let author = app.author.clone();
                let edit = app.edit.clone();
                let cx = CanvasCx {
                    tool,
                    wheel_zooms: app.wheel_zooms,
                    hide_markups: app.hide_markups,
                    want_thumbs: app.thumbs_wanted_last,
                    snaps: app.snaps,
                    show_grid: app.show_grid,
                    template: template.as_ref(),
                    drawing_mode,
                    stamp: app.stamp,
                    author: &author,
                    edit: &edit,
                };
                let doc = app.docs.get_mut(app.active)?;
                let out = canvas::show(ui, doc, &cx);
                Some(out)
            })
            .inner
        })
        .inner;
    if let Some(out) = out {
        app.apply_canvas_out(out);
    }
    super::overlay::paint(app, ui, rect);
}

fn second_pane(app: &mut AppState, ui: &mut egui::Ui, rect: Rect) {
    let t = Tokens::get(ui.ctx());
    let threads = app.threads;
    let ctx = ui.ctx().clone();
    // A header with the document picker.
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), 24.0));
    let body = Rect::from_min_max(egui::pos2(rect.left(), head.bottom()), rect.max);
    let names: Vec<(u64, String)> = app.docs.iter().map(|d| (d.uid, d.name.clone())).collect();
    let mut pick = None;
    let mut close = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(head), |ui| {
        ui.painter().rect_filled(head, 0.0, t.chrome);
        ui.horizontal_centered(|ui| {
            ui.add_space(4.0);
            let cur = app.shell.split.as_ref().map(|s| s.pane.uid);
            let label = names
                .iter()
                .find(|(u, _)| Some(*u) == cur)
                .map_or_else(String::new, |(_, n)| n.clone());
            egui::ComboBox::from_id_salt("split-doc")
                .selected_text(RichText::new(label).size(12.0))
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (u, n) in &names {
                        if ui.selectable_label(Some(*u) == cur, n).clicked() {
                            pick = Some(*u);
                        }
                    }
                });
            ui.label(
                RichText::new("Split view (click to focus)")
                    .size(11.0)
                    .color(t.text_faint),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::icons::button(ui, "x", 18.0, false, "Unsplit (Ctrl+Shift+2)").clicked() {
                    close = true;
                }
            });
        });
    });
    if close {
        app.shell.split = None;
        return;
    }
    let tool = crate::tools::find(app.tool).unwrap_or(&crate::tools::select::TOOL);
    let author = app.author.clone();
    let edit = app.edit.clone();
    let template = app.template().0.cloned();
    let Some(s) = &mut app.shell.split else { return };
    if let Some(u) = pick
        && u != s.pane.uid
    {
        s.pane.uid = u;
        s.pane.render = None;
        s.pane.bytes = None;
        s.pane.view = DocView::default();
    }
    let uid = s.pane.uid;
    let Some(doc) = app.docs.iter_mut().find(|d| d.uid == uid) else {
        return;
    };
    ensure_render(&mut s.pane, doc, threads, &ctx);
    // Show the document with the pane's own view and renderer.
    std::mem::swap(&mut doc.view, &mut s.pane.view);
    std::mem::swap(&mut doc.render, &mut s.pane.render);
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
    let out = ui
        .scope_builder(egui::UiBuilder::new().max_rect(body).id_salt("split-pane"), |ui| {
            ui.set_clip_rect(body.intersect(ui.clip_rect()));
            canvas::show(ui, doc, &cx)
        })
        .inner;
    std::mem::swap(&mut doc.view, &mut s.pane.view);
    std::mem::swap(&mut doc.render, &mut s.pane.render);
    app.apply_canvas_out(out);
}
