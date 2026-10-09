//! Thumbnails (Alt+T): every page as a small picture. Click to go there, Ctrl+click and
//! Shift+click to select several, arrow keys to move between pages, drag a thumbnail to
//! reorder the pages (bookmarks follow), the size slider grows or shrinks them, the label and
//! the page scale show under each (toggles). Right-click for the page commands: rotate, insert
//! blank, insert pages, extract, replace, delete, crop, page setup, cut / copy / paste pages,
//! copy to snapshot, set scale, rename the label, print, markup summary and flatten. Double-click
//! a label (or F2) renames it in place; Tab / Shift+Tab move to the next / previous label. PDFs
//! dropped on the panel are inserted before the page they land on.

use egui::{Align2, Color32, FontId, Sense, Stroke, Vec2, vec2};

use super::{PanelDef, Slot};
use crate::AppState;
use crate::commands::alt;
use crate::theme::Tokens;

pub static PANEL: PanelDef = PanelDef {
    id: "thumbnails",
    title: "Thumbnails",
    icon: "layout-grid",
    slot: Slot::Left,
    keys: alt(egui::Key::T),
    ui,
};

/// The panel's view state (lives in `AppState::shell.thumbs`).
#[derive(Debug, Clone)]
pub struct ThumbState {
    /// Thumbnail width, points.
    pub size: f32,
    pub show_label: bool,
    pub show_scale: bool,
    /// Selected pages (0-based).
    pub selected: Vec<usize>,
    /// The last clicked page, for Shift+click ranges.
    pub anchor: Option<usize>,
    /// Pages cut or copied: a temporary PDF and how many pages it holds.
    pub clipboard: Option<(std::path::PathBuf, usize)>,
    /// A thumbnail being dragged (page) and where it would drop (before this page).
    pub dragging: Option<(usize, usize)>,
    /// Where files dropped now would go (before this page; past the end = after the last).
    pub drop_before: Option<usize>,
}

impl Default for ThumbState {
    fn default() -> Self {
        Self {
            size: 140.0,
            show_label: true,
            show_scale: false,
            selected: Vec::new(),
            anchor: None,
            clipboard: None,
            dragging: None,
            drop_before: None,
        }
    }
}

/// Click handling: plain click selects one, Ctrl toggles, Shift extends from the anchor.
pub fn click(st: &mut ThumbState, page: usize, ctrl: bool, shift: bool) {
    if shift && let Some(a) = st.anchor {
        let (lo, hi) = (a.min(page), a.max(page));
        st.selected = (lo..=hi).collect();
        return;
    }
    if ctrl {
        if let Some(i) = st.selected.iter().position(|p| *p == page) {
            st.selected.remove(i);
        } else {
            st.selected.push(page);
        }
    } else {
        st.selected = vec![page];
    }
    st.anchor = Some(page);
}

/// The pages a thumbnail command acts on: the selection when the clicked page is in it, else
/// that page.
fn targets(st: &ThumbState, page: usize) -> Vec<usize> {
    if st.selected.contains(&page) {
        let mut v = st.selected.clone();
        v.sort_unstable();
        v.dedup();
        v
    } else {
        vec![page]
    }
}

