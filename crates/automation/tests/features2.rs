//! End-to-end tests of export, archive, flatten recovery, compare and overlay extras and the
//! batch compare tools, through `Automation::call`, on PDFs built here.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use markupcraft_engine::synthetic::{SyntheticPage, line, pdf, rect, text};
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-features2-{}-{}",
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

fn write(d: &Path, name: &str, pages: &[SyntheticPage]) -> PathBuf {
    let p = d.join(name);
    std::fs::write(&p, pdf(pages)).unwrap();
    p
}

fn letter(content: String) -> SyntheticPage {
    SyntheticPage::new(612.0, 792.0, content)
}

fn schedule() -> String {
    let mut c = text(72.0, 720.0, 16.0, "EQUIPMENT SCHEDULE");
    for (i, (a, b, m)) in [
        ("TAG", "CFM", "MODEL"),
        ("AHU-1", "1,200", "XR40"),
        ("AHU-2", "950", "XR30"),
    ]
    .iter()
    .enumerate()
    {
        let y = 650.0 - 20.0 * i as f64;
        c.push_str(&text(72.0, y, 10.0, a));
        c.push_str(&text(200.0, y, 10.0, b));
        c.push_str(&text(320.0, y, 10.0, m));
    }
    c.push_str(&line(60.0, 100.0, 550.0, 100.0, 2.0));
    c
}

#[test]
fn export_images_office_formats_and_page_region() {
    let d = dir();
    let mut a = auto(&d);
    write(
        &d,
        "plan.pdf",
        &[letter(schedule()), letter(text(72.0, 700.0, 20.0, "SHEET TWO"))],
    );
    call(&mut a, "doc_open", json!({ "path": "plan.pdf" }));
    std::fs::create_dir_all(d.join("img")).unwrap();
    let magic: [(&str, &[u8]); 4] = [
        ("png", b"\x89PNG"),
        ("jpg", b"\xFF\xD8"),
        ("tif", b"II"),
        ("bmp", b"BM"),
    ];
    for (fmt, head) in magic {
        let v = call(
            &mut a,
            "export_images",
            json!({ "dir": "img", "format": fmt, "dpi": 36, "suffix": "-p" }),
        );
        let files = v["files"].as_array().unwrap();
        assert_eq!(files.len(), 2, "{v}");
        let first = files[0].as_str().unwrap();
        assert!(first.ends_with(&format!("plan-p1.{fmt}")), "{first}");
        let bytes = std::fs::read(first).unwrap();
        assert!(bytes.starts_with(head), "{fmt}");
    }
    let one = call(
        &mut a,
        "export_images",
        json!({ "dir": "img", "pages": [2], "name": "only" }),
    );
    assert_eq!(one["files"].as_array().unwrap().len(), 1);
    assert!(fails(&mut a, "export_images", json!({ "dir": "img", "dpi": 5000 })).contains("dpi"));

    for (out, head) in [
        ("plan.docx", "PK"),
        ("plan.xlsx", "PK"),
        ("plan.pptx", "PK"),
        ("plan.html", "<"),
        ("plan.rtf", "{\\rtf"),
    ] {
        call(&mut a, "export_document", json!({ "out": out }));
        let b = std::fs::read(d.join(out)).unwrap();
        assert!(b.starts_with(head.as_bytes()), "{out}");
    }
    call(&mut a, "export_document", json!({ "out": "plan.txt", "pages": [1] }));
    let txt = std::fs::read_to_string(d.join("plan.txt")).unwrap();
    assert!(
        txt.contains("EQUIPMENT SCHEDULE") && !txt.contains("SHEET TWO"),
        "{txt}"
    );
    let html = std::fs::read_to_string(d.join("plan.html")).unwrap();
    assert!(html.contains("SHEET TWO"));
    // The workbook holds the schedule as cells, numbers as numbers.
    let xlsx = std::fs::read(d.join("plan.xlsx")).unwrap();
    assert!(
        xlsx.windows(b"worksheets/sheet2.xml".len())
            .any(|w| w == b"worksheets/sheet2.xml")
    );

    let v = call(
        &mut a,
        "export_region",
        json!({ "page": 1, "rect": [60, 600, 420, 665], "out": "schedule.csv" }),
    );
    assert_eq!(
        v["rows"],
        json!([
            ["TAG", "CFM", "MODEL"],
            ["AHU-1", "1,200", "XR40"],
            ["AHU-2", "950", "XR30"]
        ])
    );
    let csv = std::fs::read_to_string(d.join("schedule.csv")).unwrap();
    assert!(csv.contains("AHU-1,\"1,200\",XR40"), "{csv}");
    call(
        &mut a,
        "export_region",
        json!({ "page": 1, "rect": [60, 600, 420, 665], "out": "schedule.xlsx" }),
    );
    assert!(std::fs::read(d.join("schedule.xlsx")).unwrap().starts_with(b"PK"));
    assert!(
        fails(
            &mut a,
            "export_region",
            json!({ "page": 1, "rect": [0, 0, 30, 30], "out": "e.xlsx" })
        )
        .contains("no text")
    );
}

