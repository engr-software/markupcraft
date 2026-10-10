//! The application shell driven headlessly (egui_kittest, no window): document tabs, view
//! modes and history, rotate view, zoom tool, wheel preferences, split views with sync,
//! rulers, crosshair and dimmer, full screen and presentation, bars and toolbars, the panel
//! layout, Preferences, recent files and File Access, Thumbnails and the page dialogs, and
//! Revu's default keys for the Window, View, Document and File groups.

use std::path::{Path, PathBuf};

use egui::{Event, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_engine::prefs::PrefStore;
use markupcraft_geom::Point;
use markupcraft_ui_egui::MarkupCraftApp;
use markupcraft_ui_egui::canvas::{Fit, PageMode};
use markupcraft_ui_egui::shell::{self, Screen};

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

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("markupcraft-shell-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A copy of the sample drawing set on disk.
fn sample_file(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, markupcraft_render::synthetic::sample_pdf()).unwrap();
    p
}

fn run(h: &mut Harness<'_, MarkupCraftApp>, id: &str) {
    h.state_mut().state.queue(id);
    h.run_steps(3);
}

fn keys(h: &mut Harness<'_, MarkupCraftApp>, m: Modifiers, k: Key) {
    h.key_press_modifiers(m, k);
    h.run_steps(3);
}

fn view<'a>(h: &'a Harness<'_, MarkupCraftApp>) -> &'a markupcraft_ui_egui::canvas::DocView {
    &h.state().state.doc().unwrap().view
}

fn pages(h: &Harness<'_, MarkupCraftApp>) -> usize {
    h.state().state.doc().unwrap().session.page_count()
}

fn screen(h: &Harness<'_, MarkupCraftApp>, page: usize, x: f64, y: f64) -> Pos2 {
    let d = h.state().state.doc().unwrap();
    d.view
        .user_to_screen(page, Point::new(x, y), d.render.as_ref().unwrap().pages())
        .unwrap()
}

fn press(h: &mut Harness<'_, MarkupCraftApp>, at: Pos2, b: PointerButton, down: bool, m: Modifiers) {
    h.event(Event::PointerButton {
        pos: at,
        button: b,
        pressed: down,
        modifiers: m,
    });
    h.step();
}

fn wheel(h: &mut Harness<'_, MarkupCraftApp>, at: Pos2, dy: f32, m: Modifiers) {
    h.hover_at(at);
    h.step();
    h.event(Event::ModifiersChanged(m));
    h.event(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, dy),
        modifiers: m,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(2);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.step();
}

const C: Modifiers = Modifiers::COMMAND;
const S: Modifiers = Modifiers::SHIFT;
const A: Modifiers = Modifiers::ALT;

fn cs() -> Modifiers {
    Modifiers::COMMAND | Modifiers::SHIFT
}

#[test]
fn document_tabs_cycle_close_others_close_all_and_save_all() {
    let dir = temp_dir("tabs");
    let mut h = harness();
    let a = sample_file(&dir, "A-101 Floor Plan.pdf");
    let b = sample_file(&dir, "A-201 Elevations.pdf");
    h.state_mut().open_path(&a);
    h.state_mut().open_path(&b);
    h.run_steps(3);
    assert_eq!(h.state().state.docs.len(), 3);
    assert_eq!(h.state().state.active, 2);
    // Ctrl+Tab / Ctrl+Shift+Tab cycle the tabs.
    keys(&mut h, C, Key::Tab);
    assert_eq!(h.state().state.active, 0);
    keys(&mut h, cs(), Key::Tab);
    assert_eq!(h.state().state.active, 2);
    // Tabs move without changing the active document.
    let uid = h.state().state.doc().unwrap().uid;
    shell::tabs::move_tab(&mut h.state_mut().state, 2, 0);
    assert_eq!(h.state().state.docs[0].uid, uid);
    assert_eq!(h.state().state.active, 0);
    // Save All (Shift+F2) saves every edited file that has a path.
    {
        let st = &mut h.state_mut().state;
        for d in st.docs.iter_mut().filter(|d| d.path.is_some()) {
            d.session
                .move_markups(&["SAMPLESQUAREAAAA".to_string()], 10.0, 0.0)
                .unwrap();
        }
    }
    keys(&mut h, S, Key::F2);
    assert!(
        h.state()
            .state
            .docs
            .iter()
            .filter(|d| d.path.is_some())
            .all(|d| !d.session.is_dirty())
    );
    // Close Others keeps the active tab; Close All (Ctrl+Shift+W) closes the rest.
    run(&mut h, "file.close_others");
    assert_eq!(h.state().state.docs.len(), 1);
    assert_eq!(h.state().state.doc().unwrap().uid, uid);
    keys(&mut h, cs(), Key::W);
    assert!(h.state().state.docs.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn page_layout_modes_side_by_side_and_cover() {
    let mut h = harness();
    run(&mut h, "document.insert_blank");
    run(&mut h, "document.insert_blank");
    assert_eq!(pages(&h), 4);
    keys(&mut h, C, Key::Num6);
    assert_eq!(view(&h).mode, PageMode::SideBySide);
    // Two pages to a row: pages 0 and 1 side by side, same top.
    let p0 = screen(&h, 0, 0.0, 792.0);
    let p1 = screen(&h, 1, 0.0, 792.0);
    assert!((p0.y - p1.y).abs() < 1.0 && p1.x > p0.x, "{p0:?} {p1:?}");
    // Side by Side shows one spread: page 2 is not laid out.
    let d = h.state().state.doc().unwrap();
    assert!(
        d.view
            .user_to_screen(2, Point::new(0.0, 0.0), d.render.as_ref().unwrap().pages())
            .is_none()
    );
    // Continuous Side by Side (Ctrl+7) lays out every page; a cover page sits alone.
    keys(&mut h, C, Key::Num7);
    assert_eq!(view(&h).mode, PageMode::ContinuousSideBySide);
    run(&mut h, "view.cover_page");
    assert!(view(&h).cover);
    assert_eq!(view(&h).rows(4), vec![(0, 1), (1, 2), (3, 1)]);
    let p1 = screen(&h, 1, 0.0, 792.0);
    let p2 = screen(&h, 2, 0.0, 792.0);
    assert!((p1.y - p2.y).abs() < 1.0 && p2.x > p1.x);
    assert!(h.state().state.checked("view.continuous_side") == Some(true));
}

#[test]
fn previous_and_next_view_walk_the_history() {
    let mut h = harness();
    run(&mut h, "view.single_page");
    keys(&mut h, C, Key::ArrowRight);
    h.run_steps(3);
    assert_eq!(view(&h).current, 1);
    keys(&mut h, Modifiers::NONE, Key::Plus);
    h.run_steps(3);
    let zoomed = view(&h).zoom;
    // Alt+Left: back to the unzoomed page 2, then to page 1; Alt+Right forward again.
    keys(&mut h, A, Key::ArrowLeft);
    assert!((view(&h).zoom - zoomed).abs() > 0.01);
    assert_eq!(view(&h).current, 1);
    keys(&mut h, A, Key::ArrowLeft);
    assert_eq!(view(&h).current, 0);
    keys(&mut h, A, Key::ArrowRight);
    assert_eq!(view(&h).current, 1);
    keys(&mut h, A, Key::ArrowRight);
    assert!((view(&h).zoom - zoomed).abs() < 0.01);
    assert!(!h.state().state.enabled("view.next_view"));
}

#[test]
fn rotate_view_turns_the_display_not_the_file() {
    let mut h = harness();
    let before = screen(&h, 0, 100.0, 700.0);
    let corner = screen(&h, 0, 1200.0, 700.0);
    assert!(corner.x > before.x);
    keys(&mut h, cs(), Key::Plus);
    h.run_steps(3);
    assert_eq!(view(&h).rotation, 90);
    // Turned clockwise: what ran left to right now runs top to bottom.
    let a = screen(&h, 0, 100.0, 700.0);
    let b = screen(&h, 0, 1200.0, 700.0);
    assert!(b.y > a.y + 50.0 && (b.x - a.x).abs() < 1.0, "{a:?} {b:?}");
    // The file is unchanged and the page raster still arrives.
    let d = h.state().state.doc().unwrap();
    assert_eq!(d.session.doc().pages[0].rotate, 0);
    assert!(!d.session.is_dirty());
    keys(&mut h, cs(), Key::Minus);
    keys(&mut h, cs(), Key::Minus);
    assert_eq!(view(&h).rotation, 270);
}

#[test]
fn zoom_tool_clicks_and_boxes() {
    let mut h = harness();
    keys(&mut h, Modifiers::NONE, Key::Z);
    assert_eq!(h.state().state.tool, "zoom");
    let z0 = view(&h).zoom;
    let at = screen(&h, 0, 600.0, 400.0);
    h.hover_at(at);
    h.step();
    press(&mut h, at, PointerButton::Primary, true, Modifiers::NONE);
    press(&mut h, at, PointerButton::Primary, false, Modifiers::NONE);
    h.run_steps(2);
    assert!(view(&h).zoom > z0 * 1.2);
    // Ctrl+click zooms out.
    let z1 = view(&h).zoom;
    h.event(Event::ModifiersChanged(C));
    press(&mut h, at, PointerButton::Primary, true, C);
    press(&mut h, at, PointerButton::Primary, false, C);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
    assert!(view(&h).zoom < z1);
    // A box fills the view.
    let z2 = view(&h).zoom;
    let (a, b) = (screen(&h, 0, 300.0, 600.0), screen(&h, 0, 500.0, 450.0));
    h.hover_at(a);
    h.step();
    press(&mut h, a, PointerButton::Primary, true, Modifiers::NONE);
    for i in 1..=6 {
        h.hover_at(a + (b - a) * (i as f32 / 6.0));
        h.step();
    }
    press(&mut h, b, PointerButton::Primary, false, Modifiers::NONE);
    h.run_steps(2);
    assert!(view(&h).zoom > z2 * 2.0, "{} vs {z2}", view(&h).zoom);
    // Shift+Z toggles back to the tool before.
    keys(&mut h, S, Key::Z);
    assert_ne!(h.state().state.tool, "zoom");
    keys(&mut h, S, Key::Z);
    assert_eq!(h.state().state.tool, "zoom");
}

#[test]
fn fit_zoom_and_page_navigation_keys() {
    let mut h = harness();
    keys(&mut h, C, Key::Num8);
    assert!((view(&h).zoom - 1.0).abs() < 1e-3);
    keys(&mut h, C, Key::Num0);
    assert_eq!(view(&h).fit, Fit::Width);
    keys(&mut h, C, Key::Num9);
    assert_eq!(view(&h).fit, Fit::Page);
    let z = view(&h).zoom;
    keys(&mut h, Modifiers::NONE, Key::Minus);
    assert!(view(&h).zoom < z);
    run(&mut h, "view.single_page");
    keys(&mut h, Modifiers::NONE, Key::End);
    assert_eq!(view(&h).current, 1);
    keys(&mut h, Modifiers::NONE, Key::Home);
    assert_eq!(view(&h).current, 0);
    keys(&mut h, Modifiers::NONE, Key::PageDown);
    assert_eq!(view(&h).current, 1);
    keys(&mut h, C, Key::ArrowLeft);
    assert_eq!(view(&h).current, 0);
    // The maximum zoom preference caps zooming.
    // (Kept low: the test renderer's textures are at most 2048 pixels.)
    h.state_mut().state.shell.ui.max_zoom_pct = 120.0;
    h.run_steps(2);
    for _ in 0..20 {
        keys(&mut h, Modifiers::NONE, Key::Plus);
    }
    assert!(view(&h).zoom <= 1.2 + 1e-3);
}

#[test]
fn wheel_zooms_or_scrolls_by_preference_and_ctrl_swaps() {
    let mut h = harness();
    run(&mut h, "view.fit_width");
    let at = screen(&h, 0, 600.0, 400.0);
    // Default: the wheel zooms.
    let z = view(&h).zoom;
    wheel(&mut h, at, 120.0, Modifiers::NONE);
    assert!(view(&h).zoom > z * 1.1);
    // Ctrl+wheel scrolls instead.
    let (z, off) = (view(&h).zoom, view(&h).offset);
    wheel(&mut h, at, -120.0, C);
    assert!((view(&h).zoom - z).abs() < 1e-4);
    assert!(view(&h).offset.y > off.y);
    // Preference: the wheel scrolls in continuous modes; Ctrl now zooms; reverse flips it.
    h.state_mut().state.shell.ui.wheel_zooms_continuous = false;
    h.run_steps(2);
    let (z, off) = (view(&h).zoom, view(&h).offset);
    wheel(&mut h, at, -120.0, Modifiers::NONE);
    assert!((view(&h).zoom - z).abs() < 1e-4 && view(&h).offset.y > off.y);
    wheel(&mut h, at, 120.0, C);
    assert!(view(&h).zoom > z);
    h.state_mut().state.shell.ui.reverse_wheel = true;
    h.run_steps(2);
    let z = view(&h).zoom;
    wheel(&mut h, at, 120.0, C);
    assert!(view(&h).zoom < z);
}

#[test]
fn middle_drag_pans_and_double_click_recentres() {
    let mut h = harness();
    run(&mut h, "view.actual_size");
    h.run_steps(2);
    let off = view(&h).offset;
    let a = screen(&h, 0, 600.0, 400.0);
    let b = a + vec2(-120.0, -80.0);
    h.hover_at(a);
    h.step();
    press(&mut h, a, PointerButton::Middle, true, Modifiers::NONE);
    for i in 1..=6 {
        h.hover_at(a + (b - a) * (i as f32 / 6.0));
        h.step();
    }
    press(&mut h, b, PointerButton::Middle, false, Modifiers::NONE);
    h.run_steps(2);
    let moved = view(&h).offset - off;
    assert!(moved.x > 60.0 && moved.y > 40.0, "{moved:?}");
    // Middle double-click brings the clicked point to the middle of the view.
    h.run_steps(30);
    let centre = view(&h).viewport().center();
    let p = centre + vec2(150.0, 100.0);
    let off = view(&h).offset;
    h.hover_at(p);
    h.step();
    for _ in 0..2 {
        press(&mut h, p, PointerButton::Middle, true, Modifiers::NONE);
        press(&mut h, p, PointerButton::Middle, false, Modifiers::NONE);
    }
    h.run_steps(2);
    let moved = view(&h).offset - off;
    assert!((moved - vec2(150.0, 100.0)).length() < 1.0, "{moved:?}");
    // Holding Space pans with the left button whatever the tool, and keeps the tool.
    h.state_mut().state.set_tool("rectangle");
    h.run_steps(30);
    let off = view(&h).offset;
    let a = centre;
    let b = centre + vec2(-90.0, -60.0);
    h.hover_at(a);
    h.step();
    h.event(Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.step();
    press(&mut h, a, PointerButton::Primary, true, Modifiers::NONE);
    for i in 1..=6 {
        h.hover_at(a + (b - a) * (i as f32 / 6.0));
        h.step();
    }
    press(&mut h, b, PointerButton::Primary, false, Modifiers::NONE);
    h.event(Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    let moved = view(&h).offset - off;
    assert!(moved.x > 40.0 && moved.y > 30.0, "{moved:?}");
    assert_eq!(h.state().state.tool, "rectangle");
    assert_eq!(h.state().state.doc().unwrap().session.doc().markups.len(), 6);
}

#[test]
fn split_views_sync_switch_balance_unsplit() {
    let mut h = harness();
    keys(&mut h, C, Key::Num2);
    h.run_steps(4);
    let s = h.state().state.shell.split.as_ref().unwrap();
    assert!(s.vertical);
    assert_eq!(s.pane.uid, h.state().state.doc().unwrap().uid);
    // The second pane renders the same document on its own.
    assert!(s.pane.render.is_some());
    // Ctrl+I flips to a horizontal split; Shift+F12 evens the panes.
    keys(&mut h, C, Key::I);
    assert!(!h.state().state.shell.split.as_ref().unwrap().vertical);
    h.state_mut().state.shell.split.as_mut().unwrap().frac = 0.3;
    keys(&mut h, S, Key::F12);
    assert_eq!(h.state().state.shell.split.as_ref().unwrap().frac, 0.5);
    // Synchronize Document: the second pane follows page and zoom.
    run(&mut h, "view.sync_document");
    run(&mut h, "view.single_page");
    keys(&mut h, C, Key::ArrowRight);
    keys(&mut h, Modifiers::NONE, Key::Plus);
    h.run_steps(3);
    let main = (view(&h).current, view(&h).zoom);
    let pane = &h.state().state.shell.split.as_ref().unwrap().pane.view;
    assert_eq!(pane.current, main.0);
    assert!((pane.zoom - main.1).abs() < 1e-4);
    // Synchronize Page: zooming one zooms the other by as much.
    run(&mut h, "view.sync_page");
    h.run_steps(2);
    let pz = h.state().state.shell.split.as_ref().unwrap().pane.view.zoom;
    keys(&mut h, Modifiers::NONE, Key::Plus);
    h.run_steps(2);
    let pz2 = h.state().state.shell.split.as_ref().unwrap().pane.view.zoom;
    assert!(pz2 > pz * 1.1, "{pz} -> {pz2}");
    // Ctrl+1 switches what the panes show (one document: no change); a second document.
    h.state_mut()
        .open_bytes("other.pdf", None, markupcraft_render::synthetic::sample_pdf())
        .unwrap();
    h.run_steps(3);
    let other = h.state().state.doc().unwrap().uid;
    keys(&mut h, C, Key::Num1);
    assert_eq!(h.state().state.shell.split.as_ref().unwrap().pane.uid, other);
    assert_ne!(h.state().state.doc().unwrap().uid, other);
    // Ctrl+Shift+2 unsplits.
    keys(&mut h, cs(), Key::Num2);
    assert!(h.state().state.shell.split.is_none());
    keys(&mut h, C, Key::H);
    assert!(!h.state().state.shell.split.as_ref().unwrap().vertical);
}

#[test]
fn rulers_crosshair_and_dimmer() {
    let mut h = harness();
    keys(&mut h, C, Key::R);
    assert!(h.state().state.shell.ui.rulers);
    // The canvas shrank by the rulers: page 1's corner moved right and down.
    let st = &h.state().state;
    assert_eq!(st.checked("view.rulers"), Some(true));
    let unit = st.shell.ui.ruler_unit;
    assert_eq!(unit, shell::RulerUnit::In);
    assert!((shell::RulerUnit::Cm.points() - 28.346).abs() < 0.01);
    // The pointer position reads in ruler units on the status bar.
    let at = screen(&h, 0, 72.0 * 2.0, 0.0);
    h.hover_at(at);
    h.run_steps(2);
    let r = shell::overlay::pointer_readout(&h.state().state).unwrap();
    assert!(r.ends_with(" in"), "{r}");
    h.state_mut().state.shell.ui.ruler_unit = shell::RulerUnit::Mm;
    h.run_steps(2);
    let r = shell::overlay::pointer_readout(&h.state().state).unwrap();
    assert!(r.ends_with(" mm"), "{r}");
    run(&mut h, "view.crosshair");
    assert!(h.state().state.shell.ui.crosshair);
    // The status bar shows the page size; Shift+F9 shows the grid.
    assert_eq!(
        shell::overlay::page_size_readout(&h.state().state).as_deref(),
        Some("17.00 x 11.00 in")
    );
    keys(&mut h, S, Key::F9);
    assert_eq!(h.state().state.checked("view.show_grid"), Some(true));
    // The page scale on the navigation bar starts Calibrate.
    h.get_by_label("Scale 1/8 in = 1 ft").click();
    h.run_steps(3);
    assert_eq!(h.state().state.tool, "calibrate");
    keys(&mut h, Modifiers::NONE, Key::Escape);
    // Ctrl+F5: the dimmer fades the page content by the preference amount.
    keys(&mut h, C, Key::F5);
    assert!(h.state().state.shell.dimmer);
    assert!((view(&h).opts.dim - 0.5).abs() < 1e-4);
    h.state_mut().state.shell.ui.dimmer_pct = 80.0;
    h.run_steps(2);
    assert!((view(&h).opts.dim - 0.8).abs() < 1e-4);
    keys(&mut h, C, Key::F5);
    assert_eq!(view(&h).opts.dim, 0.0);
}

#[test]
fn full_screen_presentation_always_on_top_and_bars() {
    let mut h = harness();
    keys(&mut h, Modifiers::NONE, Key::F11);
    assert_eq!(h.state().state.shell.screen, Screen::FullScreen);
    assert!(!h.state().state.shell.menu_visible());
    keys(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(h.state().state.shell.screen, Screen::Normal);
    // Presentation: one page at a time, arrows advance, Esc returns to the old layout.
    keys(&mut h, C, Key::Enter);
    assert_eq!(h.state().state.shell.screen, Screen::Presentation);
    assert_eq!(view(&h).mode, PageMode::Single);
    keys(&mut h, Modifiers::NONE, Key::ArrowRight);
    assert_eq!(view(&h).current, 1);
    keys(&mut h, Modifiers::NONE, Key::ArrowLeft);
    assert_eq!(view(&h).current, 0);
    // Loop (Preferences > Window > Presentation): past the last page comes the first.
    h.state_mut().state.shell.ui.presentation_loop = true;
    keys(&mut h, Modifiers::NONE, Key::ArrowRight);
    keys(&mut h, Modifiers::NONE, Key::ArrowRight);
    assert_eq!(view(&h).current, 0);
    keys(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(h.state().state.shell.screen, Screen::Normal);
    assert_eq!(view(&h).mode, PageMode::Continuous);
    // Ctrl+F12 always on top; F9 / F4 / F8 menu, navigation and status bars; Shift+F4 panels.
    keys(&mut h, C, Key::F12);
    assert!(h.state().state.shell.always_on_top);
    keys(&mut h, Modifiers::NONE, Key::F9);
    assert!(!h.state().state.shell.ui.show_menu);
    keys(&mut h, Modifiers::NONE, Key::F4);
    assert!(!h.state().state.shell.ui.show_nav_bar);
    keys(&mut h, Modifiers::NONE, Key::F8);
    assert!(!h.state().state.shell.ui.show_status_bar);
    let open = h.state().state.open_panels.len();
    assert!(open > 3);
    keys(&mut h, S, Key::F4);
    h.run_steps(2);
    assert!(h.state().state.open_panels.is_empty());
    keys(&mut h, S, Key::F4);
    h.run_steps(2);
    assert_eq!(h.state().state.open_panels.len(), open);
}

#[test]
fn toolbars_show_hide_customize_and_lock() {
    let mut h = harness();
    let tb = h.state().state.shell.ui.toolbars.clone();
    assert!(tb.main.iter().any(|i| i == "tool.zoom"));
    run(&mut h, "window.toolbar_markup");
    assert!(!h.state().state.shell.ui.toolbars.show_markup);
    // Customize: add and remove a command.
    assert!(shell::toolbars::add(&mut h.state_mut().state, "view.rulers"));
    h.run_steps(2);
    assert!(
        h.state()
            .state
            .shell
            .ui
            .toolbars
            .main
            .iter()
            .any(|i| i == "view.rulers")
    );
    assert!(shell::toolbars::remove(&mut h.state_mut().state, "edit.delete"));
    assert!(!shell::toolbars::add(&mut h.state_mut().state, "no.such.command"));
    // The Customize window opens; locked toolbars refuse changes.
    run(&mut h, "window.customize_toolbars");
    assert!(h.query_by_label("Add Separator").is_some());
    run(&mut h, "window.lock_toolbars");
    assert!(!shell::toolbars::add(&mut h.state_mut().state, "view.dimmer"));
    h.run_steps(2);
    assert!(h.query_by_label("Add Separator").is_none());
}

fn store_app(dir: &Path) -> Harness<'static, MarkupCraftApp> {
    let dir = dir.to_path_buf();
    let mut h = Harness::builder()
        .with_size(vec2(1500.0, 950.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(move |_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            app.state.shell.store = Some(PrefStore::new(&dir));
            app.state.shell.recent_path = Some(dir.join("recent.json"));
            markupcraft_ui_egui::prefs_ui::load_from_store(&mut app.state);
            app
        });
    h.run_steps(4);
    h
}

#[test]
fn preferences_dialog_applies_and_persists() {
    let dir = temp_dir("prefs");
    let mut h = store_app(&dir);
    h.state_mut()
        .open_bytes("sample.pdf", None, markupcraft_render::synthetic::sample_pdf())
        .unwrap();
    h.run_steps(3);
    keys(&mut h, C, Key::K);
    assert!(h.state().state.shell.show_prefs);
    h.get_by_label("Grid & Snap").click();
    h.run_steps(2);
    assert!(!h.state().state.snaps.grid);
    h.get_by_label("Snap to grid").click();
    h.run_steps(3);
    assert!(h.state().state.snaps.grid);
    // Saved to the active profile.
    let saved = markupcraft_engine::prefs::PrefStore::new(&dir).load().unwrap();
    assert!(saved.snapping.grid);
    // General: dark theme applies at once.
    h.get_by_label("General").click();
    h.run_steps(2);
    h.get_by_label("Dark").click();
    h.run_steps(3);
    assert_eq!(h.state().state.shell.applied_theme, Some(true));
    // Navigation and Tools write the interface preferences file.
    // (The menu bar has a Tools menu too: the page list's entry is the last one.)
    h.get_all_by_label("Tools").last().unwrap().click();
    h.run_steps(2);
    h.get_by_label("Reuse markup tools (stay in the tool after placing a markup)")
        .click();
    h.run_steps(3);
    assert!(h.state().state.tool_locked);
    // Snap sensitivity and grid spacing from Grid & Snap drive the snapping.
    assert_eq!(markupcraft_ui_egui::snapping::reach(), 8.0);
    assert_eq!(markupcraft_ui_egui::snapping::grid(), 18.0);
    // The user name becomes the author of new markups in every open document.
    h.state_mut().state.shell.prefs.author = "Estimator Two".into();
    markupcraft_ui_egui::prefs_ui::apply_live(&mut h.state_mut().state);
    assert_eq!(h.state().state.author, "Estimator Two");
    let ui = shell::load_ui(&PrefStore::new(&dir));
    assert!(ui.reuse_tools);
    // Admin: profiles.
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "new", "Estimates");
    assert_eq!(PrefStore::new(&dir).active(), "Estimates");
    assert!(h.state().state.shell.prefs.snapping.grid);
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "switch", "Default");
    assert_eq!(PrefStore::new(&dir).active(), "Default");
    // Back up and restore through the file dialog.
    let backup = dir.join("backup.json");
    h.state_mut().state.dialogs.scripted = Some(vec![backup.clone()]);
    h.get_by_label("Admin").click();
    h.run_steps(2);
    h.get_by_label("Back Up Settings...").click();
    h.run_steps(4);
    assert!(backup.is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn layout_persists_and_a_new_session_restores_it() {
    let dir = temp_dir("layout");
    {
        let mut h = store_app(&dir);
        assert!(h.state().state.open_panels.contains(&"bookmarks"));
        keys(&mut h, A, Key::B);
        h.run_steps(3);
        assert!(!h.state().state.open_panels.contains(&"bookmarks"));
    }
    let mut h = store_app(&dir);
    h.run_steps(3);
    assert!(!h.state().state.open_panels.contains(&"bookmarks"));
    assert!(h.state().state.open_panels.contains(&"thumbnails"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn recent_files_reopen_at_the_last_page_and_pin() {
    let dir = temp_dir("recent");
    let a = sample_file(&dir, "plan.pdf");
    let b = sample_file(&dir, "details.pdf");
    let mut h = store_app(&dir);
    h.state_mut().open_path(&a);
    h.run_steps(3);
    run(&mut h, "view.single_page");
    keys(&mut h, C, Key::ArrowRight);
    h.state_mut().open_path(&b);
    h.run_steps(3);
    let st = &h.state().state.shell.recent;
    assert_eq!(st.files.len(), 2);
    assert_eq!(st.sorted(shell::recent::Sort::Date)[0].path, b);
    assert_eq!(st.last_session.len(), 2);
    // Close the plan and open it again: page 2, single page.
    let i = h
        .state()
        .state
        .docs
        .iter()
        .position(|d| d.path.as_deref() == Some(&a))
        .unwrap();
    h.state_mut().state.close_doc(i);
    h.run_steps(2);
    h.state_mut().open_path(&a);
    h.run_steps(3);
    assert_eq!(view(&h).current, 1);
    assert_eq!(view(&h).mode, PageMode::Single);
    let st = &h.state().state.shell.recent;
    assert_eq!(st.find(&a).unwrap().count, 2);
    assert_eq!(st.sorted(shell::recent::Sort::MostUsed)[0].path, a);
    // Pin into a category; the store persists.
    h.state_mut().state.shell.recent.toggle_pin(&b, "Architectural");
    shell::recent::persist(&mut h.state_mut().state);
    let back = shell::recent::RecentStore::load(&dir.join("recent.json"));
    assert!(back.is_pinned(&b));
    assert_eq!(back.pinned[0].category, "Architectural");
    // File Access (Alt+A) toggles; File > Open Recent lists the files.
    let open = h.state().state.open_panels.contains(&"file_access");
    keys(&mut h, A, Key::A);
    assert_ne!(h.state().state.open_panels.contains(&"file_access"), open);
    // A new session reopens the last session's files.
    let mut again = markupcraft_ui_egui::AppState::default();
    again.threads = 0;
    again.shell.recent = shell::recent::RecentStore::load(&dir.join("recent.json"));
    shell::recent::reopen_last_session(&mut again);
    assert_eq!(again.docs.len(), 2);
    // Clear Recent Files.
    run(&mut h, "file.clear_recent");
    assert!(h.state().state.shell.recent.files.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_blank_pdf_and_refresh_from_disk() {
    let dir = temp_dir("refresh");
    let mut h = harness();
    keys(&mut h, C, Key::N);
    let d = h.state().state.doc().unwrap();
    assert_eq!(d.name, "Untitled 1.pdf");
    assert_eq!(d.session.page_count(), 1);
    // Shift+F5 reloads the file from disk (another program changed it).
    let p = sample_file(&dir, "sheet.pdf");
    h.state_mut().open_path(&p);
    h.run_steps(3);
    assert_eq!(pages(&h), 2);
    std::fs::write(&p, markupcraft_engine::blank::pdf_bytes(&[(612.0, 792.0); 3]).unwrap()).unwrap();
    keys(&mut h, S, Key::F5);
    assert_eq!(pages(&h), 3);
    // F5 redraws the view.
    keys(&mut h, Modifiers::NONE, Key::F5);
    assert_eq!(h.state().state.status, "Refreshed");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn thumbnails_select_reorder_and_page_commands() {
    use markupcraft_ui_egui::panels::thumbnails;
    let mut h = harness();
    run(&mut h, "document.insert_blank");
    assert_eq!(pages(&h), 3);
    // The panel shows each page's label; the size slider and label toggles change it.
    assert!(h.query_by_label("Labels").is_some());
    h.state_mut().state.shell.thumbs.show_scale = true;
    h.run_steps(3);
    assert!(h.query_all_by_label_contains("1/8 in = 1 ft").next().is_some());
    // Select pages 1 and 3 (Ctrl+click), rotate them.
    {
        let st = &mut h.state_mut().state.shell.thumbs;
        thumbnails::click(st, 0, false, false);
        thumbnails::click(st, 2, true, false);
    }
    thumbnails::command(&mut h.state_mut().state, "rotate_cw", &[0, 2]);
    h.run_steps(2);
    let r: Vec<i32> = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .pages
        .iter()
        .map(|p| p.rotate)
        .collect();
    assert_eq!(r, vec![90, 0, 90]);
    // Drag page 3 to the front.
    let first_label = h.state().state.doc().unwrap().session.doc().pages[2].media;
    thumbnails::drop_pages(&mut h.state_mut().state, &[2], 0);
    h.run_steps(2);
    assert_eq!(h.state().state.doc().unwrap().session.doc().pages[0].media, first_label);
    assert_eq!(h.state().state.doc().unwrap().session.doc().pages[0].rotate, 90);
    // Copy and paste pages; delete them.
    thumbnails::command(&mut h.state_mut().state, "copy", &[0]);
    thumbnails::command(&mut h.state_mut().state, "paste", &[2]);
    h.run_steps(2);
    assert_eq!(pages(&h), 4);
    thumbnails::command(&mut h.state_mut().state, "delete", &[3]);
    h.run_steps(2);
    assert_eq!(pages(&h), 3);
    thumbnails::command(&mut h.state_mut().state, "move_down", &[0]);
    h.run_steps(2);
    assert_eq!(h.state().state.doc().unwrap().session.doc().pages[1].rotate, 90);
    // The page commands open their dialogs for the selected pages.
    thumbnails::command(&mut h.state_mut().state, "crop", &[1]);
    h.run_steps(2);
    assert!(h.query_by_label("Crop Pages").is_some());
}

fn ok(h: &mut Harness<'_, MarkupCraftApp>) {
    // Let the dialog settle at its size after the fields changed.
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
}

#[test]
fn page_dialogs_insert_rotate_crop_setup_delete() {
    use shell::pages::Range;
    let mut h = harness();
    // Insert Blank Pages: two Tabloid pages before page 1.
    run(&mut h, "document.insert_blank_pages");
    {
        let d = h.state_mut().state.shell.page_dialog.as_mut().unwrap();
        d.count = 2;
        d.size = Some(2);
        d.after = false;
        d.at_page = 1;
    }
    ok(&mut h);
    assert_eq!(pages(&h), 4);
    let m = h.state().state.doc().unwrap().session.doc().pages[0].media;
    assert!(((m.x1 - m.x0) - 792.0).abs() < 0.5 && ((m.y1 - m.y0) - 1224.0).abs() < 0.5);
    // Rotate Pages (Ctrl+Shift+R): the even pages, 180 degrees.
    keys(&mut h, cs(), Key::R);
    {
        let d = h.state_mut().state.shell.page_dialog.as_mut().unwrap();
        d.pages.range = Range::Even;
        d.degrees = 180;
    }
    ok(&mut h);
    let r: Vec<i32> = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .pages
        .iter()
        .map(|p| p.rotate)
        .collect();
    assert_eq!(r, vec![0, 180, 0, 180]);
    // Crop Pages (Shift+Alt+O): one inch off every side of page 1.
    keys(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::O);
    {
        let d = h.state_mut().state.shell.page_dialog.as_mut().unwrap();
        d.pages.range = Range::First;
        d.margins = [1.0; 4];
    }
    ok(&mut h);
    let b = h.state().state.doc().unwrap().session.page_boxes().unwrap()[0];
    assert!(((b.crop.x1 - b.crop.x0) - (792.0 - 144.0)).abs() < 0.5, "{:?}", b.crop);
    // Page Setup: page 3 to Letter landscape.
    run(&mut h, "document.page_setup");
    {
        let d = h.state_mut().state.shell.page_dialog.as_mut().unwrap();
        d.pages.range = Range::Custom;
        d.pages.custom = "3".into();
        d.size = Some(0);
        d.landscape = true;
    }
    ok(&mut h);
    assert!(
        h.state().state.shell.page_dialog.is_none(),
        "{:?}",
        h.state().state.shell.page_dialog
    );
    let m = h.state().state.doc().unwrap().session.doc().pages[2].media;
    assert!(((m.x1 - m.x0) - 792.0).abs() < 0.5 && ((m.y1 - m.y0) - 612.0).abs() < 0.5);
    // Delete Pages: a bad range is refused in the dialog; then pages 1-2.
    run(&mut h, "document.delete_pages");
    {
        let d = h.state_mut().state.shell.page_dialog.as_mut().unwrap();
        d.pages.range = Range::Custom;
        d.pages.custom = "9".into();
    }
    ok(&mut h);
    assert!(!h.state().state.shell.page_dialog.as_ref().unwrap().error.is_empty());
    h.state_mut().state.shell.page_dialog.as_mut().unwrap().pages.custom = "1-2".into();
    ok(&mut h);
    assert_eq!(pages(&h), 2);
    assert!(h.state().state.shell.page_dialog.is_none());
    // All of it is undoable.
    for _ in 0..5 {
        keys(&mut h, C, Key::Z);
    }
    assert_eq!(pages(&h), 2);
    let r: Vec<i32> = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .pages
        .iter()
        .map(|p| p.rotate)
        .collect();
    assert_eq!(r, vec![0, 0]);
}

#[test]
fn extract_and_replace_pages_through_dialogs() {
    let dir = temp_dir("extract");
    let mut h = harness();
    let out = dir.join("extract.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    run(&mut h, "document.extract_pages");
    {
        let d = h.state_mut().state.shell.page_dialog.as_mut().unwrap();
        d.pages.range = shell::pages::Range::All;
        d.open_after = true;
    }
    ok(&mut h);
    h.run_steps(3);
    assert!(out.is_file());
    assert_eq!(h.state().state.docs.len(), 2);
    // Replace Pages (Ctrl+Shift+Y): page 1 of the first document from a blank file.
    h.state_mut().state.active = 0;
    let src = dir.join("blank.pdf");
    std::fs::write(&src, markupcraft_engine::blank::pdf_bytes(&[(612.0, 792.0)]).unwrap()).unwrap();
    h.state_mut().state.dialogs.scripted = Some(vec![src]);
    keys(&mut h, cs(), Key::Y);
    ok(&mut h);
    h.run_steps(3);
    let st = &h.state().state;
    assert!(st.status.starts_with("Replaced"), "{}", st.status);
    let d = st.doc().unwrap();
    assert!(d.session.is_dirty());
    // Markups stay on the replaced page.
    assert!(d.session.doc().markups.iter().any(|m| m.page == 0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ctrl_places_a_point_without_snapping() {
    let mut h = harness();
    h.state_mut().state.snaps.grid = true;
    h.state_mut().state.set_tool("line");
    h.run_steps(2);
    // 7,7 is off the 18 pt grid: snapped it lands on 0,0 / 18,18; with Ctrl it stays.
    let a = screen(&h, 0, 307.0, 307.0);
    let b = screen(&h, 0, 407.0, 307.0);
    for (p, m) in [(a, C), (b, C)] {
        h.hover_at(p);
        h.step();
        h.event(Event::ModifiersChanged(m));
        press(&mut h, p, PointerButton::Primary, true, m);
        press(&mut h, p, PointerButton::Primary, false, m);
        h.event(Event::ModifiersChanged(Modifiers::NONE));
        h.run_steps(30);
    }
    let m = h
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
    assert!((m.pts[0].x - 307.0).abs() < 1.0, "{:?}", m.pts);
}

#[test]
fn reuse_tool_keeps_drawing() {
    let mut h = harness();
    run(&mut h, "window.reuse_tools");
    assert!(h.state().state.tool_locked);
    h.state_mut().state.set_tool("rectangle");
    let (a, b) = (screen(&h, 0, 200.0, 300.0), screen(&h, 0, 260.0, 260.0));
    h.hover_at(a);
    h.step();
    press(&mut h, a, PointerButton::Primary, true, Modifiers::NONE);
    for i in 1..=5 {
        h.hover_at(a + (b - a) * (i as f32 / 5.0));
        h.step();
    }
    press(&mut h, b, PointerButton::Primary, false, Modifiers::NONE);
    h.run_steps(3);
    assert_eq!(h.state().state.tool, "rectangle");
}

/// Revu 21's default keys (docs/revu_features/05_shortcuts.md) for the Window, View, Document
/// and File groups that MarkupCraft has, each bound to its command.
#[test]
fn revu_window_view_document_file_shortcuts_are_bound() {
    use markupcraft_ui_egui::commands::{Keys, bindings};
    let k = |c: bool, s: bool, a: bool, key: Key| Keys::new(c, s, a, key);
    let (y, n) = (true, false);
    #[rustfmt::skip]
    let expected: &[(Keys, &str)] = &[
        // File
        (k(y, n, n, Key::N), "file.new"), (k(n, y, n, Key::F2), "file.save_all"),
        // View
        (k(n, y, n, Key::F12), "view.balance"), (k(y, n, n, Key::Num7), "view.continuous_side"),
        (k(y, n, n, Key::F5), "view.dimmer"), (k(n, n, y, Key::ArrowRight), "view.next_view"),
        (k(n, n, y, Key::ArrowLeft), "view.prev_view"), (k(n, n, n, Key::F5), "view.refresh"),
        (k(y, y, n, Key::Plus), "view.rotate_view_cw"), (k(y, y, n, Key::Minus), "view.rotate_view_ccw"),
        (k(y, n, n, Key::R), "view.rulers"), (k(y, n, n, Key::Num6), "view.side_by_side"),
        (k(y, n, n, Key::H), "view.split_horizontal"), (k(y, n, n, Key::Num2), "view.split_vertical"),
        (k(y, n, n, Key::Num1), "view.switch"), (k(y, n, n, Key::I), "view.toggle_split"),
        (k(y, y, n, Key::Num2), "view.unsplit"), (k(n, n, n, Key::Z), "tool.zoom"),
        (k(n, y, n, Key::Z), "view.toggle_zoom"),
        // Document
        (k(n, y, y, Key::O), "document.crop_pages"), (k(n, y, n, Key::F5), "file.refresh"),
        (k(y, y, n, Key::Y), "document.replace_pages"), (k(y, y, n, Key::R), "document.rotate_pages"),
        // Window
        (k(y, n, n, Key::F12), "window.always_on_top"), (k(y, y, n, Key::W), "file.close_all"),
        (k(n, n, y, Key::A), "panel.file_access"), (k(n, n, n, Key::F11), "window.full_screen"),
        (k(n, y, n, Key::F4), "window.hide_panels"), (k(n, n, n, Key::F9), "window.menu_bar"),
        (k(n, n, n, Key::F4), "window.nav_bar"), (k(y, n, n, Key::K), "window.preferences"),
        (k(y, n, n, Key::Enter), "window.presentation"), (k(n, n, n, Key::F8), "window.status_bar"),
    ];
    let b = bindings();
    for (keys, id) in expected {
        assert!(
            b.iter().any(|(bk, bid)| bk == keys && bid == id),
            "{} should run {id}",
            keys.label()
        );
    }
}

#[test]
fn dropped_files_open_in_tabs_pan_tool_and_fit_width_lock() {
    let dir = temp_dir("drop");
    let mut h = harness();
    let a = sample_file(&dir, "dropped.pdf");
    #[derive(Debug)]
    struct Dropped(PathBuf);
    impl egui::DroppedFile for Dropped {
        fn path(&self) -> &Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            std::fs::read(&self.0).map_err(|e| e.to_string())
        }
    }
    h.input_mut()
        .dropped_files
        .push(std::sync::Arc::new(Dropped(a.clone())));
    h.run_steps(3);
    assert_eq!(h.state().state.docs.len(), 2);
    assert_eq!(h.state().state.doc().unwrap().path.as_deref(), Some(a.as_path()));
    // Pan tool (Shift+V): the left button drags the view.
    keys(&mut h, S, Key::V);
    assert_eq!(h.state().state.tool, "pan");
    run(&mut h, "view.fit_width");
    run(&mut h, "view.zoom_in");
    h.run_steps(30);
    let off = view(&h).offset;
    let c = view(&h).viewport().center();
    let b = c + vec2(-80.0, -60.0);
    h.hover_at(c);
    h.step();
    press(&mut h, c, PointerButton::Primary, true, Modifiers::NONE);
    for i in 1..=6 {
        h.hover_at(c + (b - c) * (i as f32 / 6.0));
        h.step();
    }
    press(&mut h, b, PointerButton::Primary, false, Modifiers::NONE);
    h.run_steps(2);
    let moved = view(&h).offset - off;
    assert!(moved.x > 40.0 && moved.y > 30.0, "{moved:?}");
    // Lock panning in Fit Width: only up and down.
    run(&mut h, "view.fit_width");
    h.state_mut().state.shell.ui.lock_fit_width = true;
    h.run_steps(30);
    let off = view(&h).offset;
    h.hover_at(c);
    h.step();
    press(&mut h, c, PointerButton::Primary, true, Modifiers::NONE);
    for i in 1..=6 {
        h.hover_at(c + (b - c) * (i as f32 / 6.0));
        h.step();
    }
    press(&mut h, b, PointerButton::Primary, false, Modifiers::NONE);
    h.run_steps(2);
    let moved = view(&h).offset - off;
    assert!(moved.x.abs() < 0.5 && moved.y > 30.0, "{moved:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dark_theme_and_menus_render() {
    let mut h = harness();
    h.state_mut().state.shell.prefs.theme = "dark".into();
    h.run_steps(3);
    assert_eq!(h.state().state.shell.applied_theme, Some(true));
    // The View menu lists the new commands with their keys.
    h.get_by_label("View").click();
    h.run_steps(2);
    h.run_steps(3);
    assert!(h.query_all_by_label_contains("Split Vertical").next().is_some());
    assert!(h.query_all_by_label_contains("Rotate View Clockwise").next().is_some());
    assert!(h.query_all_by_label_contains("Previous View").next().is_some());
}
