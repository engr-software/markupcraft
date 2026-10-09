//! The second round of document and review features driven through the real shell headlessly
//! (egui_kittest): File > Export, Repair / PDF/A / Color Processing / Unflatten, Compare's
//! advanced options and split-view review, Overlay's manual and auto alignment, the Batch
//! Compare wizard, and the search, links, layers, bookmarks, sets, forms and summary additions.

use std::path::PathBuf;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_geom::Point;
use markupcraft_model::Kind;
use markupcraft_ui_egui::MarkupCraftApp;

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

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("markupcraft-features2-ui-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn screen(h: &Harness<'_, MarkupCraftApp>, x: f64, y: f64) -> Pos2 {
    let d = h.state().state.doc().unwrap();
    let page = d.view.current;
    d.view
        .user_to_screen(page, Point::new(x, y), d.render.as_ref().unwrap().pages())
        .unwrap()
}

fn button(h: &mut Harness<'_, MarkupCraftApp>, at: Pos2, pressed: bool) {
    h.event(Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
    h.step();
}

fn drag_screen(h: &mut Harness<'_, MarkupCraftApp>, a: Pos2, b: Pos2) {
    h.hover_at(a);
    h.step();
    button(h, a, true);
    for i in 1..=8 {
        h.hover_at(a + (b - a) * (i as f32 / 8.0));
        h.step();
    }
    button(h, b, false);
    h.run_steps(3);
}

fn drag(h: &mut Harness<'_, MarkupCraftApp>, from: (f64, f64), to: (f64, f64)) {
    let (a, b) = (screen(h, from.0, from.1), screen(h, to.0, to.1));
    drag_screen(h, a, b);
}

fn key(h: &mut Harness<'_, MarkupCraftApp>, m: Modifiers, k: Key) {
    h.key_press_modifiers(m, k);
    h.run_steps(3);
}

fn run(h: &mut Harness<'_, MarkupCraftApp>, id: &str) {
    h.state_mut().state.queue(id);
    h.run_steps(4);
}

fn press(h: &mut Harness<'_, MarkupCraftApp>, label: &str) {
    let node = h
        .get_all_by_label(label)
        .last()
        .unwrap_or_else(|| panic!("no {label:?}"));
    node.click();
    h.run_steps(4);
}

fn shows(h: &Harness<'_, MarkupCraftApp>, text: &str) -> bool {
    h.query_all_by_label_contains(text).next().is_some()
}

fn markups(h: &Harness<'_, MarkupCraftApp>) -> Vec<markupcraft_model::Markup> {
    h.state().state.doc().unwrap().session.doc().markups.clone()
}

fn script(h: &mut Harness<'_, MarkupCraftApp>, paths: Vec<PathBuf>) {
    h.state_mut().state.dialogs.scripted = Some(paths);
}

/// The sample with a header added (a changed revision).
fn revised_sample() -> Vec<u8> {
    let mut s =
        markupcraft_engine::Session::from_bytes(markupcraft_render::synthetic::sample_pdf(), "rev.pdf").unwrap();
    let hf = markupcraft_engine::marks::HeaderFooter {
        text: [
            String::new(),
            "REVISION B ADDED NOTE".into(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ],
        font_size: 24.0,
        ..Default::default()
    };
    s.add_header_footer(&[0], &hf, true).unwrap();
    s.render_bytes().unwrap().to_vec()
}

#[test]
fn export_dialogs_write_images_office_files_and_a_region() {
    let mut h = harness();
    let dir = temp("export");
    run(&mut h, "file.export_images");
    assert!(shows(&h, "Export Pages as Images"));
    h.state_mut().state.features.export.dpi = 36.0;
    script(&mut h, vec![dir.clone()]);
    press(&mut h, "Export...");
    let pngs: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
        .collect();
    assert!(!pngs.is_empty(), "{}", h.state().state.features.export.message);

    run(&mut h, "file.export_document");
    press(&mut h, "Excel");
    let xlsx = dir.join("sample.xlsx");
    script(&mut h, vec![xlsx.clone()]);
    press(&mut h, "Export...");
    assert!(std::fs::read(&xlsx).unwrap().starts_with(b"PK"));
    press(&mut h, "Close");

    let region = dir.join("region.csv");
    run(&mut h, "file.export_region");
    script(&mut h, vec![region.clone()]);
    drag(&mut h, (40.0, 600.0), (580.0, 780.0));
    let text = std::fs::read_to_string(&region).unwrap_or_default();
    assert!(!text.is_empty(), "{}", h.state().state.status);
}

#[test]
fn repair_pdfa_color_processing_and_unflatten() {
    let mut h = harness();
    run(&mut h, "document.repair");
    assert!(
        h.state().state.status.starts_with("Repaired"),
        "{}",
        h.state().state.status
    );
    run(&mut h, "document.pdfa");
    press(&mut h, "Archive");
    assert!(
        h.state().state.doc().unwrap().session.pdfa_declared().is_some(),
        "{}",
        h.state().state.features.export.message
    );
    press(&mut h, "Unlock");
    assert!(h.state().state.doc().unwrap().session.pdfa_declared().is_none());
    press(&mut h, "Close");
    run(&mut h, "document.color_processing");
    press(&mut h, "Apply");
    assert!(
        h.state().state.status.starts_with("Color Processing on"),
        "{}",
        h.state().state.status
    );
    press(&mut h, "Remove");
    press(&mut h, "Close");

    // Flatten with recovery, then Ctrl+Shift+U brings the markups back.
    let all = markups(&h).len() + 1;
    {
        let s = &mut h.state_mut().state.doc_mut().unwrap().session;
        let m = markupcraft_model::Markup::new(Kind::Line, 0, vec![Point::new(100.0, 100.0), Point::new(300.0, 300.0)]);
        s.add_markup(m).unwrap();
        let opts = markupcraft_engine::flatten::FlattenOptions {
            recoverable: true,
            layer: None,
        };
        s.flatten_markups_with(&Default::default(), &opts).unwrap();
    }
    assert!(markups(&h).is_empty());
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::U);
    assert_eq!(markups(&h).len(), all, "{}", h.state().state.status);
}

#[test]
fn compare_advanced_options_and_review_in_split_view_with_the_dimmer() {
    let mut h = harness();
    h.state_mut().open_bytes("revised.pdf", None, revised_sample()).unwrap();
    h.run_steps(4);
    run(&mut h, "tools.compare");
    {
        let c = &mut h.state_mut().state.features.compare;
        c.preset = "different_printer".into();
        c.align = 1;
        c.fill = Some(markupcraft_model::Color::rgb(1.0, 1.0, 0.0));
        c.lock = true;
        c.split_review = true;
    }
    h.run_steps(2);
    assert!(shows(&h, "Advanced"));
    press(&mut h, "OK");
    let s = &h.state().state;
    assert!(!s.features.compare.regions.is_empty(), "{}", s.features.compare.message);
    assert!(
        markups(&h)
            .iter()
            .filter(|m| m.subject == "Compare")
            .all(|m| m.locked() && m.fill.is_some())
    );
    let split = s.shell.split.as_ref().expect("split view");
    let old = s.features.compare.old_uid.unwrap();
    assert_eq!(split.pane.uid, old, "the older revision beside the newer");
    assert!(s.shell.dimmer);
    h.state_mut().state.shell.split = None;
    h.state_mut().state.shell.dimmer = false;
    h.run_steps(2);
    press(&mut h, "Split + Dimmer");
    assert!(h.state().state.shell.split.is_some() && h.state().state.shell.dimmer);
}

#[test]
fn overlay_three_points_auto_align_and_defaults() {
    let mut h = harness();
    h.state_mut().open_bytes("revised.pdf", None, revised_sample()).unwrap();
    h.run_steps(4);
    run(&mut h, "file.overlay");
    press(&mut h, "Manual (3 points)");
    {
        let o = &mut h.state_mut().state.features.overlay;
        assert_eq!(o.align, markupcraft_ui_egui::features::overlay::Align::Three);
        if let Some(r) = o.rows.get_mut(1) {
            r.from = [0.0, 0.0, 100.0, 0.0, 0.0, 100.0];
            r.to = [0.0, 0.0, 100.0, 0.0, 0.0, 100.0];
            r.background = Some(markupcraft_model::Color::rgb(1.0, 1.0, 0.8));
            r.region = Some([0.0, 0.0, 400.0, 400.0]);
        }
        o.blend = markupcraft_engine::overlay::OverlayBlend::Darken;
        o.adjust.rotation = 0.0;
    }
    let out = temp("overlay").join("three.pdf");
    script(&mut h, vec![out.clone()]);
    press(&mut h, "OK");
    h.run_steps(3);
    assert!(out.is_file(), "{}", h.state().state.features.overlay.message);
    run(&mut h, "file.overlay");
    press(&mut h, "Auto Align");
    let out2 = temp("overlay").join("auto.pdf");
    script(&mut h, vec![out2.clone()]);
    press(&mut h, "OK");
    h.run_steps(3);
    assert!(out2.is_file(), "{}", h.state().state.features.overlay.message);
}

#[test]
fn batch_compare_wizard_matches_drags_to_repair_and_reports() {
    let mut h = harness();
    let dir = temp("batchcmp");
    let (cur, rev) = (dir.join("cur"), dir.join("rev"));
    std::fs::create_dir_all(&cur).unwrap();
    std::fs::create_dir_all(&rev).unwrap();
    for name in ["A-101 plan.pdf", "A-102 elev.pdf"] {
        std::fs::write(cur.join(name), markupcraft_render::synthetic::sample_pdf()).unwrap();
        std::fs::write(rev.join(name.replace(".pdf", " r2.pdf")), revised_sample()).unwrap();
    }
    run(&mut h, "batch.compare");
    assert!(shows(&h, "Batch Compare Documents"));
    script(&mut h, vec![cur.clone()]);
    let add = h.get_all_by_label("Add Folder...").next().unwrap();
    add.click();
    h.run_steps(4);
    script(&mut h, vec![rev.clone()]);
    press(&mut h, "Add Folder...");
    h.state_mut().state.features.batch_compare.job.filter = "@?#".into();
    press(&mut h, "Match");
    let pairs = h.state().state.features.batch_compare.job.pairs.clone();
    let pages = markupcraft_engine::Session::from_bytes(markupcraft_render::synthetic::sample_pdf(), "s.pdf")
        .unwrap()
        .page_count();
    assert_eq!(
        pairs.len(),
        2 * pages,
        "{}",
        h.state().state.features.batch_compare.message
    );
    // Drag the second revised sheet onto the first row: the two swap.
    let first = h.get_by_label("A-101 plan r2.pdf p1").rect().center();
    let second = h.get_by_label("A-102 elev r2.pdf p1").rect().center();
    drag_screen(&mut h, second, first);
    let swapped = h.state().state.features.batch_compare.job.pairs.clone();
    let moved = pairs
        .iter()
        .position(|p| p.revised.file.ends_with("A-102 elev r2.pdf") && p.revised.page == 0)
        .unwrap();
    assert_eq!(swapped[0].revised, pairs[moved].revised, "re-paired by dragging");
    // Save the batch file, run, and write the report.
    let job = dir.join("job.pcbatch");
    script(&mut h, vec![job.clone()]);
    press(&mut h, "Save Batch...");
    assert!(job.is_file());
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    script(&mut h, vec![out.clone()]);
    press(&mut h, "Run...");
    let rep = h.state().state.features.batch_compare.report.clone().expect("report");
    assert_eq!(
        rep.outputs.len(),
        2,
        "{}",
        h.state().state.features.batch_compare.message
    );
    assert!(rep.total_differences() > 0);
    let csv = dir.join("report.csv");
    script(&mut h, vec![csv.clone()]);
    press(&mut h, "Report...");
    assert!(std::fs::read_to_string(&csv).unwrap().contains("Differences"));
}

#[test]
fn search_scopes_targets_f3_replace_and_selected_text() {
    let mut h = harness();
    h.state_mut().open_bytes("revised.pdf", None, revised_sample()).unwrap();
    h.run_steps(4);
    // All open documents: BEDROOM is in both.
    key(&mut h, Modifiers::COMMAND, Key::F);
    {
        let s = &mut h.state_mut().state.features.search;
        s.query = "BEDROOM".into();
        s.scope = markupcraft_ui_egui::features::search::Scope::AllOpen;
    }
    press(&mut h, "Search");
    let hits = h.state().state.features.search.hits.clone();
    assert_eq!(hits.len(), 2, "{}", h.state().state.features.search.message);
    assert_ne!(hits[0].doc, hits[1].doc);
    // F3 / Shift+F3 step through them, switching documents.
    key(&mut h, Modifiers::NONE, Key::F3);
    assert_eq!(h.state().state.features.search.current, Some(1));
    assert_eq!(h.state().state.doc().unwrap().uid, hits[1].doc.unwrap());
    key(&mut h, Modifiers::SHIFT, Key::F3);
    assert_eq!(h.state().state.features.search.current, Some(0));
    // Back to this document: replace the checked result in the page text.
    h.state_mut().state.features.search.scope = markupcraft_ui_egui::features::search::Scope::AllPages;
    press(&mut h, "Search");
    h.state_mut().state.features.search.replace = "SUITE".into();
    h.run_steps(2);
    press(&mut h, "Replace Checked");
    let text = h.state().state.doc().unwrap().session.page_text(0).unwrap();
    assert!(
        text.contains("SUITE") && !text.contains("BEDROOM"),
        "{}",
        h.state().state.status
    );
    // Search Selected Text: drag over a word, it becomes the search.
    run(&mut h, "tools.search_selection");
    let hit = {
        let d = h.state().state.doc().unwrap();
        d.session.search_text("SUITE", &Default::default()).unwrap().hits[0].rects[0]
    };
    drag(&mut h, (hit.x0 - 2.0, hit.y0 - 2.0), (hit.x1 + 2.0, hit.y1 + 2.0));
    assert_eq!(h.state().state.features.search.query, "SUITE");
    assert!(!h.state().state.features.search.hits.is_empty());
}

#[test]
fn visual_search_options_and_result_thumbnails() {
    let mut h = harness();
    run(&mut h, "tools.visual_search");
    press(&mut h, "Select Region");
    drag(&mut h, (105.0, 720.0), (135.0, 750.0));
    h.state_mut().state.features.search.sensitivity = 0.3;
    h.run_steps(2);
    press(&mut h, "In 45-degree steps");
    press(&mut h, "Filter by color");
    press(&mut h, "Limit to selection");
    let s = &h.state().state.features.search;
    assert!(s.fine_rotations && s.color_filter && s.limit_to_selection);
    press(&mut h, "Search");
    h.run_steps(3);
    let s = &h.state().state.features.search;
    assert!(!s.visual_hits.is_empty(), "{}", s.message);
    assert!(!s.thumbs.is_empty(), "thumbnails made for the results");
}

#[test]
fn form_keys_add_a_signature_field_and_open_the_editor() {
    let mut h = harness();
    key(&mut h, Modifiers::NONE, Key::X);
    assert!(
        h.state().state.features.pick.is_some(),
        "X starts placing a signature field"
    );
    drag(&mut h, (300.0, 100.0), (450.0, 140.0));
    let fields = h.state().state.doc().unwrap().session.form_fields();
    assert!(fields.iter().any(|f| f.kind == "signature"), "{fields:?}");
    h.state_mut().state.features.forms.open = false;
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::F);
    assert!(h.state().state.features.forms.open);
}

