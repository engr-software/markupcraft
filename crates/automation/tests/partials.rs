//! End-to-end tests of the features that were finished from partial rows, through
//! `Automation::call`: pages from several PDFs, styled blank pages and templates, one file per
//! page, Page Setup's content placement, GIF and multi-page TIFF, Word / Excel / DXF to PDF,
//! one PDF per source, page and email templates, Quantity Link into an existing workbook, and
//! caption placement and styles.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use markupcraft_engine::finish::zip;
use markupcraft_engine::synthetic::{SyntheticPage, pdf, rect, text};
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-partials-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn auto(d: &Path) -> Automation {
    Automation::new()
        .with_root(d)
        .unwrap()
        .with_author("Tester")
        .with_config_dir(d.join("config"))
}

fn call(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args) {
        Ok(v) => v,
        Err(e) => panic!("{tool} {args}: {e}"),
    }
}

fn pages(d: &Path, name: &str, n: usize) {
    let list: Vec<SyntheticPage> = (0..n)
        .map(|i| SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, &format!("{name} page {}", i + 1))))
        .collect();
    std::fs::write(d.join(name), pdf(&list)).unwrap();
}

fn docx(text: &str) -> Vec<u8> {
    let doc = format!(
        "<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p><w:p><w:r><w:t>Second paragraph.</w:t></w:r></w:p></w:body></w:document>"
    );
    zip::write(&[zip::Entry {
        name: "word/document.xml".into(),
        data: doc.into_bytes(),
    }])
}

const DXF: &str = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n100\n21\n0\n0\nCIRCLE\n10\n50\n20\n50\n40\n20\n0\nTEXT\n10\n10\n20\n80\n40\n5\n1\nLEVEL 2 PLAN\n0\nENDSEC\n0\nEOF\n";

#[test]
fn insert_from_several_pdfs_grid_template_extract_each_and_page_setup() {
    let d = dir();
    pages(&d, "main.pdf", 2);
    pages(&d, "a.pdf", 3);
    pages(&d, "b.pdf", 2);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "main.pdf" }));
    let v = call(
        &mut a,
        "page_insert_files",
        json!({ "at": 2, "files": [{ "path": "a.pdf", "pages": "2-3" }, { "path": "b.pdf" }] }),
    );
    assert_eq!(v["report"]["pages_after"], 6);
    let t = call(&mut a, "page_text", json!({ "page": 2 }));
    assert!(t.to_string().contains("a.pdf page 2"), "{t}");
    let t = call(&mut a, "page_text", json!({ "page": 4 }));
    assert!(t.to_string().contains("b.pdf page 1"), "{t}");
    // one undo step takes every inserted page out again
    call(&mut a, "edit_undo", json!({}));
    assert_eq!(call(&mut a, "doc_info", json!({}))["pages"], 2);
    assert!(a.call("page_insert_files", &json!({ "at": 1, "files": [] })).is_err());

    let v = call(
        &mut a,
        "page_insert_blank_styled",
        json!({ "at": 3, "count": 2, "width": 792, "height": 612, "grid_spacing": 36 }),
    );
    assert_eq!(v["report"]["pages_after"], 4);
    let v = call(
        &mut a,
        "page_insert_blank_styled",
        json!({ "at": 1, "template": "b.pdf" }),
    );
    assert_eq!(v["report"]["pages_after"], 5);
    let t = call(&mut a, "page_text", json!({ "page": 1 }));
    assert!(t.to_string().contains("b.pdf page 1"));
    assert!(
        a.call("page_insert_blank_styled", &json!({ "at": 1, "grid_spacing": 1 }))
            .is_err()
    );

    std::fs::create_dir_all(d.join("each")).unwrap();
    call(
        &mut a,
        "page_label_set",
        json!({ "labels": { "1": "A-101", "2": "A-102" } }),
    );
    let v = call(
        &mut a,
        "page_extract_each",
        json!({ "pages": "1-2", "dir": "each", "stem": "sheet", "by_label": true }),
    );
    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "{v}");
    let v2 = call(
        &mut a,
        "page_extract_each",
        json!({ "pages": "1-2", "dir": "each", "stem": "sheet", "by_label": true }),
    );
    assert!(
        v2["files"][0].as_str().unwrap().contains("(2)"),
        "kept the first files: {v2}"
    );
    let v3 = call(
        &mut a,
        "page_extract_each",
        json!({ "pages": "1-2", "dir": "each", "stem": "sheet", "by_label": true, "overwrite": true }),
    );
    assert_eq!(v3["files"], v["files"]);

    let v = call(
        &mut a,
        "page_setup",
        json!({ "pages": [2], "width": 1224, "height": 792, "margins": [72, 18, 18, 18], "border": 2, "level_line": [[0, 0], [100, 100]] }),
    );
    assert!((v["rotation"].as_f64().unwrap() + 45.0).abs() < 1e-6);
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(info["page_list"][1]["width"], 1224.0);
    assert!(
        a.call(
            "page_setup",
            &json!({ "width": 100, "height": 100, "margins": [60, 60, 0, 0] })
        )
        .is_err()
    );
}

