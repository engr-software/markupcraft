//! Review slice: Compare Documents, Overlay Pages, Search and Visual Search, and the Spaces,
//! Links and Signatures panels (rows ui-001 .. ui-072), written from
//! docs/revu_features/04_compare_ui_collab.md.
use markupcraft_acceptance::*;
use std::path::{Path, PathBuf};
use synthetic::{SyntheticPage, line, rect, text};

// ---------------------------------------------------------------------------------------------
// Drawings
// ---------------------------------------------------------------------------------------------

/// A small floor plan, 600 x 400 pt. `rev` 0 is the original; rev 1 moves the interior wall from
/// x = 300 to x = 330, adds a door (a filled block at 400..440 x 150..158) and changes the room
/// tag "ROOM 101" to "ROOM 102". The outer walls and the "STAIR" tag never change.
fn plan(rev: u32) -> String {
    let mut c = String::new();
    c += &line(50.0, 50.0, 550.0, 50.0, 2.0);
    c += &line(50.0, 350.0, 550.0, 350.0, 2.0);
    c += &line(50.0, 50.0, 50.0, 350.0, 2.0);
    c += &line(550.0, 50.0, 550.0, 350.0, 2.0);
    c += &text(90.0, 90.0, 12.0, "STAIR");
    if rev == 0 {
        c += &line(300.0, 50.0, 300.0, 350.0, 2.0);
        c += &text(100.0, 300.0, 12.0, "ROOM 101");
    } else {
        c += &line(330.0, 50.0, 330.0, 350.0, 2.0);
        c += &rect(400.0, 150.0, 40.0, 8.0);
        c += &text(100.0, 300.0, 12.0, "ROOM 102");
    }
    c
}

fn write_pdf(dir: &Path, name: &str, pages: &[String]) -> PathBuf {
    let p = dir.join(name);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let pages: Vec<SyntheticPage> = pages
        .iter()
        .map(|c| SyntheticPage::new(600.0, 400.0, c.clone()))
        .collect();
    std::fs::write(&p, synthetic::pdf(&pages)).unwrap();
    p
}

fn name(p: &Path) -> String {
    p.file_name().unwrap().to_str().unwrap().to_string()
}

fn open(a: &mut Automation, p: &Path) -> u64 {
    call(a, "doc_open", json!({ "path": name(p) }))["doc"].as_u64().unwrap()
}

fn rects(v: &Value) -> Vec<[f64; 4]> {
    v["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let a: Vec<f64> = r["rect"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect();
            [a[0], a[1], a[2], a[3]]
        })
        .collect()
}

fn covers(r: &[f64; 4], x: f64, y: f64) -> bool {
    r[0] - 1.0 <= x && x <= r[2] + 1.0 && r[1] - 1.0 <= y && y <= r[3] + 1.0
}

fn any_covers(rs: &[[f64; 4]], x: f64, y: f64) -> bool {
    rs.iter().any(|r| covers(r, x, y))
}

/// The colour of the rendered page at a user-space point.
fn pixel(pdf: &Path, page: usize, x: f64, y: f64) -> [u8; 3] {
    let bytes = std::sync::Arc::new(std::fs::read(pdf).unwrap());
    let r = markupcraft_engine::raster::Renderable::new(bytes, false).unwrap();
    r.render_rgba(page, 2.0).unwrap().pixel(x, y)
}

fn is_white(c: [u8; 3]) -> bool {
    c.iter().all(|&v| v > 235)
}

fn is_dark(c: [u8; 3]) -> bool {
    c.iter().all(|&v| v < 90)
}

/// Strongly coloured: one channel clearly above another.
fn is_tinted(c: [u8; 3]) -> bool {
    let mx = *c.iter().max().unwrap() as i32;
    let mn = *c.iter().min().unwrap() as i32;
    mx - mn > 80
}

/// Two revisions of the plan written to `dir` as old.pdf / new.pdf.
fn revisions(dir: &Path) -> (PathBuf, PathBuf) {
    (
        write_pdf(dir, "old.pdf", &[plan(0)]),
        write_pdf(dir, "new.pdf", &[plan(1)]),
    )
}

/// The changed spots and the unchanged ones in `plan`.
const CHANGED: [(f64, f64); 4] = [(300.0, 200.0), (330.0, 200.0), (420.0, 154.0), (150.0, 304.0)];
const UNCHANGED: [(f64, f64); 4] = [(105.0, 94.0), (50.0, 200.0), (550.0, 200.0), (450.0, 50.0)];

