//! Batch > Compare Documents and Batch > Overlay Pages: pair many current sheets with their
//! revisions and compare (or overlay) every pair in one run.
//!
//! - Sources: files, a folder, or a folder with its subfolders ([`collect_pdfs`]).
//! - Matching ([`MatchBy`]): file name + page index, page label, the text read from a
//!   title-block region, or manual pairs. A wildcard filter ([`filter_key`]) cuts the part of a
//!   file name (or label) that identifies the sheet: `#` a run of digits, `@` a run of letters,
//!   `*` a run of anything but digits, `?` one separator (`-`, `_`, `.` or a space), `\` makes
//!   the next character literal; other characters match themselves (case-insensitively).
//! - The job (file lists, matching and pairs) saves as JSON (`.pcbatch`) for reuse by compare
//!   and overlay.
//! - Results: one compared copy per revised file (clouds on the revised pages) or one overlay
//!   PDF per pair, and a report (CSV, or a PDF with a link to each result).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use markupcraft_model::{Color, Rect};
use serde::{Deserialize, Serialize};

use crate::compare::CompareOptions;
use crate::links::{LinkLook, LinkTarget};
use crate::overlay::{OverlayAlign, OverlayLayer, default_color, overlay_pages};
use crate::raster::{Renderable, read_pdf, words};
use crate::{EngineError, Result, Session, invalid};

pub const BATCH_FORMAT: &str = "markupcraft-batch";
/// Most files on one side of a batch, and most pairs.
pub const MAX_FILES: usize = 2_000;
pub const MAX_PAIRS: usize = 10_000;
/// Folder depth searched for PDFs.
const MAX_DEPTH: usize = 16;

/// PDF files given directly or found in folders (sorted, at most [`MAX_FILES`]).
pub fn collect_pdfs(paths: &[PathBuf], recursive: bool) -> Result<Vec<PathBuf>> {
    fn walk(dir: &Path, recursive: bool, depth: usize, out: &mut Vec<PathBuf>) -> Result<()> {
        let rd = std::fs::read_dir(dir).map_err(|e| EngineError::Io {
            path: dir.display().to_string(),
            source: e,
        })?;
        let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for p in entries {
            if out.len() >= MAX_FILES {
                return Ok(());
            }
            if p.is_dir() {
                if recursive && depth < MAX_DEPTH {
                    walk(&p, recursive, depth + 1, out)?;
                }
            } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                out.push(p);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            walk(p, recursive, 0, &mut out)?;
        } else {
            out.push(p.clone());
        }
        if out.len() > MAX_FILES {
            return Err(invalid(format!("at most {MAX_FILES} files")));
        }
    }
    Ok(out)
}

/// The part of `name` the wildcard `filter` matches (the first match), or `None`.
pub fn filter_key(filter: &str, name: &str) -> Option<String> {
    #[derive(Clone, Copy, PartialEq)]
    enum Tok {
        Digits,
        Letters,
        NonDigits,
        Sep,
        Lit(char),
    }
    let mut toks = Vec::new();
    let mut it = filter.chars();
    while let Some(c) = it.next() {
        toks.push(match c {
            '#' => Tok::Digits,
            '@' => Tok::Letters,
            '*' => Tok::NonDigits,
            '?' => Tok::Sep,
            '\\' => Tok::Lit(it.next()?),
            c => Tok::Lit(c),
        });
        if toks.len() > 64 {
            return None;
        }
    }
    if toks.is_empty() {
        return Some(name.to_string());
    }
    let chars: Vec<char> = name.chars().collect();
    // Greedy match of the tokens from position `at`; returns the end.
    let run = |at: usize| -> Option<usize> {
        let mut i = at;
        for t in &toks {
            let take = |pred: &dyn Fn(char) -> bool, i: usize| {
                let mut j = i;
                while chars.get(j).is_some_and(|c| pred(*c)) {
                    j += 1;
                }
                (j > i).then_some(j)
            };
            i = match t {
                Tok::Digits => take(&|c| c.is_ascii_digit(), i)?,
                Tok::Letters => take(&|c| c.is_alphabetic(), i)?,
                Tok::NonDigits => take(&|c| !c.is_ascii_digit(), i)?,
                Tok::Sep => {
                    if chars.get(i).is_some_and(|c| matches!(c, '-' | '_' | '.' | ' ')) {
                        i + 1
                    } else {
                        return None;
                    }
                }
                Tok::Lit(l) => {
                    if chars.get(i).is_some_and(|c| c.eq_ignore_ascii_case(l)) {
                        i + 1
                    } else {
                        return None;
                    }
                }
            };
        }
        Some(i)
    };
    for start in 0..chars.len() {
        if let Some(end) = run(start) {
            return Some(chars.get(start..end)?.iter().collect::<String>().to_uppercase());
        }
    }
    None
}