#[test]
fn gif_multi_page_tiff_office_dxf_and_one_pdf_per_source() {
    let d = dir();
    // a GIF made by our own encoder (export_images writes GIF too)
    pages(&d, "plan.pdf", 2);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "plan.pdf" }));
    std::fs::create_dir_all(d.join("img")).unwrap();
    let v = call(
        &mut a,
        "export_images",
        json!({ "dir": "img", "format": "gif", "dpi": 18 }),
    );
    let gifs = v["files"].as_array().unwrap();
    assert_eq!(gifs.len(), 2);
    let first = gifs[0].as_str().unwrap();
    assert!(std::fs::read(first).unwrap().starts_with(b"GIF89a"));
    std::fs::copy(first, d.join("scan.gif")).unwrap();
    let v = call(&mut a, "doc_from_image", json!({ "image": "scan.gif" }));
    assert_eq!(v["pages"], 1);

    // a three-page TIFF becomes three pages
    let mut tif = std::io::Cursor::new(Vec::new());
    {
        let mut enc = tiff::encoder::TiffEncoder::new(&mut tif).unwrap();
        for c in [[255u8, 0, 0], [0, 255, 0], [0, 0, 255]] {
            let px: Vec<u8> = (0..30 * 20).flat_map(|_| c).collect();
            enc.write_image::<tiff::encoder::colortype::RGB8>(30, 20, &px).unwrap();
        }
    }
    std::fs::write(d.join("set.tif"), tif.into_inner()).unwrap();
    let v = call(&mut a, "doc_from_image", json!({ "image": "set.tif" }));
    assert_eq!(v["pages"], 3);

    std::fs::write(d.join("spec.docx"), docx("Door Schedule Notes")).unwrap();
    std::fs::write(d.join("plan.dxf"), DXF).unwrap();
    let mut t = markupcraft_engine::convert::TextTable::default();
    t.rows.push(vec!["Item".into(), "Qty".into()]);
    t.rows.push(vec!["Doors".into(), "12".into()]);
    std::fs::write(
        d.join("bid.xlsx"),
        markupcraft_engine::finish::xlsx_edit::new_workbook(&[("Bid".into(), t)]),
    )
    .unwrap();
    let v = call(
        &mut a,
        "doc_create_from_files",
        json!({ "files": ["spec.docx", "bid.xlsx", "plan.dxf", "scan.gif"], "out": "made.pdf" }),
    );
    assert_eq!(v["pages"], 4, "{v}");
    call(&mut a, "doc_open", json!({ "path": "made.pdf" }));
    let words = call(&mut a, "page_text", json!({ "page": 1 })).to_string();
    assert!(words.contains("Door Schedule Notes"), "{words}");
    assert!(
        call(&mut a, "page_text", json!({ "page": 2 }))
            .to_string()
            .contains("Doors")
    );
    assert!(
        call(&mut a, "page_text", json!({ "page": 3 }))
            .to_string()
            .contains("LEVEL 2 PLAN")
    );
    std::fs::write(d.join("x.dwg"), b"AC1032").unwrap();
    let e = a
        .call(
            "doc_create_from_files",
            &json!({ "files": ["x.dwg"], "out": "dwg.pdf" }),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("DXF"), "{e}");

    std::fs::create_dir_all(d.join("out")).unwrap();
    let v = call(
        &mut a,
        "doc_create_each",
        json!({ "files": ["spec.docx", "plan.dxf", "plan.pdf"], "out_dir": "out" }),
    );
    assert_eq!(v["files"].as_array().unwrap().len(), 2);
    assert!(d.join("out/spec.pdf").is_file() && d.join("out/plan.pdf").is_file());
    let v = call(&mut a, "doc_create_each", json!({ "files": ["bid.xlsx"] }));
    assert!(v["files"][0].as_str().unwrap().ends_with("bid.pdf"));
}

