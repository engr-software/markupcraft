//! End-to-end tests of the organizing tools (layers, spaces, legends, dynamic fill, hatch,
//! stamps, batch, sets, preferences), through `Automation::call`, on PDFs built here.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-organize-{}-{}",
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

fn fails(a: &mut Automation, tool: &str, args: Value) -> String {
    match a.call(tool, &args) {
        Ok(v) => panic!("{tool} should fail, got {v}"),
        Err(e) => e.to_string(),
    }
}

fn new_doc(a: &mut Automation, name: &str, pages: u64) -> u64 {
    let v = call(a, "doc_new", json!({ "path": name, "pages": pages }));
    call(a, "doc_save", json!({}));
    v["doc"].as_u64().unwrap()
}

fn square(x: f64, y: f64, s: f64) -> Value {
    json!([[x, y], [x + s, y], [x + s, y + s], [x, y + s]])
}

fn add(a: &mut Automation, args: Value) -> String {
    call(a, "markup_add", args)["id"].as_str().unwrap().to_string()
}

fn markup(a: &mut Automation, id: &str) -> Value {
    let v = call(a, "markup_list", json!({}));
    v["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no markup {id}"))
}

/// A PDF of letter pages with these content streams (font /F1 = Helvetica).
fn content_pdf(d: &Path, name: &str, pages: &[String]) {
    let pages: Vec<markupcraft_engine::synthetic::SyntheticPage> = pages
        .iter()
        .map(|c| markupcraft_engine::synthetic::SyntheticPage::new(612.0, 792.0, c.clone()))
        .collect();
    std::fs::write(d.join(name), markupcraft_engine::synthetic::pdf(&pages)).unwrap();
}

/// Two rooms (100..200 and 200..300 by 100..200) with a column in the left one.
fn plan_content() -> String {
    use markupcraft_engine::synthetic::line;
    let mut c = String::new();
    for (a, b, x, y) in [
        (100.0, 100.0, 300.0, 100.0),
        (300.0, 100.0, 300.0, 200.0),
        (300.0, 200.0, 100.0, 200.0),
        (100.0, 200.0, 100.0, 100.0),
        (200.0, 90.0, 200.0, 210.0),
        (160.0, 120.0, 170.0, 120.0),
        (170.0, 120.0, 170.0, 130.0),
        (170.0, 130.0, 160.0, 130.0),
        (160.0, 130.0, 160.0, 120.0),
    ] {
        c.push_str(&line(a, b, x, y, 1.0));
    }
    c
}

#[test]
fn layers_create_assign_hide_filter_rename_delete() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "layers.pdf", 1);
    call(&mut a, "layer_create", json!({ "name": "Electrical" }));
    assert!(fails(&mut a, "layer_create", json!({ "name": "Electrical" })).contains("already"));
    let r1 = add(
        &mut a,
        json!({ "kind": "Rectangle", "page": 1, "points": square(100.0, 100.0, 50.0) }),
    );
    let r2 = add(
        &mut a,
        json!({ "kind": "Rectangle", "page": 1, "points": square(300.0, 100.0, 50.0) }),
    );
    call(&mut a, "layer_assign", json!({ "ids": [r1], "layer": "Electrical" }));
    call(&mut a, "layer_assign", json!({ "ids": [r2], "layer": "Plumbing" }));
    call(
        &mut a,
        "layer_set",
        json!({ "name": "Plumbing", "visible": false, "print": false, "locked": true }),
    );
    let vis = call(&mut a, "layer_markups", json!({ "visible_only": true }));
    assert_eq!(vis["ids"], json!([r1]));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));

    call(&mut a, "doc_open", json!({ "path": "layers.pdf" }));
    let l = call(&mut a, "layer_list", json!({}));
    assert_eq!(l["layers"][0]["name"], "Electrical");
    assert_eq!(l["layers"][1]["name"], "Plumbing");
    assert_eq!(l["layers"][1]["visible"], false);
    assert_eq!(l["layers"][1]["print"], false);
    assert_eq!(l["layers"][1]["locked"], true);
    assert_eq!(l["layers"][1]["markups"], 1);
    assert_eq!(markup(&mut a, &r2)["layer"], "Plumbing");
    let on = call(
        &mut a,
        "layer_markups",
        json!({ "layers": ["Plumbing"], "select": true }),
    );
    assert_eq!(on["ids"], json!([r2]));

    call(&mut a, "layer_set", json!({ "show_all": true }));
    call(&mut a, "layer_set", json!({ "name": "Electrical", "isolate": true }));
    let l = call(&mut a, "layer_list", json!({}));
    assert_eq!(l["layers"][1]["visible"], false);
    call(
        &mut a,
        "layer_rename",
        json!({ "name": "Plumbing", "new_name": "Mechanical" }),
    );
    assert_eq!(markup(&mut a, &r2)["layer"], "Mechanical");
    let del = call(
        &mut a,
        "layer_delete",
        json!({ "name": "Mechanical", "delete_markups": true }),
    );
    assert_eq!(del["markups"], 1);
    assert_eq!(del["document"]["markups"], 1);
    call(&mut a, "edit_undo", json!({}));
    assert_eq!(markup(&mut a, &r2)["layer"], "Mechanical");
    fails(&mut a, "layer_set", json!({ "name": "Electrical" }));
    fails(&mut a, "layer_delete", json!({ "name": "Nope" }));
}