#[test]
fn places_hyperlink_actions_and_markup_edit_action() {
    let mut h = harness();
    // A Place at the current page from the Links panel.
    h.state_mut().set_option("panel", "links");
    h.run_steps(3);
    h.state_mut().state.features.links.new_place = "Detail 1".into();
    h.run_steps(2);
    press(&mut h, "Add Here");
    assert_eq!(
        h.state().state.doc().unwrap().session.places().len(),
        1,
        "{}",
        h.state().state.features.links.message
    );
    // The Hyperlink tool to that Place.
    key(&mut h, Modifiers::SHIFT, Key::H);
    drag(&mut h, (980.0, 70.0), (1150.0, 110.0));
    press(&mut h, "Place");
    h.state_mut().state.features.links.place = "Detail 1".into();
    h.run_steps(2);
    press(&mut h, "OK");
    let links = h.state().state.doc().unwrap().session.links();
    assert_eq!(
        links[0].target,
        markupcraft_engine::links::LinkTarget::Place("Detail 1".into())
    );
    // Edit Action on the link: a view rectangle set to the current view.
    press(&mut h, "Edit Action");
    press(&mut h, "View");
    press(&mut h, "Set to Current View");
    press(&mut h, "OK");
    let links = h.state().state.doc().unwrap().session.links();
    assert!(
        matches!(links[0].target, markupcraft_engine::links::LinkTarget::View { .. }),
        "{:?}",
        links[0].target
    );
    // Edit Action on a markup (Ctrl+Shift+E).
    let id = markups(&h)[0].id.clone();
    markupcraft_ui_egui::actions::select(&mut h.state_mut().state.doc_mut().unwrap().session, vec![id.clone()]);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::E);
    assert!(shows(&h, "Edit Action"));
    press(&mut h, "Web address");
    h.state_mut().state.features.links.url = "https://example.com/rfi".into();
    h.run_steps(2);
    press(&mut h, "OK");
    let t = h.state().state.doc().unwrap().session.markup_action(&id).unwrap();
    assert_eq!(
        t,
        Some(markupcraft_engine::links::LinkTarget::Url(
            "https://example.com/rfi".into()
        ))
    );
}

