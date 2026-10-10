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
    /// Parts no larger than this many bytes (as many pages as fit).
    Size(u64),
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
            SplitBy::Size(_) => {
                return Err(invalid("a split by file size needs split_document_with"));
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
    combine_file_ranges(files, &[], out, add_bookmarks)
}

/// Combine as [`combine_files`], each file with its own pages: `pages[i]` (0-based, in the
/// order given) for `files[i]`, all its pages when `None` or missing.
pub fn combine_file_ranges(
    files: &[PathBuf],
    pages: &[Option<Vec<usize>>],
    out: &Path,
    add_bookmarks: bool,
) -> Result<usize> {
    if files.len() < 2 && pages.iter().flatten().next().is_none() {
        return Err(invalid("give at least two files to combine"));
    }
    if files.is_empty() {
        return Err(invalid("give the files to combine"));
    }
    if files.len() > MAX_FILES {
        return Err(invalid(format!("{MAX_FILES} files at most")));
    }
    let pick = |i: usize, n: usize, f: &Path| -> Result<Option<Vec<usize>>> {
        match pages.get(i).cloned().flatten() {
            None => Ok(None),
            Some(list) => {
                if list.is_empty() {
                    return Err(invalid(format!("{}: the page range is empty", f.display())));
                }
                if let Some(bad) = list.iter().find(|p| **p >= n) {
                    return Err(invalid(format!("{} has no page {} ({n} pages)", f.display(), bad + 1)));
                }
                Ok(Some(list))
            }
        }
    };
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
    if let Some(list) = pick(0, n0, first)? {
        plan.pages = list.iter().filter_map(|p| plan.pages.get(*p).copied()).collect();
    }
    let mut starts = vec![(stem(first), 0usize)];
    for (k, f) in rest.iter().enumerate() {
        let foreign = ForeignPdf::open(f)?;
        let list = pick(k + 1, foreign.page_count(), f)?;
        starts.push((stem(f), plan.pages.len()));
        let at = plan.pages.len();
        plan.insert_file(at, foreign, list.as_deref())?;
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

/// Most pages a split by file size weighs.
const MAX_SIZE_SPLIT_PAGES: usize = 5_000;

/// Split options beyond where to cut (Document > Split).
#[derive(Debug, Clone, PartialEq)]
pub struct SplitOptions {
    pub by: SplitBy,
    /// Text before the name; `#` stands for the part number (`###` pads it to 3 digits).
    pub prefix: String,
    /// Text after the name; `#` as in `prefix`.
    pub suffix: String,
    /// Name each part after its top-level bookmark (splitting by bookmarks), not the file.
    pub bookmark_names: bool,
    /// Put the parts in a subfolder named after the document.
    pub subfolder: bool,
    /// Links to a page that went to another part open that part at the page.
    pub update_links: bool,
    /// Remove layers no markup or page content of the part uses.
    pub drop_empty_layers: bool,
}

impl SplitOptions {
    pub fn new(by: SplitBy) -> Self {
        Self {
            by,
            prefix: String::new(),
            suffix: String::new(),
            bookmark_names: false,
            subfolder: false,
            update_links: false,
            drop_empty_layers: false,
        }
    }
}

/// `#` runs replaced by the part number, padded to the run's length.
fn number_runs(t: &str, n: usize) -> String {
    let mut out = String::new();
    let mut run = 0usize;
    let flush = |out: &mut String, run: &mut usize| {
        if *run > 0 {
            out.push_str(&format!("{n:0width$}", width = (*run).min(9)));
            *run = 0;
        }
    };
    for c in t.chars() {
        if c == '#' {
            run += 1;
        } else {
            flush(&mut out, &mut run);
            out.push(c);
        }
    }
    flush(&mut out, &mut run);
    out
}

impl Session {
    /// The pages of each part when no part may be larger than `limit` bytes (a single page
    /// larger than that is a part of its own).
    fn size_parts(&self, base: &CosDoc, limit: u64) -> Result<Vec<(String, Vec<usize>)>> {
        if limit < 1024 {
            return Err(invalid("a part must be allowed at least 1 KB"));
        }
        let n = self.page_count();
        if n > MAX_SIZE_SPLIT_PAGES {
            return Err(invalid(format!(
                "splitting by size weighs {MAX_SIZE_SPLIT_PAGES} pages at most; split by pages first"
            )));
        }
        let size_of = |pages: &[usize]| -> Result<u64> {
            let mut cos = base.clone();
            let plan = PagePlan::identity(n).extract(pages);
            apply_plan(&mut cos, &plan)?;
            let bytes = markupcraft_revu::cos::write_full(&cos, &crate::docutil::save_options())?;
            Ok(bytes.len() as u64)
        };
        let mut parts = Vec::new();
        let mut start = 0usize;
        while start < n {
            let mut end = start + 1;
            while end < n {
                let pages: Vec<usize> = (start..=end).collect();
                if size_of(&pages)? > limit {
                    break;
                }
                end += 1;
            }
            parts.push((format!("{}", parts.len() + 1), (start..end).collect()));
            start = end;
            if parts.len() > MAX_PARTS {
                return Err(invalid(format!("a split makes {MAX_PARTS} files at most")));
            }
        }
        Ok(parts)
    }

    /// Split with naming and folder options. Returns the files written.
    pub fn split_document_with(&self, dir: &Path, o: &SplitOptions) -> Result<Vec<SplitPart>> {
        if !dir.is_dir() {
            return Err(invalid(format!("{} is not a folder", dir.display())));
        }
        let base = self.current_copy();
        let parts = match o.by {
            SplitBy::Size(limit) => self.size_parts(&base, limit)?,
            _ => self.split_parts(&o.by)?,
        };
        let stem = self
            .path()
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".into());
        let dir = if o.subfolder {
            let d = dir.join(safe_name(&stem));
            std::fs::create_dir_all(&d).map_err(|e| EngineError::Io {
                path: d.display().to_string(),
                source: e,
            })?;
            d
        } else {
            dir.to_path_buf()
        };
        let plain = o.prefix.is_empty() && o.suffix.is_empty() && !o.bookmark_names;
        let numbered = o.prefix.contains('#') || o.suffix.contains('#');
        let mut used: Vec<String> = Vec::new();
        let mut names = Vec::new();
        for (i, (name, _)) in parts.iter().enumerate() {
            let n = i + 1;
            let file = if plain {
                format!("{}-{}", safe_name(&stem), safe_name(name))
            } else {
                let core = if o.bookmark_names && matches!(o.by, SplitBy::Bookmarks) {
                    // the part name is "<n> <title>"
                    name.split_once(' ').map_or(name.as_str(), |x| x.1).to_string()
                } else {
                    stem.clone()
                };
                let mut f = format!("{}{core}{}", number_runs(&o.prefix, n), number_runs(&o.suffix, n));
                if !numbered && !(o.bookmark_names && matches!(o.by, SplitBy::Bookmarks)) {
                    f.push_str(&format!("-{n}"));
                }
                safe_name(&f)
            };
            let mut unique = file.clone();
            let mut k = 2;
            while used.iter().any(|u| u.eq_ignore_ascii_case(&unique)) {
                unique = format!("{file} ({k})");
                k += 1;
            }
            used.push(unique.clone());
            names.push(dir.join(format!("{unique}.pdf")));
        }
        // which part (and page in it) each source page went to
        let mut home: Vec<Option<(usize, usize)>> = vec![None; self.page_count()];
        for (pi, (_, pages)) in parts.iter().enumerate() {
            for (k, p) in pages.iter().enumerate() {
                if let Some(h) = home.get_mut(*p) {
                    *h = Some((pi, k));
                }
            }
        }
        let mut out = Vec::new();
        for (pi, ((_, pages), path)) in parts.iter().zip(&names).enumerate() {
            if same_file(path, self.path()) {
                return Err(invalid("a split part would overwrite the open document"));
            }
            let mut cos = base.clone();
            if o.update_links {
                retarget_links(&mut cos, pages, pi, &home, &names)?;
            }
            let plan = PagePlan::identity(self.page_count()).extract(pages);
            apply_plan(&mut cos, &plan)?;
            write_doc(&cos, path)?;
            if o.drop_empty_layers {
                drop_unused_layers(path)?;
            }
            out.push(SplitPart {
                path: path.clone(),
                pages: pages.clone(),
            });
        }
        Ok(out)
    }
}

/// On the pages of part `part`, links to a page that went to another part become links to
/// that part's file at the page.
fn retarget_links(
    cos: &mut CosDoc,
    pages: &[usize],
    part: usize,
    home: &[Option<(usize, usize)>],
    names: &[PathBuf],
) -> Result<()> {
    use crate::links::LinkTarget;
    use markupcraft_revu::cos::Object;
    let objs = page_objs(cos)?;
    for p in pages {
        let Some(page_ref) = objs.get(*p).copied() else {
            continue;
        };
        for a in crate::docutil::annots_of(cos, page_ref) {
            let Object::Ref(r) = a else { continue };
            let Some(d) = cos.dict(&a) else { continue };
            if d.name(b"Subtype") != Some(b"Link") {
                continue;
            }
            let to = match crate::links::target_of(cos, &d, &objs) {
                LinkTarget::Page(q) | LinkTarget::Zoomed { page: q, .. } | LinkTarget::View { page: q, .. } => q,
                _ => continue,
            };
            let Some(Some((other, at))) = home.get(to) else {
                continue;
            };
            if *other == part {
                continue;
            }
            let Some(file) = names.get(*other).and_then(|f| f.file_name()) else {
                continue;
            };
            let target = LinkTarget::File {
                path: file.to_string_lossy().into_owned(),
                page: Some(*at),
            };
            let (k, v) = crate::links::target_entry(cos, &target)?;
            cos.update_dict(r, |d| {
                d.remove(b"Dest");
                d.remove(b"A");
                d.set(k.to_vec(), v);
            })?;
        }
    }
    Ok(())
}

/// Delete the layers of the PDF at `path` that neither a markup nor page content uses.
fn drop_unused_layers(path: &Path) -> Result<()> {
    let mut s = Session::open(path)?;
    let mut used: Vec<String> = s.doc().markups.iter().map(|m| m.layer.clone()).collect();
    for p in 0..s.page_count() {
        used.extend(s.layers_on_page(p).unwrap_or_default());
    }
    let mut changed = false;
    for l in s.layers() {
        if !used.contains(&l.name) && s.delete_layer(&l.name, false).is_ok() {
            changed = true;
        }
    }
    if changed {
        s.save(true)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_safe() {
        assert_eq!(safe_name("A1.01 / Plan: Level 2"), "A1.01 _ Plan_ Level 2");
        assert_eq!(safe_name(".."), "part");
        assert_eq!(number_runs("Part ###", 7), "Part 007");
        assert_eq!(number_runs("#-x", 12), "12-x");
    }
}