/// Run a thumbnail command on `pages`.
pub fn command(app: &mut AppState, cmd: &str, pages: &[usize]) {
    let threads = app.threads;
    let first = pages.first().copied().unwrap_or(0);
    let last = pages.last().copied().unwrap_or(0);
    let edits_pages = !matches!(
        cmd,
        "copy"
            | "extract"
            | "snapshot"
            | "set_scale"
            | "rename"
            | "print"
            | "summary"
            | "flatten"
            | "stamp"
            | "number"
            | "export_images"
            | "email"
            | "repair"
    );
    if edits_pages && let Some(msg) = crate::shell::extra::page_edits_refused(app) {
        app.status = msg;
        return;
    }
    match cmd {
        "snapshot" => {
            if let Some(d) = app.doc_mut() {
                let n = d.session.page_count();
                d.view.go_to_page(first, n);
            }
            crate::shell::extra::copy_page_snapshot(app);
        }
        "set_scale" => crate::shell::extra::open_scale(app, pages.to_vec()),
        "rename" => start_rename(app, first),
        "print" => app.queue("file.print"),
        "summary" => app.queue("markup.summary"),
        "flatten" => app.queue("document.flatten"),
        "stamp" => app.queue("markup.stamps"),
        "number" => crate::features::partials_more3::open_number(app, pages.to_vec()),
        "export_images" => {
            let text = pages.iter().map(|p| (p + 1).to_string()).collect::<Vec<_>>().join(", ");
            app.features.export.pages = text;
            app.queue("file.export_images");
        }
        "email" => app.queue("file.email"),
        "repair" => app.queue("document.repair"),
        "rotate_cw" | "rotate_ccw" | "delete" | "insert_blank" | "move_up" | "move_down" => {
            let Some(d) = app.doc_mut() else { return };
            let n = d.session.page_count();
            let r = match cmd {
                "rotate_cw" => d.session.rotate_pages(pages, 90).map(|_| "Rotated"),
                "rotate_ccw" => d.session.rotate_pages(pages, -90).map(|_| "Rotated"),
                "delete" if pages.len() >= n => {
                    app.status = "A document keeps at least one page".into();
                    return;
                }
                "delete" => d.session.delete_pages(pages).map(|_| "Deleted"),
                "move_up" => d.session.move_pages(pages, first.saturating_sub(1)).map(|_| "Moved"),
                "move_down" => d.session.move_pages(pages, (last + 2).min(n)).map(|_| "Moved"),
                _ => d
                    .session
                    .insert_blank_pages(last + 1, 1, None)
                    .map(|_| "Inserted a blank page"),
            };
            d.sync_pages(threads);
            app.status = crate::actions::report(r, |s| s.to_string());
            app.shell.thumbs.selected.clear();
            if cmd.starts_with("move") {
                crate::features::partials_more2::after_page_move(app);
            }
        }
        "copy" | "cut" => {
            // pid + a per-process counter: two copies in the same second never share a file.
            static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let tmp = std::env::temp_dir().join(format!(
                "markupcraft-pages-{}-{}-{}.pdf",
                std::process::id(),
                crate::shell::recent::now_secs(),
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let Some(d) = app.doc_mut() else { return };
            let r = d.session.extract_pages(pages, &tmp, cmd == "cut");
            d.sync_pages(threads);
            match r {
                Ok(n) => {
                    app.shell.thumbs.clipboard = Some((tmp, n));
                    app.status = format!(
                        "{} {}",
                        if cmd == "cut" { "Cut" } else { "Copied" },
                        crate::actions::plural(n, "page")
                    );
                }
                Err(e) => app.status = e.to_string(),
            }
            app.shell.thumbs.selected.clear();
        }
        "paste" => {
            let Some((path, _)) = app.shell.thumbs.clipboard.clone() else {
                return;
            };
            let Some(d) = app.doc_mut() else { return };
            let r = d.session.insert_file_pages(last + 1, &path, None);
            d.sync_pages(threads);
            app.status = crate::actions::report(r, |rep| {
                format!(
                    "Pasted {}",
                    crate::actions::plural(rep.pages_after.saturating_sub(rep.pages_before), "page")
                )
            });
        }
        "insert_pages" => {
            app.dialogs.open(
                crate::dialogs::Purpose::InsertPages { at: last + 1 },
                crate::dialogs::PDF,
                false,
            );
        }
        "extract" | "replace" | "crop" | "page_setup" | "rotate_dialog" | "delete_dialog" => {
            app.shell.thumbs.selected = pages.to_vec();
            let id = match cmd {
                "extract" => "document.extract_pages",
                "replace" => "document.replace_pages",
                "crop" => "document.crop_pages",
                "page_setup" => "document.page_setup",
                "rotate_dialog" => "document.rotate_pages",
                _ => "document.delete_pages",
            };
            crate::shell::pages::open(app, id);
        }
        _ => {}
    }
}

/// Start renaming page `page`'s label in place.
pub fn start_rename(app: &mut AppState, page: usize) {
    let label = app
        .doc()
        .and_then(|d| d.session.doc().pages.get(page).map(|p| p.label.clone()))
        .unwrap_or_default();
    app.shell.extra.label_edit = Some((page, label));
}

/// Give page `page` the label `text` ("" = its number again). Undoable.
pub fn rename(app: &mut AppState, page: usize, text: &str) {
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let r = d.session.set_page_labels(&[(page, text.trim().to_string())]);
    d.rerender(threads);
    app.status = crate::actions::report(r, |_| format!("Page {} label set", page + 1));
}

/// Move the dragged pages before page `before`.
pub fn drop_pages(app: &mut AppState, pages: &[usize], before: usize) {
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let r = d.session.move_pages(pages, before);
    d.sync_pages(threads);
    app.status = crate::actions::report(r, |_| format!("Moved {}", crate::actions::plural(pages.len(), "page")));
    app.shell.thumbs.selected.clear();
    crate::features::partials_more2::after_page_move(app);
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    app.thumbs_wanted = true;
    app.shell.extra.thumbs_rect = Some(ui.max_rect());
    let t = Tokens::get(ui.ctx());
    if !app.has_doc() {
        super::empty(ui, "No document open.");
        return;
    }
    // Header: size slider and label toggles.
    ui.horizontal(|ui| {
        let st = &mut app.shell.thumbs;
        ui.add(
            egui::Slider::new(&mut st.size, 60.0..=320.0)
                .show_value(false)
                .text("Size"),
        )
        .on_hover_text("Thumbnail size");
        ui.menu_button("Labels", |ui| {
            ui.checkbox(&mut st.show_label, "Page label");
            ui.checkbox(&mut st.show_scale, "Page scale");
        });
    });
    let (count, scales): (usize, Vec<String>) = match app.doc() {
        Some(d) => (
            d.session.page_count(),
            d.session
                .doc()
                .pages
                .iter()
                .map(|p| {
                    // A page scale is stored as a page-wide viewport (as Revu does), so fall
                    // back to the first viewport like the navigation bar's readout.
                    p.scale
                        .as_ref()
                        .filter(|s| s.valid())
                        .or(p.viewports.first().map(|v| &v.scale))
                        .filter(|s| !s.ratio.is_empty())
                        .map_or_else(|| "Scale Not Set".to_string(), |s| s.ratio.clone())
                })
                .collect(),
        ),
        None => return,
    };
    let st = app.shell.thumbs.clone();
    let width = st.size.min(ui.available_width() - 24.0).clamp(40.0, 320.0);
    let mut go = None;
    let mut clicked: Option<(usize, bool, bool)> = None;
    let mut cmd: Option<(&'static str, Vec<usize>)> = None;
    let mut drag: Option<(usize, usize)> = st.dragging;
    let mut dropped: Option<(Vec<usize>, usize)> = None;
    let mut drop_before: Option<usize> = None;
    let has_clip = st.clipboard.is_some();
    let mut editing = app.shell.extra.label_edit.clone();
    let mut rename_done: Option<(usize, String, i32)> = None;
    let mut rename_start: Option<usize> = None;
    let Some(doc) = app.doc_mut() else { return };
    let Some(render) = doc.render.as_ref() else { return };
    let current = doc.view.current;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(6.0);
        let mut slots: Vec<egui::Rect> = Vec::with_capacity(count);
        for (i, g) in render.pages().iter().enumerate() {
            let h = width * g.height / g.width.max(1.0);
            ui.vertical_centered(|ui| {
                let (rect, resp) = ui.allocate_exact_size(vec2(width, h.min(width * 2.0)), Sense::click_and_drag());
                slots.push(rect);
                if ui.is_rect_visible(rect) {
                    let p = ui.painter();
                    p.rect_filled(rect, 0.0, Color32::WHITE);
                    match doc.view.thumbs.get(&i) {
                        Some(tex) => {
                            p.image(
                                tex.id(),
                                rect,
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        None => {
                            p.text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                "...",
                                FontId::proportional(12.0),
                                t.text_faint,
                            );
                        }
                    }
                    let selected = st.selected.contains(&i);
                    if selected {
                        p.rect_filled(rect, 0.0, t.accent.gamma_multiply(0.15));
                    }
                    let stroke = if i == current || selected {
                        Stroke::new(2.5, t.accent)
                    } else {
                        Stroke::new(1.0, t.border)
                    };
                    p.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Outside);
                }
                if resp.clicked() {
                    let m = ui.input(|i| i.modifiers);
                    clicked = Some((i, m.command, m.shift));
                    if !m.command && !m.shift {
                        go = Some(i);
                    }
                }
                if resp.drag_started() {
                    drag = Some((i, i));
                }
                let menu_pages = targets(&st, i);
                resp.context_menu(|ui| {
                    let n = menu_pages.len();
                    ui.label(egui::RichText::new(crate::actions::plural(n, "page")).weak());
                    for (id, label) in [
                        ("rotate_cw", "Rotate Clockwise"),
                        ("rotate_ccw", "Rotate Counterclockwise"),
                        ("rotate_dialog", "Rotate Pages..."),
                        ("insert_blank", "Insert Blank Page After"),
                        ("insert_pages", "Insert Pages..."),
                        ("extract", "Extract Pages..."),
                        ("replace", "Replace Pages..."),
                        ("crop", "Crop Pages..."),
                        ("page_setup", "Page Setup..."),
                        ("move_up", "Move Up"),
                        ("move_down", "Move Down"),
                        ("cut", "Cut Pages"),
                        ("copy", "Copy Pages"),
                        ("paste", "Paste Pages After"),
                        ("delete", "Delete Pages"),
                        ("snapshot", "Copy Page to Snapshot"),
                        ("set_scale", "Set Scale..."),
                        ("rename", "Rename Page Label"),
                        ("print", "Print..."),
                        ("summary", "Markup Summary..."),
                        ("flatten", "Flatten Markups..."),
                        ("stamp", "Stamp..."),
                        ("number", "Number Pages..."),
                        ("export_images", "Export Pages as Images..."),
                        ("email", "Email..."),
                        ("repair", "Repair PDF"),
                    ] {
                        let enabled = id != "paste" || has_clip;
                        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                            cmd = Some((id, menu_pages.clone()));
                            ui.close();
                        }
                    }
                });
                let mut caption = Vec::new();
                if st.show_label {
                    caption.push(g.label.clone());
                }
                if st.show_scale {
                    caption.push(scales.get(i).cloned().unwrap_or_default());
                }
                match &mut editing {
                    Some((p, text)) if *p == i => {
                        let id = egui::Id::new("thumb-label-edit");
                        let r = ui.add(egui::TextEdit::singleline(text).id(id).desired_width(width));
                        if !r.has_focus() {
                            r.request_focus();
                        }
                        let (enter, esc, tab, shift) = ui.input_mut(|inp| {
                            (
                                inp.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                                inp.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                                inp.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
                                    || inp.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab),
                                inp.modifiers.shift,
                            )
                        });
                        if esc {
                            rename_done = Some((i, String::new(), i32::MIN));
                        } else if tab {
                            rename_done = Some((i, text.clone(), if shift { -1 } else { 1 }));
                        } else if enter || (r.lost_focus() && !tab) {
                            rename_done = Some((i, text.clone(), 0));
                        }
                    }
                    _ => {
                        let r = ui
                            .add(
                                egui::Label::new(
                                    egui::RichText::new(caption.join("  |  "))
                                        .size(11.0)
                                        .color(t.text_muted),
                                )
                                .sense(Sense::click()),
                            )
                            .on_hover_text(format!("Page {} of {count} (double-click to rename)", i + 1));
                        if r.double_clicked() {
                            rename_start = Some(i);
                        }
                    }
                }
            });
            ui.add_space(8.0);
        }
        ui.allocate_space(Vec2::ZERO);
        // Files dragged over the panel would go before the page under the pointer.
        if let Some(p) = ui.input(|i| i.pointer.latest_pos()) {
            drop_before = Some(slots.iter().position(|r| p.y < r.center().y).unwrap_or(slots.len()));
        }
        // Dragging: the drop point is the gap nearest the pointer.
        if let Some((from, _)) = drag {
            let ptr = ui.input(|i| i.pointer.latest_pos());
            if let Some(p) = ptr {
                let before = slots.iter().position(|r| p.y < r.center().y).unwrap_or(slots.len());
                drag = Some((from, before));
                let y = slots
                    .get(before)
                    .map_or_else(|| slots.last().map_or(0.0, |r| r.bottom() + 4.0), |r| r.top() - 4.0);
                if let Some(r) = slots.first() {
                    ui.painter().hline(r.x_range(), y, Stroke::new(2.0, t.accent));
                }
            }
            if ui.input(|i| i.pointer.any_released()) {
                let pages = targets(&st, from);
                if let Some((_, before)) = drag
                    && !(pages.contains(&before) || pages.last().is_some_and(|l| l + 1 == before))
                {
                    dropped = Some((pages, before));
                }
                drag = None;
            }
        }
    });
    // Arrow keys move through the pages while the panel has the pointer.
    let hovered = ui.rect_contains_pointer(ui.max_rect());
    if hovered && !ui.ctx().egui_wants_keyboard_input() {
        let (up, down) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
            )
        });
        if up {
            go = Some(current.saturating_sub(1));
        } else if down {
            go = Some((current + 1).min(count.saturating_sub(1)));
        }
    }
    if let Some(i) = go {
        doc.view.go_to_page(i, count);
    }
    app.shell.thumbs.dragging = drag;
    app.shell.thumbs.drop_before = drop_before;
    let f2 = hovered
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2));
    if f2 {
        rename_start = Some(current);
    }
    if let Some(p) = rename_start {
        start_rename(app, p);
    } else if let Some((p, text, step)) = rename_done {
        app.shell.extra.label_edit = None;
        if step != i32::MIN {
            rename(app, p, &text);
            let next = if step > 0 {
                Some(p + 1).filter(|n| *n < count)
            } else if step < 0 {
                p.checked_sub(1)
            } else {
                None
            };
            if let Some(n) = next {
                start_rename(app, n);
            }
        }
    } else if editing.is_some() {
        app.shell.extra.label_edit = editing;
    }
    if let Some((i, ctrl, shift)) = clicked {
        click(&mut app.shell.thumbs, i, ctrl, shift);
    }
    if let Some((pages, before)) = dropped {
        drop_pages(app, &pages, before);
    }
    if let Some((c, pages)) = cmd {
        command(app, c, &pages);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_clicks_select_like_a_file_list() {
        let mut st = ThumbState::default();
        click(&mut st, 2, false, false);
        assert_eq!(st.selected, vec![2]);
        click(&mut st, 5, false, true);
        assert_eq!(st.selected, vec![2, 3, 4, 5]);
        click(&mut st, 3, true, false);
        assert_eq!(st.selected, vec![2, 4, 5]);
        assert_eq!(targets(&st, 4), vec![2, 4, 5]);
        assert_eq!(targets(&st, 0), vec![0]);
    }
}
