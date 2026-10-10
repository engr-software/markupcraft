//! More bookmarks: properties (text colour, bold, italic) for one or many, actions (the same
//! targets as links: page with zoom, Place, view rectangle or Space, web address, file),
//! copying a bookmark with its children, AutoMark (bookmarks from the text in a title-block
//! region of each page), bookmark structures (saved folder trees that file bookmarks by their
//! titles), auditing broken bookmarks, and exporting the bookmarks of many PDFs as a CSV or
//! PDF report.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use markupcraft_model::{Color, Rect};
use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, PdfString};
use serde::{Deserialize, Serialize};

use crate::bookmarks::BookmarkItem;
use crate::docutil::{err, name_tree_entries, names_tree, page_objs, text_of};
use crate::links::{LinkLook, LinkTarget, target_entry, target_of};
use crate::{EngineError, Result, Session, invalid};

const MAX_DEPTH: usize = 64;

/// A bookmark's look.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BookmarkStyle {
    pub color: Option<Color>,
    pub bold: bool,
    pub italic: bool,
}

/// A bookmark's details: its look and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct BookmarkDetails {
    pub path: Vec<usize>,
    pub title: String,
    pub style: BookmarkStyle,
    pub target: Option<LinkTarget>,
}

/// The outline item at `path`.
fn item_ref(cos: &CosDoc, path: &[usize]) -> Option<ObjRef> {
    let root = cos.root()?;
    let mut node = cos.get(root).as_dict()?.reference(b"Outlines")?;
    for (depth, &i) in path.iter().enumerate() {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut cur = cos.get(node).as_dict()?.reference(b"First")?;
        let mut seen = HashSet::new();
        for _ in 0..i {
            if !seen.insert(cur) {
                return None;
            }
            cur = cos.get(cur).as_dict()?.reference(b"Next")?;
        }
        node = cur;
    }
    Some(node)
}

fn style_of(cos: &CosDoc, d: &Dict) -> BookmarkStyle {
    let flags = d.get(b"F").and_then(|f| cos.resolve(f).as_int()).unwrap_or(0);
    let color = d.get(b"C").map(|c| cos.resolve(c)).and_then(|c| {
        let v: Vec<f64> = c.as_array()?.iter().filter_map(|x| cos.resolve(x).as_f64()).collect();
        match v.as_slice() {
            [r, g, b] => Some(Color::rgb(*r, *g, *b)),
            _ => None,
        }
    });
    BookmarkStyle {
        color: color.filter(|c| (c.r, c.g, c.b) != (0.0, 0.0, 0.0)),
        italic: flags & 1 != 0,
        bold: flags & 2 != 0,
    }
}

/// One folder of a bookmark structure: its title, which bookmarks it files (titles starting
/// with any of `prefixes`, case-insensitive), and its sub-folders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructureNode {
    pub title: String,
    #[serde(default)]
    pub prefixes: Vec<String>,
    #[serde(default)]
    pub children: Vec<StructureNode>,
}

/// A saved bookmark structure (Bookmarks > Structures).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookmarkStructure {
    pub name: String,
    pub folders: Vec<StructureNode>,
}

pub fn load_structure(path: &Path) -> Result<BookmarkStructure> {
    let io = |e| EngineError::Io {
        path: path.display().to_string(),
        source: e,
    };
    if std::fs::metadata(path).map_err(io)?.len() > 4 << 20 {
        return Err(invalid("the structure file is too large"));
    }
    let t = markupcraft_revu::fsio::read_to_string(path).map_err(io)?;
    serde_json::from_str(&t).map_err(|e| invalid(format!("not a bookmark structure: {e}")))
}

pub fn save_structure(path: &Path, s: &BookmarkStructure) -> Result<()> {
    let t = serde_json::to_string_pretty(s).map_err(|e| invalid(e.to_string()))?;
    crate::write_atomic(path, t.as_bytes())
}

/// A broken bookmark found by Audit Bookmarks.
#[derive(Debug, Clone, PartialEq)]
pub struct BrokenBookmark {
    pub path: Vec<usize>,
    pub title: String,
    pub reason: String,
}

