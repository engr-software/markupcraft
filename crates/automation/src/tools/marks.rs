//! Page marks: headers and footers, watermarks, Bates numbering.

use markupcraft_engine::marks::{Bates, HeaderFooter, MarkKind, Slot, Watermark};
use serde_json::{Value, json};

use super::{Tool, pages_arg, schema};
use crate::{Args, Result, bad_args, summary};

const SLOTS: [&str; 6] = [
    "header_left",
    "header_center",
    "header_right",
    "footer_left",
    "footer_center",
    "footer_right",
];

fn margins_arg() -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4, "description": "Distance from the page edges in points: [top, bottom, left, right]." })
}

fn margins(a: &Args, dflt: [f64; 4]) -> Result<[f64; 4]> {
    let Some(v) = a.get("margins") else { return Ok(dflt) };
    let arr: Vec<f64> = v
        .as_array()
        .map(|x| x.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    match arr.as_slice() {
        [t, b, l, r] => Ok([*t, *b, *l, *r]),
        _ => Err(bad_args("margins must be [top, bottom, left, right]")),
    }
}

fn pages(a: &Args, count: usize) -> Result<Vec<usize>> {
    Ok(a.opt_pages("pages", count)?.unwrap_or_else(|| (0..count).collect()))
}

fn mark_kind(s: &str) -> Option<MarkKind> {
    Some(match s {
        "header_footer" | "bates" => MarkKind::HeaderFooter,
        "watermark" => MarkKind::Watermark,
        "background" => MarkKind::Background,
        _ => return None,
    })
}

fn kind_name(k: MarkKind) -> &'static str {
    match k {
        MarkKind::HeaderFooter => "header_footer",
        MarkKind::Watermark => "watermark",
        MarkKind::Background => "background",
    }
}

pub static HEADER_FOOTER: Tool = Tool {
    name: "header_footer_add",
    title: "Add header and footer",
    description: "Add header/footer text to pages (default all) in any of six places: header_left, header_center, header_right, footer_left, footer_center, footer_right. Tokens: <<1>> page number, <<n>> page count, <<Page 1 of n>>, <<m/d/yyyy>>, <<yyyy-mm-dd>>, <<Bates Number#digits#start#prefix#suffix>>. replace: true removes existing headers/footers on those pages first. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        let mut p = json!({
            "pages": pages_arg("to mark (default: all)"),
            "font_size": { "type": "number", "minimum": 1 },
            "color": { "type": ["string", "array"] },
            "underline": { "type": "boolean" },
            "margins": margins_arg(),
            "start_number": { "type": "integer", "minimum": 1, "description": "The number <<1>> shows on the first page." },
            "replace": { "type": "boolean" }
        });
        if let Some(o) = p.as_object_mut() {
            for s in SLOTS {
                o.insert(s.into(), json!({ "type": "string" }));
            }
        }
        schema(p, &[])
    },
    run: |a, args| {
        let mut hf = HeaderFooter::default();
        for (i, s) in SLOTS.iter().enumerate() {
            if let (Some(t), Some(slot)) = (args.opt_string(s)?, hf.text.get_mut(i)) {
                *slot = t;
            }
        }
        if let Some(f) = args.opt_num("font_size")? {
            hf.font_size = f;
        }
        if let Some(c) = args.opt_color("color")? {
            hf.color = c;
        }
        hf.underline = args.bool_or("underline", false)?;
        hf.margins = margins(args, hf.margins)?;
        if let Some(n) = args.opt_u64("start_number")? {
            hf.start_number = u32::try_from(n).map_err(|_| bad_args("start_number is too large"))?;
        }
        let replace = args.bool_or("replace", false)?;
        let (doc, s) = a.session(args)?;
        let pages = pages(args, s.page_count())?;
        s.add_header_footer(&pages, &hf, replace)?;
        Ok(json!({ "pages": pages.len(), "document": summary(doc, s) }))
    },
};

