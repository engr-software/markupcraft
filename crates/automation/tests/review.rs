//! End-to-end tests of the review tools (compare, overlay, visual search, OCR, redaction,
//! forms, signatures, spell check) through `Automation::call`, on PDFs built here.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use markupcraft_engine::synthetic::{SyntheticPage, line, pdf, rect, text};
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-review-{}-{}",
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

fn write(d: &Path, name: &str, pages: &[SyntheticPage]) -> PathBuf {
    let p = d.join(name);
    std::fs::write(&p, pdf(pages)).unwrap();
    p
}

fn letter(content: String) -> SyntheticPage {
    SyntheticPage::new(612.0, 792.0, content)
}

fn markups(a: &mut Automation) -> Vec<Value> {
    call(a, "markup_list", json!({}))["markups"].as_array().unwrap().clone()
}

#[test]
fn compare_documents_clouds_changes_on_the_newer_revision() {
    let d = dir();
    let common = line(40.0, 40.0, 560.0, 40.0, 2.0);
    write(
        &d,
        "rev1.pdf",
        &[
            letter(format!(
                "{common}{}{}",
                text(50.0, 700.0, 14.0, "ROOM 101 OFFICE"),
                rect(300.0, 300.0, 50.0, 50.0)
            )),
            letter(text(50.0, 700.0, 14.0, "SHEET TWO")),
        ],
    );
    write(
        &d,
        "rev2.pdf",
        &[
            letter(format!(
                "{common}{}{}",
                text(50.0, 700.0, 14.0, "ROOM 102 OFFICE"),
                rect(100.0, 100.0, 50.0, 50.0)
            )),
            letter(text(50.0, 700.0, 14.0, "SHEET TWO")),
        ],
    );
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "rev2.pdf" }));
    let r = call(&mut a, "compare_documents", json!({ "old": "rev1.pdf" }));
    assert_eq!(r["pairs"], json!([[1, 1], [2, 2]]));
    assert_eq!(r["changes"], 3, "{r}");
    let regions = r["regions"].as_array().unwrap();
    assert!(regions.iter().all(|g| g["page"] == 1));
    assert!(
        regions
            .iter()
            .any(|g| g["kind"] == "removed" && g["source"] == "graphics")
    );
    assert!(regions.iter().any(|g| g["kind"] == "added"));
    assert!(
        regions
            .iter()
            .any(|g| g["text"].as_str().unwrap().contains("101 -> 102"))
    );
    let clouds = markups(&mut a);
    assert_eq!(clouds.len(), 3);
    assert!(clouds.iter().all(|m| m["subject"] == "Compare" && m["kind"] == "Cloud"));
    // One undo removes them all.
    call(&mut a, "edit_undo", json!({}));
    assert!(markups(&mut a).is_empty());
    // Text only, one page pair, red rectangles, a custom subject.
    let r = call(
        &mut a,
        "compare_documents",
        json!({ "old": "rev1.pdf", "mode": "text", "pairs": [[1, 1]], "color": "#FF0000", "cloud": 0, "subject": "Rev 2" }),
    );
    assert_eq!(r["changes"], 1);
    let m = &markups(&mut a)[0];
    assert_eq!(
        (m["subject"].as_str(), m["color"].as_str()),
        (Some("Rev 2"), Some("#FF0000"))
    );
    // The older revision can be an open document too; comparing with itself finds nothing.
    let me = r["document"]["doc"].as_u64().unwrap();
    let r = call(
        &mut a,
        "compare_documents",
        json!({ "old_doc": me, "include_markups": true }),
    );
    assert_eq!(r["changes"], 0);
    // Errors, not crashes.
    assert!(fails(&mut a, "compare_documents", json!({})).contains("old"));
    assert!(
        fails(
            &mut a,
            "compare_documents",
            json!({ "old": "rev1.pdf", "mode": "smell" })
        )
        .contains("mode")
    );
    assert!(
        fails(
            &mut a,
            "compare_documents",
            json!({ "old": "rev1.pdf", "pairs": [[1, 9]] })
        )
        .contains("page")
    );
    std::fs::write(d.join("junk.pdf"), b"not a pdf").unwrap();
    fails(&mut a, "compare_documents", json!({ "old": "junk.pdf" }));
}

