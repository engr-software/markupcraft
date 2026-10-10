//! Batch tools and Sets: many PDFs at once.
//!
//! - **Sets**: a named list of PDFs navigated as one drawing set without merging them. Our own
//!   JSON format (`.pcset`): `{ "format": "markupcraft-set", "version": 1, "name": "...",
//!   "files": ["sheets/A.pdf", ...] }`, paths relative to the set file's folder when inside it.
//! - **Batch Summary**: the Markups List of many files as one CSV with a File column.
//! - **Batch Link**: wherever a page's text shows the label (sheet number) of a page in the
//!   set, add a link to that page (in the same file, or a page of another file).
//! - **Slip Sheet**: replace the pages of an old set with the revised pages of a new file whose
//!   labels match, keeping the old pages' markups; new sheets that match nothing go at the end.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use markupcraft_model::{Color, Rect};
use serde::{Deserialize, Serialize};

use crate::links::{LinkLook, LinkTarget};
use crate::{EngineError, Result, Session, invalid};

pub const SET_FORMAT: &str = "markupcraft-set";
/// Most files in one set or batch.
pub const MAX_FILES: usize = 2_000;
/// Most links one Batch Link run adds.
pub const MAX_LINKS: usize = 200_000;
/// A label found on a page: (page, box, label).
pub type LabelHit = (usize, Rect, String);
/// Words joined to match a label with spaces.
const MAX_LABEL_WORDS: usize = 4;

// ---- sets --------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawingSet {
    pub name: String,
    /// absolute (or as given) paths
    pub files: Vec<PathBuf>,
    /// Custom sheet tags: `<file name>#<page from 1>` -> {tag: value} (Set > Edit Tags).
    pub tags: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SetFile {
    format: String,
    version: u32,
    #[serde(default)]
    name: String,
    files: Vec<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    tags: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> EngineError + '_ {
    move |e| EngineError::Io {
        path: path.display().to_string(),
        source: e,
    }
}

/// Read a set file (paths made absolute against its folder).
pub fn load_set(path: &Path) -> Result<DrawingSet> {
    if std::fs::metadata(path).map_err(io(path))?.len() > 16 << 20 {
        return Err(invalid("the set file is too large"));
    }
    let text = std::fs::read_to_string(path).map_err(io(path))?;
    let f: SetFile = serde_json::from_str(&text).map_err(|e| invalid(format!("not a set file: {e}")))?;
    if f.format != SET_FORMAT {
        return Err(invalid(format!("not a MarkupCraft set (format {:?})", f.format)));
    }
    if f.files.len() > MAX_FILES {
        return Err(invalid(format!("a set holds at most {MAX_FILES} files")));
    }
    let base = path.parent().unwrap_or(Path::new(""));
    Ok(DrawingSet {
        tags: f.tags,
        name: f.name,
        files: f
            .files
            .iter()
            .map(|p| {
                let p = PathBuf::from(p);
                if p.is_absolute() { p } else { base.join(p) }
            })
            .collect(),
    })
}

/// Write a set file (atomic); files inside its folder are stored relative to it.
pub fn save_set(path: &Path, set: &DrawingSet) -> Result<()> {
    if set.files.is_empty() || set.files.len() > MAX_FILES {
        return Err(invalid(format!("a set holds 1 to {MAX_FILES} files")));
    }
    let base = path
        .parent()
        .map(|b| b.canonicalize().unwrap_or_else(|_| b.to_path_buf()));
    let files = set
        .files
        .iter()
        .map(|f| {
            let abs = f.canonicalize().unwrap_or_else(|_| f.clone());
            match base.as_ref().and_then(|b| abs.strip_prefix(b).ok()) {
                Some(rel) => rel.to_string_lossy().replace('\\', "/"),
                None => abs.to_string_lossy().into_owned(),
            }
        })
        .collect();
    let f = SetFile {
        format: SET_FORMAT.into(),
        version: 1,
        name: set.name.clone(),
        files,
        tags: set.tags.clone(),
    };
    let text = serde_json::to_string_pretty(&f).map_err(|e| invalid(e.to_string()))?;
    crate::write_atomic(path, text.as_bytes())
}

/// One sheet of a set.
#[derive(Debug, Clone, PartialEq)]
pub struct SetSheet {
    /// index into the set's files
    pub file: usize,
    /// 0-based
    pub page: usize,
    /// page label ("" = none)
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetSort {
    FileOrder,
    Label,
    FileThenLabel,
}

/// Natural order ("A-2" before "A-10").
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |s: &str| {
        let mut out: Vec<(String, u64)> = Vec::new();
        let mut text = String::new();
        let mut num = String::new();
        for c in s.to_lowercase().chars() {
            if c.is_ascii_digit() {
                num.push(c);
            } else {
                if !num.is_empty() {
                    out.push((std::mem::take(&mut text), num.parse().unwrap_or(u64::MAX)));
                    num.clear();
                }
                text.push(c);
            }
        }
        out.push((text, num.parse().unwrap_or(0)));
        out
    };
    key(a).cmp(&key(b))
}