#[test]
fn bookmarks_automark_properties_copy_action_and_audit() {
    let mut h = harness();
    h.state_mut().set_option("panel", "bookmarks");
    h.run_steps(3);
    press(&mut h, "AutoMark");
    // The sample's title text at the top-left.
    let hit = {
        let d = h.state().state.doc().unwrap();
        d.session.search_text("BEDROOM", &Default::default()).unwrap().hits[0].rects[0]
    };
    drag(&mut h, (hit.x0 - 2.0, hit.y0 - 2.0), (hit.x1 + 2.0, hit.y1 + 2.0));
    let b = h.state().state.doc().unwrap().session.bookmarks();
    assert!(
        !b.is_empty() && b[0].title.contains("BEDROOM"),
        "{b:?} {}",
        h.state().state.status
    );
    h.state_mut().state.features.bookmarks.selected = Some(vec![0]);
    h.state_mut().state.features.bookmarks.bold = true;
    h.run_steps(2);
    press(&mut h, "Apply");
    assert!(
        h.state()
            .state
            .doc()
            .unwrap()
            .session
            .bookmark_details(&[0])
            .unwrap()
            .style
            .bold
    );
    let n = h.state().state.doc().unwrap().session.bookmarks().len();
    press(&mut h, "Copy");
    assert_eq!(h.state().state.doc().unwrap().session.bookmarks().len(), n + 1);
    press(&mut h, "Action...");
    press(&mut h, "Place");
    h.state_mut().state.features.links.place = "Nowhere".into();
    h.run_steps(2);
    press(&mut h, "OK");
    press(&mut h, "Audit");
    assert_eq!(h.state().state.features.bookmarks.broken.len(), 1);
    assert!(shows(&h, "broken"));
}

