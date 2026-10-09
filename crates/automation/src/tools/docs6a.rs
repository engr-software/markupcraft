//! Document tools of wave 6A: PDF Packages, bookmarks from the source structure, Insert Pages
//! options and layered pages, stitching, Batch Sign & Seal, file manager integration, Smart
//! Overlay, document JavaScript and the console, the Web Tab (link pages and page capture),
//! scanners (eSCL) and cameras, interactive stamps, stamp settings and Mark Text for Redaction.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use markupcraft_engine::Rect;
use markupcraft_engine::batch_sign::{BatchSign, DateText, batch_sign};
use markupcraft_engine::devices::{Camera, EsclScanner, ScanSettings, TestCamera, pdf_from_images};
use markupcraft_engine::extras6::StampSettings;
use markupcraft_engine::insert_more::InsertOptions;
use markupcraft_engine::package::create_pdf_package;
use markupcraft_engine::redact::MarkStyle;
use markupcraft_engine::shell_integration::{TargetOs, shell_combine, shell_convert, write_integration};
use markupcraft_engine::smart_overlay::{smart_overlay, smart_report_csv};
use markupcraft_engine::stamps::{StampPlace, StampSource};
use markupcraft_engine::stitch::StitchLayout;
use markupcraft_engine::webtab::{capture_web_page, find_browser, link_page_pdf, normalize_url};
use serde_json::{Value, json};

use super::batch_compare::{job_from, job_props};
use super::{Tool, page_arg, pages_arg, path_arg, point_arg, rect_arg, schema, schema_nodoc};
use crate::{Args, Automation, Result, bad_args, failed};

/// Largest digital ID file read.
const MAX_P12: u64 = 1 << 20;

fn files_arg(a: &Automation, args: &Args, key: &str) -> Result<Vec<PathBuf>> {
    let f = args.opt_strings(key)?.unwrap_or_default();
    f.iter().map(|p| a.resolve(p, false)).collect()
}

fn strings() -> Value {
    json!({ "type": "array", "items": { "type": "string" } })
}

fn str_map(args: &Args, key: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    match args.get(key) {
        None | Some(Value::Null) => {}
        Some(Value::Object(o)) => {
            for (k, v) in o {
                let v = match v {
                    Value::String(s) => s.clone(),
                    Value::Bool(true) => "on".into(),
                    Value::Bool(false) => String::new(),
                    Value::Number(n) => n.to_string(),
                    _ => return Err(bad_args(format!("{key}.{k}: a string"))),
                };
                out.insert(k.clone(), v);
            }
        }
        Some(_) => return Err(bad_args(format!("{key} is an object of label: value"))),
    }
    Ok(out)
}

fn place_arg(args: &Args) -> Result<StampPlace> {
    match (args.opt_rect("rect")?, args.opt_point("at")?) {
        (Some(r), _) => Ok(StampPlace::Rect(r)),
        (None, Some(p)) => Ok(StampPlace::Center(p)),
        (None, None) => Err(bad_args(format!("{}: give rect or at", args.tool()))),
    }
}

pub static PACKAGE: Tool = Tool {
    name: "pdf_package",
    title: "PDF Package",
    description: "File > Create > PDF Package (ISO 32000 portfolio): action create writes a package at `out` holding `files` (none: an empty package with a cover page) and returns how many; make turns the open document into a package (its attachments are the package's files; undoable); info says whether the open document is a package and lists its files.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "action": { "type": "string", "enum": ["create", "make", "info"] },
                "files": strings(),
                "out": path_arg("The package to write")
            }),
            &["action"],
        )
    },
    run: |a, args| match args.str("action")? {
        "create" => {
            let files = files_arg(a, args, "files")?;
            let out = a.resolve(args.str("out")?, true)?;
            let n = create_pdf_package(&files, &out)?;
            Ok(json!({ "files": n, "path": out.display().to_string() }))
        }
        "make" => {
            let (_, s) = a.session(args)?;
            s.make_package()?;
            Ok(json!({ "package": s.is_package(), "files": s.package_files() }))
        }
        "info" => {
            let (_, s) = a.session_ref(args)?;
            Ok(json!({ "package": s.is_package(), "files": s.package_files() }))
        }
        o => Err(bad_args(format!("action {o:?}"))),
    },
};

