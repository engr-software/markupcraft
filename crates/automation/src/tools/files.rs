//! Whole-file operations: reduce file size, print-ready PDF, split, combine, replace pages,
//! page boxes (crop) and page size.

use markupcraft_engine::boxes::{Anchor, BoxChange, PageBox};
use markupcraft_engine::combine::{SplitBy, combine_files};
use markupcraft_engine::printing::{PrintJob, send_to_printer};
use markupcraft_engine::printout::{PrintLayout, PrintSettings, paper_size};
use markupcraft_engine::reduce::ReduceSettings;
use serde_json::{Value, json};

use super::{Tool, pages_arg, path_arg, rect_arg, schema, schema_nodoc};
use crate::{bad_args, summary};

pub static REDUCE: Tool = Tool {
    name: "doc_reduce_size",
    title: "Reduce file size",
    description: "Shrink the document: images drawn above `above_ppi` (default 225) are resampled to `target_ppi` (default 150) and recompressed (JPEG `quality` 1-100, default 60; quality 0 = lossless), thumbnails are dropped, unencoded streams compressed. Reports full-save sizes before and after. Takes effect on the next save (a full rewrite). Undoable until then.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "downsample": { "type": "boolean" },
                "target_ppi": { "type": "number" },
                "above_ppi": { "type": "number" },
                "quality": { "type": "integer", "minimum": 0, "maximum": 100 },
                "gray_target_ppi": { "type": "number", "description": "Gray images: their own target ppi (default: as colour)." },
                "gray_above_ppi": { "type": "number" },
                "gray_quality": { "type": "integer", "minimum": 0, "maximum": 100 },
                "discard_thumbnails": { "type": "boolean" },
                "discard_alternate_images": { "type": "boolean" },
                "discard_tags": { "type": "boolean" },
                "discard_print_settings": { "type": "boolean" },
                "compress_streams": { "type": "boolean" },
                "remove_invalid_links": { "type": "boolean" },
                "remove_unreferenced_dests": { "type": "boolean" },
                "discard_metadata": { "type": "boolean" },
                "discard_private": { "type": "boolean" },
                "crop_to_crop_box": { "type": "boolean" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let mut r = ReduceSettings::default();
        r.downsample = args.bool_or("downsample", r.downsample)?;
        if let Some(t) = args.opt_num("target_ppi")? {
            r.target_ppi = t;
            r.above_ppi = r.above_ppi.max(t);
        }
        if let Some(t) = args.opt_num("above_ppi")? {
            r.above_ppi = t;
        }
        if let Some(q) = args.opt_u64("quality")? {
            r.jpeg_quality = match q {
                0 => None,
                1..=100 => Some(q as u8),
                _ => return Err(bad_args("quality must be 0 to 100")),
            };
        }
        if let Some(t) = args.opt_num("gray_target_ppi")? {
            let above = args.opt_num("gray_above_ppi")?.unwrap_or(t * 1.5).max(t);
            let q = match args.opt_u64("gray_quality")? {
                None => r.jpeg_quality,
                Some(0) => None,
                Some(q @ 1..=100) => Some(q as u8),
                Some(_) => return Err(bad_args("gray_quality must be 0 to 100")),
            };
            r.gray = Some((t, above, q));
        }
        r.discard_thumbnails = args.bool_or("discard_thumbnails", r.discard_thumbnails)?;
        r.discard_alternate_images = args.bool_or("discard_alternate_images", r.discard_alternate_images)?;
        r.discard_tags = args.bool_or("discard_tags", r.discard_tags)?;
        r.discard_print_settings = args.bool_or("discard_print_settings", r.discard_print_settings)?;
        r.compress_streams = args.bool_or("compress_streams", r.compress_streams)?;
        r.remove_invalid_links = args.bool_or("remove_invalid_links", r.remove_invalid_links)?;
        r.remove_unreferenced_dests = args.bool_or("remove_unreferenced_dests", r.remove_unreferenced_dests)?;
        r.discard_metadata = args.bool_or("discard_metadata", r.discard_metadata)?;
        r.discard_private = args.bool_or("discard_private", r.discard_private)?;
        r.crop_to_crop_box = args.bool_or("crop_to_crop_box", r.crop_to_crop_box)?;
        let (doc, s) = a.session(args)?;
        let rep = s.reduce_file_size(&r)?;
        Ok(json!({
            "bytes_before": rep.bytes_before,
            "bytes_after": rep.bytes_after,
            "images": rep.images,
            "images_resampled": rep.images_resampled,
            "images_recompressed": rep.images_recompressed,
            "streams_compressed": rep.streams_compressed,
            "thumbnails_removed": rep.thumbnails,
            "invalid_links_removed": rep.invalid_links,
            "document": summary(doc, s),
        }))
    },
};

