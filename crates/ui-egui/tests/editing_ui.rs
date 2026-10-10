//! Markup and measurement editing driven through the real shell headlessly (egui_kittest):
//! arrange commands, nudging, the Format Painter, the rotation handle, Apply to All Pages,
//! the circular and angular measurement tools, Volume, Area by rectangle, Ellipse Cutout,
//! viewports, Sketch to Scale and Save All.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Kind, Markup};
use markupcraft_ui_egui::MarkupCraftApp;

const AREA: &str = "SAMPLEAREAAAAAAA";

fn harness() -> Harness<'static, MarkupCraftApp> {
    // The spelling dictionary loads before the first frame, not on a racing worker thread.
    markupcraft_ui_egui::richedit::load_dictionary_blocking();
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

fn click_at(h: &mut Harness<'_, MarkupCraftApp>, at: Pos2) {
    h.hover_at(at);
    h.step();
    button(h, at, true, PointerButton::Primary, Modifiers::NONE);
    button(h, at, false, PointerButton::Primary, Modifiers::NONE);
    h.run_steps(2);
}

fn click(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64) {
    let at = screen(h, x, y);
    click_at(h, at);
}

fn drag_screen(h: &mut Harness<'_, MarkupCraftApp>, a: Pos2, b: Pos2, m: Modifiers) {
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
    let (a, b) = (screen(h, from.0, from.1), screen(h, to.0, to.1));
    drag_screen(h, a, b, Modifiers::NONE);
}

fn key(h: &mut Harness<'_, MarkupCraftApp>, k: Key) {
    h.key_press(k);
    h.run_steps(2);
}