pub static BOOKMARKS_FROM_SOURCE: Tool = Tool {
    name: "bookmarks_from_source",
    title: "Bookmarks from the source structure",
    description: "Make bookmarks the way the Office and CAD plugins do: from a tagged PDF's heading elements (H, H1-H6, Title), else from text set larger than the body text (three levels; a slide's or sheet's title is its largest line). replace clears the bookmarks first. preview lists the headings without changing anything. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "replace": { "type": "boolean" }, "preview": { "type": "boolean" } }),
            &[],
        )
    },
    run: |a, args| {
        let (_, s) = a.session(args)?;
        if args.bool_or("preview", false)? {
            let (h, kind) = s.source_headings()?;
            return Ok(json!({
                "source": kind.name(),
                "headings": h.iter().map(|(l, t, p)| json!({ "level": l, "title": t, "page": p + 1 })).collect::<Vec<_>>()
            }));
        }
        let (n, kind) = s.bookmarks_from_source(args.bool_or("replace", false)?)?;
        Ok(json!({ "bookmarks": n, "source": kind.name() }))
    },
};

pub static INSERT_WITH: Tool = Tool {
    name: "pages_insert_with",
    title: "Insert Pages with options",
    description: "Insert Pages with the dialog's options: pages of the PDF at `path` (all, or `pages`) before 1-based `at` (default: the end); bookmarks (under one named after the file), attachments, properties (fill empty document properties from the file), layers (keep its optional content groups in the layer list), labels_from_name (label the new pages from the file name), interleave (rejoin odd and even scans: page 1, source 1, page 2, ...) with reverse (take the source last to first). One undo step.",
    read_only: false,
    destructive: false,
    schema: || {
        let b = || json!({ "type": "boolean" });
        schema(
            json!({
                "path": path_arg("The PDF to insert from"),
                "pages": pages_arg("of that PDF"),
                "at": page_arg("position the first new page takes"),
                "bookmarks": b(), "attachments": b(), "properties": b(), "layers": b(),
                "labels_from_name": b(), "interleave": b(), "reverse": b()
            }),
            &["path"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let o = InsertOptions {
            bookmarks: args.bool_or("bookmarks", false)?,
            attachments: args.bool_or("attachments", false)?,
            properties: args.bool_or("properties", false)?,
            layers: args.bool_or("layers", false)?,
            labels_from_name: args.bool_or("labels_from_name", false)?,
            interleave: args.bool_or("interleave", false)?,
            reverse: args.bool_or("reverse", false)?,
        };
        let src_count = markupcraft_engine::Session::open(&path)?.page_count();
        let pages = args.opt_pages("pages", src_count)?;
        let (_, s) = a.session(args)?;
        let at = args.opt_page("at")?.unwrap_or(s.page_count());
        let r = s.insert_pages_with(at, &path, pages.as_deref(), &o)?;
        Ok(json!({ "pages_before": r.pages_before, "pages_after": r.pages_after }))
    },
};

pub static INSERT_LAYERED: Tool = Tool {
    name: "pages_insert_layered",
    title: "Insert Layered Pages",
    description: "Document > Insert > Layered Pages: page k of the PDF at `path` (all, or `pages`) is drawn on page `first` + k of this document as a new layer named `name` (default: the file's name). One undo step.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "path": path_arg("The PDF whose pages become layers"),
                "pages": pages_arg("of that PDF"),
                "first": page_arg("that receives the first layered page (default 1)"),
                "name": { "type": "string" }
            }),
            &["path"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let src_count = markupcraft_engine::Session::open(&path)?.page_count();
        let pages = args.opt_pages("pages", src_count)?;
        let name = args.opt_string("name")?;
        let (_, s) = a.session(args)?;
        let first = args.opt_page("first")?.unwrap_or(0);
        let n = s.insert_layered_pages(&path, pages.as_deref(), first, name.as_deref())?;
        Ok(json!({ "layered": n, "layers": s.layers().len() }))
    },
};

