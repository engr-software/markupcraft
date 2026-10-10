//! Act on search results: make a hyperlink over each checked result (a page of this document,
//! a file or a web address), as one undo step.

use markupcraft_geom::Rect;

use crate::links::{LinkLook, LinkTarget};
use crate::{Result, Session, invalid};

/// Most links made at once.
pub const MAX_LINKS: usize = 5_000;

impl Session {
    /// A link over each hit (`page` 0-based, the hit's rectangles joined into one box per
    /// hit) to `target`. One undo step; returns the new links' ids.
    pub fn link_hits(
        &mut self,
        hits: &[(usize, Vec<Rect>)],
        target: &LinkTarget,
        look: LinkLook,
    ) -> Result<Vec<String>> {
        if hits.is_empty() {
            return Err(invalid("no results to link"));
        }
        if hits.len() > MAX_LINKS {
            return Err(invalid(format!("at most {MAX_LINKS} links at once")));
        }
        let mut boxes = Vec::with_capacity(hits.len());
        for (page, rects) in hits {
            self.page(*page)?;
            let mut it = rects.iter().map(|r| r.normalized());
            let Some(first) = it.next() else {
                return Err(invalid("a result has no rectangle"));
            };
            let b = it.fold(first, |a, r| {
                Rect::new(a.x0.min(r.x0), a.y0.min(r.y0), a.x1.max(r.x1), a.y1.max(r.y1))
            });
            // Tiny hits (a single narrow glyph) still get a clickable box.
            let grow_x = ((1.0 - b.width()) / 2.0).max(0.0);
            let grow_y = ((1.0 - b.height()) / 2.0).max(0.0);
            boxes.push((
                *page,
                Rect::new(b.x0 - grow_x, b.y0 - grow_y, b.x1 + grow_x, b.y1 + grow_y),
            ));
        }
        let saved = self.merge.take();
        self.seal();
        self.merge = Some("link-search-results".into());
        let mut ids = Vec::with_capacity(boxes.len());
        let mut err = None;
        for (page, r) in boxes {
            match self.add_link(page, r, target, look) {
                Ok(id) => ids.push(id),
                Err(e) => {
                    err = Some(e);
                    break;
                }
            }
        }
        self.merge = saved;
        self.seal();
        match err {
            Some(e) => {
                if !ids.is_empty() {
                    // Leave nothing half done.
                    let _ = self.undo();
                }
                Err(e)
            }
            None => Ok(ids),
        }
    }
}