pub static PRINT: Tool = Tool {
    name: "print_pdf",
    title: "Print to a print-ready PDF",
    description: "Lay the pages (default all, in the order given) out on paper and write the sheets as a new PDF at `out`: layout fit (default), actual, shrink, percent (with `percent`), nup (`cols` x `rows` per sheet, optional `border`) or tile (each page at `percent` over several sheets with `overlap` points and `cut_marks`). paper: letter, legal, tabloid, ansi_c..e, arch_a..e, a0..a5, or [width, height] in points. markups: false prints page content only. The document is not changed.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "out": path_arg("The print-ready PDF"),
                "pages": { "type": ["array", "string"], "items": { "type": "integer", "minimum": 1 }, "description": "Pages in print order: [3, 1, 2] or a range like \"1-3\"." },
                "paper": { "type": ["string", "array"] },
                "orientation": { "type": "string", "enum": ["auto", "portrait", "landscape"] },
                "layout": { "type": "string", "enum": ["fit", "actual", "shrink", "percent", "nup", "tile"] },
                "percent": { "type": "number" },
                "cols": { "type": "integer", "minimum": 1 },
                "rows": { "type": "integer", "minimum": 1 },
                "border": { "type": "boolean" },
                "overlap": { "type": "number", "minimum": 0 },
                "cut_marks": { "type": "boolean" },
                "markups": { "type": "boolean" },
                "markups_only": { "type": "boolean", "description": "Print only the markups." },
                "region": { "type": "array", "items": { "type": "number" }, "minItems": 5, "maxItems": 5, "description": "Get Window: [page, x0, y0, x1, y1] prints only that box." },
                "copies": { "type": "integer", "minimum": 1, "maximum": 999 },
                "collate": { "type": "boolean" },
                "reverse": { "type": "boolean" },
                "margin": { "type": "number", "minimum": 0, "description": "Fit/reduce to margins: points on every side." },
                "offset": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2, "description": "Manual position: [dx, dy] from the centre, points." },
                "dim_content": { "type": "boolean" },
                "dim_except": { "type": "array", "items": { "type": "string" }, "description": "Markup ids printed at full strength; others dimmed." },
                "spaces": { "type": "boolean" },
                "links": { "type": "boolean" },
                "printer": { "type": "string", "description": "Also send the sheets to this printer (\"\" = the default printer) with the system's print command." }
            }),
            &["out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let paper = match args.get("paper") {
            None => (612.0, 792.0),
            Some(Value::String(name)) => paper_size(name).ok_or_else(|| bad_args(format!("unknown paper {name:?}")))?,
            Some(Value::Array(v)) => match v.as_slice() {
                [w, h] => (
                    w.as_f64().ok_or_else(|| bad_args("paper width must be a number"))?,
                    h.as_f64().ok_or_else(|| bad_args("paper height must be a number"))?,
                ),
                _ => return Err(bad_args("paper must be a name or [width, height]")),
            },
            Some(_) => return Err(bad_args("paper must be a name or [width, height]")),
        };
        let landscape = match args.opt_str("orientation")?.unwrap_or("auto") {
            "auto" => None,
            "portrait" => Some(false),
            "landscape" => Some(true),
            o => return Err(bad_args(format!("unknown orientation {o:?}"))),
        };
        let percent = args.opt_num("percent")?;
        let layout = match args.opt_str("layout")?.unwrap_or("fit") {
            "fit" => PrintLayout::Fit,
            "actual" => PrintLayout::ActualSize,
            "shrink" => PrintLayout::Shrink,
            "percent" => PrintLayout::Percent(percent.ok_or_else(|| bad_args("layout percent needs `percent`"))?),
            "nup" => PrintLayout::NUp {
                cols: args.opt_u64("cols")?.unwrap_or(2) as usize,
                rows: args.opt_u64("rows")?.unwrap_or(2) as usize,
                border: args.bool_or("border", false)?,
            },
            "tile" => PrintLayout::Tile {
                percent: percent.unwrap_or(100.0),
                overlap: args.opt_num("overlap")?.unwrap_or(18.0),
                cut_marks: args.bool_or("cut_marks", true)?,
            },
            l => return Err(bad_args(format!("unknown layout {l:?}"))),
        };
        let markups = args.bool_or("markups", true)?;
        let (doc, s) = a.session_ref(args)?;
        // Print order is the order given, so a list is not sorted.
        let pages: Vec<usize> = match args.get("pages") {
            Some(Value::Array(v)) => v
                .iter()
                .map(|x| match x.as_u64() {
                    Some(p) if p >= 1 => Ok(p as usize - 1),
                    _ => Err(bad_args("pages must be page numbers from 1")),
                })
                .collect::<crate::Result<_>>()?,
            Some(_) => args.pages("pages", s.page_count())?,
            None => Vec::new(),
        };
        let settings = PrintSettings {
            pages,
            paper,
            landscape,
            layout,
            markups,
        };
        let region = match args.get("region").and_then(Value::as_array) {
            Some(v) => {
                let n: Vec<f64> = v.iter().filter_map(Value::as_f64).collect();
                match n.as_slice() {
                    [p, x0, y0, x1, y1] if *p >= 1.0 => {
                        Some((*p as usize - 1, markupcraft_engine::Rect::new(*x0, *y0, *x1, *y1)))
                    }
                    _ => return Err(bad_args("region: [page, x0, y0, x1, y1]")),
                }
            }
            None => None,
        };
        let offset = match args.get("offset").and_then(Value::as_array) {
            Some(v) => match v.iter().filter_map(Value::as_f64).collect::<Vec<_>>().as_slice() {
                [x, y] => (*x, *y),
                _ => return Err(bad_args("offset: [dx, dy]")),
            },
            None => (0.0, 0.0),
        };
        let job = PrintJob {
            settings,
            markups_only: args.bool_or("markups_only", false)?,
            region,
            copies: args.opt_u64("copies")?.unwrap_or(1) as usize,
            collate: args.bool_or("collate", true)?,
            reverse: args.bool_or("reverse", false)?,
            margin: args.opt_num("margin")?.unwrap_or(0.0),
            offset,
            dim_content: args.bool_or("dim_content", false)?,
            dim_except: args.opt_strings("dim_except")?,
            spaces: args.bool_or("spaces", false)?,
            links: args.bool_or("links", false)?,
        };
        let sheets = s.print_job_to_pdf(&out, &job)?;
        let mut sent = false;
        if let Some(p) = args.opt_str("printer")? {
            send_to_printer(&out, (!p.is_empty()).then_some(p), 1)?;
            sent = true;
        }
        Ok(json!({ "doc": doc, "out": out.display().to_string(), "sheets": sheets, "sent_to_printer": sent }))
    },
};