#[test]
fn page_and_email_templates() {
    let d = dir();
    std::fs::write(
        d.join("tb.pdf"),
        pdf(&[SyntheticPage::new(1224.0, 792.0, rect(36.0, 36.0, 1152.0, 720.0))]),
    )
    .unwrap();
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "tb.pdf" }));
    let v = call(
        &mut a,
        "page_template",
        json!({ "action": "save", "dir": "tpl", "name": "Title Block 11x17" }),
    );
    assert_eq!(v["templates"], json!(["Title Block 11x17"]));
    let v = call(
        &mut a,
        "page_template",
        json!({ "action": "new", "dir": "tpl", "name": "title block 11x17", "path": "new.pdf" }),
    );
    assert_eq!(v["pages"], 1);
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(info["page_list"][0]["width"], 1224.0);
    assert!(
        a.call(
            "page_template",
            &json!({ "action": "save", "dir": "tpl", "name": "a/b" })
        )
        .is_err()
    );
    call(
        &mut a,
        "page_template",
        json!({ "action": "remove", "dir": "tpl", "name": "Title Block 11x17" }),
    );
    assert_eq!(
        call(&mut a, "page_template", json!({ "action": "list", "dir": "tpl" }))["templates"],
        json!([])
    );

    call(
        &mut a,
        "email_template",
        json!({ "action": "save", "path": "mail.json", "name": "RFI", "to": "pm@example.com", "subject": "RFI: {file}", "body": "Please see {file}." }),
    );
    let v = call(
        &mut a,
        "email_template",
        json!({ "action": "list", "path": "mail.json" }),
    );
    assert_eq!(v["templates"][0]["to"], "pm@example.com");
    call(&mut a, "doc_open", json!({ "path": "tb.pdf" }));
    call(
        &mut a,
        "email_template",
        json!({ "action": "draft", "path": "mail.json", "name": "RFI", "out": "rfi.eml" }),
    );
    let eml = String::from_utf8_lossy(&std::fs::read(d.join("rfi.eml")).unwrap()).into_owned();
    assert!(eml.contains("To: pm@example.com"));
    assert!(eml.contains("RFI: tb"));
    assert!(eml.contains("X-Unsent: 1"));
    call(
        &mut a,
        "email_template",
        json!({ "action": "delete", "path": "mail.json", "name": "RFI" }),
    );
}

