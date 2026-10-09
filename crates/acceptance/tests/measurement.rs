//! Measurement slice (docs/revu_features/01_measurement.md): scales, viewports, linear, area,
//! circular and count tools, Dynamic Fill, sketching, snapping, properties, Tool Chest, Markups
//! List columns, Spaces, legends and reports. Each test is written from the inventory text;
//! expected values are worked out by hand from the scale rules (72 pt = 1 paper inch).
use markupcraft_acceptance::*;

// ---------------------------------------------------------------- helpers local to this slice

/// 1/8" = 1'-0": 9 pt of paper is one foot.
fn eighth() -> Value {
    json!({ "kind": "architectural", "paper_inches": 0.125, "real_feet": 1 })
}

/// 1/4" = 1'-0": 18 pt of paper is one foot.
fn quarter() -> Value {
    json!({ "kind": "architectural", "paper_inches": 0.25, "real_feet": 1 })
}

/// A blank document of `pages` letter pages saved as `name` in `dir`, open in a fresh tool table.
fn blank(dir: &std::path::Path, pages: u32) -> Automation {
    let mut a = automation(dir);
    call(&mut a, "doc_new", json!({ "path": "blank.pdf", "pages": pages }));
    a
}

/// The sample plan (page 1 tabloid at 1/8" = 1'-0", page 2 a letter notes page), opened.
fn plan(dir: &std::path::Path) -> Automation {
    let mut a = automation(dir);
    let pdf = sample_pdf(dir, "plan.pdf");
    call(
        &mut a,
        "doc_open",
        json!({ "path": pdf.file_name().unwrap().to_str().unwrap() }),
    );
    a
}

/// Add a markup; returns the markup object.
fn add(a: &mut Automation, args: Value) -> Value {
    call(a, "markup_add", args)["markup"].clone()
}

fn qty(m: &Value) -> f64 {
    m["quantity"].as_f64().unwrap_or(f64::NAN)
}

fn text(m: &Value) -> String {
    m["quantity_text"].as_str().unwrap_or("").to_string()
}

fn id(m: &Value) -> String {
    m["id"].as_str().unwrap().to_string()
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// The markup with this id, as markup_list reports it now.
fn get(a: &mut Automation, id: &str) -> Value {
    let l = call(a, "markup_list", json!({}));
    l["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap_or(Value::Null)
}

/// Save to `name`, close, reopen it; returns the reopened markups.
fn save_reopen(a: &mut Automation, name: &str) -> Vec<Value> {
    call(a, "doc_save", json!({ "path": name, "full": true }));
    call(a, "doc_close", json!({}));
    call(a, "doc_open", json!({ "path": name }));
    call(a, "markup_list", json!({}))["markups"].as_array().unwrap().clone()
}

/// The real app, headless, with a PDF from disk open (as File > Open does).
fn app_with(path: &std::path::Path) -> Harness<'static, MarkupCraftApp> {
    markupcraft_ui_egui::richedit::load_dictionary_blocking();
    let bytes = std::fs::read(path).unwrap();
    let path = path.to_path_buf();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(move |_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            app.open_bytes("doc.pdf", Some(path.clone()), bytes.clone()).unwrap();
            app
        });
    h.run_steps(6);
    h
}

/// Open a dock panel by id and let it draw.
fn panel(h: &mut Harness<'_, MarkupCraftApp>, id: &'static str) {
    h.state_mut().state.show_panel(id);
    h.run_steps(6);
}

/// Click the widget labelled exactly `label` (scrolled into view first).
fn press(h: &mut Harness<'_, MarkupCraftApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(30);
    h.get_by_label(label).click();
    h.run_steps(4);
}

/// Open the combo box now showing `value` and pick `item` from it.
fn pick(h: &mut Harness<'_, MarkupCraftApp>, value: &str, item: &str) {
    use egui::accesskit::Role;
    let combo = |h: &Harness<'_, MarkupCraftApp>| {
        h.get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(value))
            .next()
            .is_some()
    };
    assert!(combo(h), "no combo box showing {value:?}");
    h.get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(value))
        .next()
        .unwrap()
        .scroll_to_me();
    h.run_steps(30);
    h.get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(value))
        .next()
        .unwrap()
        .click();
    h.run_steps(4);
    h.get_all_by_label(item)
        .last()
        .unwrap_or_else(|| panic!("no item {item:?}"))
        .click();
    h.run_steps(4);
}

fn page_scale(a: &mut Automation, page: usize) -> Value {
    let info = call(a, "doc_info", json!({}));
    info["page_list"][page - 1]["viewports"].clone()
}

// ---------------------------------------------------------------- Scale and calibration

/// M-001: a line over a known dimension, its real length typed, gives the page scale.
#[test]
fn calibrate_derives_the_page_scale_from_a_known_length() {
    let dir = temp_dir("m-cal");
    let mut a = blank(&dir, 2);
    // 72 pt on paper is 4 ft: 1/4" = 1'-0".
    call(
        &mut a,
        "scale_calibrate",
        json!({ "page": 1, "from": [100, 100], "to": [172, 100], "length": 4, "unit": "ft" }),
    );
    let m = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[100, 200], [244, 200]] }),
    );
    assert!(near(qty(&m), 8.0, 1e-6), "{m}");
    assert_eq!(text(&m), "8'-0\"");
    // Only the page the points are on (default) got the scale.
    assert!(page_scale(&mut a, 2).as_array().unwrap().is_empty());
    // Metric calibration gives a decimal metric scale: 72 pt = 2 m.
    call(
        &mut a,
        "scale_calibrate",
        json!({ "page": 2, "from": [0, 0], "to": [0, 72], "length": 2, "unit": "m" }),
    );
    let m = add(
        &mut a,
        json!({ "page": 2, "kind": "Length", "points": [[100, 100], [100, 136]] }),
    );
    assert!(near(qty(&m), 1.0, 1e-6), "{m}");
    assert_eq!(m["unit"], "m");

    // The UI tool: two clicks open the Calibrate dialog asking for the distance.
    let mut h = app();
    run(&mut h, "tool.calibrate");
    click(&mut h, 200.0, 300.0);
    click(&mut h, 380.0, 300.0);
    assert!(shows(&h, "Calibrate"), "the calibrate dialog opens after two points");
}

/// M-002: standard architectural, engineering and metric scales without calibrating.
#[test]
fn preset_scales_measure_by_hand_worked_values() {
    let dir = temp_dir("m-preset");
    let mut a = blank(&dir, 3);
    call(&mut a, "scale_set", json!({ "pages": [1], "scale": eighth() }));
    call(
        &mut a,
        "scale_set",
        json!({ "pages": [2], "scale": { "kind": "engineering", "feet_per_inch": 20 } }),
    );
    call(
        &mut a,
        "scale_set",
        json!({ "pages": [3], "scale": { "kind": "ratio", "ratio": 100, "unit": "m" } }),
    );
    // 1/8" = 1'-0": 90 pt = 1.25 in = 10 ft.
    let m = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[100, 100], [190, 100]] }),
    );
    assert!(near(qty(&m), 10.0, 1e-6), "{m}");
    assert_eq!(text(&m), "10'-0\"");
    // 1" = 20': 36 pt = 0.5 in = 10 ft, decimal feet.
    let m = add(
        &mut a,
        json!({ "page": 2, "kind": "Length", "points": [[100, 100], [136, 100]] }),
    );
    assert!(near(qty(&m), 10.0, 1e-6), "{m}");
    assert_eq!(m["unit"], "ft");
    // 1:100: 72 pt = 25.4 mm on paper = 2.54 m.
    let m = add(
        &mut a,
        json!({ "page": 3, "kind": "Length", "points": [[100, 100], [172, 100]] }),
    );
    assert!(near(qty(&m), 2.54, 1e-6), "{m}");
    assert_eq!(m["unit"], "m");
}

/// M-003: a custom scale with mixed systems: 1 mm on paper = 1 ft in the field.
#[test]
fn custom_scale_mixes_unit_systems() {
    let dir = temp_dir("m-custom");
    let mut a = blank(&dir, 1);
    // 1 pt = 25.4 / 72 mm; at 1 mm = 1 ft that is 0.352777... ft per point.
    let ft_per_pt = 25.4 / 72.0;
    let sc = json!({ "kind": "custom", "scale": {
        "ratio": "1 mm = 1 ft",
        "x": [{ "unit": "ft", "conv": ft_per_pt, "fmt": "Decimal", "den": 100, "no_reduce": false,
                "thousands": ",", "decimal": ".", "prefix": " ", "suffix": "", "label_first": false }],
        "y": [],
        "dist": [{ "unit": "ft", "conv": 1.0, "fmt": "Decimal", "den": 100, "no_reduce": false,
                   "thousands": ",", "decimal": ".", "prefix": " ", "suffix": "", "label_first": false }],
        "area": [{ "unit": "sf", "conv": 1.0, "fmt": "Decimal", "den": 100, "no_reduce": false,
                   "thousands": ",", "decimal": ".", "prefix": " ", "suffix": "", "label_first": false }],
        "volume": [], "target_unit_conversion": null } });
    call(&mut a, "scale_set", json!({ "scale": sc }));
    // 72 pt = 25.4 mm = 25.4 ft.
    let m = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[100, 100], [172, 100]] }),
    );
    assert!(near(qty(&m), 25.4, 1e-6), "{m}");
    assert_eq!(text(&m), "25.4 ft");
}

/// M-005 / M-007: a scale goes to a page range like "1-3, 5"; every page keeps its own.
#[test]
fn scale_applies_to_a_page_range_and_each_page_keeps_its_own() {
    let dir = temp_dir("m-range");
    let mut a = blank(&dir, 6);
    call(&mut a, "scale_set", json!({ "pages": "1-3, 5", "scale": eighth() }));
    for p in 1..=6 {
        let has = !page_scale(&mut a, p).as_array().unwrap().is_empty();
        assert_eq!(has, [1, 2, 3, 5].contains(&p), "page {p}");
    }
    call(&mut a, "scale_set", json!({ "pages": [2], "scale": quarter() }));
    let one = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[0, 0], [72, 0]] }),
    );
    let two = add(
        &mut a,
        json!({ "page": 2, "kind": "Length", "points": [[0, 0], [72, 0]] }),
    );
    assert!(near(qty(&one), 8.0, 1e-6), "{one}");
    assert!(near(qty(&two), 4.0, 1e-6), "{two}");
    // Per-page scales survive save and reopen.
    let ms = save_reopen(&mut a, "range.pdf");
    let q: Vec<f64> = ms.iter().map(qty).collect();
    assert!(
        q.iter().any(|v| near(*v, 8.0, 1e-6)) && q.iter().any(|v| near(*v, 4.0, 1e-6)),
        "{q:?}"
    );
    let m = add(
        &mut a,
        json!({ "page": 5, "kind": "Length", "points": [[0, 0], [72, 0]] }),
    );
    assert!(near(qty(&m), 8.0, 1e-6), "{m}");
}

/// M-006 / M-012: copy a scale to more pages later, then Recalculate pushes it onto
/// existing measurements (their units and precision stay).
#[test]
fn add_scale_to_more_pages_and_recalculate() {
    let dir = temp_dir("m-recalc");
    let mut a = blank(&dir, 2);
    call(&mut a, "scale_set", json!({ "pages": [1, 2], "scale": eighth() }));
    let m = add(
        &mut a,
        json!({ "page": 2, "kind": "Length", "points": [[0, 0], [72, 0]] }),
    );
    assert!(near(qty(&m), 8.0, 1e-6));
    // Change page 2's scale without touching the markup: its value stays.
    call(&mut a, "scale_set", json!({ "pages": [2], "scale": quarter() }));
    assert!(near(qty(&get(&mut a, &id(&m))), 8.0, 1e-6));
    // Recalculate: the measurement takes the scale in effect now.
    call(&mut a, "measure_recalculate", json!({ "pages": [2] }));
    let after = get(&mut a, &id(&m));
    assert!(near(qty(&after), 4.0, 1e-6), "{after}");
    assert_eq!(text(&after), "4'-0\"");

    // The UI: Measurements panel > Apply to (a page range) copies a preset to more pages.
    let mut h = app();
    panel(&mut h, "measurements");
    h.state_mut().state.measure.pages = "1-2".into();
    pick(&mut h, "Choose...", "1/4\" = 1'-0\"");
    let d = h.state().state.doc().unwrap().session.doc().clone();
    for p in 0..2 {
        let sc = d.pages[p].scale.clone().expect("scale on both pages");
        let v = sc.length_of(&[Point::new(0.0, 0.0), Point::new(72.0, 0.0)], false);
        assert!(near(v, 4.0, 1e-6), "page {p}: {v}");
    }
    assert!(markupcraft_ui_egui::commands::find("measure.recalculate").is_some());
}

