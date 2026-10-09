//! The real shell driven headlessly (egui_kittest, no window, no GPU needed): select a
//! markup by clicking it, move it by dragging, delete it with the Delete key, undo with
//! Ctrl+Z, draw a rectangle with the Rectangle tool.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use markupcraft_geom::Point;
use markupcraft_ui_egui::MarkupCraftApp;

const SQUARE: &str = "SAMPLESQUAREAAAA";

fn harness() -> Harness<'static, MarkupCraftApp> {
    let mut h = Harness::builder().with_size(vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = MarkupCraftApp::new();
        app.state.threads = 0;
        app.open_bytes("sample.pdf", None, markupcraft_render::synthetic::sample_pdf())
            .unwrap();
        app
    });
    h.run_steps(6);
    h
}

fn screen_of(h: &Harness<'_, MarkupCraftApp>, p: Point) -> Pos2 {
    let d = h.state().state.doc().unwrap();
    d.view.user_to_screen(0, p, d.render.as_ref().unwrap().pages()).unwrap()
}

fn press(h: &mut Harness<'_, MarkupCraftApp>, at: Pos2, pressed: bool) {
    h.event(Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
    h.step();
}

fn drag(h: &mut Harness<'_, MarkupCraftApp>, from: Pos2, to: Pos2) {
    h.hover_at(from);
    h.step();
    press(h, from, true);
    for i in 1..=6 {
        h.hover_at(from + (to - from) * (i as f32 / 6.0));
        h.step();
    }
    press(h, to, false);
    h.run_steps(2);
}

fn square_center(h: &Harness<'_, MarkupCraftApp>) -> Point {
    let m = h.state().state.doc().unwrap().session.doc().find(SQUARE).unwrap();
    Point::new((m.rect.x0 + m.rect.x1) / 2.0, (m.rect.y0 + m.rect.y1) / 2.0)
}

#[test]
fn click_selects_drag_moves_delete_and_undo() {
    let mut h = harness();
    let at = screen_of(&h, Point::new(950.0, 650.0));
    h.hover_at(at);
    h.step();
    press(&mut h, at, true);
    press(&mut h, at, false);
    h.run_steps(2);
    assert_eq!(h.state().state.doc().unwrap().selection(), vec![SQUARE.to_string()]);

    // Drag 60 screen points to the right: the square follows in page units.
    let k = {
        let d = h.state().state.doc().unwrap();
        d.view.zoom * markupcraft_ui_egui::canvas::PT
    };
    drag(&mut h, at, at + vec2(60.0, 0.0));
    let c = square_center(&h);
    assert!((c.x - (950.0 + f64::from(60.0 / k))).abs() < 1.0, "moved to {c:?}");
    assert!((c.y - 650.0).abs() < 0.5);

    h.key_press(Key::Delete);
    h.run_steps(2);
    assert!(h.state().state.doc().unwrap().session.doc().find(SQUARE).is_none());

    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert!(h.state().state.doc().unwrap().session.doc().find(SQUARE).is_some());
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert!((square_center(&h).x - 950.0).abs() < 0.5, "move undone");
}

#[test]
fn rectangle_tool_draws_and_returns_to_select() {
    let mut h = harness();
    h.key_press(Key::R);
    h.run_steps(2);
    assert_eq!(h.state().state.tool, "rectangle");
    let before = h.state().state.doc().unwrap().session.doc().markups.len();
    let a = screen_of(&h, Point::new(150.0, 400.0));
    let b = screen_of(&h, Point::new(250.0, 330.0));
    drag(&mut h, a, b);
    let s = &h.state().state;
    let d = s.doc().unwrap();
    assert_eq!(d.session.doc().markups.len(), before + 1);
    let m = d.session.doc().markups.last().unwrap();
    assert_eq!(m.kind, markupcraft_model::Kind::Rectangle);
    assert!(
        (m.rect.x0 - 150.0).abs() < 1.0 && (m.rect.y1 - 400.0).abs() < 1.0,
        "{:?}",
        m.rect
    );
    assert_eq!(d.selection(), vec![m.id.clone()]);
    assert_eq!(s.tool, "select");
    assert!(d.session.is_dirty());
}

#[test]
fn box_select_and_view_commands() {
    let mut h = harness();
    // A box around the square and the ellipse, from empty paper.
    let a = screen_of(&h, Point::new(690.0, 720.0));
    let b = screen_of(&h, Point::new(1010.0, 550.0));
    drag(&mut h, a, b);
    let mut sel = h.state().state.doc().unwrap().selection().to_vec();
    sel.sort();
    assert_eq!(sel, vec!["SAMPLECIRCLEAAAA".to_string(), SQUARE.to_string()]);

    // Ctrl+5 / Ctrl+4 switch page modes; Ctrl+Right turns the page.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num4);
    h.key_press_modifiers(Modifiers::COMMAND, Key::ArrowRight);
    h.run_steps(3);
    let v = &h.state().state.doc().unwrap().view;
    assert_eq!(v.mode, markupcraft_ui_egui::canvas::PageMode::Single);
    assert_eq!(v.current, 1);
    // Plus zooms in.
    let z = v.zoom;
    h.key_press(Key::Plus);
    h.run_steps(2);
    assert!(h.state().state.doc().unwrap().view.zoom > z * 1.2);
}