#[test]
fn overlay_pages_writes_coloured_toggleable_layers() {
    let d = dir();
    let common = line(50.0, 100.0, 550.0, 100.0, 6.0);
    write(
        &d,
        "a.pdf",
        &[letter(format!("{common}{}", line(100.0, 400.0, 500.0, 400.0, 6.0)))],
    );
    write(
        &d,
        "b.pdf",
        &[letter(format!("{common}{}", line(100.0, 600.0, 500.0, 600.0, 6.0)))],
    );
    write(
        &d,
        "b_shifted.pdf",
        &[SyntheticPage::new(
            700.0,
            900.0,
            format!(
                "{}{}",
                line(80.0, 130.0, 580.0, 130.0, 6.0),
                line(130.0, 630.0, 530.0, 630.0, 6.0)
            ),
        )],
    );
    let mut a = auto(&d);
    let r = call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "a.pdf", "name": "Rev 1" }, { "path": "b.pdf", "color": "blue", "opacity": 0.8 }], "out": "overlay.pdf" }),
    );
    assert_eq!(r["pages"], 1);
    assert_eq!(r["layers"], json!(["Rev 1", "Layer 2"]));
    let bytes = std::fs::read(d.join("overlay.pdf")).unwrap();
    let cos = markupcraft_revu::cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
    let cat = cos
        .dict(&markupcraft_revu::cos::Object::Ref(cos.root().unwrap()))
        .unwrap();
    let ocp = cos.dict(cat.get(b"OCProperties").unwrap()).unwrap();
    let ocgs = cos.resolve(ocp.get(b"OCGs").unwrap());
    assert_eq!(ocgs.as_array().unwrap().len(), 2, "layers are optional content groups");
    // Two-point alignment of a shifted sheet; the result opens as a document.
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "a.pdf" }, { "path": "b_shifted.pdf", "align": "points", "from": [[80, 130], [580, 130]], "to": [[50, 100], [550, 100]] }], "out": "aligned.pdf" }),
    );
    let doc = call(&mut a, "doc_open", json!({ "path": "aligned.pdf" }));
    assert_eq!(doc["pages"], 1);
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "a.pdf" }, { "path": "b_shifted.pdf", "align": "bounds" }], "out": "bounds.pdf" }),
    );
    // Errors.
    assert!(
        fails(
            &mut a,
            "overlay_pages",
            json!({ "layers": [{ "path": "a.pdf" }], "out": "x.pdf" })
        )
        .contains("two layers")
    );
    assert!(
        fails(
            &mut a,
            "overlay_pages",
            json!({ "layers": [{ "path": "a.pdf" }, { "path": "b.pdf", "align": "points" }], "out": "x.pdf" })
        )
        .contains("from")
    );
    fails(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "a.pdf", "bogus": 1 }, { "path": "b.pdf" }], "out": "x.pdf" }),
    );
    fails(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "a.pdf", "pages": [4] }, { "path": "b.pdf", "pages": [1] }], "out": "x.pdf" }),
    );
}

/// An "L" symbol (30 x 18 pt) at (x, y), upright or turned a quarter turn.
fn symbol(x: f64, y: f64, turned: bool) -> String {
    if turned {
        format!("{}{}", rect(x, y, 6.0, 30.0), rect(x, y + 24.0, 18.0, 6.0))
    } else {
        format!("{}{}", rect(x, y, 30.0, 6.0), rect(x, y, 6.0, 18.0))
    }
}

