//! Wave 6A through the real shell, headlessly (egui_kittest): the Web Tab, the JavaScript
//! Console, scanners (a fake eSCL scanner on loopback) and the test camera, PDF Packages,
//! insert options, layered pages, stitching, bookmarks from structure, Batch Sign & Seal,
//! Smart Overlay, file manager integration, File Access links, interactive stamps, stamp
//! settings, symbols, Mark Text for Redaction, Snapshot Content, the Sketch commands, the
//! modifier keys and the new Preferences pages. No real camera, scanner, browser or system
//! setting is touched.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use markupcraft_engine::synthetic::{SyntheticPage, pdf, rect, text};
use markupcraft_geom::Point;
use markupcraft_model::{Kind, Markup, Rect};
use markupcraft_ui_egui::MarkupCraftApp;
use markupcraft_ui_egui::commands::{Keys, bindings};

/// A letter page with text to mark and linework.
fn sample() -> Vec<u8> {
    pdf(&[
        SyntheticPage::new(
            612.0,
            792.0,
            format!(
                "{}{}{}",
                text(72.0, 700.0, 14.0, "SECRET plan here"),
                rect(100.0, 100.0, 300.0, 200.0),
                text(
                    72.0,
                    600.0,
                    10.0,
                    "Body text runs here in the usual size of the document."
                ),
            ),
        ),
        SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 14.0, "Second")),
    ])
}

fn harness_with(bytes: Vec<u8>) -> Harness<'static, MarkupCraftApp> {
    let mut h = Harness::builder()
        .with_size(vec2(1500.0, 950.0))
        .with_step_dt(1.0 / 60.0)
        .build_eframe(move |_cc| {
            let mut app = MarkupCraftApp::new();
            app.state.threads = 0;
            app.open_bytes("sample.pdf", None, bytes.clone()).unwrap();
            app
        });
    h.run_steps(6);
    h
}

fn harness() -> Harness<'static, MarkupCraftApp> {
    harness_with(sample())
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("markupcraft-final-ui-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
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

fn button(h: &mut Harness<'_, MarkupCraftApp>, at: Pos2, pressed: bool, m: Modifiers) {
    h.event(Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: m,
    });
    h.step();
}

fn click_mod(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64, m: Modifiers) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.step();
    h.event(Event::ModifiersChanged(m));
    button(h, at, true, m);
    button(h, at, false, m);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

fn click(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64) {
    click_mod(h, x, y, Modifiers::NONE);
}

fn drag_mod(h: &mut Harness<'_, MarkupCraftApp>, from: (f64, f64), to: (f64, f64), m: Modifiers) {
    let (a, b) = (screen(h, from.0, from.1), screen(h, to.0, to.1));
    h.hover_at(a);
    h.step();
    h.event(Event::ModifiersChanged(m));
    button(h, a, true, m);
    for i in 1..=8 {
        h.hover_at(a + (b - a) * (i as f32 / 8.0));
        h.step();
    }
    button(h, b, false, m);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
}

fn drag(h: &mut Harness<'_, MarkupCraftApp>, from: (f64, f64), to: (f64, f64)) {
    drag_mod(h, from, to, Modifiers::NONE);
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

fn markups(h: &Harness<'_, MarkupCraftApp>) -> Vec<Markup> {
    h.state().state.doc().unwrap().session.doc().markups.clone()
}

fn script(h: &mut Harness<'_, MarkupCraftApp>, paths: Vec<PathBuf>) {
    h.state_mut().state.dialogs.scripted = Some(paths);
}

/// Step until `done` or a time limit (workers: captures, scans).
fn wait(h: &mut Harness<'_, MarkupCraftApp>, done: impl Fn(&Harness<'_, MarkupCraftApp>) -> bool) {
    let start = Instant::now();
    while !done(h) && start.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(20));
        h.run_steps(2);
    }
}

fn select(h: &mut Harness<'_, MarkupCraftApp>, id: &str) {
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .select(&[id.to_string()])
        .unwrap();
    h.run_steps(2);
}

fn add(h: &mut Harness<'_, MarkupCraftApp>, m: Markup) -> String {
    let id = h.state_mut().state.doc_mut().unwrap().session.add_markup(m).unwrap();
    h.run_steps(2);
    id
}

fn find(h: &Harness<'_, MarkupCraftApp>, id: &str) -> Markup {
    h.state().state.doc().unwrap().session.doc().find(id).unwrap().clone()
}

