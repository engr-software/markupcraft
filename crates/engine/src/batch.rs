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
    /// A place that already has a link gets the new link anyway (else it is skipped).
    pub replace_existing: bool,
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BatchLinkReport {
    /// (file, links added)
    pub files: Vec<(PathBuf, usize)>,
    pub links: usize,
    /// matches skipped because a link was already there
    pub existing: usize,
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
        let wanted: HashMap<String, &String> = labels
            .iter()
            .filter(|l| !norm(l, match_case).is_empty())
            .map(|l| (norm(l, match_case), l))
            .collect();
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let r = self.renderable(true)?;
        let mut out = Vec::new();
        for page in 0..self.page_count() {
            let Some(text) = r.text(page) else { continue };
            let geom = r.geom(page)?;
            let words = crate::raster::words(&text, geom);
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
        Ok(out)
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

/// Batch Link over `files`: every page label in the set becomes a link target; wherever a
/// page shows another page's label, a link to it is added. Files are saved in place
/// (incrementally) when links were added.
pub fn batch_link(files: &[PathBuf], opts: &BatchLinkOptions) -> Result<BatchLinkReport> {
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(invalid(format!("give 1 to {MAX_FILES} files")));
    }
    if !(opts.padding.is_finite() && (0.0..=36.0).contains(&opts.padding)) {
        return Err(invalid("padding: 0 to 36 points"));
    }
    let mut report = BatchLinkReport::default();
    // targets: label -> (file, page), first one wins
    let mut targets: HashMap<String, (usize, usize)> = HashMap::new();
    let mut labels: Vec<String> = Vec::new();
    let mut opened: Vec<Option<Session>> = Vec::new();
    let add_target =
        |term: &str, fi: usize, p: usize, targets: &mut HashMap<String, (usize, usize)>, labels: &mut Vec<String>| {
            let t = filtered(term, opts.filter_char, opts.keep_start);
            let k = norm(&t, opts.match_case);
            if !k.is_empty() && !targets.contains_key(&k) {
                targets.insert(k, (fi, p));
                labels.push(t);
            }
        };
    if let LinkTerms::Custom(list) = &opts.terms {
        for (t, fi, p) in list {
            add_target(t, *fi, *p, &mut targets, &mut labels);
        }
    }
    for (fi, f) in files.iter().enumerate() {
        match Session::open(f) {
            Ok(s) => {
                match &opts.terms {
                    LinkTerms::PageLabels => {
                        for (p, info) in s.doc().pages.iter().enumerate() {
                            add_target(&info.label, fi, p, &mut targets, &mut labels);
                        }
                    }
                    LinkTerms::FileNames => {
                        let stem = f
                            .file_stem()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        add_target(&stem, fi, 0, &mut targets, &mut labels);
                    }
                    LinkTerms::Region(r) => {
                        for p in 0..s.page_count() {
                            let t = s.text_in_rect(p, *r).unwrap_or_default();
                            add_target(&t, fi, p, &mut targets, &mut labels);
                        }
                    }
                    LinkTerms::Custom(_) => {}
                }
                opened.push(Some(s));
            }
            Err(e) => {
                report.errors.push(format!("{}: {e}", f.display()));
                opened.push(None);
            }
        }
    }
    let look = LinkLook {
        width: opts.width,
        color: opts.color,
    };
    for (fi, slot) in opened.iter_mut().enumerate() {
        let Some(s) = slot else { continue };
        let Some(path) = files.get(fi) else { continue };
        let hits = match s.find_labels(&labels, opts.match_case) {
            Ok(h) => h,
            Err(e) => {
                report.errors.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let mut existing: Vec<(usize, Rect)> = s.links().iter().map(|l| (l.page, l.rect)).collect();
        let mut added = 0;
        for (page, rect, label) in hits {
            if report.links >= MAX_LINKS {
                break;
            }
            let Some(&(tf, tp)) = targets.get(&norm(&label, opts.match_case)) else {
                continue;
            };
            if tf == fi && tp == page {
                continue;
            }
            let r = rect.padded(opts.padding);
            let area = (r.width() * r.height()).max(1e-9);
            if existing.iter().any(|(p, e)| *p == page && overlap(e, &r) > 0.5 * area) {
                report.existing += 1;
                if !opts.replace_existing {
                    continue;
                }
                let old: Vec<String> = s
                    .links()
                    .into_iter()
                    .filter(|l| l.page == page && overlap(&l.rect, &r) > 0.5 * area)
                    .map(|l| l.id)
                    .collect();
                if !old.is_empty() {
                    let _ = s.delete_links(&old);
                }
            }
            let target = if tf == fi {
                LinkTarget::Page(tp)
            } else {
                let Some(to) = files.get(tf) else { continue };
                LinkTarget::File {
                    path: if opts.full_paths {
                        to.display().to_string()
                    } else {
                        link_path(path, to)
                    },
                    page: Some(tp),
                }
            };
            match s.add_link(page, r, &target, look) {
                Ok(_) => {
                    added += 1;
                    report.links += 1;
                    existing.push((page, r));
                    if let Some(c) = opts.highlight {
                        let mut m = markupcraft_model::Markup::new(
                            markupcraft_model::Kind::Rectangle,
                            page,
                            r.corners().to_vec(),
                        );
                        m.color = c;
                        m.fill = Some(c);
                        m.fill_opacity = 0.3;
                        m.opacity = 0.5;
                        m.line_width = 0.0;
                        m.subject = "Link".into();
                        let _ = s.add_markup(m);
                    }
                }
                Err(e) => report.errors.push(format!("{} page {}: {e}", path.display(), page + 1)),
            }
        }
        if added > 0
            && let Err(e) = s.save(false)
        {
            report.errors.push(format!("{}: {e}", path.display()));
        }
        report.files.push((path.clone(), added));
    }
    Ok(report)
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