pub static SPLIT: Tool = Tool {
    name: "doc_split",
    title: "Split a document",
    description: "Split the document into PDFs in folder `dir`, named <name>-<part>.pdf: every `pages_per_file` pages, at each top-level bookmark (by: \"bookmarks\"), or into `ranges` ([\"1-3\", \"4-9\"]). Markups go with their pages. The document is not changed.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "dir": path_arg("The folder for the parts"),
                "pages_per_file": { "type": "integer", "minimum": 1 },
                "by": { "type": "string", "enum": ["pages", "bookmarks", "ranges"] },
                "ranges": { "type": "array", "items": { "type": "string" } }
            }),
            &["dir"],
        )
    },
    run: |a, args| {
        let dir = a.resolve(args.str("dir")?, true)?;
        let by_name = args.opt_str("by")?;
        let (doc, s) = a.session_ref(args)?;
        let n = s.page_count();
        let by = match (by_name, args.get("ranges"), args.opt_u64("pages_per_file")?) {
            (Some("bookmarks"), None, None) => SplitBy::Bookmarks,
            (None | Some("ranges"), Some(r), None) => {
                let list = r
                    .as_array()
                    .ok_or_else(|| bad_args("ranges must be a list of page ranges"))?;
                let mut parts = Vec::with_capacity(list.len());
                for x in list {
                    let t = x
                        .as_str()
                        .ok_or_else(|| bad_args("each range must be text like \"1-3\""))?;
                    parts.push(crate::parse_range(t, n).map_err(bad_args)?);
                }
                SplitBy::Ranges(parts)
            }
            (None | Some("pages"), None, Some(k)) => SplitBy::Pages(k as usize),
            _ => {
                return Err(bad_args("doc_split: give pages_per_file, ranges, or by: \"bookmarks\""));
            }
        };
        let parts = s.split_document(&dir, &by)?;
        let files: Vec<Value> = parts
            .iter()
            .map(|p| {
                json!({ "path": p.path.display().to_string(), "pages": p.pages.iter().map(|i| i + 1).collect::<Vec<_>>() })
            })
            .collect();
        Ok(json!({ "doc": doc, "files": files }))
    },
};

