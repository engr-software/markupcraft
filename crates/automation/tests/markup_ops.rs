//! End-to-end tests of the drawing and takeoff operations: Dimension and Arc markups, the
//! Eraser, Count series, lasso selection, arcs, cutouts as measurements, Recalculate and the
//! measurement caption / slope properties.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-markup-ops-{}-{}",
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

fn fails(a: &mut Automation, tool: &str, args: Value) -> String {
    match a.call(tool, &args) {
        Ok(v) => panic!("{tool} should fail, got {v}"),
        Err(e) => e.to_string(),
    }
}

fn add(a: &mut Automation, args: Value) -> String {
    call(a, "markup_add", args)["id"].as_str().unwrap().to_string()
}

fn markup(a: &mut Automation, id: &str) -> Value {
    let list = call(a, "markup_list", json!({}));
    list["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no markup {id}"))
}

fn setup(name: &str) -> (PathBuf, Automation) {
    let d = dir();
    let mut a = auto(&d);
    call(&mut a, "doc_new", json!({ "path": name, "pages": 1 }));
    call(
        &mut a,
        "scale_set",
        json!({ "scale": { "kind": "architectural", "paper_inches": 0.125, "real_feet": 1 } }),
    );
    (d, a)
}

#[test]
fn dimension_and_arc_markups_save_and_reopen() {
    let (_d, mut a) = setup("dims.pdf");
    let dim = add(
        &mut a,
        json!({ "page": 1, "kind": "Dimension", "points": [[100, 100], [190, 100]], "contents": "10'-0\"" }),
    );
    let arc = add(
        &mut a,
        json!({ "page": 1, "kind": "Arc", "points": [[100, 300], [150, 350], [200, 300]] }),
    );
    let v = call(
        &mut a,
        "measure_props_set",
        json!({ "ids": [dim], "offset": 20, "extension": 5 }),
    );
    assert_eq!(v["changed"], 1);
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "dims.pdf" }));
    let d = markup(&mut a, &dim);
    assert_eq!(d["kind"], "Dimension");
    assert_eq!(d["contents"], "10'-0\"");
    // the appearance's rect covers the dimension line 20 pt above the points
    assert!(d["rect"][3].as_f64().unwrap() >= 120.0, "{}", d["rect"]);
    let r = markup(&mut a, &arc);
    assert_eq!(r["kind"], "Arc");
    assert_eq!(r["points"].as_array().unwrap().len(), 3);
}

#[test]
fn eraser_cuts_ink_and_deletes_what_is_rubbed_out() {
    let (_d, mut a) = setup("ink.pdf");
    let pen = add(
        &mut a,
        json!({ "page": 1, "kind": "Pen", "points": [[100, 100], [120, 100], [146, 100], [154, 100], [180, 100], [200, 100]] }),
    );
    let v = call(
        &mut a,
        "ink_erase",
        json!({ "page": 1, "path": [[150, 90], [150, 110]], "radius": 6 }),
    );
    assert_eq!(v["changed"], 1);
    let m = markup(&mut a, &pen);
    assert_eq!(
        m["points"].as_array().unwrap().len(),
        4,
        "the two points by x = 150 are gone: {m}"
    );
    call(
        &mut a,
        "ink_erase",
        json!({ "page": 1, "path": [[90, 100], [210, 100]] }),
    );
    let list = call(&mut a, "markup_list", json!({}));
    assert_eq!(list["count"], 0, "erasing every point deletes the markup");
    call(&mut a, "edit_undo", json!({}));
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], 1);
}

#[test]
fn count_series_resume_delete_split_and_merge() {
    let (_d, mut a) = setup("counts.pdf");
    let c = add(
        &mut a,
        json!({ "page": 1, "kind": "Count", "points": [[10, 10], [20, 10], [30, 10]] }),
    );
    let v = call(&mut a, "count_edit", json!({ "id": c, "add": [[40, 10], [50, 10]] }));
    assert_eq!(v["result"]["items"], 5);
    let v = call(&mut a, "count_edit", json!({ "id": c, "delete_item": 1 }));
    assert_eq!(v["result"]["items"], 4);
    let v = call(&mut a, "count_edit", json!({ "id": c, "split": [3, 4] }));
    let new_id = v["result"]["new_id"].as_str().unwrap().to_string();
    assert_eq!(markup(&mut a, &c)["quantity"], 2.0);
    assert_eq!(markup(&mut a, &new_id)["quantity"], 2.0);
    call(&mut a, "markup_edit", json!({ "ids": [new_id], "status": "Completed" }));
    assert_eq!(
        markup(&mut a, &new_id)["status"],
        "Completed",
        "a split count takes its own status"
    );
    let v = call(&mut a, "count_edit", json!({ "id": c, "merge": [new_id] }));
    assert_eq!(v["result"]["items"], 4);
    assert_eq!(v["counts"].as_array().unwrap().len(), 1);
    let e = fails(&mut a, "count_edit", json!({ "id": c, "split": [1, 2, 3, 4] }));
    assert!(e.contains("at least one item"), "{e}");
}

