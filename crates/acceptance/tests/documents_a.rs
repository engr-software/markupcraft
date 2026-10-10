//! Documents slice A (doc-001 .. doc-110): opening, tabs, navigation, split views, thumbnails,
//! page labels, bookmarks, page operations and creating PDFs, each test written from the Revu
//! inventory text (`docs/revu_features/03_documents.md`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use egui::{Key, Modifiers};
use markupcraft_acceptance::*;
use markupcraft_engine::prefs::PrefStore;
use markupcraft_ui_egui::shell;

const CTRL: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};
const CTRL_SHIFT: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: true,
    mac_cmd: false,
    command: true,
};

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

/// The label of a command's default key (`Ctrl+Shift+S`), if it has one.
/// A key label in Revu's Windows wording (macOS shows Cmd and Option for Ctrl and Alt).
fn windows_label(label: String) -> String {
    label.replace("Cmd+", "Ctrl+").replace("Option+", "Alt+")
}

fn shortcut(id: &str) -> Option<String> {
    markupcraft_ui_egui::commands::find(id)
        .and_then(|c| c.keys)
        .map(|k| windows_label(k.label()))
}

/// The command bound to a key label, if any.
fn bound(label: &str) -> Option<String> {
    markupcraft_ui_egui::commands::bindings()
        .into_iter()
        .find(|(k, _)| windows_label(k.label()) == label)
        .map(|(_, id)| id)
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// The headless app with nothing open.
fn empty_app() -> Harness<'static, MarkupCraftApp> {
    markupcraft_ui_egui::richedit::load_dictionary_blocking();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(|_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            app
        });
    h.run_steps(4);
    h
}

/// The app launched the way the desktop app starts, with its settings in `cfg` instead of the
/// user's folder (preferences, recent files, last session, recovery).
fn launch(cfg: &Path) -> Harness<'static, MarkupCraftApp> {
    markupcraft_ui_egui::richedit::load_dictionary_blocking();
    let cfg = cfg.to_path_buf();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(move |_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            let st = &mut app.state;
            st.shell.recent_path = Some(cfg.join("recent.json"));
            st.shell.recent = shell::recent::RecentStore::load(&cfg.join("recent.json"));
            st.shell.store = Some(PrefStore::new(&cfg));
            shell::files::enable_recovery(st, cfg.join("recovery"));
            markupcraft_ui_egui::prefs_ui::load_from_store(st);
            shell::recent::reopen_last_session(st);
            shell::extra::startup(st);
            app
        });
    h.run_steps(6);
    h
}

fn open(h: &mut Harness<'_, MarkupCraftApp>, p: &Path) {
    h.state_mut().state.open_path(p);
    h.run_steps(4);
}

fn names(h: &Harness<'_, MarkupCraftApp>) -> Vec<String> {
    h.state().state.docs.iter().map(|d| d.name.clone()).collect()
}

fn active_name(h: &Harness<'_, MarkupCraftApp>) -> String {
    h.state().state.doc().map(|d| d.name.clone()).unwrap_or_default()
}

fn page(h: &Harness<'_, MarkupCraftApp>) -> usize {
    h.state().state.doc().unwrap().view.current
}

fn markup_count(a: &mut Automation, path: &str) -> usize {
    let v = call(a, "doc_open", json!({ "path": path }));
    let id = v["doc"].as_u64().unwrap();
    let n = call(a, "markup_list", json!({ "doc": id }))["markups"]
        .as_array()
        .unwrap()
        .len();
    call(a, "doc_close", json!({ "doc": id, "discard_changes": true }));
    n
}

fn page_count(a: &mut Automation, path: &str) -> usize {
    let v = call(a, "doc_open", json!({ "path": path }));
    let id = v["doc"].as_u64().unwrap();
    let n = v["pages"].as_u64().unwrap() as usize;
    call(a, "doc_close", json!({ "doc": id, "discard_changes": true }));
    n
}

/// A PNG made by exporting page 1 of the sample.
fn sample_png(dir: &Path) -> PathBuf {
    let mut a = automation(dir);
    sample_pdf(dir, "img-src.pdf");
    std::fs::create_dir_all(dir.join("png")).unwrap();
    call(&mut a, "doc_open", json!({ "path": "img-src.pdf" }));
    let v = call(
        &mut a,
        "export_images",
        json!({ "dir": "png", "pages": [1], "dpi": 36, "format": "png" }),
    );
    let f = v["files"][0].as_str().unwrap().to_string();
    let p = PathBuf::from(&f);
    if p.is_absolute() { p } else { dir.join(f) }
}

// ---------------------------------------------------------------- opening, tabs, saving

/// D-001 Open: several PDFs each in their own tab. D-004 every file has a tab. D-005 Ctrl+Tab
/// cycles tabs. D-006 Close / Close All (Ctrl+F4 / Ctrl+Shift+W).
#[test]
fn open_several_pdfs_into_tabs_cycle_and_close() {
    let dir = temp_dir("doca-tabs");
    let p1 = sample_pdf(&dir, "first.pdf");
    let p2 = sample_pdf(&dir, "second.pdf");
    let p3 = sample_pdf(&dir, "third.pdf");
    let mut h = empty_app();
    for p in [&p1, &p2, &p3] {
        open(&mut h, p);
    }
    assert_eq!(names(&h), ["first.pdf", "second.pdf", "third.pdf"]);
    for n in ["first.pdf", "second.pdf", "third.pdf"] {
        assert!(shows(&h, n), "tab for {n}");
    }
    // Opening a file that is already open does not add a second tab.
    open(&mut h, &p1);
    assert_eq!(h.state().state.docs.len(), 3);
    let start = h.state().state.active;
    key(&mut h, CTRL, Key::Tab);
    let next = h.state().state.active;
    assert_eq!(next, (start + 1) % 3, "Ctrl+Tab goes to the next document");
    key(&mut h, CTRL_SHIFT, Key::Tab);
    assert_eq!(h.state().state.active, start, "Ctrl+Shift+Tab goes back");
    key(&mut h, CTRL, Key::F4);
    assert_eq!(h.state().state.docs.len(), 2, "Ctrl+F4 closes the active tab");
    key(&mut h, CTRL_SHIFT, Key::W);
    assert_eq!(h.state().state.docs.len(), 0, "Ctrl+Shift+W closes every tab");

    // Tools: two documents open side by side.
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "first.pdf" }));
    call(&mut a, "doc_open", json!({ "path": "second.pdf" }));
    let l = call(&mut a, "doc_list", json!({}));
    assert_eq!(l["documents"].as_array().map_or(0, Vec::len), 2, "{l}");
}

/// D-002 TIF/JPG/PNG/GIF/BMP open as PDF pages.
#[test]
fn images_open_as_pdf_pages() {
    let dir = temp_dir("doca-img");
    let png = sample_png(&dir);
    let mut a = automation(&dir);
    let v = call(
        &mut a,
        "doc_from_image",
        json!({ "image": s(&png), "path": "fromimg.pdf" }),
    );
    assert_eq!(v["pages"].as_u64(), Some(1), "{v}");
    call(&mut a, "doc_save", json!({}));
    assert_eq!(page_count(&mut a, "fromimg.pdf"), 1);
    // The app's File > Open takes the image too.
    let mut h = empty_app();
    open(&mut h, &png);
    assert_eq!(h.state().state.docs.len(), 1, "{}", h.state().state.status);
    let d = h.state().state.doc().unwrap();
    assert_eq!(d.render.as_ref().map(|r| r.pages().len()), Some(1));
}

/// D-003 dropping files on the workspace opens them, each in its own tab.
#[test]
fn dropping_files_on_the_workspace_opens_them() {
    let dir = temp_dir("doca-drop");
    let p1 = sample_pdf(&dir, "dropA.pdf");
    let p2 = sample_pdf(&dir, "dropB.pdf");
    let mut h = empty_app();
    for p in [p1, p2] {
        h.input_mut().dropped_files.push(std::sync::Arc::new(Dropped(p)));
    }
    h.run_steps(4);
    assert_eq!(names(&h), ["dropA.pdf", "dropB.pdf"]);
}

/// D-007 Save writes the PDF; Save As writes a new file. Reopened, the markups are there.
/// D-008 Save All saves every open file that has unsaved changes.
#[test]
fn save_save_as_and_save_all_write_the_files() {
    let dir = temp_dir("doca-save");
    sample_pdf(&dir, "a.pdf");
    sample_pdf(&dir, "b.pdf");
    let mut a = automation(&dir);
    let before = markup_count(&mut a, "a.pdf");
    call(&mut a, "doc_open", json!({ "path": "a.pdf" }));
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[100, 100], [200, 200]] }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_save", json!({ "path": "a-copy.pdf" }));
    assert_eq!(markup_count(&mut a, "a.pdf"), before + 1);
    assert_eq!(markup_count(&mut a, "a-copy.pdf"), before + 1);

    // Save All in the app: both edited files are written.
    let mut h = empty_app();
    for n in ["a-copy.pdf", "b.pdf"] {
        open(&mut h, &dir.join(n));
        h.state_mut().state.doc_mut().unwrap().session.select_all(None);
        run(&mut h, "edit.delete");
    }
    run(&mut h, "file.save_all");
    h.run_steps(4);
    assert_eq!(markup_count(&mut a, "a-copy.pdf"), 0, "{}", h.state().state.status);
    assert_eq!(markup_count(&mut a, "b.pdf"), 0, "{}", h.state().state.status);
    // Shift+F2 is Save All too.
    assert_eq!(shortcut("file.save_all").as_deref(), Some("Shift+F2"));
    assert_eq!(shortcut("file.save").as_deref(), Some("Ctrl+S"));
    assert_eq!(shortcut("file.save_as").as_deref(), Some("Ctrl+Shift+S"));
}

/// D-009 long file names in tabs are cut from the start or from the end (preference).
#[test]
fn long_tab_names_are_truncated_from_start_or_end() {
    let dir = temp_dir("doca-trunc");
    let long = "Riverside-Tower-Architectural-Floor-Plans-A101.pdf";
    let p = sample_pdf(&dir, long);
    let mut h = empty_app();
    h.state_mut().state.shell.ui.tab_max_chars = 16;
    h.state_mut().state.shell.ui.tab_truncate_start = true;
    open(&mut h, &p);
    h.run_steps(3);
    assert!(shows(&h, "...lans-A101.pdf"), "cut from the start keeps the end");
    h.state_mut().state.shell.ui.tab_truncate_start = false;
    h.run_steps(3);
    assert!(shows(&h, "Riverside-Tow..."), "cut from the end keeps the start");
}

/// D-010 the files open when the app last closed are reopened at the next launch.
/// D-012 each PDF reopens at the page it had when closed.
#[test]
fn last_session_reopens_and_files_reopen_at_their_last_page() {
    let dir = temp_dir("doca-session");
    let cfg = dir.join("config");
    std::fs::create_dir_all(&cfg).unwrap();
    let p1 = sample_pdf(&dir, "one.pdf");
    let p2 = sample_pdf(&dir, "two.pdf");
    {
        let mut h = launch(&cfg);
        open(&mut h, &p1);
        open(&mut h, &p2);
        run(&mut h, "view.last_page");
        assert_eq!(page(&h), 1);
        h.run_steps(4);
    }
    let mut h = launch(&cfg);
    h.run_steps(4);
    assert_eq!(names(&h), ["one.pdf", "two.pdf"], "last session reopened");
    // two.pdf was on page 2.
    let i = h.state().state.docs.iter().position(|d| d.name == "two.pdf").unwrap();
    assert_eq!(h.state().state.docs[i].view.current, 1, "reopened at its last page");
    // Close and reopen in the same session: same page.
    h.state_mut().state.active = i;
    run(&mut h, "file.close");
    open(&mut h, &p2);
    h.run_steps(4);
    assert_eq!(page(&h), 1);
}

/// D-011 startup options: a PDF to open on start, the startup mode (view hides the markup tools).
#[test]
fn startup_file_and_startup_mode_apply_at_launch() {
    let dir = temp_dir("doca-startup");
    let cfg = dir.join("config");
    std::fs::create_dir_all(&cfg).unwrap();
    let p = sample_pdf(&dir, "start.pdf");
    {
        let mut h = launch(&cfg);
        let ui = &mut h.state_mut().state.shell;
        ui.ui.reopen_last_session = false;
        ui.ui.extra.startup_file = s(&p);
        ui.ui.extra.startup_mode = "view".into();
        ui.save_ui();
    }
    let h = launch(&cfg);
    assert_eq!(names(&h), ["start.pdf"]);
    assert!(
        !h.state().state.shell.ui.toolbars.show_markup,
        "View mode: markup tools hidden"
    );
}

/// D-013 unsaved edits survive a crash: on the next launch they are offered back.
#[test]
fn unsaved_edits_are_recovered_after_a_crash() {
    let dir = temp_dir("doca-recover");
    let cfg = dir.join("config");
    std::fs::create_dir_all(&cfg).unwrap();
    let p = sample_pdf(&dir, "work.pdf");
    let mut a = automation(&dir);
    let before = markup_count(&mut a, "work.pdf");
    {
        let mut h = launch(&cfg);
        open(&mut h, &p);
        h.state_mut().state.doc_mut().unwrap().session.select_all(None);
        run(&mut h, "edit.delete");
        assert!(markups(&h).is_empty());
        assert!(shell::files::autosave_now(&mut h.state_mut().state) >= 1);
        // crash: dropped without saving
    }
    assert_eq!(markup_count(&mut a, "work.pdf"), before, "the file itself is unchanged");
    let mut h = launch(&cfg);
    h.run_steps(4);
    assert!(shows(&h, "Recover") || shows(&h, "recover"), "recovery is offered");
    let found = shell::files::scan(&cfg.join("recovery"));
    assert_eq!(found.len(), 1);
    shell::files::restore(&mut h.state_mut().state, &found[0]);
    h.run_steps(4);
    assert!(markups(&h).is_empty(), "the unsaved edit is back");
}

/// D-014 a file in use elsewhere: offer to open a read-only copy.
#[cfg(windows)]
#[test]
fn a_locked_file_offers_a_read_only_copy() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = temp_dir("doca-locked");
    let p = sample_pdf(&dir, "inuse.pdf");
    // In use elsewhere: open for writing with only reading shared (FILE_SHARE_READ).
    let _lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(1)
        .open(&p)
        .unwrap();
    let mut h = empty_app();
    open(&mut h, &p);
    h.run_steps(3);
    assert!(
        shows(&h, "read-only") || shows(&h, "Read-Only") || shows(&h, "Read-only"),
        "prompt shown"
    );
    shell::files::open_copy(&mut h.state_mut().state, &p);
    h.run_steps(3);
    assert_eq!(h.state().state.docs.len(), 1, "{}", h.state().state.status);
}

/// D-015 File > Open Recent lists recently opened PDFs for one-click reopening.
#[test]
fn open_recent_lists_and_reopens_files() {
    let dir = temp_dir("doca-recent");
    let cfg = dir.join("config");
    std::fs::create_dir_all(&cfg).unwrap();
    let p = sample_pdf(&dir, "recent-one.pdf");
    let mut h = launch(&cfg);
    open(&mut h, &p);
    run(&mut h, "file.close");
    assert!(h.state().state.docs.is_empty());
    let r = h.state().state.shell.recent.find(&p).cloned();
    assert!(r.is_some(), "listed in Open Recent");
    // A new launch still lists it (kept on disk).
    drop(h);
    let mut h = launch(&cfg);
    assert!(h.state().state.shell.recent.find(&p).is_some());
    run(&mut h, "file.clear_recent");
    assert!(h.state().state.shell.recent.find(&p).is_none(), "Clear Recent Files");
}