pub static WATERMARK: Tool = Tool {
    name: "watermark_add",
    title: "Add a watermark",
    description: "Add a text watermark to pages (default all): centred, turned `rotation` degrees counter-clockwise (default 45), `opacity` 0-1 (default 0.5), font_size 0 = fit the page. behind: true draws it under the page content. replace: true removes existing watermarks first. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "text": { "type": "string" },
                "pages": pages_arg("to mark (default: all)"),
                "font_size": { "type": "number", "minimum": 0 },
                "color": { "type": ["string", "array"] },
                "opacity": { "type": "number", "minimum": 0, "maximum": 1 },
                "rotation": { "type": "number" },
                "behind": { "type": "boolean" },
                "offset": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2, "description": "[right, up] from the page centre, points." },
                "replace": { "type": "boolean" }
            }),
            &["text"],
        )
    },
    run: |a, args| {
        let mut wm = Watermark {
            text: args.str("text")?.to_string(),
            ..Default::default()
        };
        if let Some(f) = args.opt_num("font_size")? {
            wm.font_size = f;
        }
        if let Some(c) = args.opt_color("color")? {
            wm.color = c;
        }
        if let Some(o) = args.opt_num("opacity")? {
            wm.opacity = o;
        }
        if let Some(r) = args.opt_num("rotation")? {
            wm.rotation = r;
        }
        wm.behind = args.bool_or("behind", false)?;
        if let Some(p) = args.opt_point("offset")? {
            wm.offset = [p.x, p.y];
        }
        let replace = args.bool_or("replace", false)?;
        let (doc, s) = a.session(args)?;
        let pages = pages(args, s.page_count())?;
        s.add_watermark(&pages, &wm, replace)?;
        Ok(json!({ "pages": pages.len(), "document": summary(doc, s) }))
    },
};

pub static BATES: Tool = Tool {
    name: "bates_add",
    title: "Bates numbering",
    description: "Number pages (default all, in order) with Bates numbers: prefix + zero-padded number + suffix, from `start` (default 1) with `digits` digits (default 6), in one of the header/footer places (default footer_right). Undoable; returns the first and last number.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to number (default: all)"),
                "prefix": { "type": "string" },
                "suffix": { "type": "string" },
                "digits": { "type": "integer", "minimum": 1, "maximum": 15 },
                "start": { "type": "integer", "minimum": 0 },
                "position": { "type": "string", "enum": SLOTS },
                "font_size": { "type": "number", "minimum": 1 },
                "color": { "type": ["string", "array"] },
                "margins": margins_arg(),
                "replace": { "type": "boolean" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let mut b = Bates {
            prefix: args.opt_string("prefix")?.unwrap_or_default(),
            suffix: args.opt_string("suffix")?.unwrap_or_default(),
            ..Default::default()
        };
        if let Some(d) = args.opt_u64("digits")? {
            b.digits = d as usize;
        }
        if let Some(s) = args.opt_u64("start")? {
            b.start = s;
        }
        if let Some(p) = args.opt_str("position")? {
            b.slot = Slot::from_name(p).ok_or_else(|| bad_args(format!("unknown position {p:?}")))?;
        }
        if let Some(f) = args.opt_num("font_size")? {
            b.font_size = f;
        }
        if let Some(c) = args.opt_color("color")? {
            b.color = c;
        }
        b.margins = margins(args, b.margins)?;
        let replace = args.bool_or("replace", false)?;
        let (doc, s) = a.session(args)?;
        let pages = pages(args, s.page_count())?;
        let (first, last) = s.add_bates(&pages, &b, replace)?;
        Ok(json!({ "first": first, "last": last, "pages": pages.len(), "document": summary(doc, s) }))
    },
};

pub static REMOVE: Tool = Tool {
    name: "marks_remove",
    title: "Remove headers, footers or watermarks",
    description: "Remove page marks of one kind (header_footer, which includes Bates numbers; watermark; background) from pages (default all). Returns how many were removed and the kinds still present. Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "kind": { "type": "string", "enum": ["header_footer", "bates", "watermark", "background"] }, "pages": pages_arg("(default: all)") }),
            &["kind"],
        )
    },
    run: |a, args| {
        let k = args.str("kind")?;
        let kind = mark_kind(k).ok_or_else(|| bad_args(format!("unknown kind {k:?}")))?;
        let (doc, s) = a.session(args)?;
        let pages = pages(args, s.page_count())?;
        let n = s.remove_marks(&pages, kind)?;
        let present: Vec<&str> = s.marks_present().into_iter().map(kind_name).collect();
        Ok(json!({ "removed": n, "present": present, "document": summary(doc, s) }))
    },
};
