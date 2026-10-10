//! The features finished from partial rows, driven through the real shell headlessly
//! (egui_kittest): the page dialogs' further options, page and email templates, GIF and
//! multi-page TIFF, captions along the last segment with strike-through and superscript, and
//! the rest of wave 6B.

use std::path::{Path, PathBuf};

use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_geom::Point;
use markupcraft_model::{Kind, Markup};
use markupcraft_ui_egui::MarkupCraftApp;

fn harness() -> Harness<'static, MarkupCraftApp> {
    let mut h = Harness::builder()
        .with_size(vec2(1500.0, 1000.0))
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
    let d = std::env::temp_dir().join(format!("markupcraft-partials-ui-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
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

fn script(h: &mut Harness<'_, MarkupCraftApp>, paths: Vec<PathBuf>) {
    h.state_mut().state.dialogs.scripted = Some(paths);
}

fn status(h: &Harness<'_, MarkupCraftApp>) -> String {
    h.state().state.status.clone()
}

fn page_count(h: &Harness<'_, MarkupCraftApp>) -> usize {
    h.state().state.doc().unwrap().session.page_count()
}

fn pdf_of(dir: &Path, name: &str, n: usize) -> PathBuf {
    use markupcraft_engine::synthetic::{SyntheticPage, pdf, text};
    let pages: Vec<SyntheticPage> = (0..n)
        .map(|i| SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, &format!("{name} {}", i + 1))))
        .collect();
    let p = dir.join(name);
    std::fs::write(&p, pdf(&pages)).unwrap();
    p
}

#[test]
fn page_dialogs_insert_several_files_grid_extract_each_and_place_content() {
    let mut h = harness();
    let dir = temp("pages");
    let a = pdf_of(&dir, "a.pdf", 3);
    let b = pdf_of(&dir, "b.pdf", 2);
    let before = page_count(&h);

    run(&mut h, "pages.insert");
    assert!(shows(&h, "Files (pages of each"));
    script(&mut h, vec![a.clone(), b.clone()]);
    press(&mut h, "Add Files...");
    h.state_mut().state.dialogs.scripted = None;
    {
        let d = h.state_mut().state.shell.page_dialog.as_mut().unwrap();
        assert_eq!(d.more.files.len(), 2);
        d.more.files[0].1 = "2-3".into();
    }
    press(&mut h, "OK");
    assert_eq!(page_count(&h), before + 4, "{}", status(&h));
    assert!(status(&h).contains("from 2 files"), "{}", status(&h));

    run(&mut h, "document.insert_blank_pages");
    press(&mut h, "Grid");
    press(&mut h, "OK");
    assert_eq!(page_count(&h), before + 5, "{}", status(&h));

    run(&mut h, "document.insert_blank_pages");
    script(&mut h, vec![b.clone()]);
    press(&mut h, "Choose Template...");
    h.state_mut().state.dialogs.scripted = None;
    assert!(shows(&h, "Template page: b.pdf"));
    press(&mut h, "OK");
    assert_eq!(page_count(&h), before + 6);

    let out = dir.join("each");
    std::fs::create_dir_all(&out).unwrap();
    run(&mut h, "document.extract_pages");
    {
        let d = h.state_mut().state.shell.page_dialog.as_mut().unwrap();
        d.pages.range = markupcraft_ui_egui::shell::pages::Range::Custom;
        d.pages.custom = "1-2".into();
    }
    press(&mut h, "One file per page (into a folder)");
    script(&mut h, vec![out.clone()]);
    press(&mut h, "OK");
    h.state_mut().state.dialogs.scripted = None;
    h.run_steps(3);
    let n = std::fs::read_dir(&out).unwrap().count();
    let names: Vec<_> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(n, 2, "{} {names:?}", status(&h));

    run(&mut h, "document.page_setup");
    press(&mut h, "Place the content on the new size");
    {
        let d = h.state_mut().state.shell.page_dialog.as_mut().unwrap();
        d.size = Some(2);
        d.landscape = true;
        d.more.border = 2.0;
        d.more.margins = [1.0, 0.25, 0.25, 0.25];
    }
    h.run_steps(3);
    press(&mut h, "OK");
    assert!(status(&h).starts_with("Set up"), "{}", status(&h));
    let w = h.state().state.doc().unwrap().session.doc().pages[0].media.width();
    assert!((w - 1224.0).abs() < 0.1, "{w}");
}

#[test]
fn page_templates_and_email_templates() {
    let mut h = harness();
    let dir = temp("templates");
    h.state_mut().state.features.partials.config = Some(dir.clone());
    run(&mut h, "file.new_from_template");
    assert!(shows(&h, "New PDF from Template"));
    h.state_mut().state.features.partials.template_name = "Title Block".into();
    press(&mut h, "Save Current Document as Template");
    assert!(
        dir.join("templates/Title Block.pdf").is_file(),
        "{}",
        h.state().state.features.partials.message
    );
    h.state_mut().state.features.partials.template_pick = Some("Title Block".into());
    h.run_steps(2);
    let docs = h.state().state.docs.len();
    press(&mut h, "New PDF");
    assert_eq!(
        h.state().state.docs.len(),
        docs + 1,
        "{} {}",
        status(&h),
        h.state().state.features.partials.message
    );
    assert!(h.state().state.doc().unwrap().name.starts_with("Untitled"));

    run(&mut h, "file.email_templates");
    {
        let e = &mut h.state_mut().state.features.partials.email;
        e.name = "Transmittal".into();
        e.to = "office@example.com".into();
        e.subject = "Sheets: {file}".into();
    }
    press(&mut h, "Save Template");
    assert!(dir.join("email_templates.json").is_file());
    press(&mut h, "Email Document");
    let draft = h.state().state.shell.extra.files.last_email.clone().expect("a draft");
    let text = String::from_utf8_lossy(&std::fs::read(&draft).unwrap()).into_owned();
    assert!(text.contains("To: office@example.com"), "{text}");
    assert!(text.contains("Sheets: Untitled"), "{}", &text[..200.min(text.len())]);
    let _ = std::fs::remove_file(draft);
}

