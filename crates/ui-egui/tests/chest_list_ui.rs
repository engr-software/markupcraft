//! The Tool Chest's views, tool set scale, import and export; customised keyboard shortcuts;
//! the Markups List's saved views and Copy Rows (egui_kittest, headless).

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Kind, Markup, Scale};
use markupcraft_ui_egui::MarkupCraftApp;
use markupcraft_ui_egui::chest::Mode;
use markupcraft_ui_egui::chest_sets::ChestView;

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

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("markupcraft-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn tool_chest_symbol_view_scale_export_and_import() {
    let mut h = harness();
    // A Drawing-mode rectangle in a new set drawn at 1/4" = 1'.
    let set = h.state_mut().state.toolchest.add_set("Symbols");
    let mut m = Markup::new(Kind::Rectangle, 0, Rect::new(0.0, 0.0, 40.0, 20.0).corners().to_vec());
    m.subject = "Equipment Pad".into();
    let item = h.state_mut().state.toolchest.add_markup(&set, &m).unwrap();
    h.state_mut()
        .state
        .toolchest
        .update_item(&set, &item, |i| i.mode = Mode::Drawing);
    h.state_mut()
        .state
        .toolchest
        .set_scale(&set, Some(Scale::architectural(0.25, 1.0)));
    h.state_mut().set_option("panel", "toolchest");
    h.run_steps(4);
    // Symbol view: tiles named by their items.
    h.get_by_label("Symbol").click();
    h.run_steps(3);
    assert_eq!(h.state().state.toolchest.view, ChestView::Symbol);
    h.get_by_label("Equipment Pad").click();
    h.run_steps(3);
    assert_eq!(h.state().state.tool, "rectangle");
    // Placed on the 1/8" page: half the paper size, the same real size.
    click(&mut h, 150.0, 600.0);
    let placed = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .markups
        .last()
        .unwrap()
        .clone();
    let b = bbox(&placed.pts).unwrap();
    assert!(
        (b.width() - 20.0).abs() < 1e-3 && (b.height() - 10.0).abs() < 1e-3,
        "{b:?}"
    );
    h.get_by_label("Detail").click();
    h.run_steps(2);
    assert_eq!(h.state().state.toolchest.view, ChestView::Detail);

    // Export the set, import it back as a new set.
    let dir = temp_dir("toolset");
    let file = dir.join("symbols.mctools");
    h.state_mut().state.dialogs.scripted = Some(vec![file.clone()]);
    h.state_mut().state.dialogs.save(
        markupcraft_ui_egui::dialogs::Purpose::Edit("export_toolset", set.clone()),
        markupcraft_ui_egui::dialogs::TOOLSET,
        "x",
    );
    h.run_steps(2);
    assert!(file.is_file(), "{}", h.state().state.status);
    let n = h.state().state.toolchest.sets.len();
    h.get_by_label("Import...").click();
    h.run_steps(3);
    let chest = &h.state().state.toolchest;
    assert_eq!(chest.sets.len(), n + 1, "{}", h.state().state.status);
    let imported = chest.sets.last().unwrap();
    assert_eq!(imported.title, "Symbols");
    assert_eq!(imported.items.len(), 1);
    assert!(imported.scale.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keyboard_shortcuts_can_be_changed() {
    let mut h = harness();
    h.state_mut().state.queue("tools.customize_keys");
    h.run_steps(3);
    assert!(h.state().state.keys.show);
    // Save All's key button shows its default (Shift+F2); click it, then press the new keys.
    h.get_by_label("Shift+F2").click();
    h.run_steps(2);
    assert_eq!(h.state().state.keys.capturing.as_deref(), Some("file.save_all"));
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::K);
    h.run_steps(2);
    let k = h.state().state.keys.keys_for("file.save_all").unwrap();
    assert!(k.ctrl && k.alt && k.key == Key::K, "{}", h.state().state.status);
    // The new keys run the command.
    h.state_mut().state.keys.show = false;
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::K);
    h.run_steps(2);
    assert_eq!(h.state().state.status, "Nothing to save");
}

#[test]
fn markups_list_saves_views_and_copies_rows() {
    let mut h = harness();
    h.state_mut().state.list.view.search = "Area".into();
    h.run_steps(2);
    h.get_by_label("Views").click();
    h.run_steps(2);
    h.state_mut().state.list.prefs.new_name = "Areas only".into();
    h.run_steps(2);
    h.get_by_label("Save View").click();
    h.run_steps(2);
    assert_eq!(h.state().state.list.prefs.views.len(), 1);
    h.state_mut().state.list.view.search.clear();
    h.run_steps(2);
    h.get_by_label("Views").click();
    h.run_steps(2);
    h.get_by_label("Areas only").click();
    h.run_steps(2);
    assert_eq!(h.state().state.list.view.search, "Area");
    // Copy Rows from a row's menu.
    h.state_mut().state.list.view.search.clear();
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .select(&["SAMPLEAREAAAAAAA".to_string()])
        .unwrap();
    h.run_steps(3);
    h.get_all_by_label("629.63 sf").next().unwrap().click_secondary();
    h.run_steps(2);
    h.get_by_label("Copy Rows").click();
    h.run_steps(1);
    let copied: Vec<String> = h
        .output()
        .platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    let text = copied.last().cloned().unwrap_or_default();
    assert!(text.starts_with("Subject\t"), "{text:?}");
    assert!(
        text.lines()
            .nth(1)
            .is_some_and(|l| l.starts_with("Area\t") && l.contains("629.63 sf")),
        "{text:?}"
    );
}