#[test]
fn lasso_arcs_cutouts_and_recalculate() {
    let (_d, mut a) = setup("takeoff.pdf");
    let inside = add(
        &mut a,
        json!({ "page": 1, "kind": "Line", "points": [[110, 110], [150, 150]] }),
    );
    let _outside = add(
        &mut a,
        json!({ "page": 1, "kind": "Line", "points": [[110, 110], [400, 150]] }),
    );
    let v = call(
        &mut a,
        "select_lasso",
        json!({ "page": 1, "points": [[100, 100], [200, 100], [200, 200], [100, 200]] }),
    );
    assert_eq!(v["selection"], json!([inside]));

    let pl = add(
        &mut a,
        json!({ "page": 1, "kind": "Polylength", "points": [[100, 400], [190, 400]] }),
    );
    let before = markup(&mut a, &pl)["quantity"].as_f64().unwrap();
    let v = call(&mut a, "arc_edit", json!({ "id": pl, "convert": 1 }));
    assert_eq!(v["arcs"], 1);
    let after = v["markup"]["quantity"].as_f64().unwrap();
    assert!(after > before, "an arc is longer than its chord: {after} > {before}");
    let v = call(&mut a, "arc_edit", json!({ "id": pl, "straighten": 1 }));
    assert_eq!(v["arcs"], 0);
    assert!((v["markup"]["quantity"].as_f64().unwrap() - before).abs() < 1e-9);

    let area = add(
        &mut a,
        json!({ "page": 1, "kind": "Area", "points": [[300, 300], [390, 300], [390, 390], [300, 390]] }),
    );
    call(
        &mut a,
        "cutout_add",
        json!({ "id": area, "points": [[310, 310], [319, 310], [319, 319], [310, 319]] }),
    );
    let v = call(&mut a, "cutout_to_measurement", json!({ "id": area, "index": 1 }));
    assert_eq!(v["markup"]["kind"], "Area");
    assert_eq!(v["markup"]["quantity_text"], "1 sf");
    assert_eq!(markup(&mut a, &area)["quantity_text"], "99 sf", "the hole stays");

    // A new page scale: Recalculate pushes it onto the measurements.
    call(
        &mut a,
        "scale_set",
        json!({ "scale": { "kind": "architectural", "paper_inches": 0.25, "real_feet": 1 } }),
    );
    assert_eq!(markup(&mut a, &area)["quantity_text"], "99 sf");
    let v = call(&mut a, "measure_recalculate", json!({}));
    assert!(v["changed"].as_u64().unwrap() >= 2);
    assert_eq!(markup(&mut a, &area)["quantity_text"], "24.75 sf");
}

#[test]
fn unflatten_after_save_and_import_markups_from_a_pdf() {
    let (d, mut a) = setup("flat.pdf");
    let keep = add(
        &mut a,
        json!({ "page": 1, "kind": "Line", "points": [[10, 10], [90, 10]] }),
    );
    let gone = add(
        &mut a,
        json!({ "page": 1, "kind": "Rectangle", "points": [[100, 100], [200, 160]], "subject": "Burned" }),
    );
    call(&mut a, "markup_flatten", json!({ "ids": [gone] }));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "flat.pdf" }));
    let list = call(&mut a, "markup_list", json!({}));
    assert_eq!(list["count"], 1, "flattened");
    let v = call(&mut a, "unflatten", json!({}));
    assert_eq!(v["unflattened"], 1);
    let list = call(&mut a, "markup_list", json!({}));
    assert_eq!(list["count"], 2);
    assert!(
        list["markups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["subject"] == "Burned")
    );
    let e = fails(&mut a, "unflatten", json!({}));
    assert!(e.contains("flattened by MarkupCraft"), "{e}");
    call(&mut a, "doc_save", json!({}));

    // Import Markups from that PDF into a new document: same pages, new annotations.
    call(&mut a, "doc_new", json!({ "path": "into.pdf", "pages": 1 }));
    let v = call(&mut a, "markup_import_pdf", json!({ "path": "flat.pdf" }));
    assert_eq!(v["imported"], 2);
    let list = call(&mut a, "markup_list", json!({}));
    assert_eq!(list["count"], 2);
    assert!(list["markups"].as_array().unwrap().iter().any(|m| m["id"] == keep));
    // Importing again: the ids are taken, so the copies get new ones.
    call(&mut a, "markup_import_pdf", json!({ "path": "flat.pdf" }));
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], 4);
    drop(d);
}

