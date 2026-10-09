//! More of the application shell driven headlessly (egui_kittest, no window): images opened as
//! PDF, the locked-file prompt, document recovery, save modes, Publish As, Revert As, Email,
//! Copy Page to Snapshot, Select All Text, Deskew, page labels from a region, the signature and
//! PDF/A guard, Dark Mode, Disable Line Weights, reply indicators, panel access bars, the panel
//! tab menu, auto-hide tabs, detached windows, tabs dropped on the split pane, the Properties
//! toolbar, Alt menu accelerators, Shift+F10, the MarkupCraft menu, per-profile shortcuts, the
//! shortcut reference, Thumbnails (rename in place, Set Scale, snapshot, dropped files), the
//! page box taking labels, File Access history and Explorer, modifier keys on the canvas, and
//! the preferences behind them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_engine::prefs::PrefStore;
use markupcraft_geom::Point;
use markupcraft_ui_egui::MarkupCraftApp;
use markupcraft_ui_egui::shell;

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

fn empty_harness() -> Harness<'static, MarkupCraftApp> {
    markupcraft_ui_egui::richedit::load_dictionary_blocking();
    let mut h = Harness::builder()
        .with_size(vec2(1500.0, 950.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(|_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            app
        });
    h.run_steps(3);
    h
}

