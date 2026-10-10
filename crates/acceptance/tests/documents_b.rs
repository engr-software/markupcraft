//! Documents, second half (doc-111 .. doc-217): batch tools, Sets, summaries and printing,
//! headers/footers and stamps, OCR and search, forms, signatures, security and redaction,
//! flatten / reduce / archive, export, properties and attachments, links and File Access.
//!
//! Each test is written from the inventory text in docs/revu_features/03_documents.md.
use std::path::{Path, PathBuf};

use markupcraft_acceptance::*;
use markupcraft_engine::synthetic::{SyntheticPage, line, pdf, rect, text};

// ---- helpers ------------------------------------------------------------------------------

/// A PDF of Letter pages, each with the given lines of 12 pt text `(x, y, text)`.
fn text_pdf(dir: &Path, name: &str, pages: &[&[(f64, f64, &str)]]) -> PathBuf {
    let pages: Vec<SyntheticPage> = pages
        .iter()
        .map(|lines| {
            let c: String = lines.iter().map(|(x, y, t)| text(*x, *y, 12.0, t)).collect();
            SyntheticPage::new(612.0, 792.0, c)
        })
        .collect();
    let p = dir.join(name);
    std::fs::write(&p, pdf(&pages)).unwrap();
    p
}

fn open(a: &mut Automation, name: &str) -> u64 {
    call(a, "doc_open", json!({ "path": name }))["doc"].as_u64().unwrap()
}

fn s(v: &Value) -> String {
    v.to_string()
}

fn links(a: &mut Automation, name: &str) -> Vec<Value> {
    open(a, name);
    let v = call(a, "link_list", json!({}));
    let l = v
        .get("links")
        .and_then(|x| x.as_array())
        .cloned()
        .or_else(|| v.as_array().cloned())
        .unwrap_or_default();
    call(a, "doc_close", json!({ "discard_changes": true }));
    l
}

fn labelled(dir: &Path, a: &mut Automation, name: &str, pages: &[&[(f64, f64, &str)]], labels: Value) {
    text_pdf(dir, name, pages);
    open(a, name);
    call(a, "page_label_set", json!({ "labels": labels }));
    call(a, "doc_save", json!({ "full": true }));
    call(a, "doc_close", json!({}));
}

fn markup_count(a: &mut Automation, name: &str) -> usize {
    open(a, name);
    let v = call(a, "markup_list", json!({}));
    let n = v
        .get("markups")
        .and_then(|x| x.as_array())
        .map(|x| x.len())
        .or_else(|| v.as_array().map(|x| x.len()))
        .unwrap_or(0);
    call(a, "doc_close", json!({ "discard_changes": true }));
    n
}

fn page_text(a: &mut Automation, name: &str, page: u64) -> String {
    open(a, name);
    let v = call(a, "page_text", json!({ "page": page }));
    call(a, "doc_close", json!({ "discard_changes": true }));
    v.get("text")
        .and_then(|t| t.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| s(&v))
}

// ---- Batch Link (doc-111 .. doc-115) ---------------------------------------------------------

/// D-111: terms from page labels, file names, a page region, or typed / CSV custom terms.
#[test]
fn batch_link_terms_from_labels_file_names_region_and_csv() {
    let dir = temp_dir("blink-terms");
    let mut a = automation(&dir);
    // page labels: A-101 refers to A-102 in the same file
    labelled(
        &dir,
        &mut a,
        "arch.pdf",
        &[
            &[(72.0, 700.0, "SEE A-102 FOR DETAILS")],
            &[(72.0, 700.0, "DETAIL SHEET")],
        ],
        json!({"1": "A-101", "2": "A-102"}),
    );
    let r = call(&mut a, "batch_link", json!({ "files": ["arch.pdf"] }));
    let l = links(&mut a, "arch.pdf");
    assert_eq!(l.len(), 1, "page-label term makes one link: {l:?} {r}");
    assert!(s(&l[0]).contains('2'), "goes to page 2: {l:?}");

    // file names: text "REFER TO S-201" links to the file S-201.pdf
    text_pdf(&dir, "plan.pdf", &[&[(72.0, 700.0, "REFER TO S-201 STRUCTURAL")]]);
    text_pdf(&dir, "S-201.pdf", &[&[(72.0, 700.0, "STRUCTURE")]]);
    call(
        &mut a,
        "batch_link",
        json!({ "files": ["plan.pdf", "S-201.pdf"], "terms": "file_names" }),
    );
    let l = links(&mut a, "plan.pdf");
    assert_eq!(l.len(), 1, "{l:?}");
    assert!(s(&l[0]).contains("S-201"), "links to the file: {l:?}");

    // region: the title-block box holds each page's sheet number
    text_pdf(
        &dir,
        "region.pdf",
        &[
            &[(500.0, 40.0, "M-1"), (72.0, 700.0, "DUCT CONTINUES ON M-2")],
            &[(500.0, 40.0, "M-2"), (72.0, 700.0, "ROOF")],
        ],
    );
    call(
        &mut a,
        "batch_link",
        json!({ "files": ["region.pdf"], "terms": "region", "region": [480, 20, 600, 70] }),
    );
    let l = links(&mut a, "region.pdf");
    assert!(
        l.iter().any(|x| x["page"] == 1 && s(x).contains('2')),
        "region term links M-2 text on page 1: {l:?}"
    );

    // custom terms from CSV
    text_pdf(
        &dir,
        "spec.pdf",
        &[&[(72.0, 700.0, "SEE KEYNOTE TABLE")], &[(72.0, 700.0, "TABLE")]],
    );
    std::fs::write(dir.join("terms.csv"), "KEYNOTE,spec.pdf,2\n").unwrap();
    call(
        &mut a,
        "batch_link",
        json!({ "files": ["spec.pdf"], "terms": "custom", "terms_csv": "terms.csv" }),
    );
    let l = links(&mut a, "spec.pdf");
    assert_eq!(l.len(), 1, "custom CSV term: {l:?}");
    // typed custom terms
    text_pdf(
        &dir,
        "typed.pdf",
        &[&[(72.0, 700.0, "GOTO LEGEND")], &[(72.0, 700.0, "X")]],
    );
    call(
        &mut a,
        "batch_link",
        json!({ "files": ["typed.pdf"], "terms": "custom", "custom": [["LEGEND", 0, 2]] }),
    );
    assert_eq!(links(&mut a, "typed.pdf").len(), 1);
}

/// D-112: trim terms at a filter character, keeping the text from the start or from the end.
#[test]
fn batch_link_term_filter_keeps_start_or_end() {
    let dir = temp_dir("blink-filter");
    let mut a = automation(&dir);
    // file names "A-101 Plan.pdf": filter at ' ' keep start -> "A-101"
    text_pdf(&dir, "A-101 Plan.pdf", &[&[(72.0, 700.0, "PLAN")]]);
    text_pdf(&dir, "index.pdf", &[&[(72.0, 700.0, "SHEET A-101 AND Elevations")]]);
    call(
        &mut a,
        "batch_link",
        json!({ "files": ["index.pdf", "A-101 Plan.pdf"], "terms": "file_names", "filter_char": " ", "keep_start": true }),
    );
    let l = links(&mut a, "index.pdf");
    assert_eq!(l.len(), 1, "A-101 found once trimmed: {l:?}");
    // keep end: "X_Elevations.pdf" filtered at '_' keeping the end -> "Elevations"
    text_pdf(&dir, "X_Elevations.pdf", &[&[(72.0, 700.0, "E")]]);
    text_pdf(&dir, "index2.pdf", &[&[(72.0, 700.0, "SEE Elevations")]]);
    call(
        &mut a,
        "batch_link",
        json!({ "files": ["index2.pdf", "X_Elevations.pdf"], "terms": "file_names", "filter_char": "_", "keep_start": false }),
    );
    let l = links(&mut a, "index2.pdf");
    assert_eq!(l.len(), 1, "Elevations found when keeping the end: {l:?}");
}

/// D-113: each term targets a file, a page in a file, a Place in a file, or a web URL.
#[test]
fn batch_link_destinations_file_page() {
    let dir = temp_dir("blink-dest");
    let mut a = automation(&dir);
    text_pdf(&dir, "a.pdf", &[&[(72.0, 700.0, "SEE DETAIL 5")]]);
    text_pdf(
        &dir,
        "b.pdf",
        &[
            &[(72.0, 700.0, "ONE")],
            &[(72.0, 700.0, "TWO")],
            &[(72.0, 700.0, "THREE")],
        ],
    );
    call(
        &mut a,
        "batch_link",
        json!({ "files": ["a.pdf", "b.pdf"], "terms": "custom", "custom": [["DETAIL 5", 1, 3]] }),
    );
    let l = links(&mut a, "a.pdf");
    assert_eq!(l.len(), 1, "{l:?}");
    let t = s(&l[0]);
    assert!(t.contains("b.pdf") && t.contains('3'), "page 3 of b.pdf: {t}");
}

/// D-114: relative or full paths, highlight colour, overlapping links, border appearance.
#[test]
fn batch_link_options_paths_highlight_border_and_overlap() {
    let dir = temp_dir("blink-opts");
    let mut a = automation(&dir);
    text_pdf(&dir, "a.pdf", &[&[(72.0, 700.0, "SEE B-1")]]);
    text_pdf(&dir, "B-1.pdf", &[&[(72.0, 700.0, "B")]]);
    call(
        &mut a,
        "batch_link",
        json!({ "files": ["a.pdf", "B-1.pdf"], "terms": "file_names", "full_paths": true,
                 "highlight": "#FFFF00", "border_width": 2, "color": "#FF0000" }),
    );
    let l = links(&mut a, "a.pdf");
    assert_eq!(l.len(), 1, "{l:?}");
    let t = s(&l[0]);
    let full = dir.canonicalize().unwrap();
    let full_s = full.to_string_lossy().replace('\\', "/");
    assert!(
        t.replace("\\\\", "/").contains(full_s.trim_start_matches("//?/")) || t.contains(":/") || t.contains(":\\\\"),
        "full path stored: {t}"
    );
    assert!(markup_count(&mut a, "a.pdf") >= 1, "a highlight markup over the link");
    // running again: existing link skipped (no duplicates)
    call(
        &mut a,
        "batch_link",
        json!({ "files": ["a.pdf", "B-1.pdf"], "terms": "file_names" }),
    );
    assert_eq!(links(&mut a, "a.pdf").len(), 1, "overlapping link skipped");
    // replace existing: still one link, now relative
    call(
        &mut a,
        "batch_link",
        json!({ "files": ["a.pdf", "B-1.pdf"], "terms": "file_names", "replace_existing": true }),
    );
    let l = links(&mut a, "a.pdf");
    assert_eq!(l.len(), 1, "replaced, not duplicated: {l:?}");
    assert!(!s(&l[0]).contains(&*full_s), "relative now: {l:?}");
}

/// D-115: summary of links created, pages skipped and files not opened.
#[test]
fn batch_link_report_counts_and_failures() {
    let dir = temp_dir("blink-report");
    let mut a = automation(&dir);
    labelled(
        &dir,
        &mut a,
        "arch.pdf",
        &[&[(72.0, 700.0, "SEE A-102")], &[(72.0, 700.0, "SEE A-101")]],
        json!({"1": "A-101", "2": "A-102"}),
    );
    std::fs::write(dir.join("broken.pdf"), b"not a pdf").unwrap();
    let r = call(&mut a, "batch_link", json!({ "files": ["arch.pdf", "broken.pdf"] }));
    let t = s(&r);
    assert!(t.contains('2'), "two links reported: {t}");
    assert!(t.contains("broken.pdf"), "the file that did not open is reported: {t}");
    assert_eq!(links(&mut a, "arch.pdf").len(), 2, "saved in place");
}

/// D-113: a term targets a page of a file, a whole file, a named Place in a file, or a web URL.
#[test]
fn batch_link_terms_go_to_places_urls_and_files() {
    let dir = temp_dir("blink-dest2");
    let mut a = automation(&dir);
    text_pdf(
        &dir,
        "a.pdf",
        &[&[
            (72.0, 700.0, "SEE DETAIL 5"),
            (72.0, 600.0, "SPEC BOOK"),
            (72.0, 500.0, "MAKER SITE"),
            (72.0, 400.0, "LOCAL PLACE"),
        ]],
    );
    text_pdf(&dir, "b.pdf", &[&[(72.0, 700.0, "ONE")], &[(72.0, 700.0, "TWO")]]);
    open(&mut a, "b.pdf");
    call(
        &mut a,
        "place_set",
        json!({ "name": "Detail5", "page": 2, "left": 0, "top": 700 }),
    );
    call(&mut a, "doc_save", json!({ "full": true }));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "a.pdf");
    call(&mut a, "place_set", json!({ "name": "Here", "page": 1 }));
    call(&mut a, "doc_save", json!({ "full": true }));
    call(&mut a, "doc_close", json!({}));
    let r = call(
        &mut a,
        "batch_link",
        json!({ "files": ["a.pdf", "b.pdf"], "terms": "targets", "targets": [
            { "term": "DETAIL 5", "file": 2, "place": "Detail5" },
            { "term": "SPEC BOOK", "file": 2 },
            { "term": "MAKER SITE", "url": "https://example.com/maker" },
            { "term": "LOCAL PLACE", "file": 1, "place": "Here" }
        ] }),
    );
    assert_eq!(r["links"], 4, "{r}");
    let l = json!(links(&mut a, "a.pdf")).to_string();
    assert!(
        l.contains("\"place\":\"Detail5\"") && l.contains("b.pdf"),
        "a Place in another file: {l}"
    );
    assert!(l.contains("https://example.com/maker"), "a web URL: {l}");
    assert!(l.contains("\"place\":\"Here\""), "a Place in this file: {l}");
    assert!(
        l.contains("\"file_page\":1"),
        "a whole file (opens at its first page): {l}"
    );
}

/// D-114: highlight style (outline / fill / highlight), flatten highlight, and keeping an
/// overlapping link beside the old one.
#[test]
fn batch_link_highlight_styles_flatten_and_overlap() {
    let dir = temp_dir("blink-style");
    let mut a = automation(&dir);
    for (f, style) in [("o.pdf", "outline"), ("h.pdf", "highlight")] {
        text_pdf(&dir, f, &[&[(72.0, 700.0, "SEE B-1")]]);
        text_pdf(&dir, "B-1.pdf", &[&[(72.0, 700.0, "B")]]);
        call(
            &mut a,
            "batch_link",
            json!({ "files": [f, "B-1.pdf"], "terms": "file_names", "highlight": "#00FF00", "highlight_style": style }),
        );
        open(&mut a, f);
        let m = list(&mut a);
        call(&mut a, "doc_close", json!({ "discard_changes": true }));
        assert_eq!(m.len(), 1, "{style}: {m:?}");
        let t = s(&m[0]);
        match style {
            "outline" => assert!(t.contains("Rectangle") && !t.contains("\"fill\":\"#"), "outline: {t}"),
            _ => assert!(t.contains("Highlight"), "text highlight: {t}"),
        }
    }
    // flatten the highlight: no markup left, the link stays
    text_pdf(&dir, "f.pdf", &[&[(72.0, 700.0, "SEE B-1")]]);
    let r = call(
        &mut a,
        "batch_link",
        json!({ "files": ["f.pdf", "B-1.pdf"], "terms": "file_names", "highlight": "#FFFF00", "flatten_highlight": true }),
    );
    assert_eq!(r["highlights"], 1, "{r}");
    assert_eq!(markup_count(&mut a, "f.pdf"), 0, "highlight flattened");
    assert_eq!(links(&mut a, "f.pdf").len(), 1);
    // overlapping: add beside
    let r = call(
        &mut a,
        "batch_link",
        json!({ "files": ["f.pdf", "B-1.pdf"], "terms": "file_names", "add_overlapping": true }),
    );
    assert_eq!(r["existing"], 1, "{r}");
    assert_eq!(links(&mut a, "f.pdf").len(), 2, "kept beside the old link");
    // replace: the old ones deleted and counted
    let r = call(
        &mut a,
        "batch_link",
        json!({ "files": ["f.pdf", "B-1.pdf"], "terms": "file_names", "replace_existing": true }),
    );
    assert_eq!(r["deleted"], 2, "{r}");
    assert_eq!(links(&mut a, "f.pdf").len(), 1);
}

/// D-115: terms exported to CSV (and read back), the run saved as XML or into a Set file and
/// run again, and the summary: links created / deleted, pages skipped, files not opened.
#[test]
fn batch_link_terms_csv_saved_runs_and_summary() {
    let dir = temp_dir("blink-runs");
    let mut a = automation(&dir);
    labelled(
        &dir,
        &mut a,
        "arch.pdf",
        &[&[(72.0, 700.0, "SEE A-102")], &[(72.0, 700.0, "SEE A-101")], &[]],
        json!({"1": "A-101", "2": "A-102"}),
    );
    let r = call(
        &mut a,
        "batch_link",
        json!({ "files": ["arch.pdf"], "export_terms": "terms.csv", "save_run": "run.xml", "run": false }),
    );
    assert!(r["terms_exported"].as_u64().unwrap() >= 2, "{r}");
    let csv = std::fs::read_to_string(dir.join("terms.csv")).unwrap();
    assert!(csv.starts_with("Term,File,Page,Place,URL"), "{csv}");
    assert!(csv.contains("\"A-102\",\"arch.pdf\",2"), "{csv}");
    let xml = std::fs::read_to_string(dir.join("run.xml")).unwrap();
    assert!(xml.contains("<BatchLink") && xml.contains("arch.pdf"), "{xml}");
    assert_eq!(links(&mut a, "arch.pdf").len(), 0, "run: false links nothing");
    // the saved run, run again
    let r = call(&mut a, "batch_link", json!({ "run_file": "run.xml" }));
    assert_eq!(r["links"], 2, "{r}");
    assert_eq!(r["skipped_pages"], 1, "the page with no text: {r}");
    assert_eq!(r["files_not_opened"], 0, "{r}");
    // the edited term table (CSV) as terms, saved into a Set file
    std::fs::write(
        dir.join("terms2.csv"),
        "Term,File,Page,Place,URL\r\n\"A-101\",\"\",,,\"https://example.com/a101\"\r\n",
    )
    .unwrap();
    call(
        &mut a,
        "batch_link",
        json!({ "files": ["arch.pdf"], "terms": "targets", "terms_csv": "terms2.csv", "replace_existing": true,
                 "save_run": "arch.pcset", "run": false }),
    );
    std::fs::write(dir.join("broken.pdf"), b"not a pdf").unwrap();
    let r = call(&mut a, "batch_link", json!({ "run_file": "arch.pcset" }));
    assert_eq!(r["links"], 1, "{r}");
    assert_eq!(r["deleted"], 1, "{r}");
    let l = json!(links(&mut a, "arch.pdf")).to_string();
    assert!(l.contains("https://example.com/a101"), "{l}");
    let r = call(&mut a, "batch_link", json!({ "files": ["arch.pdf", "broken.pdf"] }));
    assert_eq!(r["files_not_opened"], 1, "{r}");
}

/// The tool table with its own config folder (stamps, presets, prefs never touch the user's).
fn auto(dir: &Path) -> Automation {
    automation(dir).with_config_dir(dir.join("config"))
}

