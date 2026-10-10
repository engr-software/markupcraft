//! Document tabs: one per open file, names truncated at the end or the start (preference),
//! drag a tab to reorder (or onto the split view's second pane to show it there), right-click for
//! Close / Close Others / Close All / Save / Open in Split View / Detach to New Window (moves the
//! tab; with Ctrl held, or Detach a Copy, the tab stays here too) / Copy Path, Ctrl+Tab cycles.
//! Tabs moved to a detached window are not shown here. File > Close All and Save All.

use egui::{RichText, vec2};

use crate::theme::Tokens;
use crate::{AppState, icons};

/// The name shown on a tab: at most `max` characters, cut at the end (or the start).
pub fn truncate(name: &str, max: usize, from_start: bool) -> String {
    let n = name.chars().count();
    if n <= max || max < 4 {
        return name.to_string();
    }
    let keep = max - 3;
    if from_start {
        let tail: String = name.chars().skip(n - keep).collect();
        format!("...{tail}")
    } else {
        let head: String = name.chars().take(keep).collect();
        format!("{head}...")
    }
}

/// Close every document but `keep` (asking about unsaved ones).
pub fn close_all(app: &mut AppState, keep: Option<u64>) {
    let uids: Vec<u64> = app.docs.iter().map(|d| d.uid).filter(|u| Some(*u) != keep).collect();
    for u in uids {
        if let Some(i) = app.docs.iter().position(|d| d.uid == u) {
            app.close_doc(i);
        }
    }
    if let Some(k) = keep
        && let Some(i) = app.docs.iter().position(|d| d.uid == k)
    {
        app.active = i;
    }
}

/// Save every document with unsaved changes (Save As for ones without a file).
pub fn save_all(app: &mut AppState) {
    let dirty: Vec<u64> = app
        .docs
        .iter()
        .filter(|d| d.session.is_dirty())
        .map(|d| d.uid)
        .collect();
    let n = dirty.len();
    for u in dirty {
        app.save_doc(u, false, false);
    }
    if n == 0 {
        app.status = "Nothing to save".into();
    }
}

/// Move tab `from` to position `to`, keeping the active document active.
pub fn move_tab(app: &mut AppState, from: usize, to: usize) {
    if from >= app.docs.len() || to >= app.docs.len() || from == to {
        return;
    }
    let active = app.doc().map(|d| d.uid);
    let d = app.docs.remove(from);
    app.docs.insert(to, d);
    if let Some(u) = active
        && let Some(i) = app.docs.iter().position(|d| d.uid == u)
    {
        app.active = i;
    }
}