#[test]
fn gif_and_multi_page_tiff_open_and_export() {
    let dir = temp("gif");
    let mut tif = std::io::Cursor::new(Vec::new());
    {
        let mut enc = tiff::encoder::TiffEncoder::new(&mut tif).unwrap();
        for c in [[200u8, 0, 0], [0, 0, 200]] {
            let px: Vec<u8> = (0..40 * 30).flat_map(|_| c).collect();
            enc.write_image::<tiff::encoder::colortype::RGB8>(40, 30, &px).unwrap();
        }
    }
    let t = dir.join("scans.tif");
    std::fs::write(&t, tif.into_inner()).unwrap();
    let mut h = harness();
    h.state_mut().open_path(&t);
    h.run_steps(4);
    assert_eq!(page_count(&h), 2, "{}", status(&h));
    run(&mut h, "file.export_images");
    h.state_mut().state.features.export.image_format = 4;
    h.state_mut().state.features.export.dpi = 18.0;
    script(&mut h, vec![dir.clone()]);
    press(&mut h, "Export...");
    let gifs: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "gif"))
        .collect();
    assert_eq!(
        gifs.len(),
        2,
        "{} {}",
        status(&h),
        h.state().state.features.export.message
    );
    h.state_mut().state.dialogs.scripted = None;
    h.state_mut().open_path(&gifs[0]);
    h.run_steps(4);
    assert!(h.state().state.doc().unwrap().name.ends_with(".pdf"));
    assert!(markupcraft_ui_egui::dialogs::PDF_OR_IMAGE.1.contains(&"gif"));
}

#[test]
fn caption_placement_and_strike_superscript_in_properties() {
    let mut h = harness();
    let m = Markup::new(
        Kind::Perimeter,
        0,
        vec![
            Point::new(100.0, 100.0),
            Point::new(300.0, 100.0),
            Point::new(300.0, 300.0),
            Point::new(100.0, 300.0),
        ],
    );
    let id = h.state_mut().state.doc_mut().unwrap().session.add_markup(m).unwrap();
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .select(std::slice::from_ref(&id))
        .unwrap();
    h.state_mut().state.show_panel("properties");
    h.run_steps(6);
    let node = h.get_by_label("Along the last segment");
    node.scroll_to_me();
    h.run_steps(20);
    press(&mut h, "Along the last segment");
    let get = |h: &Harness<'_, MarkupCraftApp>| h.state().state.doc().unwrap().session.doc().find(&id).unwrap().clone();
    assert!(get(&h).caption_last_segment);
    let anchor = markupcraft_model::caption::default_caption_anchor(&get(&h));
    assert!(anchor.y > 300.0, "beside the last segment, outside: {anchor:?}");
    let n = h.get_by(|n| n.label().as_deref() == Some("S") || n.label().as_deref() == Some("Strikethrough"));
    n.click();
    h.run_steps(4);
    assert!(get(&h).text.strike);
    press(&mut h, "x\u{b2}");
    assert_eq!(get(&h).text.script, 1);
}