/// M-003 in the UI: Measurements panel > Custom, 1 mm = 1 ft (mixed systems).
#[test]
fn custom_scale_in_the_measurements_panel() {
    use markupcraft_measure::units::LengthUnit;
    let mut h = app();
    panel(&mut h, "measurements");
    {
        let st = &mut h.state_mut().state.measure;
        st.paper = "1".into();
        st.paper_unit = LengthUnit::Millimeter;
        st.real = "1".into();
        st.real_unit = LengthUnit::Foot;
        st.pages = String::new();
    }
    press(&mut h, "Apply custom scale");
    let sc = h.state().state.doc().unwrap().session.doc().pages[0]
        .scale
        .clone()
        .unwrap();
    let v = sc.length_of(&[Point::new(0.0, 0.0), Point::new(72.0, 0.0)], false);
    assert!(near(v, 25.4, 1e-6), "72 pt = 25.4 mm = 25.4 ft, got {v}");
}

/// M-004: save the page scale as a user preset; it lists first and only it can be deleted.
#[test]
fn user_scale_presets_list_first_and_can_be_deleted() {
    let mut h = app();
    panel(&mut h, "measurements");
    // The sample's page 1 has a scale (1/8" = 1'-0"): the panel must not call it unset.
    assert!(
        !shows(&h, "Not set"),
        "the panel sees the page scale the status bar shows"
    );
    h.state_mut().state.measure.preset_name = "Site detail".into();
    press(&mut h, "+ Add Preset");
    let names: Vec<String> = h
        .state()
        .state
        .toolchest
        .extras
        .scale_presets
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(names, ["Site detail"]);
    // Listed first, with the only delete button (built-ins have none).
    pick(&mut h, "Choose...", "Delete preset");
    assert!(h.state().state.toolchest.extras.scale_presets.is_empty());
}

/// M-008: a separate Y scale for stretched drawings: vertical runs use it.
#[test]
fn separate_y_scale_measures_vertical_runs_differently() {
    let dir = temp_dir("m-yscale");
    let mut a = blank(&dir, 1);
    call(
        &mut a,
        "viewport_add",
        json!({ "page": 1, "box": [0, 0, 612, 792], "name": "Profile", "scale": eighth() }),
    );
    // X: 72 pt = 4 ft; Y: 1/4" = 1'-0" (72 pt = 4 ft) vs X at 1/8" (72 pt = 8 ft).
    call(
        &mut a,
        "viewport_calibrate",
        json!({ "page": 1, "index": 1, "from": [0, 0], "to": [72, 0], "length": 8, "unit": "ft", "y_scale": quarter() }),
    );
    let h = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[100, 100], [172, 100]] }),
    );
    let v = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[100, 100], [100, 172]] }),
    );
    assert!(near(qty(&h), 8.0, 1e-6), "{h}");
    assert!(near(qty(&v), 4.0, 1e-6), "{v}");
}

/// M-009: precision per scale, as decimals or as a fraction of an inch.
#[test]
fn precision_rounds_to_the_chosen_fraction_or_decimals() {
    let dir = temp_dir("m-prec");
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    // 10.1 pt at 1/8" = 1'-0" is 1.12222 ft = 1'-1.4667": to 1/4" that is 1'-1 1/2".
    let m = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[0, 0], [10.1, 0]] }),
    );
    assert_eq!(text(&m), "1'-1 1/2\"", "{m}");
    // Engineering: 2 decimal places by default.
    call(
        &mut a,
        "scale_set",
        json!({ "scale": { "kind": "engineering", "feet_per_inch": 10 } }),
    );
    let m = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[0, 0], [10, 0]] }),
    );
    // 10 pt = 0.13889 in = 1.3889 ft.
    assert_eq!(text(&m), "1.39 ft", "{m}");
}

/// M-010 / M-013: the status bar shows the page scale, and "Scale Not Set" on a page without one.
#[test]
fn status_bar_shows_the_scale_or_that_none_is_set() {
    let mut h = app();
    // Page 1 of the sample is at 1/8" = 1'-0".
    assert!(shows(&h, "1/8"), "status bar shows the page scale");
    run(&mut h, "view.next_page");
    h.run_steps(4);
    assert!(shows(&h, "Scale Not Set"), "page 2 has no scale");

    // A measurement on a page with no scale is not given a bogus value.
    let dir = temp_dir("m-noscale");
    let mut a = blank(&dir, 1);
    let m = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[0, 0], [72, 0]] }),
    );
    assert!(m["quantity"].is_null(), "{m}");
    let ms = save_reopen(&mut a, "noscale.pdf");
    assert!(ms[0]["quantity"].is_null(), "save does not invent a scale: {}", ms[0]);
    assert!(page_scale(&mut a, 1).as_array().unwrap().is_empty());
}

/// M-011: a temporary scale is used now but the file keeps its own; reopening brings it back.
#[test]
fn temporary_scale_reverts_when_the_document_is_reopened() {
    let dir = temp_dir("m-temp");
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    call(&mut a, "doc_save", json!({ "path": "temp.pdf", "full": true }));
    call(&mut a, "scale_set_temporary", json!({ "scale": quarter() }));
    let m = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[0, 0], [72, 0]] }),
    );
    assert!(near(qty(&m), 4.0, 1e-6), "{m}");
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_close", json!({}));
    call(&mut a, "doc_open", json!({ "path": "temp.pdf" }));
    let m = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[0, 0], [72, 0]] }),
    );
    assert!(near(qty(&m), 8.0, 1e-6), "the file's own scale is back: {m}");
}

// ---------------------------------------------------------------- Viewports

/// M-014..M-019: a viewport gives a region its own scale, has a label, takes a preset or
/// calibrated scale, can be highlighted, deleted, cleared, and copied to other pages.
#[test]
fn viewports_give_regions_their_own_scale() {
    let dir = temp_dir("m-vp");
    let mut a = blank(&dir, 3);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    call(
        &mut a,
        "viewport_add",
        json!({ "page": 1, "box": [300, 300, 600, 600], "name": "Detail 3", "scale": quarter() }),
    );
    // Inside the viewport 72 pt = 4 ft; outside the page scale gives 8 ft.
    let inside = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[350, 350], [422, 350]] }),
    );
    let outside = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[50, 50], [122, 50]] }),
    );
    assert!(near(qty(&inside), 4.0, 1e-6), "{inside}");
    assert!(near(qty(&outside), 8.0, 1e-6), "{outside}");
    // M-015: the label is listed.
    let vps = page_scale(&mut a, 1);
    assert!(vps.to_string().contains("Detail 3"), "{vps}");
    let idx = vps
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "Detail 3")
        .unwrap()["index"]
        .as_u64()
        .unwrap();
    call(
        &mut a,
        "viewport_edit",
        json!({ "page": 1, "index": idx, "rename": "Detail 4" }),
    );
    assert!(page_scale(&mut a, 1).to_string().contains("Detail 4"));
    // M-016: rescale by calibration: 72 pt = 2 ft; the measurement inside follows.
    call(
        &mut a,
        "viewport_calibrate",
        json!({ "page": 1, "index": idx, "from": [350, 350], "to": [422, 350], "length": 2, "unit": "ft" }),
    );
    assert!(near(qty(&get(&mut a, &id(&inside))), 2.0, 1e-6));
    assert!(near(qty(&get(&mut a, &id(&outside))), 8.0, 1e-6));
    // M-019: copy to other pages at the same place.
    call(
        &mut a,
        "viewport_edit",
        json!({ "page": 1, "index": idx, "copy_to": [2, 3] }),
    );
    let on2 = add(
        &mut a,
        json!({ "page": 2, "kind": "Length", "points": [[350, 350], [422, 350]] }),
    );
    assert!(near(qty(&on2), 2.0, 1e-6), "{on2}");
    // M-018: delete one, clear all on a page.
    let idx3 = page_scale(&mut a, 3)
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "Detail 4")
        .unwrap()["index"]
        .as_u64()
        .unwrap();
    call(&mut a, "viewport_delete", json!({ "page": 3, "index": idx3 }));
    assert!(!page_scale(&mut a, 3).to_string().contains("Detail 4"));
    call(&mut a, "viewport_edit", json!({ "page": 2, "clear": true }));
    let p2 = page_scale(&mut a, 2);
    assert!(!p2.to_string().contains("Detail 4"), "{p2}");
    assert_eq!(p2.as_array().unwrap().len(), 1, "the page scale stays: {p2}");
    // Viewports survive save.
    save_reopen(&mut a, "vp.pdf");
    assert!(page_scale(&mut a, 1).to_string().contains("Detail 4"));

    // M-014 / M-017 in the UI: the Viewport tool and Highlight Viewports.
    let mut h = app();
    assert!(markupcraft_ui_egui::tools::find("viewport").is_some());
    run(&mut h, "view.highlight_viewports");
}

// ---------------------------------------------------------------- UI gesture helpers

const SHIFT_ALT: egui::Modifiers = egui::Modifiers {
    alt: true,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: false,
};

/// Markups added since `before` markups existed.
fn added(h: &Harness<'_, MarkupCraftApp>, before: usize) -> Vec<markupcraft_model::Markup> {
    markups(h).into_iter().skip(before).collect()
}

/// Double-click at a page point.
fn double_click(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.step();
    for _ in 0..2 {
        button(h, at, true, egui::Modifiers::NONE);
        button(h, at, false, egui::Modifiers::NONE);
    }
    h.run_steps(3);
}

/// Hold (or release, with NONE) modifier keys: the state egui reads as `i.modifiers`.
fn hold(h: &mut Harness<'_, MarkupCraftApp>, m: egui::Modifiers) {
    h.event(egui::Event::ModifiersChanged(m));
    h.step();
}

/// Ctrl as Windows reports it (ctrl and command both set).
const CTRL: egui::Modifiers = egui::Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};

/// Click at a page point with modifiers held.
fn click_with(h: &mut Harness<'_, MarkupCraftApp>, m: egui::Modifiers, x: f64, y: f64) {
    let at = screen(h, x, y);
    h.hover_at(at);
    hold(h, m);
    button(h, at, true, m);
    button(h, at, false, m);
    hold(h, egui::Modifiers::NONE);
    h.run_steps(3);
}

/// Drag between page points with modifiers held.
fn drag_with(h: &mut Harness<'_, MarkupCraftApp>, m: egui::Modifiers, from: (f64, f64), to: (f64, f64)) {
    let (a, b) = (screen(h, from.0, from.1), screen(h, to.0, to.1));
    h.hover_at(a);
    hold(h, m);
    button(h, a, true, m);
    for i in 1..=8 {
        h.hover_at(a + (b - a) * (i as f32 / 8.0));
        h.step();
    }
    button(h, b, false, m);
    hold(h, egui::Modifiers::NONE);
    h.run_steps(3);
}

/// A shortcut with modifiers, the way the keyboard sends it.
fn shortcut(h: &mut Harness<'_, MarkupCraftApp>, m: egui::Modifiers, k: egui::Key) {
    h.key_press_modifiers(m, k);
    h.run_steps(4);
}

/// Select markups by id in the open document.
fn select(h: &mut Harness<'_, MarkupCraftApp>, ids: &[String]) {
    h.state_mut().state.doc_mut().unwrap().session.select(ids).unwrap();
    h.run_steps(3);
}

fn find(h: &Harness<'_, MarkupCraftApp>, id: &str) -> markupcraft_model::Markup {
    h.state().state.doc().unwrap().session.doc().find(id).unwrap().clone()
}

const SAMPLE_AREA: &str = "SAMPLEAREAAAAAAA";

// ---------------------------------------------------------------- Linear tools

/// M-020 / M-094: Shift+Alt+L, two clicks: a straight Length at the page scale
/// (sample page: 1/8" = 1'-0", so 90 pt = 10 ft), with a live readout while placing.
#[test]
fn length_tool_two_clicks() {
    let mut h = app();
    let before = markups(&h).len();
    key(&mut h, SHIFT_ALT, egui::Key::L);
    assert_eq!(h.state().state.tool, "length");
    click(&mut h, 150.0, 400.0);
    let at = screen(&h, 240.0, 400.0);
    h.hover_at(at);
    h.run_steps(3);
    let live = h.state().state.status.clone();
    assert!(live.starts_with("Length: 10"), "live readout while placing: {live:?}");
    click(&mut h, 240.0, 400.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1, "{live}");
    assert_eq!(new[0].kind, Kind::Length);
    assert!(near(new[0].quantity().unwrap(), 10.0, 0.01), "{:?}", new[0].quantity());
}

/// M-021 / M-023 / M-024: Polylength (Shift+Alt+Q) sums its segments; Backspace removes the
/// last point; Enter finishes.
#[test]
fn polylength_backspace_and_enter() {
    let mut h = app();
    let before = markups(&h).len();
    key(&mut h, SHIFT_ALT, egui::Key::Q);
    assert_eq!(h.state().state.tool, "polylength");
    click(&mut h, 150.0, 400.0);
    click(&mut h, 240.0, 400.0);
    click(&mut h, 240.0, 490.0);
    click(&mut h, 150.0, 600.0);
    key(&mut h, egui::Modifiers::NONE, egui::Key::Backspace);
    key(&mut h, egui::Modifiers::NONE, egui::Key::Enter);
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].kind, Kind::Polylength);
    assert_eq!(new[0].pts.len(), 3, "Backspace removed the last vertex");
    assert!(near(new[0].quantity().unwrap(), 20.0, 0.01), "{:?}", new[0].quantity());
}