pub static STITCH: Tool = Tool {
    name: "pages_stitch",
    title: "Stitching",
    description: "Document > Stitching: join `pages` of the open document edge to edge into one large page in a new PDF at `out` (vector, nothing rasterized), `columns` pages per row (0 = one row), overlapping neighbours by `overlap` points so match lines coincide. Markups come along, moved with their pages.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to join, in order (left to right, rows top down)"),
                "columns": { "type": "integer", "minimum": 0 },
                "overlap": { "type": "number", "minimum": 0 },
                "out": path_arg("The stitched PDF")
            }),
            &["pages", "out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let (_, s) = a.session_ref(args)?;
        let pages = args.pages("pages", s.page_count())?;
        let layout = StitchLayout {
            columns: args.opt_u64("columns")?.unwrap_or(0).min(1_000) as usize,
            overlap: args.opt_num("overlap")?.unwrap_or(0.0),
        };
        let r = s.stitch_pages(&pages, layout, &out)?;
        Ok(
            json!({ "width": r.width, "height": r.height, "pages": r.pages, "markups": r.markups, "path": out.display().to_string() }),
        )
    },
};

pub static BATCH_SIGN: Tool = Tool {
    name: "batch_sign",
    title: "Batch Sign & Seal",
    description: "Batch > Sign & Seal: for each of `files`, add a date (date_format in stamp time letters, at date_rect), place a seal image (`seal`, PNG/JPEG/PDF page, at seal_rect or the signature box), then digitally sign with the digital ID `p12` (+ password): in the empty signature field named `field` when the file has one, else a new signature on `page` (past the end = the last page) at `rect` (no rect: invisible), or certify (1 no changes, 2 forms, 3 also comments). Signed copies go to out_dir with `suffix`; without out_dir every file is signed in place. A file that fails is reported, not fatal.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "files": strings(),
                "p12": path_arg("The digital ID (.p12/.pfx)"),
                "password": { "type": "string" },
                "field": { "type": "string" },
                "page": page_arg("for a new signature (default the last)"),
                "rect": rect_arg("The signature box"),
                "seal": path_arg("A seal image"),
                "seal_rect": rect_arg("The seal's box"),
                "date_format": { "type": "string" },
                "date_rect": rect_arg("Where the date goes"),
                "certify": { "type": "integer", "minimum": 1, "maximum": 3 },
                "reason": { "type": "string" },
                "location": { "type": "string" },
                "out_dir": path_arg("Folder for the signed copies"),
                "suffix": { "type": "string" }
            }),
            &["files", "p12", "password"],
        )
    },
    run: |a, args| {
        let files = files_arg(a, args, "files")?;
        let p12 = a.resolve(args.str("p12")?, false)?;
        let len = std::fs::metadata(&p12).map_err(failed)?.len();
        if len > MAX_P12 {
            return Err(bad_args("the digital ID file is too large"));
        }
        let date = match (args.opt_string("date_format")?, args.opt_rect("date_rect")?) {
            (Some(format), Some(rect)) => Some(DateText { format, rect }),
            (None, None) => None,
            _ => return Err(bad_args("date_format and date_rect go together")),
        };
        let job = BatchSign {
            p12: std::fs::read(&p12).map_err(failed)?,
            password: args.str("password")?.to_string(),
            field: args.opt_string("field")?,
            page: args.opt_page("page")?.unwrap_or(usize::MAX),
            rect: args.opt_rect("rect")?,
            seal: args.opt_str("seal")?.map(|s| a.resolve(s, false)).transpose()?,
            seal_rect: args.opt_rect("seal_rect")?,
            date,
            certify: args.opt_u64("certify")?.map(|c| c.min(3) as u8),
            reason: args.opt_string("reason")?,
            location: args.opt_string("location")?,
            out_dir: args.opt_str("out_dir")?.map(|s| a.resolve(s, true)).transpose()?,
            suffix: args.opt_string("suffix")?.unwrap_or_default(),
        };
        if let Some(d) = &job.out_dir {
            std::fs::create_dir_all(d).map_err(crate::failed)?;
        }
        let r = batch_sign(&files, &job)?;
        let ok = r.iter().filter(|o| o.error.is_empty()).count();
        Ok(json!({ "signed": ok, "failed": r.len() - ok, "files": r }))
    },
};