/// How current and revised sheets are paired.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "by")]
pub enum MatchBy {
    /// Same file name (or filter key) and the same page index.
    #[default]
    FileAndPage,
    /// Same page label (sheet number).
    PageLabel,
    /// Same text inside this box (title block) of the page, in PDF points.
    Region { rect: [f64; 4] },
    /// The pairs as given.
    Manual,
}

/// One sheet: a file and a 0-based page, with the key it was matched by.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetRef {
    pub file: PathBuf,
    pub page: usize,
    #[serde(default)]
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetPair {
    pub current: SheetRef,
    pub revised: SheetRef,
}

/// A batch job: the two file lists, the matching and the pairs (the saved batch file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BatchJob {
    #[serde(default)]
    pub current: Vec<PathBuf>,
    #[serde(default)]
    pub revised: Vec<PathBuf>,
    #[serde(default)]
    pub matching: MatchBy,
    /// Wildcard filter for the keys ("" = the whole name or label).
    #[serde(default)]
    pub filter: String,
    #[serde(default)]
    pub pairs: Vec<SheetPair>,
}

#[derive(Serialize, Deserialize)]
struct BatchFile {
    format: String,
    version: u32,
    job: BatchJob,
}

/// Save a job (atomic).
pub fn save_job(path: &Path, job: &BatchJob) -> Result<()> {
    let f = BatchFile {
        format: BATCH_FORMAT.into(),
        version: 1,
        job: job.clone(),
    };
    let text = serde_json::to_string_pretty(&f).map_err(|e| invalid(e.to_string()))?;
    crate::write_atomic(path, text.as_bytes())
}

/// Read a saved job.
pub fn load_job(path: &Path) -> Result<BatchJob> {
    let io = |e| EngineError::Io {
        path: path.display().to_string(),
        source: e,
    };
    if std::fs::metadata(path).map_err(io)?.len() > 32 << 20 {
        return Err(invalid("the batch file is too large"));
    }
    let text = std::fs::read_to_string(path).map_err(io)?;
    let f: BatchFile = serde_json::from_str(&text).map_err(|e| invalid(format!("not a batch file: {e}")))?;
    if f.format != BATCH_FORMAT {
        return Err(invalid("not a MarkupCraft batch file"));
    }
    if f.job.current.len() > MAX_FILES || f.job.revised.len() > MAX_FILES || f.job.pairs.len() > MAX_PAIRS {
        return Err(invalid("the batch file lists too many files"));
    }
    Ok(f.job)
}

/// The sheets of a file with the key `matching` gives them.
fn sheets_of(file: &Path, matching: &MatchBy, filter: &str) -> Result<Vec<SheetRef>> {
    let bytes = read_pdf(file)?;
    let s = Session::from_bytes(bytes.as_ref().clone(), file)?;
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let keyed = |raw: &str| -> String {
        if filter.trim().is_empty() {
            raw.trim().to_uppercase()
        } else {
            filter_key(filter, raw).unwrap_or_default()
        }
    };
    let mut out = Vec::new();
    let render = match matching {
        MatchBy::Region { .. } => Some(Renderable::new(bytes.clone(), true)?),
        _ => None,
    };
    for page in 0..s.page_count() {
        let key = match matching {
            MatchBy::FileAndPage | MatchBy::Manual => format!("{}#{}", keyed(&stem), page),
            MatchBy::PageLabel => keyed(&s.doc().pages.get(page).map(|p| p.label.clone()).unwrap_or_default()),
            MatchBy::Region { rect } => {
                let r = Rect::new(rect[0], rect[1], rect[2], rect[3]).normalized();
                let text: Vec<String> = match &render {
                    Some(doc) => match doc.text(page) {
                        Some(t) => words(&t, doc.geom(page)?)
                            .into_iter()
                            .filter(|w| {
                                let (cx, cy) = ((w.rect.x0 + w.rect.x1) / 2.0, (w.rect.y0 + w.rect.y1) / 2.0);
                                cx >= r.x0 && cx <= r.x1 && cy >= r.y0 && cy <= r.y1
                            })
                            .map(|w| w.text)
                            .collect(),
                        None => Vec::new(),
                    },
                    None => Vec::new(),
                };
                keyed(&text.join(" "))
            }
        };
        out.push(SheetRef {
            file: file.to_path_buf(),
            page,
            key,
        });
    }
    Ok(out)
}

