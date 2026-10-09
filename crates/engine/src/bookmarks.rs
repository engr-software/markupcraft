//! Bookmarks (the document outline, ISO 32000-1 Â§12.3.3): read the tree with each bookmark's
//! page, add, rename, re-target, move or nest, delete, and create bookmarks from pages.
//!
//! Bookmarks are addressed by their **path**: child indices from the top level, so `[0, 2]` is
//! the third child of the first top-level bookmark. Edits go through PdfCraft's outline editor
//! (`pdfcraft-organize`), which relinks only the affected items and keeps everything else on
//! them (colour, style, actions). Every edit is one undoable step.

use std::collections::HashSet;

use markupcraft_revu::cos::{Document as CosDoc, ObjRef};

use crate::docutil::{err, item_page, page_objs, text_of};
use crate::{Result, Session, invalid};

/// Bookmarks read from one outline, at most.
const MAX_ITEMS: usize = 100_000;
const MAX_DEPTH: usize = 64;

/// One bookmark, in outline order (parents before their children).
#[derive(Debug, Clone, PartialEq)]
pub struct BookmarkItem {
    pub path: Vec<usize>,
    pub title: String,
    /// The page it goes to (0-based), when it goes to a page of this document.
    pub page: Option<usize>,
    /// Shown expanded.
    pub open: bool,
    pub children: usize,
}

/// What the bookmarks made from pages are called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookmarkTitles {
    /// The page label (`Page N` where a page has none).
    Labels,
    /// `Page N`.
    PageNumbers,
}

fn outline_root(cos: &CosDoc) -> Option<ObjRef> {
    let root = cos.root()?;
    cos.get(root).as_dict()?.reference(b"Outlines")
}

/// The outline as a flat list (document order).
pub(crate) fn read(cos: &CosDoc) -> Vec<BookmarkItem> {
    let Some(root) = outline_root(cos) else {
        return Vec::new();
    };
    let pages = page_objs(cos).unwrap_or_default();
    let mut out = Vec::new();
    let mut seen = HashSet::from([root]);
    walk(cos, root, &[], &pages, &mut seen, &mut out, 0);
    out
}

fn walk(
    cos: &CosDoc,
    parent: ObjRef,
    path: &[usize],
    pages: &[ObjRef],
    seen: &mut HashSet<ObjRef>,
    out: &mut Vec<BookmarkItem>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let mut next = cos.get(parent).as_dict().and_then(|d| d.reference(b"First"));
    let mut i = 0;
    while let Some(r) = next {
        if out.len() >= MAX_ITEMS || !seen.insert(r) {
            return;
        }
        let obj = cos.get(r);
        let Some(d) = obj.as_dict() else { return };
        let mut p = path.to_vec();
        p.push(i);
        let at = out.len();
        out.push(BookmarkItem {
            path: p.clone(),
            title: text_of(cos, d.get(b"Title")),
            page: item_page(cos, d, pages),
            open: d.int(b"Count").is_some_and(|c| c > 0),
            children: 0,
        });
        walk(cos, r, &p, pages, seen, out, depth + 1);
        let kids = out.iter().skip(at + 1).filter(|b| b.path.len() == p.len() + 1).count();
        if let Some(b) = out.get_mut(at) {
            b.children = kids;
        }
        next = d.reference(b"Next");
        i += 1;
    }
}

impl Session {
    /// Every bookmark, parents before their children.
    pub fn bookmarks(&self) -> Vec<BookmarkItem> {
        read(&self.file.cos)
    }

    fn check_page(&self, page: usize) -> Result<()> {
        self.page(page).map(|_| ())
    }

    /// Add a bookmark titled `title` going to `page`, as child `index` of `parent` (`[]` = the
    /// top level; `None` or past the end appends). Returns its path.
    pub fn add_bookmark(
        &mut self,
        parent: &[usize],
        index: Option<usize>,
        title: &str,
        page: usize,
    ) -> Result<Vec<usize>> {
        self.check_page(page)?;
        self.cos_edit("Add Bookmark", |cos| {
            let path =
                pdfcraft_organize::add_bookmark(cos, parent, index.unwrap_or(usize::MAX), title, page).map_err(err)?;
            Ok((path, true))
        })
    }

    pub fn rename_bookmark(&mut self, path: &[usize], title: &str) -> Result<()> {
        self.cos_edit("Rename Bookmark", |cos| {
            pdfcraft_organize::rename_bookmark(cos, path, title).map_err(err)?;
            Ok(((), true))
        })
    }