#[test]
fn wave6a_shortcuts_are_bound() {
    let b = bindings();
    let k = |ctrl: bool, shift: bool, alt: bool, key: Key| Keys::new(ctrl, shift, alt, key);
    let (c, s, a, n) = (true, true, true, false);
    #[rustfmt::skip]
    let expected: &[(Keys, &str)] = &[
        (k(c, n, n, Key::T), "view.web_tab"), (k(n, n, a, Key::J), "window.javascript_console"),
        (k(c, n, a, Key::I), "markup.camera"), (k(n, s, n, Key::I), "markup.image_scanner"),
        (k(n, s, n, Key::K), "document.mark_text_redaction"), (k(n, s, n, Key::G), "document.snapshot_content"),
        (k(n, s, n, Key::U), "tool.squiggly"), (k(c, s, n, Key::U), "document.unflatten"),
    ];
    for (keys, id) in expected {
        assert!(
            b.iter().any(|(bk, bid)| bk == keys && bid == id),
            "{} should run {id}",
            keys.label()
        );
    }
    // Shift+U picks the Squiggly tool; Ctrl+Shift+U unflattens.
    let mut h = harness();
    key(&mut h, Modifiers::SHIFT, Key::U);
    assert_eq!(h.state().state.tool, "squiggly");
}

/// A stand-in browser that "prints" `src` to the --print-to-pdf path.
fn fake_browser(d: &Path, src: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let f = d.join("printer.cmd");
        let script = format!(
            "@echo off\r\n:loop\r\nif \"%~1\"==\"\" goto done\r\nset \"a=%~1\"\r\nif \"%a:~0,15%\"==\"--print-to-pdf=\" copy /y \"{}\" \"%a:~15%\" >nul\r\nshift\r\ngoto loop\r\n:done\r\n",
            src.display()
        );
        std::fs::write(&f, script).unwrap();
        f
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let f = d.join("printer.sh");
        let script = format!(
            "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in --print-to-pdf=*) cp '{}' \"${{a#--print-to-pdf=}}\";; esac; done\n",
            src.display()
        );
        std::fs::write(&f, script).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        f
    }
}

#[test]
fn web_tab_favorites_browser_and_capture() {
    let dir = temp("web");
    let mut h = harness();
    key(&mut h, Modifiers::COMMAND, Key::T);
    assert!(h.state().state.features.more6.web.open);
    assert_eq!(h.state().state.doc().unwrap().name, "Web Tab");
    assert!(shows(&h, "Open in Browser"));
    // An address opens in the system browser (headless: recorded, nothing launched).
    h.state_mut().state.features.more6.web.address = "example.com/spec".into();
    press(&mut h, "Open in Browser");
    assert_eq!(
        h.state().state.features.more6.web.last_opened.as_deref(),
        Some("https://example.com/spec")
    );
    // A favourite: saved in the preferences and listed as a link on the Web Tab.
    h.state_mut().state.features.more6.web.fav_name = "Spec".into();
    press(&mut h, "Add to Favorites");
    assert_eq!(h.state().state.shell.prefs.more.webtab.favorites.len(), 1);
    let links = h.state().state.doc().unwrap().session.links();
    assert_eq!(links.len(), 1, "{links:?}");
    // Capture as PDF with the browser set in Preferences > WebTab (a stand-in here).
    std::fs::write(
        dir.join("page.pdf"),
        pdf(&[SyntheticPage::new(
            612.0,
            792.0,
            text(72.0, 700.0, 14.0, "Example Domain"),
        )]),
    )
    .unwrap();
    let browser = fake_browser(&dir, &dir.join("page.pdf"));
    h.state_mut().state.shell.prefs.more.webtab.browser_path = browser.display().to_string();
    script(&mut h, vec![dir.join("captured.pdf")]);
    press(&mut h, "Capture as PDF...");
    wait(&mut h, |h| {
        h.state().state.doc().is_some_and(|d| d.name == "captured.pdf")
    });
    let d = h.state().state.doc().unwrap();
    assert_eq!(d.name, "captured.pdf", "{}", h.state().state.features.more6.web.message);
    assert!(d.session.page_text(0).unwrap().contains("Example Domain"));
    // Links to web pages follow Preferences > WebTab: capture asks where to save.
    h.state_mut().state.shell.prefs.more.webtab.open_links_in = "capture".into();
    h.state_mut().state.dialogs.scripted = None;
    let ctx = h.ctx.clone();
    markupcraft_ui_egui::features::more6::web::follow_url(&mut h.state_mut().state, &ctx, "https://example.org/x");
    assert_eq!(h.state().state.features.more6.web.capture_url, "https://example.org/x");
}

