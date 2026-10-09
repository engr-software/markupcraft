//! Whole-file operations: split a document into several PDFs, combine PDFs into one, and
//! replace pages with pages of another PDF.
//!
//! Split and combine use the page plans of [`crate::pages`], so markups, page scales and page
//! labels travel with their pages. Replace Pages swaps what a page shows (content, resources,
//! page boxes) and keeps the page's own markups, links and the bookmarks that go to it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use markupcraft_revu::cos::Document as CosDoc;

use crate::docutil::{err, page_objs, same_file, write_doc};
use crate::pages::{ForeignPdf, PagePlan, apply_plan};
use crate::{EngineError, Result, Session, bookmarks, invalid};

/// Files written by one split, at most.
const MAX_PARTS: usize = 10_000;
/// Files combined at once, at most.
const MAX_FILES: usize = 1_000;

/// Where a split cuts.
#[derive(Debug, Clone, PartialEq)]
pub enum SplitBy {
    /// Every `n` pages.
    Pages(usize),
    /// At each top-level bookmark (pages before the first one form their own part).
    Bookmarks,
    /// Explicit parts: each a list of 0-based pages.
    Ranges(Vec<Vec<usize>>),
}

/// One file a split wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct SplitPart {
    pub path: PathBuf,
    /// 0-based pages of the source.
    pub pages: Vec<usize>,
}

/// Characters a file name cannot hold on any of our platforms.
fn safe_name(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if c.is_control() || "\\/:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(80)
        .collect();
    let t = t.trim().trim_matches('.').to_string();
    if t.is_empty() { "part".into() } else { t }
}

