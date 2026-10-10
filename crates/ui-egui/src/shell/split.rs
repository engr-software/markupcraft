//! Split views (MultiView): the document area in two panes, side by side (Split Vertical,
//! Ctrl+2) or one above the other (Split Horizontal, Ctrl+H), each showing any open document,
//! the same one included. Toggle Split (Ctrl+I) flips the orientation, Switch (Ctrl+1) swaps
//! what the panes show, Balance (Shift+F12) evens their sizes, Unsplit (Ctrl+Shift+2) goes
//! back to one pane. Synchronize: Document (the panes follow each other page for page) or Page
//! (they pan and zoom together whatever page each shows).
//!
//! Splitting again adds a pane (up to [`MAX_PANES`]); three or more panes share the area in a
//! grid, side by side first after Split Vertical, stacked first after Split Horizontal, and
//! each pane's header closes it.
//!
//! The pane that has focus shows the active document (menus and keys act on it); clicking in
//! the other pane moves the focus there. The second pane keeps its own view and renderer, so
//! one document can show in both panes at different places.
//!
//! Every pane has its own tab bar: the first pane uses the document tabs above the area, each
//! other pane a row of tabs in its header (a tab dropped on a pane, or added with its "+" menu,
//! joins that pane's tabs; each tab keeps its own place in the pane).

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
    /// The pane's own tabs, in order (the shown document is one of them).
    pub tabs: Vec<u64>,
    /// The place each tab not shown was left at.
    parked: Vec<(u64, DocView)>,
}

/// Most tabs one pane holds.
pub const MAX_PANE_TABS: usize = 64;

/// Most panes the document area splits into.
pub const MAX_PANES: usize = 16;

pub struct Split {
    /// Side by side (else one above the other).
    pub vertical: bool,
    /// Share of the first pane.
    pub frac: f32,
    /// The active document shows in the first pane (else in the second).
    pub active_first: bool,
    pub pane: Pane,
    /// Panes beyond the second, in order.
    pub more: Vec<Pane>,
    pub sync: Sync,
    /// The focused pane's (zoom, offset) last frame, for Page sync.
    pub last: Option<(f32, Vec2)>,
}

impl Split {
    /// Panes on screen (the active document's included).
    pub fn panes(&self) -> usize {
        2 + self.more.len()
    }
}