fn keys(h: &mut Harness<'_, MarkupCraftApp>, m: Modifiers, k: Key) {
    h.key_press_modifiers(m, k);
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

/// Add a rectangle on page 1 through the engine; returns its id.
fn add_rect(h: &mut Harness<'_, MarkupCraftApp>, r: Rect) -> String {
    let mut m = Markup::new(Kind::Rectangle, 0, r.corners().to_vec());
    m.color = markupcraft_model::Color::rgb(0.0, 0.0, 1.0);
    let id = h.state_mut().state.doc_mut().unwrap().session.add_markup(m).unwrap();
    h.run_steps(2);
    id
}

fn select(h: &mut Harness<'_, MarkupCraftApp>, ids: &[String]) {
    h.state_mut().state.doc_mut().unwrap().session.select(ids).unwrap();
    h.run_steps(2);
}

fn ext(h: &Harness<'_, MarkupCraftApp>, id: &str) -> Rect {
    bbox(&find(h, id).pts).unwrap()
}

#[test]
fn align_distribute_and_flip_from_the_keyboard() {
    let mut h = harness();
    let a = add_rect(&mut h, Rect::new(60.0, 460.0, 100.0, 500.0));
    let b = add_rect(&mut h, Rect::new(130.0, 560.0, 190.0, 600.0));
    let c = add_rect(&mut h, Rect::new(250.0, 650.0, 270.0, 700.0));
    let ids = vec![a.clone(), b.clone(), c.clone()];
    select(&mut h, &ids);
    // Ctrl+Alt+L: Align Left, to the last selected markup (Revu's reference).
    keys(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::L);
    for id in &ids {
        assert!((ext(&h, id).x0 - 250.0).abs() < 1e-6, "{}", status(&h));
    }
    keys(&mut h, Modifiers::COMMAND, Key::Z);
    // Ctrl+Alt+T: Align Top.
    keys(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::T);
    for id in &ids {
        assert!((ext(&h, id).y1 - 700.0).abs() < 1e-6);
    }
    keys(&mut h, Modifiers::COMMAND, Key::Z);
    // Distribute from the Markup menu (no default key).
    h.state_mut().state.queue("arrange.distribute_horizontal");
    h.run_steps(2);
    // widths 40 + 60 + 20 = 120 over 60..270: gaps of 45
    assert!((ext(&h, &b).x0 - 145.0).abs() < 1e-6, "{:?}", ext(&h, &b));
    // Ctrl+Alt+H: Flip Horizontal about the joint centre.
    select(&mut h, std::slice::from_ref(&a));
    let before = ext(&h, &a);
    keys(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::H);
    assert_eq!(
        ext(&h, &a),
        before,
        "a rectangle flipped about its own centre keeps its box"
    );
    keys(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::V);
    assert!(status(&h).contains("flipped"), "{}", status(&h));
}

#[test]
fn arrow_keys_nudge_the_selection() {
    let mut h = harness();
    let a = add_rect(&mut h, Rect::new(60.0, 460.0, 100.0, 500.0));
    select(&mut h, std::slice::from_ref(&a));
    key(&mut h, Key::ArrowRight);
    key(&mut h, Key::ArrowUp);
    assert_eq!(ext(&h, &a), Rect::new(61.0, 461.0, 101.0, 501.0));
    keys(&mut h, Modifiers::SHIFT, Key::ArrowLeft);
    assert_eq!(ext(&h, &a), Rect::new(51.0, 461.0, 91.0, 501.0));
}

#[test]
fn format_painter_copies_a_look() {
    let mut h = harness();
    let a = add_rect(&mut h, Rect::new(60.0, 460.0, 100.0, 500.0));
    let b = add_rect(&mut h, Rect::new(160.0, 460.0, 200.0, 500.0));
    {
        let d = h.state_mut().state.doc_mut().unwrap();
        let p = markupcraft_engine::MarkupPatch {
            color: Some(markupcraft_model::Color::rgb(0.0, 0.6, 0.0)),
            line_width: Some(4.0),
            dash: Some(vec![6.0, 3.0]),
            ..Default::default()
        };
        d.session.set_properties(std::slice::from_ref(&a), &p).unwrap();
    }
    select(&mut h, std::slice::from_ref(&a));
    h.state_mut().state.queue("markup.format_painter");
    h.run_steps(2);
    assert!(h.state().state.edit.painter.is_some());
    click(&mut h, 160.0, 480.0);
    let m = find(&h, &b);
    assert_eq!(m.line_width, 4.0);
    assert_eq!(m.dash, vec![6.0, 3.0]);
    assert!((m.color.g - 0.6).abs() < 1e-9);
    key(&mut h, Key::Escape);
    assert!(h.state().state.edit.painter.is_none());
}

#[test]
fn rotation_handle_turns_a_markup() {
    let mut h = harness();
    let pts = vec![
        Point::new(100.0, 600.0),
        Point::new(200.0, 600.0),
        Point::new(150.0, 640.0),
    ];
    let id = h
        .state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .add_markup(Markup::new(Kind::Polygon, 0, pts))
        .unwrap();
    select(&mut h, std::slice::from_ref(&id));
    // The handle sits 22 screen points above the top centre of the selection.
    let handle = screen(&h, 150.0, 640.0) - vec2(0.0, 22.0);
    let centre = screen(&h, 150.0, 620.0);
    let right = centre + vec2(80.0, 0.0);
    drag_screen(&mut h, handle, right, Modifiers::SHIFT);
    let m = find(&h, &id);
    // A quarter turn clockwise about the centre (150, 620): (150, 640) -> (170, 620).
    assert!(
        m.pts
            .iter()
            .any(|p| (p.x - 170.0).abs() < 0.5 && (p.y - 620.0).abs() < 0.5),
        "{:?} {}",
        m.pts,
        status(&h)
    );
}

#[test]
fn apply_to_all_pages_and_remove_from_group() {
    let mut h = harness();
    let a = add_rect(&mut h, Rect::new(60.0, 460.0, 100.0, 500.0));
    let b = add_rect(&mut h, Rect::new(160.0, 460.0, 200.0, 500.0));
    let c = add_rect(&mut h, Rect::new(260.0, 460.0, 300.0, 500.0));
    select(&mut h, std::slice::from_ref(&a));
    h.state_mut().state.queue("markup.apply_to_all_pages");
    h.run_steps(2);
    let on_two: Vec<Markup> = markups(&h).into_iter().filter(|m| m.page == 1).collect();
    assert_eq!(on_two.len(), 1, "{}", status(&h));
    assert_eq!(bbox(&on_two[0].pts).unwrap(), Rect::new(60.0, 460.0, 100.0, 500.0));
    select(&mut h, &[a.clone(), b.clone(), c.clone()]);
    keys(&mut h, Modifiers::COMMAND, Key::G);
    assert!(!find(&h, &c).group.is_empty());
    select(&mut h, std::slice::from_ref(&c));
    keys(&mut h, Modifiers::COMMAND | Modifiers::SHIFT | Modifiers::ALT, Key::G);
    assert!(find(&h, &c).group.is_empty(), "{}", status(&h));
    assert!(!find(&h, &a).group.is_empty());
}

#[test]
fn angle_diameter_and_radius_tools_measure() {
    let mut h = harness();
    // Shift+Alt+G: Angle. Arm end, vertex, other arm end.
    keys(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::G);
    assert_eq!(h.state().state.tool, "angle");
    click(&mut h, 200.0, 500.0);
    click(&mut h, 100.0, 500.0);
    click(&mut h, 100.0, 600.0);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Angle);
    assert_eq!(m.quantity_text(), "90\u{b0}");
    // Shift+Alt+D: Diameter (9 points = 1 foot on the sample page).
    keys(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::D);
    click(&mut h, 100.0, 650.0);
    click(&mut h, 190.0, 650.0);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Diameter);
    assert!((m.quantity().unwrap() - 10.0).abs() < 0.2, "{}", m.quantity_text());
    // Shift+Alt+U: Center Radius.
    keys(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::U);
    click(&mut h, 150.0, 700.0);
    click(&mut h, 150.0, 745.0);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Radius);
    assert!((m.quantity().unwrap() - 5.0).abs() < 0.2);
    // 3-Point Radius: three points on a circle of radius 45 around (1100, 600).
    tool(&mut h, "radius3");
    click(&mut h, 1145.0, 600.0);
    click(&mut h, 1100.0, 645.0);
    click(&mut h, 1055.0, 600.0);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Radius);
    assert!(
        (m.pts[0].x - 1100.0).abs() < 1.0 && (m.pts[0].y - 600.0).abs() < 1.0,
        "{:?}",
        m.pts
    );
    assert!((m.quantity().unwrap() - 5.0).abs() < 0.2);
}

