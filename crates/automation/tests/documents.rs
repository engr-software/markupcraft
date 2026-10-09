//! End-to-end tests of the document tools (bookmarks, labels, search, links, attachments,
//! properties, marks, flatten, XFDF, security, reduce, print, split/combine, replace, boxes),
//! through `Automation::call`, on PDFs built here.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-documents-{}-{}",
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

/// A saved document of blank letter pages, open in `a`.
fn new_doc(a: &mut Automation, name: &str, pages: u64) -> u64 {
    let v = call(a, "doc_new", json!({ "path": name, "pages": pages }));
    call(a, "doc_save", json!({}));
    v["doc"].as_u64().unwrap()
}

/// A letter-size PDF whose pages show Helvetica text at the given places.
fn text_pdf(pages: &[&[(f64, f64, &str)]]) -> Vec<u8> {
    let mut objs: Vec<String> = Vec::new();
    let kids: Vec<String> = (0..pages.len()).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    objs.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    objs.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    ));
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into());
    for (i, items) in pages.iter().enumerate() {
        let mut c = String::new();
        for (x, y, t) in items.iter() {
            c.push_str(&format!("BT /F1 18 Tf {x} {y} Td ({t}) Tj ET\n"));
        }
        objs.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            5 + 2 * i
        ));
        objs.push(format!("<< /Length {} >>\nstream\n{c}endstream", c.len()));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn write_text_pdf(d: &Path, name: &str, pages: &[&[(f64, f64, &str)]]) {
    std::fs::write(d.join(name), text_pdf(pages)).unwrap();
}

fn rect_markup(a: &mut Automation, page: u64, x: f64, y: f64, subject: &str) -> String {
    let v = call(
        a,
        "markup_add",
        json!({ "page": page, "kind": "Rectangle", "points": [[x, y], [x + 50.0, y + 30.0]], "subject": subject }),
    );
    v["id"].as_str().unwrap().to_string()
}

fn count(v: &Value, key: &str) -> usize {
    v[key].as_array().map_or(0, Vec::len)
}

fn titles(v: &Value) -> Vec<String> {
    v["bookmarks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["title"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn bookmarks_add_rename_move_delete_and_create_from_labels() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "set.pdf", 4);
    call(
        &mut a,
        "page_label_number",
        json!({ "pages": "1-4", "prefix": "A-10", "start": 1 }),
    );
    let v = call(&mut a, "bookmark_create", json!({}));
    assert_eq!(v["created"], 4);
    assert_eq!(titles(&v), ["A-101", "A-102", "A-103", "A-104"]);
    assert_eq!(v["bookmarks"][2]["page"], 3);

    // Nest: a child under the first bookmark, then move the fourth under it too.
    let v = call(
        &mut a,
        "bookmark_add",
        json!({ "page": 2, "title": "Detail 5", "parent": [1] }),
    );
    assert_eq!(v["path"], json!([1, 1]));
    let v = call(
        &mut a,
        "bookmark_move",
        json!({ "path": [4], "parent": [1], "index": 1 }),
    );
    assert_eq!(v["path"], json!([1, 1]));
    let list = call(&mut a, "bookmark_list", json!({}));
    assert_eq!(titles(&list), ["A-101", "A-104", "Detail 5", "A-102", "A-103"]);
    assert_eq!(list["bookmarks"][0]["children"], 2);
    assert_eq!(list["bookmarks"][1]["depth"], 1);

    call(
        &mut a,
        "bookmark_edit",
        json!({ "path": [1, 2], "title": "Detail 6", "page": 4 }),
    );
    let list = call(&mut a, "bookmark_list", json!({}));
    assert_eq!(list["bookmarks"][2]["title"], "Detail 6");
    assert_eq!(list["bookmarks"][2]["page"], 4);

    call(&mut a, "bookmark_delete", json!({ "path": [2] }));
    let list = call(&mut a, "bookmark_list", json!({}));
    assert_eq!(titles(&list), ["A-101", "A-104", "Detail 6", "A-103"]);
    let e = fails(&mut a, "bookmark_delete", json!({ "path": [9] }));
    assert!(e.contains("no bookmark"), "{e}");
    let e = fails(&mut a, "bookmark_add", json!({ "page": 9 }));
    assert!(e.contains("does not exist"), "{e}");

    // Saved and reopened, the outline is in the file; undo works before that.
    call(&mut a, "edit_undo", json!({}));
    assert_eq!(count(&call(&mut a, "bookmark_list", json!({})), "bookmarks"), 5);
    call(&mut a, "edit_redo", json!({}));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "set.pdf" }));
    let list = call(&mut a, "bookmark_list", json!({}));
    assert_eq!(titles(&list), ["A-101", "A-104", "Detail 6", "A-103"]);

    // Replace with page-number titles; then delete them all.
    let v = call(
        &mut a,
        "bookmark_create",
        json!({ "titles": "numbers", "replace": true, "pages": [1, 3] }),
    );
    assert_eq!(titles(&v), ["Page 1", "Page 3"]);
    assert_eq!(call(&mut a, "bookmark_delete", json!({ "all": true }))["deleted"], 2);
    assert_eq!(count(&call(&mut a, "bookmark_list", json!({})), "bookmarks"), 0);
}

