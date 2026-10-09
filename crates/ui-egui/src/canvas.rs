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
    /// Two pages at a time, side by side (Ctrl+6).
    SideBySide,
    /// Every page, two to a row (Ctrl+7).
    ContinuousSideBySide,
}

impl PageMode {
    /// Pages sit two to a row.
    pub fn two_up(self) -> bool {
        matches!(self, PageMode::SideBySide | PageMode::ContinuousSideBySide)
    }

    /// Every page is laid out (else only the current page or spread).
    pub fn continuous(self) -> bool {
        matches!(self, PageMode::Continuous | PageMode::ContinuousSideBySide)
    }
}

/// How the view behaves, from Preferences (set by the shell every frame).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewOpts {
    /// The wheel zooms (else scrolls) in Single Page / Side by Side.
    pub wheel_zooms_single: bool,
    /// The wheel zooms (else scrolls) in the continuous modes.
    pub wheel_zooms_continuous: bool,
    /// Wheel up zooms out.
    pub reverse_wheel: bool,
    /// Zoom per wheel notch (1 = Revu's default step).
    pub wheel_sensitivity: f32,
    /// Largest zoom (1 = 100 %).
    pub max_zoom: f32,
    /// In Fit Width, panning moves up and down only.
    pub lock_fit_width: bool,
    /// Fade the page content by this fraction (0 = off), so markups stand out.
    pub dim: f32,
    /// Dark Mode: pages drawn light on dark.
    pub dark: bool,
}

impl Default for ViewOpts {
    fn default() -> Self {
        Self {
            wheel_zooms_single: true,
            wheel_zooms_continuous: true,
            reverse_wheel: false,
            wheel_sensitivity: 1.0,
            max_zoom: MAX_ZOOM,
            lock_fit_width: false,
            dim: 0.0,
            dark: false,
        }
    }
}

/// Map a point of the unrotated page (normalized `u, v`) to the displayed page rotated `rot`
/// degrees clockwise.
pub fn rot_uv(rot: u16, u: f32, v: f32) -> (f32, f32) {
    match rot % 360 {
        90 => (1.0 - v, u),
        180 => (1.0 - u, 1.0 - v),
        270 => (v, 1.0 - u),
        _ => (u, v),
    }
}

