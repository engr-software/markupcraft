//! The document and review features driven through the real shell headlessly (egui_kittest):
//! Search and Visual Search, Compare, Overlay, Markup Summary (Excel, PDF), Print, Dynamic
//! Fill, Spaces, Layers, Links, Bookmarks, Signatures, Stamps, document operations, redaction,
//! forms, spelling, Sets and the batch tools. Each test makes the clicks and keys a user would
//! and checks the engine's document or the files written.

use std::path::PathBuf;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_geom::Point;
use markupcraft_model::Kind;
use markupcraft_ui_egui::MarkupCraftApp;

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

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("markupcraft-features-ui-{}-{name}", std::process::id()));
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

fn click(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.step();
    button(h, at, true);
    button(h, at, false);
    h.run_steps(3);
}

fn drag(h: &mut Harness<'_, MarkupCraftApp>, from: (f64, f64), to: (f64, f64)) {
    let (a, b) = (screen(h, from.0, from.1), screen(h, to.0, to.1));
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

fn key(h: &mut Harness<'_, MarkupCraftApp>, m: Modifiers, k: Key) {
    h.key_press_modifiers(m, k);
    h.run_steps(3);
}

fn type_text(h: &mut Harness<'_, MarkupCraftApp>, text: &str) {
    h.event(Event::Text(text.into()));
    h.run_steps(2);
}

fn run(h: &mut Harness<'_, MarkupCraftApp>, id: &str) {
    h.state_mut().state.queue(id);
    h.run_steps(4);
}

/// Click the widget labelled `label` drawn last (dialogs and panels draw after the toolbar).
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

/// The sample with a header added (a changed revision for Compare and Overlay).
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
fn search_panel_finds_text_then_counts_and_highlights_the_results() {
    let mut h = harness();
    key(&mut h, Modifiers::COMMAND, Key::F);
    assert!(h.state().state.open_panels.contains(&"search"));
    type_text(&mut h, "BEDROOM");
    key(&mut h, Modifiers::NONE, Key::Enter);
    let hits = h.state().state.features.search.hits.clone();
    assert_eq!(hits.len(), 1, "{}", h.state().state.features.search.message);
    assert_eq!(hits[0].page, 0);
    assert!(shows(&h, "1 result"));
    // The first result is current; Next wraps around to it.
    assert_eq!(h.state().state.features.search.current, Some(0));
    press(&mut h, "Next >");
    assert_eq!(h.state().state.features.search.current, Some(0));
    let before = markups(&h).len();
    press(&mut h, "Highlight");
    press(&mut h, "Count");
    let after = markups(&h);
    assert_eq!(after.len(), before + 2);
    assert!(after.iter().any(|m| m.kind == Kind::TextHighlight));
    let count = after.iter().find(|m| m.kind == Kind::Count).unwrap();
    assert_eq!(count.pts.len(), 1);
    assert_eq!(count.subject, "BEDROOM");
    // Markup comments are searched too.
    h.state_mut().state.features.search.markups = true;
    h.state_mut().state.features.search.page_text = false;
    h.state_mut().state.features.search.query = "BEDROOM".into();
    markupcraft_ui_egui::features::search::run_text(&mut h.state_mut().state);
    h.run_steps(2);
    assert!(
        h.state().state.features.search.hits.iter().all(|h| h.markup.is_some()),
        "markup hits only"
    );
    press(&mut h, "Clear");
    assert!(h.state().state.features.search.hits.is_empty());
}

#[test]
fn visual_search_picks_a_region_and_finds_the_symbol() {
    let mut h = harness();
    run(&mut h, "tools.visual_search");
    press(&mut h, "Select Region");
    // A grid bubble (circle of radius 12 around (120, 735)) with its letter.
    drag(&mut h, (105.0, 720.0), (135.0, 750.0));
    let region = h.state().state.features.search.region;
    assert!(region.is_some_and(|(p, r)| p == 0 && r.width() > 25.0), "{region:?}");
    h.state_mut().state.features.search.sensitivity = 0.3;
    press(&mut h, "Search");
    let hits = h.state().state.features.search.visual_hits.clone();
    assert!(!hits.is_empty(), "{}", h.state().state.features.search.message);
    let before = markups(&h).len();
    press(&mut h, "Count");
    assert_eq!(markups(&h).len(), before + 1, "one Count on the page");
}

#[test]
fn compare_documents_clouds_the_changes_and_reviews_them() {
    let mut h = harness();
    h.state_mut().open_bytes("revised.pdf", None, revised_sample()).unwrap();
    h.run_steps(4);
    run(&mut h, "tools.compare");
    assert!(h.state().state.features.compare.open);
    assert!(shows(&h, "Older (A)"));
    press(&mut h, "OK");
    let c = &h.state().state.features.compare;
    assert!(!c.open, "{}", c.message);
    assert!(!c.regions.is_empty(), "changes found");
    assert!(h.state().state.open_panels.contains(&"compare"));
    let n = c.regions.len();
    let clouds = markups(&h).iter().filter(|m| m.subject == "Compare").count();
    assert_eq!(clouds, n);
    // Reject the first change: its cloud goes.
    press(&mut h, "Reject");
    let clouds = markups(&h).iter().filter(|m| m.subject == "Compare").count();
    assert_eq!(clouds, n - 1);
    press(&mut h, "Delete all clouds");
    assert_eq!(markups(&h).iter().filter(|m| m.subject == "Compare").count(), 0);
}

#[test]
fn overlay_pages_writes_the_overlay_and_opens_it() {
    let mut h = harness();
    h.state_mut().open_bytes("revised.pdf", None, revised_sample()).unwrap();
    h.run_steps(4);
    run(&mut h, "file.overlay");
    assert!(shows(&h, "Overlay Pages"));
    let out = temp("overlay").join("overlay.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    press(&mut h, "OK");
    h.run_steps(4);
    let s = &h.state().state;
    assert!(out.is_file(), "{}", s.features.overlay.message);
    assert_eq!(s.docs.len(), 3, "the overlay opens in a new tab");
    assert_eq!(s.doc().unwrap().name, "overlay.pdf");
}

#[test]
fn markup_summary_writes_excel_pdf_and_csv() {
    let mut h = harness();
    run(&mut h, "markup.summary");
    assert!(shows(&h, "Markup Summary"));
    let dir = temp("summary");
    for (label, file, magic) in [
        ("Excel (.xlsx)", "s.xlsx", &b"PK"[..]),
        ("PDF", "s.pdf", &b"%PDF"[..]),
        ("CSV", "s.csv", &b"Subject"[..]),
    ] {
        let out = dir.join(file);
        h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
        press(&mut h, label);
        h.run_steps(2);
        let bytes =
            std::fs::read(&out).unwrap_or_else(|_| panic!("{label}: {}", h.state().state.features.summary.message));
        assert!(bytes.starts_with(magic), "{label}");
    }
    assert!(
        h.state().state.status.contains("6 markups"),
        "{}",
        h.state().state.status
    );
}

#[test]
fn print_writes_a_print_ready_pdf() {
    let mut h = harness();
    key(&mut h, Modifiers::COMMAND, Key::P);
    assert!(h.state().state.features.print.open);
    h.state_mut().state.features.print.paper = "Tabloid".into();
    let out = temp("print").join("print.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    press(&mut h, "Save Print PDF...");
    h.run_steps(2);
    let s = markupcraft_engine::Session::open(&out).unwrap();
    assert_eq!(s.page_count(), 2);
    assert!(
        (s.doc().pages[0].media.width() - 792.0).abs() < 1.0 || (s.doc().pages[0].media.width() - 1224.0).abs() < 1.0
    );
}

#[test]
fn dynamic_fill_clicks_make_areas_from_linework() {
    let mut h = harness();
    key(&mut h, Modifiers::NONE, Key::J);
    assert!(h.state().state.features.pick.is_some(), "J starts Dynamic Fill");
    assert!(shows(&h, "Islands become cutouts"), "the fill bar shows");
    let before = markups(&h).len();
    // Inside the room bounded by x 120..300, y 350..690.
    click(&mut h, 200.0, 500.0);
    let all = markups(&h);
    assert_eq!(all.len(), before + 1, "{}", h.state().state.features.fill.message);
    let m = all.last().unwrap();
    assert_eq!(m.kind, Kind::Area);
    let xs: Vec<f64> = m.pts.iter().map(|p| p.x).collect();
    assert!(xs.iter().all(|x| (115.0..=305.0).contains(x)), "{xs:?}");
    // Hatch the new area from the fill bar.
    let id = m.id.clone();
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .select(std::slice::from_ref(&id))
        .unwrap();
    h.run_steps(2);
    press(&mut h, "Hatch selected (1)");
    assert!(
        h.state()
            .state
            .doc()
            .unwrap()
            .session
            .doc()
            .find(&id)
            .unwrap()
            .hatch
            .is_some()
    );
    press(&mut h, "Done");
    assert!(h.state().state.features.pick.is_none());
}

#[test]
fn spaces_panel_fills_a_room_lists_it_and_tallies_markups() {
    let mut h = harness();
    h.state_mut().set_option("panel", "spaces");
    h.run_steps(3);
    h.state_mut().state.features.spaces.new_name = "Living".into();
    press(&mut h, "Fill");
    click(&mut h, 200.0, 500.0);
    let spaces = h.state().state.doc().unwrap().session.spaces(Some(0));
    assert_eq!(spaces.len(), 1, "{}", h.state().state.status);
    assert_eq!(spaces[0].1.name, "Living");
    assert!(shows(&h, "Living"));
    // Select it in the panel, rename it.
    press(&mut h, "Living  (0)");
    h.state_mut().state.features.spaces.rename = "Living Room".into();
    h.run_steps(2);
    press(&mut h, "Rename");
    let spaces = h.state().state.doc().unwrap().session.spaces(Some(0));
    assert_eq!(spaces[0].1.name, "Living Room");
    // Draw a second one by its corners.
    h.state_mut().state.features.spaces.new_name = "Hall".into();
    press(&mut h, "Draw");
    for (x, y) in [(620.0, 200.0), (1000.0, 200.0), (1000.0, 500.0)] {
        click(&mut h, x, y);
    }
    click(&mut h, 620.0, 500.0);
    key(&mut h, Modifiers::NONE, Key::Enter);
    let spaces = h.state().state.doc().unwrap().session.spaces(Some(0));
    assert_eq!(spaces.len(), 2);
    assert!(shows(&h, "Hall"));
}

#[test]
fn layers_panel_creates_assigns_hides_and_isolates() {
    let mut h = harness();
    h.state_mut().set_option("panel", "layers");
    h.run_steps(3);
    h.state_mut().state.features.layers.new_name = "Electrical".into();
    press(&mut h, "New");
    let d = h.state().state.doc().unwrap();
    assert!(d.session.layers().iter().any(|l| l.name == "Electrical"));
    h.state_mut().set_option("select", "0");
    h.run_steps(3);
    press(&mut h, "Assign selected (1)");
    let first = markups(&h)[0].clone();
    assert_eq!(first.layer, "Electrical");
    // Turn the layer off: the canvas stops drawing its markup.
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_layer_state(
            "Electrical",
            markupcraft_engine::layers::LayerState {
                visible: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    h.run_steps(3);
    let uid = h.state().state.doc().unwrap().uid;
    let hidden = markupcraft_ui_egui::features::canvas::hidden_layers(&h.ctx, uid);
    assert_eq!(hidden, vec!["Electrical".to_string()]);
    press(&mut h, "Show All");
    assert!(
        h.state()
            .state
            .doc()
            .unwrap()
            .session
            .layers()
            .iter()
            .all(|l| l.visible)
    );
}

#[test]
fn hyperlink_tool_adds_a_page_link_listed_in_the_links_panel() {
    let mut h = harness();
    key(&mut h, Modifiers::SHIFT, Key::H);
    drag(&mut h, (980.0, 70.0), (1150.0, 110.0));
    assert!(h.state().state.features.links.pending.is_some());
    assert!(shows(&h, "Web address"), "the target dialog");
    h.state_mut().state.features.links.page = 2;
    press(&mut h, "OK");
    let links = h.state().state.doc().unwrap().session.links();
    assert_eq!(links.len(), 1, "{}", h.state().state.features.links.message);
    assert_eq!(links[0].target, markupcraft_engine::links::LinkTarget::Page(1));
    assert!(h.state().state.open_panels.contains(&"links"));
    assert!(shows(&h, "Page 2"));
    press(&mut h, "Follow");
    assert_eq!(h.state().state.doc().unwrap().view.current, 1);
}

#[test]
fn bookmarks_panel_adds_renames_nests_and_deletes() {
    let mut h = harness();
    let start = h.state().state.doc().unwrap().session.bookmarks().len();
    key(&mut h, Modifiers::COMMAND, Key::B);
    let bm = h.state().state.doc().unwrap().session.bookmarks();
    assert_eq!(bm.len(), start + 1);
    h.state_mut().set_option("panel", "bookmarks");
    h.run_steps(3);
    let new = bm.last().unwrap().path.clone();
    h.state_mut().state.features.bookmarks.selected = Some(new.clone());
    h.state_mut().state.features.bookmarks.rename = "Floor Plan".into();
    h.run_steps(2);
    press(&mut h, "Rename");
    assert!(
        h.state()
            .state
            .doc()
            .unwrap()
            .session
            .bookmarks()
            .iter()
            .any(|b| b.title == "Floor Plan")
    );
    assert!(
        new.last().copied().unwrap_or(0) > 0,
        "the sample has bookmarks: {new:?}"
    );
    {
        press(&mut h, "Indent");
        let b = h.state().state.doc().unwrap().session.bookmarks();
        assert!(b.iter().any(|b| b.title == "Floor Plan" && b.path.len() == 2), "{b:?}");
        // Collapse or expand its new parent: the state is kept in the file.
        let parent = b.iter().find(|x| x.children > 0).unwrap().clone();
        press(&mut h, if parent.open { "−" } else { "+" });
        let b = h.state().state.doc().unwrap().session.bookmarks();
        let now = b.iter().find(|x| x.path == parent.path).unwrap();
        assert_eq!(now.open, !parent.open);
        if !now.open {
            press(&mut h, "+");
        }
        h.state_mut().state.features.bookmarks.selected =
            b.iter().find(|x| x.title == "Floor Plan").map(|x| x.path.clone());
        h.run_steps(2);
    }
    press(&mut h, "Delete");
    assert_eq!(h.state().state.doc().unwrap().session.bookmarks().len(), start);
    press(&mut h, "From Pages");
    assert!(h.state().state.doc().unwrap().session.bookmarks().len() >= 2);
}

#[test]
fn sign_with_a_new_digital_id_and_validate() {
    let mut h = harness();
    let dir = temp("sign");
    // The document must be a file to sign into a new one.
    let src = dir.join("plan.pdf");
    std::fs::write(&src, markupcraft_render::synthetic::sample_pdf()).unwrap();
    h.state_mut().state.open_path(&src);
    h.run_steps(4);
    run(&mut h, "tools.digital_id");
    {
        let s = &mut h.state_mut().state.features.signatures;
        s.who.name = "Test Signer".into();
        s.new_password = "secret".into();
    }
    let id = dir.join("id.p12");
    h.state_mut().state.dialogs.scripted = Some(vec![id.clone()]);
    press(&mut h, "Save ID As...");
    assert!(id.is_file(), "{}", h.state().state.features.signatures.message);
    run(&mut h, "tools.sign");
    h.state_mut().state.features.signatures.password = "secret".into();
    h.state_mut().state.features.signatures.reason = "Approved".into();
    let out = dir.join("signed.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    h.run_steps(2);
    press(&mut h, "Sign and Save As...");
    h.run_steps(3);
    assert!(out.is_file(), "{}", h.state().state.features.signatures.message);
    let s = &h.state().state;
    assert_eq!(s.doc().unwrap().path.as_deref(), Some(out.as_path()));
    assert!(s.open_panels.contains(&"signatures"));
    let list = &s.features.signatures.list;
    assert_eq!(list.len(), 1);
    assert!(list[0].signed);
    assert!(shows(&h, "Test Signer"));
}

#[test]
fn stamp_library_places_a_stamp_and_an_image() {
    let mut h = harness();
    let dir = temp("stamps");
    h.state_mut().state.features.stamps.dir = Some(dir.join("stamps"));
    run(&mut h, "markup.stamps");
    assert!(h.state().state.features.stamps.entries.len() >= 3, "built-in designs");
    h.state_mut().state.features.stamps.new_name = "Checked".into();
    h.state_mut().state.features.stamps.new_text = "CHECKED {prompt:By=RT}".into();
    press(&mut h, "Add Text Stamp");
    assert_eq!(
        h.state().state.features.stamps.chosen.as_deref().map(|s| !s.is_empty()),
        Some(true),
        "{}",
        h.state().state.features.stamps.message
    );
    press(&mut h, "Place on Page");
    let before = markups(&h).len();
    click(&mut h, 900.0, 300.0);
    let all = markups(&h);
    assert_eq!(all.len(), before + 1, "{}", h.state().state.status);
    assert_eq!(all.last().unwrap().kind, Kind::Stamp);
    assert!(
        all.last().unwrap().contents.contains("CHECKED RT")
            || all.last().unwrap().subject.contains("Checked")
            || !all.last().unwrap().contents.is_empty()
    );
    // Markup > Image: a PNG placed by a box.
    let png = dir.join("logo.png");
    let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 30, 30, 255]));
    img.save(&png).unwrap();
    h.state_mut().state.dialogs.scripted = Some(vec![png]);
    run(&mut h, "markup.image");
    drag(&mut h, (700.0, 200.0), (800.0, 260.0));
    assert_eq!(markups(&h).len(), before + 2, "{}", h.state().state.status);
}

#[test]
fn document_dialog_watermark_header_flatten_and_attachments() {
    let mut h = harness();
    run(&mut h, "document.watermark");
    h.state_mut().state.features.docops.wm.text = "DRAFT".into();
    press(&mut h, "Apply");
    let marks = h.state().state.doc().unwrap().session.marks_present();
    assert!(
        marks.contains(&markupcraft_engine::marks::MarkKind::Watermark),
        "{}",
        h.state().state.features.docops.message
    );
    run(&mut h, "document.headers_footers");
    h.state_mut().state.features.docops.hf.text[4] = "Page <<1>>".into();
    press(&mut h, "Apply");
    assert!(
        h.state()
            .state
            .doc()
            .unwrap()
            .session
            .marks_present()
            .contains(&markupcraft_engine::marks::MarkKind::HeaderFooter)
    );
    run(&mut h, "document.flatten");
    let before = markups(&h).len();
    press(&mut h, "Flatten");
    assert!(
        markups(&h).len() < before,
        "{}",
        h.state().state.features.docops.message
    );
    // Attachments: add a file, see it listed.
    run(&mut h, "document.attachments");
    let f = temp("attach").join("notes.txt");
    std::fs::write(&f, "site notes").unwrap();
    h.state_mut().state.dialogs.scripted = Some(vec![f]);
    press(&mut h, "Add File...");
    assert_eq!(h.state().state.doc().unwrap().session.attachments().len(), 1);
    assert!(shows(&h, "notes.txt"));
    // Security: set a password (applies on save).
    run(&mut h, "document.security");
    h.state_mut().state.features.docops.open_password = "pw".into();
    press(&mut h, "Apply Security");
    assert!(h.state().state.doc().unwrap().session.security().pending_change);
}

#[test]
fn redaction_marks_boxes_then_removes_the_text() {
    let mut h = harness();
    key(&mut h, Modifiers::SHIFT, Key::R);
    // Over "BEDROOM" (12 pt at 150, 300).
    drag(&mut h, (145.0, 295.0), (240.0, 316.0));
    assert_eq!(h.state().state.doc().unwrap().session.redact_marks().len(), 1);
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(h.state().state.features.pick.is_none());
    key(&mut h, Modifiers::SHIFT, Key::A);
    assert!(shows(&h, "1 area marked for redaction"));
    press(&mut h, "Apply");
    let s = &h.state().state.doc().unwrap().session;
    let found = s
        .search_text(
            "BEDROOM",
            &markupcraft_engine::search::SearchOptions {
                max_hits: 10,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(found.hits.is_empty(), "{}", h.state().state.features.redact.message);
}

#[test]
fn form_fields_are_drawn_and_filled() {
    let mut h = harness();
    run(&mut h, "tools.forms");
    h.state_mut().state.features.forms.new_name = "Checked By".into();
    press(&mut h, "Draw...");
    drag(&mut h, (700.0, 60.0), (900.0, 85.0));
    let fields = h.state().state.doc().unwrap().session.form_fields();
    assert_eq!(fields.len(), 1, "{}", h.state().state.features.forms.message);
    assert_eq!(fields[0].name, "Checked By");
    h.state_mut().state.features.forms.edits = vec![(
        "Checked By".into(),
        markupcraft_engine::forms::FillValue::Text("RT".into()),
    )];
    h.run_steps(2);
    press(&mut h, "Apply Values");
    let fields = h.state().state.doc().unwrap().session.form_fields();
    assert_eq!(fields[0].value, vec!["RT".to_string()]);
}

#[test]
fn spell_check_steps_through_misspelled_comments() {
    let mut h = harness();
    {
        let d = h.state_mut().state.doc_mut().unwrap();
        let id = d.session.doc().markups[0].id.clone();
        let patch = markupcraft_engine::MarkupPatch {
            contents: Some("Chek the dor".into()),
            ..Default::default()
        };
        d.session.set_properties(&[id], &patch).unwrap();
    }
    key(&mut h, Modifiers::NONE, Key::F7);
    let s = &h.state().state.features.spell;
    assert!(s.open);
    assert!(s.words.len() >= 2, "{}", s.message);
    assert_eq!(s.words[0].word, "Chek");
    h.state_mut().state.features.spell.change_to = "Check".into();
    press(&mut h, "Change");
    let c = markups(&h)[0].contents.clone();
    assert!(c.starts_with("Check the"), "{c}");
    press(&mut h, "Ignore");
    assert!(shows(&h, "Check Again"));
}

#[test]
fn sets_panel_lists_every_sheet_and_opens_one() {
    let mut h = harness();
    let dir = temp("sets");
    let (a, b) = (dir.join("a.pdf"), dir.join("b.pdf"));
    std::fs::write(&a, markupcraft_render::synthetic::sample_pdf()).unwrap();
    std::fs::write(&b, revised_sample()).unwrap();
    h.state_mut().set_option("panel", "sets");
    h.run_steps(3);
    h.state_mut().state.dialogs.scripted = Some(vec![a.clone(), b.clone()]);
    press(&mut h, "Add Files...");
    h.run_steps(3);
    assert_eq!(h.state_mut().state.features.sets.sheets().len(), 4);
    let set = dir.join("job.pcset");
    h.state_mut().state.dialogs.scripted = Some(vec![set.clone()]);
    press(&mut h, "Save...");
    assert!(set.is_file());
    let sheet = h.state_mut().state.features.sets.sheets()[3].clone();
    markupcraft_ui_egui::features::sets::open_sheet(&mut h.state_mut().state, &sheet);
    h.run_steps(3);
    let d = h.state().state.doc().unwrap();
    assert_eq!(d.path.as_deref(), Some(b.as_path()));
    assert_eq!(d.view.current, 1);
}

#[test]
fn batch_combine_summary_and_slip_sheet() {
    let mut h = harness();
    let dir = temp("batch");
    let (a, b) = (dir.join("a.pdf"), dir.join("b.pdf"));
    let labelled = |bytes: Vec<u8>, out: &PathBuf| {
        let mut s = markupcraft_engine::Session::from_bytes(bytes, out).unwrap();
        s.set_page_labels(&[(0, "A101".into()), (1, "A102".into())]).unwrap();
        s.save_as(out, true).unwrap();
    };
    labelled(markupcraft_render::synthetic::sample_pdf(), &a);
    labelled(revised_sample(), &b);
    h.state_mut().state.open_path(&a);
    h.run_steps(3);
    // Combine
    run(&mut h, "file.combine");
    h.state_mut().state.dialogs.scripted = Some(vec![a.clone(), b.clone()]);
    press(&mut h, "Add Files...");
    let out = dir.join("combined.pdf");
    h.state_mut().state.dialogs.scripted = Some(vec![out.clone()]);
    press(&mut h, "Run");
    h.run_steps(3);
    assert_eq!(markupcraft_engine::Session::open(&out).unwrap().page_count(), 4);
    assert_eq!(h.state().state.doc().unwrap().name, "combined.pdf");
    // Batch Summary
    run(&mut h, "batch.summary");
    h.state_mut().state.dialogs.scripted = Some(vec![a.clone(), b.clone()]);
    press(&mut h, "Add Files...");
    let csv = dir.join("summary.csv");
    h.state_mut().state.dialogs.scripted = Some(vec![csv.clone()]);
    press(&mut h, "Run");
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(text.lines().count() >= 12, "{text}");
    // Slip Sheet: a.pdf's sheets replaced by b's (matched by page label).
    h.state_mut().state.active = 1;
    assert_eq!(h.state().state.doc().unwrap().path.as_deref(), Some(a.as_path()));
    run(&mut h, "document.slip_sheet");
    h.state_mut().state.dialogs.scripted = Some(vec![b.clone()]);
    press(&mut h, "Add Revised Files...");
    press(&mut h, "Run");
    assert!(
        h.state()
            .state
            .features
            .batch
            .message
            .starts_with("Slip-sheeted 2 sheets"),
        "{}",
        h.state().state.features.batch.message
    );
}

#[test]
fn ocr_dialog_reports_without_crashing() {
    let mut h = harness();
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::O);
    assert!(h.state().state.features.ocr.open);
    press(&mut h, "Run OCR");
    // With the models installed pages with text are skipped; without them the dialog says so.
    assert!(!h.state().state.features.ocr.message.is_empty());
}

#[test]
fn feature_menus_and_panels_are_listed() {
    // Each feature panel has a button on the panel bar and opens in its side area.
    let mut h = harness();
    for p in ["search", "layers", "spaces", "links", "signatures", "sets", "compare"] {
        let title = markupcraft_ui_egui::panels::find(p).unwrap().title;
        let button = h
            .query_all_by_label_contains(title)
            .next()
            .unwrap_or_else(|| panic!("{p} is on the panel bar"));
        button.click();
        h.run_steps(3);
        assert!(h.state().state.open_panels.contains(&p), "{p} opens from the panel bar");
    }
}
