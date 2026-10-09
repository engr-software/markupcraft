//! Markup exchange as XFDF / FDF.

use serde_json::json;

use super::{Tool, path_arg, schema};
use crate::summary;

pub static EXPORT: Tool = Tool {
    name: "xfdf_export",
    title: "Export markups (XFDF)",
    description: "Write every markup (unsaved ones included) to `out` as XFDF, or FDF when `out` ends in .fdf (atomic).",
    read_only: false,
    destructive: true,
    schema: || schema(json!({ "out": path_arg("The XFDF or FDF file") }), &["out"]),
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let (doc, s) = a.session_ref(args)?;
        let n = s.export_xfdf_file(&out)?;
        Ok(json!({ "doc": doc, "out": out.display().to_string(), "bytes": n, "markups": s.doc().markups.len() }))
    },
};

pub static IMPORT: Tool = Tool {
    name: "xfdf_import",
    title: "Import markups (XFDF)",
    description: "Import markups from an XFDF or FDF file. A markup whose id already exists on its page is replaced; the others are added. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "path": path_arg("The XFDF or FDF file") }), &["path"]),
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let (doc, s) = a.session(args)?;
        let r = s.import_xfdf_file(&path)?;
        Ok(json!({
            "imported": r.imported,
            "markups_before": r.markups_before,
            "markups_after": r.markups_after,
            "document": summary(doc, s),
        }))
    },
};
