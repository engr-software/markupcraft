//! Detached document windows (MultiView on a second monitor): Window > Detach Tab to New Window
//! (or a tab's right-click Detach, or dragging a tab out of the window) MOVES the document into
//! a window of its own, with its own view and renderer; the main window's tab goes with it.
//! Ctrl-dragging a tab out, a tab's Detach with Ctrl held, or Window > Detach a Copy of the Tab
//! leaves the tab in the main window as well (another view of the same file: edits show in
//! both). A detached window holds a tab bar: Add Tab moves another open document into it, a tab
//! is switched to by clicking it and put back in the main window with its x. Drawing and
//! selecting work as in the main window; clicking in a window makes its document the active
//! one, so the panels, menus and keys follow it, while the main window keeps showing its own
//! tab. Reattach (or closing the window) puts its tabs back.

use egui::{RichText, ViewportBuilder, ViewportId};

use super::split::Pane;
use crate::AppState;
use crate::canvas::{self, CanvasCx, DocView};

pub struct Detached {
    /// The window's own id (its tabs come and go).
    pub id: u64,
    /// The tab the window shows.
    pub pane: Pane,
    /// The window's other tabs.
    pub others: Vec<Pane>,
    /// The tabs in the order the tab bar shows them (document uids).
    pub order: Vec<u64>,
    /// Documents of this window that also stay in the main window (detached as a copy).
    pub copies: Vec<u64>,
    /// Split inside the window: a second view of the document, side by side.
    pub second: Option<Pane>,
    /// The second view follows the first (page for page, or by as much as it pans and zooms).
    pub sync: super::split::Sync,
    /// The window follows the main window's view of the document (page for page).
    pub follow_main: bool,
}

impl Detached {
    /// The document the window shows.
    pub fn uid(&self) -> u64 {
        self.pane.uid
    }

    /// Every document in the window, in tab order.
    pub fn uids(&self) -> Vec<u64> {
        self.order.clone()
    }

    pub fn holds(&self, uid: u64) -> bool {
        self.order.contains(&uid)
    }

    /// Show tab `uid` (it must be one of the window's).
    fn switch_to(&mut self, uid: u64) {
        if self.pane.uid == uid {
            return;
        }
        if let Some(o) = self.others.iter_mut().find(|p| p.uid == uid) {
            std::mem::swap(&mut self.pane, o);
            self.second = None;
        }
    }

    /// Take tab `uid` out of the window; false when the window is left empty.
    fn remove(&mut self, uid: u64) -> bool {
        self.order.retain(|u| *u != uid);
        self.copies.retain(|u| *u != uid);
        if self.pane.uid == uid {
            if self.others.is_empty() {
                return false;
            }
            self.pane = self.others.remove(0);
            self.second = None;
        } else {
            self.others.retain(|p| p.uid != uid);
        }
        true
    }
}

fn pane_for(app: &AppState, uid: u64) -> Option<Pane> {
    let d = app.docs.iter().find(|d| d.uid == uid)?;
    let mut view = DocView::default();
    view.mode = d.view.mode;
    view.fit = d.view.fit;
    view.go_to_page(d.view.current, d.session.page_count());
    Some(Pane::new(uid, view))
}

/// The window holding document `uid`, if any.
pub fn window_of(app: &AppState, uid: u64) -> Option<usize> {
    app.shell.extra.detached.iter().position(|w| w.holds(uid))
}

/// Documents moved out of the main window (in a detached window, not as a copy).
pub fn moved_out(app: &AppState) -> Vec<u64> {
    app.shell
        .extra
        .detached
        .iter()
        .flat_map(|w| w.order.iter().filter(|u| !w.copies.contains(u)).copied())
        .collect()
}

/// The documents the main window's tab bar shows.
pub fn main_window_docs(app: &AppState) -> Vec<u64> {
    let moved = moved_out(app);
    app.docs.iter().map(|d| d.uid).filter(|u| !moved.contains(u)).collect()
}

/// Detach the active document into a new window, moving its tab out of the main window.
pub fn detach(app: &mut AppState) {
    detach_with(app, false);
}

