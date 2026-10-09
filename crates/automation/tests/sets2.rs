//! End-to-end tests of the layer additions (hierarchy, configurations, page layers, preview,
//! import and export), Sets (tags, categories, revisions, publish, print), the Markup Summary
//! options and printing (jobs, Batch Print), through `Automation::call`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use markupcraft_engine::synthetic::{SyntheticPage, pdf, rect, text};
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-sets2-{}-{}",
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
fn layer_hierarchy_configurations_views_import_and_export() {
    let d = dir();
    one(&d, "plan.pdf", rect(10.0, 10.0, 20.0, 20.0));
    one(&d, "electrical.pdf", rect(300.0, 300.0, 100.0, 100.0));
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "plan.pdf" }));
    for n in ["Arch", "Doors"] {
        call(&mut a, "layer_create", json!({ "name": n }));
    }
    let v = call(&mut a, "layer_nest", json!({ "name": "Doors", "parent": "Arch" }));
    assert_eq!(v["tree"][1], json!({ "name": "Doors", "depth": 1, "parent": "Arch" }));
    call(
        &mut a,
        "layer_import",
        json!({ "path": "electrical.pdf", "page": 1, "name": "Electrical" }),
    );
    let v = call(&mut a, "layer_view", json!({ "page": 1 }));
    assert_eq!(v["on_page"], json!(["Electrical"]));
    call(&mut a, "layer_set", json!({ "name": "Electrical", "visible": false }));
    let v = call(&mut a, "layer_config", json!({ "action": "save", "name": "No power" }));
    assert_eq!(v["configs"], json!(["No power"]));
    call(&mut a, "layer_set", json!({ "name": "Electrical", "visible": true }));
    let v = call(&mut a, "layer_config", json!({ "action": "apply", "name": "No power" }));
    let e = v["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "Electrical")
        .unwrap()
        .clone();
    assert_eq!(e["visible"], false);
    call(&mut a, "layer_set", json!({ "name": "Electrical", "visible": true }));
    call(&mut a, "layer_set", json!({ "name": "Doors", "print": false }));
    let v = call(
        &mut a,
        "layer_view",
        json!({ "preview": "print", "name": "Arch", "export_state": false }),
    );
    let vis = |v: &Value, n: &str| {
        v["layers"].as_array().unwrap().iter().find(|l| l["name"] == n).unwrap()["visible"].clone()
    };
    assert_eq!(vis(&v, "Doors"), false);
    let v = call(&mut a, "layer_view", json!({ "preview": "end" }));
    assert_eq!(vis(&v, "Doors"), true);
    let v = call(
        &mut a,
        "layer_export",
        json!({ "name": "Electrical", "out": "electrical-only.pdf" }),
    );
    assert_eq!(v["pages"], 1);
    assert!(d.join("electrical-only.pdf").is_file());
}

