//! Batch tools over many PDFs (Batch Link, Batch Summary, Slip Sheet, batch apply) and Sets.

use std::path::PathBuf;

use markupcraft_engine::Session;
use markupcraft_engine::batch::{
    BatchLinkOptions, DrawingSet, SetSort, SlipSheetOptions, batch_link, batch_summary_csv, load_set, save_set,
    set_sheets,
};
use serde_json::{Map, Value, json};

use super::{Tool, path_arg, schema, schema_nodoc};
use crate::{Args, Automation, Result, bad_args, failed, summary};

/// Tools batch_apply may run on each file.
const APPLY_TOOLS: &[&str] = &[
    "markup_flatten",
    "header_footer_add",
    "watermark_add",
    "bates_add",
    "marks_remove",
    "stamp_add",
    "markup_paste",
    "markup_add",
    "layer_assign",
    "legend_add",
    "legend_update",
    "page_label_set",
];

fn files_args() -> Value {
    json!({
        "files": { "type": "array", "items": { "type": "string" }, "description": "PDF files (relative to --root when one is set)." },
        "set": path_arg("A set file (.pcset) naming the files")
    })
}

fn with(mut base: Value, more: Value) -> Value {
    if let (Some(b), Some(m)) = (base.as_object_mut(), more.as_object()) {
        for (k, v) in m {
            b.insert(k.clone(), v.clone());
        }
    }
    base
}

/// The files `files` or `set` names.
fn files_of(a: &Automation, args: &Args) -> Result<Vec<PathBuf>> {
    match (args.opt_strings("files")?, args.opt_str("set")?) {
        (Some(f), None) if !f.is_empty() => f.iter().map(|p| a.resolve(p, false)).collect(),
        (None, Some(set)) => {
            let set = load_set(&a.resolve(set, false)?)?;
            set.files
                .iter()
                .map(|p| a.resolve(&p.display().to_string(), false))
                .collect()
        }
        _ => Err(bad_args(format!("{}: give files or set", args.tool()))),
    }
}

pub static SET_SAVE: Tool = Tool {
    name: "set_save",
    title: "Save a set",
    description: "Write a Set: a named list of PDFs navigated as one drawing set without merging them (JSON .pcset; paths relative to the set file's folder when inside it).",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "path": path_arg("The set file to write"),
                "name": { "type": "string" },
                "files": { "type": "array", "items": { "type": "string" } }
            }),
            &["path", "files"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, true)?;
        let files: Vec<PathBuf> = args
            .opt_strings("files")?
            .unwrap_or_default()
            .iter()
            .map(|f| a.resolve(f, false))
            .collect::<Result<_>>()?;
        let set = DrawingSet {
            name: args.opt_string("name")?.unwrap_or_default(),
            files,
        };
        save_set(&path, &set)?;
        Ok(json!({ "path": path.display().to_string(), "files": set.files.len() }))
    },
};

pub static SET_SHEETS: Tool = Tool {
    name: "set_sheets",
    title: "Sheets of a set",
    description: "Every sheet of a set (file, page, label), sorted by file order (default), label, or file then label. `open` also opens every file (doc ids in the result), so the set can be worked as one.",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({
                "set": path_arg("The set file"),
                "sort": { "type": "string", "enum": ["file", "label", "file_label"] },
                "open": { "type": "boolean" }
            }),
            &["set"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("set")?, false)?;
        let mut set = load_set(&path)?;
        for f in &mut set.files {
            *f = a.resolve(&f.display().to_string(), false)?;
        }
        let sort = match args.opt_str("sort")?.unwrap_or("file") {
            "file" => SetSort::FileOrder,
            "label" => SetSort::Label,
            "file_label" => SetSort::FileThenLabel,
            other => return Err(bad_args(format!("set_sheets: unknown sort {other:?}"))),
        };
        let (sheets, errors) = set_sheets(&set, sort);
        let mut docs = Vec::new();
        if args.bool_or("open", false)? {
            for f in &set.files {
                if let Ok(s) = Session::open(f) {
                    docs.push(a.add_doc(s)?);
                }
            }
        }
        let list: Vec<Value> = sheets
            .iter()
            .map(|s| {
                json!({
                    "file": set.files.get(s.file).map(|f| f.display().to_string()),
                    "doc": docs.get(s.file),
                    "page": s.page + 1,
                    "label": s.label,
                })
            })
            .collect();
        Ok(json!({ "name": set.name, "sheets": list, "docs": docs, "errors": errors }))
    },
};