#[test]
fn quantity_link_updates_an_existing_workbook_in_place() {
    let d = dir();
    pages(&d, "a.pdf", 1);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "a.pdf" }));
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Count", "points": [[10, 10], [20, 20], [30, 30]], "subject": "Door" }),
    );
    call(&mut a, "doc_save", json!({}));
    // the estimator's own workbook: a header, a formula row and another sheet
    let mut t = markupcraft_engine::convert::TextTable::default();
    t.rows.push(vec!["Item".into(), "Qty".into(), "Note".into()]);
    t.rows.push(vec!["Doors".into(), String::new(), "keep me".into()]);
    let mut other = markupcraft_engine::convert::TextTable::default();
    other.rows.push(vec!["Labour".into()]);
    std::fs::write(
        d.join("bid.xlsx"),
        markupcraft_engine::finish::xlsx_edit::new_workbook(&[("Bid".into(), t), ("Rates".into(), other)]),
    )
    .unwrap();
    call(
        &mut a,
        "quantity_link",
        json!({ "action": "save", "path": "links.json", "name": "Doors", "sheet": "Bid", "cell": "B2", "files": ["a.pdf"], "measure": "count", "subjects": ["Door"] }),
    );
    call(
        &mut a,
        "quantity_link",
        json!({ "action": "update", "path": "links.json", "out": "bid.xlsx" }),
    );
    let bytes = std::fs::read(d.join("bid.xlsx")).unwrap();
    let bid = markupcraft_engine::finish::xlsx_edit::read_sheet(&bytes, "Bid").unwrap();
    assert!(bid.contains(&(1, 1, "3".into())), "{bid:?}");
    assert!(bid.contains(&(1, 2, "keep me".into())));
    assert!(bid.contains(&(0, 0, "Item".into())));
    let rates = markupcraft_engine::finish::xlsx_edit::read_sheet(&bytes, "Rates").unwrap();
    assert_eq!(rates, vec![(0, 0, "Labour".into())]);
    assert!(markupcraft_engine::finish::xlsx_edit::read_sheet(&bytes, "Links").is_ok());
}

#[test]
fn caption_along_the_last_segment_with_strike_and_superscript() {
    let d = dir();
    pages(&d, "m.pdf", 1);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "m.pdf" }));
    let v = call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Perimeter", "points": [[100, 100], [300, 100], [300, 300], [100, 300]] }),
    );
    let id = v["id"].as_str().unwrap().to_string();
    call(
        &mut a,
        "measure_props_set",
        json!({ "ids": [id], "caption_last_segment": true, "caption_strike": true, "caption_script": "superscript", "caption_bold": true }),
    );
    assert!(
        a.call(
            "measure_props_set",
            &json!({ "ids": [id], "caption_script": "sideways" })
        )
        .is_err()
    );
    call(&mut a, "doc_save", json!({}));
    let raw = String::from_utf8_lossy(&std::fs::read(d.join("m.pdf")).unwrap()).into_owned();
    assert!(raw.contains("/PCCaptionLastSeg true"), "the placement is saved");
    assert!(
        raw.contains("line-through") && raw.contains("vertical-align:super"),
        "the styles are in /DS"
    );
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "m.pdf" }));
    let s = call(&mut a, "markup_list", json!({}));
    assert_eq!(s["markups"].as_array().unwrap().len(), 1, "{s}");
}

