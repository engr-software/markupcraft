//! Text markups in the shell (egui_kittest): the canvas draws a text box's text, rich text
//! typed with Ctrl+B / Ctrl+I and the toolbar saves as `/RC` spans and reloads editable, and
//! the editor's spelling suggestions replace a misspelled word.

use egui::text::{CCursor, CCursorRange};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_geom::Point;
use markupcraft_model::{Kind, Markup};
use markupcraft_ui_egui::MarkupCraftApp;

const TEXT: &str = "SAMPLETEXTAAAAAA";

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

fn click(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.step();
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(2);
}

fn last(h: &Harness<'_, MarkupCraftApp>) -> Markup {
    h.state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .markups
        .last()
        .unwrap()
        .clone()
}

fn editor_id() -> egui::Id {
    egui::Id::new("markupcraft-text-editor").with("edit")
}

/// Select chars `a..b` in the open editor.
fn select_chars(h: &mut Harness<'_, MarkupCraftApp>, a: usize, b: usize) {
    let ctx = h.ctx.clone();
    let mut state = egui::TextEdit::load_state(&ctx, editor_id()).expect("the editor is open");
    state
        .cursor
        .set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
    state.store(&ctx, editor_id());
    h.run_steps(1);
}

#[test]
fn the_canvas_draws_a_text_box_text() {
    let mut h = harness();
    let m = h.state().state.doc().unwrap().session.doc().find(TEXT).unwrap().clone();
    // The sample's /C equals its text colour and it has no /DS: /C is the border, not a fill
    // that would hide the text.
    assert_eq!(m.fill, None);
    assert_eq!(m.contents, "Verify sink location");
    h.run_steps(4);
    // The painted frame holds the text, inside the box, in the text colour.
    let (a, b) = (screen(&h, 640.0, 236.0), screen(&h, 900.0, 200.0));
    let boxed = egui::Rect::from_two_pos(a, b);
    let mut found = false;
    let mut red_fill = false;
    for cs in &h.output().shapes {
        match &cs.shape {
            egui::Shape::Text(t)
                if t.galley.job.text.contains("Verify sink location") && boxed.expand(4.0).contains(t.pos) =>
            {
                found = true;
            }
            egui::Shape::Rect(r) if r.rect.contains_rect(boxed.shrink(2.0)) && r.fill.r() > 200 && r.fill.g() < 60 => {
                red_fill = true;
            }
            _ => {}
        }
    }
    assert!(found, "the text is painted");
    assert!(!red_fill, "no red bar over the text");
}

#[test]
fn rich_text_saves_as_spans_and_reloads_editable() {
    let mut h = harness();
    h.state_mut().set_option("tool", "text");
    h.run_steps(2);
    click(&mut h, 100.0, 700.0);
    h.event(Event::Text("Hello bold world".into()));
    h.run_steps(2);
    select_chars(&mut h, 6, 10);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    select_chars(&mut h, 11, 16);
    h.get_by_label("Blue").click();
    h.run_steps(2);
    assert!(
        h.state().state.doc().unwrap().view.editor.is_some(),
        "the toolbar keeps the editor open"
    );
    h.key_press(Key::Escape);
    h.run_steps(2);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Text);
    assert_eq!(m.contents, "Hello bold world");
    let bold: Vec<_> = m.rich.iter().filter(|r| r.bold).map(|r| (r.start, r.end)).collect();
    assert_eq!(bold, vec![(6, 10)], "{:?}", m.rich);
    assert!(m.rich.iter().any(|r| r.start == 11 && r.end == 16 && r.color.b > 0.9));

    let dir = std::env::temp_dir().join(format!("markupcraft-rich-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rich.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![path.clone()]);
    h.state_mut().state.queue("file.save");
    h.run_steps(4);
    let (_f, doc) = markupcraft_revu::open(&path).unwrap();
    let back = doc.find(&m.id).unwrap();
    assert!(!back.foreign_look, "our rich text stays editable");
    assert_eq!(back.rich, m.rich);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn spelling_suggestions_replace_a_misspelled_word() {
    let mut h = harness();
    h.state_mut().set_option("tool", "text");
    h.run_steps(2);
    click(&mut h, 100.0, 700.0);
    h.event(Event::Text("the qwick fox".into()));
    h.run_steps(2);
    for _ in 0..600 {
        if markupcraft_ui_egui::richedit::dictionary_settled() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        markupcraft_ui_egui::richedit::dictionary().is_some(),
        "the en_US dictionary loads"
    );
    select_chars(&mut h, 6, 6);
    h.run_steps(2);
    h.get_by_label("quick").click();
    h.run_steps(2);
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert_eq!(last(&h).contents, "the quick fox");
}
