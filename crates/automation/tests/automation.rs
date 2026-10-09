//! End-to-end tests: every tool through `Automation::call`, on PDFs built here.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::mcp::McpServer;
use markupcraft_automation::{Automation, TOOLS, ToolError};
use serde_json::{Value, json};

/// A fresh folder per test.
fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-automation-{}-{}",
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

/// A saved document with `pages` letter pages, open in `a`.
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
    let list = call(a, "markup_list", json!({}));
    list["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no markup {id}"))
}

fn ids_on(a: &mut Automation, page: u64) -> Vec<String> {
    let list = call(a, "markup_list", json!({ "page": page }));
    list["markups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn tool_table_is_well_formed() {
    let mut names = std::collections::HashSet::new();
    for t in TOOLS {
        assert!(t.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{}", t.name);
        assert!(names.insert(t.name), "duplicate {}", t.name);
        assert!(!t.description.is_empty() && !t.title.is_empty());
        let s = (t.schema)();
        assert_eq!(s["type"], "object", "{}", t.name);
        assert_eq!(s["additionalProperties"], false, "{}", t.name);
        let props = s["properties"].as_object().unwrap();
        for r in s["required"].as_array().unwrap() {
            assert!(
                props.contains_key(r.as_str().unwrap()),
                "{}: required {r} not a property",
                t.name
            );
        }
    }
    let listed = markupcraft_automation::tools::list_json();
    assert_eq!(listed.as_array().unwrap().len(), TOOLS.len());
}

#[test]
fn unknown_tool_and_arguments_are_refused() {
    let d = dir();
    let mut a = auto(&d);
    assert!(matches!(a.call("nope", &json!({})), Err(ToolError::UnknownTool(_))));
    let e = a.call("doc_new", &json!({ "path": "x.pdf", "colour": 1 })).unwrap_err();
    assert!(
        matches!(e, ToolError::InvalidArgs(_)) && e.to_string().contains("unknown argument \"colour\""),
        "{e}"
    );
    let e = fails(&mut a, "doc_new", json!({}));
    assert!(e.contains("missing argument path"), "{e}");
    let e = fails(&mut a, "markup_list", json!({}));
    assert!(e.contains("doc_open"), "no document yet: {e}");
    assert!(a.call("doc_new", &json!("not an object")).is_err());
}

#[test]
fn doc_new_save_open_list_info_close() {
    let d = dir();
    let mut a = auto(&d);
    let v = call(
        &mut a,
        "doc_new",
        json!({ "path": "new.pdf", "pages": 2, "width": 1224, "height": 792 }),
    );
    assert_eq!((v["pages"].as_u64(), v["dirty"].as_bool()), (Some(2), Some(true)));
    add(
        &mut a,
        json!({ "page": 2, "kind": "Line", "points": [[0, 0], [100, 0]] }),
    );
    let saved = call(&mut a, "doc_save", json!({}));
    assert_eq!(saved["dirty"], false);
    assert!(d.join("new.pdf").exists());
    let v = call(&mut a, "doc_open", json!({ "path": "new.pdf" }));
    assert_eq!((v["doc"].as_u64(), v["markups"].as_u64()), (Some(2), Some(1)));
    let list = call(&mut a, "doc_list", json!({}));
    assert_eq!(list["documents"].as_array().unwrap().len(), 2);
    let info = call(&mut a, "doc_info", json!({ "doc": 2 }));
    assert_eq!(info["page_list"][0]["width"], 1224.0);
    assert_eq!(info["kinds"]["Line"], 1);
    assert_eq!(info["subjects"]["Line"], 1);
    // Save as, full.
    call(
        &mut a,
        "doc_save",
        json!({ "doc": 2, "path": "copy.pdf", "full": true }),
    );
    assert!(d.join("copy.pdf").exists());
    call(&mut a, "doc_close", json!({ "doc": 2 }));
    assert!(fails(&mut a, "doc_info", json!({ "doc": 2 })).contains("no open document 2"));
}

#[test]
fn doc_close_refuses_unsaved_changes() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "c.pdf", 1);
    add(
        &mut a,
        json!({ "page": 1, "kind": "Line", "points": [[0, 0], [10, 0]] }),
    );
    let e = fails(&mut a, "doc_close", json!({}));
    assert!(e.contains("unsaved") && e.contains("discard_changes"), "{e}");
    call(&mut a, "doc_close", json!({ "discard_changes": true }));
}

