//! More markup and takeoff tools driven through the real shell headlessly (egui_kittest):
//! Dimension, Arc, Squiggly, Flag, Insert / Replace Text, the Eraser, Lasso and Select Text,
//! the Measure tool, Count series from the canvas menu, arcs and cutouts from the canvas menu,
//! Review Text, Import / Export Markups and the caption, slope and centroid properties.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_geom::{Point, Rect};
use markupcraft_model::{Kind, Markup};
use markupcraft_ui_egui::MarkupCraftApp;

const AREA: &str = "SAMPLEAREAAAAAAA";

fn harness() -> Harness<'static, MarkupCraftApp> {
    let mut h = Harness::builder()
        .with_size(vec2(1500.0, 950.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(|_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            app.open_bytes("sample.pdf", None, markupcraft_render::synthetic::sample_pdf())
                .unwrap();
            // These tests edit through the Properties panel: show it in the left panel area
            // (the default layout shows Thumbnails there).
            app.state.show_panel("properties");
            app
        });
    h.run_steps(6);
    h
}

fn screen(h: &Harness<'_, MarkupCraftApp>, x: f64, y: f64) -> Pos2 {
    let d = h.state().state.doc().unwrap();
    d.view
        .user_to_screen(0, Point::new(x, y), d.render.as_ref().unwrap().pages())
        .unwrap()
}

fn button(h: &mut Harness<'_, MarkupCraftApp>, at: Pos2, pressed: bool, b: PointerButton, m: Modifiers) {
    h.event(Event::PointerButton {
        pos: at,
        button: b,
        pressed,
        modifiers: m,
    });
    h.step();
}

fn click(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.step();
    button(h, at, true, PointerButton::Primary, Modifiers::NONE);
    button(h, at, false, PointerButton::Primary, Modifiers::NONE);
    h.run_steps(2);
}

fn right_click(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.step();
    button(h, at, true, PointerButton::Secondary, Modifiers::NONE);
    button(h, at, false, PointerButton::Secondary, Modifiers::NONE);
    h.run_steps(2);
}

