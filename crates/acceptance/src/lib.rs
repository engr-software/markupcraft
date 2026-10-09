//! Blind acceptance testing.
//!
//! Every row of `parity/revu-features.toml` is checked by a test written from the inventory
//! text (`docs/revu_features/*.md`) BEFORE reading how MarkupCraft implements it, by someone who
//! did not build it. Tests drive the product the way a user or an agent would: the automation
//! tools (`Automation::call`, the same table as `markupcraft-cli run` and MCP) and the real UI
//! headlessly (egui_kittest). A row's `accepted` field lists its acceptance tests and
//! `acceptance` records the verdict: `pass`, `fixed` (a bug was found and fixed, with a
//! regression test) or `gap` (behavior differs from the inventory; the row's notes say how).
//!
//! One test file per slice in `tests/` (`measurement.rs`, `markups.rs`, ...). Shared helpers live
//! here; slice-specific helpers stay in the slice file so parallel work never collides.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub use egui;
pub use egui_kittest::{Harness, kittest::Queryable};
pub use markupcraft_automation::Automation;
pub use markupcraft_engine::synthetic;
pub use markupcraft_geom::Point;
pub use markupcraft_model::Kind;
pub use markupcraft_ui_egui::MarkupCraftApp;
pub use serde_json::{Value, json};

/// A fresh, empty folder for one test (`<temp>/markupcraft-acceptance-<pid>-<n>-<tag>`).
pub fn temp_dir(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-acceptance-{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The automation tool table rooted at `dir` (every path a tool touches stays inside it).
/// Preferences, stamps and tool sets go to `dir/config`, never the user's real config folder.
pub fn automation(dir: &Path) -> Automation {
    let config = dir.join("config");
    std::fs::create_dir_all(&config).unwrap();
    Automation::new()
        .with_root(dir)
        .unwrap()
        .with_author("Acceptance")
        .with_config_dir(config)
}

/// Call a tool; panic with the tool's own error message if it fails.
pub fn call(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args) {
        Ok(v) => v,
        Err(e) => panic!("{tool} {args}: {e}"),
    }
}

/// Call a tool that must fail; returns its error message.
pub fn fails(a: &mut Automation, tool: &str, args: Value) -> String {
    match a.call(tool, &args) {
        Ok(v) => panic!("{tool} should fail, got {v}"),
        Err(e) => e.to_string(),
    }
}

/// A two-page plan-like sample PDF (walls, rooms, grid lines, text) written into `dir`.
pub fn sample_pdf(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, markupcraft_render::synthetic::sample_pdf().as_slice()).unwrap();
    p
}

/// The real app, headless, with the sample plan open (1500 x 950 window, 60 Hz steps).
pub fn app() -> Harness<'static, MarkupCraftApp> {
    markupcraft_ui_egui::richedit::load_dictionary_blocking();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
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

/// Screen position of a point in page user space on the current page.
pub fn screen(h: &Harness<'_, MarkupCraftApp>, x: f64, y: f64) -> egui::Pos2 {
    let d = h.state().state.doc().unwrap();
    let page = d.view.current;
    d.view
        .user_to_screen(page, Point::new(x, y), d.render.as_ref().unwrap().pages())
        .unwrap()
}

/// Press or release the primary button at a screen position.
pub fn button(h: &mut Harness<'_, MarkupCraftApp>, at: egui::Pos2, pressed: bool, modifiers: egui::Modifiers) {
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    });
    h.step();
}

/// Click at a page point.
pub fn click(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.step();
    button(h, at, true, egui::Modifiers::NONE);
    button(h, at, false, egui::Modifiers::NONE);
    h.run_steps(3);
}

/// Drag between two page points with the primary button.
pub fn drag(h: &mut Harness<'_, MarkupCraftApp>, from: (f64, f64), to: (f64, f64)) {
    let (a, b) = (screen(h, from.0, from.1), screen(h, to.0, to.1));
    h.hover_at(a);
    h.step();
    button(h, a, true, egui::Modifiers::NONE);
    for i in 1..=8 {
        let t = i as f32 / 8.0;
        h.hover_at(a + (b - a) * t);
        h.step();
    }
    button(h, b, false, egui::Modifiers::NONE);
    h.run_steps(3);
}

/// Press a key with modifiers.
pub fn key(h: &mut Harness<'_, MarkupCraftApp>, modifiers: egui::Modifiers, k: egui::Key) {
    h.event(egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    h.step();
    h.event(egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers,
    });
    h.run_steps(3);
}

/// Run a command from the command table (menus, toolbars and shortcuts all go through it).
pub fn run(h: &mut Harness<'_, MarkupCraftApp>, id: &str) {
    h.state_mut().state.queue(id);
    h.run_steps(4);
}

/// Whether any widget's label contains `text`.
pub fn shows(h: &Harness<'_, MarkupCraftApp>, text: &str) -> bool {
    h.query_all_by_label_contains(text).next().is_some()
}

/// The open document's markups.
pub fn markups(h: &Harness<'_, MarkupCraftApp>) -> Vec<markupcraft_model::Markup> {
    h.state()
        .state
        .doc()
        .map(|d| d.session.doc().markups.clone())
        .unwrap_or_default()
}