/// Pair the job's sheets automatically (keys that are empty or appear twice are skipped).
/// Returns the pairs and the sheets left unmatched on each side.
pub fn match_sheets(job: &BatchJob) -> Result<(Vec<SheetPair>, Vec<SheetRef>, Vec<SheetRef>)> {
    if job.matching == MatchBy::Manual {
        return Ok((job.pairs.clone(), Vec::new(), Vec::new()));
    }
    let mut cur = Vec::new();
    for f in &job.current {
        cur.extend(sheets_of(f, &job.matching, &job.filter)?);
    }
    let mut rev = Vec::new();
    for f in &job.revised {
        rev.extend(sheets_of(f, &job.matching, &job.filter)?);
    }
    let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, r) in rev.iter().enumerate() {
        if !r.key.is_empty() && !r.key.starts_with('#') {
            by_key.entry(r.key.clone()).or_default().push(i);
        }
    }
    let mut pairs = Vec::new();
    let mut used = vec![false; rev.len()];
    let mut left_current = Vec::new();
    for c in cur {
        match by_key.get(&c.key).map(Vec::as_slice) {
            Some([i]) if !c.key.is_empty() => {
                if let (Some(r), Some(u)) = (rev.get(*i), used.get_mut(*i)) {
                    *u = true;
                    pairs.push(SheetPair {
                        current: c,
                        revised: r.clone(),
                    });
                }
            }
            _ => left_current.push(c),
        }
        if pairs.len() > MAX_PAIRS {
            return Err(invalid(format!("at most {MAX_PAIRS} pairs")));
        }
    }
    let left_revised = rev.into_iter().zip(used).filter(|(_, u)| !u).map(|(r, _)| r).collect();
    Ok((pairs, left_current, left_revised))
}

/// One line of a batch report.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatchResult {
    pub current: SheetRef,
    pub revised: SheetRef,
    /// Differences found (compare) or 0 (overlay).
    pub differences: usize,
    /// The file the result is in.
    pub output: PathBuf,
    /// An error for this pair, when it failed.
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct BatchReport {
    pub results: Vec<BatchResult>,
    pub outputs: Vec<PathBuf>,
}

impl BatchReport {
    pub fn total_differences(&self) -> usize {
        self.results.iter().map(|r| r.differences).sum()
    }
}

fn out_name(dir: &Path, file: &Path, suffix: &str, ext: &str) -> PathBuf {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sheet".into());
    dir.join(format!("{stem}{suffix}.{ext}"))
}

fn check_out(dir: &Path, suffix: &str) -> Result<()> {
    if !dir.is_dir() {
        return Err(invalid(format!("{} is not a folder", dir.display())));
    }
    if suffix.chars().any(|c| "/\\:*?\"<>|".contains(c) || c.is_control()) {
        return Err(invalid("the suffix cannot hold path or wildcard characters"));
    }
    Ok(())
}