/// Press at the first point, move through the others, release at the last.
fn drag_path(h: &mut Harness<'_, MarkupCraftApp>, pts: &[(f64, f64)], m: Modifiers) {
    let s: Vec<Pos2> = pts.iter().map(|(x, y)| screen(h, *x, *y)).collect();
    let (Some(first), Some(last)) = (s.first().copied(), s.last().copied()) else {
        return;
    };
    h.hover_at(first);
    h.step();
    h.event(Event::ModifiersChanged(m));
    button(h, first, true, PointerButton::Primary, m);
    for w in s.windows(2) {
        for i in 1..=6 {
            h.hover_at(w[0] + (w[1] - w[0]) * (i as f32 / 6.0));
            h.step();
        }
    }
    button(h, last, false, PointerButton::Primary, m);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

fn drag(h: &mut Harness<'_, MarkupCraftApp>, from: (f64, f64), to: (f64, f64)) {
    drag_path(h, &[from, to], Modifiers::NONE);
}

fn key(h: &mut Harness<'_, MarkupCraftApp>, k: Key) {
    h.key_press(k);
    h.run_steps(2);
}

fn keys(h: &mut Harness<'_, MarkupCraftApp>, m: Modifiers, k: Key) {
    h.key_press_modifiers(m, k);
    h.run_steps(2);
}

fn type_text(h: &mut Harness<'_, MarkupCraftApp>, text: &str) {
    h.event(Event::Text(text.into()));
    h.run_steps(2);
}

fn tool(h: &mut Harness<'_, MarkupCraftApp>, id: &str) {
    h.state_mut().set_option("tool", id);
    h.run_steps(2);
    assert_eq!(h.state().state.tool, id);
}

fn markups(h: &Harness<'_, MarkupCraftApp>) -> Vec<Markup> {
    h.state().state.doc().unwrap().session.doc().markups.clone()
}

fn find(h: &Harness<'_, MarkupCraftApp>, id: &str) -> Markup {
    h.state().state.doc().unwrap().session.doc().find(id).unwrap().clone()
}

fn last(h: &Harness<'_, MarkupCraftApp>) -> Markup {
    markups(h).last().unwrap().clone()
}

fn status(h: &Harness<'_, MarkupCraftApp>) -> String {
    h.state().state.status.clone()
}

fn add(h: &mut Harness<'_, MarkupCraftApp>, m: Markup) -> String {
    let id = h.state_mut().state.doc_mut().unwrap().session.add_markup(m).unwrap();
    h.run_steps(2);
    id
}

fn select(h: &mut Harness<'_, MarkupCraftApp>, ids: &[String]) {
    h.state_mut().state.doc_mut().unwrap().session.select(ids).unwrap();
    h.run_steps(2);
}

fn run(h: &mut Harness<'_, MarkupCraftApp>, id: &str) {
    h.state_mut().state.queue(id);
    h.run_steps(3);
}

/// The rightmost widget with this label (a canvas menu opens over the page, right of the left
/// panel area).
fn menu_item(h: &Harness<'_, MarkupCraftApp>, label: &str) {
    let items: Vec<_> = h.get_all_by_label(label).collect();
    let item = items
        .iter()
        .max_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
        .unwrap_or_else(|| panic!("no {label} in the menu"));
    item.click();
}

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

#[test]
fn dimension_arc_squiggly_and_flag_tools_draw() {
    let mut h = harness();
    // Shift+L: Dimension, two clicks; its text is the scaled length (1/8" = 1').
    keys(&mut h, Modifiers::SHIFT, Key::L);
    assert_eq!(h.state().state.tool, "dimension");
    click(&mut h, 100.0, 500.0);
    click(&mut h, 190.0, 500.0);
    let d = last(&h);
    assert_eq!(d.kind, Kind::Dimension, "{}", status(&h));
    assert!(
        d.pts.len() == 2 && (d.pts[1].x - d.pts[0].x - 90.0).abs() < 0.5,
        "{:?}",
        d.pts
    );
    assert!(d.contents.starts_with("10"), "{}", d.contents);
    assert_eq!((d.line_start.as_str(), d.line_end.as_str()), ("OpenArrow", "OpenArrow"));

    // Shift+C: Arc: start, end, then a point it passes through.
    keys(&mut h, Modifiers::SHIFT, Key::C);
    assert_eq!(h.state().state.tool, "arc");
    click(&mut h, 300.0, 450.0);
    click(&mut h, 400.0, 450.0);
    click(&mut h, 350.0, 490.0);
    let a = last(&h);
    assert_eq!(a.kind, Kind::Arc, "{}", status(&h));
    assert_eq!(a.pts.len(), 3);
    assert!(
        (a.pts[1].y - 490.0).abs() < 1.0,
        "the through point is in the middle: {:?}",
        a.pts
    );

    // Shift+U: Squiggly across KITCHEN.
    keys(&mut h, Modifiers::SHIFT, Key::U);
    assert_eq!(h.state().state.tool, "squiggly");
    drag(&mut h, (652.0, 645.0), (700.0, 645.0));
    let s = last(&h);
    assert_eq!(s.kind, Kind::Squiggly);
    assert_eq!(s.pts.len(), 4);

    // Shift+F: Flag.
    keys(&mut h, Modifiers::SHIFT, Key::F);
    assert_eq!(h.state().state.tool, "flag");
    click(&mut h, 500.0, 520.0);
    let f = last(&h);
    assert_eq!(
        (f.kind, f.icon.as_str(), f.subject.as_str()),
        (Kind::Note, "Flag", "Flag")
    );
}

#[test]
fn eraser_lasso_and_select_text_gestures() {
    let mut h = harness();
    let mut ink = Markup::new(
        Kind::Ink,
        0,
        (0..=20).map(|i| p(100.0 + f64::from(i) * 10.0, 520.0)).collect(),
    );
    ink.line_width = 1.5;
    let ink = add(&mut h, ink);
    // Shift+E: drag the eraser down across the middle of the stroke.
    keys(&mut h, Modifiers::SHIFT, Key::E);
    assert_eq!(h.state().state.tool, "eraser");
    drag(&mut h, (200.0, 545.0), (200.0, 495.0));
    let m = find(&h, &ink);
    assert_eq!(m.strokes.len(), 1, "the stroke is cut in two: {}", status(&h));
    assert!(m.pts.len() < 21);

    // Shift+O: a lasso loop around one rectangle selects it alone.
    let mut r = Markup::new(
        Kind::Rectangle,
        0,
        Rect::new(420.0, 440.0, 470.0, 480.0).corners().to_vec(),
    );
    r.rect = Rect::new(420.0, 440.0, 470.0, 480.0);
    let rect = add(&mut h, r);
    keys(&mut h, Modifiers::SHIFT, Key::O);
    assert_eq!(h.state().state.tool, "lasso");
    drag_path(
        &mut h,
        &[
            (410.0, 430.0),
            (480.0, 430.0),
            (490.0, 460.0),
            (480.0, 490.0),
            (410.0, 490.0),
            (405.0, 460.0),
        ],
        Modifiers::NONE,
    );
    assert_eq!(
        h.state().state.doc().unwrap().selection(),
        std::slice::from_ref(&rect),
        "{}",
        status(&h)
    );

    // Shift+T: Select Text copies the words dragged across.
    keys(&mut h, Modifiers::SHIFT, Key::T);
    assert_eq!(h.state().state.tool, "selecttext");
    drag(&mut h, (652.0, 645.0), (700.0, 645.0));
    assert!(status(&h).starts_with("Copied"), "{}", status(&h));
}

#[test]
fn insert_and_replace_text_add_carets() {
    let mut h = harness();
    tool(&mut h, "inserttext");
    click(&mut h, 610.0, 644.0);
    assert!(
        h.state().state.doc().unwrap().view.editor.is_some(),
        "the comment editor opens"
    );
    type_text(&mut h, "NEW");
    key(&mut h, Key::Escape);
    let c = last(&h);
    assert_eq!((c.kind, c.contents.as_str()), (Kind::Caret, "NEW"));

    // A drag across KITCHEN: strike it through and add the replacement caret, grouped.
    tool(&mut h, "inserttext");
    drag(&mut h, (652.0, 645.0), (700.0, 645.0));
    assert!(h.state().state.doc().unwrap().view.editor.is_some());
    type_text(&mut h, "GALLEY");
    key(&mut h, Key::Escape);
    let all = markups(&h);
    let (strike, caret) = (&all[all.len() - 2], &all[all.len() - 1]);
    assert_eq!(strike.kind, Kind::Strikeout);
    assert_eq!((caret.kind, caret.contents.as_str()), (Kind::Caret, "GALLEY"));
    assert!(
        !caret.group.is_empty() && caret.group == strike.group,
        "Replace groups them"
    );
}

#[test]
fn measure_tool_switches_modes_and_recalculates() {
    let mut h = harness();
    key(&mut h, Key::M);
    assert_eq!(h.state().state.tool, "length");
    assert!(h.state().state.open_panels.contains(&"measurements"));
    // The panel's mode buttons switch the measurement.
    // (the Measure toolbar has an Area button too; the panel is the rightmost)
    let items: Vec<_> = h.get_all_by_label("Area").collect();
    items
        .iter()
        .max_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
        .unwrap()
        .click();
    h.run_steps(3);
    assert_eq!(h.state().state.tool, "area");
    key(&mut h, Key::Escape);
    key(&mut h, Key::M);
    assert_eq!(h.state().state.tool, "area", "M comes back to the last mode");
    run(&mut h, "measure.recalculate");
    assert!(status(&h).starts_with("Recalculated"), "{}", status(&h));
}

#[test]
fn count_series_from_the_canvas_menu() {
    let mut h = harness();
    let c = add(
        &mut h,
        Markup::new(Kind::Count, 0, vec![p(100.0, 560.0), p(140.0, 560.0), p(180.0, 560.0)]),
    );
    // Right-click the middle item: Delete Count Item.
    right_click(&mut h, 140.0, 560.0);
    menu_item(&h, "Delete Count Item");
    h.run_steps(3);
    assert_eq!(find(&h, &c).pts.len(), 2, "{}", status(&h));
    // Resume Count: new clicks join the series.
    select(&mut h, std::slice::from_ref(&c));
    run(&mut h, "measure.resume_count");
    assert_eq!(h.state().state.tool, "count");
    click(&mut h, 220.0, 560.0);
    click(&mut h, 260.0, 560.0);
    key(&mut h, Key::Enter);
    assert_eq!(find(&h, &c).pts.len(), 4, "{}", status(&h));
    key(&mut h, Key::Escape);
    // Split Off This Item, then Merge Counts.
    right_click(&mut h, 260.0, 560.0);
    menu_item(&h, "Split Off This Item");
    h.run_steps(3);
    assert_eq!(find(&h, &c).pts.len(), 3);
    let counts: Vec<String> = markups(&h)
        .iter()
        .filter(|m| m.kind == Kind::Count)
        .map(|m| m.id.clone())
        .collect();
    assert_eq!(counts.len(), 2);
    select(&mut h, &counts);
    run(&mut h, "measure.merge_counts");
    assert_eq!(find(&h, &c).pts.len(), 4);
    assert_eq!(markups(&h).iter().filter(|m| m.kind == Kind::Count).count(), 1);
}

#[test]
fn convert_to_arc_and_cutout_to_measurement_from_the_menu() {
    let mut h = harness();
    let before = find(&h, AREA).quantity().unwrap();
    // Right-click the bottom edge of the sample area: Convert to Arc.
    right_click(&mut h, 450.0, 180.0);
    menu_item(&h, "Convert to Arc");
    h.run_steps(3);
    let a = find(&h, AREA);
    assert_eq!(a.arcs.len(), 1, "{}", status(&h));
    assert!(a.quantity().unwrap() > before, "the arc bulges outward");
    right_click(&mut h, 450.0, 180.0 - 75.0);
    menu_item(&h, "Convert to Line");
    h.run_steps(3);
    assert!(find(&h, AREA).arcs.is_empty(), "{}", status(&h));
    // A cutout made its own measurement.
    let ring = vec![p(320.0, 200.0), p(392.0, 200.0), p(392.0, 272.0), p(320.0, 272.0)];
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .add_cutout(AREA, ring)
        .unwrap();
    h.run_steps(2);
    right_click(&mut h, 356.0, 236.0);
    menu_item(&h, "Cutout to Measurement");
    h.run_steps(3);
    let n = last(&h);
    assert_eq!(n.kind, Kind::Area);
    assert_eq!(n.pts.len(), 4);
    assert_eq!(find(&h, AREA).holes.len(), 1);
}

#[test]
fn ctrl_drag_curves_a_segment_and_its_handle_bends_it() {
    let mut h = harness();
    let before = find(&h, AREA).quantity().unwrap();
    select(&mut h, &[AREA.to_string()]);
    // Ctrl+drag the top edge up: it becomes an arc through the pointer.
    drag_path(
        &mut h,
        &[(450.0, 350.0), (450.0, 380.0), (450.0, 400.0)],
        Modifiers::COMMAND,
    );
    let a = find(&h, AREA);
    assert_eq!(a.arcs.len(), 1, "{}", status(&h));
    let handle = markupcraft_model::measure_extras::arc_handle(&a, 0).unwrap();
    assert!(
        (a.pts[handle].y - 400.0).abs() < 2.0,
        "the arc passes the pointer: {:?}",
        a.pts[handle]
    );
    let bent = a.quantity().unwrap();
    assert!(bent > before);
    // Dragging its handle bends it further; the arc stays an arc.
    drag(&mut h, (450.0, a.pts[handle].y), (450.0, 430.0));
    let b = find(&h, AREA);
    assert_eq!(b.arcs.len(), 1);
    assert!(b.quantity().unwrap() > bent, "{}", status(&h));
}

#[test]
fn review_text_edits_comments_in_sequence() {
    let mut h = harness();
    let mut note = Markup::new(
        Kind::Note,
        0,
        Rect::new(1000.0, 500.0, 1024.0, 524.0).corners().to_vec(),
    );
    note.contents = "Second comment".into();
    add(&mut h, note);
    keys(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::R);
    assert!(h.state().state.edit.more.review_open);
    h.get_by_label("Next").click();
    h.run_steps(3);
    assert_eq!(h.state().state.edit.more.review_at, 1);
    h.get_by_label("Previous").click();
    h.run_steps(3);
    let text_id = markupcraft_ui_egui::more::review_rows(h.state().state.doc().unwrap().session.doc())[0].clone();
    assert_eq!(
        h.state().state.doc().unwrap().selection(),
        std::slice::from_ref(&text_id)
    );
    let before = find(&h, &text_id).contents;
    // Type into the first row.
    let field = h
        .get_all_by_role(egui::accesskit::Role::MultilineTextInput)
        .next()
        .expect("a text field per markup");
    field.click();
    h.run_steps(2);
    key(&mut h, Key::End);
    type_text(&mut h, " checked");
    assert_eq!(find(&h, &text_id).contents, format!("{before} checked"));
}

#[test]
fn import_and_export_markups_as_xfdf() {
    let mut h = harness();
    let out = std::env::temp_dir().join(format!("markupcraft-editing2-{}.xfdf", std::process::id()));
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    keys(&mut h, Modifiers::COMMAND, Key::F2);
    h.run_steps(3);
    assert!(status(&h).starts_with("Exported"), "{}", status(&h));
    assert!(std::fs::read_to_string(&out).unwrap().contains("xfdf"));
    // Delete everything, then Ctrl+F3 brings it back.
    let n = markups(&h).len();
    h.state_mut().state.doc_mut().unwrap().session.select_all(None);
    key(&mut h, Key::Delete);
    let left = markups(&h).len();
    assert!(left < n);
    keys(&mut h, Modifiers::COMMAND, Key::F3);
    h.run_steps(3);
    assert!(status(&h).starts_with("Imported"), "{}", status(&h));
    assert!(markups(&h).len() > left, "{}", status(&h));
    let _ = std::fs::remove_file(&out);
}

fn retype(h: &mut Harness<'_, MarkupCraftApp>, role: Role, value: &str, text: &str) {
    let shown: Vec<String> = h
        .query_all_by(|n| n.role() == role)
        .map(|n| n.value().unwrap_or_default())
        .collect();
    let n = h
        .query_all_by(|n| n.role() == role && n.value().as_deref() == Some(value))
        .next()
        .unwrap_or_else(|| panic!("no {role:?} showing {value:?} in {shown:?}"));
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

/// Open the combo box showing `value` and pick `item`.
fn pick(h: &mut Harness<'_, MarkupCraftApp>, value: &str, item: &str) {
    if let Some(n) = h
        .get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(value))
        .next()
    {
        n.scroll_to_me();
    }
    h.run_steps(40);
    h.get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(value))
        .next()
        .unwrap_or_else(|| panic!("no combo box showing {value:?}"))
        .click();
    h.run_steps(3);
    h.get_all_by_label(item)
        .last()
        .unwrap_or_else(|| panic!("no {item:?}"))
        .click();
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
fn caption_slope_centroid_and_units_from_properties() {
    let mut h = harness();
    select(&mut h, &[AREA.to_string()]);
    let plain = find(&h, AREA).quantity().unwrap();
    press(&mut h, "Show Caption Leader Line");
    press(&mut h, "Show Centroid");
    press(&mut h, "Show All Measurements");
    let a = find(&h, AREA);
    assert!(a.caption_leader && a.show_centroid, "{}", status(&h));
    assert_eq!(a.caption_template, "{all}");
    assert!(markupcraft_model::caption::caption_text(&a).starts_with("P: "));
    // Caption contents typed: fields fill in.
    retype(&mut h, Role::TextInput, "{all}", "{subject} = {value}");
    let a = find(&h, AREA);
    assert_eq!(a.caption_template, "{subject} = {value}");
    assert_eq!(
        markupcraft_model::caption::caption_text(&a),
        format!("Area = {}", a.quantity_text())
    );
    // A 4 in 12 pitch: the true sloped area.
    pick(&mut h, "No slope", "Pitch (in 12)");
    let a = find(&h, AREA);
    assert_eq!((a.slope_type, a.slope), (1, 4.0));
    let k = (1.0f64 + (4.0f64 / 12.0).powi(2)).sqrt();
    assert!((a.quantity().unwrap() - plain * k).abs() < 1e-6);
    // Its own area unit.
    pick(&mut h, "sf", "sy");
    assert!(
        find(&h, AREA).quantity_text().ends_with(" sy"),
        "{}",
        find(&h, AREA).quantity_text()
    );
}

#[test]
fn dimension_offset_and_measurement_rotation_snaps() {
    let mut h = harness();
    tool(&mut h, "dimension");
    click(&mut h, 100.0, 500.0);
    click(&mut h, 190.0, 500.0);
    let d = last(&h);
    select(&mut h, std::slice::from_ref(&d.id));
    retype(&mut h, Role::SpinButton, "0.0 pt", "24");
    assert_eq!(find(&h, &d.id).leader, 24.0, "{}", status(&h));

    // A measurement turned by its handle snaps to 15 degrees.
    let mut l = Markup::new(Kind::Length, 0, vec![p(100.0, 600.0), p(200.0, 600.0)]);
    l.scale = find(&h, AREA).scale;
    let id = add(&mut h, l);
    select(&mut h, std::slice::from_ref(&id));
    let handle = screen(&h, 150.0, 600.0) - vec2(0.0, 22.0);
    let ang = 70f64.to_radians();
    let to = screen(&h, 150.0 + 60.0 * ang.cos(), 600.0 + 60.0 * ang.sin());
    h.hover_at(handle);
    h.step();
    button(&mut h, handle, true, PointerButton::Primary, Modifiers::NONE);
    for i in 1..=8 {
        h.hover_at(handle + (to - handle) * (i as f32 / 8.0));
        h.step();
    }
    button(&mut h, to, false, PointerButton::Primary, Modifiers::NONE);
    h.run_steps(2);
    let m = find(&h, &id);
    let a = (m.pts[1].y - m.pts[0].y).atan2(m.pts[1].x - m.pts[0].x).to_degrees();
    assert!((a + 15.0).abs() < 0.01, "turned {a} degrees: {}", status(&h));
}

#[test]
fn cutout_vertices_drag_add_and_delete() {
    let mut h = harness();
    let ring = vec![p(320.0, 200.0), p(392.0, 200.0), p(392.0, 272.0), p(320.0, 272.0)];
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .add_cutout(AREA, ring)
        .unwrap();
    select(&mut h, &[AREA.to_string()]);
    let before = find(&h, AREA).quantity().unwrap();
    // Drag the cutout's top-right vertex outwards: the deduction grows.
    drag(&mut h, (392.0, 272.0), (410.0, 290.0));
    let a = find(&h, AREA);
    assert!(
        a.holes[0][2].dist(p(410.0, 290.0)) < 1.5,
        "{:?} {}",
        a.holes[0],
        status(&h)
    );
    assert!(a.quantity().unwrap() < before);
    // Right-click inside it: Add Cutout Vertex, then Delete Cutout Vertex.
    right_click(&mut h, 330.0, 205.0);
    menu_item(&h, "Add Cutout Vertex");
    h.run_steps(3);
    assert_eq!(find(&h, AREA).holes[0].len(), 5, "{}", status(&h));
    right_click(&mut h, 330.0, 205.0);
    menu_item(&h, "Delete Cutout Vertex");
    h.run_steps(3);
    assert_eq!(find(&h, AREA).holes[0].len(), 4, "{}", status(&h));
}

#[test]
fn temporary_measurements_and_keep_last_subject() {
    let mut h = harness();
    let n = markups(&h).len();
    run(&mut h, "measure.make_annotations");
    assert_eq!(h.state().state.checked("measure.make_annotations"), Some(false));
    tool(&mut h, "length");
    click(&mut h, 100.0, 500.0);
    click(&mut h, 190.0, 500.0);
    assert_eq!(markups(&h).len(), n, "nothing is added");
    assert!(status(&h).starts_with("Length (temporary): "), "{}", status(&h));
    run(&mut h, "measure.make_annotations");

    // Keep Last Subject and Label: the next Length takes the last one's.
    run(&mut h, "measure.keep_subject");
    tool(&mut h, "length");
    click(&mut h, 100.0, 520.0);
    click(&mut h, 190.0, 520.0);
    let first = last(&h);
    assert_eq!(first.kind, Kind::Length);
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_properties(
            std::slice::from_ref(&first.id),
            &markupcraft_engine::MarkupPatch {
                subject: Some("Duct run".into()),
                label: Some("D-1".into()),
                ..Default::default()
            },
        )
        .unwrap();
    select(&mut h, std::slice::from_ref(&first.id));
    tool(&mut h, "length");
    click(&mut h, 100.0, 540.0);
    click(&mut h, 190.0, 540.0);
    let next = last(&h);
    assert_ne!(next.id, first.id);
    assert_eq!((next.subject.as_str(), next.label.as_str()), ("Duct run", "D-1"));
}

#[test]
fn cloud_drags_a_rectangle_and_count_shows_a_live_total() {
    let mut h = harness();
    tool(&mut h, "cloud");
    drag(&mut h, (100.0, 480.0), (200.0, 540.0));
    let c = last(&h);
    assert_eq!(c.kind, Kind::Cloud, "{}", status(&h));
    assert_eq!(c.pts.len(), 4, "a rectangle cloud");
    // Clicks still make a polygon cloud.
    tool(&mut h, "cloud");
    click(&mut h, 300.0, 480.0);
    click(&mut h, 380.0, 480.0);
    click(&mut h, 340.0, 540.0);
    key(&mut h, Key::Enter);
    assert_eq!(last(&h).pts.len(), 3);
    // Counting: the running total sits in the lower-right corner.
    tool(&mut h, "count");
    click(&mut h, 100.0, 560.0);
    click(&mut h, 130.0, 560.0);
    key(&mut h, Key::Enter);
    assert_eq!(last(&h).pts.len(), 2);
}

#[test]
fn markups_list_row_menu_layer_legend_lock_and_delete() {
    let mut h = harness();
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .create_layer("Walls")
        .unwrap();
    h.run_steps(2);
    let rect = "SAMPLESQUAREAAAA".to_string();
    // The list row of the sample rectangle (its Subject cell, in the lower panel).
    let row = |h: &Harness<'_, MarkupCraftApp>| {
        h.get_all_by_label("Rectangle")
            .map(|n| n.rect())
            .filter(|r| r.top() > 650.0)
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .expect("the Rectangle row")
            .center()
    };
    let open_menu = |h: &mut Harness<'_, MarkupCraftApp>| {
        let at = row(h);
        h.hover_at(at);
        h.step();
        button(h, at, true, PointerButton::Secondary, Modifiers::NONE);
        button(h, at, false, PointerButton::Secondary, Modifiers::NONE);
        h.run_steps(3);
    };
    open_menu(&mut h);
    let lr = h.get_by_label("Layer \u{23f5}").rect();
    h.hover_at(lr.center());
    h.run_steps(10);
    h.run_steps(3);
    h.get_by_label("Walls").click();
    h.run_steps(3);
    assert_eq!(find(&h, &rect).layer, "Walls", "{}", status(&h));
    open_menu(&mut h);
    h.get_by_label("Create Legend").click();
    h.run_steps(3);
    let legends = h.state().state.doc().unwrap().session.legends();
    assert_eq!(legends.len(), 1, "{}", status(&h));
    assert_eq!(legends[0].options.subjects, vec!["Rectangle".to_string()]);
    select(&mut h, std::slice::from_ref(&rect));
    open_menu(&mut h);
    h.get_by_label("Lock").click();
    h.run_steps(3);
    assert!(find(&h, &rect).locked());
    open_menu(&mut h);
    h.get_by_label("Unlock").click();
    h.run_steps(3);
    assert!(!find(&h, &rect).locked());
    open_menu(&mut h);
    menu_item(&h, "Properties");
    h.run_steps(3);
    assert!(h.state().state.open_panels.contains(&"properties"));
    open_menu(&mut h);
    menu_item(&h, "Delete");
    h.run_steps(3);
    assert!(h.state().state.doc().unwrap().session.doc().find(&rect).is_none());
}

