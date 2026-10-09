//! More of the Search panel: what else a search looks in (markup text, the file name, the
//! document properties, form field values), searching other files (open documents, a Set, a
//! folder and its subfolders), and Replace: found text replaced in the page content (not in
//! markups), in the line's own font when it can show the new text, else in Helvetica (or the
//! line is skipped, as asked).

use std::path::{Path, PathBuf};

use markupcraft_model::{Markup, Rect};

use crate::search::SearchOptions;
use crate::{Result, Session, invalid};

/// Most lines one Replace rewrites.
pub const MAX_REPLACE: usize = 10_000;

/// What a search looks in besides the page text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchTargets {
    pub page_text: bool,
    pub markups: bool,
    pub file_name: bool,
    pub properties: bool,
    pub form_fields: bool,
}

impl Default for SearchTargets {
    fn default() -> Self {
        Self {
            page_text: true,
            markups: false,
            file_name: false,
            properties: false,
            form_fields: false,
        }
    }
}

/// Where a hit was found.
#[derive(Debug, Clone, PartialEq)]
pub enum HitSource {
    PageText,
    /// A markup (its id).
    Markup(String),
    FileName,
    /// A document property (its name).
    Property(String),
    /// A form field (its name).
    FormField(String),
}

impl HitSource {
    pub fn label(&self) -> String {
        match self {
            HitSource::PageText => "text".into(),
            HitSource::Markup(_) => "markup".into(),
            HitSource::FileName => "file name".into(),
            HitSource::Property(k) => format!("property {k}"),
            HitSource::FormField(n) => format!("field {n}"),
        }
    }
}

/// One hit of [`Session::search_all`].
#[derive(Debug, Clone, PartialEq)]
pub struct FoundHit {
    pub source: HitSource,
    /// The page (none for the file name and properties).
    pub page: Option<usize>,
    pub text: String,
    pub context: String,
    pub rects: Vec<Rect>,
}

/// The character ranges where `needle` occurs in `hay` (case-insensitive unless `case`; whole
/// words only when `whole`).
pub fn find_ranges(hay: &str, needle: &str, case: bool, whole: bool) -> Vec<std::ops::Range<usize>> {
    let fold = |c: char| if case { c } else { c.to_lowercase().next().unwrap_or(c) };
    let h: Vec<char> = hay.chars().collect();
    let n: Vec<char> = needle.chars().map(fold).collect();
    let mut out = Vec::new();
    if n.is_empty() || n.len() > h.len() {
        return out;
    }
    let word = |i: Option<&char>| i.is_some_and(|c| c.is_alphanumeric());
    let mut i = 0;
    while i + n.len() <= h.len() {
        let hit = h
            .get(i..i + n.len())
            .is_some_and(|w| w.iter().zip(&n).all(|(a, b)| fold(*a) == *b));
        let bounded = !whole || (!word(i.checked_sub(1).and_then(|k| h.get(k))) && !word(h.get(i + n.len())));
        if hit && bounded {
            out.push(i..i + n.len());
            i += n.len();
        } else {
            i += 1;
        }
    }
    out
}

/// Does `needle` occur in `hay`?
pub fn text_matches(hay: &str, needle: &str, case: bool, whole: bool) -> bool {
    !find_ranges(hay, needle, case, whole).is_empty()
}

/// `hay` with every occurrence replaced.
pub fn replace_in(hay: &str, needle: &str, with: &str, case: bool, whole: bool) -> (String, usize) {
    let ranges = find_ranges(hay, needle, case, whole);
    let chars: Vec<char> = hay.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    for r in &ranges {
        out.extend(chars.get(at..r.start).unwrap_or(&[]));
        out.push_str(with);
        at = r.end;
    }
    out.extend(chars.get(at..).unwrap_or(&[]));
    (out, ranges.len())
}

fn markup_text(m: &Markup, needle: &str, case: bool, whole: bool) -> Option<String> {
    [&m.contents, &m.subject, &m.label, &m.author]
        .into_iter()
        .find(|t| text_matches(t, needle, case, whole))
        .map(|t| t.chars().take(120).collect())
}

/// One file's results of [`search_files`].
#[derive(Debug, Clone, PartialEq)]
pub struct FileHits {
    pub path: PathBuf,
    pub hits: Vec<FoundHit>,
    /// Why it could not be searched.
    pub error: Option<String>,
}

/// What [`Session::replace_text`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplaceReport {
    /// Occurrences replaced and lines rewritten.
    pub replaced: usize,
    pub lines: usize,
    /// Lines set in Helvetica because their font could not show the new text.
    pub substituted: usize,
    /// Lines left alone (their font could not show it and fallback was off, or an error).
    pub skipped: usize,
}

