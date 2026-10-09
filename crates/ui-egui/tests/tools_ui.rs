//! Every drawing tool driven through the real shell headlessly (egui_kittest, no window): the
//! clicks, drags, keys and typing a user would make, checked against the engine's document.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_geom::Point;
use markupcraft_model::{Kind, Markup};
use markupcraft_ui_egui::MarkupCraftApp;

const AREA: &str = "SAMPLEAREAAAAAAA";
const SQUARE: &str = "SAMPLESQUAREAAAA";
const TEXT: &str = "SAMPLETEXTAAAAAA";

fn harness() -> Harness<'static, MarkupCraftApp> {
    let mut h = Harness::builder()
        .with_size(vec2(1500.0, 950.0))
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

fn click_mod(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64, m: Modifiers) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.step();
    h.event(Event::ModifiersChanged(m));
    button(h, at, true, PointerButton::Primary, m);
    button(h, at, false, PointerButton::Primary, m);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

fn click(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64) {
    click_mod(h, x, y, Modifiers::NONE);
}

fn double_click(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64) {
    // Far enough from the last click that this is a double, not a triple, click.
    h.run_steps(30);
    let at = screen(h, x, y);
    h.hover_at(at);
    h.step();
    for _ in 0..2 {
        button(h, at, true, PointerButton::Primary, Modifiers::NONE);
        button(h, at, false, PointerButton::Primary, Modifiers::NONE);
    }
    h.run_steps(2);
}

fn drag_mod(h: &mut Harness<'_, MarkupCraftApp>, from: (f64, f64), to: (f64, f64), m: Modifiers) {
    let (a, b) = (screen(h, from.0, from.1), screen(h, to.0, to.1));
    h.hover_at(a);
    h.step();
    h.event(Event::ModifiersChanged(m));
    button(h, a, true, PointerButton::Primary, m);
    for i in 1..=8 {
        h.hover_at(a + (b - a) * (i as f32 / 8.0));
        h.step();
    }
    button(h, b, false, PointerButton::Primary, m);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

fn drag(h: &mut Harness<'_, MarkupCraftApp>, from: (f64, f64), to: (f64, f64)) {
    drag_mod(h, from, to, Modifiers::NONE);
}

fn key(h: &mut Harness<'_, MarkupCraftApp>, k: Key) {
    h.key_press(k);
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

fn near(a: Point, x: f64, y: f64) -> bool {
    (a.x - x).abs() < 1.5 && (a.y - y).abs() < 1.5
}

#[test]
fn polyline_takes_clicks_backspace_and_enter() {
    let mut h = harness();
    h.key_press_modifiers(Modifiers::SHIFT, Key::N);
    h.run_steps(2);
    assert_eq!(h.state().state.tool, "polyline");
    let before = markups(&h).len();
    for (x, y) in [(100.0, 100.0), (200.0, 140.0), (300.0, 100.0), (400.0, 160.0)] {
        click(&mut h, x, y);
    }
    key(&mut h, Key::Backspace);
    key(&mut h, Key::Enter);
    let ms = markups(&h);
    assert_eq!(ms.len(), before + 1);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Polyline);
    assert_eq!(m.pts.len(), 3, "{:?}", m.pts);
    assert!(near(m.pts[2], 300.0, 100.0));
    assert_eq!(h.state().state.tool, "select");
    // One undo removes it.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(markups(&h).len(), before);
}

#[test]
fn escape_cancels_a_drawing() {
    let mut h = harness();
    tool(&mut h, "polygon");
    let before = markups(&h).len();
    click(&mut h, 100.0, 100.0);
    click(&mut h, 200.0, 100.0);
    assert!(h.state().state.doc().unwrap().view.draft.is_some());
    key(&mut h, Key::Escape);
    assert!(h.state().state.doc().unwrap().view.draft.is_none());
    assert_eq!(markups(&h).len(), before);
    assert_eq!(h.state().state.tool, "select");
}

#[test]
fn shift_constrains_a_line_and_a_drag_makes_an_arrow() {
    let mut h = harness();
    key(&mut h, Key::L);
    click(&mut h, 100.0, 100.0);
    click_mod(&mut h, 300.0, 112.0, Modifiers::SHIFT);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Line);
    assert!((m.pts[1].y - 100.0).abs() < 1e-6, "{:?}", m.pts);
    assert!(m.pts[1].x > 290.0);

    key(&mut h, Key::A);
    drag(&mut h, (100.0, 60.0), (250.0, 90.0));
    let a = last(&h);
    assert_eq!(a.kind, Kind::Arrow);
    assert_eq!(a.line_start, "OpenArrow");
    assert!(
        near(a.pts[0], 100.0, 60.0) && near(a.pts[1], 250.0, 90.0),
        "{:?}",
        a.pts
    );
}