#[test]
fn root_confines_paths() {
    let d = dir();
    let mut a = auto(&d);
    let e = fails(&mut a, "doc_new", json!({ "path": "../escape.pdf" }));
    assert!(e.contains("outside the allowed directory"), "{e}");
    let outside = std::env::temp_dir().join("markupcraft-outside.pdf");
    let e = fails(&mut a, "doc_open", json!({ "path": outside.display().to_string() }));
    assert!(e.contains("outside the allowed directory"), "{e}");
    new_doc(&mut a, "in.pdf", 1);
    let e = fails(&mut a, "doc_save", json!({ "path": "../out.pdf" }));
    assert!(e.contains("outside"), "{e}");
    let e = fails(&mut a, "export_csv", json!({ "out": "../x.csv" }));
    assert!(e.contains("outside"), "{e}");
    let e = fails(&mut a, "doc_open", json!({ "path": "missing.pdf" }));
    assert!(
        !e.contains("outside"),
        "missing inside the root is reported as missing: {e}"
    );
}

#[test]
fn markup_add_every_creatable_kind() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "kinds.pdf", 1);
    call(
        &mut a,
        "scale_set",
        json!({ "scale": { "kind": "architectural", "paper_inches": 0.125, "real_feet": 1 } }),
    );
    let cases = [
        ("Area", square(0.0, 0.0, 90.0), Some("100 sf")),
        ("Perimeter", square(100.0, 0.0, 9.0), Some("4'-0\"")),
        ("Length", json!([[0, 200], [9, 200]]), Some("1'-0\"")),
        ("Polylength", json!([[0, 300], [9, 300], [9, 309]]), Some("2'-0\"")),
        ("Count", json!([[10, 400], [20, 400], [30, 400]]), None),
        ("Polygon", square(200.0, 200.0, 20.0), None),
        ("Polyline", json!([[0, 0], [5, 5], [10, 0]]), None),
        ("Line", json!([[0, 0], [5, 5]]), None),
        ("Rectangle", json!([[300, 300], [350, 320]]), None),
        ("Ellipse", json!([[400, 300], [450, 320]]), None),
        ("Pen", json!([[0, 0], [1, 1], [2, 0]]), None),
    ];
    for (kind, pts, text) in cases {
        let v = call(
            &mut a,
            "markup_add",
            json!({ "page": 1, "kind": kind, "points": pts, "color": "#0000FF" }),
        );
        let m = &v["markup"];
        assert_eq!(m["color"], "#0000FF", "{kind}");
        if let Some(t) = text {
            assert_eq!(m["quantity_text"], t, "{kind}");
        }
    }
    let rect = &call(&mut a, "markup_list", json!({ "kind": "Rectangle" }))["markups"][0];
    assert_eq!(rect["points"].as_array().unwrap().len(), 4, "two corners become a box");
    let count = &call(&mut a, "markup_list", json!({ "kind": "Count" }))["markups"][0];
    assert_eq!(count["quantity"], 3.0);
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "kinds.pdf" }));
    let back = call(&mut a, "markup_list", json!({}));
    assert_eq!(back["count"], 11);
    let area = back["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["kind"] == "Area")
        .unwrap();
    assert_eq!(area["contents"], "100 sf", "Revu's /Contents is written on save");
}

#[test]
fn markup_add_explains_what_it_cannot_do() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "k.pdf", 1);
    let e = fails(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Hyperlink", "points": [[0, 0]] }),
    );
    assert!(e.contains("cannot be created yet") && e.contains("Area"), "{e}");
    let e = fails(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Blob", "points": [[0, 0]] }),
    );
    assert!(e.contains("unknown kind") && e.contains("Polygon"), "{e}");
    let e = fails(
        &mut a,
        "markup_add",
        json!({ "page": 2, "kind": "Line", "points": [[0, 0], [1, 1]] }),
    );
    assert!(e.contains("page 2 does not exist (the document has 1 pages)"), "{e}");
    let e = fails(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Area", "points": [[0, 0], [1, 1]] }),
    );
    assert!(e.contains("at least 3 points"), "{e}");
    let e = fails(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Line", "points": [[0, 0], [1, 1]], "opacity": 3 }),
    );
    assert!(e.contains("opacity must be 0 to 1"), "{e}");
    let e = fails(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Line", "points": [[0, 0], [1, 1]], "color": "#XYZ" }),
    );
    assert!(e.contains("colour"), "{e}");
}

#[test]
fn markup_list_filters() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "l.pdf", 2);
    add(
        &mut a,
        json!({ "page": 1, "kind": "Line", "points": [[0, 0], [1, 1]], "subject": "Wall" }),
    );
    add(&mut a, json!({ "page": 2, "kind": "Line", "points": [[0, 0], [1, 1]] }));
    add(
        &mut a,
        json!({ "page": 2, "kind": "Polygon", "points": square(0.0, 0.0, 5.0) }),
    );
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], 3);
    assert_eq!(call(&mut a, "markup_list", json!({ "page": 2 }))["count"], 2);
    assert_eq!(call(&mut a, "markup_list", json!({ "kind": "line" }))["count"], 2);
    assert_eq!(call(&mut a, "markup_list", json!({ "subject": "Wall" }))["count"], 1);
    assert_eq!(call(&mut a, "markup_list", json!({ "limit": 1 }))["count"], 1);
    assert!(fails(&mut a, "markup_list", json!({ "page": 9 })).contains("page 9"));
}