impl Session {
    /// Search the page text and, as asked, markups, the file name, the document properties and
    /// form field values.
    pub fn search_all(&self, needle: &str, opts: &SearchOptions, t: &SearchTargets) -> Result<Vec<FoundHit>> {
        let needle_t = needle.trim();
        if needle_t.is_empty() {
            return Err(invalid("type the text to search for"));
        }
        let (case, whole) = (opts.case_sensitive, opts.whole_words);
        let pages: Vec<usize> = opts.pages.clone().unwrap_or_else(|| (0..self.page_count()).collect());
        let mut out = Vec::new();
        if t.file_name {
            let name = self
                .path()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if text_matches(&name, needle_t, case, whole) {
                out.push(FoundHit {
                    source: HitSource::FileName,
                    page: None,
                    text: name.clone(),
                    context: self.path().display().to_string(),
                    rects: Vec::new(),
                });
            }
        }
        if t.properties {
            let p = self.doc_properties();
            for (k, v) in p.standard.iter().chain(&p.custom) {
                if text_matches(v, needle_t, case, whole) {
                    out.push(FoundHit {
                        source: HitSource::Property(k.clone()),
                        page: None,
                        text: v.chars().take(120).collect(),
                        context: k.clone(),
                        rects: Vec::new(),
                    });
                }
            }
        }
        if t.page_text {
            let r = self.search_text(needle_t, opts)?;
            out.extend(r.hits.into_iter().map(|h| FoundHit {
                source: HitSource::PageText,
                page: Some(h.page),
                text: h.text,
                context: h.context,
                rects: h.rects,
            }));
        }
        if t.markups {
            for m in self.doc.markups.iter().filter(|m| pages.contains(&m.page)) {
                if let Some(text) = markup_text(m, needle_t, case, whole) {
                    out.push(FoundHit {
                        source: HitSource::Markup(m.id.clone()),
                        page: Some(m.page),
                        text: m.subject.clone(),
                        context: text,
                        rects: vec![m.rect],
                    });
                }
            }
        }
        if t.form_fields {
            for f in self.form_fields() {
                if f.page.is_some_and(|p| !pages.contains(&p)) {
                    continue;
                }
                let value = f.value.join(", ");
                if text_matches(&value, needle_t, case, whole) {
                    out.push(FoundHit {
                        source: HitSource::FormField(f.name.clone()),
                        page: f.page,
                        text: value.chars().take(120).collect(),
                        context: f.name.clone(),
                        rects: f.rect.into_iter().collect(),
                    });
                }
            }
        }
        Ok(out)
    }

    /// Replace `needle` with `with` in the page text (`opts` pages, case, whole words). Only
    /// the lines touching `only` (page, rectangle) when given (the checked results). Lines
    /// whose font cannot show the new text are set in Helvetica when `fallback`, else skipped.
    /// One undoable step.
    pub fn replace_text(
        &mut self,
        needle: &str,
        with: &str,
        opts: &SearchOptions,
        only: Option<&[(usize, Rect)]>,
        fallback: bool,
    ) -> Result<ReplaceReport> {
        let needle = needle.trim().to_string();
        if needle.is_empty() {
            return Err(invalid("type the text to replace"));
        }
        if with.chars().count() > 1_000 || needle.chars().count() > 1_000 {
            return Err(invalid("the text is too long (1000 characters at most)"));
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
        let (case, whole) = (opts.case_sensitive, opts.whole_words);
        let only: Option<Vec<(usize, Rect)>> = only.map(<[_]>::to_vec);
        self.graph_edit("Replace Text", |cos, _| {
            let mut rep = ReplaceReport::default();
            for page in pages {
                let mut next = 0usize;
                loop {
                    if rep.lines + rep.skipped >= MAX_REPLACE {
                        return Ok(rep);
                    }
                    let lines = pdfcraft_edit::text_lines(cos, page).map_err(crate::docutil::err)?;
                    let found = lines.iter().enumerate().skip(next).find(|(_, l)| {
                        text_matches(&l.text, &needle, case, whole)
                            && only.as_ref().is_none_or(|o| {
                                o.iter().any(|(p, r)| {
                                    *p == page
                                        && r.x0 <= l.rect[2]
                                        && l.rect[0] <= r.x1
                                        && r.y0 <= l.rect[3]
                                        && l.rect[1] <= r.y1
                                })
                            })
                    });
                    let Some((idx, line)) = found else { break };
                    next = idx + 1;
                    let (new_text, n) = replace_in(&line.text, &needle, with, case, whole);
                    let before = cos.clone();
                    match pdfcraft_edit::replace_line(cos, page, idx, &new_text) {
                        Ok(e) if e.substituted.is_some() && !fallback => {
                            *cos = before;
                            rep.skipped += 1;
                        }
                        Ok(e) => {
                            rep.lines += 1;
                            rep.replaced += n;
                            if e.substituted.is_some() {
                                rep.substituted += 1;
                            }
                        }
                        Err(_) => {
                            *cos = before;
                            rep.skipped += 1;
                        }
                    }
                }
            }
            if rep.lines == 0 && rep.skipped == 0 {
                return Err(invalid(format!("\"{needle}\" is not in the page text")));
            }
            Ok(rep)
        })
    }
}

/// Search other files (each opened read-only; files that cannot be read are reported).
pub fn search_files(paths: &[PathBuf], needle: &str, opts: &SearchOptions, t: &SearchTargets) -> Vec<FileHits> {
    paths
        .iter()
        .take(crate::batch::MAX_FILES)
        .map(|p| {
            let r = Session::open(p).and_then(|s| {
                let o = SearchOptions {
                    pages: None,
                    ..opts.clone()
                };
                s.search_all(needle, &o, t)
            });
            match r {
                Ok(hits) => FileHits {
                    path: p.clone(),
                    hits,
                    error: None,
                },
                Err(e) => FileHits {
                    path: p.clone(),
                    hits: Vec::new(),
                    error: Some(e.to_string()),
                },
            }
        })
        .collect()
}

/// The PDFs in a folder (and its subfolders when `recursive`).
pub fn folder_pdfs(dir: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Err(invalid(format!("{} is not a folder", dir.display())));
    }
    crate::batch_compare::collect_pdfs(&[dir.to_path_buf()], recursive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};