pub static COMBINE: Tool = Tool {
    name: "doc_combine",
    title: "Combine PDFs",
    description: "Combine PDFs (in order, with their markups and page labels) into a new PDF at `out`. bookmarks: true (default) gives it one bookmark per file. No document needs to be open; open the result with doc_open.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "files": { "type": "array", "items": { "type": "string" }, "description": "The PDFs to combine (relative to --root when one is set)." },
                "out": path_arg("The combined PDF"),
                "bookmarks": { "type": "boolean" }
            }),
            &["files", "out"],
        )
    },
    run: |a, args| {
        let names = args.opt_strings("files")?.unwrap_or_default();
        let files = names
            .iter()
            .map(|f| a.resolve(f, false))
            .collect::<crate::Result<Vec<_>>>()?;
        let out = a.resolve(args.str("out")?, true)?;
        let pages = combine_files(&files, &out, args.bool_or("bookmarks", true)?)?;
        Ok(json!({ "out": out.display().to_string(), "pages": pages, "files": files.len() }))
    },
};

pub static REPLACE: Tool = Tool {
    name: "page_replace",
    title: "Replace pages",
    description: "Replace `pages` with `source_pages` of the PDF at `path` (as many of each, paired in order): each page shows the new drawing but keeps its markups, links and bookmarks (a new revision of a sheet). The next save rewrites the file in full. Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to replace"),
                "path": path_arg("The PDF with the new pages"),
                "source_pages": pages_arg("of that PDF (default: as many from page 1)")
            }),
            &["pages", "path"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let src_pages = markupcraft_engine::pages::ForeignPdf::open(&path)?.page_count();
        let (doc, s) = a.session(args)?;
        let targets = args.pages("pages", s.page_count())?;
        let source = match args.get("source_pages") {
            Some(_) => args.pages("source_pages", src_pages)?,
            None => (0..targets.len()).collect(),
        };
        s.replace_pages(&targets, &path, &source)?;
        Ok(json!({ "replaced": targets.len(), "document": summary(doc, s) }))
    },
};

fn rect_json(r: &markupcraft_engine::Rect) -> Value {
    json!(r.as_array())
}

pub static BOXES: Tool = Tool {
    name: "page_boxes",
    title: "Page boxes",
    description: "Each page's media, crop, bleed, trim and art boxes ([x0, y0, x1, y1] in points).",
    read_only: true,
    destructive: false,
    schema: || schema(json!({ "pages": pages_arg("(default: all)") }), &[]),
    run: |a, args| {
        let (doc, s) = a.session_ref(args)?;
        let pages = args
            .opt_pages("pages", s.page_count())?
            .unwrap_or_else(|| (0..s.page_count()).collect());
        let all = s.page_boxes()?;
        let list: Vec<Value> = pages
            .iter()
            .filter_map(|p| all.get(*p).map(|b| (p, b)))
            .map(|(p, b)| {
                json!({
                    "page": p + 1,
                    "media": rect_json(&b.media),
                    "crop": rect_json(&b.crop),
                    "bleed": rect_json(&b.bleed),
                    "trim": rect_json(&b.trim),
                    "art": rect_json(&b.art),
                })
            })
            .collect();
        Ok(json!({ "doc": doc, "pages": list }))
    },
};