#[test]
fn batch_split_and_batch_script() {
    let d = dir();
    pages(&d, "a.pdf", 4);
    pages(&d, "b.pdf", 2);
    std::fs::create_dir_all(d.join("parts")).unwrap();
    let mut a = auto(&d);
    let v = call(
        &mut a,
        "batch_split",
        json!({ "files": ["a.pdf", "b.pdf"], "dir": "parts", "pages_per_file": 2 }),
    );
    assert_eq!(v["files"][0]["parts"], 2);
    assert_eq!(v["files"][1]["parts"], 1);
    assert_eq!(std::fs::read_dir(d.join("parts")).unwrap().count(), 3);
    std::fs::write(
        d.join("stamp.json"),
        r#"[{"tool": "watermark_add", "params": {"text": "PRELIMINARY"}}, {"tool": "page_rotate", "params": {"degrees": 90}}]"#,
    )
    .unwrap();
    let v = call(
        &mut a,
        "batch_script",
        json!({ "files": ["a.pdf", "b.pdf"], "script": "stamp.json" }),
    );
    assert_eq!(v["done"], 2, "{v}");
    call(&mut a, "doc_open", json!({ "path": "b.pdf" }));
    let t = call(&mut a, "page_text", json!({ "page": 1 }));
    assert!(t.to_string().contains("PRELIMINARY"), "{t}");
    std::fs::write(d.join("bad.json"), r#"[{"tool": "doc_close"}]"#).unwrap();
    assert!(
        a.call("batch_script", &json!({ "files": ["a.pdf"], "script": "bad.json" }))
            .is_err()
    );
}

#[test]
fn toolset_legend_and_review_columns_capture_legend_3d_view() {
    let d = dir();
    pages(&d, "l.pdf", 1);
    std::fs::write(
        d.join("doors.mctools"),
        r#"{"format": "markupcraft-toolset", "version": 1, "set": {"id": "s1", "title": "Doors and Windows", "items": [
            {"id": "t1", "name": "Single Door", "tool": "count", "markup": {"subject": "Door"}},
            {"id": "t2", "name": "Window", "tool": "count", "markup": {"subject": ""}}]}}"#,
    )
    .unwrap();
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "l.pdf" }));
    let v = call(
        &mut a,
        "legend_from_toolset",
        json!({ "path": "doors.mctools", "page": 1, "at": [400, 700] }),
    );
    assert_eq!(v["subjects"], json!(["Door", "Window"]));
    let l = call(&mut a, "legend_list", json!({}));
    let rows = l["legends"][0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "every subject listed with none drawn: {l}");
    // a door drawn later is picked up on update
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Count", "points": [[10, 10], [30, 30]], "subject": "Door" }),
    );
    call(&mut a, "legend_update", json!({}));
    let l = call(&mut a, "legend_list", json!({}));
    let door = l["legends"][0]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["subject"] == "Door")
        .cloned()
        .unwrap();
    assert_eq!(door["count"], 2, "{door}");
    assert!(
        a.call(
            "legend_from_toolset",
            &json!({ "path": "nope.mctools", "page": 1, "at": [0, 0] })
        )
        .is_err()
    );

    // the review columns: Capture (an attachment's file), Legend, 3D View
    let mut doc = markupcraft_model::Document::default();
    let mut att = markupcraft_model::Markup::new(
        markupcraft_model::Kind::Attachment,
        0,
        vec![markupcraft_model::Point::new(1.0, 1.0)],
    );
    att.attachment_name = "site photo.jpg".into();
    let mut leg = markupcraft_model::Markup::new(
        markupcraft_model::Kind::Text,
        0,
        vec![markupcraft_model::Point::new(1.0, 1.0)],
    );
    leg.subject = "Legend".into();
    doc.markups = vec![att, leg];
    let t = markupcraft_model::table::MarkupTable::new(&doc);
    assert_eq!(t.cell_by_id(0, "capture").text, "site photo.jpg");
    assert_eq!(t.cell_by_id(1, "capture").text, "");
    assert!(!t.cell_by_id(1, "legend").text.is_empty());
    assert!(t.cell_by_id(0, "legend").text.is_empty() || t.cell_by_id(0, "legend").num == Some(0.0));
    assert_eq!(t.cell_by_id(0, "view3d").text, "");
    let ids: Vec<String> = markupcraft_model::columns::standard_columns()
        .into_iter()
        .map(|c| c.id)
        .collect();
    for c in ["status", "checkmark", "lock", "capture", "legend", "view3d"] {
        assert!(ids.iter().any(|i| i == c), "{c}");
    }
}

#[test]
fn count_status_report_and_viewport_calibration_with_separate_y() {
    let d = dir();
    pages(&d, "s.pdf", 1);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "s.pdf" }));
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Count", "points": [[10, 10], [20, 20], [30, 30]], "subject": "Door", "status": "Installed" }),
    );
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Count", "points": [[40, 40]], "subject": "Door" }),
    );
    let v = call(
        &mut a,
        "count_status_report",
        json!({ "out": "status.pdf", "csv": "status.csv" }),
    );
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{v}");
    let installed = rows.iter().find(|r| r["status"] == "Installed").unwrap();
    assert_eq!(installed["items"], 3);
    assert!(
        std::fs::read_to_string(d.join("status.csv"))
            .unwrap()
            .contains("Door,Installed,3,1")
    );
    call(&mut a, "doc_open", json!({ "path": "status.pdf" }));
    let l = call(&mut a, "markup_list", json!({}));
    let colored = l["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["status"] == "Installed")
        .unwrap()["color"]
        .clone();
    assert_eq!(
        colored, installed["color"],
        "the visual report draws each status in its colour"
    );

    call(&mut a, "doc_open", json!({ "path": "s.pdf" }));
    call(
        &mut a,
        "viewport_add",
        json!({ "page": 1, "box": [300, 300, 600, 600], "name": "Detail", "scale": { "kind": "architectural", "paper_inches": 0.25, "real_feet": 1 } }),
    );
    // 72 points known to be 10 feet
    let v = call(
        &mut a,
        "viewport_calibrate",
        json!({ "page": 1, "index": 1, "from": [310, 310], "to": [382, 310], "length": 10, "unit": "ft" }),
    );
    assert!((v["x"].as_f64().unwrap() - 10.0 / 72.0).abs() < 1e-6, "{v}");
    let v = call(
        &mut a,
        "viewport_calibrate",
        json!({ "page": 1, "index": 1, "y_scale": { "kind": "architectural", "paper_inches": 0.5, "real_feet": 1 } }),
    );
    assert!(
        (v["y"].as_f64().unwrap() - v["x"].as_f64().unwrap()).abs() > 1e-6,
        "separate Y: {v}"
    );
    assert!(a.call("viewport_calibrate", &json!({ "page": 1, "index": 5 })).is_err());
}