pub static SHELL_INTEGRATION: Tool = Tool {
    name: "shell_integration",
    title: "File manager integration",
    description: "Explorer / Finder / Linux file manager entries for Combine in MarkupCraft and Convert to PDF with MarkupCraft. action write: write the files a user installs themselves into the folder `dir` (a current-user .reg file and Send To commands on Windows, Quick Actions on macOS, a KDE service menu and Nautilus scripts on Linux, each with a README; nothing outside `dir` changes) for `os` (windows, macos, linux; default this one) running the CLI at `cli`. combine / convert run what those entries run on `files`.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "action": { "type": "string", "enum": ["write", "combine", "convert"] },
                "dir": path_arg("Folder for the integration files"),
                "cli": { "type": "string", "description": "The path of markupcraft-cli the entries run." },
                "os": { "type": "string", "enum": ["windows", "macos", "linux"] },
                "files": strings()
            }),
            &["action"],
        )
    },
    run: |a, args| match args.str("action")? {
        "write" => {
            let dir = a.resolve(args.str("dir")?, true)?;
            let os = match args.opt_str("os")? {
                Some(o) => TargetOs::from_name(o).ok_or_else(|| bad_args(format!("os {o:?}")))?,
                None => TargetOs::current(),
            };
            let cli = PathBuf::from(args.str("cli")?);
            let files = write_integration(&dir, &cli, os)?;
            Ok(json!({ "files": files.iter().map(|f| f.display().to_string()).collect::<Vec<_>>() }))
        }
        "combine" => {
            let out = shell_combine(&files_arg(a, args, "files")?)?;
            Ok(json!({ "path": out.display().to_string() }))
        }
        "convert" => {
            let out = shell_convert(&files_arg(a, args, "files")?)?;
            Ok(json!({ "files": out.iter().map(|f| f.display().to_string()).collect::<Vec<_>>() }))
        }
        o => Err(bad_args(format!("action {o:?}"))),
    },
};

pub static SMART_OVERLAY: Tool = Tool {
    name: "smart_overlay",
    title: "Smart Overlay",
    description: "Smart Overlay: whole sets overlaid sheet by sheet (pair them as batch_match does), each sheet registered by matching its drawing rather than its page box, written to out_dir as `<sheet> overlay.pdf`, with a match score per sheet (1 = the same drawing) and per discipline (from the sheet number's letters). advanced_shading keeps grey tones. report writes the scores as CSV.",
    read_only: false,
    destructive: true,
    schema: || {
        let mut p = job_props();
        if let Some(o) = p.as_object_mut() {
            o.insert("out_dir".into(), path_arg("Folder for the overlays"));
            o.insert("advanced_shading".into(), json!({ "type": "boolean" }));
            o.insert("report".into(), path_arg("A CSV report to write"));
        }
        schema_nodoc(p, &["out_dir"])
    },
    run: |a, args| {
        let job = job_from(a, args)?;
        let out_dir = a.resolve(args.str("out_dir")?, true)?;
        let r = smart_overlay(&job, &out_dir, args.bool_or("advanced_shading", false)?)?;
        if let Some(p) = args.opt_str("report")? {
            let p = a.resolve(p, true)?;
            std::fs::write(&p, smart_report_csv(&r)).map_err(failed)?;
        }
        Ok(json!({
            "sheets": r.sheets.iter().map(|s| json!({
                "sheet": s.sheet, "discipline": s.discipline, "score": s.score,
                "output": s.output.display().to_string(), "error": s.error
            })).collect::<Vec<_>>(),
            "disciplines": r.disciplines.iter().map(|(d, n, avg)| json!({ "discipline": d, "sheets": n, "score": avg })).collect::<Vec<_>>(),
            "unmatched_current": r.unmatched_current.len(),
            "unmatched_revised": r.unmatched_revised.len(),
        }))
    },
};

pub static JAVASCRIPT: Tool = Tool {
    name: "javascript",
    title: "JavaScript",
    description: "Document JavaScript in a sandbox (no file, network or timer access; loops bounded): action run runs `script` as the JavaScript Console does (the document's scripts defined, `page` as this.pageNum; this.getField, app, util, console); list shows the document-level scripts; document runs them as on opening. Field changes are applied to the form as one undo step. Returns the result, console output, alerts, changed fields and what the script asked the viewer to do (go to a page, open URLs, print).",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "action": { "type": "string", "enum": ["run", "list", "document"] },
                "script": { "type": "string" },
                "page": page_arg("this.pageNum (default 1)")
            }),
            &["action"],
        )
    },
    run: |a, args| {
        let (_, s) = a.session(args)?;
        let out = match args.str("action")? {
            "list" => {
                return Ok(json!({
                    "scripts": s.document_scripts().into_iter().map(|(n, src)| json!({ "name": n, "source": src })).collect::<Vec<_>>()
                }));
            }
            "run" => {
                let page = args.opt_page("page")?.unwrap_or(0);
                s.run_javascript(args.str("script")?, page)?
            }
            "document" => s.run_document_scripts()?,
            o => return Err(bad_args(format!("action {o:?}"))),
        };
        Ok(json!({
            "result": out.result, "console": out.console, "alerts": out.alerts, "error": out.error,
            "changed": out.changed, "go_to_page": out.go_to_page.map(|p| p + 1), "urls": out.urls, "print": out.print
        }))
    },
};

