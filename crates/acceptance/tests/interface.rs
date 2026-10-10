//! Interface slice: application layout, profiles and keyboard, view modes, rulers / grid / snap,
//! the Preferences dialog, mouse and modifier keys (`ui-073`..`ui-196`, `ui-226`) and all of
//! Revu's default keyboard shortcuts (`key-*`).
//!
//! Shortcuts are pressed as real key combinations, the way the platform layer delivers them:
//! modifiers go down first (`ModifiersChanged`), and Ctrl+C / Ctrl+X / Ctrl+V (with any other
//! modifier) arrive as `Copy` / `Cut` / `Paste` events exactly as egui-winit sends them.
#![allow(clippy::type_complexity, clippy::collapsible_if)]
use std::panic::{AssertUnwindSafe, catch_unwind};

use egui::{Key, Modifiers};
use markupcraft_acceptance::*;

type H = Harness<'static, MarkupCraftApp>;

/// The app with every file dialog answered by the test (never a native dialog).
fn app() -> H {
    let mut h = markupcraft_acceptance::app();
    h.state_mut().state.dialogs.scripted = Some(Vec::new());
    h
}

// ---- input, the way the platform delivers it ---------------------------------------------------

/// Ctrl (Cmd on macOS) / Shift / Alt as the platform reports them.
fn mods(ctrl: bool, shift: bool, alt: bool) -> Modifiers {
    Modifiers {
        alt,
        ctrl: ctrl && !cfg!(target_os = "macos"),
        shift,
        mac_cmd: ctrl && cfg!(target_os = "macos"),
        command: ctrl,
    }
}

/// The text the app last put on the system clipboard (what a real Paste would carry).
fn clipboard_text(h: &H) -> Option<String> {
    h.output().platform_output.commands.iter().rev().find_map(|c| match c {
        egui::OutputCommand::CopyText(t) => Some(t.clone()),
        _ => None,
    })
}

/// Press and release a key combination, as egui-winit delivers it.
fn press_with(h: &mut H, m: Modifiers, k: Key, clip: Option<String>) -> Option<String> {
    h.event(egui::Event::ModifiersChanged(m));
    // egui-winit turns Cmd+X / Cmd+C / Cmd+V (whatever else is held) into clipboard events and
    // sends no key event for them; Paste only when the clipboard holds text.
    let clipboard = match k {
        Key::X if m.command => Some(egui::Event::Cut),
        Key::C if m.command => Some(egui::Event::Copy),
        Key::V if m.command => Some(egui::Event::Paste(clip.clone().unwrap_or_default())),
        _ => None,
    };
    let mut copied = None;
    if let Some(ev) = clipboard {
        if !matches!(&ev, egui::Event::Paste(t) if t.is_empty()) {
            h.event(ev);
        }
        h.step();
        copied = clipboard_text(h);
    } else {
        h.event(egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: m,
        });
        h.step();
        copied = copied.or_else(|| clipboard_text(h));
        h.event(egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: m,
        });
    }
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    for _ in 0..4 {
        h.step();
        copied = copied.or_else(|| clipboard_text(h));
    }
    copied
}

fn press(h: &mut H, m: Modifiers, k: Key) {
    press_with(h, m, k, None);
}

// ---- observations -----------------------------------------------------------------------------

fn st(h: &H) -> &markupcraft_ui_egui::AppState {
    &h.state().state
}

fn view(h: &H) -> &markupcraft_ui_egui::canvas::DocView {
    &st(h).doc().unwrap().view
}

fn mk(h: &H, id: &str) -> markupcraft_model::Markup {
    markups(h)
        .into_iter()
        .find(|m| m.id == id)
        .unwrap_or_else(|| panic!("no markup {id}"))
}

fn select(h: &mut H, ids: &[&str]) {
    let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
    h.state_mut().state.doc_mut().unwrap().session.select(&ids).unwrap();
    h.run_steps(2);
}

fn selection(h: &H) -> Vec<String> {
    st(h).doc().unwrap().selection().to_vec()
}

fn page_count(h: &H) -> usize {
    st(h).doc().unwrap().session.page_count()
}

fn panel_open(h: &H, id: &str) -> bool {
    st(h).open_panels.contains(&id)
}

const SQUARE: &str = "SAMPLESQUAREAAAA";
const CIRCLE: &str = "SAMPLECIRCLEAAAA";
const TEXTBOX: &str = "SAMPLETEXTAAAAAA";
const POLYLINE: &str = "SAMPLEPOLYLINEAA";

/// Snap reach and grid spacing are process-wide (one app per process in real use): tests that
/// change preferences or depend on snapping take turns.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// Run every case on a fresh app; report all failures together.
fn run_cases<T>(cases: &[(&'static str, T)], f: impl Fn(&T)) {
    let mut failed = Vec::new();
    for (id, c) in cases {
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| f(c))) {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            failed.push(format!("{id}: {msg}"));
        }
    }
    assert!(failed.is_empty(), "{} failed:\n{}", failed.len(), failed.join("\n"));
}

// ---- key rows: tools ----------------------------------------------------------------------------

/// Each single-key or Shift/Alt tool shortcut makes that tool the active one.
#[test]
fn shortcut_keys_activate_tools() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    const N: (bool, bool, bool) = (false, false, false);
    const S: (bool, bool, bool) = (false, true, false);
    const SA: (bool, bool, bool) = (false, true, true);
    let cases: &[(&str, ((bool, bool, bool), Key, &str))] = &[
        ("key-007 Arc", (S, Key::C, "arc")),
        ("key-008 Arrow", (N, Key::A, "arrow")),
        ("key-012 Callout", (N, Key::Q, "callout")),
        ("key-015 Cloud", (N, Key::C, "cloud")),
        ("key-016 Cloud+", (N, Key::K, "cloudplus")),
        ("key-017 Dimension", (S, Key::L, "dimension")),
        ("key-019 Ellipse", (N, Key::E, "ellipse")),
        ("key-020 Eraser", (S, Key::E, "eraser")),
        ("key-022 File Attachment", (N, Key::F, "attachment")),
        ("key-023 Flag", (S, Key::F, "flag")),
        ("key-027 Highlight", (N, Key::H, "highlight")),
        ("key-032 Angle", (SA, Key::G, "angle")),
        ("key-033 Area", (SA, Key::A, "area")),
        ("key-034 Count", (SA, Key::C, "count")),
        ("key-035 Diameter", (SA, Key::D, "diameter")),
        ("key-037 Length", (SA, Key::L, "length")),
        ("key-039 Perimeter", (SA, Key::P, "perimeter")),
        ("key-040 Polylength", (SA, Key::Q, "polylength")),
        ("key-041 Radius", (SA, Key::U, "radius")),
        ("key-042 Volume", (SA, Key::V, "volume")),
        ("key-045 Line", (N, Key::L, "line")),
        ("key-047 Note", (N, Key::N, "note")),
        ("key-048 Pen", (N, Key::P, "pen")),
        ("key-049 Polygon", (S, Key::P, "polygon")),
        ("key-050 Polyline", (S, Key::N, "polyline")),
        ("key-051 Rectangle", (N, Key::R, "rectangle")),
        ("key-056 Stamp", (N, Key::S, "stamp")),
        ("key-057 Text Box", (N, Key::T, "text")),
        ("key-058 Typewriter", (N, Key::W, "typewriter")),
        ("key-078 Snapshot", (N, Key::G, "snapshot")),
        ("key-108 Lasso", (S, Key::O, "lasso")),
        ("key-109 Pan", (S, Key::V, "pan")),
        ("key-111 Select Text", (S, Key::T, "selecttext")),
        ("key-115 Zoom Tool", (N, Key::Z, "zoom")),
        ("key-142 Squiggly", (S, Key::U, "squiggly")),
        ("key-143 Strikethrough", (N, Key::D, "strikethrough")),
        ("key-144 Underline", (N, Key::U, "underline")),
    ];
    run_cases(cases, |&((c, s, a), k, tool)| {
        let mut h = app();
        press(&mut h, mods(c, s, a), k);
        assert_eq!(st(&h).tool, tool, "expected tool {tool}");
    });
}

/// V returns to Select from any tool; Shift+Z toggles the Zoom tool on and back off.
#[test]
fn shortcut_select_and_toggle_zoom() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(false, false, false), Key::R);
    assert_eq!(st(&h).tool, "rectangle");
    press(&mut h, mods(false, false, false), Key::V);
    assert_eq!(st(&h).tool, "select", "key-110 V selects");
    press(&mut h, mods(false, false, false), Key::L);
    press(&mut h, mods(false, true, false), Key::Z);
    assert_eq!(st(&h).tool, "zoom", "key-112 Shift+Z to zoom");
    press(&mut h, mods(false, true, false), Key::Z);
    assert_eq!(st(&h).tool, "line", "key-112 Shift+Z back to the tool before");
}

/// Tools that are not a plain draw tool in our table: their key still starts them.
#[test]
fn shortcut_special_tools_start() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let cases: &[(&str, ((bool, bool, bool), Key))] = &[
        ("key-028 Hyperlink", ((false, true, false), Key::H)),
        ("key-036 Dynamic Fill", ((false, false, false), Key::J)),
        ("key-038 Measure Tool", ((false, false, false), Key::M)),
        ("key-043 Add Signature Field", ((false, false, false), Key::X)),
        ("key-132 Mark for Redaction", ((false, true, false), Key::R)),
        ("key-133 Mark Text for Redaction", ((false, true, false), Key::K)),
        ("key-141 Snapshot Content", ((false, true, false), Key::G)),
    ];
    run_cases(cases, |&((c, s, a), k)| {
        let mut h = app();
        let before = (st(&h).tool, st(&h).status.clone(), st(&h).features.pick.is_some());
        press(&mut h, mods(c, s, a), k);
        let after = (st(&h).tool, st(&h).status.clone(), st(&h).features.pick.is_some());
        assert_ne!(before, after, "nothing started");
        assert!(after.0 != "select" || after.2, "no tool or pick active: {after:?}");
    });
}

// ---- key rows: markup editing ---------------------------------------------------------------------

fn rect_of(h: &H, id: &str) -> markupcraft_geom::Rect {
    mk(h, id).rect.normalized()
}

/// Ctrl+Alt+B/E/L/M/R/T line the selected markups up on one edge or centre.
#[test]
fn shortcut_align() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    type Edge = fn(&markupcraft_geom::Rect) -> f64;
    // PDF user space: y grows up, so Bottom is the smaller y.
    let cases: &[(&str, (Key, Edge))] = &[
        ("key-001 Align Bottom", (Key::B, |r| r.y0)),
        ("key-002 Align Center", (Key::E, |r| (r.x0 + r.x1) / 2.0)),
        ("key-003 Align Left", (Key::L, |r| r.x0)),
        ("key-004 Align Middle", (Key::M, |r| (r.y0 + r.y1) / 2.0)),
        ("key-005 Align Right", (Key::R, |r| r.x1)),
        ("key-006 Align Top", (Key::T, |r| r.y1)),
    ];
    run_cases(cases, |&(k, edge)| {
        let mut h = app();
        select(&mut h, &[SQUARE, CIRCLE]);
        assert!((edge(&rect_of(&h, SQUARE)) - edge(&rect_of(&h, CIRCLE))).abs() > 1.0);
        press(&mut h, mods(true, false, true), k);
        let (a, b) = (edge(&rect_of(&h, SQUARE)), edge(&rect_of(&h, CIRCLE)));
        assert!((a - b).abs() < 0.01, "edges {a} {b} not aligned");
    });
}

/// Ctrl+Alt+H / Ctrl+Alt+V mirror the selected markup.
#[test]
fn shortcut_flip() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let cases: &[(&str, (Key, bool))] = &[
        ("key-024 Flip Horizontal", (Key::H, true)),
        ("key-025 Flip Vertical", (Key::V, false)),
    ];
    run_cases(cases, |&(k, horizontal)| {
        let mut h = app();
        select(&mut h, &[POLYLINE]);
        let before = mk(&h, POLYLINE).pts;
        let r = rect_of(&h, POLYLINE);
        // Ctrl+Alt+V reaches the app as Paste, which the platform sends while the clipboard
        // holds text (it nearly always does).
        press_with(&mut h, mods(true, false, true), k, Some("text".into()));
        let after = mk(&h, POLYLINE).pts;
        assert_eq!(before.len(), after.len());
        // A mirror maps the first point to its reflection about the box centre.
        let (b, a) = (before[0], after[0]);
        if horizontal {
            assert!(
                (a.x - (r.x0 + r.x1 - b.x)).abs() < 3.0 && (a.y - b.y).abs() < 0.5,
                "{b:?} -> {a:?} in {r:?}"
            );
        } else {
            assert!(
                (a.y - (r.y0 + r.y1 - b.y)).abs() < 3.0 && (a.x - b.x).abs() < 0.5,
                "{b:?} -> {a:?} in {r:?}"
            );
        }
    });
}

fn order(h: &H, page: usize) -> Vec<String> {
    markups(h)
        .into_iter()
        .filter(|m| m.page == page)
        .map(|m| m.id)
        .collect()
}

/// Ctrl+] / Ctrl+Shift+] / Ctrl+[ / Ctrl+Shift+[ restack the selected markup.
#[test]
fn shortcut_arrange() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let cases: &[(&str, ((bool, bool, bool), Key, &str))] = &[
        (
            "key-010 Bring Forward",
            ((true, false, false), Key::CloseBracket, "forward"),
        ),
        (
            "key-011 Bring to Front",
            ((true, true, false), Key::CloseBracket, "front"),
        ),
        (
            "key-054 Send Backward",
            ((true, false, false), Key::OpenBracket, "backward"),
        ),
        ("key-055 Send to Back", ((true, true, false), Key::OpenBracket, "back")),
    ];
    run_cases(cases, |&((c, s, a), k, what)| {
        let mut h = app();
        let before = order(&h, 0);
        assert!(before.len() >= 3, "{before:?}");
        let mid = before[before.len() / 2].clone();
        let i = before.iter().position(|x| *x == mid).unwrap();
        select(&mut h, &[&mid]);
        press(&mut h, mods(c, s, a), k);
        let after = order(&h, 0);
        let j = after.iter().position(|x| *x == mid).unwrap();
        let want = match what {
            "forward" => i + 1,
            "front" => after.len() - 1,
            "backward" => i - 1,
            _ => 0,
        };
        assert_eq!(j, want, "{what}: {before:?} -> {after:?}");
    });
}

/// Ctrl+G groups, Ctrl+Shift+Alt+G takes one markup out of its group, Ctrl+Shift+G ungroups.
#[test]
fn shortcut_group_ungroup_remove() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    select(&mut h, &[SQUARE, CIRCLE, TEXTBOX]);
    press(&mut h, mods(true, false, false), Key::G);
    let g = mk(&h, SQUARE).group;
    assert!(!g.is_empty(), "key-026 Ctrl+G groups");
    assert_eq!(mk(&h, CIRCLE).group, g);
    assert_eq!(mk(&h, TEXTBOX).group, g);
    select(&mut h, &[TEXTBOX]);
    press(&mut h, mods(true, true, true), Key::G);
    assert!(mk(&h, TEXTBOX).group.is_empty(), "key-052 removed from group");
    assert!(!mk(&h, SQUARE).group.is_empty() && mk(&h, SQUARE).group == mk(&h, CIRCLE).group);
    select(&mut h, &[SQUARE]);
    press(&mut h, mods(true, true, false), Key::G);
    assert!(
        mk(&h, SQUARE).group.is_empty() && mk(&h, CIRCLE).group.is_empty(),
        "key-059 ungroups"
    );
}

/// Ctrl+Shift+L locks the selected markup (it can no longer be moved), and unlocks it.
#[test]
fn shortcut_lock() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    select(&mut h, &[SQUARE]);
    press(&mut h, mods(true, true, false), Key::L);
    assert!(mk(&h, SQUARE).flags & 128 != 0, "key-046 /F Locked bit set");
    let r = rect_of(&h, SQUARE);
    drag(&mut h, (950.0, 650.0), (850.0, 500.0));
    assert_eq!(rect_of(&h, SQUARE), r, "a locked markup does not move");
    select(&mut h, &[SQUARE]);
    press(&mut h, mods(true, true, false), Key::L);
    assert!(mk(&h, SQUARE).flags & 128 == 0, "key-046 toggles back");
}

/// Alt+Z fits the text box to its text.
#[test]
fn shortcut_autosize_text_box() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    select(&mut h, &[TEXTBOX]);
    let before = mk(&h, TEXTBOX).pts;
    press(&mut h, mods(false, false, true), Key::Z);
    let after = mk(&h, TEXTBOX).pts;
    assert_ne!(before, after, "key-009 the box resized to its text");
}

/// Del deletes; Ctrl+Z undoes; Ctrl+Y redoes; Ctrl+A selects all markups on the page.
#[test]
fn shortcut_delete_undo_redo_select_all() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let n = markups(&h).len();
    select(&mut h, &[SQUARE]);
    press(&mut h, mods(false, false, false), Key::Delete);
    assert_eq!(markups(&h).len(), n - 1, "key-071 Del deletes");
    press(&mut h, mods(true, false, false), Key::Z);
    assert_eq!(markups(&h).len(), n, "key-079 Ctrl+Z undoes");
    press(&mut h, mods(true, false, false), Key::Y);
    assert_eq!(markups(&h).len(), n - 1, "key-075 Ctrl+Y redoes");
    press(&mut h, mods(true, false, false), Key::A);
    let page = view(&h).current;
    let on_page: Vec<_> = markups(&h).into_iter().filter(|m| m.page == page).collect();
    assert!(!on_page.is_empty());
    for m in &on_page {
        assert!(selection(&h).contains(&m.id), "key-076 {} not selected", m.id);
    }
}