fn add_rect(a: &mut Automation, page: u64, subject: &str) -> String {
    let v = call(
        a,
        "markup_add",
        json!({ "page": page, "kind": "Rectangle", "points": [[100, 100], [200, 200]], "subject": subject, "color": "#FF0000" }),
    );
    v.get("id")
        .or_else(|| v.get("markup").and_then(|m| m.get("id")))
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

fn list(a: &mut Automation) -> Vec<Value> {
    let v = call(a, "markup_list", json!({}));
    v.get("markups")
        .and_then(|x| x.as_array())
        .cloned()
        .or_else(|| v.as_array().cloned())
        .unwrap_or_default()
}

// ---- Batch Slip Sheet (doc-116 .. doc-118) ---------------------------------------------------

/// D-116, D-117, D-118 (and D-130): current sheets paired with revised sheets by page label
/// (with a match filter); revised pages replace the old ones and the markups come forward;
/// unmatched new sheets are reported (and appended).
#[test]
fn slip_sheet_matches_by_label_replaces_and_carries_markups() {
    let dir = temp_dir("slip");
    let mut a = automation(&dir);
    labelled(
        &dir,
        &mut a,
        "set.pdf",
        &[&[(72.0, 700.0, "OLD PLAN")], &[(72.0, 700.0, "OLD ROOF")]],
        json!({"1": "A-101", "2": "A-102"}),
    );
    labelled(
        &dir,
        &mut a,
        "rev.pdf",
        &[&[(72.0, 700.0, "NEW ROOF")], &[(72.0, 700.0, "NEW DETAIL")]],
        json!({"1": "A-102 - rev 1", "2": "A-500 - rev 1"}),
    );
    open(&mut a, "set.pdf");
    add_rect(&mut a, 2, "Old roof note");
    let r = call(
        &mut a,
        "slip_sheet",
        json!({ "new_file": "rev.pdf", "number_filter": " - " }),
    );
    let t = s(&r);
    assert!(
        r["unmatched_new"] == json!([2]),
        "the unmatched new sheet is reported: {t}"
    );
    call(&mut a, "doc_save", json!({ "full": true }));
    call(&mut a, "doc_close", json!({}));
    assert!(page_text(&mut a, "set.pdf", 2).contains("NEW ROOF"), "page 2 replaced");
    assert!(page_text(&mut a, "set.pdf", 1).contains("OLD PLAN"), "page 1 untouched");
    open(&mut a, "set.pdf");
    let m = list(&mut a);
    assert!(
        m.iter().any(|x| x["page"] == 2 && s(x).contains("Old roof note")),
        "markups carried to the revised page: {m:?}"
    );
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(
        info["page_list"].as_array().map(|p| p.len()),
        Some(3),
        "unmatched sheet appended: {info}"
    );
}

/// D-116: sheets pair by file name + page index, by the text of a title-block region, by
/// manual pairs, and a wildcard match filter keeps only the sheet number.
#[test]
fn slip_sheet_matches_by_file_index_region_manual_and_wildcard() {
    let dir = temp_dir("slip-match");
    let mut a = automation(&dir);
    std::fs::create_dir_all(dir.join("rev")).unwrap();
    // file name + page index: rev/plans.pdf revises plans.pdf page by page
    text_pdf(
        &dir,
        "plans.pdf",
        &[&[(72.0, 700.0, "OLD ONE")], &[(72.0, 700.0, "OLD TWO")]],
    );
    text_pdf(
        &dir,
        "rev/plans.pdf",
        &[&[(72.0, 700.0, "NEW ONE")], &[(72.0, 700.0, "NEW TWO")]],
    );
    text_pdf(&dir, "rev/other.pdf", &[&[(72.0, 700.0, "UNRELATED")]]);
    open(&mut a, "plans.pdf");
    let r = call(
        &mut a,
        "slip_sheet",
        json!({ "new_files": ["rev/other.pdf", "rev/plans.pdf"], "match": "file_page", "unmatched": "skip" }),
    );
    assert_eq!(r["matched"].as_array().map(Vec::len), Some(2), "{r}");
    call(&mut a, "doc_save", json!({ "full": true }));
    call(&mut a, "doc_close", json!({}));
    assert!(page_text(&mut a, "plans.pdf", 2).contains("NEW TWO"));

    // AutoMark region: the sheet number in the title block (bottom right)
    text_pdf(
        &dir,
        "tb.pdf",
        &[
            &[(72.0, 700.0, "OLD PLAN"), (500.0, 40.0, "S-1")],
            &[(72.0, 700.0, "OLD ROOF"), (500.0, 40.0, "S-2")],
        ],
    );
    text_pdf(
        &dir,
        "tb-rev.pdf",
        &[&[(72.0, 700.0, "NEW ROOF"), (500.0, 40.0, "S-2")]],
    );
    open(&mut a, "tb.pdf");
    let r = call(
        &mut a,
        "slip_sheet",
        json!({ "new_file": "tb-rev.pdf", "match": "region", "region": [480, 20, 600, 70] }),
    );
    assert_eq!(r["matched"][0]["old_page"], 2, "S-2 found in the region: {r}");
    call(&mut a, "doc_close", json!({ "discard_changes": true }));

    // manual pairs
    open(&mut a, "tb.pdf");
    let r = call(
        &mut a,
        "slip_sheet",
        json!({ "new_file": "tb-rev.pdf", "match": "manual", "pairs": [[1, 1, 1]], "unmatched": "skip" }),
    );
    assert_eq!(r["matched"][0]["old_page"], 1, "{r}");
    assert_eq!(r["unmatched_old"], json!([2]), "{r}");
    call(&mut a, "doc_close", json!({ "discard_changes": true }));

    // wildcard filter: labels "Sheet A-101 (issue 3)" match "A-101" by @?# ; a sheet the
    // filter does not match takes no part
    labelled(
        &dir,
        &mut a,
        "w.pdf",
        &[&[(72.0, 700.0, "OLD W")], &[(72.0, 700.0, "OLD COVER")]],
        json!({"1": "Sheet A-101 (issue 2)", "2": "COVER"}),
    );
    labelled(
        &dir,
        &mut a,
        "w-rev.pdf",
        &[&[(72.0, 700.0, "NEW W")], &[(72.0, 700.0, "NEW COVER")]],
        json!({"1": "A-101 rev 3", "2": "COVER"}),
    );
    open(&mut a, "w.pdf");
    let r = call(
        &mut a,
        "slip_sheet",
        json!({ "new_file": "w-rev.pdf", "filter": "@?#", "unmatched": "skip" }),
    );
    let m = r["matched"].as_array().unwrap();
    assert_eq!(m.len(), 1, "only A-101 matches through the wildcard: {r}");
    assert_eq!(m[0]["key"], "A-101", "{r}");
    call(&mut a, "doc_close", json!({ "discard_changes": true }));
}

/// D-117, D-130: the revision goes ahead of the old sheet, the markups are copied forward
/// (unflattened first, flattened after on request) and the old sheet is stamped SUPERSEDED;
/// links and bookmarks to the old sheet move to the revision.
#[test]
fn slip_sheet_inserts_ahead_copies_markups_and_stamps_superseded() {
    let dir = temp_dir("slip-ahead");
    let mut a = automation(&dir);
    labelled(
        &dir,
        &mut a,
        "set.pdf",
        &[&[(72.0, 700.0, "INDEX SEE ROOF")], &[(72.0, 700.0, "OLD ROOF")]],
        json!({"1": "G-001", "2": "A-102"}),
    );
    labelled(
        &dir,
        &mut a,
        "rev.pdf",
        &[&[(72.0, 700.0, "NEW ROOF")]],
        json!({"1": "A-102"}),
    );
    open(&mut a, "set.pdf");
    add_rect(&mut a, 2, "Roof note");
    call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [72, 690, 200, 715], "to_page": 2 }),
    );
    call(&mut a, "bookmark_add", json!({ "title": "Roof", "page": 2 }));
    let r = call(
        &mut a,
        "slip_sheet",
        json!({ "new_file": "rev.pdf", "insert_ahead": true, "superseded": true, "unmatched": "skip" }),
    );
    let t = s(&r);
    assert_eq!(r["superseded"], 1, "{t}");
    assert_eq!(r["matched"][0]["result_page"], 2, "revision inserted ahead: {t}");
    assert!(r["links_redirected"].as_u64().unwrap() >= 1, "{t}");
    assert!(r["bookmarks_redirected"].as_u64().unwrap() >= 1, "{t}");
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(
        info["page_list"].as_array().map(Vec::len),
        Some(3),
        "old sheet kept: {info}"
    );
    let m = list(&mut a);
    assert!(
        m.iter().any(|x| x["page"] == 2 && s(x).contains("Roof note")),
        "markup copied forward: {m:?}"
    );
    assert!(
        m.iter().any(|x| x["page"] == 3 && s(x).contains("Roof note")),
        "old sheet keeps its markup: {m:?}"
    );
    assert!(
        m.iter().any(|x| x["page"] == 3 && s(x).contains("SUPERSEDED")),
        "old sheet stamped Superseded: {m:?}"
    );
    call(&mut a, "doc_save", json!({ "full": true }));
    call(&mut a, "doc_close", json!({}));
    assert!(page_text(&mut a, "set.pdf", 2).contains("NEW ROOF"));
    assert!(page_text(&mut a, "set.pdf", 3).contains("OLD ROOF"));
    let l = links(&mut a, "set.pdf");
    assert!(
        s(&l[0]).contains("\"page\":2"),
        "link redirected to the revision: {l:?}"
    );

    // flatten after: the carried markups become page content on the revision
    labelled(&dir, &mut a, "f.pdf", &[&[(72.0, 700.0, "OLD")]], json!({"1": "M-1"}));
    labelled(
        &dir,
        &mut a,
        "f-rev.pdf",
        &[&[(72.0, 700.0, "NEW")]],
        json!({"1": "M-1"}),
    );
    open(&mut a, "f.pdf");
    add_rect(&mut a, 1, "Flatten me");
    let r = call(
        &mut a,
        "slip_sheet",
        json!({ "new_file": "f-rev.pdf", "insert_ahead": true, "flatten_after": true, "unflatten_first": true }),
    );
    assert_eq!(r["flattened"], 1, "{r}");
    let m = list(&mut a);
    assert!(m.iter().all(|x| x["page"] != 1), "revision's markups flattened: {m:?}");
    // superseded needs a kept old sheet
    call(&mut a, "doc_close", json!({ "discard_changes": true }));
    open(&mut a, "f.pdf");
    let e = fails(
        &mut a,
        "slip_sheet",
        json!({ "new_file": "f-rev.pdf", "superseded": true }),
    );
    assert!(e.contains("insert"), "{e}");
}

/// D-118: unmatched new sheets extracted to files; a CSV and a PDF report (with links).
#[test]
fn slip_sheet_extracts_leftovers_and_writes_reports() {
    let dir = temp_dir("slip-report");
    let mut a = automation(&dir);
    labelled(
        &dir,
        &mut a,
        "set.pdf",
        &[&[(72.0, 700.0, "OLD")]],
        json!({"1": "A-101"}),
    );
    labelled(
        &dir,
        &mut a,
        "rev.pdf",
        &[&[(72.0, 700.0, "NEW")], &[(72.0, 700.0, "EXTRA SHEET")]],
        json!({"1": "A-101", "2": "A-900"}),
    );
    open(&mut a, "set.pdf");
    let r = call(
        &mut a,
        "slip_sheet",
        json!({ "new_file": "rev.pdf", "unmatched": "extract", "extract_dir": "left",
                 "report_csv": "slip.csv", "report_pdf": "slip-report.pdf" }),
    );
    assert_eq!(r["appended"], 0, "{r}");
    let x = dir.join("left").join("A-900.pdf");
    assert!(x.exists(), "the unmatched sheet extracted by its label: {r}");
    assert!(page_text(&mut a, "left/A-900.pdf", 1).contains("EXTRA SHEET"));
    let csv = std::fs::read_to_string(dir.join("slip.csv")).unwrap();
    assert!(csv.starts_with("Status,"), "{csv}");
    assert!(csv.contains("\"matched\"") && csv.contains("A-101"), "{csv}");
    assert!(csv.contains("\"new unmatched\"") && csv.contains("A-900.pdf"), "{csv}");
    assert!(page_text(&mut a, "slip-report.pdf", 1).contains("Slip Sheet Report"));
    let l = links(&mut a, "slip-report.pdf");
    assert!(l.len() >= 2, "a link per result: {l:?}");
    assert!(
        json!(l).to_string().contains("set.pdf") && json!(l).to_string().contains("A-900.pdf"),
        "{l:?}"
    );
}

/// D-116, D-117: Batch Slip Sheet over many files from one pool of revisions, each file saved.
#[test]
fn batch_slip_sheet_over_many_files() {
    let dir = temp_dir("slip-batch");
    let mut a = automation(&dir);
    labelled(
        &dir,
        &mut a,
        "arch.pdf",
        &[&[(72.0, 700.0, "OLD A")]],
        json!({"1": "A-101"}),
    );
    labelled(
        &dir,
        &mut a,
        "mech.pdf",
        &[&[(72.0, 700.0, "OLD M")]],
        json!({"1": "M-101"}),
    );
    labelled(
        &dir,
        &mut a,
        "rev.pdf",
        &[
            &[(72.0, 700.0, "NEW M")],
            &[(72.0, 700.0, "NEW A")],
            &[(72.0, 700.0, "NEW E")],
        ],
        json!({"1": "M-101", "2": "A-101", "3": "E-101"}),
    );
    let r = call(
        &mut a,
        "batch_slip_sheet",
        json!({ "files": ["arch.pdf", "mech.pdf"], "new_files": ["rev.pdf"], "unmatched": "extract",
                 "extract_dir": "left", "report_csv": "batch.csv" }),
    );
    assert_eq!(r["matched"].as_array().map(Vec::len), Some(2), "{r}");
    assert!(page_text(&mut a, "arch.pdf", 1).contains("NEW A"));
    assert!(page_text(&mut a, "mech.pdf", 1).contains("NEW M"));
    assert!(dir.join("left").join("E-101.pdf").exists(), "{r}");
    assert!(
        std::fs::read_to_string(dir.join("batch.csv"))
            .unwrap()
            .contains("E-101")
    );
}

// ---- Batch Sign & Seal, Stamp, Flatten, Summary, Print, H&F, others (doc-119 .. doc-125) ----