/// D-016 New blank PDF, and a new PDF from a saved page template.
#[test]
fn new_blank_pdf_and_new_from_template() {
    let dir = temp_dir("doca-new");
    let mut h = empty_app();
    key(&mut h, CTRL, Key::N);
    assert_eq!(h.state().state.docs.len(), 1, "Ctrl+N makes a new blank PDF");
    assert_eq!(h.state().state.doc().unwrap().render.as_ref().unwrap().pages().len(), 1);
    let mut a = automation(&dir);
    call(
        &mut a,
        "doc_new",
        json!({ "path": "blank.pdf", "pages": 3, "width": 792, "height": 612 }),
    );
    call(&mut a, "doc_save", json!({}));
    assert_eq!(page_count(&mut a, "blank.pdf"), 3);
    sample_pdf(&dir, "tpl-src.pdf");
    call(&mut a, "doc_open", json!({ "path": "tpl-src.pdf" }));
    call(
        &mut a,
        "page_template",
        json!({ "action": "save", "dir": "templates", "name": "Plan sheet" }),
    );
    let l = call(&mut a, "page_template", json!({ "action": "list", "dir": "templates" }));
    assert!(l.to_string().contains("Plan sheet"), "{l}");
    call(
        &mut a,
        "page_template",
        json!({ "action": "new", "dir": "templates", "name": "Plan sheet", "path": "from-tpl.pdf" }),
    );
    call(&mut a, "doc_save", json!({}));
    assert!(page_count(&mut a, "from-tpl.pdf") >= 1);
    call(
        &mut a,
        "page_template",
        json!({ "action": "remove", "dir": "templates", "name": "Plan sheet" }),
    );
    let l = call(&mut a, "page_template", json!({ "action": "list", "dir": "templates" }));
    assert!(!l.to_string().contains("Plan sheet"), "{l}");
}

/// D-017 File > Create > PDF Package: an empty package.
#[test]
fn an_empty_pdf_package_is_created() {
    let dir = temp_dir("doca-pkg");
    let mut a = automation(&dir);
    call(&mut a, "pdf_package", json!({ "action": "create", "out": "pkg.pdf" }));
    call(&mut a, "doc_open", json!({ "path": "pkg.pdf" }));
    let i = call(&mut a, "pdf_package", json!({ "action": "info" }));
    assert!(i.to_string().contains("true"), "{i}");
    assert!(i["files"].as_array().is_none_or(Vec::is_empty), "{i}");
}

/// D-018 Email PDF: a new message with the current PDF attached; templates prefill it (Ctrl+E).
#[test]
fn email_attaches_the_pdf_and_templates_prefill() {
    let dir = temp_dir("doca-email");
    sample_pdf(&dir, "sheet.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "sheet.pdf" }));
    call(
        &mut a,
        "email_template",
        json!({ "action": "save", "path": "mail.json", "name": "RFI", "to": "pm@example.com",
                "subject": "Markups on {file}", "body": "See attached." }),
    );
    call(
        &mut a,
        "email_template",
        json!({ "action": "draft", "path": "mail.json", "name": "RFI", "out": "m.eml" }),
    );
    let eml = std::fs::read_to_string(dir.join("m.eml")).unwrap();
    assert!(eml.contains("pm@example.com"), "{eml}");
    assert!(eml.contains("Subject: Markups on sheet"), "{eml}");
    assert!(eml.to_ascii_lowercase().contains("attachment"), "{eml}");
    assert!(eml.contains("sheet.pdf"));
    assert!(bound("Ctrl+E").is_some(), "Ctrl+E is bound");
}

/// D-019 saving keeps revisions in the file (incremental), a full save drops them.
/// D-020 Revert As writes an earlier revision out as a new file.
#[test]
fn revisions_are_kept_and_an_earlier_one_reverts_as_a_new_file() {
    let dir = temp_dir("doca-rev");
    sample_pdf(&dir, "r.pdf");
    let mut a = automation(&dir);
    let original = markup_count(&mut a, "r.pdf");
    call(&mut a, "doc_open", json!({ "path": "r.pdf" }));
    let n0 = call(&mut a, "doc_revisions", json!({}))["revisions"].as_u64().unwrap();
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[50, 50], [90, 90]] }),
    );
    call(&mut a, "doc_save", json!({}));
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[150, 50], [190, 90]] }),
    );
    call(&mut a, "doc_save", json!({}));
    let n2 = call(&mut a, "doc_revisions", json!({}))["revisions"].as_u64().unwrap();
    assert_eq!(n2, n0 + 2, "each incremental save adds a revision");
    call(&mut a, "doc_revisions", json!({ "revision": n0, "out": "back.pdf" }));
    assert_eq!(
        markup_count(&mut a, "back.pdf"),
        original,
        "Revert As: the earlier state"
    );
    call(&mut a, "doc_revisions", json!({ "revision": n0 + 1, "out": "mid.pdf" }));
    assert_eq!(markup_count(&mut a, "mid.pdf"), original + 1);
    call(&mut a, "doc_save", json!({ "full": true }));
    let n3 = call(&mut a, "doc_revisions", json!({}))["revisions"].as_u64().unwrap();
    assert_eq!(n3, 1, "a full save keeps no history");
}

/// D-021 Publish As: flattened (markups burned in), compressed 1.5, uncompressed.
#[test]
fn publish_as_flattened_compressed_and_uncompressed() {
    let dir = temp_dir("doca-publish");
    sample_pdf(&dir, "p.pdf");
    let mut a = automation(&dir);
    let n = markup_count(&mut a, "p.pdf");
    assert!(n > 0);
    call(&mut a, "doc_open", json!({ "path": "p.pdf" }));
    call(&mut a, "doc_publish", json!({ "mode": "flattened", "out": "flat.pdf" }));
    call(
        &mut a,
        "doc_publish",
        json!({ "mode": "compressed", "out": "comp.pdf" }),
    );
    call(
        &mut a,
        "doc_publish",
        json!({ "mode": "uncompressed", "out": "plain.pdf" }),
    );
    assert_eq!(markup_count(&mut a, "flat.pdf"), 0, "markups are burned in");
    assert_eq!(markup_count(&mut a, "comp.pdf"), n);
    assert_eq!(markup_count(&mut a, "plain.pdf"), n);
    let comp = std::fs::read(dir.join("comp.pdf")).unwrap();
    assert!(comp.starts_with(b"%PDF-1.5") || comp.starts_with(b"%PDF-1.6") || comp.starts_with(b"%PDF-1.7"));
    assert!(comp.windows(7).any(|w| w == b"/ObjStm"), "object streams");
    let plain = std::fs::read(dir.join("plain.pdf")).unwrap();
    assert!(!plain.windows(7).any(|w| w == b"/ObjStm"));
    assert!(
        plain.windows(5).any(|w| w == b"xref\n" || w == b"xref\r"),
        "classic xref table"
    );
    assert!(bound("Ctrl+Shift+P").is_some(), "Ctrl+Shift+P publishes compressed");
    assert!(bound("Ctrl+Alt+F").is_some(), "Ctrl+Alt+F publishes flattened");
}

/// D-022 Refresh: F5 re-renders, Shift+F5 reloads the document from disk.
#[test]
fn refresh_reloads_the_document_from_disk() {
    let dir = temp_dir("doca-refresh");
    let p = sample_pdf(&dir, "live.pdf");
    let mut h = empty_app();
    open(&mut h, &p);
    let n = markups(&h).len();
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "live.pdf" }));
    call(
        &mut a,
        "markup_add",
        json!({ "page": 1, "kind": "Rectangle", "points": [[10, 10], [40, 40]] }),
    );
    call(&mut a, "doc_save", json!({}));
    run(&mut h, "view.refresh");
    assert_eq!(markups(&h).len(), n, "F5 only re-renders");
    key(&mut h, Modifiers::SHIFT, Key::F5);
    h.run_steps(3);
    assert_eq!(markups(&h).len(), n + 1, "Shift+F5 reloads from disk");
    assert_eq!(shortcut("view.refresh").as_deref(), Some("F5"));
}

/// D-023 Web Tab (partial): link pages for web addresses; Ctrl+T.
#[test]
fn web_tab_link_page_opens_web_addresses() {
    let dir = temp_dir("doca-web");
    let mut a = automation(&dir);
    call(
        &mut a,
        "webtab",
        json!({ "action": "links", "out": "web.pdf", "title": "Project sites",
                "links": [{ "name": "Spec portal", "url": "https://example.com/spec" }] }),
    );
    call(&mut a, "doc_open", json!({ "path": "web.pdf" }));
    let l = call(&mut a, "link_list", json!({}));
    assert!(l.to_string().contains("https://example.com/spec"), "{l}");
    assert!(bound("Ctrl+T").is_some(), "Ctrl+T opens the Web Tab");
}

/// D-024 PDF/A files: tab icon, opened locked for editing until unlocked; Verify reports.
#[test]
fn pdfa_files_are_marked_and_locked_until_unlocked() {
    let dir = temp_dir("doca-pdfa");
    sample_pdf(&dir, "arch.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "arch.pdf" }));
    call(&mut a, "doc_pdfa", json!({ "action": "archive", "level": "2b" }));
    call(&mut a, "doc_save", json!({ "path": "arch-a.pdf", "full": true }));
    call(&mut a, "doc_open", json!({ "path": "arch-a.pdf" }));
    let st = call(&mut a, "doc_standards", json!({}));
    assert!(st.to_string().contains("2B") || st.to_string().contains("2b"), "{st}");
    let v = call(&mut a, "doc_pdfa", json!({ "action": "verify" }));
    assert!(!v.is_null());
    // In the app the page edit is refused.
    let mut h = empty_app();
    open(&mut h, &dir.join("arch-a.pdf"));
    let pages = h.state().state.doc().unwrap().render.as_ref().unwrap().pages().len();
    run(&mut h, "document.delete_page");
    h.run_steps(3);
    assert_eq!(
        h.state().state.doc().unwrap().render.as_ref().unwrap().pages().len(),
        pages,
        "PDF/A refuses page edits"
    );
    assert!(shows(&h, "PDF/A"), "PDF/A is marked");
    // Unlocked, it is an ordinary PDF again.
    call(&mut a, "doc_pdfa", json!({ "action": "unlock" }));
    let st = call(&mut a, "doc_standards", json!({}));
    assert!(
        !st.to_string().contains("2B") && !st.to_string().contains("\"2b\""),
        "{st}"
    );
}

// ---------------------------------------------------------------- navigation and zoom

fn view<'a>(h: &'a Harness<'_, MarkupCraftApp>) -> &'a markupcraft_ui_egui::canvas::DocView {
    &h.state().state.doc().unwrap().view
}

/// The middle of the document area on screen.
fn middle(h: &Harness<'_, MarkupCraftApp>) -> egui::Pos2 {
    view(h).viewport().center()
}

fn press(h: &mut Harness<'_, MarkupCraftApp>, at: egui::Pos2, b: egui::PointerButton, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos: at,
        button: b,
        pressed,
        modifiers: Modifiers::NONE,
    });
    h.step();
}