/// Ctrl+C / Ctrl+X / Ctrl+V / Ctrl+Shift+V, delivered as the platform's clipboard events.
#[test]
fn shortcut_copy_cut_paste_paste_in_place() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let n = markups(&h).len();
    let kind = mk(&h, SQUARE).kind;
    select(&mut h, &[SQUARE]);
    let clip = press_with(&mut h, mods(true, false, false), Key::C, None);
    assert_eq!(markups(&h).len(), n, "key-068 copy leaves the document alone");
    // paste in place on page 2: same coordinates
    press(&mut h, mods(true, false, false), Key::ArrowRight);
    assert_eq!(view(&h).current, 1);
    press_with(&mut h, mods(true, true, false), Key::V, clip.clone());

    let pasted: Vec<_> = markups(&h)
        .into_iter()
        .filter(|m| m.page == 1 && m.kind == kind)
        .collect();
    assert_eq!(pasted.len(), 1, "key-074 Ctrl+Shift+V pastes in place on page 2");
    assert_eq!(pasted[0].rect.normalized(), rect_of(&h, SQUARE), "same position");
    // plain paste
    press_with(&mut h, mods(true, false, false), Key::V, clip.clone());
    assert_eq!(markups(&h).len(), n + 2, "key-073 Ctrl+V pastes");
    // cut
    press(&mut h, mods(true, false, false), Key::ArrowLeft);
    select(&mut h, &[CIRCLE]);
    let clip2 = press_with(&mut h, mods(true, false, false), Key::X, None);
    assert!(markups(&h).iter().all(|m| m.id != CIRCLE), "key-070 Ctrl+X removes");
    press_with(&mut h, mods(true, false, false), Key::V, clip2.or(clip));
    assert_eq!(markups(&h).len(), n + 2, "the cut markup pastes back");
}

/// Ctrl+Shift+C picks up the selected markup's look for the Format Painter.
#[test]
fn shortcut_format_painter() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    select(&mut h, &[SQUARE]);
    press(&mut h, mods(true, true, false), Key::C);
    // the painter is loaded: clicking another markup gives it the square's look
    click(&mut h, 780.0, 620.0);
    assert_eq!(mk(&h, CIRCLE).color, mk(&h, SQUARE).color, "key-072 painter applied");
}

/// Ctrl+Shift+A selects all the text on the page.
#[test]
fn shortcut_select_all_text() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let before = st(&h).status.clone();
    press(&mut h, mods(true, true, false), Key::A);
    let s = st(&h);
    assert!(
        s.tool == "selecttext" || s.status != before,
        "key-077 page text selected (tool {}, status {:?})",
        s.tool,
        s.status
    );
}

// ---- key rows: view -----------------------------------------------------------------------------

use markupcraft_ui_egui::canvas::{Fit, PageMode};

/// Ctrl+8 / Ctrl+9 / Ctrl+0, Ctrl+4..7, Plus / Minus.
#[test]
fn shortcut_zoom_and_page_modes() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(true, false, false), Key::Num8);
    assert!(
        (view(&h).zoom - 1.0).abs() < 1e-3,
        "key-080 Actual Size = 100%: {}",
        view(&h).zoom
    );
    press(&mut h, mods(true, false, false), Key::Num9);
    assert_eq!(view(&h).fit, Fit::Page, "key-085 Fit Page");
    let page_zoom = view(&h).zoom;
    press(&mut h, mods(true, false, false), Key::Num0);
    assert_eq!(view(&h).fit, Fit::Width, "key-086 Fit Width");
    assert!(
        view(&h).zoom > page_zoom,
        "a landscape page is wider than tall in a wide window too"
    );
    let z = view(&h).zoom;
    press(&mut h, mods(false, false, false), Key::Plus);
    assert!(view(&h).zoom > z * 1.05, "key-113 Plus zooms in");
    let z = view(&h).zoom;
    press(&mut h, mods(false, false, false), Key::Minus);
    assert!(view(&h).zoom < z * 0.95, "key-114 Minus zooms out");
    for (k, mode, row) in [
        (Key::Num4, PageMode::Single, "key-098"),
        (Key::Num5, PageMode::Continuous, "key-082"),
        (Key::Num6, PageMode::SideBySide, "key-097"),
        (Key::Num7, PageMode::ContinuousSideBySide, "key-083"),
        (Key::Num4, PageMode::Single, "key-098"),
    ] {
        press(&mut h, mods(true, false, false), k);
        assert_eq!(view(&h).mode, mode, "{row}");
    }
}

/// Ctrl+Right / Ctrl+Left / Home / End move between pages; Alt+Left / Alt+Right walk views.
#[test]
fn shortcut_page_and_view_navigation() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(true, false, false), Key::ArrowRight);
    assert_eq!(view(&h).current, 1, "key-087 Ctrl+Right next page");
    press(&mut h, mods(true, false, false), Key::ArrowLeft);
    assert_eq!(view(&h).current, 0, "key-089 Ctrl+Left previous page");
    press(&mut h, mods(false, false, false), Key::End);
    assert_eq!(view(&h).current, 1, "key-175 End last page");
    press(&mut h, mods(false, false, false), Key::Home);
    assert_eq!(view(&h).current, 0, "key-174 Home first page");
    press(&mut h, mods(false, false, true), Key::ArrowLeft);
    assert_eq!(
        view(&h).current,
        1,
        "key-090 Alt+Left back to the previous view (page 2)"
    );
    press(&mut h, mods(false, false, true), Key::ArrowRight);
    assert_eq!(view(&h).current, 0, "key-088 Alt+Right forward again");
}

/// Ctrl+Shift+Plus / Minus rotate the view (not the file); F5 refreshes.
#[test]
fn shortcut_rotate_view_and_refresh() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let rot = st(&h).doc().unwrap().session.doc().pages[0].rotate;
    press(&mut h, mods(true, true, false), Key::Plus);
    assert_eq!(view(&h).rotation, 90, "key-093 clockwise");
    press(&mut h, mods(true, true, false), Key::Minus);
    press(&mut h, mods(true, true, false), Key::Minus);
    assert_eq!(view(&h).rotation, 270, "key-094 counterclockwise");
    assert_eq!(
        st(&h).doc().unwrap().session.doc().pages[0].rotate,
        rot,
        "the file is unchanged"
    );
    assert!(!st(&h).doc().unwrap().session.is_dirty());
    let status = st(&h).status.clone();
    press(&mut h, mods(false, false, false), Key::F5);
    assert!(
        st(&h).status != status || view(&h).missing > 0,
        "key-091 F5 redraws the view"
    );
}

/// Ctrl+R rulers, Shift+F9 grid, Ctrl+Shift+F7/F8/F9 snaps, Ctrl+F5 dimmer: each toggles.
#[test]
fn shortcut_view_toggles() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    type Get = fn(&H) -> bool;
    let cases: &[(&str, ((bool, bool, bool), Key, Get))] = &[
        (
            "key-095 Rulers",
            ((true, false, false), Key::R, |h| st(h).shell.ui.rulers),
        ),
        (
            "key-096 Show Grid",
            ((false, true, false), Key::F9, |h| st(h).show_grid),
        ),
        (
            "key-099 Snap to Content",
            ((true, true, false), Key::F8, |h| st(h).snaps.content),
        ),
        (
            "key-100 Snap to Grid",
            ((true, true, false), Key::F9, |h| st(h).snaps.grid),
        ),
        (
            "key-101 Snap to Markup",
            ((true, true, false), Key::F7, |h| st(h).snaps.markup),
        ),
        (
            "key-084 Dimmer",
            ((true, false, false), Key::F5, |h| st(h).shell.dimmer),
        ),
        (
            "key-147 Always on Top",
            ((true, false, false), Key::F12, |h| st(h).shell.always_on_top),
        ),
        (
            "key-159 Menu Bar",
            ((false, false, false), Key::F9, |h| st(h).shell.ui.show_menu),
        ),
        (
            "key-160 Navigation Bar",
            ((false, false, false), Key::F4, |h| st(h).shell.ui.show_nav_bar),
        ),
        (
            "key-169 Status Bar",
            ((false, false, false), Key::F8, |h| st(h).shell.ui.show_status_bar),
        ),
        (
            "key-153 Hide Panels",
            ((false, true, false), Key::F4, |h| st(h).shell.panels_hidden),
        ),
    ];
    run_cases(cases, |&((c, s, a), k, get)| {
        let mut h = app();
        let before = get(&h);
        press(&mut h, mods(c, s, a), k);
        assert_ne!(get(&h), before, "first press toggles");
        press(&mut h, mods(c, s, a), k);
        assert_eq!(get(&h), before, "second press toggles back");
    });
}

fn split(h: &H) -> Option<(bool, f32)> {
    st(h).shell.split.as_ref().map(|s| (s.vertical, s.frac))
}

/// Ctrl+2 / Ctrl+H split; Ctrl+I flips; Ctrl+1 switches; Shift+F12 balances; Ctrl+Shift+2 unsplits.
#[test]
fn shortcut_multiview() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(true, false, false), Key::Num2);
    assert_eq!(split(&h).map(|s| s.0), Some(true), "key-103 Ctrl+2 vertical split");
    press(&mut h, mods(true, false, false), Key::Num2);
    press(&mut h, mods(true, true, false), Key::Num2);
    assert!(split(&h).is_none(), "key-106 Ctrl+Shift+2 unsplits");
    press(&mut h, mods(true, false, false), Key::H);
    assert_eq!(split(&h).map(|s| s.0), Some(false), "key-102 Ctrl+H horizontal split");
    press(&mut h, mods(true, false, false), Key::I);
    assert_eq!(split(&h).map(|s| s.0), Some(true), "key-105 Ctrl+I toggles orientation");
    // Switch: with two documents, the active tab moves to the other pane and back.
    {
        let s = &mut h.state_mut().state;
        s.open_bytes("second.pdf", None, markupcraft_render::synthetic::sample_pdf())
            .unwrap();
    }
    h.run_steps(3);
    let pane = |h: &H| st(h).shell.split.as_ref().unwrap().pane.uid;
    let (doc0, pane0) = (st(&h).doc().unwrap().uid, pane(&h));
    if doc0 == pane0 {
        h.state_mut().state.active = 0;
        h.run_steps(2);
    }
    let (doc0, pane0) = (st(&h).doc().unwrap().uid, pane(&h));
    assert_ne!(doc0, pane0, "two documents, one per pane");
    press(&mut h, mods(true, false, false), Key::Num1);
    assert_eq!(
        (st(&h).doc().unwrap().uid, pane(&h)),
        (pane0, doc0),
        "key-104 Ctrl+1 switches"
    );

    h.state_mut().state.shell.split.as_mut().unwrap().frac = 0.2;
    press(&mut h, mods(false, true, false), Key::F12);
    assert!((split(&h).unwrap().1 - 0.5).abs() < 0.01, "key-081 Shift+F12 balances");
}

/// F11 full screen, Ctrl+Enter presentation; Esc leaves both.
#[test]
fn shortcut_full_screen_and_presentation() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    use markupcraft_ui_egui::shell::Screen;
    let mut h = app();
    press(&mut h, mods(false, false, false), Key::F11);
    assert_eq!(st(&h).shell.screen, Screen::FullScreen, "key-152 F11");
    press(&mut h, mods(false, false, false), Key::Escape);
    assert_eq!(st(&h).shell.screen, Screen::Normal, "Esc leaves full screen");
    press(&mut h, mods(true, false, false), Key::Enter);
    assert_eq!(st(&h).shell.screen, Screen::Presentation, "key-162 Ctrl+Enter");
    press(&mut h, mods(false, false, false), Key::ArrowRight);
    assert_eq!(view(&h).current, 1, "arrows advance the presentation");
    press(&mut h, mods(false, false, false), Key::Escape);
    assert_eq!(st(&h).shell.screen, Screen::Normal, "Esc leaves the presentation");
}

// ---- key rows: panels -----------------------------------------------------------------------------

/// Alt+letter opens (and closes) each panel.
#[test]
fn shortcut_panels() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let cases: &[(&str, (Key, &str))] = &[
        ("key-148 Bookmarks", (Key::B, "bookmarks")),
        ("key-150 File Access", (Key::A, "file_access")),
        ("key-155 Layers", (Key::Y, "layers")),
        ("key-156 Links", (Key::N, "links")),
        ("key-157 Markups", (Key::L, "markups")),
        ("key-158 Measurements", (Key::U, "measurements")),
        ("key-163 Properties", (Key::P, "properties")),
        ("key-164 Search", (Key::Num1, "search")),
        ("key-165 Sets", (Key::Num2, "sets")),
        ("key-167 Signatures", (Key::Num4, "signatures")),
        ("key-168 Spaces", (Key::S, "spaces")),
        ("key-171 Thumbnails", (Key::T, "thumbnails")),
        ("key-172 Tool Chest", (Key::X, "toolchest")),
    ];
    run_cases(cases, |&(k, id)| {
        let mut h = app();
        let before = panel_open(&h, id);
        press(&mut h, mods(false, false, true), k);
        assert_ne!(panel_open(&h, id), before, "first press");
        press(&mut h, mods(false, false, true), k);
        assert_eq!(panel_open(&h, id), before, "second press");
    });
}

/// Alt+J shows the JavaScript Console.
#[test]
fn shortcut_forms_and_console() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let cases: &[(&str, (Key, &str))] = &[("key-154 JavaScript Console", (Key::J, "JavaScript"))];
    run_cases(cases, |&(k, title)| {
        let mut h = app();
        assert!(!shows(&h, title));
        press(&mut h, mods(false, false, true), k);
        h.run_steps(3);
        assert!(shows(&h, title), "{title} shows");
    });
}

// ---- key rows: dialogs and document commands --------------------------------------------------

/// Shortcuts that open a dialog or window: the dialog shows after the key.
#[test]
fn shortcut_dialogs_show() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let cases: &[(&str, ((bool, bool, bool), Key, &str))] = &[
        ("key-014 Spell Check", ((false, false, false), Key::F7, "Spell")),
        ("key-063 Print", ((true, false, false), Key::P, "Print")),
        ("key-122 Crop Pages", ((false, true, true), Key::O, "Crop")),
        ("key-123 Delete Pages", ((true, true, false), Key::D, "Delete Pages")),
        (
            "key-125 Document Properties",
            ((true, false, false), Key::D, "Properties"),
        ),
        ("key-127 Extract Pages", ((true, true, false), Key::X, "Extract")),
        ("key-128 Flatten", ((true, true, false), Key::M, "Flatten")),
        ("key-134 OCR", ((true, true, false), Key::O, "OCR")),
        ("key-136 Replace Pages", ((true, true, false), Key::Y, "Replace")),
        ("key-139 Rotate Pages", ((true, true, false), Key::R, "Rotate")),
        ("key-140 Security", ((true, false, false), Key::L, "Security")),
        ("key-145 Unflatten", ((true, true, false), Key::U, "flattened")),
        ("key-161 Preferences", ((true, false, false), Key::K, "Preferences")),
        ("key-173 Help", ((false, false, false), Key::F1, "MarkupCraft Help")),
        ("key-118 Search", ((true, false, false), Key::F, "Search")),
        ("key-107 Web Tab", ((true, false, false), Key::T, "Web")),
        ("key-013 Camera", ((true, false, true), Key::I, "Camera")),
        ("key-030 Image From Scanner", ((false, true, false), Key::I, "Scan")),
        ("key-124 Deskew", ((true, false, true), Key::D, "Deskew")),
    ];
    run_cases(cases, |&((c, s, a), k, title)| {
        let mut h = app();
        h.state_mut().state.dialogs.scripted = Some(Vec::new());
        let label_count = h.query_all_by_label_contains(title).count();
        let status = st(&h).status.clone();
        press(&mut h, mods(c, s, a), k);
        h.run_steps(3);
        let now = h.query_all_by_label_contains(title).count();
        assert!(
            now > label_count || (st(&h).status != status && st(&h).status.contains(title)),
            "{title}: labels {label_count} -> {now}, status {:?}",
            st(&h).status
        );
    });
}

/// Ctrl+B adds a bookmark for the current page.
#[test]
fn shortcut_add_bookmark() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let n = st(&h).doc().unwrap().session.bookmarks().len();
    press(&mut h, mods(true, false, false), Key::B);
    h.run_steps(3);
    let b = st(&h).doc().unwrap().session.bookmarks().len();
    assert!(b == n + 1 || shows(&h, "Bookmark"), "key-120 Ctrl+B: {n} -> {b}");
}

/// Shift+Alt+Plus / Minus rotate the current page in the file.
#[test]
fn shortcut_rotate_pages() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let rot = |h: &H| st(h).doc().unwrap().session.doc().pages[0].rotate;
    let r0 = rot(&h);
    press(&mut h, mods(false, true, true), Key::Plus);
    assert_eq!(rot(&h), (r0 + 90) % 360, "key-137 page rotated clockwise");
    press(&mut h, mods(false, true, true), Key::Minus);
    press(&mut h, mods(false, true, true), Key::Minus);
    assert_eq!(rot(&h), (r0 + 270) % 360, "key-138 counterclockwise");
}

/// Ctrl+Shift+N inserts a blank page.
#[test]
fn shortcut_insert_blank_page() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(true, true, false), Key::N);
    h.run_steps(3);
    assert!(
        page_count(&h) == 3 || shows(&h, "Insert Blank"),
        "key-130 page count {}",
        page_count(&h)
    );
}

/// Shift+A applies the document's redactions: a redaction markup is burned in and removed.
#[test]
fn shortcut_apply_redactions() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(false, true, false), Key::R);
    drag(&mut h, (100.0, 600.0), (300.0, 700.0));
    let n = markups(&h).len();
    assert!(
        markups(&h).iter().any(|m| m.subtype == "Redact"),
        "a redaction was marked"
    );
    press(&mut h, mods(false, false, false), Key::Escape);
    press(&mut h, mods(false, true, false), Key::A);
    h.run_steps(3);
    // a confirmation may come first
    if markups(&h).len() == n {
        let ok: Vec<_> = h.query_all_by_label("Apply").collect();
        if let Some(b) = ok.last() {
            b.click();
            h.run_steps(4);
        }
    }
    assert!(
        markups(&h).iter().all(|m| m.subtype != "Redact"),
        "key-121 redactions applied"
    );
}

