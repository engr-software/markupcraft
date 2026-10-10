//! Page rendering for MarkupCraft, over PdfCraft's `pdfcraft-render` (hayro).
//!
//! - [`RenderDoc::open`] reads page geometry, labels and the outline, and starts a background
//!   [`pdfcraft_render::RenderPool`] that rasterizes whole pages, tiles and thumbnails at any
//!   zoom. Requests carry a caller tag that comes back with the result, so a UI can drop stale
//!   renders after a zoom change.
//! - MarkupCraft draws the markups it models itself (edits show live, without a re-render).
//!   [`RenderOptions::hide`] lists those annotations: they are marked Hidden in a private copy
//!   of the bytes given to the renderer, so they are not drawn twice. Every other annotation
//!   (foreign looks we cannot redraw, links, form fields) still comes from the PDF's `/AP`.
//! - [`text`] is the hook the search feature will use.
//!
//! Nothing here depends on a GUI toolkit.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod snap;
pub mod synthetic;
pub mod text;
pub mod thin;

use std::sync::Arc;

use pdfcraft_cos::{Document as CosDoc, ObjRef, Object, SaveOptions};
pub use pdfcraft_render::{
    MAX_PIXELS, MAX_SIDE, RenderRequest, RenderedPage, RequestKind, Tile, device_pixels, effective_scale,
};
use pdfcraft_render::{RenderConfig, RenderPool};

/// A PDF object id `(number, generation)`, as `markupcraft-model` writes it.
pub type ObjId = (u32, u16);

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("the file is not a readable PDF: {0}")]
    Invalid(String),
    #[error("the document is protected by a password")]
    NeedsPassword,
    #[error("{0}")]
    Other(String),
}

impl From<pdfcraft_render::OpenError> for RenderError {
    fn from(e: pdfcraft_render::OpenError) -> Self {
        match e {
            pdfcraft_render::OpenError::Invalid(s) => RenderError::Invalid(s),
            pdfcraft_render::OpenError::NeedsPassword | pdfcraft_render::OpenError::WrongPassword => {
                RenderError::NeedsPassword
            }
            pdfcraft_render::OpenError::Unsupported(s) => RenderError::Other(s),
        }
    }
}

/// One page as it is displayed: size after `/Rotate`, the crop box, the label.
#[derive(Debug, Clone, PartialEq)]
pub struct PageGeom {
    /// Displayed size in points (after `/Rotate` and `/UserUnit`).
    pub width: f32,
    pub height: f32,
    /// Effective crop box in user space `[x0, y0, x1, y1]`.
    pub crop: [f32; 4],
    /// Clockwise page rotation in degrees (0, 90, 180, 270).
    pub rotation: u16,
    /// `/PageLabels` label, else the 1-based page number.
    pub label: String,
}

impl PageGeom {
    fn of(p: &pdfcraft_render::PageInfo) -> Self {
        Self {
            width: p.width.max(1.0),
            height: p.height.max(1.0),
            crop: p.crop,
            rotation: p.rotation,
            label: p.label.clone(),
        }
    }

    fn info(&self) -> pdfcraft_render::PageInfo {
        pdfcraft_render::PageInfo {
            width: self.width,
            height: self.height,
            label: self.label.clone(),
            crop: self.crop,
            rotation: self.rotation,
        }
    }

    /// PDF user space (y up) to view space (points from the displayed page's top-left, y down).
    pub fn user_to_view(&self, x: f32, y: f32) -> [f32; 2] {
        self.info().user_to_view(x, y)
    }

    /// View space to PDF user space.
    pub fn view_to_user(&self, x: f32, y: f32) -> [f32; 2] {
        self.info().view_to_user(x, y)
    }

    /// The scale (device pixels per point) that makes the longer side `max_side` pixels.
    pub fn scale_for_side(&self, max_side: f32) -> f32 {
        (max_side / self.width.max(self.height)).max(0.001)
    }
}

/// One outline (bookmark) entry, flattened: `depth` 0 is the top level.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineEntry {
    pub title: String,
    pub page: Option<usize>,
    pub depth: usize,
}

const MAX_OUTLINE: usize = 20_000;
const MAX_OUTLINE_DEPTH: usize = 32;

fn flatten(items: &[pdfcraft_render::OutlineItem], depth: usize, out: &mut Vec<OutlineEntry>) {
    if depth > MAX_OUTLINE_DEPTH {
        return;
    }
    for it in items {
        if out.len() >= MAX_OUTLINE {
            return;
        }
        out.push(OutlineEntry {
            title: it.title.clone(),
            page: it.page,
            depth,
        });
        flatten(&it.children, depth + 1, out);
    }
}