fn wheel(h: &mut Harness<'_, MarkupCraftApp>, delta: egui::Vec2, m: Modifiers) {
    let at = middle(h);
    h.hover_at(at);
    h.step();
    h.event(egui::Event::ModifiersChanged(m));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta,
        modifiers: m,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(2);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

/// Type into the navigation bar's page box and press Enter.
fn type_page(h: &mut Harness<'_, MarkupCraftApp>, text: &str) {
    h.ctx.memory_mut(|m| m.request_focus(egui::Id::new("page-entry")));
    h.run_steps(2);
    for _ in 0..8 {
        key(h, Modifiers::NONE, Key::Backspace);
        key(h, Modifiers::NONE, Key::Delete);
    }
    h.event(egui::Event::Text(text.to_string()));
    h.step();
    key(h, Modifiers::NONE, Key::Enter);
    h.run_steps(3);
}

/// D-025 next / previous page (Ctrl+Right / Ctrl+Left, PgDn / PgUp). D-026 first / last
/// (Home / End). D-027 the page box takes a number or a label. D-028 Alt+Left / Alt+Right go
/// back and forward through view history.
#[test]
fn page_navigation_keys_page_box_and_view_history() {
    let dir = temp_dir("doca-nav");
    sample_pdf(&dir, "nav.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "nav.pdf" }));
    call(&mut a, "page_insert_blank", json!({ "at": 3, "count": 2 }));
    call(
        &mut a,
        "page_label_set",
        json!({ "labels": { "1": "A-101", "2": "A-102", "3": "A-103", "4": "A-104" } }),
    );
    call(&mut a, "doc_save", json!({}));
    let mut h = empty_app();
    open(&mut h, &dir.join("nav.pdf"));
    view_single(&mut h);
    assert_eq!(page(&h), 0);
    key(&mut h, CTRL, Key::ArrowRight);
    assert_eq!(page(&h), 1, "Ctrl+Right");
    key(&mut h, CTRL, Key::ArrowLeft);
    assert_eq!(page(&h), 0, "Ctrl+Left");
    key(&mut h, Modifiers::NONE, Key::PageDown);
    assert_eq!(page(&h), 1, "PgDn");
    key(&mut h, Modifiers::NONE, Key::PageUp);
    assert_eq!(page(&h), 0, "PgUp");
    key(&mut h, Modifiers::NONE, Key::End);
    assert_eq!(page(&h), 3, "End");
    key(&mut h, Modifiers::NONE, Key::Home);
    assert_eq!(page(&h), 0, "Home");
    // The page box: a number, then a label.
    type_page(&mut h, "3");
    assert_eq!(page(&h), 2, "typed page number");
    type_page(&mut h, "A-102");
    assert_eq!(page(&h), 1, "typed page label");
    assert!(shows(&h, "A-102"), "the label is shown in the navigation bar");
    // View history.
    key(&mut h, Modifiers::ALT, Key::ArrowLeft);
    assert_eq!(page(&h), 2, "Alt+Left goes back");
    key(&mut h, Modifiers::ALT, Key::ArrowLeft);
    key(&mut h, Modifiers::ALT, Key::ArrowRight);
    assert_eq!(page(&h), 2, "Alt+Right goes forward");
    // History includes zoom.
    run(&mut h, "view.actual_size");
    let z1 = view(&h).zoom;
    run(&mut h, "view.zoom_in");
    assert!(view(&h).zoom > z1);
    key(&mut h, Modifiers::ALT, Key::ArrowLeft);
    assert!((view(&h).zoom - z1).abs() < 1e-3, "back restores the zoom");
}

fn view_single(h: &mut Harness<'_, MarkupCraftApp>) {
    key(h, CTRL, Key::Num4);
}

/// D-029 Fit Page / Fit Width / Actual Size (Ctrl+9 / Ctrl+0 / Ctrl+8). D-030 zoom in / out
/// (Plus / Minus). D-035 the zoom stops at the maximum zoom preference.
#[test]
fn zoom_presets_steps_and_maximum() {
    use markupcraft_ui_egui::canvas::Fit;
    let mut h = app();
    key(&mut h, CTRL, Key::Num8);
    assert!((view(&h).zoom - 1.0).abs() < 1e-3, "Actual Size = 100 %");
    key(&mut h, CTRL, Key::Num0);
    assert_eq!(view(&h).fit, Fit::Width);
    let width_zoom = view(&h).zoom;
    key(&mut h, CTRL, Key::Num9);
    assert_eq!(view(&h).fit, Fit::Page);
    let page_zoom = view(&h).zoom;
    assert!(width_zoom >= page_zoom, "{width_zoom} vs {page_zoom}");
    let z = view(&h).zoom;
    key(&mut h, Modifiers::NONE, Key::Plus);
    assert!(view(&h).zoom > z, "Plus zooms in");
    let z = view(&h).zoom;
    key(&mut h, Modifiers::NONE, Key::Minus);
    assert!(view(&h).zoom < z, "Minus zooms out");
    h.state_mut().state.shell.ui.max_zoom_pct = 400.0;
    h.run_steps(3);
    for _ in 0..40 {
        run(&mut h, "view.zoom_in");
    }
    assert!(view(&h).zoom <= 4.0 + 1e-3, "capped at 400 %: {}", view(&h).zoom);
}

/// D-031 the Zoom tool: drag a rectangle to fill the screen, Ctrl-click zooms out, Shift+Z
/// toggles it temporarily.
#[test]
fn zoom_tool_box_click_out_and_toggle() {
    let mut h = app();
    run(&mut h, "view.fit_page");
    key(&mut h, Modifiers::NONE, Key::Z);
    assert_eq!(h.state().state.tool, "zoom");
    let z = view(&h).zoom;
    drag(&mut h, (300.0, 300.0), (500.0, 450.0));
    let zin = view(&h).zoom;
    assert!(zin > z * 2.0, "the box fills the view: {z} -> {zin}");
    let at = middle(&h);
    h.hover_at(at);
    h.step();
    h.run_steps(30);
    let zin = view(&h).zoom;
    button(&mut h, at, true, Modifiers::NONE);
    button(&mut h, at, false, Modifiers::NONE);
    h.run_steps(30);
    let zplain = view(&h).zoom;
    assert!(zplain > zin, "click zooms in {zin} {zplain}");
    h.event(egui::Event::ModifiersChanged(CTRL));
    h.step();
    button(&mut h, at, true, CTRL);
    button(&mut h, at, false, CTRL);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
    assert!(
        view(&h).zoom < zplain,
        "Ctrl-click zooms out {zplain} {}",
        view(&h).zoom
    );
    run(&mut h, "tool.select");
    let before = h.state().state.tool;
    key(&mut h, Modifiers::SHIFT, Key::Z);
    assert_eq!(h.state().state.tool, "zoom", "Shift+Z picks the zoom tool");
    key(&mut h, Modifiers::SHIFT, Key::Z);
    assert_eq!(h.state().state.tool, before, "Shift+Z again goes back");
}

/// D-032 the wheel zooms or scrolls per layout mode; Ctrl swaps; reverse and sensitivity.
#[test]
fn mouse_wheel_zooms_or_scrolls_and_ctrl_swaps() {
    let mut h = app();
    run(&mut h, "view.continuous");
    run(&mut h, "view.fit_width");
    h.state_mut().state.shell.ui.wheel_zooms_continuous = false;
    h.run_steps(3);
    let (z, off) = (view(&h).zoom, view(&h).offset);
    wheel(&mut h, egui::vec2(0.0, -120.0), Modifiers::NONE);
    assert!((view(&h).zoom - z).abs() < 1e-4, "scroll mode keeps the zoom");
    assert!(view(&h).offset.y > off.y, "scroll mode scrolls down");
    let z = view(&h).zoom;
    wheel(&mut h, egui::vec2(0.0, 120.0), CTRL);
    assert!((view(&h).zoom - z).abs() > 1e-4, "Ctrl swaps to zoom");
    // Zoom mode, and reversed.
    h.state_mut().state.shell.ui.wheel_zooms_continuous = true;
    h.run_steps(3);
    let z = view(&h).zoom;
    wheel(&mut h, egui::vec2(0.0, 120.0), Modifiers::NONE);
    let up = view(&h).zoom;
    assert!(up > z, "wheel up zooms in");
    h.state_mut().state.shell.ui.reverse_wheel = true;
    h.run_steps(3);
    wheel(&mut h, egui::vec2(0.0, 120.0), Modifiers::NONE);
    assert!(view(&h).zoom < up, "reversed: wheel up zooms out");
}

/// D-033 middle-button drag pans; Spacebar held pans without dropping the markup in progress.
/// D-034 the Pan tool. D-036 horizontal wheel pans. D-037 Fit Width locks sideways panning.
#[test]
fn panning_middle_button_space_pan_tool_and_fit_width_lock() {
    let mut h = app();
    run(&mut h, "view.actual_size");
    h.run_steps(3);
    let off = view(&h).offset;
    let a = middle(&h);
    let b = a + egui::vec2(-80.0, -60.0);
    h.hover_at(a);
    h.step();
    press(&mut h, a, egui::PointerButton::Middle, true);
    for i in 1..=6 {
        h.hover_at(a + (b - a) * (i as f32 / 6.0));
        h.step();
    }
    press(&mut h, b, egui::PointerButton::Middle, false);
    h.run_steps(2);
    let moved = view(&h).offset - off;
    assert!(moved.length() > 40.0, "middle drag pans: {moved:?}");
    // Pan tool (Shift+V).
    key(&mut h, Modifiers::SHIFT, Key::V);
    assert_eq!(h.state().state.tool, "pan");
    let off = view(&h).offset;
    drag(&mut h, (500.0, 400.0), (450.0, 350.0));
    assert!((view(&h).offset - off).length() > 20.0, "the Pan tool pans");
    // Horizontal wheel.
    let off = view(&h).offset;
    wheel(&mut h, egui::vec2(-120.0, 0.0), Modifiers::NONE);
    assert!((view(&h).offset.x - off.x).abs() > 1.0, "tilt wheel pans sideways");
    // Spacebar held while drawing.
    run(&mut h, "tool.rectangle");
    let before = markups(&h).len();
    let p = middle(&h);
    h.hover_at(p);
    h.step();
    h.event(egui::Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.step();
    let off = view(&h).offset;
    button(&mut h, p, true, Modifiers::NONE);
    for i in 1..=6 {
        h.hover_at(p + egui::vec2(10.0 * i as f32, 8.0 * i as f32));
        h.step();
    }
    button(&mut h, p + egui::vec2(60.0, 48.0), false, Modifiers::NONE);
    h.event(egui::Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(3);
    assert!(
        (view(&h).offset - off).length() > 20.0,
        "Space + drag pans: {off:?} {:?} {:?}",
        view(&h).offset,
        markups(&h).len() - before
    );
    assert_eq!(markups(&h).len(), before, "no markup drawn while panning");
    assert_eq!(h.state().state.tool, "rectangle", "the tool is kept");
    // Fit Width lock.
    h.state_mut().state.shell.ui.lock_fit_width = true;
    run(&mut h, "view.fit_width");
    run(&mut h, "tool.pan");
    h.run_steps(3);
    let off = view(&h).offset;
    drag(&mut h, (500.0, 400.0), (380.0, 300.0));
    let d = view(&h).offset - off;
    assert!(d.x.abs() < 0.5, "no sideways drift in Fit Width: {d:?}");
}

/// D-038 Single Page, Continuous, Side-by-Side, Continuous Side-by-Side, cover page alone.
/// D-039 a default layout for every PDF opened.
#[test]
fn page_layout_modes_cover_page_and_default_layout() {
    use markupcraft_ui_egui::canvas::PageMode;
    let mut h = app();
    let modes = [
        (Key::Num4, PageMode::Single),
        (Key::Num5, PageMode::Continuous),
        (Key::Num6, PageMode::SideBySide),
        (Key::Num7, PageMode::ContinuousSideBySide),
    ];
    for (k, m) in modes {
        key(&mut h, CTRL, k);
        assert_eq!(view(&h).mode, m, "{k:?}");
    }
    run(&mut h, "view.cover_page");
    assert!(view(&h).cover, "cover page alone");
    // Default layout preference.
    let dir = temp_dir("doca-layout");
    let p = sample_pdf(&dir, "lay.pdf");
    let p2 = sample_pdf(&dir, "lay2.pdf");
    let mut h = empty_app();
    h.state_mut().state.shell.ui.default_mode = "continuous".into();
    open(&mut h, &p);
    assert_eq!(view(&h).mode, PageMode::Continuous);
    h.state_mut().state.shell.ui.default_mode = "single".into();
    open(&mut h, &p2);
    assert_eq!(view(&h).mode, PageMode::Single);
}

/// D-040 rotate the display 90 degrees without changing the file.
#[test]
fn rotate_view_turns_the_display_not_the_file() {
    let mut h = app();
    key(&mut h, CTRL_SHIFT, Key::Plus);
    assert_eq!(view(&h).rotation, 90);
    key(&mut h, CTRL_SHIFT, Key::Minus);
    key(&mut h, CTRL_SHIFT, Key::Minus);
    assert_eq!(view(&h).rotation, 270);
    let d = h.state().state.doc().unwrap();
    assert!(!d.session.is_dirty(), "the file is unchanged");
}

/// D-041 Full screen (F11) / Presentation (Ctrl+Enter). D-042 Always on top (Ctrl+F12).
/// D-043 hide panels (Shift+F4), menu bar (F9), navigation bar (F4), status bar (F8).
#[test]
fn window_full_screen_presentation_on_top_and_hiding_bars() {
    let mut h = app();
    let on = |h: &Harness<'_, MarkupCraftApp>, id: &str| h.state().state.checked(id) == Some(true);
    key(&mut h, Modifiers::NONE, Key::F11);
    assert!(on(&h, "window.full_screen"), "F11");
    key(&mut h, Modifiers::NONE, Key::F11);
    assert!(!on(&h, "window.full_screen"));
    key(&mut h, CTRL, Key::Enter);
    assert!(on(&h, "window.presentation"), "Ctrl+Enter");
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(!on(&h, "window.presentation"), "Esc leaves the presentation");
    key(&mut h, CTRL, Key::F12);
    assert!(h.state().state.shell.always_on_top, "Ctrl+F12");
    key(&mut h, Modifiers::SHIFT, Key::F4);
    assert!(h.state().state.shell.panels_hidden, "Shift+F4");
    key(&mut h, Modifiers::SHIFT, Key::F4);
    assert!(!h.state().state.shell.panels_hidden);
    let ui = |h: &Harness<'_, MarkupCraftApp>| {
        let u = &h.state().state.shell.ui;
        (u.show_menu, u.show_nav_bar, u.show_status_bar)
    };
    assert_eq!(ui(&h), (true, true, true));
    key(&mut h, Modifiers::NONE, Key::F9);
    key(&mut h, Modifiers::NONE, Key::F4);
    key(&mut h, Modifiers::NONE, Key::F8);
    assert_eq!(ui(&h), (false, false, false));
}

/// D-044 the navigation bar shows the page's print size and its scale ("Scale Not Set" when
/// none); clicking the scale starts calibration.
#[test]
fn navigation_bar_shows_page_size_and_scale() {
    let mut h = app();
    key(&mut h, Modifiers::NONE, Key::End);
    let txt = h.state().state.scale_readout();
    let d = h.state().state.doc().unwrap();
    let g = d.render.as_ref().unwrap().page(d.view.current).unwrap();
    let (w, hh) = (g.width / 72.0, g.height / 72.0);
    let size = format!("{w:.2} x {hh:.2} in");
    assert!(shows(&h, &size), "page size {size}");
    // Revu's wording when a page has no scale.
    assert_eq!(txt, "Scale Not Set");
    h.get_by_label("Scale Not Set").click();
    h.run_steps(3);
    assert_eq!(
        h.state().state.tool,
        "calibrate",
        "clicking the scale starts calibration"
    );
}

/// D-045 rulers (Ctrl+R) in a chosen unit. D-046 crosshair. D-047 Dimmer (Ctrl+F5).
/// D-048 Disable Line Weights. D-049 Dark mode.
#[test]
fn rulers_crosshair_dimmer_line_weights_and_dark_mode() {
    let mut h = app();
    key(&mut h, CTRL, Key::R);
    assert!(h.state().state.shell.ui.rulers, "Ctrl+R");
    run(&mut h, "view.crosshair");
    assert!(h.state().state.shell.ui.crosshair);
    key(&mut h, CTRL, Key::F5);
    assert!(h.state().state.shell.dimmer, "Ctrl+F5");
    assert!(view(&h).opts.dim > 0.0, "the page is faded");
    run(&mut h, "view.line_weights");
    assert_eq!(h.state().state.checked("view.line_weights"), Some(true));
    run(&mut h, "view.dark_workspace");
    assert_eq!(h.state().state.checked("view.dark_workspace"), Some(true));
    assert!(view(&h).opts.dark, "pages drawn light on dark");
}

// ---------------------------------------------------------------- split views

fn split<'a>(h: &'a Harness<'static, MarkupCraftApp>) -> Option<&'a shell::split::Split> {
    h.state().state.shell.split.as_ref()
}

/// D-050 Split Vertical (Ctrl+2) / Horizontal (Ctrl+H), same or different files.
/// D-051 Unsplit (Ctrl+Shift+2). D-052 Toggle Split (Ctrl+I), Switch (Ctrl+1), Balance
/// (Shift+F12).
#[test]
fn split_toggle_switch_balance_and_unsplit() {
    let dir = temp_dir("doca-split");
    let pa = sample_pdf(&dir, "left.pdf");
    let pb = sample_pdf(&dir, "right.pdf");
    let mut h = empty_app();
    open(&mut h, &pa);
    open(&mut h, &pb);
    key(&mut h, CTRL, Key::Num2);
    assert!(split(&h).is_some_and(|s| s.vertical), "Ctrl+2 splits vertically");
    key(&mut h, CTRL, Key::H);
    assert!(split(&h).is_some_and(|s| !s.vertical), "Ctrl+H splits horizontally");
    key(&mut h, CTRL, Key::I);
    assert!(split(&h).is_some_and(|s| s.vertical), "Ctrl+I toggles the direction");
    // A different file in the second pane.
    let ua = h.state().state.docs[0].uid;
    shell::split::show_in_pane(&mut h.state_mut().state, ua);
    h.run_steps(3);
    assert_eq!(active_name(&h), "right.pdf");
    assert_eq!(split(&h).unwrap().pane.uid, ua);
    key(&mut h, CTRL, Key::Num1);
    assert_eq!(active_name(&h), "left.pdf", "Ctrl+1 switches the panes");
    h.state_mut().state.shell.split.as_mut().unwrap().frac = 0.2;
    key(&mut h, Modifiers::SHIFT, Key::F12);
    assert!((split(&h).unwrap().frac - 0.5).abs() < 1e-3, "Shift+F12 balances");
    key(&mut h, CTRL_SHIFT, Key::Num2);
    assert!(split(&h).is_none(), "Ctrl+Shift+2 unsplits");
}

/// D-053 Synchronize Document: page 1 with page 1, pan and zoom together.
/// D-054 Synchronize Page: pan / zoom together whatever the page numbers.
#[test]
fn synchronized_splits_follow_by_document_or_by_page() {
    let mut h = app();
    run(&mut h, "view.single_page");
    run(&mut h, "view.split_vertical");
    run(&mut h, "view.sync_document");
    h.run_steps(4);
    key(&mut h, Modifiers::NONE, Key::End);
    h.run_steps(4);
    assert_eq!(split(&h).unwrap().pane.view.current, 1, "Document sync: same page");
    run(&mut h, "view.zoom_in");
    h.run_steps(4);
    assert!(
        (split(&h).unwrap().pane.view.zoom - view(&h).zoom).abs() < 1e-3,
        "same zoom"
    );
    // Page sync: the other pane stays on its page but zooms along.
    run(&mut h, "view.sync_page");
    key(&mut h, Modifiers::NONE, Key::Home);
    h.run_steps(4);
    let pane_page = split(&h).unwrap().pane.view.current;
    let z = split(&h).unwrap().pane.view.zoom;
    run(&mut h, "view.zoom_in");
    h.run_steps(4);
    let s = split(&h).unwrap();
    assert!(
        s.pane.view.zoom > z,
        "Page sync zooms along: {z} -> {}",
        s.pane.view.zoom
    );
    assert_eq!(s.pane.view.current, pane_page, "Page sync keeps the other pane's page");
}

/// D-055 drag a document tab onto the other pane.
#[test]
fn a_document_tab_dragged_onto_the_second_pane_shows_there() {
    let dir = temp_dir("doca-tabdrag");
    let pa = sample_pdf(&dir, "one.pdf");
    let pb = sample_pdf(&dir, "two.pdf");
    let mut h = empty_app();
    open(&mut h, &pa);
    open(&mut h, &pb);
    run(&mut h, "view.split_vertical");
    h.run_steps(4);
    let ua = h.state().state.docs[0].uid;
    assert_ne!(split(&h).unwrap().pane.uid, ua);
    let from = h.get_by_label("one.pdf").rect().center();
    let to = egui::pos2(1150.0, 500.0);
    h.hover_at(from);
    h.step();
    button(&mut h, from, true, Modifiers::NONE);
    for i in 1..=10 {
        h.hover_at(from + (to - from) * (i as f32 / 10.0));
        h.step();
    }
    button(&mut h, to, false, Modifiers::NONE);
    h.run_steps(4);
    assert_eq!(split(&h).unwrap().pane.uid, ua, "{}", h.state().state.status);
}

/// D-050: up to 16 panes, each with its own tabs; any pane shows any open document, and each
/// tab keeps its place in its pane.
#[test]
fn split_panes_have_their_own_tab_bars() {
    let dir = temp_dir("doca-pane-tabs");
    let pa = sample_pdf(&dir, "alpha.pdf");
    let pb = sample_pdf(&dir, "bravo.pdf");
    let pc = sample_pdf(&dir, "charlie.pdf");
    let mut h = empty_app();
    open(&mut h, &pa);
    open(&mut h, &pb);
    open(&mut h, &pc);
    run(&mut h, "view.split_vertical");
    h.run_steps(4);
    let uid = |h: &Harness<'_, MarkupCraftApp>, n: &str| h.state().state.docs.iter().find(|d| d.name == n).unwrap().uid;
    let (ua, uc) = (uid(&h, "alpha.pdf"), uid(&h, "charlie.pdf"));
    assert_eq!(
        split(&h).unwrap().pane.tabs,
        vec![uc],
        "the new pane starts with one tab"
    );
    // A second document joins the pane's own tabs (as a dropped tab does).
    shell::split::show_in_pane(&mut h.state_mut().state, ua);
    h.run_steps(4);
    assert_eq!(split(&h).unwrap().pane.tabs, vec![uc, ua]);
    assert_eq!(split(&h).unwrap().pane.uid, ua);
    h.state_mut()
        .state
        .shell
        .split
        .as_mut()
        .unwrap()
        .pane
        .view
        .go_to_page(1, 2);
    h.run_steps(2);
    // The pane's tab bar: click its charlie.pdf tab (the one inside the pane, not the main bar).
    let pane = h.state().state.shell.extra.pane_rect.expect("the second pane");
    let tab = h
        .query_all_by_label("charlie.pdf")
        .map(|n| n.rect())
        .find(|r| pane.contains(r.center()))
        .expect("the pane has its own charlie.pdf tab");
    h.hover_at(tab.center());
    h.step();
    button(&mut h, tab.center(), true, Modifiers::NONE);
    button(&mut h, tab.center(), false, Modifiers::NONE);
    h.run_steps(4);
    let s = split(&h).unwrap();
    assert_eq!(s.pane.uid, uc, "the pane's tab shows its document");
    assert_eq!(s.pane.tabs, vec![uc, ua], "both tabs stay in the pane");
    // Back to alpha in the pane: the page it was left at comes back.
    h.state_mut().state.shell.split.as_mut().unwrap().pane.show(ua);
    h.run_steps(3);
    assert_eq!(
        split(&h).unwrap().pane.view.current,
        1,
        "alpha kept its page in the pane"
    );
    // Closing a tab in the pane leaves the pane with its other tab.
    let s = h.state_mut().state.shell.split.as_mut().unwrap();
    assert!(s.pane.close_tab(ua));
    assert_eq!((s.pane.tabs.clone(), s.pane.uid), (vec![uc], uc));
    assert!(!s.pane.close_tab(uc), "the last tab closes the pane instead");
    // Up to 16 panes.
    for _ in 0..20 {
        run(&mut h, "view.split_vertical");
    }
    assert_eq!(split(&h).unwrap().panes(), shell::split::MAX_PANES, "at most 16 panes");
    h.run_steps(4);
    assert!(
        split(&h).unwrap().more.iter().all(|p| p.tabs.len() == 1),
        "each new pane has its own tab bar"
    );
}

/// D-050: a pane's tab keeps its own place (page) when the pane shows another tab and back.
#[test]
fn a_pane_tab_keeps_its_place() {
    let dir = temp_dir("doca-pane-place");
    let pa = sample_pdf(&dir, "one.pdf");
    let pb = sample_pdf(&dir, "two.pdf");
    let mut h = empty_app();
    open(&mut h, &pa);
    open(&mut h, &pb);
    run(&mut h, "view.split_vertical");
    h.run_steps(4);
    let ua = h.state().state.docs[0].uid;
    let ub = h.state().state.docs[1].uid;
    shell::split::show_in_pane(&mut h.state_mut().state, ua);
    h.run_steps(3);
    h.state_mut()
        .state
        .shell
        .split
        .as_mut()
        .unwrap()
        .pane
        .view
        .go_to_page(1, 2);
    h.run_steps(3);
    let pane = &mut h.state_mut().state.shell.split.as_mut().unwrap().pane;
    pane.show(ub);
    assert_eq!(pane.view.current, 0, "a fresh tab starts at the top");
    pane.show(ua);
    assert_eq!(pane.view.current, 1, "the tab came back where it was left");
}

/// D-056 Detach a tab into its own window; Reattach puts it back.
#[test]
fn detach_a_tab_into_its_own_window_and_reattach() {
    let mut h = app();
    run(&mut h, "window.detach");
    h.run_steps(4);
    assert_eq!(h.state().state.shell.extra.detached.len(), 1);
    run(&mut h, "window.reattach");
    h.run_steps(4);
    assert!(h.state().state.shell.extra.detached.is_empty());
}

/// D-056: detaching MOVES the tab out of the main window (a Ctrl-drag or Detach a Copy leaves
/// it there too); a floating window holds a tab bar of several documents.
#[test]
fn detached_window_holds_tabs_and_detach_moves_the_tab() {
    use markupcraft_ui_egui::shell::detach;
    let dir = temp_dir("doca-detach-tabs");
    let other = sample_pdf(&dir, "other.pdf");
    let third = sample_pdf(&dir, "third.pdf");
    let mut h = app();
    let sample = h.state().state.doc().unwrap().uid;
    open(&mut h, &other);
    open(&mut h, &third);
    let uid_of =
        |h: &Harness<'_, MarkupCraftApp>, n: &str| h.state().state.docs.iter().find(|d| d.name == n).unwrap().uid;
    let (other_uid, third_uid) = (uid_of(&h, "other.pdf"), uid_of(&h, "third.pdf"));
    // Detach the sample: its tab leaves the main window.
    let i = h.state().state.docs.iter().position(|d| d.uid == sample).unwrap();
    h.state_mut().state.active = i;
    run(&mut h, "window.detach");
    h.run_steps(4);
    let st = &h.state().state;
    assert_eq!(st.shell.extra.detached.len(), 1);
    assert!(!detach::main_window_docs(st).contains(&sample), "moved, not copied");
    assert!(detach::main_window_docs(st).contains(&other_uid));
    // The window's tab bar: Add Tab moves other.pdf in.
    h.get_by_label("Add Tab").click();
    h.run_steps(3);
    h.get_all_by_label("other.pdf").last().unwrap().click();
    h.run_steps(4);
    let st = &h.state().state;
    assert_eq!(
        st.shell.extra.detached[0].uids(),
        [sample, other_uid],
        "two tabs in one window"
    );
    assert_eq!(
        detach::main_window_docs(st),
        [third_uid],
        "the main window keeps the rest"
    );
    assert_eq!(st.shell.extra.detached[0].uid(), other_uid, "the added tab shows");
    // Switch tabs in the window.
    let tab = h.get_all_by_label("sample.pdf").last().unwrap().rect().center();
    h.hover_at(tab);
    h.step();
    button(&mut h, tab, true, Modifiers::NONE);
    button(&mut h, tab, false, Modifiers::NONE);
    h.run_steps(3);
    assert_eq!(
        h.state().state.shell.extra.detached[0].uid(),
        sample,
        "clicked tab shows"
    );
    // The main window still shows its own document while the window's is active.
    h.run_steps(3);
    assert_eq!(active_name(&h), "sample.pdf", "the window's document is active");
    // Reattach: every tab comes back.
    run(&mut h, "window.reattach");
    h.run_steps(3);
    assert_eq!(detach::main_window_docs(&h.state().state).len(), 3);
    // Detach a copy (what Ctrl-drag does): the main window keeps the tab.
    run(&mut h, "window.detach_copy");
    h.run_steps(3);
    let st = &h.state().state;
    assert_eq!(st.shell.extra.detached.len(), 1);
    assert_eq!(detach::main_window_docs(st).len(), 3, "a copy leaves the tab here");
}

// ---------------------------------------------------------------- thumbnails

fn geoms(h: &Harness<'_, MarkupCraftApp>) -> Vec<markupcraft_render::PageGeom> {
    h.state().state.doc().unwrap().render.as_ref().unwrap().pages().to_vec()
}

fn labels(h: &Harness<'_, MarkupCraftApp>) -> Vec<String> {
    geoms(h).into_iter().map(|g| g.label).collect()
}

/// A four-page labelled set: A-101 .. A-104 with one bookmark per page.
fn labelled_set(dir: &Path, name: &str) -> PathBuf {
    sample_pdf(dir, name);
    let mut a = automation(dir);
    call(&mut a, "doc_open", json!({ "path": name }));
    call(&mut a, "page_insert_blank", json!({ "at": 3, "count": 2 }));
    call(
        &mut a,
        "page_label_set",
        json!({ "labels": { "1": "A-101", "2": "A-102", "3": "A-103", "4": "A-104" } }),
    );
    call(
        &mut a,
        "bookmark_create",
        json!({ "titles": "numbers", "replace": true }),
    );
    call(&mut a, "doc_save", json!({ "full": true }));
    dir.join(name)
}

/// Bring the Thumbnails panel to the front (and close the Markups list, whose page column
/// repeats the labels).
fn show_thumbs(h: &mut Harness<'static, MarkupCraftApp>) {
    if h.state().state.open_panels.contains(&"markups") {
        run(h, "panel.markups");
    }
    h.state_mut().state.show_panel("thumbnails");
    h.run_steps(6);
}

/// The middle of the thumbnail picture above a label.
fn thumb_at(h: &Harness<'static, MarkupCraftApp>, label: &str) -> egui::Pos2 {
    let r = h.get_by_label(label).rect();
    egui::pos2(r.center().x, r.top() - 20.0)
}

fn thumb_click(h: &mut Harness<'static, MarkupCraftApp>, label: &str, m: Modifiers) {
    let at = thumb_at(h, label);
    h.hover_at(at);
    h.step();
    h.event(egui::Event::ModifiersChanged(m));
    h.step();
    button(h, at, true, m);
    button(h, at, false, m);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
}

fn thumbs<'a>(
    h: &'a mut Harness<'static, MarkupCraftApp>,
) -> &'a mut markupcraft_ui_egui::panels::thumbnails::ThumbState {
    &mut h.state_mut().state.shell.thumbs
}

/// D-057 the Thumbnails panel: click to go to a page; arrows move. D-058 size slider.
/// D-059 page label and page scale under each thumbnail. D-067 page labels shown and used.
#[test]
fn thumbnails_go_to_pages_resize_and_show_labels_and_scales() {
    let dir = temp_dir("doca-thumbs");
    let p = labelled_set(&dir, "set.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "set.pdf" }));
    call(
        &mut a,
        "scale_set",
        json!({ "pages": [2], "scale": { "kind": "architectural", "paper_inches": 0.25, "real_feet": 1 } }),
    );
    call(&mut a, "doc_save", json!({}));
    let mut h = empty_app();
    open(&mut h, &p);
    run(&mut h, "view.single_page");
    let was = h.state().state.open_panels.contains(&"thumbnails");
    key(&mut h, Modifiers::ALT, Key::T);
    h.run_steps(6);
    assert_ne!(
        h.state().state.open_panels.contains(&"thumbnails"),
        was,
        "Alt+T toggles the panel"
    );
    show_thumbs(&mut h);
    assert_eq!(labels(&h), ["A-101", "A-102", "A-103", "A-104"]);
    assert!(shows(&h, "A-103"), "labels under the thumbnails");
    thumb_click(&mut h, "A-103", Modifiers::NONE);
    assert_eq!(page(&h), 2, "click a thumbnail to go there");
    key(&mut h, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(page(&h), 3, "Down moves to the next page");
    key(&mut h, Modifiers::NONE, Key::ArrowUp);
    assert_eq!(page(&h), 2, "Up moves back");
    // Scale under the thumbnail.
    thumbs(&mut h).show_scale = true;
    h.run_steps(4);
    assert!(shows(&h, "A-102  |  0.25 in = 1 ft"), "page scale shown under page 2");
    assert!(shows(&h, "A-103  |  Scale Not Set"), "no scale on page 3");
    thumbs(&mut h).show_label = false;
    h.run_steps(4);
    assert!(!shows(&h, "A-104"), "labels hidden");
    // The size slider.
    assert!(
        h.query_all_by_role(egui::accesskit::Role::Slider).next().is_some(),
        "size slider"
    );
    let small = thumbs(&mut h).size;
    thumbs(&mut h).size = small * 2.0;
    h.run_steps(4);
    assert!(thumbs(&mut h).size > small);
}

/// D-060 Shift/Ctrl+click select pages; page commands apply to all.
/// D-094 rotate page quick buttons (Shift+Alt+Plus / Minus) rotate the selected pages.
/// D-065 page commands from the thumbnail menu.
#[test]
fn thumbnail_selection_and_page_commands() {
    use markupcraft_ui_egui::panels::thumbnails;
    let dir = temp_dir("doca-thsel");
    let p = labelled_set(&dir, "sel.pdf");
    let mut h = empty_app();
    open(&mut h, &p);
    show_thumbs(&mut h);
    thumb_click(&mut h, "A-101", Modifiers::NONE);
    thumb_click(&mut h, "A-103", Modifiers::SHIFT);
    assert_eq!(
        h.state().state.shell.thumbs.selected,
        [0, 1, 2],
        "Shift+click selects a run"
    );
    thumb_click(&mut h, "A-102", CTRL);
    assert_eq!(h.state().state.shell.thumbs.selected, [0, 2], "Ctrl+click toggles one");
    // The quick buttons turn the current page (page 1 here)...
    key(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::Plus);
    h.run_steps(3);
    let rot: Vec<u16> = geoms(&h).iter().map(|g| g.rotation).collect();
    assert_eq!(rot, [90, 0, 0, 0], "Shift+Alt+Plus rotates the current page");
    key(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::Minus);
    let rot: Vec<u16> = geoms(&h).iter().map(|g| g.rotation).collect();
    assert_eq!(rot, [0, 0, 0, 0], "Shift+Alt+Minus turns it back");
    // ...or every page with "Rotate all Pages by Default".
    h.state_mut().state.shell.ui.extra.rotate_all_pages = true;
    key(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::Plus);
    let rot: Vec<u16> = geoms(&h).iter().map(|g| g.rotation).collect();
    assert_eq!(rot, [90, 90, 90, 90], "all pages by default");
    key(&mut h, Modifiers::SHIFT | Modifiers::ALT, Key::Minus);
    h.state_mut().state.shell.ui.extra.rotate_all_pages = false;
    // Menu commands on the selection.
    let sel = h.state().state.shell.thumbs.selected.clone();
    thumbnails::command(&mut h.state_mut().state, "rotate_cw", &sel);
    h.run_steps(3);
    assert_eq!(geoms(&h)[2].rotation, 90);
    thumbnails::command(&mut h.state_mut().state, "delete", &[3]);
    h.run_steps(3);
    assert_eq!(
        labels(&h),
        ["A-101", "A-102", "A-103"],
        "Delete from the thumbnail menu"
    );
    thumbnails::command(&mut h.state_mut().state, "rename", &[0]);
    h.run_steps(3);
    assert!(
        h.query_all_by_role(egui::accesskit::Role::TextInput).next().is_some(),
        "rename edits in place"
    );
}

/// D-061 drag thumbnails to reorder pages; bookmarks follow their pages.
/// D-062 cut / copy / paste pages within or between documents.
#[test]
fn thumbnails_reorder_by_drag_and_cut_copy_paste_pages() {
    use markupcraft_ui_egui::panels::thumbnails;
    let dir = temp_dir("doca-threorder");
    let p = labelled_set(&dir, "re.pdf");
    let q = labelled_set(&dir, "other.pdf");
    let mut h = empty_app();
    open(&mut h, &p);
    show_thumbs(&mut h);
    // Drag A-104 above A-101.
    // Drag A-102 above A-101 (into the upper part of the first thumbnail).
    let from = thumb_at(&h, "A-102");
    let to = thumb_at(&h, "A-101") - egui::vec2(0.0, 70.0);
    h.hover_at(from);
    h.step();
    button(&mut h, from, true, Modifiers::NONE);
    for i in 1..=12 {
        h.hover_at(from + (to - from) * (i as f32 / 12.0));
        h.step();
    }
    button(&mut h, to, false, Modifiers::NONE);
    h.run_steps(4);
    assert_eq!(
        labels(&h),
        ["A-102", "A-101", "A-103", "A-104"],
        "{}",
        h.state().state.status
    );
    run(&mut h, "file.save");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "re.pdf" }));
    let bm = call(&mut a, "bookmark_list", json!({}));
    let first = bm["bookmarks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["title"] == "Page 2")
        .unwrap();
    assert_eq!(first["page"].as_u64(), Some(1), "the bookmark followed its page: {bm}");
    // Copy page 2 of re.pdf, paste into other.pdf.
    thumbnails::command(&mut h.state_mut().state, "copy", &[1]);
    open(&mut h, &q);
    let before = geoms(&h).len();
    thumbnails::command(&mut h.state_mut().state, "paste", &[0]);
    h.run_steps(4);
    assert_eq!(
        geoms(&h).len(),
        before + 1,
        "pasted between documents: {}",
        h.state().state.status
    );
    // Cut within a document: the page moves.
    thumbnails::command(&mut h.state_mut().state, "cut", &[4]);
    h.run_steps(3);
    assert_eq!(geoms(&h).len(), before, "cut removes the page");
    thumbnails::command(&mut h.state_mut().state, "paste", &[0]);
    h.run_steps(3);
    assert_eq!(geoms(&h).len(), before + 1, "paste puts it back");
}

/// D-063 Copy Page to Snapshot (Ctrl+Alt+C), pasted with Ctrl+V.
#[test]
fn copy_page_to_snapshot_and_paste() {
    let mut h = app();
    let n = markups(&h).len();
    key(&mut h, CTRL | Modifiers::ALT, Key::C);
    h.run_steps(3);
    key(&mut h, CTRL, Key::V);
    h.run_steps(3);
    assert_eq!(markups(&h).len(), n + 1, "{}", h.state().state.status);
    let m = markups(&h).last().cloned().unwrap();
    assert!(
        format!("{:?}", m.kind).contains("Snapshot") || format!("{:?}", m.kind).contains("Image"),
        "{:?}",
        m.kind
    );
}

/// D-064 Set Scale from the thumbnails for one or many pages.
#[test]
fn set_scale_from_thumbnails_for_several_pages() {
    use markupcraft_ui_egui::panels::thumbnails;
    let dir = temp_dir("doca-thscale");
    let p = labelled_set(&dir, "sc.pdf");
    let mut h = empty_app();
    open(&mut h, &p);
    show_thumbs(&mut h);
    thumbnails::command(&mut h.state_mut().state, "set_scale", &[1, 2]);
    h.run_steps(4);
    assert!(shows(&h, "Scale"), "the scale dialog opens");
    // Same through the tool: several pages at once.
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "sc.pdf" }));
    call(
        &mut a,
        "scale_set",
        json!({ "pages": [2, 3], "scale": { "kind": "engineering", "feet_per_inch": 20 } }),
    );
    let info = call(&mut a, "doc_info", json!({}));
    let pages = info["page_list"].as_array().unwrap_or_else(|| panic!("{info}"));
    assert!(pages[1].to_string().contains("20"), "{}", pages[1]);
    assert!(pages[2].to_string().contains("20"));
    assert!(!pages[0].to_string().contains("20"));
}