/// Every sheet of every file (files that cannot be read are named in the errors).
pub fn set_sheets(set: &DrawingSet, sort: SetSort) -> (Vec<SetSheet>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for (fi, f) in set.files.iter().enumerate() {
        match markupcraft_revu::open(f) {
            Ok((file, doc)) => {
                let labels = crate::labels::read(&file.cos, doc.pages.len());
                for p in 0..doc.pages.len() {
                    let label = labels
                        .as_ref()
                        .and_then(|l| l.get(p).cloned().flatten())
                        .map(|s| s.text())
                        .unwrap_or_default();
                    out.push(SetSheet {
                        file: fi,
                        page: p,
                        label,
                    });
                }
            }
            Err(e) => errors.push(format!("{}: {e}", f.display())),
        }
    }
    match sort {
        SetSort::FileOrder => {}
        SetSort::Label => out.sort_by(|a, b| natural(&a.label, &b.label)),
        SetSort::FileThenLabel => out.sort_by(|a, b| a.file.cmp(&b.file).then(natural(&a.label, &b.label))),
    }
    (out, errors)
}

// ---- batch summary -------------------------------------------------------------------------------

fn csv_q(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// The Markups List of many files as one CSV (a File column first). Returns the CSV, the
/// markup count and the files that could not be read.
/// Rows of CSV text (quoted cells, doubled quotes, CRLF or LF).
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, c) => cell.push(c),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut cell)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

/// Batch Summary as an Excel workbook (one sheet, a File column first).
pub fn batch_summary_xlsx(files: &[PathBuf], measurements_only: bool) -> (Vec<u8>, usize, Vec<String>) {
    let (csv, n, errors) = batch_summary_csv(files, measurements_only);
    let table = crate::convert::TextTable { rows: parse_csv(&csv) };
    (crate::convert::xlsx(&[("Summary".into(), table)]), n, errors)
}

pub fn batch_summary_csv(files: &[PathBuf], measurements_only: bool) -> (String, usize, Vec<String>) {
    let mut out = String::new();
    let mut n = 0;
    let mut errors = Vec::new();
    for f in files.iter().take(MAX_FILES) {
        let (_, mut doc) = match markupcraft_revu::open(f) {
            Ok(x) => x,
            Err(e) => {
                errors.push(format!("{}: {e}", f.display()));
                continue;
            }
        };
        if measurements_only {
            doc.markups.retain(|m| m.kind.is_measurement());
        }
        n += doc.markups.len();
        let csv = crate::export::markups_csv(&doc, None);
        let name = f
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        for (i, line) in csv.lines().enumerate() {
            if i == 0 {
                if out.is_empty() {
                    out.push_str("File,");
                    out.push_str(line);
                    out.push('\n');
                }
                continue;
            }
            out.push_str(&csv_q(&name));
            out.push(',');
            out.push_str(line);
            out.push('\n');
        }
    }
    if out.is_empty() {
        out.push_str("File,Page\n");
    }
    (out, n, errors)
}

// ---- batch link -----------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct BatchLinkOptions {
    pub match_case: bool,
    /// border width (0 = invisible)
    pub width: f64,
    pub color: Color,
    /// grow each word box by this many points
    pub padding: f64,
    /// Where the search terms come from.
    pub terms: LinkTerms,
    /// Term filter: cut each generated term at this character, keeping the text before it
    /// (`keep_start`) or after it.
    pub filter_char: Option<char>,
    pub keep_start: bool,
    /// Links to other files store the full path (else relative to the linking file).
    pub full_paths: bool,
    /// A highlight markup of this colour over each new link.
    pub highlight: Option<Color>,
    /// A place that already has a link: the old link is deleted and the new one added.
    pub replace_existing: bool,
    /// A place that already has a link gets the new link beside it (both kept).
    pub add_overlapping: bool,
    /// How the highlight over each new link looks.
    pub highlight_style: HighlightStyle,
    /// Flatten the highlights into the pages.
    pub flatten_highlight: bool,
}

/// How Batch Link marks each new link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HighlightStyle {
    /// A translucent filled box.
    #[default]
    Fill,
    /// An outlined box.
    Outline,
    /// A text highlight (multiplied over the words).
    Highlight,
}

impl HighlightStyle {
    pub fn name(self) -> &'static str {
        match self {
            HighlightStyle::Fill => "fill",
            HighlightStyle::Outline => "outline",
            HighlightStyle::Highlight => "highlight",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "fill" => HighlightStyle::Fill,
            "outline" => HighlightStyle::Outline,
            "highlight" => HighlightStyle::Highlight,
            _ => return None,
        })
    }
}

/// Where a Batch Link term goes (`file` indexes the batch's files).
#[derive(Debug, Clone, PartialEq)]
pub enum TermDest {
    /// A page of a file.
    Page { file: usize, page: usize },
    /// A file (opened at its first page; a non-PDF file is launched).
    File { file: usize },
    /// A named Place (destination) in a file.
    Place { file: usize, name: String },
    /// A web address.
    Url(String),
}

/// A search term and its destination.
#[derive(Debug, Clone, PartialEq)]
pub struct TermTarget {
    pub term: String,
    pub dest: TermDest,
}

/// Where Batch Link's search terms come from.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum LinkTerms {
    /// Each page's label (sheet number).
    #[default]
    PageLabels,
    /// Each file's name (its first page is the target).
    FileNames,
    /// The text inside this box of each page (AutoMark region).
    Region(Rect),
    /// Typed or imported terms: (term, target file index, target page).
    Custom(Vec<(String, usize, usize)>),
    /// Terms with any destination: a page, a file, a Place in a file or a web address.
    Targets(Vec<TermTarget>),
}

