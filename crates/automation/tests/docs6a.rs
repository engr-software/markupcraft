//! End-to-end tests of wave 6A's tools through `Automation::call`: PDF Packages, bookmarks from
//! the source structure, insert options and layered pages, stitching, Batch Sign & Seal, file
//! manager integration, Smart Overlay and Advanced Color Shading, document JavaScript, the Web
//! Tab, scanners (a fake eSCL scanner on loopback) and the test camera, interactive stamps,
//! stamp settings and Mark Text for Redaction.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use markupcraft_automation::Automation;
use markupcraft_engine::synthetic::{SyntheticPage, pdf, rect, text};
use serde_json::{Value, json};

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-docs6a-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn auto(d: &Path) -> Automation {
    Automation::new()
        .with_root(d)
        .unwrap()
        .with_author("Tester")
        .with_config_dir(d.join("config"))
}

fn call(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args) {
        Ok(v) => v,
        Err(e) => panic!("{tool} {args}: {e}"),
    }
}

fn write(d: &Path, name: &str, pages: &[SyntheticPage]) {
    std::fs::write(d.join(name), pdf(pages)).unwrap();
}

fn page(content: String) -> SyntheticPage {
    SyntheticPage::new(612.0, 792.0, content)
}

#[test]
fn packages_source_bookmarks_insert_options_layers_and_stitching() {
    let d = dir();
    let heading = |t: &str, sub: &str| {
        format!(
            "{}{}{}{}",
            text(72.0, 720.0, 24.0, t),
            text(72.0, 680.0, 16.0, sub),
            text(
                72.0,
                650.0,
                10.0,
                "Body text runs here in the usual size of the document."
            ),
            text(72.0, 635.0, 10.0, "More body text so the body size is the common one.")
        )
    };
    write(
        &d,
        "spec.pdf",
        &[
            page(heading("Mechanical", "Ductwork")),
            page(heading("Electrical", "Lighting")),
        ],
    );
    write(
        &d,
        "odd.pdf",
        &[
            page(text(72.0, 700.0, 12.0, "ODD")),
            page(text(72.0, 700.0, 12.0, "ODD2")),
        ],
    );
    std::fs::write(d.join("notes.txt"), "site notes\n").unwrap();
    let mut a = auto(&d);

    // PDF Package: a cover page plus the embedded member files, marked /Collection.
    let v = call(
        &mut a,
        "pdf_package",
        json!({ "action": "create", "files": ["odd.pdf", "notes.txt"], "out": "pkg.pdf" }),
    );
    assert_eq!(v["files"], 2);
    call(&mut a, "doc_open", json!({ "path": "pkg.pdf" }));
    let v = call(&mut a, "pdf_package", json!({ "action": "info" }));
    assert_eq!(v["package"], true);
    assert_eq!(v["files"].as_array().unwrap().len(), 2, "{v}");

    // Bookmarks from the headings.
    call(&mut a, "doc_open", json!({ "path": "spec.pdf" }));
    let v = call(&mut a, "bookmarks_from_source", json!({ "preview": true }));
    assert_eq!(v["source"], "headings");
    assert_eq!(v["headings"][0]["title"], "Mechanical");
    let v = call(&mut a, "bookmarks_from_source", json!({ "replace": true }));
    assert_eq!(v["bookmarks"], 4);
    let v = call(&mut a, "bookmark_list", json!({}));
    assert!(v.to_string().contains("Ductwork"), "{v}");

    // Insert with options: interleaved, labelled from the file name, its bookmarks carried.
    let v = call(
        &mut a,
        "pages_insert_with",
        json!({ "path": "odd.pdf", "interleave": true, "labels_from_name": true, "bookmarks": true }),
    );
    assert_eq!(v["pages_after"], 4);
    let t = call(&mut a, "page_text", json!({ "page": 2 }));
    assert!(t.to_string().contains("ODD"), "{t}");

    // Layered pages: odd.pdf's first page drawn on page 1 as a layer.
    let v = call(
        &mut a,
        "pages_insert_layered",
        json!({ "path": "odd.pdf", "pages": [1], "first": 1, "name": "Overlay A" }),
    );
    assert_eq!(v["layered"], 1);
    let v = call(&mut a, "layer_list", json!({}));
    assert!(v.to_string().contains("Overlay A"), "{v}");

    // Stitching two pages side by side.
    let v = call(
        &mut a,
        "pages_stitch",
        json!({ "pages": [1, 2], "columns": 0, "overlap": 12, "out": "stitched.pdf" }),
    );
    assert_eq!(v["pages"], 2);
    assert_eq!(v["width"], 612.0 * 2.0 - 12.0);
    assert!(d.join("stitched.pdf").is_file());
}