#[test]
fn javascript_console_runs_scripts_on_the_form() {
    let mut h = harness();
    {
        let d = h.state_mut().state.doc_mut().unwrap();
        d.session
            .form_add_field(
                0,
                Rect::new(72.0, 500.0, 300.0, 520.0),
                &markupcraft_engine::forms::NewFieldKind::Text { multiline: false },
                Some("Qty"),
            )
            .unwrap();
    }
    key(&mut h, Modifiers::ALT, Key::J);
    assert!(h.state().state.features.more6.script.open);
    assert!(shows(&h, "JavaScript Console"));
    h.state_mut().state.features.more6.script.code =
        "this.getField('Qty').value = 7; console.println('set'); this.pageNum = 1; 6 * 7".into();
    press(&mut h, "Run");
    let st = &h.state().state.features.more6.script;
    assert!(st.log.iter().any(|l| l == "42"), "{:?}", st.log);
    assert!(st.log.iter().any(|l| l == "set"), "{:?}", st.log);
    let d = h.state().state.doc().unwrap();
    assert_eq!(d.session.form_values().get("Qty").map(String::as_str), Some("7"));
    assert_eq!(d.view.current, 1);
    // Errors and endless loops come back as console lines.
    h.state_mut().state.features.more6.script.code = "while (true) {}".into();
    press(&mut h, "Run");
    assert!(
        h.state()
            .state
            .features
            .more6
            .script
            .log
            .last()
            .unwrap()
            .starts_with("error"),
        "{:?}",
        h.state().state.features.more6.script.log
    );
    run(&mut h, "tools.document_javascript");
    assert!(
        h.state().state.status.contains("document script"),
        "{}",
        h.state().state.status
    );
    key(&mut h, Modifiers::ALT, Key::J);
    assert!(!h.state().state.features.more6.script.open);
}

/// A fake eSCL scanner on loopback serving `pages` JPEGs, one per NextDocument.
fn fake_scanner(pages: Vec<Vec<u8>>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        let mut left = pages;
        let mut served_this_job = false;
        for s in l.incoming().take(60) {
            let Ok(mut s) = s else { continue };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let n = s.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let head = String::from_utf8_lossy(&buf).to_string();
                if let Some(end) = head.find("\r\n\r\n") {
                    let len: usize = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let line = String::from_utf8_lossy(&buf)
                .lines()
                .next()
                .unwrap_or_default()
                .to_string();
            let mut reply = |status: &str, headers: &str, body: &[u8]| {
                let _ = s.write_all(
                    format!("HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\n\r\n", body.len()).as_bytes(),
                );
                let _ = s.write_all(body);
            };
            if line.starts_with("GET /eSCL/ScannerCapabilities") {
                reply(
                    "200 OK",
                    "Content-Type: text/xml\r\n",
                    b"<scan:ScannerCapabilities xmlns:scan=\"x\" xmlns:pwg=\"y\"><pwg:MakeAndModel>Loopback Scanner</pwg:MakeAndModel><scan:Platen></scan:Platen><scan:ColorMode>RGB24</scan:ColorMode><scan:XResolution>300</scan:XResolution></scan:ScannerCapabilities>",
                );
            } else if line.starts_with("POST /eSCL/ScanJobs") {
                served_this_job = false;
                reply("201 Created", "Location: /eSCL/ScanJobs/1\r\n", b"");
            } else if line.starts_with("GET /eSCL/ScanJobs/1/NextDocument") && !served_this_job && !left.is_empty() {
                served_this_job = true;
                let p = left.remove(0);
                reply("200 OK", "Content-Type: image/jpeg\r\n", &p);
            } else if line.starts_with("GET /eSCL/ScanJobs/1/NextDocument") {
                reply("404 Not Found", "", b"");
            } else {
                reply("200 OK", "", b"");
            }
        }
    });
    format!("http://{addr}/eSCL")
}

fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 255) as u8, (y % 255) as u8, 90]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
    out.into_inner()
}

#[test]
fn scanner_and_camera_into_markups_pages_and_new_pdfs() {
    let dir = temp("acquire");
    let mut h = harness();
    let url = fake_scanner(vec![jpeg(120, 160), jpeg(100, 100)]);
    // Image From Scanner (Shift+I): connect, scan, finish, place.
    key(&mut h, Modifiers::SHIFT, Key::I);
    assert!(h.state().state.features.more6.acquire.open);
    h.state_mut().state.features.more6.acquire.scanner_url = url.clone();
    press(&mut h, "Connect");
    wait(&mut h, |h| h.state().state.features.more6.acquire.job.is_none());
    assert!(
        h.state().state.features.more6.acquire.caps.is_some(),
        "{}",
        h.state().state.features.more6.acquire.message
    );
    assert_eq!(h.state().state.shell.prefs.more.import_export.scanner_url, url);
    press(&mut h, "Scan");
    wait(&mut h, |h| h.state().state.features.more6.acquire.job.is_none());
    assert_eq!(
        h.state().state.features.more6.acquire.acquired.len(),
        1,
        "{}",
        h.state().state.features.more6.acquire.message
    );
    press(&mut h, "Finish");
    let before = markups(&h).len();
    click(&mut h, 400.0, 400.0);
    let all = markups(&h);
    assert_eq!(all.len(), before + 1, "{}", h.state().state.status);
    assert_eq!(all.last().unwrap().kind, Kind::Stamp);

    // Camera (Ctrl+Alt+I) with the test camera: Insert from Scanner or Camera adds a page.
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::I);
    assert!(h.state().state.features.more6.acquire.camera_tab);
    h.state_mut().state.features.more6.acquire.camera = Some(Box::new(markupcraft_engine::devices::TestCamera {
        width: 64,
        height: 48,
    }));
    run(&mut h, "document.insert_scanner");
    press(&mut h, "Camera");
    press(&mut h, "Take Picture");
    assert_eq!(h.state().state.features.more6.acquire.acquired.len(), 1);
    let pages = h.state().state.doc().unwrap().session.page_count();
    press(&mut h, "Finish");
    assert_eq!(
        h.state().state.doc().unwrap().session.page_count(),
        pages + 1,
        "{}",
        h.state().state.status
    );

    // File > Create from Scanner or Camera: a new PDF.
    run(&mut h, "file.create_from_scanner");
    press(&mut h, "Camera");
    press(&mut h, "Take Picture");
    press(&mut h, "Take Picture");
    script(&mut h, vec![dir.join("photos.pdf")]);
    press(&mut h, "Finish");
    let d = h.state().state.doc().unwrap();
    assert_eq!(
        d.name,
        "photos.pdf",
        "{}",
        h.state().state.features.more6.acquire.message
    );
    assert_eq!(d.session.page_count(), 2);
}