fn assert_clouds_exactly_on_changes(rs: &[[f64; 4]]) {
    for (x, y) in CHANGED {
        assert!(any_covers(rs, x, y), "no cloud on the change at ({x}, {y}): {rs:?}");
    }
    for (x, y) in UNCHANGED {
        assert!(
            !any_covers(rs, x, y),
            "cloud on unchanged linework at ({x}, {y}): {rs:?}"
        );
    }
    for r in rs {
        assert!(
            r[2] - r[0] < 250.0 && r[3] - r[1] < 330.0,
            "a cloud covers most of the sheet: {r:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 1. Compare Documents
// ---------------------------------------------------------------------------------------------

/// ui-001, ui-002, ui-006, ui-015: compare an original and a revision (picked as a file or as an
/// open document); clouds land on every changed region and nowhere else, orange by default, as
/// ordinary markups in the Markups List; the files on disk stay untouched.
#[test]
fn compare_clouds_every_change_and_nothing_else() {
    let dir = temp_dir("cmp-basic");
    let (old, new) = revisions(&dir);
    let old_bytes = std::fs::read(&old).unwrap();
    let new_bytes = std::fs::read(&new).unwrap();
    let mut a = automation(&dir);

    // Document A from a file browser, Document B the open document.
    open(&mut a, &new);
    let r = call(&mut a, "compare_documents", json!({ "old": "old.pdf" }));
    let rs = rects(&r);
    assert_clouds_exactly_on_changes(&rs);
    assert_eq!(r["changes"].as_u64().unwrap() as usize, rs.len());

    // Results are ordinary markups: listable and filterable by subject, orange clouds.
    let list = call(&mut a, "markup_list", json!({ "subject": "Compare" }));
    assert_eq!(list["count"].as_u64().unwrap() as usize, rs.len(), "{list}");
    for m in list["markups"].as_array().unwrap() {
        let c = m["color"].as_str().unwrap().trim_start_matches('#').to_string();
        let rgb: Vec<u8> = (0..3)
            .map(|i| u8::from_str_radix(&c[2 * i..2 * i + 2], 16).unwrap())
            .collect();
        assert!(
            rgb[0] > 200 && rgb[1] > 60 && rgb[1] < 200 && rgb[2] < 80,
            "not orange: {m}"
        );
    }
    // Every markup on the page is a compare result: nothing else was added.
    assert_eq!(call(&mut a, "markup_list", json!({}))["count"], list["count"]);

    // The originals are untouched on disk.
    assert_eq!(std::fs::read(&old).unwrap(), old_bytes);
    assert_eq!(std::fs::read(&new).unwrap(), new_bytes);

    // Document A picked from the open tabs instead.
    let mut b = automation(&dir);
    let a_id = open(&mut b, &old);
    open(&mut b, &new);
    let r2 = call(&mut b, "compare_documents", json!({ "old_doc": a_id }));
    assert_clouds_exactly_on_changes(&rects(&r2));
    // The older document got no clouds.
    assert_eq!(call(&mut b, "markup_list", json!({ "doc": a_id }))["count"], 0);

    // Identical documents: no clouds at all.
    let mut c = automation(&dir);
    open(&mut c, &old);
    let same = call(&mut c, "compare_documents", json!({ "old": "old.pdf" }));
    assert_eq!(same["changes"], 0, "{same}");

    // ui-006: the clouds go on a separate result copy, <name>_Diff.pdf beside the newer
    // revision, which opens; the open document and both files stay untouched.
    let mut e = automation(&dir);
    let new_id = open(&mut e, &new);
    let r = call(
        &mut e,
        "compare_documents",
        json!({ "old": "old.pdf", "result_file": true }),
    );
    let out = dir.join("new_Diff.pdf");
    assert!(out.exists(), "{r}");
    assert!(r["result"].as_str().unwrap().ends_with("new_Diff.pdf"), "{r}");
    assert_clouds_exactly_on_changes(&rects(&r));
    let result_doc = r["result_doc"].as_u64().unwrap();
    assert_ne!(result_doc, new_id);
    assert_eq!(
        call(
            &mut e,
            "markup_list",
            json!({ "doc": result_doc, "subject": "Compare" })
        )["count"]
            .as_u64()
            .unwrap() as usize,
        rects(&r).len(),
        "the result copy holds the clouds"
    );
    assert_eq!(
        call(&mut e, "markup_list", json!({ "doc": new_id }))["count"],
        0,
        "the newer document has none"
    );
    assert_eq!(std::fs::read(&old).unwrap(), old_bytes);
    assert_eq!(std::fs::read(&new).unwrap(), new_bytes);
    // The Compare dialog offers the same: the result opens as its own tab.
    std::fs::remove_file(&out).unwrap();
    let mut h = app_with(new_bytes.clone(), Some(new.clone()));
    {
        let st = &mut h.state_mut().state;
        let uid = st.doc().unwrap().uid;
        let c = &mut st.features.compare;
        c.old_file = Some(old.clone());
        c.new_doc = Some(uid);
        c.result_file = true;
        markupcraft_ui_egui::features::compare::run_compare(st);
    }
    h.run_steps(4);
    assert!(out.exists(), "the dialog wrote the _Diff file");
    assert_eq!(h.state().state.docs.len(), 2, "and opened it");
    assert!(
        h.state().state.docs[0].session.doc().markups.is_empty(),
        "the newer document is untouched"
    );
    assert_eq!(std::fs::read(&new).unwrap(), new_bytes);
}

/// ui-003: compare only the pages asked for.
#[test]
fn compare_page_range_per_side() {
    let dir = temp_dir("cmp-pages");
    write_pdf(&dir, "old.pdf", &[plan(0), plan(0), plan(0)]);
    let new = write_pdf(&dir, "new.pdf", &[plan(0), plan(1), plan(1)]);
    let mut a = automation(&dir);
    open(&mut a, &new);
    let r = call(
        &mut a,
        "compare_documents",
        json!({ "old": "old.pdf", "pairs": [[2, 2]] }),
    );
    let pages: Vec<u64> = r["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["page"].as_u64().unwrap())
        .collect();
    assert!(!pages.is_empty() && pages.iter().all(|&p| p == 2), "{pages:?}");
    // Old page 1 against new page 3: every change of page 3 shows.
    let r = call(
        &mut a,
        "compare_documents",
        json!({ "old": "old.pdf", "pairs": [[1, 3]] }),
    );
    assert!(
        r["regions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["page"] == 3 && g["old_page"] == 1)
    );
    assert_clouds_exactly_on_changes(&rects(&r));
    // Default: every page in order; page 1 has no changes.
    let mut b = automation(&dir);
    open(&mut b, &new);
    let r = call(&mut b, "compare_documents", json!({ "old": "old.pdf" }));
    let pages: Vec<u64> = r["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["page"].as_u64().unwrap())
        .collect();
    assert!(
        pages.contains(&2) && pages.contains(&3) && !pages.contains(&1),
        "{pages:?}"
    );
}

/// ui-004, ui-014: a revision whose whole sheet moved 30 pt right (plus one added door). Page
/// align clouds the whole drawing; Auto align, a known offset and matching points register the
/// sheets so only the door is clouded.
#[test]
fn compare_alignment_methods_register_a_shifted_sheet() {
    let dir = temp_dir("cmp-align");
    let shifted = |door: bool| {
        let mut c = format!("1 0 0 1 30 0 cm\n{}", plan(0));
        if door {
            c += &rect(400.0, 150.0, 40.0, 8.0);
        }
        format!("q\n{c}Q\n")
    };
    write_pdf(&dir, "old.pdf", &[plan(0)]);
    let new = write_pdf(&dir, "new.pdf", &[shifted(true)]);
    // The door ends up at 430..470 on the new sheet.
    let door = (450.0, 154.0);
    let only_door = |r: &Value| {
        let rs = rects(r);
        assert!(any_covers(&rs, door.0, door.1), "door not clouded: {rs:?}");
        assert!(!any_covers(&rs, 80.0, 200.0), "shifted wall clouded: {rs:?}");
        assert!(!any_covers(&rs, 330.0, 200.0), "shifted interior wall clouded: {rs:?}");
    };

    let mut a = automation(&dir);
    open(&mut a, &new);
    let page = call(
        &mut a,
        "compare_documents",
        json!({ "old": "old.pdf", "align": "page" }),
    );
    let rs = rects(&page);
    assert!(
        any_covers(&rs, 80.0, 200.0) || any_covers(&rs, 50.0, 200.0),
        "page align should see the shift: {rs:?}"
    );

    for args in [
        json!({ "old": "old.pdf", "align": "auto" }),
        json!({ "old": "old.pdf", "align": "offset", "offset": [30, 0] }),
        json!({ "old": "old.pdf", "align": "points", "old_points": [[50, 50], [550, 350]], "new_points": [[80, 50], [580, 350]] }),
    ] {
        let mut a = automation(&dir);
        open(&mut a, &new);
        only_door(&call(&mut a, "compare_documents", args));
    }
}

/// ui-005: only the dragged window is compared.
#[test]
fn compare_selected_window_only() {
    let dir = temp_dir("cmp-window");
    let (_, new) = revisions(&dir);
    let mut a = automation(&dir);
    open(&mut a, &new);
    let r = call(
        &mut a,
        "compare_documents",
        json!({ "old": "old.pdf", "window": [380, 120, 480, 200] }),
    );
    let rs = rects(&r);
    assert!(any_covers(&rs, 420.0, 154.0), "{rs:?}");
    for (x, y) in [(300.0, 200.0), (330.0, 200.0), (150.0, 304.0)] {
        assert!(!any_covers(&rs, x, y), "change outside the window clouded: {rs:?}");
    }
    for r in &rs {
        assert!(
            r[0] >= 370.0 && r[2] <= 490.0 && r[1] >= 110.0 && r[3] <= 210.0,
            "cloud leaves the window: {r:?}"
        );
    }
}

/// ui-007: subject, line colour, fill colour, opacity, width, cloud on/off and lock.
#[test]
fn compare_difference_markup_appearance() {
    let dir = temp_dir("cmp-look");
    let (_, new) = revisions(&dir);
    let mut a = automation(&dir);
    open(&mut a, &new);
    call(
        &mut a,
        "compare_documents",
        json!({ "old": "old.pdf", "subject": "Rev B", "color": "#0000FF", "fill": "#FFFF00",
                "fill_opacity": 0.25, "opacity": 0.8, "width": 3.0, "cloud": 0, "lock": true }),
    );
    let list = call(&mut a, "markup_list", json!({ "subject": "Rev B" }));
    assert!(list["count"].as_u64().unwrap() >= 3, "{list}");
    for m in list["markups"].as_array().unwrap() {
        assert_eq!(m["color"].as_str().unwrap().to_ascii_uppercase(), "#0000FF", "{m}");
        assert_eq!(m["fill"].as_str().unwrap().to_ascii_uppercase(), "#FFFF00", "{m}");
        assert!((m["fill_opacity"].as_f64().unwrap() - 0.25).abs() < 0.01, "{m}");
        assert!((m["opacity"].as_f64().unwrap() - 0.8).abs() < 0.01, "{m}");
        assert!((m["width"].as_f64().unwrap() - 3.0).abs() < 0.01, "{m}");
        assert_eq!(m["locked"], true, "{m}");
        // cloud 0: a plain rectangular outline, not a cloud.
        assert_ne!(m["kind"], "Cloud", "{m}");
        assert_eq!(m["points"].as_array().unwrap().len(), 4, "{m}");
    }
    // Default: a cloud, not a rectangle.
    let mut b = automation(&dir);
    open(&mut b, &new);
    call(&mut b, "compare_documents", json!({ "old": "old.pdf" }));
    let list = call(&mut b, "markup_list", json!({ "subject": "Compare" }));
    for m in list["markups"].as_array().unwrap() {
        assert_eq!(m["kind"], "Cloud", "default output should be clouds: {m}");
        assert_eq!(m["locked"], false, "{m}");
    }
}

/// ui-008: three built-in presets, saved custom presets, Restore Defaults.
#[test]
fn compare_presets_builtin_custom_and_restore() {
    let dir = temp_dir("cmp-presets");
    let (_, new) = revisions(&dir);
    let mut a = automation(&dir);
    let l = call(&mut a, "compare_preset", json!({ "action": "list" }));
    let built: Vec<&str> = l["built_in"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(built.len(), 3, "{l}");
    for p in ["same_printer", "different_printer", "scanned"] {
        assert!(built.contains(&p), "{l}");
    }
    let l = call(
        &mut a,
        "compare_preset",
        json!({ "action": "save", "name": "Mine", "sensitivity": 0.9, "dpi": 150 }),
    );
    assert_eq!(l["custom"][0]["name"], "Mine", "{l}");
    open(&mut a, &new);
    for p in ["same_printer", "different_printer", "scanned", "Mine"] {
        let r = call(&mut a, "compare_documents", json!({ "old": "old.pdf", "preset": p }));
        assert!(any_covers(&rects(&r), 420.0, 154.0), "preset {p} missed the door: {r}");
    }
    let l = call(&mut a, "compare_preset", json!({ "action": "restore_defaults" }));
    assert_eq!(l["custom"].as_array().unwrap().len(), 0);
    assert!(
        fails(
            &mut a,
            "compare_documents",
            json!({ "old": "old.pdf", "preset": "Mine" })
        )
        .contains("Mine")
    );
}

/// ui-009, ui-010, ui-011: grid cell / pixel density, colour sensitivity and DPI change what
/// counts as a difference.
#[test]
fn compare_grid_density_sensitivity_and_dpi() {
    let dir = temp_dir("cmp-tuning");
    let base = plan(0);
    write_pdf(&dir, "old.pdf", std::slice::from_ref(&base));
    // A faint grey line (light change), and a tiny dot (few pixels) far from everything else.
    let faint = write_pdf(
        &dir,
        "faint.pdf",
        &[format!("{base}0.85 G 1 w 100 200 m 250 200 l S\n0 G\n")],
    );
    let dot = write_pdf(&dir, "dot.pdf", &[format!("{base}{}", rect(450.0, 300.0, 1.2, 1.2))]);

    let run = |p: &Path, extra: Value| -> Vec<[f64; 4]> {
        let mut a = automation(&dir);
        open(&mut a, p);
        let mut args = json!({ "old": "old.pdf", "mode": "graphics" });
        for (k, v) in extra.as_object().unwrap() {
            args[k] = v.clone();
        }
        rects(&call(&mut a, "compare_documents", args))
    };
    // Colour sensitivity: the faint line shows at high sensitivity, not at low.
    assert!(
        any_covers(&run(&faint, json!({ "sensitivity": 1.0 })), 175.0, 200.0),
        "faint line missed at sensitivity 1"
    );
    assert!(
        !any_covers(&run(&faint, json!({ "sensitivity": 0.0 })), 175.0, 200.0),
        "faint line found at sensitivity 0"
    );
    // Pixel density: the dot is a handful of pixels; demanding many changed pixels per cell hides it.
    assert!(
        any_covers(&run(&dot, json!({ "dpi": 150 })), 450.6, 300.6),
        "dot missed"
    );
    assert!(
        !any_covers(
            &run(&dot, json!({ "dpi": 150, "cell": 16, "density": 60 })),
            450.6,
            300.6
        ),
        "dot found though a cell needs 60 changed pixels"
    );
    // DPI: at a very low resolution the dot vanishes; raising it finds it.
    assert!(
        !any_covers(&run(&dot, json!({ "dpi": 18 })), 450.6, 300.6),
        "dot found at 18 dpi"
    );
    assert!(
        any_covers(&run(&dot, json!({ "dpi": 200 })), 450.6, 300.6),
        "dot missed at 200 dpi"
    );
}

/// ui-012: a band along the sheet edge is ignored.
#[test]
fn compare_ignore_margin() {
    let dir = temp_dir("cmp-margin");
    write_pdf(&dir, "old.pdf", &[plan(0)]);
    let new = write_pdf(
        &dir,
        "new.pdf",
        &[format!("{}{}", plan(1), text(400.0, 10.0, 8.0, "PLOTTED 2026-10-09"))],
    );
    let mut a = automation(&dir);
    open(&mut a, &new);
    let all = rects(&call(&mut a, "compare_documents", json!({ "old": "old.pdf" })));
    assert!(
        any_covers(&all, 430.0, 13.0),
        "edge stamp not seen without a margin: {all:?}"
    );
    let mut b = automation(&dir);
    open(&mut b, &new);
    let some = rects(&call(
        &mut b,
        "compare_documents",
        json!({ "old": "old.pdf", "margin": 30 }),
    ));
    assert!(
        !any_covers(&some, 430.0, 13.0),
        "edge stamp inside the margin clouded: {some:?}"
    );
    assert!(any_covers(&some, 420.0, 154.0), "{some:?}");
}

/// ui-013: existing markups and recoverable flattened markups are ignored unless asked for.
#[test]
fn compare_include_markups_and_flattened() {
    let dir = temp_dir("cmp-markups");
    let (_, _) = revisions(&dir);
    // A copy of the old sheet with a markup on it.
    let same = write_pdf(&dir, "same.pdf", &[plan(0)]);
    let mut a = automation(&dir);
    open(&mut a, &same);
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[420, 250], [500, 320]], "color": "#FF0000", "width": 3 }),
    );
    let r = rects(&call(
        &mut a,
        "compare_documents",
        json!({ "old": "old.pdf", "mode": "graphics" }),
    ));
    assert!(
        !any_covers(&r, 420.0, 285.0),
        "a markup counted as a change by default: {r:?}"
    );
    let ids = a_ids(&mut a, "Compare");
    if !ids.is_empty() {
        call(&mut a, "markup_delete", json!({ "ids": ids }));
    }
    let r = rects(&call(
        &mut a,
        "compare_documents",
        json!({ "old": "old.pdf", "mode": "graphics", "include_markups": true }),
    ));
    assert!(
        any_covers(&r, 420.0, 285.0) || any_covers(&r, 500.0, 285.0),
        "include_markups missed the markup: {r:?}"
    );

    // Flattened (recoverable) markup.
    let mut f = automation(&dir);
    open(&mut f, &same);
    call(
        &mut f,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[420, 250], [500, 320]], "color": "#FF0000", "width": 3 }),
    );
    let ids = a_ids(&mut f, "");
    call(&mut f, "markup_flatten", json!({ "ids": ids, "recoverable": true }));
    call(&mut f, "doc_save", json!({ "path": "flat.pdf" }));
    let mut g = automation(&dir);
    open(&mut g, &dir.join("flat.pdf"));
    let r = rects(&call(
        &mut g,
        "compare_documents",
        json!({ "old": "old.pdf", "mode": "graphics" }),
    ));
    assert!(
        !any_covers(&r, 420.0, 285.0) && !any_covers(&r, 500.0, 285.0),
        "flattened markup counted by default: {r:?}"
    );
    let ids = a_ids(&mut g, "Compare");
    if !ids.is_empty() {
        call(&mut g, "markup_delete", json!({ "ids": ids }));
    }
    let r = rects(&call(
        &mut g,
        "compare_documents",
        json!({ "old": "old.pdf", "mode": "graphics", "include_flattened": true }),
    ));
    assert!(
        any_covers(&r, 420.0, 285.0) || any_covers(&r, 500.0, 285.0),
        "include_flattened missed it: {r:?}"
    );
}

