//! Reorder bookmarks by page (General > Document's "Reorder bookmarks when pages move"): at
//! every level, bookmarks follow the order of the pages they go to; ones without a page keep
//! their place after them.

use crate::{Result, Session};

impl Session {
    /// Sort every level of bookmarks by page. Returns how many moved.
    pub fn sort_bookmarks_by_page(&mut self) -> Result<usize> {
        let mut moved = 0;
        let mut parents: Vec<Vec<usize>> = vec![Vec::new()];
        let mut guard = 0;
        while let Some(parent) = parents.pop() {
            guard += 1;
            if guard > 10_000 {
                break;
            }
            let level = |s: &Session| -> Vec<(Vec<usize>, Option<usize>, usize)> {
                s.bookmarks()
                    .into_iter()
                    .filter(|b| b.path.len() == parent.len() + 1 && b.path.starts_with(&parent))
                    .map(|b| (b.path, b.page, b.children))
                    .collect()
            };
            let n = level(self).len();
            for k in 0..n {
                let items = level(self);
                let best = items
                    .iter()
                    .enumerate()
                    .skip(k)
                    .min_by_key(|(i, (_, page, _))| (page.unwrap_or(usize::MAX), *i))
                    .map(|(i, _)| i);
                if let Some(j) = best
                    && j != k
                    && let Some((path, _, _)) = items.get(j)
                {
                    self.move_bookmark(path, &parent, Some(k))?;
                    moved += 1;
                }
            }
            for (path, _, children) in level(self) {
                if children > 0 {
                    parents.push(path);
                }
            }
        }
        Ok(moved)
    }
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use crate::synthetic::{SyntheticPage, pdf};

    #[test]
    fn bookmarks_follow_their_pages() {
        let pages: Vec<SyntheticPage> = (0..3)
            .map(|_| SyntheticPage::new(612.0, 792.0, String::new()))
            .collect();
        let mut s = Session::from_bytes(pdf(&pages), "b.pdf").unwrap();
        s.add_bookmark(&[], None, "Third", 2).unwrap();
        s.add_bookmark(&[], None, "First", 0).unwrap();
        s.add_bookmark(&[], None, "Second", 1).unwrap();
        let n = s.sort_bookmarks_by_page().unwrap();
        assert!(n >= 1);
        let titles: Vec<String> = s.bookmarks().into_iter().map(|b| b.title).collect();
        assert_eq!(titles, vec!["First", "Second", "Third"]);
        assert_eq!(s.sort_bookmarks_by_page().unwrap(), 0);
    }
}