/// F3 / Shift+F3 step through search results.
#[test]
fn shortcut_search_results() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(true, false, false), Key::F);
    h.state_mut().state.features.search.query = "a".into();
    markupcraft_ui_egui::features::search::run_text(&mut h.state_mut().state);
    h.run_steps(10);
    let at = |h: &H| st(h).features.search.current;
    let a = at(&h);
    press(&mut h, mods(false, false, false), Key::F3);
    let b = at(&h);
    assert_ne!(a, b, "key-116 F3 next result");
    press(&mut h, mods(false, true, false), Key::F3);
    assert_eq!(at(&h), a, "key-117 Shift+F3 previous result");
}

/// Shift+F10 opens the context menu for the selection.
#[test]
fn shortcut_context_menu() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    select(&mut h, &[SQUARE]);
    press(&mut h, mods(false, true, false), Key::F10);
    h.run_steps(2);
    assert!(view(&h).context.is_some(), "key-166 the context menu is open");
}

/// Ctrl+C on a page with nothing selected, Ctrl+Alt+C: copy the page to a snapshot.
#[test]
fn shortcut_copy_page_to_snapshot() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let n = markups(&h).len();
    let clip = press_with(&mut h, mods(true, false, true), Key::C, None);
    let images = h
        .output()
        .platform_output
        .commands
        .iter()
        .any(|c| matches!(c, egui::OutputCommand::CopyImage(_)));
    assert_eq!(markups(&h).len(), n, "the page is not changed");
    assert!(
        images || clip.is_some() || st(&h).status.to_lowercase().contains("snapshot"),
        "key-069 the page went to the clipboard (status {:?})",
        st(&h).status
    );
}

// ---- key rows: files and documents ------------------------------------------------------------

/// The app with the sample saved to `dir/plan.pdf` and opened from there.
fn app_on_disk(tag: &str) -> (H, std::path::PathBuf) {
    let dir = temp_dir(tag);
    let p = sample_pdf(&dir, "plan.pdf");
    let mut h = app();
    {
        let s = &mut h.state_mut().state;
        let bytes = std::fs::read(&p).unwrap();
        s.docs.clear();
        s.open_bytes("plan.pdf", Some(p.clone()), bytes).unwrap();
        s.active = 0;
    }
    h.run_steps(4);
    (h, dir)
}

fn docs(h: &H) -> usize {
    st(h).docs.len()
}

/// Ctrl+O opens, Ctrl+N creates, Ctrl+Tab / Ctrl+Shift+Tab cycle, Ctrl+F4 closes,
/// Ctrl+Shift+W closes all.
#[test]
fn shortcut_open_new_cycle_close() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let dir = temp_dir("open");
    let other = sample_pdf(&dir, "other.pdf");
    let mut h = app();
    h.state_mut().state.dialogs.scripted = Some(vec![other]);
    press(&mut h, mods(true, false, false), Key::O);
    h.run_steps(4);
    assert_eq!(docs(&h), 2, "key-062 Ctrl+O opened the file");
    assert_eq!(st(&h).doc().unwrap().name, "other.pdf");
    h.state_mut().state.dialogs.scripted = Some(Vec::new());
    press(&mut h, mods(true, false, false), Key::N);
    h.run_steps(4);
    assert_eq!(docs(&h), 3, "key-061 Ctrl+N makes a new PDF");
    let a = st(&h).active;
    press(&mut h, mods(true, false, false), Key::Tab);
    assert_eq!(st(&h).active, (a + 1) % 3, "key-176 Ctrl+Tab next document");
    press(&mut h, mods(true, true, false), Key::Tab);
    assert_eq!(st(&h).active, a, "key-177 Ctrl+Shift+Tab previous document");
    // close the new (unsaved, unchanged) document
    let name = st(&h).doc().unwrap().name.clone();
    press(&mut h, mods(true, false, false), Key::F4);
    h.run_steps(2);
    assert_eq!(docs(&h), 2, "key-060 Ctrl+F4 closes {name}");
    press(&mut h, mods(true, true, false), Key::W);
    h.run_steps(2);
    assert_eq!(docs(&h), 0, "key-149 Ctrl+Shift+W closes all");
}

/// Ctrl+S saves in place; Ctrl+Shift+S saves to a new file; Shift+F2 saves all;
/// Ctrl+Shift+P publishes compressed; Shift+F5 reloads from disk.
#[test]
fn shortcut_save_variants() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let (mut h, dir) = app_on_disk("save");
    let path = dir.join("plan.pdf");
    let n = markups(&h).len();
    select(&mut h, &[SQUARE]);
    press(&mut h, mods(false, false, false), Key::Delete);
    assert!(st(&h).doc().unwrap().session.is_dirty());
    press(&mut h, mods(true, false, false), Key::S);
    h.run_steps(3);
    assert!(!st(&h).doc().unwrap().session.is_dirty(), "key-065 Ctrl+S saved");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "plan.pdf" }));
    let listed = call(&mut a, "markup_list", json!({}));
    assert!(!listed.to_string().contains(SQUARE), "the deletion is on disk");

    // Save As
    let copy = dir.join("copy.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![copy.clone()]);
    press(&mut h, mods(true, true, false), Key::S);
    h.run_steps(4);
    assert!(copy.exists(), "key-067 Ctrl+Shift+S wrote the new file");

    // Save All: two changed documents
    h.state_mut().state.dialogs.scripted = Some(Vec::new());
    let p2 = sample_pdf(&dir, "second.pdf");
    {
        let s = &mut h.state_mut().state;
        let bytes = std::fs::read(&p2).unwrap();
        s.open_bytes("second.pdf", Some(p2.clone()), bytes).unwrap();
    }
    h.run_steps(3);
    for i in 0..2 {
        h.state_mut().state.active = i;
        h.run_steps(2);
        select(&mut h, &[CIRCLE]);
        press(&mut h, mods(false, false, false), Key::Delete);
    }
    assert!(st(&h).docs.iter().all(|d| d.session.is_dirty()));
    press(&mut h, mods(false, true, false), Key::F2);
    h.run_steps(3);
    assert!(
        st(&h).docs.iter().all(|d| !d.session.is_dirty()),
        "key-066 Shift+F2 saved all"
    );

    // Publish compressed
    let before = std::fs::metadata(&p2).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let pubpath = dir.join("published.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![pubpath.clone()]);
    press(&mut h, mods(true, true, false), Key::P);
    h.run_steps(4);
    let after = std::fs::metadata(&p2).unwrap().modified().unwrap();
    assert!(
        pubpath.exists() || after > before,
        "key-064 Ctrl+Shift+P published a file"
    );

    // Refresh Document: an outside change shows after Shift+F5
    h.state_mut().state.active = 0;
    h.run_steps(2);
    let _ = path;
    let current = st(&h).doc().unwrap().path.clone().unwrap();
    std::fs::write(&current, markupcraft_render::synthetic::sample_pdf()).unwrap();
    press(&mut h, mods(false, true, false), Key::F5);
    h.run_steps(4);
    assert_eq!(markups(&h).len(), n, "key-135 Shift+F5 reloaded the file from disk");
}

/// Ctrl+F2 exports the markups; Ctrl+F3 imports them back.
#[test]
fn shortcut_export_import_markups() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let (mut h, dir) = app_on_disk("xfdf");
    let n = markups(&h).len();
    let out = dir.join("markups.xml");
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    press(&mut h, mods(true, false, false), Key::F2);
    h.run_steps(4);
    // a dialog may ask first: press its button if it shows
    if !out.exists() {
        if let Some(b) = h.query_all_by_label("Export").last() {
            b.click();
            h.run_steps(4);
        }
    }
    let written: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    assert!(
        written.iter().any(|p| p != &dir.join("plan.pdf")),
        "key-021 Ctrl+F2 wrote markups: {written:?}"
    );
    let file = written.into_iter().find(|p| p != &dir.join("plan.pdf")).unwrap();
    // delete them all, import back
    press(&mut h, mods(true, false, false), Key::A);
    press(&mut h, mods(false, false, false), Key::Delete);
    press(&mut h, mods(true, false, false), Key::ArrowRight);
    press(&mut h, mods(true, false, false), Key::A);
    press(&mut h, mods(false, false, false), Key::Delete);
    assert!(markups(&h).is_empty());
    h.state_mut().state.dialogs.scripted = Some(vec![file]);
    press(&mut h, mods(true, false, false), Key::F3);
    h.run_steps(4);
    if markups(&h).is_empty() {
        if let Some(b) = h.query_all_by_label("Import").last() {
            b.click();
            h.run_steps(4);
        }
    }
    assert_eq!(markups(&h).len(), n, "key-031 Ctrl+F3 imported the markups back");
}

/// Ctrl+E emails the document (a draft with the PDF attached).
#[test]
fn shortcut_email() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(true, false, false), Key::E);
    h.run_steps(3);
    let e = st(&h).shell.extra.files.last_email.clone();
    assert!(
        e.as_ref().is_some_and(|p| p.exists()) || shows(&h, "Email"),
        "key-126 Ctrl+E: {e:?}"
    );
}

/// A 2 x 2 red PNG.
const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 2, 0, 0, 0, 253, 212, 154,
    115, 0, 0, 0, 16, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 68, 12, 16, 10, 0, 31, 238, 3, 253, 139, 95, 20,
    212, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// I asks for an image, then places it where the user clicks.
#[test]
fn shortcut_image() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let dir = temp_dir("image");
    let png = dir.join("red.png");
    std::fs::write(&png, PNG).unwrap();
    let mut h = app();
    h.state_mut().state.dialogs.scripted = Some(vec![png]);
    let n = markups(&h).len();
    press(&mut h, mods(false, false, false), Key::I);
    h.run_steps(3);
    assert!(st(&h).features.pick.is_some(), "key-029 placing the image");
    click(&mut h, 200.0, 500.0);
    assert_eq!(markups(&h).len(), n + 1, "key-029 the image was placed");
}

/// Shift+Alt+R opens Review Text; Alt+Q the form fields.
#[test]
fn shortcut_review_text_and_forms() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(false, true, true), Key::R);
    assert!(h.state().state.edit.more.review_open, "key-053 Review Text is open");
    let mut h = app();
    press(&mut h, mods(false, false, true), Key::Q);
    assert!(st(&h).features.forms.open, "key-151 Forms is open");
    let mut h = app();
    press(&mut h, mods(true, true, false), Key::F);
    assert!(st(&h).features.forms.open, "key-044 Ctrl+Shift+F opens the form editor");
}

/// Ctrl+Shift+E edits the selected markup's action.
#[test]
fn shortcut_edit_action() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    select(&mut h, &[SQUARE]);
    press(&mut h, mods(true, true, false), Key::E);
    assert!(
        st(&h).features.links.editing.is_some(),
        "key-018 the action editor is open"
    );
}

/// Ctrl+Shift+I asks for a PDF and inserts its pages (after the dialog's options).
#[test]
fn shortcut_insert_pages() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let dir = temp_dir("insert");
    let other = sample_pdf(&dir, "other.pdf");
    let mut h = app();
    h.state_mut().state.dialogs.scripted = Some(vec![other]);
    press(&mut h, mods(true, true, false), Key::I);
    h.run_steps(4);
    if page_count(&h) == 2 {
        // an Insert dialog: confirm it
        if let Some(b) = h.query_all_by_label("Insert").last() {
            b.click();
            h.run_steps(4);
        }
    }
    assert_eq!(page_count(&h), 4, "key-131 two pages inserted");
}

/// Ctrl+Alt+F publishes a flattened copy.
#[test]
fn shortcut_publish_flattened() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let dir = temp_dir("flat");
    let out = dir.join("flat.pdf");
    let mut h = app();
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    press(&mut h, mods(true, false, true), Key::F);
    h.run_steps(4);
    if !out.exists()
        && let Some(b) = h.query_all_by_label_contains("Flatten").last()
    {
        b.click();
        h.run_steps(4);
    }
    assert!(out.exists(), "key-129 a flattened copy was written");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "flat.pdf" }));
    let listed = call(&mut a, "markup_list", json!({}));
    assert!(
        !listed.to_string().contains(SQUARE),
        "the copy has no live markups: {listed}"
    );
}

// ---- UI rows ------------------------------------------------------------------------------------

/// Every widget label on screen.
fn labels(h: &H) -> Vec<String> {
    use egui_kittest::kittest::NodeT;
    h.query_all_by_label_contains("")
        .filter_map(|n| {
            let a = n.accesskit_node();
            a.label().or_else(|| a.value())
        })
        .collect()
}

/// Open a top-level menu by its label.
fn open_menu(h: &mut H, name: &str) {
    h.query_all_by_label(name)
        .next()
        .unwrap_or_else(|| panic!("no menu {name}"))
        .click();
    h.run_steps(3);
}

/// Open the Preferences dialog on `page`.
fn prefs_on(h: &mut H, page: &'static str) {
    press(h, mods(true, false, false), Key::K);
    h.state_mut().state.shell.prefs_page = page;
    h.run_steps(3);
}

/// Whether any widget's label or value contains `text`.
fn has(h: &H, text: &str) -> bool {
    labels(h).iter().any(|l| l.contains(text))
}

/// The app with preferences, profiles and layout kept in `dir` (a settings folder).
fn app_with_store(dir: &std::path::Path) -> H {
    let mut h = app();
    {
        let s = &mut h.state_mut().state;
        s.shell.store = Some(markupcraft_engine::prefs::PrefStore::new(dir));
        markupcraft_ui_egui::prefs_ui::load_from_store(s);
    }
    h.run_steps(4);
    h
}

/// Click the first widget whose label is exactly `label`.
fn click_label(h: &mut H, label: &str) {
    // Bring it into view first (long menus scroll).
    h.query_all_by_label(label)
        .next()
        .unwrap_or_else(|| panic!("no widget {label:?}"))
        .scroll_to_me();
    h.run_steps(3);
    h.query_all_by_label(label)
        .next()
        .unwrap_or_else(|| panic!("no widget {label:?}"))
        .click();
    h.run_steps(3);
}

/// Click the first widget whose label contains `label`.
fn click_contains(h: &mut H, label: &str) {
    h.query_all_by_label_contains(label)
        .next()
        .unwrap_or_else(|| panic!("no widget containing {label:?}"))
        .scroll_to_me();
    h.run_steps(3);
    h.query_all_by_label_contains(label)
        .next()
        .unwrap_or_else(|| panic!("no widget containing {label:?}"))
        .click();
    h.run_steps(3);
}

/// Open a menu, hover a submenu, and return the labels that appeared.
fn submenu(h: &mut H, menu: &str, sub: &str) -> Vec<String> {
    open_menu(h, menu);
    let before = labels(h);
    h.query_all_by_label_contains(sub)
        .next()
        .unwrap_or_else(|| panic!("no submenu {sub}"))
        .hover();
    h.run_steps(4);
    labels(h).into_iter().filter(|l| !before.contains(l)).collect()
}

/// ui-073 / ui-099: a classic menu bar (application menu, File, Edit, View, Document, Batch,
/// Tools, Window, Help) that F9 hides; the application menu holds About, Preferences,
/// Profiles, Keyboard Shortcuts and Exit.
#[test]
fn ui_menu_bar_and_application_menu() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    for m in [
        "MarkupCraft",
        "File",
        "Edit",
        "View",
        "Document",
        "Batch",
        "Tools",
        "Window",
        "Help",
    ] {
        assert!(h.query_all_by_label(m).next().is_some(), "menu {m}");
    }
    open_menu(&mut h, "MarkupCraft");
    for item in [
        "About",
        "Preferences",
        "Profiles",
        "Manage Profiles",
        "Keyboard Shortcuts",
        "Administrator",
        "Exit",
    ] {
        assert!(has(&h, item), "application menu item {item}");
    }
    // ui-099: Administrator opens the administrator settings (Preferences > Admin).
    click_contains(&mut h, "Administrator");
    h.run_steps(3);
    assert!(st(&h).shell.show_prefs && st(&h).shell.prefs_page == "Admin");
    assert!(has(&h, "Reset All Settings"), "the Admin page shows");
    let mut h = app();
    press(&mut h, mods(false, false, false), Key::F9);
    assert!(h.query_all_by_label("Batch").next().is_none(), "F9 hides the menu bar");
    press(&mut h, mods(false, false, false), Key::F9);
    assert!(h.query_all_by_label("Batch").next().is_some(), "F9 shows it again");
}

/// ui-074: Alt + a menu's first letter opens the menu when the preference is on (off by default).
#[test]
fn ui_alt_menu_accelerators() {
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(false, false, true), Key::F);
    assert!(!has(&h, "Save As..."), "off by default: Alt+F opens nothing");
    prefs_on(&mut h, "Navigation");
    click_contains(&mut h, "Alt + a menu's first letter");
    assert!(st(&h).shell.ui.extra.alt_menus, "the preference is on");
    h.state_mut().state.shell.show_prefs = false;
    h.run_steps(3);
    press(&mut h, mods(false, false, true), Key::F);
    h.run_steps(2);
    assert!(has(&h, "Save As..."), "Alt+F opened the File menu");
}