    #[test]
    fn ranges_respect_case_and_words() {
        assert_eq!(find_ranges("Door door DOORS", "door", false, false).len(), 3);
        assert_eq!(find_ranges("Door door DOORS", "door", false, true).len(), 2);
        assert_eq!(find_ranges("Door door DOORS", "door", true, false), vec![5..9]);
        assert_eq!(
            replace_in("Room 101, room 102", "room", "Suite", false, true),
            ("Suite 101, Suite 102".into(), 2)
        );
    }

    #[test]
    fn replace_rewrites_the_page_text_and_undoes() {
        let c = format!(
            "{}{}",
            text(72.0, 700.0, 14.0, "ROOM 101 OFFICE"),
            text(72.0, 650.0, 14.0, "ROOM 102 STORAGE")
        );
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, c)]), "r.pdf").unwrap();
        let r = s
            .replace_text("ROOM", "SUITE", &SearchOptions::default(), None, true)
            .unwrap();
        assert_eq!((r.replaced, r.lines), (2, 2), "{r:?}");
        let t = s.page_text(0).unwrap();
        assert!(t.contains("SUITE 101 OFFICE") && !t.contains("ROOM"), "{t}");
        s.undo().unwrap();
        assert!(s.page_text(0).unwrap().contains("ROOM 101"));
        // Only the checked hit (the second line).
        let only = [(0usize, Rect::new(70.0, 645.0, 200.0, 665.0))];
        let r = s
            .replace_text("ROOM", "SUITE", &SearchOptions::default(), Some(&only), true)
            .unwrap();
        assert_eq!(r.lines, 1);
        let t = s.page_text(0).unwrap();
        assert!(t.contains("ROOM 101") && t.contains("SUITE 102"), "{t}");
        assert!(
            s.replace_text("NOPE", "X", &SearchOptions::default(), None, true)
                .is_err()
        );
    }

    #[test]
    fn search_all_looks_in_names_properties_and_markups() {
        let mut s = Session::from_bytes(
            pdf(&[SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, "PUMP P-1"))]),
            "pump-schedule.pdf",
        )
        .unwrap();
        s.set_doc_properties(&[("Title".into(), Some("Pump room".into()))])
            .unwrap();
        let mut m = Markup::new(markupcraft_model::Kind::Rectangle, 0, Vec::new());
        m.pts = vec![
            markupcraft_geom::Point::new(10.0, 10.0),
            markupcraft_geom::Point::new(50.0, 50.0),
        ];
        m.contents = "check the pump".into();
        s.add_markup(m).unwrap();
        let t = SearchTargets {
            page_text: true,
            markups: true,
            file_name: true,
            properties: true,
            form_fields: true,
        };
        let hits = s.search_all("pump", &SearchOptions::default(), &t).unwrap();
        let kinds: Vec<String> = hits.iter().map(|h| h.source.label()).collect();
        assert!(kinds.contains(&"file name".to_string()), "{kinds:?}");
        assert!(kinds.contains(&"property Title".to_string()), "{kinds:?}");
        assert!(
            kinds.contains(&"text".to_string()) && kinds.contains(&"markup".to_string()),
            "{kinds:?}"
        );
    }
}