#[test]
fn batch_list_sources_save_load_split_script_and_one_pdf_per_file() {
    let mut h = harness();
    let dir = temp("batch");
    let a = pdf_of(&dir, "a.pdf", 4);
    let b = pdf_of(&dir, "b.pdf", 2);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let c = pdf_of(&dir.join("sub"), "c.pdf", 1);
    h.state_mut().open_path(&a);
    h.run_steps(4);

    run(&mut h, "batch.split");
    assert!(shows(&h, "Batch Split"));
    press(&mut h, "Add Open Files");
    assert_eq!(h.state().state.features.batch.files, vec![a.clone()]);
    script(&mut h, vec![dir.clone()]);
    press(&mut h, "Add Folder...");
    h.state_mut().state.dialogs.scripted = None;
    let files = h.state().state.features.batch.files.clone();
    assert!(files.contains(&b) && files.contains(&c), "{files:?}");
    let list = dir.join("list.json");
    script(&mut h, vec![list.clone()]);
    press(&mut h, "Save List...");
    h.state_mut().state.dialogs.scripted = None;
    assert!(list.is_file());
    press(&mut h, "Clear");
    assert!(h.state().state.features.batch.files.is_empty());
    script(&mut h, vec![list.clone()]);
    press(&mut h, "Load List...");
    assert_eq!(h.state().state.features.batch.files.len(), 3);
    // drag the last row to the top
    markupcraft_ui_egui::features::batch_list::move_row(&mut h.state_mut().state.features.batch.files, 2, 0);
    h.state_mut().state.features.batch.more.split_pages = 2;
    let parts = dir.join("parts");
    std::fs::create_dir_all(&parts).unwrap();
    script(&mut h, vec![parts.clone()]);
    press(&mut h, "Run");
    h.state_mut().state.dialogs.scripted = None;
    h.run_steps(3);
    assert_eq!(
        std::fs::read_dir(&parts).unwrap().count(),
        4,
        "{}",
        h.state().state.features.batch.message
    );

    run(&mut h, "batch.script");
    assert!(shows(&h, "Batch Script"));
    h.state_mut().state.features.batch.files = vec![b.clone()];
    let steps = dir.join("steps.json");
    std::fs::write(&steps, r#"[{"tool": "watermark_add", "params": {"text": "VOID"}}]"#).unwrap();
    h.run_steps(2);
    script(&mut h, vec![steps.clone()]);
    press(&mut h, "Choose Script...");
    h.state_mut().state.dialogs.scripted = None;
    assert!(
        shows(&h, "Script: steps.json"),
        "{:?} {}",
        h.state().state.features.batch.more.script,
        h.state().state.features.batch.message
    );
    press(&mut h, "Run");
    assert!(
        h.state()
            .state
            .features
            .batch
            .message
            .contains("Ran the script on 1 file"),
        "{}",
        h.state().state.features.batch.message
    );

    let txt = dir.join("notes.txt");
    std::fs::write(&txt, "Field notes\nSecond line").unwrap();
    run(&mut h, "file.create_from_files");
    h.state_mut().state.features.batch.files = vec![txt.clone()];
    h.run_steps(2);
    press(&mut h, "One PDF per file");
    press(&mut h, "Run");
    assert!(
        dir.join("notes.pdf").is_file(),
        "{}",
        h.state().state.features.batch.message
    );
}

fn toolset_file(dir: &Path) -> (PathBuf, String) {
    use markupcraft_ui_egui::chest::ToolChest;
    let mut c = ToolChest::default();
    let id = c.add_set("Doors");
    let mut m = Markup::new(Kind::Count, 0, vec![Point::new(10.0, 10.0)]);
    m.subject = "Door".into();
    c.add_markup(&id, &m).unwrap();
    let p = dir.join("doors.mctools");
    std::fs::write(&p, c.export_set(&id).unwrap()).unwrap();
    (p, id)
}

#[test]
fn shared_tool_sets_check_out_and_in_through_a_shared_folder() {
    use markupcraft_ui_egui::chest_more::{ChestAct, apply};
    let dir = temp("shared");
    let (file, _) = toolset_file(&dir);
    let mut h = harness();
    h.state_mut().state.author = "Estimator A".into();
    script(&mut h, vec![file.clone()]);
    run(&mut h, "tools.add_shared_toolset");
    h.state_mut().state.dialogs.scripted = None;
    h.run_steps(3);
    let id = h
        .state()
        .state
        .toolchest
        .extras
        .shared
        .first()
        .expect("linked")
        .set
        .clone();
    assert!(h.state().state.toolchest.is_locked(&id), "read-only until checked out");
    assert!(shows(&h, "Doors"));
    apply(&mut h.state_mut().state, vec![ChestAct::CheckOut(id.clone())]);
    assert!(!h.state().state.toolchest.is_locked(&id), "{}", status(&h));
    let lock = std::fs::read_to_string(format!("{}.lock", file.display())).unwrap();
    assert_eq!(lock, "Estimator A");
    // someone else cannot check it out meanwhile
    let mut other = markupcraft_ui_egui::chest::ToolChest::default();
    let oid = other.add_shared(&file).unwrap();
    let e = other.check_out(&oid, "Estimator B").unwrap_err();
    assert!(e.contains("Checked out by Estimator A"), "{e}");
    // edit, then check in: the shared file takes the change and the lock goes
    let mut w = Markup::new(Kind::Count, 0, vec![Point::new(10.0, 10.0)]);
    w.subject = "Window".into();
    h.state_mut().state.toolchest.add_markup(&id, &w).unwrap();
    apply(&mut h.state_mut().state, vec![ChestAct::CheckIn(id.clone())]);
    assert!(h.state().state.toolchest.is_locked(&id));
    assert!(!std::path::Path::new(&format!("{}.lock", file.display())).exists());
    assert!(std::fs::read_to_string(&file).unwrap().contains("Window"));
    other.refresh_shared(&oid).unwrap();
    assert_eq!(other.find_set(&oid).unwrap().items.len(), 2);
}

#[test]
fn profile_columns_every_document_takeoff_profile_and_toolset_legend() {
    let dir = temp("profile");
    let mut h = harness();
    h.state_mut().state.toolchest.extras.columns = vec![markupcraft_model::columns::CustomColumn {
        id: "cost".into(),
        name: "Unit Cost".into(),
        ..Default::default()
    }];
    run(&mut h, "markup.profile_columns");
    assert!(shows(&h, "Profile Columns"));
    press(&mut h, "Add them to every document opened");
    h.run_steps(3);
    let cols = h.state().state.doc().unwrap().session.doc().columns.clone();
    assert!(cols.iter().any(|c| c.name == "Unit Cost"), "{cols:?}");
    let other = pdf_of(&dir, "other.pdf", 1);
    h.state_mut().open_path(&other);
    h.run_steps(4);
    assert!(
        h.state()
            .state
            .doc()
            .unwrap()
            .session
            .doc()
            .columns
            .iter()
            .any(|c| c.name == "Unit Cost")
    );
    press(&mut h, "Delete from Profile");
    assert!(h.state().state.toolchest.extras.columns.is_empty());

    // Takeoff Workspace is a saved profile
    h.state_mut().state.shell.store = Some(markupcraft_engine::prefs::PrefStore::new(dir.join("config")));
    run(&mut h, "window.takeoff_workspace");
    assert!(status(&h).starts_with("Takeoff profile"), "{}", status(&h));
    let store = h.state().state.shell.store.clone().unwrap();
    assert_eq!(store.active(), "Takeoff");
    assert!(store.profiles().iter().any(|p| p == "Takeoff"));

    // a legend of a Tool Chest set
    let (_, set) = {
        let mut c = std::mem::take(&mut h.state_mut().state.toolchest);
        let id = c.add_set("Doors");
        let mut m = Markup::new(Kind::Count, 0, vec![Point::new(10.0, 10.0)]);
        m.subject = "Door".into();
        c.add_markup(&id, &m).unwrap();
        h.state_mut().state.toolchest = c;
        ((), id)
    };
    let s = h.state().state.toolchest.find_set(&set).unwrap().clone();
    assert_eq!(
        markupcraft_ui_egui::features::partials_more::set_subjects(&s),
        vec!["Door".to_string()]
    );
    run(&mut h, "measure.legend");
    assert!(shows(&h, "From Tool Chest set"));
}

fn screen(h: &Harness<'_, MarkupCraftApp>, x: f64, y: f64) -> egui::Pos2 {
    let d = h.state().state.doc().unwrap();
    let page = d.view.current;
    d.view
        .user_to_screen(page, Point::new(x, y), d.render.as_ref().unwrap().pages())
        .unwrap()
}

fn button(h: &mut Harness<'_, MarkupCraftApp>, at: egui::Pos2, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
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

#[test]
fn count_readout_hides_and_comes_back_and_the_status_report() {
    let mut h = harness();
    h.state_mut().state.set_tool("count");
    h.run_steps(2);
    click(&mut h, 200.0, 500.0);
    click(&mut h, 240.0, 500.0);
    assert!(shows(&h, "Count: 2"), "the running count");
    press(&mut h, "Hide");
    assert!(shows(&h, "Show Count"), "hidden: a button brings it back");
    press(&mut h, "Show Count");
    assert!(!shows(&h, "Show Count") && shows(&h, "Hide"), "clicking brings it back");
    h.key_press(egui::Key::Escape);
    h.run_steps(4);

    let dir = temp("status");
    let mut m = Markup::new(Kind::Count, 0, vec![Point::new(10.0, 10.0), Point::new(20.0, 20.0)]);
    m.subject = "Door".into();
    m.status = "Installed".into();
    h.state_mut().state.doc_mut().unwrap().session.add_markup(m).unwrap();
    run(&mut h, "measure.status_report");
    assert!(shows(&h, "Count Status Report"));
    assert!(shows(&h, "Installed"));
    let out = dir.join("visual.pdf");
    script(&mut h, vec![out.clone()]);
    press(&mut h, "Export Visual Report...");
    h.state_mut().state.dialogs.scripted = None;
    assert!(
        out.is_file(),
        "{}",
        h.state().state.features.partials.more2.status_message
    );
    let csv = dir.join("t.csv");
    script(&mut h, vec![csv.clone()]);
    press(&mut h, "Export Table...");
    assert!(std::fs::read_to_string(&csv).unwrap().contains("Door,Installed,2,1"));
}

#[test]
fn viewport_calibrate_raster_fill_redaction_kinds_and_snapshot_cut() {
    let mut h = harness();
    let sc = markupcraft_measure::units::scale_presets()[0].scale.clone();
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .add_viewport(
            0,
            markupcraft_geom::Rect::new(100.0, 100.0, 500.0, 500.0),
            "Detail",
            &sc,
        )
        .unwrap();
    h.state_mut().state.show_panel("measurements");
    h.run_steps(6);
    // the rescale box of the viewport, then Calibrate
    let link = h.get_by_label(&sc.ratio);
    link.scroll_to_me();
    h.run_steps(20);
    let link = h.get_by_label(&sc.ratio);
    link.click();
    h.run_steps(4);
    assert!(shows(&h, "Calibrate: length"));
    h.state_mut().state.edit.viewports_panel.more.length = "20".into();
    h.run_steps(2);
    let n = h.get_by_label("Pick Two Points");
    n.scroll_to_me();
    h.run_steps(20);
    press(&mut h, "Pick Two Points");
    let st = format!(
        "{:?} {:?} {:?}",
        h.state().state.edit.viewports_panel.more.want_pick,
        h.state().state.edit.viewports_panel.more.pending,
        h.state().state.features.pick
    );
    h.run_steps(2);
    drag(&mut h, (150.0, 200.0), (294.0, 200.0));
    let vp = h.state().state.doc().unwrap().session.doc().pages[0].viewports[0].clone();
    assert!(
        (vp.scale.x_conv() - 20.0 / 144.0).abs() < 0.01,
        "{} {}",
        vp.scale.x_conv(),
        h.state().state.status.clone() + &st
    );

    // Dynamic Fill on the page image
    run(&mut h, "measure.dynamic_fill");
    assert!(shows(&h, "Detection and cursor"));
    h.state_mut().state.features.fill.more.raster = true;
    h.state_mut().state.features.fill.more.dpi = 50.0;
    let opts = {
        let f = &h.state().state.features.fill;
        f.more.raster(f.gap, f.cutouts)
    };
    assert_eq!(opts.map(|o| o.dpi), Some(50.0));
    h.key_press(egui::Key::Escape);
    h.run_steps(4);

    // redaction: text only
    run(&mut h, "document.apply_redactions");
    press(&mut h, "Text only");
    assert_eq!(
        h.state().state.features.redact.kinds,
        markupcraft_engine::finish::redact_kinds::RedactKinds::TextOnly
    );
    press(&mut h, "Cancel");

    // Cut Snapshot, then paste it back
    let before = h.state().state.doc().unwrap().session.page_text(0).unwrap();
    run(&mut h, "edit.snapshot_cut");
    drag(&mut h, (40.0, 40.0), (580.0, 760.0));
    let after = h.state().state.doc().unwrap().session.page_text(0).unwrap();
    assert!(after.len() < before.len(), "{}", h.state().state.status);
    assert_eq!(h.state().state.doc().unwrap().session.clipboard().len(), 1);
}

#[test]
fn security_icon_pdfa_badge_verify_page_tags_ids_numbering_and_compressed_saves() {
    let mut h = harness();
    // the status bar's security icon opens the Security dialog
    let n = h.get_by_label("Security: No security");
    n.click();
    h.run_steps(4);
    assert!(shows(&h, "Security"));

    // a PDF/A document shows a badge on its tab, and Properties verifies or unlocks it
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .archive_pdfa(markupcraft_engine::archive::PdfaLevel::A2b)
        .unwrap();
    h.run_steps(4);
    assert!(
        h.query_all_by_label_contains("[PDF/A-2B]").next().is_some(),
        "the tab badge"
    );
    run(&mut h, "document.properties");
    assert!(shows(&h, "Page tags"));
    press(&mut h, "Verify");
    assert!(h.state().state.features.partials.more3.verify.is_some());
    press(&mut h, "Unlock");
    assert!(
        h.state().state.doc().unwrap().session.standards().pdfa.is_none(),
        "{}",
        status(&h)
    );

    // Digital IDs
    let dir = temp("ids");
    h.state_mut().state.features.partials.config = Some(dir.clone());
    run(&mut h, "tools.digital_ids");
    {
        let st = &mut h.state_mut().state.features.partials.more3;
        st.id_name = "Field".into();
        st.id_person = "Sam Inspector".into();
        st.id_password = "secret".into();
    }
    h.run_steps(2);
    press(&mut h, "Create ID");
    assert!(
        dir.join("ids/Field.p12").is_file(),
        "{}",
        h.state().state.features.partials.more3.id_message
    );
    h.state_mut().state.features.partials.more3.id_selected = Some("Field".into());
    h.run_steps(2);
    press(&mut h, "Log In");
    assert_eq!(
        h.state().state.features.partials.more3.unlocked,
        vec!["Field".to_string()]
    );
    press(&mut h, "Log Out");
    assert!(h.state().state.features.partials.more3.unlocked.is_empty());

    // Number Pages from the thumbnails
    markupcraft_ui_egui::panels::thumbnails::command(&mut h.state_mut().state, "number", &[0]);
    h.run_steps(3);
    assert!(shows(&h, "Number Pages"));
    if let Some(n) = h.state_mut().state.features.partials.more3.number.as_mut() {
        n.1 = "S-".into();
        n.2 = 7;
    }
    h.run_steps(2);
    press(&mut h, "Number");
    assert_eq!(
        h.state().state.doc().unwrap().session.page_labels()[0],
        "S-7",
        "{}",
        status(&h)
    );

    // a recent file's preview, and saves published compressed
    let file = dir.join("saved.pdf");
    h.state_mut().state.shell.prefs.save_mode = "compressed".into();
    script(&mut h, vec![file.clone()]);
    run(&mut h, "file.save_as");
    h.run_steps(3);
    let bytes = std::fs::read(&file).unwrap();
    assert!(
        String::from_utf8_lossy(&bytes).contains("/ObjStm"),
        "compressed object streams: {}",
        status(&h)
    );
    let ctx = h.ctx.clone();
    let tex = markupcraft_ui_egui::features::partials_more3::preview(&mut h.state_mut().state, &ctx, &file);
    assert!(tex.is_some_and(|t| t.size()[0] <= 160));
}

#[test]
fn form_highlight_properties_actions_and_xfa_layout() {
    let mut h = harness();
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .form_add_field(
            0,
            markupcraft_geom::Rect::new(100.0, 600.0, 300.0, 620.0),
            &markupcraft_engine::forms::NewFieldKind::Text { multiline: false },
            Some("Owner"),
        )
        .unwrap();
    run(&mut h, "tools.forms");
    assert!(shows(&h, "Highlight fields"));
    press(&mut h, "Highlight fields");
    let mut marks = Vec::new();
    h.state()
        .state
        .features
        .forms
        .more
        .marks(h.state().state.doc().unwrap(), &mut marks);
    assert_eq!(marks.len(), 1, "the field is tinted");
    markupcraft_ui_egui::features::forms_more::apply(
        &mut h.state_mut().state,
        markupcraft_ui_egui::features::forms_more::Act::Select("Owner".into()),
    );
    h.run_steps(3);
    {
        let m = &mut h.state_mut().state.features.forms.more;
        m.name = "Client".into();
        m.required = true;
    }
    h.run_steps(2);
    press(&mut h, "Apply Properties");
    let f = h.state().state.doc().unwrap().session.form_fields();
    assert!(
        f.iter().any(|f| f.name == "Client" && f.required),
        "{}",
        h.state().state.features.forms.message
    );
    {
        let m = &mut h.state_mut().state.features.forms.more;
        m.action = 1;
        m.action_value = "https://example.com".into();
    }
    h.run_steps(2);
    press(&mut h, "Set Action");
    assert_eq!(
        h.state()
            .state
            .doc()
            .unwrap()
            .session
            .form_actions("Client")
            .unwrap()
            .len(),
        1
    );

    // a dynamic XFA form: Lay Out XFA Form makes pages and fields
    let shell = pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::template(2));
    h.state_mut().open_bytes("xfa.pdf", None, shell).unwrap();
    h.run_steps(4);
    run(&mut h, "tools.forms");
    press(&mut h, "Lay Out XFA Form");
    assert!(
        !h.state().state.doc().unwrap().session.form_fields().is_empty(),
        "{}",
        h.state().state.features.forms.message
    );
}

fn mclick(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64, b: egui::PointerButton, m: egui::Modifiers) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: b,
            pressed,
            modifiers: m,
        });
        h.step();
    }
    h.run_steps(3);
}