pub static SUMMARY: Tool = Tool {
    name: "batch_summary",
    title: "Batch Summary",
    description: "The Markups List of many PDFs as one CSV with a File column (written to `out`, or returned).",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            with(
                files_args(),
                json!({ "out": path_arg("The CSV to write"), "measurements_only": { "type": "boolean" } }),
            ),
            &[],
        )
    },
    run: |a, args| {
        let files = files_of(a, args)?;
        let (csv, n, errors) = batch_summary_csv(&files, args.bool_or("measurements_only", false)?);
        match args.opt_str("out")? {
            Some(out) => {
                let out = a.resolve(out, true)?;
                let tmp = out.with_extension("csv.markupcraft-tmp");
                std::fs::write(&tmp, csv.as_bytes()).map_err(failed)?;
                std::fs::rename(&tmp, &out).map_err(failed)?;
                Ok(json!({ "markups": n, "path": out.display().to_string(), "errors": errors }))
            }
            None => Ok(json!({ "markups": n, "csv": csv, "errors": errors })),
        }
    },
};

pub static LINK: Tool = Tool {
    name: "batch_link",
    title: "Batch Link",
    description: "Batch Link: every page label (sheet number) in the files is a target; wherever a page's text shows another page's label, a link to that page is added (same file: a page link; another file: a link to its page). Files are saved in place; places that already have a link are skipped, so it can be run again.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            with(
                files_args(),
                json!({
                    "match_case": { "type": "boolean" },
                    "border_width": { "type": "number", "minimum": 0, "maximum": 12 },
                    "color": { "type": ["string", "array"] },
                    "padding": { "type": "number", "minimum": 0, "maximum": 36 }
                }),
            ),
            &[],
        )
    },
    run: |a, args| {
        let files = files_of(a, args)?;
        check_closed(a, &files)?;
        let mut o = BatchLinkOptions {
            match_case: args.bool_or("match_case", false)?,
            ..Default::default()
        };
        if let Some(w) = args.opt_num("border_width")? {
            o.width = w;
        }
        if let Some(c) = args.opt_color("color")? {
            o.color = c;
        }
        if let Some(p) = args.opt_num("padding")? {
            o.padding = p;
        }
        let r = batch_link(&files, &o)?;
        Ok(json!({
            "links": r.links,
            "existing": r.existing,
            "files": r.files.iter().map(|(f, n)| json!({ "file": f.display().to_string(), "links": n })).collect::<Vec<_>>(),
            "errors": r.errors,
        }))
    },
};

/// Files changed in place must not be open here (their unsaved state would be overwritten).
fn check_closed(a: &Automation, files: &[PathBuf]) -> Result<()> {
    for f in files {
        if a.docs().iter().any(|(_, s)| same_file(s.path(), f)) {
            return Err(failed(format!(
                "{} is open; save and close it first (doc_close)",
                f.display()
            )));
        }
    }
    Ok(())
}

fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

pub static SLIP: Tool = Tool {
    name: "slip_sheet",
    title: "Slip Sheet",
    description: "Slip Sheet: replace this document's pages with the revised pages of `new_file` whose page labels match (by the whole label, or the part before `number_filter`, e.g. \" - \"); the replaced pages keep their markups. New sheets that match nothing are appended with their labels (`append_unmatched`, default true). Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "new_file": path_arg("The revised set"),
                "number_filter": { "type": "string" },
                "match_case": { "type": "boolean" },
                "append_unmatched": { "type": "boolean" }
            }),
            &["new_file"],
        )
    },
    run: |a, args| {
        let new = a.resolve(args.str("new_file")?, false)?;
        let o = SlipSheetOptions {
            number_filter: args.opt_string("number_filter")?.unwrap_or_default(),
            match_case: args.bool_or("match_case", false)?,
            append_unmatched: args.bool_or("append_unmatched", true)?,
        };
        let (doc, s) = a.session(args)?;
        let r = s.slip_sheet(&new, &o)?;
        Ok(json!({
            "matched": r.matched.iter().map(|p| json!({
                "label": p.label, "old_page": p.old_page + 1, "new_page": p.new_page + 1, "markups": p.markups,
            })).collect::<Vec<_>>(),
            "unmatched_old": r.unmatched_old.iter().map(|p| p + 1).collect::<Vec<_>>(),
            "unmatched_new": r.unmatched_new.iter().map(|p| p + 1).collect::<Vec<_>>(),
            "appended": r.appended,
            "document": summary(doc, s),
        }))
    },
};

