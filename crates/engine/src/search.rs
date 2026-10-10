//! Text search across pages, with the rectangles of every hit (one per line it spans) in PDF
//! user space, so a hit can be shown, zoomed to or turned into a highlight markup.
//!
//! The text layer comes from PdfCraft's renderer (glyphs with Unicode and boxes). It reads the
//! document as it is now: unsaved markups do not add text, but unsaved headers, footers and
//! watermarks do.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

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

/// Glyphs the search cache keeps at most (about 100 bytes each), so a huge document cannot grow
/// it without bound; pages past the cap are read again on the next search.
const MAX_CACHED_GLYPHS: usize = 4_000_000;
/// Pages a worker thread reads text from in one go, at least.
const PAGES_PER_WORKER: usize = 4;

/// The text layers of pages (markups left out) for the bytes they were read from: a search
/// reads each page once, and the next search, or one after a markup edit, reuses them.
#[derive(Default)]
pub(crate) struct TextCache {
    bytes: Option<Arc<Vec<u8>>>,
    /// `None`: the page's text could not be read.
    pages: HashMap<usize, Option<Arc<PageText>>>,
    glyphs: usize,
}

impl TextCache {
    pub(crate) fn new() -> Mutex<Self> {
        Mutex::new(Self::default())
    }
}