/// Split the document area; the new pane shows the active document. Splitting an area that
/// is split already adds another pane (up to [`MAX_PANES`]).
pub fn split(app: &mut AppState, vertical: bool) {
    let Some(d) = app.doc() else { return };
    let mut view = DocView::default();
    view.mode = d.view.mode;
    view.fit = d.view.fit;
    view.go_to_page(d.view.current, d.session.page_count());
    let uid = d.uid;
    if let Some(s) = &mut app.shell.split {
        s.vertical = vertical;
        if s.panes() >= MAX_PANES {
            app.status = format!("The workspace splits into at most {MAX_PANES} panes");
        } else {
            s.more.push(Pane::new(uid, view));
            app.status = format!("{} panes", s.panes());
        }
        return;
    }
    app.shell.split = Some(Split {
        vertical,
        frac: 0.5,
        active_first: true,
        pane: Pane::new(uid, view),
        more: Vec::new(),
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
    s.pane.replace_shown(other, active);
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
    s.pane.show(uid);
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
        s.pane.replace_shown(target, active_uid);
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
            tabs: vec![uid],
            parked: Vec::new(),
        }
    }

    /// Show document `uid`, adding it to the pane's tabs; the tab left keeps its place.
    pub fn show(&mut self, uid: u64) {
        if !self.tabs.contains(&uid) {
            if self.tabs.len() >= MAX_PANE_TABS {
                self.tabs.remove(0);
            }
            self.tabs.push(uid);
        }
        if self.uid == uid {
            return;
        }
        let view = match self.parked.iter().position(|(u, _)| *u == uid) {
            Some(i) => self.parked.remove(i).1,
            None => DocView::default(),
        };
        let old = std::mem::replace(&mut self.view, view);
        let was = self.uid;
        self.parked.retain(|(u, _)| *u != was);
        if self.parked.len() < MAX_PANE_TABS {
            self.parked.push((was, old));
        }
        self.uid = uid;
        self.render = None;
        self.bytes = None;
        self.view.invalidate();
    }

    /// Take tab `uid` out of the pane; the shown tab moves to a neighbour. False when it was
    /// the pane's last tab (the pane itself should close).
    pub fn close_tab(&mut self, uid: u64) -> bool {
        let Some(i) = self.tabs.iter().position(|u| *u == uid) else {
            return true;
        };
        if self.tabs.len() <= 1 {
            return false;
        }
        self.tabs.remove(i);
        if self.uid == uid
            && let Some(next) = self.tabs.get(i.min(self.tabs.len().saturating_sub(1))).copied()
        {
            self.show(next);
        }
        self.parked.retain(|(u, _)| *u != uid);
        true
    }

    /// Forget tabs whose documents closed (the shown one is handled by the caller).
    fn retain_open(&mut self, open: &[u64]) {
        let shown = self.uid;
        self.tabs.retain(|u| open.contains(u) || *u == shown);
        self.parked.retain(|(u, _)| open.contains(u));
        if !self.tabs.contains(&shown) {
            self.tabs.insert(0, shown);
        }
    }

    /// The document it shows changed underneath it (a focus swap): the tab of `from` now
    /// holds `to`.
    fn replace_shown(&mut self, from: u64, to: u64) {
        if !self.tabs.contains(&to) {
            match self.tabs.iter_mut().find(|u| **u == from) {
                Some(t) => *t = to,
                None => self.tabs.push(to),
            }
        }
        self.parked.retain(|(u, _)| *u != to);
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
                let gone = s.pane.uid;
                s.pane.replace_shown(gone, u);
                s.pane.uid = u;
                s.pane.render = None;
                s.pane.bytes = None;
            }
            None => app.shell.split = None,
        }
    }
    let open: Vec<u64> = app.docs.iter().map(|d| d.uid).collect();
    if let Some(s) = &mut app.shell.split {
        s.more.retain(|p| open.contains(&p.uid));
        s.pane.retain_open(&open);
        for p in &mut s.more {
            p.retain_open(&open);
        }
    }
    if app.shell.split.as_ref().is_some_and(|s| !s.more.is_empty()) {
        grid(app, ui);
        return;
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
    if second_pane(app, ui, secondary, 0) {
        app.shell.split = None;
        return;
    }
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
    // Focus follows a press in the other pane (its page area, not its tab bar).
    let pressed = ui.input(|i| {
        i.pointer
            .press_origin()
            .filter(|_| i.pointer.any_pressed())
            .is_some_and(|p| body_of(secondary).contains(p))
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

/// Three or more panes: a grid of cells, the active document in the first.
fn grid(app: &mut AppState, ui: &mut egui::Ui) {
    let Some((n, vertical)) = app.shell.split.as_ref().map(|s| (s.panes(), s.vertical)) else {
        return;
    };
    let full = ui.available_rect_before_wrap();
    let cells = cells(full, n, vertical, 4.0);
    let Some(first) = cells.first().copied() else { return };
    pane_ui(app, ui, first);
    let mut close: Option<usize> = None;
    for (k, cell) in cells.iter().enumerate().skip(1) {
        // Pane k-1 of the split: the second pane, then the others swapped in to draw.
        let idx = k - 1;
        swap_in(app, idx);
        let closed = second_pane(app, ui, *cell, idx);
        swap_in(app, idx);
        if closed {
            close = Some(idx);
        }
    }
    if let Some(idx) = close {
        close_pane(app, idx);
        return;
    }
    app.shell.extra.pane_rect = cells.get(1).copied();
    let pressed = ui.input(|i| {
        i.pointer
            .press_origin()
            .filter(|_| i.pointer.any_pressed())
            .and_then(|p| cells.iter().skip(1).position(|c| body_of(*c).contains(p)))
    });
    if let Some(idx) = pressed {
        swap_in(app, idx);
        focus_pane(app);
    }
    sync(app);
}

/// The cells of `n` panes in `full`: as square a grid as fits, filled row by row (side by
/// side first) or column by column.
pub fn cells(full: Rect, n: usize, vertical: bool, gap: f32) -> Vec<Rect> {
    let n = n.clamp(1, MAX_PANES);
    let mut cols = 1;
    while cols * cols < n {
        cols += 1;
    }
    let rows = n.div_ceil(cols);
    let (cols, rows) = if vertical { (cols, rows) } else { (rows, cols) };
    let w = (full.width() - gap * (cols as f32 - 1.0)) / cols as f32;
    let h = (full.height() - gap * (rows as f32 - 1.0)) / rows as f32;
    (0..n)
        .map(|i| {
            let (c, r) = if vertical {
                (i % cols, i / cols)
            } else {
                (i / rows, i % rows)
            };
            let min = full.min + vec2(c as f32 * (w + gap), r as f32 * (h + gap));
            Rect::from_min_size(min, vec2(w.max(1.0), h.max(1.0)))
        })
        .collect()
}

/// Exchange the second pane with pane `idx` (0 = the second pane itself: nothing to do).
fn swap_in(app: &mut AppState, idx: usize) {
    if let Some(s) = &mut app.shell.split
        && let Some(p) = idx.checked_sub(1).and_then(|i| s.more.get_mut(i))
    {
        std::mem::swap(&mut s.pane, p);
    }
}

/// Close pane `idx` (0 = the second pane); one pane left unsplits.
pub fn close_pane(app: &mut AppState, idx: usize) {
    let Some(s) = &mut app.shell.split else { return };
    if s.more.is_empty() {
        app.shell.split = None;
        return;
    }
    if idx == 0 {
        s.pane = s.more.remove(0);
    } else if idx - 1 < s.more.len() {
        s.more.remove(idx - 1);
    }
    s.last = None;
}

/// Height of a pane's header (its tab bar).
const HEADER: f32 = 24.0;

/// A pane's page area: its rect below the header.
fn body_of(rect: Rect) -> Rect {
    Rect::from_min_max(
        egui::pos2(rect.left(), (rect.top() + HEADER).min(rect.bottom())),
        rect.max,
    )
}

/// One pane of the split (pane `idx`, 0 = the second): its header and canvas. Returns true
/// when its header's close button was pressed.
fn second_pane(app: &mut AppState, ui: &mut egui::Ui, rect: Rect, idx: usize) -> bool {
    let t = Tokens::get(ui.ctx());
    let threads = app.threads;
    let ctx = ui.ctx().clone();
    // A header with the document picker.
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER));
    let body = body_of(rect);
    let names: Vec<(u64, String)> = app.docs.iter().map(|d| (d.uid, d.name.clone())).collect();
    let (max_chars, from_start) = (app.shell.ui.tab_max_chars.clamp(8, 40), app.shell.ui.tab_truncate_start);
    let mut pick = None;
    let mut close = false;
    let mut close_tab = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(head), |ui| {
        ui.painter().rect_filled(head, 0.0, t.chrome);
        // The close button first (right), so a narrow pane keeps it; the pane's tabs take the rest.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(2.0);
            if crate::icons::button(ui, "x", 18.0, false, "Close this pane").clicked() {
                close = true;
            }
            let (cur, tabs) = app
                .shell
                .split
                .as_ref()
                .map(|s| (Some(s.pane.uid), s.pane.tabs.clone()))
                .unwrap_or_default();
            // "+": put any open document in this pane's tabs.
            ui.menu_button(RichText::new("+").size(13.0), |ui| {
                for (u, n) in &names {
                    if ui.selectable_label(tabs.contains(u), n).clicked() {
                        pick = Some(*u);
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text("Show another document in this pane");
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                egui::ScrollArea::horizontal()
                    .id_salt(("split-tabs", idx))
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for u in &tabs {
                                let Some((_, n)) = names.iter().find(|(x, _)| x == u) else {
                                    continue;
                                };
                                let text = super::tabs::truncate(n, max_chars, from_start);
                                let on = Some(*u) == cur;
                                let r = ui
                                    .selectable_label(on, RichText::new(text).size(12.0))
                                    .on_hover_text(n.as_str());
                                if r.clicked() && !on {
                                    pick = Some(*u);
                                }
                                if tabs.len() > 1
                                    && ui
                                        .small_button(RichText::new("\u{d7}").size(10.0))
                                        .on_hover_text("Close this tab in the pane")
                                        .clicked()
                                {
                                    close_tab = Some(*u);
                                }
                                ui.add_space(2.0);
                            }
                        });
                    });
            });
        });
    });
    if close {
        return true;
    }
    if let Some(u) = close_tab
        && let Some(s) = &mut app.shell.split
        && !s.pane.close_tab(u)
    {
        return true;
    }
    let tool = crate::tools::find(app.tool).unwrap_or(&crate::tools::select::TOOL);
    let author = app.author.clone();
    let edit = app.edit.clone();
    let template = app.template().0.cloned();
    let Some(s) = &mut app.shell.split else { return false };
    if let Some(u) = pick {
        s.pane.show(u);
    }
    let uid = s.pane.uid;
    let Some(doc) = app.docs.iter_mut().find(|d| d.uid == uid) else {
        return false;
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
        .scope_builder(
            egui::UiBuilder::new().max_rect(body).id_salt(("split-pane", idx)),
            |ui| {
                ui.set_clip_rect(body.intersect(ui.clip_rect()));
                canvas::show(ui, doc, &cx)
            },
        )
        .inner;
    std::mem::swap(&mut doc.view, &mut s.pane.view);
    std::mem::swap(&mut doc.render, &mut s.pane.render);
    app.apply_canvas_out(out);
    false
}