#[test]
fn page_labels_number_set_clear_and_from_bookmarks() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "labels.pdf", 5);
    let v = call(
        &mut a,
        "page_label_number",
        json!({ "pages": "1-2", "style": "roman_lower" }),
    );
    assert_eq!(v["labels"], json!(["i", "ii", "3", "4", "5"]));
    let v = call(
        &mut a,
        "page_label_number",
        json!({ "pages": "3-5", "style": "decimal", "prefix": "S-", "start": 7 }),
    );
    assert_eq!(v["labels"], json!(["i", "ii", "S-7", "S-8", "S-9"]));
    let v = call(&mut a, "page_label_set", json!({ "labels": { "1": "Cover", "4": "" } }));
    assert_eq!(v["labels"], json!(["Cover", "ii", "S-7", "4", "S-9"]));
    let e = fails(&mut a, "page_label_set", json!({ "labels": { "0": "x" } }));
    assert!(e.contains("page number"), "{e}");
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "labels.pdf" }));
    assert_eq!(
        call(&mut a, "page_labels", json!({}))["labels"],
        json!(["Cover", "ii", "S-7", "4", "S-9"])
    );

    // Labels from bookmarks: only pages a bookmark goes to change.
    call(&mut a, "bookmark_add", json!({ "page": 2, "title": "Plans" }));
    call(&mut a, "bookmark_add", json!({ "page": 5, "title": "Details" }));
    let v = call(&mut a, "page_label_from_bookmarks", json!({}));
    assert_eq!(v["labelled"], 2);
    assert_eq!(v["labels"], json!(["Cover", "Plans", "S-7", "4", "Details"]));

    let v = call(&mut a, "page_label_clear", json!({}));
    assert_eq!(v["cleared"], true);
    assert_eq!(v["labels"], json!(["", "", "", "", ""]));
    call(&mut a, "edit_undo", json!({}));
    assert_eq!(call(&mut a, "page_labels", json!({}))["labels"][1], "Plans");
}

#[test]
fn text_search_finds_hits_with_rectangles() {
    let d = dir();
    write_text_pdf(
        &d,
        "plans.pdf",
        &[
            &[(100.0, 700.0, "Kitchen Sink"), (100.0, 600.0, "Bathroom")],
            &[(72.0, 400.0, "Second kitchen"), (72.0, 300.0, "kitchenette")],
        ],
    );
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "plans.pdf" }));
    let v = call(&mut a, "text_search", json!({ "text": "kitchen" }));
    assert_eq!(v["count"], 3, "{v}");
    let first = &v["hits"][0];
    assert_eq!(first["page"], 1);
    assert_eq!(first["text"], "Kitchen");
    assert!(first["context"].as_str().unwrap().contains("Sink"));
    let r: Vec<f64> = first["rects"][0]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect();
    assert!((95.0..105.0).contains(&r[0]), "{r:?}");
    assert!(r[2] > r[0] + 40.0 && r[2] < 180.0, "{r:?}");
    assert!(
        (685.0..705.0).contains(&r[1]) && (705.0..730.0).contains(&r[3]),
        "{r:?}"
    );

    let v = call(
        &mut a,
        "text_search",
        json!({ "text": "Kitchen", "case_sensitive": true }),
    );
    assert_eq!(v["count"], 1);
    let v = call(
        &mut a,
        "text_search",
        json!({ "text": "kitchen", "whole_words": true, "pages": [2] }),
    );
    assert_eq!(v["count"], 1);
    assert_eq!(v["hits"][0]["page"], 2);
    assert_eq!(v["pages_searched"], 1);
    let v = call(&mut a, "text_search", json!({ "text": "kitchen", "max_hits": 2 }));
    assert_eq!(v["count"], 2);
    assert_eq!(v["truncated"], true);
    assert_eq!(call(&mut a, "text_search", json!({ "text": "garage" }))["count"], 0);
    let e = fails(&mut a, "text_search", json!({ "text": "  " }));
    assert!(e.contains("search for"), "{e}");
    let t = call(&mut a, "page_text", json!({ "page": 1 }));
    assert!(t["text"].as_str().unwrap().contains("Bathroom"), "{t}");
}

