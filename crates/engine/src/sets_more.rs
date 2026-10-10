//! More of Sets: sheet tags (sheet number, revision, discipline, sheet type, custom), categories
//! (sheets grouped by discipline from the file name or the sheet number, with editable
//! templates), revision handling (the versions of each sheet, the latest found by revision or
//! date, a wildcard filter for the sheet key), and publishing (one combined PDF with a bookmark
//! per sheet, a package folder, a drawing log) and printing a whole Set.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::batch::{DrawingSet, SetSheet, SetSort, set_sheets};
use crate::printing::PrintJob;
use crate::{EngineError, Result, Session, invalid};

/// Where a sheet's category comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CategoryMode {
    #[default]
    Off,
    FileName,
    SheetNumber,
}

/// A category template: sheets whose key starts with `prefix` belong to `name`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryRule {
    pub prefix: String,
    pub name: String,
}

/// The default discipline categories (US National CAD Standard designators, longest first).
pub fn default_categories() -> Vec<CategoryRule> {
    [
        ("FP", "Fire Protection"),
        ("G", "General"),
        ("H", "Hazardous Materials"),
        ("V", "Survey and Mapping"),
        ("B", "Geotechnical"),
        ("C", "Civil"),
        ("L", "Landscape"),
        ("S", "Structural"),
        ("A", "Architectural"),
        ("I", "Interiors"),
        ("Q", "Equipment"),
        ("F", "Fire Protection"),
        ("P", "Plumbing"),
        ("D", "Process"),
        ("M", "Mechanical"),
        ("E", "Electrical"),
        ("W", "Distributed Energy"),
        ("T", "Telecommunications"),
        ("R", "Resource"),
        ("X", "Other Disciplines"),
        ("Z", "Contractor/Shop Drawings"),
        ("O", "Operations"),
    ]
    .iter()
    .map(|(p, n)| CategoryRule {
        prefix: (*p).into(),
        name: (*n).into(),
    })
    .collect()
}

/// The category of `key` by the rules (the longest matching prefix followed by a non-letter).
pub fn category_of(key: &str, rules: &[CategoryRule]) -> String {
    let k = key.trim().to_uppercase();
    rules
        .iter()
        .filter(|r| {
            let p = r.prefix.to_uppercase();
            !p.is_empty() && k.starts_with(&p) && !k[p.len()..].chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        })
        .max_by_key(|r| r.prefix.len())
        .map(|r| r.name.clone())
        .unwrap_or_else(|| "Uncategorized".into())
}

/// The NCS sheet type of a sheet number: the first digit after the discipline letters.
pub fn sheet_type(number: &str) -> String {
    let digit = number.chars().find(char::is_ascii_digit);
    match digit {
        Some('0') => "General",
        Some('1') => "Plans",
        Some('2') => "Elevations",
        Some('3') => "Sections",
        Some('4') => "Large-Scale Views",
        Some('5') => "Details",
        Some('6') => "Schedules and Diagrams",
        Some('7') | Some('8') => "User Defined",
        Some('9') => "3D Representations",
        _ => "",
    }
    .to_string()
}

/// The revision written in a file name (`rev 2`, `Rev2`, `R3`, `_r1`), as a number.
pub fn revision_of(name: &str) -> Option<u32> {
    let lower = name.to_lowercase();
    let bytes: Vec<char> = lower.chars().collect();
    let mut best = None;
    for i in 0..bytes.len() {
        let at_word = i == 0 || !bytes.get(i - 1).is_some_and(|c| c.is_ascii_alphanumeric());
        if !at_word {
            continue;
        }
        let rest: String = bytes.get(i..).unwrap_or(&[]).iter().collect();
        let tail = rest
            .strip_prefix("revision")
            .or_else(|| rest.strip_prefix("rev"))
            .or_else(|| rest.strip_prefix('r'));
        if let Some(t) = tail {
            let t = t.trim_start_matches([' ', '.', '-', '_']);
            let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
            if !digits.is_empty() {
                best = digits.parse().ok();
            }
        }
    }
    best
}

/// The sheet number of a sheet: its page label, else the start of the file name up to the
/// first space.
pub fn sheet_number(sheet: &SetSheet, files: &[PathBuf]) -> String {
    if !sheet.label.trim().is_empty() {
        return sheet.label.trim().to_string();
    }
    files
        .get(sheet.file)
        .and_then(|f| f.file_stem())
        .map(|s| s.to_string_lossy().split_whitespace().next().unwrap_or("").to_string())
        .unwrap_or_default()
}