#[test]
fn volume_and_area_by_rectangle() {
    let mut h = harness();
    // Shift+Alt+V: Volume, a closed outline with a depth (1 by default).
    keys(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::V);
    for (x, y) in [(60.0, 460.0), (150.0, 460.0), (150.0, 550.0), (60.0, 550.0)] {
        click(&mut h, x, y);
    }
    key(&mut h, Key::Enter);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Volume);
    assert_eq!(m.depth, 1.0);
    assert!(
        (m.quantity().unwrap() - 100.0).abs() < 0.2,
        "10' x 10' x 1': {}",
        m.quantity_text()
    );
    // Area (Rectangle): a drag.
    tool(&mut h, "area_rect");
    drag(&mut h, (60.0, 600.0), (150.0, 690.0));
    let m = last(&h);
    assert_eq!(m.kind, Kind::Area);
    assert_eq!(m.pts.len(), 4);
    assert!((m.quantity().unwrap() - 100.0).abs() < 0.2, "{}", m.quantity_text());
}

#[test]
fn ellipse_cutout_cuts_an_ellipse() {
    let mut h = harness();
    let gross = find(&h, AREA).quantity().unwrap();
    tool(&mut h, "ellipse_cutout");
    drag(&mut h, (380.0, 220.0), (452.0, 292.0));
    let a = find(&h, AREA);
    assert_eq!(a.holes.len(), 1, "{}", status(&h));
    assert!(a.holes[0].len() > 30, "an elliptical ring");
    // A circle 8' across: about 50.27 sf (a 64-gon is a touch less).
    let cut = gross - a.quantity().unwrap();
    assert!((cut - 50.27).abs() < 0.5, "{cut}");
}

#[test]
fn viewport_tool_asks_for_a_name_and_scale() {
    let mut h = harness();
    tool(&mut h, "viewport");
    drag(&mut h, (60.0, 460.0), (280.0, 740.0));
    assert!(h.state().state.edit.viewport.is_some(), "{}", status(&h));
    h.state_mut().state.edit.viewport.as_mut().unwrap().name = "Detail 1".into();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let d = h.state().state.doc().unwrap();
    let vp = d.session.doc().pages[0]
        .viewports
        .iter()
        .find(|v| v.name == "Detail 1")
        .cloned()
        .expect("the viewport");
    assert!(vp.scale.valid());
    assert!(
        (vp.bbox.x0 - 60.0).abs() < 1.0 && (vp.bbox.y1 - 740.0).abs() < 1.0,
        "{:?}",
        vp.bbox
    );
    // Highlight Viewports toggles from the View menu command.
    h.state_mut().state.queue("view.highlight_viewports");
    h.run_steps(2);
    assert!(h.state().state.edit.highlight_viewports);
    // A measurement inside it takes its scale.
    keys(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::L);
    click(&mut h, 100.0, 500.0);
    click(&mut h, 190.0, 500.0);
    assert_eq!(last(&h).scale.as_ref().map(|s| s.ratio.clone()), Some(vp.scale.ratio));
}

#[test]
fn viewports_panel_renames_copies_and_clears() {
    let mut h = harness();
    {
        let d = h.state_mut().state.doc_mut().unwrap();
        let sc = markupcraft_model::Scale::architectural(0.25, 1.0);
        d.session
            .add_viewport(0, Rect::new(60.0, 460.0, 280.0, 740.0), "Detail", &sc)
            .unwrap();
    }
    h.state_mut().set_option("panel", "measurements");
    h.run_steps(4);
    h.get_by_label("Copy").click();
    h.run_steps(3);
    let d = h.state().state.doc().unwrap();
    assert!(
        d.session.doc().pages[1].viewports.iter().any(|v| v.name == "Detail"),
        "{}",
        status(&h)
    );
    let page = h.state().state.doc().unwrap().view.current;
    h.get_by_label("Clear All").click();
    h.run_steps(3);
    let d = h.state().state.doc().unwrap();
    assert!(
        !d.session.doc().pages[page].viewports.iter().any(|v| v.name == "Detail"),
        "{}",
        status(&h)
    );
}