/// M-022 / M-024: Perimeter (Shift+Alt+P), closed by clicking the first point.
#[test]
fn perimeter_closes_on_the_first_point() {
    let mut h = app();
    let before = markups(&h).len();
    key(&mut h, SHIFT_ALT, egui::Key::P);
    assert_eq!(h.state().state.tool, "perimeter");
    for (x, y) in [
        (150.0, 400.0),
        (240.0, 400.0),
        (240.0, 490.0),
        (150.0, 490.0),
        (150.0, 400.0),
    ] {
        click(&mut h, x, y);
    }
    let new = added(&h, before);
    assert_eq!(new.len(), 1, "clicking the first point finishes");
    assert_eq!(new[0].kind, Kind::Perimeter);
    assert!(
        near(new[0].quantity().unwrap(), 40.0, 0.01),
        "closed: {:?}",
        new[0].quantity()
    );
}

/// M-025: Rise/Drop adds a vertical run to a Polylength, and saves.
#[test]
fn polylength_rise_drop_adds_and_saves() {
    let dir = temp_dir("m-rise");
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    let m = add(
        &mut a,
        json!({ "page": 1, "kind": "Polylength", "points": [[0, 0], [90, 0], [90, 90]], "rise_drop": 12 }),
    );
    assert!(near(qty(&m), 32.0, 1e-6), "20 ft run + 12 ft rise: {m}");
    let ms = save_reopen(&mut a, "rise.pdf");
    assert!(near(qty(&ms[0]), 32.0, 1e-6), "{}", ms[0]);
}

/// M-026: slope as pitch, degrees or grade gives the true sloped length/area.
#[test]
fn slope_gives_true_sloped_length_and_area() {
    let dir = temp_dir("m-slope");
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    let l = add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[0, 0], [90, 0]] }),
    );
    let ar = add(
        &mut a,
        json!({ "page": 1, "kind": "Area", "points": [[100, 100], [190, 100], [190, 190], [100, 190]] }),
    );
    // 12 in 12 pitch = 45 degrees: x sqrt(2).
    call(
        &mut a,
        "measure_props_set",
        json!({ "ids": [id(&l)], "slope_type": "pitch", "slope": 12 }),
    );
    assert!(near(qty(&get(&mut a, &id(&l))), 10.0 * 2f64.sqrt(), 1e-3));
    call(
        &mut a,
        "measure_props_set",
        json!({ "ids": [id(&ar)], "slope_type": "degrees", "slope": 60 }),
    );
    assert!(near(qty(&get(&mut a, &id(&ar))), 200.0, 1e-3), "100 sf / cos 60");
    // 100 % grade = 45 degrees.
    call(
        &mut a,
        "measure_props_set",
        json!({ "ids": [id(&l)], "slope_type": "grade", "slope": 100 }),
    );
    assert!(near(qty(&get(&mut a, &id(&l))), 10.0 * 2f64.sqrt(), 1e-3));
    let ms = save_reopen(&mut a, "slope.pdf");
    let l2 = ms.iter().find(|m| m["kind"] == "Length").unwrap();
    assert!(near(qty(l2), 10.0 * 2f64.sqrt(), 1e-3), "slope saves: {l2}");
}

/// M-027 / M-028 / M-029: Show Segment Values from Properties; convert a segment to an arc
/// (the value follows) and back; add and delete control points.
#[test]
fn segment_values_arcs_and_control_points() {
    let dir = temp_dir("m-seg");
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    let m = add(
        &mut a,
        json!({ "page": 1, "kind": "Polylength", "points": [[100, 100], [190, 100], [190, 190]] }),
    );
    let mid = id(&m);
    // Arc: the 90 pt segment bent through a point 45 pt off becomes a half circle: pi * 5 ft.
    call(&mut a, "arc_edit", json!({ "id": mid, "convert": 1 }));
    call(&mut a, "arc_edit", json!({ "id": mid, "bend": 1, "toward": [145, 55] }));
    let v = qty(&get(&mut a, &mid));
    assert!(near(v, 10.0 + std::f64::consts::PI * 5.0, 0.05), "{v}");
    call(&mut a, "arc_edit", json!({ "id": mid, "straighten": 1 }));
    assert!(near(qty(&get(&mut a, &mid)), 20.0, 1e-6));
    // Control points.
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": [mid], "insert_vertex": { "index": 3, "point": [190, 280] } }),
    );
    let after = get(&mut a, &mid);
    assert_eq!(after["points"].as_array().unwrap().len(), 4, "{after}");
    call(&mut a, "markup_transform", json!({ "ids": [mid], "delete_vertex": 4 }));
    assert_eq!(get(&mut a, &mid)["points"].as_array().unwrap().len(), 3);

    // Segment values: Properties > Show Segment Values.
    let mut h = app();
    let before = markups(&h).len();
    key(&mut h, SHIFT_ALT, egui::Key::Q);
    click(&mut h, 150.0, 400.0);
    click(&mut h, 240.0, 400.0);
    double_click(&mut h, 240.0, 490.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    assert!(near(new[0].quantity().unwrap(), 20.0, 0.01), "double-click finishes");
    select(&mut h, &[new[0].id.clone()]);
    panel(&mut h, "properties");
    press(&mut h, "Show Segment Values");
    assert!(find(&h, &new[0].id).segment_values);
}

// ---------------------------------------------------------------- Area and volume

/// M-030 / M-024: Area (Shift+Alt+A) by clicks, finished with a double-click.
#[test]
fn area_tool_clicks_and_double_click() {
    let mut h = app();
    let before = markups(&h).len();
    key(&mut h, SHIFT_ALT, egui::Key::A);
    assert_eq!(h.state().state.tool, "area");
    click(&mut h, 150.0, 400.0);
    click(&mut h, 240.0, 400.0);
    click(&mut h, 240.0, 490.0);
    double_click(&mut h, 150.0, 490.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].kind, Kind::Area);
    assert!(near(new[0].quantity().unwrap(), 100.0, 0.01), "{:?}", new[0].quantity());
}

/// M-031: drag a rectangle area in one gesture.
#[test]
fn area_by_drag_rectangle() {
    let mut h = app();
    let before = markups(&h).len();
    run(&mut h, "tool.area_rect");
    drag(&mut h, (150.0, 400.0), (240.0, 490.0));
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].kind, Kind::Area);
    assert!(near(new[0].quantity().unwrap(), 100.0, 0.01), "{:?}", new[0].quantity());
}

/// M-032 / M-033 / M-034 / M-035: Volume = area x depth; Depth enables wall area
/// (perimeter x depth); Show All Measurements lists P / A / WA / V.
#[test]
fn volume_depth_wall_area_and_all_measurements() {
    let dir = temp_dir("m-vol");
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    let sq = json!([[100, 100], [190, 100], [190, 190], [100, 190]]);
    let v = add(&mut a, json!({ "page": 1, "kind": "Volume", "points": sq, "depth": 8 }));
    assert!(near(qty(&v), 800.0, 1e-6), "100 sf x 8 ft: {v}");
    add(
        &mut a,
        json!({ "page": 1, "kind": "Perimeter", "points": sq, "depth": 8 }),
    );
    // The Markups List's Wall Area and Depth columns: 40 ft x 8 ft = 320 sf.
    call(
        &mut a,
        "summary_export",
        json!({ "out": "wa.csv", "columns": ["type", "depth", "wallarea"] }),
    );
    let csv = std::fs::read_to_string(dir.join("wa.csv")).unwrap();
    assert!(
        csv.contains("Wall Area") && csv.contains("320"),
        "wall area 320 sf: {csv}"
    );
    // Show All Measurements: caption {all} shows P, A, WA and V.
    call(
        &mut a,
        "measure_props_set",
        json!({ "ids": [id(&v)], "caption": "{all}" }),
    );
    let ms = save_reopen(&mut a, "vol.pdf");
    let vv = ms.iter().find(|m| m["kind"] == "Volume").unwrap();
    assert!(near(qty(vv), 800.0, 1e-6), "{vv}");
    // What the markup shows, opened in the app from the saved file.
    let h = app_with(&dir.join("vol.pdf"));
    let m = markups(&h).into_iter().find(|m| m.kind == Kind::Volume).unwrap();
    let c = markupcraft_model::caption::caption_text(&m);
    for want in ["40", "100", "320", "800"] {
        assert!(c.contains(want), "{want} in {c:?}");
    }
}

/// M-036: Show Centroid from Properties.
#[test]
fn show_centroid_from_properties() {
    let dir = temp_dir("m-centroid");
    let mut a = plan(&dir);
    call(
        &mut a,
        "measure_props_set",
        json!({ "ids": [SAMPLE_AREA], "show_centroid": true }),
    );
    let before = std::fs::metadata(dir.join("plan.pdf")).unwrap().len();
    call(&mut a, "doc_save", json!({ "path": "c.pdf", "full": true }));
    assert!(std::fs::metadata(dir.join("c.pdf")).unwrap().len() > 0 && before > 0);
    let mut h = app();
    select(&mut h, &[SAMPLE_AREA.to_string()]);
    panel(&mut h, "properties");
    assert!(!find(&h, SAMPLE_AREA).show_centroid);
    press(&mut h, "Show Centroid");
    assert!(find(&h, SAMPLE_AREA).show_centroid);
}