/// The key of a sheet's custom tags in the set file.
pub fn tag_key(sheet: &SetSheet, files: &[PathBuf]) -> String {
    let name = files
        .get(sheet.file)
        .and_then(|f| f.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{name}#{}", sheet.page + 1)
}

/// A sheet with its tags.
#[derive(Debug, Clone, PartialEq)]
pub struct TaggedSheet {
    pub sheet: SetSheet,
    pub file: PathBuf,
    /// Sheet Number, Revision, Discipline, Sheet Type and custom tags.
    pub tags: BTreeMap<String, String>,
}

/// Every sheet of the set with its derived and custom tags.
pub fn tagged_sheets(set: &DrawingSet, sort: SetSort, rules: &[CategoryRule]) -> (Vec<TaggedSheet>, Vec<String>) {
    tagged_sheets_with(set, sort, rules, true)
}

/// [`tagged_sheets`]; with `auto_tags` false the discipline and sheet type are not derived
/// from the sheet number (the sheet number, revision and custom tags stay).
pub fn tagged_sheets_with(
    set: &DrawingSet,
    sort: SetSort,
    rules: &[CategoryRule],
    auto_tags: bool,
) -> (Vec<TaggedSheet>, Vec<String>) {
    let (sheets, errors) = set_sheets(set, sort);
    let out = sheets
        .into_iter()
        .map(|sh| {
            let file = set.files.get(sh.file).cloned().unwrap_or_default();
            let number = sheet_number(&sh, &set.files);
            let mut tags = BTreeMap::new();
            tags.insert("Sheet Number".to_string(), number.clone());
            let fname = file
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if let Some(r) = revision_of(&fname) {
                tags.insert("Revision".to_string(), r.to_string());
            }
            if auto_tags {
                tags.insert("Discipline".to_string(), category_of(&number, rules));
                let ty = sheet_type(&number);
                if !ty.is_empty() {
                    tags.insert("Sheet Type".to_string(), ty);
                }
            }
            if let Some(custom) = set.tags.get(&tag_key(&sh, &set.files)) {
                for (k, v) in custom {
                    tags.insert(k.clone(), v.clone());
                }
            }
            TaggedSheet { sheet: sh, file, tags }
        })
        .collect();
    (out, errors)
}

/// The sheets grouped by category (in category order of the rules, then Uncategorized).
pub fn categorize(
    sheets: &[TaggedSheet],
    mode: CategoryMode,
    rules: &[CategoryRule],
) -> Vec<(String, Vec<TaggedSheet>)> {
    let mut groups: Vec<(String, Vec<TaggedSheet>)> = Vec::new();
    for s in sheets {
        let key = match mode {
            CategoryMode::Off => String::new(),
            CategoryMode::FileName => category_of(
                &s.file
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                rules,
            ),
            CategoryMode::SheetNumber => category_of(s.tags.get("Sheet Number").map_or("", String::as_str), rules),
        };
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.push(s.clone()),
            None => groups.push((key, vec![s.clone()])),
        }
    }
    groups.sort_by_key(|(k, _)| rules.iter().position(|r| &r.name == k).unwrap_or(usize::MAX));
    groups
}

/// The versions of one sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetVersions {
    pub key: String,
    /// Indices into the tagged sheets, oldest first.
    pub versions: Vec<usize>,
}

impl SheetVersions {
    pub fn latest(&self) -> Option<usize> {
        self.versions.last().copied()
    }
}