#[test]
fn packages_insert_options_layered_pages_stitching_and_source_bookmarks() {
    let dir = temp("docs");
    std::fs::write(
        dir.join("a.pdf"),
        pdf(&[SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, "INSERTED"))]),
    )
    .unwrap();
    std::fs::write(dir.join("notes.txt"), "notes\n").unwrap();
    let mut h = harness();
    // PDF Package.
    run(&mut h, "file.create_package");
    script(&mut h, vec![dir.join("a.pdf"), dir.join("notes.txt")]);
    press(&mut h, "Add Files...");
    assert_eq!(h.state().state.features.more6.docs.package_files.len(), 2);
    script(&mut h, vec![dir.join("package.pdf")]);
    press(&mut h, "Create...");
    let d = h.state().state.doc().unwrap();
    assert_eq!(d.name, "package.pdf", "{}", h.state().state.features.more6.docs.message);
    assert!(d.session.is_package());
    assert_eq!(d.session.package_files().len(), 2);
    h.state_mut().state.active = 0;
    h.run_steps(2);

    // Insert Pages with Options: interleaved, labelled from the file name.
    script(&mut h, vec![dir.join("a.pdf")]);
    run(&mut h, "document.insert_with_options");
    assert!(h.state().state.features.more6.docs.insert_file.is_some());
    h.state_mut().state.features.more6.docs.insert.labels_from_name = true;
    h.state_mut().state.features.more6.docs.insert_at = "2".into();
    press(&mut h, "Insert");
    let d = h.state().state.doc().unwrap();
    assert_eq!(d.session.page_count(), 3, "{}", h.state().state.status);
    assert!(d.session.page_text(1).unwrap().contains("INSERTED"));

    // Insert Layered Pages: a.pdf drawn on page 1 as a layer.
    script(&mut h, vec![dir.join("a.pdf")]);
    run(&mut h, "document.insert_layered");
    h.state_mut().state.features.more6.docs.layered_first = 1;
    h.state_mut().state.features.more6.docs.layered_name = "Consultant".into();
    press(&mut h, "Insert");
    let layers = h.state().state.doc().unwrap().session.layers();
    assert!(
        layers.iter().any(|l| l.name == "Consultant"),
        "{layers:?} {}",
        h.state().state.status
    );

    // Stitching pages 1 and 3 side by side into a new PDF.
    run(&mut h, "document.stitch");
    h.state_mut().state.features.more6.docs.stitch_pages = "1, 3".into();
    h.state_mut().state.features.more6.docs.stitch_overlap = 12.0;
    script(&mut h, vec![dir.join("stitched.pdf")]);
    press(&mut h, "Stitch...");
    let d = h.state().state.doc().unwrap();
    assert_eq!(
        d.name,
        "stitched.pdf",
        "{}",
        h.state().state.features.more6.docs.message
    );
    assert_eq!(d.session.page_count(), 1);
    assert!((d.session.page(0).unwrap().media.width() - (612.0 * 2.0 - 12.0)).abs() < 0.5);

    // Bookmarks from Structure: the larger text lines become bookmarks.
    h.state_mut().state.active = 0;
    h.run_steps(2);
    run(&mut h, "document.bookmarks_from_source");
    let n = h.state().state.doc().unwrap().session.bookmarks().len();
    assert!(n >= 1, "{}", h.state().state.status);
}

