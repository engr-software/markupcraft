//! The document canvas: GPU-textured page rasters and tiles from the background render pool,
//! smooth pan and zoom, single-page and continuous modes, and markups drawn from the model.
//! What the pointer and keys do with the active tool lives in `interact.rs`.
//!
//! Coordinates: *content space* is the laid-out document in screen points (origin at the top
//! left of the first page's margin); the viewport shows content from `offset`. Page rasters
//! are requested at exactly the device scale, so they map texel for texel onto the screen.

use std::collections::HashMap;

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2, pos2, vec2,
};
use markupcraft_geom::Point;
use markupcraft_model::Markup;
use markupcraft_render::{PageGeom, RenderDoc, RenderRequest, device_pixels};

use crate::actions::editable;
use crate::interact::{self, Draft, Gesture, TextEditor};
use crate::painter::{self, Xf};
use crate::snapping::Snapped;
use crate::theme::Tokens;
use crate::tools::{ToolDef, ToolKind};
use crate::{DocTab, Snaps};

/// Screen points per PDF point at 100 % (96 dpi screens, 72 points per inch).
pub const PT: f32 = 96.0 / 72.0;
const GAP: f32 = 18.0;
const MARGIN: f32 = 24.0;
/// Pages larger than this (device pixels) are drawn from tiles over a low-res backdrop.
const TILE_THRESHOLD: f32 = 4096.0;
const TILE: u32 = 1024;
const BACKDROP_SIDE: f32 = 2048.0;
/// Thumbnails: longer side in device pixels; tags carry this bit.
pub const THUMB_SIDE: f32 = 220.0;
const THUMB_TAG: u64 = 1 << 62;
/// Renders wait until the zoom has been still this long (seconds).
const ZOOM_SETTLE: f64 = 0.18;
pub const MIN_ZOOM: f32 = 0.02;
pub const MAX_ZOOM: f32 = 64.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageMode {
    Single,
    Continuous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// The zoom is whatever the user set.
    None,
    Page,
    Width,
}

/// Per-document view state.
pub struct DocView {
    pub zoom: f32,
    pub fit: Fit,
    pub mode: PageMode,
    /// 0-based page shown (single mode) or mostly visible (continuous).
    pub current: usize,
    /// Content-space position of the viewport's top-left corner.
    pub offset: Vec2,
    viewport: Rect,
    /// Whole-page rasters: page -> (tag, texture).
    pages: HashMap<usize, (u64, TextureHandle)>,
    /// Tiles: (page, tile x, tile y) -> (tag, texture).
    tiles: HashMap<(usize, u32, u32), (u64, TextureHandle)>,
    pub thumbs: HashMap<usize, TextureHandle>,
    pub errors: HashMap<usize, String>,
    last_queue: Vec<RenderRequest>,
    zoom_changed_at: f64,
    /// A page to scroll to once the viewport size is known.
    scroll_to: Option<usize>,
    /// A Select-tool gesture in progress (move, reshape, caption, box select).
    pub gesture: Option<Gesture>,
    /// A markup being drawn.
    pub draft: Option<Draft>,
    /// Markups shown with live geometry while a gesture lasts (committed on release).
    pub preview: HashMap<String, Markup>,
    /// The text editor over the page.
    pub editor: Option<TextEditor>,
    /// The last snap, for its indicator: page and point.
    pub snapped: Option<(usize, Snapped)>,
    /// What the context menu was opened on.
    pub context: Option<interact::ContextTarget>,
    /// Page and user-space position under the pointer.
    pub pointer: Option<(usize, Point)>,
    /// Visible renders still missing (for the headless screenshot and tests).
    pub missing: usize,
}

impl Default for DocView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            fit: Fit::Page,
            mode: PageMode::Continuous,
            current: 0,
            offset: Vec2::ZERO,
            viewport: Rect::NOTHING,
            pages: HashMap::new(),
            tiles: HashMap::new(),
            thumbs: HashMap::new(),
            errors: HashMap::new(),
            last_queue: Vec::new(),
            zoom_changed_at: f64::NEG_INFINITY,
            scroll_to: Some(0),
            gesture: None,
            draft: None,
            preview: HashMap::new(),
            editor: None,
            snapped: None,
            context: None,
            pointer: None,
            missing: 1,
        }
    }
}

