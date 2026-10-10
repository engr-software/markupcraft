//! Rendering pages for the review features (compare, visual search, OCR): the document's
//! current state as PDF bytes, pages rendered to grayscale through `markupcraft-render`, the
//! page's text layer, and the maps between image pixels and PDF user space.

use std::sync::Arc;

use markupcraft_geom::{Point, Rect};
use markupcraft_render::text::{PageText, TextExtractor, TextSource};
use markupcraft_render::{PageGeom, RenderDoc, RenderOptions, page_request};

use crate::{Result, Session, invalid};

/// How long one page render may take. Renders run on a worker whose watchdog reports a page
/// stuck after 20 s (a hostile page, say a dashed line billions of dashes long, must not hang a
/// feature); this is the backstop after it.
const RENDER_DEADLINE: std::time::Duration = std::time::Duration::from_secs(40);

/// A grayscale image: row-major, 0 = black, 255 = white.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Gray {
    pub fn new(w: usize, h: usize, fill: u8) -> Self {
        Self {
            w,
            h,
            px: vec![fill; w.saturating_mul(h)],
        }
    }

    /// From premultiplied RGBA (as the renderer returns it) composited on white.
    pub fn from_rgba(rgba: &[u8], w: usize, h: usize) -> Self {
        let mut g = Gray::new(w, h, 255);
        for (out, p) in g.px.iter_mut().zip(rgba.as_chunks::<4>().0) {
            let (r, gr, b, a) = (p[0] as u32, p[1] as u32, p[2] as u32, p[3] as u32);
            // Premultiplied over white: c + (255 - a).
            let over = |c: u32| (c + 255 - a.min(255)).min(255);
            *out = ((over(r) * 77 + over(gr) * 150 + over(b) * 29 + 128) >> 8) as u8;
        }
        g
    }

    pub fn get(&self, x: usize, y: usize) -> u8 {
        if x >= self.w {
            return 255;
        }
        self.px
            .get(y.saturating_mul(self.w).saturating_add(x))
            .copied()
            .unwrap_or(255)
    }

    /// The `[x0, x1) x [y0, y1)` part (clamped to the image).
    pub fn crop(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> Gray {
        let (x1, y1) = (x1.min(self.w), y1.min(self.h));
        let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        let mut out = Gray::new(w, h, 255);
        for y in 0..h {
            for x in 0..w {
                if let Some(v) = out.px.get_mut(y * w + x) {
                    *v = self.get(x0 + x, y0 + y);
                }
            }
        }
        out
    }

    /// Shrink by an integer factor (box filter).
    pub fn downsample(&self, f: usize) -> Gray {
        let f = f.max(1);
        if f == 1 {
            return self.clone();
        }
        let (w, h) = (self.w.div_ceil(f), self.h.div_ceil(f));
        let mut out = Gray::new(w, h, 255);
        for y in 0..h {
            for x in 0..w {
                let (mut sum, mut n) = (0u32, 0u32);
                for yy in y * f..((y + 1) * f).min(self.h) {
                    for xx in x * f..((x + 1) * f).min(self.w) {
                        sum += self.get(xx, yy) as u32;
                        n += 1;
                    }
                }
                if let Some(v) = out.px.get_mut(y * w + x) {
                    *v = sum.checked_div(n).map_or(255, |m| m as u8);
                }
            }
        }
        out
    }

    /// Turned clockwise by `quarters` quarter turns.
    pub fn rotated(&self, quarters: u32) -> Gray {
        let q = quarters % 4;
        if q == 0 {
            return self.clone();
        }
        let (w, h) = if q % 2 == 1 { (self.h, self.w) } else { (self.w, self.h) };
        let mut out = Gray::new(w, h, 255);
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = match q {
                    1 => (y, self.h.saturating_sub(1 + x)),
                    2 => (self.w.saturating_sub(1 + x), self.h.saturating_sub(1 + y)),
                    _ => (self.w.saturating_sub(1 + y), x),
                };
                if let Some(v) = out.px.get_mut(y * w + x) {
                    *v = self.get(sx, sy);
                }
            }
        }
        out
    }
}

/// One rendered page and how its pixels map to user space.
#[derive(Debug, Clone)]
pub struct PageImage {
    pub page: usize,
    pub gray: Gray,
    /// Device pixels per point.
    pub scale: f32,
    pub geom: PageGeom,
}

impl PageImage {
    /// An image pixel position (x right, y down) in PDF user space.
    pub fn to_user(&self, x: f64, y: f64) -> Point {
        let s = self.scale.max(1e-6) as f64;
        let [ux, uy] = self.geom.view_to_user((x / s) as f32, (y / s) as f32);
        Point::new(ux as f64, uy as f64)
    }