/// M-037 / M-038 / M-039 / M-040: polygon and ellipse cutouts subtract; overlapping
/// cutouts merge; delete one; turn a cutout into its own measurement.
#[test]
fn cutouts_subtract_merge_delete_and_become_measurements() {
    let dir = temp_dir("m-cut");
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    // 180 x 180 pt = 20 x 20 ft = 400 sf.
    let ar = add(
        &mut a,
        json!({ "page": 1, "kind": "Area", "points": [[100, 100], [280, 100], [280, 280], [100, 280]] }),
    );
    let aid = id(&ar);
    // A 45 x 45 pt hole = 5 x 5 ft = 25 sf.
    call(
        &mut a,
        "cutout_add",
        json!({ "id": aid, "points": [[120, 120], [165, 120], [165, 165], [120, 165]] }),
    );
    assert!(near(qty(&get(&mut a, &aid)), 375.0, 1e-6));
    // A second hole overlapping the first by half: the union (37.5 sf) is deducted once.
    call(
        &mut a,
        "cutout_add",
        json!({ "id": aid, "points": [[142.5, 120], [187.5, 120], [187.5, 165], [142.5, 165]] }),
    );
    let v = qty(&get(&mut a, &aid));
    assert!(near(v, 400.0 - 37.5, 1e-6), "overlapping cutouts merge, got {v}");
    // Delete the cutout(s) back to the full area.
    while !get(&mut a, &aid)["holes"].as_array().unwrap().is_empty() {
        call(&mut a, "cutout_delete", json!({ "id": aid, "index": 1 }));
    }
    assert!(near(qty(&get(&mut a, &aid)), 400.0, 1e-6));
    // Cutout to its own measurement.
    call(
        &mut a,
        "cutout_add",
        json!({ "id": aid, "points": [[200, 200], [245, 200], [245, 245], [200, 245]] }),
    );
    let before = call(&mut a, "markup_list", json!({}))["count"].as_u64().unwrap();
    call(&mut a, "cutout_to_measurement", json!({ "id": aid, "index": 1 }));
    let l = call(&mut a, "markup_list", json!({}));
    assert_eq!(l["count"].as_u64().unwrap(), before + 1);
    let newest = l["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] != aid)
        .unwrap();
    assert!(near(qty(newest), 25.0, 1e-6), "{newest}");

    // M-038: Ellipse Cutout in the UI on the sample area.
    let mut h = app();
    let full = find(&h, SAMPLE_AREA).quantity().unwrap();
    run(&mut h, "tool.ellipse_cutout");
    drag(&mut h, (400.0, 230.0), (490.0, 320.0));
    let m = find(&h, SAMPLE_AREA);
    assert_eq!(m.holes.len(), 1, "the ellipse became a cutout");
    // A 90 pt circle = 10 ft diameter = 78.54 sf.
    assert!(
        near(full - m.quantity().unwrap(), 78.54, 0.5),
        "{full} -> {:?}",
        m.quantity()
    );
}

/// M-041: hatch fill on an area, with a scale (spacing), saved.
#[test]
fn hatch_fill_on_an_area_saves() {
    let dir = temp_dir("m-hatch");
    let mut a = plan(&dir);
    call(
        &mut a,
        "markup_hatch",
        json!({ "ids": [SAMPLE_AREA], "style": "Cross", "spacing": 12 }),
    );
    let ms = save_reopen(&mut a, "hatch.pdf");
    let m = ms.iter().find(|m| m["id"] == SAMPLE_AREA).unwrap();
    assert!(!m["hatch"].is_null(), "{m}");
}

/// M-042: rotate a measurement by a typed angle: the area value is unchanged.
#[test]
fn rotate_a_measurement_keeps_its_value() {
    let dir = temp_dir("m-rot");
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    let ar = add(
        &mut a,
        json!({ "page": 1, "kind": "Area", "points": [[100, 100], [190, 100], [190, 190], [100, 190]] }),
    );
    call(&mut a, "markup_transform", json!({ "ids": [id(&ar)], "rotate": 30 }));
    let m = get(&mut a, &id(&ar));
    assert!(near(qty(&m), 100.0, 1e-6), "{m}");
    let p = m["points"][1].clone();
    assert!(!near(p[1].as_f64().unwrap(), 100.0, 1.0), "rotated: {m}");
}

// ---------------------------------------------------------------- Circular and angular

/// M-043..M-046: Diameter, Center Radius, 3-Point Radius and Angle report the right values.
#[test]
fn diameter_radius_and_angle_tools() {
    let mut h = app();
    let mut before = markups(&h).len();
    // Diameter (Shift+Alt+D): 90 pt across = 10 ft.
    key(&mut h, SHIFT_ALT, egui::Key::D);
    assert_eq!(h.state().state.tool, "diameter");
    click(&mut h, 150.0, 450.0);
    click(&mut h, 240.0, 450.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1, "diameter: two clicks");
    assert_eq!(new[0].kind, Kind::Diameter);
    assert!(near(new[0].quantity().unwrap(), 10.0, 0.01), "{:?}", new[0].quantity());
    before = markups(&h).len();
    // Center Radius (Shift+Alt+U): centre then a point on the circle, 45 pt = 5 ft.
    key(&mut h, SHIFT_ALT, egui::Key::U);
    assert_eq!(h.state().state.tool, "radius");
    click(&mut h, 200.0, 550.0);
    click(&mut h, 245.0, 550.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1, "radius: two clicks");
    assert!(near(new[0].quantity().unwrap(), 5.0, 0.01), "{:?}", new[0].quantity());
    before = markups(&h).len();
    // 3-Point Radius: end, mid, end on a circle of radius 45 pt centred at (200, 450).
    run(&mut h, "tool.radius3");
    click(&mut h, 155.0, 450.0);
    click(&mut h, 200.0, 495.0);
    click(&mut h, 245.0, 450.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1, "3-point radius: three clicks");
    assert!(near(new[0].quantity().unwrap(), 5.0, 0.02), "{:?}", new[0].quantity());
    before = markups(&h).len();
    // Angle (Shift+Alt+G): a right angle.
    key(&mut h, SHIFT_ALT, egui::Key::G);
    assert_eq!(h.state().state.tool, "angle");
    click(&mut h, 240.0, 400.0);
    click(&mut h, 150.0, 400.0);
    click(&mut h, 150.0, 490.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1, "angle: three clicks");
    assert_eq!(new[0].kind, Kind::Angle);
    assert!(near(new[0].quantity().unwrap(), 90.0, 0.05), "{:?}", new[0].quantity());
}

// ---------------------------------------------------------------- Count

/// M-047: Count (Shift+Alt+C): each click adds a numbered item to one series; Esc ends.
#[test]
fn count_clicks_build_one_series_until_escape() {
    let mut h = app();
    let before = markups(&h).len();
    key(&mut h, SHIFT_ALT, egui::Key::C);
    assert_eq!(h.state().state.tool, "count");
    for x in [150.0, 180.0, 210.0] {
        click(&mut h, x, 450.0);
    }
    // M-055: the live count while placing.
    h.hover_at(screen(&h, 260.0, 450.0));
    h.run_steps(3);
    let live = h.state().state.status.clone();
    assert!(live.starts_with("Count: "), "live count: {live:?}");
    key(&mut h, egui::Modifiers::NONE, egui::Key::Escape);
    let new = added(&h, before);
    assert_eq!(new.len(), 1, "one series");
    assert_eq!(new[0].kind, Kind::Count);
    assert_eq!(new[0].quantity().unwrap(), 3.0);
    // After Esc, the next click starts a new series.
    key(&mut h, SHIFT_ALT, egui::Key::C);
    click(&mut h, 240.0, 450.0);
    key(&mut h, egui::Modifiers::NONE, egui::Key::Escape);
    assert_eq!(added(&h, before).len(), 2);
}

/// M-048 / M-050 / M-051 / M-052 / M-053: symbol look, resume, delete one item (renumbers),
/// split and merge, and dimensions on a count.
#[test]
fn count_series_edits() {
    let dir = temp_dir("m-count");
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    let c = add(
        &mut a,
        json!({ "page": 1, "kind": "Count", "points": [[100, 100], [150, 100], [200, 100]],
        "color": "#00AA00", "opacity": 0.5 }),
    );
    let cid = id(&c);
    assert_eq!(qty(&c), 3.0);
    call(
        &mut a,
        "count_edit",
        json!({ "id": cid, "add": [[250, 100], [300, 100]] }),
    );
    assert_eq!(qty(&get(&mut a, &cid)), 5.0);
    call(&mut a, "count_edit", json!({ "id": cid, "delete_item": 2 }));
    let m = get(&mut a, &cid);
    assert_eq!(qty(&m), 4.0);
    assert_eq!(m["points"][1], json!([200.0, 100.0]), "renumbered: {m}");
    call(&mut a, "count_edit", json!({ "id": cid, "split": [3, 4] }));
    assert_eq!(qty(&get(&mut a, &cid)), 2.0);
    let l = call(&mut a, "markup_list", json!({ "kind": "Count" }));
    let other = l["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] != cid)
        .unwrap()
        .clone();
    assert_eq!(qty(&other), 2.0);
    call(&mut a, "count_edit", json!({ "id": cid, "merge": [id(&other)] }));
    assert_eq!(qty(&get(&mut a, &cid)), 4.0);
    assert_eq!(call(&mut a, "markup_list", json!({ "kind": "Count" }))["count"], 1);
    // The look survives save.
    let ms = save_reopen(&mut a, "count.pdf");
    assert_eq!(ms[0]["color"], "#00AA00");
    assert!(near(ms[0]["opacity"].as_f64().unwrap(), 0.5, 0.01), "{}", ms[0]);
    // Count dimensions: a depth on the count is kept; the count stays a count.
    call(&mut a, "markup_edit", json!({ "ids": [ms[0]["id"]], "depth": 2 }));
    let ms = save_reopen(&mut a, "count2.pdf");
    assert_eq!(qty(&ms[0]), 4.0);
    call(
        &mut a,
        "summary_export",
        json!({ "out": "dims.csv", "columns": ["type", "depth"] }),
    );
    let csv = std::fs::read_to_string(dir.join("dims.csv")).unwrap();
    assert!(
        csv.lines().nth(1).is_some_and(|l| l.contains('2')),
        "the Depth column: {csv}"
    );
}

/// M-049: any markup becomes a count symbol via the Tool Chest.
#[test]
fn custom_count_symbol_from_a_markup() {
    let mut h = app();
    let before = markups(&h).len();
    key(&mut h, SHIFT_ALT, egui::Key::C);
    click(&mut h, 150.0, 450.0);
    click(&mut h, 200.0, 450.0);
    key(&mut h, egui::Modifiers::NONE, egui::Key::Escape);
    let count = added(&h, before)[0].id.clone();
    // Select the count and the red square: Properties offers the square as the symbol.
    run(&mut h, "tool.select");
    select(&mut h, &[count.clone(), "SAMPLESQUAREAAAA".to_string()]);
    panel(&mut h, "properties");
    let label = "Use the Rectangle as the symbol";
    press(&mut h, label);
    let c = find(&h, &count);
    assert!(!c.symbol_paths.is_empty(), "the square's outline is the count symbol");
    assert_eq!(c.quantity(), Some(2.0));
    // The count with its symbol goes to the Tool Chest for reuse.
    assert!(markupcraft_ui_egui::commands::find("markup.add_to_toolchest").is_some());
}

/// M-054 / M-056: count totals per page and per Space; statuses on counts.
#[test]
fn counts_per_page_space_and_status() {
    let dir = temp_dir("m-countspace");
    let mut a = blank(&dir, 2);
    call(
        &mut a,
        "space_add",
        json!({ "page": 1, "name": "Room 101", "points": [[0, 0], [300, 0], [300, 300], [0, 300]] }),
    );
    let c = add(
        &mut a,
        json!({ "page": 1, "kind": "Count", "subject": "Diffuser",
        "points": [[100, 100], [200, 100], [400, 400]] }),
    );
    add(
        &mut a,
        json!({ "page": 2, "kind": "Count", "subject": "Diffuser", "points": [[100, 100]] }),
    );
    let t = call(&mut a, "space_tally", json!({}));
    let s = t.to_string();
    assert!(s.contains("Room 101"), "{s}");
    // Room 101 holds two of page 1's three items; one is outside every space.
    let rows = t["spaces"].as_array().unwrap();
    let room = rows
        .iter()
        .find(|r| r["space"] == "Room 101" && r["page"] == 1)
        .unwrap();
    assert_eq!(room["counts"]["Diffuser"], 2, "{t}");
    let outside1 = rows.iter().find(|r| r["space"] == "" && r["page"] == 1).unwrap();
    assert_eq!(outside1["counts"]["Diffuser"], 1, "{t}");
    let page2 = rows.iter().find(|r| r["page"] == 2).unwrap();
    assert_eq!(page2["counts"]["Diffuser"], 1, "per page: {t}");
    call(&mut a, "markup_edit", json!({ "ids": [id(&c)], "status": "Completed" }));
    let r = call(&mut a, "count_status_report", json!({}));
    assert!(r.to_string().contains("Completed"), "{r}");
}

// ---------------------------------------------------------------- Dynamic Fill and detection

/// The sample's LIVING room: walls at x 120..300, y 350..690 = 20 ft x 37.78 ft. The dashed
/// grid line at y = 520 crosses it; ROOM is its upper part (y 520..690), what a vector fill that
/// stops at every drawn line finds.
const LIVING: f64 = (180.0 / 9.0) * (340.0 / 9.0);
const ROOM: f64 = (180.0 / 9.0) * (170.0 / 9.0);

/// The markup a dynamic_fill call made.
fn filled(v: &Value) -> Value {
    v["created"]["markup"].clone()
}

/// M-057 / M-058 / M-121: click inside a room; the fill follows the walls and becomes an Area,
/// a Perimeter, a Polygon or a Space.
#[test]
fn dynamic_fill_turns_a_room_into_measurements() {
    let dir = temp_dir("m-fill");
    let mut a = plan(&dir);
    let v = call(&mut a, "dynamic_fill", json!({ "page": 1, "point": [200, 600] }));
    let m = filled(&v);
    assert_eq!(m["kind"], "Area", "{v}");
    assert!(near(qty(&m), ROOM, ROOM * 0.03), "{} vs {ROOM}: {v}", qty(&m));
    let v = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [200, 600], "output": "perimeter" }),
    );
    let p = filled(&v);
    assert_eq!(p["kind"], "Perimeter");
    let want = 2.0 * (20.0 + 170.0 / 9.0);
    assert!(near(qty(&p), want, want * 0.03), "{} vs {want}", qty(&p));
    let v = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [200, 600], "output": "polygon" }),
    );
    assert_eq!(filled(&v)["kind"], "Polygon", "{v}");
    let v = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [200, 600], "output": "volume", "depth": 10 }),
    );
    let vol = filled(&v);
    assert!(near(qty(&vol), ROOM * 10.0, ROOM * 0.3), "{v}");
    call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [200, 600], "output": "space", "space_name": "Living" }),
    );
    let spaces = call(&mut a, "space_list", json!({ "page": 1 }));
    assert!(spaces.to_string().contains("Living"), "{spaces}");
    // J arms Dynamic Fill in the app.
    let mut h = app();
    key(&mut h, egui::Modifiers::NONE, egui::Key::J);
    h.run_steps(4);
    assert!(
        h.state().state.tool.contains("fill") || shows(&h, "Dynamic Fill"),
        "J starts Dynamic Fill (tool {})",
        h.state().state.tool
    );
}

/// M-059 / M-060 / M-061: a temporary boundary splits a room; dragging fills every region the
/// path passes through; the toolbar clears fill and boundaries.
#[test]
fn dynamic_fill_boundaries_and_drag() {
    let dir = temp_dir("m-fill2");
    let mut a = plan(&dir);
    // A boundary across ROOM at y = 605: the upper part is 20 ft x 9.44 ft.
    let v = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [200, 650], "boundaries": [[[120, 605], [300, 605]]] }),
    );
    let want = 20.0 * (85.0 / 9.0);
    assert!(near(qty(&filled(&v)), want, want * 0.04), "{v}");
    // Drag from ROOM down across the grid line: both regions in one fill (the whole room).
    let v = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "path": [[200, 600], [200, 450]] }),
    );
    // MarkupCraft makes one Area per region the drag passed through (Revu: one fill).
    let ids: Vec<String> = v["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), 2, "{v}");
    let total: f64 = ids.iter().map(|i| qty(&get(&mut a, i))).sum();
    assert!(near(total, LIVING, LIVING * 0.04), "both regions: {total}: {v}");
    // The Dynamic Fill toolbar's clear actions.
    let mut h = app();
    run(&mut h, "measure.dynamic_fill");
    h.run_steps(4);
    // Each click applies its fill at once (Edit > Undo takes one back), so the bar has no
    // Clear Fill / Clear All of a pending fill; boundaries clear here.
    for label in ["Add Boundary", "Clear Boundaries", "Drag across regions"] {
        assert!(shows(&h, label), "{label}");
    }
}