/// ui-075..ui-078: named toolbars, shown and hidden from Window > Toolbars, customized in a
/// dialog, and locked.
#[test]
fn ui_toolbars_show_hide_customize_lock() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let items = submenu(&mut h, "Window", "Toolbars");
    for t in [
        "Main Toolbar",
        "Markup Tools",
        "Measure Tools",
        "Customize...",
        "Lock Toolbars",
    ] {
        assert!(
            items.iter().any(|i| i.contains(t)),
            "Toolbars menu lists {t}: {items:?}"
        );
    }
    // ui-075: named toolbars beside Main, Markup and Measure, each shown from this menu.
    for t in [
        "File",
        "Edit",
        "Navigation",
        "Zoom",
        "Shapes",
        "Text",
        "Text Markup",
        "Sketch",
        "Order",
        "Alignment",
        "Rotation",
        "Document",
    ] {
        assert!(
            h.query_all(
                egui_kittest::kittest::by()
                    .label(t)
                    .role(egui::accesskit::Role::CheckBox)
            )
            .next()
            .is_some(),
            "Toolbars menu lists {t}: {items:?}"
        );
    }
    assert!(!has(&h, "Shapes toolbar"), "named toolbars start hidden");
    click_label(&mut h, "Shapes");
    h.run_steps(3);
    assert!(has(&h, "Shapes toolbar"), "ui-075 the Shapes toolbar shows");
    // Its buttons pick the tools; its grip docks it on the left.
    let shapes = markupcraft_ui_egui::shell::toolbars_more::items(st(&h), "Shapes");
    assert!(shapes.contains(&"tool.rectangle".to_string()), "{shapes:?}");
    markupcraft_ui_egui::shell::toolbars_more::set_dock(
        &mut h.state_mut().state,
        "Shapes",
        markupcraft_ui_egui::shell::toolbars_more::Dock::Left,
    );
    h.run_steps(3);
    assert!(has(&h, "Shapes toolbar"), "docked on the left");
    let mut h = app();
    submenu(&mut h, "Window", "Toolbars");
    assert!(has(&h, "Markup toolbar"), "ui-075 the Markup toolbar shows");
    click_contains(&mut h, "Markup Tools");
    h.run_steps(2);
    assert!(!has(&h, "Markup toolbar"), "ui-076 unchecking hides the toolbar");
    let mut h = app();
    submenu(&mut h, "Window", "Toolbars");
    click_contains(&mut h, "Lock Toolbars");
    assert!(st(&h).shell.ui.toolbars.locked, "ui-078 toolbars locked");
    let mut h = app();
    submenu(&mut h, "Window", "Toolbars");
    click_contains(&mut h, "Customize...");
    h.run_steps(2);
    assert!(st(&h).shell.show_customize, "ui-077 the Customize dialog is open");
    assert!(has(&h, "Customize"), "ui-077 the Customize dialog shows");
}

/// ui-079: the Properties toolbar edits the selected markup's colour, line width...
#[test]
fn ui_properties_toolbar() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    if !st(&h).shell.ui.extra.properties_toolbar {
        open_menu(&mut h, "Window");
        click_label(&mut h, "Properties Toolbar");
    }
    assert!(st(&h).shell.ui.extra.properties_toolbar);
    let before = labels(&h);
    select(&mut h, &[SQUARE]);
    h.run_steps(2);
    let now = labels(&h);
    let fresh: Vec<_> = now.iter().filter(|l| !before.contains(l)).collect();
    assert!(!fresh.is_empty(), "the strip shows the selection's properties");
    assert!(has(&h, "Rectangle"), "it names what it edits: {fresh:?}");
}

/// ui-080: the navigation bar has page navigation, view history, split, rotate, dimmer and the
/// page scale; F4 hides it.
#[test]
fn ui_navigation_bar() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    for w in [
        "First Page",
        "Previous Page",
        "Next Page",
        "Last Page",
        "Rotate View Clockwise",
        "Split |",
        "Dimmer",
        "Scale",
    ] {
        assert!(has(&h, w), "navigation bar has {w}");
    }
    click_contains(&mut h, "Last Page (End)");
    assert_eq!(view(&h).current, 1, "Last Page goes to page 2");
    click_contains(&mut h, "First Page (Home)");
    assert_eq!(view(&h).current, 0);
    press(&mut h, mods(false, false, false), Key::F4);
    assert!(!has(&h, "Last Page (End)"), "F4 hides the navigation bar");
}

/// ui-081: the status bar holds the grid / snap / reuse toggles; F8 hides it.
#[test]
fn ui_status_bar() {
    let _serial = serial();
    let mut h = app();
    let grid = st(&h).snaps.grid;
    click_label(&mut h, "Grid");
    assert_ne!(st(&h).snaps.grid, grid, "the Grid toggle snaps to grid");
    let content = st(&h).snaps.content;
    click_label(&mut h, "Content");
    assert_ne!(st(&h).snaps.content, content);
    let reuse = st(&h).tool_locked;
    click_label(&mut h, "Reuse");
    assert_ne!(st(&h).tool_locked, reuse, "the Reuse toggle keeps the tool");
    // cursor coordinates while over the page
    h.hover_at(screen(&h, 300.0, 400.0));
    h.run_steps(3);
    assert!(view(&h).pointer.is_some(), "the pointer is tracked over the page");
    press(&mut h, mods(false, false, false), Key::F8);
    assert!(
        h.query_all_by_label("Reuse").next().is_none(),
        "F8 hides the status bar"
    );
}

/// ui-082: the page scale shows on the bar ("not set" without one); clicking starts calibration.
#[test]
fn ui_page_scale_on_bar() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    assert!(has(&h, "1/8 in = 1 ft"), "page 1's scale shows");
    press(&mut h, mods(true, false, false), Key::ArrowRight);
    assert!(
        has(&h, "not set") || has(&h, "Not set"),
        "page 2 has no scale: {:?}",
        labels(&h).iter().filter(|l| l.contains("cale")).collect::<Vec<_>>()
    );
    click_contains(&mut h, "cale");
    assert_eq!(st(&h).tool, "calibrate", "clicking the scale starts calibration");
}

/// ui-083 / ui-084: side and bottom panels from the Window menu; panel access bars.
#[test]
fn ui_panels_and_access_bars() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    open_menu(&mut h, "Window");
    for p in markupcraft_ui_egui::panels::PANELS {
        assert!(has(&h, p.title), "Window menu lists {}", p.title);
    }
    let mut h = app();
    let was = st(&h).shell.ui.extra.panel_bars;
    open_menu(&mut h, "Window");
    click_label(&mut h, "Panel Access Bars");
    assert_ne!(st(&h).shell.ui.extra.panel_bars, was, "ui-084 panel access bars toggle");
}

/// ui-087 / ui-088: the bottom panel can span the window; clicking a panel edge collapses it.
#[test]
fn ui_bottom_overlap_and_edge_collapse() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let was = st(&h).shell.ui.extra.bottom_full_width;
    open_menu(&mut h, "Window");
    click_label(&mut h, "Bottom Panel Across the Window");
    assert_ne!(
        st(&h).shell.ui.extra.bottom_full_width,
        was,
        "ui-087 bottom panel overlap toggles"
    );
    let mut h = app();
    let open_before = st(&h).open_panels.clone();
    let left: Vec<_> = open_before
        .iter()
        .filter(|p| {
            markupcraft_ui_egui::panels::find(p).is_some_and(|d| d.slot == markupcraft_ui_egui::panels::Slot::Left)
        })
        .copied()
        .collect();
    assert!(!left.is_empty(), "some left panels are open");
    click_label(&mut h, "Collapse the left panels");
    h.run_steps(3);
    let gone = left.iter().all(|p| !st(&h).open_panels.contains(p));
    assert!(
        gone || !has(&h, "Collapse the left panels"),
        "ui-088 the left panels slid shut"
    );
}

/// ui-089: the panel layout is restored on the next launch.
#[test]
fn ui_layout_persistence() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let dir = temp_dir("layout");
    let mut h = app_with_store(&dir);
    let was = panel_open(&h, "bookmarks");
    press(&mut h, mods(false, false, true), Key::B);
    h.run_steps(4);
    assert_ne!(panel_open(&h, "bookmarks"), was);
    drop(h);
    let h = app_with_store(&dir);
    assert_ne!(panel_open(&h, "bookmarks"), was, "the layout came back after a restart");
}

/// The screen point on panel `id`'s dock tab (first tab of its group only), and its group's tabs.
fn dock_tab(h: &H, id: &'static str) -> Option<(egui::Pos2, Vec<markupcraft_ui_egui::dock::Tab>)> {
    use markupcraft_ui_egui::dock::Tab;
    h.state().dock().iter_leaves().find_map(|(_, l)| {
        let tabs = l.tabs();
        (tabs.first() == Some(&Tab::Panel(id))).then(|| (l.rect().left_top() + egui::vec2(40.0, 8.0), tabs.to_vec()))
    })
}

/// The panel ids sharing a dock group with `id`.
fn group_of(h: &H, id: &'static str) -> Vec<markupcraft_ui_egui::dock::Tab> {
    use markupcraft_ui_egui::dock::Tab;
    h.state()
        .dock()
        .iter_leaves()
        .find(|(_, l)| l.tabs().contains(&Tab::Panel(id)))
        .map(|(_, l)| l.tabs().to_vec())
        .unwrap_or_default()
}

fn right_click_at(h: &mut H, at: egui::Pos2) {
    h.hover_at(at);
    h.run_steps(3);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(3);
}

/// ui-085 / ui-086: right-click a panel tab: Show Tab, Hide, Attach Left/Right/Bottom, and
/// Split Below to show two panels of a group at once.
#[test]
fn ui_panel_tab_context_menu_and_split() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    use markupcraft_ui_egui::dock::Tab;
    let mut h = app();
    let first = markupcraft_ui_egui::panels::PANELS
        .iter()
        .map(|p| p.id)
        .find(|id| dock_tab(&h, id).is_some_and(|(_, t)| t.len() > 1))
        .expect("a panel group with two tabs");
    let (at, tabs) = dock_tab(&h, first).unwrap();
    right_click_at(&mut h, at);
    for item in [
        "Show Tab",
        "Hide",
        "Attach Left",
        "Attach Right",
        "Attach Bottom",
        "Split Below",
    ] {
        assert!(has(&h, item), "ui-085 tab menu has {item}");
    }
    click_label(&mut h, "Split Below");
    h.run_steps(3);
    let group = group_of(&h, first);
    assert_eq!(
        group,
        vec![Tab::Panel(first)],
        "ui-086 the panel has its own place under the group"
    );
    let rest = tabs.iter().find(|t| **t != Tab::Panel(first)).unwrap();
    let Tab::Panel(other) = rest else { panic!() };
    assert!(!group_of(&h, other).contains(&Tab::Panel(first)), "both show at once");
    // Hide
    let mut h = app();
    let (at, _) = dock_tab(&h, first).unwrap();
    right_click_at(&mut h, at);
    click_label(&mut h, "Hide");
    assert!(!panel_open(&h, first), "ui-085 Hide closes the panel");
    // Attach to another side: it joins a different group
    let mut h = app();
    let before = group_of(&h, first);
    let (at, _) = dock_tab(&h, first).unwrap();
    right_click_at(&mut h, at);
    let target = if markupcraft_ui_egui::panels::find(first).unwrap().slot == markupcraft_ui_egui::panels::Slot::Right {
        "Attach Left"
    } else {
        "Attach Right"
    };
    click_label(&mut h, target);
    h.run_steps(3);
    let after = group_of(&h, first);
    assert!(
        panel_open(&h, first) && after != before,
        "ui-085 {target} moved it: {before:?} -> {after:?}"
    );
}

/// ui-090 / ui-094 / ui-196: each open PDF is a tab; Ctrl+Tab cycles; the tab menu closes
/// one, the others or all.
#[test]
fn ui_document_tabs() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    for name in ["b.pdf", "c.pdf"] {
        h.state_mut()
            .state
            .open_bytes(name, None, markupcraft_render::synthetic::sample_pdf())
            .unwrap();
    }
    h.run_steps(3);
    for name in ["sample.pdf", "b.pdf", "c.pdf"] {
        assert!(h.query_all_by_label(name).next().is_some(), "tab {name}");
    }
    click_label(&mut h, "sample.pdf");
    assert_eq!(st(&h).doc().unwrap().name, "sample.pdf", "clicking a tab shows it");
    press(&mut h, mods(true, false, false), Key::Tab);
    assert_eq!(st(&h).doc().unwrap().name, "b.pdf", "ui-196 Ctrl+Tab");
    let at = h.query_all_by_label("b.pdf").next().unwrap().rect().center();
    right_click_at(&mut h, at);
    for item in ["Close", "Close Others", "Close All", "Detach to New Window"] {
        assert!(has(&h, item), "tab menu {item}");
    }
    click_label(&mut h, "Close Others");
    assert_eq!(st(&h).docs.len(), 1, "ui-094 Close Others");
    open_menu(&mut h, "File");
    click_contains(&mut h, "Close All");
    assert!(st(&h).docs.is_empty(), "ui-094 Close All");
}

/// ui-091: long tab names are shortened at the end or the start (preference).
#[test]
fn ui_tab_truncation() {
    let _serial = serial();
    let mut h = app();
    let long = "A-101 Floor Plan Level 1 With A Very Long Title.pdf";
    h.state_mut()
        .state
        .open_bytes(long, None, markupcraft_render::synthetic::sample_pdf())
        .unwrap();
    h.state_mut().state.shell.ui.tab_max_chars = 16;
    h.run_steps(3);
    let tab_text = |h: &H| {
        labels(h)
            .into_iter()
            .filter(|l| l.contains("A-101") || l.contains("Title.pdf"))
            .collect::<Vec<_>>()
    };
    assert!(
        tab_text(&h)
            .iter()
            .any(|l| l.starts_with("A-101 Floor") && l.ends_with("...")),
        "shortened at the end: {:?}",
        tab_text(&h)
    );
    h.state_mut().state.shell.ui.tab_truncate_start = true;
    h.run_steps(3);
    assert!(
        tab_text(&h)
            .iter()
            .any(|l| l.starts_with("...") && l.ends_with("Title.pdf")),
        "shortened at the start: {:?}",
        tab_text(&h)
    );
    prefs_on(&mut h, "General");
    assert!(has(&h, "Shorten long names at the"), "the preference is on General");
}

/// ui-092: Window > Auto-Hide Tabs hides the document tabs until the pointer reaches the top.
#[test]
fn ui_auto_hide_tabs() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let tabs = |h: &H| h.query_all_by_label("sample.pdf").count();
    let shown = tabs(&h);
    open_menu(&mut h, "Window");
    click_label(&mut h, "Auto-Hide Tabs");
    assert!(st(&h).shell.ui.extra.auto_hide_tabs);
    h.hover_at(screen(&h, 600.0, 300.0));
    h.run_steps(6);
    assert!(tabs(&h) < shown, "tabs hidden: {} -> {}", shown, tabs(&h));
    let top = st(&h).shell.extra.doc_area.unwrap().center_top() + egui::vec2(0.0, 2.0);
    h.hover_at(top);
    h.run_steps(6);
    assert_eq!(tabs(&h), shown, "tabs back at the top edge");
}

/// ui-093: Detach a tab into its own window; Reattach brings it back.
#[test]
fn ui_detach_tab() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    h.state_mut()
        .state
        .open_bytes("b.pdf", None, markupcraft_render::synthetic::sample_pdf())
        .unwrap();
    h.run_steps(3);
    open_menu(&mut h, "Window");
    click_label(&mut h, "Detach Tab to New Window");
    h.run_steps(3);
    assert_eq!(st(&h).shell.extra.detached.len(), 1, "a detached window");
    assert!(has(&h, "Reattach"), "the detached window offers Reattach");
    open_menu(&mut h, "Window");
    click_label(&mut h, "Reattach All Windows");
    if !st(&h).shell.extra.detached.is_empty() {
        click_label(&mut h, "Reattach");
    }
    assert!(
        st(&h).shell.extra.detached.is_empty(),
        "reattached: status {}",
        st(&h).status
    );
}

/// ui-095: Shift+F10 opens the context menu of the selection, with its commands.
#[test]
fn ui_context_menu_key() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    select(&mut h, &[SQUARE]);
    press(&mut h, mods(false, true, false), Key::F10);
    h.run_steps(2);
    for item in ["Copy", "Delete", "Lock"] {
        assert!(has(&h, item), "context menu shows {item}");
    }
}

/// ui-096: Always on Top asks the window to stay above other applications.
#[test]
fn ui_always_on_top() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    open_menu(&mut h, "Window");
    click_contains(&mut h, "Always on Top");
    assert!(st(&h).shell.always_on_top, "Window > Always on Top");
}

/// ui-097: a Web Tab is a browser-like tab with an address.
#[test]
fn ui_web_tab() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let n = labels(&h).len();
    press(&mut h, mods(true, false, false), Key::T);
    h.run_steps(3);
    let _ = n;
    assert!(
        st(&h).docs.iter().any(|d| d.name.contains("Web")),
        "a Web Tab opened: {}",
        st(&h).status
    );
    assert!(st(&h).doc().unwrap().name.contains("Web"), "and shows");
}

/// ui-098: the File Access panel lists recent files; clicking one opens it.
#[test]
fn ui_file_access_recent_files() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let dir = temp_dir("recent");
    let other = sample_pdf(&dir, "recent-one.pdf");
    let mut h = app();
    h.state_mut().state.dialogs.scripted = Some(vec![other]);
    press(&mut h, mods(true, false, false), Key::O);
    h.run_steps(3);
    press(&mut h, mods(true, false, false), Key::F4);
    assert!(st(&h).docs.iter().all(|d| d.name != "recent-one.pdf"));
    if !panel_open(&h, "file_access") {
        press(&mut h, mods(false, false, true), Key::A);
    }
    h.state_mut().state.show_panel("file_access");
    h.run_steps(4);
    assert!(has(&h, "recent-one.pdf"), "the recent file is listed");
    // the entry in the panel (the status bar may name the file too)
    let entry = |h: &H| {
        h.query_all_by_label("recent-one.pdf")
            .map(|n| n.rect())
            .find(|r| r.left() < 300.0)
            .expect("the File Access entry")
    };
    let at = entry(&h).center();
    h.hover_at(at);
    h.step();
    button(&mut h, at, true, Modifiers::NONE);
    button(&mut h, at, false, Modifiers::NONE);
    h.run_steps(3);
    if !st(&h).docs.iter().any(|d| d.name == "recent-one.pdf") {
        // a double-click opens
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.step();
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(4);
    }
    assert!(
        st(&h).docs.iter().any(|d| d.name == "recent-one.pdf"),
        "opened from File Access: {:?} status {}",
        h.query_all_by_label_contains("recent-one.pdf")
            .map(|n| n.rect())
            .collect::<Vec<_>>(),
        st(&h).status
    );
}