/// Detach the active document into a new window; `copy` leaves its tab in the main window.
pub fn detach_with(app: &mut AppState, copy: bool) {
    let Some(d) = app.doc() else { return };
    let (uid, name) = (d.uid, d.name.clone());
    if window_of(app, uid).is_some() {
        app.status = format!("{name} already has a window");
        return;
    }
    let Some(pane) = pane_for(app, uid) else { return };
    let id = app
        .shell
        .extra
        .detached
        .iter()
        .map(|w| w.id)
        .max()
        .map_or(1, |m| m.saturating_add(1));
    app.shell.extra.detached.push(Detached {
        id,
        pane,
        others: Vec::new(),
        order: vec![uid],
        copies: if copy { vec![uid] } else { Vec::new() },
        second: None,
        sync: super::split::Sync::Off,
        follow_main: false,
    });
    app.status = if copy {
        format!("{name} detached to a new window (the tab stays here too)")
    } else {
        format!("{name} moved to a new window")
    };
}

/// Move document `uid` into window `w` as a tab (out of the main window).
pub fn add_tab(app: &mut AppState, w: usize, uid: u64) {
    if window_of(app, uid).is_some() {
        return;
    }
    let Some(pane) = pane_for(app, uid) else { return };
    if let Some(win) = app.shell.extra.detached.get_mut(w) {
        win.others.push(pane);
        win.order.push(uid);
        win.switch_to(uid);
    }
}

/// Put tab `uid` back in the main window.
pub fn reattach_tab(app: &mut AppState, uid: u64) {
    if let Some(w) = window_of(app, uid)
        && let Some(win) = app.shell.extra.detached.get_mut(w)
        && !win.remove(uid)
    {
        app.shell.extra.detached.remove(w);
    }
}

fn main_doc_id() -> egui::Id {
    egui::Id::new("markupcraft-main-window-doc")
}

/// What the main window's document area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainView {
    /// The active document (it is in the main window).
    Active,
    /// Another document: the active one is in a detached window (restore it afterwards).
    Swapped(u64),
    /// Nothing: every document is in a detached window.
    Empty,
}

/// Before the main window's document area draws: when the active document was moved to a
/// detached window, the main window shows its own tab (the one it showed last) instead.
pub fn enter_main(app: &mut AppState, ui: &egui::Ui) -> MainView {
    let moved = moved_out(app);
    let Some(active) = app.doc().map(|d| d.uid) else {
        return MainView::Active;
    };
    if !moved.contains(&active) {
        ui.data_mut(|m| m.insert_temp(main_doc_id(), active));
        return MainView::Active;
    }
    let last: Option<u64> = ui.data(|m| m.get_temp(main_doc_id()));
    let main = last
        .filter(|u| !moved.contains(u) && app.docs.iter().any(|d| d.uid == *u))
        .or_else(|| app.docs.iter().map(|d| d.uid).find(|u| !moved.contains(u)));
    let Some(main) = main else { return MainView::Empty };
    ui.data_mut(|m| m.insert_temp(main_doc_id(), main));
    if let Some(i) = app.docs.iter().position(|d| d.uid == main) {
        app.active = i;
    }
    MainView::Swapped(active)
}

/// After the main window's document area drew: give the active document back to the detached
/// window unless the user worked in the main window (a click there, or a tab chosen).
pub fn leave_main(app: &mut AppState, ui: &egui::Ui, area: egui::Rect, view: MainView) {
    let MainView::Swapped(back) = view else { return };
    let main: Option<u64> = ui.data(|m| m.get_temp(main_doc_id()));
    let now = app.doc().map(|d| d.uid);
    let pressed_here =
        ui.input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| area.contains(p)));
    if now.is_some() && now != main && now != Some(back) {
        // A main-window tab was chosen: it becomes the main window's and the active document.
        if let Some(u) = now {
            ui.data_mut(|m| m.insert_temp(main_doc_id(), u));
        }
        return;
    }
    if let Some(i) = app.docs.iter().position(|d| d.uid == back) {
        app.active = i;
    }
    // A press in the main window makes its document active, unless a detached window took
    // it (decided once the windows have drawn, in `show`).
    if pressed_here && let Some(u) = main {
        ui.data_mut(|m| m.insert_temp(main_press_id(), u));
    }
}

fn main_press_id() -> egui::Id {
    egui::Id::new("markupcraft-main-window-press")
}