/// D-066 PDFs dropped on Thumbnails are inserted (pages added) instead of opened.
#[test]
fn files_dropped_on_thumbnails_are_inserted() {
    let dir = temp_dir("doca-thdrop");
    let p = labelled_set(&dir, "base.pdf");
    let extra = sample_pdf(&dir, "extra.pdf");
    let mut h = empty_app();
    open(&mut h, &p);
    show_thumbs(&mut h);
    let at = thumb_at(&h, "A-102");
    h.hover_at(at);
    h.step();
    h.input_mut().dropped_files.push(std::sync::Arc::new(Dropped(extra)));
    h.run_steps(4);
    assert_eq!(h.state().state.docs.len(), 1, "not opened as a tab");
    assert_eq!(geoms(&h).len(), 6, "two pages inserted: {}", h.state().state.status);
}

// ---------------------------------------------------------------- page labels

fn tool_labels(a: &mut Automation) -> Vec<String> {
    let v = call(a, "page_labels", json!({}));
    let arr = v["labels"]
        .as_array()
        .or_else(|| v.as_array())
        .unwrap_or_else(|| panic!("{v}"))
        .clone();
    arr.iter()
        .map(|x| {
            x.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| x["label"].as_str().unwrap_or("").to_string())
        })
        .collect()
}