#[test]
fn visual_search_finds_symbols_and_counts_them() {
    let d = dir();
    write(
        &d,
        "plan.pdf",
        &[
            letter(format!(
                "{}{}{}{}",
                symbol(100.0, 600.0, false),
                symbol(400.0, 600.0, false),
                symbol(250.0, 300.0, true),
                rect(450.0, 200.0, 30.0, 30.0)
            )),
            letter(symbol(300.0, 400.0, false)),
        ],
    );
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "plan.pdf" }));
    let r = call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [97, 597, 133, 621] }),
    );
    assert_eq!(r["count"], 4, "{r}");
    assert!(r["hits"].as_array().unwrap().iter().any(|h| h["page"] == 2));
    assert!(r["hits"].as_array().unwrap().iter().any(|h| h["rotation"] != 0));
    assert!(markups(&mut a).is_empty(), "reporting only adds nothing");
    let r = call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [97, 597, 133, 621], "rotations": false, "pages": [1], "action": "count", "subject": "Diffusers" }),
    );
    assert_eq!(r["count"], 2);
    let list = markups(&mut a);
    assert_eq!(list.len(), 1);
    assert_eq!(
        (list[0]["kind"].as_str(), list[0]["quantity"].as_f64()),
        (Some("Count"), Some(2.0))
    );
    assert_eq!(list[0]["subject"], "Diffusers");
    let r = call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [97, 597, 133, 621], "action": "highlight", "max_hits": 3 }),
    );
    assert_eq!(r["markups"].as_array().unwrap().len(), 3);
    assert!(
        fails(
            &mut a,
            "visual_search",
            json!({ "page": 1, "rect": [500, 700, 540, 740] })
        )
        .contains("blank")
    );
    fails(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [97, 597, 133, 621], "action": "explode" }),
    );
    fails(
        &mut a,
        "visual_search",
        json!({ "page": 3, "rect": [97, 597, 133, 621] }),
    );
}

fn page_text(path: &Path, page: usize) -> String {
    let bytes = std::sync::Arc::new(std::fs::read(path).unwrap());
    let r = markupcraft_engine::raster::Renderable::new(bytes, false).unwrap();
    r.text(page).map(|t| t.plain_text()).unwrap_or_default()
}

#[test]
fn ocr_pages_adds_a_searchable_text_layer() {
    use markupcraft_engine::ocr;
    let d = dir();
    // A "scan": the word is drawn as filled shapes, so the page has no text.
    write(
        &d,
        "scan.pdf",
        &[
            letter(rect(100.0, 500.0, 220.0, 50.0)),
            letter(text(50.0, 50.0, 12.0, "typed")),
        ],
    );
    write(&d, "printed.pdf", &[letter(text(80.0, 600.0, 48.0, "HELLO WORLD"))]);
    let mut a = auto(&d);
    if ocr::find_models().is_some() {
        // The real recogniser reads printed text.
        call(&mut a, "doc_open", json!({ "path": "printed.pdf" }));
        let r = call(&mut a, "ocr_pages", json!({ "skip_text": false }));
        let read = r["pages"][0]["text"].as_str().unwrap().to_uppercase();
        assert!(read.contains("HELL") && read.contains("WOR"), "{r}");
    } else {
        call(&mut a, "doc_open", json!({ "path": "printed.pdf" }));
        let e = fails(&mut a, "ocr_pages", json!({ "skip_text": false }));
        assert!(e.contains("cargo xtask models"), "{e}");
    }
    // The pipeline end to end with a stand-in recogniser.
    ocr::install_recognizer(std::sync::Arc::new(ocr::InkWord { text: "KITCHEN".into() }));
    call(&mut a, "doc_open", json!({ "path": "scan.pdf" }));
    let r = call(&mut a, "ocr_pages", json!({ "dpi": 150 }));
    assert_eq!(r["words"], 1, "{r}");
    assert_eq!(r["pages"][0]["text"], "KITCHEN");
    assert_eq!(r["pages"][1]["skipped"], "the page already has text");
    assert_eq!(r["document"]["undo"], "OCR");
    call(&mut a, "doc_save", json!({}));
    assert!(page_text(&d.join("scan.pdf"), 0).contains("KITCHEN"));
    assert!(page_text(&d.join("scan.pdf"), 1).contains("typed"));
    fails(&mut a, "ocr_pages", json!({ "dpi": 2 }));
    fails(&mut a, "ocr_pages", json!({ "pages": [5] }));
}