#[test]
fn links_to_pages_urls_and_files() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "links.pdf", 3);
    let m = rect_markup(&mut a, 1, 300.0, 300.0, "Keep");
    let to_page = call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [72, 72, 144, 96], "to_page": 3 }),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [72, 120, 144, 140], "url": "https://example.com/spec", "border_width": 1, "color": "#FF0000" }),
    );
    call(
        &mut a,
        "link_add",
        json!({ "page": 2, "rect": [72, 72, 200, 90], "file": "Specs/Section 09.pdf", "file_page": 4 }),
    );
    call(
        &mut a,
        "link_add",
        json!({ "page": 2, "rect": [72, 100, 200, 120], "file": "schedule.xlsx" }),
    );
    let e = fails(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [0, 0, 10, 10], "url": "x", "to_page": 2 }),
    );
    assert!(e.contains("exactly one"), "{e}");
    let e = fails(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [0, 0, 10, 10], "to_page": 9 }),
    );
    assert!(e.contains("does not exist"), "{e}");

    // The markup is untouched; links are not markups.
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], 1);
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "links.pdf" }));
    let list = call(&mut a, "link_list", json!({}));
    let links = list["links"].as_array().unwrap();
    assert_eq!(links.len(), 4, "{list}");
    assert_eq!(links[0]["target"], json!({ "page": 3 }));
    assert_eq!(links[0]["rect"], json!([72.0, 72.0, 144.0, 96.0]));
    assert_eq!(links[1]["target"], json!({ "url": "https://example.com/spec" }));
    assert_eq!(
        links[2]["target"],
        json!({ "file": "Specs/Section 09.pdf", "file_page": 4 })
    );
    assert_eq!(links[3]["target"]["file"], "schedule.xlsx");
    let markups = call(&mut a, "markup_list", json!({}));
    assert_eq!(markups["count"], 1);
    assert_eq!(markups["markups"][0]["id"], m);

    assert_eq!(call(&mut a, "link_delete", json!({ "ids": [to_page] }))["deleted"], 1);
    assert_eq!(count(&call(&mut a, "link_list", json!({})), "links"), 3);
    let e = fails(&mut a, "link_delete", json!({ "ids": ["nope"] }));
    assert!(e.contains("no link"), "{e}");
}

#[test]
fn attachments_add_list_extract_delete() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "att.pdf", 1);
    std::fs::write(d.join("takeoff.csv"), "Item,Qty\nDuct,12\n").unwrap();
    std::fs::write(d.join("notes.txt"), vec![b'n'; 5000]).unwrap();
    call(
        &mut a,
        "attachment_add",
        json!({ "path": "takeoff.csv", "description": "Quantities" }),
    );
    call(
        &mut a,
        "attachment_add",
        json!({ "path": "notes.txt", "name": "Field notes" }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "att.pdf" }));
    let list = call(&mut a, "attachment_list", json!({}));
    let items = list["attachments"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{list}");
    assert_eq!(items[0]["name"], "Field notes");
    assert_eq!(items[0]["file"], "notes.txt");
    assert_eq!(items[0]["size"], 5000);
    assert_eq!(items[1]["name"], "takeoff.csv");
    assert_eq!(items[1]["description"], "Quantities");

    let v = call(
        &mut a,
        "attachment_extract",
        json!({ "name": "takeoff.csv", "out": "out.csv" }),
    );
    assert_eq!(v["bytes"], 17);
    assert_eq!(
        std::fs::read_to_string(d.join("out.csv")).unwrap(),
        "Item,Qty\nDuct,12\n"
    );
    call(&mut a, "attachment_delete", json!({ "name": "Field notes" }));
    assert_eq!(count(&call(&mut a, "attachment_list", json!({})), "attachments"), 1);
    let e = fails(
        &mut a,
        "attachment_extract",
        json!({ "name": "Field notes", "out": "x" }),
    );
    assert!(e.contains("no attachment"), "{e}");
    let e = fails(&mut a, "attachment_add", json!({ "path": "missing.bin" }));
    assert!(!e.is_empty());
}

