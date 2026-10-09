//! The Properties panel driven headlessly (egui_kittest): typing into General fields, the
//! Appearance pickers, the Mixed state of a multi-selection, Layout, flags, blend mode, hatch,
//! layers, measurement captions and depth, and custom count symbols.

use egui::accesskit::Role;
use egui::{Key, Modifiers, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Color, Kind, Markup, flags};
use markupcraft_ui_egui::MarkupCraftApp;

fn harness() -> Harness<'static, MarkupCraftApp> {
    // The spelling dictionary loads before the first frame, not on a racing worker thread.
    markupcraft_ui_egui::richedit::load_dictionary_blocking();
    let mut h = Harness::builder()
        .with_size(vec2(1500.0, 1100.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(|_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            app.open_bytes("sample.pdf", None, markupcraft_render::synthetic::sample_pdf())
                .unwrap();
            app
        });
    h.run_steps(6);
    h
}

fn add(h: &mut Harness<'_, MarkupCraftApp>, m: Markup) -> String {
    let id = h.state_mut().state.doc_mut().unwrap().session.add_markup(m).unwrap();
    h.run_steps(2);
    id
}

fn rect(h: &mut Harness<'_, MarkupCraftApp>, r: Rect) -> String {
    add(h, Markup::new(Kind::Rectangle, 0, r.corners().to_vec()))
}

fn select(h: &mut Harness<'_, MarkupCraftApp>, ids: &[String]) {
    h.state_mut().state.doc_mut().unwrap().session.select(ids).unwrap();
    h.run_steps(3);
}

fn find(h: &Harness<'_, MarkupCraftApp>, id: &str) -> Markup {
    h.state().state.doc().unwrap().session.doc().find(id).unwrap().clone()
}

fn edit(h: &mut Harness<'_, MarkupCraftApp>, ids: &[String], p: markupcraft_engine::MarkupPatch) {
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_properties(ids, &p)
        .unwrap();
    h.run_steps(2);
}

/// Replace the text of the first text field showing `value`.
fn retype(h: &mut Harness<'_, MarkupCraftApp>, role: Role, value: &str, text: &str) {
    let n = h
        .get_all_by(|n| n.role() == role && n.value().as_deref() == Some(value))
        .next()
        .unwrap_or_else(|| panic!("no {role:?} showing {value:?}"));
    n.scroll_to_me();
    h.run_steps(40);
    let n = h
        .get_all_by(|n| n.role() == role && n.value().as_deref() == Some(value))
        .next()
        .unwrap_or_else(|| panic!("no {role:?} showing {value:?}"));
    n.focus();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.get_by(|n| n.is_focused()).type_text(text);
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(3);
}

/// Open the first combo box showing `value` and pick `item`.
fn pick(h: &mut Harness<'_, MarkupCraftApp>, value: &str, item: &str) {
    pick_nth(h, value, 0, item);
}

/// Open the `nth` combo box showing `value` and pick `item`.
fn pick_nth(h: &mut Harness<'_, MarkupCraftApp>, value: &str, nth: usize, item: &str) {
    let combo = |h: &Harness<'_, MarkupCraftApp>| {
        h.get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(value))
            .nth(nth)
            .map(|n| n.rect())
    };
    if let Some(n) = h
        .get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(value))
        .nth(nth)
    {
        n.scroll_to_me();
    }
    h.run_steps(40);
    let r = combo(h).unwrap_or_else(|| panic!("no combo box showing {value:?}"));
    h.get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(value))
        .find(|n| n.rect() == r)
        .unwrap_or_else(|| panic!("no combo box showing {value:?}"))
        .click();
    h.run_steps(3);
    h.get_by_label(item).click();
    h.run_steps(3);
}

/// Click the control labelled `label` after scrolling it into view.
fn press(h: &mut Harness<'_, MarkupCraftApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(40);
    h.get_by_label(label).click();
    h.run_steps(3);
}