#[test]
fn redaction_marks_applies_and_verifies() {
    let d = dir();
    write(
        &d,
        "contract.pdf",
        &[
            letter(format!(
                "{}{}{}",
                text(50.0, 700.0, 12.0, "Account 4417 1234 5678 9113 on file"),
                text(50.0, 650.0, 12.0, "Keep this line"),
                rect(400.0, 400.0, 60.0, 60.0)
            )),
            letter(text(50.0, 700.0, 12.0, "Card 4417 1234 5678 9113")),
        ],
    );
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "contract.pdf" }));
    let r = call(
        &mut a,
        "redact_mark",
        json!({ "text": "4417 1234 5678 9113", "overlay": "REDACTED" }),
    );
    assert_eq!((r["marked"].as_u64(), r["marks"].as_u64()), (Some(2), Some(2)));
    call(
        &mut a,
        "redact_mark",
        json!({ "page": 1, "rects": [[390, 390, 470, 470]], "fill": "#FFFFFF" }),
    );
    let list = call(&mut a, "redact_list", json!({}));
    assert_eq!(list["count"], 3);
    assert!(
        list["marks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["overlay"] == "REDACTED")
    );
    // Only page 2 first, then the rest.
    let r = call(&mut a, "redact_apply", json!({ "pages": [2] }));
    assert_eq!(
        (r["marks"].as_u64(), r["verified"].as_bool()),
        (Some(1), Some(true)),
        "{r}"
    );
    assert_eq!(call(&mut a, "redact_list", json!({}))["count"], 2);
    let r = call(&mut a, "redact_apply", json!({}));
    assert_eq!(r["marks"], 2);
    assert_eq!(r["verified"], true, "{r}");
    assert!(
        r["glyphs"].as_u64().unwrap() >= 19 && r["paths"].as_u64().unwrap() >= 1,
        "{r}"
    );
    call(&mut a, "doc_save", json!({}));
    let saved = d.join("contract.pdf");
    let bytes = std::fs::read(&saved).unwrap();
    assert!(
        !String::from_utf8_lossy(&bytes).contains("5678"),
        "no revision keeps the number"
    );
    let p1 = page_text(&saved, 0);
    assert!(
        p1.contains("Keep this line") && p1.contains("Account") && !p1.contains("9113"),
        "{p1}"
    );
    assert!(!page_text(&saved, 1).contains("9113"));
    // Errors.
    assert!(fails(&mut a, "redact_apply", json!({})).contains("no redaction marks"));
    fails(&mut a, "redact_mark", json!({}));
    fails(&mut a, "redact_mark", json!({ "page": 1, "rects": [[1, 2]] }));
    fails(&mut a, "redact_mark", json!({ "page": 9, "rects": [[0, 0, 10, 10]] }));
}

#[test]
fn forms_create_fill_and_flatten() {
    let d = dir();
    write(&d, "form.pdf", &[letter(text(50.0, 740.0, 14.0, "Submittal cover"))]);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "form.pdf" }));
    assert_eq!(call(&mut a, "form_list", json!({}))["count"], 0);
    let add = |a: &mut Automation, args: Value| call(a, "form_add_field", args)["name"].as_str().unwrap().to_string();
    assert_eq!(
        add(
            &mut a,
            json!({ "page": 1, "rect": [50, 700, 250, 720], "type": "text", "name": "Spec" })
        ),
        "Spec"
    );
    assert_eq!(
        add(
            &mut a,
            json!({ "page": 1, "rect": [50, 660, 66, 676], "type": "checkbox" })
        ),
        "Check Box1"
    );
    let g = add(
        &mut a,
        json!({ "page": 1, "rect": [50, 620, 66, 636], "type": "radio", "group": "Action", "export": "Approved" }),
    );
    add(
        &mut a,
        json!({ "page": 1, "rect": [80, 620, 96, 636], "type": "radio", "group": "Action", "export": "Revise" }),
    );
    assert_eq!(g, "Action");
    add(
        &mut a,
        json!({ "page": 1, "rect": [50, 580, 250, 600], "type": "combo", "name": "Status", "options": ["Open", "Closed"] }),
    );
    add(
        &mut a,
        json!({ "page": 1, "rect": [50, 480, 250, 560], "type": "list", "name": "Copies", "options": ["GC", "Owner", "Architect"], "multi": true }),
    );
    add(
        &mut a,
        json!({ "page": 1, "rect": [300, 700, 400, 720], "type": "button", "caption": "Print" }),
    );
    assert_eq!(
        add(
            &mut a,
            json!({ "page": 1, "rect": [300, 100, 500, 150], "type": "signature" })
        ),
        "Signature1"
    );
    let list = call(&mut a, "form_list", json!({}));
    assert_eq!(list["count"], 7);
    let kinds: Vec<&str> = list["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["text", "checkbox", "radio", "combo", "list", "button", "signature"]
    );
    let r = call(
        &mut a,
        "form_fill",
        json!({ "values": { "Spec": "23 31 13 Metal Ducts", "Check Box1": true, "Action": "Revise", "Status": "Closed", "Copies": ["GC", "Architect"] } }),
    );
    assert_eq!(r["filled"], 5);
    call(&mut a, "doc_save", json!({}));
    // The values survive a save and reopen.
    let mut b = auto(&d);
    call(&mut b, "doc_open", json!({ "path": "form.pdf" }));
    let fields = call(&mut b, "form_list", json!({}))["fields"]
        .as_array()
        .unwrap()
        .clone();
    let value = |n: &str| fields.iter().find(|f| f["name"] == n).unwrap()["value"].clone();
    assert_eq!(value("Spec"), json!(["23 31 13 Metal Ducts"]));
    assert_eq!(value("Action"), json!(["Revise"]));
    assert_eq!(value("Copies"), json!(["GC", "Architect"]));
    // Flatten keeps the text as page content.
    assert!(call(&mut b, "form_flatten", json!({}))["flattened"].as_u64().unwrap() >= 7);
    assert_eq!(call(&mut b, "form_list", json!({}))["count"], 0);
    call(&mut b, "doc_save", json!({}));
    assert!(page_text(&d.join("form.pdf"), 0).contains("23 31 13 Metal Ducts"));
    // Reset and errors on the first session (still unflattened in memory).
    assert_eq!(call(&mut a, "form_reset", json!({ "names": ["Spec"] }))["reset"], 1);
    assert!(fails(&mut a, "form_fill", json!({ "values": { "Missing": "x" } })).contains("Missing"));
    assert!(fails(&mut a, "form_fill", json!({ "values": { "Spec": true } })).contains("text"));
    fails(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "rect": [0, 0, 10, 10], "type": "radio" }),
    );
    fails(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "rect": [0, 0, 10, 10], "type": "slider" }),
    );
    fails(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "rect": [0, 0, 10, 10], "type": "combo" }),
    );
}