/// M-062: raster detection with its DPI and sensitivity, markups hidden while filling.
#[test]
fn dynamic_fill_raster_settings() {
    let dir = temp_dir("m-fill3");
    let mut a = plan(&dir);
    let v = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [200, 500], "detect": "raster", "dpi": 72, "sensitivity": 160, "hide_markups": true }),
    );
    // On the image the dashed grid line leaks, so the fill is about the whole room (less the
    // room name's glyphs, which become cutouts).
    let q = qty(&filled(&v));
    assert!(q > ROOM * 1.5 && q < LIVING * 1.02, "{q} vs {LIVING}: {v}");
}

/// M-063 / M-064 / M-066: Visual Search finds the grid bubbles from one boxed bubble; Apply
/// Count puts a count on every hit; an agent can place counts (no symbol recognition needed).
#[test]
fn visual_search_and_count_from_hits() {
    let dir = temp_dir("m-vs");
    let mut a = plan(&dir);
    // Bubble A at (120, 735), r = 12.
    let v = call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [104, 719, 136, 751], "sensitivity": 0.8, "pages": [1] }),
    );
    let hits = v["hits"].as_array().map_or(0, Vec::len);
    assert!(hits >= 6, "six bubbles: {v}");
    let strict = call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [104, 719, 136, 751], "sensitivity": 0.0, "pages": [1] }),
    );
    assert!(
        strict["hits"].as_array().map_or(0, Vec::len) <= hits,
        "lower = stricter"
    );
    call(
        &mut a,
        "visual_search",
        json!({ "page": 1, "rect": [104, 719, 136, 751], "sensitivity": 0.8, "pages": [1], "action": "count", "subject": "Grid bubble" }),
    );
    let l = call(
        &mut a,
        "markup_list",
        json!({ "kind": "Count", "subject": "Grid bubble" }),
    );
    let c = &l["markups"][0];
    assert_eq!(qty(c) as usize, hits, "{l}");
    // An agent places counts directly.
    let m = add(
        &mut a,
        json!({ "page": 1, "kind": "Count", "subject": "Agent", "points": [[10, 10], [20, 20]] }),
    );
    assert_eq!(qty(&m), 2.0);
}

/// M-065: counts from text search hits (Search panel > Apply Count).
#[test]
fn count_from_text_search_hits() {
    let mut h = app();
    shortcut(&mut h, egui::Modifiers::COMMAND, egui::Key::F);
    assert!(h.state().state.open_panels.contains(&"search"));
    h.event(egui::Event::Text("BEDROOM".into()));
    h.run_steps(2);
    shortcut(&mut h, egui::Modifiers::NONE, egui::Key::Enter);
    assert!(shows(&h, "1 result"));
    let before = markups(&h).len();
    press(&mut h, "Count");
    let new = added(&h, before);
    assert_eq!(new.len(), 1, "one count per page with hits");
    assert_eq!(new[0].kind, Kind::Count);
    assert_eq!(new[0].pts.len(), 1);
    assert_eq!(new[0].subject, "BEDROOM");
}

// ---------------------------------------------------------------- Sketch to Scale

/// Type a segment in the Sketch to Scale bar and press Add.
fn sketch_segment(h: &mut Harness<'_, MarkupCraftApp>, length: &str, angle: &str) {
    h.state_mut().state.edit.sketch.length = length.into();
    h.state_mut().state.edit.sketch.angle = angle.into();
    h.run_steps(2);
    press(h, "Add");
}

/// Empty the length box and press Finish (Finish with a length places that segment too).
fn finish_sketch(h: &mut Harness<'_, MarkupCraftApp>) {
    h.state_mut().state.edit.sketch.length.clear();
    h.run_steps(2);
    press(h, "Finish");
}

/// M-067 / M-068 / M-071: polyline and polygon from typed lengths and angles at page scale
/// (1/8" = 1'-0": 10 ft = 90 pt), absolute or relative angles.
#[test]
fn sketch_polyline_and_polygon_to_scale() {
    let mut h = app();
    let before = markups(&h).len();
    run(&mut h, "tool.polyline");
    click(&mut h, 150.0, 400.0);
    assert!(shows(&h, "Sketch to Scale"), "the bar shows while drawing");
    sketch_segment(&mut h, "10'", "0");
    sketch_segment(&mut h, "5'", "90");
    finish_sketch(&mut h);
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    let p = &new[0].pts;
    assert_eq!(p.len(), 3, "{p:?}");
    assert!(near(p[1].x - p[0].x, 90.0, 0.01) && near(p[1].y, p[0].y, 0.01), "{p:?}");
    assert!(near(p[2].x, p[1].x, 0.01) && near(p[2].y - p[1].y, 45.0, 0.01), "{p:?}");
    // Relative angles: 90 turns left from the previous segment.
    let before = markups(&h).len();
    run(&mut h, "tool.polygon");
    click(&mut h, 150.0, 500.0);
    run(&mut h, "tools.sketch_relative");
    sketch_segment(&mut h, "10'", "0");
    sketch_segment(&mut h, "10'", "90");
    sketch_segment(&mut h, "10'", "90");
    finish_sketch(&mut h);
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].kind, Kind::Polygon);
    let p = &new[0].pts;
    assert!(
        near(p[3].x, p[0].x, 0.01) && near(p[3].y - p[0].y, 90.0, 0.01),
        "a square: {p:?}"
    );
}

/// M-069 / M-070: rectangle from typed width x height; ellipse from width x height or a radius.
#[test]
fn sketch_rectangle_and_ellipse_to_scale() {
    let mut h = app();
    let before = markups(&h).len();
    run(&mut h, "tool.rectangle");
    click(&mut h, 150.0, 400.0);
    h.state_mut().state.edit.sketch.width = "20'".into();
    h.state_mut().state.edit.sketch.height = "10'".into();
    press(&mut h, "Place");
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    let r = new[0].rect.normalized();
    assert!(near(r.width(), 180.0, 4.0) && near(r.height(), 90.0, 4.0), "{r:?}");
    let before = markups(&h).len();
    h.state_mut().state.shell.ui.extra.sketch_radius = true;
    run(&mut h, "tool.ellipse");
    click(&mut h, 200.0, 550.0);
    h.state_mut().state.edit.sketch.width = "5'".into();
    press(&mut h, "Place");
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    let r = new[0].rect.normalized();
    assert!(
        near(r.width(), 90.0, 4.0) && near(r.height(), 90.0, 4.0),
        "a 5 ft radius circle: {r:?}"
    );
}

// ---------------------------------------------------------------- Drawing aids and snapping

/// Snap settings: content, markup and grid.
fn snaps(h: &mut Harness<'_, MarkupCraftApp>, content: bool, markup: bool, grid: bool) {
    let s = &mut h.state_mut().state.snaps;
    s.content = content;
    s.markup = markup;
    s.grid = grid;
    h.run_steps(2);
}

/// M-072: Shift constrains a segment to 0/45/90 degrees.
#[test]
fn shift_constrains_to_orthogonal() {
    let mut h = app();
    snaps(&mut h, false, false, false);
    let before = markups(&h).len();
    run(&mut h, "tool.length");
    click(&mut h, 150.0, 400.0);
    click_with(&mut h, egui::Modifiers::SHIFT, 240.0, 407.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    assert!(near(new[0].pts[1].y, 400.0, 0.01), "horizontal: {:?}", new[0].pts);
}

/// M-073 / M-074 / M-075 / M-076 / M-077 / M-078: snaps to drawing vectors, to markups and to a
/// grid; the grid shows; snap targets and sensitivity are preferences.
#[test]
fn snapping_to_content_markup_and_grid() {
    let mut h = app();
    // Content: the wall corner at (300, 350), clicked 2 pt off.
    snaps(&mut h, true, false, false);
    let before = markups(&h).len();
    run(&mut h, "tool.length");
    click(&mut h, 302.0, 352.5);
    click(&mut h, 302.0, 500.0);
    let new = added(&h, before);
    assert!(
        near(new[0].pts[0].x, 300.0, 0.3) && near(new[0].pts[0].y, 350.0, 0.3),
        "{:?}",
        new[0].pts
    );
    // Markup: the red square's corner (900, 600), drawing snaps off.
    snaps(&mut h, false, true, false);
    let before = markups(&h).len();
    run(&mut h, "tool.length");
    click(&mut h, 902.0, 602.0);
    click(&mut h, 850.0, 500.0);
    let new = added(&h, before);
    assert!(
        near(new[0].pts[0].x, 900.0, 0.3) && near(new[0].pts[0].y, 600.0, 0.3),
        "{:?}",
        new[0].pts
    );
    // The shortcut toggles snaps.
    let was = h.state().state.snaps.content;
    shortcut(&mut h, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::F8);
    assert_ne!(h.state().state.snaps.content, was, "Ctrl+Shift+F8");
    let was = h.state().state.snaps.markup;
    shortcut(&mut h, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::F7);
    assert_ne!(h.state().state.snaps.markup, was, "Ctrl+Shift+F7");
    let was = h.state().state.snaps.grid;
    shortcut(&mut h, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::F9);
    assert_ne!(h.state().state.snaps.grid, was, "Ctrl+Shift+F9");
    // Show Grid.
    let was = h.state().state.show_grid;
    run(&mut h, "view.show_grid");
    assert_ne!(h.state().state.show_grid, was);
    // Grid snapping and sensitivity are preferences (the tool table's prefs).
    let dir = temp_dir("m-snap");
    let mut a = automation(&dir);
    call(
        &mut a,
        "prefs_set",
        json!({ "values": { "snapping": { "grid": true, "grid_spacing": 12, "sensitivity_px": 15 } } }),
    );
    let p = call(&mut a, "prefs_get", json!({}));
    assert_eq!(p["preferences"]["snapping"]["grid_spacing"], 12.0, "{p}");
    assert_eq!(p["preferences"]["snapping"]["sensitivity_px"], 15.0, "{p}");
    // Snap point types: a mask of targets.
    // Snap point types (Preferences > Grid & Snap > Snap to): with every type off nothing
    // snaps; with only endpoints on, the wall corner does again.
    {
        let x = &mut h.state_mut().state.shell.ui.extra;
        x.snap_endpoints = false;
        x.snap_midpoints = false;
        x.snap_intersections = false;
        x.snap_nearest = false;
        x.snap_centers = false;
    }
    snaps(&mut h, true, false, false);
    let before = markups(&h).len();
    run(&mut h, "tool.length");
    click(&mut h, 302.0, 352.5);
    click(&mut h, 302.0, 500.0);
    let p = added(&h, before)[0].pts[0];
    assert!(
        !(near(p.x, 300.0, 0.3) && near(p.y, 350.0, 0.3)),
        "no snap types: {p:?}"
    );
    h.state_mut().state.shell.ui.extra.snap_endpoints = true;
    h.run_steps(2);
    let before = markups(&h).len();
    run(&mut h, "tool.length");
    click(&mut h, 302.0, 352.5);
    click(&mut h, 302.0, 500.0);
    let p = added(&h, before)[0].pts[0];
    assert!(near(p.x, 300.0, 0.3) && near(p.y, 350.0, 0.3), "endpoint: {p:?}");
}

/// M-075: grid snapping puts points on the grid.
#[test]
fn grid_snap_rounds_points_to_the_grid() {
    let mut h = app();
    snaps(&mut h, false, false, true);
    let before = markups(&h).len();
    run(&mut h, "tool.length");
    click(&mut h, 151.3, 402.6);
    click(&mut h, 233.8, 402.6);
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    let p = new[0].pts[0];
    let snapped = |v: f64| {
        [9.0, 10.0, 12.0, 18.0, 36.0, 72.0]
            .iter()
            .any(|g| near(v / g, (v / g).round(), 1e-6))
    };
    assert!(snapped(p.x) && snapped(p.y), "on a grid: {p:?}");
}

/// M-079: rulers toggle (Ctrl+R).
#[test]
fn rulers_toggle() {
    let mut h = app();
    let cmd = markupcraft_ui_egui::commands::find("view.rulers").unwrap();
    assert!(cmd.keys.is_some(), "a shortcut");
    run(&mut h, "view.rulers");
    run(&mut h, "view.rulers");
}

/// M-080: hold Space to pan without ending the measurement in progress.
#[test]
fn space_pans_while_drawing() {
    let mut h = app();
    snaps(&mut h, false, false, false);
    run(&mut h, "view.zoom_in");
    run(&mut h, "view.zoom_in");
    let before = markups(&h).len();
    run(&mut h, "tool.polylength");
    click(&mut h, 150.0, 400.0);
    click(&mut h, 240.0, 400.0);
    let page_before = screen(&h, 0.0, 0.0);
    h.event(egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    let (from, to) = (screen(&h, 500.0, 500.0), screen(&h, 450.0, 450.0));
    h.hover_at(from);
    h.step();
    button(&mut h, from, true, egui::Modifiers::NONE);
    for i in 1..=6 {
        h.hover_at(from + (to - from) * (i as f32 / 6.0));
        h.step();
    }
    button(&mut h, to, false, egui::Modifiers::NONE);
    h.event(egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
    assert_ne!(screen(&h, 0.0, 0.0), page_before, "the view panned");
    assert_eq!(markups(&h).len(), before, "the draft is still open");
    click(&mut h, 240.0, 490.0);
    key(&mut h, egui::Modifiers::NONE, egui::Key::Enter);
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].pts.len(), 3, "the measurement kept its points: {:?}", new[0].pts);
}

/// M-081: Ctrl swaps the mouse wheel between zoom and pan.
#[test]
fn ctrl_wheel_swaps_zoom_and_pan() {
    let mut h = app();
    let zoom = |h: &Harness<'_, MarkupCraftApp>| h.state().state.doc().unwrap().view.zoom;
    let at = screen(&h, 600.0, 400.0);
    h.hover_at(at);
    h.step();
    let z0 = zoom(&h);
    let wheel = |h: &mut Harness<'_, MarkupCraftApp>, m: egui::Modifiers| {
        hold(h, m);
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 3.0),
            modifiers: m,
            phase: egui::TouchPhase::Move,
        });
        h.step();
        hold(h, egui::Modifiers::NONE);
        h.run_steps(3);
    };
    let wheel_zooms = h.state().state.wheel_zooms;
    wheel(&mut h, egui::Modifiers::NONE);
    let z1 = zoom(&h);
    assert_eq!(z1 != z0, wheel_zooms, "plain wheel follows the preference");
    wheel(&mut h, CTRL);
    let z2 = zoom(&h);
    assert_eq!(z2 != z1, !wheel_zooms, "Ctrl does the other one: {z0} {z1} {z2}");
}