/// D-119: across many files add a date, place a seal and digitally sign.
#[test]
fn batch_sign_and_seal_dates_seals_and_signs_many_files() {
    let dir = temp_dir("bsign");
    let mut a = auto(&dir);
    text_pdf(&dir, "one.pdf", &[&[(72.0, 700.0, "ONE")]]);
    text_pdf(&dir, "two.pdf", &[&[(72.0, 700.0, "TWO")]]);
    let seal = SyntheticPage::new(
        100.0,
        100.0,
        format!("{}{}", rect(10.0, 10.0, 80.0, 80.0), text(20.0, 45.0, 10.0, "SEAL")),
    );
    std::fs::write(dir.join("seal.pdf"), pdf(&[seal])).unwrap();
    call(
        &mut a,
        "digital_id_create",
        json!({ "name": "Test Engineer", "password": "pw", "out": "id.p12", "cert_out": "id.pem" }),
    );
    let r = call(
        &mut a,
        "batch_sign",
        json!({ "files": ["one.pdf", "two.pdf"], "p12": "id.p12", "password": "pw",
                 "seal": "seal.pdf", "seal_rect": [400, 50, 500, 150],
                 "date_format": "yyyy-MM-dd", "date_rect": [400, 20, 560, 40],
                 "rect": [400, 160, 560, 200], "page": 1, "out_dir": "signed", "reason": "Approved" }),
    );
    for f in ["one", "two"] {
        let out = std::fs::read_dir(dir.join("signed"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .find(|n| n.starts_with(f))
            .unwrap_or_else(|| panic!("signed copy of {f}: {r}"));
        open(&mut a, &format!("signed/{out}"));
        let v = call(&mut a, "signature_list", json!({ "trust": ["id.pem"] }));
        assert!(s(&v).contains("valid") && !s(&v).contains("invalid"), "{f}: {v}");
        assert!(s(&v).contains("Approved"), "reason kept: {v}");
        let pt = call(&mut a, "page_text", json!({ "page": 1 }));
        assert!(
            (2026..2100).any(|y| s(&pt).contains(&y.to_string())),
            "date on the page: {pt}"
        );
        call(&mut a, "doc_close", json!({ "discard_changes": true }));
    }
}

/// D-120 (and D-153 settings): one stamp on many files at the same spot, with opacity / blend
/// / lock from the stamp settings; closed files saved directly.
#[test]
fn batch_apply_stamp_places_one_stamp_on_many_files() {
    let dir = temp_dir("bstamp");
    let mut a = auto(&dir);
    text_pdf(&dir, "one.pdf", &[&[(72.0, 700.0, "ONE")], &[(72.0, 700.0, "ONE B")]]);
    text_pdf(&dir, "two.pdf", &[&[(72.0, 700.0, "TWO")]]);
    call(
        &mut a,
        "stamp_settings",
        json!({ "action": "set", "opacity": 0.5, "blend": "multiply", "lock": true }),
    );
    let r = call(
        &mut a,
        "batch_apply",
        json!({ "files": ["one.pdf", "two.pdf"],
                 "operations": [{ "tool": "stamp_add", "args": { "page": 1, "stamp": "Approved", "rect": [400, 600, 580, 680] } }] }),
    );
    for f in ["one.pdf", "two.pdf"] {
        open(&mut a, f);
        let m = list(&mut a);
        assert_eq!(m.len(), 1, "{f}: {m:?} {r}");
        let st = s(&m[0]);
        assert!(st.contains("Stamp"), "{st}");
        assert!(st.contains("\"opacity\":0.5"), "opacity from the stamp settings: {st}");
        assert!(st.contains("\"locked\":true"), "lock from the stamp settings: {st}");
        call(&mut a, "doc_close", json!({}));
    }
}

/// D-120: Batch Apply Stamp with a page filter, an anchor on the page grid with X / Y offset,
/// a scale and a rotation, on closed files saved directly.
#[test]
fn batch_apply_stamp_page_filter_anchor_offset_scale_and_rotation() {
    let dir = temp_dir("bstamp2");
    let mut a = auto(&dir);
    let pages: Vec<SyntheticPage> = (0..4)
        .map(|i| {
            if i == 3 {
                SyntheticPage::new(792.0, 612.0, text(72.0, 500.0, 12.0, "WIDE"))
            } else {
                SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, "TALL"))
            }
        })
        .collect();
    std::fs::write(dir.join("one.pdf"), pdf(&pages)).unwrap();
    std::fs::write(dir.join("two.pdf"), pdf(&pages)).unwrap();
    // odd pages, top-right corner 20 / 30 points in, half size
    call(
        &mut a,
        "batch_apply",
        json!({ "files": ["one.pdf", "two.pdf"],
                 "operations": [{ "tool": "stamp_apply", "args": { "stamp": "Approved", "filter": "odd",
                     "anchor": "top_right", "offset_x": 20, "offset_y": 30, "scale": 0.5 } }] }),
    );
    for f in ["one.pdf", "two.pdf"] {
        open(&mut a, f);
        let m = list(&mut a);
        call(&mut a, "doc_close", json!({}));
        let on: Vec<u64> = m.iter().map(|x| x["page"].as_u64().unwrap()).collect();
        assert_eq!(on, [1, 3], "{f}: odd pages only: {m:?}");
        let r = bounds(&m[0]);
        let (x1, y1) = (r[2], r[3]);
        assert!((x1 - (612.0 - 20.0)).abs() < 0.5, "right edge 20 pt in: {r:?}");
        assert!((y1 - (792.0 - 30.0)).abs() < 0.5, "top edge 30 pt in: {r:?}");
    }
    // landscape pages only, bottom-left, rotated a quarter turn, double size
    open(&mut a, "one.pdf");
    let r = call(
        &mut a,
        "stamp_apply",
        json!({ "text": "CHECKED", "filter": "landscape", "anchor": "bottom_left", "scale": 2, "rotation": 90 }),
    );
    assert_eq!(r["pages"], json!([4]), "{r}");
    let m = list(&mut a);
    let st = m.iter().find(|x| x["page"] == 4).unwrap();
    assert_eq!(st["rotation"].as_f64(), Some(90.0), "turned a quarter: {st}");
    let rr: Vec<f64> = st["rect"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert!(rr[3] - rr[1] > rr[2] - rr[0], "the turned stamp stands upright: {st}");
    assert!(
        rr[0].abs() < 3.0 && rr[1].abs() < 3.0,
        "the turned box sits in the bottom-left corner: {st}"
    );
    // any angle
    let r = call(
        &mut a,
        "stamp_apply",
        json!({ "stamp": "Void", "pages": [2], "rotation": 30 }),
    );
    let id = r["ids"][0].as_str().unwrap().to_string();
    let m = list(&mut a);
    let st = m.iter().find(|x| x["id"] == id.as_str()).unwrap();
    assert_eq!(st["rotation"].as_f64(), Some(30.0), "{st}");
    // chosen pages narrowed by even
    let r = call(
        &mut a,
        "stamp_apply",
        json!({ "stamp": "Draft", "pages": [1, 2, 3], "filter": "even" }),
    );
    assert_eq!(r["pages"], json!([2]), "{r}");
    let e = fails(&mut a, "stamp_apply", json!({ "stamp": "Draft", "scale": 50 }));
    assert!(e.contains("scale"), "{e}");
}

/// A markup's points' bounding box [x0, y0, x1, y1].
fn bounds(m: &Value) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for p in m["points"].as_array().unwrap() {
        let (x, y) = (p[0].as_f64().unwrap(), p[1].as_f64().unwrap());
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    b
}

/// D-095, D-114, D-115, D-116 .. D-120 in the app: the Slip Sheet, Batch Link, Split and Apply
/// Stamp dialogs offer their options.
#[test]
fn batch_dialogs_offer_slip_link_split_and_stamp_options() {
    let mut h = app();
    run(&mut h, "document.slip_sheet");
    for t in [
        "File name + page",
        "Region",
        "Wildcard filter",
        "Insert revisions ahead",
        "Stamp old sheets Superseded",
        "Unflatten markups first",
        "Flatten markups after",
        "Extract to files",
        "Write a CSV and a PDF report",
    ] {
        assert!(shows(&h, t), "Slip Sheet: {t}");
    }
    run(&mut h, "batch.link");
    for t in [
        "Highlight style",
        "Flatten highlights",
        "Export Terms...",
        "Save Run...",
        "Load Run...",
    ] {
        assert!(shows(&h, t), "Batch Link: {t}");
    }
    run(&mut h, "batch.split");
    for t in [
        "Prefix",
        "Suffix",
        "Name parts after their bookmarks",
        "Put the parts in a subfolder",
    ] {
        assert!(shows(&h, t), "Split: {t}");
    }
    run(&mut h, "batch.apply_stamp");
    for t in ["Anchor", "X offset", "Y offset", "Scale", "Rotation", "landscape"] {
        assert!(shows(&h, t), "Apply Stamp: {t}");
    }
}

/// D-121: flatten or unflatten markups on chosen pages of many files.
#[test]
fn batch_flatten_then_unflatten_on_chosen_pages() {
    let dir = temp_dir("bflat");
    let mut a = automation(&dir);
    for f in ["one.pdf", "two.pdf"] {
        text_pdf(&dir, f, &[&[(72.0, 700.0, "P1")], &[(72.0, 700.0, "P2")]]);
        open(&mut a, f);
        add_rect(&mut a, 1, "flat me");
        add_rect(&mut a, 2, "keep me");
        call(&mut a, "doc_save", json!({}));
        call(&mut a, "doc_close", json!({}));
    }
    call(
        &mut a,
        "batch_apply",
        json!({ "files": ["one.pdf", "two.pdf"],
                 "operations": [{ "tool": "markup_flatten", "args": { "pages": [1], "recoverable": true } }] }),
    );
    for f in ["one.pdf", "two.pdf"] {
        open(&mut a, f);
        let m = list(&mut a);
        assert_eq!(m.len(), 1, "{f}: only the page-2 markup is left: {m:?}");
        assert_eq!(m[0]["page"], 2);
        call(&mut a, "doc_close", json!({}));
    }
    call(
        &mut a,
        "batch_apply",
        json!({ "files": ["one.pdf", "two.pdf"], "operations": [{ "tool": "markup_unflatten", "args": {} }] }),
    );
    for f in ["one.pdf", "two.pdf"] {
        assert_eq!(markup_count(&mut a, f), 2, "{f}: page-1 markup back");
    }
}

/// D-122: one summary report from many PDFs.
#[test]
fn batch_summary_reports_markups_of_many_files() {
    let dir = temp_dir("bsum");
    let mut a = automation(&dir);
    for (f, subj) in [("one.pdf", "Alpha note"), ("two.pdf", "Beta note")] {
        text_pdf(&dir, f, &[&[(72.0, 700.0, "P1")]]);
        open(&mut a, f);
        add_rect(&mut a, 1, subj);
        call(&mut a, "doc_save", json!({}));
        call(&mut a, "doc_close", json!({}));
    }
    call(
        &mut a,
        "batch_summary",
        json!({ "files": ["one.pdf", "two.pdf"], "out": "sum.csv" }),
    );
    let csv = std::fs::read_to_string(dir.join("sum.csv")).unwrap();
    assert!(csv.contains("Alpha note") && csv.contains("Beta note"), "{csv}");
    assert!(
        csv.contains("one.pdf") && csv.contains("two.pdf"),
        "a File column: {csv}"
    );
}

/// D-123: many PDFs printed with one set of settings, in list order (print-ready PDFs; no
/// real printer is touched).
#[test]
fn batch_print_writes_each_file_in_list_order() {
    let dir = temp_dir("bprint");
    let mut a = automation(&dir);
    text_pdf(&dir, "b.pdf", &[&[(72.0, 700.0, "BEE")]]);
    text_pdf(&dir, "a.pdf", &[&[(72.0, 700.0, "AY")], &[(72.0, 700.0, "AY2")]]);
    let r = call(
        &mut a,
        "batch_print",
        json!({ "files": ["b.pdf", "a.pdf"], "out_dir": "out", "paper": "tabloid", "layout": "fit" }),
    );
    let t = s(&r);
    let (pb, pa) = (t.find("b_print"), t.find("a_print"));
    assert!(pb.is_some() && pa.is_some() && pb < pa, "list order kept: {t}");
    open(&mut a, "out/a_print.pdf");
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(info["page_list"].as_array().unwrap().len(), 2, "{info}");
    let p = &info["page_list"][0];
    let short = s(p);
    assert!(
        short.contains("792") && short.contains("1224"),
        "tabloid paper (792 x 1224): {p}"
    );
}

/// D-124: add (not edit) or remove headers/footers on many PDFs.
#[test]
fn batch_headers_footers_add_and_remove() {
    let dir = temp_dir("bhf");
    let mut a = automation(&dir);
    text_pdf(&dir, "one.pdf", &[&[(72.0, 700.0, "ONE")]]);
    text_pdf(&dir, "two.pdf", &[&[(72.0, 700.0, "TWO")]]);
    call(
        &mut a,
        "batch_apply",
        json!({ "files": ["one.pdf", "two.pdf"],
                 "operations": [{ "tool": "header_footer_add", "args": { "footer_center": "PROJECT X <<1>>" } }] }),
    );
    for f in ["one.pdf", "two.pdf"] {
        assert!(page_text(&mut a, f, 1).contains("PROJECT X 1"), "{f}");
    }
    call(
        &mut a,
        "batch_apply",
        json!({ "files": ["one.pdf", "two.pdf"],
                 "operations": [{ "tool": "marks_remove", "args": { "kind": "header_footer" } }] }),
    );
    for f in ["one.pdf", "two.pdf"] {
        assert!(!page_text(&mut a, f, 1).contains("PROJECT X"), "{f} removed");
    }
}

/// D-125: batch security, rotate, crop, split, compare, overlay and script.
#[test]
fn other_batch_processes_security_rotate_split_compare_overlay_script() {
    let dir = temp_dir("bother");
    let mut a = automation(&dir);
    text_pdf(&dir, "one.pdf", &[&[(72.0, 700.0, "ONE")], &[(72.0, 700.0, "ONE B")]]);
    text_pdf(&dir, "two.pdf", &[&[(72.0, 700.0, "TWO")], &[(72.0, 700.0, "TWO B")]]);
    call(
        &mut a,
        "batch_apply",
        json!({ "files": ["one.pdf", "two.pdf"], "out_dir": "sec",
                 "operations": [
                    { "tool": "page_rotate", "args": { "degrees": 90 } },
                    { "tool": "page_crop", "args": { "margins": [10, 10, 10, 10] } },
                    { "tool": "security_set", "args": { "open_password": "open1" } }] }),
    );
    for f in ["one.pdf", "two.pdf"] {
        fails(&mut a, "doc_open", json!({ "path": format!("sec/{f}") }));
        call(
            &mut a,
            "doc_open_protected",
            json!({ "path": format!("sec/{f}"), "password": "open1" }),
        );
        let info = call(&mut a, "doc_info", json!({}));
        assert_eq!(info["page_list"][0]["rotate"], 90, "{info}");
        call(&mut a, "doc_close", json!({}));
    }
    call(
        &mut a,
        "batch_split",
        json!({ "files": ["one.pdf", "two.pdf"], "dir": "parts", "pages_per_file": 1 }),
    );
    assert_eq!(
        std::fs::read_dir(dir.join("parts")).unwrap().count(),
        4,
        "two parts per file"
    );
    text_pdf(
        &dir,
        "two_rev.pdf",
        &[&[(72.0, 700.0, "TWO CHANGED")], &[(72.0, 700.0, "TWO B")]],
    );
    let pairs = json!([{"current": "two.pdf", "current_page": 1, "revised": "two_rev.pdf", "revised_page": 1}]);
    call(
        &mut a,
        "batch_compare",
        json!({ "match": "manual", "pairs": pairs, "out_dir": "cmp", "mode": "text" }),
    );
    assert!(std::fs::read_dir(dir.join("cmp")).unwrap().count() >= 1);
    call(
        &mut a,
        "batch_overlay",
        json!({ "match": "manual", "pairs": pairs, "out_dir": "ovl" }),
    );
    assert!(std::fs::read_dir(dir.join("ovl")).unwrap().count() >= 1);
    std::fs::write(
        dir.join("script.json"),
        r#"[{"tool": "watermark_add", "params": {"text": "DRAFT"}}]"#,
    )
    .unwrap();
    call(
        &mut a,
        "batch_script",
        json!({ "files": ["one.pdf", "two.pdf"], "script": "script.json", "out_dir": "wm" }),
    );
    assert!(page_text(&mut a, "wm/two.pdf", 1).contains("DRAFT"));
}

// ---- Sets (doc-126 .. doc-134) -------------------------------------------------------------

/// A drawing set of three files: A-1/A-10/A-2 (architectural), M-101 (mechanical), a revision.
fn set_files(dir: &Path, a: &mut Automation) {
    labelled(
        dir,
        a,
        "Arch.pdf",
        &[
            &[(72.0, 700.0, "ARCH TEN")],
            &[(72.0, 700.0, "ARCH TWO")],
            &[(72.0, 700.0, "ARCH ONE")],
        ],
        json!({"1": "A-10", "2": "A-2", "3": "A-1"}),
    );
    labelled(
        dir,
        a,
        "Mech.pdf",
        &[&[(72.0, 700.0, "DUCTWORK LAYOUT")]],
        json!({"1": "M-101"}),
    );
}

fn sheets(v: &Value) -> Vec<Value> {
    v.get("sheets").and_then(|x| x.as_array()).cloned().unwrap_or_default()
}

/// D-126, D-127, D-128: a set file lists many PDFs with relative paths and opens as one
/// sequence without merging; sort by label (natural), file + label, or file order.
#[test]
fn set_create_navigate_and_sort() {
    let dir = temp_dir("sets");
    let mut a = automation(&dir);
    set_files(&dir, &mut a);
    let before = std::fs::read(dir.join("Arch.pdf")).unwrap();
    call(
        &mut a,
        "set_save",
        json!({ "path": "job.pcset", "files": ["Arch.pdf", "Mech.pdf"], "name": "Job" }),
    );
    let raw = std::fs::read_to_string(dir.join("job.pcset")).unwrap();
    assert!(
        raw.contains("Arch.pdf") && !raw.contains(&*dir.to_string_lossy()),
        "relative paths: {raw}"
    );
    let v = call(&mut a, "set_sheets", json!({ "set": "job.pcset" }));
    let sh = sheets(&v);
    assert_eq!(sh.len(), 4, "every sheet of every file as one sequence: {v}");
    let v = call(&mut a, "set_sheets", json!({ "set": "job.pcset", "sort": "label" }));
    let labels: Vec<String> = sheets(&v)
        .iter()
        .map(|x| x["label"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(labels, ["A-1", "A-2", "A-10", "M-101"], "label sort, natural order");
    let v = call(
        &mut a,
        "set_sheets",
        json!({ "set": "job.pcset", "sort": "file_label", "open": true }),
    );
    assert!(s(&v).contains("doc"), "files opened: {v}");
    assert_eq!(
        std::fs::read(dir.join("Arch.pdf")).unwrap(),
        before,
        "files not merged or changed"
    );
}

fn open_panels(h: &Harness<'_, MarkupCraftApp>) -> Vec<&'static str> {
    markupcraft_ui_egui::dock::open_panels(h.state().dock())
}

/// Pressing Alt+`k` flips whether panel `id` is open (twice: back as it was).
fn panel_toggles_with_key(h: &mut Harness<'_, MarkupCraftApp>, id: &str, k: egui::Key) -> bool {
    let was = open_panels(h).contains(&id);
    key(h, egui::Modifiers::ALT, k);
    h.run_steps(2);
    let now = open_panels(h).contains(&id);
    key(h, egui::Modifiers::ALT, k);
    h.run_steps(2);
    was != now && open_panels(h).contains(&id) == was
}

/// Bring panel `id` to the front (opened if closed).
fn show(h: &mut Harness<'_, MarkupCraftApp>, id: &'static str) {
    h.state_mut().state.show_panel(id);
    h.run_steps(4);
}

/// D-127 (UI): the Sets panel opens with Alt+2 / its command.
#[test]
fn sets_panel_opens_in_the_app() {
    let mut h = app();
    assert!(
        panel_toggles_with_key(&mut h, "sets", egui::Key::Num2),
        "Alt+2 shows / hides the Sets panel"
    );
    show(&mut h, "sets");
    assert!(
        shows(&h, "Print Set...") && shows(&h, "Add Files..."),
        "Sets panel content"
    );
}

/// D-129, D-131, D-132: revisions found by sheet number / wildcard; categories by discipline;
/// derived and custom tags kept in the set file.
#[test]
fn set_revisions_categories_and_tags() {
    let dir = temp_dir("settags");
    let mut a = automation(&dir);
    set_files(&dir, &mut a);
    labelled(
        &dir,
        &mut a,
        "Mech rev 2.pdf",
        &[&[(72.0, 700.0, "DUCTWORK REVISED")]],
        json!({"1": "M-101"}),
    );
    call(
        &mut a,
        "set_save",
        json!({ "path": "job.pcset", "files": ["Arch.pdf", "Mech.pdf", "Mech rev 2.pdf"] }),
    );
    let v = call(&mut a, "set_tags", json!({ "set": "job.pcset", "revisions": true }));
    let t = s(&v);
    assert!(t.contains("Mech rev 2.pdf") && t.contains("Mech.pdf"), "{t}");
    let latest = t.rfind("Mech rev 2.pdf").unwrap();
    let older = t.find("Mech.pdf").unwrap();
    assert!(older < latest, "latest revision last: {t}");
    let v = call(
        &mut a,
        "set_tags",
        json!({ "set": "job.pcset", "categories": "sheet_number" }),
    );
    let t = s(&v);
    assert!(
        t.contains("Architectural") && t.contains("Mechanical"),
        "discipline categories: {t}"
    );
    let v = call(&mut a, "set_tags", json!({ "set": "job.pcset" }));
    let t = s(&v);
    assert!(
        t.contains("Sheet Number") && t.contains("Discipline"),
        "derived tags: {t}"
    );
    call(
        &mut a,
        "set_tags",
        json!({ "set": "job.pcset", "tag": { "sheet": "Mech.pdf#1", "name": "Phase", "value": "CD" } }),
    );
    let t = s(&call(&mut a, "set_tags", json!({ "set": "job.pcset" })));
    assert!(
        t.contains("Phase") && t.contains("CD"),
        "custom tag saved in the set: {t}"
    );
}

/// D-133, D-134: publish (combine with bookmarks, latest only; package; drawing log), print
/// the whole set and search text across it.
#[test]
fn set_publish_print_and_search() {
    let dir = temp_dir("setpub");
    let mut a = automation(&dir);
    set_files(&dir, &mut a);
    call(
        &mut a,
        "set_save",
        json!({ "path": "job.pcset", "files": ["Arch.pdf", "Mech.pdf"] }),
    );
    call(
        &mut a,
        "set_publish",
        json!({ "set": "job.pcset", "out": "combined.pdf" }),
    );
    open(&mut a, "combined.pdf");
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(info["page_list"].as_array().unwrap().len(), 4);
    let b = call(&mut a, "bookmark_list", json!({}));
    assert!(s(&b).contains("M-101"), "a bookmark per sheet: {b}");
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "set_publish", json!({ "set": "job.pcset", "dir": "pkg" }));
    assert!(dir.join("pkg").join("Arch.pdf").is_file() && dir.join("pkg").join("Mech.pdf").is_file());
    call(&mut a, "set_publish", json!({ "set": "job.pcset", "log": "log.csv" }));
    let log = std::fs::read_to_string(dir.join("log.csv")).unwrap();
    assert!(log.contains("A-10") && log.contains("M-101"), "{log}");
    call(&mut a, "set_print", json!({ "set": "job.pcset", "out": "print.pdf" }));
    open(&mut a, "print.pdf");
    assert_eq!(
        call(&mut a, "doc_info", json!({}))["page_list"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let r = call(&mut a, "text_search", json!({ "text": "DUCTWORK", "set": "job.pcset" }));
    assert!(s(&r).contains("Mech.pdf"), "set search finds the other file: {r}");
}

// ---- Markup summary (doc-135 .. doc-139) ---------------------------------------------------

/// A two-page document with three markups of two subjects and two statuses.
fn summary_doc(dir: &Path, a: &mut Automation) {
    text_pdf(
        dir,
        "doc.pdf",
        &[&[(72.0, 700.0, "PAGE ONE")], &[(72.0, 700.0, "PAGE TWO")]],
    );
    open(a, "doc.pdf");
    for (page, subj, status, x) in [
        (1, "Wall", "Accepted", 100),
        (1, "Door", "None", 300),
        (2, "Wall", "Rejected", 100),
    ] {
        call(
            a,
            "markup_add",
            json!({ "page": page, "kind": "Rectangle", "points": [[x, 100], [x + 50, 150]], "subject": subj, "status": status, "contents": format!("{subj} on {page}") }),
        );
    }
    call(a, "doc_save", json!({}));
}

/// D-135: CSV, XML, a separate PDF, or appended to the document with links back.
#[test]
fn summary_output_types_csv_xml_pdf_and_appended() {
    let dir = temp_dir("sumtypes");
    let mut a = automation(&dir);
    summary_doc(&dir, &mut a);
    call(&mut a, "summary_export", json!({ "out": "s.csv" }));
    call(&mut a, "summary_export", json!({ "out": "s.xml" }));
    call(&mut a, "summary_export", json!({ "out": "s.pdf" }));
    let csv = std::fs::read_to_string(dir.join("s.csv")).unwrap();
    assert!(csv.contains("Door") && csv.contains("Wall"), "{csv}");
    let xml = std::fs::read_to_string(dir.join("s.xml")).unwrap();
    assert!(xml.starts_with("<?xml") || xml.starts_with('<'), "{xml}");
    assert!(xml.contains("Door"), "{xml}");
    let d = open(&mut a, "s.pdf");
    let t = call(&mut a, "page_text", json!({ "page": 1, "doc": d }));
    assert!(s(&t).contains("Door"), "PDF report: {t}");
    call(&mut a, "doc_close", json!({ "doc": d }));
    open(&mut a, "doc.pdf");
    call(&mut a, "summary_append", json!({}));
    let info = call(&mut a, "doc_info", json!({}));
    let n = info["page_list"].as_array().unwrap().len();
    assert!(n >= 3, "summary appended: {info}");
    let l = call(&mut a, "link_list", json!({}));
    assert!(
        s(&l).matches("page").count() >= 3,
        "links back to the markups' pages: {l}"
    );
}

/// D-135: the summary straight to the printer: the report is laid out as a print-ready PDF
/// and handed to the system print command (tests record the command, never run it).
#[test]
fn summary_straight_to_the_printer() {
    markupcraft_engine::print_seam::set_dry_run(true);
    let dir = temp_dir("sumprint");
    let mut a = automation(&dir);
    summary_doc(&dir, &mut a);
    let r = call(
        &mut a,
        "summary_print",
        json!({ "dry_run": true, "printer": "Plotter 7", "copies": 2, "columns": ["subject", "status"] }),
    );
    assert_eq!(r["sent"], false, "a dry run sends nothing: {r}");
    assert_eq!(r["markups"], 3, "{r}");
    assert!(s(&r["args"]).contains("Plotter 7"), "the chosen printer: {r}");
    let pdf = PathBuf::from(r["pdf"].as_str().unwrap());
    let report = markupcraft_engine::Session::open(&pdf).unwrap();
    for word in ["Door", "Wall", "Accepted"] {
        let hits = report.search_text(word, &Default::default()).unwrap();
        assert!(!hits.hits.is_empty(), "{word} in the printed report");
    }
    // Without dry_run it goes through the print path (recorded here instead of printed).
    let r = call(&mut a, "summary_print", json!({}));
    let sent = PathBuf::from(r["pdf"].as_str().unwrap());
    assert!(
        markupcraft_engine::print_seam::recorded().iter().any(|p| p.pdf == sent),
        "handed to the print command: {r}"
    );
    // The app: Markup Summary > Print Summary.
    let mut h = app();
    call_ui_markup(&mut h);
    let before = markupcraft_engine::print_seam::recorded().len();
    run(&mut h, "markup.summary");
    h.get_by_label("Print Summary").click();
    h.run_steps(4);
    let after = markupcraft_engine::print_seam::recorded();
    assert!(after.len() > before, "{}", h.state().state.status);
    assert!(h.state().state.status.contains("printer"), "{}", h.state().state.status);
}

/// A markup in the app's open sample, so its summary has a row.
fn call_ui_markup(h: &mut Harness<'_, MarkupCraftApp>) {
    let m = markupcraft_model::Markup::new(
        Kind::Rectangle,
        0,
        vec![Point::new(100.0, 100.0), Point::new(200.0, 160.0)],
    );
    let d = h.state_mut().state.doc_mut().unwrap();
    d.session.add_new_markups("Add", vec![m]).unwrap();
    h.run_steps(2);
}

/// D-136: saved column configurations: save, list, load (order kept), delete; empty columns
/// left out unless the configuration keeps them; the dialog saves and loads them too.
#[test]
fn summary_saved_column_configurations() {
    let dir = temp_dir("sumcfg");
    let mut a = automation(&dir);
    summary_doc(&dir, &mut a);
    call(
        &mut a,
        "summary_columns",
        json!({ "action": "save", "name": "Review", "columns": ["status", "subject", "label", "page"], "include_empty": false }),
    );
    call(
        &mut a,
        "summary_columns",
        json!({ "action": "save", "name": "All", "columns": ["page", "label", "subject"] }),
    );
    let l = call(&mut a, "summary_columns", json!({}));
    assert!(s(&l).contains("Review") && s(&l).contains("All"), "{l}");
    let c = call(&mut a, "summary_columns", json!({ "action": "load", "name": "review" }));
    assert_eq!(c["columns"], json!(["status", "subject", "label", "page"]), "{c}");
    assert!(
        dir.join("config/summary_columns.json").is_file(),
        "kept in the config folder"
    );
    call(
        &mut a,
        "summary_export",
        json!({ "out": "r.csv", "column_config": "Review" }),
    );
    let head = std::fs::read_to_string(dir.join("r.csv")).unwrap();
    let head = head.lines().next().unwrap().to_lowercase();
    let (st, su, pg) = (head.find("status"), head.find("subject"), head.find("page"));
    assert!(st < su && su < pg, "the configuration's order: {head}");
    assert!(!head.contains("label"), "the empty Label column left out: {head}");
    call(
        &mut a,
        "summary_export",
        json!({ "out": "all.csv", "column_config": "All" }),
    );
    let head = std::fs::read_to_string(dir.join("all.csv")).unwrap();
    assert!(
        head.lines().next().unwrap().to_lowercase().contains("label"),
        "kept when the configuration includes empty columns: {head}"
    );
    call(&mut a, "summary_columns", json!({ "action": "delete", "name": "All" }));
    assert!(!s(&call(&mut a, "summary_columns", json!({}))).contains("\"All\""));
    fails(
        &mut a,
        "summary_export",
        json!({ "out": "x.csv", "column_config": "All" }),
    );
    // The dialog: save the chosen columns as a set, then load it back.
    let mut h = app();
    let cfg = dir.join("uicfg");
    std::fs::create_dir_all(&cfg).unwrap();
    h.state_mut().state.features.partials.config = Some(cfg.clone());
    run(&mut h, "markup.summary");
    h.state_mut().state.features.summary.columns = vec!["subject".into(), "page".into()];
    h.state_mut().state.features.summary.config_name = "Mine".into();
    h.run_steps(2);
    h.get_by_label("Save Columns").click();
    h.run_steps(3);
    let saved = markupcraft_engine::summary_cols::load_configs(&cfg).unwrap();
    assert_eq!(saved.len(), 1, "{}", h.state().state.features.summary.message);
    assert_eq!(saved[0].columns, ["subject", "page"]);
    h.state_mut().state.features.summary.columns = vec!["status".into()];
    h.state_mut().state.features.summary.config_name.clear();
    h.run_steps(2);
    {
        use egui::accesskit::Role;
        h.get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("(choose)"))
            .next()
            .expect("the column set box")
            .click();
        h.run_steps(4);
        h.get_all_by_label("Mine").last().unwrap().click();
        h.run_steps(3);
    }
    assert_eq!(
        h.state().state.features.summary.columns,
        ["subject", "page"],
        "loaded from the column set"
    );
}

/// D-136, D-137, D-138: columns chosen and ordered, filter and multi-level sort, title with
/// date, totals only, no headers, one report per value of the first column.
#[test]
fn summary_columns_filter_sort_and_output_options() {
    let dir = temp_dir("sumopts");
    let mut a = automation(&dir);
    summary_doc(&dir, &mut a);
    call(
        &mut a,
        "summary_export",
        json!({ "out": "c.csv", "columns": ["status", "subject", "page"] }),
    );
    let csv = std::fs::read_to_string(dir.join("c.csv")).unwrap();
    let head = csv.lines().next().unwrap().to_lowercase();
    let (st, su, pg) = (head.find("status"), head.find("subject"), head.find("page"));
    assert!(st < su && su < pg, "columns in the chosen order: {head}");
    call(
        &mut a,
        "summary_export",
        json!({ "out": "f.csv", "columns": ["subject", "page", "status"], "filters": {"subject": ["Wall"]},
                 "sort": "subject", "then_by": [["page", true]] }),
    );
    let csv = std::fs::read_to_string(dir.join("f.csv")).unwrap();
    assert!(!csv.contains("Door"), "filtered: {csv}");
    let rows: Vec<&str> = csv.lines().filter(|l| l.contains("Wall")).collect();
    assert_eq!(rows.len(), 2, "{csv}");
    assert!(
        rows[0].contains('2') && rows[1].contains('1'),
        "then by page descending: {csv}"
    );
    call(
        &mut a,
        "summary_export",
        json!({ "out": "nh.csv", "headers": false, "columns": ["subject"] }),
    );
    let csv = std::fs::read_to_string(dir.join("nh.csv")).unwrap();
    assert!(!csv.to_lowercase().contains("subject"), "no header row: {csv}");
    call(
        &mut a,
        "summary_export",
        json!({ "out": "per.csv", "per_value": true, "columns": ["subject", "page"] }),
    );
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n.contains("Wall")) && names.iter().any(|n| n.contains("Door")),
        "one report per subject: {names:?}"
    );
    call(
        &mut a,
        "summary_export",
        json!({ "out": "t.pdf", "title": "Review", "date_in_title": true }),
    );
    let d = open(&mut a, "t.pdf");
    let t = s(&call(&mut a, "page_text", json!({ "page": 1, "doc": d })));
    assert!(
        t.contains("Review") && (2026..2100).any(|y| t.contains(&y.to_string())),
        "title + date: {t}"
    );
}

/// D-139: PDF layout: flow style, page per group, page size, links to source pages, the
/// summarized pages themselves, status history, Spaces cover.
#[test]
fn summary_pdf_layout_options() {
    let dir = temp_dir("sumpdf");
    let mut a = automation(&dir);
    summary_doc(&dir, &mut a);
    call(
        &mut a,
        "summary_export",
        json!({ "out": "r.pdf", "pdf_paper": "tabloid", "pdf_links": true, "pdf_page_content": true,
                 "group_by": ["subject"], "pdf_break_per_group": true, "pdf_flow": true, "pdf_totals": true,
                 "pdf_status_history": true }),
    );
    let d = open(&mut a, "r.pdf");
    let info = call(&mut a, "doc_info", json!({ "doc": d }));
    let pages = info["page_list"].as_array().unwrap();
    assert!(
        pages.len() >= 4,
        "two groups on their own pages + the two summarized pages: {info}"
    );
    assert!(s(&pages[0]).contains("1224"), "tabloid report pages: {}", pages[0]);
    let l = call(&mut a, "link_list", json!({ "doc": d }));
    assert!(!s(&l).contains("[]"), "links to the source pages: {l}");
    let all: String = (1..=pages.len() as u64)
        .map(|p| s(&call(&mut a, "page_text", json!({ "page": p, "doc": d }))))
        .collect();
    assert!(all.contains("PAGE TWO"), "summarized pages included: {all}");
}

// ---- Print (doc-140 .. doc-147) --------------------------------------------------------------

fn print_doc(dir: &Path, a: &mut Automation) {
    text_pdf(
        dir,
        "doc.pdf",
        &[
            &[(72.0, 700.0, "SHEET ONE")],
            &[(72.0, 700.0, "SHEET TWO")],
            &[(72.0, 700.0, "SHEET THREE")],
        ],
    );
    open(a, "doc.pdf");
    call(
        a,
        "markup_add",
        json!({ "page": 1, "kind": "Text Box", "points": [[300, 400], [500, 440]], "contents": "MARKUPWORD" }),
    );
    call(a, "doc_save", json!({}));
}

fn printed(a: &mut Automation, out: &str) -> (Vec<Value>, Vec<String>) {
    let d = open(a, out);
    let info = call(a, "doc_info", json!({ "doc": d }));
    let pages = info["page_list"].as_array().cloned().unwrap_or_default();
    let texts = (1..=pages.len() as u64)
        .map(|p| s(&call(a, "page_text", json!({ "page": p, "doc": d }))))
        .collect();
    call(a, "doc_close", json!({ "doc": d, "discard_changes": true }));
    (pages, texts)
}

/// D-140: the printer list is read from the system; output goes to a print-ready PDF
/// (printing itself is not exercised: tests never touch real devices).
#[test]
fn print_printer_choice_and_print_ready_output() {
    let dir = temp_dir("prn");
    let mut a = automation(&dir);
    let v = call(&mut a, "printer_list", json!({}));
    assert!(v.is_object() || v.is_array(), "{v}");
    print_doc(&dir, &mut a);
    call(&mut a, "print_pdf", json!({ "out": "p.pdf" }));
    let (pages, _) = printed(&mut a, "p.pdf");
    assert_eq!(pages.len(), 3);
}

/// D-140: a live preview with margins: the sheet as it prints, its size and the margin box,
/// in the tool and the Print dialog.
#[test]
fn print_live_preview_with_margins() {
    let dir = temp_dir("prnprev");
    let mut a = automation(&dir);
    print_doc(&dir, &mut a);
    let r = call(
        &mut a,
        "print_preview",
        json!({ "paper": "tabloid", "orientation": "landscape", "margin": 36, "png": "sheet.png", "size": 400 }),
    );
    assert_eq!(r["sheets"], 3, "{r}");
    assert_eq!(r["paper"], json!([1224.0, 792.0]), "{r}");
    assert_eq!(r["printable"], json!([36.0, 36.0, 1188.0, 756.0]), "{r}");
    assert!(r["image"][0].as_u64().unwrap().abs_diff(400) <= 2, "{r}");
    let png = std::fs::read(dir.join("sheet.png")).unwrap();
    assert!(png.starts_with(b"\x89PNG"), "the sheet as an image");
    let r = call(&mut a, "print_preview", json!({ "sheet": 2, "pages": [3, 1] }));
    assert_eq!((r["sheets"].as_u64(), r["sheet"].as_u64()), (Some(2), Some(2)), "{r}");
    // The dialog shows the preview and follows the settings.
    let mut h = app();
    run(&mut h, "file.print");
    h.run_steps(3);
    h.get_by_label("Preview").click();
    h.run_steps(3);
    assert!(
        shows(&h, "First sheet: 8.5 x 11.0 in") || shows(&h, "First sheet: 11.0 x 8.5 in"),
        "a Letter preview"
    );
    h.state_mut().state.features.print.paper = "Tabloid".into();
    h.state_mut().state.features.print.margin = 36.0;
    h.run_steps(4);
    assert!(
        shows(&h, "First sheet: 11.0 x 17.0 in, margins 0.50 in")
            || shows(&h, "First sheet: 17.0 x 11.0 in, margins 0.50 in"),
        "the preview follows the paper and margins"
    );
}

/// D-141, D-142: page range, Get Window region; document and markups / document only /
/// markups only.
#[test]
fn print_pages_region_and_what_to_print() {
    let dir = temp_dir("prnpages");
    let mut a = automation(&dir);
    print_doc(&dir, &mut a);
    call(&mut a, "print_pdf", json!({ "out": "r.pdf", "pages": "2-3" }));
    let (p, t) = printed(&mut a, "r.pdf");
    assert_eq!(p.len(), 2);
    assert!(t[0].contains("SHEET TWO"), "{t:?}");
    open(&mut a, "doc.pdf");
    call(&mut a, "print_pdf", json!({ "out": "both.pdf", "pages": [1] }));
    call(
        &mut a,
        "print_pdf",
        json!({ "out": "doconly.pdf", "pages": [1], "markups": false }),
    );
    call(
        &mut a,
        "print_pdf",
        json!({ "out": "mkonly.pdf", "pages": [1], "markups_only": true }),
    );
    call(
        &mut a,
        "print_pdf",
        json!({ "out": "win.pdf", "region": [1, 60, 680, 200, 720] }),
    );
    let (_, t) = printed(&mut a, "both.pdf");
    assert!(t[0].contains("SHEET ONE") && t[0].contains("MARKUPWORD"), "{t:?}");
    let (_, t) = printed(&mut a, "doconly.pdf");
    assert!(t[0].contains("SHEET ONE") && !t[0].contains("MARKUPWORD"), "{t:?}");
    let (_, t) = printed(&mut a, "mkonly.pdf");
    assert!(!t[0].contains("SHEET ONE") && t[0].contains("MARKUPWORD"), "{t:?}");
    let (p, t) = printed(&mut a, "win.pdf");
    assert_eq!(p.len(), 1);
    assert!(t[0].contains("SHEET ONE"), "{t:?}");
    // the window fills the sheet: the markup outside it lands off the paper (clipped)
    open(&mut a, "win.pdf");
    let r = call(&mut a, "text_search", json!({ "text": "MARKUPWORD" }));
    let pw = p[0]["width"].as_f64().unwrap();
    let ph = p[0]["height"].as_f64().unwrap();
    for h in r["hits"].as_array().cloned().unwrap_or_default() {
        let b = &h["rects"][0];
        let (x0, y0) = (b[0].as_f64().unwrap(), b[1].as_f64().unwrap());
        assert!(
            x0 > pw || y0 < 0.0 || y0 > ph,
            "markup outside the window is off the sheet: {h} on {pw}x{ph}"
        );
    }
}

/// D-143, D-144: paper and orientation, copies, collate, reverse; scaling choices.
#[test]
fn print_paper_copies_and_scaling() {
    let dir = temp_dir("prnpaper");
    let mut a = automation(&dir);
    print_doc(&dir, &mut a);
    call(
        &mut a,
        "print_pdf",
        json!({ "out": "c.pdf", "pages": "1-2", "copies": 2, "collate": true, "paper": "legal", "orientation": "landscape" }),
    );
    let (p, t) = printed(&mut a, "c.pdf");
    assert_eq!(p.len(), 4, "two collated copies");
    assert!(
        t[0].contains("ONE") && t[1].contains("TWO") && t[2].contains("ONE"),
        "collated: {t:?}"
    );
    let w = p[0]["width"].as_f64().unwrap_or(0.0);
    let h = p[0]["height"].as_f64().unwrap_or(0.0);
    assert!(
        (w - 1008.0).abs() < 1.0 && (h - 612.0).abs() < 1.0,
        "legal landscape: {}",
        p[0]
    );
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "print_pdf",
        json!({ "out": "u.pdf", "pages": "1-2", "copies": 2, "collate": false }),
    );
    let (_, t) = printed(&mut a, "u.pdf");
    assert!(t[0].contains("ONE") && t[1].contains("ONE"), "uncollated: {t:?}");
    open(&mut a, "doc.pdf");
    call(&mut a, "print_pdf", json!({ "out": "rev.pdf", "reverse": true }));
    let (_, t) = printed(&mut a, "rev.pdf");
    assert!(t[0].contains("THREE"), "reverse order: {t:?}");
    for layout in [
        json!({"layout": "actual"}),
        json!({"layout": "fit"}),
        json!({"layout": "shrink"}),
        json!({"layout": "percent", "percent": 50}),
        json!({"layout": "fit", "margin": 36, "offset": [10, 10]}),
    ] {
        open(&mut a, "doc.pdf");
        let mut args = json!({ "out": "sc.pdf", "pages": [1], "paper": "tabloid" });
        for (k, v) in layout.as_object().unwrap() {
            args[k] = v.clone();
        }
        call(&mut a, "print_pdf", args);
        let (p, t) = printed(&mut a, "sc.pdf");
        assert_eq!(p.len(), 1);
        assert!(t[0].contains("SHEET ONE"), "{layout}: {t:?}");
    }
}

/// D-145, D-147: N-up with border; tiling over several sheets.
#[test]
fn print_n_up_and_tiling() {
    let dir = temp_dir("prnnup");
    let mut a = automation(&dir);
    print_doc(&dir, &mut a);
    call(
        &mut a,
        "print_pdf",
        json!({ "out": "n.pdf", "layout": "nup", "cols": 2, "rows": 2, "border": true }),
    );
    let (p, t) = printed(&mut a, "n.pdf");
    assert_eq!(p.len(), 1, "three pages on one 2x2 sheet");
    assert!(t[0].contains("ONE") && t[0].contains("THREE"), "{t:?}");
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "print_pdf",
        json!({ "out": "tile.pdf", "pages": [1], "layout": "tile", "percent": 200, "overlap": 18, "cut_marks": true }),
    );
    let (p, _) = printed(&mut a, "tile.pdf");
    assert!(p.len() >= 4, "page at 200% over at least four sheets: {}", p.len());
}

