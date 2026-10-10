//! Slip Sheet, the whole run: pair this document's sheets with the revised sheets of one or
//! more new files, then replace them or insert the revisions ahead of them.
//!
//! - Matching ([`MatchBy`], shared with Batch Compare): page label, file name + page index, the
//!   text inside a title-block region (AutoMark), or manual pairs. A match filter cuts the
//!   label at a separator (`number_filter`, the part before it), and a wildcard filter
//!   ([`crate::batch_compare::filter_key`]: `#` digits, `@` letters, `*` non-digits, `?` a
//!   separator) keeps only the part of the key it matches; sheets it does not match take no part.
//! - Apply: replace each matched page in place (its markups, links and bookmarks stay), or
//!   insert the revised page ahead of it and copy the markups forward (the old page stays and
//!   can be stamped SUPERSEDED). Flattened markups can be unflattened first, and the carried
//!   markups flattened after. Links and bookmarks to an old page can be redirected to its
//!   revision.
//! - Leftovers: new sheets that match nothing are appended, left out, or extracted to files.
//! - Report: CSV and a PDF with a link to each result page or extracted file.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use markupcraft_model::{Color, Rect};

use crate::batch_compare::{MatchBy, filter_key};
use crate::flatten::FlattenFilter;
use crate::links::{LinkLook, LinkTarget};
use crate::pages::{ForeignPdf, PagePlan, PageSource, PlanPage};
use crate::stamps::{StampPlace, StampSource};
use crate::{Result, Session, invalid};

/// Most new files in one run.
pub const MAX_NEW_FILES: usize = 2_000;

/// What happens to new sheets that match nothing.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Leftovers {
    /// Added at the end of the document with their labels.
    #[default]
    Append,
    /// Left out.
    Skip,
    /// Each written to its own PDF in this folder (named by its label).
    Extract(PathBuf),
}

/// A manual pair: (old page, index into the new files, page of that file), all 0-based.
pub type ManualPair = (usize, usize, usize);

#[derive(Debug, Clone, PartialEq)]
pub struct SlipRun {
    pub new_files: Vec<PathBuf>,
    pub matching: MatchBy,
    /// Manual pairs (with `MatchBy::Manual`).
    pub pairs: Vec<ManualPair>,
    /// Match on the part of the key before this ("" = the whole key).
    pub number_filter: String,
    /// Wildcard match filter ("" = every sheet, whole key).
    pub filter: String,
    pub match_case: bool,
    /// Insert the revised page ahead of the old one (else replace it).
    pub insert_ahead: bool,
    /// Bring the old page's markups to the revision (replace mode keeps them in place).
    pub carry_markups: bool,
    /// Stamp the kept old pages SUPERSEDED (insert-ahead mode).
    pub superseded: bool,
    /// Unflatten the old page's recoverable flattened markups before carrying them.
    pub unflatten_first: bool,
    /// Flatten the carried markups on the revised pages.
    pub flatten_after: bool,
    /// Links and bookmarks to an old page go to its revision (insert-ahead mode).
    pub redirect_links: bool,
    pub leftovers: Leftovers,
}