/// A three-page set with a sheet number printed at the same place on every page (A-101 ..
/// A-103 in the bottom-right corner) and a sheet title in the bottom-left corner.
fn titled_set(dir: &Path, name: &str) -> (PathBuf, [f64; 4], [f64; 4]) {
    let mut a = automation(dir);
    call(&mut a, "doc_new", json!({ "path": name, "pages": 3 }));
    call(
        &mut a,
        "bates_add",
        json!({ "prefix": "A-10", "digits": 1, "start": 1, "position": "footer_right", "font_size": 14 }),
    );
    call(
        &mut a,
        "header_footer_add",
        json!({ "footer_left": "FLOOR PLAN", "font_size": 14 }),
    );
    call(&mut a, "doc_save", json!({ "full": true }));
    call(&mut a, "doc_open", json!({ "path": name }));
    let hit = |a: &mut Automation, text: &str| -> [f64; 4] {
        let v = call(a, "text_search", json!({ "text": text, "pages": [1] }));
        let r = v["hits"][0]["rects"][0]
            .as_array()
            .unwrap_or_else(|| panic!("{text}: {v}"))
            .clone();
        let f: Vec<f64> = r.iter().map(|x| x.as_f64().unwrap()).collect();
        [f[0] - 30.0, f[1] - 6.0, f[2] + 30.0, f[3] + 6.0]
    };
    let num = hit(&mut a, "A-101");
    let title = hit(&mut a, "FLOOR PLAN");
    (dir.join(name), num, title)
}

/// D-067 PDF page labels are read and shown; styles (D, R, r, A, a), prefix and start.
/// D-069 Number Pages: style, prefix, start number for a range or a custom list.
/// D-070 style None + prefix = a text-only label; style None, no prefix = labels removed.
#[test]
fn page_labels_number_pages_text_labels_and_clearing() {
    let dir = temp_dir("doca-labels");
    sample_pdf(&dir, "l.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "l.pdf" }));
    call(&mut a, "page_insert_blank", json!({ "at": 3, "count": 3 }));
    call(
        &mut a,
        "page_label_number",
        json!({ "pages": "1-2", "style": "roman_lower", "start": 1 }),
    );
    call(
        &mut a,
        "page_label_number",
        json!({ "pages": "3,5", "style": "decimal", "prefix": "C-", "start": 5 }),
    );
    call(
        &mut a,
        "page_label_number",
        json!({ "pages": [4], "style": "alpha", "start": 2 }),
    );
    call(&mut a, "doc_save", json!({ "full": true }));
    call(&mut a, "doc_open", json!({ "path": "l.pdf" }));
    assert_eq!(tool_labels(&mut a), ["i", "ii", "C-5", "B", "C-6"]);
    // Shown in the app's navigation bar.
    let mut h = empty_app();
    open(&mut h, &dir.join("l.pdf"));
    assert_eq!(labels(&h), ["i", "ii", "C-5", "B", "C-6"]);
    key(&mut h, Modifiers::NONE, Key::End);
    assert!(shows(&h, "(C-6) of 5"), "label in the navigation bar");
    // Text-only label and clearing.
    call(
        &mut a,
        "page_label_number",
        json!({ "pages": [1], "style": "none", "prefix": "COVER" }),
    );
    let l = tool_labels(&mut a);
    assert_eq!(l[0], "COVER");
    call(
        &mut a,
        "page_label_number",
        json!({ "pages": [2], "style": "none", "prefix": "" }),
    );
    let l = tool_labels(&mut a);
    assert!(
        l[1].is_empty() || l[1] == "2",
        "style none without prefix removes the label: {l:?}"
    );
    call(&mut a, "page_label_clear", json!({}));
    let l = tool_labels(&mut a);
    assert!(
        l.iter()
            .enumerate()
            .all(|(i, s)| s.is_empty() || *s == (i + 1).to_string()),
        "{l:?}"
    );
    // The thumbnail menu opens Number Pages.
    let mut h = empty_app();
    open(&mut h, &dir.join("l.pdf"));
    markupcraft_ui_egui::panels::thumbnails::command(&mut h.state_mut().state, "number", &[0, 1]);
    h.run_steps(4);
    assert!(shows(&h, "Number Pages"), "Number Pages dialog");
}

/// D-068 rename a label in place: F2 or double-click; Tab / Shift+Tab move on while editing.
#[test]
fn rename_page_labels_inline_with_tab_to_move_on() {
    let dir = temp_dir("doca-rename");
    let p = labelled_set(&dir, "rn.pdf");
    let mut h = empty_app();
    open(&mut h, &p);
    show_thumbs(&mut h);
    let at = thumb_at(&h, "A-101");
    h.hover_at(at);
    h.step();
    key(&mut h, Modifiers::NONE, Key::F2);
    h.run_steps(2);
    let edit = |h: &mut Harness<'static, MarkupCraftApp>, text: &str| {
        key(h, CTRL, Key::A);
        h.event(egui::Event::Text(text.to_string()));
        h.step();
    };
    edit(&mut h, "S-001");
    key(&mut h, Modifiers::NONE, Key::Tab);
    h.run_steps(2);
    edit(&mut h, "S-002");
    h.event(egui::Event::ModifiersChanged(Modifiers::SHIFT));
    key(&mut h, Modifiers::SHIFT, Key::Tab);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
    edit(&mut h, "S-000");
    key(&mut h, Modifiers::NONE, Key::Enter);
    h.run_steps(3);
    assert_eq!(labels(&h), ["S-000", "S-002", "A-103", "A-104"]);
    // Double-click a label to edit it.
    let r = h.get_by_label("S-002").rect().center();
    h.hover_at(r);
    h.step();
    for _ in 0..2 {
        button(&mut h, r, true, Modifiers::NONE);
        button(&mut h, r, false, Modifiers::NONE);
    }
    h.run_steps(2);
    edit(&mut h, "S-003");
    key(&mut h, Modifiers::NONE, Key::Enter);
    h.run_steps(3);
    assert_eq!(labels(&h)[1], "S-003", "double-click renames");
}