#[test]
fn file_attachment_snapshots_and_capture_summary() {
    let mut h = harness();
    let dir = temp("capture");
    let file = dir.join("photo.jpg");
    std::fs::write(&file, b"JPGfake").unwrap();
    script(&mut h, vec![file.clone()]);
    // F is the File Attachment drawing tool; the Markup menu command asks for a place too.
    run(&mut h, "markup.file_attachment");
    assert!(
        h.state().state.features.pick.is_some(),
        "the command asks where the icon goes"
    );
    let at = screen(&h, 300.0, 300.0);
    h.hover_at(at);
    h.step();
    button(&mut h, at, true);
    button(&mut h, at, false);
    h.run_steps(3);
    assert!(
        markups(&h).iter().any(|m| m.kind == Kind::Attachment),
        "{}",
        h.state().state.status
    );
    let media = dir.join("media");
    std::fs::create_dir_all(&media).unwrap();
    script(&mut h, vec![media.clone()]);
    run(&mut h, "markup.capture_summary");
    assert!(media.join("photo.jpg").is_file() && media.join("Capture Summary.csv").is_file());
    // Copy Page to Snapshot, then paste.
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::C);
    assert_eq!(
        h.state().state.doc().unwrap().session.clipboard()[0].kind,
        Kind::Snapshot
    );
    // Snapshot from a space.
    let id = h
        .state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .add_space(
            0,
            "Hall",
            vec![
                Point::new(50.0, 50.0),
                Point::new(150.0, 50.0),
                Point::new(150.0, 120.0),
                Point::new(50.0, 120.0),
            ],
            None,
            None,
        )
        .unwrap();
    h.state_mut().set_option("panel", "spaces");
    h.state_mut().state.features.spaces.selected = Some(id);
    h.run_steps(3);
    press(&mut h, "Snapshot");
    let clip = h.state().state.doc().unwrap().session.clipboard()[0].clone();
    assert_eq!(clip.rect, markupcraft_geom::Rect::new(50.0, 50.0, 150.0, 120.0));
}