#[test]
fn sketch_to_scale_places_typed_lengths() {
    let mut h = harness();
    // Polylength: first point by click, then 10' right and 5' up typed.
    keys(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::Q);
    click(&mut h, 60.0, 460.0);
    h.state_mut().state.edit.sketch.length = "10'".into();
    h.state_mut().state.edit.sketch.angle = "0".into();
    h.run_steps(2);
    h.get_by_label("Add").click();
    h.run_steps(2);
    h.state_mut().state.edit.sketch.length = "5'".into();
    h.state_mut().state.edit.sketch.angle = "90".into();
    h.run_steps(2);
    h.get_by_label("Add").click();
    h.run_steps(2);
    h.state_mut().state.edit.sketch.length.clear();
    h.run_steps(1);
    h.get_by_label("Finish").click();
    h.run_steps(3);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Polylength, "{}", status(&h));
    assert_eq!(m.pts.len(), 3);
    let first = m.pts[0];
    assert!(
        (m.pts[1].x - first.x - 90.0).abs() < 1e-3 && (m.pts[2].y - first.y - 45.0).abs() < 1e-3,
        "{:?}",
        m.pts
    );
    assert!((m.quantity().unwrap() - 15.0).abs() < 1e-6);
    // Rectangle: a click for the corner, then a typed width and height.
    key(&mut h, Key::R);
    click(&mut h, 60.0, 740.0);
    h.state_mut().state.edit.sketch.width = "20'".into();
    h.state_mut().state.edit.sketch.height = "10'".into();
    h.run_steps(2);
    h.get_by_label("Place").click();
    h.run_steps(3);
    let r = last(&h);
    assert_eq!(r.kind, Kind::Rectangle);
    let b = bbox(&r.pts).unwrap();
    assert!(
        (b.width() - 180.0).abs() < 1e-3 && (b.height() - 90.0).abs() < 1e-3,
        "{b:?}"
    );
    // Ellipse: the same corner-and-size entry.
    key(&mut h, Key::E);
    click(&mut h, 1050.0, 500.0);
    h.state_mut().state.edit.sketch.width = "10'".into();
    h.state_mut().state.edit.sketch.height = "5'".into();
    h.run_steps(2);
    h.get_by_label("Place").click();
    h.run_steps(3);
    let e = last(&h);
    assert_eq!(e.kind, Kind::Ellipse);
    let b = bbox(&e.pts).unwrap();
    assert!(
        (b.width() - 90.0).abs() < 1e-3 && (b.height() - 45.0).abs() < 1e-3,
        "{b:?}"
    );
    // Polygon with a relative angle: 10' right, then a left turn of 90 degrees, 10'.
    keys(&mut h, Modifiers::SHIFT, Key::P);
    click(&mut h, 1050.0, 650.0);
    h.state_mut().state.edit.sketch.length = "10'".into();
    h.state_mut().state.edit.sketch.angle = "0".into();
    h.state_mut().state.edit.sketch.relative = true;
    h.run_steps(2);
    h.get_by_label("Add").click();
    h.run_steps(2);
    h.state_mut().state.edit.sketch.angle = "90".into();
    h.run_steps(2);
    h.get_by_label("Add").click();
    h.run_steps(2);
    h.state_mut().state.edit.sketch.length.clear();
    h.run_steps(1);
    h.get_by_label("Finish").click();
    h.run_steps(3);
    let p = last(&h);
    assert_eq!(p.kind, Kind::Polygon, "{}", status(&h));
    assert_eq!(p.pts.len(), 3);
    assert!((p.pts[2].x - p.pts[1].x).abs() < 1e-3 && (p.pts[2].y - p.pts[1].y - 90.0).abs() < 1e-3);
}

#[test]
fn save_all_saves_every_edited_document() {
    let dir = std::env::temp_dir().join(format!("markupcraft-saveall-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = harness();
    let paths: Vec<_> = ["a.pdf", "b.pdf"].iter().map(|n| dir.join(n)).collect();
    for p in &paths {
        std::fs::write(p, markupcraft_render::synthetic::sample_pdf()).unwrap();
        h.state_mut().open_path(p);
        h.run_steps(2);
        let d = h.state_mut().state.doc_mut().unwrap();
        d.session
            .move_markups(&["SAMPLESQUAREAAAA".to_string()], 5.0, 0.0)
            .unwrap();
    }
    assert!(h.state().state.docs.iter().filter(|d| d.session.is_dirty()).count() >= 2);
    h.state_mut().state.queue("file.save_all");
    h.run_steps(3);
    for d in h.state().state.docs.iter().filter(|d| d.path.is_some()) {
        assert!(!d.session.is_dirty(), "{} saved", d.name);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