#[test]
fn document_properties_and_xmp() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "props.pdf", 1);
    call(
        &mut a,
        "doc_properties_set",
        json!({ "properties": { "Title": "Level 2 Mechanical", "Author": "Estimator", "Keywords": "hvac, duct", "Project": "Tower" } }),
    );
    let e = fails(
        &mut a,
        "doc_properties_set",
        json!({ "properties": { "Bad Key": "x" } }),
    );
    assert!(e.contains("property name"), "{e}");
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "props.pdf" }));
    let v = call(&mut a, "doc_properties", json!({ "xmp": true }));
    let p = &v["properties"];
    assert_eq!(p["standard"]["Title"], "Level 2 Mechanical");
    assert_eq!(p["standard"]["Author"], "Estimator");
    assert_eq!(p["custom"]["Project"], "Tower");
    assert_eq!(p["has_xmp"], true);
    let xmp = p["xmp"].as_str().unwrap();
    assert!(
        xmp.contains("<rdf:li xml:lang=\"x-default\">Level 2 Mechanical</rdf:li>"),
        "{xmp}"
    );
    assert!(xmp.contains("<pdf:Keywords>hvac, duct</pdf:Keywords>"), "{xmp}");

    // Removing a value removes it from both places.
    call(
        &mut a,
        "doc_properties_set",
        json!({ "properties": { "Author": null, "Project": "" } }),
    );
    let v = call(&mut a, "doc_properties", json!({ "xmp": true }));
    assert!(v["properties"]["standard"].get("Author").is_none());
    assert!(v["properties"]["custom"].get("Project").is_none());
    assert!(!v["properties"]["xmp"].as_str().unwrap().contains("Estimator"));
}

#[test]
fn headers_footers_watermarks_and_bates() {
    let d = dir();
    write_text_pdf(
        &d,
        "marks.pdf",
        &[
            &[(72.0, 400.0, "One")],
            &[(72.0, 400.0, "Two")],
            &[(72.0, 400.0, "Three")],
        ],
    );
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "marks.pdf" }));
    call(
        &mut a,
        "header_footer_add",
        json!({ "header_right": "<<Page 1 of n>>", "footer_left": "Issued for review" }),
    );
    call(
        &mut a,
        "watermark_add",
        json!({ "text": "PRELIMINARY", "rotation": 0, "opacity": 0.3, "pages": [1] }),
    );
    let v = call(
        &mut a,
        "bates_add",
        json!({ "prefix": "ABC", "start": 5, "digits": 6, "position": "footer_center" }),
    );
    assert_eq!(v["first"], "ABC000005");
    assert_eq!(v["last"], "ABC000007");
    let e = fails(&mut a, "bates_add", json!({ "digits": 1, "start": 9 }));
    assert!(e.contains("digits"), "{e}");

    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "marks.pdf" }));
    let hits = |a: &mut Automation, t: &str| -> Vec<u64> {
        call(a, "text_search", json!({ "text": t }))["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["page"].as_u64().unwrap())
            .collect()
    };
    assert_eq!(hits(&mut a, "Page 2 of 3"), [2]);
    assert_eq!(hits(&mut a, "Issued for review"), [1, 2, 3]);
    assert_eq!(hits(&mut a, "PRELIMINARY"), [1]);
    assert_eq!(hits(&mut a, "ABC000006"), [2]);
    assert_eq!(hits(&mut a, "Two"), [2], "the page's own text is still there");

    let v = call(&mut a, "marks_remove", json!({ "kind": "watermark" }));
    assert_eq!(v["removed"], 1);
    assert_eq!(v["present"], json!(["header_footer"]));
    assert!(hits(&mut a, "PRELIMINARY").is_empty());
    call(&mut a, "marks_remove", json!({ "kind": "header_footer", "pages": [3] }));
    assert_eq!(hits(&mut a, "Issued for review"), [1, 2]);
    assert_eq!(hits(&mut a, "Three"), [3]);
}

