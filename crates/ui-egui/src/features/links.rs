//! Links: the Hyperlink tool (drag the link area, or the text to link, then choose where it
//! goes), Edit Action on a link, a markup or a bookmark (a page with a zoom, a Place, a view
//! rectangle, a Space, a web address or a file, relative or full path), the links drawn on the
//! pages, and following a link.

use std::path::PathBuf;

use egui::{Color32, RichText};
use markupcraft_engine::links::{LinkInfo, LinkLook, LinkTarget, Zoom};
use markupcraft_geom::{Point, Rect};

use super::Mark;
use crate::{AppState, DocTab, actions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetKind {
    #[default]
    Page,
    Place,
    View,
    Space,
    Url,
    File,
}

/// What the action dialog edits.
#[derive(Debug, Clone, PartialEq)]
pub enum Editing {
    /// A new link over this box.
    NewLink,
    /// An existing link (its id).
    Link(String),
    /// A markup's action (its id).
    Markup(String),
    /// A bookmark's action (its path).
    Bookmark(Vec<usize>),
    /// Several links at once (their ids): one new action for all of them.
    Links(Vec<String>),
}

pub struct LinksState {
    pub highlight: bool,
    pub selected: Option<String>,
    /// Hyperlinks list: more links picked with Ctrl+click or Shift+click (besides `selected`).
    pub picked: Vec<String>,
    /// Hyperlinks list: only links whose page or target contains this.
    pub filter: String,
    /// A dragged link area waiting for its target.
    pub pending: Option<(usize, Rect)>,
    /// The action dialog is open for this.
    pub editing: Option<Editing>,
    /// New link: fit the link to the words inside the box.
    pub on_text: bool,
    pub kind: TargetKind,
    pub page: usize,
    pub zoom: Zoom,
    pub place: String,
    /// View: page (1-based) and rectangle (x0, y0, x1, y1).
    pub view_page: usize,
    pub view: [f64; 4],
    pub space: String,
    pub url: String,
    pub file: String,
    pub file_page: usize,
    pub relative: bool,
    pub look: LinkLook,
    /// The Places list: a new Place's name.
    pub new_place: String,
    pub place_filter: String,
    pub message: String,
}

impl Default for LinksState {
    fn default() -> Self {
        Self {
            highlight: false,
            selected: None,
            picked: Vec::new(),
            filter: String::new(),
            pending: None,
            editing: None,
            on_text: false,
            kind: TargetKind::Page,
            page: 1,
            zoom: Zoom::FitPage,
            place: String::new(),
            view_page: 1,
            view: [0.0, 0.0, 612.0, 792.0],
            space: String::new(),
            url: "https://".into(),
            file: String::new(),
            file_page: 0,
            relative: true,
            look: LinkLook::default(),
            new_place: String::new(),
            place_filter: String::new(),
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

    /// Fill the dialog from an existing target.
    pub fn load(&mut self, t: &LinkTarget) {
        match t {
            LinkTarget::Page(p) => {
                self.kind = TargetKind::Page;
                self.page = p + 1;
                self.zoom = Zoom::FitPage;
            }
            LinkTarget::Zoomed { page, zoom } => {
                self.kind = TargetKind::Page;
                self.page = page + 1;
                self.zoom = *zoom;
            }
            LinkTarget::View { page, rect } => {
                self.kind = TargetKind::View;
                self.view_page = page + 1;
                self.view = rect.as_array();
            }
            LinkTarget::Place(n) => {
                self.kind = TargetKind::Place;
                self.place = n.clone();
            }
            LinkTarget::Url(u) => {
                self.kind = TargetKind::Url;
                self.url = u.clone();
            }
            LinkTarget::File { path, page } => {
                self.kind = TargetKind::File;
                self.file = path.clone();
                self.file_page = page.map_or(0, |p| p + 1);
            }
            LinkTarget::FileView { path, page, .. } => {
                self.kind = TargetKind::File;
                self.file = path.clone();
                self.file_page = page + 1;
            }
            LinkTarget::Other(_) => {}
        }
    }
}

/// What a link goes to, as text.
pub fn describe(t: &LinkTarget) -> String {
    match t {
        LinkTarget::Page(p) => format!("Page {}", p + 1),
        LinkTarget::Zoomed { page, zoom } => format!("Page {} ({})", page + 1, zoom.name().replace('_', " ")),
        LinkTarget::View { page, rect } => format!(
            "Page {} view {:.0},{:.0} to {:.0},{:.0}",
            page + 1,
            rect.x0,
            rect.y0,
            rect.x1,
            rect.y1
        ),
        LinkTarget::Place(n) => format!("Place {n}"),
        LinkTarget::Url(u) => u.clone(),
        LinkTarget::File { path, page } => match page {
            Some(p) => format!("{path} (page {})", p + 1),
            None => path.clone(),
        },
        LinkTarget::FileView { path, page, .. } => format!("{path} (page {}, view)", page + 1),
        LinkTarget::Other(s) => s.clone(),
    }
}

pub fn rect_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    match super::rect_of(pts) {
        Some(r) => {
            let l = &mut app.features.links;
            l.pending = Some((page, r));
            l.editing = Some(Editing::NewLink);
            l.message.clear();
        }
        None => app.status = "Drag a box over the link area".into(),
    }
}

/// Show `rect` of `page` filling the view.
fn show_view(app: &mut AppState, page: usize, rect: Rect, ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    let Some(d) = app.doc_mut() else { return };
    let n = d.session.page_count();
    d.view.go_to_page(page, n);
    if let Some(render) = d.render.as_ref() {
        let pages = render.pages();
        d.view.relayout(pages);
        d.view.center_on(
            page,
            Point::new((rect.x0 + rect.x1) / 2.0, (rect.y0 + rect.y1) / 2.0),
            pages,
        );
        d.view.relayout(pages);
        let a = d.view.user_to_screen(page, Point::new(rect.x0, rect.y0), pages);
        let b = d.view.user_to_screen(page, Point::new(rect.x1, rect.y1), pages);
        if let (Some(a), Some(b)) = (a, b) {
            d.view.zoom_to_rect(egui::Rect::from_two_pos(a, b), pages, now);
        }
    }
}

/// Follow a link: go to its page, Place or view, open its file, or open its web address.
pub fn follow(app: &mut AppState, l: &LinkInfo, ctx: &egui::Context) {
    follow_target(app, &l.target, ctx);
}

pub fn follow_target(app: &mut AppState, t: &LinkTarget, ctx: &egui::Context) {
    match t {
        LinkTarget::Page(p) | LinkTarget::Zoomed { page: p, .. } => {
            if let Some(d) = app.doc_mut() {
                let n = d.session.page_count();
                d.view.go_to_page(*p, n);
                // The target's zoom: fit page / fit width / actual size (inherit keeps it).
                match t {
                    LinkTarget::Zoomed {
                        zoom: Zoom::FitWidth, ..
                    } => d.view.set_fit(crate::canvas::Fit::Width),
                    LinkTarget::Zoomed {
                        zoom: Zoom::FitPage, ..
                    } => d.view.set_fit(crate::canvas::Fit::Page),
                    _ => {}
                }
            }
            if matches!(t, LinkTarget::Zoomed { zoom: Zoom::Actual, .. }) {
                app.set_zoom(1.0, ctx);
            }
        }
        LinkTarget::View { page, rect } => show_view(app, *page, *rect, ctx),
        LinkTarget::Place(name) => {
            let place = app
                .doc()
                .and_then(|d| d.session.places().into_iter().find(|p| &p.name == name));
            match place.and_then(|p| p.page.map(|pg| (pg, p))) {
                Some((pg, p)) => match (p.left, p.top) {
                    (Some(x), Some(y)) => show_view(app, pg, Rect::new(x, y - 300.0, x + 400.0, y), ctx),
                    _ => {
                        if let Some(d) = app.doc_mut() {
                            let n = d.session.page_count();
                            d.view.go_to_page(pg, n);
                        }
                    }
                },
                None => app.status = format!("The Place {name:?} is not in this document"),
            }
        }
        LinkTarget::Url(u) => super::more6::web::follow_url(app, ctx, u),
        LinkTarget::File { path, page } => open_file(app, path, *page, None, ctx),
        LinkTarget::FileView { path, page, rect } => open_file(app, path, Some(*page), Some(*rect), ctx),
        LinkTarget::Other(s) => app.status = format!("This link runs {s}"),
    }
}

fn open_file(app: &mut AppState, path: &str, page: Option<usize>, rect: Option<Rect>, ctx: &egui::Context) {
    let base = app
        .doc()
        .and_then(|d| d.path.as_ref().and_then(|p| p.parent().map(|p| p.to_path_buf())));
    let p = PathBuf::from(path);
    let p = if p.is_relative() {
        base.map_or(p.clone(), |b| b.join(&p))
    } else {
        p
    };
    if !p.is_file() {
        app.status = format!("{} is not found", p.display());
        return;
    }
    app.open_path(&p);
    match (page, rect) {
        (Some(pg), Some(r)) => show_view(app, pg, r, ctx),
        (Some(pg), None) => {
            if let Some(d) = app.doc_mut() {
                let n = d.session.page_count();
                d.view.go_to_page(pg, n);
            }
        }
        _ => {}
    }
}

/// The target the dialog describes.
fn dialog_target(app: &AppState) -> Result<LinkTarget, String> {
    let l = &app.features.links;
    Ok(match l.kind {
        TargetKind::Page => match l.zoom {
            Zoom::FitPage => LinkTarget::Page(l.page.saturating_sub(1)),
            z => LinkTarget::Zoomed {
                page: l.page.saturating_sub(1),
                zoom: z,
            },
        },
        TargetKind::Place => {
            if l.place.trim().is_empty() {
                return Err("Choose a Place".into());
            }
            LinkTarget::Place(l.place.trim().to_string())
        }
        TargetKind::View => LinkTarget::View {
            page: l.view_page.saturating_sub(1),
            rect: Rect::new(l.view[0], l.view[1], l.view[2], l.view[3]).normalized(),
        },
        TargetKind::Space => {
            let d = app.doc().ok_or("No document")?;
            let (page, sp) = d
                .session
                .spaces(None)
                .into_iter()
                .find(|(_, s)| s.id == l.space)
                .ok_or("Choose a Space")?;
            let b = markupcraft_geom::bbox(&sp.pts).ok_or("The Space has no outline")?;
            LinkTarget::View { page, rect: b }
        }
        TargetKind::Url => LinkTarget::Url(l.url.trim().to_string()),
        TargetKind::File => {
            let mut path = l.file.trim().to_string();
            if l.relative
                && let Some(base) = app.doc().and_then(|d| d.path.as_ref()).and_then(|p| p.parent())
                && let Ok(rel) = std::path::Path::new(&path).strip_prefix(base)
            {
                path = rel.to_string_lossy().replace('\\', "/");
            }
            LinkTarget::File {
                path,
                page: (l.file_page > 0).then(|| l.file_page - 1),
            }
        }
    })
}

/// Set to Current View: the View target becomes what the canvas shows now.
fn current_view(app: &mut AppState) {
    let Some(d) = app.doc() else { return };
    let Some(render) = d.render.as_ref() else { return };
    let Some((page, c)) = d.view.center_point(render.pages()) else {
        return;
    };
    let vp = d.view.viewport();
    let scale = f64::from(d.view.zoom * crate::canvas::PT).max(1e-6);
    let (hw, hh) = (
        f64::from(vp.width()) / scale / 2.0,
        f64::from(vp.height()) / scale / 2.0,
    );
    let l = &mut app.features.links;
    l.kind = TargetKind::View;
    l.view_page = page + 1;
    l.view = [c.x - hw, c.y - hh, c.x + hw, c.y + hh];
}

/// Edit Action on the selected markup (Ctrl+Shift+E).
pub fn edit_markup_action(app: &mut AppState) {
    let Some(d) = app.doc() else { return };
    let Some(id) = d.selection().last().cloned() else {
        app.status = "Select a markup first".into();
        return;
    };
    let current = d.session.markup_action(&id).ok().flatten();
    let l = &mut app.features.links;
    if let Some(t) = &current {
        l.load(t);
    }
    l.editing = Some(Editing::Markup(id));
}

/// Edit Action on a link.
pub fn edit_link_action(app: &mut AppState, link: &LinkInfo) {
    let l = &mut app.features.links;
    l.load(&link.target);
    l.editing = Some(Editing::Link(link.id.clone()));
}

/// Edit Action on several links at once (they all get the action chosen).
pub fn edit_links_action(app: &mut AppState, ids: Vec<String>) {
    let first = app
        .doc()
        .and_then(|d| d.session.links().into_iter().find(|l| ids.first() == Some(&l.id)));
    let l = &mut app.features.links;
    if let Some(f) = &first {
        l.load(&f.target);
    }
    l.editing = Some(Editing::Links(ids));
}

/// Hyperlinks list: the links the filter keeps (`text` against the page and the target).
pub fn filtered<'a>(all: &'a [LinkInfo], text: &str) -> Vec<&'a LinkInfo> {
    let f = text.trim().to_lowercase();
    all.iter()
        .filter(|l| {
            f.is_empty() || describe(&l.target).to_lowercase().contains(&f) || format!("p.{}", l.page + 1).contains(&f)
        })
        .collect()
}