/// Draw every detached window.
pub fn show(app: &mut AppState, ctx: &egui::Context) {
    // Documents that closed leave their windows; empty windows go away.
    let open: Vec<u64> = app.docs.iter().map(|d| d.uid).collect();
    let mut k = 0;
    while let Some(w) = app.shell.extra.detached.get_mut(k) {
        let gone: Vec<u64> = w.order.iter().copied().filter(|u| !open.contains(u)).collect();
        let mut keep = true;
        for u in gone {
            keep &= w.remove(u);
        }
        if keep {
            k += 1;
        } else {
            app.shell.extra.detached.remove(k);
        }
    }
    let ids: Vec<u64> = app.shell.extra.detached.iter().map(|w| w.id).collect();
    let pending: Option<u64> = ctx.data_mut(|m| {
        let p = m.get_temp(main_press_id());
        m.remove::<u64>(main_press_id());
        p
    });
    let mut window_pressed = false;
    for id in ids {
        let Some(uid) = app.shell.extra.detached.iter().find(|w| w.id == id).map(Detached::uid) else {
            continue;
        };
        let Some(name) = app.docs.iter().find(|d| d.uid == uid).map(|d| d.name.clone()) else {
            continue;
        };
        let builder = ViewportBuilder::default()
            .with_title(format!("{name} - MarkupCraft"))
            .with_inner_size([900.0, 700.0]);
        let close = ctx.show_viewport_immediate(ViewportId::from_hash_of(("detached", id)), builder, |ui, _class| {
            let r = ui.max_rect();
            window_pressed |=
                ui.input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| r.contains(p)));
            window_ui(app, ui, id)
        });
        if close {
            app.shell.extra.detached.retain(|w| w.id != id);
        }
    }
    if let Some(u) = pending
        && !window_pressed
        && let Some(i) = app.docs.iter().position(|d| d.uid == u)
    {
        app.active = i;
    }
}

/// The window's tab bar: its tabs (click to show, x to put back in the main window) and Add
/// Tab (move another open document in).
fn tab_bar(app: &mut AppState, ui: &mut egui::Ui, id: u64) {
    let Some(wi) = app.shell.extra.detached.iter().position(|w| w.id == id) else {
        return;
    };
    let Some(w) = app.shell.extra.detached.get(wi) else {
        return;
    };
    let tabs: Vec<(u64, String)> = w
        .order
        .iter()
        .filter_map(|u| app.docs.iter().find(|d| d.uid == *u).map(|d| (*u, d.name.clone())))
        .collect();
    let current = w.uid();
    let others: Vec<(u64, String)> = app
        .docs
        .iter()
        .filter(|d| window_of(app, d.uid).is_none())
        .map(|d| (d.uid, d.name.clone()))
        .collect();
    let (mut show, mut back, mut add) = (None, None, None);
    ui.horizontal(|ui| {
        for (u, name) in &tabs {
            let r = ui.add(egui::Button::new(RichText::new(name).size(12.0)).selected(*u == current));
            if r.clicked() {
                show = Some(*u);
            }
            if ui
                .small_button("x")
                .on_hover_text(format!("Put {name} back in the main window"))
                .clicked()
            {
                back = Some(*u);
            }
            ui.add_space(4.0);
        }
        ui.add_enabled_ui(!others.is_empty(), |ui| {
            ui.menu_button("Add Tab", |ui| {
                for (u, name) in &others {
                    if ui.button(name).clicked() {
                        add = Some(*u);
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text("Move another open document into this window");
        });
    });
    if let Some(u) = show
        && let Some(w) = app.shell.extra.detached.get_mut(wi)
    {
        w.switch_to(u);
        if let Some(i) = app.docs.iter().position(|d| d.uid == u) {
            app.active = i;
        }
    }
    if let Some(u) = add {
        add_tab(app, wi, u);
    }
    if let Some(u) = back {
        reattach_tab(app, u);
    }
}

/// One detached window's contents; true when it should close.
fn window_ui(app: &mut AppState, ui: &mut egui::Ui, id: u64) -> bool {
    let mut close = ui.input(|i| i.viewport().close_requested());
    tab_bar(app, ui, id);
    let Some(uid) = app.shell.extra.detached.iter().find(|w| w.id == id).map(Detached::uid) else {
        return true;
    };
    ui.horizontal(|ui| {
        if ui.button("Reattach").clicked() {
            close = true;
        }
        if let Some(w) = app.shell.extra.detached.iter_mut().find(|w| w.id == id) {
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
            egui::ComboBox::from_id_salt(("detached-sync", id))
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
    let Some(w) = app.shell.extra.detached.iter_mut().find(|w| w.id == id) else {
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
                    .id_salt(("detached-canvas", id, uid, k)),
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