/// Group the sheets into versions of the same sheet: by sheet number, or by the part of the
/// file name a wildcard `filter` keeps (see `batch_compare::filter_key`). The newest version
/// (highest revision, then newest file) comes last.
pub fn revisions(sheets: &[TaggedSheet], filter: &str) -> Vec<SheetVersions> {
    let mut groups: Vec<SheetVersions> = Vec::new();
    for (i, s) in sheets.iter().enumerate() {
        let key = if filter.trim().is_empty() {
            s.tags.get("Sheet Number").cloned().unwrap_or_default().to_uppercase()
        } else {
            let name = s
                .file
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            crate::batch_compare::filter_key(filter, &name).unwrap_or_default()
        };
        if key.is_empty() {
            continue;
        }
        match groups.iter_mut().find(|g| g.key == key) {
            Some(g) => g.versions.push(i),
            None => groups.push(SheetVersions { key, versions: vec![i] }),
        }
    }
    let modified = |s: &TaggedSheet| std::fs::metadata(&s.file).and_then(|m| m.modified()).ok();
    for g in &mut groups {
        g.versions.sort_by(|a, b| {
            let (sa, sb) = (sheets.get(*a), sheets.get(*b));
            let rev = |s: Option<&TaggedSheet>| {
                s.and_then(|s| s.tags.get("Revision"))
                    .and_then(|r| r.parse::<u32>().ok())
            };
            rev(sa)
                .cmp(&rev(sb))
                .then_with(|| sa.and_then(modified).cmp(&sb.and_then(modified)))
        });
    }
    groups
}

/// What carrying a revision forward did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CarryReport {
    /// Sheets that got a newer revision among the new files.
    pub sheets: usize,
    /// Markups copied onto the new revisions.
    pub markups: usize,
    /// Old revisions stamped SUPERSEDED.
    pub stamped: usize,
}

/// New revisions joined a Set (`new_files`, already in the set): for each sheet whose newest
/// version is in a new file, copy the previous version's markups onto it and (or) stamp the
/// previous version SUPERSEDED (Preferences > Sets). Files are saved in place.
pub fn carry_forward(
    set: &DrawingSet,
    filter: &str,
    new_files: &[PathBuf],
    copy_markups: bool,
    stamp_superseded: bool,
) -> Result<CarryReport> {
    let mut report = CarryReport::default();
    if !copy_markups && !stamp_superseded {
        return Ok(report);
    }
    let (tagged, _) = tagged_sheets(set, SetSort::FileOrder, &default_categories());
    let is_new = |p: &Path| new_files.iter().any(|n| n == p);
    let mut sessions: BTreeMap<PathBuf, Session> = BTreeMap::new();
    let mut touched: Vec<PathBuf> = Vec::new();
    for g in revisions(&tagged, filter) {
        let n = g.versions.len();
        if n < 2 {
            continue;
        }
        let (Some(new), Some(old)) = (
            g.versions.get(n - 1).and_then(|i| tagged.get(*i)),
            g.versions.get(n - 2).and_then(|i| tagged.get(*i)),
        ) else {
            continue;
        };
        if !is_new(&new.file) || is_new(&old.file) || new.file == old.file {
            continue;
        }
        report.sheets += 1;
        for f in [&old.file, &new.file] {
            if !sessions.contains_key(f) {
                sessions.insert(f.clone(), Session::open(f)?);
            }
        }
        if copy_markups {
            let carried: Vec<markupcraft_model::Markup> = sessions
                .get(&old.file)
                .map(|s| {
                    s.doc()
                        .markups
                        .iter()
                        .filter(|m| m.page == old.sheet.page)
                        .take(10_000)
                        .cloned()
                        .map(|mut m| {
                            m.page = new.sheet.page;
                            m
                        })
                        .collect()
                })
                .unwrap_or_default();
            if !carried.is_empty()
                && let Some(s) = sessions.get_mut(&new.file)
            {
                report.markups += s.add_new_markups("Copy Markups to Revision", carried)?.len();
                touched.push(new.file.clone());
            }
        }
        if stamp_superseded && let Some(s) = sessions.get_mut(&old.file) {
            let crop = s.page(old.sheet.page)?.crop.normalized();
            let at = markupcraft_model::Point::new(crop.x1 - 110.0, crop.y1 - 40.0);
            s.place_stamp(
                old.sheet.page,
                crate::stamps::StampPlace::Center(at),
                &crate::stamps::StampSource::Text {
                    text: "SUPERSEDED".into(),
                    color: markupcraft_model::Color::rgb(0.8, 0.0, 0.0),
                },
                None,
                &BTreeMap::new(),
                None,
            )?;
            report.stamped += 1;
            touched.push(old.file.clone());
        }
    }
    touched.sort();
    touched.dedup();
    for f in touched {
        if let Some(s) = sessions.get_mut(&f) {
            s.save(false)?;
        }
    }
    Ok(report)
}