#[test]
fn perimeter_rise_drop_vertices_and_precision() {
    let mut h = harness();
    // Perimeter: clicks, then Enter.
    tool(&mut h, "perimeter");
    click(&mut h, 100.0, 480.0);
    click(&mut h, 190.0, 480.0);
    click(&mut h, 190.0, 540.0);
    key(&mut h, Key::Enter);
    let pm = last(&h);
    assert_eq!((pm.kind, pm.pts.len()), (Kind::Perimeter, 3));
    assert!(pm.quantity().unwrap() > 0.0);

    // Rise/Drop adds to a Polylength's run.
    tool(&mut h, "polylength");
    click(&mut h, 300.0, 480.0);
    click(&mut h, 390.0, 480.0);
    click(&mut h, 390.0, 540.0);
    key(&mut h, Key::Enter);
    let pl = last(&h);
    let run_len = pl.quantity().unwrap();
    select(&mut h, std::slice::from_ref(&pl.id));
    retype(&mut h, Role::SpinButton, "0.0", "5");
    let pl2 = find(&h, &pl.id);
    assert_eq!(pl2.rise_drop, 5.0, "{}", status(&h));
    assert!((pl2.quantity().unwrap() - run_len - 5.0).abs() < 1e-9);

    // Right-click the first segment: Add Vertex; right-click it: Delete Vertex.
    key(&mut h, Key::Escape);
    right_click(&mut h, 345.0, 480.0);
    menu_item(&h, "Add Vertex");
    h.run_steps(3);
    assert_eq!(find(&h, &pl.id).pts.len(), 4, "{}", status(&h));
    right_click(&mut h, 345.0, 480.0);
    menu_item(&h, "Delete Vertex");
    h.run_steps(3);
    assert_eq!(find(&h, &pl.id).pts.len(), 3, "{}", status(&h));

    // Precision from the Properties panel.
    select(&mut h, std::slice::from_ref(&pl.id));
    let sc = find(&h, &pl.id).scale.unwrap();
    let now = markupcraft_measure::units::scale_precision(&sc).name();
    let want = markupcraft_measure::units::Precision::decimals(0);
    pick(&mut h, &now, &want.name());
    let sc = find(&h, &pl.id).scale.unwrap();
    assert_eq!(markupcraft_measure::units::scale_precision(&sc), want);
}