    /// A pixel box `[x0, y0, x1, y1]` as a user-space rectangle.
    pub fn rect_to_user(&self, r: [f64; 4]) -> Rect {
        let a = self.to_user(r[0], r[1]);
        let b = self.to_user(r[2], r[3]);
        Rect::new(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
    }

    /// A user-space rectangle as a pixel box `[x0, y0, x1, y1]` (unclamped).
    pub fn rect_to_px(&self, r: Rect) -> [f64; 4] {
        let s = self.scale as f64;
        let a = self.geom.user_to_view(r.x0 as f32, r.y0 as f32);
        let b = self.geom.user_to_view(r.x1 as f32, r.y1 as f32);
        let (x0, x1) = (a[0].min(b[0]) as f64 * s, a[0].max(b[0]) as f64 * s);
        let (y0, y1) = (a[1].min(b[1]) as f64 * s, a[1].max(b[1]) as f64 * s);
        [x0, y0, x1, y1]
    }
}

/// A page rendered in colour (premultiplied RGBA).
#[derive(Debug, Clone)]
pub struct RgbImage {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    pub scale: f32,
    pub geom: PageGeom,
}

impl RgbImage {
    /// The colour (on white) at user-space point `(x, y)`; white outside the page.
    pub fn pixel(&self, x: f64, y: f64) -> [u8; 3] {
        let [vx, vy] = self.geom.user_to_view(x as f32, y as f32);
        let (px, py) = ((vx * self.scale).floor(), (vy * self.scale).floor());
        if px < 0.0 || py < 0.0 || px as usize >= self.w || py as usize >= self.h {
            return [255; 3];
        }
        let i = (py as usize * self.w + px as usize) * 4;
        match self.rgba.get(i..i + 4) {
            Some(p) => {
                let a = 255 - p[3] as u32;
                [
                    (p[0] as u32 + a).min(255) as u8,
                    (p[1] as u32 + a).min(255) as u8,
                    (p[2] as u32 + a).min(255) as u8,
                ]
            }
            None => [255; 3],
        }
    }
}

/// A PDF to render: its bytes and page geometry.
pub struct Renderable {
    pub bytes: Arc<Vec<u8>>,
    doc: RenderDoc,
    hide_markups: bool,
    /// The text layer, parsed once and cached per page (a large set re-parsed per page made
    /// comparing or exporting it quadratic).
    text: std::sync::Mutex<Option<TextExtractor>>,
    /// Tags requests, so a late result of an earlier one is never taken for this one.
    tag: std::sync::atomic::AtomicU64,
}

impl Renderable {
    /// `hide_markups`: leave markup annotations out (fields and links still draw).
    pub fn new(bytes: Arc<Vec<u8>>, hide_markups: bool) -> Result<Self> {
        // One worker rather than rendering inline: its watchdog gives up on a stuck page.
        let opts = RenderOptions {
            threads: 1,
            hide_all_markups: hide_markups,
            ..Default::default()
        };
        let doc = RenderDoc::open(bytes.clone(), &opts).map_err(|e| invalid(e.to_string()))?;
        Ok(Self {
            bytes,
            doc,
            hide_markups,
            text: std::sync::Mutex::new(None),
            tag: std::sync::atomic::AtomicU64::new(1),
        })
    }

    /// Give up on a page render after `limit` instead of the default 20 s.
    pub fn set_stuck_after(&mut self, limit: std::time::Duration) {
        self.doc.set_stuck_after(limit);
    }

    pub fn page_count(&self) -> usize {
        self.doc.page_count()
    }

    pub fn geom(&self, page: usize) -> Result<&PageGeom> {
        self.doc.page(page).ok_or_else(|| crate::EngineError::NoPage {
            page: page.saturating_add(1),
            count: self.doc.page_count(),
        })
    }

    /// Render `page` with its longer side at most `max_side` pixels and at most `scale`
    /// pixels per point.
    pub fn render(&self, page: usize, scale: f32, max_side: f32) -> Result<PageImage> {
        let (r, geom) = self.render_raw(page, scale, max_side)?;
        let (w, h) = (r.width as usize, r.height as usize);
        Ok(PageImage {
            page,
            gray: Gray::from_rgba(&r.rgba, w, h),
            scale: r.request.scale,
            geom,
        })
    }

    /// Render `page` in colour at `scale` pixels per point.
    pub fn render_rgba(&self, page: usize, scale: f32) -> Result<RgbImage> {
        let (r, geom) = self.render_raw(page, scale, 8_000.0)?;
        Ok(RgbImage {
            w: r.width as usize,
            h: r.height as usize,
            rgba: r.rgba,
            scale: r.request.scale,
            geom,
        })
    }

    fn render_raw(
        &self,
        page: usize,
        scale: f32,
        max_side: f32,
    ) -> Result<(markupcraft_render::RenderedPage, PageGeom)> {
        let geom = self.geom(page)?.clone();
        let scale = scale.min(geom.scale_for_side(max_side)).max(0.01);
        let tag = self.tag.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.doc.request(vec![page_request(page, scale, tag)]);
        let start = std::time::Instant::now();
        loop {
            match self.doc.poll() {
                Some(r) if r.request.tag == tag && r.request.page == page => {
                    if let Some(e) = r.error {
                        return Err(invalid(format!("page {} could not be rendered: {e}", page + 1)));
                    }
                    return Ok((r, geom));
                }
                Some(_) => {}
                None if start.elapsed() > RENDER_DEADLINE => {
                    return Err(invalid(format!(
                        "page {} could not be rendered (it took too long)",
                        page + 1
                    )));
                }
                None => std::thread::sleep(std::time::Duration::from_micros(500)),
            }
        }
    }