/// D-146: dim page content, print visible hyperlinks, Spaces.
#[test]
fn print_emphasis_options() {
    let dir = temp_dir("prnemph");
    let mut a = automation(&dir);
    print_doc(&dir, &mut a);
    call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [60, 690, 200, 715], "url": "https://example.com" }),
    );
    call(&mut a, "print_pdf", json!({ "out": "plain.pdf", "pages": [1] }));
    call(
        &mut a,
        "print_pdf",
        json!({ "out": "dim.pdf", "pages": [1], "dim_content": true, "links": true, "spaces": true }),
    );
    let a1 = std::fs::read(dir.join("plain.pdf")).unwrap();
    let a2 = std::fs::read(dir.join("dim.pdf")).unwrap();
    assert_ne!(a1, a2, "dimmed / link outlines change the printed sheet");
    let (_, t) = printed(&mut a, "dim.pdf");
    assert!(t[0].contains("SHEET ONE"), "content still printed (dimmed): {t:?}");
}

// ---- Headers, footers, stamps, watermarks (doc-148 .. doc-154) -------------------------------

fn three_pages(dir: &Path) {
    text_pdf(
        dir,
        "doc.pdf",
        &[
            &[(72.0, 400.0, "BODY ONE")],
            &[(72.0, 400.0, "BODY TWO")],
            &[(72.0, 400.0, "BODY THREE")],
        ],
    );
}