/// What Publish writes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PublishReport {
    pub pages: usize,
    pub files: Vec<PathBuf>,
    pub log_rows: usize,
}

/// Publish a Set as one PDF (`out`): every sheet in set order (latest versions only when
/// `latest_only`), a bookmark per sheet titled by its sheet number.
pub fn publish_combined(set: &DrawingSet, out: &Path, latest_only: bool, filter: &str) -> Result<PublishReport> {
    let rules = default_categories();
    let (sheets, errors) = tagged_sheets(set, SetSort::FileOrder, &rules);
    if let Some(e) = errors.first() {
        return Err(invalid(e.clone()));
    }
    let keep: Vec<usize> = if latest_only {
        let mut v: Vec<usize> = revisions(&sheets, filter)
            .iter()
            .filter_map(SheetVersions::latest)
            .collect();
        v.sort_unstable();
        v
    } else {
        (0..sheets.len()).collect()
    };
    if keep.is_empty() {
        return Err(invalid("the set has no sheets"));
    }
    // Start from the first file and append the others' pages; then keep the chosen sheets.
    let first = sheets
        .first()
        .map(|s| s.file.clone())
        .ok_or_else(|| invalid("the set has no sheets"))?;
    let mut s = Session::open(&first)?;
    let mut offsets: BTreeMap<PathBuf, usize> = BTreeMap::new();
    offsets.insert(first.clone(), 0);
    for f in &set.files {
        if offsets.contains_key(f) {
            continue;
        }
        let at = s.page_count();
        s.insert_file_pages(at, f, None)?;
        offsets.insert(f.clone(), at);
    }
    let wanted: Vec<usize> = keep
        .iter()
        .filter_map(|i| sheets.get(*i))
        .filter_map(|t| offsets.get(&t.file).map(|o| o + t.sheet.page))
        .collect();
    let drop: Vec<usize> = (0..s.page_count()).filter(|p| !wanted.contains(p)).collect();
    if !drop.is_empty() {
        s.delete_pages(&drop)?;
    }
    s.clear_bookmarks().ok();
    let mut kept_sorted = wanted.clone();
    kept_sorted.sort_unstable();
    for (new_page, old) in kept_sorted.iter().enumerate() {
        if let Some(t) = keep
            .iter()
            .filter_map(|i| sheets.get(*i))
            .find(|t| offsets.get(&t.file).map(|o| o + t.sheet.page) == Some(*old))
        {
            let title = t
                .tags
                .get("Sheet Number")
                .cloned()
                .unwrap_or_else(|| format!("Page {}", new_page + 1));
            let title = if title.is_empty() {
                format!("Page {}", new_page + 1)
            } else {
                title
            };
            s.add_bookmark(&[], None, &title, new_page)?;
        }
    }
    s.save_as(out, true)?;
    Ok(PublishReport {
        pages: s.page_count(),
        files: vec![out.to_path_buf()],
        log_rows: 0,
    })
}

/// The drawing log as CSV: Sheet Number, Discipline, Sheet Type, Revision, File, Page.
pub fn drawing_log(set: &DrawingSet) -> (String, usize) {
    let rules = default_categories();
    let (sheets, _) = tagged_sheets(set, SetSort::Label, &rules);
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = String::from("Sheet Number,Discipline,Sheet Type,Revision,File,Page\r\n");
    for t in &sheets {
        let g = |k: &str| t.tags.get(k).cloned().unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{},{}\r\n",
            q(&g("Sheet Number")),
            q(&g("Discipline")),
            q(&g("Sheet Type")),
            q(&g("Revision")),
            q(&t.file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()),
            t.sheet.page + 1
        ));
    }
    (out, sheets.len())
}