/// ui-100..ui-104: profiles: several shipped; switching rearranges the interface; the current
/// arrangement is kept with the profile; create / delete; export and import.
#[test]
fn ui_profiles() {
    let _serial = serial();
    let dir = temp_dir("profiles");
    let mut h = app_with_store(&dir);
    let items = submenu(&mut h, "MarkupCraft", "Profiles");
    assert!(!items.is_empty(), "ui-100 Profiles lists profiles: {items:?}");
    // Create "Review" with the markup toolbar hidden, switch back: the toolbar returns.
    let first = st(&h).shell.store.as_ref().unwrap().active();
    prefs_on(&mut h, "Admin");
    h.query_all_by_label("New Profile").next().expect("New Profile button");
    // type the name into the field beside the button
    let field = h
        .query_all(egui_kittest::kittest::by().role(egui::accesskit::Role::TextInput))
        .last()
        .expect("profile name field");
    field.focus();
    h.run_steps(1);
    h.query_all(egui_kittest::kittest::by().role(egui::accesskit::Role::TextInput))
        .last()
        .unwrap()
        .type_text("Review");
    h.run_steps(2);
    click_label(&mut h, "New Profile");
    h.run_steps(3);
    let store = st(&h).shell.store.clone().unwrap();
    assert_eq!(
        store.active(),
        "Review",
        "ui-103 created and active: {:?}",
        store.profiles()
    );
    h.state_mut().state.shell.ui.toolbars.show_markup = false;
    h.run_steps(3);
    assert!(
        store.profiles().contains(&first),
        "the profile we left is still listed: {:?}",
        store.profiles()
    );
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "switch", &first);
    h.run_steps(3);
    assert!(
        st(&h).shell.ui.toolbars.show_markup,
        "ui-101 the other profile's toolbars"
    );
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "switch", "Review");
    h.run_steps(3);
    assert!(
        !st(&h).shell.ui.toolbars.show_markup,
        "ui-102 Review kept its arrangement"
    );
    // Manage: delete Review (from another profile)
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "switch", &first);
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "delete", "Review");
    assert!(!store.profiles().contains(&"Review".to_string()), "ui-103 deleted");
    // Export / import (Admin > Back Up / Restore Settings)
    let out = dir.join("profile-backup.json");
    prefs_on(&mut h, "Admin");
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    click_label(&mut h, "Back Up Settings...");
    h.run_steps(4);
    assert!(out.exists(), "ui-104 the profile was exported");

    // ui-100: MarkupCraft ships profiles, each with its own interface.
    h.state_mut().state.shell.show_prefs = false;
    h.run_steps(2);
    let items = submenu(&mut h, "MarkupCraft", "Profiles");
    for p in ["Takeoff", "Construction", "Design Review", "Simple"] {
        assert!(items.iter().any(|i| i == p), "ui-100 {p} ships: {items:?}");
    }
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "switch", "Simple");
    h.run_steps(3);
    assert!(
        !st(&h).shell.ui.toolbars.show_markup && !st(&h).shell.ui.toolbars.show_measure,
        "ui-100 the Simple profile hides the tool strips"
    );
    assert!(!st(&h).shell.prefs.snapping.content, "and turns snapping off");
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "switch", "Takeoff");
    h.run_steps(3);
    assert!(st(&h).shell.ui.rulers, "ui-100 Takeoff shows the rulers");

    // ui-103: rename the active profile in Preferences > Admin.
    prefs_on(&mut h, "Admin");
    assert!(has(&h, "Rename Profile"), "Admin renames profiles");
    h.state_mut().state.shell.extra2.rename_to = "Estimating".into();
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "rename", "Takeoff");
    h.run_steps(3);
    assert_eq!(store.active(), "Estimating", "ui-103 renamed, still active");
    assert!(
        dir.join("ui").join("Estimating.json").exists(),
        "its interface moved with it"
    );

    // ui-104: export the profile with its dependencies (the Tool Chest), import it elsewhere.
    std::fs::write(dir.join("toolchest.json"), "{\"sets\": []}").unwrap();
    let bundle = dir.join("estimating.mcprofile");
    h.state_mut().state.dialogs.scripted = Some(vec![bundle.clone()]);
    click_label(&mut h, "Include dependencies");
    assert!(st(&h).shell.extra2.export_dependencies);
    click_label(&mut h, "Export Profile...");
    h.run_steps(4);
    let text = std::fs::read_to_string(&bundle).expect("ui-104 profile file written");
    assert!(
        text.contains("toolchest.json") && text.contains("\"interface\""),
        "{text}"
    );
    let other_dir = temp_dir("profiles-import");
    let mut other = app_with_store(&other_dir);
    other.state_mut().state.dialogs.scripted = Some(vec![bundle.clone()]);
    prefs_on(&mut other, "Admin");
    click_label(&mut other, "Import Profile...");
    other.run_steps(4);
    let ost = st(&other).shell.store.clone().unwrap();
    assert_eq!(ost.active(), "Estimating", "ui-104 imported and switched to");
    assert!(st(&other).shell.ui.rulers, "the interface came along");
    assert!(other_dir.join("toolchest.json").exists(), "the dependencies came along");
}

/// ui-105: the Keyboard Shortcuts dialog lists every command with its keys; a new keystroke
/// rebinds a command, taking the key from another command; menus show the new key.
#[test]
fn ui_keyboard_shortcuts_dialog() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    open_menu(&mut h, "MarkupCraft");
    click_contains(&mut h, "Customize Keyboard");
    h.run_steps(2);
    assert!(has(&h, "Rectangle") && has(&h, "Fit Width"), "commands listed");
    let find = |h: &mut H, text: &str| {
        h.state_mut().state.keys.filter = text.into();
        h.run_steps(3);
    };
    // Rectangle: click its key (R), press F6.
    find(&mut h, "Rectangle");
    click_label(&mut h, "R");
    assert!(
        has(&h, "Press keys"),
        "waiting for the new keys: filter {:?} capturing {:?} R-labels {}",
        st(&h).keys.filter,
        st(&h).keys.capturing,
        h.query_all_by_label("R").count()
    );
    press(&mut h, mods(false, false, false), Key::F6);
    // Ellipse: click its key (E), press F6, which Rectangle has: Ellipse takes it.
    find(&mut h, "Ellipse");
    click_label(&mut h, "E");
    press(&mut h, mods(false, false, false), Key::F6);
    assert!(
        st(&h).status.contains("taken"),
        "reassigned from Rectangle: {}",
        st(&h).status
    );
    h.state_mut().state.keys.show = false;
    h.run_steps(2);
    press(&mut h, mods(false, false, false), Key::F6);
    assert_eq!(st(&h).tool, "ellipse", "F6 runs Ellipse now");
    open_menu(&mut h, "Markup");
    assert!(
        has(&h, "Ellipse F6"),
        "the menu shows the new key: {:?}",
        labels(&h)
            .iter()
            .filter(|l| l.starts_with("Ellipse"))
            .collect::<Vec<_>>()
    );
    let fwd: Vec<_> = labels(&h)
        .into_iter()
        .filter(|l| l.starts_with("Bring Forward"))
        .collect();
    assert!(fwd.iter().any(|l| l.ends_with("+]")), "keys shown as typed: {fwd:?}");
}

/// ui-106: single-key tool shortcuts (Select V, Pan Shift+V, Zoom Z, Lasso Shift+O).
#[test]
fn ui_single_key_tool_shortcuts() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    for (m, k, tool) in [
        (mods(false, true, false), Key::V, "pan"),
        (mods(false, false, false), Key::Z, "zoom"),
        (mods(false, true, false), Key::O, "lasso"),
        (mods(false, false, false), Key::V, "select"),
    ] {
        press(&mut h, m, k);
        assert_eq!(st(&h).tool, tool);
    }
}

/// ui-107: Help > Shortcut Reference writes a printable PDF of shortcuts, mouse and modifiers.
#[test]
fn ui_printable_shortcut_reference() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let dir = temp_dir("keyref");
    let out = dir.join("shortcuts.pdf");
    let mut h = app();
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    open_menu(&mut h, "Help");
    click_contains(&mut h, "Shortcut Reference");
    h.run_steps(4);
    assert!(out.exists(), "a PDF was written");
    let mut a = automation(&dir);
    let opened = call(&mut a, "doc_open", json!({ "path": "shortcuts.pdf" }));
    let pages = opened["pages"].as_u64().unwrap_or(1);
    let mut text = String::new();
    for p in 1..=pages {
        text.push_str(&call(&mut a, "page_text", json!({ "page": p })).to_string());
    }

    for w in ["Ctrl", "Shift", "Rectangle"] {
        assert!(text.contains(w), "the reference mentions {w}");
    }
}

// ---- view modes, rulers, grid and snap ------------------------------------------------------

/// Drag between two page points with modifiers held (as the platform reports them).
fn drag_mods(h: &mut H, from: (f64, f64), to: (f64, f64), m: Modifiers) {
    let (a, b) = (screen(h, from.0, from.1), screen(h, to.0, to.1));
    h.event(egui::Event::ModifiersChanged(m));
    h.hover_at(a);
    h.step();
    button(h, a, true, m);
    for i in 1..=8 {
        let t = i as f32 / 8.0;
        h.hover_at(a + (b - a) * t);
        h.step();
    }
    button(h, b, false, m);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
}

/// Click a page point with modifiers held.
fn click_mods(h: &mut H, x: f64, y: f64, m: Modifiers) {
    let at = screen(h, x, y);
    h.event(egui::Event::ModifiersChanged(m));
    h.hover_at(at);
    h.step();
    button(h, at, true, m);
    button(h, at, false, m);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
}

/// The newest markup's points.
fn last_pts(h: &H) -> Vec<Point> {
    markups(h).last().map(|m| m.pts.clone()).unwrap_or_default()
}

/// Draw a line with the Line tool (drag) and return its two ends.
fn draw_line(h: &mut H, a: (f64, f64), b: (f64, f64), m: Modifiers) -> (Point, Point) {
    press(h, mods(false, false, false), Key::L);
    let n = markups(h).len();
    drag_mods(h, a, b, m);
    assert_eq!(markups(h).len(), n + 1, "a line was drawn");
    let p = last_pts(h);
    (p[0], *p.last().unwrap())
}

fn near(p: Point, x: f64, y: f64) -> bool {
    (p.x - x).abs() < 0.01 && (p.y - y).abs() < 0.01
}

fn snaps_off(h: &mut H) {
    let s = &mut h.state_mut().state.snaps;
    s.grid = false;
    s.content = false;
    s.markup = false;
}

/// ui-110: the Zoom tool: click zooms in, Ctrl+click out, a dragged box zooms to that area.
#[test]
fn ui_zoom_tool() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(false, false, false), Key::Z);
    let z = view(&h).zoom;
    click(&mut h, 600.0, 400.0);
    assert!(view(&h).zoom > z * 1.1, "click zooms in: {z} -> {}", view(&h).zoom);
    let z = view(&h).zoom;
    click_mods(&mut h, 600.0, 400.0, mods(true, false, false));
    assert!(view(&h).zoom < z * 0.95, "Ctrl+click zooms out");
    let mut h = app();
    press(&mut h, mods(false, false, false), Key::Z);
    let z = view(&h).zoom;
    drag(&mut h, (500.0, 300.0), (600.0, 400.0));
    assert!(
        view(&h).zoom > z * 3.0,
        "a 100-pt box fills the view: {z} -> {}",
        view(&h).zoom
    );
}

/// ui-111: Side by Side modes can show the first page alone (a cover).
#[test]
fn ui_page_layout_cover_page() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(true, false, false), Key::Num6);
    let c = view(&h).cover;
    open_menu(&mut h, "View");
    click_label(&mut h, "Show Cover Page Alone");
    assert_ne!(view(&h).cover, c, "the cover option toggles");
}

/// ui-112: the navigation bar's page box takes a page number.
#[test]
fn ui_page_number_box() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let field = h
        .query_all(egui_kittest::kittest::by().role(egui::accesskit::Role::TextInput))
        .find(|n| {
            (n.rect().center().y
                - h.query_all_by_label_contains("Next Page")
                    .next()
                    .unwrap()
                    .rect()
                    .center()
                    .y)
                .abs()
                < 12.0
        })
        .expect("the page number box");
    field.focus();
    h.run_steps(1);
    h.event(egui::Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.event(egui::Event::Text("2".into()));
    h.run_steps(1);
    h.event(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(4);
    assert_eq!(view(&h).current, 1, "typing 2 goes to page 2");
}

/// ui-116: the workspace splits; Revu allows up to 16 panes.
#[test]
fn ui_split_repeatedly() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(true, false, false), Key::Num2);
    {
        let at = h.query_all_by_label("Close this pane").next().unwrap().rect().center();
        h.hover_at(at);
        h.step();
        button(&mut h, at, true, Modifiers::NONE);
        button(&mut h, at, false, Modifiers::NONE);
        h.run_steps(3);
        assert!(st(&h).shell.split.is_none(), "closing the second pane unsplits");
        press(&mut h, mods(true, false, false), Key::Num2);
    }
    press(&mut h, mods(true, false, false), Key::H);
    assert!(st(&h).shell.split.is_some(), "split");
    let panes = |h: &H| st(h).shell.split.as_ref().map_or(1, |s| s.panes());
    assert_eq!(panes(&h), 3, "splitting again adds a pane");
    for _ in 0..20 {
        press(&mut h, mods(true, false, false), Key::Num2);
    }
    assert_eq!(panes(&h), 16, "up to 16 panes");
    // Every pane draws its own header (a document picker and a close button).
    assert_eq!(h.query_all_by_label("Close this pane").count(), 15);
    // Closing one pane leaves the others; Unsplit goes back to one.
    let at = h.query_all_by_label("Close this pane").nth(3).unwrap().rect().center();
    h.hover_at(at);
    h.step();
    button(&mut h, at, true, Modifiers::NONE);
    button(&mut h, at, false, Modifiers::NONE);
    h.run_steps(3);
    assert_eq!(panes(&h), 15);
    press(&mut h, mods(true, true, false), Key::Num2);
    assert!(st(&h).shell.split.is_none(), "Unsplit");
}

/// ui-118: synchronized views: in Document mode the other pane follows the page.
#[test]
fn ui_synchronize_views() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(true, false, false), Key::Num2);
    open_menu(&mut h, "View");
    click_label(&mut h, "Synchronize: Document");
    press(&mut h, mods(true, false, false), Key::ArrowRight);
    h.run_steps(3);
    let pane = st(&h).shell.split.as_ref().unwrap().pane.view.current;
    assert_eq!(pane, view(&h).current, "the other pane shows the same page");
}

/// ui-119 / ui-120 / ui-193: full screen hides the chrome; presentation options live in
/// Preferences > Window.
#[test]
fn ui_full_screen_and_presentation_options() {
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(false, false, false), Key::F11);
    assert!(
        h.query_all_by_label("Batch").next().is_none(),
        "the menu bar is hidden in full screen"
    );
    press(&mut h, mods(false, false, false), Key::Escape);
    assert!(h.query_all_by_label("Batch").next().is_some());
    prefs_on(&mut h, "Window");
    assert!(has(&h, "Loop back to the first page"), "presentation options");
}

/// ui-121 / ui-122 / ui-124 / ui-136 / ui-137: View toggles.
#[test]
fn ui_view_toggles() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    type Get = fn(&H) -> bool;
    let cases: &[(&str, (&str, Get))] = &[
        ("ui-121 Dimmer", ("Dimmer", |h| st(h).shell.dimmer)),
        (
            "ui-122 Dark Mode",
            ("Dark Mode", |h| st(h).shell.ui.extra.dark_workspace),
        ),
        (
            "ui-124 Reply indicators",
            ("Always Show Reply Indicators", |h| {
                st(h).shell.ui.extra.reply_indicators
            }),
        ),
        (
            "ui-136 Crosshair",
            ("Full-Screen Crosshair", |h| st(h).shell.ui.crosshair),
        ),
        (
            "ui-137 Line weights",
            ("Disable Line Weights", |h| st(h).shell.ui.extra.thin_lines),
        ),
    ];
    run_cases(cases, |&(item, get)| {
        let mut h = app();
        let before = get(&h);
        open_menu(&mut h, "View");
        click_contains(&mut h, item);
        assert_ne!(get(&h), before, "View > {item} toggles");
    });
}

/// ui-123: the Dark / Light theme preference restyles the application.
#[test]
fn ui_theme() {
    let _serial = serial();
    let mut h = app();
    prefs_on(&mut h, "General");
    click_label(&mut h, "Dark");
    h.run_steps(3);
    assert!(h.ctx.global_style().visuals.dark_mode, "dark theme");
    click_label(&mut h, "Light");
    h.run_steps(3);
    assert!(!h.ctx.global_style().visuals.dark_mode, "light theme");
}

/// ui-125 / ui-126: Ctrl+R shows rulers; right-clicking a ruler picks its unit.
#[test]
fn ui_rulers_and_units() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    if !st(&h).shell.ui.rulers {
        press(&mut h, mods(true, false, false), Key::R);
    }
    assert!(st(&h).shell.ui.rulers);
    let area = st(&h).shell.extra.doc_area.unwrap();
    let unit = format!("{:?}", st(&h).shell.ui.ruler_unit);
    let mut menu = Vec::new();
    for dy in (2..90).step_by(4) {
        let before = labels(&h);
        right_click_at(&mut h, area.left_top() + egui::vec2(300.0, dy as f32));
        menu = labels(&h).into_iter().filter(|l| !before.contains(l)).collect();
        if menu.iter().any(|l| l.to_lowercase().contains("inch")) {
            break;
        }
        press(&mut h, mods(false, false, false), Key::Escape);
    }
    assert!(menu.len() >= 5, "the ruler menu lists units: {menu:?}");
    let pick = menu
        .iter()
        .find(|l| l.to_lowercase().contains("cent") || l.contains("cm"))
        .cloned()
        .expect("centimetres");
    click_label(&mut h, &pick);
    assert_ne!(format!("{:?}", st(&h).shell.ui.ruler_unit), unit, "the unit changed");
}