#[test]
fn layers_tree_drag_configurations_page_only_preview_import_and_export() {
    let mut h = harness();
    let dir = temp("layers");
    let src = dir.join("power.pdf");
    std::fs::write(&src, markupcraft_render::synthetic::sample_pdf()).unwrap();
    h.state_mut().set_option("panel", "layers");
    h.run_steps(3);
    for n in ["Arch", "Doors"] {
        h.state_mut().state.features.layers.new_name = n.into();
        h.run_steps(1);
        press(&mut h, "New");
    }
    // Drag Doors onto Arch: Doors nests under it.
    let a = h.get_all_by_label("Arch").next().unwrap().rect().center();
    let b = h.get_all_by_label("Doors").next().unwrap().rect().center();
    drag_screen(&mut h, b, a);
    let tree = h.state().state.doc().unwrap().session.layer_tree();
    assert!(
        tree.iter()
            .any(|n| n.name == "Doors" && n.parent.as_deref() == Some("Arch")),
        "{tree:?}"
    );
    press(&mut h, "Top Level");
    // A configuration.
    h.state_mut().state.features.layers.config_name = "Arch only".into();
    h.run_steps(1);
    press(&mut h, "Save");
    assert_eq!(
        h.state().state.doc().unwrap().session.layer_configs(),
        vec!["Arch only".to_string()]
    );
    // Import a page as a layer, then this page only lists it.
    script(&mut h, vec![src.clone()]);
    press(&mut h, "Import...");
    h.run_steps(2);
    assert!(
        h.state()
            .state
            .doc()
            .unwrap()
            .session
            .layers()
            .iter()
            .any(|l| l.name == "power")
    );
    h.state_mut().state.features.layers.page_only = true;
    h.run_steps(2);
    assert!(!shows(&h, "Doors"), "Doors is on no page");
    h.state_mut().state.features.layers.page_only = false;
    h.run_steps(2);
    // Print preview, then end it.
    press(&mut h, "Print Layers");
    assert!(h.state().state.features.layers.previewing);
    press(&mut h, "End Preview");
    assert!(!h.state().state.features.layers.previewing);
    // Export the imported layer.
    h.state_mut().state.features.layers.selected = Some("power".into());
    h.run_steps(2);
    let out = dir.join("power-only.pdf");
    script(&mut h, vec![out.clone()]);
    press(&mut h, "Export...");
    assert!(out.is_file(), "{}", h.state().state.status);
}