/// The request tag for a raster at `scale`: equal tags mean the same scale (to 1/65536).
fn scale_tag(scale: f32) -> u64 {
    (f64::from(scale.max(0.0)) * 65536.0).round() as u64 & !THUMB_TAG
}

impl DocView {
    /// Drop every raster (after the document's bytes changed, e.g. a save).
    pub fn invalidate(&mut self) {
        self.pages.clear();
        self.tiles.clear();
        self.thumbs.clear();
        self.errors.clear();
        self.last_queue.clear();
    }

    pub fn page_size(&self, g: &PageGeom) -> Vec2 {
        vec2(g.width, g.height) * self.zoom * PT
    }

    /// Page rects in content space (`None` for pages not shown in single-page mode).
    fn layout(&self, pages: &[PageGeom]) -> (Vec<Option<Rect>>, Vec2) {
        let vw = self.viewport.width().max(100.0);
        let max_w = pages.iter().map(|g| self.page_size(g).x).fold(0.0, f32::max);
        let content_w = vw.max(max_w + 2.0 * MARGIN);
        let mut rects = Vec::with_capacity(pages.len());
        let mut y = MARGIN;
        for (i, g) in pages.iter().enumerate() {
            if self.mode == PageMode::Single && i != self.current {
                rects.push(None);
                continue;
            }
            let s = self.page_size(g);
            rects.push(Some(Rect::from_min_size(pos2((content_w - s.x) / 2.0, y), s)));
            y += s.y + GAP;
        }
        (rects, vec2(content_w, y - GAP + MARGIN))
    }

    fn clamp_offset(&mut self, content: Vec2) {
        let vs = self.viewport.size();
        let fix = |off: f32, c: f32, v: f32| {
            if c <= v {
                -(v - c) / 2.0
            } else if off.is_finite() {
                off.clamp(0.0, c - v)
            } else {
                0.0
            }
        };
        self.offset = vec2(fix(self.offset.x, content.x, vs.x), fix(self.offset.y, content.y, vs.y));
    }

    fn fit_zoom(&mut self, pages: &[PageGeom]) {
        let (vw, vh) = (self.viewport.width().max(100.0), self.viewport.height().max(100.0));
        let page = pages.get(self.current).or(pages.first());
        let Some(g) = page else { return };
        let z = match self.fit {
            Fit::None => return,
            Fit::Width => {
                let w = if self.mode == PageMode::Single {
                    g.width
                } else {
                    pages.iter().map(|p| p.width).fold(1.0, f32::max)
                };
                (vw - 2.0 * MARGIN) / (w * PT)
            }
            Fit::Page => ((vw - 2.0 * MARGIN) / (g.width * PT)).min((vh - 2.0 * MARGIN) / (g.height * PT)),
        };
        if z.is_finite() {
            self.zoom = z.clamp(MIN_ZOOM, MAX_ZOOM);
        }
    }

    pub fn set_fit(&mut self, fit: Fit) {
        self.fit = fit;
        self.scroll_to = Some(self.current);
    }

    pub fn set_mode(&mut self, mode: PageMode) {
        self.mode = mode;
        self.scroll_to = Some(self.current);
    }