/// Edit Action on a bookmark.
pub fn edit_bookmark_action(app: &mut AppState, path: Vec<usize>) {
    let current = app
        .doc()
        .and_then(|d| d.session.bookmark_details(&path).ok())
        .and_then(|b| b.target);
    let l = &mut app.features.links;
    if let Some(t) = &current {
        l.load(t);
    }
    l.editing = Some(Editing::Bookmark(path));
}

/// The action dialog: a new link, or Edit Action on a link, markup or bookmark.
pub fn window(app: &mut AppState, ctx: &egui::Context) {
    let Some(editing) = app.features.links.editing.clone() else {
        return;
    };
    let count = app.doc().map_or(0, |d| d.session.page_count());
    let places: Vec<String> = app
        .doc()
        .map(|d| d.session.places().into_iter().map(|p| p.name).collect())
        .unwrap_or_default();
    let spaces: Vec<(String, String)> = app
        .doc()
        .map(|d| {
            d.session
                .spaces(None)
                .into_iter()
                .map(|(_, s)| (s.id, s.name))
                .collect()
        })
        .unwrap_or_default();
    let title = match editing {
        Editing::NewLink => "Hyperlink",
        Editing::Links(_) => "Edit Action (several links)",
        _ => "Edit Action",
    };
    let (mut open, mut ok, mut here) = (true, false, false);
    super::window(title).open(&mut open).show(ctx, |ui| {
        let l = &mut app.features.links;
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut l.kind, TargetKind::Page, "Page");
            ui.selectable_value(&mut l.kind, TargetKind::Place, "Place");
            ui.selectable_value(&mut l.kind, TargetKind::View, "View");
            ui.selectable_value(&mut l.kind, TargetKind::Space, "Space");
            ui.selectable_value(&mut l.kind, TargetKind::Url, "Web address");
            ui.selectable_value(&mut l.kind, TargetKind::File, "File");
        });
        match l.kind {
            TargetKind::Page => {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut l.page)
                            .range(1..=count.max(1))
                            .prefix("Page "),
                    );
                    for (z, n) in [
                        (Zoom::FitPage, "Fit page"),
                        (Zoom::FitWidth, "Fit width"),
                        (Zoom::Actual, "Actual size"),
                        (Zoom::Inherit, "Inherit"),
                    ] {
                        ui.selectable_value(&mut l.zoom, z, n);
                    }
                });
            }
            TargetKind::Place => {
                egui::ComboBox::from_id_salt("link-place")
                    .selected_text(if l.place.is_empty() {
                        "(choose)"
                    } else {
                        l.place.as_str()
                    })
                    .show_ui(ui, |ui| {
                        for p in &places {
                            ui.selectable_value(&mut l.place, p.clone(), p);
                        }
                    });
            }
            TargetKind::View => {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut l.view_page)
                            .range(1..=count.max(1))
                            .prefix("Page "),
                    );
                    for v in l.view.iter_mut() {
                        ui.add(egui::DragValue::new(v).speed(1.0));
                    }
                });
                if ui.button("Set to Current View").clicked() {
                    here = true;
                }
            }
            TargetKind::Space => {
                let shown = spaces
                    .iter()
                    .find(|s| s.0 == l.space)
                    .map_or("(choose)", |s| s.1.as_str());
                egui::ComboBox::from_id_salt("link-space")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (id, name) in &spaces {
                            ui.selectable_value(&mut l.space, id.clone(), name);
                        }
                    });
            }
            TargetKind::Url => {
                ui.text_edit_singleline(&mut l.url);
            }
            TargetKind::File => {
                ui.add(egui::TextEdit::singleline(&mut l.file).hint_text("path to a file"));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut l.file_page)
                            .range(0..=100_000)
                            .prefix("Page "),
                    )
                    .on_hover_text("0 = just open the file");
                    ui.checkbox(&mut l.relative, "Relative path");
                });
            }
        }
        if editing == Editing::NewLink {
            ui.checkbox(&mut l.on_text, "Fit the link to the text in the box");
        }
        if matches!(editing, Editing::NewLink | Editing::Link(_) | Editing::Links(_)) {
            ui.horizontal(|ui| {
                ui.label("Border");
                ui.add(egui::DragValue::new(&mut l.look.width).range(0.0..=12.0));
                super::color_edit(ui, &mut l.look.color);
            });
        }
        if !l.message.is_empty() {
            ui.label(RichText::new(&l.message).small());
        }
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                ok = true;
            }
            if ui.button("Cancel").clicked() {
                l.pending = None;
                l.editing = None;
            }
        });
    });
    if !open {
        app.features.links.pending = None;
        app.features.links.editing = None;
    }
    if here {
        current_view(app);
    }
    if !ok {
        return;
    }
    let target = match dialog_target(app) {
        Ok(t) => t,
        Err(e) => {
            app.features.links.message = e;
            return;
        }
    };
    let (look, on_text, pending) = {
        let l = &app.features.links;
        (l.look, l.on_text, l.pending)
    };
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let r = match &editing {
        Editing::NewLink => {
            let Some((page, rect)) = pending else { return };
            let r = if on_text {
                d.session.add_link_on_text(page, rect, &target, look)
            } else {
                d.session.add_link(page, rect, &target, look)
            };
            r.map(Some)
        }
        Editing::Link(id) => d.session.edit_link(id, Some(&target), None, Some(look)).map(|_| None),
        Editing::Markup(id) => d.session.set_markup_action(id, Some(&target)).map(|_| None),
        Editing::Bookmark(path) => d.session.set_bookmark_action(path, &target).map(|_| None),
        Editing::Links(ids) => d.session.edit_links(ids, Some(&target), Some(look)).map(|_| None),
    };
    d.rerender(threads);
    match r {
        Ok(new_id) => {
            let l = &mut app.features.links;
            l.pending = None;
            l.editing = None;
            if let Some(id) = new_id {
                l.selected = Some(id);
                app.show_panel("links");
            }
            app.status = format!("Action: {}", describe(&target));
        }
        Err(e) => app.features.links.message = e.to_string(),
    }
}

/// Create Hyperlinks from URLs on every page.
pub fn from_urls(app: &mut AppState) {
    let threads = app.threads;
    let look = app.features.links.look;
    let Some(d) = app.doc_mut() else { return };
    let r = d.session.links_from_urls(&[], look);
    d.rerender(threads);
    app.status = actions::report(r, |n| format!("Created {}", actions::plural(n, "link")));
}
