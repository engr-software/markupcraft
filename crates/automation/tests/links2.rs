//! End-to-end tests of Places, link actions and editing, links from text and URLs, markup
//! actions, the bookmark additions (properties, actions, copy, AutoMark, structures, audit,
//! export), snapshots and File Attachment markups, through `Automation::call`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use markupcraft_engine::synthetic::{SyntheticPage, pdf, text};
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-links2-{}-{}",
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

fn sheets(d: &Path) {
    let page = |n: &str, extra: &str| {
        SyntheticPage::new(
            612.0,
            792.0,
            format!(
                "{}{}{}",
                text(500.0, 40.0, 14.0, n),
                text(72.0, 700.0, 12.0, "PLAN NOTES"),
                extra
            ),
        )
    };
    let bytes = pdf(&[
        page(
            "A-101",
            &text(72.0, 600.0, 12.0, "See https://example.com/spec and SHEET A-102"),
        ),
        page("A-102", ""),
        page("M-201", ""),
    ]);
    std::fs::write(d.join("set.pdf"), bytes).unwrap();
}

#[test]
fn places_link_actions_text_and_url_links_and_markup_actions() {
    let d = dir();
    sheets(&d);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "set.pdf" }));
    call(
        &mut a,
        "place_set",
        json!({ "name": "Detail 4", "page": 2, "left": 72, "top": 700, "zoom": 2 }),
    );
    let p = call(&mut a, "place_list", json!({}));
    assert_eq!(p["places"][0]["page"], 2);
    let l = call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [10, 10, 60, 30], "place": "Detail 4" }),
    );
    let id = l["id"].as_str().unwrap().to_string();
    // Moving the Place keeps the link going to it.
    call(&mut a, "place_set", json!({ "name": "Detail 4", "page": 3 }));
    let links = call(&mut a, "link_list", json!({}));
    assert_eq!(links["links"][0]["target"], json!({ "place": "Detail 4" }));
    // Edit Action: a zoomed page, then a view rectangle, then a Space.
    call(
        &mut a,
        "link_edit",
        json!({ "id": id, "to_page": 2, "zoom": "fit_width" }),
    );
    assert_eq!(
        call(&mut a, "link_list", json!({}))["links"][0]["target"],
        json!({ "page": 2, "zoom": "fit_width" })
    );
    call(
        &mut a,
        "link_edit",
        json!({ "id": id, "to_page": 3, "view": [100, 100, 300, 200], "rect": [10, 10, 80, 30] }),
    );
    let t = call(&mut a, "link_list", json!({}))["links"][0].clone();
    assert_eq!(t["target"]["view"], json!([100.0, 100.0, 300.0, 200.0]));
    assert_eq!(t["rect"], json!([10.0, 10.0, 80.0, 30.0]));
    let sp = call(
        &mut a,
        "space_add",
        json!({ "page": 2, "name": "Lobby", "points": [[100, 100], [200, 100], [200, 180], [100, 180]] }),
    );
    let space = sp["id"].as_str().unwrap().to_string();
    call(&mut a, "link_edit", json!({ "id": id, "space": space }));
    assert_eq!(
        call(&mut a, "link_list", json!({}))["links"][0]["target"]["view"],
        json!([100.0, 100.0, 200.0, 180.0])
    );
    // A link fitted to the words in a box, and the written URLs.
    let t = call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [250, 590, 420, 615], "on_text": true, "to_page": 2 }),
    );
    assert!(t["id"].is_string());
    let u = call(&mut a, "link_from_urls", json!({}));
    assert_eq!(u["added"], 1);
    // Markup action.
    let m = call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[300, 300], [350, 350]] }),
    );
    let mid = m["id"].as_str().unwrap().to_string();
    let v = call(
        &mut a,
        "markup_action",
        json!({ "id": mid, "url": "https://example.com" }),
    );
    assert_eq!(v["action"], json!({ "url": "https://example.com" }));
    let v = call(
        &mut a,
        "markup_action",
        json!({ "id": mid, "file": "other.pdf", "file_page": 2, "view": [0, 0, 100, 100] }),
    );
    assert_eq!(v["action"]["file_page"], 2);
    let v = call(&mut a, "markup_action", json!({ "id": mid, "remove": true }));
    assert!(v["action"].is_null());
    call(
        &mut a,
        "place_set",
        json!({ "name": "Detail 4", "new_name": "Detail 9" }),
    );
    assert_eq!(
        call(&mut a, "place_delete", json!({ "names": ["Detail 9"] }))["deleted"],
        1
    );
}