/// Compare every pair: for each revised file, a copy in `out_dir` (named with `suffix`) with
/// the changes of its paired pages clouded. A pair that fails is reported, not fatal.
pub fn batch_compare(pairs: &[SheetPair], opts: &CompareOptions, out_dir: &Path, suffix: &str) -> Result<BatchReport> {
    check_out(out_dir, suffix)?;
    if pairs.is_empty() || pairs.len() > MAX_PAIRS {
        return Err(invalid(format!("a batch compares 1 to {MAX_PAIRS} pairs")));
    }
    // Group by revised file, then by current file inside it.
    let mut order: Vec<PathBuf> = Vec::new();
    let mut groups: HashMap<PathBuf, Vec<&SheetPair>> = HashMap::new();
    for p in pairs {
        if !groups.contains_key(&p.revised.file) {
            order.push(p.revised.file.clone());
        }
        groups.entry(p.revised.file.clone()).or_default().push(p);
    }
    let mut report = BatchReport::default();
    for rev in order {
        let list = groups.remove(&rev).unwrap_or_default();
        let out = out_name(out_dir, &rev, suffix, "pdf");
        let fail = |report: &mut BatchReport, e: String| {
            for p in &list {
                report.results.push(BatchResult {
                    current: p.current.clone(),
                    revised: p.revised.clone(),
                    differences: 0,
                    output: out.clone(),
                    error: e.clone(),
                });
            }
        };
        let mut s = match read_pdf(&rev).and_then(|b| Session::from_bytes(b.as_ref().clone(), &out)) {
            Ok(s) => s,
            Err(e) => {
                fail(&mut report, e.to_string());
                continue;
            }
        };
        let mut olds: Vec<PathBuf> = Vec::new();
        for p in &list {
            if !olds.contains(&p.current.file) {
                olds.push(p.current.file.clone());
            }
        }
        let mut lines = Vec::new();
        for old in &olds {
            let mine: Vec<&&SheetPair> = list.iter().filter(|p| &p.current.file == old).collect();
            let o = CompareOptions {
                pairs: mine.iter().map(|p| (p.current.page, p.revised.page)).collect(),
                ..opts.clone()
            };
            match read_pdf(old).and_then(|b| s.compare_with(b, &o)) {
                Ok(r) => {
                    for p in mine {
                        let n = r
                            .regions
                            .iter()
                            .filter(|g| g.page == p.revised.page && g.old_page == p.current.page)
                            .count();
                        lines.push(BatchResult {
                            current: p.current.clone(),
                            revised: p.revised.clone(),
                            differences: n,
                            output: out.clone(),
                            error: String::new(),
                        });
                    }
                }
                Err(e) => {
                    let error = e.to_string();
                    for p in mine {
                        lines.push(BatchResult {
                            current: p.current.clone(),
                            revised: p.revised.clone(),
                            differences: 0,
                            output: out.clone(),
                            error: error.clone(),
                        });
                    }
                }
            }
        }
        match s.save_as(&out, true) {
            Ok(()) => report.outputs.push(out.clone()),
            Err(e) => {
                for l in &mut lines {
                    l.error = e.to_string();
                }
            }
        }
        report.results.extend(lines);
    }
    Ok(report)
}

/// Overlay every pair (current in red, revised in blue by default): one overlay PDF per pair
/// in `out_dir`.
pub fn batch_overlay(
    pairs: &[SheetPair],
    colors: [Color; 2],
    align: OverlayAlign,
    out_dir: &Path,
    suffix: &str,
) -> Result<BatchReport> {
    check_out(out_dir, suffix)?;
    if pairs.is_empty() || pairs.len() > MAX_PAIRS {
        return Err(invalid(format!("a batch overlays 1 to {MAX_PAIRS} pairs")));
    }
    let mut report = BatchReport::default();
    let mut names: HashMap<PathBuf, usize> = HashMap::new();
    for p in pairs {
        let mut out = out_name(out_dir, &p.revised.file, suffix, "pdf");
        // Several pages of one file: number the outputs.
        let n = names.entry(out.clone()).or_default();
        *n += 1;
        if *n > 1 || pairs.iter().filter(|q| q.revised.file == p.revised.file).count() > 1 {
            out = out_dir.join(format!(
                "{}{suffix}-{}.pdf",
                p.revised
                    .file
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                p.revised.page + 1
            ));
        }
        let layer = |file: &Path, page: usize, i: usize, color: Color| -> Result<OverlayLayer> {
            Ok(OverlayLayer {
                bytes: crate::flatten::without_flattened(read_pdf(file)?),
                pages: vec![page],
                color,
                opacity: 1.0,
                name: format!(
                    "{} {}",
                    if i == 0 { "Current" } else { "Revised" },
                    file.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ),
                align: if i == 0 { OverlayAlign::Page } else { align },
                ..OverlayLayer::new(Arc::new(Vec::new()))
            })
        };
        let r = layer(&p.current.file, p.current.page, 0, colors[0])
            .and_then(|a| Ok((a, layer(&p.revised.file, p.revised.page, 1, colors[1])?)))
            .and_then(|(a, b)| overlay_pages(&[a, b], &out));
        let error = match r {
            Ok(_) => {
                report.outputs.push(out.clone());
                String::new()
            }
            Err(e) => e.to_string(),
        };
        report.results.push(BatchResult {
            current: p.current.clone(),
            revised: p.revised.clone(),
            differences: 0,
            output: out,
            error,
        });
    }
    Ok(report)
}

/// The default overlay colours of a batch overlay.
pub fn batch_overlay_colors() -> [Color; 2] {
    [default_color(0), default_color(1)]
}

fn sheet_text(s: &SheetRef) -> String {
    format!(
        "{} p{}",
        s.file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        s.page + 1
    )
}