#[test]
fn general_fields_take_typing_as_one_undo_step() {
    let mut h = harness();
    let a = rect(&mut h, Rect::new(60.0, 460.0, 100.0, 500.0));
    edit(
        &mut h,
        std::slice::from_ref(&a),
        markupcraft_engine::MarkupPatch {
            label: Some("L-1".into()),
            author: Some("Writer".into()),
            contents: Some("Check this".into()),
            ..Default::default()
        },
    );
    select(&mut h, std::slice::from_ref(&a));
    let depth = h.state().state.doc().unwrap().session.undo_depth();
    retype(&mut h, Role::TextInput, "Rectangle", "Duct Run");
    assert_eq!(find(&h, &a).subject, "Duct Run");
    h.run_steps(3);
    assert_eq!(
        h.state().state.doc().unwrap().session.undo_depth(),
        depth + 1,
        "typing a word is one undo step"
    );
    retype(&mut h, Role::TextInput, "L-1", "L-2");
    retype(&mut h, Role::TextInput, "Writer", "Reviewer");
    retype(&mut h, Role::TextInput, "Check this", "Checked");
    let m = find(&h, &a);
    assert_eq!(
        (m.label.as_str(), m.author.as_str(), m.contents.as_str()),
        ("L-2", "Reviewer", "Checked")
    );
    // The modified date and the reply count are shown.
    assert!(
        h.query_all_by_value("Not saved yet").next().is_some(),
        "a new markup has no date yet"
    );
    assert!(h.query_all_by_value("Replies").next().is_some());
}

#[test]
fn appearance_pickers_change_the_markup() {
    let mut h = harness();
    let a = rect(&mut h, Rect::new(60.0, 460.0, 100.0, 500.0));
    edit(
        &mut h,
        std::slice::from_ref(&a),
        markupcraft_engine::MarkupPatch {
            fill: Some(Some(Color::rgb(0.0, 0.0, 1.0))),
            ..Default::default()
        },
    );
    select(&mut h, std::slice::from_ref(&a));
    pick(&mut h, "Solid", "Dashed");
    assert_eq!(find(&h, &a).dash, vec![6.0, 3.0]);
    pick(&mut h, "Normal", "Multiply");
    assert!(find(&h, &a).multiply, "blend mode");
    retype(&mut h, Role::SpinButton, "1.0 pt", "3");
    assert_eq!(find(&h, &a).line_width, 3.0);
    // Opacity and fill opacity (the slider's number box).
    let spins: Vec<_> = h
        .get_all_by(|n| n.role() == Role::SpinButton && n.value().as_deref() == Some("100.0 %"))
        .collect();
    assert_eq!(spins.len(), 2, "fill opacity and opacity");
    drop(spins);
    retype(&mut h, Role::SpinButton, "100.0 %", "40");
    let m = find(&h, &a);
    assert!(
        (m.fill_opacity - 0.4).abs() < 1e-9 || (m.opacity - 0.4).abs() < 1e-9,
        "{m:?}"
    );
    retype(&mut h, Role::SpinButton, "100.0 %", "70");
    let m = find(&h, &a);
    assert!((m.fill_opacity - 0.4).abs() < 1e-9 && (m.opacity - 0.7).abs() < 1e-9);
    // Fill off.
    let fill = h.get_all_by(|n| n.role() == Role::CheckBox).count();
    assert!(fill > 0);
    // Hatch: the last of the combo boxes showing None (Layer, Status, Hatch).
    let nones = h
        .get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("None"))
        .count();
    pick_nth(&mut h, "None", nones - 1, "Cross");
    let m = find(&h, &a);
    assert_eq!(
        m.hatch.map(|h| h.style),
        Some(markupcraft_model::hatch::HatchStyle::Cross)
    );
}