#[test]
fn sets_tags_categories_revisions_publish_and_print() {
    let d = dir();
    one(&d, "A-101 Plan.pdf", text(72.0, 700.0, 14.0, "OLD PLAN"));
    one(&d, "A-101 Plan Rev 2.pdf", text(72.0, 700.0, 14.0, "NEW PLAN"));
    one(&d, "M-201 Mech.pdf", text(72.0, 700.0, 14.0, "MECH"));
    let mut a = auto(&d);
    call(
        &mut a,
        "set_save",
        json!({ "path": "job.pcset", "name": "Job", "files": ["A-101 Plan.pdf", "A-101 Plan Rev 2.pdf", "M-201 Mech.pdf"] }),
    );
    let v = call(
        &mut a,
        "set_tags",
        json!({ "set": "job.pcset", "tag": { "sheet": "M-201 Mech.pdf#1", "name": "Phase", "value": "CD" }, "categories": "file_name", "revisions": true, "filter": "@?#" }),
    );
    assert_eq!(v["sheets"][2]["tags"]["Phase"], "CD");
    assert_eq!(v["sheets"][1]["tags"]["Revision"], "2");
    assert_eq!(v["categories"][0]["category"], "Architectural");
    let a101 = v["revisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["sheet"] == "A-101")
        .unwrap()
        .clone();
    assert!(a101["latest"].as_str().unwrap().ends_with("A-101 Plan Rev 2.pdf"));
    // The tag was saved in the set file.
    assert!(std::fs::read_to_string(d.join("job.pcset")).unwrap().contains("Phase"));
    let v = call(
        &mut a,
        "set_publish",
        json!({ "set": "job.pcset", "out": "published.pdf", "latest_only": true, "filter": "@?#", "dir": "package", "log": "log.csv" }),
    );
    assert_eq!(v["pages"], 2);
    assert_eq!(v["log_rows"], 3);
    assert!(d.join("package").join("Drawing Log.csv").is_file());
    let v = call(
        &mut a,
        "set_print",
        json!({ "set": "job.pcset", "out": "set-print.pdf", "copies": 2 }),
    );
    assert_eq!(v["sheets"], 6);
    // Search a Set.
    let v = call(&mut a, "text_search", json!({ "text": "PLAN", "set": "job.pcset" }));
    assert_eq!(v["count"], 2);
}

#[test]
fn summary_options_and_print_jobs() {
    let d = dir();
    one(&d, "s.pdf", rect(100.0, 100.0, 200.0, 200.0));
    std::fs::write(
        d.join("other.pdf"),
        pdf(&[
            SyntheticPage::new(612.0, 792.0, ""),
            SyntheticPage::new(612.0, 792.0, ""),
        ]),
    )
    .unwrap();
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "s.pdf" }));
    for (x, subj) in [(100.0, "Beta"), (300.0, "Alpha"), (500.0, "Alpha")] {
        call(
            &mut a,
            "markup_add",
            json!({ "page": 1, "kind": "Rectangle", "points": [[x, 500], [x + 50.0, 550]], "subject": subj }),
        );
    }
    let v = call(
        &mut a,
        "summary_export",
        json!({ "out": "sum.csv", "columns": ["subject"], "sort": "subject", "then_by": [["subject", true]], "content": "markups", "headers": false, "filters": { "subject": ["Alpha"] } }),
    );
    assert_eq!(v["markups"], 2);
    let csv = std::fs::read_to_string(d.join("sum.csv")).unwrap();
    assert_eq!(csv.lines().collect::<Vec<_>>(), ["Alpha", "Alpha"], "{csv}");
    call(
        &mut a,
        "summary_export",
        json!({ "out": "sum.pdf", "pdf_flow": true, "pdf_paper": "letter", "pdf_break_per_group": true, "group_by": ["subject"] }),
    );
    let pdf_doc = markupcraft_engine::Session::open(d.join("sum.pdf")).unwrap();
    assert_eq!(pdf_doc.links().len(), 3, "each markup links to its page");
    let v = call(
        &mut a,
        "summary_export",
        json!({ "out": "per.csv", "columns": ["subject"], "per_value": true }),
    );
    assert_eq!(v["files"].as_array().unwrap().len(), 2);
    // Print jobs.
    let v = call(
        &mut a,
        "print_pdf",
        json!({ "out": "copies.pdf", "copies": 3, "collate": false, "reverse": true }),
    );
    assert_eq!(v["sheets"], 3);
    let v = call(
        &mut a,
        "print_pdf",
        json!({ "out": "region.pdf", "region": [1, 50, 50, 350, 350], "margin": 36, "offset": [10, 0], "markups_only": true }),
    );
    assert_eq!(v["sheets"], 1);
    call(
        &mut a,
        "print_pdf",
        json!({ "out": "emph.pdf", "dim_content": true, "spaces": true, "links": true, "dim_except": [] }),
    );
    let v = call(
        &mut a,
        "batch_print",
        json!({ "files": ["s.pdf", "other.pdf"], "out_dir": "prints" }),
    );
    assert_eq!(v["results"][1]["sheets"], 2);
    assert!(call(&mut a, "printer_list", json!({}))["printers"].is_array());
}