/// D-071 each bookmarked page takes its bookmark's title as its label.
/// D-072 labels from title-block regions (AutoMark): several boxes, literal text before,
/// between and after, a preview before applying.
#[test]
fn labels_from_bookmarks_and_from_page_regions() {
    let dir = temp_dir("doca-lbm");
    let (_, num, title) = titled_set(&dir, "t.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "t.pdf" }));
    call(&mut a, "bookmark_add", json!({ "page": 1, "title": "Cover" }));
    call(&mut a, "bookmark_add", json!({ "page": 3, "title": "Details" }));
    call(&mut a, "page_label_from_bookmarks", json!({}));
    let l = tool_labels(&mut a);
    assert_eq!(l[0], "Cover");
    assert_eq!(l[2], "Details");
    assert_ne!(l[1], "Cover", "a page without a bookmark keeps its label");
    // AutoMark: preview, then apply.
    let v = call(
        &mut a,
        "page_labels_from_region",
        json!({ "regions": [num, title], "before": "[", "between": " | ", "after": "]" }),
    );
    assert!(v.to_string().contains("[A-102 | FLOOR PLAN]"), "{v}");
    assert_eq!(tool_labels(&mut a)[0], "Cover", "a preview changes nothing");
    call(
        &mut a,
        "page_labels_from_region",
        json!({ "regions": [num], "apply": true }),
    );
    assert_eq!(tool_labels(&mut a), ["A-101", "A-102", "A-103"]);
    // The app has the dialog.
    let mut h = empty_app();
    open(&mut h, &dir.join("t.pdf"));
    run(&mut h, "document.region_labels");
    h.run_steps(4);
    assert!(
        shows(&h, "Labels from Region") || shows(&h, "Page Labels"),
        "dialog opens"
    );
}

// ---------------------------------------------------------------- bookmarks

fn bookmarks(a: &mut Automation) -> Vec<Value> {
    call(a, "bookmark_list", json!({}))["bookmarks"]
        .as_array()
        .unwrap()
        .clone()
}

fn titles(a: &mut Automation) -> Vec<String> {
    bookmarks(a)
        .iter()
        .map(|b| b["title"].as_str().unwrap().to_string())
        .collect()
}

/// D-073 the Bookmarks panel (Alt+B): a tree; clicking goes to the page. D-074 Ctrl+B adds a
/// bookmark to the current view named after the page label, ready to edit; Before / After /
/// Child. D-075 rename (F2) and delete (Del).
#[test]
fn bookmarks_panel_add_rename_and_delete() {
    let dir = temp_dir("doca-bm");
    let p = labelled_set(&dir, "bm.pdf");
    let mut h = empty_app();
    open(&mut h, &p);
    run(&mut h, "view.single_page");
    if h.state().state.open_panels.contains(&"markups") {
        run(&mut h, "panel.markups");
    }
    h.state_mut().state.show_panel("bookmarks");
    h.run_steps(6);
    assert!(shows(&h, "Page 3"), "bookmark tree shown");
    h.get_by_label("Page 3").click();
    h.run_steps(4);
    assert_eq!(page(&h), 2, "clicking a bookmark goes to its page");
    let was = h.state().state.open_panels.contains(&"bookmarks");
    key(&mut h, Modifiers::ALT, Key::B);
    h.run_steps(4);
    assert_ne!(h.state().state.open_panels.contains(&"bookmarks"), was, "Alt+B");
    h.state_mut().state.show_panel("bookmarks");
    h.run_steps(4);
    // Ctrl+B on page 4: a bookmark titled by the label, in edit mode.
    key(&mut h, Modifiers::NONE, Key::End);
    key(&mut h, CTRL, Key::B);
    h.run_steps(4);
    assert!(
        h.query_all_by_role(egui::accesskit::Role::TextInput)
            .any(|n| format!("{n:?}").contains("A-104")),
        "the new bookmark is named after the label and editable"
    );
    key(&mut h, CTRL, Key::A);
    h.event(egui::Event::Text("Roof plan".into()));
    h.step();
    key(&mut h, Modifiers::NONE, Key::Enter);
    h.run_steps(3);
    assert!(shows(&h, "Roof plan"));
    run(&mut h, "file.save");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "bm.pdf" }));
    let t = titles(&mut a);
    assert!(t.contains(&"Roof plan".to_string()), "{t:?}");
    // Before / After / Child through the tool, then rename and delete.
    call(
        &mut a,
        "bookmark_add",
        json!({ "page": 1, "title": "Before 1", "index": 1 }),
    );
    call(
        &mut a,
        "bookmark_add",
        json!({ "page": 2, "title": "After 1", "index": 3 }),
    );
    call(
        &mut a,
        "bookmark_add",
        json!({ "page": 2, "title": "Child", "parent": [3] }),
    );
    let b = bookmarks(&mut a);
    assert_eq!(b[0]["title"], "Before 1");
    assert_eq!(b[1]["title"], "Page 1");
    assert_eq!(b[2]["title"], "After 1");
    assert_eq!(b[3]["title"], "Child");
    assert_eq!(b[3]["depth"].as_u64(), Some(1), "{}", b[3]);
    call(&mut a, "bookmark_edit", json!({ "path": [1], "title": "Front" }));
    call(&mut a, "bookmark_delete", json!({ "path": [3] }));
    let t = titles(&mut a);
    assert_eq!(t[0], "Front");
    assert!(
        !t.contains(&"After 1".to_string()) && !t.contains(&"Child".to_string()),
        "{t:?}"
    );
    // In the panel: F2 renames, Del deletes the selected bookmark.
    let mut h = empty_app();
    open(&mut h, &p);
    if h.state().state.open_panels.contains(&"markups") {
        run(&mut h, "panel.markups");
    }
    h.state_mut().state.show_panel("bookmarks");
    h.run_steps(6);
    h.get_by_label("Page 2").click();
    h.run_steps(3);
    key(&mut h, Modifiers::NONE, Key::F2);
    key(&mut h, CTRL, Key::A);
    h.event(egui::Event::Text("Second".into()));
    h.step();
    key(&mut h, Modifiers::NONE, Key::Enter);
    h.run_steps(3);
    assert!(shows(&h, "Second"), "F2 renames");
    h.get_by_label("Second").click();
    h.run_steps(3);
    key(&mut h, Modifiers::NONE, Key::Delete);
    h.run_steps(3);
    assert!(!shows(&h, "Second"), "Del deletes");
}

/// D-076 reorder, nest (drag inward) and copy bookmarks. D-077 colour, bold and italic for
/// one or many. D-083 the expand / collapse state is stored in the file.
#[test]
fn bookmarks_reorder_nest_copy_style_and_collapse_state() {
    let dir = temp_dir("doca-bm2");
    labelled_set(&dir, "b2.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "b2.pdf" }));
    call(&mut a, "bookmark_move", json!({ "path": [4], "index": 1 }));
    assert_eq!(titles(&mut a), ["Page 4", "Page 1", "Page 2", "Page 3"]);
    call(&mut a, "bookmark_move", json!({ "path": [3], "parent": [2] }));
    let b = bookmarks(&mut a);
    assert_eq!(b[2]["title"], "Page 2");
    assert_eq!(b[2]["depth"].as_u64(), Some(1), "nested under Page 1");
    call(&mut a, "bookmark_copy", json!({ "path": [2] }));
    let t = titles(&mut a);
    assert_eq!(t.iter().filter(|x| *x == "Page 1").count(), 2, "{t:?}");
    assert_eq!(
        t.iter().filter(|x| *x == "Page 2").count(),
        2,
        "children copied too: {t:?}"
    );
    call(
        &mut a,
        "bookmark_style",
        json!({ "paths": [[1], [2]], "color": "#C00000", "bold": true, "italic": true }),
    );
    call(&mut a, "bookmark_edit", json!({ "path": [2], "open": false }));
    call(&mut a, "doc_save", json!({ "full": true }));
    call(&mut a, "doc_open", json!({ "path": "b2.pdf" }));
    let d = call(&mut a, "bookmark_action", json!({ "path": [2] }));
    let s = d.to_string().to_lowercase();
    assert!(s.contains("c00000") || s.contains("0.75"), "colour kept: {d}");
    assert!(s.contains("\"bold\":true") && s.contains("\"italic\":true"), "{d}");
    let b = bookmarks(&mut a);
    assert_eq!(b[1]["open"], json!(false), "collapse state saved: {}", b[1]);
    // The app keeps it collapsed: the child is hidden.
    let mut h = empty_app();
    open(&mut h, &dir.join("b2.pdf"));
    h.state_mut().state.show_panel("bookmarks");
    h.run_steps(6);
    assert_eq!(h.query_all_by_label("Page 2").count(), 1, "only the copy's child shows");
}

/// D-078 bookmark actions: page with zoom, Place, snapshot rectangle, URL, file (relative).
#[test]
fn bookmark_actions_target_pages_views_urls_and_files() {
    let dir = temp_dir("doca-bm3");
    labelled_set(&dir, "b3.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "b3.pdf" }));
    call(
        &mut a,
        "bookmark_action",
        json!({ "path": [1], "to_page": 3, "zoom": "fit_width" }),
    );
    call(
        &mut a,
        "bookmark_action",
        json!({ "path": [2], "url": "https://example.com/rfi" }),
    );
    call(
        &mut a,
        "bookmark_action",
        json!({ "path": [3], "file": "specs/spec.pdf", "file_page": 2, "relative": true }),
    );
    call(
        &mut a,
        "bookmark_action",
        json!({ "path": [4], "to_page": 1, "view": [100, 100, 400, 300] }),
    );
    call(&mut a, "doc_save", json!({ "full": true }));
    call(&mut a, "doc_open", json!({ "path": "b3.pdf" }));
    let d1 = call(&mut a, "bookmark_action", json!({ "path": [1] })).to_string();
    assert!(d1.contains('3') && d1.to_lowercase().contains("width"), "{d1}");
    let d2 = call(&mut a, "bookmark_action", json!({ "path": [2] })).to_string();
    assert!(d2.contains("https://example.com/rfi"), "{d2}");
    let d3 = call(&mut a, "bookmark_action", json!({ "path": [3] })).to_string();
    assert!(d3.contains("spec.pdf"), "{d3}");
    let d4 = call(&mut a, "bookmark_action", json!({ "path": [4] })).to_string();
    assert!(d4.contains("400"), "{d4}");
    // In the app, the fit-width bookmark goes to page 3 at Fit Width.
    let mut h = empty_app();
    open(&mut h, &dir.join("b3.pdf"));
    run(&mut h, "view.single_page");
    h.state_mut().state.show_panel("bookmarks");
    h.run_steps(6);
    h.get_by_label("Page 1").click();
    h.run_steps(4);
    assert_eq!(page(&h), 2);
    assert_eq!(view(&h).fit, markupcraft_ui_egui::canvas::Fit::Width);
}

/// D-079 create bookmarks from page labels or title-block regions (AutoMark), for a range.
/// D-080 bookmark structures file bookmarks into folders. D-081 Audit finds broken ones.
#[test]
fn bookmarks_created_automatically_structured_and_audited() {
    let dir = temp_dir("doca-bm4");
    let (_, num, _) = titled_set(&dir, "b4.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "b4.pdf" }));
    call(&mut a, "bookmark_automark", json!({ "region": num, "replace": true }));
    assert_eq!(titles(&mut a), ["A-101", "A-102", "A-103"]);
    call(
        &mut a,
        "page_label_set",
        json!({ "labels": { "1": "S-1", "2": "S-2", "3": "M-1" } }),
    );
    call(
        &mut a,
        "bookmark_create",
        json!({ "titles": "labels", "pages": "1-2", "replace": true }),
    );
    assert_eq!(titles(&mut a), ["S-1", "S-2"]);
    call(&mut a, "bookmark_create", json!({ "titles": "labels", "pages": [3] }));
    call(
        &mut a,
        "bookmark_structure",
        json!({ "folders": [{ "title": "Structural", "prefixes": ["S-"] }, { "title": "Mechanical", "prefixes": ["M-"] }],
                "save": "struct.json" }),
    );
    let b = bookmarks(&mut a);
    let pos = |t: &str| b.iter().position(|x| x["title"] == t).unwrap();
    assert_eq!(b[pos("S-1")]["depth"].as_u64(), Some(1), "{b:?}");
    assert!(pos("Structural") < pos("S-1") && pos("Mechanical") < pos("M-1"));
    assert!(dir.join("struct.json").is_file(), "the structure is saved for reuse");
    // Audit: delete page 3; its bookmark is broken.
    assert!(
        call(&mut a, "bookmark_audit", json!({}))["broken"]
            .as_array()
            .is_none_or(Vec::is_empty)
    );
    call(&mut a, "page_delete", json!({ "pages": [3] }));
    let au = call(&mut a, "bookmark_audit", json!({}));
    assert!(au.to_string().contains("M-1"), "{au}");
}

/// D-082 export bookmarks of one or many PDFs to CSV or PDF: tree or flat, top level only,
/// hyperlinks, date stamp.
#[test]
fn bookmarks_export_to_csv_and_pdf() {
    let dir = temp_dir("doca-bm5");
    labelled_set(&dir, "e1.pdf");
    labelled_set(&dir, "e2.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "e1.pdf" }));
    call(
        &mut a,
        "bookmark_add",
        json!({ "page": 2, "title": "Nested", "parent": [1] }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "bookmark_export", json!({ "out": "flat.csv", "tree": false }));
    let csv = std::fs::read_to_string(dir.join("flat.csv")).unwrap();
    assert!(csv.contains("Page 1") && csv.contains("Nested"), "{csv}");
    call(
        &mut a,
        "bookmark_export",
        json!({ "out": "top.csv", "top_level_only": true }),
    );
    let top = std::fs::read_to_string(dir.join("top.csv")).unwrap();
    assert!(!top.contains("Nested"), "{top}");
    call(
        &mut a,
        "bookmark_export",
        json!({ "files": ["e1.pdf", "e2.pdf"], "out": "report.pdf", "tree": true, "links": true, "date_stamp": true, "paper": "Letter" }),
    );
    call(&mut a, "doc_open", json!({ "path": "report.pdf" }));
    let mut text = String::new();
    let pages = call(&mut a, "doc_info", json!({}))["page_list"]
        .as_array()
        .unwrap()
        .len();
    for p in 1..=pages {
        text.push_str(
            call(&mut a, "page_text", json!({ "page": p }))["text"]
                .as_str()
                .unwrap_or(""),
        );
    }
    assert!(
        text.contains("e1") && text.contains("e2") && text.contains("Nested"),
        "{text}"
    );
    let links = call(&mut a, "link_list", json!({}));
    assert!(links.to_string().len() > 20, "linked report: {links}");
}

/// D-084 bookmarks the way the Office / CAD plugins make them: from headings.
#[test]
fn bookmarks_from_document_headings() {
    let dir = temp_dir("doca-bm6");
    sample_pdf(&dir, "h.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "h.pdf" }));
    let p = call(&mut a, "bookmarks_from_source", json!({ "preview": true }));
    assert!(p.to_string().contains("GENERAL NOTES"), "{p}");
    let before = titles(&mut a);
    assert!(!before.iter().any(|t| t.contains("GENERAL NOTES")));
    call(&mut a, "bookmarks_from_source", json!({ "replace": true }));
    let t = titles(&mut a);
    assert!(t.iter().any(|t| t.contains("GENERAL NOTES")), "{t:?}");
}

// ---------------------------------------------------------------- page operations

fn page_texts(a: &mut Automation) -> Vec<String> {
    let n = call(a, "doc_info", json!({}))["page_list"].as_array().unwrap().len();
    (1..=n)
        .map(|p| {
            call(a, "page_text", json!({ "page": p }))["text"]
                .as_str()
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

/// A PDF whose pages say "<tag> 1", "<tag> 2", ... (header text), `n` pages.
fn tagged(dir: &Path, name: &str, tag: &str, n: u32) -> PathBuf {
    let mut a = automation(dir);
    call(&mut a, "doc_new", json!({ "path": name, "pages": n }));
    call(
        &mut a,
        "header_footer_add",
        json!({ "header_left": format!("{tag} <<1>>"), "font_size": 12 }),
    );
    call(&mut a, "doc_save", json!({ "full": true }));
    dir.join(name)
}

fn tags(a: &mut Automation) -> Vec<String> {
    page_texts(a)
        .iter()
        .map(|t| t.split_whitespace().take(2).collect::<Vec<_>>().join(" "))
        .collect()
}

/// D-085 insert pages from one or more PDFs with per-file ranges at a chosen position.
/// D-086 insert options: bookmarks, attachments, properties, layers, labels from the file
/// name, interleave (odd/even rejoin).
#[test]
fn insert_pages_from_files_and_insert_options() {
    let dir = temp_dir("doca-insert");
    tagged(&dir, "base.pdf", "BASE", 3);
    tagged(&dir, "x.pdf", "X", 3);
    tagged(&dir, "y.pdf", "Y", 2);
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "base.pdf" }));
    call(
        &mut a,
        "page_insert_files",
        json!({ "at": 2, "files": [{ "path": "x.pdf", "pages": "2-3" }, { "path": "y.pdf", "pages": [1] }] }),
    );
    assert_eq!(tags(&mut a), ["BASE 1", "X 2", "X 3", "Y 1", "BASE 2", "BASE 3"]);
    // Options: interleave odd and even scans; labels from the file name; bookmarks.
    tagged(&dir, "odd.pdf", "ODD", 3);
    tagged(&dir, "even.pdf", "EVEN", 3);
    call(&mut a, "doc_open", json!({ "path": "odd.pdf" }));
    call(&mut a, "doc_properties_set", json!({ "properties": { "Title": "" } }));
    call(
        &mut a,
        "pages_insert_with",
        json!({ "path": "even.pdf", "interleave": true, "labels_from_name": true, "bookmarks": true, "properties": true }),
    );
    assert_eq!(tags(&mut a), ["ODD 1", "EVEN 1", "ODD 2", "EVEN 2", "ODD 3", "EVEN 3"]);
    let l = tool_labels(&mut a);
    assert!(l[1].contains("even"), "labels from the file name: {l:?}");
    let t = titles(&mut a);
    assert!(
        t.iter().any(|t| t.contains("even")),
        "a bookmark named after the file: {t:?}"
    );
    // Attachments come along.
    tagged(&dir, "att.pdf", "ATT", 1);
    std::fs::write(dir.join("note.txt"), "attached").unwrap();
    call(&mut a, "doc_open", json!({ "path": "att.pdf" }));
    call(&mut a, "attachment_add", json!({ "path": "note.txt" }));
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "base.pdf" }));
    call(
        &mut a,
        "pages_insert_with",
        json!({ "path": "att.pdf", "attachments": true }),
    );
    let at = call(&mut a, "attachment_list", json!({}));
    assert!(at.to_string().contains("note.txt"), "{at}");
    // The app's Insert Pages command (Ctrl+Shift+I) asks for the files.
    assert_eq!(shortcut("document.insert_pages").as_deref(), Some("Ctrl+Shift+I"));
}

/// D-087 insert N blank pages: size, orientation, grid, template, position (Ctrl+Shift+N).
#[test]
fn insert_blank_pages_sized_ruled_or_from_a_template() {
    let dir = temp_dir("doca-blank");
    tagged(&dir, "b.pdf", "B", 2);
    tagged(&dir, "tpl.pdf", "TEMPLATE", 1);
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "b.pdf" }));
    call(
        &mut a,
        "page_insert_blank_styled",
        json!({ "at": 2, "count": 2, "width": 1224, "height": 792, "grid_spacing": 18 }),
    );
    let info = call(&mut a, "doc_info", json!({}));
    let p = info["page_list"].as_array().unwrap();
    assert_eq!(p.len(), 4);
    assert_eq!(p[1]["width"].as_f64(), Some(1224.0), "11 x 17 landscape");
    assert_eq!(p[2]["height"].as_f64(), Some(792.0));
    call(
        &mut a,
        "page_insert_blank_styled",
        json!({ "at": 5, "count": 1, "template": "tpl.pdf" }),
    );
    let t = tags(&mut a);
    assert_eq!(t[4], "TEMPLATE 1", "{t:?}");
    // Ctrl+Shift+N in the app inserts after the current page.
    let mut h = app();
    let n = geoms(&h).len();
    key(&mut h, CTRL_SHIFT, Key::N);
    assert_eq!(geoms(&h).len(), n + 1);
}