#[test]
fn area_shows_its_value_and_finishes_on_double_click() {
    let mut h = harness();
    h.key_press_modifiers(Modifiers::SHIFT | Modifiers::ALT, Key::A);
    h.run_steps(2);
    assert_eq!(h.state().state.tool, "area");
    click(&mut h, 650.0, 200.0);
    click(&mut h, 830.0, 200.0);
    click(&mut h, 830.0, 300.0);
    h.hover_at(screen(&h, 650.0, 300.0));
    h.run_steps(2);
    let status = h.state().state.status.clone();
    assert!(status.starts_with("Area: "), "live readout: {status}");
    double_click(&mut h, 650.0, 300.0);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Area);
    assert_eq!(m.pts.len(), 4, "{:?}", m.pts);
    // 180 x 100 points at 1/8" = 1': 20 ft x 11.11 ft.
    assert_eq!(m.quantity_text(), "222.22 sf");
}

#[test]
fn length_and_polylength_measure() {
    let mut h = harness();
    tool(&mut h, "length");
    click(&mut h, 120.0, 120.0);
    click(&mut h, 192.0, 120.0);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Length);
    assert_eq!(m.quantity_text(), "8 ft");
    tool(&mut h, "polylength");
    click(&mut h, 100.0, 100.0);
    click(&mut h, 172.0, 100.0);
    click(&mut h, 172.0, 172.0);
    // A right-click finishes.
    let at = screen(&h, 172.0, 172.0);
    button(&mut h, at, true, PointerButton::Secondary, Modifiers::NONE);
    button(&mut h, at, false, PointerButton::Secondary, Modifiers::NONE);
    h.run_steps(2);
    let p = last(&h);
    assert_eq!(p.kind, Kind::Polylength);
    assert_eq!(p.quantity_text(), "16 ft");
}

#[test]
fn count_adds_items_until_enter() {
    let mut h = harness();
    tool(&mut h, "count");
    for x in [700.0, 740.0, 780.0] {
        click(&mut h, x, 460.0);
    }
    key(&mut h, Key::Enter);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Count);
    assert_eq!(m.pts.len(), 3);
    assert_eq!(m.quantity_text(), "3");
    // Count stays active for the next group.
    assert_eq!(h.state().state.tool, "count");
}

#[test]
fn text_box_typewriter_and_note_type_in_place() {
    let mut h = harness();
    key(&mut h, Key::T);
    click(&mut h, 150.0, 560.0);
    assert!(h.state().state.doc().unwrap().view.editor.is_some(), "editor opens");
    type_text(&mut h, "Hello wall");
    key(&mut h, Key::Escape);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Text);
    assert_eq!(m.contents, "Hello wall");
    assert_eq!(h.state().state.tool, "select");

    key(&mut h, Key::W);
    click(&mut h, 150.0, 500.0);
    type_text(&mut h, "Typed");
    key(&mut h, Key::Escape);
    let t = last(&h);
    assert_eq!((t.kind, t.contents.as_str()), (Kind::Typewriter, "Typed"));

    key(&mut h, Key::N);
    click(&mut h, 1100.0, 600.0);
    type_text(&mut h, "Check this");
    key(&mut h, Key::Escape);
    let n = last(&h);
    assert_eq!((n.kind, n.contents.as_str()), (Kind::Note, "Check this"));

    // Double-click an existing text box: edit its text.
    let before = find(&h, TEXT).contents;
    let b = find(&h, TEXT).rect;
    double_click(&mut h, (b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    assert!(h.state().state.doc().unwrap().view.editor.is_some());
    type_text(&mut h, "!");
    key(&mut h, Key::Escape);
    assert_eq!(find(&h, TEXT).contents, format!("{before}!"));
}

#[test]
fn an_empty_text_box_is_not_added() {
    let mut h = harness();
    let before = markups(&h).len();
    key(&mut h, Key::T);
    click(&mut h, 150.0, 560.0);
    key(&mut h, Key::Escape);
    assert_eq!(markups(&h).len(), before);
}

#[test]
fn callout_tip_then_box_then_text() {
    let mut h = harness();
    key(&mut h, Key::Q);
    click(&mut h, 450.0, 450.0);
    click(&mut h, 450.0, 600.0);
    type_text(&mut h, "Relocate");
    key(&mut h, Key::Escape);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Callout);
    assert_eq!(m.pts.len(), 6);
    assert!(near(m.pts[4], 450.0, 450.0), "tip {:?}", m.pts[4]);
    assert_eq!(m.contents, "Relocate");
}