#[test]
fn repair_archive_pdfa_and_color_processing() {
    let d = dir();
    let mut a = auto(&d);
    write(
        &d,
        "c.pdf",
        &[letter(format!("1 0 0 rg {}", rect(100.0, 100.0, 200.0, 200.0)))],
    );
    call(&mut a, "doc_open", json!({ "path": "c.pdf" }));
    let r = call(&mut a, "doc_repair", json!({}));
    assert!(r["bytes_after"].as_u64().unwrap() > 0);
    let v = call(&mut a, "doc_pdfa", json!({ "action": "verify" }));
    assert_eq!(v["conforming"], false);
    let v = call(&mut a, "doc_pdfa", json!({ "action": "archive", "level": "2b" }));
    assert!(!v["fixed"].as_array().unwrap().is_empty(), "{v}");
    let v = call(&mut a, "doc_pdfa", json!({ "action": "verify" }));
    assert_eq!(v["declared"], "PDF/A-2b", "{v}");
    let v = call(&mut a, "doc_pdfa", json!({ "action": "unlock" }));
    assert_eq!(v["unlocked"], true);
    let v = call(&mut a, "doc_color_process", json!({ "mode": "grayscale" }));
    assert_eq!(v["pages"], 1);
    call(&mut a, "doc_save", json!({ "full": true }));
    let s = markupcraft_engine::Session::open(d.join("c.pdf")).unwrap();
    let px = s
        .renderable(false)
        .unwrap()
        .render_rgba(0, 0.5)
        .unwrap()
        .pixel(200.0, 200.0);
    assert!(px[0].abs_diff(px[1]) < 8, "gray after save: {px:?}");
    let v = call(&mut a, "doc_color_process", json!({ "mode": "remove" }));
    assert_eq!(v["pages"], 1);
    assert!(fails(&mut a, "doc_color_process", json!({ "mode": "tint" })).contains("color"));
    call(
        &mut a,
        "doc_color_process",
        json!({ "mode": "tint", "color": "#0000FF" }),
    );
    call(
        &mut a,
        "doc_color_process",
        json!({ "mode": "lighten", "amount": 0.5, "pages": [1] }),
    );
}

#[test]
fn recoverable_flatten_onto_a_layer_and_unflatten() {
    let d = dir();
    let mut a = auto(&d);
    write(&d, "f.pdf", &[letter(String::new()), letter(String::new())]);
    call(&mut a, "doc_open", json!({ "path": "f.pdf" }));
    for p in [1, 2] {
        call(
            &mut a,
            "markup_add",
            json!({ "page": p, "kind": "Rectangle", "points": [[100, 100], [200, 200]] }),
        );
    }
    let v = call(
        &mut a,
        "markup_flatten",
        json!({ "all": true, "recoverable": true, "layer": "Flattened" }),
    );
    assert_eq!(v["flattened"], 2);
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "f.pdf" }));
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], 0);
    let layers = call(&mut a, "layer_list", json!({}));
    assert!(layers.to_string().contains("Flattened"));
    let v = call(&mut a, "markup_unflatten", json!({ "pages": [2] }));
    assert_eq!(v["restored"], 1);
    let v = call(&mut a, "markup_unflatten", json!({}));
    assert_eq!(v["restored"], 1);
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], 2);
    assert!(fails(&mut a, "markup_unflatten", json!({})).contains("no recoverable"));
}

fn sheet(dx: f64, dy: f64, extra: bool) -> String {
    let mut c = format!(
        "{}{}{}",
        line(60.0 + dx, 80.0 + dy, 540.0 + dx, 80.0 + dy, 3.0),
        line(60.0 + dx, 80.0 + dy, 60.0 + dx, 700.0 + dy, 3.0),
        rect(200.0 + dx, 300.0 + dy, 80.0, 40.0)
    );
    if extra {
        c.push_str(&rect(400.0 + dx, 500.0 + dy, 60.0, 60.0));
    }
    c
}

