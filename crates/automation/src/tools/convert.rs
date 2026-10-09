//! File > Export (images, text, HTML, RTF, Word, Excel, PowerPoint, page region to Excel) and
//! Document > Repair PDF, Archive as PDF/A, Color Processing.

use markupcraft_engine::archive::{ColorMode, PdfaLevel};
use markupcraft_engine::convert::{ImageExport, ImageFormat, OfficeFormat};
use serde_json::{Value, json};

use super::{Tool, page_arg, pages_arg, path_arg, rect_arg, schema};
use crate::{bad_args, summary};

pub static EXPORT_IMAGES: Tool = Tool {
    name: "export_images",
    title: "Export pages as images",
    description: "Save pages (default all) as image files in the folder `dir`: <name><suffix><page>.<ext>, page numbers zero-padded. format png (default), jpg (quality 1-100), tif or bmp; dpi 18-1200 (default 150). markups: false leaves markups out. Returns the files.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "dir": path_arg("The folder for the images"),
                "name": { "type": "string", "description": "File name stem (default: the document's)." },
                "suffix": { "type": "string", "description": "Between the name and the page number (default \"_\")." },
                "format": { "type": "string", "enum": ["png", "jpg", "jpeg", "tif", "tiff", "bmp"] },
                "quality": { "type": "integer", "minimum": 1, "maximum": 100 },
                "dpi": { "type": "number" },
                "pages": pages_arg("to export (default all)"),
                "markups": { "type": "boolean" }
            }),
            &["dir"],
        )
    },
    run: |a, args| {
        let dir = a.resolve(args.str("dir")?, true)?;
        let (doc, s) = a.session_ref(args)?;
        let mut o = ImageExport::default();
        if let Some(f) = args.opt_str("format")? {
            o.format = ImageFormat::from_name(f).ok_or_else(|| bad_args(format!("unknown format {f:?}")))?;
        }
        if let Some(q) = args.opt_u64("quality")?
            && let ImageFormat::Jpeg(_) = o.format
        {
            o.format = ImageFormat::Jpeg(q.clamp(1, 100) as u8);
        }
        if let Some(d) = args.opt_num("dpi")? {
            o.dpi = d;
        }
        if let Some(sfx) = args.opt_string("suffix")? {
            o.suffix = sfx;
        }
        o.pages = args.opt_pages("pages", s.page_count())?;
        o.hide_markups = !args.bool_or("markups", true)?;
        let stem = match args.opt_string("name")? {
            Some(n) => n,
            None => s
                .path()
                .file_stem()
                .map(|x| x.to_string_lossy().into_owned())
                .unwrap_or_else(|| "page".into()),
        };
        let files = s.export_images(&dir, &stem, &o)?;
        Ok(json!({ "doc": doc, "files": files.iter().map(|f| f.display().to_string()).collect::<Vec<_>>() }))
    },
};

pub static EXPORT_DOCUMENT: Tool = Tool {
    name: "export_document",
    title: "Export the document",
    description: "Export the document (or `pages`) to `out` as plain text (.txt), HTML (.html), RTF (.rtf), Word (.docx), Excel (.xlsx: one sheet per page, words laid out in rows and columns by position, numbers as numbers) or PowerPoint (.pptx: one slide per page as a picture). `format` overrides the extension. The document is not changed.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "out": path_arg("The file to write"),
                "format": { "type": "string", "enum": ["txt", "html", "rtf", "docx", "xlsx", "pptx"] },
                "pages": pages_arg("to export (default all)")
            }),
            &["out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let format = match args.opt_str("format")? {
            Some(f) => Some(OfficeFormat::from_name(f).ok_or_else(|| bad_args(format!("unknown format {f:?}")))?),
            None => None,
        };
        let (doc, s) = a.session_ref(args)?;
        let pages = args.opt_pages("pages", s.page_count())?;
        let bytes = s.export_document(&out, format, pages)?;
        Ok(json!({ "doc": doc, "out": out.display().to_string(), "bytes": bytes }))
    },
};

pub static EXPORT_REGION: Tool = Tool {
    name: "export_region",
    title: "Export a page region to Excel",
    description: "Read the text inside `rect` on `page` (an equipment schedule, a table) as rows and columns by position and save it to `out` (.xlsx or .csv). Returns the rows.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "page": page_arg("with the table"),
                "rect": rect_arg("The region"),
                "out": path_arg("The .xlsx or .csv file")
            }),
            &["page", "rect", "out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let page = args.page("page")?;
        let rect = args.opt_rect("rect")?.ok_or_else(|| bad_args("rect is required"))?;
        let (doc, s) = a.session_ref(args)?;
        let t = s.export_region(page, rect, &out)?;
        Ok(json!({ "doc": doc, "out": out.display().to_string(), "rows": t.rows, "columns": t.columns() }))
    },
};

