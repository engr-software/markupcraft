//! End-to-end tests of security presets and status, header/footer templates kept with the
//! document, fitting content, form data exchange and automatic fields, combining with options,
//! creating PDFs from files, layered PDFs, the redaction look and metadata scrub, Batch Link term
//! sources, legend options and distribution, and the Reduce File Size options, through
//! `Automation::call`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use markupcraft_engine::synthetic::{SyntheticPage, pdf, rect, text};
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-docs5b-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn auto(d: &Path) -> Automation {
    Automation::new().with_root(d).unwrap().with_author("Tester")
}

fn call(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args) {
        Ok(v) => v,
        Err(e) => panic!("{tool} {args}: {e}"),
    }
}

fn one(d: &Path, name: &str, content: String) {
    std::fs::write(d.join(name), pdf(&[SyntheticPage::new(612.0, 792.0, content)])).unwrap();
}

#[test]
fn security_presets_status_header_footer_templates_and_fit() {
    let d = dir();
    one(&d, "a.pdf", rect(0.0, 0.0, 612.0, 792.0));
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "a.pdf" }));
    let v = call(&mut a, "security_info", json!({}));
    assert_eq!(v["security"]["status"], "No security");
    let v = call(
        &mut a,
        "security_preset",
        json!({ "action": "save", "path": "sec.json", "name": "No printing", "permissions_password": "owner", "print": false }),
    );
    assert_eq!(v["presets"], json!(["No printing"]));
    let v = call(
        &mut a,
        "security_preset",
        json!({ "action": "list", "path": "sec.json" }),
    );
    assert_eq!(v["presets"][0]["print"], false);
    assert_eq!(v["presets"][0]["permissions_password"], true);
    let v = call(
        &mut a,
        "security_preset",
        json!({ "action": "apply", "path": "sec.json", "name": "No printing" }),
    );
    assert_eq!(v["status"], "Printing or editing limited");
    assert!(
        a.call(
            "security_preset",
            &json!({ "action": "apply", "path": "sec.json", "name": "Nope" })
        )
        .is_err()
    );
    call(&mut a, "security_remove", json!({}));

    let v = call(
        &mut a,
        "header_footer_template",
        json!({ "action": "save", "path": "hf.json", "name": "Sheets", "header_center": "SHEET <<1>>", "fit_content": true }),
    );
    assert_eq!(v["templates"], json!(["Sheets"]));
    let v = call(
        &mut a,
        "header_footer_template",
        json!({ "action": "list", "path": "hf.json" }),
    );
    assert_eq!(v["templates"][0]["header_center"], "SHEET <<1>>");
    call(
        &mut a,
        "header_footer_template",
        json!({ "action": "apply", "path": "hf.json", "name": "Sheets" }),
    );
    let v = call(&mut a, "header_footer_kept", json!({ "action": "get" }));
    assert_eq!(v["kept"]["header_center"], "SHEET <<1>>");
    let t = call(&mut a, "page_text", json!({ "page": 1 }));
    assert!(t.to_string().contains("SHEET 1"), "{t}");
    call(
        &mut a,
        "header_footer_kept",
        json!({ "action": "set", "footer_right": "Rev <<1>>" }),
    );
    let v = call(&mut a, "header_footer_kept", json!({ "action": "update" }));
    assert_eq!(v["kept"]["footer_right"], "Rev <<1>>");
    let v = call(&mut a, "page_fit_content", json!({ "margins": [36, 36, 36, 36] }));
    assert_eq!(v["pages"], 1);
    assert!(a.call("page_fit_content", &json!({ "margins": [1, 2] })).is_err());
}