#[test]
fn compare_alignment_appearance_and_presets() {
    let d = dir();
    let mut a = auto(&d);
    write(&d, "old.pdf", &[letter(sheet(0.0, 0.0, false))]);
    write(&d, "new.pdf", &[letter(sheet(20.0, 10.0, true))]);
    call(&mut a, "doc_open", json!({ "path": "new.pdf" }));
    let base = json!({ "old": "old.pdf", "mode": "graphics" });
    let mut args = base.clone();
    args["align"] = json!("offset");
    args["offset"] = json!([20, 10]);
    args["fill"] = json!("#FFFF00");
    args["opacity"] = json!(0.8);
    args["lock"] = json!(true);
    let v = call(&mut a, "compare_documents", args);
    assert_eq!(v["changes"], 1, "only the added square: {v}");
    let id = v["regions"][0]["id"].as_str().unwrap().to_string();
    let m = call(&mut a, "markup_list", json!({}));
    let mk = m["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == id.as_str())
        .unwrap()
        .clone();
    assert_eq!(
        (mk["locked"].clone(), mk["fill"].clone()),
        (json!(true), json!("#FFFF00"))
    );
    call(&mut a, "edit_undo", json!({}));
    let mut auto_args = base.clone();
    auto_args["preset"] = json!("scanned");
    let v = call(&mut a, "compare_documents", auto_args);
    assert_eq!(v["changes"], 1, "auto align finds the shift: {v}");
    call(&mut a, "edit_undo", json!({}));
    let v = call(&mut a, "compare_documents", base.clone());
    let r = &v["regions"][0]["rect"];
    let w = r[2].as_f64().unwrap() - r[0].as_f64().unwrap();
    assert!(w > 300.0, "stacked as-is the whole shifted sheet differs: {v}");

    let p = call(
        &mut a,
        "compare_preset",
        json!({ "action": "save", "name": "Plotter", "sensitivity": 0.4, "auto_align": true }),
    );
    assert_eq!(p["custom"][0]["name"], "Plotter");
    assert_eq!(p["built_in"].as_array().unwrap().len(), 3);
    call(&mut a, "edit_undo", json!({}));
    let mut custom = base.clone();
    custom["preset"] = json!("plotter");
    assert_eq!(call(&mut a, "compare_documents", custom)["changes"], 1);
    call(
        &mut a,
        "compare_preset",
        json!({ "action": "delete", "name": "Plotter" }),
    );
    let p = call(&mut a, "compare_preset", json!({ "action": "list" }));
    assert!(p["custom"].as_array().unwrap().is_empty());
    call(&mut a, "compare_preset", json!({ "action": "restore_defaults" }));
    let mut bad = base;
    bad["preset"] = json!("nope");
    assert!(fails(&mut a, "compare_documents", bad).contains("unknown preset"));
}

#[test]
fn overlay_three_points_auto_region_background_and_defaults() {
    let d = dir();
    let mut a = auto(&d);
    write(&d, "a.pdf", &[letter(line(100.0, 100.0, 500.0, 100.0, 4.0))]);
    write(&d, "b.pdf", &[letter(line(50.0, 50.0, 250.0, 50.0, 2.0))]);
    let v = call(
        &mut a,
        "overlay_pages",
        json!({
            "layers": [
                { "path": "a.pdf" },
                { "path": "b.pdf", "align": "points", "from": [[50, 50], [250, 50], [50, 300]], "to": [[100, 100], [500, 100], [100, 600]],
                  "background": "#FFFFCC", "blend": "darken", "region": [0, 0, 400, 400] }
            ],
            "out": "o1.pdf",
            "defaults": { "rotation": 0, "scale": 1 }
        }),
    );
    assert_eq!(v["pages"], 1);
    let img = markupcraft_engine::Session::open(d.join("o1.pdf"))
        .unwrap()
        .renderable(false)
        .unwrap()
        .render_rgba(0, 1.0)
        .unwrap();
    let [r, g, b] = img.pixel(300.0, 100.0);
    assert!(r < 120 && g < 120 && b < 120, "aligned lines coincide {:?}", [r, g, b]);
    let [r, g, b] = img.pixel(300.0, 400.0);
    assert!(r > 230 && g > 230 && b < 230, "background colour {:?}", [r, g, b]);
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "a.pdf" }, { "path": "b.pdf", "align": "auto" }], "out": "o2.pdf", "defaults": { "blend": "multiply", "dx": 0 } }),
    );
    let e = fails(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "a.pdf" }, { "path": "b.pdf", "blend": "glow" }], "out": "o3.pdf" }),
    );
    assert!(e.contains("blend"), "{e}");
    assert!(
        fails(
            &mut a,
            "overlay_pages",
            json!({ "layers": [{ "path": "a.pdf" }, { "path": "b.pdf" }], "out": "o3.pdf", "defaults": { "spin": 1 } })
        )
        .contains("spin")
    );
}