    /// Point a bookmark at another page.
    pub fn set_bookmark_page(&mut self, path: &[usize], page: usize) -> Result<()> {
        self.check_page(page)?;
        self.cos_edit("Bookmark Destination", |cos| {
            pdfcraft_organize::set_bookmark_page(cos, path, page).map_err(err)?;
            Ok(((), true))
        })
    }

    /// Expand or collapse a bookmark that has children.
    pub fn set_bookmark_open(&mut self, path: &[usize], open: bool) -> Result<()> {
        self.cos_edit("Expand Bookmark", |cos| {
            pdfcraft_organize::set_bookmark_open(cos, path, open).map_err(err)?;
            Ok(((), true))
        })
    }

    /// Move the bookmark at `from` (with its children) to child `index` of `to_parent`
    /// (counted after taking it out; `None` or past the end appends). Returns its new path.
    pub fn move_bookmark(&mut self, from: &[usize], to_parent: &[usize], index: Option<usize>) -> Result<Vec<usize>> {
        self.cos_edit("Move Bookmark", |cos| {
            let path =
                pdfcraft_organize::move_bookmark(cos, from, to_parent, index.unwrap_or(usize::MAX)).map_err(err)?;
            Ok((path, true))
        })
    }

    /// Delete a bookmark and everything under it.
    pub fn delete_bookmark(&mut self, path: &[usize]) -> Result<()> {
        self.cos_edit("Delete Bookmark", |cos| {
            pdfcraft_organize::delete_bookmark(cos, path).map_err(err)?;
            Ok(((), true))
        })
    }

    /// Delete every bookmark; returns how many there were.
    pub fn clear_bookmarks(&mut self) -> Result<usize> {
        let n = self.bookmarks().len();
        self.cos_edit("Delete Bookmarks", |cos| {
            clear(cos)?;
            Ok((n, n > 0))
        })
    }

    /// One top-level bookmark per page in `pages` (in order), titled by its page label or
    /// number. With `replace` the existing bookmarks go first; otherwise the new ones follow
    /// them. Returns how many were made.
    pub fn bookmarks_from_pages(&mut self, pages: &[usize], titles: BookmarkTitles, replace: bool) -> Result<usize> {
        if pages.is_empty() {
            return Err(invalid("no pages given"));
        }
        for p in pages {
            self.check_page(*p)?;
        }
        let names: Vec<String> = pages
            .iter()
            .map(|p| {
                let label = self
                    .doc
                    .pages
                    .get(*p)
                    .map(|i| i.label.trim().to_string())
                    .unwrap_or_default();
                match titles {
                    BookmarkTitles::Labels if !label.is_empty() => label,
                    _ => format!("Page {}", p + 1),
                }
            })
            .collect();
        self.cos_edit("Create Bookmarks", |cos| {
            if replace {
                clear(cos)?;
            }
            for (page, title) in pages.iter().zip(&names) {
                pdfcraft_organize::add_bookmark(cos, &[], usize::MAX, title, *page).map_err(err)?;
            }
            Ok((names.len(), true))
        })
    }
}

/// Remove the outline from the catalog.
pub(crate) fn clear(cos: &mut CosDoc) -> Result<()> {
    let root = cos.root().ok_or_else(|| invalid("the PDF has no catalog"))?;
    cos.update_dict(root, |d| {
        d.remove(b"Outlines");
    })?;
    Ok(())
}

/// A top-level bookmark per `(title, page)`, after the existing ones.
pub(crate) fn append(cos: &mut CosDoc, items: &[(String, usize)]) -> Result<()> {
    for (title, page) in items {
        let title = if title.trim().is_empty() {
            "Untitled"
        } else {
            title.as_str()
        };
        pdfcraft_organize::add_bookmark(cos, &[], usize::MAX, title, *page).map_err(err)?;
    }
    Ok(())
}

/// Bookmark titles as page labels: the first bookmark that goes to a page names it.
pub(crate) fn titles_by_page(cos: &CosDoc, pages: usize) -> Vec<Option<String>> {
    let mut out = vec![None; pages];
    for b in read(cos) {
        if let Some(slot) = b.page.and_then(|p| out.get_mut(p))
            && slot.is_none()
            && !b.title.trim().is_empty()
        {
            *slot = Some(b.title.trim().to_string());
        }
    }
    out
}