#[test]
fn flatten_markups_by_filter() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "flat.pdf", 2);
    let keep = rect_markup(&mut a, 1, 100.0, 100.0, "Keep");
    let burn = rect_markup(&mut a, 1, 300.0, 300.0, "Burn");
    let other = rect_markup(&mut a, 2, 100.0, 100.0, "Burn");
    let e = fails(&mut a, "markup_flatten", json!({}));
    assert!(e.contains("all: true"), "{e}");
    let v = call(&mut a, "markup_flatten", json!({ "ids": [burn], "pages": [1] }));
    assert_eq!(v["flattened"], 1);
    let list = call(&mut a, "markup_list", json!({}));
    let ids: Vec<&str> = list["markups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [keep.as_str(), other.as_str()]);
    call(&mut a, "edit_undo", json!({}));
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], 3);
    call(&mut a, "edit_redo", json!({}));
    call(&mut a, "doc_save", json!({}));

    // The appearance is now page content.
    let s = markupcraft_engine::Session::open(d.join("flat.pdf")).unwrap();
    assert_eq!(s.doc().markups.len(), 2);
    let cos = &s.pdf().cos;
    let root = cos.root().unwrap();
    let pages = cos
        .dict(cos.get(root).as_dict().unwrap().get(b"Pages").unwrap())
        .unwrap();
    let kids = cos.resolve(pages.get(b"Kids").unwrap());
    let page = cos.dict(&kids.as_array().unwrap()[0]).unwrap();
    let res = cos.dict(page.get(b"Resources").unwrap()).unwrap();
    let xo = cos.dict(res.get(b"XObject").unwrap()).unwrap();
    assert!(
        xo.iter().any(|(k, _)| k.starts_with(b"MCFlat")),
        "flattened appearance missing"
    );

    // By kind and author: everything left.
    let v = call(
        &mut a,
        "markup_flatten",
        json!({ "kinds": ["rectangle"], "authors": ["Tester"] }),
    );
    assert_eq!(v["flattened"], 2);
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], 0);
    let e = fails(&mut a, "markup_flatten", json!({ "all": true }));
    assert!(e.contains("no markup"), "{e}");
}

#[test]
fn xfdf_export_import_round_trip() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "from.pdf", 2);
    rect_markup(&mut a, 1, 100.0, 100.0, "Slab");
    call(
        &mut a,
        "markup_add",
        json!({ "page": 2, "kind": "Line", "points": [[10, 10], [200, 10]], "subject": "Wall", "contents": "check" }),
    );
    let v = call(&mut a, "xfdf_export", json!({ "out": "marks.xfdf" }));
    assert_eq!(v["markups"], 2);
    let text = std::fs::read_to_string(d.join("marks.xfdf")).unwrap();
    assert!(
        text.contains("<xfdf") && text.contains("<square") && text.contains("<line"),
        "{text}"
    );
    call(&mut a, "xfdf_export", json!({ "out": "marks.fdf" }));
    assert!(std::fs::read(d.join("marks.fdf")).unwrap().starts_with(b"%FDF"));

    new_doc(&mut a, "to.pdf", 2);
    let v = call(&mut a, "xfdf_import", json!({ "path": "marks.xfdf" }));
    assert_eq!(v["imported"], 2);
    assert_eq!(v["markups_after"], 2);
    let list = call(&mut a, "markup_list", json!({}));
    let subjects: Vec<&str> = list["markups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["subject"].as_str().unwrap())
        .collect();
    assert_eq!(subjects, ["Slab", "Wall"]);
    assert_eq!(list["markups"][1]["page"], 2);
    assert_eq!(list["markups"][1]["contents"], "check");
    // Importing again replaces (same ids), it does not duplicate.
    call(&mut a, "xfdf_import", json!({ "path": "marks.fdf" }));
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], 2);
    std::fs::write(d.join("junk.xfdf"), "hello").unwrap();
    assert!(!fails(&mut a, "xfdf_import", json!({ "path": "junk.xfdf" })).is_empty());
}