/// ui-127..ui-129: Show Grid draws; Snap to Grid puts points on the grid (hidden or not); the
/// spacing comes from Preferences > Grid & Snap.
#[test]
fn ui_grid_snap_and_spacing() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    {
        let s = &mut h.state_mut().state;
        s.shell.prefs.snapping.grid_spacing = 36.0;
        s.shell.prefs.snapping.grid = true;
        s.shell.prefs.snapping.content = false;
        s.shell.prefs.snapping.markup = false;
        markupcraft_ui_egui::prefs_ui::apply_live(s);
    }
    assert!(
        st(&h).snaps.grid && !st(&h).show_grid,
        "snap to grid with the grid hidden"
    );
    let (a, b) = draw_line(&mut h, (101.0, 103.0), (299.0, 211.0), Modifiers::NONE);
    for p in [a, b] {
        assert!((p.x / 36.0 - (p.x / 36.0).round()).abs() < 1e-6, "x on the grid: {p:?}");
        assert!((p.y / 36.0 - (p.y / 36.0).round()).abs() < 1e-6, "y on the grid: {p:?}");
    }
    prefs_on(&mut h, "Grid & Snap");
    assert!(
        has(&h, "Grid spacing") && has(&h, "Units"),
        "spacing and units on Grid & Snap"
    );
}

/// ui-130 / ui-135: Snap to Content lands on drawing linework; Ctrl ignores all snaps.
#[test]
fn ui_snap_to_content_and_ctrl_override() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    h.state_mut().state.snaps.content = true;
    h.run_steps(10);
    let (a, _) = draw_line(&mut h, (122.0, 182.0), (400.0, 450.0), Modifiers::NONE);
    assert!(near(a, 120.0, 180.0), "snapped to the wall corner: {a:?}");
    let (a, _) = draw_line(&mut h, (122.0, 182.0), (400.0, 450.0), mods(true, false, false));
    assert!(!near(a, 120.0, 180.0), "Ctrl ignores the snap: {a:?}");
}

/// ui-131: Snap to Markup lands on other markups' points.
#[test]
fn ui_snap_to_markup() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    h.state_mut().state.snaps.markup = true;
    let (a, _) = draw_line(&mut h, (902.0, 602.0), (500.0, 450.0), Modifiers::NONE);
    assert!(near(a, 900.0, 600.0), "snapped to the rectangle's corner: {a:?}");
}

/// ui-132 / ui-133: the snap targets and the capture radius are preferences.
#[test]
fn ui_snap_filters_and_sensitivity() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    h.state_mut().state.snaps.content = true;
    {
        let s = &mut h.state_mut().state;
        s.shell.prefs.snapping.content = true;
        s.shell.prefs.snapping.markup = false;
        s.shell.prefs.snapping.grid = false;
        s.shell.prefs.snapping.sensitivity_px = 1.0;
        markupcraft_ui_egui::prefs_ui::apply_live(s);
    }
    h.run_steps(10);
    let (a, _) = draw_line(&mut h, (126.0, 186.0), (400.0, 450.0), Modifiers::NONE);
    assert!(!near(a, 120.0, 180.0), "1 px: too far to snap: {a:?}");
    {
        let s = &mut h.state_mut().state;
        s.shell.prefs.snapping.sensitivity_px = 25.0;
        markupcraft_ui_egui::prefs_ui::apply_live(s);
    }
    let (a, _) = draw_line(&mut h, (126.0, 186.0), (400.0, 450.0), Modifiers::NONE);
    assert!(near(a, 120.0, 180.0), "25 px: snaps: {a:?}");
    // Endpoints and intersections off: the corner is no longer a target.
    prefs_on(&mut h, "Grid & Snap");
    click_label(&mut h, "Endpoints");
    click_label(&mut h, "Intersections");
    h.state_mut().state.shell.show_prefs = false;
    h.run_steps(3);
    let (a, _) = draw_line(&mut h, (126.0, 186.0), (400.0, 450.0), Modifiers::NONE);
    assert!(!near(a, 120.0, 180.0), "corner not a target with endpoints off: {a:?}");
}

/// ui-134: the snap indicator colour is a preference.
#[test]
fn ui_snap_indicator_color() {
    let _serial = serial();
    let mut h = app();
    prefs_on(&mut h, "Grid & Snap");
    assert!(has(&h, "Snap indicator and crosshair colour"));
    h.state_mut().state.shell.ui.extra.snap_color = Some([0, 200, 0]);
    h.run_steps(3);
    assert_eq!(st(&h).snaps.color, Some([0, 200, 0]), "the indicator uses it");
}

// ---- the Preferences dialog ---------------------------------------------------------------

/// Type `text` into the `n`th text field of the Preferences window (top to bottom).
fn type_into_prefs_field(h: &mut H, n: usize, text: &str) {
    let win = h.query_all_by_label("Preferences").next().unwrap().rect();
    let mut fields: Vec<_> = h
        .query_all(egui_kittest::kittest::by().role(egui::accesskit::Role::TextInput))
        .map(|f| f.rect())
        .filter(|r| r.top() >= win.top() && r.left() >= win.left() - 1.0)
        .collect();
    fields.sort_by(|a, b| a.top().total_cmp(&b.top()));
    let r = *fields.get(n).expect("the field");
    let at = r.center();
    h.hover_at(at);
    h.step();
    button(h, at, true, Modifiers::NONE);
    button(h, at, false, Modifiers::NONE);
    h.event(egui::Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.event(egui::Event::Text(text.into()));
    h.run_steps(3);
}

/// ui-139 / ui-140: Ctrl+K opens one dialog with a page list; settings persist per user; the
/// user name is the author of new markups.
#[test]
fn ui_preferences_dialog_and_user_name() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let dir = temp_dir("prefs");
    let mut h = app_with_store(&dir);
    press(&mut h, mods(true, false, false), Key::K);
    for p in [
        "General",
        "Document",
        "Navigation",
        "Grid & Snap",
        "Interface",
        "Tools",
        "Window",
        "Advanced",
        "Admin",
    ] {
        assert!(has(&h, p), "page {p}");
    }
    h.state_mut().state.shell.prefs_page = "General";
    h.run_steps(2);
    type_into_prefs_field(&mut h, 0, "Pat Example");
    h.state_mut().state.shell.show_prefs = false;
    h.run_steps(3);
    assert_eq!(st(&h).author, "Pat Example", "ui-140 the user name is the author");
    press(&mut h, mods(false, false, false), Key::R);
    drag(&mut h, (200.0, 500.0), (300.0, 560.0));
    assert_eq!(
        markups(&h).last().unwrap().author,
        "Pat Example",
        "new markups carry it"
    );
    drop(h);
    let h = app_with_store(&dir);
    assert_eq!(
        st(&h).shell.prefs.author,
        "Pat Example",
        "ui-139 kept for the next session"
    );
}

/// ui-141: the theme is on General > Options; there is no UI language choice.
#[test]
fn ui_language_and_theme() {
    let _serial = serial();
    let mut h = app();
    prefs_on(&mut h, "General");
    assert!(has(&h, "Theme") && has(&h, "Dark") && has(&h, "Light"));
    assert!(!has(&h, "Language"), "no language option (English only)");
}

/// ui-142: startup options: start mode, a PDF to open, reopen last session, recents, full screen.
#[test]
fn ui_startup_options() {
    let _serial = serial();
    let mut h = app();
    prefs_on(&mut h, "General");
    for o in [
        "Reopen the files that were open last time",
        "Show recent files on the start page",
        "Markup mode",
        "View mode",
        "Browse...",
        "Start in full screen",
    ] {
        assert!(has(&h, o), "startup option {o}");
    }
    let mut h = app();
    {
        let s = &mut h.state_mut().state;
        s.shell.ui.extra.startup_full_screen = true;
        s.shell.ui.extra.startup_mode = "view".into();
        markupcraft_ui_egui::shell::extra::startup(s);
    }
    h.run_steps(3);
    assert_eq!(
        st(&h).shell.screen,
        markupcraft_ui_egui::shell::Screen::FullScreen,
        "starts in full screen"
    );
    assert!(
        !st(&h).shell.ui.toolbars.show_markup,
        "view mode hides the markup tools"
    );
    // ui-142: a home Web Tab on start; hidden messages come back with Reset Hidden Messages.
    let mut h = app();
    prefs_on(&mut h, "General");
    assert!(has(&h, "Open a Web Tab of the favourites on start"));
    h.state_mut().state.shell.show_prefs = false;
    {
        let s = &mut h.state_mut().state;
        s.shell.ui.extra2.home_web_tab = true;
        markupcraft_ui_egui::shell::extra2::startup(s);
    }
    h.run_steps(4);
    assert!(
        st(&h).docs.len() == 2 || shows(&h, "Web"),
        "the Web Tab opened on start: {}",
        st(&h).status
    );
    let mut h = app();
    markupcraft_ui_egui::shell::extra2::hide(&mut h.state_mut().state, "sign-warning");
    prefs_on(&mut h, "General");
    assert!(shows(&h, "1 hidden"));
    click_label(&mut h, "Reset Hidden Messages");
    assert!(
        st(&h).shell.ui.extra2.hidden_messages.is_empty(),
        "ui-142 hidden messages reset"
    );
}

/// ui-143: save mode: Keep revisions appends to the file; Publish rewrites it.
#[test]
fn ui_save_mode_and_recovery() {
    let _serial = serial();
    let mut h = app();
    prefs_on(&mut h, "Document");
    assert!(has(&h, "Save recovery data every"), "document recovery");
    for (mode, appends) in [
        ("Keep revisions (incremental)", true),
        ("Publish (full rewrite)", false),
    ] {
        let (mut h, dir) = app_on_disk("savemode");
        let path = dir.join("plan.pdf");
        let original = std::fs::read(&path).unwrap();
        prefs_on(&mut h, "Document");
        click_label(&mut h, mode);
        h.state_mut().state.shell.show_prefs = false;
        h.run_steps(3);
        select(&mut h, &[SQUARE]);
        press(&mut h, mods(false, false, false), Key::Delete);
        press(&mut h, mods(true, false, false), Key::S);
        h.run_steps(3);
        let saved = std::fs::read(&path).unwrap();
        assert_eq!(
            saved.starts_with(&original),
            appends,
            "{mode}: the old bytes are kept at the start"
        );
    }
}

/// ui-144: the default page layout and fit apply to documents as they open; maximum zoom caps it.
#[test]
fn ui_default_layout_fit_and_max_zoom() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    {
        let u = &mut h.state_mut().state.shell.ui;
        u.default_mode = "continuous".into();
        u.default_fit = "width".into();
        u.max_zoom_pct = 200.0;
    }
    let dir = temp_dir("deflayout");
    let b = sample_pdf(&dir, "b.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![b]);
    press(&mut h, mods(true, false, false), Key::O);
    h.run_steps(4);
    assert_eq!(st(&h).doc().unwrap().name, "b.pdf");
    assert_eq!(view(&h).mode, PageMode::Continuous, "opened in Continuous");
    assert_eq!(view(&h).fit, Fit::Width, "opened at Fit Width");
    for _ in 0..30 {
        press(&mut h, mods(false, false, false), Key::Plus);
    }
    assert!(view(&h).zoom <= 2.0 + 1e-3, "zoom capped at 200%: {}", view(&h).zoom);
}

/// ui-145: Document misc options.
#[test]
fn ui_document_misc_options() {
    let _serial = serial();
    let mut h = app();
    prefs_on(&mut h, "General");
    for o in ["Reorder bookmarks when pages move", "Detect web addresses"] {
        assert!(has(&h, o), "{o}");
    }
    prefs_on(&mut h, "Document");
    for o in [
        "Reopen each file at its last page",
        "Offer a read-only copy",
        "Rotate Pages acts on every page",
        "Links to a slip-sheeted page go to the new sheet",
    ] {
        assert!(has(&h, o), "{o}");
    }
    // ui-145: without redirection, links to a slip-sheeted page are removed with the old sheet.
    let dir = temp_dir("slip-links");
    let mut s = markupcraft_engine::Session::from_bytes(
        synthetic::pdf(&[
            synthetic::SyntheticPage::new(612.0, 792.0, String::new()),
            synthetic::SyntheticPage::new(612.0, 792.0, String::new()),
        ]),
        dir.join("set.pdf"),
    )
    .unwrap();
    s.add_link(
        0,
        markupcraft_geom::Rect::new(10.0, 10.0, 60.0, 30.0),
        &markupcraft_engine::links::LinkTarget::Page(1),
        Default::default(),
    )
    .unwrap();
    let rep = markupcraft_engine::batch::SlipSheetReport {
        matched: vec![markupcraft_engine::batch::SlipPair {
            old_page: 1,
            new_page: 0,
            label: "A-102".into(),
            markups: 0,
        }],
        ..Default::default()
    };
    assert_eq!(
        markupcraft_ui_egui::shell::extra2::drop_slip_links(&mut s, &rep).unwrap(),
        1
    );
    assert!(s.links().is_empty(), "the link went with the superseded sheet");
}

fn wheel(h: &mut H, at: egui::Pos2, dy: f32, m: Modifiers) {
    h.hover_at(at);
    h.event(egui::Event::ModifiersChanged(m));
    h.step();
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        modifiers: m,
    });
    h.run_steps(3);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

/// ui-146 / ui-147 / ui-175: the wheel zooms or scrolls per page mode; Ctrl swaps; reversing
/// flips the zoom direction.
#[test]
fn ui_wheel_zoom_or_scroll() {
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(true, false, false), Key::Num4);
    let at = screen(&h, 600.0, 400.0);
    let z = view(&h).zoom;
    wheel(&mut h, at, 1.0, Modifiers::NONE);
    assert!(
        view(&h).zoom > z * 1.02,
        "single page: the wheel zooms in ({z} -> {})",
        view(&h).zoom
    );
    let z = view(&h).zoom;
    let off = view(&h).offset;
    wheel(&mut h, at, -1.0, mods(true, false, false));
    assert!(
        (view(&h).zoom - z).abs() < 1e-4 && view(&h).offset != off,
        "Ctrl+wheel scrolls instead"
    );
    h.state_mut().state.shell.ui.wheel_zooms_single = false;
    h.run_steps(2);
    let z = view(&h).zoom;
    wheel(&mut h, at, -1.0, Modifiers::NONE);
    assert!(
        (view(&h).zoom - z).abs() < 1e-4,
        "set to Scroll: the wheel no longer zooms"
    );
    let z = view(&h).zoom;
    wheel(&mut h, at, 1.0, mods(true, false, false));
    assert!(view(&h).zoom > z * 1.02, "and Ctrl+wheel zooms");
    h.state_mut().state.shell.ui.wheel_zooms_single = true;
    h.state_mut().state.shell.ui.reverse_wheel = true;
    h.run_steps(2);
    let z = view(&h).zoom;
    wheel(&mut h, at, 1.0, Modifiers::NONE);
    assert!(view(&h).zoom < z, "reversed: wheel up zooms out");
    prefs_on(&mut h, "Navigation");
    assert!(has(&h, "Reverse zoom direction"));
    assert!(has(&h, "Tilt wheel pans sideways"), "ui-147 the tilt wheel option");
    // ui-147: the tilt wheel pans sideways, unless turned off.
    h.state_mut().state.shell.show_prefs = false;
    h.state_mut().state.shell.ui.wheel_zooms_single = false;
    h.run_steps(2);
    for _ in 0..4 {
        press(&mut h, mods(false, false, false), Key::Plus);
    }
    let tilt = |h: &mut H| {
        h.hover_at(at);
        h.step();
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(-1.0, 0.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(3);
    };
    let x = view(&h).offset.x;
    tilt(&mut h);
    assert!((view(&h).offset.x - x).abs() > 1.0, "tilt pans sideways");
    h.state_mut().state.shell.ui.extra2.tilt_pans = false;
    h.run_steps(2);
    let x = view(&h).offset.x;
    tilt(&mut h);
    assert!((view(&h).offset.x - x).abs() < 0.01, "tilt off: nothing moves");
}

/// ui-148 / ui-149: scrollbars, vertical on the left, lock panning in Fit Width; default sync;
/// Alt menus.
#[test]
fn ui_navigation_scrollbars_sync_accelerators() {
    let _serial = serial();
    let mut h = app();
    prefs_on(&mut h, "Navigation");
    for o in [
        "Show scrollbars on the document",
        "Vertical scrollbar on the left",
        "Lock panning to up and down in Fit Width",
        "Alt + a menu's first letter opens the menu",
    ] {
        assert!(has(&h, o), "{o}");
    }
    let at = h.query_all_by_label("Page").last().unwrap().rect().center();
    h.hover_at(at);
    h.step();
    button(&mut h, at, true, Modifiers::NONE);
    button(&mut h, at, false, Modifiers::NONE);
    h.run_steps(3);
    assert_eq!(st(&h).shell.ui.extra.sync_default, "page", "default sync mode");
    h.state_mut().state.shell.show_prefs = false;
    h.run_steps(2);
    press(&mut h, mods(true, false, false), Key::Num2);
    assert_eq!(
        st(&h).shell.split.as_ref().map(|s| s.sync),
        Some(markupcraft_ui_egui::shell::split::Sync::Page),
        "new splits follow the default sync"
    );
}

/// ui-150: Grid & Snap page.
#[test]
fn ui_grid_and_snap_page() {
    let _serial = serial();
    let mut h = app();
    prefs_on(&mut h, "Grid & Snap");
    for o in [
        "Units",
        "Grid spacing",
        "Snap to grid",
        "Snap to content",
        "Snap to markup",
        "Endpoints",
        "Snap sensitivity",
        "Snap indicator",
    ] {
        assert!(has(&h, o), "{o}");
    }
    let before = st(&h).shell.prefs.snapping.markup;
    click_label(&mut h, "Snap to markup");
    h.state_mut().state.shell.show_prefs = false;
    h.run_steps(3);
    assert_eq!(st(&h).snaps.markup, !before, "the page drives the snap toggles");
}