/// A date and time stamp for reports: `YYYY-MM-DD HH:MM UTC`.
pub fn stamp_now() -> String {
    let (y, m, d) = crate::docutil::today();
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let t = secs % 86_400;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", t / 3600, (t % 3600) / 60)
}

/// The report as CSV.
pub fn report_csv(r: &BatchReport, stamp: Option<&str>) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = String::new();
    if let Some(t) = stamp {
        out.push_str(&format!("{}\r\n", q(&format!("Batch report {t}"))));
    }
    out.push_str("Current,Current Page,Revised,Revised Page,Differences,Result,Error\r\n");
    for l in &r.results {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\r\n",
            q(&l.current.file.display().to_string()),
            l.current.page + 1,
            q(&l.revised.file.display().to_string()),
            l.revised.page + 1,
            l.differences,
            q(&l.output.display().to_string()),
            q(&l.error)
        ));
    }
    out
}

/// The report as a PDF (`page` size in points): one row per pair with a link to its result.
pub fn report_pdf(r: &BatchReport, out: &Path, page: (f64, f64), stamp: Option<&str>) -> Result<()> {
    use crate::synthetic::{SyntheticPage, pdf, text};
    let (w, h) = page;
    crate::blank::check_size(w, h)?;
    let rows_per = (((h - 120.0) / 16.0).floor() as usize).max(1);
    let chunks: Vec<&[BatchResult]> = if r.results.is_empty() {
        vec![&[]]
    } else {
        r.results.chunks(rows_per).collect()
    };
    let clip = |s: String, n: usize| -> String {
        let t: String = s
            .chars()
            .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
            .take(n)
            .collect();
        t
    };
    let mut pages = Vec::new();
    for (k, chunk) in chunks.iter().enumerate() {
        let mut c = text(40.0, h - 50.0, 16.0, "Batch Compare Report");
        if let Some(t) = stamp {
            c.push_str(&text(40.0, h - 68.0, 9.0, &clip(t.to_string(), 80)));
        }
        c.push_str(&text(
            w - 120.0,
            h - 50.0,
            9.0,
            &format!("Page {} of {}", k + 1, chunks.len()),
        ));
        let y0 = h - 95.0;
        c.push_str(&text(40.0, y0, 9.0, "Current"));
        c.push_str(&text(w * 0.36, y0, 9.0, "Revised"));
        c.push_str(&text(w * 0.70, y0, 9.0, "Differences"));
        c.push_str(&text(w * 0.82, y0, 9.0, "Result"));
        for (i, l) in chunk.iter().enumerate() {
            let y = y0 - 16.0 * (i as f64 + 1.0);
            c.push_str(&text(40.0, y, 9.0, &clip(sheet_text(&l.current), 40)));
            c.push_str(&text(w * 0.36, y, 9.0, &clip(sheet_text(&l.revised), 40)));
            let d = if l.error.is_empty() {
                l.differences.to_string()
            } else {
                "error".into()
            };
            c.push_str(&text(w * 0.70, y, 9.0, &d));
            c.push_str(&text(w * 0.82, y, 9.0, "Open"));
        }
        pages.push(SyntheticPage::new(w, h, c));
    }
    let mut s = Session::from_bytes(pdf(&pages), out)?;
    for (k, chunk) in chunks.iter().enumerate() {
        let y0 = h - 95.0;
        for (i, l) in chunk.iter().enumerate() {
            if !l.error.is_empty() {
                continue;
            }
            let y = y0 - 16.0 * (i as f64 + 1.0);
            let target = LinkTarget::File {
                path: l.output.display().to_string(),
                page: Some(l.revised.page),
            };
            s.add_link(
                k,
                Rect::new(w * 0.82 - 2.0, y - 3.0, w * 0.82 + 30.0, y + 10.0),
                &target,
                LinkLook::default(),
            )?;
        }
    }
    s.save_as(out, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_filters_cut_the_sheet_key() {
        assert_eq!(filter_key("@?#", "Plan A-101 rev2").as_deref(), Some("A-101"));
        assert_eq!(filter_key("#", "M201_r3").as_deref(), Some("201"));
        assert_eq!(filter_key("*#", "E-401").as_deref(), Some("E-401"));
        assert_eq!(filter_key("\\##", "#12 x").as_deref(), Some("#12"));
        assert_eq!(filter_key("@#", "no digits"), None);
        assert_eq!(filter_key("", "Same").as_deref(), Some("Same"));
    }
}