#[test]
fn batch_sign_and_seal_smart_overlay_and_shell_integration() {
    let dir = temp("batch");
    for n in ["a", "b"] {
        std::fs::write(
            dir.join(format!("{n}.pdf")),
            pdf(&[SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, "Sheet"))]),
        )
        .unwrap();
    }
    let p12 = markupcraft_engine::signatures::create_digital_id(
        &markupcraft_engine::signatures::IdentityInfo {
            name: "Engineer".into(),
            ..Default::default()
        },
        2,
        "pw",
    )
    .unwrap();
    std::fs::write(dir.join("id.p12"), p12).unwrap();
    std::fs::create_dir_all(dir.join("signed")).unwrap();
    let mut h = harness();
    run(&mut h, "batch.sign_seal");
    script(&mut h, vec![dir.join("a.pdf"), dir.join("b.pdf")]);
    press(&mut h, "Add Files...");
    script(&mut h, vec![dir.join("id.p12")]);
    h.get_all_by_label("Choose...").next().unwrap().click();
    h.run_steps(4);
    h.state_mut().state.features.more6.batch.password = "pw".into();
    h.state_mut().state.features.more6.batch.out_dir = Some(dir.join("signed"));
    press(&mut h, "Sign & Seal");
    let st = &h.state().state.features.more6.batch;
    assert_eq!(st.results.len(), 2, "{}", st.message);
    assert!(st.results.iter().all(|r| r.error.is_empty()), "{:?}", st.results);
    let signed = markupcraft_engine::Session::open(dir.join("signed/a signed.pdf")).unwrap();
    assert_eq!(signed.signatures(&[]).unwrap().len(), 1);

    // Smart Overlay of two one-sheet sets.
    for (sub, dx) in [("cur", 0.0), ("rev", 15.0)] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
        let body = format!(
            "{}{}",
            rect(100.0 + dx, 100.0, 300.0, 200.0),
            rect(150.0 + dx, 400.0, 100.0, 150.0)
        );
        std::fs::write(
            dir.join(sub).join("A-101.pdf"),
            pdf(&[SyntheticPage::new(612.0, 792.0, body)]),
        )
        .unwrap();
    }
    std::fs::create_dir_all(dir.join("ovl")).unwrap();
    run(&mut h, "batch.smart_overlay");
    {
        let st = &mut h.state_mut().state.features.more6.batch;
        st.current = vec![dir.join("cur/A-101.pdf")];
        st.revised = vec![dir.join("rev/A-101.pdf")];
        st.smart_out = Some(dir.join("ovl"));
        st.shading = true;
    }
    h.run_steps(2);
    press(&mut h, "Overlay");
    let r = h.state().state.features.more6.batch.report.clone();
    let r = r.unwrap_or_else(|| panic!("{}", h.state().state.features.more6.batch.message));
    assert_eq!(r.sheets.len(), 1);
    assert_eq!(r.sheets[0].discipline, "Architectural");
    assert!(r.sheets[0].score > 0.5, "{r:?}");
    assert!(shows(&h, "Architectural (1)"));

    // File Manager Integration: files written to a folder, nothing installed.
    std::fs::create_dir_all(dir.join("int")).unwrap();
    run(&mut h, "file.shell_integration");
    press(&mut h, "Linux");
    script(&mut h, vec![dir.join("int")]);
    press(&mut h, "Write Files to a Folder...");
    let st = &h.state().state.features.more6.docs;
    assert!(
        st.shell_written.iter().any(|f| f.ends_with("README.txt")),
        "{}",
        st.message
    );
    assert!(dir.join("int/markupcraft-servicemenu.desktop").is_file());
}

#[test]
fn interactive_stamps_stamp_settings_symbols_and_file_links() {
    let dir = temp("stamps");
    let mut h = harness();
    run(&mut h, "markup.interactive_stamp");
    assert!(shows(&h, "Interactive Stamp"));
    press(&mut h, "Place...");
    let before = markups(&h).len();
    click(&mut h, 400.0, 300.0);
    assert_eq!(markups(&h).len(), before + 1, "{}", h.state().state.status);
    // The fields open for filling in.
    let (id, mut fields) = h
        .state()
        .state
        .features
        .more6
        .stamps
        .editing
        .clone()
        .expect("fields open");
    assert!(fields.len() >= 4, "{fields:?}");
    for f in &mut fields {
        if f.label == "No Exceptions Taken" {
            f.value = "on".into();
        }
        if f.label == "Status" {
            f.value = "Closed".into();
        }
    }
    h.state_mut().state.features.more6.stamps.editing = Some((id.clone(), fields));
    h.run_steps(2);
    press(&mut h, "Apply");
    let now = h.state().state.doc().unwrap().session.stamp_fields(&id).unwrap();
    assert!(
        now.iter().any(|f| f.label == "Status" && f.value == "Closed"),
        "{now:?}"
    );
    // Edit Stamp Fields on the selected stamp.
    select(&mut h, &id);
    run(&mut h, "markup.stamp_fields");
    assert!(h.state().state.features.more6.stamps.editing.is_some());
    h.state_mut().state.features.more6.stamps.editing = None;

    // Stamp settings: default stamp, opacity, multiply, lock (this session; no config folder).
    run(&mut h, "markup.stamp_settings");
    assert!(h.state().state.features.more6.stamps.library_ids.len() >= 16);
    {
        let s = &mut h.state_mut().state.features.more6.stamps.settings;
        s.default_stamp = "Reviewed".into();
        s.opacity = 0.5;
        s.blend = "multiply".into();
        s.lock = true;
    }
    press(&mut h, "Save");
    run(&mut h, "markup.place_default_stamp");
    click(&mut h, 450.0, 450.0);
    let m = markups(&h).last().unwrap().clone();
    assert_eq!(m.kind, Kind::Stamp);
    assert!((m.opacity - 0.5).abs() < 1e-6 && m.locked() && m.multiply, "{m:?}");

    // A symbol: the selected markup saved in the Symbols tool set in Drawing mode.
    let r = add(
        &mut h,
        Markup::new(
            Kind::Rectangle,
            0,
            Rect::new(300.0, 300.0, 340.0, 320.0).corners().to_vec(),
        ),
    );
    select(&mut h, &r);
    run(&mut h, "markup.symbol_from_selection");
    let chest = &h.state().state.toolchest;
    let set = chest.sets.iter().find(|s| s.title == "Symbols").expect("Symbols set");
    assert_eq!(set.items.len(), 1);
    assert_eq!(set.items[0].mode, markupcraft_ui_egui::chest::Mode::Drawing);

    // File Access: a link area that opens a file.
    let target = dir.join("spec.pdf");
    std::fs::write(&target, sample()).unwrap();
    markupcraft_ui_egui::features::more6::docs::start_link_file(&mut h.state_mut().state, target.clone());
    drag(&mut h, (100.0, 400.0), (200.0, 450.0));
    let links = h.state().state.doc().unwrap().session.links();
    assert!(
        links.iter().any(|l| matches!(&l.target, markupcraft_engine::links::LinkTarget::File { path, .. } if path.ends_with("spec.pdf"))),
        "{links:?} {}",
        h.state().state.status
    );
}