fn a_ids(a: &mut Automation, subject: &str) -> Vec<String> {
    let args = if subject.is_empty() {
        json!({})
    } else {
        json!({ "subject": subject })
    };
    call(a, "markup_list", args)["markups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Batch compare (ui-017 .. ui-023)
// ---------------------------------------------------------------------------------------------

/// Two sheets per set: A-101 changes, A-102 does not. Current names "A-101.pdf"; revised names
/// carry a revision suffix ("A-101 rev 2.pdf") and live in a subfolder.
fn sheet_set(dir: &Path) {
    let sheet = |num: &str, rev: u32| format!("{}{}", plan(rev), text(480.0, 20.0, 10.0, num));
    write_pdf(&dir.join("current"), "A-101.pdf", &[sheet("A-101", 0)]);
    write_pdf(&dir.join("current"), "A-102.pdf", &[sheet("A-102", 0)]);
    write_pdf(
        &dir.join("revised").join("issue"),
        "A-101 rev 2.pdf",
        &[sheet("A-101", 1)],
    );
    write_pdf(
        &dir.join("revised").join("issue"),
        "A-102 rev 2.pdf",
        &[sheet("A-102", 0)],
    );
}

fn pair_keys(v: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = v["pairs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let f = |k: &str| {
                Path::new(p[k]["file"].as_str().unwrap())
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            };
            (f("current"), f("revised"))
        })
        .collect();
    out.sort();
    out
}

/// ui-017, ui-018, ui-020, ui-023: batch compare folders (with subfolders), auto-matched with a
/// wildcard filter, one clouded result per pair and a CSV / PDF summary report.
#[test]
fn batch_compare_folders_with_filter_and_report() {
    let dir = temp_dir("bcmp");
    sheet_set(&dir);
    for d in ["out", "out2"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    let mut a = automation(&dir);
    // Without subfolders nothing is found in revised/.
    let m = call(
        &mut a,
        "batch_match",
        json!({ "current": ["current"], "revised": ["revised"], "filter": "@?#" }),
    );
    assert_eq!(m["pairs"].as_array().unwrap().len(), 0, "{m}");
    let m = call(
        &mut a,
        "batch_match",
        json!({ "current": ["current"], "revised": ["revised"], "recursive": true, "filter": "@?#" }),
    );
    assert_eq!(
        pair_keys(&m),
        vec![
            ("A-101.pdf".to_string(), "A-101 rev 2.pdf".to_string()),
            ("A-102.pdf".to_string(), "A-102 rev 2.pdf".to_string())
        ],
        "{m}"
    );
    let r = call(
        &mut a,
        "batch_compare",
        json!({ "current": ["current"], "revised": ["revised"], "recursive": true, "filter": "@?#",
                "out_dir": "out", "report": "out/report.csv" }),
    );
    let csv = std::fs::read_to_string(dir.join("out").join("report.csv")).unwrap();
    assert!(csv.contains("A-101") && csv.contains("A-102"), "{csv}\n{r}");
    // The results: A-101 has clouds, A-102 none.
    let results: Vec<PathBuf> = std::fs::read_dir(dir.join("out"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
        .collect();
    assert_eq!(results.len(), 2, "{results:?} {r}");
    for p in &results {
        let mut b = automation(&dir.join("out"));
        let n = call(&mut b, "doc_open", json!({ "path": name(p) }))["markups"]
            .as_u64()
            .unwrap();
        if name(p).contains("A-101") {
            assert!(n >= 3, "A-101 result should carry the clouds: {n}");
        } else {
            assert_eq!(n, 0, "A-102 did not change: {n}");
        }
    }
    // The revised files were not touched.
    let mut c = automation(&dir.join("revised").join("issue"));
    assert_eq!(
        call(&mut c, "doc_open", json!({ "path": "A-101 rev 2.pdf" }))["markups"],
        0
    );
    // PDF report with a link to each result.
    call(
        &mut a,
        "batch_compare",
        json!({ "current": ["current"], "revised": ["revised"], "recursive": true, "filter": "@?#",
                "out_dir": "out2", "report": "out2/report.pdf" }),
    );
    let mut d = automation(&dir.join("out2"));
    call(&mut d, "doc_open", json!({ "path": "report.pdf" }));
    let links = call(&mut d, "link_list", json!({}));
    assert!(links.to_string().matches("A-10").count() >= 2, "report links: {links}");
}

/// ui-019, ui-021, ui-022: match by page label and by a title-block region, re-pair by hand, and
/// save the job as a batch file reused by Batch Overlay.
#[test]
fn batch_match_modes_repair_and_saved_job() {
    let dir = temp_dir("bmatch");
    sheet_set(&dir);
    let mut a = automation(&dir);
    // The sheet number in the title block (480, 20) decides the pairing.
    let m = call(
        &mut a,
        "batch_match",
        json!({ "current": ["current"], "revised": ["revised"], "recursive": true, "match": "region", "region": [470, 10, 560, 40] }),
    );
    assert_eq!(pair_keys(&m).len(), 2, "{m}");
    for (c, r) in pair_keys(&m) {
        assert_eq!(c[..5], r[..5], "region matching paired the wrong sheets: {m}");
    }
    // Re-pair by hand (A-101 with A-102's revision) and save the job.
    let cur = |n: &str| format!("current/{n}");
    let rev = |n: &str| format!("revised/issue/{n}");
    let m = call(
        &mut a,
        "batch_match",
        json!({ "match": "manual", "pairs": [
            { "current": cur("A-101.pdf"), "current_page": 1, "revised": rev("A-102 rev 2.pdf"), "revised_page": 1 }
        ], "save_job": "job.pcbatch" }),
    );
    assert_eq!(
        pair_keys(&m),
        vec![("A-101.pdf".to_string(), "A-102 rev 2.pdf".to_string())],
        "{m}"
    );
    assert!(dir.join("job.pcbatch").exists());
    for d in ["ovl", "cmp"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    // The saved job drives Batch Overlay.
    let r = call(
        &mut a,
        "batch_overlay",
        json!({ "job": "job.pcbatch", "out_dir": "ovl" }),
    );
    let n = std::fs::read_dir(dir.join("ovl")).unwrap().count();
    assert_eq!(n, 1, "{r}");
    // ... and Batch Compare.
    let r = call(
        &mut a,
        "batch_compare",
        json!({ "job": "job.pcbatch", "out_dir": "cmp" }),
    );
    assert_eq!(std::fs::read_dir(dir.join("cmp")).unwrap().count(), 1, "{r}");
    // Page label matching: label both sets' pages and pair by label.
    for (folder, file) in [("current", "A-101.pdf"), ("revised/issue", "A-101 rev 2.pdf")] {
        let mut b = automation(&dir.join(folder));
        call(&mut b, "doc_open", json!({ "path": file }));
        call(&mut b, "page_label_set", json!({ "labels": { "1": "SHEET-X" } }));
        call(&mut b, "doc_save", json!({}));
    }
    let m = call(
        &mut a,
        "batch_match",
        json!({ "current": ["current/A-101.pdf", "current/A-102.pdf"], "revised": ["revised/issue/A-102 rev 2.pdf", "revised/issue/A-101 rev 2.pdf"], "match": "label" }),
    );
    assert!(
        pair_keys(&m).contains(&("A-101.pdf".to_string(), "A-101 rev 2.pdf".to_string())),
        "label matching: {m}"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Overlay Pages
// ---------------------------------------------------------------------------------------------

/// ui-024, ui-025, ui-026, ui-029, ui-031, ui-036, ui-039: two revisions overlaid page-aligned:
/// shared linework dark, removed and added linework each in its layer's colour (a contrasting
/// pair by default, changeable), each source a named, toggleable PDF layer.
#[test]
fn overlay_recolors_and_stacks_revisions_as_layers() {
    let dir = temp_dir("ovl");
    revisions(&dir);
    let mut a = automation(&dir);
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "old.pdf" }, { "path": "new.pdf" }], "out": "ovl.pdf" }),
    );
    let out = dir.join("ovl.pdf");
    let shared = pixel(&out, 0, 300.0, 50.0);
    let removed = pixel(&out, 0, 300.0, 200.0);
    let added = pixel(&out, 0, 330.0, 200.0);
    assert!(is_dark(shared), "shared wall not dark: {shared:?}");
    assert!(
        is_tinted(removed) && is_tinted(added),
        "removed {removed:?} added {added:?}"
    );
    assert_ne!(removed, added, "both layers in the same colour");
    assert!(is_white(pixel(&out, 0, 200.0, 200.0)), "paper not white");

    // Custom colours and names; layers listed in the result.
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [
            { "path": "old.pdf", "color": "#00FF00", "name": "Issued" },
            { "path": "new.pdf", "color": "#0000FF", "name": "Revision 2" }
        ], "out": "ovl2.pdf" }),
    );
    let out2 = dir.join("ovl2.pdf");
    let r = pixel(&out2, 0, 300.0, 200.0);
    let b = pixel(&out2, 0, 330.0, 200.0);
    assert!(r[1] > r[0] + 60 && r[1] > r[2] + 60, "old layer not green: {r:?}");
    assert!(b[2] > b[0] + 60 && b[2] > b[1] + 60, "new layer not blue: {b:?}");
    open(&mut a, &out2);
    let layers = call(&mut a, "layer_list", json!({}));
    let names: Vec<&str> = layers["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Issued") && names.contains(&"Revision 2"), "{layers}");
    // Hiding one layer hides that revision.
    call(&mut a, "layer_set", json!({ "name": "Revision 2", "visible": false }));
    call(&mut a, "doc_save", json!({ "path": "ovl2_hidden.pdf" }));
    let hidden = dir.join("ovl2_hidden.pdf");
    assert!(is_white(pixel(&hidden, 0, 330.0, 200.0)), "hidden layer still drawn");
    assert!(!is_white(pixel(&hidden, 0, 300.0, 200.0)), "the other layer vanished");
}