#[test]
fn security_passwords_encrypt_open_and_remove() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "secret.pdf", 1);
    rect_markup(&mut a, 1, 100.0, 100.0, "Private");
    let e = fails(&mut a, "security_set", json!({}));
    assert!(e.contains("password"), "{e}");
    call(
        &mut a,
        "security_set",
        json!({ "open_password": "open-me", "permissions_password": "owner!", "print": false, "modify": false }),
    );
    let v = call(&mut a, "security_info", json!({}));
    assert_eq!(v["security"]["save_encrypted"], true);
    call(&mut a, "doc_save", json!({}));
    // Saving again after the protected save still works.
    rect_markup(&mut a, 1, 200.0, 200.0, "Second");
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));

    let e = fails(&mut a, "doc_open", json!({ "path": "secret.pdf" }));
    assert!(e.to_lowercase().contains("password"), "{e}");
    let e = fails(
        &mut a,
        "doc_open_protected",
        json!({ "path": "secret.pdf", "password": "guess" }),
    );
    assert!(e.contains("incorrect"), "{e}");
    let v = call(
        &mut a,
        "doc_open_protected",
        json!({ "path": "secret.pdf", "password": "open-me" }),
    );
    assert_eq!(v["document"]["markups"], 2);
    let sec = &v["security"];
    assert_eq!(sec["encrypted"], true);
    assert_eq!(sec["owner"], false);
    assert_eq!(sec["permissions"]["print"], false);
    assert_eq!(sec["permissions"]["copy"], true);
    assert!(sec["method"].as_str().unwrap().contains("AES-256"), "{sec}");
    let e = fails(&mut a, "security_remove", json!({}));
    assert!(e.contains("permissions password"), "{e}");
    call(&mut a, "doc_close", json!({}));

    let v = call(
        &mut a,
        "doc_open_protected",
        json!({ "path": "secret.pdf", "password": "owner!" }),
    );
    assert_eq!(v["security"]["owner"], true);
    call(&mut a, "security_remove", json!({}));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    let v = call(&mut a, "doc_open", json!({ "path": "secret.pdf" }));
    assert_eq!(v["markups"], 2);
    assert_eq!(call(&mut a, "security_info", json!({}))["security"]["encrypted"], false);

    // An owner-only password opens without a password and restricts.
    call(
        &mut a,
        "security_set",
        json!({ "permissions_password": "boss", "copy": false, "encryption": "aes128" }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "secret.pdf" }));
    let v = call(&mut a, "security_info", json!({}));
    assert_eq!(v["security"]["encrypted"], true);
    assert_eq!(v["security"]["permissions"]["copy"], false);
}

#[test]
fn reduce_file_size_reports_sizes() {
    let d = dir();
    let lines: Vec<(f64, f64, &str)> = (0..40)
        .map(|i| {
            (
                72.0,
                750.0 - 18.0 * i as f64,
                "Repeated line of plain text that compresses well",
            )
        })
        .collect();
    write_text_pdf(&d, "big.pdf", &[&lines, &lines]);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "big.pdf" }));
    rect_markup(&mut a, 1, 100.0, 100.0, "Keep");
    let v = call(&mut a, "doc_reduce_size", json!({ "target_ppi": 100 }));
    assert!(v["streams_compressed"].as_u64().unwrap() >= 2, "{v}");
    assert!(
        v["bytes_after"].as_u64().unwrap() < v["bytes_before"].as_u64().unwrap(),
        "{v}"
    );
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], 1);
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "big.pdf" }));
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], 1);
    let hits = call(&mut a, "text_search", json!({ "text": "compresses well" }));
    assert_eq!(hits["count"], 80);
    let e = fails(&mut a, "doc_reduce_size", json!({ "quality": 101 }));
    assert!(e.contains("quality"), "{e}");
}