#[test]
fn form_data_auto_fields_typewriter_and_merge() {
    let d = dir();
    one(&d, "f.pdf", text(72.0, 700.0, 12.0, "Name: ____________"));
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "f.pdf" }));
    let v = call(&mut a, "form_auto_fields", json!({}));
    assert_eq!(v["fields"], json!(["Name"]));
    call(&mut a, "form_fill", json!({ "values": { "Name": "Ada" } }));
    let v = call(&mut a, "form_data_export", json!({ "out": "data.json" }));
    assert_eq!(v["fields"], 1);
    call(&mut a, "form_fill", json!({ "values": { "Name": "" } }));
    let v = call(&mut a, "form_data_import", json!({ "path": "data.json" }));
    assert_eq!(v["values"]["Name"], "Ada");
    call(&mut a, "doc_save", json!({}));
    let v = call(
        &mut a,
        "form_data_merge",
        json!({ "files": ["f.pdf", "f.pdf"], "out": "merged.csv" }),
    );
    assert_eq!(v["rows"], 2);
    assert!(std::fs::read_to_string(d.join("merged.csv")).unwrap().contains("Ada"));
    let fields = call(&mut a, "form_list", json!({}));
    let r = fields["fields"][0]["rect"].clone();
    let (x0, y0, x1, y1) = (
        r[0].as_f64().unwrap(),
        r[1].as_f64().unwrap(),
        r[2].as_f64().unwrap(),
        r[3].as_f64().unwrap(),
    );
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Typewriter", "points": [[x0, y0], [x1, y1]], "contents": "Grace" }),
    );
    let v = call(&mut a, "form_typewriter_to_fields", json!({}));
    assert_eq!(v["moved"], 1);
    assert_eq!(v["values"]["Name"], "Grace");
}

#[test]
fn combine_create_and_layered() {
    let d = dir();
    one(&d, "A-101.pdf", String::new());
    one(&d, "A-102.pdf", String::new());
    std::fs::write(d.join("notes.txt"), "Line one\nLine two").unwrap();
    let mut a = auto(&d);
    let v = call(
        &mut a,
        "doc_combine_files",
        json!({ "files": ["A-101.pdf", "A-102.pdf"], "out": "set.pdf", "labels_from_names": true, "properties": true }),
    );
    assert_eq!(v["pages"], 2);
    call(&mut a, "doc_open", json!({ "path": "set.pdf" }));
    let v = call(&mut a, "doc_info", json!({}));
    assert!(v.to_string().contains("A-102"), "{v}");
    let v = call(
        &mut a,
        "doc_create_from_files",
        json!({ "files": ["notes.txt", "A-101.pdf"], "out": "made.pdf" }),
    );
    assert_eq!(v["pages"], 2);
    let v = call(
        &mut a,
        "doc_layered",
        json!({ "files": ["A-101.pdf", "A-102.pdf"], "out": "layers.pdf" }),
    );
    assert_eq!(v["layers"], 1);
    assert!(a.call("doc_layered", &json!({ "files": [], "out": "x.pdf" })).is_err());
}

#[test]
fn redaction_look_codes_and_scrub() {
    let d = dir();
    one(&d, "r.pdf", text(72.0, 700.0, 12.0, "SECRET plan"));
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "r.pdf" }));
    let v = call(&mut a, "redact_codes", json!({}));
    assert!(v["codes"].as_array().unwrap().len() >= 10);
    call(
        &mut a,
        "doc_properties_set",
        json!({ "properties": { "Author": "Someone" } }),
    );
    let v = call(
        &mut a,
        "redact_mark",
        json!({ "text": "SECRET", "overlay": "(b)(6)", "font": "Courier", "font_size": 6, "text_color": "#ffffff", "align": "center", "repeat": true }),
    );
    assert_eq!(v["marked"], 1);
    assert!(
        a.call("redact_mark", &json!({ "text": "plan", "align": "middle" }))
            .is_err()
    );
    let v = call(&mut a, "redact_apply", json!({ "scrub_metadata": true }));
    assert_eq!(v["verified"], true);
    let p = call(&mut a, "doc_properties", json!({}));
    assert!(!p.to_string().contains("Someone"), "{p}");
}