/// ui-027, ui-028: a layer's whitespace colour and its opacity.
#[test]
fn overlay_layer_background_and_opacity() {
    let dir = temp_dir("ovl-bg");
    revisions(&dir);
    let mut a = automation(&dir);
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "old.pdf", "background": "#FFFF00" }, { "path": "new.pdf" }], "out": "bg.pdf" }),
    );
    let paper = pixel(&dir.join("bg.pdf"), 0, 200.0, 200.0);
    assert!(
        paper[0] > 200 && paper[1] > 200 && paper[2] < 120,
        "background not yellow: {paper:?}"
    );
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "old.pdf" }, { "path": "new.pdf" }], "out": "full.pdf" }),
    );
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "old.pdf", "opacity": 0.3 }, { "path": "new.pdf" }], "out": "faint.pdf" }),
    );
    let full = pixel(&dir.join("full.pdf"), 0, 300.0, 200.0);
    let faint = pixel(&dir.join("faint.pdf"), 0, 300.0, 200.0);
    let sum = |c: [u8; 3]| c.iter().map(|&v| v as u32).sum::<u32>();
    assert!(
        sum(faint) > sum(full) + 100 && !is_white(faint),
        "opacity 0.3 not lighter: {full:?} vs {faint:?}"
    );
}

/// ui-029, ui-030: blend modes and Advanced Color Shading.
#[test]
fn overlay_blend_mode_and_advanced_shading() {
    let dir = temp_dir("ovl-blend");
    revisions(&dir);
    let mut a = automation(&dir);
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "old.pdf" }, { "path": "new.pdf" }], "out": "m.pdf" }),
    );
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "old.pdf" }, { "path": "new.pdf" }], "out": "n.pdf", "defaults": { "blend": "normal" } }),
    );
    // Default darkens where both have ink; normal lets the top layer cover.
    let m = pixel(&dir.join("m.pdf"), 0, 300.0, 50.0);
    let n = pixel(&dir.join("n.pdf"), 0, 300.0, 50.0);
    assert!(is_dark(m), "{m:?}");
    assert!(
        is_tinted(n),
        "normal blend should show the top layer's colour at shared ink: {n:?}"
    );
    // Grey fill in both revisions: shading keeps more of its tone.
    let grey = format!("{}0.6 g 120 120 60 60 re f\n", plan(0));
    write_pdf(&dir, "g1.pdf", std::slice::from_ref(&grey));
    write_pdf(&dir, "g2.pdf", &[grey]);
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "g1.pdf" }, { "path": "g2.pdf" }], "out": "plain.pdf" }),
    );
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "g1.pdf" }, { "path": "g2.pdf" }], "out": "shaded.pdf", "advanced_shading": true }),
    );
    let p = pixel(&dir.join("plain.pdf"), 0, 150.0, 150.0);
    let s = pixel(&dir.join("shaded.pdf"), 0, 150.0, 150.0);
    assert_ne!(p, s, "advanced shading changed nothing");
    assert!(!is_white(s), "{s:?}");
}

/// A copy of the plan moved by (dx, dy) and scaled.
fn moved_plan(dx: f64, dy: f64, s: f64) -> String {
    format!("q {s} 0 0 {s} {dx} {dy} cm\n{}Q\n", plan(0))
}

/// ui-031, ui-032, ui-033, ui-037: Page align keeps a moved sheet apart; Auto align, three
/// matching points and the default position offset register it on the first layer.
#[test]
fn overlay_alignment_page_auto_points_and_position_defaults() {
    let dir = temp_dir("ovl-align");
    write_pdf(&dir, "old.pdf", &[plan(0)]);
    write_pdf(&dir, "moved.pdf", &[moved_plan(40.0, 20.0, 1.0)]);
    write_pdf(&dir, "scaled.pdf", &[moved_plan(10.0, 5.0, 0.8)]);
    let mut a = automation(&dir);
    let overlay = |a: &mut Automation, second: Value, out: &str, extra: Value| {
        let mut args = json!({ "layers": [{ "path": "old.pdf" }, second], "out": out });
        for (k, v) in extra.as_object().unwrap() {
            args[k] = v.clone();
        }
        call(a, "overlay_pages", args);
        dir.join(out)
    };
    // Left wall at x = 50: page-aligned, the moved copy does not share it.
    let p = overlay(&mut a, json!({ "path": "moved.pdf" }), "page.pdf", json!({}));
    assert!(
        is_tinted(pixel(&p, 0, 50.0, 200.0)),
        "page align should not register: {:?}",
        pixel(&p, 0, 50.0, 200.0)
    );
    let p = overlay(
        &mut a,
        json!({ "path": "moved.pdf", "align": "auto" }),
        "auto.pdf",
        json!({}),
    );
    assert!(
        is_dark(pixel(&p, 0, 50.0, 200.0)),
        "auto align: {:?}",
        pixel(&p, 0, 50.0, 200.0)
    );
    let p = overlay(
        &mut a,
        json!({ "path": "scaled.pdf", "align": "auto" }),
        "auto_s.pdf",
        json!({}),
    );
    assert!(
        is_dark(pixel(&p, 0, 50.0, 200.0)),
        "auto align of a scaled sheet: {:?}",
        pixel(&p, 0, 50.0, 200.0)
    );
    let p = overlay(
        &mut a,
        json!({ "path": "scaled.pdf", "align": "points",
                "from": [[50.0 * 0.8 + 10.0, 50.0 * 0.8 + 5.0], [550.0 * 0.8 + 10.0, 50.0 * 0.8 + 5.0], [50.0 * 0.8 + 10.0, 350.0 * 0.8 + 5.0]],
                "to": [[50, 50], [550, 50], [50, 350]] }),
        "pts.pdf",
        json!({}),
    );
    assert!(
        is_dark(pixel(&p, 0, 50.0, 200.0)),
        "three-point align: {:?}",
        pixel(&p, 0, 50.0, 200.0)
    );
    assert!(
        is_dark(pixel(&p, 0, 300.0, 200.0)),
        "three-point align interior: {:?}",
        pixel(&p, 0, 300.0, 200.0)
    );
    // Position defaults: an X/Y offset applied to layers that set none.
    let p = overlay(
        &mut a,
        json!({ "path": "old.pdf" }),
        "defaults.pdf",
        json!({ "defaults": { "dx": 40, "dy": 20 } }),
    );
    // Both layers moved alike: the wall shows dark at x = 90.
    assert!(
        is_dark(pixel(&p, 0, 90.0, 220.0)),
        "defaults offset: {:?}",
        pixel(&p, 0, 90.0, 220.0)
    );
    // A rotation default of 180 turns layers that do not set their own: a block at the lower
    // left of the first layer lands at the upper right; the second layer (rotation 0) stays.
    write_pdf(&dir, "block.pdf", &[rect(100.0, 100.0, 40.0, 40.0)]);
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "block.pdf" }, { "path": "block.pdf", "rotation": 0 }], "out": "rot.pdf",
                "defaults": { "rotation": 180 } }),
    );
    let p = dir.join("rot.pdf");
    assert!(
        is_tinted(pixel(&p, 0, 120.0, 120.0)),
        "unrotated layer: {:?}",
        pixel(&p, 0, 120.0, 120.0)
    );
    assert!(
        is_tinted(pixel(&p, 0, 480.0, 280.0)),
        "rotated layer: {:?}",
        pixel(&p, 0, 480.0, 280.0)
    );
}

/// ui-034, ui-035: a region of a layer's sheet and a page choice per layer.
#[test]
fn overlay_region_and_pages_per_layer() {
    let dir = temp_dir("ovl-region");
    write_pdf(&dir, "old.pdf", &[plan(0), plan(0)]);
    write_pdf(&dir, "new.pdf", &[plan(0), plan(1)]);
    let mut a = automation(&dir);
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "old.pdf", "pages": [1] }, { "path": "new.pdf", "pages": [2], "region": [320, 60, 540, 340] }], "out": "r.pdf" }),
    );
    let out = dir.join("r.pdf");
    open(&mut a, &out);
    // New page 2's wall at 330 (inside the region) and door are drawn; its outer left wall at 50
    // (outside the region) is not, so the old layer's wall there stays in the old colour.
    assert!(
        is_tinted(pixel(&out, 0, 330.0, 200.0)),
        "{:?}",
        pixel(&out, 0, 330.0, 200.0)
    );
    assert!(
        is_tinted(pixel(&out, 0, 420.0, 154.0)),
        "{:?}",
        pixel(&out, 0, 420.0, 154.0)
    );
    assert!(
        is_tinted(pixel(&out, 0, 50.0, 200.0)),
        "region leaked: {:?}",
        pixel(&out, 0, 50.0, 200.0)
    );
    assert_eq!(call(&mut a, "doc_info", json!({}))["pages"], 1);
}

/// ui-038: recoverable flattened markups are left out unless asked for.
#[test]
fn overlay_include_flattened_markups() {
    let dir = temp_dir("ovl-flat");
    let (old, _) = revisions(&dir);
    let mut a = automation(&dir);
    open(&mut a, &old);
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[420, 250], [500, 320]], "color": "#000000", "width": 4 }),
    );
    let ids = a_ids(&mut a, "");
    call(&mut a, "markup_flatten", json!({ "ids": ids, "recoverable": true }));
    call(&mut a, "doc_save", json!({ "path": "flat.pdf" }));
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "flat.pdf" }, { "path": "new.pdf" }], "out": "no.pdf" }),
    );
    call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "flat.pdf" }, { "path": "new.pdf" }], "out": "yes.pdf", "include_flattened": true }),
    );
    assert!(
        is_white(pixel(&dir.join("no.pdf"), 0, 420.0, 285.0)),
        "flattened markup included by default"
    );
    assert!(
        !is_white(pixel(&dir.join("yes.pdf"), 0, 420.0, 285.0)),
        "flattened markup missing when asked for"
    );
}