/// The tab bar over the document area.
pub fn tab_bar(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let mut close = None;
    let mut cmd: Option<(usize, &'static str)> = None;
    let mut drag_to: Option<(usize, usize)> = None;
    let mut drag_x: Option<(usize, f32)> = None;
    let mut to_pane: Option<u64> = None;
    let mut dragging = false;
    let mut detach_out: Option<usize> = None;
    // Tabs moved to a detached window are not in the main window's bar.
    let moved = super::detach::moved_out(app);
    // Ctrl held: a detached tab leaves a copy here.
    let copy = ui.input(|i| i.modifiers.command);
    let pane_rect = app.shell.extra.pane_rect;
    let (max, from_start) = (app.shell.ui.tab_max_chars, app.shell.ui.tab_truncate_start);
    egui::Frame::NONE
        .fill(t.chrome)
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            egui::ScrollArea::horizontal()
                .id_salt("doc-tabs")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let mut rects = Vec::with_capacity(app.docs.len());
                        for i in 0..app.docs.len() {
                            let Some(d) = app.docs.get(i) else { continue };
                            if moved.contains(&d.uid) {
                                continue;
                            }
                            let mut base = truncate(&d.name, max, from_start);
                            if let Some(b) = crate::features::partials_more3::tab_badge(ui.ctx(), d) {
                                base = format!("[{b}] {base}");
                            }
                            let name = if d.session.is_dirty() {
                                format!("{base} *")
                            } else {
                                base
                            };
                            let active = i == app.active;
                            let r = ui
                                .add(
                                    egui::Button::new(RichText::new(name).size(12.0))
                                        .selected(active)
                                        .min_size(vec2(80.0, 22.0)),
                                )
                                .on_hover_text(
                                    d.path
                                        .as_ref()
                                        .map_or_else(|| d.name.clone(), |p| p.display().to_string()),
                                );
                            rects.push(r.rect);
                            // Dragging uses an id that follows the document, not the position.
                            // It covers the button, so it takes the clicks too (a right-click on a tab
                            // opens its menu).
                            let dr =
                                ui.interact(r.rect, egui::Id::new(("doc-tab", d.uid)), egui::Sense::click_and_drag());
                            if r.clicked() || dr.clicked() || dr.drag_started() {
                                app.active = i;
                            }
                            if dr.dragged()
                                && let Some(p) = dr.interact_pointer_pos()
                            {
                                dragging = true;
                                if pane_rect.is_some_and(|r| r.contains(p)) {
                                    // Over the second pane: it shows the document on release.
                                } else {
                                    drag_x = Some((i, p.x));
                                }
                            }
                            if dr.drag_stopped()
                                && let Some(p) = ui.input(|inp| inp.pointer.latest_pos())
                                && pane_rect.is_some_and(|r| r.contains(p))
                            {
                                to_pane = Some(d.uid);
                            }
                            // Dragged out of the window (or released far below the tab bar onto
                            // the edge of the screen): the document gets a window of its own.
                            if dr.drag_stopped() {
                                let screen = ui.ctx().content_rect();
                                let out = ui
                                    .input(|inp| inp.pointer.latest_pos())
                                    .is_none_or(|p| !screen.shrink(2.0).contains(p));
                                if out {
                                    detach_out = Some(i);
                                }
                            }
                            r.union(dr.clone()).context_menu(|ui| {
                                for (id, label) in [
                                    ("close", "Close"),
                                    ("close_others", "Close Others"),
                                    ("close_all", "Close All"),
                                    ("save", "Save"),
                                    ("split", "Open in Split View"),
                                    ("detach", "Detach to New Window"),
                                    ("detach_copy", "Detach a Copy to New Window"),
                                    ("copy_path", "Copy Path"),
                                ] {
                                    if ui.button(label).clicked() {
                                        cmd = Some((i, id));
                                        ui.close();
                                    }
                                }
                            });
                            if icons::button(ui, "x", 18.0, false, "Close").clicked() {
                                close = Some(i);
                            }
                            ui.add_space(6.0);
                        }
                        // A dragged tab goes where the pointer is.
                        if let Some((from, x)) = drag_x {
                            let to = rects.iter().position(|r| x < r.center().x).unwrap_or(rects.len());
                            let to = if to > from { to - 1 } else { to };
                            drag_to = Some((from, to.min(rects.len().saturating_sub(1))));
                        }
                    });
                });
        });
    app.shell.extra.dragging_tab = dragging;
    if let Some(i) = detach_out {
        app.active = i.min(app.docs.len().saturating_sub(1));
        super::detach::detach_with(app, copy);
        return;
    }
    if let Some(uid) = to_pane {
        super::split::show_in_pane(app, uid);
    }
    if let Some((from, to)) = drag_to
        && from != to
    {
        move_tab(app, from, to);
    }
    if let Some(i) = close {
        app.close_doc(i);
        return;
    }
    if let Some((i, c)) = cmd {
        let uid = app.docs.get(i).map(|d| d.uid);
        match c {
            "close" => app.close_doc(i),
            "close_others" => close_all(app, uid),
            "close_all" => close_all(app, None),
            "save" => {
                if let Some(u) = uid {
                    app.save_doc(u, false, false);
                }
            }
            "split" => {
                app.active = i;
                super::split::split(app, true);
            }
            "detach" => {
                app.active = i;
                super::detach::detach_with(app, copy);
            }
            "detach_copy" => {
                app.active = i;
                super::detach::detach_with(app, true);
            }
            "copy_path" => {
                if let Some(p) = app.docs.get(i).and_then(|d| d.path.clone()) {
                    ui.ctx().copy_text(p.display().to_string());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_names_truncate_at_either_end() {
        assert_eq!(truncate("short.pdf", 20, false), "short.pdf");
        assert_eq!(truncate("A-101 Floor Plan Level 1.pdf", 12, false), "A-101 Flo...");
        assert_eq!(truncate("A-101 Floor Plan Level 1.pdf", 12, true), "...vel 1.pdf");
    }
}
