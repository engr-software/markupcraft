//! End-to-end tests of the takeoff editing tools (cutouts, Markups List cells, custom columns)
//! through `Automation::call`, on PDFs built here.

use std::path::PathBuf;

use markupcraft_automation::Automation;
use serde_json::{Value, json};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("markupcraft-takeoff-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
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

fn square(x: f64, y: f64, s: f64) -> Value {
    json!([[x, y], [x + s, y], [x + s, y + s], [x, y + s]])
}

/// A one-page document at 1/8" = 1' with one 90 x 90 point Area (100 sf).
fn area_doc(name: &str) -> (Automation, String) {
    let d = dir(name);
    let mut a = Automation::new().with_root(&d).unwrap().with_author("Tester");
    call(&mut a, "doc_new", json!({ "path": "takeoff.pdf", "pages": 1 }));
    call(
        &mut a,
        "scale_set",
        json!({ "scale": { "kind": "architectural", "paper_inches": 0.125, "real_feet": 1.0 } }),
    );
    let id = call(
        &mut a,
        "markup_add",
        json!({ "kind": "Area", "page": 1, "points": square(100.0, 100.0, 90.0) }),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    (a, id)
}

#[test]
fn cutout_add_and_delete() {
    let (mut a, id) = area_doc("cutout");
    let v = call(
        &mut a,
        "cutout_add",
        json!({ "id": id, "points": square(100.0, 100.0, 45.0) }),
    );
    assert_eq!(v["cutouts"], 1);
    assert_eq!(v["quantity"], "75 sf");
    let e = fails(
        &mut a,
        "cutout_add",
        json!({ "id": id, "points": square(500.0, 500.0, 10.0) }),
    );
    assert!(e.contains("inside"), "{e}");
    let v = call(&mut a, "cutout_delete", json!({ "id": id, "index": 1 }));
    assert_eq!(v["quantity"], "100 sf");
    call(&mut a, "edit_undo", json!({}));
    let list = call(&mut a, "markup_list", json!({}));
    assert!((list["markups"][0]["quantity"].as_f64().unwrap() - 75.0).abs() < 1e-6);
}

#[test]
fn custom_columns_and_cells() {
    let (mut a, id) = area_doc("columns");
    let v = call(
        &mut a,
        "columns_set",
        json!({ "columns": [
            { "name": "Unit Cost", "type": "Currency" },
            { "name": "Cost", "type": "Formula", "formula": "Measurement * Unit Cost" },
            { "name": "Phase", "type": "Choice", "items": ["One", "Two"] }
        ] }),
    );
    assert_eq!(v["columns"], json!(["c:unitcost", "c:cost", "c:phase"]));
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "c:unitcost", "value": "2.5" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "c:phase", "value": "Two" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "status", "value": "Accepted" }),
    );
    let e = fails(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "c:phase", "value": "Three" }),
    );
    assert!(e.contains("does not take"), "{e}");
    let e = fails(
        &mut a,
        "columns_set",
        json!({ "columns": [{ "name": "X", "type": "Colour" }] }),
    );
    assert!(e.contains("not one of"), "{e}");
    // The list (CSV) shows the values and the formula.
    let csv = call(&mut a, "export_csv", json!({}));
    let text = csv.to_string();
    assert!(text.contains("Accepted"), "{text}");
}
