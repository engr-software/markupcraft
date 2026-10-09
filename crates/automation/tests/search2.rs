//! End-to-end tests of the Search panel additions through `Automation::call`: searching other
//! files (a list, a folder, a Set, the open documents), file names, properties and form
//! fields, Replace, the text of a box, and Visual Search's 45-degree steps, colour filter,
//! limit to selection and thumbnails.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use markupcraft_engine::synthetic::{SyntheticPage, pdf, rect, text};
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-search2-{}-{}",
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

fn write(d: &Path, name: &str, content: String) {
    std::fs::write(d.join(name), pdf(&[SyntheticPage::new(612.0, 792.0, content)])).unwrap();
}

#[test]
fn search_across_files_folders_sets_and_open_documents() {
    let d = dir();
    std::fs::create_dir_all(d.join("sheets/sub")).unwrap();
    write(&d.join("sheets"), "A-101.pdf", text(72.0, 700.0, 12.0, "VAV-1 SUPPLY"));
    write(&d.join("sheets"), "A-102.pdf", text(72.0, 700.0, 12.0, "NOTHING HERE"));
    write(
        &d.join("sheets/sub"),
        "M-201.pdf",
        text(72.0, 700.0, 12.0, "VAV-2 RETURN"),
    );
    let mut a = auto(&d);
    let v = call(&mut a, "text_search", json!({ "text": "VAV", "folder": "sheets" }));
    assert_eq!(v["count"], 1, "{v}");
    let v = call(
        &mut a,
        "text_search",
        json!({ "text": "VAV", "folder": "sheets", "recursive": true }),
    );
    assert_eq!(v["count"], 2, "{v}");
    let v = call(
        &mut a,
        "text_search",
        json!({ "text": "A-10", "folder": "sheets", "file_names": true, "page_text": false }),
    );
    assert_eq!(v["count"], 2, "file names: {v}");
    assert_eq!(v["files"][0]["hits"][0]["source"], "file_name");
    call(
        &mut a,
        "set_save",
        json!({ "path": "job.pcset", "files": ["sheets/A-102.pdf", "sheets/sub/M-201.pdf"] }),
    );
    let v = call(&mut a, "text_search", json!({ "text": "VAV", "set": "job.pcset" }));
    assert_eq!(v["count"], 1, "{v}");
    let v = call(
        &mut a,
        "text_search",
        json!({ "text": "RETURN", "files": ["sheets/sub/M-201.pdf"] }),
    );
    assert_eq!(v["files"][0]["hits"][0]["page"], 1);
    call(&mut a, "doc_open", json!({ "path": "sheets/A-101.pdf" }));
    call(&mut a, "doc_open", json!({ "path": "sheets/A-102.pdf" }));
    let v = call(&mut a, "text_search", json!({ "text": "SUPPLY", "open_docs": true }));
    let counts: Vec<u64> = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["count"].as_u64().unwrap())
        .collect();
    assert_eq!(counts, [1, 0]);
    // Properties and form fields of the current document.
    call(
        &mut a,
        "doc_properties_set",
        json!({ "properties": { "Title": "Supply air layout" } }),
    );
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "rect": [300, 100, 450, 120], "type": "text", "name": "Notes" }),
    );
    call(
        &mut a,
        "form_fill",
        json!({ "values": { "Notes": "supply duct sizes" } }),
    );
    let v = call(
        &mut a,
        "text_search",
        json!({ "text": "supply", "properties": true, "form_fields": true, "page_text": false }),
    );
    let sources: Vec<&str> = v["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["source"].as_str().unwrap())
        .collect();
    assert_eq!(sources, ["property", "form_field"], "{v}");
}

#[test]
fn replace_checked_text_and_read_a_selection() {
    let d = dir();
    write(
        &d,
        "r.pdf",
        format!(
            "{}{}",
            text(72.0, 700.0, 14.0, "ROOM 101 OFFICE"),
            text(72.0, 650.0, 14.0, "ROOM 102 STORAGE")
        ),
    );
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "r.pdf" }));
    let v = call(&mut a, "region_text", json!({ "page": 1, "rect": [60, 690, 300, 720] }));
    assert_eq!(v["text"], "ROOM 101 OFFICE");
    let found = call(&mut a, "text_search", json!({ "text": "ROOM" }));
    let second = found["hits"][1]["rects"][0].clone();
    let v = call(
        &mut a,
        "text_replace",
        json!({ "text": "ROOM", "with": "SUITE", "only": [{ "page": 1, "rect": second }] }),
    );
    assert_eq!((v["replaced"].clone(), v["lines"].clone()), (json!(1), json!(1)), "{v}");
    let t = call(&mut a, "page_text", json!({ "page": 1 }))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(t.contains("ROOM 101") && t.contains("SUITE 102"), "{t}");
    call(
        &mut a,
        "text_replace",
        json!({ "text": "office", "with": "LAB", "whole_words": true }),
    );
    call(&mut a, "doc_save", json!({}));
    let t = markupcraft_engine::Session::open(d.join("r.pdf"))
        .unwrap()
        .page_text(0)
        .unwrap();
    assert!(t.contains("ROOM 101 LAB"), "{t}");
    let e = a
        .call("text_replace", &json!({ "text": "MISSING", "with": "X" }))
        .unwrap_err()
        .to_string();
    assert!(e.contains("not in the page text"), "{e}");
}

#[test]
fn visual_search_eighth_turns_colour_limit_and_thumbnails() {
    let d = dir();
    let l = |x: f64, y: f64| format!("{}{}", rect(x, y, 30.0, 6.0), rect(x, y, 6.0, 18.0));
    let turned = format!("q 0.7071 -0.7071 0.7071 0.7071 400 600 cm {}Q\n", l(0.0, 0.0));
    let red = format!("1 0 0 rg {}", l(100.0, 300.0).replace("0 g ", ""));
    write(
        &d,
        "v.pdf",
        format!("{}{turned}{red}0 g {}", l(100.0, 600.0), l(300.0, 300.0)),
    );
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "v.pdf" }));
    let base = json!({ "page": 1, "rect": [97, 597, 133, 621], "sensitivity": 0.35 });
    let n = |v: &Value| v["count"].as_u64().unwrap();
    let plain = call(&mut a, "visual_search", base.clone());
    let mut fine = base.clone();
    fine["fine_rotations"] = json!(true);
    let fine = call(&mut a, "visual_search", fine);
    assert_eq!(n(&fine), n(&plain) + 1, "{fine}");
    let mut colour = base.clone();
    colour["color_filter"] = json!(true);
    colour["thumbnails"] = json!(32);
    let colour = call(&mut a, "visual_search", colour);
    assert_eq!(n(&colour), n(&plain) - 1, "the red copy is left out: {colour}");
    assert!(
        colour["hits"][0]["thumbnail"].as_str().unwrap().starts_with("iVBOR"),
        "a PNG"
    );
    let mut limited = base;
    limited["limit_to_selection"] = json!(true);
    assert!(n(&call(&mut a, "visual_search", limited)) >= 1);
}