#[test]
fn cloud_plus_adds_a_cloud_and_its_callout_as_a_group() {
    let mut h = harness();
    key(&mut h, Key::K);
    for (x, y) in [(650.0, 560.0), (760.0, 560.0), (760.0, 660.0)] {
        click(&mut h, x, y);
    }
    key(&mut h, Key::Enter);
    assert_eq!(last(&h).kind, Kind::Cloud);
    click(&mut h, 900.0, 470.0);
    type_text(&mut h, "Revise");
    key(&mut h, Key::Escape);
    let ms = markups(&h);
    let callout = ms.last().unwrap();
    let cloud = &ms[ms.len() - 2];
    assert_eq!((cloud.kind, callout.kind), (Kind::Cloud, Kind::Callout));
    assert!(!callout.group.is_empty() && callout.group == cloud.group);
}

#[test]
fn pen_and_highlight_draw_freehand() {
    let mut h = harness();
    key(&mut h, Key::P);
    drag(&mut h, (100.0, 600.0), (260.0, 640.0));
    let m = last(&h);
    assert_eq!(m.kind, Kind::Ink);
    assert!(m.pts.len() >= 5, "{} points", m.pts.len());
    key(&mut h, Key::H);
    drag(&mut h, (100.0, 560.0), (260.0, 560.0));
    let hl = last(&h);
    assert_eq!(hl.kind, Kind::Highlight);
    assert!(hl.multiply);
}

#[test]
fn stamp_places_the_chosen_design() {
    let mut h = harness();
    h.state_mut().state.stamp = "Reviewed";
    key(&mut h, Key::S);
    click(&mut h, 1100.0, 450.0);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Stamp);
    assert_eq!(m.stamp, "Reviewed");
    assert!(m.contents.starts_with("REVIEWED"));
}

#[test]
fn snapshot_copies_a_region_and_ctrl_v_pastes_it() {
    let mut h = harness();
    let before = markups(&h).len();
    key(&mut h, Key::G);
    drag(&mut h, (120.0, 180.0), (300.0, 350.0));
    assert_eq!(markups(&h).len(), before);
    assert_eq!(h.state().state.tool, "select");
    h.hover_at(screen(&h, 1100.0, 300.0));
    h.step();
    h.key_press_modifiers(Modifiers::COMMAND, Key::V);
    h.run_steps(2);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Snapshot);
    let c = Point::new((m.rect.x0 + m.rect.x1) / 2.0, (m.rect.y0 + m.rect.y1) / 2.0);
    assert!(near(c, 1100.0, 300.0), "pasted at {c:?}");
}

#[test]
fn text_markups_follow_the_page_text() {
    let mut h = harness();
    tool(&mut h, "texthighlight");
    // Drag across KITCHEN (12 pt Helvetica at 650, 640).
    drag(&mut h, (652.0, 645.0), (700.0, 645.0));
    let m = last(&h);
    assert_eq!(m.kind, Kind::TextHighlight);
    assert_eq!(m.pts.len(), 4);
    let ys: Vec<f64> = m.pts.iter().map(|p| p.y).collect();
    assert!(ys.iter().all(|y| (630.0..660.0).contains(y)), "{:?}", m.pts);
    // A click marks one word.
    tool(&mut h, "underline");
    click(&mut h, 160.0, 644.0);
    let u = last(&h);
    assert_eq!(u.kind, Kind::Underline);
    let xs: Vec<f64> = u.pts.iter().map(|p| p.x).collect();
    assert!(xs.iter().all(|x| (145.0..200.0).contains(x)), "LIVING: {:?}", u.pts);
}

#[test]
fn calibrate_asks_for_the_distance_and_sets_the_scale() {
    let mut h = harness();
    tool(&mut h, "calibrate");
    click(&mut h, 100.0, 100.0);
    click(&mut h, 172.0, 100.0);
    assert!(h.state().state.calibrate.is_some(), "the dialog opens");
    h.state_mut().state.calibrate.as_mut().unwrap().length = "4".into();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert!(h.state().state.calibrate.is_none());
    let d = h.state().state.doc().unwrap();
    let s = d.session.doc().pages[0].scale.clone().unwrap();
    assert!(s.ratio.contains('='), "{}", s.ratio);
    // The sample area (300 x 170 points) now measures at 1" = 4'.
    let a = d.session.doc().find(AREA).unwrap();
    assert_eq!(a.quantity_text(), "157.41 sf");
}

