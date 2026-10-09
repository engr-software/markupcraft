//! Text search across pages, with the rectangles of every hit (one per line it spans) in PDF
//! user space, so a hit can be shown, zoomed to or turned into a highlight markup.
//!
//! The text layer comes from PdfCraft's renderer (glyphs with Unicode and boxes). It reads the
//! document as it is now: unsaved markups do not add text, but unsaved headers, footers and
//! watermarks do.

use std::sync::Arc;

use markupcraft_model::{PageInfo, Rect};
use pdfcraft_render::{PageRenderer, PageText, RenderConfig, RenderRequest, RequestKind};

use crate::{Result, Session, invalid};

/// Hits returned by one search, at most (the report says when there were more).
pub const MAX_HITS: usize = 10_000;
/// Characters of context on each side of a hit.
const CONTEXT: usize = 30;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_words: bool,
    /// Pages to search (0-based); `None` = every page.
    pub pages: Option<Vec<usize>>,
    /// Stop after this many hits (0 = [`MAX_HITS`]).
    pub max_hits: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub page: usize,
    /// The matched text as it reads on the page.
    pub text: String,
    /// The line around the hit.
    pub context: String,
    /// One rectangle per line the hit spans, in user space.
    pub rects: Vec<Rect>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchReport {
    pub hits: Vec<SearchHit>,
    pub pages_searched: usize,
    /// Pages whose text could not be read.
    pub unreadable: Vec<usize>,
    /// More hits exist than were returned.
    pub truncated: bool,
}

/// The renderer's view of a page: displayed size, crop box, rotation.
fn view_info(p: &PageInfo) -> pdfcraft_render::PageInfo {
    let c = p.crop.normalized();
    let (w, h) = (c.width() as f32, c.height() as f32);
    let rotation = p.rotate.rem_euclid(360) as u16;
    let (width, height) = if rotation % 180 == 90 { (h, w) } else { (w, h) };
    pdfcraft_render::PageInfo {
        width,
        height,
        label: String::new(),
        crop: [c.x0 as f32, c.y0 as f32, c.x1 as f32, c.y1 as f32],
        rotation,
    }
}

fn page_text(r: &mut PageRenderer, page: usize) -> std::result::Result<Arc<PageText>, String> {
    let out = r.render(RenderRequest {
        page,
        kind: RequestKind::Text,
        ..Default::default()
    });
    match (out.text, out.error) {
        (Some(t), _) => Ok(t),
        (None, Some(e)) => Err(e),
        (None, None) => Ok(Arc::new(PageText::default())),
    }
}

impl Session {
    /// Find `needle` (whitespace-insensitive) on the pages.
    pub fn search_text(&self, needle: &str, opts: &SearchOptions) -> Result<SearchReport> {
        if needle.split_whitespace().next().is_none() {
            return Err(invalid("type the text to search for"));
        }
        if needle.chars().count() > 1_000 {
            return Err(invalid("the search text is too long (1000 characters at most)"));
        }
        let pages: Vec<usize> = match &opts.pages {
            Some(p) => {
                for i in p {
                    self.page(*i)?;
                }
                p.clone()
            }
            None => (0..self.page_count()).collect(),
        };
        let limit = if opts.max_hits == 0 {
            MAX_HITS
        } else {
            opts.max_hits.min(MAX_HITS)
        };
        let mut renderer = PageRenderer::new(self.current_bytes()?, RenderConfig::default());
        let mut report = SearchReport::default();
        for page in pages {
            let Some(info) = self.doc.pages.get(page) else { continue };
            report.pages_searched += 1;
            let text = match page_text(&mut renderer, page) {
                Ok(t) => t,
                Err(_) => {
                    report.unreadable.push(page);
                    continue;
                }
            };
            let geom = view_info(info);
            for range in text.find_opts(needle, opts.case_sensitive, opts.whole_words) {
                if report.hits.len() >= limit {
                    report.truncated = true;
                    return Ok(report);
                }
                let rects = text
                    .line_rects(range.clone())
                    .into_iter()
                    .map(|r| {
                        let u = geom.view_rect_to_user(r);
                        Rect::new(u[0] as f64, u[1] as f64, u[2] as f64, u[3] as f64)
                    })
                    .collect();
                let from = range.start.saturating_sub(CONTEXT);
                let to = range.end.saturating_add(CONTEXT).min(text.glyphs.len());
                report.hits.push(SearchHit {
                    page,
                    text: text.text_of(range.clone()),
                    context: text.text_of(from..to).replace('\n', " "),
                    rects,
                });
            }
        }
        Ok(report)
    }

    /// The text of one page, lines separated by line breaks.
    pub fn page_text(&self, page: usize) -> Result<String> {
        self.page(page)?;
        let mut renderer = PageRenderer::new(self.current_bytes()?, RenderConfig::default());
        page_text(&mut renderer, page)
            .map(|t| t.plain_text())
            .map_err(|e| invalid(format!("the text of page {} could not be read: {e}", page + 1)))
    }
}