impl Default for SlipRun {
    fn default() -> Self {
        Self {
            new_files: Vec::new(),
            matching: MatchBy::PageLabel,
            pairs: Vec::new(),
            number_filter: String::new(),
            filter: String::new(),
            match_case: false,
            insert_ahead: false,
            carry_markups: true,
            superseded: false,
            unflatten_first: false,
            flatten_after: false,
            redirect_links: true,
            leftovers: Leftovers::Append,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlipStatus {
    /// Slip-sheeted.
    Matched,
    /// An old sheet nothing matched (left as it was).
    OldUnmatched,
    /// A new sheet that matched nothing.
    NewUnmatched,
}

impl SlipStatus {
    pub fn name(self) -> &'static str {
        match self {
            SlipStatus::Matched => "matched",
            SlipStatus::OldUnmatched => "old unmatched",
            SlipStatus::NewUnmatched => "new unmatched",
        }
    }
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq)]
pub struct SlipRow {
    pub status: SlipStatus,
    pub key: String,
    /// 0-based page of the document before the run.
    pub old_page: Option<usize>,
    pub old_label: String,
    pub new_file: Option<PathBuf>,
    pub new_page: Option<usize>,
    pub new_label: String,
    /// 0-based page of the document after the run where the revision (or the old sheet) is.
    pub result_page: Option<usize>,
    /// Markups the revised page carries.
    pub markups: usize,
    /// The file an unmatched new sheet was extracted to.
    pub extracted: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlipRunReport {
    pub rows: Vec<SlipRow>,
    /// The document the results are in.
    pub document: PathBuf,
    pub appended: usize,
    pub superseded: usize,
    pub unflattened: usize,
    pub flattened: usize,
    pub links_redirected: usize,
    pub bookmarks_redirected: usize,
}

impl SlipRunReport {
    pub fn matched(&self) -> impl Iterator<Item = &SlipRow> {
        self.rows.iter().filter(|r| r.status == SlipStatus::Matched)
    }
    pub fn count(&self, s: SlipStatus) -> usize {
        self.rows.iter().filter(|r| r.status == s).count()
    }
}

/// One sheet of a new file.
struct NewSheet {
    file: usize,
    page: usize,
    label: String,
    key: Option<String>,
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A key as the run compares it (`None` = this sheet takes no part).
fn make_key(raw: &str, run: &SlipRun) -> Option<String> {
    let mut k = raw.trim().to_string();
    if !run.number_filter.is_empty()
        && let Some(i) = k.find(run.number_filter.as_str())
    {
        k = k.get(..i).unwrap_or_default().trim().to_string();
    }
    if !run.filter.trim().is_empty() {
        let cut = filter_key(run.filter.trim(), &k)?;
        // filter_key returns upper case; keep the original case of that span when asked.
        if run.match_case {
            let up = k.to_uppercase();
            if let Some(i) = up.find(&cut) {
                k = k.get(i..i + cut.len()).map_or(cut.clone(), str::to_string);
            } else {
                k = cut;
            }
        } else {
            k = cut;
        }
    }
    let k = k.trim().to_string();
    if k.is_empty() {
        return None;
    }
    Some(if run.match_case { k } else { k.to_uppercase() })
}

/// Characters a file name cannot hold.
pub(crate) fn safe_file_name(s: &str) -> String {
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
    if t.is_empty() { "sheet".into() } else { t }
}

/// The key of a sheet of `s` (page `page`) for this run.
fn sheet_key(s: &Session, file_stem: &str, page: usize, run: &SlipRun) -> Result<Option<String>> {
    Ok(match &run.matching {
        MatchBy::PageLabel => make_key(
            &s.doc().pages.get(page).map(|p| p.label.clone()).unwrap_or_default(),
            run,
        ),
        MatchBy::FileAndPage => make_key(file_stem, run).map(|k| format!("{k}#{page}")),
        MatchBy::Region { rect } => {
            let r = Rect::new(rect[0], rect[1], rect[2], rect[3]).normalized();
            make_key(&s.text_in_rect(page, r).unwrap_or_default(), run)
        }
        MatchBy::Manual => None,
    })
}

impl Session {
    /// Slip Sheet with every option (see the module notes). Undoable step by step; the report
    /// lists matched, unmatched old and unmatched new sheets.
    pub fn slip_sheet_run(&mut self, run: &SlipRun) -> Result<SlipRunReport> {
        let mut used = HashSet::new();
        self.slip_sheet_run_with(run, &mut used)
    }