/// M-082: Ctrl+Shift+drag copies a markup along a straight line.
#[test]
fn ctrl_shift_drag_copies_in_a_straight_line() {
    let mut h = app();
    snaps(&mut h, false, false, false);
    run(&mut h, "tool.select");
    let before = markups(&h).len();
    let m = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    drag_with(&mut h, m, (950.0, 650.0), (1100.0, 657.0));
    let new = added(&h, before);
    assert_eq!(new.len(), 1, "a copy");
    let r = new[0].rect.normalized();
    let orig = find(&h, "SAMPLESQUAREAAAA").rect.normalized();
    assert!(near(r.y0, orig.y0, 0.5), "kept on the horizontal: {r:?} vs {orig:?}");
    assert!(r.x0 > orig.x0 + 100.0, "{r:?}");
}

// ---------------------------------------------------------------- Properties, captions, appearance

/// A saved one-page document at 1/8" = 1'-0" with a Length, a Polylength, an Area and a Volume
/// (10 ft, 20 ft, 100 sf, 800 cu ft); returns (dir, ids in that order).
fn takeoff_file(tag: &str) -> (std::path::PathBuf, Vec<String>) {
    let dir = temp_dir(tag);
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    let sq = json!([[100, 100], [190, 100], [190, 190], [100, 190]]);
    let ids = vec![
        id(&add(
            &mut a,
            json!({ "page": 1, "kind": "Length", "points": [[100, 300], [190, 300]], "subject": "Pipe" }),
        )),
        id(&add(
            &mut a,
            json!({ "page": 1, "kind": "Polylength", "points": [[100, 400], [190, 400], [190, 490]], "subject": "Duct" }),
        )),
        id(&add(
            &mut a,
            json!({ "page": 1, "kind": "Area", "points": sq, "subject": "Floor" }),
        )),
        id(&add(
            &mut a,
            json!({ "page": 1, "kind": "Volume", "points": [[300, 100], [390, 100], [390, 190], [300, 190]], "depth": 8, "subject": "Concrete" }),
        )),
    ];
    call(&mut a, "doc_save", json!({ "path": "takeoff.pdf", "full": true }));
    (dir, ids)
}

/// M-083 / M-084 / M-100: subject groups in the list; the label shows on the markup with its
/// value; layer, author and comment are fields that filter the summary.
#[test]
fn subject_label_layer_author_comment() {
    let dir = temp_dir("m-subj");
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    call(&mut a, "layer_create", json!({ "name": "M-Supply" }));
    for (s, au) in [("Supply Duct", "Ann"), ("Supply Duct", "Bob"), ("Return Duct", "Ann")] {
        add(
            &mut a,
            json!({ "page": 1, "kind": "Length", "points": [[0, 0], [90, 0]], "subject": s,
            "author": au, "layer": "M-Supply", "contents": "check" }),
        );
    }
    let l = call(&mut a, "markup_list", json!({ "subject": "Supply Duct" }));
    assert_eq!(l["count"], 2);
    // Set after drawing too.
    let one = l["markups"][0]["id"].as_str().unwrap().to_string();
    call(
        &mut a,
        "markup_edit",
        json!({ "ids": [one], "subject": "Return Duct", "label": "R-1" }),
    );
    assert_eq!(
        call(&mut a, "markup_list", json!({ "subject": "Return Duct" }))["count"],
        2
    );
    call(
        &mut a,
        "summary_export",
        json!({ "out": "s.csv", "columns": ["subject", "author", "layer", "comments", "measurement"], "group_by": ["subject"],
                "filters": { "author": ["Ann"] } }),
    );
    let csv = std::fs::read_to_string(dir.join("s.csv")).unwrap();
    assert!(!csv.contains("Bob"), "filtered by author: {csv}");
    assert!(csv.contains("M-Supply") && csv.contains("check"), "{csv}");
    // The label shows on the markup itself, beside the value.
    call(&mut a, "doc_save", json!({ "path": "subj.pdf", "full": true }));
    let h = app_with(&dir.join("subj.pdf"));
    let m = markups(&h).into_iter().find(|m| m.id == one).unwrap();
    let c = markupcraft_model::caption::caption_text(&m);
    assert!(c.contains("R-1") && c.contains("10"), "caption {c:?}");
}

/// M-085: units per measurement, independent of the page scale (length, area, volume).
#[test]
fn units_per_measurement_from_properties() {
    use markupcraft_measure::units::{DisplayUnit, LengthUnit, scale_display_unit};
    let (dir, ids) = takeoff_file("m-units");
    let mut h = app_with(&dir.join("takeoff.pdf"));
    panel(&mut h, "properties");
    // The Length in inches: 10 ft = 120 in.
    select(&mut h, &[ids[0].clone()]);
    let now = scale_display_unit(find(&h, &ids[0]).scale.as_ref().unwrap())
        .unwrap()
        .name();
    pick(&mut h, &now, &DisplayUnit::single(LengthUnit::Inch).name());
    assert!(near(find(&h, &ids[0]).quantity().unwrap(), 120.0, 1e-6));
    // The page scale did not change: the Polylength is still in feet.
    assert!(near(find(&h, &ids[1]).quantity().unwrap(), 20.0, 1e-6));
    // The Volume in cubic yards: 800 cu ft = 29.63 cu yd.
    select(&mut h, &[ids[3].clone()]);
    pick(&mut h, "cu ft", "cu yd");
    assert!(near(find(&h, &ids[3]).quantity().unwrap(), 800.0 / 27.0, 1e-3));
    // The Area in square yards: 100 sf = 11.11 sy.
    select(&mut h, &[ids[2].clone()]);
    pick(&mut h, "sf", "sy");
    assert!(near(find(&h, &ids[2]).quantity().unwrap(), 100.0 / 9.0, 1e-3));
}

/// M-086: Set as Default: a configured measurement's look becomes the tool's default.
#[test]
fn set_as_default_carries_to_new_measurements() {
    let mut h = app();
    snaps(&mut h, false, false, false);
    let before = markups(&h).len();
    run(&mut h, "tool.length");
    click(&mut h, 150.0, 400.0);
    click(&mut h, 240.0, 400.0);
    let first = added(&h, before)[0].id.clone();
    let blue = markupcraft_model::Color { r: 0.0, g: 0.0, b: 1.0 };
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_properties(
            std::slice::from_ref(&first),
            &markupcraft_engine::props::MarkupPatch {
                color: Some(blue),
                subject: Some("Supply".into()),
                ..Default::default()
            },
        )
        .unwrap();
    select(&mut h, &[first]);
    run(&mut h, "markup.set_default");
    let before = markups(&h).len();
    run(&mut h, "tool.length");
    click(&mut h, 150.0, 450.0);
    click(&mut h, 240.0, 450.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].color, blue, "the default colour");
}

/// M-087 / M-089 / M-093 / M-116: caption contents (fields and custom columns), the caption
/// leader line and the caption font, all saved.
#[test]
fn caption_contents_leader_and_font() {
    let (dir, ids) = takeoff_file("m-caption");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "takeoff.pdf" }));
    call(
        &mut a,
        "columns_set",
        json!({ "columns": [{ "id": "mat", "name": "Material", "type": "Text" }] }),
    );
    call(
        &mut a,
        "markup_edit",
        json!({ "ids": [ids[2]], "columns": { "mat": "Oak" }, "font_size": 18, "bold": true }),
    );
    call(
        &mut a,
        "measure_props_set",
        json!({ "ids": [ids[2]], "caption": "{subject}: {value} {c:mat}", "caption_leader": true, "caption_italic": true }),
    );
    call(&mut a, "doc_save", json!({ "path": "cap.pdf", "full": true }));
    let h = app_with(&dir.join("cap.pdf"));
    let m = markups(&h).into_iter().find(|m| m.id == ids[2]).unwrap();
    let c = markupcraft_model::caption::caption_text(&m);
    assert_eq!(c, "Floor: 100 sf Oak");
    assert!(m.caption_leader, "leader saved");
    assert!(near(m.text.size, 18.0, 0.01), "font size saved: {:?}", m.text);
    // Show Caption off hides the value.
    call(&mut a, "markup_edit", json!({ "ids": [ids[0]], "show_caption": false }));
    call(&mut a, "doc_save", json!({ "path": "cap2.pdf", "full": true }));
    let h = app_with(&dir.join("cap2.pdf"));
    assert!(markups(&h).into_iter().find(|m| m.id == ids[0]).unwrap().hide_caption);
}

/// M-088 / M-090: captions sit at the centre of areas and along the last segment of runs; a
/// caption moves on its own and resets.
#[test]
fn caption_placement_move_and_reset() {
    use markupcraft_model::caption::caption_anchor;
    let (dir, ids) = takeoff_file("m-capmove");
    let mut h = app_with(&dir.join("takeoff.pdf"));
    let area = find(&h, &ids[2]);
    let at = caption_anchor(&area);
    assert!(
        near(at.x, 145.0, 1.0) && near(at.y, 145.0, 1.0),
        "area caption centred: {at:?}"
    );
    let run_ = find(&h, &ids[1]);
    let at = caption_anchor(&run_);
    // Last segment: (190, 400) -> (190, 490).
    assert!(
        near(at.x, 190.0, 20.0) && at.y > 400.0 && at.y < 490.0,
        "along the last segment: {at:?}"
    );
    // Shift+drag the area's caption.
    run(&mut h, "tool.select");
    snaps(&mut h, false, false, false);
    select(&mut h, &[ids[2].clone()]);
    drag_with(&mut h, egui::Modifiers::SHIFT, (145.0, 145.0), (145.0, 260.0));
    let m = find(&h, &ids[2]);
    assert!(m.caption_offset.is_some(), "the caption moved on its own");
    assert_eq!(m.pts, area.pts, "the shape stayed");
    // Right-click the markup > Reset Caption Position.
    let at = screen(&h, 120.0, 120.0);
    h.hover_at(at);
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(3);
    h.get_all_by_label("Reset Caption Position")
        .next()
        .expect("context menu item")
        .click();
    h.run_steps(4);
    assert!(find(&h, &ids[2]).caption_offset.is_none());
}

/// M-091 / M-092: line endings, colour, fill, width, dash and opacity on measurements, saved.
#[test]
fn measurement_appearance_saves() {
    let dir = temp_dir("m-look");
    let mut a = blank(&dir, 1);
    call(&mut a, "scale_set", json!({ "scale": eighth() }));
    add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[0, 100], [90, 100]],
        "line_start": "OpenArrow", "line_end": "ClosedArrow", "width": 3, "color": "#0000FF", "dash": [6, 3], "opacity": 0.6 }),
    );
    add(
        &mut a,
        json!({ "page": 1, "kind": "Area", "points": [[100, 100], [190, 100], [190, 190], [100, 190]],
        "fill": "#00FF00", "fill_opacity": 0.3 }),
    );
    let ms = save_reopen(&mut a, "look.pdf");
    let l = ms.iter().find(|m| m["kind"] == "Length").unwrap();
    assert_eq!(l["color"], "#0000FF");
    assert_eq!(l["width"], 3.0);
    assert_eq!(l["dash"], json!([6.0, 3.0]));
    assert!(near(l["opacity"].as_f64().unwrap(), 0.6, 0.01));
    let h = app_with(&dir.join("look.pdf"));
    let m = markups(&h).into_iter().find(|m| m.kind == Kind::Length).unwrap();
    assert_eq!(
        (m.line_start.as_str(), m.line_end.as_str()),
        ("OpenArrow", "ClosedArrow")
    );
    // Changed later on the saved markup, saved again.
    call(
        &mut a,
        "markup_edit",
        json!({ "ids": [m.id.clone()], "line_start": "Slash", "line_end": "None" }),
    );
    call(&mut a, "doc_save", json!({ "path": "look2.pdf", "full": true }));
    let h = app_with(&dir.join("look2.pdf"));
    let m = markups(&h).into_iter().find(|m| m.kind == Kind::Length).unwrap();
    assert_eq!((m.line_start.as_str(), m.line_end.as_str()), ("Slash", "None"));
    let ar = ms.iter().find(|m| m["kind"] == "Area").unwrap();
    assert_eq!(ar["fill"], "#00FF00");
    assert!(near(ar["fill_opacity"].as_f64().unwrap(), 0.3, 0.01));
}