#[test]
fn print_pdf_fit_nup_and_tile() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "print.pdf", 4);
    rect_markup(&mut a, 1, 100.0, 100.0, "Shown");
    let v = call(&mut a, "print_pdf", json!({ "out": "fit.pdf", "paper": "tabloid" }));
    assert_eq!(v["sheets"], 4);
    let v = call(
        &mut a,
        "print_pdf",
        json!({ "out": "nup.pdf", "layout": "nup", "cols": 2, "rows": 2, "border": true }),
    );
    assert_eq!(v["sheets"], 1);
    let v = call(
        &mut a,
        "print_pdf",
        json!({ "out": "tile.pdf", "layout": "tile", "percent": 200, "pages": [2], "paper": "letter" }),
    );
    assert!(v["sheets"].as_u64().unwrap() >= 4, "{v}");
    let v = call(
        &mut a,
        "print_pdf",
        json!({ "out": "order.pdf", "pages": [3, 1], "paper": [792, 612], "orientation": "landscape" }),
    );
    assert_eq!(v["sheets"], 2);
    let e = fails(&mut a, "print_pdf", json!({ "out": "print.pdf" }));
    assert!(e.contains("different file"), "{e}");
    let e = fails(&mut a, "print_pdf", json!({ "out": "x.pdf", "paper": "napkin" }));
    assert!(e.contains("unknown paper"), "{e}");
    // The sheets open as ordinary PDFs; the document itself is unchanged.
    let v = call(&mut a, "doc_open", json!({ "path": "nup.pdf" }));
    assert_eq!(v["pages"], 1);
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(info["page_list"][0]["width"], 612.0);
}

#[test]
fn split_and_combine_documents() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "whole.pdf", 5);
    rect_markup(&mut a, 3, 100.0, 100.0, "On three");
    call(&mut a, "page_label_number", json!({ "prefix": "M-" }));
    for f in ["parts", "ranges", "marks"] {
        std::fs::create_dir_all(d.join(f)).unwrap();
    }
    let v = call(&mut a, "doc_split", json!({ "dir": "parts", "pages_per_file": 2 }));
    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    assert_eq!(files[1]["pages"], json!([3, 4]));
    assert!(files[0]["path"].as_str().unwrap().ends_with("whole-1.pdf"));

    // Ranges and bookmarks.
    let v = call(&mut a, "doc_split", json!({ "dir": "ranges", "ranges": ["1", "2-5"] }));
    assert_eq!(v["files"][1]["pages"], json!([2, 3, 4, 5]));
    call(&mut a, "bookmark_add", json!({ "page": 2, "title": "Plans" }));
    call(&mut a, "bookmark_add", json!({ "page": 4, "title": "Sections" }));
    let v = call(&mut a, "doc_split", json!({ "dir": "marks", "by": "bookmarks" }));
    let names: Vec<&str> = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 3);
    assert!(names[1].ends_with("whole-2 Plans.pdf"), "{names:?}");
    let e = fails(&mut a, "doc_split", json!({ "dir": "parts" }));
    assert!(e.contains("pages_per_file"), "{e}");

    let v = call(&mut a, "doc_open", json!({ "path": "parts/whole-2.pdf" }));
    assert_eq!(v["pages"], 2);
    assert_eq!(v["markups"], 1);
    assert_eq!(call(&mut a, "page_labels", json!({}))["labels"], json!(["M-3", "M-4"]));

    let v = call(
        &mut a,
        "doc_combine",
        json!({ "files": ["parts/whole-1.pdf", "parts/whole-2.pdf", "parts/whole-3.pdf"], "out": "again.pdf" }),
    );
    assert_eq!(v["pages"], 5);
    call(&mut a, "doc_open", json!({ "path": "again.pdf" }));
    let list = call(&mut a, "markup_list", json!({}));
    assert_eq!(list["count"], 1);
    assert_eq!(list["markups"][0]["page"], 3);
    let b = call(&mut a, "bookmark_list", json!({}));
    assert_eq!(titles(&b), ["whole-1", "whole-2", "whole-3"]);
    assert_eq!(b["bookmarks"][2]["page"], 5);
    assert_eq!(
        call(&mut a, "page_labels", json!({}))["labels"],
        json!(["M-1", "M-2", "M-3", "M-4", "M-5"])
    );
    let e = fails(
        &mut a,
        "doc_combine",
        json!({ "files": ["parts/whole-1.pdf"], "out": "one.pdf" }),
    );
    assert!(e.contains("two files"), "{e}");
}

