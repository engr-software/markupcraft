//! An estimator's whole takeoff day on a real Revu-marked drawing set, end to end through the
//! automation tools and the headless interface. The set is never committed: the test reads it
//! from `MARKUPCRAFT_REF_PDF` and skips when that is not set. Everything it writes goes to a
//! temporary folder, and nothing about the set (name, path, text) is printed.
//!
//! The flow: open the set, walk its pages, set and check scales, take off areas (with a
//! cutout), lengths, polylengths and counts on several sheets, add custom columns with a
//! formula total, group and total the Markups List, export the summary as CSV and XLSX,
//! compare two copies, flatten a copy, save (incremental and full), reopen, and check that
//! every quantity survived and that the markups Revu saved are exactly as they were.

use std::collections::BTreeMap;
use std::path::PathBuf;

use markupcraft_acceptance::*;

const REF_ENV: &str = "MARKUPCRAFT_REF_PDF";

fn reference() -> Option<PathBuf> {
    std::env::var_os(REF_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

/// What must not change about a markup: everything the Markups List shows.
fn fingerprint(m: &Value) -> String {
    let mut m = m.clone();
    if let Some(o) = m.as_object_mut() {
        o.remove("unsaved");
    }
    m.to_string()
}

fn list(a: &mut Automation) -> Vec<Value> {
    call(a, "markup_list", json!({}))["markups"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn by_id(ms: &[Value]) -> BTreeMap<String, Value> {
    ms.iter()
        .map(|m| (m["id"].as_str().unwrap_or("").to_string(), m.clone()))
        .collect()
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// 1/8" = 1'-0": one point on the sheet is 1/9 ft.
fn eighth() -> Value {
    json!({ "kind": "architectural", "paper_inches": 0.125, "real_feet": 1 })
}

/// One sheet's takeoff: (area id, length id, polylength id, count id).
struct Sheet {
    page: u64,
    ids: Vec<String>,
}

/// Areas, a cutout, a length, a polylength and a count on `page` (1-based), placed in the
/// middle of the sheet. Quantities at 1/8" = 1'-0": area 200 sf less a 25 sf cutout, length
/// 10 ft, polylength 20 ft, count 4.
fn take_off(a: &mut Automation, page: u64, media: [f64; 4]) -> Sheet {
    let (x, y) = (
        media[0] + (media[2] - media[0]) * 0.40,
        media[1] + (media[3] - media[1]) * 0.40,
    );
    let area = call(
        a,
        "markup_add",
        json!({ "page": page, "kind": "Area", "subject": "T3 Flooring",
                "points": [[x, y], [x + 180.0, y], [x + 180.0, y + 90.0], [x, y + 90.0]] }),
    );
    let area_id = area["id"]
        .as_str()
        .or(area["markup"]["id"].as_str())
        .unwrap()
        .to_string();
    call(
        a,
        "cutout_add",
        json!({ "id": area_id, "points": [[x + 20.0, y + 20.0], [x + 65.0, y + 20.0], [x + 65.0, y + 65.0], [x + 20.0, y + 65.0]] }),
    );
    let mut ids = vec![area_id];
    for (kind, subject, pts) in [
        ("Length", "T3 Walls", json!([[x, y - 30.0], [x + 90.0, y - 30.0]])),
        (
            "Polylength",
            "T3 Base",
            json!([[x, y + 120.0], [x + 90.0, y + 120.0], [x + 90.0, y + 210.0]]),
        ),
        (
            "Count",
            "T3 Doors",
            json!([[x + 200.0, y], [x + 220.0, y], [x + 240.0, y], [x + 260.0, y]]),
        ),
    ] {
        let v = call(
            a,
            "markup_add",
            json!({ "page": page, "kind": kind, "subject": subject, "points": pts }),
        );
        ids.push(v["id"].as_str().or(v["markup"]["id"].as_str()).unwrap().to_string());
    }
    Sheet { page, ids }
}

fn quantity_of(ms: &BTreeMap<String, Value>, id: &str) -> f64 {
    ms.get(id)
        .and_then(|m| m["quantity"].as_f64())
        .unwrap_or_else(|| panic!("no quantity for {id}"))
}

/// Our sheets' quantities, as the takeoff above should give them.
fn check_quantities(ms: &BTreeMap<String, Value>, sheets: &[Sheet], when: &str) {
    for s in sheets {
        let want = [175.0, 10.0, 20.0, 4.0];
        for (id, w) in s.ids.iter().zip(want) {
            let q = quantity_of(ms, id);
            assert!(near(q, w, 1e-6), "{when}: page {} markup {id}: {q} != {w}", s.page);
        }
    }
}

#[test]
fn estimator_workflow_on_the_reference_set() {
    let Some(src) = reference() else {
        eprintln!("real_world: SKIPPED, {REF_ENV} is not set");
        return;
    };
    let dir = temp_dir("real-world");
    std::fs::copy(&src, dir.join("set.pdf")).unwrap();
    std::fs::copy(&src, dir.join("copy.pdf")).unwrap();
    let mut a = automation(&dir);

    // ---- open and walk the set
    let v = call(&mut a, "doc_open", json!({ "path": "set.pdf" }));
    let doc = v["doc"].as_u64().unwrap();
    let info = call(&mut a, "doc_info", json!({}));
    let pages = info["page_list"].as_array().unwrap().clone();
    assert_eq!(pages.len(), 121, "the reference set has 121 sheets");
    let original = list(&mut a);
    let original_ids = by_id(&original);
    assert!(original.len() >= 597, "Revu's markups load: {}", original.len());
    let measured_before = original.iter().filter(|m| m["quantity"].as_f64().is_some()).count();
    for p in [1, 60, 121] {
        let t = call(&mut a, "page_text", json!({ "page": p }));
        assert!(t.is_object(), "page {p} text reads");
    }

    // ---- scales: three empty sheets get 1/8" = 1'-0"; one Revu-scaled sheet keeps its own
    let empty: Vec<(u64, [f64; 4])> = pages
        .iter()
        .filter(|p| p["markups"].as_u64() == Some(0))
        .map(|p| {
            let mb: Vec<f64> = p["media_box"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            (p["page"].as_u64().unwrap(), [mb[0], mb[1], mb[2], mb[3]])
        })
        .collect();
    assert!(empty.len() >= 3, "sheets without markups to take off on");
    let picks: Vec<(u64, [f64; 4])> = vec![empty[0], empty[empty.len() / 2], empty[empty.len() - 1]];
    let pick_pages: Vec<u64> = picks.iter().map(|p| p.0).collect();
    call(&mut a, "scale_set", json!({ "pages": pick_pages, "scale": eighth() }));
    let info = call(&mut a, "doc_info", json!({}));
    for p in &pick_pages {
        let vp = &info["page_list"][(*p - 1) as usize]["viewports"];
        assert!(
            vp.as_array().is_some_and(|v| !v.is_empty()),
            "page {p} has the scale: {vp}"
        );
    }
    let revu_page = original
        .iter()
        .find(|m| m["kind"] == "Length" && m["scale"].is_string())
        .map(|m| (m["page"].as_u64().unwrap(), m["unit"].clone()));

    // ---- takeoff on several sheets
    let sheets: Vec<Sheet> = picks.iter().map(|(p, mb)| take_off(&mut a, *p, *mb)).collect();
    let now = by_id(&list(&mut a));
    check_quantities(&now, &sheets, "after takeoff");
    // A length on a sheet Revu scaled takes that sheet's scale and unit.
    let mut extra = Vec::new();
    if let Some((page, unit)) = &revu_page {
        let mb: Vec<f64> = pages[(*page - 1) as usize]["media_box"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let v = call(
            &mut a,
            "markup_add",
            json!({ "page": page, "kind": "Length", "subject": "T3 Walls",
                    "points": [[mb[0] + 50.0, mb[1] + 50.0], [mb[0] + 140.0, mb[1] + 50.0]] }),
        );
        let id = v["id"].as_str().or(v["markup"]["id"].as_str()).unwrap().to_string();
        let m = by_id(&list(&mut a)).remove(&id).unwrap();
        assert!(m["quantity"].as_f64().unwrap() > 0.0, "{m}");
        assert_eq!(&m["unit"], unit, "the Revu sheet's unit");
        extra.push(id);
    }

    // ---- custom columns, a formula total, the Markups List grouped and totalled
    call(
        &mut a,
        "columns_set",
        json!({ "columns": [
            { "id": "cost", "name": "Unit Cost", "type": "Currency", "decimals": 2, "symbol": "$" },
            { "id": "total", "name": "Total", "type": "Formula", "formula": "Measurement * [Unit Cost]", "total": true, "decimals": 2 }
        ] }),
    );
    for s in &sheets {
        for (id, cost) in s.ids.iter().zip(["4.5", "12", "3", "250"]) {
            call(
                &mut a,
                "list_cell_set",
                json!({ "id": id, "column": "c:cost", "value": cost }),
            );
        }
    }
    let n = sheets.len() as f64;
    let want_total = n * (175.0 * 4.5 + 10.0 * 12.0 + 20.0 * 3.0 + 4.0 * 250.0);
    call(
        &mut a,
        "summary_export",
        json!({ "out": "summary.csv", "group_by": ["subject"], "pages": pick_pages,
                "columns": ["subject", "page", "measurement", "c:cost", "c:total"] }),
    );
    let csv = std::fs::read_to_string(dir.join("summary.csv")).unwrap();
    for s in ["T3 Flooring", "T3 Walls", "T3 Base", "T3 Doors"] {
        assert!(csv.contains(s), "summary groups {s}");
    }
    let total_line = csv
        .lines()
        .rev()
        .find(|l| l.starts_with("Total"))
        .unwrap_or_else(|| panic!("a grand total line: {csv}"));
    // The last field, which is quoted when it holds a thousands separator.
    let last = match total_line.strip_suffix('"') {
        Some(t) => t.rsplit('"').next().unwrap_or(""),
        None => total_line.rsplit(',').next().unwrap_or(""),
    };
    let digits: String = last.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    assert!(
        near(digits.parse::<f64>().unwrap_or(-1.0), want_total, 0.01),
        "formula grand total {want_total} in {total_line:?}"
    );
    call(
        &mut a,
        "summary_export",
        json!({ "out": "summary.xlsx", "group_by": ["subject"], "measurements_only": true }),
    );
    let xlsx = std::fs::read(dir.join("summary.xlsx")).unwrap();
    assert!(xlsx.starts_with(b"PK"), "an xlsx is a zip");

    // ---- save, reopen, every quantity and value survives; Revu's markups are untouched
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_save", json!({ "path": "full.pdf", "full": true }));
    for file in ["set.pdf", "full.pdf"] {
        let mut b = automation(&dir);
        call(&mut b, "doc_open", json!({ "path": file }));
        let after = by_id(&list(&mut b));
        check_quantities(&after, &sheets, file);
        for id in &extra {
            assert!(after.contains_key(id), "{file}: the length on the Revu sheet");
        }
        for s in &sheets {
            for id in &s.ids {
                assert!(
                    after[id]["columns"].to_string().contains("cost"),
                    "{file}: {id} keeps its unit cost: {}",
                    after[id]
                );
            }
        }
        let mut changed = Vec::new();
        for (id, m) in &original_ids {
            match after.get(id) {
                Some(n) if fingerprint(n) == fingerprint(m) => {}
                Some(n) => changed.push(format!("{id}: {} -> {}", fingerprint(m), fingerprint(n))),
                None => changed.push(format!("{id}: missing")),
            }
        }
        assert!(
            changed.is_empty(),
            "{file}: {} of Revu's markups changed, first: {:?}",
            changed.len(),
            changed.first()
        );
        let measured = original_ids
            .keys()
            .filter(|id| after[*id]["quantity"].as_f64().is_some())
            .count();
        assert_eq!(
            measured, measured_before,
            "{file}: every Revu measurement still measures"
        );
        assert_eq!(
            after.len(),
            original.len() + 4 * sheets.len() + extra.len(),
            "{file}: markup count"
        );
        call(&mut b, "doc_close", json!({ "discard_changes": true }));
    }

    // ---- compare two copies: the same sheets show no changes
    let mut c = automation(&dir);
    call(&mut c, "doc_open", json!({ "path": "copy.pdf" }));
    let p = pick_pages[0];
    let r = call(
        &mut c,
        "compare_documents",
        json!({ "old": "set.pdf", "pairs": [[p, p], [1, 1]] }),
    );
    assert_eq!(r["changes"].as_u64(), Some(0), "identical sheets: {r}");

    // ---- flatten a copy: the page keeps its look, its markups are gone, others stay
    let flat_page = original[0]["page"].as_u64().unwrap();
    let on_page = original
        .iter()
        .filter(|m| m["page"].as_u64() == Some(flat_page))
        .count();
    call(&mut c, "markup_flatten", json!({ "pages": [flat_page] }));
    call(&mut c, "doc_save", json!({ "path": "flat.pdf" }));
    call(&mut c, "doc_close", json!({ "discard_changes": true }));
    let mut f = automation(&dir);
    call(&mut f, "doc_open", json!({ "path": "flat.pdf" }));
    let flat = list(&mut f);
    assert_eq!(flat.len(), original.len() - on_page, "flattened page {flat_page}");
    assert!(
        !flat.iter().any(|m| m["page"].as_u64() == Some(flat_page)),
        "no markups left on the flattened page"
    );
    let flat_ids = by_id(&flat);
    for (id, m) in &original_ids {
        if m["page"].as_u64() != Some(flat_page) {
            assert_eq!(fingerprint(&flat_ids[id]), fingerprint(m), "flatten left {id} alone");
        }
    }

    // ---- the interface: open the saved set, walk pages, the Markups List groups and totals
    let bytes = std::fs::read(dir.join("set.pdf")).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(move |_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            app.open_bytes("set.pdf", None, bytes).unwrap();
            app
        });
    h.run_steps(4);
    assert_eq!(markups(&h).len(), original.len() + 4 * sheets.len() + extra.len());
    let current = |h: &Harness<'_, MarkupCraftApp>| h.state().state.doc().unwrap().view.current;
    run(&mut h, "view.last_page");
    assert_eq!(current(&h), 120);
    run(&mut h, "view.first_page");
    assert_eq!(current(&h), 0);
    run(&mut h, "view.next_page");
    assert_eq!(current(&h), 1);
    let sheet = pick_pages[1];
    h.state_mut().set_option("page", &sheet.to_string());
    h.state_mut().set_option("panel", "markups");
    h.state_mut().set_option("group-by", "subject");
    h.state_mut().state.list.view.scope = markupcraft_model::Scope::CurrentPage(0);
    h.run_steps(6);
    assert_eq!(current(&h) as u64, sheet - 1);
    for g in [
        "T3 Flooring (1)",
        "T3 Walls (1)",
        "T3 Base (1)",
        "T3 Doors (1)",
        "Total (4)",
    ] {
        assert!(shows(&h, g), "the Markups List groups this sheet by subject: {g}");
    }
    assert!(shows(&h, "175 sf"), "the area's subtotal");
    // Draw one more area in the interface on that sheet; it measures with the sheet's scale.
    let before = markups(&h).len();
    let media = picks[1].1;
    let (x, y) = (
        media[0] + (media[2] - media[0]) * 0.6,
        media[1] + (media[3] - media[1]) * 0.6,
    );
    h.state_mut().set_option("tool", "area");
    h.run_steps(2);
    for (px, py) in [(x, y), (x + 90.0, y), (x + 90.0, y + 90.0), (x, y + 90.0)] {
        click(&mut h, px, py);
    }
    key(&mut h, egui::Modifiers::NONE, egui::Key::Enter);
    let all = markups(&h);
    assert_eq!(all.len(), before + 1, "the area tool added one markup");
    let new = all.last().unwrap();
    assert_eq!(new.kind, Kind::Area);
    let q = new.quantity().unwrap_or(0.0);
    assert!(near(q, 100.0, 2.0), "a 90 x 90 pt area at 1/8\" is about 100 sf: {q}");
    let _ = doc;
}