    /// Zoom to `zoom`, keeping the page point under `anchor` (screen) in place.
    pub fn zoom_to(&mut self, zoom: f32, anchor: Option<Pos2>, pages: &[PageGeom], now: f64) {
        let zoom = if zoom.is_finite() {
            zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            self.zoom
        };
        if (zoom - self.zoom).abs() < 1e-6 {
            return;
        }
        if !self.viewport.is_positive() || !self.viewport.is_finite() {
            // Not laid out yet: no point to keep still; show the current page.
            self.zoom = zoom;
            self.fit = Fit::None;
            self.scroll_to = Some(self.current);
            return;
        }
        let anchor = anchor
            .filter(|a| self.viewport.contains(*a))
            .unwrap_or(self.viewport.center());
        let (rects, _) = self.layout(pages);
        let at = anchor - self.viewport.min + self.offset;
        // The page under the anchor (or the nearest one) and the fraction across it.
        let hit = rects
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.map(|r| (i, r)))
            .min_by(|(_, a), (_, b)| {
                a.distance_to_pos(at.to_pos2())
                    .total_cmp(&b.distance_to_pos(at.to_pos2()))
            });
        self.zoom = zoom;
        self.fit = Fit::None;
        self.zoom_changed_at = now;
        if let Some((i, r)) = hit {
            let f = (at.to_pos2() - r.min) / r.size();
            let (rects, content) = self.layout(pages);
            if let Some(Some(nr)) = rects.get(i) {
                let p = nr.min + f * nr.size();
                self.offset = p.to_vec2() - (anchor - self.viewport.min);
                self.clamp_offset(content);
            }
        }
    }

    /// Step the zoom by `factor` about `anchor`.
    pub fn zoom_by(&mut self, factor: f32, anchor: Option<Pos2>, pages: &[PageGeom], now: f64) {
        self.zoom_to(self.zoom * factor, anchor, pages, now);
    }

    pub fn go_to_page(&mut self, page: usize, count: usize) {
        if count == 0 {
            return;
        }
        self.current = page.min(count - 1);
        self.scroll_to = Some(self.current);
    }

    /// Bring a user-space point of `page` to the middle of the view.
    pub fn center_on(&mut self, page: usize, p: Point, pages: &[PageGeom]) {
        if self.mode == PageMode::Single {
            self.current = page;
        }
        let (rects, content) = self.layout(pages);
        let (Some(Some(r)), Some(g)) = (rects.get(page), pages.get(page)) else {
            return;
        };
        let v = g.user_to_view(p.x as f32, p.y as f32);
        let at = r.min + vec2(v[0], v[1]) * self.zoom * PT;
        self.offset = at.to_vec2() - self.viewport.size() / 2.0;
        self.clamp_offset(content);
        self.current = page;
        self.scroll_to = None;
    }

    /// Where a user-space point of `page` was on screen in the last frame.
    pub fn user_to_screen(&self, page: usize, p: Point, pages: &[PageGeom]) -> Option<Pos2> {
        let (rects, _) = self.layout(pages);
        let r = (*rects.get(page)?)?;
        let g = pages.get(page)?;
        let v = g.user_to_view(p.x as f32, p.y as f32);
        Some(self.viewport.min - self.offset + r.min.to_vec2() + vec2(v[0], v[1]) * self.zoom * PT)
    }

    /// Pull finished renders into textures.
    pub fn receive(&mut self, ctx: &egui::Context, render: &RenderDoc) {
        let budget = if render.is_inline() { 1 } else { 64 };
        let mut got = false;
        for _ in 0..budget {
            let Some(r) = render.poll() else { break };
            got = true;
            let page = r.request.page;
            if let Some(e) = r.error {
                log::warn!("page {}: {e}", page + 1);
                self.errors.insert(page, e);
                continue;
            }
            let (w, h) = (r.width as usize, r.height as usize);
            if w == 0 || h == 0 || w.checked_mul(h).and_then(|n| n.checked_mul(4)) != Some(r.rgba.len()) {
                continue;
            }
            let img = egui::ColorImage::from_rgba_premultiplied([w, h], &r.rgba);
            if let Some(t) = r.request.tile {
                let tex = ctx.load_texture(format!("tile-{page}-{}-{}", t.x, t.y), img, TextureOptions::LINEAR);
                self.tiles.insert((page, t.x / TILE, t.y / TILE), (r.request.tag, tex));
            } else if r.request.tag & THUMB_TAG != 0 {
                let tex = ctx.load_texture(format!("thumb-{page}"), img, TextureOptions::LINEAR);
                self.thumbs.insert(page, tex);
            } else {
                let tex = ctx.load_texture(format!("page-{page}"), img, TextureOptions::LINEAR);
                self.pages.insert(page, (r.request.tag, tex));
            }
        }
        if got {
            ctx.request_repaint();
        }
    }
}