/// ui-151: General > Spelling.
#[test]
fn ui_spelling_preferences() {
    let _serial = serial();
    let mut h = app();
    prefs_on(&mut h, "General");
    for o in [
        "Check spelling as you type",
        "Ignore words in capitals",
        "Add to Dictionary",
        "Auto-complete from this list",
        "English (United States)",
    ] {
        assert!(has(&h, o), "{o}");
    }
    // ui-151: the dictionary is a choice; English (United Kingdom) accepts British spellings.
    let dict = markupcraft_engine::spell::dictionary("en_US").unwrap();
    let text = "The colour of the organised centre";
    let us = markupcraft_engine::spell::SpellOptions::default();
    let gb = markupcraft_engine::spell::SpellOptions {
        british: true,
        ..Default::default()
    };
    assert_eq!(markupcraft_engine::spell::check_text(&dict, text, &us).len(), 3);
    assert!(markupcraft_engine::spell::check_text(&dict, text, &gb).is_empty());
    assert_eq!(
        markupcraft_engine::spell::check_text(&dict, "The colour is wrnog", &gb).len(),
        1,
        "real misspellings are still caught"
    );
    h.state_mut().state.shell.ui.extra.spell.british = true;
    h.run_steps(2);
    assert!(has(&h, "English (United Kingdom)"));
}

/// ui-152..ui-169: the remaining pages and their options.
#[test]
fn ui_preference_pages() {
    let _serial = serial();
    let cases: &[(&str, (&'static str, &[&str]))] = &[
        (
            "ui-152",
            (
                "Interface",
                &[
                    "Recent files listed",
                    "Forget files not opened for",
                    "Preview recent files",
                    "Clear Recent Files",
                ],
            ),
        ),
        (
            "ui-153",
            (
                "Markups List",
                &[
                    "Selecting a markup in the list goes to it",
                    "dominant (first) markup only",
                    "Show comments as rich text",
                    "Wrap long comments",
                    "Leave filtered-out markups out of exports",
                    "Dim filtered-out markups on the page",
                ],
            ),
        ),
        (
            "ui-154",
            (
                "Layers",
                &["child layers", "List only the layers used on the current page"],
            ),
        ),
        ("ui-155", ("Tools", &["Reuse markup tools"])),
        (
            "ui-156",
            (
                "Measure",
                &[
                    "Islands inside a region become cutouts",
                    "Split counts by space",
                    "Detect on the page image",
                    "Edge sensitivity",
                    "Hide markups while filling",
                    "Fill cursor",
                ],
            ),
        ),
        (
            "ui-157",
            (
                "Tools",
                &[
                    "relative to the last segment",
                    "absolute (0 = right)",
                    "width x height",
                    "a radius from the centre",
                ],
            ),
        ),
        (
            "ui-158",
            (
                "Forms",
                &["Highlight form fields", "Single-key shortcuts for the form tools"],
            ),
        ),
        ("ui-160", ("Tablet", &["The eraser's size follows the zoom"])),
        (
            "ui-161",
            (
                "Window",
                &[
                    "Loop back to the first page after the last",
                    "Transition",
                    "Fade",
                    "Background",
                    "Hide the mouse pointer",
                ],
            ),
        ),
        (
            "ui-162",
            (
                "WebTab",
                &[
                    "Switch to a new Web Tab",
                    "Open in the browser",
                    "Capture as PDF",
                    "Open captured pages in a split view",
                ],
            ),
        ),
        (
            "ui-163",
            ("Sets", &["Open a sheet in place", "Show only the latest revision"]),
        ),
        (
            "ui-166",
            ("Advanced", &["Disable line weights", "Full-screen crosshair"]),
        ),
        ("ui-168", ("Advanced", &["Open PDF/A documents locked"])),
        (
            "ui-169",
            (
                "Admin",
                &["Back Up Settings...", "Restore Settings...", "Reset All Settings"],
            ),
        ),
        ("ui-172", ("Integrations", &["Add Service"])),
        (
            "ui-159",
            (
                "Signature",
                &[
                    "Digital ID folder",
                    "Remember the digital ID password for",
                    "Block changes that would invalidate signatures",
                ],
            ),
        ),
        ("ui-165", ("Import/Export", &[])),
    ];
    run_cases(cases, |&(page, opts)| {
        let mut h = app();
        prefs_on(&mut h, page);
        for o in opts {
            assert!(has(&h, o), "{page} has {o}");
        }
    });
}

/// ui-158: with single-key form shortcuts off, X no longer adds a signature field.
#[test]
fn ui_forms_single_key_shortcuts() {
    let _serial = serial();
    let mut h = app();
    prefs_on(&mut h, "Forms");
    click_contains(&mut h, "Single-key shortcuts for the form tools");
    h.state_mut().state.shell.show_prefs = false;
    h.run_steps(2);
    let before = (st(&h).tool, st(&h).features.pick.is_some());
    press(&mut h, mods(false, false, false), Key::X);
    assert_eq!((st(&h).tool, st(&h).features.pick.is_some()), before, "X does nothing");
}

/// ui-161 / ui-120: presentation: arrows advance, it loops when asked, Esc leaves.
#[test]
fn ui_presentation_loop() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    h.state_mut().state.shell.ui.presentation_loop = true;
    press(&mut h, mods(true, false, false), Key::Enter);
    press(&mut h, mods(false, false, false), Key::ArrowRight);
    assert_eq!(view(&h).current, 1);
    press(&mut h, mods(false, false, false), Key::ArrowRight);
    assert_eq!(view(&h).current, 0, "loops back to the first page");
    // ui-161: the background colour around the page, a transition, the hidden pointer.
    h.state_mut().state.shell.ui.extra2.presentation_background = [10, 20, 30];
    h.state_mut().state.shell.ui.extra2.transition = "fade".into();
    h.run_steps(2);
    assert_eq!(
        view(&h).opts.background,
        Some(egui::Color32::from_rgb(10, 20, 30)),
        "the page sits on the presentation background"
    );
    press(&mut h, mods(false, false, false), Key::ArrowRight);
    let shown = st(&h).shell.extra2.shown_page;
    assert!(
        shown.is_some_and(|(_, p, _)| p == 1),
        "the transition tracks the page: {shown:?}"
    );
    press(&mut h, mods(false, false, false), Key::Escape);
    h.run_steps(2);
    assert_eq!(view(&h).opts.background, None, "the workspace colour again");
}

/// ui-170: back up all settings, change them, restore.
#[test]
fn ui_settings_backup_restore() {
    let _serial = serial();
    let dir = temp_dir("backup");
    let mut h = app_with_store(&dir);
    h.state_mut().state.shell.prefs.author = "Before".into();
    h.state_mut().state.shell.save_prefs();
    let out = dir.join("backup.json");
    prefs_on(&mut h, "Admin");
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    click_label(&mut h, "Back Up Settings...");
    h.run_steps(3);
    assert!(out.exists(), "backed up");
    h.state_mut().state.shell.prefs.author = "After".into();
    h.state_mut().state.shell.save_prefs();
    click_label(&mut h, "Restore Settings...");
    h.run_steps(4);
    assert_eq!(st(&h).shell.prefs.author, "Before", "restored");
}

/// ui-171: Admin offers the MCP connection for AI assistants (copied to the clipboard).
#[test]
fn ui_mcp_connector() {
    let _serial = serial();
    let mut h = app();
    prefs_on(&mut h, "Admin");
    h.query_all_by_label("Copy MCP Configuration").next().unwrap().click();
    h.step();
    let clip = clipboard_text(&h).or_else(|| {
        h.step();
        clipboard_text(&h)
    });
    assert!(
        clip.is_some_and(|c| c.to_lowercase().contains("mcp")),
        "an MCP configuration was copied"
    );
}

// ---- mouse, gestures and modifier keys -------------------------------------------------------

fn pointer_button(h: &mut H, at: egui::Pos2, b: egui::PointerButton, pressed: bool, m: Modifiers) {
    h.event(egui::Event::PointerButton {
        pos: at,
        button: b,
        pressed,
        modifiers: m,
    });
    h.step();
}

/// Drag with any button between two screen points.
fn drag_button(h: &mut H, a: egui::Pos2, b: egui::Pos2, button: egui::PointerButton, m: Modifiers) {
    h.event(egui::Event::ModifiersChanged(m));
    h.hover_at(a);
    h.step();
    pointer_button(h, a, button, true, m);
    for i in 1..=8 {
        let t = i as f32 / 8.0;
        h.hover_at(a + (b - a) * t);
        h.step();
    }
    pointer_button(h, b, button, false, m);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
}

/// Drag with any button between two page points.
fn drag_page(h: &mut H, a: (f64, f64), b: (f64, f64), button: egui::PointerButton, m: Modifiers) {
    let (a, b) = (screen(h, a.0, a.1), screen(h, b.0, b.1));
    drag_button(h, a, b, button, m);
}

fn bbox(pts: &[Point]) -> (f64, f64, f64, f64) {
    let xs = pts.iter().map(|p| p.x);
    let ys = pts.iter().map(|p| p.y);
    (
        xs.clone().fold(f64::MAX, f64::min),
        ys.clone().fold(f64::MAX, f64::min),
        xs.fold(f64::MIN, f64::max),
        ys.fold(f64::MIN, f64::max),
    )
}

/// ui-173 / ui-174: middle-drag pans in any tool; a middle double-click re-centres.
#[test]
fn ui_middle_button_pan_and_recenter() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(false, false, false), Key::R);
    press(&mut h, mods(false, false, false), Key::Plus);
    press(&mut h, mods(false, false, false), Key::Plus);
    let off = view(&h).offset;
    let n = markups(&h).len();
    drag_page(
        &mut h,
        (500.0, 400.0),
        (400.0, 300.0),
        egui::PointerButton::Middle,
        Modifiers::NONE,
    );
    assert_ne!(view(&h).offset, off, "middle-drag panned");
    assert_eq!(markups(&h).len(), n, "and drew nothing");
    assert_eq!(st(&h).tool, "rectangle", "the tool stays");
    // double-click the middle button on a point: it moves to the centre
    let at = screen(&h, 450.0, 350.0);
    for _ in 0..2 {
        pointer_button(&mut h, at, egui::PointerButton::Middle, true, Modifiers::NONE);
        pointer_button(&mut h, at, egui::PointerButton::Middle, false, Modifiers::NONE);
    }
    h.run_steps(3);
    let c = st(&h).shell.extra.doc_area.unwrap().center();
    let now = screen(&h, 450.0, 350.0);
    assert!(
        now.distance(c) < now.distance(at).max(1.0) + 40.0 && now.distance(c) < 60.0,
        "re-centred: {now:?} vs centre {c:?}"
    );
}

/// ui-176: holding Space pans in the middle of a drawing; the drawing continues after.
#[test]
fn ui_space_temporary_pan() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    press(&mut h, mods(true, false, false), Key::Num8);
    press(&mut h, mods(false, true, false), Key::N);
    click(&mut h, 300.0, 300.0);
    click(&mut h, 400.0, 300.0);
    let off = view(&h).offset;
    h.event(egui::Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.step();
    drag_page(
        &mut h,
        (400.0, 400.0),
        (330.0, 360.0),
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    h.event(egui::Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert_ne!(view(&h).offset, off, "Space+drag panned");
    assert!(view(&h).draft.is_some(), "the polyline in progress survived");
    click(&mut h, 400.0, 400.0);
    press(&mut h, mods(false, false, false), Key::Enter);
    let m = markups(&h).last().unwrap().clone();
    assert_eq!(m.kind, Kind::Polyline);
    assert_eq!(m.pts.len(), 3, "three points: {:?}", m.pts);
}

/// ui-177: right-click a markup or the page for its commands.
#[test]
fn ui_right_click_context_menus() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    let before = labels(&h);
    let p = screen(&h, 950.0, 650.0);
    right_click_at(&mut h, p);
    let menu: Vec<String> = labels(&h).into_iter().filter(|l| !before.contains(l)).collect();
    for item in ["Cut", "Copy", "Delete", "Lock"] {
        assert!(
            menu.iter().any(|l| l.starts_with(item)),
            "markup menu has {item}: {menu:?}"
        );
    }
    let mut h = app();
    let before = labels(&h);
    let p = screen(&h, 400.0, 760.0);
    right_click_at(&mut h, p);
    let menu: Vec<String> = labels(&h).into_iter().filter(|l| !before.contains(l)).collect();
    assert!(!menu.is_empty(), "the page has a menu");
}

/// ui-178 / ui-179: right-drag and Shift+drag draw a selection box; Shift+click adds.
#[test]
fn ui_box_and_shift_select() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let box_ids = |h: &H| {
        let mut s = selection(h);
        s.sort();
        s
    };
    let mut h = app();
    drag_page(
        &mut h,
        (680.0, 720.0),
        (1020.0, 540.0),
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
    let s = box_ids(&h);
    assert!(
        s.contains(&SQUARE.to_string()) && s.contains(&CIRCLE.to_string()),
        "ui-178 right-drag selected: {s:?}"
    );
    let mut h = app();
    drag_page(
        &mut h,
        (680.0, 720.0),
        (1020.0, 540.0),
        egui::PointerButton::Primary,
        mods(false, true, false),
    );
    let s = box_ids(&h);
    assert!(
        s.contains(&SQUARE.to_string()) && s.contains(&CIRCLE.to_string()),
        "ui-179 Shift+drag selected: {s:?}"
    );
    let mut h = app();
    click(&mut h, 950.0, 650.0);
    assert_eq!(selection(&h), vec![SQUARE.to_string()]);
    click_mods(&mut h, 780.0, 620.0, mods(false, true, false));
    let s = box_ids(&h);
    assert_eq!(s.len(), 2, "Shift+click added: {s:?}");
}

/// ui-180 / ui-181: Shift constrains lines to 0/45/90 degrees, rectangles to squares and
/// ellipses to circles.
#[test]
fn ui_shift_constrains_drawing() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    let (a, b) = draw_line(&mut h, (200.0, 500.0), (400.0, 520.0), mods(false, true, false));
    assert!((a.y - b.y).abs() < 1e-6, "horizontal: {a:?} {b:?}");
    let (a, b) = draw_line(&mut h, (200.0, 500.0), (300.0, 590.0), mods(false, true, false));
    assert!(
        ((b.x - a.x).abs() - (b.y - a.y).abs()).abs() < 1e-6,
        "45 degrees: {a:?} {b:?}"
    );
    for (k, kind) in [(Key::R, Kind::Rectangle), (Key::E, Kind::Ellipse)] {
        press(&mut h, mods(false, false, false), k);
        drag_mods(&mut h, (200.0, 400.0), (320.0, 460.0), mods(false, true, false));
        let m = markups(&h).last().unwrap().clone();
        assert_eq!(m.kind, kind);
        let (x0, y0, x1, y1) = bbox(&m.pts);
        let (w, hh) = if m.pts.is_empty() {
            (m.rect.width(), m.rect.height())
        } else {
            (x1 - x0, y1 - y0)
        };
        assert!((w - hh).abs() < 0.5, "{kind:?} square: {w} x {hh}");
    }
}

/// ui-182: Alt draws an ellipse from its centre.
#[test]
fn ui_alt_draws_from_center() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    press(&mut h, mods(false, false, false), Key::E);
    drag_mods(&mut h, (300.0, 400.0), (350.0, 430.0), mods(false, false, true));
    let m = markups(&h).last().unwrap().clone();
    assert_eq!(m.kind, Kind::Ellipse);
    let r = if m.pts.is_empty() {
        m.rect.normalized()
    } else {
        let (a, b, c, d) = bbox(&m.pts);
        markupcraft_geom::Rect::new(a, b, c, d)
    };
    assert!(
        ((r.x0 + r.x1) / 2.0 - 300.0).abs() < 1.5 && ((r.y0 + r.y1) / 2.0 - 400.0).abs() < 1.5,
        "centred on the press: {r:?}"
    );
    assert!((r.width() - 100.0).abs() < 1.5, "twice the drag: {r:?}");
}

/// ui-182: Alt+click builds a three-point arc in a polyline.
#[test]
fn ui_alt_click_three_point_arc() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    press(&mut h, mods(false, true, false), Key::N);
    click(&mut h, 200.0, 400.0);
    click_mods(&mut h, 250.0, 450.0, mods(false, false, true));
    click(&mut h, 300.0, 400.0);
    press(&mut h, mods(false, false, false), Key::Enter);
    let m = markups(&h).last().unwrap().clone();
    assert!(!m.arcs.is_empty(), "the polyline has an arc: {:?}", m.pts);
}

/// ui-183 / ui-184: Shift-drag moves on an axis; Ctrl-drag copies; Ctrl+Shift copies on an axis.
#[test]
fn ui_move_and_copy_drags() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    click(&mut h, 950.0, 650.0);
    let r0 = rect_of(&h, SQUARE);
    drag_mods(&mut h, (950.0, 650.0), (1000.0, 665.0), mods(false, true, false));
    let r1 = rect_of(&h, SQUARE);
    assert!(
        (r1.x0 - r0.x0 - 50.0).abs() < 0.5 && (r1.y0 - r0.y0).abs() < 1e-6,
        "moved on x only: {r0:?} -> {r1:?}"
    );
    let n = markups(&h).len();
    drag_mods(&mut h, (1000.0, 650.0), (1040.0, 600.0), mods(true, false, false));
    assert_eq!(markups(&h).len(), n + 1, "Ctrl-drag copied");
    assert_eq!(rect_of(&h, SQUARE), r1, "the original stayed");
    let copy = markups(&h).last().unwrap().rect.normalized();
    assert!(
        (copy.x0 - r1.x0 - 40.0).abs() < 0.5 && (copy.y0 - r1.y0 + 50.0).abs() < 0.5,
        "copy where dropped: {copy:?}"
    );
    click(&mut h, 1020.0, 690.0);
    assert_eq!(selection(&h), vec![SQUARE.to_string()]);
    drag_mods(&mut h, (1020.0, 690.0), (1080.0, 680.0), mods(true, true, false));
    assert_eq!(markups(&h).len(), n + 2, "Ctrl+Shift-drag copied");
    let copy = markups(&h).last().unwrap().rect.normalized();
    assert!((copy.y0 - r1.y0).abs() < 1e-6, "on an axis: {copy:?}");
}