/// M-095: measurements without annotations: the tool reads out, nothing is added.
#[test]
fn measure_without_making_annotations() {
    let mut h = app();
    snaps(&mut h, false, false, false);
    run(&mut h, "measure.make_annotations");
    let before = markups(&h).len();
    run(&mut h, "tool.length");
    click(&mut h, 150.0, 400.0);
    click(&mut h, 240.0, 400.0);
    assert_eq!(markups(&h).len(), before, "no markup");
    assert!(
        h.state().state.status.contains("10"),
        "the value is read out: {}",
        h.state().state.status
    );
    run(&mut h, "measure.make_annotations");
    run(&mut h, "tool.length");
    click(&mut h, 150.0, 400.0);
    click(&mut h, 240.0, 400.0);
    assert_eq!(markups(&h).len(), before + 1);
}

/// M-096 / M-097: M arms the unified Measure tool and opens the Measurements panel, which holds
/// the tool buttons, the scale and precision, and the viewports.
#[test]
fn measure_tool_and_measurements_panel() {
    let mut h = app();
    key(&mut h, egui::Modifiers::NONE, egui::Key::M);
    h.run_steps(4);
    assert!(
        h.state().state.open_panels.contains(&"measurements"),
        "{:?}",
        h.state().state.open_panels
    );
    for label in ["Calibrate...", "Precision", "Units", "Preset"] {
        assert!(shows(&h, label), "{label}");
    }
    assert!(shows(&h, "Viewport"), "viewports section");
    assert!(
        shows(&h, "Length") && shows(&h, "Area") && shows(&h, "Count"),
        "tool buttons"
    );
}

/// M-098 / M-099: totals of a mixed selection; bulk edit of many measurements at once.
#[test]
fn totals_and_bulk_edit_of_the_selection() {
    let (dir, ids) = takeoff_file("m-totals");
    let mut h = app_with(&dir.join("takeoff.pdf"));
    select(&mut h, &ids);
    h.run_steps(4);
    // 10 ft + 20 ft of length; 100 sf.
    assert!(shows(&h, "30"), "length total of the selection");
    assert!(shows(&h, "100"), "area total of the selection");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "takeoff.pdf" }));
    // Box selection then one change for all.
    let sel = call(
        &mut a,
        "select_lasso",
        json!({ "page": 1, "points": [[50, 50], [250, 50], [250, 550], [50, 550]] }),
    );
    assert_eq!(sel["selection"].as_array().unwrap().len(), 3, "{sel}");
    call(&mut a, "markup_edit", json!({ "color": "#123456", "subject": "Bulk" }));
    assert_eq!(call(&mut a, "markup_list", json!({ "subject": "Bulk" }))["count"], 3);
}

/// M-101: a locked markup cannot be moved; Ctrl+Shift+L locks in the app.
#[test]
fn locked_measurements_do_not_move() {
    let (dir, ids) = takeoff_file("m-lock");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "takeoff.pdf" }));
    call(&mut a, "markup_edit", json!({ "ids": [ids[2]], "locked": true }));
    fails(&mut a, "markup_transform", json!({ "ids": [ids[2]], "move": [10, 0] }));
    let ms = save_reopen(&mut a, "lock.pdf");
    assert_eq!(ms.iter().find(|m| m["id"] == ids[2].as_str()).unwrap()["locked"], true);
    let mut h = app_with(&dir.join("takeoff.pdf"));
    select(&mut h, &[ids[0].clone()]);
    shortcut(&mut h, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::L);
    assert!(find(&h, &ids[0]).locked(), "Ctrl+Shift+L locks");
}

/// M-102: keep the last subject and label for new measurements (a toggle).
#[test]
fn keep_last_subject_and_label() {
    let mut h = app();
    snaps(&mut h, false, false, false);
    run(&mut h, "measure.keep_subject");
    let before = markups(&h).len();
    run(&mut h, "tool.length");
    click(&mut h, 150.0, 400.0);
    click(&mut h, 240.0, 400.0);
    let first = added(&h, before)[0].id.clone();
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_properties(
            &[first],
            &markupcraft_engine::props::MarkupPatch {
                subject: Some("Sprinkler main".into()),
                label: Some("SP".into()),
                ..Default::default()
            },
        )
        .unwrap();
    h.run_steps(2);
    let before = markups(&h).len();
    run(&mut h, "tool.length");
    click(&mut h, 150.0, 450.0);
    click(&mut h, 240.0, 450.0);
    let new = added(&h, before);
    assert_eq!(new[0].subject, "Sprinkler main");
    assert_eq!(new[0].label, "SP");
}

// ---------------------------------------------------------------- Tool Chest for takeoff

/// The Tool Chest item made from `id` by markup.add_to_toolchest: (set id, item id).
fn chest_item_from(h: &mut Harness<'_, MarkupCraftApp>, id: &str) -> (String, String) {
    let had: usize = h.state().state.toolchest.sets.iter().map(|s| s.items.len()).sum();
    select(h, &[id.to_string()]);
    run(h, "markup.add_to_toolchest");
    let chest = &h.state().state.toolchest;
    let now: usize = chest.sets.iter().map(|s| s.items.len()).sum();
    assert_eq!(now, had + 1, "{}", h.state().state.status);
    chest
        .sets
        .iter()
        .flat_map(|s| s.items.iter().map(move |i| (s, i)))
        .find(|(_, i)| i.markup.subject == find(h, id).subject)
        .map(|(s, i)| (s.id.clone(), i.id.clone()))
        .unwrap()
}

/// M-103 / M-104: a configured measurement saved to the Tool Chest draws new ones with its
/// subject, colour, depth and custom column values (Properties mode), or stamps its exact
/// geometry (Drawing mode).
#[test]
fn tool_chest_measurement_tools() {
    let mut h = app();
    snaps(&mut h, false, false, false);
    let before = markups(&h).len();
    run(&mut h, "tool.area");
    for (x, y) in [(150.0, 400.0), (240.0, 400.0), (240.0, 490.0)] {
        click(&mut h, x, y);
    }
    double_click(&mut h, 150.0, 490.0);
    let src = added(&h, before)[0].id.clone();
    let green = markupcraft_model::Color { r: 0.0, g: 0.6, b: 0.0 };
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_properties(
            std::slice::from_ref(&src),
            &markupcraft_engine::props::MarkupPatch {
                color: Some(green),
                subject: Some("Carpet".into()),
                depth: Some(0.5),
                ..Default::default()
            },
        )
        .unwrap();
    h.run_steps(2);
    let (set, item) = chest_item_from(&mut h, &src);
    // Properties mode: new geometry, the saved look.
    h.state_mut().state.use_item(&set, &item);
    h.run_steps(2);
    let before = markups(&h).len();
    for (x, y) in [(400.0, 400.0), (445.0, 400.0), (445.0, 445.0)] {
        click(&mut h, x, y);
    }
    double_click(&mut h, 400.0, 445.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].subject, "Carpet");
    assert_eq!(new[0].color, green);
    assert!(near(new[0].depth, 0.5, 1e-9));
    assert!(
        near(new[0].quantity().unwrap(), 25.0, 0.05),
        "new geometry: {:?}",
        new[0].quantity()
    );
    // Drawing mode: one click stamps the saved 10 x 10 ft shape.
    h.state_mut()
        .state
        .toolchest
        .update_item(&set, &item, |i| i.mode = markupcraft_ui_egui::chest::Mode::Drawing);
    h.state_mut().state.use_item(&set, &item);
    h.run_steps(2);
    let before = markups(&h).len();
    click(&mut h, 450.0, 600.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1, "one click places the copy");
    assert!(near(new[0].quantity().unwrap(), 100.0, 0.05), "{:?}", new[0].quantity());
}

/// M-105 / M-106: named tool sets exported and imported elsewhere; a set authored at one scale
/// resizes its symbols on a page of another scale.
#[test]
fn tool_sets_share_and_rescale() {
    let mut h = app();
    let sq = find(&h, "SAMPLESQUAREAAAA");
    let chest = &mut h.state_mut().state.toolchest;
    let set = chest.add_set("Supply duct");
    let item = chest.add_markup(&set, &sq).expect("added");
    let text = chest.export_set(&set).unwrap();
    let mut other = markupcraft_ui_egui::chest::ToolChest::default();
    let imported = other.import_set(&text).unwrap();
    let s = other.find_set(&imported).unwrap();
    assert_eq!(s.title, "Supply duct");
    assert_eq!(s.items.len(), 1);
    // Scaler: the set was authored at 1/16" = 1'-0"; the sample page is 1/8" = 1'-0", so a
    // Drawing-mode copy doubles on paper (same real size).
    let chest = &mut h.state_mut().state.toolchest;
    chest.set_scale(&set, Some(markupcraft_measure::Scale::architectural(1.0 / 16.0, 1.0)));
    chest.update_item(&set, &item, |i| i.mode = markupcraft_ui_egui::chest::Mode::Drawing);
    h.state_mut().state.edit.item_scale = h.state().state.toolchest.item_set_scale(&set).cloned();
    h.state_mut().state.use_item(&set, &item);
    h.run_steps(2);
    let before = markups(&h).len();
    click(&mut h, 450.0, 600.0);
    let new = added(&h, before);
    assert_eq!(new.len(), 1);
    let (a, b) = (new[0].rect.normalized(), sq.rect.normalized());
    assert!(near(a.width() / b.width(), 2.0, 0.05), "{a:?} vs {b:?}");
}

/// M-107: a takeoff profile arranges the panels for takeoff.
#[test]
fn takeoff_workspace_profile() {
    let mut h = app();
    run(&mut h, "window.takeoff_workspace");
    let open = h.state().state.open_panels.clone();
    for p in ["measurements", "markups", "toolchest"] {
        assert!(open.contains(&p), "{p}: {open:?}");
    }
}

// ---------------------------------------------------------------- Markups List and columns

/// The markup rows of a summary CSV (no header, no "Total (n)" rows).
fn data_rows(csv: &str) -> Vec<&str> {
    csv.lines()
        .skip(1)
        .filter(|l| !l.starts_with("Total") && !l.is_empty())
        .collect()
}

fn csv_of(dir: &std::path::Path, a: &mut Automation, args: Value) -> String {
    let mut args = args;
    args["out"] = json!("list.csv");
    call(a, "summary_export", args);
    std::fs::read_to_string(dir.join("list.csv")).unwrap()
}

/// M-108 / M-109 / M-110: the measurement column with totals, the measurement columns, and
/// sorting, grouping and filtering.
#[test]
fn markups_list_measurement_columns_totals_sort_filter() {
    let (dir, _ids) = takeoff_file("m-list");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "takeoff.pdf" }));
    let csv = csv_of(
        &dir,
        &mut a,
        json!({ "columns": ["subject", "measurement", "length", "area", "volume", "depth", "wallarea", "count", "space", "pagelabel", "layer"], "content": "both" }),
    );
    let head = csv.lines().next().unwrap();
    for c in [
        "Measurement",
        "Length",
        "Area",
        "Volume",
        "Depth",
        "Wall Area",
        "Count",
        "Space",
        "Page Label",
        "Layer",
    ] {
        assert!(head.contains(c), "{c} in {head}");
    }
    assert!(csv.to_lowercase().contains("total"), "totals rows: {csv}");
    // 10 + 20 ft of length in total.
    assert!(csv.contains("30"), "{csv}");
    // Sorted by subject descending; filtered by subject.
    let csv = csv_of(
        &dir,
        &mut a,
        json!({ "columns": ["subject"], "sort": "subject", "descending": true }),
    );
    let rows = data_rows(&csv);
    let mut sorted = rows.clone();
    sorted.sort();
    sorted.reverse();
    assert_eq!(rows, sorted, "{csv}");
    let csv = csv_of(
        &dir,
        &mut a,
        json!({ "columns": ["subject"], "filters": { "subject": ["Pipe"] } }),
    );
    assert_eq!(data_rows(&csv).len(), 1, "{csv}");
    let csv = csv_of(
        &dir,
        &mut a,
        json!({ "columns": ["subject", "measurement"], "group_by": ["subject"] }),
    );
    assert!(csv.contains("Duct") && csv.contains("Floor"), "{csv}");
}