#[test]
fn batch_sign_and_seal_shell_integration_and_smart_overlay() {
    let d = dir();
    for n in ["a", "b"] {
        write(&d, &format!("{n}.pdf"), &[page(text(72.0, 700.0, 12.0, "Sheet"))]);
    }
    let mut a = auto(&d);
    call(
        &mut a,
        "digital_id_create",
        json!({ "name": "Engineer", "password": "pw", "out": "id.p12" }),
    );
    std::fs::create_dir_all(d.join("signed")).unwrap();
    let v = call(
        &mut a,
        "batch_sign",
        json!({
            "files": ["a.pdf", "b.pdf"], "p12": "id.p12", "password": "pw",
            "rect": [400, 50, 560, 110], "date_format": "yyyy", "date_rect": [400, 120, 560, 140],
            "reason": "Sealed", "out_dir": "signed", "suffix": " signed"
        }),
    );
    assert_eq!(v["signed"], 2, "{v}");
    call(&mut a, "doc_open", json!({ "path": "signed/a signed.pdf" }));
    let v = call(&mut a, "signature_list", json!({}));
    assert!(v.to_string().contains("\"signed\":true"), "{v}");
    // A wrong password fails the whole batch before any file changes.
    assert!(
        a.call(
            "batch_sign",
            &json!({ "files": ["a.pdf"], "p12": "id.p12", "password": "no" })
        )
        .is_err()
    );

    // Integration files for every system, and the combine entry point.
    std::fs::create_dir_all(d.join("int")).unwrap();
    let v = call(
        &mut a,
        "shell_integration",
        json!({ "action": "write", "dir": "int", "cli": "/opt/markupcraft/markupcraft-cli", "os": "linux" }),
    );
    assert!(v["files"].to_string().contains("README.txt"));
    let v = call(
        &mut a,
        "shell_integration",
        json!({ "action": "combine", "files": ["a.pdf", "b.pdf"] }),
    );
    assert!(v["path"].as_str().unwrap().ends_with("a combined.pdf"));

    // Smart Overlay: two sets, sheets registered by their drawing, a score per sheet.
    for (sub, dx) in [("cur", 0.0), ("rev", 15.0)] {
        std::fs::create_dir_all(d.join(sub)).unwrap();
        let body = format!(
            "{}{}{}",
            rect(100.0 + dx, 100.0, 300.0, 200.0),
            rect(150.0 + dx, 400.0, 100.0, 150.0),
            text(450.0, 60.0, 14.0, "A-101")
        );
        write(&d.join(sub), "A-101.pdf", &[page(body)]);
    }
    std::fs::create_dir_all(d.join("ovl")).unwrap();
    let v = call(
        &mut a,
        "smart_overlay",
        json!({ "current": ["cur"], "revised": ["rev"], "out_dir": "ovl", "advanced_shading": true, "report": "ovl/report.csv" }),
    );
    assert_eq!(v["sheets"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(v["sheets"][0]["discipline"], "Architectural");
    assert!(v["sheets"][0]["score"].as_f64().unwrap() > 0.5, "{v}");
    assert!(d.join("ovl/report.csv").is_file());

    // Overlay Pages with Advanced Color Shading.
    let v = call(
        &mut a,
        "overlay_pages",
        json!({ "layers": [{ "path": "cur/A-101.pdf" }, { "path": "rev/A-101.pdf" }], "out": "shaded.pdf", "advanced_shading": true }),
    );
    assert_eq!(v["pages"], 1);
    let bytes = std::fs::read(d.join("shaded.pdf")).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("/Screen"));
}

#[test]
fn javascript_console_and_document_scripts() {
    let d = dir();
    write(&d, "form.pdf", &[page(String::new()), page(String::new())]);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "form.pdf" }));
    for (name, y) in [("Qty", 700), ("Total", 660)] {
        call(
            &mut a,
            "form_add_field",
            json!({ "page": 1, "rect": [72, y, 300, y + 20], "type": "text", "name": name }),
        );
    }
    call(&mut a, "form_fill", json!({ "values": { "Qty": "4" } }));
    let v = call(&mut a, "javascript", json!({ "action": "run", "script": "6 * 7" }));
    assert_eq!(v["result"], "42");
    let v = call(
        &mut a,
        "javascript",
        json!({ "action": "run", "script": "this.getField('Total').value = this.getField('Qty').value * 2; console.println('ok'); this.pageNum = 1;" }),
    );
    assert_eq!(v["changed"], json!(["Total"]));
    assert_eq!(v["go_to_page"], 2);
    let v = call(&mut a, "form_list", json!({}));
    assert!(v.to_string().contains("\"8\""), "{v}");
    let v = call(
        &mut a,
        "javascript",
        json!({ "action": "run", "script": "while(true){}" }),
    );
    assert!(v["error"].is_string());
    let v = call(&mut a, "javascript", json!({ "action": "list" }));
    assert_eq!(v["scripts"], json!([]));
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
fn web_tab_link_pages_and_page_capture() {
    let d = dir();
    let mut a = auto(&d);
    let v = call(
        &mut a,
        "webtab",
        json!({ "action": "links", "title": "Favorites", "links": [{ "name": "Spec", "url": "example.com/spec" }, { "url": "https://example.org" }], "out": "links.pdf" }),
    );
    assert_eq!(v["links"], 2);
    call(&mut a, "doc_open", json!({ "path": "links.pdf" }));
    let v = call(&mut a, "link_list", json!({}));
    assert!(v.to_string().contains("https://example.com/spec"), "{v}");
    assert!(
        a.call(
            "webtab",
            &json!({ "action": "links", "links": [{ "url": "javascript:alert(1)" }], "out": "x.pdf" })
        )
        .is_err()
    );
    write(&d, "page.pdf", &[page(text(72.0, 700.0, 14.0, "Example Domain"))]);
    let browser = fake_browser(&d, &d.join("page.pdf"));
    let v = call(
        &mut a,
        "webtab",
        json!({ "action": "capture", "url": "https://example.com", "browser": browser.display().to_string(), "out": "captured.pdf", "timeout": 30 }),
    );
    assert_eq!(v["pages"], 1, "{v}");
    call(&mut a, "doc_open", json!({ "path": "captured.pdf" }));
    let t = call(&mut a, "page_text", json!({ "page": 1 }));
    assert!(t.to_string().contains("Example Domain"));
}