/// D-088 another PDF's pages become new layers on existing pages.
/// D-108 a layered PDF built from several PDFs, one layer per file named after it.
#[test]
fn layered_pages_and_layered_pdfs() {
    let dir = temp_dir("doca-layer");
    tagged(&dir, "plan.pdf", "PLAN", 2);
    tagged(&dir, "mep.pdf", "MEP", 2);
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "plan.pdf" }));
    call(
        &mut a,
        "pages_insert_layered",
        json!({ "path": "mep.pdf", "name": "MEP overlay" }),
    );
    let l = call(&mut a, "layer_list", json!({}));
    assert!(l.to_string().contains("MEP overlay"), "{l}");
    let t = page_texts(&mut a);
    assert!(t[1].len() > "PLAN 2".len() + 4, "page 2 carries both drawings: {t:?}");
    assert_eq!(t.len(), 2, "no pages added");
    call(
        &mut a,
        "doc_layered",
        json!({ "files": ["plan.pdf", "mep.pdf"], "out": "layered.pdf" }),
    );
    call(&mut a, "doc_open", json!({ "path": "layered.pdf" }));
    let l = call(&mut a, "layer_list", json!({})).to_string();
    assert!(l.contains("plan") && l.contains("mep"), "{l}");
    let pages = call(&mut a, "doc_info", json!({}))["page_list"]
        .as_array()
        .unwrap()
        .len();
    // Every page of the sources is layered, page for page.
    assert_eq!(pages, 2);
}

/// D-089 / D-107 pages from a scanner or camera into the document or a new PDF.
#[test]
fn pages_from_the_camera_or_a_scanner() {
    let dir = temp_dir("doca-scan");
    tagged(&dir, "s.pdf", "S", 2);
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "s.pdf" }));
    call(
        &mut a,
        "camera_capture",
        json!({ "device": "test", "into": "pages", "at": 2, "width": 640, "height": 480 }),
    );
    let n = call(&mut a, "doc_info", json!({}))["page_list"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(n, 3, "the picture is a new page");
    call(
        &mut a,
        "camera_capture",
        json!({ "device": "test", "into": "pdf", "out": "photo.pdf" }),
    );
    assert_eq!(page_count(&mut a, "photo.pdf"), 1);
    // No scanner here: a clear error, no crash.
    let e = fails(
        &mut a,
        "scan",
        json!({ "action": "capabilities", "url": "http://127.0.0.1:9/eSCL" }),
    );
    assert!(!e.is_empty());
    let mut h = app();
    run(&mut h, "document.insert_scanner");
    h.run_steps(4);
    assert!(
        shows(&h, "Scan") || shows(&h, "Camera"),
        "the Insert from Scanner or Camera dialog"
    );
}

/// D-090 extract a range to one file or one file per page (named by label), delete after,
/// open after (Ctrl+Shift+X).
#[test]
fn extract_pages_to_one_file_or_one_per_page() {
    let dir = temp_dir("doca-extract");
    labelled_set(&dir, "ex.pdf");
    std::fs::create_dir_all(dir.join("each")).unwrap();
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "ex.pdf" }));
    call(&mut a, "page_extract", json!({ "pages": "2-3", "out": "part.pdf" }));
    assert_eq!(page_count(&mut a, "part.pdf"), 2);
    call(&mut a, "doc_open", json!({ "path": "ex.pdf" }));
    call(
        &mut a,
        "page_extract_each",
        json!({ "pages": "1-2", "dir": "each", "by_label": true }),
    );
    assert!(dir.join("each").join("A-101.pdf").is_file());
    assert!(dir.join("each").join("A-102.pdf").is_file());
    call(
        &mut a,
        "page_extract_each",
        json!({ "pages": [1], "dir": "each", "by_label": true }),
    );
    assert!(
        dir.join("each").join("A-101 (2).pdf").is_file(),
        "no overwrite: a new name"
    );
    call(
        &mut a,
        "page_extract_each",
        json!({ "pages": [1], "dir": "each", "by_label": true, "overwrite": true }),
    );
    assert!(!dir.join("each").join("A-101 (3).pdf").exists());
    call(
        &mut a,
        "page_extract",
        json!({ "pages": [4], "out": "last.pdf", "delete": true }),
    );
    assert_eq!(
        tool_labels(&mut a),
        ["A-101", "A-102", "A-103"],
        "deleted after extracting"
    );
    assert_eq!(shortcut("document.extract_pages").as_deref(), Some("Ctrl+Shift+X"));
}

/// D-091 replace pages; "content only" keeps the old page's markups and links (slip sheet).
/// D-092 delete a range (Ctrl+Shift+D opens Delete Pages, as in Revu).
#[test]
fn replace_pages_keeps_markups_and_delete_pages() {
    let dir = temp_dir("doca-replace");
    sample_pdf(&dir, "old.pdf");
    tagged(&dir, "new.pdf", "REV2", 2);
    let mut a = automation(&dir);
    let markups_before = markup_count(&mut a, "old.pdf");
    call(&mut a, "doc_open", json!({ "path": "old.pdf" }));
    call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [10, 10, 60, 60], "url": "https://example.com" }),
    );
    call(
        &mut a,
        "page_replace",
        json!({ "pages": [1], "path": "new.pdf", "source_pages": [2] }),
    );
    call(&mut a, "doc_save", json!({}));
    call(&mut a, "doc_open", json!({ "path": "old.pdf" }));
    let t = page_texts(&mut a);
    assert!(t[0].contains("REV2 2"), "the new drawing: {}", t[0]);
    assert_eq!(markup_count(&mut a, "old.pdf"), markups_before, "markups kept");
    call(&mut a, "doc_open", json!({ "path": "old.pdf" }));
    assert!(
        call(&mut a, "link_list", json!({})).to_string().contains("example.com"),
        "links kept"
    );
    assert_eq!(shortcut("document.replace_pages").as_deref(), Some("Ctrl+Shift+Y"));
    // Delete a range.
    tagged(&dir, "del.pdf", "D", 5);
    call(&mut a, "doc_open", json!({ "path": "del.pdf" }));
    call(&mut a, "page_delete", json!({ "pages": "2-3, 5" }));
    assert_eq!(tags(&mut a), ["D 1", "D 4"]);
    let mut h = app();
    let n = geoms(&h).len();
    key(&mut h, CTRL_SHIFT, Key::D);
    assert_eq!(geoms(&h).len(), n, "Ctrl+Shift+D asks first, it never deletes at once");
    assert!(shows(&h, "Delete Pages"), "Ctrl+Shift+D opens the Delete Pages dialog");
}

/// D-093 Rotate Pages dialog: 90 / 180 on all, current, even, odd, landscape, portrait.
/// D-099 the shared page range picker: all, current, selected, even, odd, landscape,
/// portrait, first, last, a custom list.
#[test]
fn rotate_pages_dialog_and_the_page_range_picker() {
    let dir = temp_dir("doca-rotate");
    let p = labelled_set(&dir, "rot.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "rot.pdf" }));
    call(&mut a, "page_rotate", json!({ "pages": "2, 4", "degrees": 180 }));
    let r: Vec<i64> = call(&mut a, "doc_info", json!({}))["page_list"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["rotate"].as_i64().unwrap())
        .collect();
    assert_eq!(r, [0, 180, 0, 180]);
    // Across many (closed) files at once, saved in place.
    tagged(&dir, "m1.pdf", "M", 2);
    tagged(&dir, "m2.pdf", "N", 1);
    call(
        &mut a,
        "batch_apply",
        json!({ "files": ["m1.pdf", "m2.pdf"], "operations": [{ "tool": "page_rotate", "args": { "degrees": 90 } }] }),
    );
    for f in ["m1.pdf", "m2.pdf"] {
        call(&mut a, "doc_open", json!({ "path": f }));
        let info = call(&mut a, "doc_info", json!({}));
        assert!(
            info["page_list"].as_array().unwrap().iter().all(|p| p["rotate"] == 90),
            "{f}: {info}"
        );
    }
    let mut h = empty_app();
    open(&mut h, &p);
    let rotations =
        |h: &Harness<'static, MarkupCraftApp>| -> Vec<u16> { geoms(h).iter().map(|g| g.rotation).collect() };
    for (choice, want) in [("Even pages", [0, 90, 0, 90]), ("Landscape pages", [90, 180, 0, 180])] {
        key(&mut h, CTRL_SHIFT, Key::R);
        h.run_steps(4);
        assert!(
            h.state().state.shell.page_dialog.is_some(),
            "Ctrl+Shift+R opens Rotate Pages"
        );
        let at = h
            .query_all_by_role(egui::accesskit::Role::ComboBox)
            .find(|n| n.value().as_deref() == Some("Current page"))
            .expect("the range picker")
            .rect()
            .center();
        h.hover_at(at);
        h.step();
        button(&mut h, at, true, Modifiers::NONE);
        button(&mut h, at, false, Modifiers::NONE);
        h.run_steps(3);
        for c in [
            "All pages",
            "Selected thumbnails",
            "Even pages",
            "Odd pages",
            "Landscape pages",
            "Portrait pages",
            "First page",
            "Last page",
            "Pages:",
        ] {
            assert!(shows(&h, c), "range choice {c}");
        }
        h.get_by_label(choice).click();
        h.run_steps(3);
        h.get_by_label("OK").click();
        h.run_steps(4);
        assert_eq!(rotations(&h), want, "{choice}");
    }
}

/// D-095 split a document by page count or top-level bookmarks.
#[test]
fn split_document_by_page_count_and_bookmarks() {
    let dir = temp_dir("doca-splitdoc");
    labelled_set(&dir, "sp.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "sp.pdf" }));
    std::fs::create_dir_all(dir.join("parts")).unwrap();
    std::fs::create_dir_all(dir.join("bm")).unwrap();
    call(&mut a, "doc_split", json!({ "dir": "parts", "pages_per_file": 3 }));
    let mut files: Vec<String> = std::fs::read_dir(dir.join("parts"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(files.len(), 2, "{files:?}");
    assert_eq!(page_count(&mut a, &format!("parts/{}", files[0])), 3);
    call(&mut a, "doc_open", json!({ "path": "sp.pdf" }));
    call(&mut a, "doc_split", json!({ "dir": "bm", "by": "bookmarks" }));
    let n = std::fs::read_dir(dir.join("bm")).unwrap().count();
    assert_eq!(n, 4, "one part per top-level bookmark");
}

/// D-095 split by file size (MB); names with a prefix / suffix where # is the part number, or
/// the bookmark names; a subfolder; links updated to the other parts; unused layers dropped.
#[test]
fn split_document_by_size_names_subfolder_links_and_layers() {
    let dir = temp_dir("doca-splitmore");
    labelled_set(&dir, "sp.pdf");
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "sp.pdf" }));
    call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [50, 50, 150, 80], "to_page": 4 }),
    );
    call(&mut a, "layer_create", json!({ "name": "Spare" }));
    call(&mut a, "doc_save", json!({ "full": true }));
    for d in ["size", "named", "bm", "linked"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    // by size: every part within the limit unless it is a single page
    let one_page = {
        call(&mut a, "doc_split", json!({ "dir": "linked", "pages_per_file": 1 }));
        std::fs::read_dir(dir.join("linked"))
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().len())
            .max()
            .unwrap()
    };
    for e in std::fs::read_dir(dir.join("linked")).unwrap() {
        std::fs::remove_file(e.unwrap().path()).unwrap();
    }
    let limit = (one_page + 64) as f64;
    let v = call(
        &mut a,
        "doc_split",
        json!({ "dir": "size", "by": "size", "max_mb": limit / 1_048_576.0, "prefix": "Part ##-", "subfolder": true }),
    );
    let files = v["files"].as_array().unwrap();
    assert!(files.len() >= 2 && files.len() <= 4, "{v}");
    for f in files {
        let p = std::path::PathBuf::from(f["path"].as_str().unwrap());
        assert!(p.parent().unwrap().ends_with("sp"), "in the subfolder: {p:?}");
        let pages = f["pages"].as_array().unwrap().len();
        assert!(
            pages == 1 || std::fs::metadata(&p).unwrap().len() as f64 <= limit,
            "{p:?} too large"
        );
    }
    assert!(dir.join("size/sp/Part 01-sp.pdf").exists(), "{v}");
    // suffix with # and the bookmark names
    let v = call(
        &mut a,
        "doc_split",
        json!({ "dir": "named", "pages_per_file": 2, "suffix": " (#)" }),
    );
    let vs = v.to_string();
    assert!(vs.contains("sp (1).pdf") && vs.contains("sp (2).pdf"), "{v}");
    call(
        &mut a,
        "doc_split",
        json!({ "dir": "bm", "by": "bookmarks", "bookmark_names": true }),
    );
    let mut names: Vec<String> = std::fs::read_dir(dir.join("bm"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert!(
        names.iter().any(|n| n == "Page 1.pdf"),
        "named after the bookmarks: {names:?}"
    );
    // links to a page in another part open that part; the unused layer is dropped
    let v = call(
        &mut a,
        "doc_split",
        json!({ "dir": "linked", "pages_per_file": 2, "update_links": true, "drop_empty_layers": true }),
    );
    let first = v["files"][0]["path"].as_str().unwrap().to_string();
    let second = v["files"][1]["path"].as_str().unwrap().to_string();
    call(&mut a, "doc_open", json!({ "path": first }));
    let l = call(&mut a, "link_list", json!({})).to_string();
    let second_name = std::path::Path::new(&second)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        l.contains(&second_name) && l.contains("\"file_page\":2"),
        "link goes to part 2, page 2: {l}"
    );
    let layers = call(&mut a, "layer_list", json!({})).to_string();
    assert!(!layers.contains("Spare"), "unused layer dropped: {layers}");
}