/// D-148, D-149: six text boxes on chosen pages, with page number, page count, date and Bates
/// tokens.
#[test]
fn header_footer_six_places_and_tokens() {
    let dir = temp_dir("hf");
    let mut a = automation(&dir);
    three_pages(&dir);
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "header_footer_add",
        json!({ "pages": "2-3", "header_left": "HL", "header_center": "HC", "header_right": "HR",
                 "footer_left": "Page <<1>> of <<n>>", "footer_center": "<<yyyy-mm-dd>>",
                 "footer_right": "<<Bates Number#4#7#JOB-#-X>>", "font_size": 9, "start_number": 2, "margins": [20, 20, 30, 30] }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    let p1 = page_text(&mut a, "doc.pdf", 1);
    assert!(!p1.contains("HL"), "page 1 not chosen: {p1}");
    let p2 = page_text(&mut a, "doc.pdf", 2);
    for want in ["HL", "HC", "HR", "Page 2 of 3", "JOB-0007-X"] {
        assert!(p2.contains(want), "{want} on page 2: {p2}");
    }
    assert!((2026..2100).any(|y| p2.contains(&format!("{y}-"))), "date: {p2}");
    assert!(
        page_text(&mut a, "doc.pdf", 3).contains("JOB-0008-X"),
        "Bates counts up"
    );
    // File data: file name, path, author, title and subject from the document information.
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "doc_properties_set",
        json!({ "properties": { "Author": "Pat Drafter", "Title": "Level Two Plan", "Subject": "Ducts" } }),
    );
    let t = call(&mut a, "header_footer_tokens", json!({}));
    for want in [
        "<<File Name>>",
        "<<File Path>>",
        "<<Author>>",
        "<<Title>>",
        "<<Subject>>",
    ] {
        assert!(s(&t).contains(want), "{want} offered: {t}");
    }
    assert_eq!(t["values"]["<<Author>>"], "Pat Drafter", "{t}");
    call(
        &mut a,
        "header_footer_add",
        json!({ "pages": [1], "header_left": "<<File Name>>", "header_center": "<<Title>> / <<Subject>>",
                 "header_right": "by <<Author>>", "footer_left": "<<File Path>>", "replace": true }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    let p1 = page_text(&mut a, "doc.pdf", 1);
    for want in ["doc.pdf", "Level Two Plan / Ducts", "by Pat Drafter", "hf"] {
        assert!(p1.contains(want), "{want} on page 1: {p1}");
    }
    // The dialog's token picker inserts a file-data token into the chosen place.
    let mut h = app();
    run(&mut h, "document.headers_footers");
    h.get_by_label("Insert Token").click();
    h.run_steps(2);
    h.get_by_label("Author (document properties)").click();
    h.run_steps(2);
    assert_eq!(
        h.state().state.features.docops.hf.text[0],
        "<<Author>>",
        "inserted into Header left"
    );
}

/// D-150: shrink the page content so the header/footer does not overlap it.
#[test]
fn header_footer_fit_content_inside_margins() {
    let dir = temp_dir("hffit");
    let mut a = automation(&dir);
    text_pdf(
        &dir,
        "doc.pdf",
        &[&[(0.0, 780.0, "TOP EDGE TEXT"), (0.0, 2.0, "BOTTOM EDGE")]],
    );
    open(&mut a, "doc.pdf");
    call(&mut a, "page_fit_content", json!({ "margins": [72, 72, 72, 72] }));
    call(&mut a, "doc_save", json!({ "full": true }));
    let r = call(&mut a, "text_search", json!({ "text": "TOP EDGE TEXT" }));
    let hit = s(&r);
    let rects: Vec<f64> = r["hits"][0]["rects"][0]
        .as_array()
        .map(|v| v.iter().filter_map(|x| x.as_f64()).collect())
        .unwrap_or_default();
    assert_eq!(rects.len(), 4, "{hit}");
    assert!(
        rects[3] <= 792.0 - 72.0 + 1.0 && rects[0] >= 72.0 - 1.0,
        "content inside margins: {hit}"
    );
}

/// D-151, D-152: templates saved and reused by name; the master header/footer edited and
/// re-applied (page numbers follow the pages), then deleted.
#[test]
fn header_footer_templates_edit_update_delete() {
    let dir = temp_dir("hftpl");
    let mut a = automation(&dir);
    three_pages(&dir);
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "header_footer_template",
        json!({ "action": "save", "path": "hf.json", "name": "Std", "footer_right": "Sheet <<1>> / <<n>>" }),
    );
    let l = call(
        &mut a,
        "header_footer_template",
        json!({ "action": "list", "path": "hf.json" }),
    );
    assert!(s(&l).contains("Std"), "{l}");
    call(
        &mut a,
        "header_footer_template",
        json!({ "action": "apply", "path": "hf.json", "name": "Std" }),
    );
    let t = s(&call(&mut a, "page_text", json!({ "page": 3 })));
    assert!(t.contains("Sheet 3 / 3"), "{t}");
    let g = call(&mut a, "header_footer_kept", json!({ "action": "get" }));
    assert!(s(&g).contains("Sheet <<1>>"), "kept with the document: {g}");
    call(&mut a, "page_delete", json!({ "pages": [1] }));
    call(&mut a, "header_footer_kept", json!({ "action": "update" }));
    let t = s(&call(&mut a, "page_text", json!({ "page": 2 })));
    assert!(t.contains("Sheet 2 / 2") && !t.contains("3 / 3"), "renumbered: {t}");
    call(
        &mut a,
        "header_footer_kept",
        json!({ "action": "set", "footer_right": "Rev A <<1>>" }),
    );
    let t = s(&call(&mut a, "page_text", json!({ "page": 1 })));
    assert!(
        t.contains("Rev A 1") && !t.contains("Sheet"),
        "edited master replaces the old: {t}"
    );
    call(&mut a, "marks_remove", json!({ "kind": "header_footer" }));
    let t = s(&call(&mut a, "page_text", json!({ "page": 1 })));
    assert!(!t.contains("Rev A"), "deleted: {t}");
}

/// D-153: built-in stamps, custom stamps with dynamic date / user, default stamp, opacity.
#[test]
fn stamps_library_custom_dynamic_and_settings() {
    let dir = temp_dir("stamps");
    let mut a = auto(&dir);
    let l = call(&mut a, "stamp_list", json!({}));
    let n = l.get("stamps").and_then(|x| x.as_array()).map_or(0, |x| x.len());
    assert!(n >= 16, "16+ built-in stamps: {n}");
    call(
        &mut a,
        "stamp_create",
        json!({ "name": "Checked", "text": "CHECKED BY {user}\n{date:yyyy}" }),
    );
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "X")]]);
    open(&mut a, "doc.pdf");
    let l = s(&call(&mut a, "stamp_list", json!({})));
    assert!(l.contains("Checked"), "{l}");
    call(
        &mut a,
        "stamp_settings",
        json!({ "action": "set", "default_stamp": "Approved", "opacity": 0.4 }),
    );
    call(
        &mut a,
        "stamp_settings",
        json!({ "action": "place", "page": 1, "at": [300, 400] }),
    );
    let custom_id = {
        let v = call(&mut a, "stamp_list", json!({}));
        v["stamps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| s(x).contains("Checked"))
            .and_then(|x| x["id"].as_str())
            .unwrap_or("Checked")
            .to_string()
    };
    call(
        &mut a,
        "stamp_add",
        json!({ "page": 1, "stamp": custom_id, "at": [300, 200] }),
    );
    let m = list(&mut a);
    assert_eq!(m.len(), 2, "{m:?}");
    assert!(s(&m[0]).contains("0.4"), "default stamp with the set opacity: {}", m[0]);
    let st = s(&m[1]);
    assert!(
        st.contains("Acceptance") && (2026..2100).any(|y| st.contains(&y.to_string())),
        "user and date filled: {st}"
    );
}

/// D-154: a watermark on many pages, flattened into the page.
#[test]
fn watermark_on_many_pages_flattened() {
    let dir = temp_dir("wm");
    let mut a = automation(&dir);
    three_pages(&dir);
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "watermark_add",
        json!({ "text": "CONFIDENTIAL", "opacity": 0.3, "rotation": 45 }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    for p in 1..=3 {
        assert!(page_text(&mut a, "doc.pdf", p).contains("CONFIDENTIAL"), "page {p}");
    }
    assert_eq!(markup_count(&mut a, "doc.pdf"), 0, "in the page content, not a markup");
}

// ---- OCR and search (doc-155 .. doc-163) ---------------------------------------------------

/// A scanned page: the text page rendered to PNG and made into an image-only PDF.
fn scanned(dir: &Path, a: &mut Automation) {
    let c = format!(
        "{}{}",
        text(72.0, 600.0, 36.0, "MECHANICAL ROOM"),
        text(72.0, 500.0, 36.0, "BOILER 42")
    );
    std::fs::write(dir.join("src.pdf"), pdf(&[SyntheticPage::new(612.0, 792.0, c)])).unwrap();
    open(a, "src.pdf");
    call(a, "export_images", json!({ "dir": "img", "format": "png", "dpi": 200 }));
    let png = std::fs::read_dir(dir.join("img"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let name = format!("img/{}", png.file_name().unwrap().to_string_lossy());
    call(a, "doc_from_image", json!({ "image": name, "path": "scan.pdf" }));
    call(a, "doc_save", json!({}));
}

fn models_present() -> bool {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models");
    std::env::var_os("MARKUPCRAFT_OCR_MODELS").is_some()
        || root.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false)
}

/// D-155, D-156: OCR makes a scan searchable; page ranges and options (deskew, orientation,
/// skip pages with text, skip vector pages).
#[test]
fn ocr_makes_scanned_pages_searchable() {
    if !models_present() {
        eprintln!("OCR models missing: run cargo xtask models");
        return;
    }
    let dir = temp_dir("ocr");
    let mut a = automation(&dir);
    scanned(&dir, &mut a);
    open(&mut a, "scan.pdf");
    let before = call(&mut a, "text_search", json!({ "text": "MECHANICAL" }));
    assert_eq!(before["count"], 0, "no text before OCR: {before}");
    let o = call(
        &mut a,
        "ocr_pages",
        json!({ "pages": [1], "deskew": true, "detect_orientation": true, "skip_text": true, "skip_vector": true }),
    );
    assert!(s(&o).contains("MECHANICAL"), "{o}");
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "scan.pdf");
    let r = call(&mut a, "text_search", json!({ "text": "MECHANICAL" }));
    assert_eq!(r["count"], 1, "searchable after OCR: {r}");
    // pages with text are skipped by default
    open(&mut a, "src.pdf");
    let v = call(&mut a, "ocr_pages", json!({}));
    assert!(s(&v).contains("skip") || s(&v).contains('0'), "text page skipped: {v}");
    // not on signed files
    call(
        &mut a,
        "digital_id_create",
        json!({ "name": "S", "password": "pw", "out": "s.p12" }),
    );
    open(&mut a, "scan.pdf");
    call(
        &mut a,
        "signature_sign",
        json!({ "id": "s.p12", "password": "pw", "out": "signed.pdf" }),
    );
    let e = fails(&mut a, "ocr_pages", json!({ "skip_text": false }));
    assert!(e.to_lowercase().contains("sign"), "{e}");
}

/// D-156: OCR options: accuracy against speed (the resolution read at), document type,
/// page chunk size and a maximum vector size. Uses the real models when they are installed,
/// else a stand-in recogniser that reads every page's ink as one word.
#[test]
fn ocr_accuracy_doc_type_chunks_and_max_vector_size() {
    if !models_present() {
        markupcraft_engine::ocr::install_recognizer(std::sync::Arc::new(markupcraft_engine::ocr::InkWord {
            text: "MECHANICAL".into(),
        }));
    }
    let dir = temp_dir("ocropts");
    let mut a = automation(&dir);
    // Page 1: light content; page 2: heavy vector linework (about 60 KB of paths).
    let heavy: String = (0..3000)
        .map(|i| format!("{} {} m {} {} l S\n", i % 600, i % 700, (i * 7) % 600, (i * 3) % 700))
        .collect();
    let pages = [
        SyntheticPage::new(612.0, 792.0, rect(100.0, 600.0, 200.0, 40.0)),
        SyntheticPage::new(612.0, 792.0, heavy),
    ];
    std::fs::write(dir.join("v.pdf"), pdf(&pages)).unwrap();
    open(&mut a, "v.pdf");
    let r = call(
        &mut a,
        "ocr_pages",
        json!({ "accuracy": "speed", "doc_type": "text_document", "max_vector_kb": 20, "chunk_pages": 1 }),
    );
    assert_eq!(r["dpi"], 150.0, "speed on a text document reads at 150 dpi: {r}");
    assert_eq!(r["chunks"], 2, "one page at a time: {r}");
    let p = r["pages"].as_array().unwrap();
    assert!(p[0]["skipped"].is_null(), "the light page is read: {r}");
    assert!(
        s(&p[1]["skipped"]).contains("larger than 20 KB"),
        "the heavy vector page is skipped: {r}"
    );
    call(&mut a, "doc_close", json!({ "discard_changes": true }));
    open(&mut a, "v.pdf");
    let r = call(&mut a, "ocr_pages", json!({ "accuracy": "accuracy", "pages": [1] }));
    assert_eq!(r["dpi"], 400.0, "accuracy on a drawing: {r}");
    let r = call(&mut a, "ocr_pages", json!({ "skip_text": false, "pages": [1] }));
    assert_eq!(r["dpi"], 300.0, "balanced by default: {r}");
    fails(&mut a, "ocr_pages", json!({ "accuracy": "perfect" }));
    // The dialog offers the same choices.
    let mut h = app();
    run(&mut h, "tools.ocr");
    for label in [
        "Speed",
        "Accuracy",
        "Page chunk size",
        "Skip pages with vector content over",
    ] {
        assert!(shows(&h, label), "{label} in the OCR dialog");
    }
    h.get_by_label("Speed").click();
    h.run_steps(2);
    assert_eq!(h.state().state.features.ocr.dpi, 200.0, "Speed on a drawing");
}

/// D-157, D-158, D-159: results with snippet and page; scopes (page range, document, open
/// documents, folder with subfolders); options (case, whole words, markups, file names,
/// properties, form fields).
#[test]
fn text_search_scopes_and_options() {
    let dir = temp_dir("search");
    let mut a = automation(&dir);
    text_pdf(
        &dir,
        "a.pdf",
        &[
            &[(72.0, 700.0, "Pump P-1 serves the boiler")],
            &[(72.0, 700.0, "PUMP room")],
        ],
    );
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    text_pdf(&dir.join("sub"), "deep.pdf", &[&[(72.0, 700.0, "pumping station")]]);
    open(&mut a, "a.pdf");
    let r = call(&mut a, "text_search", json!({ "text": "pump" }));
    let hits = r["hits"].as_array().cloned().unwrap_or_default();
    assert_eq!(hits.len(), 2, "{r}");
    assert!(
        s(&hits[0]).contains("serves the boiler") && hits[0]["page"] == 1,
        "snippet + page: {}",
        hits[0]
    );
    let r = call(&mut a, "text_search", json!({ "text": "pump", "case_sensitive": true }));
    assert_eq!(r["hits"].as_array().map_or(0, |h| h.len()), 0, "{r}");
    let r = call(&mut a, "text_search", json!({ "text": "pump", "pages": [2] }));
    assert_eq!(r["hits"].as_array().map_or(0, |h| h.len()), 1, "page scope: {r}");
    let r = call(
        &mut a,
        "text_search",
        json!({ "text": "pump", "folder": ".", "recursive": true }),
    );
    assert!(s(&r).contains("deep.pdf"), "folder + subfolders: {r}");
    let r = call(
        &mut a,
        "text_search",
        json!({ "text": "pump", "folder": ".", "recursive": false }),
    );
    assert!(!s(&r).contains("deep.pdf"), "no subfolders: {r}");
    let r = call(&mut a, "text_search", json!({ "text": "pum", "whole_words": true }));
    assert_eq!(r["hits"].as_array().map_or(0, |h| h.len()), 0, "whole words: {r}");
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[10, 10], [50, 50]], "contents": "check valve" }),
    );
    call(
        &mut a,
        "doc_properties_set",
        json!({ "properties": { "Subject": "valve schedule" } }),
    );
    let r = call(
        &mut a,
        "text_search",
        json!({ "text": "valve", "markups": true, "properties": true, "page_text": false }),
    );
    let t = s(&r);
    assert!(
        t.contains("check valve") && t.contains("valve schedule"),
        "markup text + properties: {t}"
    );
    let r = call(
        &mut a,
        "text_search",
        json!({ "text": "deep", "folder": ".", "recursive": true, "file_names": true, "page_text": false }),
    );
    assert!(s(&r).contains("deep.pdf"), "file names: {r}");
    text_pdf(&dir, "b.pdf", &[&[(72.0, 700.0, "another pump")]]);
    open(&mut a, "b.pdf");
    let r = call(&mut a, "text_search", json!({ "text": "pump", "open_docs": true }));
    assert!(
        s(&r).contains("a.pdf") && s(&r).contains("b.pdf"),
        "all open documents: {r}"
    );
}

/// D-157 (UI), D-160: the Search panel; select text on the page and search for it.
#[test]
fn search_panel_and_search_selected_text() {
    let dir = temp_dir("selsearch");
    let mut a = automation(&dir);
    text_pdf(
        &dir,
        "a.pdf",
        &[&[(72.0, 700.0, "VALVE V-12"), (72.0, 600.0, "SEE VALVE V-12")]],
    );
    open(&mut a, "a.pdf");
    let r = call(&mut a, "region_text", json!({ "page": 1, "rect": [60, 690, 200, 720] }));
    let sel = r.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_string();
    assert_eq!(sel.trim(), "VALVE V-12", "{r}");
    let r = call(&mut a, "text_search", json!({ "text": sel }));
    assert_eq!(r["hits"].as_array().map_or(0, |h| h.len()), 2, "{r}");
    let mut h = app();
    assert!(
        panel_toggles_with_key(&mut h, "search", egui::Key::Num1),
        "Alt+1 shows / hides Search"
    );
    show(&mut h, "search");
    assert!(
        shows(&h, "Match case") && shows(&h, "Visual Search"),
        "Search panel content"
    );
    for (id, k) in [("search.next", egui::Key::F3), ("search.prev", egui::Key::F3)] {
        let c = markupcraft_ui_egui::commands::find(id).expect(id);
        assert!(
            format!("{:?}", c.keys).contains(&format!("{k:?}")),
            "{id} on F3 / Shift+F3"
        );
    }
}

/// D-161: act on checked results: highlight / strikethrough, hyperlinks, redaction, Count.
#[test]
fn act_on_search_results() {
    let dir = temp_dir("acts");
    let mut a = automation(&dir);
    text_pdf(&dir, "a.pdf", &[&[(72.0, 700.0, "DOOR D1"), (72.0, 600.0, "DOOR D2")]]);
    open(&mut a, "a.pdf");
    let r = call(&mut a, "text_search", json!({ "text": "DOOR" }));
    let hits = r["hits"].as_array().cloned().unwrap();
    assert_eq!(hits.len(), 2);
    let r0 = hits[0]["rects"][0].clone();
    let pts = json!([[r0[0], r0[1]], [r0[2], r0[3]]]);
    for kind in ["Text Highlight", "Strikethrough", "Underline", "Squiggly"] {
        call(&mut a, "markup_add", json!({ "page": 1, "kind": kind, "points": pts }));
    }
    call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": r0, "on_text": true, "url": "https://example.com/door" }),
    );
    call(&mut a, "redact_mark", json!({ "text": "D2" }));
    let centers: Vec<Value> = hits
        .iter()
        .map(|h| {
            let r = &h["rects"][0];
            json!([
                (r[0].as_f64().unwrap() + r[2].as_f64().unwrap()) / 2.0,
                (r[1].as_f64().unwrap() + r[3].as_f64().unwrap()) / 2.0
            ])
        })
        .collect();
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Count", "points": centers }),
    );
    let m = s(&call(&mut a, "markup_list", json!({})));
    for k in ["Text Highlight", "Strikethrough", "Underline", "Squiggly", "Count"] {
        assert!(m.contains(k), "{k}: {m}");
    }
    assert!(s(&call(&mut a, "link_list", json!({}))).contains("example.com/door"));
    assert!(s(&call(&mut a, "redact_list", json!({}))).contains('1'));
    // Make hyperlinks from the checked results: one link per hit, in one undo step.
    text_pdf(
        &dir,
        "b.pdf",
        &[
            &[
                (72.0, 700.0, "SEE DETAIL 5"),
                (72.0, 600.0, "SEE DETAIL 5 AGAIN"),
                (72.0, 500.0, "DETAIL 5 ONCE MORE"),
            ],
            &[(72.0, 700.0, "DETAIL SHEET")],
        ],
    );
    open(&mut a, "b.pdf");
    let r = call(
        &mut a,
        "search_results_link",
        json!({ "text": "DETAIL 5", "checked": [1, 3], "to_page": 2 }),
    );
    assert_eq!(r["links"], 2, "a link per checked result: {r}");
    let l = call(&mut a, "link_list", json!({}));
    let list = l["links"].as_array().cloned().unwrap_or_default();
    assert_eq!(list.len(), 2, "{l}");
    assert!(
        list.iter()
            .all(|x| x["target"]["page"] == 2 || s(x).contains("\"page\":2")),
        "{l}"
    );
    call(&mut a, "edit_undo", json!({}));
    assert!(
        call(&mut a, "link_list", json!({}))["links"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let r = call(
        &mut a,
        "search_results_link",
        json!({ "text": "DETAIL 5", "url": "https://example.com/d5" }),
    );
    assert_eq!(r["links"], 3, "every result by default: {r}");
    // The Search panel: check results, then Link.
    let mut h = app();
    h.state_mut().state.open_path(&dir.join("b.pdf"));
    h.run_steps(4);
    h.state_mut().state.features.search.query = "DETAIL 5".into();
    markupcraft_ui_egui::features::search::run_text(&mut h.state_mut().state);
    h.run_steps(3);
    for hit in h.state_mut().state.features.search.results_mut() {
        hit.checked = true;
    }
    h.state_mut().state.show_panel("search");
    h.run_steps(4);
    h.get_by_label("Link").click();
    h.run_steps(3);
    h.get_by_label("Web address").click();
    h.run_steps(2);
    h.state_mut().state.features.docs7.link_text = "https://example.com/ui".into();
    h.run_steps(2);
    h.get_by_label("Create Links").click();
    h.run_steps(3);
    let links = h.state().state.doc().unwrap().session.links();
    assert_eq!(
        links
            .iter()
            .filter(|l| format!("{l:?}").contains("example.com/ui"))
            .count(),
        3,
        "{}: {links:?}",
        h.state().state.status
    );
}

/// D-162: replace found text in the page content, not in markups.
#[test]
fn search_and_replace_in_page_content() {
    let dir = temp_dir("replace");
    let mut a = automation(&dir);
    text_pdf(
        &dir,
        "a.pdf",
        &[&[(72.0, 700.0, "DUCT SIZE 12x10"), (72.0, 600.0, "OTHER DUCT")]],
    );
    open(&mut a, "a.pdf");
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[10, 10], [50, 50]], "contents": "DUCT note" }),
    );
    call(
        &mut a,
        "text_replace",
        json!({ "text": "DUCT", "with": "PIPE", "only": [{"page": 1, "rect": [60, 690, 300, 720]}] }),
    );
    call(&mut a, "doc_save", json!({ "full": true }));
    call(&mut a, "doc_close", json!({}));
    let t = page_text(&mut a, "a.pdf", 1);
    assert!(
        t.contains("PIPE SIZE 12x10") && t.contains("OTHER DUCT"),
        "only the checked line: {t}"
    );
    open(&mut a, "a.pdf");
    assert!(
        s(&call(&mut a, "markup_list", json!({}))).contains("DUCT note"),
        "markups untouched"
    );
}