/// How bookmarks are exported.
#[derive(Debug, Clone, PartialEq)]
pub struct BookmarkExport {
    /// Nested (indented tree) or a flat index.
    pub tree: bool,
    /// Only the top level.
    pub top_level_only: bool,
    /// PDF: a link from each line to its page.
    pub links: bool,
    /// A date and time on the report.
    pub date_stamp: bool,
    /// PDF page size in points.
    pub page_size: (f64, f64),
}

impl Default for BookmarkExport {
    fn default() -> Self {
        Self {
            tree: true,
            top_level_only: false,
            links: true,
            date_stamp: true,
            page_size: (612.0, 792.0),
        }
    }
}

impl Session {
    /// A bookmark's look and action.
    pub fn bookmark_details(&self, path: &[usize]) -> Result<BookmarkDetails> {
        let cos = &self.file.cos;
        let r = item_ref(cos, path).ok_or_else(|| invalid(format!("no bookmark at {path:?}")))?;
        let d = cos.dict(&Object::Ref(r)).unwrap_or_default();
        let pages = page_objs(cos)?;
        let has = d.get(b"Dest").is_some() || d.get(b"A").is_some();
        Ok(BookmarkDetails {
            path: path.to_vec(),
            title: text_of(cos, d.get(b"Title")),
            style: style_of(cos, &d),
            target: has.then(|| target_of(cos, &d, &pages)),
        })
    }

    /// Bookmark properties for one or many: text colour (`None` = black), bold, italic.
    pub fn set_bookmark_style(&mut self, paths: &[Vec<usize>], style: BookmarkStyle) -> Result<()> {
        if paths.is_empty() {
            return Err(invalid("choose bookmarks"));
        }
        let refs: Vec<ObjRef> = paths
            .iter()
            .map(|p| item_ref(&self.file.cos, p).ok_or_else(|| invalid(format!("no bookmark at {p:?}"))))
            .collect::<Result<_>>()?;
        self.cos_edit("Bookmark Properties", |cos| {
            for r in refs {
                cos.update_dict(r, |d| {
                    let f = i64::from(style.italic) | (i64::from(style.bold) << 1);
                    if f == 0 {
                        d.remove(b"F");
                    } else {
                        d.set(b"F".to_vec(), Object::Int(f));
                    }
                    match style.color {
                        Some(c) => d.set(
                            b"C".to_vec(),
                            Object::Array(vec![Object::Real(c.r), Object::Real(c.g), Object::Real(c.b)]),
                        ),
                        None => {
                            d.remove(b"C");
                        }
                    }
                })?;
            }
            Ok(((), true))
        })
    }

    /// Bookmark > Action: where the bookmark goes.
    pub fn set_bookmark_action(&mut self, path: &[usize], target: &LinkTarget) -> Result<()> {
        let r = item_ref(&self.file.cos, path).ok_or_else(|| invalid(format!("no bookmark at {path:?}")))?;
        target_entry(&self.file.cos, target)?;
        let target = target.clone();
        self.cos_edit("Bookmark Action", |cos| {
            let (k, v) = target_entry(cos, &target)?;
            cos.update_dict(r, |d| {
                d.remove(b"Dest");
                d.remove(b"A");
                d.set(k.to_vec(), v);
            })?;
            Ok(((), true))
        })
    }

    /// Copy a bookmark (and its children, look and action) to child `index` of `to_parent`.
    /// Returns the copy's path.
    pub fn copy_bookmark(&mut self, from: &[usize], to_parent: &[usize], index: Option<usize>) -> Result<Vec<usize>> {
        let src = item_ref(&self.file.cos, from).ok_or_else(|| invalid(format!("no bookmark at {from:?}")))?;
        if !to_parent.is_empty() && item_ref(&self.file.cos, to_parent).is_none() {
            return Err(invalid(format!("no bookmark at {to_parent:?}")));
        }
        self.cos_edit("Copy Bookmark", |cos| {
            fn copy(cos: &mut CosDoc, src: ObjRef, parent: &[usize], index: usize, depth: usize) -> Result<Vec<usize>> {
                if depth > MAX_DEPTH {
                    return Ok(parent.to_vec());
                }
                let d = cos.dict(&Object::Ref(src)).unwrap_or_default();
                let title = {
                    let t = text_of(cos, d.get(b"Title"));
                    if t.trim().is_empty() { "Bookmark".to_string() } else { t }
                };
                let path = pdfcraft_organize::add_bookmark(cos, parent, index, &title, 0).map_err(err)?;
                let new = item_ref(cos, &path).ok_or_else(|| invalid("the copy was not made"))?;
                cos.update_dict(new, |n| {
                    n.remove(b"Dest");
                    n.remove(b"A");
                    for k in [b"Dest".as_slice(), b"A", b"C", b"F"] {
                        if let Some(v) = d.get(k) {
                            n.set(k.to_vec(), v.clone());
                        }
                    }
                })?;
                let mut kid = d.reference(b"First");
                let mut seen = HashSet::new();
                while let Some(k) = kid {
                    if !seen.insert(k) {
                        break;
                    }
                    copy(cos, k, &path, usize::MAX, depth + 1)?;
                    kid = cos.get(k).as_dict().and_then(|x| x.reference(b"Next"));
                }
                Ok(path)
            }
            let p = copy(cos, src, to_parent, index.unwrap_or(usize::MAX), 0)?;
            Ok((p, true))
        })
    }