/// M-111 / M-112 / M-113 / M-115: Number and Choice (with values) columns feed a Formula that
/// uses operators, functions and constants; a Currency display.
#[test]
fn custom_columns_number_choice_formula_currency() {
    let (dir, ids) = takeoff_file("m-cols");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "takeoff.pdf" }));
    call(
        &mut a,
        "columns_set",
        json!({ "columns": [
            { "id": "rate", "name": "Rate", "type": "Number", "decimals": 2 },
            { "id": "mat", "name": "Material", "type": "Choice", "items": [ { "text": "Steel", "value": 12.5 }, { "text": "PVC", "value": 3 } ] },
            { "id": "cost", "name": "Cost", "type": "Formula", "formula": "Measurement * Material + Rate", "display": "Currency", "symbol": "$", "decimals": 2 },
            { "id": "math", "name": "Math", "type": "Formula", "formula": "round(2 ^ 3 + 10 % 4 + sqrt(16) + floor(pi), 0)" },
            { "id": "price", "name": "Price", "type": "Currency", "symbol": "$", "decimals": 2 }
        ] }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": ids[0], "column": "c:rate", "value": "5" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": ids[0], "column": "c:mat", "value": "Steel" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": ids[0], "column": "c:price", "value": "1234.5" }),
    );
    // A number column refuses text.
    fails(
        &mut a,
        "list_cell_set",
        json!({ "id": ids[0], "column": "c:rate", "value": "lots" }),
    );
    let csv = csv_of(
        &dir,
        &mut a,
        json!({ "columns": ["subject", "c:rate", "c:mat", "c:cost", "c:math", "c:price"] }),
    );
    let pipe = csv.lines().find(|l| l.contains("Pipe")).unwrap();
    // 10 ft x 12.5 + 5 = 130; 8 + 2 + 4 + 3 = 17.
    assert!(pipe.contains("130"), "cost: {pipe}");
    assert!(
        pipe.contains("$130.00") || pipe.contains("$ 130.00"),
        "currency display: {pipe}"
    );
    assert!(pipe.contains("17"), "math: {pipe}");
    assert!(
        pipe.contains("$1,234.50") || pipe.contains("$ 1,234.50"),
        "currency: {pipe}"
    );
}

/// M-114 / M-117: Text, Date and Checkmark columns; review statuses, filterable.
#[test]
fn custom_text_date_checkmark_and_statuses() {
    let (dir, ids) = takeoff_file("m-cols2");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "takeoff.pdf" }));
    call(
        &mut a,
        "columns_set",
        json!({ "columns": [
            { "id": "note", "name": "Note", "type": "Text" },
            { "id": "due", "name": "Due", "type": "Date" },
            { "id": "ok", "name": "Verified", "type": "Checkmark" }
        ] }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": ids[1], "column": "c:note", "value": "Main trunk" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": ids[1], "column": "c:due", "value": "2026-11-02" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": ids[1], "column": "c:ok", "value": "true" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": ids[1], "column": "status", "value": "Accepted" }),
    );
    let ms = save_reopen(&mut a, "cols2.pdf");
    let m = ms.iter().find(|m| m["id"] == ids[1].as_str()).unwrap();
    assert_eq!(m["columns"]["note"], "Main trunk", "{m}");
    assert_eq!(m["status"], "Accepted");
    let csv = csv_of(
        &dir,
        &mut a,
        json!({ "columns": ["subject", "status", "c:due", "c:ok"], "filters": { "status": ["Accepted"] } }),
    );
    assert_eq!(data_rows(&csv).len(), 1, "{csv}");
    assert!(csv.contains("Duct") && csv.contains("2026"), "{csv}");
}

// ---------------------------------------------------------------- Spaces

/// M-118 / M-119 / M-120: markups inside a named space get its name, even when drawn before
/// the space; the list groups and totals by space.
#[test]
fn spaces_name_and_total_their_markups() {
    let (dir, ids) = takeoff_file("m-spaces");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "takeoff.pdf" }));
    // Drawn before the space existed: the Floor (100..190) and the Pipe (y 300).
    call(
        &mut a,
        "space_add",
        json!({ "page": 1, "name": "Room 101", "points": [[50, 50], [250, 50], [250, 350], [50, 350]] }),
    );
    let l = call(&mut a, "space_list", json!({ "page": 1 }));
    let s = l.to_string();
    assert!(s.contains(&ids[0]) && s.contains(&ids[2]), "members: {s}");
    let csv = csv_of(
        &dir,
        &mut a,
        json!({ "columns": ["subject", "space", "measurement"], "group_by": ["space"] }),
    );
    assert!(
        csv.lines().any(|l| l.contains("Floor") && l.contains("Room 101")),
        "{csv}"
    );
    assert!(
        csv.lines().any(|l| l.contains("Duct") && !l.contains("Room 101")),
        "{csv}"
    );
    // Editing the outline re-assigns membership.
    let sid = l["spaces"][0]["id"].as_str().unwrap().to_string();
    call(
        &mut a,
        "space_edit",
        json!({ "id": sid, "points": [[50, 380], [250, 380], [250, 520], [50, 520]] }),
    );
    let s = call(&mut a, "space_list", json!({ "page": 1 })).to_string();
    assert!(s.contains(&ids[1]) && !s.contains(&ids[2]), "{s}");
}

// ---------------------------------------------------------------- Legends

/// M-122 / M-124 / M-125 / M-126 / M-127: a legend tied to a tool set lists its subjects with
/// live counts, takes scope, columns and appearance, copies to every page and freezes.
#[test]
fn legends_from_tool_sets_scope_columns_look_and_copies() {
    let (dir, _ids) = takeoff_file("m-legend");
    // A tool set with a Pipe and a Sprinkler tool, as the Tool Chest exports it.
    let mut chest = markupcraft_ui_egui::chest::ToolChest::default();
    let set = chest.add_set("Plumbing");
    for subject in ["Pipe", "Sprinkler"] {
        let mut m = markupcraft_model::Markup::new(Kind::Length, 0, vec![Point::new(0.0, 0.0), Point::new(9.0, 0.0)]);
        m.subject = subject.into();
        chest.add_markup(&set, &m).unwrap();
    }
    std::fs::write(dir.join("plumbing.mctools"), chest.export_set(&set).unwrap()).unwrap();
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "takeoff.pdf" }));
    call(&mut a, "page_insert_blank", json!({ "at": 2, "count": 1 }));
    let v = call(
        &mut a,
        "legend_from_toolset",
        json!({ "page": 1, "at": [300, 700], "path": "plumbing.mctools" }),
    );
    let lid = v["id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| v["legend"]["id"].as_str().unwrap().to_string());
    let l = call(&mut a, "legend_list", json!({})).to_string();
    assert!(
        l.contains("Pipe") && l.contains("Sprinkler"),
        "every subject of the set: {l}"
    );
    // A new Sprinkler appears on the next update.
    add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[400, 400], [490, 400]], "subject": "Sprinkler" }),
    );
    call(&mut a, "legend_update", json!({}));
    let l = call(&mut a, "legend_list", json!({}));
    let rows = l.to_string();
    assert!(rows.contains("Sprinkler"), "{rows}");
    // Scope, columns and look.
    call(
        &mut a,
        "legend_update",
        json!({ "id": lid, "scope": "document", "title": "Plumbing takeoff", "header": false, "font_size": 9,
                "columns": ["symbol", "subject", "count", "total"], "fill_color": "#FFFFEE", "opacity": 0.8, "line_width": 2, "symbol_scale": 1.5 }),
    );
    let l = call(&mut a, "legend_list", json!({})).to_string();
    assert!(l.contains("Plumbing takeoff"), "{l}");
    // Copy to every page, then a static snapshot.
    let before = call(&mut a, "markup_list", json!({}))["count"].as_u64().unwrap();
    call(&mut a, "legend_copy", json!({ "id": lid }));
    assert!(call(&mut a, "markup_list", json!({}))["count"].as_u64().unwrap() > before);
    call(&mut a, "legend_freeze", json!({ "id": lid }));
}

/// M-123: a legend of a chosen selection of markups only.
#[test]
fn ad_hoc_legend_of_a_selection() {
    let (dir, ids) = takeoff_file("m-legend2");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "takeoff.pdf" }));
    call(
        &mut a,
        "legend_add",
        json!({ "page": 1, "at": [300, 700], "ids": [ids[0], ids[2]] }),
    );
    let l = call(&mut a, "legend_list", json!({})).to_string();
    assert!(l.contains("Pipe") && l.contains("Floor"), "{l}");
    assert!(
        !l.contains("Concrete") && !l.contains("Duct"),
        "only the selection: {l}"
    );
}

// ---------------------------------------------------------------- Export, reporting, Excel

/// M-128 / M-129 / M-131: the Markup Summary as CSV, XML, Excel and a PDF report, filtered and
/// sorted.
#[test]
fn markup_summary_formats_filters_and_sort() {
    let (dir, _ids) = takeoff_file("m-summary");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "takeoff.pdf" }));
    call(
        &mut a,
        "summary_export",
        json!({ "out": "s.csv", "measurements_only": true }),
    );
    let csv = std::fs::read_to_string(dir.join("s.csv")).unwrap();
    assert!(csv.contains("Pipe") && csv.contains("100 sf"), "{csv}");
    call(&mut a, "summary_export", json!({ "out": "s.xml" }));
    let xml = std::fs::read_to_string(dir.join("s.xml")).unwrap();
    assert!(xml.contains("<") && xml.contains("Pipe"), "{xml}");
    call(&mut a, "summary_export", json!({ "out": "s.xlsx" }));
    assert!(std::fs::read(dir.join("s.xlsx")).unwrap().starts_with(b"PK"));
    call(
        &mut a,
        "summary_export",
        json!({ "out": "s.pdf", "title": "Takeoff", "pdf_thumbnails": 64, "pdf_page_content": true }),
    );
    assert!(std::fs::read(dir.join("s.pdf")).unwrap().starts_with(b"%PDF"));
    // Filter by author and a two-level sort.
    let csv = csv_of(
        &dir,
        &mut a,
        json!({ "columns": ["type", "subject"], "sort": "type", "then_by": [["subject", true]], "filters": { "type": ["Length", "Polylength"] } }),
    );
    let rows = data_rows(&csv);
    assert_eq!(rows.len(), 2, "{csv}");
    assert!(rows[0].contains("Length") && rows[1].contains("Polylength"), "{csv}");
}

/// M-130: one summary across a folder of PDFs, with a File column.
#[test]
fn summary_across_a_folder() {
    let (dir, _ids) = takeoff_file("m-batch");
    let sub = dir.join("more");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::copy(dir.join("takeoff.pdf"), sub.join("second.pdf")).unwrap();
    let mut a = automation(&dir);
    let v = call(
        &mut a,
        "batch_summary",
        json!({ "folder": ".", "recursive": true, "measurements_only": true }),
    );
    let s = v.to_string();
    assert!(s.contains("second.pdf") && s.contains("takeoff.pdf"), "{s}");
}

/// M-132 / M-133 / M-134: Quantity Link binds workbook cells to filtered totals of length,
/// area, volume, count or a custom column, and updates when the markups change.
#[test]
fn quantity_links_to_filtered_totals() {
    let (dir, ids) = takeoff_file("m-qlink");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "takeoff.pdf" }));
    call(
        &mut a,
        "columns_set",
        json!({ "columns": [{ "id": "hrs", "name": "Hours", "type": "Number" }] }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": ids[2], "column": "c:hrs", "value": "6" }),
    );
    add(
        &mut a,
        json!({ "page": 1, "kind": "Count", "points": [[500, 500], [520, 500], [540, 500]], "subject": "Head" }),
    );
    call(&mut a, "doc_save", json!({}));
    let link = |a: &mut Automation, name: &str, cell: &str, measure: &str, subjects: Value| {
        call(
            a,
            "quantity_link",
            json!({ "action": "save", "path": "links.json", "name": name, "sheet": "Takeoff", "cell": cell,
                    "files": ["takeoff.pdf"], "measure": measure, "subjects": subjects }),
        );
    };
    link(&mut a, "pipe", "B2", "length", json!(["Pipe"]));
    link(&mut a, "floor", "B3", "area", json!(["Floor"]));
    link(&mut a, "slab", "B4", "volume", json!([]));
    link(&mut a, "heads", "B5", "count", json!(["Head"]));
    link(&mut a, "hours", "B6", "column:hrs", json!([]));
    let l = call(
        &mut a,
        "quantity_link",
        json!({ "action": "list", "path": "links.json" }),
    );
    let total = |name: &str| {
        l["links"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == name)
            .map(|x| x["value"].as_f64().unwrap_or(f64::NAN))
    };
    assert!(near(total("pipe").unwrap(), 10.0, 1e-6), "{l}");
    assert!(near(total("floor").unwrap(), 100.0, 1e-6), "{l}");
    assert!(near(total("slab").unwrap(), 800.0, 1e-6), "{l}");
    assert!(near(total("heads").unwrap(), 3.0, 1e-6), "{l}");
    assert!(near(total("hours").unwrap(), 6.0, 1e-6), "{l}");
    call(
        &mut a,
        "quantity_link",
        json!({ "action": "update", "path": "links.json", "out": "takeoff.xlsx" }),
    );
    assert!(std::fs::read(dir.join("takeoff.xlsx")).unwrap().starts_with(b"PK"));
    // The markups change: the link follows on the next update.
    add(
        &mut a,
        json!({ "page": 1, "kind": "Length", "points": [[100, 600], [145, 600]], "subject": "Pipe" }),
    );
    call(&mut a, "doc_save", json!({}));
    let l = call(
        &mut a,
        "quantity_link",
        json!({ "action": "list", "path": "links.json" }),
    );
    let pipe = l["links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "pipe")
        .unwrap();
    assert!(near(pipe["value"].as_f64().unwrap(), 15.0, 1e-6), "{l}");
}