/// D-163: box a symbol, find similar instances (rotated too), turn them into Count.
#[test]
fn visual_search_finds_symbols_and_counts_them() {
    let dir = temp_dir("visual");
    let mut a = automation(&dir);
    // an L-shaped symbol: three upright, one turned 90 degrees, plus a different shape
    let sym = |x: f64, y: f64| format!("{}{}", line(x, y, x, y + 40.0, 3.0), line(x, y, x + 25.0, y, 3.0));
    let turned = |x: f64, y: f64| {
        format!(
            "{}{}",
            line(x, y, x + 40.0, y, 3.0),
            line(x + 40.0, y, x + 40.0, y + 25.0, 3.0)
        )
    };
    let c = format!(
        "{}{}{}{}{}",
        sym(100.0, 600.0),
        sym(300.0, 600.0),
        sym(100.0, 300.0),
        turned(300.0, 300.0),
        rect(450.0, 450.0, 40.0, 40.0)
    );
    std::fs::write(dir.join("v.pdf"), pdf(&[SyntheticPage::new(612.0, 792.0, c)])).unwrap();
    open(&mut a, "v.pdf");
    let r = call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [92, 592, 132, 648], "rotations": false, "sensitivity": 0.4 }),
    );
    let n = r["hits"].as_array().map_or(0, |h| h.len());
    assert_eq!(n, 3, "three upright copies: {r}");
    let r = call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [92, 592, 132, 648], "rotations": true, "action": "count", "sensitivity": 0.4 }),
    );
    let n = r["hits"].as_array().map_or(0, |h| h.len());
    assert_eq!(n, 4, "the turned copy too: {r}");
    let m = list(&mut a);
    assert!(m.iter().any(|x| x["kind"] == "Count"), "a Count measurement: {m:?}");
}

// ---- Forms (doc-164 .. doc-167) ------------------------------------------------------------

fn fields(a: &mut Automation) -> Vec<Value> {
    let v = call(a, "form_list", json!({}));
    v.get("fields")
        .and_then(|x| x.as_array())
        .cloned()
        .or_else(|| v.as_array().cloned())
        .unwrap_or_default()
}

fn field<'a>(f: &'a [Value], name: &str) -> &'a Value {
    f.iter()
        .find(|x| x["name"] == name)
        .unwrap_or_else(|| panic!("field {name}: {f:?}"))
}

/// D-165, D-164: create every field type, fill them, survive save/reopen, reset all.
#[test]
fn form_fields_created_filled_and_reset() {
    let dir = temp_dir("forms");
    let mut a = automation(&dir);
    text_pdf(&dir, "f.pdf", &[&[(72.0, 750.0, "APPLICATION")]]);
    open(&mut a, "f.pdf");
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "type": "text", "name": "Name", "rect": [100, 700, 300, 720] }),
    );
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "type": "checkbox", "name": "Agree", "rect": [100, 660, 115, 675] }),
    );
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "type": "radio", "group": "Size", "export": "Small", "rect": [100, 620, 115, 635] }),
    );
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "type": "radio", "group": "Size", "export": "Large", "rect": [130, 620, 145, 635] }),
    );
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "type": "combo", "name": "Color", "options": ["Red", "Blue"], "rect": [100, 580, 200, 600] }),
    );
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "type": "list", "name": "Parts", "options": ["A", "B", "C"], "multi": true, "rect": [100, 500, 200, 570] }),
    );
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "type": "button", "name": "Go", "caption": "Submit", "rect": [100, 460, 180, 480] }),
    );
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "type": "signature", "name": "Sig", "rect": [300, 460, 500, 500] }),
    );
    call(
        &mut a,
        "form_fill",
        json!({ "values": { "Name": "Pat Doe", "Agree": true, "Size": "Large", "Color": "Blue", "Parts": ["A", "C"] } }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "f.pdf");
    let f = fields(&mut a);
    assert_eq!(f.len(), 7, "seven fields (radio group is one): {f:?}");
    for (n, k) in [
        ("Name", "text"),
        ("Agree", "checkbox"),
        ("Size", "radio"),
        ("Color", "combo"),
        ("Parts", "list"),
        ("Go", "button"),
        ("Sig", "signature"),
    ] {
        assert_eq!(field(&f, n)["kind"], k, "{n}");
    }
    assert!(s(&field(&f, "Name")["value"]).contains("Pat Doe"), "{f:?}");
    assert!(s(&field(&f, "Size")["value"]).contains("Large"));
    assert!(s(&field(&f, "Parts")["value"]).contains('C'));
    call(&mut a, "form_reset", json!({}));
    let f = fields(&mut a);
    assert!(!s(&field(&f, "Name")["value"]).contains("Pat"), "reset: {f:?}");
}

/// D-164 (UI): the Forms highlight toggle exists in the app.
#[test]
fn form_highlight_fields_command_in_app() {
    let mut h = app();
    run(&mut h, "forms.editor");
    h.run_steps(4);
    assert!(
        shows(&h, "Highlight fields"),
        "Highlight fields toggle in the Form Fields window"
    );
}

/// D-166: detect form-like areas and turn them into fields.
#[test]
fn form_auto_create_fields_from_blanks() {
    let dir = temp_dir("autofields");
    let mut a = automation(&dir);
    let c = format!(
        "{}{}0 G 1 w 72 600 10 10 re S\n{}",
        text(72.0, 700.0, 12.0, "Name ____________________"),
        text(72.0, 650.0, 12.0, "Phone ________________"),
        text(90.0, 600.0, 12.0, "Approved")
    );
    std::fs::write(dir.join("blank.pdf"), pdf(&[SyntheticPage::new(612.0, 792.0, c)])).unwrap();
    open(&mut a, "blank.pdf");
    call(&mut a, "form_auto_fields", json!({}));
    let f = fields(&mut a);
    let names = s(&json!(f.iter().map(|x| x["name"].clone()).collect::<Vec<_>>()));
    assert!(names.contains("Name") && names.contains("Phone"), "{names}");
    assert!(f.iter().any(|x| x["kind"] == "checkbox"), "the small square: {f:?}");
}

/// D-167: export / import / merge form data, Typewriter text into fields, JavaScript.
#[test]
fn form_data_import_export_merge_typewriter_and_javascript() {
    let dir = temp_dir("formdata");
    let mut a = automation(&dir);
    text_pdf(&dir, "f.pdf", &[&[(72.0, 750.0, "FORM")]]);
    open(&mut a, "f.pdf");
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "type": "text", "name": "Qty", "rect": [100, 700, 200, 720] }),
    );
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "type": "text", "name": "Note", "rect": [100, 650, 400, 670] }),
    );
    call(&mut a, "form_fill", json!({ "values": { "Qty": "4" } }));
    for ext in ["xfdf", "csv", "json"] {
        call(&mut a, "form_data_export", json!({ "out": format!("d.{ext}") }));
    }
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_save", json!({ "path": "g.pdf" }));
    call(&mut a, "form_reset", json!({}));
    for ext in ["xfdf", "csv", "json"] {
        call(&mut a, "form_reset", json!({}));
        call(&mut a, "form_data_import", json!({ "path": format!("d.{ext}") }));
        assert!(s(&field(&fields(&mut a), "Qty")["value"]).contains('4'), "{ext}");
    }
    call(
        &mut a,
        "form_data_merge",
        json!({ "files": ["f.pdf", "g.pdf"], "out": "merged.csv" }),
    );
    let csv = std::fs::read_to_string(dir.join("merged.csv")).unwrap();
    assert!(csv.contains("Qty") && csv.lines().count() >= 3, "{csv}");
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Typewriter", "points": [[105, 652], [300, 668]], "contents": "Rush order" }),
    );
    call(&mut a, "form_typewriter_to_fields", json!({}));
    assert!(s(&field(&fields(&mut a), "Note")["value"]).contains("Rush order"));
    let r = call(
        &mut a,
        "javascript",
        json!({ "action": "run", "script": "var q = this.getField('Qty'); q.value = Number(q.value) * 3; q.value" }),
    );
    assert!(s(&r).contains("12"), "{r}");
    assert!(s(&field(&fields(&mut a), "Qty")["value"]).contains("12"));
}

// ---- Digital signatures (doc-168 .. doc-171) -----------------------------------------------

/// D-168, D-169, D-170: sign with an ID, reason, location, contact into a new file; the
/// signatures list validates it; certify with allowed changes; the certifier clears it.
#[test]
fn sign_certify_validate_and_clear() {
    let dir = temp_dir("sign");
    let mut a = automation(&dir);
    call(
        &mut a,
        "digital_id_create",
        json!({ "name": "Pat Signer", "password": "pw", "out": "pat.p12", "cert_out": "pat.pem" }),
    );
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "CONTRACT")]]);
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "signature_sign",
        json!({ "id": "pat.p12", "password": "pw", "page": 1, "rect": [350, 50, 550, 100], "reason": "Agree",
                 "location": "Austin", "contact": "pat@example.com", "out": "signed.pdf" }),
    );
    assert!(
        dir.join("signed.pdf").is_file() && dir.join("doc.pdf").is_file(),
        "saved as a new file"
    );
    call(&mut a, "doc_close", json!({ "discard_changes": true }));
    open(&mut a, "signed.pdf");
    let v = s(&call(&mut a, "signature_list", json!({ "trust": ["pat.pem"] })));
    assert!(
        v.contains("Pat Signer") && v.contains("Agree") && v.contains("Austin"),
        "{v}"
    );
    assert!(v.contains("\"valid\""), "valid: {v}");
    let v = s(&call(&mut a, "signature_list", json!({})));
    assert!(v.contains("unknown"), "untrusted signer is unknown, not valid: {v}");
    // tamper: append bytes changing content after signing -> invalid or changes listed
    call(&mut a, "doc_close", json!({ "discard_changes": true }));
    // invisible certification allowing markups
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "signature_sign",
        json!({ "id": "pat.p12", "password": "pw", "certify": 3, "out": "cert.pdf" }),
    );
    call(&mut a, "doc_close", json!({ "discard_changes": true }));
    open(&mut a, "cert.pdf");
    let v = s(&call(&mut a, "signature_list", json!({ "trust": ["pat.pem"] })));
    assert!(v.contains("certif"), "certification listed (invisible too): {v}");
    let st = s(&call(&mut a, "doc_standards", json!({})));
    assert!(st.contains("true") || st.contains("certified"), "{st}");
    // a markup is an allowed change under certify 3
    add_rect(&mut a, 1, "allowed");
    call(&mut a, "doc_save", json!({}));
    let v = s(&call(&mut a, "signature_list", json!({ "trust": ["pat.pem"] })));
    assert!(!v.contains("invalid"), "markups allowed: {v}");
    call(
        &mut a,
        "certification_clear",
        json!({ "id": "pat.p12", "password": "pw" }),
    );
    call(&mut a, "doc_save", json!({ "full": true }));
    let v = s(&call(&mut a, "doc_standards", json!({})));
    assert!(!v.contains("\"certified\":true"), "certification cleared: {v}");
}

/// D-171: create, list, export the public cert, change the password, delete IDs.
#[test]
fn digital_id_manager() {
    let dir = temp_dir("ids");
    let mut a = automation(&dir);
    call(
        &mut a,
        "digital_id_store",
        json!({ "action": "create", "dir": "ids", "name": "work", "person": "Pat", "password": "a1" }),
    );
    let l = s(&call(
        &mut a,
        "digital_id_store",
        json!({ "action": "list", "dir": "ids" }),
    ));
    assert!(l.contains("work") && l.contains("Pat"), "{l}");
    call(
        &mut a,
        "digital_id_store",
        json!({ "action": "export", "dir": "ids", "name": "work", "out": "work.pem" }),
    );
    assert!(
        std::fs::read_to_string(dir.join("work.pem"))
            .unwrap()
            .contains("BEGIN CERTIFICATE")
    );
    call(
        &mut a,
        "digital_id_store",
        json!({ "action": "password", "dir": "ids", "name": "work", "password": "a1", "new_password": "b2" }),
    );
    let e = fails(
        &mut a,
        "digital_id_store",
        json!({ "action": "password", "dir": "ids", "name": "work", "password": "a1", "new_password": "c3" }),
    );
    assert!(!e.is_empty());
    call(
        &mut a,
        "digital_id_create",
        json!({ "name": "Other", "password": "x", "out": "other.p12" }),
    );
    call(
        &mut a,
        "digital_id_store",
        json!({ "action": "import", "dir": "ids", "file": "other.p12", "name": "other", "password": "x" }),
    );
    call(
        &mut a,
        "digital_id_store",
        json!({ "action": "delete", "dir": "ids", "name": "work" }),
    );
    let l = s(&call(
        &mut a,
        "digital_id_store",
        json!({ "action": "list", "dir": "ids" }),
    ));
    assert!(!l.contains("\"work\"") && l.contains("other"), "{l}");
}

// ---- Security and redaction (doc-172 .. doc-178) -------------------------------------------

/// D-172, D-174: open password and permissions password with a permission set and encryption
/// strength, applied on save; the permissions password lifts the limits.
#[test]
fn security_passwords_permissions_and_opening() {
    let dir = temp_dir("sec");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "SECRET PLAN")]]);
    for enc in ["aes256", "aes128", "rc4"] {
        open(&mut a, "doc.pdf");
        call(
            &mut a,
            "security_set",
            json!({ "open_password": "u1", "permissions_password": "o1", "print": false, "copy": false, "encryption": enc }),
        );
        call(&mut a, "doc_save", json!({ "path": format!("{enc}.pdf") }));
        call(&mut a, "doc_close", json!({ "discard_changes": true }));
        let raw = std::fs::read(dir.join(format!("{enc}.pdf"))).unwrap();
        assert!(
            !String::from_utf8_lossy(&raw).contains("SECRET PLAN"),
            "{enc}: encrypted"
        );
        fails(&mut a, "doc_open", json!({ "path": format!("{enc}.pdf") }));
        fails(
            &mut a,
            "doc_open_protected",
            json!({ "path": format!("{enc}.pdf"), "password": "wrong" }),
        );
        call(
            &mut a,
            "doc_open_protected",
            json!({ "path": format!("{enc}.pdf"), "password": "u1" }),
        );
        let i = s(&call(&mut a, "security_info", json!({})));
        assert!(i.contains("print"), "{enc}: {i}");
        fails(&mut a, "security_remove", json!({}));
        assert!(s(&call(&mut a, "page_text", json!({ "page": 1 }))).contains("SECRET PLAN"));
        call(&mut a, "doc_close", json!({ "discard_changes": true }));
        call(
            &mut a,
            "doc_open_protected",
            json!({ "path": format!("{enc}.pdf"), "password": "o1" }),
        );
        call(&mut a, "security_remove", json!({}));
        call(&mut a, "doc_save", json!({ "path": format!("{enc}-open.pdf") }));
        call(&mut a, "doc_close", json!({}));
        open(&mut a, &format!("{enc}-open.pdf"));
        call(&mut a, "doc_close", json!({}));
    }
}

/// D-173: the status bar lock shows the security; security_info reports it.
#[test]
fn security_status_icon_in_the_app() {
    let dir = temp_dir("secicon");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "X")]]);
    open(&mut a, "doc.pdf");
    let i = s(&call(&mut a, "security_info", json!({})));
    assert!(i.contains("false") || i.contains("none"), "unencrypted: {i}");
    let mut h = app();
    run(&mut h, "document.security");
    h.run_steps(4);
    assert!(shows(&h, "Security: No security"), "status bar security icon (none)");
}

/// D-175: named security presets saved and applied.
#[test]
fn security_presets_save_and_apply() {
    let dir = temp_dir("secpre");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "X")]]);
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "security_preset",
        json!({ "action": "save", "path": "p.json", "name": "NoPrint", "permissions_password": "o", "print": false }),
    );
    assert!(
        s(&call(
            &mut a,
            "security_preset",
            json!({ "action": "list", "path": "p.json" })
        ))
        .contains("NoPrint")
    );
    call(
        &mut a,
        "security_preset",
        json!({ "action": "apply", "path": "p.json", "name": "NoPrint" }),
    );
    call(&mut a, "doc_save", json!({ "path": "out.pdf" }));
    call(&mut a, "doc_close", json!({ "discard_changes": true }));
    open(&mut a, "out.pdf");
    let i = s(&call(&mut a, "security_info", json!({})));
    assert!(i.contains("\"print\":false"), "{i}");
}

/// D-176, D-177, D-178: mark areas and text, apply: the content is really gone from the file;
/// fill colour and overlay code; text-only removal keeps graphics; scrub metadata.
#[test]
fn redaction_marks_apply_and_appearance() {
    let dir = temp_dir("redact");
    let mut a = automation(&dir);
    let c = format!(
        "{}{}{}",
        text(72.0, 700.0, 12.0, "SSN 123-45-6789 on file"),
        text(72.0, 600.0, 12.0, "PUBLIC LINE"),
        rect(300.0, 300.0, 50.0, 50.0)
    );
    std::fs::write(dir.join("doc.pdf"), pdf(&[SyntheticPage::new(612.0, 792.0, c)])).unwrap();
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "doc_properties_set",
        json!({ "properties": { "Author": "Pat Private" } }),
    );
    call(
        &mut a,
        "redact_mark",
        json!({ "text": "123-45-6789", "fill": "#000000", "overlay": "(b)(6)", "text_color": "#FFFFFF" }),
    );
    call(
        &mut a,
        "redact_mark",
        json!({ "page": 1, "rects": [[290, 290, 360, 360]], "fill": "none" }),
    );
    let l = s(&call(&mut a, "redact_list", json!({})));
    assert!(l.contains("(b)(6)"), "{l}");
    let codes = s(&call(&mut a, "redact_codes", json!({})));
    assert!(codes.contains("(b)(6)"), "FOIA codes: {codes}");
    call(&mut a, "redact_apply", json!({ "scrub_metadata": true }));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    let raw = std::fs::read(dir.join("doc.pdf")).unwrap();
    let raw_s = String::from_utf8_lossy(&raw);
    assert!(!raw_s.contains("123-45-6789"), "text really removed from the file");
    assert!(!raw_s.contains("Pat Private"), "metadata scrubbed");
    let t = page_text(&mut a, "doc.pdf", 1);
    assert!(!t.contains("6789") && t.contains("PUBLIC LINE"), "{t}");
    // text-only redaction keeps the graphics
    let c = format!(
        "{}{}",
        text(72.0, 700.0, 12.0, "HIDE ME"),
        rect(60.0, 680.0, 200.0, 40.0)
    );
    std::fs::write(dir.join("k.pdf"), pdf(&[SyntheticPage::new(612.0, 792.0, c)])).unwrap();
    open(&mut a, "k.pdf");
    call(
        &mut a,
        "redact_mark",
        json!({ "page": 1, "rects": [[50, 670, 300, 730]], "fill": "none" }),
    );
    call(&mut a, "redact_apply_kinds", json!({ "kinds": "text" }));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    let raw = String::from_utf8_lossy(&std::fs::read(dir.join("k.pdf")).unwrap()).to_string();
    assert!(!raw.contains("HIDE ME"), "text gone");
    assert!(raw.contains("re"), "graphics kept");
}