#[test]
fn batch_compare_and_overlay_with_saved_batch_file_and_reports() {
    let d = dir();
    let mut a = auto(&d);
    for sub in ["cur", "rev", "out"] {
        std::fs::create_dir_all(d.join(sub)).unwrap();
    }
    // Current: A-101 (2 pages) and M-201; revised: renamed with "rev2", one change each.
    write(
        &d.join("cur"),
        "A-101 plan.pdf",
        &[letter(sheet(0.0, 0.0, false)), letter(sheet(0.0, 0.0, false))],
    );
    write(&d.join("cur"), "M-201 mech.pdf", &[letter(sheet(0.0, 0.0, false))]);
    write(
        &d.join("rev"),
        "A-101 plan rev2.pdf",
        &[letter(sheet(0.0, 0.0, true)), letter(sheet(0.0, 0.0, false))],
    );
    write(&d.join("rev"), "M-201 mech rev2.pdf", &[letter(sheet(0.0, 0.0, true))]);
    let m = call(
        &mut a,
        "batch_match",
        json!({ "current": ["cur"], "revised": ["rev"], "filter": "@?#", "save_job": "job.pcbatch" }),
    );
    assert_eq!(m["pairs"].as_array().unwrap().len(), 3, "{m}");
    assert_eq!(m["pairs"][0]["current"]["key"], "A-101#0");
    // Without the filter nothing matches (the names differ).
    let none = call(&mut a, "batch_match", json!({ "current": ["cur"], "revised": ["rev"] }));
    assert!(none["pairs"].as_array().unwrap().is_empty());
    assert_eq!(none["unmatched_current"].as_array().unwrap().len(), 3);

    let v = call(
        &mut a,
        "batch_compare",
        json!({ "job": "job.pcbatch", "out_dir": "out", "mode": "graphics", "report": "report.csv" }),
    );
    assert_eq!(v["outputs"].as_array().unwrap().len(), 2, "{v}");
    let diffs: Vec<u64> = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["differences"].as_u64().unwrap())
        .collect();
    assert_eq!(diffs, [1, 0, 1], "{v}");
    let csv = std::fs::read_to_string(d.join("report.csv")).unwrap();
    assert!(
        csv.contains("Differences") && csv.contains("A-101 plan rev2_compared.pdf"),
        "{csv}"
    );
    let compared = markupcraft_engine::Session::open(d.join("out").join("A-101 plan rev2_compared.pdf")).unwrap();
    assert_eq!(compared.doc().markups.len(), 1);

    // Manual re-pairing: page 1 of A-101 against M-201's revision.
    let v = call(
        &mut a,
        "batch_compare",
        json!({
            "pairs": [{ "current": "cur/A-101 plan.pdf", "current_page": 2, "revised": "rev/M-201 mech rev2.pdf", "revised_page": 1 }],
            "out_dir": "out", "suffix": "_manual", "mode": "graphics", "report": "report.pdf"
        }),
    );
    assert_eq!(v["results"][0]["differences"], 1);
    let rep = markupcraft_engine::Session::open(d.join("report.pdf")).unwrap();
    assert_eq!(rep.links().len(), 1, "a link to the result");

    let v = call(
        &mut a,
        "batch_overlay",
        json!({ "job": "job.pcbatch", "out_dir": "out", "report": "overlay.csv" }),
    );
    assert_eq!(v["outputs"].as_array().unwrap().len(), 3, "{v}");
    assert!(d.join("out").join("A-101 plan rev2_overlay-1.pdf").exists());
    assert!(
        fails(
            &mut a,
            "batch_compare",
            json!({ "job": "missing.pcbatch", "out_dir": "out" })
        )
        .contains("missing")
    );
}

#[test]
fn batch_unflatten_repair_rotate_and_recolour() {
    let d = dir();
    let mut a = auto(&d);
    for n in ["one.pdf", "two.pdf"] {
        write(&d, n, &[letter(String::new())]);
        call(&mut a, "doc_open", json!({ "path": n }));
        call(
            &mut a,
            "markup_add",
            json!({ "page": 1, "kind": "Rectangle", "points": [[100, 100], [200, 200]] }),
        );
        call(&mut a, "markup_flatten", json!({ "all": true, "recoverable": true }));
        call(&mut a, "doc_save", json!({}));
        call(&mut a, "doc_close", json!({}));
    }
    let v = call(
        &mut a,
        "batch_apply",
        json!({
            "files": ["one.pdf", "two.pdf"],
            "out_dir": "out",
            "operations": [
                { "tool": "markup_unflatten" },
                { "tool": "doc_repair" },
                { "tool": "doc_color_process", "args": { "mode": "grayscale" } },
                { "tool": "page_rotate", "args": { "degrees": 90 } }
            ]
        }),
    );
    assert_eq!(v["done"], 2, "{v}");
    let s = markupcraft_engine::Session::open(d.join("out").join("two.pdf")).unwrap();
    assert_eq!(s.doc().markups.len(), 1, "unflattened");
    assert_eq!(s.doc().pages[0].rotate, 90);
}