#[test]
fn markup_edit_properties_and_lock() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "e.pdf", 1);
    let id = add(
        &mut a,
        json!({ "page": 1, "kind": "Polygon", "points": square(0.0, 0.0, 50.0), "locked": true }),
    );
    assert_eq!(markup(&mut a, &id)["locked"], true);
    let e = fails(&mut a, "markup_edit", json!({ "ids": [id], "color": "red" }));
    assert!(e.contains("locked") && e.contains("unlock"), "{e}");
    let v = call(
        &mut a,
        "markup_edit",
        json!({
            "ids": [id], "locked": false, "color": "green", "fill": "#FFFF00", "opacity": 0.5, "fill_opacity": 0.25,
            "width": 3, "dash": [4, 2], "subject": "Slab", "label": "L1", "contents": "note", "layer": "Concrete",
            "status": "Accepted", "checked": true, "columns": { "cost": 12.5 }
        }),
    );
    assert_eq!(v["changed"], 1);
    let m = markup(&mut a, &id);
    assert_eq!(
        (
            &m["color"],
            &m["fill"],
            &m["width"],
            &m["subject"],
            &m["status"],
            &m["locked"],
            &m["columns"]["cost"]
        ),
        (
            &json!("#008000"),
            &json!("#FFFF00"),
            &json!(3.0),
            &json!("Slab"),
            &json!("Accepted"),
            &json!(false),
            &json!("12.5")
        )
    );
    call(&mut a, "markup_edit", json!({ "ids": [id], "fill": null }));
    assert_eq!(markup(&mut a, &id)["fill"], Value::Null);
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "e.pdf" }));
    let m = markup(&mut a, &id);
    assert_eq!(
        (&m["subject"], &m["label"], &m["width"]),
        (&json!("Slab"), &json!("L1"), &json!(3.0))
    );
}

#[test]
fn markup_transform_moves_rotates_resizes_and_edits_vertices() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "t.pdf", 1);
    let id = add(
        &mut a,
        json!({ "page": 1, "kind": "Polygon", "points": square(0.0, 0.0, 10.0) }),
    );
    call(&mut a, "markup_transform", json!({ "ids": [id], "move": [5, 5] }));
    assert_eq!(markup(&mut a, &id)["points"][0], json!([5.0, 5.0]));
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": [id], "rotate": 90, "center": [5, 5] }),
    );
    assert_eq!(markup(&mut a, &id)["points"][1], json!([5.0, 15.0]));
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": [id], "resize": [0, 0, 20, 40] }),
    );
    let pts = markup(&mut a, &id)["points"].clone();
    let xs: Vec<f64> = pts.as_array().unwrap().iter().map(|p| p[0].as_f64().unwrap()).collect();
    assert_eq!(xs.iter().cloned().fold(f64::MIN, f64::max), 20.0);
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": [id], "points": [[0, 0], [10, 0], [10, 10]] }),
    );
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": [id], "insert_vertex": { "index": 4, "point": [0, 10] } }),
    );
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": [id], "move_vertex": { "index": 1, "point": [-5, -5] } }),
    );
    assert_eq!(markup(&mut a, &id)["points"][0], json!([-5.0, -5.0]));
    call(&mut a, "markup_transform", json!({ "ids": [id], "delete_vertex": 4 }));
    assert_eq!(markup(&mut a, &id)["points"].as_array().unwrap().len(), 3);
    let e = fails(&mut a, "markup_transform", json!({ "ids": [id], "delete_vertex": 1 }));
    assert!(e.contains("at least 3"), "{e}");
    let e = fails(
        &mut a,
        "markup_transform",
        json!({ "ids": [id], "move": [1, 1], "rotate": 3 }),
    );
    assert!(e.contains("exactly one of"), "{e}");
    let e = fails(&mut a, "markup_transform", json!({ "ids": [id] }));
    assert!(e.contains("got none"), "{e}");
}

#[test]
fn markup_delete_and_undo_redo() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "u.pdf", 1);
    let id = add(&mut a, json!({ "page": 1, "kind": "Line", "points": [[0, 0], [9, 9]] }));
    let v = call(&mut a, "markup_delete", json!({ "ids": [id] }));
    assert_eq!(
        (v["deleted"].as_u64(), v["document"]["markups"].as_u64()),
        (Some(1), Some(0))
    );
    let v = call(&mut a, "edit_undo", json!({}));
    assert_eq!(
        (v["undone"].as_str(), v["document"]["markups"].as_u64()),
        (Some("Delete"), Some(1))
    );
    let v = call(&mut a, "edit_redo", json!({}));
    assert_eq!(v["document"]["markups"], 0);
    call(&mut a, "edit_undo", json!({}));
    call(&mut a, "edit_undo", json!({}));
    assert!(fails(&mut a, "edit_undo", json!({})).contains("nothing to undo"));
    call(&mut a, "edit_redo", json!({}));
    let locked = add(
        &mut a,
        json!({ "page": 1, "kind": "Line", "points": [[0, 0], [9, 9]], "locked": true }),
    );
    assert!(fails(&mut a, "markup_delete", json!({ "ids": [locked] })).contains("locked"));
    let v = call(
        &mut a,
        "markup_delete",
        json!({ "ids": [id, locked], "skip_locked": true }),
    );
    assert_eq!(v["deleted"], 1);
    assert!(fails(&mut a, "markup_delete", json!({ "ids": ["NOSUCHID"] })).contains("markup_list shows the ids"));
}

