//! Redaction: mark, list, apply.

use markupcraft_engine::redact::MarkStyle;
use serde_json::{Value, json};

use super::{Tool, page_arg, pages_arg, rect_arg, schema};
use crate::{bad_args, summary};

pub static MARK: Tool = Tool {
    name: "redact_mark",
    title: "Mark for Redaction",
    description: "Mark content for redaction: rectangles on a page (page + rects), or every occurrence of `text` in the page text (optionally only on `pages`). Marks are Redact annotations: nothing is removed until redact_apply. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("to mark areas on"),
                "rects": { "type": "array", "items": rect_arg("An area"), "description": "Areas to mark, [[x0, y0, x1, y1], ...]." },
                "text": { "type": "string", "description": "Mark every occurrence of this text instead." },
                "case_sensitive": { "type": "boolean" },
                "whole_words": { "type": "boolean" },
                "pages": pages_arg("to search (default: all)"),
                "fill": { "type": ["string", "array"], "description": "Box colour once applied (default black; \"none\" = no box)." },
                "overlay": { "type": "string", "description": "Text drawn on the box once applied (a redaction code from redact_codes, or any text)." },
                "font": { "type": "string", "enum": ["Helvetica", "Times", "Courier"] },
                "font_size": { "type": "number", "minimum": 0, "maximum": 144, "description": "0 = fit the box." },
                "text_color": { "type": ["string", "array"] },
                "align": { "type": "string", "enum": ["left", "center", "right"] },
                "repeat": { "type": "boolean", "description": "Repeat the overlay text to fill the box." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let mut style = MarkStyle::default();
        if args.get("fill").and_then(Value::as_str) == Some("none") {
            style.fill = None;
        } else if let Some(c) = args.opt_color("fill")? {
            style.fill = Some(c);
        }
        if let Some(o) = args.opt_string("overlay")? {
            style.overlay = o;
        }
        if let Some(f) = args.opt_string("font")? {
            if !["Helvetica", "Times", "Courier"].contains(&f.as_str()) {
                return Err(bad_args("font is Helvetica, Times or Courier"));
            }
            style.font = f;
        }
        if let Some(sz) = args.opt_num("font_size")? {
            if !(0.0..=144.0).contains(&sz) {
                return Err(bad_args("font_size is 0 to 144"));
            }
            style.font_size = sz;
        }
        if let Some(c) = args.opt_color("text_color")? {
            style.text_color = c;
        }
        if let Some(al) = args.opt_str("align")? {
            style.align = match al {
                "left" => 0,
                "center" => 1,
                "right" => 2,
                other => return Err(bad_args(format!("align is left, center or right, not {other:?}"))),
            };
        }
        if let Some(r) = args.opt_bool("repeat")? {
            style.repeat = r;
        }
        let (doc, s) = a.session(args)?;
        let marked = match (args.opt_str("text")?, args.opt_page("page")?) {
            (Some(t), None) => {
                let pages = args.opt_pages("pages", s.page_count())?.unwrap_or_default();
                s.redact_search(
                    t,
                    args.bool_or("case_sensitive", false)?,
                    args.bool_or("whole_words", false)?,
                    &pages,
                    &style,
                )?
            }
            (None, Some(page)) => {
                let list = args
                    .get("rects")
                    .and_then(Value::as_array)
                    .ok_or_else(|| bad_args("give rects: [[x0, y0, x1, y1], ...] with page"))?;
                let rects = list
                    .iter()
                    .map(|r| crate::args::rect_of(r).ok_or_else(|| bad_args("rects must be [[x0, y0, x1, y1], ...]")))
                    .collect::<crate::Result<Vec<_>>>()?;
                s.redact_mark(page, &rects, &style)?
            }
            _ => return Err(bad_args("give either page + rects, or text")),
        };
        Ok(json!({ "marked": marked, "marks": s.redact_marks().len(), "document": summary(doc, s) }))
    },
};

pub static LIST: Tool = Tool {
    name: "redact_list",
    title: "List redaction marks",
    description: "The redaction marks not yet applied: page, rectangles, fill colour, overlay text.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session_ref(args)?;
        let marks: Vec<Value> = s
            .redact_marks()
            .iter()
            .map(|m| {
                json!({
                    "page": m.page + 1,
                    "rects": m.rects.iter().map(|r| r.as_array()).collect::<Vec<_>>(),
                    "fill": m.fill.map(|c| c.hex()),
                    "overlay": m.overlay,
                })
            })
            .collect();
        Ok(json!({ "doc": doc, "count": marks.len(), "marks": marks }))
    },
};

pub static APPLY: Tool = Tool {
    name: "redact_apply",
    title: "Apply Redactions",
    description: "Apply the redaction marks (on `pages`, default all): text, images and paths under them are removed, overlapping comments, links and fields too, and the boxes are drawn. Then verifies no text is readable under them (residue lists any). The next save rewrites the whole file so earlier revisions do not keep the content. Undoable until saved.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to apply marks on (default: all)"),
                "scrub_metadata": { "type": "boolean", "description": "Also remove the document properties, XMP metadata, attachments and JavaScript (default false)." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let scrub = args.bool_or("scrub_metadata", false)?;
        let (doc, s) = a.session(args)?;
        let pages = args.opt_pages("pages", s.page_count())?;
        let r = s.redact_apply_with(pages.as_deref(), scrub)?;
        Ok(json!({
            "marks": r.marks,
            "pages": r.pages,
            "glyphs": r.glyphs,
            "images": r.images,
            "paths": r.paths,
            "annotations": r.annotations,
            "fields": r.fields,
            "verified": r.residue.is_empty(),
            "residue": r.residue,
            "document": summary(doc, s),
        }))
    },
};

pub static CODES: Tool = Tool {
    name: "redact_codes",
    title: "Redaction codes",
    description: "The built-in redaction codes for overlay text (US FOIA exemptions and DOD codes) with what each means.",
    read_only: true,
    destructive: false,
    schema: || super::schema_nodoc(json!({}), &[]),
    run: |_, _| {
        let codes: Vec<Value> = markupcraft_engine::redact::REDACTION_CODES
            .iter()
            .map(|(c, d)| json!({ "code": c, "description": d }))
            .collect();
        Ok(json!({ "codes": codes }))
    },
};