/// How a document is rendered.
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// Annotations MarkupCraft draws itself: hidden in the renderer's copy of the file.
    pub hide: Vec<ObjId>,
    /// Worker threads; 0 renders inline on the calling thread inside [`RenderDoc::poll`].
    pub threads: usize,
    /// Hide every markup annotation (View > Hide Markups); links and fields stay.
    pub hide_all_markups: bool,
}

impl RenderOptions {
    /// A sensible default pool size for this machine.
    /// The browser build has no threads: 0, every page renders inline in [`RenderDoc::poll`].
    pub fn default_threads() -> usize {
        if cfg!(target_arch = "wasm32") {
            return 0;
        }
        std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, 6))
    }
}

/// An open document on the render side: geometry plus a background pool.
pub struct RenderDoc {
    pages: Vec<PageGeom>,
    outline: Vec<OutlineEntry>,
    pool: RenderPool,
    hidden: usize,
}

impl RenderDoc {
    /// Read page geometry and start the render pool over `bytes`.
    pub fn open(bytes: Arc<Vec<u8>>, opts: &RenderOptions) -> Result<Self, RenderError> {
        let info = pdfcraft_render::inspect(bytes.clone(), None)?;
        let pages: Vec<PageGeom> = info.pages.iter().map(PageGeom::of).collect();
        let mut outline = Vec::new();
        flatten(&info.outline, 0, &mut outline);
        let (render_bytes, hidden) = if opts.hide.is_empty() {
            (bytes, 0)
        } else {
            match hide_annotations(&bytes, &opts.hide) {
                Ok((b, n)) => (b, n),
                Err(e) => {
                    log::warn!("could not hide modelled annotations, the PDF draws them too: {e}");
                    (bytes, 0)
                }
            }
        };
        let config = RenderConfig {
            hide_comments: opts.hide_all_markups,
            ..Default::default()
        };
        let pool = if opts.threads == 0 {
            RenderPool::new_inline(render_bytes, config)
        } else {
            RenderPool::new(render_bytes, opts.threads, config)
        };
        Ok(Self {
            pages,
            outline,
            pool,
            hidden,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn pages(&self) -> &[PageGeom] {
        &self.pages
    }

    pub fn page(&self, i: usize) -> Option<&PageGeom> {
        self.pages.get(i)
    }

    pub fn outline(&self) -> &[OutlineEntry] {
        &self.outline
    }

    /// How many annotations are hidden from the renderer because MarkupCraft draws them.
    pub fn hidden_count(&self) -> usize {
        self.hidden
    }

    /// Replace the queue of pending renders (most urgent first). A request equal to one
    /// already rendering is not rendered twice.
    pub fn request(&self, reqs: Vec<RenderRequest>) {
        self.pool.set_queue(reqs);
    }

    /// One finished render, if any (inline pools render here, one per call).
    pub fn poll(&self) -> Option<RenderedPage> {
        self.pool.try_recv()
    }

    /// Give up on a render after `limit` (default 20 s): the pool's watchdog then answers it
    /// with an error and replaces the stuck worker (pools with worker threads only).
    pub fn set_stuck_after(&mut self, limit: std::time::Duration) {
        self.pool.set_stuck_after(limit);
    }

    /// `true` when there are no worker threads.
    pub fn is_inline(&self) -> bool {
        self.pool.is_inline()
    }
}

/// A whole page at `scale` device pixels per point.
pub fn page_request(page: usize, scale: f32, tag: u64) -> RenderRequest {
    RenderRequest {
        page,
        kind: RequestKind::Pixels,
        tile: None,
        scale,
        tag,
    }
}

/// One tile of a page at `scale`.
pub fn tile_request(page: usize, scale: f32, tile: Tile, tag: u64) -> RenderRequest {
    RenderRequest {
        page,
        kind: RequestKind::Pixels,
        tile: Some(tile),
        scale,
        tag,
    }
}

/// The tiles (at most `side` pixels square) covering the device-pixel region
/// `[x0, x1) x [y0, y1)` of a page that is `dw` x `dh` pixels.
pub fn tiles_covering(dw: u32, dh: u32, side: u32, region: [u32; 4]) -> Vec<Tile> {
    let side = side.max(16);
    let [x0, y0, x1, y1] = region;
    let (x1, y1) = (x1.min(dw), y1.min(dh));
    let mut out = Vec::new();
    if x0 >= x1 || y0 >= y1 {
        return out;
    }
    for ty in y0 / side..=(y1 - 1) / side {
        for tx in x0 / side..=(x1 - 1) / side {
            let (x, y) = (tx.saturating_mul(side), ty.saturating_mul(side));
            let (w, h) = (side.min(dw.saturating_sub(x)), side.min(dh.saturating_sub(y)));
            if w > 0 && h > 0 {
                out.push(Tile { x, y, w, h });
            }
        }
    }
    out
}

/// A copy of `bytes` in which the annotation objects `objs` carry the Hidden flag (ISO 32000-1
/// §12.5.3, bit 2), written as an incremental update. Returns the bytes and how many objects
/// were hidden.
pub fn hide_annotations(bytes: &Arc<Vec<u8>>, objs: &[ObjId]) -> Result<(Arc<Vec<u8>>, usize), String> {
    let mut cos = CosDoc::open(bytes.clone()).map_err(|e| e.to_string())?;
    let mut n = 0;
    for &(num, generation) in objs {
        if num == 0 {
            continue;
        }
        let r = ObjRef { num, generation };
        if cos.get(r).as_dict().is_none() {
            continue;
        }
        let res = cos.update_dict(r, |d| {
            let old = match d.get(b"F") {
                Some(Object::Int(i)) => *i,
                _ => 0,
            };
            d.set(b"F".to_vec(), Object::Int(old | 2));
        });
        if res.is_ok() {
            n += 1;
        }
    }
    if n == 0 {
        return Ok((bytes.clone(), 0));
    }
    let out = pdfcraft_cos::write_incremental(&cos, &SaveOptions::default()).map_err(|e| e.to_string())?;
    Ok((Arc::new(out), n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(r: &RenderedPage, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * r.width + x) * 4) as usize;
        [r.rgba[i], r.rgba[i + 1], r.rgba[i + 2], r.rgba[i + 3]]
    }

    fn render_once(doc: &RenderDoc, req: RenderRequest) -> RenderedPage {
        doc.request(vec![req]);
        for _ in 0..1000 {
            if let Some(r) = doc.poll() {
                return r;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("no render");
    }

    #[test]
    fn opens_the_sample_and_reads_geometry() {
        let bytes = Arc::new(synthetic::sample_pdf());
        let doc = RenderDoc::open(bytes, &RenderOptions::default()).unwrap();
        assert_eq!(doc.page_count(), 2);
        let p = doc.page(0).unwrap();
        assert_eq!((p.width, p.height), (1224.0, 792.0));
        let v = p.user_to_view(0.0, 792.0);
        assert_eq!(v, [0.0, 0.0]);
        assert_eq!(doc.outline().len(), 2);
    }

    #[test]
    fn renders_a_page_and_a_tile() {
        let bytes = Arc::new(synthetic::sample_pdf());
        let doc = RenderDoc::open(bytes, &RenderOptions::default()).unwrap();
        let r = render_once(&doc, page_request(1, 0.5, 7));
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!((r.width, r.height, r.request.tag), (306, 396, 7));
        let t = render_once(
            &doc,
            tile_request(
                0,
                1.0,
                Tile {
                    x: 0,
                    y: 0,
                    w: 64,
                    h: 32,
                },
                9,
            ),
        );
        assert_eq!((t.width, t.height), (64, 32));
    }

    #[test]
    fn hidden_annotations_are_not_drawn() {
        let bytes = Arc::new(synthetic::sample_pdf());
        // The sample's filled square annotation sits at user (900..1000, 600..700) on page 1.
        let at = |doc: &RenderDoc| {
            let r = render_once(doc, page_request(0, 0.5, 1));
            pixel(&r, 475, 70)
        };
        let shown = RenderDoc::open(bytes.clone(), &RenderOptions::default()).unwrap();
        let c = at(&shown);
        assert!(c[1] < 100 && c[2] < 100 && c[0] > 200, "square drawn red: {c:?}");
        let opts = RenderOptions {
            hide: vec![synthetic::SQUARE_OBJ],
            ..Default::default()
        };
        let hidden = RenderDoc::open(bytes, &opts).unwrap();
        assert_eq!(hidden.hidden_count(), 1);
        let c = at(&hidden);
        assert!(c[0] > 200 && c[1] > 200 && c[2] > 200, "square hidden: {c:?}");
    }

    #[test]
    fn tiles_cover_the_region_once() {
        let t = tiles_covering(2500, 1000, 1024, [0, 0, 2500, 1000]);
        assert_eq!(t.len(), 3);
        assert_eq!(
            t[2],
            Tile {
                x: 2048,
                y: 0,
                w: 452,
                h: 1000
            }
        );
        assert!(tiles_covering(100, 100, 1024, [200, 0, 300, 50]).is_empty());
    }

    #[test]
    fn garbage_is_an_error_not_a_crash() {
        assert!(RenderDoc::open(Arc::new(b"not a pdf".to_vec()), &RenderOptions::default()).is_err());
        // Either an error or nothing hidden; never a panic.
        let r = hide_annotations(&Arc::new(b"%PDF-1.4 junk".to_vec()), &[(5, 0)]);
        assert!(!matches!(r, Ok((_, n)) if n > 0));
    }
}