#[test]
fn markup_duplicate_offsets_copies() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "dup.pdf", 1);
    let id = add(
        &mut a,
        json!({ "page": 1, "kind": "Line", "points": [[0, 0], [9, 9]], "status": "Accepted" }),
    );
    let v = call(&mut a, "markup_duplicate", json!({ "ids": [id], "offset": [100, 0] }));
    let new = v["ids"][0].as_str().unwrap().to_string();
    let m = markup(&mut a, &new);
    assert_eq!(m["points"][0], json!([100.0, 0.0]));
    assert_eq!(m["status"], "", "copies start without review state");
    // Default offset, on the selection (the copy).
    let v = call(&mut a, "markup_duplicate", json!({}));
    assert_eq!(
        markup(&mut a, v["ids"][0].as_str().unwrap())["points"][0],
        json!([110.0, -10.0])
    );
}

#[test]
fn markup_arrange_changes_and_saves_z_order() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "z.pdf", 1);
    let x = add(&mut a, json!({ "page": 1, "kind": "Line", "points": [[0, 0], [9, 9]] }));
    let y = add(&mut a, json!({ "page": 1, "kind": "Line", "points": [[0, 0], [9, 9]] }));
    assert_eq!(
        call(&mut a, "markup_arrange", json!({ "ids": [x], "order": "front" }))["moved"],
        true
    );
    assert_eq!(
        call(&mut a, "markup_arrange", json!({ "ids": [x], "order": "forward" }))["moved"],
        false
    );
    assert_eq!(ids_on(&mut a, 1), [y.clone(), x.clone()]);
    call(&mut a, "markup_arrange", json!({ "ids": [x], "order": "back" }));
    call(&mut a, "markup_arrange", json!({ "ids": [x], "order": "forward" }));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "z.pdf" }));
    assert_eq!(ids_on(&mut a, 1), [y, x]);
    assert!(fails(&mut a, "markup_arrange", json!({ "order": "sideways", "ids": [] })).contains("order must be"));
}

#[test]
fn markup_copy_paste_across_pages_and_documents() {
    let d = dir();
    let mut a = auto(&d);
    let one = new_doc(&mut a, "one.pdf", 2);
    let id = add(
        &mut a,
        json!({ "page": 1, "kind": "Polygon", "points": square(10.0, 10.0, 20.0) }),
    );
    assert!(fails(&mut a, "markup_paste", json!({})).contains("copy some markups first"));
    assert_eq!(call(&mut a, "markup_copy", json!({ "ids": [id] }))["copied"], 1);
    let v = call(&mut a, "markup_paste", json!({ "page": 2 }));
    let pasted = v["ids"][0].as_str().unwrap().to_string();
    let m = markup(&mut a, &pasted);
    assert_eq!((m["page"].as_u64(), &m["points"][0]), (Some(2), &json!([10.0, 10.0])));
    let v = call(&mut a, "markup_paste", json!({ "page": 1, "at": [300, 300] }));
    assert_eq!(
        markup(&mut a, v["ids"][0].as_str().unwrap())["points"][0],
        json!([290.0, 290.0])
    );
    // Into another document; then cut.
    let two = new_doc(&mut a, "two.pdf", 1);
    call(&mut a, "markup_paste", json!({ "doc": two, "page": 1 }));
    assert_eq!(call(&mut a, "markup_list", json!({ "doc": two }))["count"], 1);
    let v = call(&mut a, "markup_copy", json!({ "doc": one, "ids": [id], "cut": true }));
    assert_eq!(v["cut"], 1);
    assert_eq!(call(&mut a, "markup_list", json!({ "doc": one }))["count"], 2);
}

#[test]
fn markup_group_and_ungroup() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "g.pdf", 2);
    let x = add(&mut a, json!({ "page": 1, "kind": "Line", "points": [[0, 0], [9, 9]] }));
    let y = add(&mut a, json!({ "page": 1, "kind": "Line", "points": [[0, 0], [9, 9]] }));
    let z = add(&mut a, json!({ "page": 2, "kind": "Line", "points": [[0, 0], [9, 9]] }));
    assert!(fails(&mut a, "markup_group", json!({ "ids": [x, z] })).contains("same page"));
    assert!(fails(&mut a, "markup_group", json!({ "ids": [x] })).contains("at least two"));
    let g = call(&mut a, "markup_group", json!({ "ids": [x, y] }))["group"].clone();
    assert_eq!(g, json!(x));
    assert_eq!(markup(&mut a, &y)["group"], g);
    assert_eq!(call(&mut a, "markup_ungroup", json!({ "ids": [y] }))["ungrouped"], 2);
    assert_eq!(markup(&mut a, &x)["group"], "");
}