#[test]
fn replace_pages_keeps_markups() {
    let d = dir();
    write_text_pdf(&d, "revB.pdf", &[&[(72.0, 700.0, "Revision B drawing")]]);
    let mut a = auto(&d);
    new_doc(&mut a, "sheets.pdf", 3);
    let m = rect_markup(&mut a, 2, 100.0, 100.0, "Comment");
    call(&mut a, "bookmark_add", json!({ "page": 2, "title": "Sheet 2" }));
    let e = fails(
        &mut a,
        "page_replace",
        json!({ "pages": [1, 2], "path": "revB.pdf", "source_pages": [1] }),
    );
    assert!(e.contains("as many"), "{e}");
    call(&mut a, "page_replace", json!({ "pages": [2], "path": "revB.pdf" }));
    let v = call(&mut a, "text_search", json!({ "text": "Revision B" }));
    assert_eq!(v["count"], 1);
    assert_eq!(v["hits"][0]["page"], 2);
    let list = call(&mut a, "markup_list", json!({ "page": 2 }));
    assert_eq!(list["markups"][0]["id"], m);
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "sheets.pdf" }));
    assert_eq!(call(&mut a, "markup_list", json!({ "page": 2 }))["count"], 1);
    assert_eq!(call(&mut a, "bookmark_list", json!({}))["bookmarks"][0]["page"], 2);
    assert_eq!(call(&mut a, "text_search", json!({ "text": "Revision B" }))["count"], 1);
}

#[test]
fn crop_boxes_and_page_resize() {
    let d = dir();
    let mut a = auto(&d);
    new_doc(&mut a, "boxes.pdf", 2);
    call(
        &mut a,
        "scale_set",
        json!({ "scale": { "kind": "architectural", "paper_inches": 0.125, "real_feet": 1 } }),
    );
    let line = call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Length", "points": [[100, 100], [172, 100]] }),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    let quantity = |a: &mut Automation, id: &str| -> Value {
        call(a, "markup_list", json!({}))["markups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == id)
            .unwrap()["quantity_text"]
            .clone()
    };
    let before = quantity(&mut a, &line);

    call(
        &mut a,
        "page_crop",
        json!({ "pages": [1], "margins": [36, 36, 36, 36] }),
    );
    let v = call(&mut a, "page_boxes", json!({ "pages": [1] }));
    assert_eq!(v["pages"][0]["crop"], json!([36.0, 36.0, 576.0, 756.0]));
    assert_eq!(v["pages"][0]["media"], json!([0.0, 0.0, 612.0, 792.0]));
    call(&mut a, "page_crop", json!({ "pages": [1], "remove": true }));
    let v = call(&mut a, "page_boxes", json!({ "pages": [1] }));
    assert_eq!(v["pages"][0]["crop"], json!([0.0, 0.0, 612.0, 792.0]));
    call(
        &mut a,
        "page_crop",
        json!({ "pages": [2], "box": "trim", "rect": [10, 10, 300, 300] }),
    );
    assert_eq!(
        call(&mut a, "page_boxes", json!({ "pages": [2] }))["pages"][0]["trim"],
        json!([10.0, 10.0, 300.0, 300.0])
    );

    call(&mut a, "page_resize", json!({ "pages": [1], "paper": "tabloid" }));
    let v = call(&mut a, "page_boxes", json!({ "pages": [1] }));
    assert_eq!(v["pages"][0]["media"], json!([-90.0, -216.0, 702.0, 1008.0]));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "boxes.pdf" }));
    let info = call(&mut a, "doc_info", json!({}));
    let p1 = &info["page_list"][0];
    assert_eq!(p1["width"], 792.0);
    assert_eq!(p1["height"], 1224.0);
    // The markup and the page scale did not move or change.
    let m = call(&mut a, "markup_list", json!({}));
    assert_eq!(m["markups"][0]["points"], json!([[100.0, 100.0], [172.0, 100.0]]));
    assert_eq!(quantity(&mut a, &line), before);
    let vps = p1["viewports"].as_array().unwrap();
    assert_eq!(vps.len(), 1, "{info}");
    assert_eq!(vps[0]["bbox"], json!([-90.0, -216.0, 702.0, 1008.0]));

    call(
        &mut a,
        "page_resize",
        json!({ "pages": [2], "width": 300, "height": 400, "anchor": "bottom_left" }),
    );
    let v = call(&mut a, "page_boxes", json!({ "pages": [2] }));
    assert_eq!(v["pages"][0]["media"], json!([0.0, 0.0, 300.0, 400.0]));
    let e = fails(&mut a, "page_resize", json!({ "width": 1 , "height": 5 }));
    assert!(e.contains("page sides"), "{e}");
}