#[test]
fn batch_link_terms_highlight_and_filters() {
    let d = dir();
    one(&d, "index.pdf", text(72.0, 700.0, 12.0, "See DETAILS and Plumbing"));
    one(&d, "Plumbing.pdf", text(72.0, 700.0, 12.0, "P-1"));
    let mut a = auto(&d);
    let v = call(
        &mut a,
        "batch_link",
        json!({ "files": ["index.pdf", "Plumbing.pdf"], "terms": "file_names", "highlight": "#ffff00" }),
    );
    assert_eq!(v["links"], 1, "{v}");
    let v = call(
        &mut a,
        "batch_link",
        json!({ "files": ["index.pdf", "Plumbing.pdf"], "terms": "custom", "custom": [["DETAILS:x", 1, 1]], "filter_char": ":", "full_paths": true }),
    );
    assert_eq!(v["links"], 1, "{v}");
    std::fs::write(d.join("terms.csv"), "Plumbing,Plumbing.pdf,1\n").unwrap();
    let v = call(
        &mut a,
        "batch_link",
        json!({ "files": ["index.pdf", "Plumbing.pdf"], "terms": "custom", "terms_csv": "terms.csv", "replace_existing": true }),
    );
    assert_eq!(v["existing"], 1, "{v}");
    assert_eq!(v["links"], 1, "{v}");
    call(&mut a, "doc_open", json!({ "path": "index.pdf" }));
    let m = call(&mut a, "markup_list", json!({}));
    assert!(m.to_string().contains("Link"), "{m}");
    assert!(
        a.call("batch_link", &json!({ "files": ["index.pdf"], "terms": "custom" }))
            .is_err()
    );
}

#[test]
fn legend_options_copy_freeze_and_reduce_options() {
    let d = dir();
    std::fs::write(
        d.join("l.pdf"),
        pdf(&[
            SyntheticPage::new(612.0, 792.0, String::new()),
            SyntheticPage::new(612.0, 792.0, String::new()),
        ]),
    )
    .unwrap();
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "l.pdf" }));
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[100, 100], [200, 200]], "subject": "Door" }),
    );
    let v = call(
        &mut a,
        "legend_add",
        json!({ "page": 1, "at": [300, 700], "subjects": ["Door", "Window"], "show_empty": true, "fill_color": "none", "header": false, "symbol_scale": 1.5 }),
    );
    let id = v["id"].as_str().unwrap().to_string();
    assert_eq!(v["legend"]["rows"].as_array().unwrap().len(), 2, "{v}");
    assert_eq!(v["legend"]["header"], false);
    assert_eq!(v["legend"]["fill_color"], Value::Null);
    let v = call(&mut a, "legend_copy", json!({ "id": id }));
    assert_eq!(v["legends"].as_array().unwrap().len(), 1);
    let v = call(&mut a, "legend_freeze", json!({ "id": id }));
    assert_eq!(v["legends"], 1);
    let v = call(
        &mut a,
        "doc_reduce_size",
        json!({ "gray_target_ppi": 100, "discard_metadata": true, "discard_private": true, "crop_to_crop_box": true }),
    );
    assert!(v["bytes_after"].as_u64().is_some());
    assert!(
        a.call(
            "doc_reduce_size",
            &json!({ "gray_target_ppi": 100, "gray_quality": 101 })
        )
        .is_err()
    );
}