#[test]
fn markup_select_feeds_tools_without_ids() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "s.pdf", 2);
    let x = add(&mut a, json!({ "page": 1, "kind": "Line", "points": [[0, 0], [9, 9]] }));
    add(&mut a, json!({ "page": 2, "kind": "Line", "points": [[0, 0], [9, 9]] }));
    let v = call(&mut a, "markup_select", json!({ "all": true, "page": 2 }));
    assert_eq!(v["selection"].as_array().unwrap().len(), 1);
    call(&mut a, "markup_select", json!({ "ids": [x] }));
    call(&mut a, "markup_edit", json!({ "subject": "Selected" }));
    assert_eq!(markup(&mut a, &x)["subject"], "Selected");
    call(&mut a, "markup_select", json!({ "ids": [] }));
    assert!(fails(&mut a, "markup_edit", json!({ "subject": "x" })).contains("markup_select"));
    assert!(fails(&mut a, "markup_select", json!({ "ids": ["NOPE"] })).contains("NOPE"));
    assert!(fails(&mut a, "markup_select", json!({})).contains("either ids or all"));
}

#[test]
fn scale_set_measure_and_apply_to_markups() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "sc.pdf", 2);
    let e = fails(&mut a, "measure", json!({ "page": 1, "points": [[0, 0], [72, 0]] }));
    assert!(e.contains("no scale") && e.contains("scale_calibrate"), "{e}");
    let v = call(
        &mut a,
        "measure",
        json!({ "page": 1, "kind": "Count", "points": [[0, 0], [1, 1], [2, 2]] }),
    );
    assert_eq!(v["value"], 3.0);
    let id = add(
        &mut a,
        json!({ "page": 2, "kind": "Length", "points": [[0, 0], [72, 0]] }),
    );
    assert_eq!(markup(&mut a, &id)["quantity"], Value::Null);
    let v = call(
        &mut a,
        "scale_set",
        json!({ "pages": "2", "scale": { "kind": "engineering", "feet_per_inch": 20 }, "apply_to_markups": true }),
    );
    assert_eq!(v["markups_updated"], 1);
    assert_eq!(markup(&mut a, &id)["quantity_text"], "20 ft");
    let v = call(
        &mut a,
        "measure",
        json!({ "page": 2, "kind": "Area", "points": square(0.0, 0.0, 72.0) }),
    );
    assert_eq!((v["value"].as_f64(), v["unit"].as_str()), (Some(400.0), Some("sf")));
    // An explicit scale, metric.
    let v = call(
        &mut a,
        "measure",
        json!({ "page": 1, "points": [[0, 0], [72, 0]], "scale": { "kind": "ratio", "ratio": 100, "unit": "m" } }),
    );
    assert!((v["value"].as_f64().unwrap() - 2.54).abs() < 1e-9, "{v}");
    assert!(fails(&mut a, "scale_set", json!({ "scale": { "kind": "huge" } })).contains("architectural"));
    // The scale is in the file.
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "sc.pdf" }));
    assert_eq!(
        call(&mut a, "measure", json!({ "page": 2, "points": [[0, 0], [36, 0]] }))["text"],
        "10 ft"
    );
}

#[test]
fn scale_calibrate_from_two_points() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "cal.pdf", 3);
    let v = call(
        &mut a,
        "scale_calibrate",
        json!({ "page": 1, "from": [100, 100], "to": [100, 172], "length": 12, "unit": "in", "pages": [1, 3] }),
    );
    assert_eq!(v["pages"], json!([1, 3]));
    let m = call(&mut a, "measure", json!({ "page": 3, "points": [[0, 0], [144, 0]] }));
    assert_eq!(m["text"], "2'-0\"");
    assert!(fails(&mut a, "measure", json!({ "page": 2, "points": [[0, 0], [1, 0]] })).contains("no scale"));
    let e = fails(
        &mut a,
        "scale_calibrate",
        json!({ "page": 1, "from": [1, 1], "to": [1, 1], "length": 5 }),
    );
    assert!(e.contains("apart"), "{e}");
    let e = fails(
        &mut a,
        "scale_calibrate",
        json!({ "page": 1, "from": [1, 1], "to": [2, 1], "length": 5, "unit": "cubits" }),
    );
    assert!(e.contains("length unit"), "{e}");
    call(
        &mut a,
        "scale_calibrate",
        json!({ "page": 2, "from": [0, 0], "to": [100, 0], "length": 5, "unit": "m" }),
    );
    assert_eq!(
        call(&mut a, "measure", json!({ "page": 2, "points": [[0, 0], [200, 0]] }))["text"],
        "10 m"
    );
}