#[test]
fn spaces_add_column_tally_export_import() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "spaces.pdf", 1);
    let office = call(
        &mut a,
        "space_add",
        json!({ "page": 1, "name": "Office", "points": square(0.0, 0.0, 200.0), "color": "#3366CC" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &mut a,
        "space_add",
        json!({ "page": 1, "name": "Lobby", "points": square(200.0, 0.0, 200.0) }),
    );
    let r = add(
        &mut a,
        json!({ "kind": "Rectangle", "page": 1, "points": square(20.0, 20.0, 30.0) }),
    );
    add(
        &mut a,
        json!({ "kind": "Count", "page": 1, "subject": "Outlet", "points": [[150, 20], [250, 20], [260, 40]] }),
    );
    let l = call(&mut a, "space_list", json!({ "page": 1 }));
    assert_eq!(l["spaces"][0]["name"], "Office");
    assert_eq!(l["spaces"][0]["markups"], json!([r]));
    let t = call(&mut a, "space_tally", json!({}));
    let lobby = t["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["space"] == "Lobby")
        .unwrap();
    assert_eq!(lobby["counts"]["Outlet"], 2);

    call(&mut a, "space_export", json!({ "path": "spaces.json" }));
    call(
        &mut a,
        "space_edit",
        json!({ "id": office, "name": "Office 101", "opacity": 0.5 }),
    );
    assert_eq!(call(&mut a, "space_list", json!({}))["spaces"][0]["name"], "Office 101");
    call(&mut a, "space_delete", json!({ "ids": [office] }));
    assert_eq!(
        call(&mut a, "space_list", json!({}))["spaces"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    call(&mut a, "doc_save", json!({}));

    new_doc(&mut a, "rev2.pdf", 1);
    let imp = call(
        &mut a,
        "space_import",
        json!({ "path": "spaces.json", "match": "number" }),
    );
    assert_eq!(imp["imported"], 2);
    fails(
        &mut a,
        "space_add",
        json!({ "page": 1, "name": "X", "points": [[0, 0], [1, 1]] }),
    );
    fails(&mut a, "space_edit", json!({ "id": "nope", "name": "Y" }));
}

#[test]
fn dynamic_fill_makes_areas_spaces_and_hatch() {
    let d = dir();
    let mut a = auto(&d);
    content_pdf(&d, "plan.pdf", &[plan_content()]);
    call(&mut a, "doc_open", json!({ "path": "plan.pdf" }));
    let pv = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [130, 130], "preview": true }),
    );
    assert_eq!(pv["outline"].as_array().unwrap().len(), 4);
    assert_eq!(pv["cutouts"].as_array().unwrap().len(), 1);
    assert_eq!(
        call(&mut a, "markup_list", json!({}))["markups"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let r = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [130, 130], "subject": "Floor", "color": "#008000", "scale": { "kind": "architectural", "paper_inches": 0.125, "real_feet": 1 } }),
    );
    assert!((r["area_pt2"].as_f64().unwrap() - 9_900.0).abs() < 1e-6, "{r}");
    let area = r["created"]["id"].as_str().unwrap().to_string();
    let m = markup(&mut a, &area);
    assert_eq!(m["kind"], "Area");
    assert_eq!(m["subject"], "Floor");
    assert_eq!(m["holes"].as_array().unwrap().len(), 1);
    let sp = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [250, 150], "output": "space", "space_name": "Room 2" }),
    );
    assert!(sp["created"]["space"].is_string());
    assert_eq!(call(&mut a, "space_list", json!({}))["spaces"][0]["name"], "Room 2");
    // open linework: nothing to fill
    let err = fails(&mut a, "dynamic_fill", json!({ "page": 1, "point": [500, 500] }));
    assert!(err.contains("no closed region"), "{err}");
    fails(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [250, 150], "output": "space" }),
    );

    call(
        &mut a,
        "markup_hatch",
        json!({ "ids": [area], "style": "DiagonalCross", "spacing": 5 }),
    );
    assert_eq!(markup(&mut a, &area)["hatch"], "DiagonalCross");
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "plan.pdf" }));
    assert_eq!(markup(&mut a, &area)["hatch"], "DiagonalCross");
    call(&mut a, "markup_hatch", json!({ "ids": [area], "style": "none" }));
    assert!(markup(&mut a, &area)["hatch"].is_null());
    assert!(fails(&mut a, "markup_hatch", json!({ "ids": [area], "style": "Plaid" })).contains("style"));
}

