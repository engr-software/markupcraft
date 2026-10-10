//! More document features in the interface: saved summary column configurations and the
//! summary straight to the printer (Markup Summary dialog), hyperlinks from the checked search
//! results (Search panel > Link), and links dragged out of a File Access file row onto the page.

use std::path::{Path, PathBuf};

use egui::RichText;
use markupcraft_engine::links::{LinkLook, LinkTarget};
use markupcraft_engine::summary_cols::{ColumnConfig, delete_config, load_configs, save_config};
use markupcraft_geom::{Point, Rect};

use crate::painter::Xf;
use crate::{AppState, actions};

/// Where links made from search results go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkTo {
    #[default]
    Page,
    File,
    Url,
}

#[derive(Default)]
pub struct Docs7State {
    /// Search results > Link: the target window is open.
    pub link_open: bool,
    pub link_to: LinkTo,
    /// 1-based page (LinkTo::Page).
    pub link_page: usize,
    /// A file path or web address.
    pub link_text: String,
    pub message: String,
}

// ---- Markup Summary: saved column configurations and printing ------------------------------

/// The summary dialog's column-configuration row: pick a saved configuration (its columns and
/// whether empty columns are kept), save the current columns under a name, delete one.
pub fn summary_columns_ui(ui: &mut egui::Ui, s: &mut super::summary::SummaryState, cfg: Option<&Path>) {
    ui.checkbox(&mut s.include_empty, "Include empty columns");
    let Some(cfg) = cfg else {
        return;
    };
    let configs = load_configs(cfg).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("Column set:");
        egui::ComboBox::from_id_salt("summary-column-config")
            .selected_text(if s.config_name.is_empty() {
                "(choose)".to_string()
            } else {
                s.config_name.clone()
            })
            .show_ui(ui, |ui| {
                for c in &configs {
                    if ui.selectable_label(s.config_name == c.name, &c.name).clicked() {
                        s.config_name = c.name.clone();
                        s.columns = c.columns.clone();
                        s.include_empty = c.include_empty;
                        s.message = format!("Loaded the column set {}", c.name);
                    }
                }
            });
        ui.add(
            egui::TextEdit::singleline(&mut s.config_name)
                .hint_text("name")
                .desired_width(110.0),
        );
        if ui
            .button("Save Columns")
            .on_hover_text("Save the chosen columns, in order, as a column set")
            .clicked()
        {
            let c = ColumnConfig {
                name: s.config_name.clone(),
                columns: s.columns.clone(),
                include_empty: s.include_empty,
            };
            s.message = match save_config(cfg, &c) {
                Ok(()) => format!("Saved the column set {}", c.name.trim()),
                Err(e) => e.to_string(),
            };
        }
        if ui.button("Delete Columns").clicked() {
            s.message = match delete_config(cfg, &s.config_name) {
                Ok(true) => format!("Deleted the column set {}", s.config_name),
                Ok(false) => format!("No column set {:?}", s.config_name),
                Err(e) => e.to_string(),
            };
        }
    });
}

/// The column order as the dialog lists it: move a chosen column up or down.
pub fn summary_order_ui(ui: &mut egui::Ui, s: &mut super::summary::SummaryState, header_of: &dyn Fn(&str) -> String) {
    let mut swap: Option<(usize, usize)> = None;
    ui.horizontal_wrapped(|ui| {
        ui.label("Order:");
        let n = s.columns.len();
        for (i, id) in s.columns.iter().enumerate() {
            ui.label(RichText::new(header_of(id)).small());
            if i > 0 && ui.small_button("<").on_hover_text("Earlier").clicked() {
                swap = Some((i, i - 1));
            }
            if i + 1 < n && ui.small_button(">").on_hover_text("Later").clicked() {
                swap = Some((i, i + 1));
            }
        }
    });
    if let Some((a, b)) = swap {
        s.columns.swap(a, b);
    }
}

/// The summary's options for document `doc`, empty columns dropped unless they are kept.
pub fn summary_options(app: &AppState) -> Result<markupcraft_engine::summary::SummaryOptions, String> {
    let d = app.doc().ok_or("Open a document")?;
    let s = &app.features.summary;
    let mut o = super::summary::options(s, d.session.page_count())?;
    if !s.include_empty {
        o.drop_empty_columns(d.session.doc());
    }
    Ok(o)
}

/// Print the Markup Summary on the default printer (the print command; the report is a
/// print-ready PDF).
pub fn print_summary(app: &mut AppState) {
    let o = match summary_options(app) {
        Ok(o) => o,
        Err(e) => {
            app.features.summary.message = e;
            return;
        }
    };
    let Some(d) = app.doc() else { return };
    let r = d.session.print_summary(&o, None, 1, false);
    app.status = actions::report(r, |p| {
        format!(
            "Sent the summary of {} to the printer",
            actions::plural(p.markups, "markup")
        )
    });
    app.features.summary.message = app.status.clone();
}

// ---- Search results > Link ---------------------------------------------------------------