/// D-096 crop pages by margins (Shift+Alt+O). D-097 page setup: new media size, content
/// scaled to it or placed, offset, rotation. D-098 deskew by two points.
#[test]
fn crop_page_setup_and_deskew() {
    let dir = temp_dir("doca-crop");
    tagged(&dir, "c.pdf", "C", 2);
    let mut a = automation(&dir);
    call(&mut a, "doc_open", json!({ "path": "c.pdf" }));
    call(
        &mut a,
        "page_crop",
        json!({ "pages": [1], "margins": [36, 36, 36, 36] }),
    );
    let b = call(&mut a, "page_boxes", json!({ "pages": [1] })).to_string();
    assert!(b.contains("36.0") && b.contains("576.0") && b.contains("756.0"), "{b}");
    call(&mut a, "doc_save", json!({ "path": "cropped.pdf" }));
    let mut h = empty_app();
    open(&mut h, &dir.join("cropped.pdf"));
    assert_eq!(geoms(&h)[0].width, 540.0, "the page shows at the crop size");
    assert_eq!(shortcut("document.crop_pages").as_deref(), Some("Shift+Alt+O"));
    call(
        &mut a,
        "page_setup",
        json!({ "pages": [2], "width": 1224, "height": 792, "center": true }),
    );
    let info = call(&mut a, "doc_info", json!({}));
    assert_eq!(info["page_list"][1]["width"].as_f64(), Some(1224.0));
    assert!(page_texts(&mut a)[1].contains("C 2"), "content kept");
    call(
        &mut a,
        "page_deskew",
        json!({ "pages": [2], "from": [100, 100], "to": [500, 120] }),
    );
    let d = call(&mut a, "doc_info", json!({}));
    assert_eq!(d["page_list"].as_array().unwrap().len(), 2);
    assert_eq!(bound("Ctrl+Alt+D").as_deref(), Some("document.deskew"));
}

/// D-100 the shared batch file list: add files and folders, reorder, save / load the list.
#[test]
fn batch_file_list_adds_reorders_and_saves() {
    let dir = temp_dir("doca-batchlist");
    tagged(&dir, "one.pdf", "ONE", 1);
    tagged(&dir, "two.pdf", "TWO", 2);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    tagged(&dir.join("sub"), "three.pdf", "THREE", 1);
    let mut a = automation(&dir);
    std::fs::create_dir_all(dir.join("out")).unwrap();
    call(
        &mut a,
        "batch_split",
        json!({ "files": ["two.pdf", "one.pdf"], "dir": "out", "pages_per_file": 1 }),
    );
    let n = std::fs::read_dir(dir.join("out")).unwrap().count();
    assert_eq!(n, 3, "closed files processed");
    let mut h = app();
    run(&mut h, "batch.split");
    h.run_steps(4);
    for b in ["Add Files", "Add Folder", "Save List", "Load List"] {
        assert!(shows(&h, b), "{b}");
    }
}

/// D-101 page edits are blocked on certified or PDF/A files; on signed files the user is
/// warned that signatures will be cleared.
#[test]
fn signed_and_certified_documents_guard_page_edits() {
    let dir = temp_dir("doca-signed");
    tagged(&dir, "sig.pdf", "SIG", 2);
    let mut a = automation(&dir);
    call(
        &mut a,
        "digital_id_create",
        json!({ "name": "Test Signer", "out": "id.p12", "password": "test-only" }),
    );
    call(&mut a, "doc_open", json!({ "path": "sig.pdf" }));
    call(
        &mut a,
        "signature_sign",
        json!({ "id": "id.p12", "password": "test-only", "page": 1, "rect": [50, 50, 200, 100], "out": "signed.pdf" }),
    );
    call(&mut a, "doc_open", json!({ "path": "sig.pdf" }));
    call(
        &mut a,
        "signature_sign",
        json!({ "id": "id.p12", "password": "test-only", "page": 1, "rect": [50, 50, 200, 100], "out": "certified.pdf", "certify": 1 }),
    );
    // Signed: a page edit warns.
    let mut h = empty_app();
    open(&mut h, &dir.join("signed.pdf"));
    run(&mut h, "document.delete_page");
    h.run_steps(3);
    let st = h.state().state.status.to_lowercase();
    assert!(
        st.contains("signature") || shows(&h, "signature") || shows(&h, "Signature"),
        "warned: {st}"
    );
    // Certified: refused.
    let mut h = empty_app();
    open(&mut h, &dir.join("certified.pdf"));
    let n = geoms(&h).len();
    run(&mut h, "document.delete_page");
    h.run_steps(3);
    assert_eq!(geoms(&h).len(), n, "certified documents refuse page edits");
}

// ---------------------------------------------------------------- creating and combining

/// D-102 combine several PDFs in order, each with its own range. D-103 options: bookmarks,
/// attachments, properties, layers, labels from file names; signatures cleared with a warning.
#[test]
fn combine_pdfs_with_ranges_and_options() {
    let dir = temp_dir("doca-combine");
    tagged(&dir, "a.pdf", "A", 3);
    tagged(&dir, "b.pdf", "B", 2);
    let mut a = automation(&dir);
    call(
        &mut a,
        "doc_combine",
        json!({ "files": ["b.pdf", "a.pdf"], "out": "ab.pdf" }),
    );
    call(&mut a, "doc_open", json!({ "path": "ab.pdf" }));
    assert_eq!(tags(&mut a), ["B 1", "B 2", "A 1", "A 2", "A 3"]);
    let t = titles(&mut a);
    assert_eq!(t.len(), 2, "one bookmark per file: {t:?}");
    // Each file in the list gets its own page range ("" = every page).
    call(
        &mut a,
        "doc_combine_files",
        json!({ "files": ["a.pdf", "b.pdf"], "ranges": ["2-3", ""], "out": "ranges.pdf" }),
    );
    call(&mut a, "doc_open", json!({ "path": "ranges.pdf" }));
    assert_eq!(tags(&mut a), ["A 2", "A 3", "B 1", "B 2"]);
    call(
        &mut a,
        "doc_combine",
        json!({ "files": ["b.pdf", "a.pdf"], "ranges": ["2", "1,3"], "out": "r2.pdf" }),
    );
    call(&mut a, "doc_open", json!({ "path": "r2.pdf" }));
    assert_eq!(tags(&mut a), ["B 2", "A 1", "A 3"]);
    let e = fails(
        &mut a,
        "doc_combine",
        json!({ "files": ["a.pdf", "b.pdf"], "ranges": ["9", ""], "out": "bad.pdf" }),
    );
    assert!(e.contains('9') || e.contains("page"), "{e}");
    let v = call(
        &mut a,
        "doc_combine_files",
        json!({ "files": ["a.pdf", "b.pdf"], "out": "opts.pdf", "bookmarks": true, "labels_from_names": true, "properties": true, "layers": true, "attachments": true }),
    );
    call(&mut a, "doc_open", json!({ "path": "opts.pdf" }));
    let l = tool_labels(&mut a);
    assert!(l[0].starts_with('a') && l[3].starts_with('b'), "{l:?} {v}");
}

/// D-104 create a PDF from one non-PDF file. D-105 several files into one PDF, or one PDF per
/// source in the source folder or a chosen folder.
#[test]
fn create_pdfs_from_files_one_or_many() {
    let dir = temp_dir("doca-create");
    let png = sample_png(&dir);
    std::fs::write(dir.join("notes.txt"), "Line one\nLine two\n").unwrap();
    tagged(&dir, "p.pdf", "P", 1);
    let mut a = automation(&dir);
    call(
        &mut a,
        "doc_create_from_files",
        json!({ "files": [s(&png), "notes.txt", "p.pdf"], "out": "stapled.pdf" }),
    );
    call(&mut a, "doc_open", json!({ "path": "stapled.pdf" }));
    let t = page_texts(&mut a);
    assert_eq!(t.len(), 3, "{t:?}");
    assert!(t[1].contains("Line one") && t[2].contains("P 1"));
    std::fs::create_dir_all(dir.join("each")).unwrap();
    call(
        &mut a,
        "doc_create_each",
        json!({ "files": ["notes.txt", s(&png)], "out_dir": "each" }),
    );
    assert!(dir.join("each").join("notes.pdf").is_file());
    call(&mut a, "doc_create_each", json!({ "files": ["notes.txt"] }));
    assert!(dir.join("notes.pdf").is_file(), "beside its source");
    let mut h = app();
    run(&mut h, "file.create_from_files");
    h.run_steps(4);
    assert!(shows(&h, "Create PDF"), "the Create from Files dialog");
}

/// D-105: the Stapler's jobs are queued and run in the background, one after another, with
/// progress and cancel (headless through job_start / job_list / job_wait / job_cancel, and in
/// the app through Window > Jobs while the interface stays live).
#[test]
fn stapler_jobs_run_in_a_background_queue() {
    let dir = temp_dir("doca-jobs");
    let mut names = Vec::new();
    for i in 0..6 {
        let n = format!("note{i}.txt");
        std::fs::write(dir.join(&n), format!("Note {i}\nSecond line")).unwrap();
        names.push(n);
    }
    let mut a = automation(&dir);
    std::fs::create_dir_all(dir.join("out")).unwrap();
    let id = call(
        &mut a,
        "job_start",
        json!({ "kind": "create_each", "files": names, "out_dir": "out" }),
    )["job"]
        .as_u64()
        .unwrap();
    let combined = call(
        &mut a,
        "job_start",
        json!({ "kind": "create", "files": ["note0.txt", "note1.txt"], "out": "all.pdf" }),
    )["job"]
        .as_u64()
        .unwrap();
    let listed = call(&mut a, "job_list", json!({}));
    assert_eq!(listed["jobs"].as_array().unwrap().len(), 2, "{listed}");
    let j = call(&mut a, "job_wait", json!({ "job": id, "timeout_secs": 60 }));
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(
        (j["done"].as_u64(), j["total"].as_u64()),
        (Some(6), Some(6)),
        "progress by file"
    );
    assert_eq!(j["outputs"].as_array().unwrap().len(), 6);
    assert!(dir.join("out").join("note5.pdf").is_file());
    let j = call(&mut a, "job_wait", json!({ "job": combined, "timeout_secs": 60 }));
    assert_eq!(j["state"], "done", "{j}");
    call(&mut a, "doc_open", json!({ "path": "all.pdf" }));
    assert_eq!(
        page_texts(&mut a).len(),
        2,
        "the queue ran the second job after the first"
    );
    // Cancel: a queued job never runs.
    let many: Vec<String> = names.iter().cycle().take(30).cloned().collect();
    let long = call(
        &mut a,
        "job_start",
        json!({ "kind": "create_each", "files": many, "out_dir": "out" }),
    )["job"]
        .as_u64()
        .unwrap();
    let queued = call(
        &mut a,
        "job_start",
        json!({ "kind": "create", "files": ["note2.txt"], "out": "never.pdf" }),
    )["job"]
        .as_u64()
        .unwrap();
    assert_eq!(call(&mut a, "job_cancel", json!({ "job": queued }))["cancelled"], true);
    call(&mut a, "job_wait", json!({ "job": long, "timeout_secs": 60 }));
    let j = call(&mut a, "job_wait", json!({ "job": queued, "timeout_secs": 5 }));
    assert_eq!(j["state"], "cancelled", "{j}");
    assert!(!dir.join("never.pdf").exists(), "a cancelled job writes nothing");
    assert!(
        a.call("job_start", &json!({ "kind": "nope", "files": ["note0.txt"] }))
            .is_err()
    );

    // The app: the Stapler queues its job and the interface keeps running.
    let mut h = app();
    h.state_mut().state.features.jobs.background = Some(true);
    run(&mut h, "file.create_from_files");
    {
        let s = &mut h.state_mut().state;
        s.features.batch.files = names.iter().map(|n| dir.join(n)).collect();
        s.features.batch.more.each = true;
        s.features.batch.more.beside_source = true;
        markupcraft_ui_egui::features::batch::run(s);
    }
    // On a fast machine the job may already have finished by the time we look.
    let st = h.state().state.status.clone();
    assert!(st.contains("queued") || st.contains("Created"), "{st}");
    run(&mut h, "window.jobs");
    h.run_steps(3);
    assert!(shows(&h, "Jobs"), "Window > Jobs");
    assert!(shows(&h, "Create PDF from Files (one per file)"), "the job is listed");
    let q = h.state().state.features.jobs.queue.clone();
    let id = q.list().last().unwrap().id;
    let j = q.wait(id, std::time::Duration::from_secs(60)).unwrap();
    assert_eq!(j.state, markupcraft_engine::jobs::JobState::Done, "{}", j.message);
    h.run_steps(3);
    assert!(
        h.state().state.status.contains("Created 6 PDFs beside their sources"),
        "the finished job reports: {}",
        h.state().state.status
    );
    assert!(dir.join("note3.pdf").is_file());
    assert!(shows(&h, "6 of 6"), "its progress in the dialog");
    // The dialog's Clear Finished empties the list.
    h.get_by_label("Clear Finished").click();
    h.run_steps(3);
    assert!(h.state().state.features.jobs.queue.list().is_empty());
}

/// D-106 Explorer right-click Combine / Convert entries.
#[test]
fn file_manager_entries_to_combine_and_convert() {
    let dir = temp_dir("doca-shell");
    tagged(&dir, "a.pdf", "A", 1);
    tagged(&dir, "b.pdf", "B", 1);
    let mut a = automation(&dir);
    std::fs::create_dir_all(dir.join("integration")).unwrap();
    call(
        &mut a,
        "shell_integration",
        json!({ "action": "write", "dir": "integration", "os": "windows", "cli": "markupcraft-cli.exe" }),
    );
    let files: Vec<String> = std::fs::read_dir(dir.join("integration"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(files.iter().any(|f| f.ends_with(".reg")), "{files:?}");
    let reg = files.iter().find(|f| f.ends_with(".reg")).unwrap();
    let text = std::fs::read(dir.join("integration").join(reg)).unwrap();
    let text = String::from_utf8_lossy(&text).replace('\0', "");
    assert!(text.contains("HKEY_CURRENT_USER"), "current user only");
    call(
        &mut a,
        "shell_integration",
        json!({ "action": "combine", "files": ["a.pdf", "b.pdf"] }),
    );
    let pdfs: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".pdf"))
        .collect();
    assert!(pdfs.len() >= 3, "a combined file was written");
}