#[test]
fn bookmark_properties_actions_copy_automark_structures_audit_and_export() {
    let d = dir();
    sheets(&d);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "set.pdf" }));
    let v = call(
        &mut a,
        "bookmark_automark",
        json!({ "region": [480, 30, 600, 60], "replace": true }),
    );
    assert_eq!(v["added"], 3);
    let v = call(
        &mut a,
        "bookmark_structure",
        json!({ "folders": [
            { "title": "Architectural", "prefixes": ["A-"] },
            { "title": "Mechanical", "prefixes": ["M-"] }
        ] }),
    );
    assert_eq!(v["filed"], 3);
    call(
        &mut a,
        "bookmark_structure",
        json!({ "save": "disc.json", "name": "Disciplines" }),
    );
    assert!(d.join("disc.json").is_file());
    let v = call(
        &mut a,
        "bookmark_style",
        json!({ "paths": [[1], [2]], "color": "#C00000", "bold": true }),
    );
    assert_eq!(v["bookmarks"][0]["bold"], true);
    assert_eq!(v["bookmarks"][1]["color"], "#C00000");
    let v = call(
        &mut a,
        "bookmark_action",
        json!({ "path": [1, 1], "to_page": 1, "zoom": "actual" }),
    );
    assert_eq!(v["bookmark"]["action"], json!({ "page": 1, "zoom": "actual" }));
    let v = call(&mut a, "bookmark_copy", json!({ "path": [2], "index": 1 }));
    assert_eq!(v["path"], json!([1]));
    assert!(
        call(&mut a, "bookmark_audit", json!({}))["broken"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    call(&mut a, "bookmark_action", json!({ "path": [1, 1], "place": "Missing" }));
    let broken = call(&mut a, "bookmark_audit", json!({}));
    assert_eq!(broken["broken"][0]["path"], json!([1, 1]));
    call(&mut a, "doc_save", json!({}));
    let v = call(
        &mut a,
        "bookmark_export",
        json!({ "out": "bm.csv", "files": ["set.pdf"] }),
    );
    assert!(v["bookmarks"].as_u64().unwrap() >= 6);
    let csv = std::fs::read_to_string(d.join("bm.csv")).unwrap();
    assert!(csv.contains("Architectural") && csv.contains("M-201"), "{csv}");
    call(&mut a, "bookmark_export", json!({ "out": "bm.pdf", "tree": false }));
    assert!(d.join("bm.pdf").is_file());
}

#[test]
fn snapshots_and_file_attachment_markups_with_capture_summary() {
    let d = dir();
    sheets(&d);
    std::fs::write(d.join("photo.jpg"), b"\xFF\xD8fake").unwrap();
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "set.pdf" }));
    let v = call(&mut a, "snapshot_copy", json!({ "page": 1 }));
    assert_eq!(v["rect"], json!([0.0, 0.0, 612.0, 792.0]));
    call(&mut a, "markup_paste", json!({ "page": 2 }));
    let list = call(&mut a, "markup_list", json!({}));
    assert_eq!(list["markups"][0]["kind"], "Snapshot");
    let sp = call(
        &mut a,
        "space_add",
        json!({ "page": 1, "name": "Room", "points": [[60, 680], [200, 680], [200, 720], [60, 720]] }),
    );
    let v = call(&mut a, "snapshot_copy", json!({ "space": sp["id"] }));
    assert_eq!(v["rect"], json!([60.0, 680.0, 200.0, 720.0]));
    let v = call(
        &mut a,
        "attachment_markup_add",
        json!({ "page": 1, "at": [400, 400], "path": "photo.jpg", "description": "east wall" }),
    );
    let id = v["id"].as_str().unwrap().to_string();
    let list = call(&mut a, "markup_list", json!({}));
    assert!(
        list["markups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == id.as_str() && m["kind"] == "File Attachment")
    );
    std::fs::create_dir_all(d.join("media")).unwrap();
    let v = call(
        &mut a,
        "capture_summary",
        json!({ "csv": "capture.csv", "dir": "media" }),
    );
    assert_eq!(v["attachments"][0]["file"], "photo.jpg");
    assert_eq!(
        std::fs::read(d.join("media").join("photo.jpg")).unwrap(),
        b"\xFF\xD8fake"
    );
    assert!(
        std::fs::read_to_string(d.join("capture.csv"))
            .unwrap()
            .contains("east wall")
    );
}