/// The target window for links made from the checked search results.
fn link_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.docs7.link_open {
        return;
    }
    let count = app.doc().map_or(1, |d| d.session.page_count());
    let checked = app
        .features
        .search
        .results()
        .iter()
        .filter(|h| h.checked && h.markup.is_none())
        .count();
    let (mut open, mut go) = (true, false);
    super::window("Link Search Results").open(&mut open).show(ctx, |ui| {
        let s = &mut app.features.docs7;
        ui.label(format!("A hyperlink over each of the {checked} checked results, to:"));
        ui.horizontal(|ui| {
            ui.selectable_value(&mut s.link_to, LinkTo::Page, "Page");
            ui.selectable_value(&mut s.link_to, LinkTo::File, "File");
            ui.selectable_value(&mut s.link_to, LinkTo::Url, "Web address");
        });
        match s.link_to {
            LinkTo::Page => {
                s.link_page = s.link_page.clamp(1, count.max(1));
                ui.add(
                    egui::DragValue::new(&mut s.link_page)
                        .range(1..=count.max(1))
                        .prefix("Page "),
                );
            }
            LinkTo::File => {
                ui.add(egui::TextEdit::singleline(&mut s.link_text).hint_text("file path"));
            }
            LinkTo::Url => {
                ui.add(egui::TextEdit::singleline(&mut s.link_text).hint_text("https://"));
            }
        }
        if !s.message.is_empty() {
            ui.label(RichText::new(&s.message).small());
        }
        ui.horizontal(|ui| {
            if ui.button("Create Links").clicked() {
                go = true;
            }
            if ui.button("Cancel").clicked() {
                s.link_open = false;
            }
        });
    });
    if !open {
        app.features.docs7.link_open = false;
    }
    if go {
        link_checked(app);
    }
}

/// Open the Link window for the checked results.
pub fn start_link(app: &mut AppState) {
    let s = &mut app.features.docs7;
    s.link_open = true;
    s.message.clear();
    if s.link_page == 0 {
        s.link_page = 1;
    }
}

/// Make the links (one undo step).
pub fn link_checked(app: &mut AppState) {
    let s = &app.features.docs7;
    let target = match s.link_to {
        LinkTo::Page => LinkTarget::Page(s.link_page.saturating_sub(1)),
        LinkTo::File | LinkTo::Url if s.link_text.trim().is_empty() => {
            app.features.docs7.message = "Type where the links go".into();
            return;
        }
        LinkTo::File => LinkTarget::File {
            path: s.link_text.trim().to_string(),
            page: None,
        },
        LinkTo::Url => LinkTarget::Url(s.link_text.trim().to_string()),
    };
    let hits: Vec<(usize, Vec<Rect>)> = app
        .features
        .search
        .results()
        .iter()
        .filter(|h| h.checked && h.markup.is_none() && h.file.is_none() && !h.rects.is_empty())
        .map(|h| (h.page, h.rects.clone()))
        .collect();
    let uid = app.features.search.doc;
    let threads = app.threads;
    let Some(d) = app.docs.iter_mut().find(|d| d.uid == uid) else {
        app.features.docs7.message = "The search results' document is not open".into();
        return;
    };
    let r = d.session.link_hits(&hits, &target, LinkLook::default());
    d.rerender(threads);
    let msg = actions::report(r, |ids| format!("Made {}", actions::plural(ids.len(), "link")));
    app.status = msg.clone();
    app.features.docs7.message = msg;
    if app.status.starts_with("Made") {
        app.features.docs7.link_open = false;
    }
}

// ---- File Access: drag a file row onto the page ------------------------------------------

/// The drag payload of a File Access file row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLinkDrag(pub PathBuf);

/// A file dropped on a page: the link to make.
#[derive(Debug, Clone)]
struct FileDrop {
    doc: u64,
    page: usize,
    at: Point,
    path: PathBuf,
}

fn drop_id() -> egui::Id {
    egui::Id::new("markupcraft-file-link-drop")
}

/// The size of a link area dropped from the file list, points (1.5 in by 0.5 in).
pub const DROP_LINK_SIZE: (f64, f64) = (108.0, 36.0);

/// A file row starts a drag: it carries the file.
pub fn start_file_drag(ctx: &egui::Context, path: &Path) {
    egui::DragAndDrop::set_payload(ctx, FileLinkDrag(path.to_path_buf()));
}