/// ui-186 / ui-187: Shift-click a segment adds a vertex, a vertex removes it; Ctrl-click
/// turns a vertex into a curve.
#[test]
fn ui_vertex_editing() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    click(&mut h, 760.0, 325.0); // on the first segment of the polyline (700,240)-(820,410)
    assert_eq!(selection(&h), vec![POLYLINE.to_string()]);
    let n = mk(&h, POLYLINE).pts.len();
    click_mods(&mut h, 760.0, 325.0, mods(false, true, false));
    assert_eq!(
        mk(&h, POLYLINE).pts.len(),
        n + 1,
        "ui-186 Shift-click on a segment adds a vertex"
    );
    click_mods(&mut h, 900.0, 260.0, mods(false, true, false));
    assert_eq!(
        mk(&h, POLYLINE).pts.len(),
        n,
        "ui-186 Shift-click on a vertex removes it"
    );
    let arcs = mk(&h, POLYLINE).arcs.len();
    click_mods(&mut h, 820.0, 410.0, mods(true, false, false));
    assert_ne!(mk(&h, POLYLINE).arcs.len(), arcs, "ui-187 Ctrl-click curved the vertex");
}

/// ui-188: the rotation handle turns by 15 degrees; Shift frees it to 1 degree.
#[test]
fn ui_rotation_snap() {
    let _serial = serial();
    let angle = |h: &H| {
        let p = mk(h, POLYLINE).pts;
        (p[1].y - p[0].y).atan2(p[1].x - p[0].x).to_degrees()
    };
    for (m, step) in [(Modifiers::NONE, 15.0), (mods(false, true, false), 1.0)] {
        let mut h = app();
        snaps_off(&mut h);
        click(&mut h, 760.0, 325.0);
        let a0 = angle(&h);
        // the handle: above the box's top centre
        let top = screen(&h, 850.0, 410.0) - egui::vec2(0.0, 22.0);
        let pivot = screen(&h, 850.0, 325.0);
        let v = top - pivot;
        let t = 20f32.to_radians();
        let to = pivot + egui::vec2(v.x * t.cos() - v.y * t.sin(), v.x * t.sin() + v.y * t.cos());
        drag_button(&mut h, top, to, egui::PointerButton::Primary, m);
        let mut d = angle(&h) - a0;
        while d > 180.0 {
            d -= 360.0;
        }
        while d < -180.0 {
            d += 360.0;
        }
        assert!(d.abs() > 0.5, "it turned");
        assert!(
            (d / step - (d / step).round()).abs() < 0.02,
            "a multiple of {step}: {d}"
        );
        if step == 1.0 {
            assert!((d.abs() - 15.0).abs() > 0.5, "Shift: not snapped to 15: {d}");
        }
    }
}

/// ui-189: Shift-drag a measurement's caption moves it alone.
#[test]
fn ui_shift_drag_measurement_caption() {
    let _serial = serial();
    let area = "SAMPLEAREAAAAAAA";
    let mut h = app();
    snaps_off(&mut h);
    click(&mut h, 450.0, 265.0);
    assert_eq!(selection(&h), vec![area.to_string()]);
    let pts = mk(&h, area).pts;
    drag_mods(&mut h, (450.0, 265.0), (480.0, 300.0), mods(false, true, false));
    assert_eq!(mk(&h, area).pts, pts, "the area did not move");
    assert!(mk(&h, area).caption_offset.is_some(), "the caption moved");
}

/// ui-190: corner resize keeps a stamp's aspect; Shift breaks it.
#[test]
fn ui_shift_breaks_aspect_ratio() {
    let _serial = serial();
    for shift in [false, true] {
        let mut h = app();
        snaps_off(&mut h);
        press(&mut h, mods(false, false, false), Key::S);
        click(&mut h, 300.0, 500.0);
        press(&mut h, mods(false, false, false), Key::V);
        let m = markups(&h).last().unwrap().clone();
        assert_eq!(m.kind, Kind::Stamp, "a stamp was placed");
        let (x0, y0, x1, y1) = bbox(&m.pts);
        click(&mut h, (x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let ratio = (x1 - x0) / (y1 - y0);
        drag_mods(&mut h, (x1, y1), (x1 + 60.0, y1 + 10.0), mods(false, shift, false));
        let after = markups(&h).into_iter().find(|x| x.id == m.id).unwrap();
        let (a, b, c, d) = bbox(&after.pts);
        let r = (c - a) / (d - b);
        assert!((c - a) > (x1 - x0) + 1.0, "resized: {:?} -> {:?}", m.pts, after.pts);
        if shift {
            assert!((r - ratio).abs() > 0.05, "Shift: aspect free {ratio} -> {r}");
        } else {
            assert!((r - ratio).abs() < 0.02, "aspect kept {ratio} -> {r}");
        }
    }
    // Polylines, polygons and clouds: the corner of the selection box scales the whole shape in
    // proportion; Shift stretches it.
    for kind in [Kind::Polygon, Kind::Polyline, Kind::Cloud] {
        for shift in [false, true] {
            let mut h = app();
            snaps_off(&mut h);
            press(&mut h, mods(false, false, false), Key::V);
            let diamond = vec![
                Point::new(300.0, 450.0),
                Point::new(360.0, 500.0),
                Point::new(300.0, 550.0),
                Point::new(240.0, 500.0),
            ];
            let id = {
                let d = h.state_mut().state.doc_mut().unwrap();
                let m = markupcraft_model::Markup::new(kind, 0, diamond.clone());
                let id = d.session.add_new_markups("Add", vec![m]).unwrap().remove(0);
                d.session.select(std::slice::from_ref(&id)).unwrap();
                id
            };
            h.run_steps(3);
            let (x0, y0, x1, y1) = bbox(&diamond);
            let ratio = (x1 - x0) / (y1 - y0);
            drag_mods(&mut h, (x1, y1), (x1 + 60.0, y1 + 10.0), mods(false, shift, false));
            let after = markups(&h).into_iter().find(|x| x.id == id).unwrap();
            assert_eq!(after.pts.len(), 4, "{kind:?}: still four vertices");
            let (a, b, c, d) = bbox(&after.pts);
            let r = (c - a) / (d - b);
            assert!((c - a) > (x1 - x0) + 1.0, "{kind:?} resized: {:?}", after.pts);
            assert!(
                (a - x0).abs() < 0.5 && (b - y0).abs() < 0.5,
                "the opposite corner stays"
            );
            if shift {
                assert!((r - ratio).abs() > 0.05, "{kind:?} Shift: aspect free {ratio} -> {r}");
            } else {
                assert!((r - ratio).abs() < 0.02, "{kind:?} aspect kept {ratio} -> {r}");
            }
        }
    }
}

/// ui-191: Alt-drag a callout's box moves the whole callout, arrow tip included.
#[test]
fn ui_alt_drag_callout() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    press(&mut h, mods(false, false, false), Key::Q);
    click(&mut h, 200.0, 500.0); // tip
    click(&mut h, 300.0, 600.0); // box
    h.event(egui::Event::Text("Check".into()));
    h.run_steps(2);
    press(&mut h, mods(false, false, false), Key::V);
    click(&mut h, 600.0, 100.0); // elsewhere: the text is done
    let m = markups(&h).last().unwrap().clone();
    assert_eq!(m.kind, Kind::Callout, "a callout was drawn");
    press(&mut h, mods(false, false, false), Key::V);
    let (x0, y0, x1, y1) = bbox(&m.pts[..4.min(m.pts.len())]);
    let c = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    click(&mut h, c.0, c.1);
    drag_mods(&mut h, c, (c.0 + 40.0, c.1 + 20.0), mods(false, false, true));
    let after = markups(&h).into_iter().find(|x| x.id == m.id).unwrap();
    for (p, q) in m.pts.iter().zip(after.pts.iter()) {
        assert!(
            (q.x - p.x - 40.0).abs() < 0.5 && (q.y - p.y - 20.0).abs() < 0.5,
            "every point moved: {p:?} -> {q:?}"
        );
    }
}

/// ui-192: Ctrl+click a File Access entry opens it in the background.
#[test]
fn ui_ctrl_click_opens_in_background() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let dir = temp_dir("bg");
    let other = sample_pdf(&dir, "background-one.pdf");
    let mut h = app();
    h.state_mut().state.dialogs.scripted = Some(vec![other]);
    press(&mut h, mods(true, false, false), Key::O);
    h.run_steps(3);
    press(&mut h, mods(true, false, false), Key::F4);
    h.state_mut().state.show_panel("file_access");
    h.run_steps(4);
    let at = h
        .query_all_by_label("background-one.pdf")
        .map(|n| n.rect())
        .find(|r| r.left() < 300.0)
        .expect("the File Access entry")
        .center();
    let active = st(&h).doc().unwrap().name.clone();
    h.event(egui::Event::ModifiersChanged(mods(true, false, false)));
    h.hover_at(at);
    h.step();
    button(&mut h, at, true, mods(true, false, false));
    button(&mut h, at, false, mods(true, false, false));
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
    assert!(st(&h).docs.iter().any(|d| d.name == "background-one.pdf"), "opened");
    assert_eq!(st(&h).doc().unwrap().name, active, "in the background");
}

/// ui-193 / ui-194: Esc cancels a drawing in progress; Enter or a double-click finishes one.
#[test]
fn ui_esc_enter_double_click() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    let n = markups(&h).len();
    press(&mut h, mods(false, true, false), Key::N);
    click(&mut h, 200.0, 400.0);
    click(&mut h, 300.0, 400.0);
    assert!(view(&h).draft.is_some());
    press(&mut h, mods(false, false, false), Key::Escape);
    assert!(view(&h).draft.is_none(), "Esc cancelled the polyline");
    assert_eq!(markups(&h).len(), n, "nothing added");
    press(&mut h, mods(false, true, false), Key::P);
    for (x, y) in [(200.0, 400.0), (300.0, 400.0), (300.0, 480.0)] {
        click(&mut h, x, y);
    }
    press(&mut h, mods(false, false, false), Key::Enter);
    assert_eq!(markups(&h).len(), n + 1, "Enter finished the polygon");
    assert_eq!(markups(&h).last().unwrap().kind, Kind::Polygon);
    press(&mut h, mods(false, true, false), Key::N);
    click(&mut h, 200.0, 300.0);
    click(&mut h, 300.0, 300.0);
    let at = screen(&h, 350.0, 250.0);
    h.hover_at(at);
    h.step();
    for _ in 0..2 {
        button(&mut h, at, true, Modifiers::NONE);
        button(&mut h, at, false, Modifiers::NONE);
    }
    h.run_steps(3);
    assert_eq!(markups(&h).len(), n + 2, "a double-click finished the polyline");
}

/// ui-195: Reuse keeps the tool after placing a markup (else back to Select).
#[test]
fn ui_reuse_markup_tool() {
    let _serial = serial();
    let mut h = app();
    snaps_off(&mut h);
    h.state_mut().state.tool_locked = false;
    press(&mut h, mods(false, false, false), Key::R);
    drag(&mut h, (200.0, 400.0), (260.0, 450.0));
    assert_eq!(st(&h).tool, "select", "back to Select");
    click_label(&mut h, "Reuse");
    press(&mut h, mods(false, false, false), Key::R);
    drag(&mut h, (200.0, 500.0), (260.0, 550.0));
    assert_eq!(st(&h).tool, "rectangle", "Reuse keeps the tool");
    prefs_on(&mut h, "Tools");
    assert!(has(&h, "Reuse markup tools"), "and it is a preference");
}

/// ui-226: Batch > Run Script runs a script of commands over files.
#[test]
fn ui_scripting() {
    // The app keeps snap and preference state process-wide: tests take turns.
    let _serial = serial();
    let mut h = app();
    open_menu(&mut h, "Batch");
    click_label(&mut h, "Run Script...");
    h.run_steps(3);
    assert!(has(&h, "Script"), "the script window is open");
    // A script of commands over files (the same runner the window and the CLI use).
    let dir = temp_dir("script");
    sample_pdf(&dir, "plan.pdf");
    std::fs::write(
        dir.join("go.json"),
        r#"[{"tool": "watermark_add", "params": {"text": "SCRIPTED"}}]"#,
    )
    .unwrap();
    let mut a = automation(&dir);
    let v = call(
        &mut a,
        "batch_script",
        json!({ "files": ["plan.pdf"], "script": "go.json" }),
    );
    assert_eq!(v["done"], 1, "{v}");
    call(&mut a, "doc_open", json!({ "path": "plan.pdf" }));
    let t = call(&mut a, "page_text", json!({ "page": 1 }));
    assert!(t.to_string().contains("SCRIPTED"), "the script ran");
}

/// ui-153, ui-156, ui-159, ui-162: what the new Preferences options do.
#[test]
fn ui_preference_options_take_effect() {
    let _serial = serial();
    // ui-153: filtered-out markups dim on the page and (by choice) stay in exports.
    let mut h = app();
    {
        let s = &mut h.state_mut().state;
        let d = s.doc_mut().unwrap();
        let sq = vec![
            Point::new(100.0, 100.0),
            Point::new(150.0, 100.0),
            Point::new(150.0, 150.0),
            Point::new(100.0, 150.0),
        ];
        let mut a = markupcraft_model::Markup::new(Kind::Rectangle, 0, sq);
        a.subject = "Keep".into();
        let mut b = a.clone();
        b.subject = "Hide".into();
        d.session.add_new_markups("Add", vec![a, b]).unwrap();
        s.list
            .view
            .filters
            .insert("subject".into(), ["Keep".to_string()].into());
        s.shell.prefs.more.markups_list.dim_filtered_pct = 60;
    }
    h.run_steps(3);
    let out = markupcraft_ui_egui::panels::markups_list::filtered_out(st(&h).doc().unwrap(), &st(&h).list.view);
    let total = markups(&h).len();
    let kept = markups(&h).iter().filter(|m| m.subject == "Keep").count();
    assert_eq!(out.len(), total - kept, "the filtered-out markups dim on their page");
    assert!(!out.is_empty());
    let dir = temp_dir("list-export");
    let csv = dir.join("list.csv");
    let lines = |h: &mut H, exclude: bool| {
        h.state_mut()
            .state
            .shell
            .prefs
            .more
            .markups_list
            .exclude_filtered_from_export = exclude;
        markupcraft_ui_egui::panels::markups_list::export(
            &mut h.state_mut().state,
            &markupcraft_ui_egui::dialogs::Purpose::ExportCsv,
            &csv,
        );
        std::fs::read_to_string(&csv).unwrap()
    };
    let only = lines(&mut h, true);
    let all = lines(&mut h, false);
    assert!(
        !only.contains("Hide") && all.contains("Hide"),
        "exports follow the preference"
    );

    // ui-156: the Dynamic Fill options reach the fill tool.
    {
        let s = &mut h.state_mut().state;
        s.shell.prefs.more.measure.fill_raster = true;
        s.shell.prefs.more.measure.fill_dpi = 200.0;
        s.shell.prefs.more.measure.fill_cursor_px = 30.0;
        markupcraft_ui_egui::prefs_ui::apply_live(s);
    }
    let fm = &st(&h).features.fill.more;
    assert!(fm.raster && fm.dpi == 200.0 && fm.cursor_size == 30.0);

    // ui-159: the digital ID password is forgotten at once unless the preference keeps it.
    h.state_mut().state.features.signatures.password = "secret".into();
    h.run_steps(2);
    assert!(st(&h).features.signatures.password.is_empty(), "forgotten");
    h.state_mut().state.shell.prefs.more.signature.password_minutes = 5;
    h.state_mut().state.features.signatures.password = "secret".into();
    h.run_steps(2);
    assert_eq!(st(&h).features.signatures.password, "secret", "kept for 5 minutes");

    // ui-159: changes that would invalidate signatures are blocked when asked.
    let dir = temp_dir("sig-block");
    std::fs::write(dir.join("p.pdf"), markupcraft_render::synthetic::sample_pdf()).unwrap();
    let mut a = automation(&dir);
    call(
        &mut a,
        "digital_id_create",
        json!({ "name": "Test Signer", "out": "id.p12", "password": "test-only" }),
    );
    call(&mut a, "doc_open", json!({ "path": "p.pdf" }));
    call(
        &mut a,
        "signature_sign",
        json!({ "id": "id.p12", "password": "test-only", "page": 1, "rect": [50, 50, 200, 100], "out": "signed.pdf" }),
    );
    let mut h = app();
    {
        let s = &mut h.state_mut().state;
        let bytes = std::fs::read(dir.join("signed.pdf")).unwrap();
        s.open_bytes("signed.pdf", Some(dir.join("signed.pdf")), bytes).unwrap();
        s.shell.prefs.more.signature.block_breaking_changes = true;
    }
    h.run_steps(3);
    let rot = st(&h).doc().unwrap().session.doc().pages[0].rotate;
    press(&mut h, mods(false, true, true), Key::Plus);
    assert_eq!(
        st(&h).doc().unwrap().session.doc().pages[0].rotate,
        rot,
        "the page edit was refused"
    );
    assert!(st(&h).status.contains("blocked"), "{}", st(&h).status);
    assert!(st(&h).shell.extra.sign_warning.is_none(), "no question asked");

    // ui-162: a captured page opens in a split view beside the document.
    let mut h = app();
    h.state_mut().state.shell.prefs.more.webtab.captures_in_split = true;
    markupcraft_ui_egui::features::more6::web::open_capture(&mut h.state_mut().state, &dir.join("p.pdf"));
    h.run_steps(3);
    let s = st(&h);
    assert_eq!(s.docs.len(), 2);
    let pane = s.shell.split.as_ref().expect("a split view").pane.uid;
    assert_ne!(pane, s.doc().unwrap().uid, "the capture shows beside the document");
}