#[test]
fn sets_categories_revisions_tags_publish_and_print() {
    let mut h = harness();
    let dir = temp("sets2");
    let files: Vec<PathBuf> = ["A-101 Plan.pdf", "A-101 Plan Rev 2.pdf", "M-201 Mech.pdf"]
        .iter()
        .map(|n| dir.join(n))
        .collect();
    for f in &files {
        std::fs::write(f, markupcraft_render::synthetic::sample_pdf()).unwrap();
    }
    h.state_mut().set_option("panel", "sets");
    h.run_steps(3);
    script(&mut h, files.clone());
    press(&mut h, "Add Files...");
    {
        let s = &mut h.state_mut().state.features.sets;
        s.categories = markupcraft_engine::sets_more::CategoryMode::FileName;
        s.revision_filter = "@?#".into();
        s.previous = 1;
        s.show_tags = true;
    }
    h.run_steps(3);
    assert!(shows(&h, "Architectural") && shows(&h, "Mechanical"));
    h.state_mut().state.features.sets.tag_sheet = "M-201 Mech.pdf#1".into();
    h.state_mut().state.features.sets.tag_name = "Phase".into();
    h.state_mut().state.features.sets.tag_value = "CD".into();
    h.run_steps(1);
    press(&mut h, "Set Tag");
    assert!(h.state().state.features.sets.set.tags.contains_key("M-201 Mech.pdf#1"));
    let out = dir.join("published.pdf");
    script(&mut h, vec![out.clone()]);
    press(&mut h, "Publish...");
    let p = markupcraft_engine::Session::open(&out).unwrap();
    assert!(p.bookmarks().len() >= 2, "{}", h.state().state.features.sets.message);
    let pkg = dir.join("pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    script(&mut h, vec![pkg.clone()]);
    press(&mut h, "Package...");
    assert!(pkg.join("Drawing Log.csv").is_file());
    let pr = dir.join("set-print.pdf");
    script(&mut h, vec![pr.clone()]);
    press(&mut h, "Print Set...");
    assert!(pr.is_file(), "{}", h.state().state.features.sets.message);
}

#[test]
fn summary_filters_then_sort_output_and_pdf_layout() {
    let mut h = harness();
    run(&mut h, "markup.summary");
    {
        let s = &mut h.state_mut().state.features.summary;
        s.sort = "subject".into();
        s.then_sort = "type".into();
        s.then_descending = true;
        s.content = markupcraft_engine::summary::SummaryContent::Markups;
        s.headers = false;
        s.layout.flow = true;
        s.layout.break_per_group = true;
        s.group_by = "type".into();
    }
    h.run_steps(2);
    assert!(shows(&h, "Then by:"));
    let dir = temp("summary2");
    let csv = dir.join("s.csv");
    script(&mut h, vec![csv.clone()]);
    press(&mut h, "CSV");
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(!text.starts_with("Subject") && !text.contains("Total"), "{text}");
    let pdf = dir.join("s.pdf");
    script(&mut h, vec![pdf.clone()]);
    press(&mut h, "PDF");
    let back = markupcraft_engine::Session::open(&pdf).unwrap();
    assert!(back.page_text(0).unwrap().contains("Subject:"), "flow layout");
    assert_eq!(back.links().len(), 6);
    h.state_mut().state.features.summary.per_value = true;
    h.run_steps(1);
    script(&mut h, vec![dir.join("per.csv")]);
    press(&mut h, "CSV");
    assert!(
        h.state().state.status.starts_with("Wrote"),
        "{}",
        h.state().state.status
    );
}

#[test]
fn print_dialog_copies_window_markups_only_emphasis_and_batch_print() {
    let mut h = harness();
    key(&mut h, Modifiers::COMMAND, Key::P);
    assert!(shows(&h, "Copies"));
    {
        let p = &mut h.state_mut().state.features.print;
        p.copies = 2;
        p.collate = false;
        p.reverse = true;
        p.margin = 36.0;
        p.spaces = true;
        p.links = true;
        p.dim_content = true;
    }
    h.run_steps(1);
    press(&mut h, "Get Window");
    drag(&mut h, (50.0, 50.0), (400.0, 400.0));
    assert!(h.state().state.features.print.region.is_some());
    press(&mut h, "Markups only");
    let out = temp("print2").join("job.pdf");
    script(&mut h, vec![out.clone()]);
    press(&mut h, "Save Print PDF...");
    let s = markupcraft_engine::Session::open(&out).unwrap();
    assert_eq!(
        s.page_count(),
        2,
        "one region, two copies: {}",
        h.state().state.features.print.message
    );
    // Batch Print with the dialog's settings.
    h.state_mut().state.features.print.region = None;
    h.state_mut().state.features.print.copies = 1;
    let dir = temp("batchprint");
    let f = dir.join("a.pdf");
    std::fs::write(&f, markupcraft_render::synthetic::sample_pdf()).unwrap();
    run(&mut h, "batch.print");
    script(&mut h, vec![f.clone()]);
    press(&mut h, "Add Files...");
    script(&mut h, vec![dir.clone()]);
    press(&mut h, "Run");
    assert!(
        dir.join("a_print.pdf").is_file(),
        "{}",
        h.state().state.features.batch.message
    );
}

#[test]
fn security_presets_status_and_kept_header_footer_templates() {
    let mut h = harness();
    let dir = temp("presets");
    run(&mut h, "document.security");
    {
        let s = &mut h.state_mut().state.features.docops;
        s.presets_dir = Some(dir.clone());
        s.permissions_password = "owner".into();
        s.permissions.print = false;
        s.preset_name = "No printing".into();
    }
    h.run_steps(1);
    assert!(shows(&h, "Status: No security"));
    press(&mut h, "Save Preset");
    assert!(
        dir.join("security_presets.json").is_file(),
        "{}",
        h.state().state.features.docops.message
    );
    press(&mut h, "No printing");
    h.run_steps(2);
    assert!(
        shows(&h, "Status: Printing or editing limited"),
        "{}",
        h.state().state.features.docops.message
    );
    // Header & Footer kept with the document, shrunk content, Edit, Update and templates.
    run(&mut h, "document.headers_footers");
    {
        let s = &mut h.state_mut().state.features.docops;
        s.hf.text[1] = "SHEET <<1>>".into();
        s.fit_content = true;
        s.template_name = "Sheets".into();
    }
    h.run_steps(1);
    press(&mut h, "Apply");
    let kept = h.state().state.doc().unwrap().session.kept_header_footer();
    assert!(
        kept.as_ref()
            .is_some_and(|t| t.fit_content && t.text[1] == "SHEET <<1>>"),
        "{kept:?}"
    );
    h.state_mut().state.features.docops.hf.text[1].clear();
    press(&mut h, "Edit");
    assert_eq!(h.state().state.features.docops.hf.text[1], "SHEET <<1>>");
    press(&mut h, "Update");
    assert!(h.state().state.features.docops.message.contains("updated"));
    press(&mut h, "Save Template");
    assert!(dir.join("header_footer_templates.json").is_file());
    press(&mut h, "Sheets");
    assert!(
        h.state()
            .state
            .features
            .docops
            .message
            .contains("Template Sheets applied")
    );
    run(&mut h, "document.hf_update");
    assert!(h.state().state.status.contains("updated"), "{}", h.state().state.status);
    // Reduce File Size runs on a worker thread with the new options.
    run(&mut h, "document.reduce");
    h.state_mut().state.features.docops.reduce.discard_metadata = true;
    press(&mut h, "Discard private application data");
    press(&mut h, "Reduce");
    for _ in 0..200 {
        if h.state().state.features.docops.reducing.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        h.step();
    }
    h.run_steps(2);
    assert!(
        h.state().state.features.docops.message.starts_with("Reduced"),
        "{}",
        h.state().state.features.docops.message
    );
}

#[test]
fn redaction_properties_scrub_form_data_and_legend_distribution() {
    use markupcraft_engine::forms::{FillValue, NewFieldKind};
    let mut h = harness();
    run(&mut h, "document.redaction_properties");
    assert!(shows(&h, "Redaction Properties"));
    press(&mut h, "Courier");
    press(&mut h, "Repeat to fill");
    h.state_mut().state.features.redact.style.overlay = "(b)(6)".into();
    h.run_steps(1);
    let st = h.state().state.features.redact.style.clone();
    assert_eq!(st.font, "Courier");
    assert!(st.repeat);
    h.state_mut().state.features.redact.properties_open = false;
    run(&mut h, "document.mark_redaction");
    drag(&mut h, (60.0, 60.0), (200.0, 200.0));
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(
        h.state().state.doc().unwrap().session.redact_marks()[0].overlay,
        "(b)(6)"
    );
    run(&mut h, "document.apply_redactions");
    press(
        &mut h,
        "Also remove document properties, metadata, attachments and scripts",
    );
    assert!(h.state().state.features.redact.scrub);
    press(&mut h, "Apply");
    assert!(
        h.state().state.status.starts_with("Redacted"),
        "{}",
        h.state().state.status
    );
    // Form data out and back in; Typewriter text into its field; automatic fields.
    {
        let d = h.state_mut().state.doc_mut().unwrap();
        let r = markupcraft_geom::Rect::new(100.0, 100.0, 300.0, 130.0);
        d.session
            .form_add_field(0, r, &NewFieldKind::Text { multiline: false }, Some("Name"))
            .unwrap();
        d.session
            .form_fill(&[("Name".into(), FillValue::Text("Ada".into()))])
            .unwrap();
    }
    let data = temp("formdata").join("data.json");
    script(&mut h, vec![data.clone()]);
    run(&mut h, "forms.export_data");
    assert!(data.is_file(), "{}", h.state().state.status);
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .form_fill(&[("Name".into(), FillValue::Text(String::new()))])
        .unwrap();
    script(&mut h, vec![data]);
    run(&mut h, "forms.import_data");
    assert_eq!(h.state().state.doc().unwrap().session.form_values()["Name"], "Ada");
    {
        let d = h.state_mut().state.doc_mut().unwrap();
        let mut tw = markupcraft_model::Markup::new(
            Kind::Typewriter,
            0,
            vec![Point::new(110.0, 105.0), Point::new(290.0, 125.0)],
        );
        tw.contents = "Grace".into();
        d.session.add_markup(tw).unwrap();
    }
    run(&mut h, "forms.typewriter_to_fields");
    assert_eq!(
        h.state().state.doc().unwrap().session.form_values()["Name"],
        "Grace",
        "{}",
        h.state().state.status
    );
    run(&mut h, "forms.auto_fields");
    assert!(
        h.state().state.status.starts_with("Created"),
        "{}",
        h.state().state.status
    );
    // Legend options, Copy Legend to Pages and Snapshot Legend.
    run(&mut h, "measure.legend");
    press(&mut h, "More options");
    press(&mut h, "Header row");
    assert!(!h.state().state.features.fill.legend.header);
    h.state_mut().state.features.fill.legend_open = false;
    let o = h.state().state.features.fill.legend.clone();
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .add_legend(0, Point::new(50.0, 700.0), &o)
        .unwrap();
    run(&mut h, "measure.legend_copy");
    assert_eq!(
        h.state().state.doc().unwrap().session.legends().len(),
        2,
        "{}",
        h.state().state.status
    );
    run(&mut h, "measure.legend_freeze");
    assert_eq!(
        h.state().state.doc().unwrap().session.legends().len(),
        1,
        "{}",
        h.state().state.status
    );
}

#[test]
fn batch_create_layered_merge_combine_options_and_link_terms() {
    let mut h = harness();
    let dir = temp("batch5");
    let a = dir.join("A-101.pdf");
    let b = dir.join("A-102.pdf");
    std::fs::write(&a, markupcraft_render::synthetic::sample_pdf()).unwrap();
    std::fs::write(&b, markupcraft_render::synthetic::sample_pdf()).unwrap();
    let txt = dir.join("notes.txt");
    std::fs::write(&txt, "Line one\nLine two").unwrap();
    // Create PDF from files.
    run(&mut h, "file.create_from_files");
    script(&mut h, vec![txt.clone()]);
    press(&mut h, "Add Files...");
    let made = dir.join("made.pdf");
    script(&mut h, vec![made.clone()]);
    press(&mut h, "Run");
    assert!(made.is_file(), "{}", h.state().state.features.batch.message);
    // Layered PDF.
    run(&mut h, "file.layered");
    script(&mut h, vec![a.clone(), b.clone()]);
    press(&mut h, "Add Files...");
    let layered = dir.join("layered.pdf");
    script(&mut h, vec![layered.clone()]);
    press(&mut h, "Run");
    let names: Vec<String> = markupcraft_engine::Session::open(&layered)
        .unwrap()
        .layers()
        .into_iter()
        .map(|l| l.name)
        .collect();
    assert_eq!(names, ["A-101", "A-102"]);
    // Merge Form Data.
    run(&mut h, "forms.merge_data");
    script(&mut h, vec![a.clone(), b.clone()]);
    press(&mut h, "Add Files...");
    let csv = dir.join("forms.csv");
    script(&mut h, vec![csv.clone()]);
    press(&mut h, "Run");
    assert!(csv.is_file(), "{}", h.state().state.features.batch.message);
    // Combine with page labels from the file names.
    run(&mut h, "file.combine");
    script(&mut h, vec![a.clone(), b.clone()]);
    press(&mut h, "Add Files...");
    press(&mut h, "Page labels from file names");
    let comb = dir.join("combined.pdf");
    script(&mut h, vec![comb.clone()]);
    press(&mut h, "Run");
    let s = markupcraft_engine::Session::open(&comb).unwrap();
    assert!(
        s.doc().pages.iter().any(|p| p.label.starts_with("A-102")),
        "{:?}",
        s.page_labels()
    );
    // Batch Link with custom terms and highlights.
    run(&mut h, "batch.link");
    script(&mut h, vec![a.clone(), b.clone()]);
    press(&mut h, "Add Files...");
    press(&mut h, "Custom terms");
    press(&mut h, "Highlight links");
    h.state_mut().state.features.batch.link_custom = "GENERAL NOTES, A-102.pdf, 2\n".into();
    h.run_steps(1);
    press(&mut h, "Run");
    let msg = h.state().state.features.batch.message.clone();
    assert!(msg.starts_with("Added") && !msg.starts_with("Added 0"), "{msg}");
}

#[test]
fn ocr_runs_on_a_worker_with_deskew_and_orientation() {
    let mut h = harness();
    run(&mut h, "tools.ocr");
    {
        let s = &mut h.state_mut().state.features.ocr;
        s.pages = "2".into();
        s.dpi = 150.0;
    }
    h.run_steps(1);
    press(&mut h, "Skip pages that already have text");
    press(&mut h, "Correct skew");
    press(&mut h, "Detect orientation (and vertical text)");
    press(&mut h, "Run OCR");
    let started = h.state().state.features.ocr.busy();
    for _ in 0..1500 {
        if !h.state().state.features.ocr.busy() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        h.step();
    }
    h.run_steps(2);
    let msg = h.state().state.features.ocr.message.clone();
    if started {
        // The models are installed: the worker read the notes page and the window kept painting.
        assert!(msg.starts_with("OCR:"), "{msg}");
    } else {
        assert!(msg.to_lowercase().contains("model"), "{msg}");
    }
}

#[test]
fn quantity_link_dialog_and_folder_summary_to_excel() {
    let mut h = harness();
    let dir = temp("qty");
    let f = dir.join("plan.pdf");
    std::fs::write(&f, markupcraft_render::synthetic::sample_pdf()).unwrap();
    h.state_mut().state.open_path(&f);
    h.run_steps(4);
    run(&mut h, "measure.quantity_link");
    {
        let q = &mut h.state_mut().state.features.quantity;
        q.name = "Rectangles".into();
        q.cell = "C3".into();
        q.subjects = "Rectangle".into();
    }
    h.run_steps(1);
    press(&mut h, "Add Link");
    assert_eq!(
        h.state().state.features.quantity.links.len(),
        1,
        "{}",
        h.state().state.features.quantity.message
    );
    let out = dir.join("q.xlsx");
    script(&mut h, vec![out.clone()]);
    press(&mut h, "Update Workbook...");
    assert!(out.is_file(), "{}", h.state().state.features.quantity.message);
    let v = h.state().state.features.quantity.values.clone();
    assert!(v.first().is_some_and(|v| v.value >= 1.0), "{v:?}");
    let links = dir.join("links.json");
    script(&mut h, vec![links.clone()]);
    press(&mut h, "Save Links...");
    h.state_mut().state.features.quantity.links.clear();
    script(&mut h, vec![links]);
    press(&mut h, "Open Links...");
    assert_eq!(h.state().state.features.quantity.links.len(), 1);
    h.state_mut().state.features.quantity.open = false;
    // Batch Summary of a folder into Excel.
    run(&mut h, "batch.summary");
    script(&mut h, vec![dir.clone()]);
    press(&mut h, "Add Folder...");
    assert_eq!(h.state().state.features.batch.files.len(), 1);
    let xl = dir.join("summary.xlsx");
    script(&mut h, vec![xl.clone()]);
    press(&mut h, "Run");
    assert!(xl.is_file(), "{}", h.state().state.features.batch.message);
}

#[test]
fn dynamic_fill_drag_boundaries_polylength_and_volume() {
    use markupcraft_engine::synthetic::{SyntheticPage, line, pdf};
    let mut h = harness();
    let content = format!(
        "{}{}{}{}{}",
        line(100.0, 100.0, 500.0, 100.0, 1.0),
        line(500.0, 100.0, 500.0, 400.0, 1.0),
        line(500.0, 400.0, 100.0, 400.0, 1.0),
        line(100.0, 400.0, 100.0, 100.0, 1.0),
        line(300.0, 100.0, 300.0, 400.0, 1.0)
    );
    let bytes = pdf(&[SyntheticPage::new(612.0, 792.0, content)]);
    h.state_mut().open_bytes("rooms.pdf", None, bytes).unwrap();
    h.run_steps(6);
    run(&mut h, "measure.dynamic_fill");
    press(&mut h, "Drag across regions to fill each one");
    assert!(matches!(
        h.state().state.features.pick,
        Some((_, markupcraft_ui_egui::features::Pick::FillDrag))
    ));
    drag(&mut h, (150.0, 250.0), (450.0, 250.0));
    let areas = markups(&h).iter().filter(|m| m.kind == Kind::Area).count();
    assert_eq!(areas, 2, "{}", h.state().state.features.fill.message);
    // Back to clicks: a Volume, then a Polylength split by a boundary line.
    press(&mut h, "Drag across regions to fill each one");
    press(&mut h, "Volume");
    h.state_mut().state.features.fill.depth = 2.5;
    h.run_steps(1);
    let at = screen(&h, 200.0, 250.0);
    h.hover_at(at);
    h.step();
    button(&mut h, at, true);
    button(&mut h, at, false);
    h.run_steps(3);
    let vol = markups(&h).into_iter().find(|m| m.kind == Kind::Volume);
    assert!(
        vol.is_some_and(|m| m.depth == 2.5),
        "{}",
        h.state().state.features.fill.message
    );
    markupcraft_ui_egui::features::fill::boundary_picked(
        &mut h.state_mut().state,
        vec![Point::new(300.0, 250.0), Point::new(500.0, 250.0)],
    );
    h.run_steps(2);
    assert_eq!(h.state().state.features.fill.boundaries.len(), 1);
    press(&mut h, "Polylength");
    let at = screen(&h, 400.0, 300.0);
    h.hover_at(at);
    h.step();
    button(&mut h, at, true);
    button(&mut h, at, false);
    h.run_steps(3);
    let pl = markups(&h).into_iter().find(|m| m.kind == Kind::Polylength);
    let ys: Vec<f64> = pl.map(|m| m.pts.iter().map(|p| p.y).collect()).unwrap_or_default();
    assert!(
        !ys.is_empty() && ys.iter().all(|y| *y >= 249.0),
        "the boundary halves the room: {ys:?} {}",
        h.state().state.features.fill.message
    );
    press(&mut h, "Clear Boundaries (1)");
    assert!(h.state().state.features.fill.boundaries.is_empty());
}