pub static REPAIR: Tool = Tool {
    name: "doc_repair",
    title: "Repair PDF",
    description: "Document > Repair PDF: rebuild the file from its objects (the next save is a full rewrite): broken annotation and content entries are dropped, the cross-reference table rebuilt. Reports what was fixed. Undoable until saved.",
    read_only: false,
    destructive: true,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let r = s.repair_pdf()?;
        Ok(json!({
            "read_fixes": r.read_fixes,
            "bad_annotations": r.bad_annotations,
            "bad_contents": r.bad_contents,
            "bytes_before": r.bytes_before,
            "bytes_after": r.bytes_after,
            "document": summary(doc, s),
        }))
    },
};

fn level(args: &crate::Args) -> crate::Result<PdfaLevel> {
    match args.opt_str("level")?.unwrap_or("2b") {
        "2b" | "a2b" | "A-2b" => Ok(PdfaLevel::A2b),
        "3b" | "a3b" | "A-3b" => Ok(PdfaLevel::A3b),
        l => Err(bad_args(format!("level {l:?}: use 2b or 3b"))),
    }
}

fn issues(v: &[markupcraft_engine::archive::PdfaIssue]) -> Vec<Value> {
    v.iter()
        .map(|i| json!({ "clause": i.clause, "message": i.message, "page": i.page.map(|p| p + 1), "fixable": i.fixable }))
        .collect()
}

pub static PDFA: Tool = Tool {
    name: "doc_pdfa",
    title: "Archive as PDF/A",
    description: "action archive (default): convert the document to PDF/A (level 2b or 3b) as far as possible without changing how pages look (XMP identification, sRGB output intent, forbidden actions, annotation flags) and report what is left; verify: only check; unlock: remove the PDF/A identification so the file can be edited as an ordinary PDF. Archive and unlock are undoable until saved.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "action": { "type": "string", "enum": ["archive", "verify", "unlock"] },
                "level": { "type": "string", "enum": ["2b", "3b"] }
            }),
            &[],
        )
    },
    run: |a, args| {
        let lv = level(args)?;
        let (doc, s) = a.session(args)?;
        match args.opt_str("action")?.unwrap_or("archive") {
            "archive" => {
                let r = s.archive_pdfa(lv)?;
                Ok(
                    json!({ "fixed": r.fixed, "remaining": issues(&r.remaining), "conforming": r.remaining.is_empty(), "document": summary(doc, s) }),
                )
            }
            "verify" => {
                let v = s.pdfa_verify(lv);
                Ok(
                    json!({ "declared": s.pdfa_declared().map(|d| format!("PDF/A-{}{}", d.0, d.1.to_lowercase())), "issues": issues(&v), "conforming": v.is_empty() }),
                )
            }
            "unlock" => {
                let had = s.pdfa_declared().is_some();
                s.unlock_pdfa()?;
                Ok(json!({ "unlocked": had, "document": summary(doc, s) }))
            }
            o => Err(bad_args(format!("action {o:?}: archive, verify or unlock"))),
        }
    },
};

pub static COLOR_PROCESS: Tool = Tool {
    name: "doc_color_process",
    title: "Color Processing",
    description: "Recolour the page content of `pages` (default all; vector and images alike; markups keep their colours): mode grayscale, tint (every colour becomes shades of `color`, white stays white) or lighten (`amount` 0-1 toward white, e.g. to grey out a background set); remove takes it off again. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "mode": { "type": "string", "enum": ["grayscale", "tint", "lighten", "remove"] },
                "color": { "type": ["string", "array"] },
                "amount": { "type": "number", "minimum": 0, "maximum": 1 },
                "pages": pages_arg("to recolour (default all)")
            }),
            &["mode"],
        )
    },
    run: |a, args| {
        let mode = args.str("mode")?;
        let color = args.opt_color("color")?;
        let amount = args.opt_num("amount")?;
        let (doc, s) = a.session(args)?;
        let pages = args.opt_pages("pages", s.page_count())?.unwrap_or_default();
        let n = match mode {
            "grayscale" => s.color_process(&pages, ColorMode::Grayscale)?,
            "tint" => s.color_process(
                &pages,
                ColorMode::Tint(color.ok_or_else(|| bad_args("tint needs color"))?),
            )?,
            "lighten" => s.color_process(&pages, ColorMode::Lighten(amount.unwrap_or(0.6)))?,
            "remove" => s.color_process_remove(&pages)?,
            m => return Err(bad_args(format!("mode {m:?}"))),
        };
        Ok(json!({ "pages": n, "document": summary(doc, s) }))
    },
};

/// Standard base64 (RFC 4648, with padding).
pub(crate) fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [
            c.first().copied().unwrap_or(0),
            c.get(1).copied().unwrap_or(0),
            c.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].iter().enumerate() {
            if i <= c.len() {
                out.push(char::from(T[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}