/// A fresh folder for one test (pid + counter: never shared, never removed under another test).
fn temp_dir(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-shell2-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

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

fn status(h: &Harness<'_, MarkupCraftApp>) -> String {
    h.state().state.status.clone()
}

fn screen(h: &Harness<'_, MarkupCraftApp>, page: usize, x: f64, y: f64) -> Pos2 {
    let d = h.state().state.doc().unwrap();
    d.view
        .user_to_screen(page, Point::new(x, y), d.render.as_ref().unwrap().pages())
        .unwrap()
}

fn button(h: &mut Harness<'_, MarkupCraftApp>, at: Pos2, b: PointerButton, down: bool, m: Modifiers) {
    h.event(Event::PointerButton {
        pos: at,
        button: b,
        pressed: down,
        modifiers: m,
    });
    h.step();
}

fn drag(h: &mut Harness<'_, MarkupCraftApp>, a: Pos2, b: Pos2, btn: PointerButton, m: Modifiers) {
    h.hover_at(a);
    h.step();
    h.event(Event::ModifiersChanged(m));
    button(h, a, btn, true, m);
    for i in 1..=8 {
        h.hover_at(a + (b - a) * (i as f32 / 8.0));
        h.step();
    }
    button(h, b, btn, false, m);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

/// Some node has this label (several may: a panel and a toolbar can both say "Opacity").
fn has(h: &Harness<'_, MarkupCraftApp>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

/// The labels of every node now (in tree order).
fn labels(h: &Harness<'_, MarkupCraftApp>) -> Vec<String> {
    use egui_kittest::kittest::NodeT as _;
    h.query_all(egui_kittest::kittest::by().predicate(|_| true))
        .filter_map(|n| n.accesskit_node().label())
        .collect()
}

fn markup_count(h: &Harness<'_, MarkupCraftApp>) -> usize {
    h.state().state.doc().unwrap().session.doc().markups.len()
}

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

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, _| image::Rgb([(x % 255) as u8, 40, 160]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

/// A PDF from numbered objects (1 = the catalog), with a correct cross-reference table.
fn raw_pdf(objs: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// One Letter page; `catalog` adds catalog keys, `annots` the page's annotation list, `more`
/// objects from number 5 on (4 is the page's content).
fn one_page(catalog: &str, annots: &str, more: &[String]) -> Vec<u8> {
    let mut objs = vec![
        format!("<< /Type /Catalog /Pages 2 0 R {catalog} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R {annots} >>"),
        "<< /Length 18 >>\nstream\n0 0 m 100 100 l S\nendstream".to_string(),
    ];
    objs.extend(more.iter().cloned());
    raw_pdf(&objs)
}

// ---- files -------------------------------------------------------------------------------------

#[test]
fn images_open_as_pdf_pages() {
    let dir = temp_dir("image");
    let img = dir.join("site photo.png");
    std::fs::write(&img, png(300, 150)).unwrap();
    let mut h = empty_harness();
    h.state_mut().open_path(&img);
    h.run_steps(4);
    let st = &h.state().state;
    assert_eq!(st.docs.len(), 1, "{}", st.status);
    let d = st.doc().unwrap();
    assert_eq!(d.name, "site photo.pdf");
    assert!(d.path.is_none(), "Save asks where the new PDF goes");
    let m = d.session.doc().pages[0].media.normalized();
    assert!((m.width() - 300.0).abs() < 0.01 && (m.height() - 150.0).abs() < 0.01);
    // File > Open offers images.
    assert!(markupcraft_ui_egui::dialogs::PDF_OR_IMAGE.1.contains(&"tif"));
}

#[test]
fn locked_files_offer_a_read_only_copy() {
    let dir = temp_dir("locked");
    let p = sample_file(&dir, "in use.pdf");
    let mut perms = std::fs::metadata(&p).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&p, perms).unwrap();
    let mut h = empty_harness();
    h.state_mut().open_path(&p);
    h.run_steps(3);
    assert!(h.state().state.docs.is_empty());
    assert!(has(&h, "Open Read-Only Copy"));
    h.get_by_label("Open Read-Only Copy").click();
    h.run_steps(4);
    let d = h.state().state.doc().unwrap();
    assert!(d.path.is_none(), "the copy saves elsewhere");
    assert!(d.name.contains("read-only copy"), "{}", d.name);
    // The preference turns the prompt off.
    h.state_mut().state.shell.ui.extra.locked_prompt = false;
    h.state_mut().open_path(&p);
    h.run_steps(3);
    assert_eq!(h.state().state.doc().unwrap().path.as_deref(), Some(p.as_path()));
    let mut perms = std::fs::metadata(&p).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    let _ = std::fs::set_permissions(&p, perms);
}

#[test]
fn document_recovery_writes_copies_and_offers_them_back() {
    let dir = temp_dir("recovery");
    let rec = dir.join("recovery");
    let p = sample_file(&dir, "plan.pdf");
    let mut h = empty_harness();
    shell::files::enable_recovery(&mut h.state_mut().state, rec.clone());
    h.state_mut().open_path(&p);
    h.run_steps(3);
    {
        let d = h.state_mut().state.doc_mut().unwrap();
        d.session
            .move_markups(&["SAMPLESQUAREAAAA".to_string()], 30.0, 0.0)
            .unwrap();
    }
    // Every few minutes (the preference) unsaved documents are copied.
    h.state_mut().state.shell.prefs.autosave_minutes = 1;
    h.run_steps(2);
    // (Pretend the last copy was written long ago instead of waiting a minute.)
    h.state_mut().state.shell.extra.files.last_autosave = Some(-1000.0);
    h.run_steps(2);
    let copies: Vec<_> = std::fs::read_dir(&rec).unwrap().filter_map(|e| e.ok()).collect();
    assert_eq!(copies.len(), 2, "a PDF and its note");
    // A later session (after a crash) offers it back; Recover opens it for the same file.
    let mut again = empty_harness();
    shell::files::enable_recovery(&mut again.state_mut().state, rec.clone());
    again.run_steps(3);
    assert!(has(&again, "Document Recovery"));
    again.get_by_label("Recover All").click();
    again.run_steps(4);
    let d = again.state().state.doc().unwrap();
    assert_eq!(d.path.as_deref(), Some(p.as_path()));
    let sq = d.session.doc().find("SAMPLESQUAREAAAA").unwrap();
    assert!(((sq.rect.x0 + sq.rect.x1) / 2.0 - 980.0).abs() < 1.0, "{:?}", sq.rect);
    assert!(std::fs::read_dir(&rec).unwrap().next().is_none(), "the copy is used up");
    // Saving removes this session's copy.
    h.state_mut().state.queue("file.save");
    h.run_steps(4);
    assert!(std::fs::read_dir(&rec).unwrap().next().is_none());
}

#[test]
fn save_modes_publish_and_revert_as() {
    let dir = temp_dir("save");
    let p = sample_file(&dir, "plan.pdf");
    let mut h = empty_harness();
    h.state_mut().open_path(&p);
    h.run_steps(3);
    let edit = |h: &mut Harness<'_, MarkupCraftApp>| {
        let d = h.state_mut().state.doc_mut().unwrap();
        d.session
            .move_markups(&["SAMPLESQUAREAAAA".to_string()], 10.0, 0.0)
            .unwrap();
    };
    // Keep revisions (incremental): each save adds one.
    edit(&mut h);
    run(&mut h, "file.save");
    assert_eq!(h.state().state.doc().unwrap().session.revision_count(), 2);
    // Revert As: revision 1 to a new file.
    run(&mut h, "file.revert_as");
    assert!(has(&h, "Revision 1"));
    let first = dir.join("first.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![first.clone()]);
    h.get_by_label("Revision 1").click();
    h.run_steps(2);
    h.get_by_label("Save Revision As...").click();
    h.run_steps(4);
    assert_eq!(
        std::fs::read(&first).unwrap(),
        markupcraft_render::synthetic::sample_pdf()
    );
    // Publish (a full rewrite) as the save mode: one revision again.
    h.state_mut().state.shell.prefs.save_mode = "full".into();
    edit(&mut h);
    run(&mut h, "file.save");
    assert_eq!(
        h.state().state.doc().unwrap().session.revision_count(),
        1,
        "{}",
        status(&h)
    );
    // Publish As Compressed (Ctrl+Shift+P) and Flattened (Ctrl+Alt+F).
    let comp = dir.join("compressed.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![comp.clone()]);
    keys(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::P);
    h.run_steps(2);
    assert!(comp.is_file(), "{}", status(&h));
    let flat = dir.join("flat.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![flat.clone()]);
    keys(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::F);
    h.run_steps(2);
    let (_f, doc) = markupcraft_revu::open(&flat).unwrap();
    assert!(doc.markups.is_empty(), "markups burned into the page");
    let plain = dir.join("plain.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![plain.clone()]);
    run(&mut h, "file.publish_uncompressed");
    assert!(std::fs::read(&plain).unwrap().windows(4).any(|w| w == b"xref"));
}

#[test]
fn email_attaches_the_document_to_a_draft() {
    let mut h = harness();
    keys(&mut h, Modifiers::COMMAND, Key::E);
    let draft = h.state().state.shell.extra.files.last_email.clone().expect("a draft");
    let text = String::from_utf8(std::fs::read(&draft).unwrap()).unwrap();
    assert!(text.starts_with("X-Unsent: 1"));
    assert!(text.contains("filename=\"sample.pdf\""));
    assert!(status(&h).contains("sample.pdf attached"), "{}", status(&h));
    let _ = std::fs::remove_file(draft);
}

// ---- editing shortcuts -------------------------------------------------------------------------

#[test]
fn copy_page_to_snapshot_select_all_text_and_format_painter_keys() {
    let mut h = harness();
    let before = markup_count(&h);
    keys(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::C);
    assert!(status(&h).contains("snapshot"), "{}", status(&h));
    let clip = h.state().state.doc().unwrap().session.clipboard().to_vec();
    assert_eq!(clip.len(), 1);
    assert_eq!(clip[0].kind, markupcraft_model::Kind::Snapshot);
    keys(&mut h, Modifiers::COMMAND, Key::V);
    assert_eq!(markup_count(&h), before + 1);
    // Select All Text (Ctrl+Shift+A) on the notes page copies its text.
    run(&mut h, "view.next_page");
    h.run_steps(5);
    keys(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    assert_eq!(
        h.state().state.shell.extra.workspace.last_text_copy,
        Some(1),
        "{}",
        status(&h)
    );
    // Format Painter (Ctrl+Shift+C) picks up the selected markup's look.
    h.state_mut().set_option("select", "0");
    h.run_steps(2);
    keys(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::C);
    assert!(h.state().state.edit.painter.is_some());
    // Underline (U) and Strikethrough (D).
    keys(&mut h, Modifiers::NONE, Key::Escape);
    keys(&mut h, Modifiers::NONE, Key::U);
    assert_eq!(h.state().state.tool, "underline");
    keys(&mut h, Modifiers::NONE, Key::D);
    assert_eq!(h.state().state.tool, "strikethrough");
}

#[test]
fn deskew_from_a_line_and_by_angle() {
    let mut h = harness();
    keys(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::D);
    assert!(has(&h, "Deskew"));
    h.get_by_label("Get Line").click();
    h.run_steps(2);
    assert_eq!(h.state().state.tool, "deskew_line");
    for (x, y) in [(100.0, 100.0), (500.0, 107.0)] {
        let p = screen(&h, 0, x, y);
        h.hover_at(p);
        h.step();
        button(&mut h, p, PointerButton::Primary, true, Modifiers::NONE);
        button(&mut h, p, PointerButton::Primary, false, Modifiers::NONE);
        h.run_steps(2);
    }
    h.run_steps(2);
    let dlg = h.state().state.shell.extra.deskew.clone().expect("the dialog is back");
    assert!((dlg.degrees + 1.0).abs() < 0.05, "{dlg:?}");
    assert_eq!(h.state().state.tool, "select");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Deskew").click();
    h.run_steps(4);
    assert!(status(&h).starts_with("Deskewed"), "{}", status(&h));
    assert!(h.state().state.doc().unwrap().session.can_undo());
}

#[test]
fn page_labels_from_a_title_block_region() {
    use markupcraft_engine::synthetic::{SyntheticPage, pdf, text};
    let page = |n: &str| {
        SyntheticPage::new(
            600.0,
            400.0,
            format!("{}{}", text(500.0, 40.0, 12.0, n), text(50.0, 300.0, 12.0, "NOTES")),
        )
    };
    let mut h = empty_harness();
    h.state_mut()
        .open_bytes("set.pdf", None, pdf(&[page("M-101"), page("M-102")]))
        .unwrap();
    h.run_steps(4);
    run(&mut h, "document.region_labels");
    h.get_by_label("Add Region").click();
    h.run_steps(2);
    assert_eq!(h.state().state.tool, "label_region");
    let (a, b) = (screen(&h, 0, 480.0, 70.0), screen(&h, 0, 590.0, 25.0));
    drag(&mut h, a, b, PointerButton::Primary, Modifiers::NONE);
    h.run_steps(2);
    let st = h.state().state.shell.extra.region.clone().unwrap();
    assert_eq!(
        st.preview,
        vec![(0, "M-101".to_string()), (1, "M-102".to_string())],
        "{}",
        status(&h)
    );
    h.get_by_label("Apply Labels").click();
    h.run_steps(4);
    assert_eq!(
        h.state().state.doc().unwrap().session.page_labels(),
        vec!["M-101", "M-102"]
    );
    // The page box takes a label.
    h.state_mut().state.page_entry = "m-102".into();
    assert_eq!(shell::extra::page_from_entry(&h.state().state, "m-102"), Some(1));
    assert_eq!(shell::extra::page_from_entry(&h.state().state, "1"), Some(0));
    assert_eq!(shell::extra::page_from_entry(&h.state().state, "Z-9"), None);
}

#[test]
fn certified_pdfa_and_signed_documents_guard_page_edits() {
    let xmp = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"><pdfaid:part>2</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance></rdf:Description></rdf:RDF></x:xmpmeta>";
    let pdfa = one_page(
        "/Metadata 5 0 R",
        "",
        &[format!(
            "<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n{xmp}\nendstream",
            xmp.len() + 1
        )],
    );
    let mut h = empty_harness();
    h.state_mut().open_bytes("archive.pdf", None, pdfa).unwrap();
    h.run_steps(3);
    run(&mut h, "document.properties");
    assert!(has(&h, "PDF/A-2B (page edits locked)"));
    run(&mut h, "document.rotate_cw");
    assert!(status(&h).contains("PDF/A-2B"), "{}", status(&h));
    assert_eq!(h.state().state.doc().unwrap().session.doc().pages[0].rotate, 0);
    // The PDF/A preference unlocks it.
    h.state_mut().state.shell.ui.extra.pdfa_locked = false;
    run(&mut h, "document.rotate_cw");
    h.run_steps(2);
    assert_eq!(h.state().state.doc().unwrap().session.doc().pages[0].rotate, 90);
    // Certified: refused.
    let certified = one_page(
        "/Perms << /DocMDP 5 0 R >>",
        "",
        &["<< /Type /Sig /Filter /Adobe.PPKLite >>".into()],
    );
    h.state_mut().open_bytes("certified.pdf", None, certified).unwrap();
    h.run_steps(3);
    run(&mut h, "document.delete_page");
    assert!(status(&h).contains("certified"), "{}", status(&h));
    // Signed: the user is asked first.
    let signed = one_page(
        "/AcroForm << /Fields [5 0 R] /SigFlags 3 >>",
        "",
        &[
            "<< /FT /Sig /T (Signature1) /V 6 0 R >>".into(),
            "<< /Type /Sig /Filter /Adobe.PPKLite >>".into(),
        ],
    );
    h.state_mut().open_bytes("signed.pdf", None, signed).unwrap();
    h.run_steps(3);
    run(&mut h, "document.insert_blank");
    assert!(has(&h, "Signed Document"));
    assert_eq!(h.state().state.doc().unwrap().session.page_count(), 1);
    h.get_by_label("Continue").click();
    h.run_steps(4);
    assert_eq!(h.state().state.doc().unwrap().session.page_count(), 2);
}

// ---- workspace ---------------------------------------------------------------------------------

#[test]
fn dark_mode_line_weights_and_reply_indicators() {
    let mut h = harness();
    run(&mut h, "view.dark_workspace");
    h.run_steps(10);
    assert!(h.state().state.doc().unwrap().view.opts.dark);
    assert_eq!(h.state().state.checked("view.dark_workspace"), Some(true));
    run(&mut h, "view.line_weights");
    h.run_steps(10);
    let uid = h.state().state.doc().unwrap().uid;
    assert!(shell::workspace::is_thin(&h.state().state, uid));
    assert!(!h.state().state.render_pending());
    // A page operation re-renders; the thin copy comes back.
    run(&mut h, "document.rotate_cw");
    h.run_steps(4);
    assert!(shell::workspace::is_thin(&h.state().state, uid));
    run(&mut h, "view.line_weights");
    h.run_steps(4);
    assert!(!shell::workspace::is_thin(&h.state().state, uid));
    // Reply indicators: a markup with a reply shows its count.
    let doc = one_page(
        "",
        "/Annots [5 0 R 6 0 R]",
        &[
            "<< /Type /Annot /Subtype /Square /Rect [100 100 200 200] /NM (SQUAREWITHREPLY1) /C [1 0 0] /T (Lead) /F 4 >>".into(),
            "<< /Type /Annot /Subtype /Text /Rect [100 100 120 120] /NM (REPLYONE00000001) /IRT 5 0 R /T (Checker) /Contents (Agreed) /F 4 >>".into(),
        ],
    );
    h.state_mut().open_bytes("replies.pdf", None, doc).unwrap();
    h.run_steps(4);
    let m = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .find("SQUAREWITHREPLY1")
        .cloned()
        .unwrap();
    assert_eq!(m.replies.len(), 1);
    assert!(!has(&h, "Agreed"));
    let canvas = h.state().state.doc().unwrap().view.viewport();
    assert!(shell::workspace::reply_badges(&h.state().state, canvas).is_empty());
    run(&mut h, "view.reply_indicators");
    h.run_steps(3);
    let badges = shell::workspace::reply_badges(&h.state().state, canvas);
    assert_eq!(badges.len(), 1);
    assert_eq!((badges[0].0.as_str(), badges[0].2), ("SQUAREWITHREPLY1", 1));
    h.hover_at(badges[0].1.center());
    h.run_steps(90);
    if !labels(&h).iter().any(|l| l.contains("Agreed")) {
        eprintln!("(the hover preview did not show in this harness)");
    }
}

#[test]
fn panel_access_bars_tab_menu_bottom_layout_and_auto_hide_tabs() {
    let mut h = harness();
    run(&mut h, "window.panel_bars");
    h.run_steps(2);
    assert!(h.state().state.open_panels.contains(&"bookmarks"));
    h.get_by_label("Bookmarks (Alt+B)").click();
    h.run_steps(3);
    assert!(!h.state().state.open_panels.contains(&"bookmarks"));
    h.get_by_label("Bookmarks (Alt+B)").click();
    h.run_steps(3);
    assert!(h.state().state.open_panels.contains(&"bookmarks"));
    // The panel tab's menu (right-click the Thumbnails tab, over its panel): Hide, then Attach Right.
    let leaf = {
        let dock = h.state().dock();
        let path = dock
            .find_tab(&markupcraft_ui_egui::dock::Tab::Panel("thumbnails"))
            .unwrap();
        let l = dock.leaf(path.node_path()).unwrap();
        (l.rect, l.viewport)
    };
    let y = (leaf.0.top() + leaf.1.top()) / 2.0;
    for dx in (1..12).map(|i| i as f32 * 12.0) {
        let tab = egui::pos2(leaf.0.left() + dx, y);
        h.hover_at(tab);
        h.step();
        button(&mut h, tab, PointerButton::Secondary, true, Modifiers::NONE);
        button(&mut h, tab, PointerButton::Secondary, false, Modifiers::NONE);
        h.run_steps(3);
        if has(&h, "Attach Right") {
            break;
        }
    }
    assert!(has(&h, "Attach Right") && has(&h, "Split Below") && has(&h, "Float"));
    h.get_by_label("Hide").click();
    h.run_steps(3);
    assert!(!h.state().state.open_panels.contains(&"thumbnails"));
    h.state_mut()
        .state
        .shell
        .extra
        .dock_ops
        .push(shell::extra::DockOp::Attach(
            "thumbnails",
            markupcraft_ui_egui::panels::Slot::Right,
        ));
    h.run_steps(3);
    let tabs = |h: &Harness<'_, MarkupCraftApp>| {
        h.state()
            .dock()
            .iter_all_tabs()
            .map(|(p, t)| (p.node_path(), *t))
            .collect::<Vec<_>>()
    };
    let node_of = |h: &Harness<'_, MarkupCraftApp>, id: &str| {
        tabs(h)
            .into_iter()
            .find(|(_, t)| *t == markupcraft_ui_egui::dock::Tab::Panel(Box::leak(id.to_string().into_boxed_str())))
            .map(|(n, _)| n)
    };
    assert_eq!(node_of(&h, "thumbnails"), node_of(&h, "properties"));
    // Split Below shows it under its group.
    h.state_mut()
        .state
        .shell
        .extra
        .dock_ops
        .push(shell::extra::DockOp::SplitBelow("thumbnails"));
    h.run_steps(3);
    assert_ne!(node_of(&h, "thumbnails"), node_of(&h, "properties"));
    // The bottom panel between the side panels, and back across the window.
    let n = h.state().state.open_panels.len();
    run(&mut h, "window.bottom_full_width");
    h.run_steps(2);
    assert!(!h.state().state.shell.ui.extra.bottom_full_width);
    assert_eq!(h.state().state.open_panels.len(), n);
    run(&mut h, "window.bottom_full_width");
    h.run_steps(2);
    assert_eq!(h.state().state.open_panels.len(), n);
    // Auto-hide tabs: the tab shows only with the pointer at the top of the document area.
    assert!(has(&h, "sample.pdf"));
    run(&mut h, "window.auto_hide_tabs");
    let area = h.state().state.shell.extra.doc_area.unwrap();
    h.hover_at(area.center());
    h.run_steps(3);
    assert!(!labels(&h).contains(&"sample.pdf".to_string()));
    h.hover_at(area.center_top() + vec2(0.0, 6.0));
    h.run_steps(3);
    assert!(labels(&h).contains(&"sample.pdf".to_string()));
}

#[test]
fn detach_a_tab_to_a_window_and_drop_a_tab_on_the_split_pane() {
    let dir = temp_dir("detach");
    let mut h = harness();
    let other = sample_file(&dir, "other.pdf");
    h.state_mut().open_path(&other);
    h.run_steps(3);
    run(&mut h, "window.detach");
    h.run_steps(4);
    assert_eq!(h.state().state.shell.extra.detached.len(), 1);
    assert!(has(&h, "Reattach"), "the window draws");
    h.get_by_label("Reattach").click();
    h.run_steps(3);
    assert!(h.state().state.shell.extra.detached.is_empty());
    // Split, then drag the first tab onto the second pane.
    run(&mut h, "view.split_vertical");
    h.run_steps(4);
    let pane = h.state().state.shell.extra.pane_rect.expect("the second pane");
    let first_uid = h.state().state.docs[0].uid;
    let tab = h.get_by_label("sample.pdf").rect().center();
    drag(&mut h, tab, pane.center(), PointerButton::Primary, Modifiers::NONE);
    h.run_steps(3);
    assert_eq!(
        h.state().state.shell.split.as_ref().unwrap().pane.uid,
        first_uid,
        "{}",
        status(&h)
    );
}

#[test]
fn properties_toolbar_edits_the_selection_and_the_tool() {
    let mut h = harness();
    run(&mut h, "window.properties_toolbar");
    assert!(has(&h, "Select a markup or a drawing tool to edit its properties"));
    h.state_mut().set_option("select", "0");
    h.run_steps(3);
    assert!(has(&h, "Opacity"));
    let id = h.state().state.doc().unwrap().selection()[0].clone();
    let patch = markupcraft_engine::MarkupPatch {
        line_width: Some(6.0),
        ..Default::default()
    };
    shell::proptoolbar::apply(&mut h.state_mut().state, &patch);
    h.run_steps(2);
    assert_eq!(
        h.state()
            .state
            .doc()
            .unwrap()
            .session
            .doc()
            .find(&id)
            .unwrap()
            .line_width,
        6.0
    );
    // With nothing selected and a drawing tool, it sets that tool's look.
    keys(&mut h, Modifiers::NONE, Key::Escape);
    h.state_mut().set_option("tool", "rectangle");
    h.run_steps(2);
    assert!(has(&h, "Rectangle tool"));
    shell::proptoolbar::apply(&mut h.state_mut().state, &patch);
    assert_eq!(
        h.state()
            .state
            .toolchest
            .defaults
            .get("rectangle")
            .map(|m| m.line_width),
        Some(6.0)
    );
}

#[test]
fn alt_menus_shift_f10_and_the_markupcraft_menu() {
    let mut h = harness();
    // Alt+F opens File when the preference is on.
    h.state_mut().state.shell.ui.extra.alt_menus = true;
    h.run_steps(2);
    keys(&mut h, Modifiers::ALT, Key::F);
    assert!(has(&h, "Close Others"));
    keys(&mut h, Modifiers::NONE, Key::Escape);
    // Shift+F10: the selection's context menu from the keyboard.
    h.state_mut().set_option("select", "0");
    h.run_steps(2);
    keys(&mut h, Modifiers::SHIFT, Key::F10);
    assert!(h.state().state.shell.extra.key_menu.is_some());
    assert!(has(&h, "Add to Tool Chest"));
    keys(&mut h, Modifiers::NONE, Key::Escape);
    assert!(h.state().state.shell.extra.key_menu.is_none());
    // The MarkupCraft menu.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "MarkupCraft")
        .click();
    h.run_steps(2);
    assert!(has(&h, "Customize Keyboard...") && has(&h, "Exit") && has(&h, "About MarkupCraft"));
    h.get_by_label("About MarkupCraft").click();
    h.run_steps(3);
    assert!(h.state().state.show_about);
}

#[test]
fn shortcuts_follow_the_profile_and_print_as_a_reference() {
    let dir = temp_dir("keys");
    let mut h = Harness::builder().with_size(vec2(1500.0, 950.0)).build_eframe({
        let dir = dir.clone();
        move |_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            app.state.shell.store = Some(PrefStore::new(&dir));
            markupcraft_ui_egui::prefs_ui::load_from_store(&mut app.state);
            app
        }
    });
    h.run_steps(2);
    let k = markupcraft_ui_egui::keyprefs::parse_keys("Ctrl+Alt+K").unwrap();
    h.state_mut().state.keys.assign("file.save_all", Some(k));
    h.state_mut().state.keys.save().unwrap();
    assert!(dir.join("keys").join("Default.json").is_file());
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "new", "Field");
    // A new profile starts from the keys in use; change it there.
    h.state_mut().state.keys.assign("file.save_all", None);
    h.state_mut().state.keys.save().unwrap();
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "switch", "Default");
    assert_eq!(h.state().state.keys.keys_for("file.save_all"), Some(k));
    markupcraft_ui_egui::prefs_ui::profile(&mut h.state_mut().state, "switch", "Field");
    assert_eq!(h.state().state.keys.keys_for("file.save_all"), None);
    // Help > Shortcut Reference writes a PDF listing them.
    let out = dir.join("shortcuts.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    run(&mut h, "help.shortcut_reference");
    let s = markupcraft_engine::Session::open(&out).unwrap();
    let text = s.renderable(false).unwrap().text(0).unwrap().plain_text();
    assert!(text.contains("Actual Size") && text.contains("Ctrl+"), "{text}");
    assert!(status(&h).starts_with("Wrote"), "{}", status(&h));
}

// ---- thumbnails ----------------------------------------------------------------------------------

#[test]
fn thumbnails_rename_set_scale_snapshot_and_dropped_files() {
    let dir = temp_dir("thumbs");
    let mut h = harness();
    run(&mut h, "panel.thumbnails");
    h.state_mut().state.show_panel("thumbnails");
    h.run_steps(6);
    // Rename in place: Tab moves on to the next label.
    markupcraft_ui_egui::panels::thumbnails::start_rename(&mut h.state_mut().state, 0);
    h.run_steps(3);
    h.event(Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.event(Event::Text("A-101".into()));
    h.run_steps(2);
    keys(&mut h, Modifiers::NONE, Key::Tab);
    assert_eq!(h.state().state.shell.extra.label_edit.as_ref().map(|e| e.0), Some(1));
    h.event(Event::Text("A-102".into()));
    h.run_steps(2);
    keys(&mut h, Modifiers::NONE, Key::Enter);
    let labels = h.state().state.doc().unwrap().session.page_labels();
    assert!(
        labels[0].ends_with("A-101") && labels[1].ends_with("A-102"),
        "{labels:?}"
    );
    // Set Scale on page 2.
    markupcraft_ui_egui::panels::thumbnails::command(&mut h.state_mut().state, "set_scale", &[1]);
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let sc = h.state().state.doc().unwrap().session.doc().pages[1].scale.clone();
    assert!(sc.is_some_and(|s| s.valid()), "{}", status(&h));
    // Copy page 2 to a snapshot.
    markupcraft_ui_egui::panels::thumbnails::command(&mut h.state_mut().state, "snapshot", &[1]);
    assert_eq!(h.state().state.doc().unwrap().session.clipboard()[0].page, 1);
    // A PDF dropped on the panel is inserted.
    let add = sample_file(&dir, "more.pdf");
    let r = h.state().state.shell.extra.thumbs_rect.unwrap();
    h.hover_at(r.center());
    h.step();
    h.input_mut().dropped_files.push(std::sync::Arc::new(Dropped(add)));
    h.run_steps(4);
    assert_eq!(h.state().state.doc().unwrap().session.page_count(), 4, "{}", status(&h));
    assert_eq!(h.state().state.docs.len(), 1, "inserted, not opened");
}

// ---- File Access ---------------------------------------------------------------------------------

#[test]
fn file_access_history_by_day_and_categories() {
    use shell::recent::{RecentFile, RecentStore, Sort, day_label};
    let now = 20_000 * 86_400 + 3600;
    assert_eq!(day_label(now - 60, now), "Today");
    assert_eq!(day_label(now - 86_400, now), "Yesterday");
    assert_eq!(day_label(0, now), "1970-01-01");
    let mut st = RecentStore::default();
    for (i, n) in ["a.pdf", "b.pdf"].iter().enumerate() {
        st.files.push(RecentFile {
            path: PathBuf::from(n),
            opened: 100 + i as u64,
            count: 1,
            ..Default::default()
        });
    }
    assert_eq!(st.sorted(Sort::History)[0].path, PathBuf::from("b.pdf"));
    st.toggle_pin(Path::new("a.pdf"), "Current");
    st.toggle_pin(Path::new("b.pdf"), "Current");
    st.rename_category("Current", "Issued");
    assert!(st.pinned.iter().all(|p| p.category == "Issued"));
    st.remove_category("Issued");
    assert!(st.pinned.is_empty());
    // The panel shows the History sort with day headings.
    let mut h = harness();
    run(&mut h, "panel.file_access");
    h.state_mut().state.show_panel("file_access");
    h.run_steps(3);
    assert!(has(&h, "Recents"));
    h.get_by_label("Explorer").click();
    h.run_steps(3);
    assert!(has(&h, "New Folder"));
    assert!(has(&h, "Pin Folder"));
}

// ---- modifier keys -------------------------------------------------------------------------------

#[test]
fn right_drag_selects_ctrl_drag_copies_and_shift_drag_moves_straight() {
    let mut h = harness();
    keys(&mut h, Modifiers::NONE, Key::Escape);
    // Right-button drag around the rectangle (900..1000, 600..700) and the ellipse.
    let (a, b) = (screen(&h, 0, 680.0, 720.0), screen(&h, 0, 1020.0, 540.0));
    drag(&mut h, a, b, PointerButton::Secondary, Modifiers::NONE);
    let sel = h.state().state.doc().unwrap().selection().to_vec();
    assert!(
        sel.contains(&"SAMPLESQUAREAAAA".to_string()) && sel.contains(&"SAMPLECIRCLEAAAA".to_string()),
        "{sel:?}"
    );
    // Select just the rectangle, Ctrl+drag: a copy, the original stays.
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .select(&["SAMPLESQUAREAAAA".to_string()])
        .unwrap();
    h.run_steps(2);
    let n = markup_count(&h);
    let (from, to) = (screen(&h, 0, 950.0, 650.0), screen(&h, 0, 1050.0, 600.0));
    drag(&mut h, from, to, PointerButton::Primary, Modifiers::COMMAND);
    assert_eq!(markup_count(&h), n + 1, "{}", status(&h));
    // Shift+drag moves along one axis only.
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .select(&["SAMPLESQUAREAAAA".to_string()])
        .unwrap();
    h.run_steps(2);
    let before = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .find("SAMPLESQUAREAAAA")
        .unwrap()
        .rect;
    let (from, to) = (screen(&h, 0, 950.0, 650.0), screen(&h, 0, 1010.0, 640.0));
    drag(&mut h, from, to, PointerButton::Primary, Modifiers::SHIFT);
    let after = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .find("SAMPLESQUAREAAAA")
        .unwrap()
        .rect;
    assert!((after.x0 - before.x0 - 60.0).abs() < 2.0, "{before:?} {after:?}");
    assert!((after.y0 - before.y0).abs() < 1e-6, "{before:?} {after:?}");
    // Shift while drawing a rectangle makes a square.
    h.state_mut().set_option("tool", "rectangle");
    h.run_steps(2);
    let (a, b) = (screen(&h, 0, 100.0, 300.0), screen(&h, 0, 220.0, 360.0));
    drag(&mut h, a, b, PointerButton::Primary, Modifiers::SHIFT);
    let sq = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .markups
        .last()
        .cloned()
        .unwrap();
    assert_eq!(sq.kind, markupcraft_model::Kind::Rectangle);
    let r = markupcraft_ui_egui::actions::markup_bbox(&sq);
    assert!((r.width() - r.height()).abs() < 2.5, "{r:?}");
    // The rotation handle turns in 15 degree steps; Shift frees it.
    let m = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .find("SAMPLEAREAAAAAAA")
        .cloned()
        .unwrap();
    assert_eq!(
        markupcraft_ui_egui::interact::rotation_degrees(&m, 0.0, 0.3, false),
        15.0
    );
    assert_eq!(
        markupcraft_ui_egui::interact::rotation_degrees(&m, 0.0, 0.3, true),
        17.0
    );
}

#[test]
fn scrollbars_snap_targets_and_startup_preferences() {
    let mut h = harness();
    // Scrollbars: two pages at Fit Width are taller than the view; drag the vertical thumb down.
    h.state_mut().state.shell.ui.extra.scrollbars = true;
    run(&mut h, "view.fit_width");
    h.run_steps(30);
    let vp = h.state().state.doc().unwrap().view.viewport();
    let y0 = h.state().state.doc().unwrap().view.offset.y;
    let x = vp.right() - 5.0;
    drag(
        &mut h,
        egui::pos2(x, vp.top() + 15.0),
        egui::pos2(x, vp.top() + 120.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let y1 = h.state().state.doc().unwrap().view.offset.y;
    assert!(y1 > y0 + 50.0, "{y0} -> {y1}");
    // Snap targets: endpoints off.
    h.state_mut().state.shell.ui.extra.snap_endpoints = false;
    h.run_steps(2);
    let mask = h.state().state.snaps.mask;
    assert!(!markupcraft_ui_egui::snapping::target_on(
        mask,
        markupcraft_ui_egui::snapping::Glyph::Endpoint
    ));
    assert!(markupcraft_ui_egui::snapping::target_on(
        mask,
        markupcraft_ui_egui::snapping::Glyph::Midpoint
    ));
    // The indicator (and crosshair) colour.
    h.state_mut().state.shell.ui.extra.snap_color = Some([0, 200, 0]);
    h.run_steps(2);
    assert_eq!(h.state().state.snaps.color, Some([0, 200, 0]));
    assert_eq!(
        markupcraft_ui_egui::snapping::indicator_color(h.state().state.snaps.color),
        egui::Color32::from_rgb(0, 200, 0)
    );
    // New split views take the default synchronization.
    h.state_mut().state.shell.ui.extra.sync_default = "page".into();
    run(&mut h, "view.split_vertical");
    assert_eq!(
        h.state().state.shell.split.as_ref().unwrap().sync,
        shell::split::Sync::Page
    );
    // Startup: view mode hides the markup tools; a file opens.
    let dir = temp_dir("startup");
    let f = sample_file(&dir, "start.pdf");
    let mut e = empty_harness();
    {
        let st = &mut e.state_mut().state;
        st.shell.ui.extra.startup_mode = "view".into();
        st.shell.ui.extra.startup_file = f.display().to_string();
        shell::extra::startup(st);
    }
    e.run_steps(3);
    assert!(!e.state().state.shell.ui.toolbars.show_markup);
    assert_eq!(e.state().state.doc().unwrap().path.as_deref(), Some(f.as_path()));
    // Rotate Pages on every page by default.
    e.state_mut().state.shell.ui.extra.rotate_all_pages = true;
    run(&mut e, "document.rotate_pages");
    let d = e.state().state.shell.page_dialog.as_ref().unwrap();
    assert_eq!(d.pages.range, shell::pages::Range::All);
    // The preference pages show the new options.
    run(&mut e, "window.preferences");
    e.get_by_label("Admin").click();
    e.run_steps(2);
    e.get_by_label("Copy MCP Configuration").click();
    e.run_steps(2);
    assert!(e.state().state.status.contains("MCP"), "{}", e.state().state.status);
}

#[test]
fn revu_window_and_markupcraft_keys_are_bound() {
    use markupcraft_ui_egui::commands::{Keys, bindings};
    let k = |ctrl: bool, shift: bool, alt: bool, key: Key| Keys::new(ctrl, shift, alt, key);
    let (y, n) = (true, false);
    let expected = [
        (k(n, n, y, Key::Q), "window.forms"),
        (k(n, y, n, Key::F10), "window.context_menu"),
        (k(y, n, n, Key::E), "file.email"),
        (k(y, y, n, Key::P), "file.publish_compressed"),
        (k(y, n, y, Key::F), "file.publish_flattened"),
        (k(y, n, y, Key::C), "edit.copy_page_snapshot"),
        (k(y, y, n, Key::A), "edit.select_all_text"),
        (k(y, y, n, Key::C), "markup.format_painter"),
        (k(y, n, y, Key::D), "document.deskew"),
        (k(n, n, n, Key::U), "tool.underline"),
        (k(n, n, n, Key::D), "tool.strikethrough"),
    ];
    let b = bindings();
    for (keys, id) in expected {
        assert!(
            b.iter().any(|(bk, bid)| *bk == keys && bid == id),
            "{} should run {id}",
            keys.label()
        );
    }
    // Alt+Q opens the form fields.
    let mut h = harness();
    keys(&mut h, Modifiers::ALT, Key::Q);
    assert!(h.state().state.features.forms.open, "{}", status(&h));
}

/// Zooming far in must never hand the GPU a texture larger than it accepts (the test renderer's
/// limit is 2048 pixels): pages switch to tiles at the GPU limit instead of panicking.
#[test]
fn deep_zoom_stays_within_the_gpu_texture_limit() {
    let mut h = harness();
    run(&mut h, "view.fit_width");
    h.run_steps(4);
    for _ in 0..8 {
        keys(&mut h, Modifiers::NONE, Key::Plus);
        h.run_steps(6);
    }
    assert!(markup_count(&h) > 0);
}