/// ui-040, ui-041: Batch Overlay over matched sets; Smart Overlay scores each sheet.
#[test]
fn batch_overlay_and_smart_overlay() {
    let dir = temp_dir("bovl");
    sheet_set(&dir);
    for d in ["out", "smart"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    let mut a = automation(&dir);
    let r = call(
        &mut a,
        "batch_overlay",
        json!({ "current": ["current"], "revised": ["revised"], "recursive": true, "filter": "@?#", "out_dir": "out", "report": "out/r.csv" }),
    );
    let pdfs: Vec<PathBuf> = std::fs::read_dir(dir.join("out"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
        .collect();
    assert_eq!(pdfs.len(), 2, "{r}");
    let a101 = pdfs.iter().find(|p| name(p).contains("A-101")).unwrap();
    assert!(is_tinted(pixel(a101, 0, 300.0, 200.0)) && is_dark(pixel(a101, 0, 50.0, 200.0)));
    assert!(
        std::fs::read_to_string(dir.join("out").join("r.csv"))
            .unwrap()
            .contains("A-101")
    );

    let s = call(
        &mut a,
        "smart_overlay",
        json!({ "current": ["current"], "revised": ["revised"], "recursive": true, "filter": "@?#", "out_dir": "smart" }),
    );
    let sheets = s["sheets"].as_array().unwrap();
    assert_eq!(sheets.len(), 2, "{s}");
    let score = |n: &str| {
        sheets
            .iter()
            .find(|x| x["sheet"].as_str().unwrap().contains(n))
            .unwrap()["score"]
            .as_f64()
            .unwrap()
    };
    assert!(score("A-102") > score("A-101"), "{s}");
    assert!(score("A-102") > 0.99, "{s}");
    assert!(s["disciplines"].to_string().contains('A'), "{s}");
}

// ---------------------------------------------------------------------------------------------
// UI helpers
// ---------------------------------------------------------------------------------------------

/// The real app, headless, with `bytes` open (saved at `path` when given).
fn app_with(bytes: Vec<u8>, path: Option<PathBuf>) -> Harness<'static, MarkupCraftApp> {
    markupcraft_ui_egui::richedit::load_dictionary_blocking();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(move |_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            app.open_bytes("plan.pdf", path.clone(), bytes.clone()).unwrap();
            app
        });
    h.run_steps(6);
    h
}

/// Show a panel (open and in front), as Window > Panels does.
fn panel(h: &mut Harness<'_, MarkupCraftApp>, id: &'static str) {
    h.state_mut().state.show_panel(id);
    h.run_steps(4);
}

/// Click the last widget labelled exactly `label` (a panel's tab title comes first).
fn press_last(h: &mut Harness<'_, MarkupCraftApp>, label: &str) {
    h.query_all_by_label(label)
        .last()
        .unwrap_or_else(|| panic!("no widget labelled {label:?}"))
        .click();
    h.run_steps(4);
}

/// Click the (first) widget labelled exactly `label`.
fn press(h: &mut Harness<'_, MarkupCraftApp>, label: &str) {
    h.query_all_by_label(label)
        .next()
        .unwrap_or_else(|| panic!("no widget labelled {label:?}"))
        .click();
    h.run_steps(4);
}

// ---------------------------------------------------------------------------------------------
// 3. Search
// ---------------------------------------------------------------------------------------------

/// Page 1: "DOOR 1" (100, 300), "DOOR 2" (300, 300), "door" (100, 100), "DOORWAY" (300, 100).
/// Page 2: "DOOR 3" (100, 300).
fn search_doc() -> Vec<u8> {
    let p1 = [
        text(100.0, 300.0, 12.0, "DOOR 1"),
        text(300.0, 300.0, 12.0, "DOOR 2"),
        text(100.0, 100.0, 12.0, "door"),
        text(300.0, 100.0, 12.0, "DOORWAY"),
    ]
    .concat();
    let p2 = text(100.0, 300.0, 12.0, "DOOR 3");
    synthetic::pdf(&[
        SyntheticPage::new(600.0, 400.0, p1),
        SyntheticPage::new(600.0, 400.0, p2),
    ])
}

fn hits(v: &Value) -> Vec<Value> {
    v["hits"].as_array().unwrap().clone()
}

fn hit_covers(h: &Value, x: f64, y: f64) -> bool {
    h["rects"].as_array().unwrap().iter().any(|r| {
        let a: Vec<f64> = r.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        covers(&[a[0], a[1], a[2], a[3]], x, y)
    })
}

/// ui-042, ui-044, ui-047, ui-048: text search in the page text with case and whole-word
/// modifiers; each hit has its page, the word highlighted in context and its position.
#[test]
fn text_search_page_text_modifiers_and_hit_positions() {
    let dir = temp_dir("search");
    std::fs::write(dir.join("s.pdf"), search_doc()).unwrap();
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "s.pdf" }));
    let count = |a: &mut Automation, args: Value| call(a, "text_search", args)["count"].as_u64().unwrap();
    assert_eq!(count(&mut a, json!({ "text": "door" })), 5);
    assert_eq!(count(&mut a, json!({ "text": "door", "whole_words": true })), 4);
    assert_eq!(count(&mut a, json!({ "text": "DOOR", "case_sensitive": true })), 4);
    assert_eq!(
        count(
            &mut a,
            json!({ "text": "DOOR", "case_sensitive": true, "whole_words": true })
        ),
        3
    );
    assert_eq!(count(&mut a, json!({ "text": "window" })), 0);
    let r = call(&mut a, "text_search", json!({ "text": "door", "whole_words": true }));
    let hs = hits(&r);
    let pages: Vec<u64> = hs.iter().map(|h| h["page"].as_u64().unwrap()).collect();
    assert_eq!(pages.iter().filter(|&&p| p == 1).count(), 3, "{r}");
    assert_eq!(pages.iter().filter(|&&p| p == 2).count(), 1, "{r}");
    for (p, x, y) in [
        (1, 105.0, 304.0),
        (1, 305.0, 304.0),
        (1, 105.0, 104.0),
        (2, 105.0, 304.0),
    ] {
        assert!(
            hs.iter().any(|h| h["page"] == p && hit_covers(h, x, y)),
            "no hit at page {p} ({x}, {y}): {r}"
        );
    }
    // No hit rectangle sits on DOORWAY with whole words.
    assert!(!hs.iter().any(|h| h["page"] == 1 && hit_covers(h, 320.0, 104.0)), "{r}");
    // Context shows the line the word is in.
    assert!(
        hs.iter().any(|h| h["context"].as_str().unwrap().contains("DOOR 2")),
        "{r}"
    );
}

/// ui-042, ui-048, ui-049, ui-054: the Search panel lists hits grouped by page; F3 / Shift+F3
/// step through them and move the view; Clear empties the list.
#[test]
fn search_panel_results_stepping_and_clear() {
    let mut h = app_with(search_doc(), None);
    panel(&mut h, "search");
    assert!(
        shows(&h, "Match case") && shows(&h, "Whole words"),
        "search panel not shown"
    );
    h.state_mut().state.features.search.query = "door".into();
    h.state_mut().state.features.search.whole_words = true;
    h.run_steps(2);
    press_last(&mut h, "Search");
    assert_eq!(h.state().state.features.search.hits.len(), 4);
    assert!(
        shows(&h, "Page 1") && shows(&h, "Page 2"),
        "results not grouped by page"
    );
    // F3 steps forward one hit at a time until the page 2 hit (the last one).
    let mut prev = h.state().state.features.search.current;
    let mut cur = 0;
    for _ in 0..4 {
        key(&mut h, egui::Modifiers::NONE, egui::Key::F3);
        cur = h
            .state()
            .state
            .features
            .search
            .current
            .expect("F3 did not select a result");
        assert_eq!(cur, prev.map_or(0, |p| (p + 1) % 4), "F3 should step to the next hit");
        prev = Some(cur);
        if cur == 3 {
            break;
        }
    }
    assert_eq!(cur, 3);
    assert_eq!(
        h.state().state.features.search.hits[cur].page,
        1,
        "the last hit is on page 2"
    );
    assert_eq!(
        h.state().state.doc().unwrap().view.current,
        1,
        "the view did not move to page 2"
    );
    key(&mut h, egui::Modifiers::SHIFT, egui::Key::F3);
    let back = h.state().state.features.search.current.unwrap();
    assert_eq!(back + 1, cur, "Shift+F3 should step back");
    assert_eq!(h.state().state.doc().unwrap().view.current, 0);
    press(&mut h, "Clear");
    assert!(h.state().state.features.search.hits.is_empty(), "Clear left results");
}

/// ui-043: scope: page range, other files, a folder with subfolders, a Set, every open document.
#[test]
fn text_search_scopes() {
    let dir = temp_dir("search-scope");
    std::fs::write(dir.join("s.pdf"), search_doc()).unwrap();
    std::fs::create_dir_all(dir.join("sub").join("deeper")).unwrap();
    std::fs::write(dir.join("sub").join("deeper").join("t.pdf"), search_doc()).unwrap();
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "s.pdf" }));
    let r = call(&mut a, "text_search", json!({ "text": "door", "pages": [2] }));
    assert_eq!(r["count"], 1, "{r}");
    let r = call(
        &mut a,
        "text_search",
        json!({ "text": "door", "files": ["sub/deeper/t.pdf"] }),
    );
    assert_eq!(r["count"], 5, "{r}");
    let r = call(&mut a, "text_search", json!({ "text": "door", "folder": "sub" }));
    assert_eq!(r["count"].as_u64().unwrap_or(0), 0, "folder without subfolders: {r}");
    let r = call(
        &mut a,
        "text_search",
        json!({ "text": "door", "folder": "sub", "recursive": true }),
    );
    assert_eq!(r["count"], 5, "{r}");
    call(
        &mut a,
        "set_save",
        json!({ "path": "set.pcset", "name": "Set", "files": ["s.pdf", "sub/deeper/t.pdf"] }),
    );
    let r = call(&mut a, "text_search", json!({ "text": "door", "set": "set.pcset" }));
    assert_eq!(r["count"], 10, "{r}");
    call(&mut a, "doc_open", json!({ "path": "sub/deeper/t.pdf" }));
    let r = call(&mut a, "text_search", json!({ "text": "door", "open_docs": true }));
    let files = r["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "{r}");
    assert!(files.iter().all(|f| f["count"] == 5), "{r}");
}

/// ui-045, ui-046: markup text, file names, document properties and form field values.
#[test]
fn text_search_markups_file_names_properties_and_fields() {
    let dir = temp_dir("search-targets");
    std::fs::write(dir.join("door-schedule.pdf"), search_doc()).unwrap();
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "door-schedule.pdf" }));
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[400, 200], [450, 250]], "contents": "check HINGE hardware" }),
    );
    call(
        &mut a,
        "doc_properties_set",
        json!({ "properties": { "Title": "Hinge schedule" } }),
    );
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 2, "rect": [300, 200, 450, 220], "type": "text", "name": "Spec" }),
    );
    call(&mut a, "form_fill", json!({ "values": { "Spec": "HINGE type B" } }));
    // Page text only (default): nothing.
    let r = call(&mut a, "text_search", json!({ "text": "hinge" }));
    assert_eq!(r["count"], 0, "page text search found markup / field text: {r}");
    let r = call(&mut a, "text_search", json!({ "text": "hinge", "markups": true }));
    assert_eq!(r["count"], 1, "{r}");
    assert_eq!(r["hits"][0]["source"], "markup", "{r}");
    assert_eq!(r["hits"][0]["page"], 1, "{r}");
    let r = call(&mut a, "text_search", json!({ "text": "hinge", "properties": true }));
    assert!(hits(&r).iter().any(|h| h["source"] == "property"), "{r}");
    let r = call(&mut a, "text_search", json!({ "text": "hinge", "form_fields": true }));
    assert!(
        hits(&r).iter().any(|h| h["source"] == "form_field" && h["page"] == 2),
        "{r}"
    );
    let r = call(
        &mut a,
        "text_search",
        json!({ "text": "schedule", "file_names": true, "page_text": false }),
    );
    assert!(hits(&r).iter().any(|h| h["source"] == "file_name"), "{r}");
    // File names over a folder.
    let r = call(
        &mut a,
        "text_search",
        json!({ "text": "schedule", "file_names": true, "folder": ".", "page_text": false }),
    );
    assert_eq!(r["count"], 1, "{r}");
}

/// ui-050: select text on the page and search for it.
#[test]
fn search_selected_text() {
    let dir = temp_dir("search-sel");
    std::fs::write(dir.join("s.pdf"), search_doc()).unwrap();
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "s.pdf" }));
    let t = call(&mut a, "region_text", json!({ "page": 1, "rect": [95, 295, 160, 315] }));
    let words = t["text"].as_str().unwrap_or_else(|| panic!("{t}")).trim().to_string();
    assert_eq!(words, "DOOR 1", "{t}");
    assert_eq!(call(&mut a, "text_search", json!({ "text": words }))["count"], 1);

    // In the app: the Search Selected Text tool, drag over the words, the panel searches them.
    let mut h = app_with(search_doc(), None);
    run(&mut h, "tools.search_selection");
    drag(&mut h, (95.0, 315.0), (160.0, 295.0));
    h.run_steps(4);
    let s = &h.state().state.features.search;
    assert_eq!(s.query.trim(), "DOOR 1", "selection not searched");
    assert_eq!(s.hits.len(), 1);
}