#[test]
fn viewport_add_and_delete() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "vp.pdf", 1);
    call(
        &mut a,
        "scale_set",
        json!({ "scale": { "kind": "engineering", "feet_per_inch": 10 } }),
    );
    call(
        &mut a,
        "viewport_add",
        json!({ "page": 1, "box": [300, 300, 400, 400], "name": "Detail 1", "scale": { "kind": "engineering", "feet_per_inch": 1 } }),
    );
    assert_eq!(
        call(
            &mut a,
            "measure",
            json!({ "page": 1, "points": [[310, 310], [382, 310]] })
        )["text"],
        "1 ft"
    );
    assert_eq!(
        call(&mut a, "measure", json!({ "page": 1, "points": [[0, 0], [72, 0]] }))["text"],
        "10 ft"
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "vp.pdf" }));
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(info["page_list"][0]["viewports"][0]["name"], "Detail 1");
    call(&mut a, "viewport_delete", json!({ "page": 1, "index": 1 }));
    assert_eq!(
        call(
            &mut a,
            "measure",
            json!({ "page": 1, "points": [[310, 310], [382, 310]] })
        )["text"],
        "10 ft"
    );
    assert!(fails(&mut a, "viewport_delete", json!({ "page": 1, "index": 5 })).contains("no viewport 5"));
    let e = fails(
        &mut a,
        "viewport_add",
        json!({ "page": 1, "box": [0, 0, 0, 10], "scale": { "kind": "engineering", "feet_per_inch": 1 } }),
    );
    assert!(e.contains("positive width"), "{e}");
}

/// A document with a markup per page and page labels "i", "ii", "1", "2" ...
fn labelled(a: &mut Automation, d: &Path, name: &str, pages: u64) -> Vec<String> {
    new_doc(a, name, pages);
    let ids: Vec<String> = (1..=pages)
        .map(|p| {
            add(
                a,
                json!({ "page": p, "kind": "Polygon", "points": square(10.0 * p as f64, 10.0, 20.0) }),
            )
        })
        .collect();
    call(a, "doc_save", json!({ "full": true }));
    call(a, "doc_close", json!({}));
    use markupcraft_revu::cos::{Dict, Document, Object, SaveOptions, write_full};
    let path = d.join(name);
    let mut cos = Document::open(std::sync::Arc::new(std::fs::read(&path).unwrap())).unwrap();
    let mut roman = Dict::new();
    roman.set(b"S".to_vec(), Object::name("r"));
    let mut dec = Dict::new();
    dec.set(b"S".to_vec(), Object::name("D"));
    let mut tree = Dict::new();
    tree.set(
        b"Nums".to_vec(),
        Object::Array(vec![
            Object::Int(0),
            Object::Dict(roman),
            Object::Int(2),
            Object::Dict(dec),
        ]),
    );
    let root = cos.root().unwrap();
    cos.update_dict(root, |d| d.set(b"PageLabels".to_vec(), Object::Dict(tree)))
        .unwrap();
    std::fs::write(&path, write_full(&cos, &SaveOptions::default()).unwrap()).unwrap();
    call(a, "doc_open", json!({ "path": name }));
    ids
}

#[test]
fn page_labels_lists_labels() {
    let d = dir();
    let mut a = auto(&d);
    labelled(&mut a, &d, "lab.pdf", 4);
    assert_eq!(
        call(&mut a, "page_labels", json!({}))["labels"],
        json!(["i", "ii", "1", "2"])
    );
    new_doc(&mut a, "plain.pdf", 2);
    assert_eq!(call(&mut a, "page_labels", json!({}))["labels"], json!(["", ""]));
}

#[test]
fn page_rotate_keeps_markups() {
    let d = dir();
    let mut a = auto(&d);
    let ids = labelled(&mut a, &d, "rot.pdf", 3);
    let v = call(&mut a, "page_rotate", json!({ "pages": [1, 3], "degrees": 90 }));
    assert_eq!(v["report"]["markups_after"], 3);
    let info = call(&mut a, "doc_info", json!({}));
    let rot: Vec<i64> = (0..3)
        .map(|i| info["page_list"][i]["rotate"].as_i64().unwrap())
        .collect();
    assert_eq!(rot, [90, 0, 90]);
    call(&mut a, "page_rotate", json!({ "degrees": -90 }));
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(info["page_list"][1]["rotate"], 270);
    assert!(fails(&mut a, "page_rotate", json!({ "degrees": 45 })).contains("multiple of 90"));
    assert_eq!(markup(&mut a, &ids[2])["page"], 3);
}