pub static WEBTAB: Tool = Tool {
    name: "webtab",
    title: "Web Tab",
    description: "The Web Tab without an embedded browser. action links writes a link page PDF at `out` (title + each {name, url} as a line that opens it in the browser); capture prints the web page at `url` to the PDF `out` with a Chromium-family browser run headless (`browser`, else one found on PATH or in its usual folder; a throwaway profile, `timeout` seconds) so it can be opened and marked up; browser reports the browser that would be used.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "action": { "type": "string", "enum": ["links", "capture", "browser"] },
                "title": { "type": "string" },
                "links": { "type": "array", "items": { "type": "object", "properties": { "name": { "type": "string" }, "url": { "type": "string" } }, "required": ["url"] } },
                "url": { "type": "string" },
                "browser": { "type": "string", "description": "The browser program to run." },
                "timeout": { "type": "integer", "minimum": 5, "maximum": 600 },
                "out": path_arg("The PDF to write")
            }),
            &["action"],
        )
    },
    run: |a, args| {
        let explicit = args.opt_string("browser")?.map(PathBuf::from);
        match args.str("action")? {
            "links" => {
                let out = a.resolve(args.str("out")?, true)?;
                let mut links = Vec::new();
                for l in args.get("links").and_then(Value::as_array).cloned().unwrap_or_default() {
                    let url = l
                        .get("url")
                        .and_then(Value::as_str)
                        .ok_or_else(|| bad_args("each link has a url"))?;
                    let name = l.get("name").and_then(Value::as_str).unwrap_or_default();
                    links.push((name.to_string(), normalize_url(url)?));
                }
                let title = args.opt_string("title")?.unwrap_or_else(|| "Web Tab".into());
                let mut s = link_page_pdf(&title, &links, &out)?;
                s.save_as(&out, true)?;
                Ok(json!({ "links": links.len(), "pages": s.page_count(), "path": out.display().to_string() }))
            }
            "capture" => {
                let out = a.resolve(args.str("out")?, true)?;
                let url = normalize_url(args.str("url")?)?;
                let browser = find_browser(explicit.as_deref()).ok_or_else(|| {
                    failed("no Chromium-family browser (Edge, Chrome, Chromium) was found to capture with")
                })?;
                let secs = args.opt_u64("timeout")?.unwrap_or(60).clamp(5, 600);
                let n = capture_web_page(&url, &out, &browser, Duration::from_secs(secs))?;
                Ok(json!({ "pages": n, "path": out.display().to_string(), "url": url }))
            }
            "browser" => Ok(json!({ "browser": find_browser(explicit.as_deref()).map(|p| p.display().to_string()) })),
            o => Err(bad_args(format!("action {o:?}"))),
        }
    },
};

fn acquired_into(a: &mut Automation, args: &Args, images: Vec<Vec<u8>>) -> Result<Value> {
    match args.opt_str("into")?.unwrap_or("pages") {
        "pages" => {
            let (_, s) = a.session(args)?;
            let at = args.opt_page("at")?.unwrap_or(s.page_count());
            let n = s.insert_acquired_pages(at, &images)?;
            Ok(json!({ "inserted": n, "pages": s.page_count() }))
        }
        "pdf" => {
            let out = a.resolve(args.str("out")?, true)?;
            let n = pdf_from_images(&images, &out)?;
            Ok(json!({ "pages": n, "path": out.display().to_string() }))
        }
        "image" => {
            let (_, s) = a.session(args)?;
            let page = args.opt_page("page")?.unwrap_or(0);
            let place = match (args.opt_rect("rect")?, args.opt_point("point")?) {
                (Some(r), _) => StampPlace::Rect(r),
                (None, Some(p)) => StampPlace::Center(p),
                (None, None) => StampPlace::Center(markupcraft_engine::devices::page_centre(s, page)?),
            };
            let first = images.first().ok_or_else(|| failed("nothing was acquired"))?;
            let id = s.add_acquired_image(page, place, first)?;
            Ok(json!({ "id": id }))
        }
        o => Err(bad_args(format!("into {o:?}"))),
    }
}