/// ui-051, ui-052: check results, then highlight, count or redact every checked hit.
#[test]
fn search_check_results_and_bulk_actions() {
    let mut h = app_with(search_doc(), None);
    panel(&mut h, "search");
    h.state_mut().state.features.search.query = "door".into();
    h.state_mut().state.features.search.whole_words = true;
    h.run_steps(2);
    press_last(&mut h, "Search");
    assert_eq!(h.state().state.features.search.hits.len(), 4);
    // Results arrive checked; Check all toggles them all off; then check two of the four.
    assert!(h.state().state.features.search.hits.iter().all(|x| x.checked));
    press(&mut h, "Check all");
    assert!(h.state().state.features.search.hits.iter().all(|x| !x.checked));
    {
        let s = &mut h.state_mut().state.features.search;
        s.hits[0].checked = true;
        s.hits[1].checked = true;
    }
    h.run_steps(2);
    let before = markups(&h).len();
    press(&mut h, "Highlight");
    let ms = markups(&h);
    assert_eq!(ms.len(), before + 2, "one highlight per checked hit");
    press(&mut h, "Count");
    let ms = markups(&h);
    let counts: Vec<_> = ms.iter().filter(|m| m.kind == Kind::Count).collect();
    let points: usize = counts.iter().map(|m| m.pts.len()).sum();
    assert_eq!(points, 2, "one count point per checked hit: {counts:?}");
    // The count points sit on the hits.
    let hs = h.state().state.features.search.hits.clone();
    for p in counts.iter().flat_map(|m| m.pts.iter()) {
        assert!(
            hs.iter().take(2).any(|hit| hit
                .rects
                .iter()
                .any(|r| r.x0 - 2.0 <= p.x && p.x <= r.x1 + 2.0 && r.y0 - 2.0 <= p.y && p.y <= r.y1 + 2.0)),
            "count point {p:?} not on a checked hit"
        );
    }
    // Check all, then mark for redaction.
    press(&mut h, "Check all");
    assert!(h.state().state.features.search.hits.iter().all(|x| x.checked));
    press(&mut h, "Redact");
    let redactions: usize = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .redact_marks()
        .iter()
        .map(|m| m.rects.len())
        .sum();
    assert_eq!(redactions, 4, "one redaction area per checked hit");
    // Underline, squiggly and strike through the checked hits; link them; bookmark them.
    {
        let s = &mut h.state_mut().state.features.search;
        for (i, x) in s.hits.iter_mut().enumerate() {
            x.checked = i < 2;
        }
    }
    h.run_steps(2);
    for (label, kind) in [
        ("Underline", Kind::Underline),
        ("Squiggly", Kind::Squiggly),
        ("Strikethrough", Kind::Strikeout),
    ] {
        let before = markups(&h).iter().filter(|m| m.kind == kind).count();
        press_last(&mut h, label);
        let now = markups(&h).iter().filter(|m| m.kind == kind).count();
        assert_eq!(now, before + 2, "{label}: one per checked hit");
    }
    h.state_mut().state.features.search.link_to = "https://example.com/doors".into();
    press_last(&mut h, "Hyperlink");
    let links = h.state().state.doc().unwrap().session.links();
    assert_eq!(links.len(), 2, "a link over each checked hit");
    assert!(
        links
            .iter()
            .all(|l| l.target == markupcraft_engine::links::LinkTarget::Url("https://example.com/doors".into()))
    );
    press_last(&mut h, "Bookmark");
    let b = h.state().state.doc().unwrap().session.bookmarks();
    assert_eq!(b.len(), 2, "a bookmark to each checked hit: {b:?}");
}

/// ui-053: replace the checked hits in the page content.
#[test]
fn search_replace_checked() {
    let dir = temp_dir("search-replace");
    std::fs::write(dir.join("s.pdf"), search_doc()).unwrap();
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "s.pdf" }));
    let r = call(&mut a, "text_search", json!({ "text": "DOOR 2" }));
    let rect = r["hits"][0]["rects"][0].clone();
    call(
        &mut a,
        "text_replace",
        json!({ "text": "DOOR", "with": "GATE", "whole_words": true, "only": [{ "page": 1, "rect": rect }] }),
    );
    let t = call(&mut a, "page_text", json!({ "page": 1 }))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(t.contains("GATE 2"), "{t}");
    assert!(
        t.contains("DOOR 1") && t.contains("DOORWAY"),
        "unchecked hits replaced: {t}"
    );
    let t2 = call(&mut a, "page_text", json!({ "page": 2 }))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(t2.contains("DOOR 3"), "{t2}");
    // Search sees the new text.
    assert_eq!(call(&mut a, "text_search", json!({ "text": "GATE" }))["count"], 1);
    // Undo restores it.
    call(&mut a, "edit_undo", json!({}));
    assert_eq!(call(&mut a, "text_search", json!({ "text": "GATE" }))["count"], 0);
}

// ---------------------------------------------------------------------------------------------
// Visual Search (ui-055 .. ui-061)
// ---------------------------------------------------------------------------------------------

/// An asymmetric symbol (an L with a dot) whose box is (x, y) .. (x + 20, y + 20).
fn symbol(x: f64, y: f64, rgb: &str) -> String {
    format!(
        "{rgb} rg {x} {y} 20 4 re f {x} {y} 4 18 re f {} {} 5 5 re f\n",
        x + 13.0,
        y + 11.0
    )
}

/// The symbol turned by `deg` degrees about its centre.
fn turned(x: f64, y: f64, deg: f64) -> String {
    let (s, c) = deg.to_radians().sin_cos();
    let (cx, cy) = (x + 10.0, y + 10.0);
    let e = cx - (c * 10.0 - s * 10.0);
    let f = cy - (s * 10.0 + c * 10.0);
    format!("q {c} {s} {} {c} {e} {f} cm\n{}Q\n", -s, symbol(0.0, 0.0, "0 0 0"))
}

const SYMBOLS: [(f64, f64); 4] = [(100.0, 300.0), (200.0, 300.0), (300.0, 300.0), (400.0, 100.0)];
const TURNED: (f64, f64) = (100.0, 100.0);
const TURNED45: (f64, f64) = (200.0, 100.0);
const RED: (f64, f64) = (500.0, 300.0);
const OTHER: (f64, f64) = (300.0, 200.0);

fn symbols_doc() -> Vec<u8> {
    let mut c = String::new();
    for (x, y) in SYMBOLS {
        c += &symbol(x, y, "0 0 0");
    }
    c += &turned(TURNED.0, TURNED.1, 90.0);
    c += &turned(TURNED45.0, TURNED45.1, 45.0);
    c += &symbol(RED.0, RED.1, "1 0 0");
    // A different symbol: a hollow square.
    c += &format!("0 G 2 w {} {} 20 20 re S\n", OTHER.0, OTHER.1);
    let p2 = symbol(250.0, 250.0, "0 0 0");
    synthetic::pdf(&[
        SyntheticPage::new(600.0, 400.0, c),
        SyntheticPage::new(600.0, 400.0, p2),
    ])
}

fn found(v: &Value, page: u64, at: (f64, f64)) -> bool {
    v["hits"].as_array().unwrap().iter().any(|h| {
        let a: Vec<f64> = h["rect"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap())
            .collect();
        h["page"] == page && covers(&[a[0], a[1], a[2], a[3]], at.0 + 10.0, at.1 + 10.0)
    })
}

fn visual(dir: &Path, extra: Value) -> Value {
    let mut a = automation(dir);
    call(&mut a, "doc_open", json!({ "path": "v.pdf" }));
    let mut args = json!({ "page": 1, "rect": [98, 298, 122, 322] });
    for (k, v) in extra.as_object().unwrap() {
        args[k] = v.clone();
    }
    call(&mut a, "visual_search", args)
}

/// ui-055, ui-056, ui-057: box a symbol and find every instance across the document, the
/// turned ones too; sensitivity trades strictness for variants.
#[test]
fn visual_search_finds_every_instance_with_rotations_and_sensitivity() {
    let dir = temp_dir("visual");
    std::fs::write(dir.join("v.pdf"), symbols_doc()).unwrap();
    let r = visual(&dir, json!({}));
    for at in SYMBOLS {
        assert!(found(&r, 1, at), "missed the symbol at {at:?}: {r}");
    }
    assert!(found(&r, 1, TURNED), "missed the 90-degree symbol: {r}");
    assert!(found(&r, 2, (250.0, 250.0)), "missed page 2: {r}");
    assert!(!found(&r, 1, OTHER), "a different symbol matched: {r}");
    // No duplicate hits on one instance.
    let n_hits = r["hits"].as_array().unwrap().len();
    assert!(n_hits <= 9, "duplicate hits: {r}");
    // Rotations off: the turned symbol is not found.
    let r = visual(&dir, json!({ "rotations": false }));
    assert!(!found(&r, 1, TURNED), "{r}");
    assert!(found(&r, 1, SYMBOLS[3]), "{r}");
    // 45-degree steps.
    let r = visual(&dir, json!({ "fine_rotations": true }));
    assert!(
        found(&r, 1, TURNED45),
        "45-degree symbol missed with fine rotations: {r}"
    );
    // Strict sensitivity still finds the exact copies, loose finds at least as many.
    let strict = visual(&dir, json!({ "sensitivity": 0.0 }));
    let loose = visual(&dir, json!({ "sensitivity": 1.0 }));
    for at in SYMBOLS {
        assert!(found(&strict, 1, at), "strict missed an exact copy at {at:?}: {strict}");
    }
    assert!(
        loose["count"].as_u64().unwrap() >= strict["count"].as_u64().unwrap(),
        "loose should find at least as many: {} vs {}",
        loose["count"],
        strict["count"]
    );
}

/// ui-058, ui-059: colour filter and limit to selection.
#[test]
fn visual_search_color_filter_and_limit_to_selection() {
    let dir = temp_dir("visual-color");
    std::fs::write(dir.join("v.pdf"), symbols_doc()).unwrap();
    let r = visual(&dir, json!({ "color_filter": true }));
    assert!(!found(&r, 1, RED), "colour filter kept the red copy: {r}");
    assert!(found(&r, 1, SYMBOLS[1]), "{r}");
    // Without the filter, the red copy is the same shape.
    let r = visual(&dir, json!({}));
    assert!(found(&r, 1, RED), "red copy not found without the colour filter: {r}");

    // A wall line runs across the selection box (above the symbol, not touching it); limit to
    // selection ignores it. (A line that touches the symbol takes the symbol with it: see the
    // ui-059 gap note.)
    let mut c = String::new();
    for (x, y) in SYMBOLS {
        c += &symbol(x, y, "0 0 0");
    }
    c += &line(60.0, 320.5, 140.0, 320.5, 1.0);
    std::fs::write(
        dir.join("v.pdf"),
        synthetic::pdf(&[SyntheticPage::new(600.0, 400.0, c)]),
    )
    .unwrap();
    let score = |r: &Value, at: (f64, f64)| {
        r["hits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| {
                let a: Vec<f64> = h["rect"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_f64().unwrap())
                    .collect();
                covers(&[a[0], a[1], a[2], a[3]], at.0 + 10.0, at.1 + 10.0)
            })
            .map_or(0.0, |h| h["score"].as_f64().unwrap())
    };
    let plain = visual(&dir, json!({ "sensitivity": 1.0 }));
    let limited = visual(&dir, json!({ "sensitivity": 1.0, "limit_to_selection": true }));
    for at in &SYMBOLS[1..] {
        assert!(
            score(&limited, *at) > score(&plain, *at) + 0.02,
            "limit to selection should match the clean copies better at {at:?}: {} vs {}",
            score(&limited, *at),
            score(&plain, *at)
        );
    }
    // ui-059: a wall line that runs into the symbol and on out of the box: only that vector
    // object is dropped, the symbol it touches stays the template.
    let mut c = String::new();
    for (x, y) in SYMBOLS {
        c += &symbol(x, y, "0 0 0");
    }
    c += &line(40.0, 309.0, 100.0, 309.0, 1.0);
    std::fs::write(
        dir.join("v.pdf"),
        synthetic::pdf(&[SyntheticPage::new(600.0, 400.0, c)]),
    )
    .unwrap();
    let limited = visual(
        &dir,
        json!({ "sensitivity": 0.5, "limit_to_selection": true, "rotations": false }),
    );
    for at in &SYMBOLS[1..] {
        assert!(
            score(&limited, *at) > 0.8,
            "the touched symbol is still the template: {at:?} scored {}: {limited}",
            score(&limited, *at)
        );
    }
}