#[test]
fn alignment_guides_ctrl_click_links_and_the_text_menu() {
    let mut h = harness();
    // Snap to Markup lines a new point up with another markup's vertex
    let r = Markup::new(
        Kind::Rectangle,
        0,
        vec![Point::new(100.0, 100.0), Point::new(200.0, 200.0)],
    );
    h.state_mut().state.doc_mut().unwrap().session.add_markup(r).unwrap();
    if !h.state().state.snaps.markup {
        run(&mut h, "snap.markup");
    }
    h.state_mut().state.set_tool("line");
    h.run_steps(2);
    click(&mut h, 101.5, 400.0);
    click(&mut h, 300.0, 400.0);
    let line = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .markups
        .iter()
        .find(|m| m.kind == Kind::Line)
        .cloned()
        .expect("a line");
    assert!(
        (line.pts[0].x - 100.0).abs() < 1e-6,
        "aligned with the corner: {:?}",
        line.pts
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.state_mut().state.set_tool("select");
    if h.state().state.snaps.markup {
        run(&mut h, "snap.markup");
    }

    // Ctrl+click on a link to another file opens it behind the current tab
    let dir = temp("links");
    let other = pdf_of(&dir, "other.pdf", 1);
    let target = markupcraft_engine::links::LinkTarget::File {
        path: other.display().to_string(),
        page: None,
    };
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .add_link(
            0,
            markupcraft_geom::Rect::new(400.0, 600.0, 500.0, 650.0),
            &target,
            Default::default(),
        )
        .unwrap();
    h.run_steps(2);
    let before = h.state().state.docs.len();
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    mclick(
        &mut h,
        450.0,
        625.0,
        egui::PointerButton::Primary,
        egui::Modifiers::COMMAND,
    );
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    assert_eq!(h.state().state.docs.len(), before + 1, "{}", status(&h));
    assert_eq!(h.state().state.active, 0, "the new tab stays behind");

    // right-click on page text
    let w = h.state().state.doc().unwrap().session.page_words(0).unwrap();
    let word = w
        .iter()
        .find(|w| w.text.chars().count() >= 3)
        .expect("page text")
        .clone();
    let c = word.rect.normalized();
    mclick(
        &mut h,
        (c.x0 + c.x1) / 2.0,
        (c.y0 + c.y1) / 2.0,
        egui::PointerButton::Secondary,
        egui::Modifiers::NONE,
    );
    assert!(shows(&h, "Copy Text"), "the text menu");
    press(&mut h, "Highlight Text");
    let n = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .markups
        .iter()
        .filter(|m| m.kind == Kind::TextHighlight)
        .count();
    assert_eq!(n, 1, "{}", status(&h));
}

#[test]
fn print_advanced_reset_and_flatten_extras() {
    let mut h = harness();
    let dir = temp("print");
    run(&mut h, "file.print");
    press(&mut h, "Advanced...");
    assert!(shows(&h, "Print in grayscale"));
    press(&mut h, "Print in grayscale");
    assert!(h.state().state.features.print.advanced.grayscale);
    let out = dir.join("sheets.pdf");
    script(&mut h, vec![out.clone()]);
    press(&mut h, "Save Print PDF...");
    h.state_mut().state.dialogs.scripted = None;
    assert!(out.is_file(), "{}", h.state().state.features.print.message);
    run(&mut h, "file.print");
    h.state_mut().state.features.print.copies = 3;
    h.run_steps(2);
    press(&mut h, "Reset to Defaults");
    assert_eq!(h.state().state.features.print.copies, 1);
    assert!(!h.state().state.features.print.advanced.grayscale);
    press(&mut h, "Cancel");

    let m = Markup::new(
        Kind::Rectangle,
        0,
        vec![Point::new(100.0, 100.0), Point::new(200.0, 200.0)],
    );
    h.state_mut().state.doc_mut().unwrap().session.add_markup(m).unwrap();
    run(&mut h, "document.flatten");
    assert!(shows(&h, "Overlay text:"));
    {
        let fm = &mut h.state_mut().state.features.docops.flatten_more;
        fm.extras.overlay = "FLATTENED".into();
        fm.extras.keep = vec!["subject".into()];
    }
    h.run_steps(2);
    let n = h.get_all_by_label("Flatten").last().unwrap();
    n.click();
    h.run_steps(4);
    assert!(
        h.state()
            .state
            .doc()
            .unwrap()
            .session
            .page_text(0)
            .unwrap()
            .contains("FLATTENED"),
        "{}",
        status(&h)
    );
    assert!(
        h.state()
            .state
            .doc()
            .unwrap()
            .session
            .doc()
            .markups
            .iter()
            .any(|m| m.kind == Kind::Note)
    );
}

#[test]
fn sketch_ellipse_by_radius_and_properties_toolbar_style_and_scale() {
    let mut h = harness();
    h.state_mut().state.shell.ui.extra.sketch_radius = true;
    h.state_mut().state.set_tool("ellipse");
    h.run_steps(2);
    click(&mut h, 300.0, 400.0);
    assert!(shows(&h, "Radius"), "the Sketch to Scale bar asks for a radius");
    h.state_mut().state.edit.sketch.width = "1".into();
    h.run_steps(2);
    press(&mut h, "Place");
    let e = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .doc()
        .markups
        .iter()
        .rev()
        .find(|m| m.kind == Kind::Ellipse)
        .cloned()
        .expect("an ellipse");
    let r = e.rect.normalized();
    assert!(
        (r.width() - r.height()).abs() < 0.5
            && ((r.x0 + r.x1) / 2.0 - 300.0).abs() < 1.0
            && ((r.y0 + r.y1) / 2.0 - 400.0).abs() < 1.0,
        "a circle about the click: {r:?}"
    );
    h.key_press(egui::Key::Escape);
    h.state_mut().state.set_tool("select");
    h.run_steps(2);

    // the Properties toolbar sets a line style and a measurement's scale
    h.state_mut().state.shell.ui.extra.properties_toolbar = true;
    let m = Markup::new(
        Kind::Length,
        0,
        vec![Point::new(100.0, 100.0), Point::new(300.0, 100.0)],
    );
    let mut m = m;
    m.dash = Vec::new();
    let id = h.state_mut().state.doc_mut().unwrap().session.add_markup(m).unwrap();
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .select(std::slice::from_ref(&id))
        .unwrap();
    h.run_steps(4);
    pick(&mut h, "Solid", "Dashed");
    let get = |h: &Harness<'_, MarkupCraftApp>| h.state().state.doc().unwrap().session.doc().find(&id).unwrap().clone();
    assert_eq!(get(&h).dash, vec![6.0, 3.0]);
    let now = get(&h).scale.map(|s| s.ratio).unwrap_or_else(|| "Page scale".into());
    let first = markupcraft_measure::units::scale_presets()
        .into_iter()
        .find(|p| p.scale.ratio != now)
        .unwrap();
    pick(&mut h, &now, &first.name);
    assert_eq!(get(&h).scale.map(|s| s.ratio), Some(first.scale.ratio));
}

fn pick(h: &mut Harness<'_, MarkupCraftApp>, value: &str, item: &str) {
    use egui::accesskit::Role;
    h.get_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(value))
        .next()
        .unwrap_or_else(|| panic!("no combo box showing {value:?}"))
        .click();
    h.run_steps(3);
    h.get_all_by_label(item)
        .last()
        .unwrap_or_else(|| panic!("no {item:?}"))
        .click();
    h.run_steps(3);
}