    /// The text layer of `page`: without markup text when the markups are hidden.
    pub fn text(&self, page: usize) -> Option<Arc<PageText>> {
        let mut t = self.text.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        t.get_or_insert_with(|| TextExtractor::with_markups(self.bytes.clone(), !self.hide_markups))
            .page_text(page)
    }
}

/// A word of a page's text layer, with its box in user space.
#[derive(Debug, Clone, PartialEq)]
pub struct PageWord {
    pub text: String,
    pub rect: Rect,
}

/// Split a page's text into words (glyph runs broken at spaces and line ends).
pub fn words(text: &PageText, geom: &PageGeom) -> Vec<PageWord> {
    let mut out = Vec::new();
    let mut cur: Option<(String, [f32; 4], u32)> = None;
    let flush = |cur: &mut Option<(String, [f32; 4], u32)>, out: &mut Vec<PageWord>| {
        if let Some((t, r, _)) = cur.take()
            && !t.trim().is_empty()
        {
            let a = geom.view_to_user(r[0], r[1]);
            let b = geom.view_to_user(r[2], r[3]);
            out.push(PageWord {
                text: t,
                rect: Rect::new(
                    a[0].min(b[0]) as f64,
                    a[1].min(b[1]) as f64,
                    a[0].max(b[0]) as f64,
                    a[1].max(b[1]) as f64,
                ),
            });
        }
    };
    for (i, g) in text.glyphs.iter().enumerate() {
        let line = text.line_of.get(i).copied().unwrap_or(0);
        let space = text.space_before.get(i).copied().unwrap_or(false);
        let blank = g.text.trim().is_empty();
        let breaks = match &cur {
            Some((_, _, l)) => *l != line || space,
            None => false,
        };
        if breaks || blank {
            flush(&mut cur, &mut out);
        }
        if blank {
            continue;
        }
        match &mut cur {
            Some((t, r, _)) => {
                t.push_str(&g.text);
                r[0] = r[0].min(g.rect[0]);
                r[1] = r[1].min(g.rect[1]);
                r[2] = r[2].max(g.rect[2]);
                r[3] = r[3].max(g.rect[3]);
            }
            None => cur = Some((g.text.clone(), g.rect, line)),
        }
    }
    flush(&mut cur, &mut out);
    out
}

impl Session {
    /// The document as it is now, ready to render.
    pub fn renderable(&self, hide_markups: bool) -> Result<Renderable> {
        Renderable::new(self.current_bytes()?, hide_markups)
    }
}

/// Read a PDF file into memory.
pub fn read_pdf(path: &std::path::Path) -> Result<Arc<Vec<u8>>> {
    std::fs::read(path).map(Arc::new).map_err(|e| crate::EngineError::Io {
        path: path.display().to_string(),
        source: e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gray_rotates_and_downsamples() {
        let mut g = Gray::new(3, 2, 255);
        g.px[0] = 0; // top-left
        let r = g.rotated(1);
        assert_eq!((r.w, r.h), (2, 3));
        assert_eq!(r.get(1, 0), 0, "top-left goes to top-right after a clockwise turn");
        assert_eq!(g.rotated(4), g);
        let d = Gray::new(4, 4, 0).downsample(2);
        assert_eq!((d.w, d.h, d.px[0]), (2, 2, 0));
    }

    /// fuzz: a dashed line billions of points long makes the renderer stroke hundreds of
    /// millions of dashes. Rendering inline, compare / export / OCR on such a page never
    /// returned; now the render worker's watchdog gives up on it and the feature gets an error.
    #[test]
    fn a_page_that_never_finishes_rendering_is_an_error_not_a_hang() {
        use crate::synthetic::{SyntheticPage, pdf};
        let hostile = "0 0 0 RG 0.5 w [6 3] 0 d 60 350 m 4294967295 350 l S";
        let bytes = pdf(&[
            SyntheticPage::new(612.0, 792.0, hostile),
            SyntheticPage::new(612.0, 792.0, "0 0 0 RG 10 10 m 100 100 l S"),
        ]);
        let mut r = Renderable::new(Arc::new(bytes), true).unwrap();
        r.set_stuck_after(std::time::Duration::from_millis(500));
        let t = std::time::Instant::now();
        let Err(e) = r.render(0, 0.15, 400.0) else {
            panic!("the stuck page is an error")
        };
        let e = e.to_string();
        assert!(e.contains("took longer"), "given up by the watchdog: {e}");
        assert!(t.elapsed() < std::time::Duration::from_secs(15), "{:?}", t.elapsed());
        // The next page still renders (on the watchdog's replacement worker).
        assert!(r.render(1, 0.15, 400.0).is_ok());
    }
}