/// A fake eSCL scanner on loopback serving `pages` JPEGs to one job.
fn fake_scanner(pages: Vec<Vec<u8>>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        let mut left = pages;
        for s in l.incoming().take(30) {
            let Ok(mut s) = s else { continue };
            // Read the whole request: the headers, then Content-Length bytes of body.
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
            let n = buf.len();
            let line = String::from_utf8_lossy(&buf[..n])
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
                    b"<scan:ScannerCapabilities xmlns:scan=\"x\" xmlns:pwg=\"y\"><pwg:MakeAndModel>Loopback Scanner</pwg:MakeAndModel><scan:Platen></scan:Platen><scan:Adf></scan:Adf><scan:XResolution>300</scan:XResolution></scan:ScannerCapabilities>",
                );
            } else if line.starts_with("POST /eSCL/ScanJobs") {
                reply("201 Created", "Location: /eSCL/ScanJobs/1\r\n", b"");
            } else if line.starts_with("GET /eSCL/ScanJobs/1/NextDocument") && !left.is_empty() {
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
fn scanner_and_camera_pages_markups_and_new_pdfs() {
    let d = dir();
    write(&d, "a.pdf", &[page(String::new())]);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "a.pdf" }));
    let url = fake_scanner(vec![jpeg(120, 160), jpeg(100, 100)]);
    let v = call(&mut a, "scan", json!({ "action": "capabilities", "url": url }));
    assert_eq!(v["make_and_model"], "Loopback Scanner");
    assert_eq!(v["sources"], json!(["Platen", "Feeder"]));
    let v = call(
        &mut a,
        "scan",
        json!({ "action": "scan", "url": url, "source": "Feeder", "into": "pages" }),
    );
    assert_eq!(v["inserted"], 2);
    assert_eq!(v["pages"], 3);
    assert!(
        a.call("scan", &json!({ "action": "scan", "url": "https://x/eSCL" }))
            .is_err()
    );
    // The test camera: an image markup, then a new PDF.
    let v = call(
        &mut a,
        "camera_capture",
        json!({ "into": "image", "page": 1, "point": [300, 400], "width": 64, "height": 48 }),
    );
    assert!(v["id"].is_string());
    let v = call(&mut a, "markup_list", json!({}));
    assert!(v.to_string().contains("Stamp"), "{v}");
    let v = call(&mut a, "camera_capture", json!({ "into": "pdf", "out": "photo.pdf" }));
    assert_eq!(v["pages"], 1);
}