#[test]
fn summary_pdf_extras_in_the_dialog() {
    let mut h = harness();
    let dir = temp("summary");
    run(&mut h, "markup.summary");
    press(&mut h, "Output");
    for l in ["Spaces cover sheet", "Status history", "Thumbnails", "Page content"] {
        press(&mut h, l);
    }
    let x = h.state().state.features.summary.extras;
    assert!(x.spaces_cover && x.status_history && x.thumbnails.is_some() && x.page_content);
    let out = dir.join("summary.pdf");
    markupcraft_ui_egui::features::summary::write(
        &mut h.state_mut().state,
        markupcraft_engine::summary::SummaryFormat::Pdf,
        &out,
    );
    let n = markupcraft_engine::Session::open(&out).unwrap().page_count();
    let pages = h.state().state.doc().unwrap().session.page_count();
    assert!(n > pages + 2, "report, extras and the pages: {n}");
}

#[test]
fn detached_window_splits_and_syncs() {
    let mut h = harness();
    run(&mut h, "window.detach");
    h.run_steps(4);
    assert!(shows(&h, "Reattach"), "the window draws");
    press(&mut h, "Split");
    assert!(
        h.state().state.shell.extra.detached[0].second.is_some(),
        "two views in the window"
    );
    h.state_mut().state.shell.extra.detached[0].sync = markupcraft_ui_egui::shell::split::Sync::Document;
    let n = h.state().state.doc().unwrap().session.page_count();
    h.state_mut().state.shell.extra.detached[0]
        .pane
        .view
        .go_to_page(1.min(n - 1), n);
    h.run_steps(4);
    let w = &h.state().state.shell.extra.detached[0];
    assert_eq!(
        w.second.as_ref().unwrap().view.current,
        w.pane.view.current,
        "the second view follows the first"
    );
    // following the main window
    h.state_mut().state.shell.extra.detached[0].follow_main = true;
    h.state_mut().state.doc_mut().unwrap().view.go_to_page(0, n);
    h.run_steps(4);
    assert_eq!(h.state().state.shell.extra.detached[0].pane.view.current, 0);
    // Reattach, then drag the tab out of the window: it detaches again
    press(&mut h, "Reattach");
    assert!(h.state().state.shell.extra.detached.is_empty());
    let tab = h
        .get_all_by_label("sample.pdf")
        .map(|n| n.rect().center())
        .min_by(|a, b| a.y.total_cmp(&b.y))
        .unwrap();
    let out = egui::pos2(1499.5, 999.5);
    h.hover_at(tab);
    h.step();
    h.event(egui::Event::PointerButton {
        pos: tab,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    for i in 1..=10 {
        h.hover_at(tab + (out - tab) * (i as f32 / 10.0));
        h.step();
    }
    h.event(egui::Event::PointerGone);
    h.event(egui::Event::PointerButton {
        pos: out,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(4);
    assert_eq!(
        h.state().state.shell.extra.detached.len(),
        1,
        "dragged out: {}",
        status(&h)
    );
}

#[test]
fn toolbars_dock_on_the_sides_and_new_toolbars() {
    use markupcraft_ui_egui::shell::toolbars_more::{Dock, at};
    let mut h = harness();
    // The icon toolbars start hidden; Window > Toolbars shows them.
    assert!(!shows(&h, "Main toolbar"));
    for id in ["window.toolbar_main", "window.toolbar_markup", "window.toolbar_measure"] {
        run(&mut h, id);
    }
    assert!(
        shows(&h, "Main toolbar") && shows(&h, "Measure toolbar"),
        "each toolbar has a grip"
    );
    // the grip's menu docks a toolbar on the left
    h.get_by_label("Measure toolbar").click_secondary();
    h.run_steps(3);
    press(&mut h, "Dock on the Left");
    assert_eq!(at(&h.state().state, Dock::Left), vec!["Measure".to_string()]);
    // dragging a grip to the right edge docks it there
    let g = h.get_by_label("Markup toolbar").rect().center();
    let right = egui::pos2(1495.0, 500.0);
    h.hover_at(g);
    h.step();
    h.event(egui::Event::PointerButton {
        pos: g,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    for i in 1..=10 {
        h.hover_at(g + (right - g) * (i as f32 / 10.0));
        h.step();
    }
    h.event(egui::Event::PointerButton {
        pos: right,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(4);
    assert_eq!(at(&h.state().state, Dock::Right), vec!["Markup".to_string()]);

    // a new toolbar with a command, in the second row
    run(&mut h, "window.customize_toolbars");
    let id = egui::Id::new("tb-new-name");
    h.ctx.data_mut(|d| d.insert_temp(id, "Review".to_string()));
    h.run_steps(2);
    press(&mut h, "New Toolbar");
    let tb = h.state().state.shell.ui.toolbars.more.custom.clone();
    assert_eq!(tb.len(), 1);
    h.ctx
        .data_mut(|d| d.insert_temp(egui::Id::new("tb-pick"), "file.print".to_string()));
    h.run_steps(2);
    press(&mut h, "Add");
    assert_eq!(
        h.state().state.shell.ui.toolbars.more.custom[0].items,
        vec!["file.print".to_string()]
    );
    markupcraft_ui_egui::shell::toolbars_more::set_dock(&mut h.state_mut().state, "Review", Dock::Row2);
    h.run_steps(3);
    assert!(shows(&h, "Review toolbar"));
}

#[test]
fn edge_click_collapses_panels_bookmarks_follow_pages_and_urls_in_text() {
    let mut h = harness();
    h.state_mut().state.show_panel("bookmarks");
    h.run_steps(4);
    let open_left = |h: &Harness<'_, MarkupCraftApp>| {
        markupcraft_ui_egui::panels::PANELS
            .iter()
            .filter(|p| p.slot == markupcraft_ui_egui::panels::Slot::Left)
            .filter(|p| h.state().state.open_panels.contains(&p.id))
            .count()
    };
    let before = open_left(&h);
    assert!(before > 0);
    press(&mut h, "Collapse the left panels");
    h.run_steps(3);
    assert_eq!(open_left(&h), 0, "{}", status(&h));
    press(&mut h, "Open the left panels");
    h.run_steps(3);
    assert_eq!(open_left(&h), before);

    // bookmarks follow their pages when pages move
    use markupcraft_engine::synthetic::{SyntheticPage, pdf, text};
    let pages: Vec<SyntheticPage> = (0..3)
        .map(|i| {
            SyntheticPage::new(
                612.0,
                792.0,
                text(
                    72.0,
                    700.0,
                    14.0,
                    if i == 0 { "See www.example.com today" } else { "Sheet" },
                ),
            )
        })
        .collect();
    h.state_mut().state.open_bytes("set.pdf", None, pdf(&pages)).unwrap();
    h.run_steps(4);
    {
        let s = &mut h.state_mut().state.doc_mut().unwrap().session;
        s.add_bookmark(&[], None, "One", 0).unwrap();
        s.add_bookmark(&[], None, "Two", 1).unwrap();
        s.add_bookmark(&[], None, "Three", 2).unwrap();
    }
    markupcraft_ui_egui::panels::thumbnails::drop_pages(&mut h.state_mut().state, &[2], 0);
    h.run_steps(2);
    let titles: Vec<String> = h
        .state()
        .state
        .doc()
        .unwrap()
        .session
        .bookmarks()
        .into_iter()
        .map(|b| b.title)
        .collect();
    assert_eq!(titles, vec!["Three", "One", "Two"], "in page order after the move");

    // Ctrl+click on a web address in the page text opens it
    let w = h.state().state.doc().unwrap().session.page_words(1).unwrap();
    let url = w
        .iter()
        .find(|w| w.text.starts_with("www."))
        .expect("the address")
        .rect
        .normalized();
    let n = h.state().state.doc().unwrap().session.page_count();
    h.state_mut().state.doc_mut().unwrap().view.go_to_page(1, n);
    h.run_steps(4);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    mclick(
        &mut h,
        (url.x0 + url.x1) / 2.0,
        (url.y0 + url.y1) / 2.0,
        egui::PointerButton::Primary,
        egui::Modifiers::COMMAND,
    );
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run_steps(2);
    assert!(status(&h).contains("www.example.com"), "{}", status(&h));
}

#[test]
fn spelling_preferences_user_dictionary_and_auto_complete() {
    markupcraft_ui_egui::richedit::load_dictionary_blocking();
    let mut h = harness();
    // auto-complete from the managed list while typing a text box
    h.state_mut().state.shell.ui.extra.spell.phrases = vec!["Verify in field".into()];
    h.run_steps(2);
    h.state_mut().set_option("tool", "text");
    h.run_steps(2);
    click(&mut h, 100.0, 700.0);
    h.event(egui::Event::Text("Please ver".into()));
    h.run_steps(3);
    assert!(shows(&h, "Verify in field"), "the editor offers the phrase");
    h.get_by_label("Verify in field").click();
    h.run_steps(3);
    let text = h
        .state()
        .state
        .doc()
        .unwrap()
        .view
        .editor
        .as_ref()
        .map(|e| e.text.clone())
        .unwrap_or_default();
    assert_eq!(text, "Please Verify in field");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);

    // the user dictionary: Check Spelling's Add to Dictionary, and words accepted as typed
    let mut m = Markup::new(Kind::Text, 0, vec![Point::new(100.0, 400.0), Point::new(300.0, 430.0)]);
    m.contents = "Zorbix panel".into();
    h.state_mut().state.doc_mut().unwrap().session.add_markup(m).unwrap();
    run(&mut h, "markup.spell");
    press(&mut h, "Add to Dictionary");
    assert!(h.state().state.shell.ui.extra.spell.words.iter().any(|w| w == "Zorbix"));
    h.run_steps(2);
    assert!(
        markupcraft_ui_egui::richedit::misspelled("Zorbix").is_empty(),
        "accepted now"
    );
    // spelling as you type can be turned off
    h.state_mut().state.shell.ui.extra.spell.live = false;
    h.run_steps(2);
    assert!(markupcraft_ui_egui::richedit::misspelled("Qwertyx").is_empty());
    h.state_mut().state.shell.ui.extra.spell = Default::default();
    h.run_steps(2);
}