#[test]
fn page_delete_removes_its_markups() {
    let d = dir();
    let mut a = auto(&d);
    let ids = labelled(&mut a, &d, "del.pdf", 4);
    let v = call(&mut a, "page_delete", json!({ "pages": "2-3" }));
    assert_eq!(
        (
            v["report"]["pages_after"].as_u64(),
            v["report"]["markups_after"].as_u64()
        ),
        (Some(2), Some(2))
    );
    assert_eq!(call(&mut a, "page_labels", json!({}))["labels"], json!(["i", "2"]));
    assert_eq!(markup(&mut a, &ids[3])["page"], 2);
    assert!(fails(&mut a, "page_delete", json!({ "pages": "1-2" })).contains("at least one page"));
    assert!(fails(&mut a, "page_delete", json!({ "pages": [7] })).contains("page 7"));
    call(&mut a, "edit_undo", json!({}));
    assert_eq!(call(&mut a, "doc_info", json!({}))["pages"], 4);
    call(&mut a, "edit_redo", json!({}));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "del.pdf" }));
    assert_eq!(call(&mut a, "doc_info", json!({}))["pages"], 2);
}

#[test]
fn page_move_reorders_with_markups() {
    let d = dir();
    let mut a = auto(&d);
    let ids = labelled(&mut a, &d, "mv.pdf", 4);
    call(&mut a, "page_move", json!({ "pages": [3, 4], "before": 1 }));
    assert_eq!(
        call(&mut a, "page_labels", json!({}))["labels"],
        json!(["1", "2", "i", "ii"])
    );
    assert_eq!(markup(&mut a, &ids[0])["page"], 3);
    assert_eq!(markup(&mut a, &ids[3])["page"], 2);
    call(&mut a, "page_move", json!({ "pages": [1], "before": 5 }));
    assert_eq!(
        call(&mut a, "page_labels", json!({}))["labels"],
        json!(["2", "i", "ii", "1"])
    );
    assert!(fails(&mut a, "page_move", json!({ "pages": [1], "before": 9 })).contains("past the end"));
}

#[test]
fn page_insert_blank_pages() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "blank.pdf", 2);
    let v = call(
        &mut a,
        "page_insert_blank",
        json!({ "at": 2, "count": 2, "width": 1224, "height": 792 }),
    );
    assert_eq!(v["report"]["pages_after"], 4);
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(info["page_list"][1]["width"], 1224.0);
    call(&mut a, "page_insert_blank", json!({ "at": 3 }));
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(info["page_list"][2]["width"], 1224.0, "size of the page before");
    assert_eq!(info["pages"], 5);
    assert!(fails(&mut a, "page_insert_blank", json!({ "at": 1, "width": 10 })).contains("both width and height"));
    assert!(
        fails(
            &mut a,
            "page_insert_blank",
            json!({ "at": 1, "width": 0.0, "height": 5 })
        )
        .contains("out of range")
    );
}

#[test]
fn page_insert_file_brings_markups() {
    let d = dir();
    let mut a = auto(&d);
    let src_ids = labelled(&mut a, &d, "src.pdf", 3);
    call(&mut a, "doc_close", json!({}));
    new_doc(&mut a, "dst.pdf", 1);
    add(&mut a, json!({ "page": 1, "kind": "Line", "points": [[0, 0], [5, 5]] }));
    let v = call(
        &mut a,
        "page_insert_file",
        json!({ "path": "src.pdf", "at": 2, "pages": "2-3" }),
    );
    assert_eq!(
        (
            v["report"]["pages_after"].as_u64(),
            v["report"]["markups_after"].as_u64()
        ),
        (Some(3), Some(3))
    );
    assert_eq!(markup(&mut a, &src_ids[1])["page"], 2);
    assert_eq!(
        call(&mut a, "page_labels", json!({}))["labels"],
        json!(["1", "ii", "1"])
    );
    // The same pages again: their ids are already used, so the copies get new ones.
    call(&mut a, "page_insert_file", json!({ "path": "src.pdf", "at": 1 }));
    let list = call(&mut a, "markup_list", json!({}));
    let ids: std::collections::HashSet<&str> = list["markups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 6);
    let e = fails(
        &mut a,
        "page_insert_file",
        json!({ "path": "src.pdf", "at": 1, "pages": [9] }),
    );
    assert!(e.contains("page 9 is not in"), "{e}");
    assert!(fails(&mut a, "page_insert_file", json!({ "path": "nope.pdf", "at": 1 })).contains("nope.pdf"));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "dst.pdf" }));
    assert_eq!(call(&mut a, "doc_info", json!({}))["markups"], 6);
}