fn into_props(mut extra: Value) -> Value {
    if let Some(o) = extra.as_object_mut() {
        o.insert(
            "into".into(),
            json!({ "type": "string", "enum": ["pages", "pdf", "image"], "description": "pages: insert into the open document (before `at`); pdf: a new PDF at `out`; image: an image markup on `page` (at `point` or in `rect`)." }),
        );
        o.insert("at".into(), page_arg("position for inserted pages (default the end)"));
        o.insert("out".into(), path_arg("The new PDF"));
        o.insert("page".into(), page_arg("for an image markup"));
        o.insert("point".into(), point_arg("The image's centre"));
        o.insert("rect".into(), rect_arg("The image's box"));
    }
    extra
}

pub static SCAN: Tool = Tool {
    name: "scan",
    title: "Scanner (eSCL / AirScan)",
    description: "A network scanner over eSCL (AirScan, Mopria: plain HTTP), at its eSCL root `url` (http://host/eSCL). action capabilities reads what it can do; scan runs a job (source Platen or Feeder, resolution dpi, color_mode RGB24 / Grayscale8 / BlackAndWhite1, format image/jpeg or application/pdf) and puts the pages `into` the open document, a new PDF or an image markup (Image From Scanner).",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            into_props(json!({
                "action": { "type": "string", "enum": ["capabilities", "scan"] },
                "url": { "type": "string" },
                "source": { "type": "string" },
                "resolution": { "type": "integer" },
                "color_mode": { "type": "string" },
                "format": { "type": "string" }
            })),
            &["action", "url"],
        )
    },
    run: |a, args| {
        let sc = EsclScanner::new(args.str("url")?)?;
        match args.str("action")? {
            "capabilities" => Ok(serde_json::to_value(sc.capabilities()?).map_err(failed)?),
            "scan" => {
                let d = ScanSettings::default();
                let st = ScanSettings {
                    source: args.opt_string("source")?.unwrap_or(d.source),
                    resolution: args
                        .opt_u64("resolution")?
                        .map_or(d.resolution, |r| r.min(100_000) as u32),
                    color_mode: args.opt_string("color_mode")?.unwrap_or(d.color_mode),
                    format: args.opt_string("format")?.unwrap_or(d.format),
                };
                let pages = sc.scan(&st)?;
                acquired_into(a, args, pages)
            }
            o => Err(bad_args(format!("action {o:?}"))),
        }
    },
};

pub static CAMERA: Tool = Tool {
    name: "camera_capture",
    title: "Camera",
    description: "Take a picture and put it `into` the open document (a page), a new PDF or an image markup (Markup > Camera). Headless runs have only the built-in test-pattern camera (device \"test\", `width` x `height`); the desktop app uses the system camera when built with its camera feature.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            into_props(json!({
                "device": { "type": "string", "enum": ["test"] },
                "width": { "type": "integer", "minimum": 1, "maximum": 4096 },
                "height": { "type": "integer", "minimum": 1, "maximum": 4096 }
            })),
            &[],
        )
    },
    run: |a, args| {
        let mut cam = TestCamera {
            width: args.opt_u64("width")?.unwrap_or(640).min(4096) as u32,
            height: args.opt_u64("height")?.unwrap_or(480).min(4096) as u32,
        };
        let png = cam.capture(0)?.png()?;
        acquired_into(a, args, vec![png])
    },
};