/// Read the text of `pages` from `bytes` on up to `threads` worker threads (each parses the
/// document itself; the renderer catches its own panics).
fn read_texts(bytes: &Arc<Vec<u8>>, pages: &[usize], threads: usize) -> Vec<(usize, Option<Arc<PageText>>)> {
    let config = || RenderConfig {
        hide_comments: true,
        ..RenderConfig::default()
    };
    let workers = threads.clamp(1, 8).min(pages.len().div_ceil(PAGES_PER_WORKER)).max(1);
    if workers <= 1 {
        let mut r = PageRenderer::new(bytes.clone(), config());
        return pages.iter().map(|&p| (p, page_text(&mut r, p).ok())).collect();
    }
    let chunk = pages.len().div_ceil(workers).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = pages
            .chunks(chunk)
            .map(|part| {
                let bytes = bytes.clone();
                s.spawn(move || {
                    let mut r = PageRenderer::new(bytes, config());
                    part.iter().map(|&p| (p, page_text(&mut r, p).ok())).collect::<Vec<_>>()
                })
            })
            .collect();
        let mut out = Vec::with_capacity(pages.len());
        for (h, part) in handles.into_iter().zip(pages.chunks(chunk)) {
            match h.join() {
                Ok(v) => out.extend(v),
                // A worker that died reads as unreadable pages, never a crash here.
                Err(_) => out.extend(part.iter().map(|&p| (p, None))),
            }
        }
        out
    })
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
        // Page text only: markup appearances (Search Markups) and form field values (Search
        // Form Fields) are separate targets, so neither is read here.
        let texts = self.search_texts(&pages)?;
        let fields: Vec<(usize, Rect)> = self
            .form_fields()
            .into_iter()
            .filter_map(|f| Some((f.page?, f.rect?.normalized())))
            .collect();
        let in_field = |page: usize, r: &Rect| {
            let (cx, cy) = ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
            fields
                .iter()
                .any(|(p, f)| *p == page && f.x0 <= cx && cx <= f.x1 && f.y0 <= cy && cy <= f.y1)
        };
        let mut report = SearchReport::default();
        for page in pages {
            let Some(info) = self.doc.pages.get(page) else { continue };
            report.pages_searched += 1;
            let Some(text) = texts.get(&page).cloned().flatten() else {
                report.unreadable.push(page);
                continue;
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
                    .collect::<Vec<Rect>>();
                if !rects.is_empty() && rects.iter().all(|r| in_field(page, r)) {
                    continue;
                }
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

    /// The bytes search reads: the file itself while only markups changed (search leaves
    /// markups out, so an unsaved takeoff does not need the document written out first).
    fn search_bytes(&self) -> Result<Arc<Vec<u8>>> {
        if !self.file.cos.is_modified() && self.file.cos.security().is_none() {
            return Ok(self.file.cos.bytes().clone());
        }
        self.current_bytes()
    }

    /// The text layers of `pages`, from the cache or read now (and cached).
    fn search_texts(&self, pages: &[usize]) -> Result<HashMap<usize, Option<Arc<PageText>>>> {
        let bytes = self.search_bytes()?;
        let mut cache = self.text_cache.lock().unwrap_or_else(PoisonError::into_inner);
        if !cache.bytes.as_ref().is_some_and(|b| Arc::ptr_eq(b, &bytes)) {
            *cache = TextCache {
                bytes: Some(bytes.clone()),
                ..TextCache::default()
            };
        }
        let mut missing: Vec<usize> = pages.iter().copied().filter(|p| !cache.pages.contains_key(p)).collect();
        missing.sort_unstable();
        missing.dedup();
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get());
        let mut out: HashMap<usize, Option<Arc<PageText>>> = pages
            .iter()
            .filter_map(|p| Some((*p, cache.pages.get(p)?.clone())))
            .collect();
        for (p, t) in read_texts(&bytes, &missing, threads) {
            let n = t.as_ref().map_or(0, |t| t.glyphs.len());
            if cache.glyphs.saturating_add(n) <= MAX_CACHED_GLYPHS {
                cache.glyphs += n;
                cache.pages.insert(p, t.clone());
            }
            out.insert(p, t);
        }
        Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};
    use markupcraft_model::{Kind, Markup, Point};

    fn cached_pages(s: &Session) -> usize {
        s.text_cache.lock().map(|c| c.pages.len()).unwrap_or(0)
    }

    /// Search reads each page's text once: a second search and one after a markup edit reuse
    /// it; a change to the page content (a watermark) is read again.
    #[test]
    fn search_text_is_cached_until_the_page_content_changes() {
        let pages: Vec<SyntheticPage> = (0..12)
            .map(|i| SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, &format!("ROOM {i} KITCHEN"))))
            .collect();
        let mut s = Session::from_bytes(pdf(&pages), "cache.pdf").unwrap();
        let opts = SearchOptions::default();
        let first = s.search_text("kitchen", &opts).unwrap();
        assert_eq!(first.hits.len(), 12);
        assert_eq!(cached_pages(&s), 12, "every page read once, in parallel");
        let again = s.search_text("room 3", &opts).unwrap();
        assert_eq!(again.hits.len(), 1);
        assert_eq!(again.hits[0].page, 3);
        // A markup edit does not change page text: the cache stays.
        s.add_markup(Markup::new(
            Kind::Rectangle,
            0,
            vec![Point::new(10.0, 10.0), Point::new(50.0, 50.0)],
        ))
        .unwrap();
        let bytes_before = s.text_cache.lock().unwrap().bytes.clone().unwrap();
        assert_eq!(s.search_text("kitchen", &opts).unwrap(), first);
        assert!(Arc::ptr_eq(
            &bytes_before,
            s.text_cache.lock().unwrap().bytes.as_ref().unwrap()
        ));
        // New page content is found.
        let wm = crate::marks::Watermark {
            text: "DRAFTCOPY".into(),
            ..Default::default()
        };
        s.add_watermark(&[5], &wm, false).unwrap();
        let w = s.search_text("draftcopy", &opts).unwrap();
        assert_eq!(w.hits.iter().map(|h| h.page).collect::<Vec<_>>(), vec![5]);
        // A subset of pages, and pages that do not exist.
        let some = SearchOptions {
            pages: Some(vec![1, 2]),
            ..Default::default()
        };
        assert_eq!(s.search_text("kitchen", &some).unwrap().hits.len(), 2);
        let bad = SearchOptions {
            pages: Some(vec![99]),
            ..Default::default()
        };
        assert!(s.search_text("kitchen", &bad).is_err());
    }
}