/// ui-060, ui-061: each hit comes with a thumbnail; count or highlight every hit.
#[test]
fn visual_search_thumbnails_and_count() {
    let dir = temp_dir("visual-count");
    std::fs::write(dir.join("v.pdf"), symbols_doc()).unwrap();
    let r = visual(&dir, json!({ "thumbnails": 48, "rotations": false }));
    for h in r["hits"].as_array().unwrap() {
        let t = h["thumbnail"]
            .as_str()
            .unwrap_or_else(|| panic!("hit without thumbnail: {h}"));
        assert!(t.starts_with("iVBOR"), "thumbnail is not a PNG");
    }
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "v.pdf" }));
    let r = call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [98, 298, 122, 322], "rotations": false, "action": "count", "subject": "Fixture" }),
    );
    let n = r["count"].as_u64().unwrap() as usize;
    let list = call(&mut a, "markup_list", json!({ "kind": "Count" }));
    let pts: usize = list["markups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["points"].as_array().unwrap().len())
        .sum();
    assert_eq!(pts, n, "one count point per hit: {list}");
    assert!(
        list["markups"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["subject"] == "Fixture")
    );
    let before = call(&mut a, "markup_list", json!({}))["count"].as_u64().unwrap();
    call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [98, 298, 122, 322], "rotations": false, "action": "highlight" }),
    );
    let after = call(&mut a, "markup_list", json!({}))["count"].as_u64().unwrap();
    assert_eq!(after - before, n as u64, "one highlight per hit");
    // ui-061: hyperlink and bookmark every hit too.
    let r = call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [98, 298, 122, 322], "rotations": false, "action": "hyperlink", "url": "https://example.com/fixture" }),
    );
    assert_eq!(r["links"].as_array().unwrap().len(), n, "a link per hit: {r}");
    let l = call(&mut a, "link_list", json!({}));
    assert!(l.to_string().contains("https://example.com/fixture"), "{l}");
    call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [98, 298, 122, 322], "rotations": false, "action": "bookmark", "subject": "Fixture" }),
    );
    let b = call(&mut a, "bookmark_list", json!({}));
    assert_eq!(b.to_string().matches("Fixture ").count(), n, "a bookmark per hit: {b}");

    // The panel: Visual Search hits listed with thumbnails, Count on the checked ones.
    let mut h = app_with(symbols_doc(), None);
    panel(&mut h, "search");
    {
        let s = &mut h.state_mut().state.features.search;
        s.visual = true;
        s.rotations = false;
        s.region = Some((0, markupcraft_geom::Rect::new(98.0, 298.0, 122.0, 322.0)));
    }
    h.run_steps(2);
    press_last(&mut h, "Search");
    h.run_steps(6);
    let n = h.state().state.features.search.visual_hits.len();
    assert!(n >= 5, "visual hits in the panel: {n}");
    assert!(!h.state().state.features.search.thumbs.is_empty(), "no thumbnails made");
    // Hits arrive checked: Count them all.
    assert!(h.state().state.features.search.visual_hits.iter().all(|x| x.checked));
    press(&mut h, "Count");
    let pts: usize = markups(&h)
        .iter()
        .filter(|m| m.kind == Kind::Count)
        .map(|m| m.pts.len())
        .sum();
    assert_eq!(pts, n);
    // ui-061: the panel's other check options on visual hits: highlight boxes, links, bookmarks.
    let before = markups(&h).len();
    press_last(&mut h, "Highlight");
    assert_eq!(markups(&h).len(), before + n, "a highlight box per hit");
    h.state_mut().state.features.search.link_to = "2".into();
    press_last(&mut h, "Hyperlink");
    let links = h.state().state.doc().unwrap().session.links();
    assert_eq!(links.len(), n, "a link per hit");
    assert!(
        links
            .iter()
            .all(|l| l.target == markupcraft_engine::links::LinkTarget::Page(1))
    );
    press_last(&mut h, "Bookmark");
    assert_eq!(
        h.state().state.doc().unwrap().session.bookmarks().len(),
        n,
        "a bookmark per hit"
    );
    assert!(
        h.query_all_by_label("Underline").next().is_none(),
        "text markups are offered on page text only"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Spaces, Links, Signatures
// ---------------------------------------------------------------------------------------------

fn rect_pts(x0: f64, y0: f64, x1: f64, y1: f64) -> Value {
    json!([[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
}

fn space(a: &mut Automation, name: &str, pts: Value) -> String {
    call(a, "space_add", json!({ "page": 1, "name": name, "points": pts }))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// ui-062, ui-063, ui-064, ui-065, ui-066: named (nestable) spaces; markups inside take the
/// innermost space's name; counts split per space; rename, reshape and delete.
#[test]
fn spaces_tag_markups_and_split_counts() {
    let dir = temp_dir("spaces");
    let (_, new) = revisions(&dir);
    let mut a = automation(&dir);
    open(&mut a, &new);
    space(&mut a, "Room A", rect_pts(50.0, 50.0, 300.0, 350.0));
    let closet = space(&mut a, "Closet", rect_pts(60.0, 60.0, 150.0, 150.0));
    let room_b = space(&mut a, "Room B", rect_pts(330.0, 50.0, 550.0, 350.0));
    // Count across spaces: 2 in Room A, 1 in the closet, 1 in Room B, 1 outside.
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Count", "subject": "Outlet",
                "points": [[200, 200], [250, 300], [100, 100], [400, 200], [580, 380]] }),
    );
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[400, 250], [450, 300]] }),
    );
    let rect_b_id = a_ids(&mut a, "").last().unwrap().clone();
    let t = call(&mut a, "space_tally", json!({ "page": 1 }));
    let row = |name: &str| {
        t["spaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["space"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("no row {name}: {t}"))
    };
    assert_eq!(row("Room A")["counts"]["Outlet"], 2, "{t}");
    assert_eq!(
        row("Room A > Closet")["counts"]["Outlet"],
        1,
        "nested spaces read Outer > Inner: {t}"
    );
    assert_eq!(row("Room B")["counts"]["Outlet"], 1, "{t}");
    assert_eq!(row("")["counts"]["Outlet"], 1, "{t}");
    // The rectangle belongs to Room B.
    let l = call(&mut a, "space_list", json!({ "page": 1 }));
    let b = l["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Room B")
        .unwrap()
        .clone();
    assert!(b["markups"].to_string().contains(rect_b_id.as_str()), "{l}");
    assert!((b["area"].as_f64().unwrap() - 220.0 * 300.0).abs() < 1.0, "{b}");
    // Edit: rename and reshape Room B to the right part only; delete the closet.
    call(
        &mut a,
        "space_edit",
        json!({ "id": room_b, "name": "Office", "points": rect_pts(420.0, 50.0, 550.0, 350.0) }),
    );
    call(&mut a, "space_delete", json!({ "ids": [closet] }));
    let t = call(&mut a, "space_tally", json!({ "page": 1 }));
    let get = |name: &str| {
        t["spaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["space"] == name)
            .cloned()
    };
    assert_eq!(
        get("Room A").unwrap()["counts"]["Outlet"],
        3,
        "closet point moves to Room A: {t}"
    );
    assert_eq!(
        get("").unwrap()["counts"]["Outlet"],
        2,
        "the point at 400 is now outside: {t}"
    );
    assert!(get("Room B").is_none() && get("Room A > Closet").is_none(), "{t}");
    call(&mut a, "edit_undo", json!({}));
    let l = call(&mut a, "space_list", json!({}));
    assert!(
        l.to_string().contains("Closet"),
        "undo did not bring the closet back: {l}"
    );
    // ui-066: split counts by space: a Count across two spaces and outside becomes three.
    let mut b = automation(&dir);
    open(&mut b, &new);
    space(&mut b, "Room A", rect_pts(50.0, 50.0, 300.0, 350.0));
    space(&mut b, "Room B", rect_pts(330.0, 50.0, 550.0, 350.0));
    call(
        &mut b,
        "markup_add",
        json!({ "page": 1, "kind": "Count", "subject": "Outlet",
                "points": [[200, 200], [250, 300], [400, 200], [580, 380]] }),
    );
    let r = call(&mut b, "space_split_counts", json!({}));
    assert_eq!(r["made"].as_array().unwrap().len(), 2, "{r}");
    let counts = call(&mut b, "markup_list", json!({ "kind": "Count" }));
    assert_eq!(counts["count"], 3, "{counts}");
    assert!(
        counts["markups"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["subject"] == "Outlet")
    );

    // The preference (Tools > Measure) splits a count as soon as it is placed.
    let mut h = app();
    {
        let st = &mut h.state_mut().state;
        st.shell.prefs.more.measure.split_counts_by_space = true;
        let d = st.doc_mut().unwrap();
        let sq = |x0: f64, x1: f64| {
            vec![
                Point::new(x0, 50.0),
                Point::new(x1, 50.0),
                Point::new(x1, 350.0),
                Point::new(x0, 350.0),
            ]
        };
        d.session.add_space(0, "Room A", sq(50.0, 300.0), None, None).unwrap();
        d.session.add_space(0, "Room B", sq(330.0, 550.0), None, None).unwrap();
        let m =
            markupcraft_model::Markup::new(Kind::Count, 0, vec![Point::new(200.0, 200.0), Point::new(400.0, 200.0)]);
        d.session.add_new_markups("Count", vec![m]).unwrap();
    }
    h.run_steps(3);
    let n = markups(&h).iter().filter(|m| m.kind == Kind::Count).count();
    assert_eq!(n, 2, "ui-066 one Count per space");
}

/// ui-062, ui-064, ui-065: the Spaces panel lists spaces; Highlight toggles the outlines; the
/// Markups List shows each markup's space.
#[test]
fn spaces_panel_and_space_column() {
    let dir = temp_dir("spaces-ui");
    let (_, new) = revisions(&dir);
    let mut a = automation(&dir);
    open(&mut a, &new);
    space(&mut a, "Kitchen", rect_pts(50.0, 50.0, 300.0, 350.0));
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[100, 100], [150, 150]], "subject": "Cabinet" }),
    );
    call(&mut a, "doc_save", json!({}));
    let mut h = app_with(std::fs::read(&new).unwrap(), Some(new.clone()));
    panel(&mut h, "spaces");
    assert!(shows(&h, "Kitchen"), "space not listed");
    let before = h.state().state.features.spaces.highlight;
    press(&mut h, "Highlight");
    assert_ne!(h.state().state.features.spaces.highlight, before, "Highlight toggle");
    // The Space column of the Markups List.
    let d = h.state().state.doc().unwrap();
    let m = d.session.doc().markups.iter().find(|m| m.subject == "Cabinet").unwrap();
    let col = markupcraft_model::spaces::space_path(d.session.doc(), m);
    assert_eq!(col, "Kitchen");
}