pub static APPLY: Tool = Tool {
    name: "batch_apply",
    title: "Batch apply",
    description: "Run document tools on many PDFs: `operations` is a list of {\"tool\": name, \"args\": {...}} run in order on each file (markup_flatten, header_footer_add, watermark_add, bates_add, marks_remove, stamp_add, markup_paste (paste what markup_copy copied), markup_add, layer_assign, legend_add, legend_update, page_label_set). Each file is saved in place, or into `out_dir` under its own name. A file whose operations fail is left untouched and reported.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            with(
                files_args(),
                json!({
                    "operations": { "type": "array", "items": { "type": "object" } },
                    "out_dir": path_arg("Write the results here instead of in place"),
                    "full": { "type": "boolean", "description": "Rewrite each file whole." }
                }),
            ),
            &["operations"],
        )
    },
    run: |a, args| {
        let files = files_of(a, args)?;
        let ops: Vec<(String, Value)> = args
            .get("operations")
            .and_then(Value::as_array)
            .ok_or_else(|| bad_args("batch_apply: operations is a list"))?
            .iter()
            .map(|o| {
                let tool = o.get("tool").and_then(Value::as_str).unwrap_or_default().to_string();
                if !APPLY_TOOLS.contains(&tool.as_str()) {
                    return Err(bad_args(format!(
                        "batch_apply: {tool:?} cannot run in a batch (one of {})",
                        APPLY_TOOLS.join(", ")
                    )));
                }
                let args = o.get("args").cloned().unwrap_or_else(|| json!({}));
                if !args.is_object() || args.get("doc").is_some() {
                    return Err(bad_args("batch_apply: each operation's args is an object without doc"));
                }
                Ok((tool, args))
            })
            .collect::<Result<_>>()?;
        if ops.is_empty() {
            return Err(bad_args("batch_apply: no operations"));
        }
        let out_dir = match args.opt_str("out_dir")? {
            Some(d) => Some(a.resolve(d, true)?),
            None => None,
        };
        if let Some(d) = &out_dir {
            std::fs::create_dir_all(d).map_err(failed)?;
        }
        let full = args.bool_or("full", false)?;
        if out_dir.is_none() {
            check_closed(a, &files)?;
        }
        let mut results = Vec::new();
        let mut ok = 0;
        for f in &files {
            let res = (|| -> Result<Value> {
                let s = Session::open(f)?;
                let id = a.add_doc(s)?;
                let run = (|| -> Result<Vec<Value>> {
                    let mut outs = Vec::new();
                    for (tool, targs) in &ops {
                        let mut m: Map<String, Value> = targs.as_object().cloned().unwrap_or_default();
                        m.insert("doc".into(), json!(id));
                        let v = a
                            .call(tool, &Value::Object(m))
                            .map_err(|e| failed(format!("{tool}: {e}")))?;
                        outs.push(v);
                    }
                    let target = match &out_dir {
                        Some(d) => d.join(f.file_name().ok_or_else(|| failed("a file has no name"))?),
                        None => f.clone(),
                    };
                    let s = a
                        .docs
                        .iter_mut()
                        .find(|(d, _)| *d == id)
                        .map(|(_, s)| s)
                        .ok_or_else(|| failed("document vanished"))?;
                    s.save_as(&target, full)?;
                    Ok(outs)
                })();
                a.remove_doc(id);
                let outs = run?;
                Ok(json!({ "operations": outs.len() }))
            })();
            match res {
                Ok(v) => {
                    ok += 1;
                    results.push(json!({ "file": f.display().to_string(), "ok": true, "result": v }));
                }
                Err(e) => results.push(json!({ "file": f.display().to_string(), "ok": false, "error": e.to_string() })),
            }
        }
        Ok(json!({ "files": results, "done": ok, "failed": files.len() - ok }))
    },
};