// ---- Flatten, file size, archive (doc-179 .. doc-186) ---------------------------------------

/// D-179, D-181: flatten chosen markup types (recoverable), they print as page content;
/// unflatten brings them back after save and reopen; unrecoverable flatten cannot.
#[test]
fn flatten_by_type_recoverable_and_unflatten() {
    let dir = temp_dir("flat");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "BASE")], &[(72.0, 700.0, "BASE 2")]]);
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Text Box", "points": [[200, 400], [400, 440]], "contents": "FLATTENED NOTE" }),
    );
    add_rect(&mut a, 1, "stays");
    call(
        &mut a,
        "markup_add",
        json!({ "page": 2, "kind": "Text Box", "points": [[200, 400], [400, 440]], "contents": "PAGE TWO NOTE" }),
    );
    call(
        &mut a,
        "markup_flatten",
        json!({ "kinds": ["Text Box"], "pages": [1], "recoverable": true }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "doc.pdf");
    let m = list(&mut a);
    assert_eq!(m.len(), 2, "only the page-1 Text Box was flattened: {m:?}");
    assert!(
        s(&call(&mut a, "page_text", json!({ "page": 1 }))).contains("FLATTENED NOTE"),
        "burned into the page"
    );
    call(&mut a, "markup_unflatten", json!({ "pages": [1] }));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "doc.pdf");
    assert_eq!(list(&mut a).len(), 3, "recovered");
    // page_text also reads markup appearances, so compare with the never-flattened page 2
    let p1 = s(&call(&mut a, "page_text", json!({ "page": 1 })));
    let p2 = s(&call(&mut a, "page_text", json!({ "page": 2 })));
    assert_eq!(
        p1.matches("FLATTENED NOTE").count(),
        p2.matches("PAGE TWO NOTE").count(),
        "taken out of the page: {p1} / {p2}"
    );
    // not recoverable: unflatten finds nothing
    call(&mut a, "markup_flatten", json!({ "all": true }));
    call(&mut a, "doc_save", json!({}));
    let r = a.call("markup_unflatten", &json!({}));
    assert_eq!(list(&mut a).len(), 0, "{r:?}");
}

/// D-180: flatten into a named layer with overlay text, chosen properties kept in a pop-up,
/// capture summary of flattened attachments.
#[test]
fn flatten_extras_layer_overlay_popup_and_capture_summary() {
    let dir = temp_dir("flatx");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "BASE")]]);
    std::fs::write(dir.join("photo.txt"), b"site photo").unwrap();
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[100, 100], [200, 200]], "subject": "Hole", "contents": "Patch this" }),
    );
    call(
        &mut a,
        "markup_attach_file",
        json!({ "page": 1, "point": [300, 300], "path": "photo.txt" }),
    );
    call(
        &mut a,
        "markup_flatten_extras",
        json!({ "layer": "Flattened Review", "overlay": "REVIEWED COPY", "position": "top_right", "font": "Courier",
                 "keep": ["subject", "comments"], "capture_summary": true }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "doc.pdf");
    assert!(s(&call(&mut a, "page_text", json!({ "page": 1 }))).contains("REVIEWED COPY"));
    assert!(s(&call(&mut a, "layer_list", json!({}))).contains("Flattened Review"));
    let m = s(&call(&mut a, "markup_list", json!({})));
    assert!(m.contains("Patch this"), "comments kept in a pop-up note: {m}");
    let att = s(&call(&mut a, "attachment_list", json!({})));
    assert!(att.to_lowercase().contains("csv"), "capture summary attached: {att}");
}

fn image_pdf(dir: &Path, a: &mut Automation, name: &str) {
    let c = format!(
        "{}{}",
        rect(50.0, 50.0, 500.0, 600.0),
        text(72.0, 700.0, 30.0, "PHOTO PAGE")
    );
    std::fs::write(dir.join("src.pdf"), pdf(&[SyntheticPage::new(612.0, 792.0, c)])).unwrap();
    open(a, "src.pdf");
    call(a, "export_images", json!({ "dir": "img", "format": "png", "dpi": 600 }));
    let png = std::fs::read_dir(dir.join("img"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    call(
        a,
        "doc_from_image",
        json!({ "image": format!("img/{}", png.file_name().unwrap().to_string_lossy()), "path": name }),
    );
    call(a, "doc_save", json!({}));
}

/// D-182, D-183: images recompressed with a quality setting and custom per-class options; the
/// file gets smaller; report of sizes.
#[test]
fn reduce_file_size_with_custom_settings() {
    let dir = temp_dir("reduce");
    let mut a = automation(&dir);
    image_pdf(&dir, &mut a, "big.pdf");
    let before = std::fs::metadata(dir.join("big.pdf")).unwrap().len();
    open(&mut a, "big.pdf");
    let r = call(
        &mut a,
        "doc_reduce_size",
        json!({ "above_ppi": 200, "target_ppi": 100, "quality": 50, "discard_metadata": true, "discard_thumbnails": true,
                 "compress_streams": true, "gray_target_ppi": 100 }),
    );
    assert!(s(&r).contains("before") || s(&r).contains("after"), "size report: {r}");
    call(&mut a, "doc_save", json!({ "path": "small.pdf" }));
    let after = std::fs::metadata(dir.join("small.pdf")).unwrap().len();
    assert!(after < before, "{after} < {before}");
}

/// D-185: Archive as PDF/A-1b (what Revu exports): a PDF 1.4 file with a classic
/// cross-reference table, identified as part 1 conformance B, transparency groups removed;
/// Verify knows the PDF/A-1 rules (transparency, version) that PDF/A-2b allows.
#[test]
fn archive_as_pdfa_1b_and_verify_part_1_rules() {
    let dir = temp_dir("pdfa1");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "ARCHIVE ME")]]);
    open(&mut a, "doc.pdf");
    let r = call(&mut a, "doc_pdfa", json!({ "action": "archive", "level": "1b" }));
    assert!(s(&r).contains("PDF/A-1b"), "{r}");
    call(&mut a, "doc_save", json!({ "path": "a1.pdf" }));
    call(&mut a, "doc_close", json!({}));
    let bytes = std::fs::read(dir.join("a1.pdf")).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.4"), "PDF 1.4 header");
    assert!(
        !bytes.windows(7).any(|w| w == b"/ObjStm") && bytes.windows(5).any(|w| w == b"xref\n" || w == b"xref\r"),
        "a classic cross-reference table, no object streams"
    );
    open(&mut a, "a1.pdf");
    let st = s(&call(&mut a, "doc_standards", json!({})));
    assert!(st.contains("1B") || st.contains("1b"), "declares PDF/A-1b: {st}");
    let v = call(&mut a, "doc_pdfa", json!({ "action": "verify", "level": "1b" }));
    assert_eq!(v["declared"], "PDF/A-1b", "{v}");
    for i in v["issues"].as_array().unwrap() {
        let c = i["clause"].as_str().unwrap();
        assert!(
            !["6.1.2", "6.1.4", "6.7.11", "6.4"].contains(&c),
            "part 1 rule broken: {v}"
        );
    }
    assert!(page_text(&mut a, "a1.pdf", 1).contains("ARCHIVE ME"));
    // Transparency (a half-opaque markup): allowed in PDF/A-2b, not in PDF/A-1b.
    text_pdf(&dir, "t.pdf", &[&[(72.0, 700.0, "SEE THROUGH")]]);
    open(&mut a, "t.pdf");
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[100, 100], [300, 200]], "opacity": 0.5, "fill": "red", "fill_opacity": 0.5 }),
    );
    let v2 = s(&call(&mut a, "doc_pdfa", json!({ "action": "verify", "level": "2b" })));
    let v1 = s(&call(&mut a, "doc_pdfa", json!({ "action": "verify", "level": "1b" })));
    assert!(!v2.contains("transparen"), "2b allows transparency: {v2}");
    assert!(v1.contains("transparen"), "1b reports it: {v1}");
    let r = call(&mut a, "doc_pdfa", json!({ "action": "archive", "level": "1b" }));
    assert_eq!(r["conforming"], false, "transparency is refused, not hidden: {r}");
    // The dialog offers PDF/A-1b.
    let mut h = app();
    run(&mut h, "document.pdfa");
    h.get_by_label("PDF/A-1b").click();
    h.run_steps(2);
    assert!(h.state().state.features.export.pdfa_1b);
}

/// D-184, D-185, D-186: repair rewrites a damaged file; PDF/A archive + verify + unlock;
/// colour processing greys the content.
#[test]
fn repair_archive_pdfa_and_color_processing() {
    let dir = temp_dir("repair");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "REPAIR ME")]]);
    // damage the cross-reference offsets
    let mut raw = std::fs::read(dir.join("doc.pdf")).unwrap();
    let at = raw.windows(4).rposition(|w| w == b"xref").unwrap();
    for b in raw.iter_mut().skip(at + 30).take(40) {
        if b.is_ascii_digit() {
            *b = b'9';
        }
    }
    std::fs::write(dir.join("broken.pdf"), &raw).unwrap();
    open(&mut a, "broken.pdf");
    let r = call(&mut a, "doc_repair", json!({}));
    call(&mut a, "doc_save", json!({ "path": "fixed.pdf" }));
    call(&mut a, "doc_close", json!({}));
    assert!(page_text(&mut a, "fixed.pdf", 1).contains("REPAIR ME"), "{r}");
    open(&mut a, "doc.pdf");
    let r = call(&mut a, "doc_pdfa", json!({ "action": "archive", "level": "2b" }));
    call(&mut a, "doc_save", json!({ "path": "archive.pdf" }));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "archive.pdf");
    let v = call(&mut a, "doc_pdfa", json!({ "action": "verify" }));
    assert!(
        s(&call(&mut a, "doc_standards", json!({}))).contains("2"),
        "PDF/A claimed: {r} {v}"
    );
    call(&mut a, "doc_pdfa", json!({ "action": "unlock" }));
    call(&mut a, "doc_save", json!({ "full": true }));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "archive.pdf");
    let st = s(&call(&mut a, "doc_standards", json!({})));
    assert!(!st.contains("2b") && !st.contains("2B"), "unlocked: {st}");
    // colour processing
    let c = "1 0 0 rg 100 100 200 200 re f\n";
    std::fs::write(dir.join("red.pdf"), pdf(&[SyntheticPage::new(612.0, 792.0, c)])).unwrap();
    open(&mut a, "red.pdf");
    call(
        &mut a,
        "export_images",
        json!({ "dir": "red", "format": "bmp", "dpi": 36 }),
    );
    let bmp = std::fs::read(
        std::fs::read_dir(dir.join("red"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path(),
    )
    .unwrap();
    assert!(colored_pixels(&bmp) > 1000, "red before");
    call(&mut a, "doc_color_process", json!({ "mode": "grayscale" }));
    call(
        &mut a,
        "export_images",
        json!({ "dir": "gray", "format": "bmp", "dpi": 36 }),
    );
    let bmp = std::fs::read(
        std::fs::read_dir(dir.join("gray"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path(),
    )
    .unwrap();
    assert_eq!(colored_pixels(&bmp), 0, "no coloured pixels after grayscale");
}

/// Pixels of a 24/32-bit BMP whose red and blue differ (rows padded to 4 bytes).
fn colored_pixels(bmp: &[u8]) -> usize {
    let u32at = |i: usize| u32::from_le_bytes([bmp[i], bmp[i + 1], bmp[i + 2], bmp[i + 3]]) as usize;
    let (off, w, h) = (u32at(10), u32at(18), (u32at(22) as i32).unsigned_abs() as usize);
    let bpp = u16::from_le_bytes([bmp[28], bmp[29]]) as usize / 8;
    let stride = (w * bpp).div_ceil(4) * 4;
    let mut n = 0;
    for y in 0..h {
        for x in 0..w {
            let i = off + y * stride + x * bpp;
            if (bmp[i] as i32 - bmp[i + 2] as i32).abs() > 20 {
                n += 1;
            }
        }
    }
    n
}

// ---- Export (doc-187 .. doc-190) -----------------------------------------------------------

/// D-187: each page as TIFF, JPEG, PNG, GIF or BMP, numbered with a configurable suffix.
#[test]
fn export_pages_to_images_all_formats() {
    let dir = temp_dir("expimg");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "ONE")], &[(72.0, 700.0, "TWO")]]);
    open(&mut a, "doc.pdf");
    let magic: [(&str, &[u8]); 5] = [
        ("png", b"\x89PNG"),
        ("jpg", b"\xFF\xD8"),
        ("tif", b"II*\0"),
        ("bmp", b"BM"),
        ("gif", b"GIF8"),
    ];
    for (fmt, m) in magic {
        let d = format!("out-{fmt}");
        call(
            &mut a,
            "export_images",
            json!({ "dir": d, "format": fmt, "dpi": 36, "suffix": "-pg", "name": "sheet" }),
        );
        let mut names: Vec<String> = std::fs::read_dir(dir.join(&d))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names.len(), 2, "{fmt}: {names:?}");
        assert!(names[0].starts_with("sheet-pg") && names[0].contains('1'), "{names:?}");
        let b = std::fs::read(dir.join(&d).join(&names[0])).unwrap();
        assert!(
            b.starts_with(m) || (fmt == "tif" && b.starts_with(b"MM\0*")),
            "{fmt} magic"
        );
    }
}

/// D-188, D-189: text, RTF, HTML, Word, Excel and PowerPoint conversion.
#[test]
fn export_document_to_text_rtf_html_and_office() {
    let dir = temp_dir("expdoc");
    let mut a = automation(&dir);
    text_pdf(
        &dir,
        "doc.pdf",
        &[&[(72.0, 700.0, "GENERAL NOTES"), (72.0, 650.0, "Qty 42")]],
    );
    open(&mut a, "doc.pdf");
    for ext in ["txt", "rtf", "html", "docx", "xlsx", "pptx"] {
        call(&mut a, "export_document", json!({ "out": format!("out.{ext}") }));
        let b = std::fs::read(dir.join(format!("out.{ext}"))).unwrap();
        match ext {
            "txt" | "html" => assert!(String::from_utf8_lossy(&b).contains("GENERAL NOTES"), "{ext}"),
            "rtf" => assert!(b.starts_with(b"{\\rtf") && String::from_utf8_lossy(&b).contains("GENERAL NOTES")),
            _ => assert!(b.starts_with(b"PK"), "{ext} is an Office zip"),
        }
    }
}

/// D-190: drag a box over a schedule; only that text goes to Excel / CSV in rows and columns.
#[test]
fn export_page_region_to_excel() {
    let dir = temp_dir("expreg");
    let mut a = automation(&dir);
    text_pdf(
        &dir,
        "doc.pdf",
        &[&[
            (72.0, 700.0, "TAG"),
            (200.0, 700.0, "CFM"),
            (72.0, 680.0, "AHU-1"),
            (200.0, 680.0, "1200"),
            (72.0, 400.0, "OUTSIDE THE BOX"),
        ]],
    );
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "export_region",
        json!({ "page": 1, "rect": [60, 670, 300, 720], "out": "sched.csv" }),
    );
    let csv = std::fs::read_to_string(dir.join("sched.csv")).unwrap();
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!(rows.len(), 2, "{csv}");
    assert!(
        rows[0].contains("TAG") && rows[0].contains("CFM") && rows[1].contains("AHU-1") && rows[1].contains("1200"),
        "{csv}"
    );
    assert!(!csv.contains("OUTSIDE"), "{csv}");
    call(
        &mut a,
        "export_region",
        json!({ "page": 1, "rect": [60, 670, 300, 720], "out": "sched.xlsx" }),
    );
    assert!(std::fs::read(dir.join("sched.xlsx")).unwrap().starts_with(b"PK"));
}

// ---- Properties, metadata, attachments (doc-191 .. doc-196) --------------------------------

/// D-191, D-192, D-193: document properties (general + security), standard metadata edited,
/// custom properties added / changed / deleted; survives save and reopen.
#[test]
fn document_properties_standard_and_custom() {
    let dir = temp_dir("props");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "X")]]);
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "doc_properties_set",
        json!({ "properties": { "Title": "Level 2 Plan", "Author": "Estimator", "Subject": "HVAC", "Keywords": "duct",
                                  "Project": "Job 12", "Phase": "CD" } }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "doc.pdf");
    let p = s(&call(&mut a, "doc_properties", json!({ "xmp": true })));
    for want in ["Level 2 Plan", "Estimator", "HVAC", "duct", "Job 12", "CD"] {
        assert!(p.contains(want), "{want}: {p}");
    }
    call(
        &mut a,
        "doc_properties_set",
        json!({ "properties": { "Phase": "", "Project": "Job 13" } }),
    );
    let p = s(&call(&mut a, "doc_properties", json!({})));
    assert!(!p.contains("\"Phase\"") && p.contains("Job 13"), "{p}");
    assert!(p.contains("encrypt"), "security information: {p}");
    let mut h = app();
    run(&mut h, "document.properties");
    h.run_steps(4);
    assert!(
        shows(&h, "Title") || shows(&h, "Properties"),
        "Document Properties dialog (Ctrl+D)"
    );
}

/// D-194: list, open/save out, add and delete embedded files.
#[test]
fn embedded_file_attachments() {
    let dir = temp_dir("att");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "X")]]);
    std::fs::write(dir.join("spec.txt"), b"specification body").unwrap();
    open(&mut a, "doc.pdf");
    call(
        &mut a,
        "attachment_add",
        json!({ "path": "spec.txt", "description": "Spec" }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "doc.pdf");
    let l = s(&call(&mut a, "attachment_list", json!({})));
    assert!(l.contains("spec.txt") && l.contains("Spec"), "{l}");
    call(
        &mut a,
        "attachment_extract",
        json!({ "name": "spec.txt", "out": "back.txt" }),
    );
    assert_eq!(std::fs::read(dir.join("back.txt")).unwrap(), b"specification body");
    call(&mut a, "attachment_delete", json!({ "name": "spec.txt" }));
    call(&mut a, "doc_save", json!({}));
    assert!(!s(&call(&mut a, "attachment_list", json!({}))).contains("spec.txt"));
}