#[test]
fn page_extract_writes_a_new_pdf() {
    let d = dir();
    let mut a = auto(&d);
    let ids = labelled(&mut a, &d, "x.pdf", 4);
    let unsaved = add(&mut a, json!({ "page": 4, "kind": "Line", "points": [[0, 0], [5, 5]] }));
    let main = call(&mut a, "doc_info", json!({}))["doc"].clone();
    let v = call(&mut a, "page_extract", json!({ "pages": "3-4", "out": "part.pdf" }));
    assert_eq!(v["pages"], 2);
    assert_eq!(v["document"]["pages"], 4, "without delete the document keeps its pages");
    let v = call(&mut a, "doc_open", json!({ "path": "part.pdf" }));
    assert_eq!((v["pages"].as_u64(), v["markups"].as_u64()), (Some(2), Some(3)));
    assert_eq!(markup(&mut a, &unsaved)["page"], 2);
    assert_eq!(markup(&mut a, &ids[2])["page"], 1);
    assert_eq!(call(&mut a, "page_labels", json!({}))["labels"], json!(["1", "2"]));
    let v = call(
        &mut a,
        "page_extract",
        json!({ "doc": main, "pages": [1], "out": "first.pdf", "delete": true }),
    );
    assert_eq!(v["document"]["pages"], 3);
    assert!(
        fails(
            &mut a,
            "page_extract",
            json!({ "doc": main, "pages": [1], "out": "../x.pdf" })
        )
        .contains("outside")
    );
}

#[test]
fn export_csv_to_file_and_text() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "csv.pdf", 2);
    call(
        &mut a,
        "scale_set",
        json!({ "scale": { "kind": "architectural", "paper_inches": 0.125, "real_feet": 1 } }),
    );
    add(
        &mut a,
        json!({ "page": 1, "kind": "Area", "points": square(0.0, 0.0, 90.0), "subject": "Floor, tile" }),
    );
    add(&mut a, json!({ "page": 2, "kind": "Line", "points": [[0, 0], [5, 5]] }));
    let v = call(&mut a, "export_csv", json!({}));
    assert_eq!(v["rows"], 2);
    let csv = v["csv"].as_str().unwrap();
    assert!(csv.starts_with("Page,Page Label,Id,Type"), "{csv}");
    assert!(csv.contains("\"Floor, tile\"") && csv.contains("\"100 sf\""), "{csv}");
    let v = call(&mut a, "export_csv", json!({ "out": "list.csv", "page": 2 }));
    assert_eq!(v["rows"], 1);
    let text = std::fs::read_to_string(d.join("list.csv")).unwrap();
    assert_eq!(text.lines().count(), 2);
}

#[test]
fn command_list_and_run() {
    let d = dir();
    let mut a = auto(&d);
    let list = call(&mut a, "command_list", json!({}));
    assert!(
        list["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "edit.delete")
    );
    new_doc(&mut a, "cmd.pdf", 1);
    add(&mut a, json!({ "page": 1, "kind": "Line", "points": [[0, 0], [5, 5]] }));
    let v = call(&mut a, "command_run", json!({ "id": "edit.duplicate" }));
    assert_eq!(v["document"]["markups"], 2);
    call(&mut a, "command_run", json!({ "id": "edit.select_all" }));
    call(&mut a, "command_run", json!({ "id": "edit.copy" }));
    let two = new_doc(&mut a, "cmd2.pdf", 1);
    call(
        &mut a,
        "command_run",
        json!({ "doc": two, "id": "edit.paste_in_place" }),
    );
    assert_eq!(
        call(&mut a, "markup_list", json!({ "doc": two }))["count"],
        2,
        "shared clipboard"
    );
    call(&mut a, "command_run", json!({ "id": "edit.select_none" }));
    let e = fails(&mut a, "command_run", json!({ "id": "edit.delete" }));
    assert!(e.contains("select markups first"), "{e}");
    assert!(fails(&mut a, "command_run", json!({ "id": "x.y" })).contains("unknown command"));
    call(&mut a, "command_run", json!({ "id": "file.save" }));
    assert_eq!(call(&mut a, "doc_info", json!({}))["dirty"], false);
}

#[test]
fn mcp_server_lists_and_calls_tools() {
    let d = dir();
    let mut server = McpServer::new(auto(&d));
    let input = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "doc_new", "arguments": { "path": "m.pdf" } } }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "markup_add", "arguments": { "page": 5, "kind": "Line", "points": [[0, 0], [1, 1]] } } }),
        json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "nope" } }),
        json!({ "jsonrpc": "2.0", "id": 6, "method": "nope" }),
    ]
    .iter()
    .map(|v| v.to_string())
    .collect::<Vec<_>>()
    .join("\n")
        + "\nnot json\n";
    let mut out = Vec::new();
    server.serve(input.as_bytes(), &mut out).unwrap();
    let replies: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(replies.len(), 7, "no reply to the notification");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "markupcraft");
    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), TOOLS.len());
    assert!(replies[1]["result"]["tools"][0]["inputSchema"].is_object());
    assert_eq!(replies[2]["result"]["isError"], false);
    assert_eq!(replies[2]["result"]["structuredContent"]["pages"], 1);
    assert_eq!(replies[3]["result"]["isError"], true);
    assert!(
        replies[3]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("page 5")
    );
    assert_eq!(replies[4]["error"]["code"], -32602);
    assert_eq!(replies[5]["error"]["code"], -32601);
    assert_eq!(replies[6]["error"]["code"], -32700);
}