    /// As [`Session::slip_sheet_run`], skipping new sheets in `used` ((file, page)) and adding
    /// the ones this run takes (Batch Slip Sheet over many documents shares one pool).
    pub fn slip_sheet_run_with(&mut self, run: &SlipRun, used: &mut HashSet<(usize, usize)>) -> Result<SlipRunReport> {
        if run.new_files.is_empty() || run.new_files.len() > MAX_NEW_FILES {
            return Err(invalid(format!("give 1 to {MAX_NEW_FILES} revised files")));
        }
        if run.superseded && !run.insert_ahead {
            return Err(invalid(
                "only kept old sheets can be stamped Superseded: insert the revisions ahead (insert_ahead)",
            ));
        }
        if let MatchBy::Region { rect } = &run.matching
            && !rect.iter().all(|v| v.is_finite())
        {
            return Err(invalid("the region must be four numbers"));
        }
        if let Leftovers::Extract(dir) = &run.leftovers {
            std::fs::create_dir_all(dir).map_err(|e| crate::EngineError::Io {
                path: dir.display().to_string(),
                source: e,
            })?;
        }
        // The new sheets.
        let mut new_sessions = Vec::new();
        let mut sheets: Vec<NewSheet> = Vec::new();
        for (fi, f) in run.new_files.iter().enumerate() {
            let s = Session::open(f)?;
            let st = stem(f);
            for p in 0..s.page_count() {
                sheets.push(NewSheet {
                    file: fi,
                    page: p,
                    label: s.doc().pages.get(p).map(|x| x.label.clone()).unwrap_or_default(),
                    key: sheet_key(&s, &st, p, run)?,
                });
            }
            new_sessions.push(s);
        }
        // Pairs: (old page, sheet index).
        let old_count = self.page_count();
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut taken = vec![false; sheets.len()];
        for (i, s) in sheets.iter().enumerate() {
            if used.contains(&(s.file, s.page))
                && let Some(t) = taken.get_mut(i)
            {
                *t = true;
            }
        }
        let mut old_keys: Vec<Option<String>> = Vec::with_capacity(old_count);
        if run.matching == MatchBy::Manual {
            if run.pairs.is_empty() {
                return Err(invalid("manual matching needs pairs (old page, new file, new page)"));
            }
            for (op, nf, np) in &run.pairs {
                self.page(*op)?;
                let si = sheets
                    .iter()
                    .position(|s| s.file == *nf && s.page == *np)
                    .ok_or_else(|| invalid(format!("new file {} has no page {}", nf + 1, np + 1)))?;
                if pairs.iter().any(|(o, _)| o == op) || taken.get(si).copied().unwrap_or(true) {
                    return Err(invalid(format!("page {} or its revision is paired twice", op + 1)));
                }
                if let Some(t) = taken.get_mut(si) {
                    *t = true;
                }
                pairs.push((*op, si));
            }
            old_keys = (0..old_count).map(|_| None).collect();
        } else {
            let st = stem(self.path());
            for op in 0..old_count {
                let k = sheet_key(self, &st, op, run)?;
                if let Some(k) = &k {
                    let found = sheets
                        .iter()
                        .enumerate()
                        .find(|(i, s)| !taken.get(*i).copied().unwrap_or(true) && s.key.as_deref() == Some(k.as_str()));
                    if let Some((si, _)) = found {
                        if let Some(t) = taken.get_mut(si) {
                            *t = true;
                        }
                        pairs.push((op, si));
                    }
                }
                old_keys.push(k);
            }
        }
        pairs.sort_unstable();
        for (_, si) in &pairs {
            if let Some(s) = sheets.get(*si) {
                used.insert((s.file, s.page));
            }
        }
        let mut report = SlipRunReport {
            document: self.path().to_path_buf(),
            ..Default::default()
        };
        // Unflatten first.
        if run.unflatten_first {
            for (op, _) in &pairs {
                if self.recoverable_flattened(&[*op]) > 0 {
                    report.unflattened += self.unflatten(&[*op]).unwrap_or(0);
                }
            }
        }
        let markups_on = |s: &Session, p: usize| s.doc().markups.iter().filter(|m| m.page == p).count();
        // where each pair's revision ends up, and where its old page went
        let mut placed: Vec<(usize, usize)> = Vec::new();
        if !pairs.is_empty() {
            if run.insert_ahead {
                let mut plan = PagePlan::identity(old_count);
                for f in &run.new_files {
                    plan.foreign.push(ForeignPdf::open(f)?);
                }
                for (op, si) in pairs.iter().rev() {
                    let Some(s) = sheets.get(*si) else { continue };
                    plan.pages.insert(
                        (*op).min(plan.pages.len()),
                        PlanPage {
                            source: PageSource::Foreign {
                                doc: s.file,
                                page: s.page,
                            },
                            rotate: 0,
                        },
                    );
                }
                self.apply_page_plan("Slip Sheet", &plan)?;
                for (k, (op, _)) in pairs.iter().enumerate() {
                    let new_at = op + k;
                    placed.push((new_at, new_at + 1));
                }
                let labels: Vec<(usize, String)> = pairs
                    .iter()
                    .zip(&placed)
                    .filter_map(|((_, si), (at, _))| {
                        let l = &sheets.get(*si)?.label;
                        (!l.is_empty()).then(|| (*at, l.clone()))
                    })
                    .collect();
                if !labels.is_empty() {
                    self.set_page_labels(&labels)?;
                }
                if run.carry_markups {
                    for (at, old_at) in &placed {
                        let olds: Vec<markupcraft_model::Markup> = self
                            .doc
                            .markups
                            .iter()
                            .filter(|m| m.page == *old_at && m.irt.is_none())
                            .cloned()
                            .collect();
                        for mut m in olds {
                            m.page = *at;
                            let _ = self.add_markup(m);
                        }
                    }
                }
                if run.redirect_links {
                    for (at, old_at) in &placed {
                        for l in self.links() {
                            let to = match &l.target {
                                LinkTarget::Page(p) if p == old_at => Some(LinkTarget::Page(*at)),
                                LinkTarget::Zoomed { page, zoom } if page == old_at => {
                                    Some(LinkTarget::Zoomed { page: *at, zoom: *zoom })
                                }
                                LinkTarget::View { page, rect } if page == old_at => {
                                    Some(LinkTarget::View { page: *at, rect: *rect })
                                }
                                _ => None,
                            };
                            if let Some(t) = to
                                && l.page != *old_at
                                && self.edit_link(&l.id, Some(&t), None, None).is_ok()
                            {
                                report.links_redirected += 1;
                            }
                        }
                        for b in self.bookmarks() {
                            if b.page == Some(*old_at) && self.set_bookmark_page(&b.path, *at).is_ok() {
                                report.bookmarks_redirected += 1;
                            }
                        }
                    }
                }
                if run.superseded {
                    for (_, old_at) in &placed {
                        let crop = self.page(*old_at)?.crop.normalized();
                        let w = (crop.width() * 0.6).clamp(120.0, 600.0);
                        let h = w / 4.0;
                        let (cx, cy) = ((crop.x0 + crop.x1) / 2.0, (crop.y0 + crop.y1) / 2.0);
                        let r = Rect::new(cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0);
                        let src = StampSource::Text {
                            text: "SUPERSEDED".into(),
                            color: Color::RED,
                        };
                        self.place_stamp(*old_at, StampPlace::Rect(r), &src, None, &Default::default(), None)?;
                        report.superseded += 1;
                    }
                }
            } else {
                // Replace in place, one step per new file.
                for fi in 0..run.new_files.len() {
                    let mine: Vec<(usize, usize)> = pairs
                        .iter()
                        .filter_map(|(op, si)| {
                            let s = sheets.get(*si)?;
                            (s.file == fi).then_some((*op, s.page))
                        })
                        .collect();
                    if mine.is_empty() {
                        continue;
                    }
                    let targets: Vec<usize> = mine.iter().map(|x| x.0).collect();
                    let sources: Vec<usize> = mine.iter().map(|x| x.1).collect();
                    if let Some(f) = run.new_files.get(fi) {
                        self.replace_pages(&targets, f, &sources)?;
                    }
                }
                if !run.carry_markups {
                    let ids: Vec<String> = self
                        .doc
                        .markups
                        .iter()
                        .filter(|m| pairs.iter().any(|(op, _)| *op == m.page))
                        .map(|m| m.id.clone())
                        .collect();
                    if !ids.is_empty() {
                        self.delete_markups(&ids, false)?;
                    }
                }
                placed = pairs.iter().map(|(op, _)| (*op, *op)).collect();
            }
            if run.flatten_after {
                let pages: Vec<usize> = placed.iter().map(|(at, _)| *at).collect();
                if pages.iter().any(|p| markups_on(self, *p) > 0) {
                    report.flattened = self
                        .flatten_markups(&FlattenFilter {
                            pages,
                            ..Default::default()
                        })
                        .unwrap_or(0);
                }
            }
        }
        // Rows for the pairs and the unmatched old sheets.
        let shift = |op: usize| -> usize {
            if run.insert_ahead {
                op + pairs.iter().filter(|(o, _)| *o <= op).count()
            } else {
                op
            }
        };
        let old_labels: Vec<String> = (0..old_count)
            .map(|op| {
                self.doc
                    .pages
                    .get(shift(op))
                    .map(|p| p.label.clone())
                    .unwrap_or_default()
            })
            .collect();
        for op in 0..old_count {
            let old_label = old_labels.get(op).cloned().unwrap_or_default();
            match pairs.iter().position(|(o, _)| *o == op) {
                Some(k) => {
                    let si = pairs.get(k).map_or(0, |x| x.1);
                    let s = sheets.get(si);
                    let at = placed.get(k).map_or(op, |x| x.0);
                    report.rows.push(SlipRow {
                        status: SlipStatus::Matched,
                        key: old_keys.get(op).cloned().flatten().unwrap_or_default(),
                        old_page: Some(op),
                        old_label,
                        new_file: s.and_then(|s| run.new_files.get(s.file).cloned()),
                        new_page: s.map(|s| s.page),
                        new_label: s.map(|s| s.label.clone()).unwrap_or_default(),
                        result_page: Some(at),
                        markups: markups_on(self, at),
                        extracted: None,
                    });
                }
                None => report.rows.push(SlipRow {
                    status: SlipStatus::OldUnmatched,
                    key: old_keys.get(op).cloned().flatten().unwrap_or_default(),
                    old_page: Some(op),
                    old_label,
                    new_file: None,
                    new_page: None,
                    new_label: String::new(),
                    result_page: Some(shift(op)),
                    markups: markups_on(self, shift(op)),
                    extracted: None,
                }),
            }
        }
        // Unmatched new sheets.
        let left: Vec<usize> = (0..sheets.len())
            .filter(|i| !taken.get(*i).copied().unwrap_or(true))
            .collect();
        let mut names: HashSet<String> = HashSet::new();
        for (n, si) in left.iter().enumerate() {
            let Some(s) = sheets.get(*si) else { continue };
            let file = run.new_files.get(s.file).cloned();
            let mut row = SlipRow {
                status: SlipStatus::NewUnmatched,
                key: s.key.clone().unwrap_or_default(),
                old_page: None,
                old_label: String::new(),
                new_file: file.clone(),
                new_page: Some(s.page),
                new_label: s.label.clone(),
                result_page: None,
                markups: 0,
                extracted: None,
            };
            match &run.leftovers {
                Leftovers::Append => row.result_page = Some(self.page_count() + n),
                Leftovers::Skip => {}
                Leftovers::Extract(dir) => {
                    let src = file.clone().unwrap_or_default();
                    let out = extract_name(dir, &src, s.page, &s.label, &mut names);
                    if let Some(ns) = new_sessions.get_mut(s.file) {
                        ns.extract_pages(&[s.page], &out, false)?;
                        row.extracted = Some(out);
                    }
                }
            }
            report.rows.push(row);
        }
        if run.leftovers == Leftovers::Append && !left.is_empty() {
            for fi in 0..run.new_files.len() {
                let pages: Vec<usize> = left
                    .iter()
                    .filter_map(|si| sheets.get(*si))
                    .filter(|s| s.file == fi)
                    .map(|s| s.page)
                    .collect();
                if pages.is_empty() {
                    continue;
                }
                let at = self.page_count();
                if let Some(f) = run.new_files.get(fi) {
                    self.insert_file_pages(at, f, Some(&pages))?;
                }
                let labels: Vec<(usize, String)> = left
                    .iter()
                    .filter_map(|si| sheets.get(*si))
                    .filter(|s| s.file == fi)
                    .enumerate()
                    .filter(|(_, s)| !s.label.is_empty())
                    .map(|(k, s)| (at + k, s.label.clone()))
                    .collect();
                if !labels.is_empty() {
                    self.set_page_labels(&labels)?;
                }
                report.appended += pages.len();
            }
        }
        Ok(report)
    }
}

/// Report rows for the sheets of `new_files` no run took (`used` holds (file, page) taken),
/// extracting them to files with `Leftovers::Extract`.
pub fn leftover_sheets(
    new_files: &[PathBuf],
    used: &HashSet<(usize, usize)>,
    leftovers: &Leftovers,
) -> Result<Vec<SlipRow>> {
    let mut rows = Vec::new();
    let mut names: HashSet<String> = HashSet::new();
    for (fi, f) in new_files.iter().enumerate() {
        let mut s = Session::open(f)?;
        for p in 0..s.page_count() {
            if used.contains(&(fi, p)) {
                continue;
            }
            let label = s.doc().pages.get(p).map(|x| x.label.clone()).unwrap_or_default();
            let mut row = SlipRow {
                status: SlipStatus::NewUnmatched,
                key: String::new(),
                old_page: None,
                old_label: String::new(),
                new_file: Some(f.clone()),
                new_page: Some(p),
                new_label: label.clone(),
                result_page: None,
                markups: 0,
                extracted: None,
            };
            if let Leftovers::Extract(dir) = leftovers {
                std::fs::create_dir_all(dir).map_err(|e| crate::EngineError::Io {
                    path: dir.display().to_string(),
                    source: e,
                })?;
                let out = extract_name(dir, f, p, &label, &mut names);
                s.extract_pages(&[p], &out, false)?;
                row.extracted = Some(out);
            }
            rows.push(row);
        }
    }
    Ok(rows)
}

/// A free file name in `dir` for an extracted sheet: its label, else `<file>-<page>`.
fn extract_name(dir: &Path, file: &Path, page: usize, label: &str, names: &mut HashSet<String>) -> PathBuf {
    let base = if label.trim().is_empty() {
        format!("{}-{}", stem(file), page + 1)
    } else {
        label.to_string()
    };
    let mut name = safe_file_name(&base);
    let mut k = 2;
    while !names.insert(name.to_lowercase()) && k < 100_000 {
        name = format!("{} ({k})", safe_file_name(&base));
        k += 1;
    }
    dir.join(format!("{name}.pdf"))
}

fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn file_text(p: Option<&PathBuf>) -> String {
    p.and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The Slip Sheet report as CSV.
pub fn slip_report_csv(r: &SlipRunReport) -> String {
    let n = |v: Option<usize>| v.map(|p| (p + 1).to_string()).unwrap_or_default();
    let mut out =
        String::from("Status,Key,Old Page,Old Label,New File,New Page,New Label,Result Page,Markups,Extracted To\r\n");
    for row in &r.rows {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\r\n",
            q(row.status.name()),
            q(&row.key),
            n(row.old_page),
            q(&row.old_label),
            q(&file_text(row.new_file.as_ref())),
            n(row.new_page),
            q(&row.new_label),
            n(row.result_page),
            row.markups,
            q(&row
                .extracted
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default())
        ));
    }
    out
}

/// The Slip Sheet report as a PDF at `out`: a row per sheet with a link to its result page in
/// the slip-sheeted document (or to the extracted file).
pub fn slip_report_pdf(r: &SlipRunReport, out: &Path) -> Result<()> {
    use crate::synthetic::{SyntheticPage, pdf, text};
    let (w, h) = (792.0, 612.0);
    let rows_per = 28;
    let chunks: Vec<&[SlipRow]> = if r.rows.is_empty() {
        vec![&[]]
    } else {
        r.rows.chunks(rows_per).collect()
    };
    let clip = |s: &str, n: usize| -> String {
        s.chars()
            .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
            .take(n)
            .collect()
    };
    let y_of = |i: usize| h - 110.0 - 16.0 * i as f64;
    let mut pages = Vec::new();
    for (k, chunk) in chunks.iter().enumerate() {
        let mut c = text(40.0, h - 50.0, 16.0, "Slip Sheet Report");
        c.push_str(&text(40.0, h - 68.0, 9.0, &clip(&file_text(Some(&r.document)), 90)));
        c.push_str(&text(
            w - 120.0,
            h - 50.0,
            9.0,
            &format!("Page {} of {}", k + 1, chunks.len()),
        ));
        let hy = h - 92.0;
        for (x, t) in [
            (40.0, "Status"),
            (130.0, "Old sheet"),
            (300.0, "New sheet"),
            (520.0, "Markups"),
            (590.0, "Result"),
        ] {
            c.push_str(&text(x, hy, 9.0, t));
        }
        for (i, row) in chunk.iter().enumerate() {
            let y = y_of(i);
            let old = match row.old_page {
                Some(p) => format!("p{} {}", p + 1, row.old_label),
                None => String::new(),
            };
            let new = match row.new_page {
                Some(p) => format!("{} p{} {}", file_text(row.new_file.as_ref()), p + 1, row.new_label),
                None => String::new(),
            };
            c.push_str(&text(40.0, y, 9.0, row.status.name()));
            c.push_str(&text(130.0, y, 9.0, &clip(&old, 30)));
            c.push_str(&text(300.0, y, 9.0, &clip(&new, 40)));
            c.push_str(&text(520.0, y, 9.0, &row.markups.to_string()));
            let res = match (&row.extracted, row.result_page) {
                (Some(_), _) => "Open file".to_string(),
                (None, Some(p)) => format!("Page {}", p + 1),
                _ => "-".into(),
            };
            c.push_str(&text(590.0, y, 9.0, &res));
        }
        pages.push(SyntheticPage::new(w, h, c));
    }
    let mut s = Session::from_bytes(pdf(&pages), out)?;
    let doc_link = match (out.parent(), r.document.parent()) {
        (Some(a), Some(b)) if a == b => file_text(Some(&r.document)),
        _ => r.document.display().to_string(),
    };
    for (k, chunk) in chunks.iter().enumerate() {
        for (i, row) in chunk.iter().enumerate() {
            let y = y_of(i);
            let target = match (&row.extracted, row.result_page) {
                (Some(f), _) => LinkTarget::File {
                    path: f.display().to_string(),
                    page: None,
                },
                (None, Some(p)) => LinkTarget::File {
                    path: doc_link.clone(),
                    page: Some(p),
                },
                _ => continue,
            };
            s.add_link(
                k,
                Rect::new(588.0, y - 3.0, 680.0, y + 10.0),
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
    fn keys_cut_by_separator_and_wildcard() {
        let run = SlipRun {
            number_filter: " - ".into(),
            ..Default::default()
        };
        assert_eq!(make_key("A-101 - rev 2", &run).as_deref(), Some("A-101"));
        let run = SlipRun {
            filter: "@?#".into(),
            ..Default::default()
        };
        assert_eq!(make_key("Sheet A-101 rev", &run).as_deref(), Some("A-101"));
        assert_eq!(make_key("no number", &run), None);
        assert_eq!(safe_file_name("A/1:"), "A_1_");
    }
}