#[test]
fn legends_list_subjects_and_follow_edits() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "legend.pdf", 1);
    call(
        &mut a,
        "scale_set",
        json!({ "scale": { "kind": "architectural", "paper_inches": 0.125, "real_feet": 1 } }),
    );
    for x in [50.0, 200.0] {
        add(
            &mut a,
            json!({ "kind": "Area", "page": 1, "subject": "Carpet", "points": square(x, 50.0, 90.0) }),
        );
    }
    add(
        &mut a,
        json!({ "kind": "Count", "page": 1, "subject": "Outlet", "points": [[10, 10], [20, 20], [30, 30]] }),
    );
    let l = call(
        &mut a,
        "legend_add",
        json!({ "page": 1, "at": [350, 700], "title": "Finishes" }),
    );
    let id = l["id"].as_str().unwrap().to_string();
    let rows = &l["legend"]["rows"];
    assert_eq!(rows[0]["subject"], "Carpet");
    assert_eq!(rows[0]["total_text"], "200 sf");
    assert_eq!(rows[1]["total_text"], "3 ea");
    let m = markup(&mut a, &id);
    assert_eq!(m["subject"], "Legend");
    assert!(m["contents"].as_str().unwrap().contains("Carpet"));

    add(
        &mut a,
        json!({ "kind": "Rectangle", "page": 1, "subject": "Door", "points": [[400, 400], [420, 440]] }),
    );
    call(&mut a, "legend_update", json!({}));
    assert!(markup(&mut a, &id)["contents"].as_str().unwrap().contains("Door"));
    call(
        &mut a,
        "legend_update",
        json!({ "id": id, "columns": ["Symbol", "Subject", "Count"], "measurements_only": true }),
    );
    let ls = call(&mut a, "legend_list", json!({}));
    assert_eq!(ls["legends"][0]["columns"], json!(["Symbol", "Subject", "Count"]));
    assert_eq!(ls["legends"][0]["rows"].as_array().unwrap().len(), 2);
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "legend.pdf" }));
    assert_eq!(
        call(&mut a, "legend_list", json!({}))["legends"][0]["title"],
        "Finishes"
    );
    fails(&mut a, "legend_update", json!({ "id": "nope" }));
    fails(&mut a, "legend_add", json!({ "page": 1, "at": [0, 0], "font_size": 1 }));
}