#[test]
fn mark_text_for_redaction_snapshot_content_and_sketch_commands() {
    let mut h = harness();
    key(&mut h, Modifiers::SHIFT, Key::K);
    drag(&mut h, (60.0, 690.0), (140.0, 720.0));
    assert_eq!(
        h.state().state.doc().unwrap().session.redact_marks().len(),
        1,
        "{}",
        h.state().state.status
    );
    key(&mut h, Modifiers::NONE, Key::Escape);
    let before = markups(&h).len();
    key(&mut h, Modifiers::SHIFT, Key::G);
    drag(&mut h, (90.0, 90.0), (310.0, 310.0));
    assert_eq!(markups(&h).len(), before + 1, "{}", h.state().state.status);
    // Sketch: the relative-angle toggle and placing a typed segment while drawing.
    run(&mut h, "tools.sketch_relative");
    assert!(h.state().state.edit.sketch.relative);
    run(&mut h, "tools.sketch_relative");
    assert!(!h.state().state.edit.sketch.relative);
    key(&mut h, Modifiers::SHIFT, Key::N);
    click(&mut h, 100.0, 500.0);
    assert!(h.state().state.enabled("tools.sketch_place"));
    h.state_mut().state.edit.sketch.length = "1".into();
    h.state_mut().state.edit.sketch.angle = "0".into();
    run(&mut h, "tools.sketch_place");
    run(&mut h, "tools.sketch_finish");
    let m = markups(&h).last().unwrap().clone();
    assert_eq!(m.kind, Kind::Polyline, "{}", h.state().state.status);
    // One inch on an unscaled sheet, to the right.
    assert!(
        (m.pts[1].x - m.pts[0].x - 72.0).abs() < 0.5 && (m.pts[1].y - m.pts[0].y).abs() < 0.5,
        "{:?}",
        m.pts
    );
}