#[test]
fn file_attachment_markup_embeds_and_gives_back_its_file() {
    let (d, mut a) = setup("attach.pdf");
    std::fs::write(d.join("spec.txt"), b"section 23 05 00").unwrap();
    let v = call(
        &mut a,
        "markup_attach_file",
        json!({ "page": 1, "point": [100, 700], "path": "spec.txt", "icon": "Paperclip" }),
    );
    let id = v["id"].as_str().unwrap().to_string();
    assert_eq!(v["markup"]["kind"], "File Attachment");
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "attach.pdf" }));
    let m = markup(&mut a, &id);
    assert_eq!(m["kind"], "File Attachment");
    let out = call(&mut a, "markup_attachment_save", json!({ "id": id, "out": "back.txt" }));
    assert_eq!(out["name"], "spec.txt");
    assert_eq!(std::fs::read(d.join("back.txt")).unwrap(), b"section 23 05 00");
    let e = fails(
        &mut a,
        "markup_attach_file",
        json!({ "page": 1, "point": [0, 0], "path": "missing.txt" }),
    );
    assert!(e.contains("missing.txt"), "{e}");
}

#[test]
fn replies_and_a_linked_summary() {
    let (_d, mut a) = setup("replies.pdf");
    let id = add(
        &mut a,
        json!({ "page": 1, "kind": "Line", "points": [[10, 10], [90, 10]], "subject": "Pipe" }),
    );
    call(&mut a, "markup_reply", json!({ "id": id, "add": "Check the slope" }));
    let v = call(&mut a, "markup_reply", json!({ "id": id, "add": "Done" }));
    assert_eq!(v["replies"].as_array().unwrap().len(), 2);
    assert_eq!(v["replies"][0]["author"], "Tester");
    call(
        &mut a,
        "markup_reply",
        json!({ "id": id, "edit": { "index": 2, "text": "Done, 2%" } }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "replies.pdf" }));
    let v = call(&mut a, "markup_reply", json!({ "id": id, "delete": 1 }));
    assert_eq!(
        v["replies"],
        json!([{ "author": "Tester", "date": v["replies"][0]["date"], "text": "Done, 2%" }])
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "replies.pdf" }));
    let v = call(&mut a, "markup_reply", json!({ "id": id, "add": "Third" }));
    assert_eq!(
        v["replies"].as_array().unwrap().len(),
        2,
        "the deleted reply stays deleted"
    );
    // A summary appended at the end, each row linked to its markup's page.
    let v = call(&mut a, "summary_append", json!({ "title": "Review" }));
    assert_eq!((v["rows"].as_u64(), v["pages"].as_u64()), (Some(1), Some(2)));
    let links = call(&mut a, "link_list", json!({}));
    assert!(links.to_string().contains("\"page\":2"), "{links}");
}

#[test]
fn a_temporary_scale_is_not_saved() {
    let (_d, mut a) = setup("temp.pdf");
    let len = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[0, 100], [90, 100]] }),
    );
    call(&mut a, "doc_save", json!({}));
    assert_eq!(markup(&mut a, &len)["quantity_text"], "10'-0\"");
    let v = call(
        &mut a,
        "scale_set_temporary",
        json!({ "scale": { "kind": "architectural", "paper_inches": 0.25, "real_feet": 1 }, "apply_to_markups": true }),
    );
    assert_eq!(v["markups_updated"], 1);
    assert_eq!(markup(&mut a, &len)["quantity_text"], "5'-0\"");
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "temp.pdf" }));
    let info = call(&mut a, "doc_info", json!({}));
    assert!(info.to_string().contains("0.125"), "the file kept its scale: {info}");
}

#[test]
fn slope_caption_and_centroid_properties() {
    let (_d, mut a) = setup("slope.pdf");
    let area = add(
        &mut a,
        json!({ "page": 1, "kind": "Area", "points": [[100, 100], [190, 100], [190, 190], [100, 190]], "subject": "Roof" }),
    );
    let v = call(
        &mut a,
        "measure_props_set",
        json!({ "ids": [area], "slope_type": "pitch", "slope": 12, "caption": "{subject}: {value}", "caption_leader": true, "show_centroid": true }),
    );
    let m = &v["markups"][0];
    assert_eq!(m["quantity_text"], "141.42 sf");
    assert_eq!(m["caption"], "Roof: 141.42 sf");
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "slope.pdf" }));
    assert_eq!(markup(&mut a, &area)["quantity_text"], "141.42 sf");
    let e = fails(
        &mut a,
        "measure_props_set",
        json!({ "ids": [area], "slope_type": "degrees", "slope": 95 }),
    );
    assert!(e.contains("slope"), "{e}");
}
