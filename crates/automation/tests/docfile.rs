//! End-to-end tests of the whole-file tools (images as PDF, revisions and Revert As, Publish
//! As, Deskew, signature / PDF/A state), through `Automation::call`, on files made here.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-docfile-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
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

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 255) as u8, (y % 255) as u8, 90]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

#[test]
fn page_labels_from_a_title_block_region() {
    use markupcraft_engine::synthetic::{SyntheticPage, pdf, text};
    let d = dir();
    let page = |n: &str| SyntheticPage::new(600.0, 400.0, text(500.0, 40.0, 12.0, n));
    std::fs::write(d.join("set.pdf"), pdf(&[page("E-201"), page("E-202")])).unwrap();
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "set.pdf" }));
    let v = call(
        &mut a,
        "page_labels_from_region",
        json!({ "regions": [[490, 30, 590, 60]], "before": "Sheet ", "apply": true }),
    );
    assert_eq!(v["labels"][1]["label"], "Sheet E-202");
    let l = call(&mut a, "page_labels", json!({}));
    assert_eq!(l["labels"][0], "Sheet E-201");
    assert!(
        a.call("page_labels_from_region", &json!({ "regions": [[1, 2]] }))
            .is_err()
    );
}

#[test]
fn doc_from_image_makes_a_page_and_saves() {
    let d = dir();
    std::fs::write(d.join("scan.png"), png(216, 288)).unwrap();
    let mut a = auto(&d);
    let v = call(&mut a, "doc_from_image", json!({ "image": "scan.png" }));
    assert_eq!(v["pages"], 1);
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(info["page_list"][0]["width"], 216.0);
    assert_eq!(info["page_list"][0]["height"], 288.0);
    call(&mut a, "doc_save", json!({}));
    assert!(std::fs::read(d.join("scan.pdf")).unwrap().starts_with(b"%PDF"));
    std::fs::write(d.join("bad.png"), b"nope").unwrap();
    assert!(a.call("doc_from_image", &json!({ "image": "bad.png" })).is_err());
}

#[test]
fn revisions_revert_publish_deskew_and_standards() {
    let d = dir();
    std::fs::write(d.join("plan.pdf"), markupcraft_render::synthetic::sample_pdf()).unwrap();
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "plan.pdf" }));
    assert_eq!(call(&mut a, "doc_revisions", json!({}))["revisions"], 1);
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": ["SAMPLESQUAREAAAA"], "move": [20, 0] }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    let plan = call(&mut a, "doc_open", json!({ "path": "plan.pdf" }))["doc"].clone();
    assert_eq!(call(&mut a, "doc_revisions", json!({}))["revisions"], 2);
    call(&mut a, "doc_revisions", json!({ "revision": 1, "out": "first.pdf" }));
    assert_eq!(
        std::fs::read(d.join("first.pdf")).unwrap(),
        markupcraft_render::synthetic::sample_pdf()
    );
    assert!(a.call("doc_revisions", &json!({ "revision": 1 })).is_err());
    let v = call(&mut a, "doc_publish", json!({ "out": "flat.pdf", "mode": "flattened" }));
    assert!(v["bytes"].as_u64().unwrap() > 0);
    call(
        &mut a,
        "doc_publish",
        json!({ "out": "plain.pdf", "mode": "uncompressed" }),
    );
    let flat = call(&mut a, "doc_open", json!({ "path": "flat.pdf" }));
    assert_eq!(flat["markups"], 0);
    let v = call(
        &mut a,
        "page_deskew",
        json!({ "doc": plan, "pages": [1], "from": [0, 0], "to": [100, 3] }),
    );
    assert!((v["degrees"].as_f64().unwrap() + 1.718).abs() < 0.01, "{v}");
    assert!(a.call("page_deskew", &json!({ "doc": plan, "degrees": 80 })).is_err());
    let st = call(&mut a, "doc_standards", json!({ "doc": plan }));
    assert_eq!(st["certified"], false);
    assert_eq!(st["page_edits_blocked"], false);
}