#[test]
fn raster_dynamic_fill_redaction_kinds_snapshot_cut_and_image_markups() {
    use markupcraft_engine::synthetic::rect as frame;
    let d = dir();
    // a thick-walled room drawn as filled strips, and words inside and outside
    let walls = format!(
        "{}{}{}{}",
        frame(100.0, 100.0, 300.0, 4.0),
        frame(100.0, 296.0, 300.0, 4.0),
        frame(100.0, 100.0, 4.0, 200.0),
        frame(396.0, 100.0, 4.0, 200.0)
    );
    let content = format!(
        "{walls}{}{}",
        text(150.0, 200.0, 14.0, "SECRET ROOM"),
        text(150.0, 600.0, 14.0, "FLOOR PLAN")
    );
    std::fs::write(d.join("r.pdf"), pdf(&[SyntheticPage::new(612.0, 792.0, content)])).unwrap();
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "r.pdf" }));
    let v = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [250, 150], "detect": "raster", "dpi": 72, "preview": true }),
    );
    let area = v["area_pt2"].as_f64().unwrap();
    assert!(area > 40_000.0 && area < 60_000.0, "{v}");

    call(
        &mut a,
        "redact_mark",
        json!({ "page": 1, "rects": [[140, 190, 320, 220]] }),
    );
    let v = call(&mut a, "redact_apply_kinds", json!({ "kinds": "images" }));
    assert_eq!(v["glyphs"], 0, "images only keeps the text: {v}");
    assert!(
        call(&mut a, "page_text", json!({ "page": 1 }))
            .to_string()
            .contains("SECRET")
    );
    call(
        &mut a,
        "redact_mark",
        json!({ "page": 1, "rects": [[140, 190, 320, 220]] }),
    );
    let v = call(&mut a, "redact_apply_kinds", json!({ "kinds": "text" }));
    assert!(v["glyphs"].as_u64().unwrap() > 0, "{v}");
    let t = call(&mut a, "page_text", json!({ "page": 1 })).to_string();
    assert!(!t.contains("SECRET") && t.contains("FLOOR PLAN"), "{t}");
    assert!(a.call("redact_apply_kinds", &json!({ "kinds": "everything" })).is_err());

    call(
        &mut a,
        "snapshot_cut",
        json!({ "page": 1, "rect": [140, 590, 320, 620] }),
    );
    assert!(
        !call(&mut a, "page_text", json!({ "page": 1 }))
            .to_string()
            .contains("FLOOR")
    );
    let v = call(&mut a, "markup_paste", json!({ "page": 1, "at": [300, 700] }));
    let pasted = v["ids"][0].clone();
    let l = call(&mut a, "markup_list", json!({}));
    assert!(
        l["markups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == pasted && m["kind"] == "Snapshot"),
        "{l}"
    );

    // a picture placed with Markup > Image is an Image markup, resizable like any box
    let img = image::RgbaImage::from_pixel(8, 4, image::Rgba([10, 120, 200, 255]));
    img.save(d.join("logo.png")).unwrap();
    let v = call(
        &mut a,
        "stamp_add",
        json!({ "page": 1, "rect": [400, 400, 480, 440], "image": "logo.png" }),
    );
    let id = v["id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| v["markup"]["id"].as_str().unwrap().to_string());
    let l = call(&mut a, "markup_list", json!({}));
    let m = l["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id.as_str())
        .unwrap()
        .clone();
    assert_eq!(m["subject"], "Image", "{m}");
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": [id], "resize": [400, 400, 560, 480] }),
    );
    let l = call(&mut a, "markup_list", json!({}));
    let m = l["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id.as_str())
        .unwrap()
        .clone();
    let r = m["rect"].as_array().unwrap();
    assert!(
        (r[2].as_f64().unwrap() - r[0].as_f64().unwrap() - 160.0).abs() < 1.0,
        "{m}"
    );
}

#[test]
fn digital_id_manager_and_clearing_a_certification() {
    let d = dir();
    pages(&d, "c.pdf", 1);
    let mut a = auto(&d);
    let v = call(
        &mut a,
        "digital_id_store",
        json!({ "action": "create", "dir": "ids", "name": "Office", "person": "Pat Certifier", "password": "pw" }),
    );
    assert_eq!(v["ids"][0]["subject"], "Pat Certifier");
    call(
        &mut a,
        "digital_id_store",
        json!({ "action": "password", "dir": "ids", "name": "Office", "password": "pw", "new_password": "pw2" }),
    );
    call(
        &mut a,
        "digital_id_store",
        json!({ "action": "export", "dir": "ids", "name": "Office", "out": "office.pem" }),
    );
    assert!(
        std::fs::read_to_string(d.join("office.pem"))
            .unwrap()
            .contains("CERTIFICATE")
    );
    let v = call(
        &mut a,
        "digital_id_store",
        json!({ "action": "import", "dir": "ids2", "file": "ids/Office.p12", "name": "Copy", "password": "pw2" }),
    );
    assert_eq!(v["ids"].as_array().unwrap().len(), 1);
    call(
        &mut a,
        "digital_id_create",
        json!({ "name": "Someone Else", "password": "x", "out": "other.p12" }),
    );

    call(&mut a, "doc_open", json!({ "path": "c.pdf" }));
    call(
        &mut a,
        "signature_sign",
        json!({ "id": "ids/Office.p12", "password": "pw2", "certify": 1 }),
    );
    let l = call(&mut a, "signature_list", json!({}));
    assert!(l.to_string().contains("\"certify\":1"), "{l}");
    let e = a
        .call("certification_clear", &json!({ "id": "other.p12", "password": "x" }))
        .unwrap_err()
        .to_string();
    assert!(e.contains("only the certifier"), "{e}");
    call(
        &mut a,
        "certification_clear",
        json!({ "id": "ids/Office.p12", "password": "pw2" }),
    );
    call(&mut a, "doc_save", json!({}));
    let l = call(&mut a, "signature_list", json!({}));
    assert!(!l.to_string().contains("\"certify\":1"), "{l}");
    call(
        &mut a,
        "digital_id_store",
        json!({ "action": "delete", "dir": "ids", "name": "Office" }),
    );
    let v = call(&mut a, "digital_id_store", json!({ "action": "list", "dir": "ids" }));
    assert_eq!(v["ids"], json!([]));
}

#[test]
fn form_field_properties_actions_and_xfa_layout() {
    let d = dir();
    pages(&d, "f.pdf", 2);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "f.pdf" }));
    let v = call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "rect": [100, 700, 300, 720], "type": "text", "name": "Owner" }),
    );
    assert!(v.to_string().contains("Owner"), "{v}");
    call(
        &mut a,
        "form_field_props",
        json!({ "name": "Owner", "new_name": "Client", "tooltip": "Who owns it", "required": true, "max_len": 30 }),
    );
    let l = call(&mut a, "form_list", json!({}));
    assert!(
        l.to_string().contains("Client") && l.to_string().contains("\"required\":true"),
        "{l}"
    );
    let v = call(
        &mut a,
        "form_field_action",
        json!({ "name": "Client", "trigger": "mouse_up", "action": "goto", "value": "2" }),
    );
    assert_eq!(v["actions"][0]["trigger"], "mouse_up");
    assert!(v["actions"][0]["action"].as_str().unwrap().contains("2"), "{v}");
    let v = call(
        &mut a,
        "form_field_action",
        json!({ "name": "Client", "trigger": "mouse_up", "action": "none" }),
    );
    assert_eq!(v["actions"], json!([]));
    assert!(
        a.call("form_field_action", &json!({ "name": "Client", "trigger": "wink" }))
            .is_err()
    );
    assert!(a.call("form_xfa_layout", &json!({})).is_err(), "not an XFA form");

    std::fs::write(
        d.join("x.pdf"),
        pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::template(2)),
    )
    .unwrap();
    call(&mut a, "doc_open", json!({ "path": "x.pdf" }));
    let v = call(&mut a, "form_xfa_layout", json!({}));
    assert!(v["fields"].as_u64().unwrap() >= 1, "{v}");
    let l = call(&mut a, "form_list", json!({}));
    let first = l["fields"][0]["name"].as_str().unwrap().to_string();
    call(&mut a, "form_fill", json!({ "values": { first: "typed" } }));
}