pub static STAMP_INTERACTIVE: Tool = Tool {
    name: "stamp_interactive",
    title: "Interactive stamps",
    description: "Stamps with fields a reviewer fills in: a template's {check:Label} / {check:Label=on}, {field:Label} / {field:Label=Default} and {choice:Label=A|B|C} (plus the dynamic fields of stamp_add). action place puts one on `page` at `at` or in `rect` (text, color, values for the fields, answers for {prompt:} fields); fields lists a placed stamp's fields and values; set changes them (check boxes take \"on\" or \"\") and redraws it. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "action": { "type": "string", "enum": ["place", "fields", "set"] },
                "page": page_arg("for the stamp"),
                "at": point_arg("The stamp's centre"),
                "rect": rect_arg("The stamp's box"),
                "text": { "type": "string" },
                "color": { "type": ["string", "array"] },
                "values": { "type": "object" },
                "answers": { "type": "object" },
                "id": { "type": "string" }
            }),
            &["action"],
        )
    },
    run: |a, args| {
        let values = str_map(args, "values")?;
        let (_, s) = a.session(args)?;
        let fields_json = |f: &[markupcraft_engine::stamp_fields::StampField]| serde_json::to_value(f).map_err(failed);
        match args.str("action")? {
            "place" => {
                let page = args.opt_page("page")?.unwrap_or(0);
                let color = args
                    .opt_color("color")?
                    .unwrap_or(markupcraft_engine::Color::rgb(0.05, 0.3, 0.8));
                let answers = str_map(args, "answers")?;
                let id =
                    s.place_interactive_stamp(page, place_arg(args)?, args.str("text")?, color, &answers, &values)?;
                let f = s.stamp_fields(&id)?;
                Ok(json!({ "id": id, "fields": fields_json(&f)? }))
            }
            "fields" => {
                let f = s.stamp_fields(args.str("id")?)?;
                Ok(json!({ "fields": fields_json(&f)? }))
            }
            "set" => {
                let id = args.str("id")?.to_string();
                s.set_stamp_fields(&id, &values)?;
                let f = s.stamp_fields(&id)?;
                Ok(json!({ "fields": fields_json(&f)? }))
            }
            o => Err(bad_args(format!("action {o:?}"))),
        }
    },
};

pub static STAMP_SETTINGS: Tool = Tool {
    name: "stamp_settings",
    title: "Stamp settings",
    description: "Tools > Stamp settings kept in the configuration folder: the stamp folder (\"\" = the default library; a shared folder serves a team), the default stamp, and the opacity (0.05 to 1), blend mode (normal or multiply) and lock every placed stamp gets. action get / set (any of the fields) / place (the default stamp, or `stamp` a library id, on `page` at `at` or in `rect`, with these settings; undoable).",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "action": { "type": "string", "enum": ["get", "set", "place"] },
                "folder": { "type": "string" },
                "default_stamp": { "type": "string" },
                "opacity": { "type": "number" },
                "blend": { "type": "string", "enum": ["normal", "multiply"] },
                "lock": { "type": "boolean" },
                "stamp": { "type": "string" },
                "page": page_arg("for the stamp"),
                "at": point_arg("The stamp's centre"),
                "rect": rect_arg("The stamp's box")
            }),
            &["action"],
        )
    },
    run: |a, args| {
        let config = a.config_dir()?;
        let mut st = StampSettings::load(&config)?;
        let json_of = |st: &StampSettings| serde_json::to_value(st).map_err(failed);
        match args.str("action")? {
            "get" => json_of(&st),
            "set" => {
                if let Some(v) = args.opt_string("folder")? {
                    st.folder = v;
                }
                if let Some(v) = args.opt_string("default_stamp")? {
                    st.default_stamp = v;
                }
                if let Some(v) = args.opt_num("opacity")? {
                    st.opacity = v;
                }
                if let Some(v) = args.opt_string("blend")? {
                    st.blend = v;
                }
                if let Some(v) = args.opt_bool("lock")? {
                    st.lock = v;
                }
                st.save(&config)?;
                json_of(&st)
            }
            "place" => {
                let lib = st.library(&config);
                let src = args.opt_string("stamp")?.map(StampSource::Library);
                let place = place_arg(args)?;
                let (_, s) = a.session(args)?;
                let page = args.opt_page("page")?.unwrap_or(0);
                let id = s.place_stamp_with_settings(page, place, src.as_ref(), Some(&lib), &st)?;
                Ok(json!({ "id": id }))
            }
            o => Err(bad_args(format!("action {o:?}"))),
        }
    },
};

pub static REDACT_TEXT: Tool = Tool {
    name: "redact_mark_text",
    title: "Mark Text for Redaction",
    description: "Mark Text for Redaction (Shift+K): every word `rect` touches on `page` is marked for redaction as one mark (apply with redact_apply). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "page": page_arg("to mark"), "rect": rect_arg("The box over the text") }),
            &["page", "rect"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let rect: Rect = args.opt_rect("rect")?.ok_or_else(|| bad_args("rect is required"))?;
        let (_, s) = a.session(args)?;
        let n = s.redact_mark_text(page, rect, &MarkStyle::default())?;
        Ok(json!({ "words": n, "marks": s.redact_marks().len() }))
    },
};