/// Package a Set: its files copied into `dir` with a set file and the drawing log.
pub fn publish_package(set: &DrawingSet, dir: &Path) -> Result<PublishReport> {
    std::fs::create_dir_all(dir).map_err(|e| EngineError::Io {
        path: dir.display().to_string(),
        source: e,
    })?;
    let mut copied = Vec::new();
    for f in &set.files {
        let name = f.file_name().ok_or_else(|| invalid("a file has no name"))?;
        let to = dir.join(name);
        std::fs::copy(f, &to).map_err(|e| EngineError::Io {
            path: f.display().to_string(),
            source: e,
        })?;
        copied.push(to);
    }
    let packaged = DrawingSet {
        name: set.name.clone(),
        files: copied.clone(),
        tags: set.tags.clone(),
    };
    let set_name = if set.name.trim().is_empty() {
        "Set".to_string()
    } else {
        set.name.trim().to_string()
    };
    let set_path = dir.join(format!("{set_name}.pcset"));
    crate::batch::save_set(&set_path, &packaged)?;
    let (log, rows) = drawing_log(&packaged);
    let log_path = dir.join("Drawing Log.csv");
    crate::write_atomic(&log_path, log.as_bytes())?;
    copied.push(set_path);
    copied.push(log_path);
    Ok(PublishReport {
        pages: 0,
        files: copied,
        log_rows: rows,
    })
}

/// Print a whole Set: its sheets combined and laid out by `job` into one print-ready PDF.
pub fn print_set(set: &DrawingSet, job: &PrintJob, out: &Path) -> Result<usize> {
    let tmp = markupcraft_revu::fsio::temp_dir().join(format!(
        "markupcraft-set-print-{}.pdf",
        markupcraft_revu::fsio::process_id()
    ));
    publish_combined(set, &tmp, false, "")?;
    let s = Session::open(&tmp)?;
    let mut j = job.clone();
    j.settings.pages.clear();
    j.region = None;
    let n = s.print_job_to_pdf(out, &j);
    let _ = std::fs::remove_file(&tmp);
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};

    #[test]
    fn tags_categories_and_types() {
        let rules = default_categories();
        assert_eq!(category_of("A-101", &rules), "Architectural");
        assert_eq!(category_of("FP-201", &rules), "Fire Protection");
        assert_eq!(category_of("AB-1", &rules), "Uncategorized");
        assert_eq!(sheet_type("M-301"), "Sections");
        assert_eq!(revision_of("A-101 Plan Rev 3"), Some(3));
        assert_eq!(revision_of("A-101_R2"), Some(2));
        assert_eq!(revision_of("Arch plans"), None);
    }

    #[test]
    fn revisions_publish_package_and_print() {
        let d = std::env::temp_dir().join(format!("markupcraft-setsmore-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let sheet = |name: &str, t: &str| {
            let p = d.join(name);
            std::fs::write(&p, pdf(&[SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 14.0, t))])).unwrap();
            p
        };
        let set = DrawingSet {
            name: "Job".into(),
            files: vec![
                sheet("A-101 Plan.pdf", "OLD"),
                sheet("A-101 Plan Rev 2.pdf", "NEW"),
                sheet("M-201 Mech.pdf", "MECH"),
            ],
            tags: [(
                "M-201 Mech.pdf#1".to_string(),
                [("Phase".to_string(), "CD".to_string())].into_iter().collect(),
            )]
            .into_iter()
            .collect(),
        };
        let rules = default_categories();
        let (sheets, errors) = tagged_sheets(&set, SetSort::FileOrder, &rules);
        assert!(errors.is_empty());
        assert_eq!(sheets[2].tags.get("Phase").map(String::as_str), Some("CD"));
        assert_eq!(sheets[1].tags.get("Revision").map(String::as_str), Some("2"));
        let cats = categorize(&sheets, CategoryMode::FileName, &rules);
        assert_eq!(
            cats.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
            ["Architectural", "Mechanical"]
        );
        let revs = revisions(&sheets, "@?#");
        let a = revs.iter().find(|g| g.key == "A-101").unwrap();
        assert_eq!(a.versions.len(), 2);
        assert_eq!(a.latest(), Some(1), "Rev 2 is the latest");
        let out = d.join("published.pdf");
        let r = publish_combined(&set, &out, true, "@?#").unwrap();
        assert_eq!(r.pages, 2, "latest versions only");
        let p = Session::open(&out).unwrap();
        assert!(p.page_text(0).unwrap().contains("NEW"));
        assert_eq!(p.bookmarks().len(), 2);
        let pkg = publish_package(&set, &d.join("package")).unwrap();
        assert_eq!(pkg.log_rows, 3);
        assert!(d.join("package").join("Drawing Log.csv").is_file());
        assert_eq!(
            print_set(&set, &PrintJob::default(), &d.join("set-print.pdf")).unwrap(),
            3
        );
    }
}