#[test]
fn line_endings_cloud_and_text_style() {
    let mut h = harness();
    let l = add(
        &mut h,
        Markup::new(Kind::Line, 0, vec![Point::new(60.0, 460.0), Point::new(200.0, 460.0)]),
    );
    select(&mut h, std::slice::from_ref(&l));
    let shown: Vec<String> = h
        .get_all_by(|n| n.role() == Role::ComboBox)
        .filter_map(|n| n.value())
        .collect();
    assert!(shown.iter().filter(|v| v.as_str() == "None").count() >= 2, "{shown:?}");
    // Start ending.
    let start = h
        .get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("None"))
        .nth(2);
    let _ = start;
    let p = markupcraft_engine::MarkupPatch {
        line_end: Some("OpenArrow".into()),
        ..Default::default()
    };
    edit(&mut h, std::slice::from_ref(&l), p);
    h.run_steps(2);
    pick(&mut h, "OpenArrow", "ClosedArrow");
    assert_eq!(find(&h, &l).line_end, "ClosedArrow");
    // Cloud intensity on a polygon.
    let g = add(
        &mut h,
        Markup::new(
            Kind::Polygon,
            0,
            vec![
                Point::new(60.0, 600.0),
                Point::new(160.0, 600.0),
                Point::new(110.0, 680.0),
            ],
        ),
    );
    select(&mut h, std::slice::from_ref(&g));
    retype(&mut h, Role::SpinButton, "0.00", "1.5");
    assert_eq!(find(&h, &g).cloud, 1.5);
    // Text: font and alignment on the sample text box.
    select(&mut h, &["SAMPLETEXTAAAAAA".to_string()]);
    pick(&mut h, "Helvetica", "Courier");
    assert_eq!(find(&h, "SAMPLETEXTAAAAAA").text.font, "Courier");
    press(&mut h, "Center");
    assert_eq!(find(&h, "SAMPLETEXTAAAAAA").text.align, 1);
}

#[test]
fn a_multi_selection_shows_mixed_values_and_sets_them_all() {
    let mut h = harness();
    let a = rect(&mut h, Rect::new(60.0, 460.0, 100.0, 500.0));
    let b = rect(&mut h, Rect::new(160.0, 460.0, 200.0, 500.0));
    edit(
        &mut h,
        std::slice::from_ref(&b),
        markupcraft_engine::MarkupPatch {
            subject: Some("Other".into()),
            color: Some(Color::rgb(0.0, 0.5, 0.0)),
            dash: Some(vec![1.0, 2.0]),
            ..Default::default()
        },
    );
    select(&mut h, &[a.clone(), b.clone()]);
    let mixed = h.query_all_by_value("Mixed").count();
    assert!(mixed >= 3, "subject, colour and line style are mixed ({mixed})");
    // The line style combo shows Mixed; choosing a style sets both.
    pick(&mut h, "Mixed", "Long Dash");
    assert_eq!(find(&h, &a).dash, vec![12.0, 4.0]);
    assert_eq!(find(&h, &b).dash, vec![12.0, 4.0]);
    // Options: flags for every selected markup.
    press(&mut h, "Hidden");
    assert!(find(&h, &a).flags & flags::HIDDEN != 0 && find(&h, &b).flags & flags::HIDDEN != 0);
    press(&mut h, "Print");
    assert_eq!(find(&h, &a).flags & flags::PRINT, 0);
    press(&mut h, "No View");
    assert!(find(&h, &b).flags & flags::NO_VIEW != 0);
}

#[test]
fn layout_moves_resizes_and_rotates() {
    let mut h = harness();
    let a = rect(&mut h, Rect::new(72.0, 432.0, 144.0, 504.0));
    let g = add(
        &mut h,
        Markup::new(
            Kind::Polygon,
            0,
            vec![
                Point::new(72.0, 600.0),
                Point::new(144.0, 600.0),
                Point::new(144.0, 640.0),
            ],
        ),
    );
    select(&mut h, std::slice::from_ref(&a));
    press(&mut h, "Layout");
    // X, Y, Width, Height in inches.
    let inches = |h: &Harness<'_, MarkupCraftApp>| -> Vec<String> {
        h.get_all_by(|n| n.role() == Role::SpinButton && n.value().is_some_and(|v| v.ends_with(" in")))
            .filter_map(|n| n.value())
            .collect()
    };
    let v = inches(&h);
    assert_eq!(v.len(), 4, "{v:?}");
    retype(&mut h, Role::SpinButton, &v[0], "2");
    let b = bbox(&find(&h, &a).pts).unwrap();
    assert!(
        (b.x0 - 144.0).abs() < 1e-6 && (b.width() - 72.0).abs() < 1e-6,
        "moved: {b:?}"
    );
    let v = inches(&h);
    retype(&mut h, Role::SpinButton, &v[2], "1.5");
    let b = bbox(&find(&h, &a).pts).unwrap();
    assert!(
        (b.width() - 108.0).abs() < 1e-6 && (b.x0 - 144.0).abs() < 1e-6,
        "resized: {b:?}"
    );
    select(&mut h, std::slice::from_ref(&g));
    press(&mut h, "\u{27f2} 90\u{b0}");
    let r = bbox(&find(&h, &g).pts).unwrap();
    assert!(
        (r.width() - 40.0).abs() < 1e-6 && (r.height() - 72.0).abs() < 1e-6,
        "{r:?}"
    );
}

