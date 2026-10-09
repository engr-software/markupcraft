//! Page operations. Markups, page scales and page labels follow their pages.

use serde_json::json;

use super::{Tool, page_arg, pages_arg, path_arg, schema};
use crate::{bad_args, failed, summary};

fn report(r: &markupcraft_engine::pages::PageReport) -> serde_json::Value {
    json!({
        "pages_before": r.pages_before,
        "pages_after": r.pages_after,
        "markups_before": r.markups_before,
        "markups_after": r.markups_after,
    })
}

pub static ROTATE: Tool = Tool {
    name: "page_rotate",
    title: "Rotate pages",
    description: "Rotate pages (default all) by a multiple of 90 degrees, clockwise as viewers show it. Markups stay on their pages. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "pages": pages_arg("to rotate (default: all)"), "degrees": { "type": "integer", "description": "90, 180, 270, -90 ..." } }),
            &["degrees"],
        )
    },
    run: |a, args| {
        let deg = args.int("degrees")?;
        let (doc, s) = a.session(args)?;
        let pages = args
            .opt_pages("pages", s.page_count())?
            .unwrap_or_else(|| (0..s.page_count()).collect());
        let r = s.rotate_pages(&pages, deg)?;
        Ok(json!({ "report": report(&r), "document": summary(doc, s) }))
    },
};

pub static DELETE: Tool = Tool {
    name: "page_delete",
    title: "Delete pages",
    description: "Delete pages and the markups on them. The next save rewrites the file in full so the pages are really gone. Undoable until saved.",
    read_only: false,
    destructive: true,
    schema: || schema(json!({ "pages": pages_arg("to delete") }), &["pages"]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let pages = args.pages("pages", s.page_count())?;
        let r = s.delete_pages(&pages)?;
        Ok(json!({ "report": report(&r), "document": summary(doc, s) }))
    },
};

pub static MOVE: Tool = Tool {
    name: "page_move",
    title: "Move pages",
    description: "Move pages so they sit together, in order, before page `before` (counted before the move; page count + 1 = the end). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "pages": pages_arg("to move"), "before": { "type": "integer", "minimum": 1, "description": "1-based position; page count + 1 moves them to the end." } }),
            &["pages", "before"],
        )
    },
    run: |a, args| {
        let before = args.position("before")?;
        let (doc, s) = a.session(args)?;
        let pages = args.pages("pages", s.page_count())?;
        let r = s.move_pages(&pages, before)?;
        Ok(json!({ "report": report(&r), "document": summary(doc, s) }))
    },
};

pub static INSERT_BLANK: Tool = Tool {
    name: "page_insert_blank",
    title: "Insert blank pages",
    description: "Insert `count` blank pages (default 1) so the first becomes page `at` (page count + 1 = the end). Size defaults to the page before. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "at": page_arg("position of the first new page"),
                "count": { "type": "integer", "minimum": 1 },
                "width": { "type": "number", "description": "Points." },
                "height": { "type": "number", "description": "Points." }
            }),
            &["at"],
        )
    },
    run: |a, args| {
        let at = args.position("at")?;
        let count = args.opt_u64("count")?.unwrap_or(1) as usize;
        let size = match (args.opt_num("width")?, args.opt_num("height")?) {
            (Some(w), Some(h)) => Some((w, h)),
            (None, None) => None,
            _ => return Err(bad_args("give both width and height, or neither")),
        };
        let (doc, s) = a.session(args)?;
        let r = s.insert_blank_pages(at, count, size)?;
        Ok(json!({ "report": report(&r), "document": summary(doc, s) }))
    },
};

pub static INSERT_FILE: Tool = Tool {
    name: "page_insert_file",
    title: "Insert pages from a PDF",
    description: "Insert pages of another PDF (default all) so the first becomes page `at`. Their markups come along; a markup id already used here gets a new one. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "path": path_arg("The PDF to take pages from"), "at": page_arg("position of the first inserted page"), "pages": pages_arg("of that file (default: all)") }),
            &["path", "at"],
        )
    },
    run: |a, args| {
        let at = args.position("at")?;
        let path = a.resolve(args.str("path")?, false)?;
        // Validate a page range against the source file's page count.
        let src_pages = markupcraft_engine::pages::ForeignPdf::open(&path)?.page_count();
        let pages = args.opt_pages("pages", src_pages)?;
        let (doc, s) = a.session(args)?;
        let r = s.insert_file_pages(at, &path, pages.as_deref())?;
        Ok(json!({ "report": report(&r), "document": summary(doc, s) }))
    },
};

pub static EXTRACT: Tool = Tool {
    name: "page_extract",
    title: "Extract pages",
    description: "Write pages, with their markups (unsaved ones included), to a new PDF at `out` (atomic; bookmarks are not carried). delete: true also removes them from this document (undoable).",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "pages": pages_arg("to extract"), "out": path_arg("The new PDF"), "delete": { "type": "boolean" } }),
            &["pages", "out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let delete = args.bool_or("delete", false)?;
        let (doc, s) = a.session(args)?;
        let pages = args.pages("pages", s.page_count())?;
        let n = s.extract_pages(&pages, &out, delete)?;
        Ok(json!({ "out": out.display().to_string(), "pages": n, "document": summary(doc, s) }))
    },
};

pub static LABELS: Tool = Tool {
    name: "page_labels",
    title: "Page labels",
    description: "Each page's label (\"\" where the file has none), as viewers show them.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session_ref(args)?;
        if s.page_count() == 0 {
            return Err(failed("the document has no pages"));
        }
        Ok(json!({ "doc": doc, "labels": s.page_labels() }))
    },
};