#[test]
fn flatten_extras_overlay_popups_and_capture_summary() {
    let d = dir();
    pages(&d, "fl.pdf", 1);
    std::fs::write(d.join("photo.txt"), b"site").unwrap();
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "fl.pdf" }));
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Cloud", "points": [[100, 100], [200, 100], [200, 200], [100, 200]], "subject": "RFI 7", "contents": "Confirm the header" }),
    );
    call(
        &mut a,
        "attachment_markup_add",
        json!({ "page": 1, "at": [300, 300], "path": "photo.txt" }),
    );
    let v = call(
        &mut a,
        "markup_flatten_extras",
        json!({ "overlay": "ISSUED FOR CONSTRUCTION", "font": "Times-Roman", "position": "bottom_left", "keep": ["subject", "comments"], "capture_summary": true, "recoverable": true }),
    );
    assert_eq!(v["flattened"], 2, "{v}");
    assert!(
        call(&mut a, "page_text", json!({ "page": 1 }))
            .to_string()
            .contains("ISSUED FOR CONSTRUCTION")
    );
    let l = call(&mut a, "markup_list", json!({}));
    assert!(l.to_string().contains("Subject: RFI 7"), "{l}");
    let att = call(&mut a, "attachment_list", json!({}));
    assert!(att.to_string().contains("Capture Summary.csv"), "{att}");
    assert!(
        a.call("markup_flatten_extras", &json!({ "position": "middle" }))
            .is_err()
    );
}