/// Terms from CSV lines `term,file name,page` (the file matched by name among `files`).
pub fn link_terms_csv(text: &str, files: &[PathBuf]) -> Vec<(String, usize, usize)> {
    text.lines()
        .filter_map(|l| {
            let mut parts = l.split(',').map(|p| p.trim().trim_matches('"'));
            let term = parts.next()?.to_string();
            let file = parts.next().unwrap_or("");
            let page = parts
                .next()
                .and_then(|p| p.parse::<usize>().ok())
                .unwrap_or(1)
                .saturating_sub(1);
            let fi = files.iter().position(|f| {
                f.file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(file))
                    || f.file_stem()
                        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(file))
            })?;
            (!term.is_empty()).then_some((term, fi, page))
        })
        .collect()
}

/// A term cut at the filter character.
fn filtered(term: &str, filter: Option<char>, keep_start: bool) -> String {
    match filter.and_then(|c| term.find(c).map(|i| (c, i))) {
        Some((_, i)) if keep_start => term[..i].trim().to_string(),
        Some((c, i)) => term[i + c.len_utf8()..].trim().to_string(),
        None => term.trim().to_string(),
    }
}

impl Default for BatchLinkOptions {
    fn default() -> Self {
        Self {
            match_case: false,
            width: 0.0,
            color: Color::rgb(0.0, 0.0, 1.0),
            padding: 1.0,
            terms: LinkTerms::PageLabels,
            filter_char: None,
            keep_start: true,
            full_paths: false,
            highlight: None,
            replace_existing: false,
            add_overlapping: false,
            highlight_style: HighlightStyle::Fill,
            flatten_highlight: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BatchLinkReport {
    /// (file, links added)
    pub files: Vec<(PathBuf, usize)>,
    pub links: usize,
    /// matches where a link was already there (skipped, replaced or kept beside)
    pub existing: usize,
    /// old links deleted (replace_existing)
    pub deleted: usize,
    /// pages with no text to search (scanned pages), skipped
    pub skipped_pages: usize,
    /// highlights added
    pub highlights: usize,
    /// files that could not be opened
    pub files_not_opened: usize,
    pub errors: Vec<String>,
}

fn norm(s: &str, case: bool) -> String {
    let t = s.trim_matches(|c: char| !c.is_alphanumeric());
    if case { t.to_string() } else { t.to_lowercase() }
}

fn overlap(a: &Rect, b: &Rect) -> f64 {
    let w = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let h = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    w * h
}

/// The path a link from `from` stores to reach `to`: the file name when both are in one folder.
fn link_path(from: &Path, to: &Path) -> String {
    match (from.parent(), to.parent()) {
        (Some(a), Some(b)) if crate::docutil::same_file(a, b) || a == b => to
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        _ => to.display().to_string(),
    }
}

impl Session {
    /// Find sheet references on this document's pages: (page, rect, label) for every place
    /// a label of `labels` appears (whole words, up to four words long).
    pub fn find_labels(&self, labels: &[String], match_case: bool) -> Result<Vec<(usize, Rect, String)>> {
        self.find_labels_counted(labels, match_case).map(|x| x.0)
    }

    /// As [`Session::find_labels`], with the number of pages that have no text to search.
    pub fn find_labels_counted(&self, labels: &[String], match_case: bool) -> Result<(Vec<LabelHit>, usize)> {
        let wanted: HashMap<String, &String> = labels
            .iter()
            .filter(|l| !norm(l, match_case).is_empty())
            .map(|l| (norm(l, match_case), l))
            .collect();
        if wanted.is_empty() {
            return Ok((Vec::new(), 0));
        }
        let r = self.renderable(true)?;
        let mut out = Vec::new();
        let mut no_text = 0;
        for page in 0..self.page_count() {
            let Some(text) = r.text(page) else {
                no_text += 1;
                continue;
            };
            let geom = r.geom(page)?;
            let words = crate::raster::words(&text, geom);
            if words.is_empty() {
                no_text += 1;
                continue;
            }
            let mut i = 0;
            while i < words.len() {
                let mut hit = None;
                for k in (1..=MAX_LABEL_WORDS).rev() {
                    let Some(run) = words.get(i..i + k) else { continue };
                    let joined = run.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
                    if let Some(l) = wanted.get(&norm(&joined, match_case)) {
                        let Some(first) = run.first() else { continue };
                        let mut rect = first.rect;
                        for w in run {
                            rect = Rect::new(
                                rect.x0.min(w.rect.x0),
                                rect.y0.min(w.rect.y0),
                                rect.x1.max(w.rect.x1),
                                rect.y1.max(w.rect.y1),
                            );
                        }
                        hit = Some((k, rect, (*l).clone()));
                        break;
                    }
                }
                match hit {
                    Some((k, rect, label)) => {
                        out.push((page, rect, label));
                        i += k;
                    }
                    None => i += 1,
                }
            }
        }
        Ok((out, no_text))
    }

    /// Slip Sheet: replace this document's pages with the pages of `new_file` whose labels
    /// match (markups stay on the replaced pages); with `append_unmatched`, new sheets that
    /// match nothing are added at the end with their labels. `number_filter` (e.g. " - ")
    /// matches on the label part before it. Undoable (one step per part).
    pub fn slip_sheet(&mut self, new_file: &Path, opts: &SlipSheetOptions) -> Result<SlipSheetReport> {
        let new = Session::open(new_file)?;
        let key = |l: &str| -> String {
            let l = if opts.number_filter.is_empty() {
                l
            } else {
                l.split(opts.number_filter.as_str()).next().unwrap_or(l)
            };
            let l = l.trim();
            if opts.match_case {
                l.to_string()
            } else {
                l.to_lowercase()
            }
        };
        let new_labels: Vec<String> = new.doc().pages.iter().map(|p| p.label.clone()).collect();
        let mut report = SlipSheetReport::default();
        let mut used_new = vec![false; new_labels.len()];
        let mut targets = Vec::new();
        let mut sources = Vec::new();
        for (op, p) in self.doc.pages.iter().enumerate() {
            let k = key(&p.label);
            if k.is_empty() {
                report.unmatched_old.push(op);
                continue;
            }
            let found = new_labels
                .iter()
                .enumerate()
                .find(|(i, l)| !used_new.get(*i).copied().unwrap_or(true) && key(l) == k);
            match found {
                Some((np, _)) => {
                    if let Some(u) = used_new.get_mut(np) {
                        *u = true;
                    }
                    report.matched.push(SlipPair {
                        old_page: op,
                        new_page: np,
                        label: p.label.clone(),
                        markups: self.doc.markups.iter().filter(|m| m.page == op).count(),
                    });
                    targets.push(op);
                    sources.push(np);
                }
                None => report.unmatched_old.push(op),
            }
        }
        report.unmatched_new = used_new
            .iter()
            .enumerate()
            .filter(|(_, u)| !**u)
            .map(|(i, _)| i)
            .collect();
        if !targets.is_empty() {
            self.replace_pages(&targets, new_file, &sources)?;
        }
        if opts.append_unmatched && !report.unmatched_new.is_empty() {
            let at = self.page_count();
            self.insert_file_pages(at, new_file, Some(&report.unmatched_new))?;
            let labels: Vec<(usize, String)> = report
                .unmatched_new
                .iter()
                .enumerate()
                .filter_map(|(k, np)| {
                    let l = new_labels.get(*np)?;
                    (!l.is_empty()).then(|| (at + k, l.clone()))
                })
                .collect();
            if !labels.is_empty() {
                self.set_page_labels(&labels)?;
            }
            report.appended = report.unmatched_new.len();
        }
        Ok(report)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlipSheetOptions {
    /// match on the part of the label before this ("" = the whole label)
    pub number_filter: String,
    pub match_case: bool,
    pub append_unmatched: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlipPair {
    pub old_page: usize,
    pub new_page: usize,
    pub label: String,
    /// markups the replaced page keeps
    pub markups: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlipSheetReport {
    pub matched: Vec<SlipPair>,
    /// old pages left as they were
    pub unmatched_old: Vec<usize>,
    /// new pages that matched nothing
    pub unmatched_new: Vec<usize>,
    /// pages appended at the end
    pub appended: usize,
}

/// The search terms of a Batch Link run and where each goes (the term table: page labels, file
/// names or region text read from the files, or the typed terms). Files that cannot be opened
/// are named in the errors.
pub fn batch_link_terms(files: &[PathBuf], opts: &BatchLinkOptions) -> Result<(Vec<TermTarget>, Vec<String>)> {
    let mut opened = Vec::new();
    let mut errors = Vec::new();
    let terms = gather_terms(files, opts, &mut opened, &mut errors);
    Ok((terms, errors))
}

fn gather_terms(
    files: &[PathBuf],
    opts: &BatchLinkOptions,
    opened: &mut Vec<Option<Session>>,
    errors: &mut Vec<String>,
) -> Vec<TermTarget> {
    let mut out: Vec<TermTarget> = Vec::new();
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut add = |term: &str, dest: TermDest, out: &mut Vec<TermTarget>| {
        let t = filtered(term, opts.filter_char, opts.keep_start);
        let k = norm(&t, opts.match_case);
        if !k.is_empty() && !seen.contains_key(&k) {
            seen.insert(k, ());
            out.push(TermTarget { term: t, dest });
        }
    };
    match &opts.terms {
        LinkTerms::Custom(list) => {
            for (t, fi, p) in list {
                add(t, TermDest::Page { file: *fi, page: *p }, &mut out);
            }
        }
        LinkTerms::Targets(list) => {
            for t in list {
                add(&t.term, t.dest.clone(), &mut out);
            }
        }
        _ => {}
    }
    for (fi, f) in files.iter().enumerate() {
        match Session::open(f) {
            Ok(s) => {
                match &opts.terms {
                    LinkTerms::PageLabels => {
                        for (p, info) in s.doc().pages.iter().enumerate() {
                            add(&info.label, TermDest::Page { file: fi, page: p }, &mut out);
                        }
                    }
                    LinkTerms::FileNames => {
                        let stem = f
                            .file_stem()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        add(&stem, TermDest::Page { file: fi, page: 0 }, &mut out);
                    }
                    LinkTerms::Region(r) => {
                        for p in 0..s.page_count() {
                            let t = s.text_in_rect(p, *r).unwrap_or_default();
                            add(&t, TermDest::Page { file: fi, page: p }, &mut out);
                        }
                    }
                    LinkTerms::Custom(_) | LinkTerms::Targets(_) => {}
                }
                opened.push(Some(s));
            }
            Err(e) => {
                errors.push(format!("{}: {e}", f.display()));
                opened.push(None);
            }
        }
    }
    out
}

/// Batch Link over `files`: every term (page labels in the set by default) becomes a link
/// target; wherever a page shows a term, a link to its destination (a page of this or another
/// file, a file, a Place in a file, or a web address) is added. Files are saved in place
/// (incrementally) when links were added.
pub fn batch_link(files: &[PathBuf], opts: &BatchLinkOptions) -> Result<BatchLinkReport> {
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(invalid(format!("give 1 to {MAX_FILES} files")));
    }
    if !(opts.padding.is_finite() && (0.0..=36.0).contains(&opts.padding)) {
        return Err(invalid("padding: 0 to 36 points"));
    }
    if let LinkTerms::Targets(list) = &opts.terms {
        for t in list {
            match &t.dest {
                TermDest::Page { file, .. } | TermDest::File { file } | TermDest::Place { file, .. }
                    if *file >= files.len() =>
                {
                    return Err(invalid(format!(
                        "term {:?}: file {} is not one of the files",
                        t.term,
                        file + 1
                    )));
                }
                TermDest::Url(u) if u.trim().is_empty() => {
                    return Err(invalid(format!("term {:?}: the web address is empty", t.term)));
                }
                _ => {}
            }
        }
    }
    let mut report = BatchLinkReport::default();
    let mut opened: Vec<Option<Session>> = Vec::new();
    let terms = gather_terms(files, opts, &mut opened, &mut report.errors);
    report.files_not_opened = report.errors.len();
    let targets: HashMap<String, TermDest> = terms
        .iter()
        .map(|t| (norm(&t.term, opts.match_case), t.dest.clone()))
        .collect();
    let labels: Vec<String> = terms.iter().map(|t| t.term.clone()).collect();
    let look = LinkLook {
        width: opts.width,
        color: opts.color,
    };
    for (fi, slot) in opened.iter_mut().enumerate() {
        let Some(s) = slot else { continue };
        let Some(path) = files.get(fi) else { continue };
        let (hits, no_text) = match s.find_labels_counted(&labels, opts.match_case) {
            Ok(h) => h,
            Err(e) => {
                report.errors.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        report.skipped_pages += no_text;
        let mut existing: Vec<(usize, Rect)> = s.links().iter().map(|l| (l.page, l.rect)).collect();
        let mut added = 0;
        let mut highlights: Vec<String> = Vec::new();
        for (page, rect, label) in hits {
            if report.links >= MAX_LINKS {
                break;
            }
            let Some(dest) = targets.get(&norm(&label, opts.match_case)) else {
                continue;
            };
            if matches!(dest, TermDest::Page { file, page: p } if *file == fi && *p == page) {
                continue;
            }
            let r = rect.padded(opts.padding);
            let area = (r.width() * r.height()).max(1e-9);
            if existing.iter().any(|(p, e)| *p == page && overlap(e, &r) > 0.5 * area) {
                report.existing += 1;
                if opts.replace_existing {
                    let old: Vec<String> = s
                        .links()
                        .into_iter()
                        .filter(|l| l.page == page && overlap(&l.rect, &r) > 0.5 * area)
                        .map(|l| l.id)
                        .collect();
                    if !old.is_empty() {
                        report.deleted += s.delete_links(&old).unwrap_or(0);
                    }
                } else if !opts.add_overlapping {
                    continue;
                }
            }
            let file_path = |tf: usize| -> Option<String> {
                let to = files.get(tf)?;
                Some(if opts.full_paths {
                    to.display().to_string()
                } else {
                    link_path(path, to)
                })
            };
            let target = match dest {
                TermDest::Page { file, page: tp } if *file == fi => LinkTarget::Page(*tp),
                TermDest::Page { file, page: tp } => {
                    let Some(p) = file_path(*file) else { continue };
                    LinkTarget::File {
                        path: p,
                        page: Some(*tp),
                    }
                }
                TermDest::File { file } if *file == fi => LinkTarget::Page(0),
                TermDest::File { file } => {
                    let Some(p) = file_path(*file) else { continue };
                    let pdf = p.to_ascii_lowercase().ends_with(".pdf");
                    LinkTarget::File {
                        path: p,
                        page: pdf.then_some(0),
                    }
                }
                TermDest::Place { file, name } if *file == fi => LinkTarget::Place(name.clone()),
                TermDest::Place { file, name } => {
                    let Some(p) = file_path(*file) else { continue };
                    LinkTarget::FilePlace {
                        path: p,
                        name: name.clone(),
                    }
                }
                TermDest::Url(u) => LinkTarget::Url(u.clone()),
            };
            match s.add_link(page, r, &target, look) {
                Ok(_) => {
                    added += 1;
                    report.links += 1;
                    existing.push((page, r));
                    if let Some(c) = opts.highlight {
                        let m = highlight_markup(page, r, c, opts.highlight_style);
                        if let Ok(id) = s.add_markup(m) {
                            highlights.push(id);
                        }
                    }
                }
                Err(e) => report.errors.push(format!("{} page {}: {e}", path.display(), page + 1)),
            }
        }
        report.highlights += highlights.len();
        if opts.flatten_highlight
            && !highlights.is_empty()
            && let Err(e) = s.flatten_markups(&crate::flatten::FlattenFilter {
                ids: highlights,
                ..Default::default()
            })
        {
            report.errors.push(format!("{}: {e}", path.display()));
        }
        if (added > 0 || report.deleted > 0)
            && let Err(e) = s.save(false)
        {
            report.errors.push(format!("{}: {e}", path.display()));
        }
        report.files.push((path.clone(), added));
    }
    Ok(report)
}

/// The mark Batch Link puts over a new link.
fn highlight_markup(page: usize, r: Rect, c: Color, style: HighlightStyle) -> markupcraft_model::Markup {
    use markupcraft_model::{Kind, Markup, Point};
    match style {
        HighlightStyle::Fill => {
            let mut m = Markup::new(Kind::Rectangle, page, r.corners().to_vec());
            m.color = c;
            m.fill = Some(c);
            m.fill_opacity = 0.3;
            m.opacity = 0.5;
            m.line_width = 0.0;
            m.subject = "Link".into();
            m
        }
        HighlightStyle::Outline => {
            let mut m = Markup::new(Kind::Rectangle, page, r.corners().to_vec());
            m.color = c;
            m.fill = None;
            m.line_width = 1.5;
            m.subject = "Link".into();
            m
        }
        HighlightStyle::Highlight => {
            let q = vec![
                Point::new(r.x0, r.y1),
                Point::new(r.x1, r.y1),
                Point::new(r.x0, r.y0),
                Point::new(r.x1, r.y0),
            ];
            let mut m = Markup::new(Kind::TextHighlight, page, q);
            m.color = c;
            m.subject = "Link".into();
            m
        }
    }
}

// ---- the term table and saved runs -------------------------------------------------------------

fn file_label(files: &[PathBuf], i: usize) -> String {
    files
        .get(i)
        .and_then(|f| f.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The term table as CSV: `Term,File,Page,Place,URL` (a page from 1).
pub fn link_terms_to_csv(terms: &[TermTarget], files: &[PathBuf]) -> String {
    let mut out = String::from("Term,File,Page,Place,URL\r\n");
    for t in terms {
        let (file, page, place, url) = match &t.dest {
            TermDest::Page { file, page } => (
                file_label(files, *file),
                (page + 1).to_string(),
                String::new(),
                String::new(),
            ),
            TermDest::File { file } => (file_label(files, *file), String::new(), String::new(), String::new()),
            TermDest::Place { file, name } => (file_label(files, *file), String::new(), name.clone(), String::new()),
            TermDest::Url(u) => (String::new(), String::new(), String::new(), u.clone()),
        };
        out.push_str(&format!(
            "{},{},{},{},{}\r\n",
            csv_q(&t.term),
            csv_q(&file),
            page,
            csv_q(&place),
            csv_q(&url)
        ));
    }
    out
}

/// Terms from a term table CSV (`Term,File,Page,Place,URL`, a header row optional; the file
/// matched by name among `files`). Rows whose file is not in the list are skipped.
pub fn link_terms_from_csv(text: &str, files: &[PathBuf]) -> Vec<TermTarget> {
    let find = |name: &str| {
        files.iter().position(|f| {
            f.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
                || f.file_stem()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })
    };
    parse_csv(text)
        .into_iter()
        .filter(|r| !r.first().is_some_and(|c| c.trim().eq_ignore_ascii_case("term")))
        .filter_map(|r| {
            let cell = |i: usize| r.get(i).map(|c| c.trim().to_string()).unwrap_or_default();
            let term = cell(0);
            if term.is_empty() {
                return None;
            }
            let (file, page, place, url) = (cell(1), cell(2), cell(3), cell(4));
            let dest = if !url.is_empty() {
                TermDest::Url(url)
            } else {
                let fi = find(&file)?;
                if !place.is_empty() {
                    TermDest::Place { file: fi, name: place }
                } else {
                    match page.parse::<usize>() {
                        Ok(p) => TermDest::Page {
                            file: fi,
                            page: p.saturating_sub(1),
                        },
                        Err(_) => TermDest::File { file: fi },
                    }
                }
            };
            Some(TermTarget { term, dest })
        })
        .take(MAX_LINKS)
        .collect()
}

/// A Batch Link run to save and run again: the files and every option.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchLinkRun {
    pub files: Vec<PathBuf>,
    pub options: BatchLinkOptions,
}

pub const LINK_RUN_FORMAT: &str = "markupcraft-batch-link";

fn hex(c: Color) -> String {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", b(c.r), b(c.g), b(c.b))
}

fn unhex(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = |i: usize| -> Option<f64> { Some(f64::from(u8::from_str_radix(s.get(i..i + 2)?, 16).ok()?) / 255.0) };
    Some(Color::rgb(v(0)?, v(2)?, v(4)?))
}

fn xml_esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The run as XML (our own schema).
pub fn link_run_xml(run: &BatchLinkRun) -> String {
    let o = &run.options;
    let mut x =
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<BatchLink format=\"{LINK_RUN_FORMAT}\" version=\"1\">\n");
    for f in &run.files {
        x.push_str(&format!("  <File path=\"{}\"/>\n", xml_esc(&f.display().to_string())));
    }
    x.push_str(&format!(
        "  <Options matchCase=\"{}\" width=\"{}\" color=\"{}\" padding=\"{}\" filter=\"{}\" keepStart=\"{}\" fullPaths=\"{}\" highlight=\"{}\" highlightStyle=\"{}\" flattenHighlight=\"{}\" replaceExisting=\"{}\" addOverlapping=\"{}\"/>\n",
        o.match_case,
        o.width,
        hex(o.color),
        o.padding,
        xml_esc(&o.filter_char.map(String::from).unwrap_or_default()),
        o.keep_start,
        o.full_paths,
        o.highlight.map(hex).unwrap_or_default(),
        o.highlight_style.name(),
        o.flatten_highlight,
        o.replace_existing,
        o.add_overlapping
    ));
    let (source, region) = match &o.terms {
        LinkTerms::PageLabels => ("page_labels", None),
        LinkTerms::FileNames => ("file_names", None),
        LinkTerms::Region(r) => ("region", Some(*r)),
        LinkTerms::Custom(_) | LinkTerms::Targets(_) => ("terms", None),
    };
    match region {
        Some(r) => x.push_str(&format!(
            "  <Terms source=\"{source}\" region=\"{} {} {} {}\">\n",
            r.x0, r.y0, r.x1, r.y1
        )),
        None => x.push_str(&format!("  <Terms source=\"{source}\">\n")),
    }
    let list: Vec<TermTarget> = match &o.terms {
        LinkTerms::Custom(l) => l
            .iter()
            .map(|(t, f, p)| TermTarget {
                term: t.clone(),
                dest: TermDest::Page { file: *f, page: *p },
            })
            .collect(),
        LinkTerms::Targets(l) => l.clone(),
        _ => Vec::new(),
    };
    for t in &list {
        let attrs = match &t.dest {
            TermDest::Page { file, page } => format!("to=\"page\" file=\"{file}\" page=\"{page}\""),
            TermDest::File { file } => format!("to=\"file\" file=\"{file}\""),
            TermDest::Place { file, name } => format!("to=\"place\" file=\"{file}\" place=\"{}\"", xml_esc(name)),
            TermDest::Url(u) => format!("to=\"url\" url=\"{}\"", xml_esc(u)),
        };
        x.push_str(&format!("    <Term text=\"{}\" {attrs}/>\n", xml_esc(&t.term)));
    }
    x.push_str("  </Terms>\n</BatchLink>\n");
    x
}

/// Read a run from its XML.
pub fn link_run_from_xml(text: &str) -> Result<BatchLinkRun> {
    let doc = roxmltree::Document::parse(text).map_err(|e| invalid(format!("not a Batch Link run: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "BatchLink" || root.attribute("format") != Some(LINK_RUN_FORMAT) {
        return Err(invalid("not a MarkupCraft Batch Link run"));
    }
    let mut run = BatchLinkRun {
        files: Vec::new(),
        options: BatchLinkOptions::default(),
    };
    let b = |n: &roxmltree::Node, k: &str, d: bool| n.attribute(k).map_or(d, |v| v == "true");
    let f = |n: &roxmltree::Node, k: &str| {
        n.attribute(k)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
    };
    let u = |n: &roxmltree::Node, k: &str| n.attribute(k).and_then(|v| v.parse::<usize>().ok());
    for n in root.children().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "File" => {
                if run.files.len() >= MAX_FILES {
                    return Err(invalid(format!("a run holds at most {MAX_FILES} files")));
                }
                run.files.push(PathBuf::from(n.attribute("path").unwrap_or_default()));
            }
            "Options" => {
                let o = &mut run.options;
                o.match_case = b(&n, "matchCase", false);
                o.width = f(&n, "width").unwrap_or(0.0).clamp(0.0, 12.0);
                o.color = n.attribute("color").and_then(unhex).unwrap_or(o.color);
                o.padding = f(&n, "padding").unwrap_or(1.0).clamp(0.0, 36.0);
                o.filter_char = n.attribute("filter").and_then(|s| s.chars().next());
                o.keep_start = b(&n, "keepStart", true);
                o.full_paths = b(&n, "fullPaths", false);
                o.highlight = n.attribute("highlight").and_then(unhex);
                o.highlight_style = n
                    .attribute("highlightStyle")
                    .and_then(HighlightStyle::from_name)
                    .unwrap_or_default();
                o.flatten_highlight = b(&n, "flattenHighlight", false);
                o.replace_existing = b(&n, "replaceExisting", false);
                o.add_overlapping = b(&n, "addOverlapping", false);
            }
            "Terms" => {
                let mut list = Vec::new();
                for t in n.children().filter(|t| t.has_tag_name("Term")).take(MAX_LINKS) {
                    let term = t.attribute("text").unwrap_or_default().to_string();
                    let file = u(&t, "file").unwrap_or(0);
                    let dest = match t.attribute("to").unwrap_or("page") {
                        "file" => TermDest::File { file },
                        "place" => TermDest::Place {
                            file,
                            name: t.attribute("place").unwrap_or_default().to_string(),
                        },
                        "url" => TermDest::Url(t.attribute("url").unwrap_or_default().to_string()),
                        _ => TermDest::Page {
                            file,
                            page: u(&t, "page").unwrap_or(0),
                        },
                    };
                    list.push(TermTarget { term, dest });
                }
                run.options.terms = match n.attribute("source").unwrap_or("page_labels") {
                    "file_names" => LinkTerms::FileNames,
                    "region" => {
                        let v: Vec<f64> = n
                            .attribute("region")
                            .unwrap_or_default()
                            .split_whitespace()
                            .filter_map(|x| x.parse().ok())
                            .collect();
                        match v.as_slice() {
                            [a, b, c, d] => LinkTerms::Region(Rect::new(*a, *b, *c, *d)),
                            _ => return Err(invalid("the run's region is not four numbers")),
                        }
                    }
                    "terms" => LinkTerms::Targets(list),
                    _ => LinkTerms::PageLabels,
                };
            }
            _ => {}
        }
    }
    Ok(run)
}

/// Save a run: as XML, or into a Set file (`.pcset`, kept beside its file list).
pub fn save_link_run(path: &Path, run: &BatchLinkRun) -> Result<()> {
    let xml = link_run_xml(run);
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pcset")) {
        let mut v: serde_json::Value = if path.exists() {
            let text = std::fs::read_to_string(path).map_err(io(path))?;
            serde_json::from_str(&text).map_err(|e| invalid(format!("not a set file: {e}")))?
        } else {
            let set = DrawingSet {
                name: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                files: run.files.clone(),
                ..Default::default()
            };
            save_set(path, &set)?;
            let text = std::fs::read_to_string(path).map_err(io(path))?;
            serde_json::from_str(&text).map_err(|e| invalid(e.to_string()))?
        };
        let obj = v.as_object_mut().ok_or_else(|| invalid("not a set file"))?;
        obj.insert("batch_link".into(), serde_json::Value::String(xml));
        let text = serde_json::to_string_pretty(&v).map_err(|e| invalid(e.to_string()))?;
        return crate::write_atomic(path, text.as_bytes());
    }
    crate::write_atomic(path, xml.as_bytes())
}

/// Load a run saved with [`save_link_run`] (XML, or the run inside a Set file).
pub fn load_link_run(path: &Path) -> Result<BatchLinkRun> {
    if std::fs::metadata(path).map_err(io(path))?.len() > 32 << 20 {
        return Err(invalid("the run file is too large"));
    }
    let text = std::fs::read_to_string(path).map_err(io(path))?;
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pcset")) {
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| invalid(format!("not a set file: {e}")))?;
        let xml = v
            .get("batch_link")
            .and_then(|x| x.as_str())
            .ok_or_else(|| invalid("the set holds no Batch Link run"))?;
        return link_run_from_xml(xml);
    }
    link_run_from_xml(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mc-batch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sheet_file(path: &Path, sheets: &[(&str, &str)]) {
        // (label, text on the page)
        let pages: Vec<SyntheticPage> = sheets
            .iter()
            .map(|(_, t)| SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 14.0, t)))
            .collect();
        std::fs::write(path, pdf(&pages)).unwrap();
        let mut s = Session::open(path).unwrap();
        let labels: Vec<(usize, String)> = sheets
            .iter()
            .enumerate()
            .map(|(i, (l, _))| (i, l.to_string()))
            .collect();
        s.set_page_labels(&labels).unwrap();
        s.save(true).unwrap();
    }

    #[test]
    fn batch_link_terms_filters_and_options() {
        let d = tmp("linkterms");
        sheet_file(&d.join("A-101 Plan.pdf"), &[("", "SEE DETAIL 5 AND M-201")]);
        sheet_file(&d.join("M-201 Mech.pdf"), &[("", "MECH")]);
        let files = vec![d.join("A-101 Plan.pdf"), d.join("M-201 Mech.pdf")];
        // File names cut at the space: "M-201".
        let o = BatchLinkOptions {
            terms: LinkTerms::FileNames,
            filter_char: Some(' '),
            keep_start: true,
            full_paths: true,
            highlight: Some(Color::rgb(1.0, 1.0, 0.0)),
            ..Default::default()
        };
        let r = batch_link(&files, &o).unwrap();
        assert_eq!(r.links, 1, "{r:?}");
        let s = Session::open(&files[0]).unwrap();
        let l = &s.links()[0];
        assert!(
            matches!(&l.target, LinkTarget::File { path, .. } if path.contains("M-201 Mech.pdf") && path.len() > 20),
            "{:?}",
            l.target
        );
        assert_eq!(s.doc().markups.len(), 1, "the highlight");
        // Custom terms from CSV, and replacing the existing link.
        let terms = link_terms_csv("DETAIL 5,M-201 Mech.pdf,1\nnope,missing.pdf,1", &files);
        assert_eq!(terms.len(), 1);
        let o = BatchLinkOptions {
            terms: LinkTerms::Custom(terms),
            replace_existing: true,
            ..Default::default()
        };
        let r = batch_link(&files, &o).unwrap();
        assert_eq!(r.links, 1);
        assert_eq!(filtered("A-101 Plan", Some(' '), false), "Plan");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn sets_sort_naturally_and_round_trip() {
        let d = tmp("set");
        sheet_file(&d.join("a.pdf"), &[("A-10", "x"), ("A-2", "y")]);
        sheet_file(&d.join("b.pdf"), &[("A-1", "z")]);
        let set = DrawingSet {
            name: "Arch".into(),
            files: vec![d.join("a.pdf"), d.join("b.pdf"), d.join("missing.pdf")],
            ..Default::default()
        };
        save_set(&d.join("arch.pcset"), &set).unwrap();
        let text = std::fs::read_to_string(d.join("arch.pcset")).unwrap();
        assert!(text.contains("\"a.pdf\""), "{text}");
        let back = load_set(&d.join("arch.pcset")).unwrap();
        let (sheets, errors) = set_sheets(&back, SetSort::Label);
        let labels: Vec<&str> = sheets.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["A-1", "A-2", "A-10"]);
        assert_eq!(errors.len(), 1);
        std::fs::remove_dir_all(&d).ok();
    }
}