#[test]
fn modifier_keys_draw_from_centre_arcs_vertices_aspect_and_callouts() {
    let mut h = harness();
    // Alt draws a rectangle from its centre.
    key(&mut h, Modifiers::NONE, Key::R);
    drag_mod(&mut h, (300.0, 300.0), (350.0, 330.0), Modifiers::ALT);
    let m = markups(&h).last().unwrap().clone();
    assert_eq!(m.kind, Kind::Rectangle);
    let b = markupcraft_geom::bbox(&m.pts).unwrap();
    assert!(
        (b.width() - 100.0).abs() < 1.5 && (b.height() - 60.0).abs() < 1.5,
        "{b:?}"
    );
    assert!((b.x0 - 250.0).abs() < 1.5 && (b.y0 - 270.0).abs() < 1.5, "{b:?}");

    // Alt+click builds a three-point arc while drawing a polyline.
    key(&mut h, Modifiers::SHIFT, Key::N);
    click(&mut h, 100.0, 450.0);
    click_mod(&mut h, 150.0, 500.0, Modifiers::ALT);
    click(&mut h, 200.0, 450.0);
    click(&mut h, 260.0, 450.0);
    key(&mut h, Modifiers::NONE, Key::Enter);
    let m = markups(&h).last().unwrap().clone();
    assert_eq!(m.kind, Kind::Polyline);
    assert_eq!(m.arcs.len(), 1, "{m:?}");
    key(&mut h, Modifiers::NONE, Key::V);

    // Shift+click: delete a vertex, add one on a segment; Ctrl+click: curve a vertex's segment.
    let poly = add(
        &mut h,
        Markup::new(
            Kind::Polygon,
            0,
            vec![
                Point::new(400.0, 400.0),
                Point::new(500.0, 400.0),
                Point::new(500.0, 500.0),
                Point::new(450.0, 550.0),
                Point::new(400.0, 500.0),
            ],
        ),
    );
    select(&mut h, &poly);
    click_mod(&mut h, 450.0, 550.0, Modifiers::SHIFT);
    assert_eq!(find(&h, &poly).pts.len(), 4, "{}", h.state().state.status);
    click_mod(&mut h, 450.0, 400.0, Modifiers::SHIFT);
    assert_eq!(find(&h, &poly).pts.len(), 5, "{}", h.state().state.status);
    assert!(h.state().state.doc().unwrap().selection().contains(&poly));
    click_mod(&mut h, 500.0, 400.0, Modifiers::COMMAND);
    let p = find(&h, &poly);
    assert_eq!(p.arcs.len(), 1, "{}", h.state().state.status);

    // Stamps keep their aspect ratio from a corner; Shift breaks it.
    let img = h
        .state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .place_stamp(
            0,
            markupcraft_engine::stamps::StampPlace::Rect(Rect::new(100.0, 250.0, 140.0, 270.0)),
            &markupcraft_engine::stamps::StampSource::Library("Approved".into()),
            None,
            &Default::default(),
            None,
        )
        .unwrap();
    h.run_steps(2);
    select(&mut h, &img);
    let m = find(&h, &img);
    let corner = markupcraft_ui_egui::painter::handles(&m)
        .into_iter()
        .max_by(|a, b| (a.x + a.y).total_cmp(&(b.x + b.y)))
        .unwrap();
    drag(&mut h, (corner.x, corner.y), (corner.x + 40.0, corner.y + 5.0));
    let b = markupcraft_geom::bbox(&find(&h, &img).pts).unwrap();
    assert!(
        (b.width() - 80.0).abs() < 2.0,
        "resized: {b:?} {}",
        h.state().state.status
    );
    assert!((b.width() / b.height() - 2.0).abs() < 0.05, "kept: {b:?}");
    let m = find(&h, &img);
    let corner = markupcraft_ui_egui::painter::handles(&m)
        .into_iter()
        .max_by(|a, b| (a.x + a.y).total_cmp(&(b.x + b.y)))
        .unwrap();
    drag_mod(
        &mut h,
        (corner.x, corner.y),
        (corner.x, corner.y + 30.0),
        Modifiers::SHIFT,
    );
    let b = markupcraft_geom::bbox(&find(&h, &img).pts).unwrap();
    assert!((b.width() / b.height() - 2.0).abs() > 0.2, "broken: {b:?}");

    // Alt+drag a callout's handle: the whole callout moves, tip included.
    let c = markupcraft_ui_egui::tools::new_markup(
        Kind::Callout,
        0,
        &[
            Point::new(150.0, 650.0),
            Point::new(200.0, 700.0),
            Point::new(280.0, 670.0),
        ],
    )
    .unwrap();
    let cid = add(&mut h, c);
    select(&mut h, &cid);
    let before = find(&h, &cid).pts.clone();
    let hd = markupcraft_ui_egui::painter::handles(&find(&h, &cid))[0];
    drag_mod(&mut h, (hd.x, hd.y), (hd.x + 30.0, hd.y), Modifiers::ALT);
    let after = find(&h, &cid).pts.clone();
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        assert!(
            (b.x - a.x - 30.0).abs() < 1.5 && (b.y - a.y).abs() < 1.5,
            "{before:?} {after:?}"
        );
    }
}

