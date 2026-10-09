//! Documents: open, new, list, info, save, close.

use std::collections::BTreeMap;

use markupcraft_engine::{Session, blank};
use serde_json::{Value, json};

use super::{Tool, path_arg, schema, schema_nodoc};
use crate::{failed, summary};

pub static OPEN: Tool = Tool {
    name: "doc_open",
    title: "Open a PDF",
    description: "Open a PDF and its markups. Returns the document id (the default `doc` for later tools), page count and markup count.",
    read_only: true,
    destructive: false,
    schema: || schema_nodoc(json!({ "path": path_arg("The PDF to open") }), &["path"]),
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let s = Session::open(&path)?;
        let id = a.add_doc(s)?;
        let (_, s) = a.session_ref(args)?;
        Ok(summary(id, s))
    },
};

pub static NEW: Tool = Tool {
    name: "doc_new",
    title: "New document",
    description: "Create a new document of blank pages that saves to `path` (not written until doc_save). Pages default to one US Letter page.",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({
                "path": path_arg("Where the document saves"),
                "pages": { "type": "integer", "minimum": 1, "description": "Number of pages (default 1)." },
                "width": { "type": "number", "description": "Page width in points (default 612)." },
                "height": { "type": "number", "description": "Page height in points (default 792)." }
            }),
            &["path"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, true)?;
        let n = args.opt_int("pages")?.unwrap_or(1);
        if !(1..=blank::MAX_PAGES as i64).contains(&n) {
            return Err(crate::bad_args(format!("pages must be 1 to {}", blank::MAX_PAGES)));
        }
        let size = (
            args.opt_num("width")?.unwrap_or(612.0),
            args.opt_num("height")?.unwrap_or(792.0),
        );
        let s = Session::new_blank(&path, &vec![size; n as usize])?;
        let id = a.add_doc(s)?;
        let (_, s) = a.session_ref(args)?;
        Ok(summary(id, s))
    },
};

pub static LIST: Tool = Tool {
    name: "doc_list",
    title: "List open documents",
    description: "The open documents with their ids, paths, page counts and unsaved state.",
    read_only: true,
    destructive: false,
    schema: || schema_nodoc(json!({}), &[]),
    run: |a, _| {
        let docs: Vec<Value> = a.docs().iter().map(|(id, s)| summary(*id, s)).collect();
        Ok(json!({ "documents": docs }))
    },
};

pub static INFO: Tool = Tool {
    name: "doc_info",
    title: "Inspect a document",
    description: "Pages (size, rotation, label, scales and viewports), markup counts by kind and by subject, and the undo state.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (id, s) = a.session_ref(args)?;
        let pages: Vec<Value> = s
            .doc()
            .pages
            .iter()
            .enumerate()
            .map(|(i, p)| {
                json!({
                    "page": i + 1,
                    "label": p.label,
                    "width": p.media.width(),
                    "height": p.media.height(),
                    "media_box": p.media.as_array(),
                    "rotate": p.rotate,
                    "markups": s.doc().markups_on(i).count(),
                    "viewports": p.viewports.iter().enumerate().map(|(k, v)| json!({
                        "index": k + 1,
                        "name": v.name,
                        "bbox": v.bbox.as_array(),
                        "scale": v.scale.ratio,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        let mut subjects: BTreeMap<&str, usize> = BTreeMap::new();
        for m in &s.doc().markups {
            *kinds.entry(m.kind.name()).or_default() += 1;
            *subjects.entry(m.subject.as_str()).or_default() += 1;
        }
        let mut out = summary(id, s);
        out["page_list"] = json!(pages);
        out["kinds"] = json!(kinds);
        out["subjects"] = json!(subjects);
        out["columns"] = json!(
            s.doc()
                .columns
                .iter()
                .map(|c| json!({ "id": c.id, "name": c.name, "type": c.kind.name() }))
                .collect::<Vec<_>>()
        );
        Ok(out)
    },
};

pub static SAVE: Tool = Tool {
    name: "doc_save",
    title: "Save a document",
    description: "Save to the document's own file, or to `path` (save as). The write is atomic (a temporary file, then a rename). Incremental by default; full: true rewrites the whole file.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "path": path_arg("Save as this file instead"),
                "full": { "type": "boolean", "description": "Rewrite the whole file (smaller, drops old revisions)." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let full = args.bool_or("full", false)?;
        let target = match args.opt_str("path")? {
            Some(p) => Some(a.resolve(p, true)?),
            None => None,
        };
        // Saving in place under a root: the document's own path must be inside it too.
        let own = a.session_ref(args)?.1.path().display().to_string();
        let target = match target {
            Some(t) => t,
            None => a.resolve(&own, true)?,
        };
        let (id, s) = a.session(args)?;
        s.save_as(&target, full)?;
        Ok(summary(id, s))
    },
};

pub static CLOSE: Tool = Tool {
    name: "doc_close",
    title: "Close a document",
    description: "Close a document. Refuses when it has unsaved changes unless discard_changes is true.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "discard_changes": { "type": "boolean", "description": "Close even with unsaved changes." } }),
            &[],
        )
    },
    run: |a, args| {
        let discard = args.bool_or("discard_changes", false)?;
        let (id, s) = a.session_ref(args)?;
        if s.is_dirty() && !discard {
            return Err(failed(format!(
                "document {id} has unsaved changes; save it (doc_save) or pass discard_changes: true"
            )));
        }
        a.remove_doc(id);
        Ok(json!({ "closed": id }))
    },
};