    /// AutoMark: one bookmark per page from the text inside `region` (a title block's sheet
    /// number or title); pages with no text there are skipped. `replace` clears the outline
    /// first. Returns how many were added.
    pub fn bookmarks_from_region(&mut self, pages: &[usize], region: Rect, replace: bool) -> Result<usize> {
        for p in pages {
            self.page(*p)?;
        }
        let list: Vec<usize> = if pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            pages.to_vec()
        };
        let mut items = Vec::new();
        for p in list {
            let t = self.text_in_rect(p, region).unwrap_or_default();
            let t: String = t
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(200)
                .collect();
            if !t.is_empty() {
                items.push((p, t));
            }
        }
        if items.is_empty() {
            return Err(invalid("no text in that region on those pages"));
        }
        let n = items.len();
        self.cos_edit("AutoMark", |cos| {
            if replace {
                while item_ref(cos, &[0]).is_some() {
                    pdfcraft_organize::delete_bookmark(cos, &[0]).map_err(err)?;
                }
            }
            for (p, t) in &items {
                pdfcraft_organize::add_bookmark(cos, &[], usize::MAX, t, *p).map_err(err)?;
            }
            Ok(((), true))
        })?;
        Ok(n)
    }

    /// The bookmark tree's titles as a structure (each folder's prefixes empty).
    pub fn bookmark_structure(&self, name: &str) -> BookmarkStructure {
        let items = self.bookmarks();
        fn build(items: &[BookmarkItem], parent: &[usize]) -> Vec<StructureNode> {
            items
                .iter()
                .filter(|b| b.path.len() == parent.len() + 1 && b.path.starts_with(parent) && b.children > 0)
                .map(|b| StructureNode {
                    title: b.title.clone(),
                    prefixes: Vec::new(),
                    children: build(items, &b.path),
                })
                .collect()
        }
        BookmarkStructure {
            name: name.to_string(),
            folders: build(&items, &[]),
        }
    }

    /// Apply a structure: make its folders (reusing top-level ones of the same title) and file
    /// every top-level bookmark whose title starts with a folder's prefix into it. Returns how
    /// many bookmarks were filed.
    pub fn apply_bookmark_structure(&mut self, s: &BookmarkStructure) -> Result<usize> {
        if s.folders.is_empty() {
            return Err(invalid("the structure has no folders"));
        }
        self.cos_edit("Apply Bookmark Structure", |cos| {
            fn titles_at(cos: &CosDoc, parent: &[usize]) -> Vec<String> {
                let mut out = Vec::new();
                let mut i = 0;
                while let Some(r) = item_ref(cos, &[parent, &[i]].concat()) {
                    out.push(
                        cos.dict(&Object::Ref(r))
                            .map(|d| text_of(cos, d.get(b"Title")))
                            .unwrap_or_default(),
                    );
                    i += 1;
                    if i > 100_000 {
                        break;
                    }
                }
                out
            }
            /// Make the folders under `parent`; returns (folder title, its path, its prefixes)
            /// for every folder, deepest last.
            fn make(
                cos: &mut CosDoc,
                nodes: &[StructureNode],
                parent: &[usize],
                out: &mut Vec<(Vec<String>, String)>,
                depth: usize,
            ) -> Result<()> {
                if depth > MAX_DEPTH {
                    return Ok(());
                }
                for n in nodes {
                    let titles = titles_at(cos, parent);
                    let path = match titles.iter().position(|t| t == &n.title) {
                        Some(i) => [parent, &[i]].concat(),
                        None => pdfcraft_organize::add_bookmark(cos, parent, usize::MAX, &n.title, 0).map_err(err)?,
                    };
                    // A folder goes nowhere.
                    if let Some(r) = item_ref(cos, &path) {
                        cos.update_dict(r, |d| {
                            d.remove(b"Dest");
                            d.remove(b"A");
                        })?;
                    }
                    out.push((n.prefixes.clone(), n.title.clone()));
                    make(cos, &n.children, &path, out, depth + 1)?;
                }
                Ok(())
            }
            let mut folders = Vec::new();
            make(cos, &s.folders, &[], &mut folders, 0)?;
            let folder_titles: Vec<String> = folders.iter().map(|f| f.1.clone()).collect();
            // File top-level bookmarks (not the folders) by prefix; the last (deepest) match wins.
            let mut filed = 0;
            let mut i = 0;
            loop {
                let titles = titles_at(cos, &[]);
                let Some(t) = titles.get(i).cloned() else { break };
                let target = if folder_titles.contains(&t) {
                    None
                } else {
                    folders
                        .iter()
                        .rev()
                        .find(|(prefixes, _)| {
                            prefixes
                                .iter()
                                .any(|p| !p.is_empty() && t.to_lowercase().starts_with(&p.to_lowercase()))
                        })
                        .map(|f| f.1.clone())
                };
                let Some(folder) = target else {
                    i += 1;
                    continue;
                };
                // Find the folder's path now (moves shift indices).
                let path = find_title(cos, &folder).ok_or_else(|| invalid("a folder vanished"))?;
                pdfcraft_organize::move_bookmark(cos, &[i], &path, usize::MAX).map_err(err)?;
                filed += 1;
                if filed > 100_000 {
                    break;
                }
            }
            Ok((filed, true))
        })
    }

    /// Audit Bookmarks: bookmarks that go to a page no longer in the file, a missing Place, or
    /// nowhere.
    pub fn audit_bookmarks(&self) -> Vec<BrokenBookmark> {
        let cos = &self.file.cos;
        let pages = page_objs(cos).unwrap_or_default();
        let places: HashSet<String> = names_tree(cos, b"Dests")
            .map(|t| name_tree_entries(cos, &t))
            .unwrap_or_default()
            .into_iter()
            .map(|(k, _)| PdfString::literal(k).to_text())
            .collect();
        let mut out = Vec::new();
        for b in self.bookmarks() {
            let Some(r) = item_ref(cos, &b.path) else { continue };
            let d = cos.dict(&Object::Ref(r)).unwrap_or_default();
            let dest = d.get(b"Dest").cloned().or_else(|| {
                let a = cos.dict(d.get(b"A")?)?;
                (a.name(b"S") == Some(b"GoTo")).then(|| a.get(b"D").cloned()).flatten()
            });
            let reason = match dest.map(|x| cos.resolve(&x)) {
                None if d.get(b"A").is_none() && b.children == 0 => Some("goes nowhere".to_string()),
                None => None,
                Some(o) => match &*o {
                    Object::Array(a) => match a.first() {
                        Some(Object::Ref(p)) if !pages.contains(p) => Some("its page is no longer in the file".into()),
                        Some(Object::Int(i)) if usize::try_from(*i).map_or(true, |i| i >= pages.len()) => {
                            Some("its page is no longer in the file".into())
                        }
                        None => Some("goes nowhere".into()),
                        _ => None,
                    },
                    Object::String(s) if !places.contains(&s.to_text()) => Some(format!("no Place {:?}", s.to_text())),
                    Object::Name(n) if !places.contains(&String::from_utf8_lossy(n).into_owned()) => {
                        Some(format!("no Place {:?}", String::from_utf8_lossy(n)))
                    }
                    _ => None,
                },
            };
            if let Some(reason) = reason {
                out.push(BrokenBookmark {
                    path: b.path.clone(),
                    title: b.title.clone(),
                    reason,
                });
            }
        }
        out
    }
}