fn read_cos(path: &Path) -> Result<CosDoc> {
    let data = std::fs::read(path).map_err(|e| EngineError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(CosDoc::open(Arc::new(data))?)
}

impl Session {
    /// The page lists a split makes.
    fn split_parts(&self, by: &SplitBy) -> Result<Vec<(String, Vec<usize>)>> {
        let n = self.page_count();
        let parts: Vec<(String, Vec<usize>)> = match by {
            SplitBy::Pages(k) => {
                if *k == 0 {
                    return Err(invalid("pages per file must be at least 1"));
                }
                (0..n)
                    .collect::<Vec<_>>()
                    .chunks(*k)
                    .enumerate()
                    .map(|(i, c)| (format!("{}", i + 1), c.to_vec()))
                    .collect()
            }
            SplitBy::Bookmarks => {
                let mut starts: Vec<(usize, String)> = self
                    .bookmarks()
                    .into_iter()
                    .filter(|b| b.path.len() == 1)
                    .filter_map(|b| b.page.map(|p| (p, b.title)))
                    .collect();
                starts.sort_by_key(|(p, _)| *p);
                starts.dedup_by_key(|(p, _)| *p);
                if starts.is_empty() {
                    return Err(invalid("no top-level bookmark goes to a page; split by pages instead"));
                }
                if starts.first().is_some_and(|(p, _)| *p > 0) {
                    starts.insert(0, (0, "Start".into()));
                }
                let mut out = Vec::new();
                for (i, (p, title)) in starts.iter().enumerate() {
                    let end = starts.get(i + 1).map_or(n, |(q, _)| *q);
                    out.push((format!("{} {}", i + 1, title), (*p..end).collect()));
                }
                out
            }
            SplitBy::Ranges(r) => {
                let mut out = Vec::new();
                for (i, pages) in r.iter().enumerate() {
                    if pages.is_empty() {
                        return Err(invalid(format!("part {} has no pages", i + 1)));
                    }
                    for p in pages {
                        self.page(*p)?;
                    }
                    out.push((format!("{}", i + 1), pages.clone()));
                }
                out
            }
        };
        if parts.len() > MAX_PARTS {
            return Err(invalid(format!("a split makes {MAX_PARTS} files at most")));
        }
        Ok(parts)
    }

    /// Split the document into PDFs in `dir`, named `<stem>-<part>.pdf`. Markups (unsaved ones
    /// included) go with their pages. Existing files of those names are replaced.
    pub fn split_document(&self, dir: &Path, by: &SplitBy) -> Result<Vec<SplitPart>> {
        if !dir.is_dir() {
            return Err(invalid(format!("{} is not a folder", dir.display())));
        }
        let parts = self.split_parts(by)?;
        let stem = self
            .path()
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".into());
        let base = self.current_copy();
        let mut out = Vec::new();
        for (name, pages) in parts {
            let path = dir.join(format!("{}-{}.pdf", safe_name(&stem), safe_name(&name)));
            if same_file(&path, self.path()) {
                return Err(invalid("a split part would overwrite the open document"));
            }
            let mut cos = base.clone();
            let plan = PagePlan::identity(self.page_count()).extract(&pages);
            apply_plan(&mut cos, &plan)?;
            write_doc(&cos, &path)?;
            out.push(SplitPart { path, pages });
        }
        Ok(out)
    }

    /// Replace `targets` (0-based, in order) with `src_pages` of the PDF at `src`: the pages
    /// show the new content while keeping their markups, links and bookmarks. Undoable.
    pub fn replace_pages(&mut self, targets: &[usize], src: &Path, src_pages: &[usize]) -> Result<()> {
        if targets.is_empty() || targets.len() != src_pages.len() {
            return Err(invalid(format!(
                "{} source pages cannot replace {} pages: give as many of each",
                src_pages.len(),
                targets.len()
            )));
        }
        for t in targets {
            self.page(*t)?;
        }
        let other = read_cos(src)?;
        let count = page_objs(&other)?.len();
        if let Some(bad) = src_pages.iter().find(|p| **p >= count) {
            return Err(invalid(format!(
                "page {} is not in {} ({count} pages)",
                bad + 1,
                src.display()
            )));
        }
        self.graph_edit("Replace Pages", |cos, _| {
            pdfcraft_organize::replace_pages(cos, targets, &other, src_pages).map_err(err)?;
            cos.require_full_save();
            Ok(())
        })
    }
}

/// Combine PDFs into one at `out`, in order, with their markups and page labels. With
/// `bookmarks`, the result gets one top-level bookmark per file (its name) instead of the
/// first file's bookmarks. Returns the page count.
pub fn combine_files(files: &[PathBuf], out: &Path, add_bookmarks: bool) -> Result<usize> {
    if files.len() < 2 {
        return Err(invalid("give at least two files to combine"));
    }
    if files.len() > MAX_FILES {
        return Err(invalid(format!("{MAX_FILES} files at most")));
    }
    if files.iter().any(|f| same_file(f, out)) {
        return Err(invalid("the combined file must not be one of the files combined"));
    }
    let (first, rest) = files.split_first().ok_or_else(|| invalid("no files"))?;
    let mut cos = read_cos(first)?;
    if cos.security().is_some() {
        return Err(invalid(format!("{} is password protected", first.display())));
    }
    let n0 = page_objs(&cos)?.len();
    let mut plan = PagePlan::identity(n0);
    let mut starts = vec![(stem(first), 0usize)];
    for f in rest {
        let foreign = ForeignPdf::open(f)?;
        starts.push((stem(f), plan.pages.len()));
        let at = plan.pages.len();
        plan.insert_file(at, foreign, None)?;
    }
    apply_plan(&mut cos, &plan)?;
    if add_bookmarks {
        bookmarks::clear(&mut cos)?;
        bookmarks::append(&mut cos, &starts)?;
    }
    write_doc(&cos, out)?;
    Ok(plan.pages.len())
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_safe() {
        assert_eq!(safe_name("A1.01 / Plan: Level 2"), "A1.01 _ Plan_ Level 2");
        assert_eq!(safe_name(".."), "part");
    }
}