#[test]
fn space_pans_while_drawing_and_autosize_shrinks_a_text_box() {
    let mut h = harness();
    tool(&mut h, "polyline");
    click(&mut h, 100.0, 480.0);
    click(&mut h, 190.0, 480.0);
    let before = h.state().state.doc().unwrap().view.offset;
    h.event(Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.step();
    drag(&mut h, (300.0, 300.0), (360.0, 330.0));
    h.event(Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    let d = h.state().state.doc().unwrap();
    assert_ne!(d.view.offset, before, "the view panned");
    assert_eq!(
        d.view.draft.as_ref().map(|d| d.pts.len()),
        Some(2),
        "the drawing went on"
    );
    click(&mut h, 190.0, 540.0);
    key(&mut h, Key::Enter);
    let pl = last(&h);
    assert_eq!((pl.kind, pl.pts.len()), (Kind::Polyline, 3));

    // Alt+Z: a text box far too big for its text shrinks to fit.
    let r = Rect::new(100.0, 300.0, 400.0, 450.0);
    let mut t = Markup::new(Kind::Text, 0, r.corners().to_vec());
    t.rect = r;
    t.contents = "Note".into();
    let id = add(&mut h, t);
    select(&mut h, std::slice::from_ref(&id));
    keys(&mut h, Modifiers::ALT, Key::Z);
    let b = markupcraft_geom::bbox(&find(&h, &id).pts).unwrap();
    assert!(b.width() < 300.0 && b.height() < 150.0, "{b:?}");
}

#[test]
fn hide_markups_hides_them_from_view_only() {
    let mut h = harness();
    let n = markups(&h).len();
    run(&mut h, "view.hide_markups");
    assert_eq!(h.state().state.checked("view.hide_markups"), Some(true));
    assert!(h.state().state.canvas_cx().hide_markups);
    // The view hides them; the document keeps them.
    assert_eq!(markups(&h).len(), n);
    assert!(!h.state().state.doc().unwrap().session.is_dirty());
    run(&mut h, "view.hide_markups");
    assert_eq!(h.state().state.checked("view.hide_markups"), Some(false));
}

fn rect_markup(r: Rect) -> Markup {
    let mut m = Markup::new(Kind::Rectangle, 0, r.corners().to_vec());
    m.rect = r;
    m
}

#[test]
fn scale_presets_and_separate_y_scale() {
    let mut h = harness();
    h.state_mut().set_option("panel", "measurements");
    h.run_steps(3);
    let s = markupcraft_measure::Scale::architectural(0.25, 1.0);
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_page_scale(&[0], &s, false)
        .unwrap();
    h.run_steps(3);
    // + Add Preset saves the page scale under a name.
    h.state_mut().state.measure.preset_name = "Quarter plan".into();
    press(&mut h, "+ Add Preset");
    let p = &h.state().state.toolchest.extras.scale_presets;
    assert_eq!(p.len(), 1, "{}", status(&h));
    assert_eq!(p[0].name, "Quarter plan");
    // It is listed first and can be deleted (the built-ins cannot).
    pick(&mut h, "Choose...", "Delete preset");
    assert!(
        h.state().state.toolchest.extras.scale_presets.is_empty(),
        "{}",
        status(&h)
    );
    // Separate Y Scale: 1 in = 8 ft across, 1 in = 16 ft up.
    press(&mut h, "Separate Y Scale");
    h.state_mut().state.measure.y_real = "16".into();
    h.run_steps(2);
    press(&mut h, "Apply custom scale");
    let sc = h.state().state.doc().unwrap().session.doc().pages[0]
        .scale
        .clone()
        .unwrap();
    assert!(!sc.y.is_empty(), "{}", status(&h));
    assert!((sc.y_conv() / sc.x_conv() - 2.0).abs() < 1e-9);
    // A 72 pt tall line measures 16 ft, a 72 pt wide one 8 ft.
    let mut tall = Markup::new(Kind::Length, 0, vec![p2(100.0, 100.0), p2(100.0, 172.0)]);
    tall.scale = Some(sc.clone());
    let mut wide = Markup::new(Kind::Length, 0, vec![p2(100.0, 100.0), p2(172.0, 100.0)]);
    wide.scale = Some(sc);
    assert!((tall.quantity().unwrap() - 16.0).abs() < 1e-6);
    assert!((wide.quantity().unwrap() - 8.0).abs() < 1e-6);
}

fn p2(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

#[test]
fn line_styles_custom_statuses_and_profile_columns() {
    let mut h = harness();
    // Markup > Line Styles...: a named dash pattern, offered in Properties.
    run(&mut h, "markup.line_styles");
    assert!(h.state().state.edit.chest_rt.styles_open);
    h.state_mut().state.edit.chest_rt.style_name = "Fence".into();
    h.state_mut().state.edit.chest_rt.style_dash = "10, 4".into();
    h.run_steps(2);
    press(&mut h, "Add Style");
    assert_eq!(h.state().state.toolchest.extras.line_styles.len(), 1, "{}", status(&h));
    h.state_mut().state.edit.chest_rt.styles_open = false;
    let id = add(&mut h, rect_markup(Rect::new(100.0, 460.0, 160.0, 500.0)));
    select(&mut h, std::slice::from_ref(&id));
    pick(&mut h, "Solid", "Fence");
    assert_eq!(find(&h, &id).dash, vec![10.0, 4.0]);

    // A custom status typed in Properties is set and kept for the menus.
    press(&mut h, "Custom Status");
    let field = h
        .get_all_by(|n| n.role() == Role::TextInput && n.placeholder() == Some("New status (Enter)"))
        .next()
        .expect("the new status field");
    field.focus();
    h.run_steps(1);
    h.get_by(|n| n.is_focused()).type_text("Installed");
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(find(&h, &id).status, "Installed");
    assert_eq!(h.state().state.toolchest.extras.statuses, vec!["Installed".to_string()]);

    // Manage Columns: Save to Profile, then Load from Profile into another column set.
    let col = markupcraft_model::CustomColumn {
        id: "cost".into(),
        name: "Cost".into(),
        kind: markupcraft_model::ColumnType::Choice,
        ..Default::default()
    };
    h.state_mut().state.list.columns_editor = Some(vec![col]);
    h.run_steps(2);
    press(&mut h, "Save to Profile");
    assert_eq!(h.state().state.toolchest.extras.columns.len(), 1);
    h.state_mut().state.list.columns_editor = Some(Vec::new());
    h.run_steps(2);
    press(&mut h, "Load from Profile");
    let cols = h.state().state.list.columns_editor.clone().unwrap();
    assert_eq!(cols.len(), 1, "{}", status(&h));
    // A Choice column's items from CSV.
    let csv = std::env::temp_dir().join(format!("markupcraft-choices-{}.csv", std::process::id()));
    std::fs::write(&csv, "Item,Subject,Value\nCopper,Pipe,12.5\n\"Steel, black\",,$8\n").unwrap();
    h.state_mut().state.dialogs.scripted = Some(vec![csv.clone()]);
    press(&mut h, "Import...");
    h.run_steps(3);
    let items = h.state().state.list.columns_editor.clone().unwrap()[0].items.clone();
    assert_eq!(items.len(), 2, "{}", status(&h));
    assert_eq!(
        (items[0].text.as_str(), items[0].subject.as_str(), items[0].value),
        ("Copper", "Pipe", Some(12.5))
    );
    assert_eq!((items[1].text.as_str(), items[1].value), ("Steel, black", Some(8.0)));
    let _ = std::fs::remove_file(&csv);
}

#[test]
fn tool_chest_sequence_pin_lock_options_and_update_on_reuse() {
    let mut h = harness();
    let mut tpl = rect_markup(Rect::new(0.0, 0.0, 40.0, 30.0));
    tpl.label = "R1".into();
    tpl.contents = "saved comment".into();
    let item = h
        .state_mut()
        .state
        .toolchest
        .add_markup(markupcraft_ui_egui::chest::MY_TOOLS, &tpl)
        .unwrap();
    let my = markupcraft_ui_egui::chest::MY_TOOLS;
    h.state_mut()
        .state
        .toolchest
        .update_item(my, &item, |i| i.sequence = true);
    // Sequence: each rectangle drawn with it takes the next label.
    for y in [460.0, 520.0] {
        h.state_mut().state.use_item(my, &item);
        h.run_steps(2);
        drag(&mut h, (100.0, y), (150.0, y + 30.0));
    }
    let all = markups(&h);
    let labels: Vec<&str> = all[all.len() - 2..].iter().map(|m| m.label.as_str()).collect();
    assert_eq!(labels, ["R1", "R2"]);
    assert_eq!(h.state().state.toolchest.item(my, &item).unwrap().markup.label, "R3");
    // Keep comments: No.
    h.state_mut().state.toolchest.extras.comments = markupcraft_ui_egui::chest_more::CommentMode::Never;
    h.run_steps(2);
    h.state_mut().state.use_item(my, &item);
    h.run_steps(2);
    drag(&mut h, (200.0, 460.0), (250.0, 490.0));
    assert_eq!(last(&h).contents, "", "the saved comment does not carry");
    // Update Tool Set Item on Reuse: a later colour change writes back to the item.
    h.state_mut().state.toolchest.extras.update_on_reuse = true;
    let placed = last(&h).id;
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_properties(
            std::slice::from_ref(&placed),
            &markupcraft_engine::MarkupPatch {
                color: Some(markupcraft_model::Color::rgb(0.0, 0.5, 0.0)),
                ..Default::default()
            },
        )
        .unwrap();
    select(&mut h, std::slice::from_ref(&placed));
    assert_eq!(
        h.state().state.toolchest.item(my, &item).unwrap().markup.color,
        markupcraft_model::Color::rgb(0.0, 0.5, 0.0)
    );
    // Recent Tools keeps the number asked for; Clear empties it.
    h.state_mut().state.toolchest.extras.recent_max = 1;
    tool(&mut h, "ellipse");
    drag(&mut h, (300.0, 460.0), (350.0, 490.0));
    assert_eq!(h.state().state.toolchest.recent.len(), 1);
    h.state_mut().state.toolchest.clear_recent();
    assert!(h.state().state.toolchest.recent.is_empty());
    // Pin to Toolbar: the set floats as a toolbar; its button picks the tool.
    h.state_mut().state.toolchest.set_pinned(my, true);
    h.run_steps(3);
    key(&mut h, Key::Escape);
    let name = h.state().state.toolchest.item(my, &item).unwrap().name.clone();
    let buttons: Vec<_> = h.get_all_by_label(&name).collect();
    assert!(!buttons.is_empty(), "the pinned toolbar shows the item");
    buttons[buttons.len() - 1].click();
    h.run_steps(3);
    assert_eq!(
        h.state().state.active_item.as_ref().map(|(_, i)| i.as_str()),
        Some(item.as_str())
    );
    // A locked set takes no new tools.
    h.state_mut().state.toolchest.set_locked(my, true);
    assert!(h.state_mut().state.toolchest.add_markup(my, &tpl).is_none());
    h.state_mut().state.toolchest.set_locked(my, false);
    assert!(h.state_mut().state.toolchest.add_markup(my, &tpl).is_some());
}

#[test]
fn collapsed_tool_set_flyout_picks_a_tool() {
    let mut h = Harness::builder()
        .with_size(vec2(1500.0, 950.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(|_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            let mut tpl = rect_markup(Rect::new(0.0, 0.0, 40.0, 30.0));
            tpl.subject = "Wall Tag".into();
            app.state
                .toolchest
                .add_markup(markupcraft_ui_egui::chest::MY_TOOLS, &tpl);
            app.state.toolchest.sets[0].collapsed = true;
            app.open_bytes("sample.pdf", None, markupcraft_render::synthetic::sample_pdf())
                .unwrap();
            app
        });
    h.run_steps(4);
    h.state_mut().set_option("panel", "toolchest");
    h.run_steps(6);
    press(&mut h, "1 tools");
    h.get_by_label("Wall Tag  (Rectangle)").click();
    h.run_steps(3);
    assert_eq!(h.state().state.tool, "rectangle");
    assert!(h.state().state.active_item.is_some());
}

#[test]
fn list_filters_dim_the_page_display_options_and_columns() {
    let mut h = harness();
    // A Subject filter keeps the Area: everything else is drawn faded.
    let allowed: std::collections::BTreeSet<String> = ["Area".to_string()].into_iter().collect();
    h.state_mut().state.list.view.filters.insert("subject".into(), allowed);
    h.run_steps(3);
    let dimmed = h.state().state.edit.more.dimmed.clone();
    assert!(!dimmed.contains(AREA));
    assert!(dimmed.contains("SAMPLESQUAREAAAA"), "{dimmed:?}");
    h.state_mut().state.list.view.filters.clear();
    h.run_steps(3);
    assert!(h.state().state.edit.more.dimmed.is_empty());
    // View > Line Weights and Rollover Comments are toggles.
    let before = h.state().state.checked("view.line_weights");
    run(&mut h, "view.line_weights");
    assert_ne!(h.state().state.checked("view.line_weights"), before);
    run(&mut h, "view.rollover_comments");
    assert_eq!(h.state().state.checked("view.rollover_comments"), Some(false));
    // Columns menu: show a hidden column.
    assert!(!h.state().state.list.view.visible.contains(&"x".to_string()));
    let at = h.get_by_label("Columns").rect().center();
    h.hover_at(at);
    h.step();
    h.get_by_label("Columns").click();
    h.run_steps(3);
    h.get_by_label("X Center").scroll_to_me();
    h.run_steps(10);
    h.get_by_label("X Center").click();
    h.run_steps(3);
    assert!(h.state().state.list.view.visible.contains(&"xcenter".to_string()));
    // A count's item size shows in its Width / Height / Depth cells.
    let mut c = Markup::new(Kind::Count, 0, vec![p(100.0, 560.0), p(140.0, 560.0)]);
    c.item_width = 2.0;
    c.depth = 1.0;
    let id = add(&mut h, c);
    let d = h.state().state.doc().unwrap();
    let t = markupcraft_model::MarkupTable::new(d.session.doc());
    let row = d.session.doc().markups.iter().position(|m| m.id == id).unwrap();
    assert!(!t.cell_by_id(row, "width").text.is_empty());
    assert!(!t.cell_by_id(row, "depth").text.is_empty());
    assert!(t.cell_by_id(row, "height").text.is_empty());
}

#[test]
fn file_attachment_tool_icon_and_save() {
    let mut h = harness();
    let src = std::env::temp_dir().join(format!("markupcraft-attach-{}.txt", std::process::id()));
    std::fs::write(&src, b"attached text").unwrap();
    h.state_mut().state.dialogs.scripted = Some(vec![src.clone()]);
    key(&mut h, Key::F);
    assert_eq!(h.state().state.tool, "attachment");
    click(&mut h, 600.0, 560.0);
    h.run_steps(4);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Attachment, "{}", status(&h));
    assert_eq!(m.attachment_name, src.file_name().unwrap().to_string_lossy());
    // Properties: its icon.
    select(&mut h, std::slice::from_ref(&m.id));
    pick(&mut h, "PushPin", "Paperclip");
    assert_eq!(find(&h, &m.id).icon, "Paperclip");
    // Save Attached File...
    let out = std::env::temp_dir().join(format!("markupcraft-attach-out-{}.txt", std::process::id()));
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    run(&mut h, "markup.save_attachment");
    h.run_steps(3);
    assert_eq!(std::fs::read(&out).unwrap(), b"attached text", "{}", status(&h));
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn replies_in_properties_and_a_linked_summary() {
    let mut h = harness();
    let rect = "SAMPLESQUAREAAAA".to_string();
    select(&mut h, std::slice::from_ref(&rect));
    press(&mut h, "Replies (0)");
    let field = h
        .get_all_by(|n| n.role() == Role::TextInput && n.placeholder() == Some("Reply (Enter)"))
        .next()
        .expect("the reply field");
    field.focus();
    h.run_steps(1);
    h.get_by(|n| n.is_focused()).type_text("Please check");
    h.key_press(Key::Enter);
    h.run_steps(3);
    let m = find(&h, &rect);
    assert_eq!(m.replies.len(), 1, "{}", status(&h));
    assert_eq!(m.replies[0].text, "Please check");
    // Properties > General shows the count; the reply can be deleted.
    assert!(h.query_by_label("Replies (1)").is_some());
    press(&mut h, "Delete reply");
    assert!(find(&h, &rect).replies.is_empty());
    // Markup > Append Summary with Links: a summary page with a link per row.
    let pages = h.state().state.doc().unwrap().session.page_count();
    run(&mut h, "markup.summary_append");
    let d = h.state().state.doc().unwrap();
    assert_eq!(d.session.page_count(), pages + 1, "{}", status(&h));
    let n = d.session.doc().markups.len();
    assert_eq!(d.session.links().iter().filter(|l| l.page == pages).count(), n);
}

#[test]
fn text_box_margin_and_line_spacing_from_properties() {
    let mut h = harness();
    let text = "SAMPLETEXTAAAAAA".to_string();
    select(&mut h, std::slice::from_ref(&text));
    retype(&mut h, Role::SpinButton, "0.0 pt", "5");
    assert_eq!(find(&h, &text).text.margin, 5.0, "{}", status(&h));
    retype(&mut h, Role::SpinButton, "1.00 x", "2");
    assert_eq!(find(&h, &text).text.line_spacing, 2.0, "{}", status(&h));
}

#[test]
fn protected_and_temporary_scales() {
    let mut h = harness();
    h.state_mut().set_option("panel", "measurements");
    h.run_steps(3);
    let s = markupcraft_measure::Scale::architectural(0.25, 1.0);
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_page_scale(&[0], &s, false)
        .unwrap();
    h.run_steps(3);
    press(&mut h, "Protect scale");
    let uid = h.state().state.doc().unwrap().uid;
    assert!(h.state().state.edit.more.protected.contains(&(uid, 0)));
    // Protected: Apply does nothing.
    press(&mut h, "Apply custom scale");
    let page = h.state().state.doc().unwrap().session.doc().pages[0]
        .scale
        .clone()
        .unwrap();
    assert_eq!(page.ratio, s.ratio, "the protected scale stays");
    press(&mut h, "Protect scale");
    // Temporary: applied, but not marked for saving.
    press(&mut h, "Temporary (not saved)");
    press(&mut h, "Apply custom scale");
    let p = h.state().state.doc().unwrap().session.doc().pages[0].clone();
    assert_ne!(p.scale.unwrap().ratio, s.ratio, "{}", status(&h));
    assert!(!p.scale_changed, "a temporary scale is not saved");
}

#[test]
fn drag_to_tool_chest_edit_action_and_takeoff_workspace() {
    let mut h = harness();
    h.state_mut().set_option("panel", "toolchest");
    h.run_steps(4);
    let rect = "SAMPLESQUAREAAAA".to_string();
    let before = find(&h, &rect).pts.clone();
    select(&mut h, std::slice::from_ref(&rect));
    // Drag the rectangle off the page onto the Tool Chest panel.
    let to = h.get_by_label("Keep tool selected").rect().center() + vec2(0.0, 60.0);
    let from = screen(&h, 950.0, 650.0);
    h.hover_at(from);
    h.step();
    button(&mut h, from, true, PointerButton::Primary, Modifiers::NONE);
    for i in 1..=12 {
        h.hover_at(from + (to - from) * (i as f32 / 12.0));
        h.step();
    }
    button(&mut h, to, false, PointerButton::Primary, Modifiers::NONE);
    h.run_steps(3);
    let chest = &h.state().state.toolchest;
    assert_eq!(chest.sets[0].items.len(), 1, "{}", status(&h));
    assert_eq!(find(&h, &rect).pts, before, "dropped off the page: not moved");

    // Edit Action is covered by features2_ui (the link dialog).

    // Window > Takeoff Workspace.
    run(&mut h, "window.takeoff_workspace");
    let open = h.state().state.open_panels.clone();
    for p in ["measurements", "markups", "toolchest"] {
        assert!(open.contains(&p), "{p} in {open:?}");
    }
    // The Sequence column reads a sequence label's number.
    let mut m = Markup::new(Kind::Line, 0, vec![p(10.0, 10.0), p(50.0, 10.0)]);
    m.label = "D-12".into();
    let id = add(&mut h, m);
    let d = h.state().state.doc().unwrap();
    let t = markupcraft_model::MarkupTable::new(d.session.doc());
    let row = d.session.doc().markups.iter().position(|m| m.id == id).unwrap();
    assert_eq!(t.cell_by_id(row, "sequence").text, "12");
}

#[test]
fn note_pop_ups_open_and_show_or_hide() {
    let mut h = harness();
    let r = Rect::new(1000.0, 500.0, 1024.0, 524.0);
    let mut n = Markup::new(Kind::Note, 0, r.corners().to_vec());
    n.rect = r;
    n.contents = "See the detail".into();
    let id = add(&mut h, n);
    select(&mut h, std::slice::from_ref(&id));
    press(&mut h, "Pop-up open");
    assert!(find(&h, &id).popup_open, "{}", status(&h));
    assert_eq!(h.state().state.checked("view.note_popups"), Some(true));
    run(&mut h, "view.note_popups");
    assert_eq!(h.state().state.checked("view.note_popups"), Some(false));
}

/// Revu 21's default keys (docs/revu_features/05_shortcuts.md) for the tools and commands
/// added here, each bound to its command.
#[test]
fn revu_markup_measure_and_selection_shortcuts_are_bound() {
    use markupcraft_ui_egui::commands::{Keys, bindings};
    let k = |ctrl: bool, shift: bool, alt: bool, key: Key| Keys::new(ctrl, shift, alt, key);
    let (c, s, a, n) = (true, true, true, false);
    let expected = [
        (k(n, s, n, Key::C), "tool.arc"),
        (k(n, s, n, Key::L), "tool.dimension"),
        (k(n, s, n, Key::E), "tool.eraser"),
        (k(n, s, n, Key::F), "tool.flag"),
        (k(n, s, n, Key::U), "tool.squiggly"),
        (k(n, s, n, Key::O), "tool.lasso"),
        (k(n, s, n, Key::T), "tool.selecttext"),
        (k(n, n, n, Key::Z), "tool.zoom"),
        (k(n, s, n, Key::Z), "view.toggle_zoom"),
        (k(n, n, n, Key::M), "measure.tool"),
        (k(n, n, n, Key::I), "markup.image"),
        (k(c, n, n, Key::F2), "markup.export"),
        (k(c, n, n, Key::F3), "markup.import"),
        (k(n, s, a, Key::R), "markup.review_text"),
        (k(n, n, n, Key::F), "tool.attachment"),
        (k(c, s, n, Key::U), "document.unflatten"),
        (k(c, s, n, Key::E), "markup.edit_action"),
    ];
    let b = bindings();
    for (keys, id) in expected {
        assert!(
            b.iter().any(|(bk, bid)| *bk == keys && bid == id),
            "{} should run {id}",
            keys.label()
        );
    }
}