fn find_title(cos: &CosDoc, title: &str) -> Option<Vec<usize>> {
    crate::bookmarks::read(cos)
        .into_iter()
        .find(|b| b.title == title)
        .map(|b| b.path)
}

/// A report line: its text, its indent, and the file page it links to.
type ReportLine = (String, f64, Option<(PathBuf, usize)>);

/// Export the bookmarks of `files` to `out`: CSV (File, Level, Title, Page) or a PDF report
/// (`.pdf`): a heading per file, the bookmarks as a tree or flat index, each linked to its
/// page. Returns how many bookmarks were written.
pub fn export_bookmarks(files: &[PathBuf], out: &Path, o: &BookmarkExport) -> Result<usize> {
    if files.is_empty() || files.len() > crate::batch::MAX_FILES {
        return Err(invalid("export the bookmarks of 1 to 2000 files"));
    }
    let mut rows: Vec<(PathBuf, usize, String, Option<usize>)> = Vec::new();
    for f in files {
        let s = Session::open(f)?;
        for b in s.bookmarks() {
            if o.top_level_only && b.path.len() > 1 {
                continue;
            }
            rows.push((f.clone(), b.path.len().saturating_sub(1), b.title, b.page));
        }
    }
    let stamp = o.date_stamp.then(crate::batch_compare::stamp_now);
    let pdf_out = out.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    if !pdf_out {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut csv = String::new();
        if let Some(t) = &stamp {
            csv.push_str(&format!("{}\r\n", q(&format!("Bookmarks {t}"))));
        }
        csv.push_str("File,Level,Title,Page\r\n");
        for (f, level, title, page) in &rows {
            let shown = if o.tree {
                format!("{}{title}", "  ".repeat(*level))
            } else {
                title.clone()
            };
            csv.push_str(&format!(
                "{},{},{},{}\r\n",
                q(&f.display().to_string()),
                level + 1,
                q(&shown),
                page.map(|p| (p + 1).to_string()).unwrap_or_default()
            ));
        }
        crate::write_atomic(out, csv.as_bytes())?;
        return Ok(rows.len());
    }
    use crate::synthetic::{SyntheticPage, pdf, text};
    let (w, h) = o.page_size;
    crate::blank::check_size(w, h)?;
    let per = (((h - 100.0) / 14.0).floor() as usize).max(1);
    let ascii = |s: &str, n: usize| -> String {
        s.chars()
            .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
            .take(n)
            .collect()
    };
    // Lines: a file heading, then its bookmarks.
    let mut lines: Vec<ReportLine> = Vec::new();
    let mut last: Option<&PathBuf> = None;
    for (f, level, title, page) in &rows {
        if last != Some(f) {
            lines.push((
                ascii(
                    &f.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    90,
                ),
                40.0,
                None,
            ));
            last = Some(f);
        }
        let indent = if o.tree {
            56.0 + 14.0 * (*level as f64).min(10.0)
        } else {
            56.0
        };
        let label = match page {
            Some(p) => format!("{}  ...  {}", ascii(title, 90), p + 1),
            None => ascii(title, 90),
        };
        lines.push((label, indent, page.map(|p| (f.clone(), p))));
    }
    let chunks: Vec<&[ReportLine]> = if lines.is_empty() {
        vec![&[]]
    } else {
        lines.chunks(per).collect()
    };
    let mut pages = Vec::new();
    for chunk in &chunks {
        let mut c = text(40.0, h - 45.0, 16.0, "Bookmarks");
        if let Some(t) = &stamp {
            c.push_str(&text(w - 200.0, h - 45.0, 8.0, t));
        }
        for (i, (label, x, _)) in chunk.iter().enumerate() {
            c.push_str(&text(
                *x,
                h - 80.0 - 14.0 * i as f64,
                if *x < 50.0 { 11.0 } else { 9.0 },
                label,
            ));
        }
        pages.push(SyntheticPage::new(w, h, c));
    }
    let mut s = Session::from_bytes(pdf(&pages), out)?;
    if o.links {
        for (k, chunk) in chunks.iter().enumerate() {
            for (i, (label, x, target)) in chunk.iter().enumerate() {
                let Some((f, p)) = target else { continue };
                let y = h - 80.0 - 14.0 * i as f64;
                let width = (label.chars().count() as f64 * 4.8).clamp(10.0, w - x - 20.0);
                let t = LinkTarget::File {
                    path: f.display().to_string(),
                    page: Some(*p),
                };
                s.add_link(
                    k,
                    Rect::new(*x - 1.0, y - 3.0, x + width, y + 10.0),
                    &t,
                    LinkLook::default(),
                )?;
            }
        }
    }
    s.save_as(out, true)?;
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};

    fn sheets() -> Session {
        let page = |n: &str| {
            SyntheticPage::new(
                612.0,
                792.0,
                format!("{}{}", text(500.0, 40.0, 14.0, n), text(72.0, 700.0, 12.0, "PLAN")),
            )
        };
        Session::from_bytes(pdf(&[page("A-101"), page("A-102"), page("M-201")]), "b.pdf").unwrap()
    }

    #[test]
    fn automark_structure_style_action_copy_and_audit() {
        let mut s = sheets();
        let n = s
            .bookmarks_from_region(&[], Rect::new(480.0, 30.0, 600.0, 60.0), true)
            .unwrap();
        assert_eq!(n, 3);
        let titles: Vec<String> = s.bookmarks().iter().map(|b| b.title.clone()).collect();
        assert_eq!(titles, ["A-101", "A-102", "M-201"]);
        let st = BookmarkStructure {
            name: "Disciplines".into(),
            folders: vec![
                StructureNode {
                    title: "Architectural".into(),
                    prefixes: vec!["A-".into()],
                    children: vec![],
                },
                StructureNode {
                    title: "Mechanical".into(),
                    prefixes: vec!["M-".into()],
                    children: vec![],
                },
            ],
        };
        assert_eq!(s.apply_bookmark_structure(&st).unwrap(), 3);
        let b = s.bookmarks();
        let top: Vec<&str> = b
            .iter()
            .filter(|x| x.path.len() == 1)
            .map(|x| x.title.as_str())
            .collect();
        assert_eq!(top, ["Architectural", "Mechanical"]);
        assert_eq!(b.iter().find(|x| x.title == "M-201").unwrap().path, vec![1, 0]);
        assert_eq!(s.bookmark_structure("x").folders.len(), 2);
        // Style for many; action; copy.
        s.set_bookmark_style(
            &[vec![0], vec![1]],
            BookmarkStyle {
                color: Some(Color::rgb(1.0, 0.0, 0.0)),
                bold: true,
                italic: false,
            },
        )
        .unwrap();
        let d = s.bookmark_details(&[0]).unwrap();
        assert!(d.style.bold && !d.style.italic && d.style.color.is_some());
        s.set_bookmark_action(&[0, 0], &LinkTarget::Url("https://example.com".into()))
            .unwrap();
        assert_eq!(
            s.bookmark_details(&[0, 0]).unwrap().target,
            Some(LinkTarget::Url("https://example.com".into()))
        );
        let copy = s.copy_bookmark(&[1], &[], None).unwrap();
        assert_eq!(copy, vec![2]);
        let b = s.bookmarks();
        assert_eq!(
            b.iter().filter(|x| x.title == "M-201").count(),
            2,
            "copied with its child"
        );
        assert!(s.bookmark_details(&[2]).unwrap().style.bold, "and its look");
        // Audit: a bookmark to a missing Place.
        assert!(s.audit_bookmarks().is_empty(), "{:?}", s.audit_bookmarks());
        s.set_bookmark_action(&[1, 0], &LinkTarget::Place("Gone".into()))
            .unwrap();
        let broken = s.audit_bookmarks();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].path, vec![1, 0]);
    }

    #[test]
    fn export_bookmarks_csv_and_pdf() {
        let d = std::env::temp_dir().join(format!("markupcraft-bmexport-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("set.pdf");
        let mut s = sheets();
        s.bookmarks_from_region(&[], Rect::new(480.0, 30.0, 600.0, 60.0), true)
            .unwrap();
        s.save_as(&f, true).unwrap();
        let csv = d.join("bm.csv");
        assert_eq!(
            export_bookmarks(std::slice::from_ref(&f), &csv, &BookmarkExport::default()).unwrap(),
            3
        );
        let t = std::fs::read_to_string(&csv).unwrap();
        assert!(t.contains("\"A-101\",1"), "{t}");
        let p = d.join("bm.pdf");
        export_bookmarks(&[f], &p, &BookmarkExport::default()).unwrap();
        let r = Session::open(&p).unwrap();
        assert_eq!(r.links().len(), 3);
        assert!(r.page_text(0).unwrap().contains("A-102"));
    }
}