pub static CROP: Tool = Tool {
    name: "page_crop",
    title: "Crop pages",
    description: "Set the crop box (or bleed/trim/art with `box`) of pages (default all): `rect`, or `margins` [left, bottom, right, top] inward from the page edge, or remove: true for the default. Markups stay where they are. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to crop (default: all)"),
                "box": { "type": "string", "enum": ["crop", "bleed", "trim", "art"] },
                "rect": rect_arg("The new box"),
                "margins": { "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4, "description": "[left, bottom, right, top] in points, inward from the media box." },
                "remove": { "type": "boolean" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let which = match args.opt_str("box")?.unwrap_or("crop") {
            "crop" => PageBox::Crop,
            "bleed" => PageBox::Bleed,
            "trim" => PageBox::Trim,
            "art" => PageBox::Art,
            b => return Err(bad_args(format!("unknown box {b:?}"))),
        };
        let margins = match args.get("margins") {
            None => None,
            Some(v) => {
                let m: Vec<f64> = v
                    .as_array()
                    .map(|x| x.iter().filter_map(Value::as_f64).collect())
                    .unwrap_or_default();
                match m.as_slice() {
                    [l, b, r, t] => Some([*l, *b, *r, *t]),
                    _ => return Err(bad_args("margins must be [left, bottom, right, top]")),
                }
            }
        };
        let change = match (args.opt_rect("rect")?, margins, args.bool_or("remove", false)?) {
            (Some(r), None, false) => BoxChange::Rect(r),
            (None, Some(m), false) => BoxChange::Margins(m),
            (None, None, true) => BoxChange::Remove,
            _ => return Err(bad_args("page_crop: give one of rect, margins or remove: true")),
        };
        let (doc, s) = a.session(args)?;
        let pages = args
            .opt_pages("pages", s.page_count())?
            .unwrap_or_else(|| (0..s.page_count()).collect());
        s.set_page_box(&pages, which, change)?;
        Ok(json!({ "pages": pages.len(), "document": summary(doc, s) }))
    },
};

pub static RESIZE: Tool = Tool {
    name: "page_resize",
    title: "Change page size",
    description: "Resize pages (default all) to `width` x `height` points as displayed (or a `paper` name), keeping the drawing, markups and scales in place at `anchor` (center, top_left, top, top_right, left, right, bottom_left, bottom, bottom_right). The drawing is not scaled. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to resize (default: all)"),
                "width": { "type": "number" },
                "height": { "type": "number" },
                "paper": { "type": "string" },
                "landscape": { "type": "boolean", "description": "With paper: turn it landscape." },
                "anchor": { "type": "string" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let (w, h) = match (args.opt_num("width")?, args.opt_num("height")?, args.opt_str("paper")?) {
            (Some(w), Some(h), None) => (w, h),
            (None, None, Some(p)) => {
                let (w, h) = paper_size(p).ok_or_else(|| bad_args(format!("unknown paper {p:?}")))?;
                if args.bool_or("landscape", false)? {
                    (h, w)
                } else {
                    (w, h)
                }
            }
            _ => return Err(bad_args("page_resize: give width and height, or paper")),
        };
        let anchor = match args.opt_str("anchor")? {
            None => Anchor::Center,
            Some(n) => Anchor::from_name(n).ok_or_else(|| bad_args(format!("unknown anchor {n:?}")))?,
        };
        let (doc, s) = a.session(args)?;
        let pages = args
            .opt_pages("pages", s.page_count())?
            .unwrap_or_else(|| (0..s.page_count()).collect());
        s.resize_pages(&pages, w, h, anchor)?;
        Ok(json!({ "pages": pages.len(), "document": summary(doc, s) }))
    },
};