#[test]
fn summary_pdf_extras_and_extract_with_links() {
    let d = dir();
    pages(&d, "s.pdf", 3);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "s.pdf" }));
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Cloud", "points": [[100, 100], [200, 100], [200, 200], [100, 200]], "subject": "RFI", "status": "Accepted" }),
    );
    let v = call(
        &mut a,
        "summary_export",
        json!({ "out": "sum.pdf", "pdf_status_history": true, "pdf_thumbnails": 96, "pdf_page_content": true }),
    );
    assert_eq!(v["markups"], 1, "{v}");
    call(&mut a, "doc_open", json!({ "path": "sum.pdf" }));
    let info = call(&mut a, "doc_info", json!({}));
    assert!(
        info["pages"].as_u64().unwrap() >= 6,
        "summary, history, sheet, index, 3 pages: {info}"
    );
    call(&mut a, "doc_open", json!({ "path": "s.pdf" }));
    call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [300, 300, 400, 350], "to_page": 3 }),
    );
    std::fs::create_dir_all(d.join("x")).unwrap();
    let v = call(
        &mut a,
        "page_extract_each",
        json!({ "pages": "1-3", "dir": "x", "stem": "s", "update_links": true }),
    );
    let first = v["files"][0].as_str().unwrap().to_string();
    let third = std::path::Path::new(v["files"][2].as_str().unwrap())
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    call(&mut a, "doc_open", json!({ "path": first }));
    let l = call(&mut a, "link_list", json!({}));
    assert!(l.to_string().contains(&third), "{l}");
}