#[test]
fn cutout_cuts_a_hole_in_the_area() {
    let mut h = harness();
    let gross = find(&h, AREA).quantity().unwrap();
    tool(&mut h, "cutout");
    click(&mut h, 350.0, 200.0);
    click(&mut h, 422.0, 200.0);
    click(&mut h, 422.0, 272.0);
    click(&mut h, 350.0, 272.0);
    key(&mut h, Key::Enter);
    let a = find(&h, AREA);
    assert_eq!(a.holes.len(), 1);
    assert!((gross - a.quantity().unwrap() - 64.0).abs() < 0.01, "8' x 8' hole");
}

#[test]
fn shift_drag_moves_a_caption_alone() {
    let mut h = harness();
    let before = find(&h, AREA);
    // The caption sits at the vertex mean (450, 265).
    drag_mod(&mut h, (450.0, 265.0), (450.0, 300.0), Modifiers::SHIFT);
    let after = find(&h, AREA);
    assert_eq!(after.pts, before.pts, "the area did not move");
    let o = after.caption_offset.expect("caption offset");
    assert!((o.y - 35.0).abs() < 1.5 && o.x.abs() < 1.5, "{o:?}");
}

#[test]
fn a_moved_caption_and_new_markups_save_and_reopen() {
    let mut h = harness();
    drag_mod(&mut h, (450.0, 265.0), (480.0, 300.0), Modifiers::SHIFT);
    tool(&mut h, "callout");
    click(&mut h, 450.0, 450.0);
    click(&mut h, 450.0, 600.0);
    type_text(&mut h, "Saved text");
    key(&mut h, Key::Escape);
    let dir = std::env::temp_dir().join(format!("markupcraft-tools-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("saved.pdf");
    // Save As goes through the (scripted) file dialog.
    h.state_mut().state.dialogs.scripted = Some(vec![path.clone()]);
    h.state_mut().state.queue("file.save");
    h.run_steps(4);
    assert!(
        h.state().state.status.starts_with("Saved"),
        "{}",
        h.state().state.status
    );
    let (_f, doc) = markupcraft_revu::open(&path).unwrap();
    let a = doc.find(AREA).unwrap();
    let o = a.caption_offset.expect("the caption offset is saved");
    assert!((o.x - 30.0).abs() < 1.5 && (o.y - 35.0).abs() < 1.5, "{o:?}");
    let c = doc
        .markups
        .iter()
        .find(|m| m.kind == Kind::Callout)
        .expect("the callout");
    assert_eq!(c.contents, "Saved text");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn snap_to_content_lands_on_a_wall_corner() {
    let mut h = harness();
    h.state_mut().state.snaps.content = true;
    key(&mut h, Key::L);
    click(&mut h, 122.5, 181.5);
    click(&mut h, 600.5, 349.0);
    let m = last(&h);
    assert_eq!(m.pts[0], Point::new(120.0, 180.0), "{:?}", m.pts);
    assert_eq!(m.pts[1], Point::new(600.0, 350.0));
}

#[test]
fn snap_to_markup_and_grid() {
    let mut h = harness();
    h.state_mut().state.snaps.markup = true;
    key(&mut h, Key::L);
    // Near the square's corner (900, 600).
    click(&mut h, 902.0, 601.0);
    click(&mut h, 1100.0, 650.0);
    assert_eq!(last(&h).pts[0], Point::new(900.0, 600.0));
    h.state_mut().state.snaps = Default::default();
    h.state_mut().state.snaps.grid = true;
    key(&mut h, Key::L);
    click(&mut h, 1101.0, 451.0);
    click(&mut h, 1160.0, 500.0);
    assert_eq!(last(&h).pts[0], Point::new(1098.0, 450.0));
}

#[test]
fn context_menu_saves_a_look_to_the_tool_chest() {
    let mut h = harness();
    let at = screen(&h, 950.0, 650.0);
    h.hover_at(at);
    h.step();
    button(&mut h, at, true, PointerButton::Secondary, Modifiers::NONE);
    button(&mut h, at, false, PointerButton::Secondary, Modifiers::NONE);
    h.run_steps(2);
    // The menu's item (the Properties panel has one too).
    let items: Vec<_> = h.get_all_by_label("Add to Tool Chest").collect();
    assert_eq!(items.len(), 2, "the menu is open");
    // The menu opens at the pointer, over the canvas (the panel is on the right).
    let menu = items
        .iter()
        .min_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
        .unwrap();
    menu.click();
    h.run_steps(3);
    let chest = &h.state().state.toolchest;
    let item = chest.sets[0].items.first().expect("saved in My Tools").clone();
    assert_eq!(item.tool, "rectangle");
    // Draw with it: the new rectangle has the square's fill.
    h.state_mut().state.use_item("mytools", &item.id);
    h.run_steps(2);
    drag(&mut h, (100.0, 600.0), (200.0, 680.0));
    let m = last(&h);
    assert_eq!(m.kind, Kind::Rectangle);
    assert!(m.fill.is_some());
    assert_eq!(m.line_width, 2.0);
    // And it is a Recent Tool.
    assert_eq!(
        h.state().state.toolchest.recent.first().map(|i| i.tool.as_str()),
        Some("rectangle")
    );
}

#[test]
fn properties_panel_edits_go_through_the_engine() {
    let mut h = harness();
    let b = find(&h, TEXT).rect;
    click(&mut h, (b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    assert_eq!(h.state().state.doc().unwrap().selection(), &[TEXT.to_string()]);
    let depth = h.state().state.doc().unwrap().session.undo_depth();
    h.get_by_label("B").click();
    h.run_steps(3);
    assert!(find(&h, TEXT).text.bold);
    assert_eq!(h.state().state.doc().unwrap().session.undo_depth(), depth + 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert!(!find(&h, TEXT).text.bold);
}

#[test]
fn markups_list_shows_totals_and_status() {
    let mut h = harness();
    h.run_steps(3);
    // The grand total row and the footer under the table.
    assert_eq!(h.query_all_by_label_contains("Total (6)").count(), 2, "the totals");
    h.state_mut().set_option("group-by", "type");
    h.run_steps(3);
    assert!(h.query_by_label_contains("Type: Area (1)").is_some(), "a group header");
    // Status through the engine's cell edit.
    let d = h.state_mut().state.doc_mut().unwrap();
    assert!(d.session.set_cell(SQUARE, "status", "Accepted").unwrap());
    h.run_steps(2);
    assert_eq!(find(&h, SQUARE).status, "Accepted");
}

#[test]
fn closing_with_unsaved_changes_prompts() {
    let mut h = harness();
    tool(&mut h, "rectangle");
    drag(&mut h, (100.0, 600.0), (200.0, 680.0));
    h.state_mut().state.queue("file.close");
    h.run_steps(3);
    assert_eq!(h.state().state.docs.len(), 1, "still open");
    assert!(h.query_by_label("Don't Save").is_some());
    h.get_by_label("Don't Save").click();
    h.run_steps(3);
    assert!(h.state().state.docs.is_empty());
}

#[test]
fn clicking_the_first_point_closes_a_polygon_and_ellipse_drags() {
    let mut h = harness();
    h.key_press_modifiers(Modifiers::SHIFT, Key::P);
    h.run_steps(2);
    for (x, y) in [(650.0, 560.0), (760.0, 560.0), (760.0, 660.0), (650.0, 560.0)] {
        click(&mut h, x, y);
    }
    let m = last(&h);
    assert_eq!(m.kind, Kind::Polygon);
    assert_eq!(m.pts.len(), 3);
    key(&mut h, Key::E);
    drag(&mut h, (100.0, 600.0), (200.0, 680.0));
    let e = last(&h);
    assert_eq!(e.kind, Kind::Ellipse);
    assert!(near(Point::new(e.rect.x0, e.rect.y1), 100.0, 680.0), "{:?}", e.rect);
}

#[test]
fn strikethrough_marks_text() {
    let mut h = harness();
    tool(&mut h, "strikethrough");
    drag(&mut h, (152.0, 304.0), (200.0, 304.0));
    let m = last(&h);
    assert_eq!(m.kind, Kind::Strikeout);
    assert_eq!(m.pts.len(), 4);
}

#[test]
fn measurements_panel_sets_a_custom_scale_on_every_page() {
    let mut h = harness();
    h.state_mut().set_option("panel", "measurements");
    h.run_steps(3);
    h.state_mut().state.measure.pages = "all".into();
    h.run_steps(2);
    h.get_by_label("Apply custom scale").click();
    h.run_steps(3);
    let d = h.state().state.doc().unwrap();
    for p in &d.session.doc().pages {
        let s = p.scale.clone().expect("every page has the scale");
        assert!(s.ratio.starts_with("1 in = 8"), "{}", s.ratio);
    }
    // The sample area took the new scale: 300 x 170 points at 1" = 8'.
    assert_eq!(d.session.doc().find(AREA).unwrap().quantity_text(), "629.63 sf");
}

fn screen_on(h: &Harness<'_, MarkupCraftApp>, page: usize, x: f64, y: f64) -> Pos2 {
    let d = h.state().state.doc().unwrap();
    d.view
        .user_to_screen(page, Point::new(x, y), d.render.as_ref().unwrap().pages())
        .unwrap()
}

#[test]
fn measuring_without_a_scale_says_so() {
    let mut h = harness();
    h.state_mut().set_option("mode", "single");
    h.state_mut().set_option("page", "2");
    h.run_steps(4);
    tool(&mut h, "length");
    let a = screen_on(&h, 1, 100.0, 100.0);
    h.hover_at(a);
    h.step();
    button(&mut h, a, true, PointerButton::Primary, Modifiers::NONE);
    button(&mut h, a, false, PointerButton::Primary, Modifiers::NONE);
    h.hover_at(screen_on(&h, 1, 200.0, 100.0));
    h.run_steps(2);
    assert!(
        h.state().state.status.starts_with("No scale"),
        "{}",
        h.state().state.status
    );
}

#[test]
fn segment_values_toggle_from_the_menu() {
    let mut h = harness();
    let at = screen(&h, 450.0, 200.0);
    h.hover_at(at);
    h.step();
    button(&mut h, at, true, PointerButton::Secondary, Modifiers::NONE);
    button(&mut h, at, false, PointerButton::Secondary, Modifiers::NONE);
    h.run_steps(2);
    let items: Vec<_> = h.get_all_by_label("Show Segment Values").collect();
    let menu = items
        .iter()
        .min_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
        .unwrap();
    menu.click();
    h.run_steps(3);
    assert!(find(&h, AREA).segment_values);
}

#[test]
fn ctrl_shift_drag_copies_in_a_straight_line() {
    let mut h = harness();
    let before = markups(&h).len();
    drag_mod(
        &mut h,
        (950.0, 650.0),
        (1050.0, 662.0),
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert_eq!(markups(&h).len(), before + 1);
    let copy = last(&h);
    assert_eq!(copy.kind, Kind::Rectangle);
    let c = Point::new((copy.rect.x0 + copy.rect.x1) / 2.0, (copy.rect.y0 + copy.rect.y1) / 2.0);
    assert!(
        (c.y - 650.0).abs() < 0.5 && (c.x - 1050.0).abs() < 1.5,
        "straight across: {c:?}"
    );
    let orig = find(&h, SQUARE);
    assert!(
        ((orig.rect.x0 + orig.rect.x1) / 2.0 - 950.0).abs() < 0.5,
        "the original stays"
    );
}

#[test]
fn set_as_default_styles_new_markups() {
    let mut h = harness();
    click(&mut h, 950.0, 650.0);
    h.state_mut().state.queue("markup.set_default");
    h.run_steps(2);
    key(&mut h, Key::R);
    drag(&mut h, (100.0, 600.0), (200.0, 680.0));
    let m = last(&h);
    assert_eq!(m.kind, Kind::Rectangle);
    assert!(m.fill.is_some());
    assert_eq!(m.line_width, 2.0);
}

#[test]
fn properties_edit_every_selected_markup() {
    let mut h = harness();
    // Box select the square and the ellipse.
    drag(&mut h, (690.0, 720.0), (1010.0, 550.0));
    assert_eq!(h.state().state.doc().unwrap().selection().len(), 2);
    h.get_by_label("Locked").click();
    h.run_steps(3);
    assert!(find(&h, SQUARE).locked());
    assert!(find(&h, "SAMPLECIRCLEAAAA").locked());
}

#[test]
fn clicking_a_list_row_selects_its_markup() {
    let mut h = harness();
    h.get_by_label("Ellipse").click();
    h.run_steps(3);
    assert_eq!(
        h.state().state.doc().unwrap().selection(),
        &["SAMPLECIRCLEAAAA".to_string()]
    );
}
