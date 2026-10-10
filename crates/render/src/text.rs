//! Page text for search (the hook the Search panel will use).
//!
//! [`TextSource`] is what a search needs: the text layer of one page, with glyph boxes in view
//! space. [`TextExtractor`] implements it with PdfCraft's text extraction on the calling thread
//! and caches each page. A search panel later can run it on a worker and add a whole-document
//! index; the trait stays the same.

use std::collections::HashMap;
use std::sync::Arc;

use pdfcraft_render::{PageRenderer, RenderConfig, RenderRequest, RequestKind};
pub use pdfcraft_render::{PageText, TextGlyph};

/// Something that can hand out a page's text layer.
pub trait TextSource {
    /// The text of `page` (0-based), `None` when it cannot be read.
    fn page_text(&mut self, page: usize) -> Option<Arc<PageText>>;
}

/// Synchronous, cached text extraction over the document bytes.
pub struct TextExtractor {
    renderer: PageRenderer,
    cache: HashMap<usize, Option<Arc<PageText>>>,
}

impl TextExtractor {
    pub fn new(bytes: Arc<Vec<u8>>) -> Self {
        Self::with_markups(bytes, true)
    }

    /// `markups: false` leaves the text of markup annotations' appearances out (the page's
    /// own text only; links and form fields still count).
    pub fn with_markups(bytes: Arc<Vec<u8>>, markups: bool) -> Self {
        let config = RenderConfig {
            hide_comments: !markups,
            ..RenderConfig::default()
        };
        Self {
            renderer: PageRenderer::new(bytes, config),
            cache: HashMap::new(),
        }
    }

    pub fn page_count(&self) -> usize {
        self.renderer.page_count()
    }

    /// Every page that contains `needle` (case-insensitive), with the number of hits.
    pub fn find(&mut self, needle: &str) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for p in 0..self.page_count() {
            if let Some(t) = self.page_text(p) {
                let n = t.find_opts(needle, false, false).len();
                if n > 0 {
                    out.push((p, n));
                }
            }
        }
        out
    }
}

impl TextSource for TextExtractor {
    fn page_text(&mut self, page: usize) -> Option<Arc<PageText>> {
        if let Some(t) = self.cache.get(&page) {
            return t.clone();
        }
        let r = self.renderer.render(RenderRequest {
            page,
            kind: RequestKind::Text,
            ..Default::default()
        });
        if let Some(e) = &r.error {
            log::warn!("text of page {}: {e}", page + 1);
        }
        self.cache.insert(page, r.text.clone());
        r.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_words_in_the_sample() {
        let mut t = TextExtractor::new(Arc::new(crate::synthetic::sample_pdf()));
        assert_eq!(t.find("kitchen"), vec![(0, 1)]);
        assert_eq!(t.find("notes"), vec![(1, 1)]);
        assert!(t.page_text(99).is_none());
    }
}