#[test]
fn stamps_builtin_custom_dynamic_fields_and_images() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "stamps.pdf", 2);
    let l = call(&mut a, "stamp_list", json!({}));
    let ids: Vec<&str> = l["stamps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    for want in ["Approved", "Rejected", "Reviewed", "Draft", "ForConstruction", "Void"] {
        assert!(ids.contains(&want), "{ids:?}");
    }
    // 2026-10-09 14:05 UTC
    let t = 1_791_554_709;
    let s = call(
        &mut a,
        "stamp_add",
        json!({ "page": 2, "at": [300, 400], "stamp": "Approved", "time": t }),
    );
    assert_eq!(s["markup"]["kind"], "Stamp");
    assert_eq!(s["markup"]["contents"], "APPROVED\rTester  2026-10-09");
    let c = call(
        &mut a,
        "stamp_create",
        json!({ "name": "Received", "text": "RECEIVED\r{prompt:Job=0000} page {page} of {pages} {date:MMM d, yyyy}", "color": "#008000" }),
    );
    let cid = c["id"].as_str().unwrap().to_string();
    let l = call(&mut a, "stamp_list", json!({}));
    let mine = l["stamps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == cid.as_str())
        .unwrap();
    assert_eq!(mine["prompts"][0]["label"], "Job");
    let s = call(
        &mut a,
        "stamp_add",
        json!({ "page": 1, "rect": [50, 50, 250, 120], "stamp": cid, "answers": { "Job": "1021" }, "time": t }),
    );
    assert_eq!(s["markup"]["contents"], "RECEIVED\r1021 page 1 of 2 Oct 9, 2026");
    assert_eq!(s["markup"]["color"], "#008000");

    // image stamps: a PNG with transparency and a PDF page
    let mut img = image::RgbaImage::new(8, 4);
    for (x, _, p) in img.enumerate_pixels_mut() {
        *p = image::Rgba([200, 0, 0, if x < 4 { 255 } else { 0 }]);
    }
    img.save(d.join("seal.png")).unwrap();
    let png = call(
        &mut a,
        "stamp_add",
        json!({ "page": 1, "at": [300, 300], "image": "seal.png" }),
    );
    let png_id = png["id"].as_str().unwrap().to_string();
    let r = png["markup"]["rect"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>();
    assert!(
        ((r[2] - r[0]) - 144.0).abs() < 1e-6 && ((r[3] - r[1]) - 72.0).abs() < 1e-6,
        "{r:?}"
    );
    std::fs::write(
        d.join("logo.pdf"),
        markupcraft_engine::synthetic::pdf(&[markupcraft_engine::synthetic::SyntheticPage::new(
            200.0,
            100.0,
            markupcraft_engine::synthetic::text(10.0, 40.0, 24.0, "OUR SEAL"),
        )]),
    )
    .unwrap();
    let lib = call(&mut a, "stamp_create", json!({ "name": "Seal", "image": "logo.pdf" }));
    let pdf = call(
        &mut a,
        "stamp_add",
        json!({ "page": 1, "at": [300, 600], "stamp": lib["id"] }),
    );
    assert_eq!(pdf["markup"]["kind"], "Stamp");
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "stamps.pdf" }));
    assert_eq!(markup(&mut a, &png_id)["kind"], "Stamp");
    call(&mut a, "stamp_remove", json!({ "id": cid }));
    fails(&mut a, "stamp_remove", json!({ "id": "Approved" }));
    fails(&mut a, "stamp_add", json!({ "page": 1, "at": [1, 1], "stamp": "nope" }));
    fails(
        &mut a,
        "stamp_create",
        json!({ "name": "x", "image": "stamps.pdf.txt" }),
    );
}