#[test]
fn preferences_pages_are_shown_and_read_by_the_features() {
    let dir = temp("prefs");
    let mut h = harness();
    run(&mut h, "window.preferences");
    for page in [
        "Markups List",
        "Layers",
        "Measure",
        "Forms",
        "Signature",
        "Tablet",
        "WebTab",
        "Sets",
        "Import/Export",
        "Integrations",
    ] {
        let node = h
            .query_all_by_label_contains(page)
            .last()
            .unwrap_or_else(|| panic!("page {page}"));
        node.click();
        h.run_steps(3);
        assert_eq!(h.state().state.shell.prefs_page, page);
    }
    // Integrations: a service whose Sign In opens its page in the browser.
    press(&mut h, "Add Service");
    h.state_mut().state.shell.prefs.more.integrations.services[0].url = "https://forms.example.com/login".into();
    h.run_steps(3);
    press(&mut h, "Sign In");
    assert_eq!(
        h.state().state.features.more6.web.last_opened.as_deref(),
        Some("https://forms.example.com/login")
    );
    // Toggle Markups List > go to the markup through the dialog.
    let node = h.query_all_by_label_contains("Markups List").last().unwrap();
    node.click();
    h.run_steps(3);
    press(&mut h, "Selecting a markup in the list goes to it on the page");
    assert!(!h.state().state.shell.prefs.more.markups_list.zoom_to_selected);
    assert!(!markupcraft_ui_egui::features::more6::prefs::zoom_to_selected());
    press(&mut h, "Selecting a markup in the list goes to it on the page");
    assert!(markupcraft_ui_egui::features::more6::prefs::zoom_to_selected());

    // The rest, set as a settings file would, then applied.
    std::fs::create_dir_all(dir.join("ids")).unwrap();
    std::fs::create_dir_all(dir.join("trust")).unwrap();
    std::fs::write(dir.join("ids/me.p12"), b"x").unwrap();
    {
        let m = &mut h.state_mut().state.shell.prefs.more;
        m.measure.fill_gap_pt = 5.0;
        m.measure.fill_cutouts = false;
        m.import_export.image_dpi = 300;
        m.sets.latest_only = false;
        m.tablet.eraser_scales_with_zoom = true;
        m.tablet.eraser_px = 10.0;
        m.signature.digital_id_folder = dir.join("ids").display().to_string();
        m.forms.single_key_shortcuts = false;
        m.layers.hide_children_with_parent = true;
    }
    markupcraft_ui_egui::prefs_ui::apply_live(&mut h.state_mut().state);
    let s = &h.state().state;
    assert_eq!(s.features.fill.gap, 5.0);
    assert!(!s.features.fill.cutouts);
    assert_eq!(s.features.export.dpi, 300.0);
    assert!(!s.features.sets.latest_only);
    assert_eq!(markupcraft_ui_egui::gestures::eraser_px(2.0), 20.0);
    assert_eq!(
        s.features.signatures.id_path.as_deref(),
        Some(dir.join("ids/me.p12").as_path())
    );
    // Single-key form shortcuts are off: X does not start a signature field.
    assert!(!markupcraft_ui_egui::features::more6::prefs::key_allowed(
        s,
        &Keys::new(false, false, false, Key::X),
        "forms.signature_field"
    ));
    key(&mut h, Modifiers::NONE, Key::Escape);
    key(&mut h, Modifiers::NONE, Key::X);
    assert!(h.state().state.features.pick.is_none());
    // Layers: hiding a parent hides its children.
    {
        let d = h.state_mut().state.doc_mut().unwrap();
        d.session.create_layer("Arch").unwrap();
        d.session.create_layer("Doors").unwrap();
        d.session.nest_layer("Doors", Some("Arch"), None).unwrap();
    }
    assert_eq!(
        markupcraft_ui_egui::features::more6::prefs::layer_children(&h.state().state, "Arch"),
        ["Doors"]
    );
    // Forms: fields are highlighted in the chosen colour.
    {
        let d = h.state_mut().state.doc_mut().unwrap();
        d.session
            .form_add_field(
                0,
                Rect::new(72.0, 500.0, 300.0, 520.0),
                &markupcraft_engine::forms::NewFieldKind::Text { multiline: false },
                Some("Name"),
            )
            .unwrap();
    }
    let mut marks = Vec::new();
    markupcraft_ui_egui::features::more6::prefs::form_marks(
        &h.state().state,
        h.state().state.doc().unwrap(),
        &mut marks,
    );
    assert_eq!(marks.len(), 1);
    h.state_mut().state.shell.prefs.more.forms.highlight_fields = false;
    let mut marks = Vec::new();
    markupcraft_ui_egui::features::more6::prefs::form_marks(
        &h.state().state,
        h.state().state.doc().unwrap(),
        &mut marks,
    );
    assert!(marks.is_empty());
    // Back to the defaults for the other tests in this process.
    h.state_mut().state.shell.prefs.more = Default::default();
    markupcraft_ui_egui::prefs_ui::apply_live(&mut h.state_mut().state);
}

#[test]
fn overlay_advanced_color_shading() {
    let dir = temp("overlay");
    for (n, dx) in [("a.pdf", 0.0), ("b.pdf", 10.0)] {
        std::fs::write(
            dir.join(n),
            pdf(&[SyntheticPage::new(612.0, 792.0, rect(100.0 + dx, 100.0, 200.0, 200.0))]),
        )
        .unwrap();
    }
    let mut h = harness();
    run(&mut h, "file.overlay");
    script(&mut h, vec![dir.join("a.pdf"), dir.join("b.pdf")]);
    press(&mut h, "Edit Defaults");
    press(&mut h, "Advanced color shading");
    assert!(h.state().state.features.overlay.advanced_shading);
    markupcraft_ui_egui::features::overlay::add_files(
        &mut h.state_mut().state,
        &[dir.join("a.pdf"), dir.join("b.pdf")],
    );
    h.run_steps(2);
    markupcraft_ui_egui::features::overlay::write(&mut h.state_mut().state, &dir.join("out.pdf"));
    let bytes = std::fs::read(dir.join("out.pdf")).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("/Screen"));
}