#[test]
fn quantity_links_and_folder_summary_to_excel() {
    let d = dir();
    std::fs::create_dir_all(d.join("sheets/sub")).unwrap();
    one(&d, "sheets/a.pdf", String::new());
    one(&d, "sheets/sub/b.pdf", String::new());
    let mut a = auto(&d);
    for f in ["sheets/a.pdf", "sheets/sub/b.pdf"] {
        call(&mut a, "doc_open", json!({ "path": f }));
        call(
            &mut a,
            "markup_add",
            json!({ "page": 1, "kind": "Count", "points": [[10, 10], [20, 20]], "subject": "Door" }),
        );
        call(
            &mut a,
            "markup_add",
            json!({ "page": 1, "kind": "Rectangle", "points": [[100, 100], [200, 200]], "subject": "Note" }),
        );
        call(&mut a, "doc_save", json!({}));
        call(&mut a, "doc_close", json!({}));
    }
    let v = call(
        &mut a,
        "quantity_link",
        json!({ "action": "save", "path": "links.json", "name": "Doors", "sheet": "Takeoff", "cell": "B2",
                "files": ["sheets/a.pdf", "sheets/sub/b.pdf"], "measure": "count", "subjects": ["Door"] }),
    );
    assert_eq!(v["links"], json!(["Doors"]));
    call(
        &mut a,
        "quantity_link",
        json!({ "action": "save", "path": "links.json", "name": "Everything", "cell": "C3",
                "files": ["sheets/a.pdf"], "measure": "count" }),
    );
    let v = call(
        &mut a,
        "quantity_link",
        json!({ "action": "list", "path": "links.json" }),
    );
    assert_eq!(v["links"][0]["value"], 4.0);
    assert_eq!(v["links"][1]["value"], 3.0);
    let v = call(
        &mut a,
        "quantity_link",
        json!({ "action": "update", "path": "links.json", "out": "takeoff.xlsx" }),
    );
    assert_eq!(v["links"][0]["markups"], 2);
    assert!(std::fs::metadata(d.join("takeoff.xlsx")).unwrap().len() > 500);
    assert!(
        a.call(
            "quantity_link",
            &json!({ "action": "save", "path": "links.json", "name": "Bad", "cell": "9Z", "files": ["sheets/a.pdf"] })
        )
        .is_err()
    );
    assert!(a.call("quantity_link", &json!({ "action": "save", "path": "links.json", "name": "Bad", "cell": "A1", "files": ["sheets/a.pdf"], "measure": "weight" })).is_err());
    call(
        &mut a,
        "quantity_link",
        json!({ "action": "delete", "path": "links.json", "name": "Everything" }),
    );
    // Batch Summary of a folder, with subfolders, into Excel.
    let v = call(
        &mut a,
        "batch_summary",
        json!({ "folder": "sheets", "recursive": true, "out": "summary.xlsx" }),
    );
    assert_eq!(v["files"], 2);
    assert_eq!(v["markups"], 4);
    let v = call(&mut a, "batch_summary", json!({ "folder": "sheets" }));
    assert_eq!(v["markups"], 2);
}

#[test]
fn dynamic_fill_by_path_polylength_and_volume() {
    use markupcraft_engine::synthetic::line;
    let d = dir();
    let content = format!(
        "{}{}{}{}{}",
        line(0.0, 0.0, 200.0, 0.0, 1.0),
        line(200.0, 0.0, 200.0, 100.0, 1.0),
        line(200.0, 100.0, 0.0, 100.0, 1.0),
        line(0.0, 100.0, 0.0, 0.0, 1.0),
        line(100.0, 0.0, 100.0, 100.0, 1.0)
    );
    one(&d, "rooms.pdf", content);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "rooms.pdf" }));
    let v = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "path": [[20, 50], [180, 50]] }),
    );
    assert_eq!(v["regions"], 2, "{v}");
    let v = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [50, 50], "output": "polylength" }),
    );
    assert_eq!(v["created"]["markup"]["kind"], "Polylength", "{v}");
    let v = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [150, 50], "output": "volume", "depth": 3 }),
    );
    assert_eq!(v["created"]["markup"]["kind"], "Volume", "{v}");
    assert!(
        a.call(
            "dynamic_fill",
            &json!({ "page": 1, "point": [150, 50], "output": "volume" })
        )
        .is_err()
    );
    assert!(
        a.call("dynamic_fill", &json!({ "page": 1, "path": [[500, 500]] }))
            .is_err()
    );
}