#[test]
fn preferences_profiles_set_switch_export_import() {
    let d = dir();
    let mut a = auto(&d);
    let p = call(&mut a, "prefs_get", json!({}));
    assert_eq!(p["active"], "Default");
    assert_eq!(p["preferences"]["units"], "ft-in");
    call(
        &mut a,
        "prefs_set",
        json!({ "values": { "author": "Estimator", "units": "m", "snapping": { "grid": true, "grid_spacing": 36 }, "autosave_minutes": 5 } }),
    );
    let p = call(&mut a, "prefs_get", json!({}));
    assert_eq!(p["preferences"]["snapping"]["grid"], true);
    assert_eq!(p["preferences"]["snapping"]["content"], true);
    // the author reaches new markups
    new_doc(&mut a, "p.pdf", 1);
    let id = add(
        &mut a,
        json!({ "kind": "Rectangle", "page": 1, "points": square(10.0, 10.0, 10.0) }),
    );
    assert_eq!(markup(&mut a, &id)["author"], "Estimator");
    assert!(fails(&mut a, "prefs_set", json!({ "values": { "units": "cubits" } })).contains("units"));
    assert!(fails(&mut a, "prefs_set", json!({ "values": { "colour": "#000000" } })).contains("unknown"));

    let s = call(&mut a, "profile_switch", json!({ "profile": "Takeoff" }));
    assert_eq!(s["preferences"]["units"], "m");
    call(&mut a, "prefs_set", json!({ "values": { "units": "ft" } }));
    call(&mut a, "prefs_export", json!({ "path": "takeoff.json" }));
    call(&mut a, "profile_switch", json!({ "profile": "Default" }));
    assert_eq!(call(&mut a, "prefs_get", json!({}))["preferences"]["units"], "m");
    call(
        &mut a,
        "prefs_import",
        json!({ "path": "takeoff.json", "profile": "Shared" }),
    );
    assert_eq!(
        call(&mut a, "prefs_get", json!({ "profile": "Shared" }))["preferences"]["units"],
        "ft"
    );
    let l = call(&mut a, "profile_list", json!({}));
    assert_eq!(l["profiles"], json!(["Default", "Shared", "Takeoff"]));
    call(&mut a, "profile_delete", json!({ "profile": "Takeoff" }));
    fails(&mut a, "profile_delete", json!({ "profile": "Default" }));
    fails(&mut a, "profile_switch", json!({ "profile": "../x" }));
    assert!(d.join("config").join("profiles").join("Default.json").exists());
}

/// A file of sheets: (label, text shown on the page), with page labels set.
fn sheets(a: &mut Automation, d: &Path, name: &str, sheets: &[(&str, &str)]) {
    let pages: Vec<String> = sheets
        .iter()
        .map(|(_, t)| markupcraft_engine::synthetic::text(72.0, 700.0, 14.0, t))
        .collect();
    content_pdf(d, name, &pages);
    call(a, "doc_open", json!({ "path": name }));
    let mut labels = serde_json::Map::new();
    for (i, (l, _)) in sheets.iter().enumerate() {
        labels.insert((i + 1).to_string(), json!(l));
    }
    call(a, "page_label_set", json!({ "labels": labels }));
    call(a, "doc_save", json!({ "full": true }));
    call(a, "doc_close", json!({}));
}