/// On the canvas: while a file row is dragged over a page, show the link area that dropping
/// it there makes; on release, record the drop.
pub fn canvas_drop(ui: &egui::Ui, resp: &egui::Response, painter: &egui::Painter, xfs: &[(usize, Xf)], doc: u64) {
    let ctx = ui.ctx();
    if !egui::DragAndDrop::has_payload_of_type::<FileLinkDrag>(ctx) {
        return;
    }
    let Some(pos) = ui.input(|i| i.pointer.latest_pos()) else {
        return;
    };
    if !resp.rect.contains(pos) {
        return;
    }
    let Some((page, xf)) = xfs.iter().find(|(_, xf)| xf.rect.contains(pos)) else {
        return;
    };
    let at = xf.to_user(pos);
    let (w, h) = DROP_LINK_SIZE;
    let r = Rect::new(at.x - w / 2.0, at.y - h / 2.0, at.x + w / 2.0, at.y + h / 2.0);
    let a = xf.to_screen(Point::new(r.x0, r.y0));
    let b = xf.to_screen(Point::new(r.x1, r.y1));
    let stroke = egui::Stroke::new(1.5, super::canvas::CURRENT_STROKE);
    painter.rect_stroke(egui::Rect::from_two_pos(a, b), 0.0, stroke, egui::StrokeKind::Middle);
    if ui.input(|i| i.pointer.any_released())
        && let Some(p) = egui::DragAndDrop::take_payload::<FileLinkDrag>(ctx)
    {
        let d = FileDrop {
            doc,
            page: *page,
            at,
            path: p.0.clone(),
        };
        ctx.data_mut(|m| m.insert_temp(drop_id(), d));
    }
}

/// A dropped file becomes a link area that opens it, centred where it was dropped.
fn file_drop(app: &mut AppState, ctx: &egui::Context) {
    let Some(d) = ctx.data_mut(|m| {
        let d = m.get_temp::<FileDrop>(drop_id());
        m.remove::<FileDrop>(drop_id());
        d
    }) else {
        return;
    };
    let threads = app.threads;
    let Some(doc) = app.docs.iter_mut().find(|x| x.uid == d.doc) else {
        return;
    };
    let (w, h) = DROP_LINK_SIZE;
    let r = Rect::new(d.at.x - w / 2.0, d.at.y - h / 2.0, d.at.x + w / 2.0, d.at.y + h / 2.0);
    let target = LinkTarget::File {
        path: d.path.display().to_string(),
        page: None,
    };
    let res = doc.session.add_link(d.page, r, &target, LinkLook::default());
    doc.rerender(threads);
    app.status = actions::report(res, |_| format!("Linked to {}", d.path.display()));
}

/// Once a frame (from `features::frame`).
pub fn frame(app: &mut AppState, ctx: &egui::Context) {
    file_drop(app, ctx);
    link_window(app, ctx);
}

// ---- Header & Footer: the token picker -----------------------------------------------------

/// Insert a token (page number, date, Bates number, file data) into one of the six places.
pub fn hf_token_picker(ui: &mut egui::Ui, hf: &mut markupcraft_engine::marks::HeaderFooter, slots: &[&str]) {
    let id = egui::Id::new("hf-token-slot");
    let mut slot = ui.ctx().data(|m| m.get_temp::<usize>(id)).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.label("Insert into");
        egui::ComboBox::from_id_salt("hf-token-place")
            .selected_text(slots.get(slot).copied().unwrap_or_default())
            .show_ui(ui, |ui| {
                for (i, label) in slots.iter().enumerate() {
                    ui.selectable_value(&mut slot, i, *label);
                }
            });
        ui.menu_button("Insert Token", |ui| {
            for (tok, label) in markupcraft_engine::hf_tokens::all_tokens() {
                if ui.button(label).on_hover_text(tok).clicked() {
                    if let Some(t) = hf.text.get_mut(slot) {
                        if !t.is_empty() && !t.ends_with(' ') {
                            t.push(' ');
                        }
                        t.push_str(tok);
                    }
                    ui.close();
                }
            }
        });
    });
    ui.ctx().data_mut(|m| m.insert_temp(id, slot));
}

// ---- File Access Explorer: path favourites -------------------------------------------------

/// The Explorer's Favorites menu: go to a favourite folder, add the folder shown, or remove
/// it. Returns the folder to go to and a status message.
pub fn favorites_ui(ui: &mut egui::Ui, cfg: Option<&Path>, folder: &Path) -> (Option<PathBuf>, Option<String>) {
    use markupcraft_engine::favorites::{add_favorite, load_favorites, remove_favorite};
    let (mut to, mut msg) = (None, None);
    let Some(cfg) = cfg else {
        return (to, msg);
    };
    let favs = load_favorites(cfg).unwrap_or_default();
    let is_fav = favs.iter().any(|f| f == folder);
    ui.menu_button("Favorites", |ui| {
        for f in &favs {
            let name = f
                .file_name()
                .map_or_else(|| f.display().to_string(), |n| n.to_string_lossy().into_owned());
            if ui.button(name).on_hover_text(f.display().to_string()).clicked() {
                to = Some(f.clone());
                ui.close();
            }
        }
        if !favs.is_empty() {
            ui.separator();
        }
        if !is_fav && ui.button("Add This Folder to Favorites").clicked() {
            msg = Some(match add_favorite(cfg, folder) {
                Ok(_) => format!("Added {} to the favourites", folder.display()),
                Err(e) => e.to_string(),
            });
            ui.close();
        }
        if is_fav && ui.button("Remove This Folder from Favorites").clicked() {
            msg = Some(match remove_favorite(cfg, folder) {
                Ok(_) => format!("Removed {} from the favourites", folder.display()),
                Err(e) => e.to_string(),
            });
            ui.close();
        }
    });
    (to, msg)
}
