//! End-to-end tests of the arranging tools (align, distribute, flip, apply to pages, remove
//! from group) through `Automation::call`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-editing-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn call(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args) {
        Ok(v) => v,
        Err(e) => panic!("{tool} {args}: {e}"),
    }
}

fn square(x: f64, y: f64, s: f64) -> Value {
    json!([[x, y], [x + s, y], [x + s, y + s], [x, y + s]])
}

fn x0(v: &Value, id: &str) -> f64 {
    let m = v["markups"].as_array().unwrap().iter().find(|m| m["id"] == id).unwrap();
    m["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p[0].as_f64().unwrap())
        .fold(f64::MAX, f64::min)
}

#[test]
fn markup_align_aligns_distributes_flips_and_copies_to_pages() {
    let d = dir();
    let mut a = Automation::new().with_root(&d).unwrap().with_author("Tester");
    call(&mut a, "doc_new", json!({ "path": "a.pdf", "pages": 3 }));
    let mut ids = Vec::new();
    for (x, y, s) in [(10.0, 10.0, 20.0), (100.0, 50.0, 40.0), (300.0, 200.0, 10.0)] {
        let v = call(
            &mut a,
            "markup_add",
            json!({ "kind": "Rectangle", "page": 1, "points": square(x, y, s) }),
        );
        ids.push(v["id"].as_str().unwrap().to_string());
    }
    // to the reference: the last id (the last selected), as Revu does
    let v = call(&mut a, "markup_align", json!({ "ids": ids, "align": "left" }));
    assert_eq!(v["changed"], 2);
    for id in &ids {
        assert!((x0(&v, id) - 300.0).abs() < 1e-6);
    }
    call(&mut a, "edit_undo", json!({}));
    // or to their joint extent
    let v = call(
        &mut a,
        "markup_align",
        json!({ "ids": ids, "align": "left", "to": "extent" }),
    );
    for id in &ids {
        assert!((x0(&v, id) - 10.0).abs() < 1e-6);
    }
    call(&mut a, "edit_undo", json!({}));
    let v = call(
        &mut a,
        "markup_align",
        json!({ "ids": ids, "distribute": "horizontal" }),
    );
    assert!((x0(&v, &ids[1]) - 145.0).abs() < 1e-6, "{v}");
    let v = call(&mut a, "markup_align", json!({ "ids": [ids[0]], "flip": "vertical" }));
    assert_eq!(v["changed"], 1);
    let v = call(&mut a, "markup_align", json!({ "ids": [ids[0]], "to_pages": "all" }));
    assert_eq!(v["new_ids"].as_array().unwrap().len(), 2);
    let v = call(&mut a, "markup_align", json!({ "ids": [ids[1]], "to_pages": [3] }));
    assert_eq!(v["new_ids"].as_array().unwrap().len(), 1);
    // only the even pages of the three
    let v = call(
        &mut a,
        "markup_align",
        json!({ "ids": [ids[2]], "to_pages": "all", "page_filter": "even" }),
    );
    assert_eq!(v["new_ids"].as_array().unwrap().len(), 1);
    assert!(
        a.call(
            "markup_align",
            &json!({ "ids": [ids[2]], "to_pages": "all", "page_filter": "sideways" })
        )
        .is_err()
    );
    call(&mut a, "markup_group", json!({ "ids": [ids[0], ids[1], ids[2]] }));
    let v = call(
        &mut a,
        "markup_align",
        json!({ "ids": [ids[2]], "remove_from_group": true }),
    );
    assert_eq!(v["changed"], 1);
    assert!(
        a.call("markup_align", &json!({ "ids": ids, "flip": "sideways" }))
            .is_err()
    );
    assert!(a.call("markup_align", &json!({ "ids": ids })).is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn viewport_edit_renames_rescales_copies_and_clears() {
    let d = dir();
    let mut a = Automation::new().with_root(&d).unwrap().with_author("Tester");
    call(&mut a, "doc_new", json!({ "path": "v.pdf", "pages": 3 }));
    let eighth = json!({ "kind": "architectural", "paper_inches": 0.125, "real_feet": 1 });
    let quarter = json!({ "kind": "architectural", "paper_inches": 0.25, "real_feet": 1 });
    call(
        &mut a,
        "viewport_add",
        json!({ "page": 1, "box": [10, 10, 200, 200], "name": "Plan", "scale": eighth }),
    );
    call(
        &mut a,
        "viewport_edit",
        json!({ "page": 1, "index": 1, "rename": "Detail A" }),
    );
    call(
        &mut a,
        "viewport_edit",
        json!({ "page": 1, "index": 1, "scale": quarter }),
    );
    let v = call(&mut a, "viewport_edit", json!({ "page": 1, "copy_to": "all" }));
    assert_eq!(v["changed"], 2);
    let v = call(&mut a, "viewport_edit", json!({ "page": 3, "clear": true }));
    assert_eq!(v["changed"], 1);
    assert!(a.call("viewport_edit", &json!({ "page": 1, "rename": "x" })).is_err());
    let _ = std::fs::remove_dir_all(&d);
}