#[test]
fn batch_link_summary_sets_and_apply() {
    let d = dir();
    let mut a = auto(&d);
    sheets(
        &mut a,
        &d,
        "arch.pdf",
        &[("A-101", "SEE A-102"), ("A-102", "REFER TO A-101 AND S-201")],
    );
    sheets(&mut a, &d, "struct.pdf", &[("S-201", "SEE A-101 FOR LAYOUT")]);
    call(
        &mut a,
        "set_save",
        json!({ "path": "job.pcset", "name": "Job", "files": ["arch.pdf", "struct.pdf"] }),
    );
    let sh = call(&mut a, "set_sheets", json!({ "set": "job.pcset", "sort": "label" }));
    let labels: Vec<&str> = sh["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["A-101", "A-102", "S-201"]);

    let r = call(&mut a, "batch_link", json!({ "set": "job.pcset" }));
    assert_eq!(r["links"], 4, "{r}");
    let again = call(&mut a, "batch_link", json!({ "files": ["arch.pdf", "struct.pdf"] }));
    assert_eq!(again["links"], 0);
    assert_eq!(again["existing"], 4);
    call(&mut a, "doc_open", json!({ "path": "arch.pdf" }));
    let links = call(&mut a, "link_list", json!({}));
    let targets: Vec<String> = links["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["target"].to_string())
        .collect();
    assert!(targets.iter().any(|t| t.contains("\"page\":2")), "{targets:?}");
    assert!(targets.iter().any(|t| t.contains("struct.pdf")), "{targets:?}");
    // a file open here is not changed behind its back
    assert!(fails(&mut a, "batch_link", json!({ "files": ["arch.pdf"] })).contains("open"));
    add(
        &mut a,
        json!({ "kind": "Rectangle", "page": 1, "subject": "Door", "points": [[10, 10], [30, 30]] }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));

    let sum = call(&mut a, "batch_summary", json!({ "set": "job.pcset" }));
    assert_eq!(sum["markups"], 1);
    assert!(sum["csv"].as_str().unwrap().starts_with("File,Page"));
    assert!(sum["csv"].as_str().unwrap().contains("\"arch.pdf\""));
    call(
        &mut a,
        "batch_summary",
        json!({ "files": ["arch.pdf"], "out": "summary.csv" }),
    );
    assert!(d.join("summary.csv").exists());

    let ap = call(
        &mut a,
        "batch_apply",
        json!({
            "set": "job.pcset",
            "out_dir": "out",
            "operations": [
                { "tool": "watermark_add", "args": { "text": "PRELIMINARY" } },
                { "tool": "stamp_add", "args": { "page": 1, "at": [300, 300], "stamp": "Reviewed" } }
            ]
        }),
    );
    assert_eq!(ap["done"], 2, "{ap}");
    call(&mut a, "doc_open", json!({ "path": "out/struct.pdf" }));
    let ms = call(&mut a, "markup_list", json!({}));
    assert_eq!(ms["markups"][0]["kind"], "Stamp");
    call(&mut a, "doc_close", json!({}));
    assert_eq!(
        call(&mut a, "doc_list", json!({}))["documents"]
            .as_array()
            .map_or(0, |v| v.len()),
        0
    );
    let bad = fails(
        &mut a,
        "batch_apply",
        json!({ "files": ["arch.pdf"], "operations": [{ "tool": "doc_close" }] }),
    );
    assert!(bad.contains("cannot run"), "{bad}");
    let failed = call(
        &mut a,
        "batch_apply",
        json!({ "files": ["arch.pdf"], "out_dir": "out2", "operations": [{ "tool": "stamp_add", "args": { "page": 9, "at": [1, 1], "stamp": "Void" } }] }),
    );
    assert_eq!(failed["failed"], 1);
}

#[test]
fn slip_sheet_replaces_matching_sheets_and_keeps_markups() {
    let d = dir();
    let mut a = auto(&d);
    sheets(
        &mut a,
        &d,
        "old.pdf",
        &[("A-101 - PLAN", "OLD PLAN"), ("A-102 - ELEVATIONS", "OLD ELEV")],
    );
    sheets(
        &mut a,
        &d,
        "new.pdf",
        &[
            ("A-102 - ELEVATIONS REV 1", "NEW ELEV"),
            ("A-103 - SECTIONS", "NEW SECT"),
        ],
    );
    call(&mut a, "doc_open", json!({ "path": "old.pdf" }));
    let m = add(
        &mut a,
        json!({ "kind": "Cloud", "page": 2, "points": square(100.0, 100.0, 50.0) }),
    );
    let r = call(
        &mut a,
        "slip_sheet",
        json!({ "new_file": "new.pdf", "number_filter": " - " }),
    );
    assert_eq!(r["matched"][0]["label"], "A-102 - ELEVATIONS");
    assert_eq!(r["matched"][0]["markups"], 1);
    assert_eq!(r["unmatched_old"], json!([1]));
    assert_eq!(r["appended"], 1);
    assert_eq!(r["document"]["pages"], 3);
    assert_eq!(markup(&mut a, &m)["page"], 2);
    let t = call(&mut a, "page_text", json!({ "page": 2 }));
    assert!(t["text"].as_str().unwrap().contains("NEW ELEV"), "{t}");
    let labels = call(&mut a, "page_labels", json!({}));
    assert!(labels.to_string().contains("A-103 - SECTIONS"), "{labels}");
    // without the filter nothing matches
    call(&mut a, "edit_undo", json!({}));
    call(&mut a, "edit_undo", json!({}));
    call(&mut a, "edit_undo", json!({}));
    let r = call(
        &mut a,
        "slip_sheet",
        json!({ "new_file": "new.pdf", "append_unmatched": false }),
    );
    assert_eq!(r["matched"].as_array().unwrap().len(), 0);
}