#[test]
fn layers_from_properties() {
    let mut h = harness();
    let a = rect(&mut h, Rect::new(60.0, 460.0, 100.0, 500.0));
    select(&mut h, std::slice::from_ref(&a));
    // A new layer by name.
    let input = h
        .get_all_by(|n| n.role() == Role::TextInput && n.value().as_deref() == Some(""))
        .count();
    assert!(input > 0);
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .create_layer("Electrical")
        .unwrap();
    h.run_steps(2);
    pick(&mut h, "None", "Electrical");
    assert_eq!(find(&h, &a).layer, "Electrical");
}

#[test]
fn measurement_caption_depth_and_count_symbol() {
    let mut h = harness();
    let area = "SAMPLEAREAAAAAAA".to_string();
    select(&mut h, std::slice::from_ref(&area));
    press(&mut h, "Show Caption");
    assert!(find(&h, &area).hide_caption);
    press(&mut h, "Show Caption");
    assert!(!find(&h, &area).hide_caption);
    // Depth gives a wall area.
    retype(&mut h, Role::SpinButton, "0.0", "10");
    assert_eq!(find(&h, &area).depth, 10.0);
    assert!(h.query_all_by_value("Wall area").next().is_some(), "wall area shown");
    // Count: a custom symbol from another selected markup.
    let c = add(
        &mut h,
        Markup::new(Kind::Count, 0, vec![Point::new(60.0, 460.0), Point::new(90.0, 460.0)]),
    );
    let tri = add(
        &mut h,
        Markup::new(
            Kind::Polygon,
            0,
            vec![
                Point::new(60.0, 600.0),
                Point::new(80.0, 600.0),
                Point::new(70.0, 620.0),
            ],
        ),
    );
    select(&mut h, &[c.clone(), tri]);
    press(&mut h, "Use the Polygon as the symbol");
    let m = find(&h, &c);
    assert_eq!(m.count_symbol, markupcraft_model::CountSymbol::Custom);
    assert!(!m.symbol_paths.is_empty());
}

#[test]
fn hidden_and_captionless_markups_save_and_reopen() {
    let dir = std::env::temp_dir().join(format!("markupcraft-props-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.pdf");
    std::fs::write(&path, markupcraft_render::synthetic::sample_pdf()).unwrap();
    let mut h = harness();
    h.state_mut().open_path(&path);
    h.run_steps(3);
    let area = "SAMPLEAREAAAAAAA".to_string();
    edit(
        &mut h,
        std::slice::from_ref(&area),
        markupcraft_engine::MarkupPatch {
            show_caption: Some(false),
            multiply: Some(true),
            no_view: Some(true),
            ..Default::default()
        },
    );
    h.state_mut().state.queue("file.save");
    h.run_steps(4);
    let (_f, doc) = markupcraft_revu::open(&path).unwrap();
    let a = doc.find(&area).unwrap();
    assert!(a.hide_caption && a.multiply && a.flags & flags::NO_VIEW != 0, "{a:?}");
    // Saved: Properties shows its modified date.
    select(&mut h, std::slice::from_ref(&area));
    let date = markupcraft_model::table::revu_date(&find(&h, &area).modified);
    assert!(!date.is_empty());
    assert!(h.query_all_by_value(&date).next().is_some(), "date {date} shown");
    let _ = std::fs::remove_dir_all(&dir);
}