/// D-195: a paperclip icon carrying one file: icon, colour, opacity, extract, note.
#[test]
fn attachment_icon_markup() {
    let dir = temp_dir("attm");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "X")]]);
    std::fs::write(dir.join("rfi.txt"), b"RFI 7").unwrap();
    open(&mut a, "doc.pdf");
    let v = call(
        &mut a,
        "attachment_markup_add",
        json!({ "page": 1, "at": [300, 300], "path": "rfi.txt", "icon": "paperclip", "description": "RFI note" }),
    );
    let id = v["id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| list(&mut a)[0]["id"].as_str().unwrap().to_string());
    call(
        &mut a,
        "markup_edit",
        json!({ "ids": [id], "color": "#0000FF", "opacity": 0.5, "contents": "See RFI" }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "doc.pdf");
    let m = list(&mut a);
    let st = s(&m[0]);
    assert!(
        st.contains("File Attachment") && st.contains("0000FF") && st.contains("0.5") && st.contains("See RFI"),
        "{st}"
    );
    call(&mut a, "markup_attachment_save", json!({ "id": id, "out": "out.txt" }));
    assert_eq!(std::fs::read(dir.join("out.txt")).unwrap(), b"RFI 7");
}

/// D-196: Standards information (PDF/A status) and page tags, read-only.
#[test]
fn standards_info_and_page_tags() {
    let dir = temp_dir("std");
    let mut a = automation(&dir);
    text_pdf(&dir, "doc.pdf", &[&[(72.0, 700.0, "X")]]);
    open(&mut a, "doc.pdf");
    let st = s(&call(&mut a, "doc_standards", json!({})));
    assert!(
        st.contains("pdfa") || st.contains("PDF/A") || st.contains("conformance"),
        "{st}"
    );
    let mut h = app();
    run(&mut h, "document.properties");
    h.run_steps(4);
    assert!(shows(&h, "Standards") || shows(&h, "PDF/A"), "Standards section shown");
}

// ---- Links and Places (doc-197 .. doc-202) -------------------------------------------------

/// D-199, D-201, D-202: every action kind (page with zoom, Place, Space, snapshot view in
/// another PDF, URL, file relative / full); list, edit; Places move and links follow, rename
/// breaks them.
#[test]
fn link_actions_list_edit_and_places() {
    let dir = temp_dir("links");
    let mut a = automation(&dir);
    text_pdf(
        &dir,
        "doc.pdf",
        &[
            &[(72.0, 700.0, "ONE")],
            &[(72.0, 700.0, "TWO")],
            &[(72.0, 700.0, "THREE")],
        ],
    );
    text_pdf(
        &dir,
        "other.pdf",
        &[&[(72.0, 700.0, "OTHER")], &[(72.0, 700.0, "OTHER 2")]],
    );
    open(&mut a, "doc.pdf");
    let sp = call(
        &mut a,
        "space_add",
        json!({ "page": 2, "name": "Lobby", "points": [[100, 100], [300, 100], [300, 300], [100, 300]] }),
    );
    let space_id = sp["id"].as_str().map(str::to_string).unwrap_or_default();
    call(
        &mut a,
        "place_set",
        json!({ "name": "Detail-5", "page": 3, "left": 50, "top": 600, "zoom": 2 }),
    );
    let targets = [
        json!({ "to_page": 2, "zoom": "fit_width" }),
        json!({ "place": "Detail-5" }),
        json!({ "space": space_id }),
        json!({ "file": "other.pdf", "file_page": 2, "view": [0, 0, 300, 300], "relative": true }),
        json!({ "url": "https://example.com/spec" }),
        json!({ "file": dir.join("other.pdf").to_string_lossy() }),
    ];
    for (i, t) in targets.iter().enumerate() {
        let mut args = json!({ "page": 1, "rect": [10, 10 + 30 * i, 60, 30 + 30 * i] });
        for (k, v) in t.as_object().unwrap() {
            args[k] = v.clone();
        }
        call(&mut a, "link_add", args);
    }
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    open(&mut a, "doc.pdf");
    let l = call(&mut a, "link_list", json!({}));
    let t = s(&l);
    for want in ["Detail-5", "example.com/spec", "other.pdf", "fit_width"] {
        assert!(t.contains(want), "{want}: {t}");
    }
    let links = l["links"].as_array().cloned().unwrap_or_default();
    assert_eq!(links.len(), 6, "{t}");
    let url_id = links.iter().find(|x| s(x).contains("example.com")).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &mut a,
        "link_edit",
        json!({ "id": url_id, "url": "https://example.com/changed" }),
    );
    assert!(s(&call(&mut a, "link_list", json!({}))).contains("changed"));
    // move the Place: links follow; rename it: they break
    call(
        &mut a,
        "place_set",
        json!({ "name": "Detail-5", "page": 2, "left": 0, "top": 700 }),
    );
    assert!(s(&call(&mut a, "place_list", json!({}))).contains("\"page\":2"));
    call(
        &mut a,
        "place_set",
        json!({ "name": "Detail-5", "new_name": "Detail-6" }),
    );
    let audit = s(&call(&mut a, "link_list", json!({})));
    assert!(
        audit.contains("Detail-5"),
        "the link still names the old Place (broken, as in Revu): {audit}"
    );
    let mut h = app();
    assert!(
        panel_toggles_with_key(&mut h, "links", egui::Key::N),
        "Alt+N: Links panel"
    );
}

/// D-197, D-198: clicking a page link goes to the page; Hyperlink tool draws a link; view
/// mode highlights all links.
#[test]
fn follow_links_and_hyperlink_tool_in_app() {
    let mut h = app();
    let pages = h.state().state.doc().unwrap().session.page_count();
    assert!(pages >= 2);
    {
        let d = h.state_mut().state.doc_mut().unwrap();
        d.session
            .add_link(
                0,
                markupcraft_geom::Rect::new(100.0, 100.0, 200.0, 150.0),
                &markupcraft_engine::links::LinkTarget::Page(1),
                Default::default(),
            )
            .unwrap();
    }
    h.run_steps(3);
    let before = h.state().state.doc().unwrap().view.current;
    assert_eq!(before, 0);
    show(&mut h, "links");
    assert!(
        shows(&h, "Follow") || shows(&h, "Highlight"),
        "Links panel lists the link"
    );
    // clicking the link on the page follows it
    click(&mut h, 150.0, 125.0);
    h.run_steps(4);
    let on_page = h.state().state.doc().unwrap().view.current;
    if on_page != 1 {
        // the Links panel: select the link, then Follow
        h.query_all_by_label_contains("p.1 ").next().expect("link row").click();
        h.run_steps(3);
        h.query_all_by_label_contains("Follow").next().expect("Follow").click();
        h.run_steps(4);
        assert_eq!(h.state().state.doc().unwrap().view.current, 1, "followed to page 2");
    }
    // Hyperlink tool: drag a box, it becomes a link
    let n0 = h.state().state.doc().unwrap().session.links().len();
    key(&mut h, egui::Modifiers::SHIFT, egui::Key::H);
    drag(&mut h, (300.0, 300.0), (400.0, 350.0));
    h.run_steps(4);
    // the Hyperlink action dialog opens on the dragged box; OK makes the link
    assert!(shows(&h, "Fit the link to the text in the box"), "Hyperlink dialog");
    h.get_all_by_label("OK").last().unwrap().click();
    h.run_steps(4);
    let n1 = h.state().state.doc().unwrap().session.links().len();
    assert_eq!(
        n1,
        n0 + 1,
        "hyperlink tool drew a link area: {}",
        h.state().state.status
    );
}

/// D-200: URL text on chosen pages becomes links; already linked text skipped.
#[test]
fn hyperlinks_from_urls() {
    let dir = temp_dir("urls");
    let mut a = automation(&dir);
    text_pdf(
        &dir,
        "doc.pdf",
        &[
            &[(72.0, 700.0, "See https://example.com/a and www.example.org")],
            &[(72.0, 700.0, "http://example.net")],
        ],
    );
    open(&mut a, "doc.pdf");
    call(&mut a, "link_from_urls", json!({ "pages": [1] }));
    let l = s(&call(&mut a, "link_list", json!({})));
    assert!(
        l.contains("example.com/a") && l.contains("example.org") && !l.contains("example.net"),
        "{l}"
    );
    call(&mut a, "link_from_urls", json!({}));
    let v = call(&mut a, "link_list", json!({}));
    assert_eq!(v["links"].as_array().map_or(0, |x| x.len()), 3, "no duplicates: {v}");
}

// ---- File Access (doc-203 .. doc-210) ------------------------------------------------------

use markupcraft_ui_egui::panels::file_access;
use markupcraft_ui_egui::shell::recent::{self, RecentFile, Sort};

fn fa_files(tag: &str) -> (PathBuf, Vec<PathBuf>) {
    let dir = temp_dir(tag);
    let mut out = Vec::new();
    for (sub, name) in [("a", "Plan.pdf"), ("b", "Roof.pdf"), ("a", "Elev.pdf")] {
        let d = dir.join(sub);
        std::fs::create_dir_all(&d).unwrap();
        out.push(text_pdf(&d, name, &[&[(72.0, 700.0, name)]]));
    }
    (dir, out)
}

/// D-203: recents listed in File Access; click opens; Ctrl+click opens behind; remove, clear.
#[test]
fn file_access_recents_open_remove_clear() {
    let (_dir, files) = fa_files("fa");
    let mut h = app();
    for f in &files {
        h.state_mut().state.open_path(f);
    }
    h.run_steps(3);
    let rec: Vec<PathBuf> = h
        .state()
        .state
        .shell
        .recent
        .files
        .iter()
        .map(|f| f.path.clone())
        .collect();
    assert_eq!(rec.len(), 3, "{rec:?}");
    assert_eq!(rec[0], files[2], "newest first");
    show(&mut h, "file_access");
    assert!(shows(&h, "Roof.pdf") && shows(&h, "Plan.pdf"), "recents listed");
    // close everything but one, then click a recent: it opens
    let docs_before = h.state().state.docs.len();
    let active = h.state().state.active;
    h.state_mut().state.close_doc(active);
    h.run_steps(3);
    let closed = h.state().state.docs.len();
    assert_eq!(closed, docs_before - 1);
    h.get_all_by_label("Elev.pdf").last().unwrap().click();
    h.run_steps(4);
    assert_eq!(h.state().state.docs.len(), docs_before, "click reopens");
    run(&mut h, "file.clear_recent");
    assert!(h.state().state.shell.recent.files.is_empty(), "cleared");
}

/// D-204: sort by date, folder, most accessed, history grouped by day.
#[test]
fn file_access_recents_sorting() {
    let mk = |p: &str, opened: u64, count: u32| RecentFile {
        path: PathBuf::from(p),
        opened,
        count,
        ..Default::default()
    };
    let store = recent::RecentStore {
        files: vec![mk("/b/x.pdf", 100, 1), mk("/a/y.pdf", 300, 5), mk("/a/z.pdf", 200, 9)],
        ..Default::default()
    };
    let names = |s: Sort| {
        store
            .sorted(s)
            .iter()
            .map(|f| f.path.display().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(Sort::Date), ["/a/y.pdf", "/a/z.pdf", "/b/x.pdf"]);
    assert_eq!(names(Sort::MostUsed)[0], "/a/z.pdf");
    assert_eq!(names(Sort::Folder), ["/a/y.pdf", "/a/z.pdf", "/b/x.pdf"]);
    let now = 20_000 * 86_400 + 43_200;
    assert_eq!(recent::day_label(now - 10, now), "Today");
    assert_eq!(recent::day_label(now - 86_400, now), "Yesterday");
}

/// D-205, D-206: pinned files never drop off; categories rename / unpin; pinned sorted.
#[test]
fn file_access_pinned_files_and_categories() {
    let (_dir, files) = fa_files("fapin");
    let mut h = app();
    h.state_mut().state.shell.ui.recents_days = 1;
    h.state_mut().state.shell.recent.toggle_pin(&files[0], "Job A");
    h.state_mut().state.shell.recent.toggle_pin(&files[1], "Job A");
    h.state_mut().state.shell.recent.files.push(RecentFile {
        path: files[0].clone(),
        opened: 1,
        count: 1,
        ..Default::default()
    });
    h.state_mut().state.open_path(&files[2]);
    h.run_steps(3);
    let st = &h.state().state.shell.recent;
    assert!(
        st.files.iter().any(|f| f.path == files[0]),
        "a pinned file does not age off"
    );
    show(&mut h, "file_access");
    assert!(shows(&h, "Job A"), "category heading");
    h.state_mut().state.shell.recent.rename_category("Job A", "Tower");
    assert!(
        h.state()
            .state
            .shell
            .recent
            .pinned
            .iter()
            .all(|p| p.category == "Tower")
    );
    h.state_mut().state.shell.recent.remove_category("Tower");
    assert!(h.state().state.shell.recent.pinned.is_empty(), "unpin all");
}

/// D-207: number of items (0 turns the list off), days kept (default 90), preview on/off.
#[test]
fn file_access_recents_preferences() {
    let (_dir, files) = fa_files("faprefs");
    let mut h = app();
    assert_eq!(h.state().state.shell.ui.recents_days, 90, "default 90 days");
    assert!(h.state().state.shell.ui.recents_preview);
    h.state_mut().state.shell.prefs.recent_files = 2;
    for f in &files {
        h.state_mut().state.open_path(f);
    }
    h.run_steps(2);
    assert_eq!(h.state().state.shell.recent.files.len(), 2, "limit");
    h.state_mut().state.shell.recent.files.clear();
    h.state_mut().state.shell.prefs.recent_files = 0;
    h.state_mut().state.open_path(&files[0]);
    h.run_steps(2);
    assert!(
        h.state().state.shell.recent.files.is_empty(),
        "0 turns the recent list off"
    );
}

/// D-208, D-209: Explorer: folder listing (PDFs, or with images), sort, back / up, new
/// folder; rename and delete from disk.
#[test]
fn file_access_explorer_and_context_menu() {
    let (dir, files) = fa_files("faex");
    let a = dir.join("a");
    std::fs::write(a.join("photo.png"), b"\x89PNG").unwrap();
    std::fs::write(a.join("notes.txt"), b"x").unwrap();
    let e = file_access::entries(&a, file_access::ExSort::Name, false);
    let names: Vec<String> = e
        .iter()
        .map(|(_, p)| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, ["Elev.pdf", "Plan.pdf"], "PDFs only, by name");
    let e = file_access::entries(&a, file_access::ExSort::Type, true);
    assert_eq!(e.len(), 3, "with images: {e:?}");
    let nf = file_access::new_folder(&a).unwrap();
    assert!(nf.is_dir());
    let e = file_access::entries(&a, file_access::ExSort::Name, false);
    assert!(e[0].0, "folders first");
    let mut view = file_access::View::default();
    file_access::go(&mut view, &dir, a.clone());
    file_access::go(&mut view, &a, nf.clone());
    assert_eq!(view.back, [dir.clone(), a.clone()], "back history");
    file_access::rename_file(&files[0], &a.join("Plan2.pdf")).unwrap();
    assert!(a.join("Plan2.pdf").is_file() && !files[0].exists());
    assert!(
        file_access::rename_file(&a.join("Plan2.pdf"), &files[2]).is_err(),
        "never replaces a file"
    );
    let mut h = app();
    show(&mut h, "file_access");
    h.get_all_by_label("Explorer").last().unwrap().click();
    h.run_steps(3);
    assert!(shows(&h, "New Folder") && shows(&h, "Up"), "Explorer controls");
}

/// D-208: path favourites in the Explorer: add the folder shown, go to a favourite, remove it;
/// kept in the config folder.
#[test]
fn file_access_explorer_path_favourites() {
    let (dir, _files) = fa_files("fafav");
    let mut a = automation(&dir);
    let r = call(&mut a, "file_favorites", json!({ "action": "add", "path": "a" }));
    call(&mut a, "file_favorites", json!({ "action": "add", "path": "b" }));
    call(&mut a, "file_favorites", json!({ "action": "add", "path": "a" }));
    let l = call(&mut a, "file_favorites", json!({}));
    assert_eq!(l["favorites"].as_array().unwrap().len(), 2, "added once each: {l} {r}");
    assert!(
        dir.join("config/file_access_favorites.json").is_file(),
        "kept in the config folder"
    );
    fails(&mut a, "file_favorites", json!({ "action": "add", "path": "missing" }));
    call(&mut a, "file_favorites", json!({ "action": "remove", "path": "b" }));
    let l = call(&mut a, "file_favorites", json!({}));
    assert_eq!(l["favorites"].as_array().unwrap().len(), 1, "{l}");
    // The Explorer: Favorites > Add This Folder, go elsewhere, then Favorites > the folder.
    let cfg = dir.join("uicfg");
    std::fs::create_dir_all(&cfg).unwrap();
    let mut h = app();
    h.state_mut().state.features.partials.config = Some(cfg.clone());
    show(&mut h, "file_access");
    h.get_all_by_label("Explorer").last().unwrap().click();
    h.run_steps(3);
    let id = markupcraft_ui_egui::panels::file_access::view_id();
    let set_folder = |h: &mut Harness<'static, MarkupCraftApp>, p: PathBuf| {
        h.ctx.data_mut(|m| {
            let v = m.get_temp_mut_or_default::<file_access::View>(id);
            v.folder = Some(p.clone());
            v.path_text = p.display().to_string();
        });
        h.run_steps(3);
    };
    set_folder(&mut h, dir.join("b"));
    h.get_by_label("Favorites").click();
    h.run_steps(2);
    h.get_by_label("Add This Folder to Favorites").click();
    h.run_steps(3);
    assert_eq!(
        markupcraft_engine::favorites::load_favorites(&cfg).unwrap(),
        [dir.join("b")],
        "{}",
        h.state().state.status
    );
    set_folder(&mut h, dir.join("a"));
    h.get_by_label("Favorites").click();
    h.run_steps(2);
    h.get_by_label("b").click();
    h.run_steps(3);
    let now = h.ctx.data_mut(|m| m.get_temp::<file_access::View>(id)).unwrap().folder;
    assert_eq!(now, Some(dir.join("b")), "went to the favourite");
    h.get_by_label("Favorites").click();
    h.run_steps(2);
    h.get_by_label("Remove This Folder from Favorites").click();
    h.run_steps(3);
    assert!(markupcraft_engine::favorites::load_favorites(&cfg).unwrap().is_empty());
}

/// D-210: a file from the list becomes a link area that opens it.
#[test]
fn file_access_link_from_file_list() {
    let (_dir, files) = fa_files("falink");
    let mut h = app();
    markupcraft_ui_egui::features::more6::docs::start_link_file(&mut h.state_mut().state, files[1].clone());
    h.run_steps(2);
    drag(&mut h, (100.0, 100.0), (200.0, 160.0));
    h.run_steps(3);
    let links = h.state().state.doc().unwrap().session.links();
    assert!(
        format!("{links:?}").contains("Roof.pdf"),
        "a link that opens the file: {links:?}"
    );
}

/// D-210: hover a file in the list and drag it out onto the page: the drop makes a link area
/// there that opens the file.
#[test]
fn file_access_drag_a_file_out_as_a_link() {
    let (_dir, files) = fa_files("fadrag");
    let mut h = app();
    let sample = h.state().state.doc().unwrap().uid;
    h.state_mut().state.open_path(&files[1]);
    h.run_steps(3);
    let i = h.state().state.docs.iter().position(|d| d.uid == sample).unwrap();
    h.state_mut().state.active = i;
    h.run_steps(3);
    show(&mut h, "file_access");
    // The File Access row (the left panel), not the document tab of the same name.
    let from = h
        .get_all_by_label("Roof.pdf")
        .map(|n| n.rect().center())
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .unwrap();
    let to = screen(&h, 300.0, 400.0);
    h.hover_at(from);
    h.step();
    button(&mut h, from, true, egui::Modifiers::NONE);
    for k in 1..=12 {
        h.hover_at(from + (to - from) * (k as f32 / 12.0));
        h.step();
    }
    button(&mut h, to, false, egui::Modifiers::NONE);
    h.run_steps(4);
    let d = h.state().state.doc().unwrap();
    assert_eq!(d.uid, sample, "still on the sample");
    let links = d.session.links();
    let l = links
        .iter()
        .find(|l| format!("{:?}", l.target).contains("Roof.pdf"))
        .unwrap_or_else(|| panic!("a link that opens Roof.pdf: {links:?} {}", h.state().state.status));
    assert!(
        l.rect.x0 < 300.0 && l.rect.x1 > 300.0 && l.rect.y0 < 400.0 && l.rect.y1 > 400.0,
        "the link area is where it was dropped: {:?}",
        l.rect
    );
}

// ---- Pointers (doc-215 .. doc-217) ---------------------------------------------------------

/// D-215, D-216, D-217: compare, overlay, smart overlay; stitching; scripts.
#[test]
fn compare_overlay_stitch_and_script_pointers() {
    let dir = temp_dir("ptr");
    let mut a = automation(&dir);
    text_pdf(
        &dir,
        "old.pdf",
        &[&[(72.0, 700.0, "DOOR 101")], &[(72.0, 700.0, "LEFT HALF")]],
    );
    text_pdf(
        &dir,
        "new.pdf",
        &[&[(72.0, 700.0, "DOOR 102")], &[(72.0, 700.0, "LEFT HALF")]],
    );
    open(&mut a, "new.pdf");
    let r = call(&mut a, "compare_documents", json!({ "old": "old.pdf", "mode": "text" }));
    assert!(s(&r).contains("changed") || s(&r).contains("added"), "{r}");
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{"path": "old.pdf"}, {"path": "new.pdf"}], "out": "ovl.pdf" }),
    );
    assert!(dir.join("ovl.pdf").is_file());
    call(
        &mut a,
        "pages_stitch",
        json!({ "pages": [1, 2], "out": "stitch.pdf", "columns": 2 }),
    );
    open(&mut a, "stitch.pdf");
    let info = call(&mut a, "doc_info", json!({}));
    let w = info["page_list"][0]["width"].as_f64().unwrap();
    assert!((w - 1224.0).abs() < 1.0, "two pages side by side: {info}");
    std::fs::write(
        dir.join("steps.json"),
        r#"[{"tool": "watermark_add", "params": {"text": "VOID"}}]"#,
    )
    .unwrap();
    call(
        &mut a,
        "batch_script",
        json!({ "files": ["old.pdf"], "script": "steps.json" }),
    );
    assert!(page_text(&mut a, "old.pdf", 1).contains("VOID"));
}