/// The inverse of [`rot_uv`].
pub fn unrot_uv(rot: u16, x: f32, y: f32) -> (f32, f32) {
    match rot % 360 {
        90 => (y, 1.0 - x),
        180 => (1.0 - x, 1.0 - y),
        270 => (1.0 - y, x),
        _ => (x, y),
    }
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
    /// View rotation, clockwise degrees (View > Rotate View; the file is unchanged).
    pub rotation: u16,
    /// Side by Side modes: the first page sits alone (a cover).
    pub cover: bool,
    pub opts: ViewOpts,
    /// Zoom tool: where a zoom-box drag started (screen).
    pub zoom_box: Option<Pos2>,
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
            rotation: 0,
            cover: false,
            opts: ViewOpts::default(),
            zoom_box: None,
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

    /// The page's displayed size in points with the view rotation applied.
    fn dims(&self, g: &PageGeom) -> (f32, f32) {
        if self.rotation % 180 == 90 {
            (g.height, g.width)
        } else {
            (g.width, g.height)
        }
    }

    pub fn page_size(&self, g: &PageGeom) -> Vec2 {
        let (w, h) = self.dims(g);
        vec2(w, h) * self.zoom * PT
    }

    /// The page geometry as displayed: the view rotation added to the page's own.
    pub fn view_geom(&self, g: &PageGeom) -> PageGeom {
        let mut v = g.clone();
        if !self.rotation.is_multiple_of(360) {
            v.rotation = (g.rotation + self.rotation) % 360;
            (v.width, v.height) = self.dims(g);
        }
        v
    }

    /// Rows of pages as laid out: (first page, how many), one or two to a row.
    pub fn rows(&self, n: usize) -> Vec<(usize, usize)> {
        if !self.mode.two_up() {
            return (0..n).map(|i| (i, 1)).collect();
        }
        let mut out = Vec::with_capacity(n / 2 + 1);
        let mut i = 0;
        if self.cover && n > 0 {
            out.push((0, 1));
            i = 1;
        }
        while i < n {
            let len = if i + 1 < n { 2 } else { 1 };
            out.push((i, len));
            i += len;
        }
        out
    }

    /// The row holding the current page.
    fn current_row(&self, n: usize) -> (usize, usize) {
        self.rows(n)
            .into_iter()
            .find(|(f, l)| (*f..f + l).contains(&self.current))
            .unwrap_or((self.current, 1))
    }

    /// Width (points, unzoomed) and height of a row.
    fn row_dims(&self, pages: &[PageGeom], (first, len): (usize, usize)) -> (f32, f32) {
        pages.iter().skip(first).take(len).fold((0.0, 0.0), |(w, h), g| {
            let (gw, gh) = self.dims(g);
            (w + gw, f32::max(h, gh))
        })
    }

    /// Page rects in content space (`None` for pages not shown in the paged modes).
    fn layout(&self, pages: &[PageGeom]) -> (Vec<Option<Rect>>, Vec2) {
        let vw = self.viewport.width().max(100.0);
        let current = self.current_row(pages.len());
        let rows: Vec<(usize, usize)> = self
            .rows(pages.len())
            .into_iter()
            .filter(|r| self.mode.continuous() || *r == current)
            .collect();
        let row_w = |r: (usize, usize)| {
            let (w, _) = self.row_dims(pages, r);
            w * self.zoom * PT + GAP * r.1.saturating_sub(1) as f32
        };
        let max_w = rows.iter().map(|r| row_w(*r)).fold(0.0, f32::max);
        let content_w = vw.max(max_w + 2.0 * MARGIN);
        let mut rects = vec![None; pages.len()];
        let mut y = MARGIN;
        for r in rows {
            let mut x = (content_w - row_w(r)) / 2.0;
            let mut h = 0.0f32;
            for i in r.0..r.0 + r.1 {
                let (Some(g), Some(slot)) = (pages.get(i), rects.get_mut(i)) else {
                    continue;
                };
                let s = self.page_size(g);
                *slot = Some(Rect::from_min_size(pos2(x, y), s));
                x += s.x + GAP;
                h = h.max(s.y);
            }
            y += h + GAP;
        }
        (rects, vec2(content_w, (y - GAP + MARGIN).max(2.0 * MARGIN)))
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
        if pages.is_empty() {
            return;
        }
        let row = self.current_row(pages.len());
        let gaps = |r: (usize, usize)| GAP * r.1.saturating_sub(1) as f32;
        let z = match self.fit {
            Fit::None => return,
            Fit::Width => {
                let (w, r) = if self.mode.continuous() {
                    self.rows(pages.len())
                        .into_iter()
                        .map(|r| (self.row_dims(pages, r).0, r))
                        .fold((1.0, row), |a, b| if b.0 > a.0 { b } else { a })
                } else {
                    (self.row_dims(pages, row).0, row)
                };
                (vw - 2.0 * MARGIN - gaps(r)) / (w.max(1.0) * PT)
            }
            Fit::Page => {
                let (w, h) = self.row_dims(pages, row);
                ((vw - 2.0 * MARGIN - gaps(row)) / (w.max(1.0) * PT)).min((vh - 2.0 * MARGIN) / (h.max(1.0) * PT))
            }
        };
        if z.is_finite() {
            self.zoom = z.clamp(MIN_ZOOM, self.max_zoom());
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
            zoom.clamp(MIN_ZOOM, self.max_zoom())
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
        if !self.mode.continuous() {
            self.current = page;
        }
        let (rects, content) = self.layout(pages);
        let (Some(Some(r)), Some(g)) = (rects.get(page), pages.get(page)) else {
            return;
        };
        let v = self.view_geom(g).user_to_view(p.x as f32, p.y as f32);
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
        let v = self.view_geom(g).user_to_view(p.x as f32, p.y as f32);
        Some(self.viewport.min - self.offset + r.min.to_vec2() + vec2(v[0], v[1]) * self.zoom * PT)
    }

    /// Show exactly this place (view history, synchronised splits): no pending scroll.
    pub fn place(&mut self, current: usize, zoom: f32, fit: Fit, offset: Vec2) {
        self.current = current;
        if zoom.is_finite() {
            self.zoom = zoom.clamp(MIN_ZOOM, self.max_zoom());
        }
        self.fit = fit;
        self.offset = offset;
        self.scroll_to = None;
    }

    /// The largest zoom allowed (Preferences > Document: maximum zoom).
    pub fn max_zoom(&self) -> f32 {
        if self.opts.max_zoom.is_finite() {
            self.opts.max_zoom.clamp(MIN_ZOOM * 2.0, MAX_ZOOM)
        } else {
            MAX_ZOOM
        }
    }

    /// The canvas area on screen in the last frame.
    pub fn viewport(&self) -> Rect {
        self.viewport
    }

    /// The size of everything the view can scroll over (pages and margins), screen points.
    pub fn content_size(&self, pages: &[PageGeom]) -> Vec2 {
        self.layout(pages).1
    }

    /// Fill the view with a screen rectangle (Zoom tool box).
    pub fn zoom_to_rect(&mut self, r: Rect, pages: &[PageGeom], now: f64) {
        let r = Rect::from_two_pos(r.min, r.max);
        if r.width() < 4.0 || r.height() < 4.0 || !self.viewport.is_positive() {
            return;
        }
        let factor = (self.viewport.width() / r.width()).min(self.viewport.height() / r.height());
        self.zoom_to(self.zoom * factor, Some(r.center()), pages, now);
        // The box centre stayed put; bring it to the middle.
        self.offset += r.center() - self.viewport.center();
        let (_, content) = self.layout(pages);
        self.clamp_offset(content);
    }

    /// Lay out now (after a change made outside the frame, e.g. from a test or a sync).
    pub fn relayout(&mut self, pages: &[PageGeom]) {
        self.fit_zoom(pages);
        let (rects, content) = self.layout(pages);
        if let Some(p) = self.scroll_to.take()
            && let Some(Some(r)) = rects.get(p)
        {
            self.offset = vec2(self.offset.x, r.top() - MARGIN);
        }
        self.clamp_offset(content);
    }

    /// The page and point (user space) at the middle of the view.
    pub fn center_point(&self, pages: &[PageGeom]) -> Option<(usize, Point)> {
        let (rects, _) = self.layout(pages);
        let at = (self.viewport.size() / 2.0 + self.offset).to_pos2();
        let (i, r) = rects
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.map(|r| (i, r)))
            .min_by(|(_, a), (_, b)| a.distance_to_pos(at).total_cmp(&b.distance_to_pos(at)))?;
        let g = self.view_geom(pages.get(i)?);
        let v = (at - r.min) / (self.zoom * PT);
        let u = g.view_to_user(v.x, v.y);
        Some((i, Point::new(f64::from(u[0]), f64::from(u[1]))))
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
            // Never hand the GPU a texture larger than it accepts (egui panics on that).
            let max_side = ctx.input(|i| i.max_texture_side);
            if w > max_side || h > max_side {
                log::warn!(
                    "page {}: raster {w}x{h} exceeds the GPU texture limit {max_side}; skipped",
                    page + 1
                );
                continue;
            }
            if w == 0 || h == 0 || w.checked_mul(h).and_then(|n| n.checked_mul(4)) != Some(r.rgba.len()) {
                continue;
            }
            let img = if self.opts.dark {
                egui::ColorImage::from_rgba_premultiplied([w, h], &crate::shell::workspace::darken(&r.rgba))
            } else {
                egui::ColorImage::from_rgba_premultiplied([w, h], &r.rgba)
            };
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
    /// Format Painter, Highlight Viewports, Sketch to Scale.
    pub edit: &'a crate::editing::EditState,
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
    /// A viewport box was drawn on this page: ask for its name and scale.
    NewViewport(usize, markupcraft_geom::Rect),
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
/// `rot` is the view rotation: the raster is of the unrotated page.
fn paint_raster(p: &egui::Painter, page: Rect, tex: &TextureHandle, src: Rect, rot: u16) {
    if rot.is_multiple_of(360) {
        let dst = Rect::from_min_max(
            page.min + src.min.to_vec2() * page.size(),
            page.min + src.max.to_vec2() * page.size(),
        );
        p.image(tex.id(), dst, uv(0.0, 0.0, 1.0, 1.0), Color32::WHITE);
        return;
    }
    let mut mesh = egui::Mesh::with_texture(tex.id());
    let corners = [
        (src.min.x, src.min.y, 0.0, 0.0),
        (src.max.x, src.min.y, 1.0, 0.0),
        (src.max.x, src.max.y, 1.0, 1.0),
        (src.min.x, src.max.y, 0.0, 1.0),
    ];
    for (u, v, tu, tv) in corners {
        let (x, y) = rot_uv(rot, u, v);
        mesh.vertices.push(egui::epaint::Vertex {
            pos: page.min + vec2(x, y) * page.size(),
            uv: pos2(tu, tv),
            color: Color32::WHITE,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    p.add(egui::Shape::mesh(mesh));
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
/// Pages whose raster would be longer than this are drawn as tiles: the smaller of our own
/// threshold and the GPU's largest texture side (a texture over that limit makes egui panic).
fn tile_threshold(ctx: &egui::Context) -> f32 {
    let gpu = ctx.input(|i| i.max_texture_side) as f32;
    if gpu >= 256.0 {
        TILE_THRESHOLD.min(gpu)
    } else {
        TILE_THRESHOLD
    }
}

pub fn show(ui: &mut egui::Ui, doc: &mut DocTab, cx: &CanvasCx<'_>) -> CanvasOut {
    let tile_limit = tile_threshold(ui.ctx());
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
        // The wheel zooms or scrolls by the mode's preference; Ctrl swaps the two (Revu).
        // Read raw wheel events: egui turns Ctrl+wheel into a zoom factor of its own.
        let (wheel, ctrl, shift, pinch, pointer) = ui.input(|i| {
            let mut d = Vec2::ZERO;
            for e in &i.events {
                if let egui::Event::MouseWheel { unit, delta, .. } = e {
                    d += match unit {
                        egui::MouseWheelUnit::Point => *delta,
                        egui::MouseWheelUnit::Line => *delta * 50.0,
                        egui::MouseWheelUnit::Page => *delta * rect.height(),
                    };
                }
            }
            let pinch = i
                .events
                .iter()
                .filter_map(|e| if let egui::Event::Zoom(z) = e { Some(*z) } else { None })
                .product::<f32>();
            (d, i.modifiers.command, i.modifiers.shift, pinch, i.pointer.hover_pos())
        });
        let mode_zooms = if view.mode.continuous() {
            view.opts.wheel_zooms_continuous
        } else {
            view.opts.wheel_zooms_single
        };
        let zooms = (cx.wheel_zooms && mode_zooms) != ctrl;
        if (pinch - 1.0).abs() > 1e-4 && pinch.is_finite() {
            view.zoom_by(pinch, pointer, pages, now);
        } else if wheel != Vec2::ZERO && wheel.is_finite() {
            if zooms && wheel.x.abs() < 0.5 && !shift {
                let dir = if view.opts.reverse_wheel { -1.0 } else { 1.0 };
                let k = view.opts.wheel_sensitivity.clamp(0.1, 10.0);
                view.zoom_by((dir * k * wheel.y / 240.0).exp(), pointer, pages, now);
            } else {
                let mut d = if shift && wheel.x.abs() < 0.5 {
                    vec2(wheel.y, 0.0)
                } else {
                    wheel
                };
                if view.fit == Fit::Width && view.opts.lock_fit_width {
                    d.x = 0.0;
                }
                view.offset -= d;
                if view.fit == Fit::Page && view.mode.continuous() {
                    view.fit = Fit::None;
                }
            }
        }
    }
    let mut zoom_band: Option<Rect> = None;
    let zoom_tool = cx.tool.id == "zoom";
    let pan_tool = matches!(cx.tool.kind, ToolKind::Pan) && !zoom_tool;
    let panning = resp.dragged_by(egui::PointerButton::Middle)
        || (resp.dragged_by(egui::PointerButton::Primary) && (space || pan_tool));
    if panning {
        let mut d = resp.drag_delta();
        if view.fit == Fit::Width && view.opts.lock_fit_width {
            d.x = 0.0;
        }
        view.offset -= d;
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if (space || pan_tool) && hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    if resp.double_clicked_by(egui::PointerButton::Middle)
        && let Some(at) = resp.interact_pointer_pos()
    {
        // Re-centre on the double-clicked point.
        view.offset += at - rect.center();
    }
    if zoom_tool && !space {
        // Zoom: click zooms in, Ctrl+click or right-click zooms out, a drag fills the view with
        // the box.
        let at = resp.interact_pointer_pos();
        if hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomIn);
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            view.zoom_box = ui.input(|i| i.pointer.press_origin()).or(at);
        }
        if let (Some(a), Some(b)) = (view.zoom_box, ui.input(|i| i.pointer.latest_pos())) {
            let r = Rect::from_two_pos(a, b);
            zoom_band = Some(r);
            if resp.drag_stopped() {
                view.zoom_box = None;
                view.zoom_to_rect(r, pages, now);
            }
        }
        let ctrl = ui.input(|i| i.modifiers.command);
        if resp.clicked_by(egui::PointerButton::Primary) {
            view.zoom_by(if ctrl { 0.8 } else { 1.25 }, at, pages, now);
        } else if resp.secondary_clicked() {
            view.zoom_by(0.8, at, pages, now);
        }
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
                geom: view.view_geom(g),
                k: view.zoom * PT,
            },
        ));
    }
    if view.mode.continuous() && best.0 >= 0.0 {
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
        let paper = if view.opts.dark {
            crate::shell::workspace::DARK_PAPER
        } else {
            Color32::WHITE
        };
        painter.rect_filled(sr, CornerRadius::ZERO, paper);
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
        // The raster is of the unrotated page; the view rotation turns it on screen.
        let Some(g) = pages.get(i) else { continue };
        let rot = view.rotation;
        let longest = g.width.max(g.height) * scale;
        let tiled = longest > tile_limit;
        let (want_scale, want_tag) = if tiled {
            let s = g.scale_for_side(BACKDROP_SIDE);
            (s, scale_tag(s))
        } else {
            (scale, tag)
        };
        match view.pages.get(&i) {
            Some((ptag, tex)) => {
                paint_raster(&painter, sr, tex, uv(0.0, 0.0, 1.0, 1.0), rot);
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
            let a = (vis.min - sr.min) / sr.size();
            let b = (vis.max - sr.min) / sr.size();
            // The visible part in unrotated page fractions.
            let (u0, v0) = unrot_uv(rot, a.x, a.y);
            let (u1, v1) = unrot_uv(rot, b.x, b.y);
            let f0 = vec2(u0.min(u1), v0.min(v1));
            let f1 = vec2(u0.max(u1), v0.max(v1));
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
                    Some((ttag, tex)) if *ttag == tag => paint_raster(&painter, sr, tex, src, rot),
                    _ => {
                        missing += 1;
                        wanted.push(markupcraft_render::tile_request(i, scale, tl, tag));
                    }
                }
            }
        }
        if view.opts.dim > 0.0 {
            // Dimmer: fade the page content toward white; markups are drawn after, unfaded.
            let a = (view.opts.dim.clamp(0.0, 0.95) * 255.0) as u8;
            painter.rect_filled(sr, CornerRadius::ZERO, Color32::from_white_alpha(a));
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
            let tiled = pages.get(n).is_some_and(|g| g.width.max(g.height) * scale > tile_limit);
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
        let hidden = crate::features::canvas::hidden_layers(ui.ctx(), doc.uid);
        for (i, xf) in &xfs {
            for m in doc.session.doc().markups_on(*i).filter(|m| !hidden.contains(&m.layer)) {
                let m = view.preview.get(&m.id).unwrap_or(m);
                if editing.as_deref() == Some(m.id.as_str()) {
                    // The editor shows its text; draw the frame only.
                    let mut frame = m.clone();
                    frame.contents.clear();
                    painter::paint_markup(&painter, xf, &frame);
                } else if crate::actions::drawn_by_us(m)
                    && m.flags & (markupcraft_model::flags::HIDDEN | markupcraft_model::flags::NO_VIEW) == 0
                {
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

    if let Some(r) = zoom_band {
        painter.rect_filled(r, 0.0, t.select.gamma_multiply(0.12));
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, t.select), egui::StrokeKind::Middle);
    }

    // ---- pointer tools -----------------------------------------------------------------
    let pointer = resp.hover_pos().or_else(|| resp.interact_pointer_pos());
    let page_at = |s: Pos2| {
        xfs.iter()
            .find(|(_, xf)| xf.rect.expand(4.0).contains(s))
            .map(|(i, xf)| (*i, xf))
    };
    doc.view.pointer = pointer.and_then(|s| page_at(s).map(|(i, xf)| (i, xf.to_user(s))));
    let picking = crate::features::canvas::layer(ui, &resp, &painter, &xfs, doc.uid);
    let mut ix = interact::Input {
        ui,
        resp: &resp,
        painter: &painter,
        xfs: &xfs,
        tokens: &t,
        panning: panning || space,
    };
    if !picking {
        interact::run(&mut ix, doc, cx, &mut out);
    }
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