/// ui-067: a snapshot of a space's region.
#[test]
fn snapshot_from_a_space() {
    let dir = temp_dir("space-snap");
    let (_, new) = revisions(&dir);
    let mut a = automation(&dir);
    open(&mut a, &new);
    let id = space(&mut a, "Door bay", rect_pts(380.0, 130.0, 460.0, 180.0));
    call(&mut a, "snapshot_copy", json!({ "space": id }));
    let p = call(&mut a, "markup_paste", json!({ "page": 1, "at": [150, 200] }));
    let ids = p["ids"].as_array().unwrap();
    assert_eq!(ids.len(), 1, "{p}");
    let list = call(&mut a, "markup_list", json!({}));
    let snap = list["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == ids[0])
        .unwrap()
        .clone();
    let r: Vec<f64> = snap["rect"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert!(
        ((r[2] - r[0]) - 80.0).abs() < 2.0 && ((r[3] - r[1]) - 50.0).abs() < 2.0,
        "snapshot not the space's box: {snap}"
    );
    // The pasted copy shows the door (box centre at 150, 200; the door at 420, 154 in a box
    // centred on 420, 155).
    call(&mut a, "doc_save", json!({ "path": "snap.pdf" }));
    let c = pixel(&dir.join("snap.pdf"), 0, 150.0, 199.0);
    assert!(!is_white(c), "snapshot is empty: {c:?}");
}

/// ui-068, ui-069, ui-070: Places (named views) and hyperlinks: add, list, edit action, targets
/// (page, Place, URL, file), delete; the Links panel lists both.
#[test]
fn links_places_and_hyperlinks() {
    let dir = temp_dir("links");
    let new = write_pdf(&dir, "set.pdf", &[plan(0), plan(1), plan(1)]);
    let mut a = automation(&dir);
    open(&mut a, &new);
    call(
        &mut a,
        "place_set",
        json!({ "name": "Detail 5", "page": 3, "left": 380, "top": 200, "zoom": 2 }),
    );
    call(
        &mut a,
        "place_set",
        json!({ "name": "Stair", "page": 1, "left": 50, "top": 120 }),
    );
    let pl = call(&mut a, "place_list", json!({}));
    assert!(
        pl.to_string().contains("Detail 5") && pl.to_string().contains("Stair"),
        "{pl}"
    );
    let pl3 = call(&mut a, "place_list", json!({ "page": 3 }));
    assert!(
        pl3.to_string().contains("Detail 5") && !pl3.to_string().contains("Stair"),
        "per-page list: {pl3}"
    );
    // Links of each kind.
    call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [400, 150, 440, 160], "place": "Detail 5" }),
    );
    call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [90, 85, 130, 100], "to_page": 2 }),
    );
    call(
        &mut a,
        "link_add",
        json!({ "page": 2, "rect": [10, 10, 60, 30], "url": "https://example.com/spec" }),
    );
    call(
        &mut a,
        "link_add",
        json!({ "page": 2, "rect": [70, 10, 120, 30], "file": "other.pdf", "file_page": 4 }),
    );
    let ls = call(&mut a, "link_list", json!({}));
    let links = ls["links"].as_array().unwrap_or_else(|| panic!("{ls}")).clone();
    assert_eq!(links.len(), 4, "{ls}");
    let s = ls.to_string();
    for want in ["Detail 5", "https://example.com/spec", "other.pdf"] {
        assert!(s.contains(want), "{want} missing: {ls}");
    }
    // Edit Action: the Place link now goes to a URL.
    let id = links.iter().find(|l| l.to_string().contains("Detail 5")).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &mut a,
        "link_edit",
        json!({ "id": id, "url": "https://example.com/door" }),
    );
    let s = call(&mut a, "link_list", json!({})).to_string();
    assert!(s.contains("https://example.com/door"), "{s}");
    call(&mut a, "link_delete", json!({ "ids": [id] }));
    assert_eq!(
        call(&mut a, "link_list", json!({}))["links"].as_array().unwrap().len(),
        3
    );
    call(&mut a, "place_delete", json!({ "names": ["Stair"] }));
    assert!(!call(&mut a, "place_list", json!({})).to_string().contains("Stair"));
    call(&mut a, "doc_save", json!({}));

    // The Links panel lists the links and the Places, with a filter.
    let mut h = app_with(std::fs::read(&new).unwrap(), Some(new.clone()));
    panel(&mut h, "links");
    assert!(shows(&h, "Places") && shows(&h, "Detail 5"), "Places not listed");
    assert!(shows(&h, "p.1") && shows(&h, "p.2"), "links not listed by page");
    h.state_mut().state.features.links.place_filter = "zzz".into();
    h.run_steps(3);
    assert!(!shows(&h, "Detail 5"), "place filter does nothing");
    // ui-069: the hyperlinks list has a filter box.
    let all = h.query_all_by_label_contains("p.2  ").count();
    assert_eq!(all, 2, "two links on page 2");
    h.state_mut().state.features.links.filter = "example.com".into();
    h.run_steps(3);
    assert_eq!(
        h.query_all_by_label_contains("p.2  ").count(),
        1,
        "the filter keeps the web link only"
    );
    assert!(shows(&h, "1 of 3"), "the filter says how many it shows");
    h.state_mut().state.features.links.filter.clear();
    h.run_steps(3);
    // Multi-select: click one link, Ctrl+click another, give both one new action.
    h.query_all_by_label_contains("p.2  ").next().unwrap().click();
    h.run_steps(3);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    h.run_steps(1);
    h.query_all_by_label_contains("p.2  ").nth(1).unwrap().click();
    h.run_steps(3);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run_steps(2);
    assert!(shows(&h, "2 links"), "two links picked");
    press_last(&mut h, "Edit Action...");
    assert!(shows(&h, "several links"), "one action dialog for both");
    h.state_mut().state.features.links.kind = markupcraft_ui_egui::features::links::TargetKind::Page;
    h.state_mut().state.features.links.page = 3;
    h.run_steps(2);
    press_last(&mut h, "OK");
    let ls = h.state().state.doc().unwrap().session.links();
    let on2: Vec<_> = ls.iter().filter(|l| l.page == 1).collect();
    assert_eq!(on2.len(), 2);
    assert!(
        on2.iter()
            .all(|l| l.target == markupcraft_engine::links::LinkTarget::Page(2)),
        "both links go to page 3 now: {on2:?}"
    );
    // ... and Delete All removes the picked ones together.
    h.query_all_by_label_contains("p.2  ").next().unwrap().click();
    h.run_steps(3);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    h.run_steps(1);
    h.query_all_by_label_contains("p.2  ").nth(1).unwrap().click();
    h.run_steps(3);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run_steps(2);
    press_last(&mut h, "Delete All");
    assert_eq!(h.state().state.doc().unwrap().session.links().len(), 1);
}

/// ui-071, ui-072: sign (visibly, certifying), add an empty signature field, validate: the
/// Signatures panel lists each signature with its status; a change after signing is reported.
#[test]
fn signatures_sign_certify_field_and_validate() {
    let dir = temp_dir("sign");
    let doc = write_pdf(&dir, "plan.pdf", &[plan(0)]);
    let mut a = automation(&dir);
    call(
        &mut a,
        "digital_id_create",
        json!({ "name": "Test Signer", "organization": "Acceptance", "password": "pw-test", "out": "id.p12", "cert_out": "id.pem" }),
    );
    open(&mut a, &doc);
    call(
        &mut a,
        "form_add_field",
        json!({ "page": 1, "rect": [400, 60, 540, 100], "type": "signature", "name": "Approval" }),
    );
    call(&mut a, "doc_save", json!({}));
    let l = call(&mut a, "signature_list", json!({}));
    assert!(l.to_string().contains("Approval"), "empty field not listed: {l}");
    call(
        &mut a,
        "signature_sign",
        json!({ "id": "id.p12", "password": "pw-test", "page": 1, "rect": [60, 60, 200, 100], "reason": "Issued for review", "certify": 3 }),
    );
    let mine = |l: &Value| -> Vec<Value> {
        l["signatures"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["signer"].as_str().is_some_and(|x| x.contains("Test Signer")))
            .cloned()
            .collect()
    };
    let l = call(&mut a, "signature_list", json!({}));
    assert_eq!(mine(&l).len(), 1, "{l}");
    assert_eq!(
        mine(&l)[0]["status"],
        "unknown",
        "untrusted signer should be 'unknown': {l}"
    );
    assert!(l.to_string().contains("Issued for review"), "{l}");
    let trusted = call(&mut a, "signature_list", json!({ "trust": ["id.pem"] }));
    assert_eq!(
        mine(&trusted)[0]["status"],
        "valid",
        "trusted signature not valid: {trusted}"
    );
    // Sign the empty field too (form fill and signing are allowed by the certification).
    call(
        &mut a,
        "signature_sign",
        json!({ "id": "id.p12", "password": "pw-test", "field": "Approval" }),
    );
    let l = call(&mut a, "signature_list", json!({ "trust": ["id.pem"] }));
    assert_eq!(mine(&l).len(), 2, "{l}");
    // A change after signing is reported.
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[300, 300], [350, 340]] }),
    );
    call(&mut a, "doc_save", json!({}));
    let l = call(&mut a, "signature_list", json!({ "trust": ["id.pem"] }));
    let first = &mine(&l)[0];
    assert!(
        first.to_string().to_lowercase().contains("change"),
        "no change reported: {l}"
    );

    // The Signatures panel.
    let mut h = app_with(std::fs::read(&doc).unwrap(), Some(doc.clone()));
    panel(&mut h, "signatures");
    assert!(shows(&h, "Test Signer"), "signer not listed in the panel");
    assert!(shows(&h, "Validate") && shows(&h, "Sign..."), "panel toolbar");
    // Sign... offers Certify; its label says what the certification allows (it certifies with
    // form fill-in and signing allowed).
    press(&mut h, "Sign...");
    assert!(
        shows(&h, "form fill-in and signing allowed"),
        "certify option mislabelled"
    );
}

/// ui-016: review a compare result beside the original in split view, with the dimmer.
#[test]
fn compare_review_split_view_and_dimmer() {
    let dir = temp_dir("cmp-review");
    let (_, new) = revisions(&dir);
    let mut h = app_with(std::fs::read(&new).unwrap(), Some(new.clone()));
    run(&mut h, "tools.compare");
    assert!(shows(&h, "Compare"), "no Compare dialog");
    run(&mut h, "view.split_vertical");
    let sp = h.state().state.shell.split.as_ref().map(|s| s.vertical);
    assert_eq!(sp, Some(true), "no side-by-side split");
    let dim = h.state().state.shell.dimmer;
    run(&mut h, "view.dimmer");
    assert_ne!(h.state().state.shell.dimmer, dim, "Dimmer did not toggle");
}