fn png(path: &Path) {
    let mut img = image::RgbaImage::new(30, 12);
    for p in img.pixels_mut() {
        *p = image::Rgba([10, 20, 160, 255]);
    }
    img.save(path).unwrap();
}

#[test]
fn signatures_sign_validate_and_track_changes() {
    let d = dir();
    write(
        &d,
        "approval.pdf",
        &[letter(text(50.0, 700.0, 14.0, "Shop drawing approval"))],
    );
    write(
        &d,
        "certified.pdf",
        &[letter(text(50.0, 700.0, 14.0, "Final record set"))],
    );
    png(&d.join("sig.png"));
    let mut a = auto(&d);
    let id = call(
        &mut a,
        "digital_id_create",
        json!({ "name": "Field Engineer", "organization": "Example Co", "country": "US", "password": "pw1", "out": "me.p12", "cert_out": "me.pem" }),
    );
    assert_eq!(id["subject"], "Field Engineer");
    assert!(d.join("me.p12").is_file() && d.join("me.pem").is_file());

    // Sign a prepared field, with a picture.
    call(&mut a, "doc_open", json!({ "path": "approval.pdf" }));
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "rect": [300, 80, 560, 140], "type": "signature" }),
    );
    call(&mut a, "doc_save", json!({}));
    let r = call(
        &mut a,
        "signature_sign",
        json!({ "id": "me.p12", "password": "pw1", "field": "Signature1", "image": "sig.png", "reason": "Reviewed", "location": "Site" }),
    );
    assert_eq!(r["signatures"], 1);
    assert_eq!(r["document"]["dirty"], false);
    let list = call(&mut a, "signature_list", json!({}));
    let s = &list["signatures"][0];
    assert_eq!(
        (s["field"].as_str(), s["signed"].as_bool()),
        (Some("Signature1"), Some(true))
    );
    assert_eq!(
        (s["signer"].as_str(), s["reason"].as_str(), s["page"].as_u64()),
        (Some("Field Engineer"), Some("Reviewed"), Some(1))
    );
    assert_eq!(s["status"], "unknown", "self-signed and not trusted yet");
    let trusted = call(&mut a, "signature_list", json!({ "trust": ["me.pem"] }));
    assert_eq!(trusted["signatures"][0]["status"], "valid", "{trusted}");

    // A comment added after signing is a permitted change.
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[50, 500], [150, 550]] }),
    );
    call(&mut a, "doc_save", json!({}));
    let after = call(&mut a, "signature_list", json!({ "trust": ["me.pem"] }));
    let s = &after["signatures"][0];
    assert_eq!(s["status"], "valid", "{after}");
    assert!(!s["modifications"].as_array().unwrap().is_empty(), "{after}");

    // Certified with no changes allowed: a later comment breaks it.
    call(&mut a, "doc_open", json!({ "path": "certified.pdf" }));
    call(
        &mut a,
        "signature_sign",
        json!({ "id": "me.p12", "password": "pw1", "certify": 1, "out": "certified-signed.pdf" }),
    );
    let c = call(&mut a, "signature_list", json!({ "trust": ["me.pem"] }));
    assert_eq!(
        (
            c["signatures"][0]["certify"].as_u64(),
            c["signatures"][0]["status"].as_str()
        ),
        (Some(1), Some("valid")),
        "{c}"
    );
    assert!(
        c["signatures"][0]["rect"].is_null() || c["signatures"][0]["rect"] == json!([0.0, 0.0, 0.0, 0.0]),
        "invisible"
    );
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[50, 500], [150, 550]] }),
    );
    call(&mut a, "doc_save", json!({}));
    let broken = call(&mut a, "signature_list", json!({ "trust": ["me.pem"] }));
    assert_eq!(broken["signatures"][0]["status"], "invalid", "{broken}");

    // Errors.
    assert!(fails(&mut a, "signature_sign", json!({ "id": "me.p12", "password": "nope" })).contains("password"));
    fails(
        &mut a,
        "signature_sign",
        json!({ "id": "me.p12", "password": "pw1", "field": "Nope", "image": "sig.png" }),
    );
    fails(
        &mut a,
        "signature_sign",
        json!({ "id": "me.p12", "password": "pw1", "image": "sig.png" }),
    );
    fails(
        &mut a,
        "digital_id_create",
        json!({ "name": "", "password": "x", "out": "x.p12" }),
    );
    fails(&mut a, "signature_list", json!({ "trust": ["sig.png"] }));
}

