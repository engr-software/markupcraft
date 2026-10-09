//! Whole-file behaviour: images opened as PDF pages, stored revisions and Revert As, Publish
//! As, Deskew, and the signature / PDF/A state that guards page edits.

use markupcraft_engine::Session;
use markupcraft_engine::docfile::{PublishMode, deskew_angle};
use serde_json::json;

use super::{Tool, pages_arg, path_arg, point_arg, rect_arg, schema, schema_nodoc};
use crate::{bad_args, summary};

pub static FROM_IMAGE: Tool = Tool {
    name: "doc_from_image",
    title: "Open an image as a PDF",
    description: "Make a new document from an image file (PNG, JPEG, TIFF or BMP): one page the size of the image at its declared resolution (72 ppi when it declares none). It saves to `path` (default: the image's name with .pdf), not written until doc_save.",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({
                "image": path_arg("The image file"),
                "path": path_arg("Where the new PDF saves")
            }),
            &["image"],
        )
    },
    run: |a, args| {
        let image = a.resolve(args.str("image")?, false)?;
        let path = match args.opt_str("path")? {
            Some(p) => a.resolve(p, true)?,
            None => image.with_extension("pdf"),
        };
        let s = Session::from_image(&image, &path)?;
        let id = a.add_doc(s)?;
        let (_, s) = a.session_ref(args)?;
        Ok(summary(id, s))
    },
};

pub static REVISIONS: Tool = Tool {
    name: "doc_revisions",
    title: "Stored revisions / Revert As",
    description: "How many revisions the document's file keeps (each incremental save adds one). With `revision` (1 = the first) and `out`, write that revision as it was saved to a new file (Revert As). The document is not changed.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "revision": { "type": "integer", "minimum": 1 },
                "out": path_arg("Where the revision is written")
            }),
            &[],
        )
    },
    run: |a, args| {
        let rev = args.opt_u64("revision")?;
        let out = args.opt_str("out")?.map(|p| a.resolve(p, true)).transpose()?;
        let (_, s) = a.session_ref(args)?;
        let count = s.revision_count();
        match (rev, out) {
            (Some(r), Some(out)) => {
                let bytes = s.revert_as((r as usize).saturating_sub(1), &out)?;
                Ok(json!({ "revisions": count, "written": out.display().to_string(), "bytes": bytes }))
            }
            (None, None) => Ok(json!({ "revisions": count })),
            _ => Err(bad_args("revision and out go together")),
        }
    },
};

pub static PUBLISH: Tool = Tool {
    name: "doc_publish",
    title: "Publish As",
    description: "Write the document as it is now to `out` without its revision history: mode flattened (markups burned into the pages), compressed (object streams, PDF 1.5) or uncompressed (a classic cross-reference table). The document is not changed.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "out": path_arg("The published PDF"),
                "mode": { "type": "string", "enum": ["flattened", "compressed", "uncompressed"] }
            }),
            &["out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let mode = args.opt_str("mode")?.unwrap_or("compressed");
        let mode = PublishMode::parse(mode).ok_or_else(|| bad_args(format!("unknown mode {mode:?}")))?;
        let (_, s) = a.session_ref(args)?;
        let bytes = s.publish_as(&out, mode)?;
        Ok(json!({ "written": out.display().to_string(), "bytes": bytes }))
    },
};

pub static DESKEW: Tool = Tool {
    name: "page_deskew",
    title: "Deskew pages",
    description: "Turn the content of `pages` (default: the first page) to straighten a skewed scan: by `degrees` (counter-clockwise, at most 45 either way), or by the turn that makes the line from `from` to `to` level. Markups stay where they are. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to straighten"),
                "degrees": { "type": "number" },
                "from": point_arg("A point on a line that should be level"),
                "to": point_arg("Another point on that line")
            }),
            &[],
        )
    },
    run: |a, args| {
        let degrees = match (args.opt_num("degrees")?, args.opt_point("from")?, args.opt_point("to")?) {
            (Some(d), None, None) => d,
            (None, Some(f), Some(t)) => deskew_angle(f, t),
            _ => return Err(bad_args("give degrees, or from and to")),
        };
        let (doc, s) = a.session(args)?;
        let pages = args.opt_pages("pages", s.page_count())?.unwrap_or_else(|| vec![0]);
        s.deskew_pages(&pages, degrees)?;
        Ok(
            json!({ "degrees": degrees, "pages": pages.iter().map(|p| p + 1).collect::<Vec<_>>(), "document": summary(doc, s) }),
        )
    },
};

pub static STANDARDS: Tool = Tool {
    name: "doc_standards",
    title: "Signatures and PDF/A",
    description: "Signed signature fields, whether the document is certified, and the PDF/A conformance its metadata claims. Certified and PDF/A documents refuse page edits in the app; signed ones warn that page edits clear the signatures.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (_, s) = a.session_ref(args)?;
        let st = s.standards();
        Ok(json!({
            "signatures": st.signatures,
            "certified": st.certified,
            "pdfa": st.pdfa,
            "page_edits_blocked": st.blocks_page_edits(),
        }))
    },
};

pub static REGION_LABELS: Tool = Tool {
    name: "page_labels_from_region",
    title: "Page labels from a page region",
    description: "Label pages from the text inside one or more boxes (`regions`: [[x0, y0, x1, y1], ...] in PDF points, the same place on every page, such as the sheet number and title in the title block). Regions are joined by `between` (default \" - \"), with `before` and `after` around. Returns the labels found; apply: true also sets them (undoable).",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "regions": { "type": "array", "items": rect_arg("A region"), "minItems": 1 },
                "before": { "type": "string" },
                "between": { "type": "string" },
                "after": { "type": "string" },
                "apply": { "type": "boolean" }
            }),
            &["regions"],
        )
    },
    run: |a, args| {
        let list = args
            .get("regions")
            .and_then(|v| v.as_array())
            .ok_or_else(|| bad_args("regions must be a list of [x0, y0, x1, y1]"))?;
        let mut regions = Vec::new();
        for v in list.iter().take(64) {
            let n: Vec<f64> = v
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
                .unwrap_or_default();
            match n.as_slice() {
                [x0, y0, x1, y1] => regions.push(markupcraft_engine::Rect::new(*x0, *y0, *x1, *y1)),
                _ => return Err(bad_args("each region is [x0, y0, x1, y1]")),
            }
        }
        let before = args.opt_str("before")?.unwrap_or("").to_string();
        let between = args.opt_str("between")?.unwrap_or(" - ").to_string();
        let after = args.opt_str("after")?.unwrap_or("").to_string();
        let apply = args.bool_or("apply", false)?;
        let (doc, s) = a.session(args)?;
        let labels = s.region_labels(&regions, &before, &between, &after)?;
        if apply {
            s.set_page_labels(&labels)?;
        }
        Ok(json!({
            "labels": labels.iter().map(|(p, l)| json!({ "page": p + 1, "label": l })).collect::<Vec<_>>(),
            "applied": apply,
            "document": summary(doc, s),
        }))
    },
};