/// Inputs the canvas needs from the app for one frame.
pub struct CanvasCx<'a> {
    pub tool: &'static ToolDef,
    pub wheel_zooms: bool,
    pub hide_markups: bool,
    pub want_thumbs: bool,
    pub snaps: Snaps,
    pub show_grid: bool,
    /// The look new markups take (a Tool Chest item, or the tool's Set as Default).
    pub template: Option<&'a Markup>,
    /// The active Tool Chest item places copies of its markup (Drawing mode).
    pub drawing_mode: bool,
    /// Stamp design for the Stamp tool.
    pub stamp: &'a str,
    pub author: &'a str,
}

/// Something the canvas asks the app to do (it cannot reach app state itself).
#[derive(Debug, Clone)]
pub enum CanvasAction {
    /// Save this markup's look as a tool in My Tools.
    AddToToolChest(Markup),
    /// Set as Default for its tool.
    SetDefault(Markup),
    /// Show a panel.
    ShowPanel(&'static str),
}

/// What happened on the canvas this frame.
#[derive(Default)]
pub struct CanvasOut {
    pub status: Option<String>,
    /// A markup was just created by this tool (Recent Tools; back to Select unless the tool
    /// keeps going).
    pub created: Option<(&'static str, Markup)>,
    /// The tool is finished: back to Select.
    pub done: bool,
    /// Calibrate: page and the two points measured.
    pub calibrate: Option<(usize, Point, Point)>,
    /// Commands to run (context menu items).
    pub commands: Vec<String>,
    pub actions: Vec<CanvasAction>,
}

fn uv(x: f32, y: f32, x1: f32, y1: f32) -> Rect {
    Rect::from_min_max(pos2(x, y), pos2(x1, y1))
}

/// Draw `texture` (a raster of the page, or part of it in normalized page coordinates `src`).
fn paint_raster(p: &egui::Painter, page: Rect, tex: &TextureHandle, src: Rect) {
    let dst = Rect::from_min_max(
        page.min + src.min.to_vec2() * page.size(),
        page.min + src.max.to_vec2() * page.size(),
    );
    p.image(tex.id(), dst, uv(0.0, 0.0, 1.0, 1.0), Color32::WHITE);
}

fn snap(r: Rect, ppp: f32) -> Rect {
    let s = |v: f32| (v * ppp).round() / ppp;
    Rect::from_min_size(pos2(s(r.min.x), s(r.min.y)), r.size())
}

/// A snapshot not saved yet shows its source region from the source page's raster (saving
/// captures the page content itself).
fn snapshot_preview(p: &egui::Painter, view: &DocView, pages: &[PageGeom], xf: &Xf, m: &Markup) {
    let Some(src) = m.snapshot.as_ref().filter(|_| !m.in_file()) else {
        return;
    };
    let (Some((_, tex)), Some(g)) = (
        src.page.and_then(|sp| view.pages.get(&sp)),
        src.page.and_then(|sp| pages.get(sp)),
    ) else {
        return;
    };
    let r = src.region.normalized();
    let a = g.user_to_view(r.x0 as f32, r.y1 as f32);
    let b = g.user_to_view(r.x1 as f32, r.y0 as f32);
    let (w, h) = (g.width.max(1.0), g.height.max(1.0));
    let uv = Rect::from_two_pos(pos2(a[0] / w, a[1] / h), pos2(b[0] / w, b[1] / h));
    let dst = xf.rect_of(crate::actions::box_of(m));
    p.image(tex.id(), dst, uv, Color32::WHITE);
}

/// Show the canvas for one document.
pub fn show(ui: &mut egui::Ui, doc: &mut DocTab, cx: &CanvasCx<'_>) -> CanvasOut {
    let mut out = CanvasOut::default();
    let t = Tokens::get(ui.ctx());
    let rect = ui.available_rect_before_wrap();
    let resp = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, t.workspace);
    let Some(render) = doc.render.as_ref() else {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "This document could not be rendered.",
            FontId::proportional(14.0),
            Color32::WHITE,
        );
        return out;
    };
    let pages = render.pages();
    if pages.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "This document has no pages.",
            FontId::proportional(14.0),
            Color32::WHITE,
        );
        return out;
    }
    let view = &mut doc.view;
    let ppp = ui.ctx().pixels_per_point();
    let now = ui.input(|i| i.time);
    view.viewport = rect;
    view.current = view.current.min(pages.len() - 1);
    view.fit_zoom(pages);

    // ---- navigation input: wheel, pinch, pan --------------------------------------------
    let hovered = resp.hovered() || resp.dragged();
    let space = ui.input(|i| i.key_down(egui::Key::Space)) && !ui.ctx().egui_wants_keyboard_input();
    if hovered {
        let (zoom_delta, scroll, pointer) =
            ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
        if (zoom_delta - 1.0).abs() > 1e-4 {
            view.zoom_by(zoom_delta, pointer, pages, now);
        } else if scroll != Vec2::ZERO {
            if cx.wheel_zooms && scroll.x.abs() < 0.5 {
                view.zoom_by((scroll.y / 240.0).exp(), pointer, pages, now);
            } else {
                view.offset -= scroll;
                view.fit = if view.fit == Fit::Page && view.mode == PageMode::Continuous {
                    Fit::None
                } else {
                    view.fit
                };
            }
        }
    }
    let panning = resp.dragged_by(egui::PointerButton::Middle)
        || (resp.dragged_by(egui::PointerButton::Primary) && (space || matches!(cx.tool.kind, ToolKind::Pan)));
    if panning {
        view.offset -= resp.drag_delta();
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if (space || matches!(cx.tool.kind, ToolKind::Pan)) && hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    if resp.double_clicked_by(egui::PointerButton::Middle) {
        view.set_fit(Fit::Page);
        view.fit_zoom(pages);
    }

    // ---- layout ------------------------------------------------------------------------
    let (rects, content) = view.layout(pages);
    if let Some(p) = view.scroll_to.take()
        && let Some(Some(r)) = rects.get(p)
    {
        view.offset = vec2(view.offset.x, r.top() - MARGIN);
    }
    view.clamp_offset(content);
    let origin = rect.min - view.offset;
    let visible = rect;
    let mut xfs: Vec<(usize, Xf)> = Vec::new();
    let mut best = (-1.0f32, view.current);
    for (i, r) in rects.iter().enumerate() {
        let (Some(r), Some(g)) = (r, pages.get(i)) else {
            continue;
        };
        let sr = snap(r.translate(origin.to_vec2()), ppp);
        if !sr.intersects(visible.expand(200.0)) {
            continue;
        }
        let overlap = sr.intersect(visible).height().max(0.0);
        if overlap > best.0 + 0.5 {
            best = (overlap, i);
        }
        xfs.push((
            i,
            Xf {
                rect: sr,
                geom: g.clone(),
                k: view.zoom * PT,
            },
        ));
    }
    if view.mode == PageMode::Continuous && best.0 >= 0.0 {
        view.current = best.1;
    }

    // ---- pages, rasters and render requests --------------------------------------------
    let settling = now - view.zoom_changed_at < ZOOM_SETTLE;
    if settling {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(ZOOM_SETTLE));
    }
    let scale = view.zoom * PT * ppp;
    let tag = scale_tag(scale);
    let mut wanted: Vec<RenderRequest> = Vec::new();
    let mut missing = 0;
    for (i, xf) in &xfs {
        let (i, sr) = (*i, xf.rect);
        let on_screen = sr.intersects(visible);
        painter.add(
            egui::epaint::Shadow {
                offset: [0, 2],
                blur: 10,
                spread: 0,
                color: t.page_shadow,
            }
            .as_shape(sr, CornerRadius::ZERO),
        );
        painter.rect_filled(sr, CornerRadius::ZERO, Color32::WHITE);
        if let Some(e) = view.errors.get(&i) {
            painter.text(
                sr.center(),
                Align2::CENTER_CENTER,
                format!("This page could not be displayed.\n{e}"),
                FontId::proportional(12.0),
                Color32::from_rgb(0x90, 0x20, 0x20),
            );
            continue;
        }
        let g = &xf.geom;
        let longest = g.width.max(g.height) * scale;
        let tiled = longest > TILE_THRESHOLD;
        let (want_scale, want_tag) = if tiled {
            let s = g.scale_for_side(BACKDROP_SIDE);
            (s, scale_tag(s))
        } else {
            (scale, tag)
        };
        match view.pages.get(&i) {
            Some((ptag, tex)) => {
                paint_raster(&painter, sr, tex, uv(0.0, 0.0, 1.0, 1.0));
                if *ptag != want_tag && !(settling && !tiled) {
                    wanted.push(markupcraft_render::page_request(i, want_scale, want_tag));
                    if on_screen && !tiled {
                        missing += 1;
                    }
                }
            }
            None => {
                wanted.push(markupcraft_render::page_request(i, want_scale, want_tag));
                if on_screen {
                    missing += 1;
                    painter.text(
                        sr.center(),
                        Align2::CENTER_CENTER,
                        "Rendering...",
                        FontId::proportional(13.0),
                        t.text_faint,
                    );
                }
            }
        }
        if tiled && on_screen && !settling {
            let (dw, dh) = (device_pixels(g.width, scale), device_pixels(g.height, scale));
            let vis = sr.intersect(visible);
            let f0 = (vis.min - sr.min) / sr.size();
            let f1 = (vis.max - sr.min) / sr.size();
            let region = [
                (f0.x.max(0.0) * dw as f32) as u32,
                (f0.y.max(0.0) * dh as f32) as u32,
                (f1.x.min(1.0) * dw as f32).ceil() as u32,
                (f1.y.min(1.0) * dh as f32).ceil() as u32,
            ];
            for tl in markupcraft_render::tiles_covering(dw, dh, TILE, region) {
                let src = uv(
                    tl.x as f32 / dw as f32,
                    tl.y as f32 / dh as f32,
                    (tl.x + tl.w) as f32 / dw as f32,
                    (tl.y + tl.h) as f32 / dh as f32,
                );
                match view.tiles.get(&(i, tl.x / TILE, tl.y / TILE)) {
                    Some((ttag, tex)) if *ttag == tag => paint_raster(&painter, sr, tex, src),
                    _ => {
                        missing += 1;
                        wanted.push(markupcraft_render::tile_request(i, scale, tl, tag));
                    }
                }
            }
        }
        painter.rect_stroke(
            sr,
            CornerRadius::ZERO,
            Stroke::new(0.5, Color32::from_black_alpha(90)),
            egui::StrokeKind::Outside,
        );
        if cx.show_grid {
            crate::snapping::paint_grid(&painter, sr, xf.k);
        }
    }
    // Most urgent first: the current page, then the rest on screen, then a page either side.
    wanted.sort_by_key(|r| (r.page != view.current, r.tile.is_some()));
    for n in [view.current + 1, view.current.saturating_sub(1)] {
        if n < pages.len() && !view.pages.contains_key(&n) && !xfs.iter().any(|(i, _)| *i == n) {
            let tiled = pages
                .get(n)
                .is_some_and(|g| g.width.max(g.height) * scale > TILE_THRESHOLD);
            let s = if tiled {
                pages.get(n).map_or(scale, |g| g.scale_for_side(BACKDROP_SIDE))
            } else {
                scale
            };
            wanted.push(markupcraft_render::page_request(n, s, scale_tag(s)));
        }
    }
    if cx.want_thumbs {
        for (i, g) in pages.iter().enumerate().take(2000) {
            if !view.thumbs.contains_key(&i) && !view.errors.contains_key(&i) {
                let s = g.scale_for_side(THUMB_SIDE * ppp.max(1.0));
                wanted.push(markupcraft_render::page_request(i, s, THUMB_TAG | scale_tag(s)));
                missing += 1;
            }
        }
    }
    if wanted != view.last_queue {
        render.request(wanted.clone());
        view.last_queue = wanted;
    }
    view.missing = missing;
    // Keep textures of pages near the view only; old-scale tiles go once replaced.
    let near: Vec<usize> = xfs.iter().map(|(i, _)| *i).collect();
    let keep = |p: &usize| near.iter().any(|n| n.abs_diff(*p) <= 2);
    view.pages.retain(|p, _| keep(p));
    view.tiles
        .retain(|(p, _, _), (tt, _)| keep(p) && (*tt == tag || settling));

    // ---- markups -----------------------------------------------------------------------
    let editing = view.editor.as_ref().and_then(TextEditor::existing_id);
    if !cx.hide_markups {
        for (i, xf) in &xfs {
            for m in doc.session.doc().markups_on(*i) {
                let m = view.preview.get(&m.id).unwrap_or(m);
                if editing.as_deref() == Some(m.id.as_str()) {
                    // The editor shows its text; draw the frame only.
                    let mut frame = m.clone();
                    frame.contents.clear();
                    painter::paint_markup(&painter, xf, &frame);
                } else if crate::actions::drawn_by_us(m) {
                    snapshot_preview(&painter, view, pages, xf, m);
                    painter::paint_markup(&painter, xf, m);
                }
            }
        }
    }
    let selection = doc.session.selection().to_vec();
    for (i, xf) in &xfs {
        for m in doc.session.doc().markups_on(*i).filter(|m| selection.contains(&m.id)) {
            let m = view.preview.get(&m.id).unwrap_or(m);
            painter::paint_selection(&painter, xf, m, &t, editable(m));
        }
    }

    // ---- pointer tools -----------------------------------------------------------------
    let pointer = resp.hover_pos().or_else(|| resp.interact_pointer_pos());
    let page_at = |s: Pos2| {
        xfs.iter()
            .find(|(_, xf)| xf.rect.expand(4.0).contains(s))
            .map(|(i, xf)| (*i, xf))
    };
    doc.view.pointer = pointer.and_then(|s| page_at(s).map(|(i, xf)| (i, xf.to_user(s))));
    let mut ix = interact::Input {
        ui,
        resp: &resp,
        painter: &painter,
        xfs: &xfs,
        tokens: &t,
        panning: panning || space,
    };
    interact::run(&mut ix, doc, cx, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages() -> Vec<PageGeom> {
        (0..3)
            .map(|i| PageGeom {
                width: 612.0,
                height: 792.0,
                crop: [0.0, 0.0, 612.0, 792.0],
                rotation: 0,
                label: format!("{}", i + 1),
            })
            .collect()
    }

    fn view() -> DocView {
        DocView {
            viewport: Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)),
            ..Default::default()
        }
    }

    #[test]
    fn fit_page_shows_the_whole_page() {
        let mut v = view();
        v.fit_zoom(&pages());
        let s = v.page_size(&pages()[0]);
        assert!(s.y <= 600.0 - 2.0 * MARGIN + 0.01 && s.x <= 800.0);
        v.set_fit(Fit::Width);
        v.fit_zoom(&pages());
        assert!((v.page_size(&pages()[0]).x - (800.0 - 2.0 * MARGIN)).abs() < 0.01);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_pointer() {
        let p = pages();
        let mut v = view();
        v.fit = Fit::None;
        v.zoom = 1.0;
        let (rects, content) = v.layout(&p);
        v.clamp_offset(content);
        let anchor = pos2(500.0, 300.0);
        let r0 = rects[0].unwrap();
        let f0 = (anchor - v.viewport.min + v.offset - r0.min.to_vec2()) / r0.size();
        v.zoom_to(2.0, Some(anchor), &p, 0.0);
        let (rects, _) = v.layout(&p);
        let r1 = rects[0].unwrap();
        let f1 = (anchor - v.viewport.min + v.offset - r1.min.to_vec2()) / r1.size();
        assert!((f0 - f1).length() < 1e-3, "{f0:?} vs {f1:?}");
        assert_eq!(v.fit, Fit::None);
    }

    #[test]
    fn single_page_mode_lays_out_one_page() {
        let mut v = view();
        v.mode = PageMode::Single;
        v.current = 1;
        let (rects, _) = v.layout(&pages());
        assert_eq!(rects.iter().filter(|r| r.is_some()).count(), 1);
        assert!(rects[1].is_some());
    }
}