#[test]
fn interactive_stamps_stamp_settings_and_mark_text_for_redaction() {
    let d = dir();
    write(&d, "a.pdf", &[page(text(72.0, 700.0, 14.0, "SECRET plan here"))]);
    let mut a = auto(&d);
    call(&mut a, "doc_open", json!({ "path": "a.pdf" }));
    let v = call(
        &mut a,
        "stamp_interactive",
        json!({ "action": "place", "page": 1, "at": [300, 300], "text": "REVIEWED\r{check:No Exceptions}  {check:Make Corrections}\r{choice:Status=Open|Closed}  {field:By}", "values": { "No Exceptions": true, "By": "RT" } }),
    );
    let id = v["id"].as_str().unwrap().to_string();
    assert_eq!(v["fields"].as_array().unwrap().len(), 4, "{v}");
    assert_eq!(v["fields"][0]["value"], "on");
    let v = call(
        &mut a,
        "stamp_interactive",
        json!({ "action": "set", "id": id, "values": { "Make Corrections": "on", "Status": "Closed" } }),
    );
    assert!(v.to_string().contains("Closed"));
    assert!(
        a.call(
            "stamp_interactive",
            &json!({ "action": "set", "id": id, "values": { "Status": "Maybe" } })
        )
        .is_err()
    );

    // Stamp settings: kept in the config folder and applied to placed stamps.
    let v = call(
        &mut a,
        "stamp_settings",
        json!({ "action": "set", "default_stamp": "Reviewed", "opacity": 0.6, "blend": "multiply", "lock": true }),
    );
    assert_eq!(v["default_stamp"], "Reviewed");
    let v = call(
        &mut a,
        "stamp_settings",
        json!({ "action": "place", "page": 1, "at": [200, 200] }),
    );
    let sid = v["id"].as_str().unwrap().to_string();
    let v = call(&mut a, "markup_list", json!({}));
    let m = v["markups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == sid.as_str())
        .unwrap()
        .clone();
    assert_eq!(m["locked"], true);
    assert!((m["opacity"].as_f64().unwrap() - 0.6).abs() < 1e-6);
    let v = call(&mut a, "stamp_list", json!({}));
    assert!(v["stamps"].as_array().unwrap().len() >= 16, "{v}");

    // Mark Text for Redaction.
    let v = call(
        &mut a,
        "redact_mark_text",
        json!({ "page": 1, "rect": [60, 690, 140, 720] }),
    );
    assert!(v["words"].as_u64().unwrap() >= 1);
    assert_eq!(v["marks"], 1);
}