#[test]
fn spell_check_flags_markup_text_with_suggestions() {
    let d = dir();
    write(&d, "notes.pdf", &[letter(String::new()), letter(String::new())]);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "notes.pdf" }));
    let good = call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[10, 10], [50, 50]], "contents": "Verify the supply duct size at AHU-1." }),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    let bad = call(
        &mut a,
        "markup_add",
        json!({ "page": 2, "kind": "Rectangle", "points": [[10, 10], [50, 50]], "contents": "Relocate befor the ceiling is closd. RTU-2 per MECH" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = call(&mut a, "spell_check", json!({}));
    assert_eq!(r["count"], 2, "{r}");
    let first = &r["misspelled"][0];
    assert_eq!(
        (first["id"].as_str(), first["page"].as_u64(), first["word"].as_str()),
        (Some(bad.as_str()), Some(2), Some("befor"))
    );
    assert_eq!(first["start"], 9);
    assert!(
        first["suggestions"].as_array().unwrap().iter().any(|s| s == "before"),
        "{r}"
    );
    assert!(
        r["misspelled"][1]["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s == "closed"),
        "{r}"
    );
    // Only some markups; accepted words; capitals checked on request.
    assert_eq!(call(&mut a, "spell_check", json!({ "ids": [good] }))["count"], 0);
    assert_eq!(
        call(&mut a, "spell_check", json!({ "accept": ["befor", "closd"] }))["count"],
        0
    );
    let caps = call(
        &mut a,
        "spell_check",
        json!({ "ids": [bad], "ignore_uppercase": false }),
    );
    assert!(
        caps["misspelled"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["word"] == "MECH"),
        "{caps}"
    );
    // Free text.
    let t = call(
        &mut a,
        "spell_check",
        json!({ "text": "Teh drawing", "suggestions": 3 }),
    );
    assert_eq!(t["misspelled"][0]["word"], "Teh");
    assert!(
        t["misspelled"][0]["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s == "The"),
        "{t}"
    );
    assert!(t["misspelled"][0]["id"].is_null());
    // Errors.
    assert!(fails(&mut a, "spell_check", json!({ "language": "xx_XX" })).contains("dictionary"));
    fails(&mut a, "spell_check", json!({ "ids": ["nope"] }));
    fails(&mut a, "spell_check", json!({ "text": "x", "ids": [] }));
}
