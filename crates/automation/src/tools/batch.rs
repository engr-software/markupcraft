//! Batch tools over many PDFs (Batch Link, Batch Summary, Slip Sheet, batch apply) and Sets.

use std::path::PathBuf;

use markupcraft_engine::Session;
use markupcraft_engine::batch::{
    BatchLinkOptions, BatchLinkRun, DrawingSet, HighlightStyle, LinkTerms, SetSort, TermDest, TermTarget, batch_link,
    batch_link_terms, batch_summary_csv, link_terms_csv, link_terms_from_csv, link_terms_to_csv, load_link_run,
    load_set, save_link_run, save_set, set_sheets,
};
use serde_json::{Map, Value, json};

use super::{Tool, path_arg, rect_arg, schema_nodoc};
use crate::{Args, Automation, Result, bad_args, failed};

/// Tools batch_apply may run on each file.
const APPLY_TOOLS: &[&str] = &[
    "markup_flatten",
    "header_footer_add",
    "watermark_add",
    "bates_add",
    "marks_remove",
    "stamp_add",
    "stamp_apply",
    "markup_paste",
    "markup_add",
    "layer_assign",
    "legend_add",
    "legend_update",
    "page_label_set",
    // more batch processes: unflatten, repair, reduce, colour, PDF/A, security, OCR, rotate,
    // crop, page size, redaction, form flattening, properties
    "markup_unflatten",
    "doc_repair",
    "doc_reduce_size",
    "doc_color_process",
    "doc_pdfa",
    "security_set",
    "security_remove",
    "ocr_pages",
    "page_rotate",
    "page_crop",
    "page_resize",
    "redact_apply",
    "form_flatten",
    "doc_properties_set",
];

fn files_args() -> Value {
    json!({
        "files": { "type": "array", "items": { "type": "string" }, "description": "PDF files (relative to --root when one is set)." },
        "set": path_arg("A set file (.pcset) naming the files"),
        "folder": path_arg("A folder: its PDFs (batch_summary only)"),
        "recursive": { "type": "boolean", "description": "With folder: subfolders too." }
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
            ..Default::default()
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
    description: "The Markups List of many PDFs (`files`, a `set`, or every PDF in a `folder`, `recursive` for subfolders) as one table with a File column: CSV (written to `out`, or returned) or an Excel workbook when `out` ends in .xlsx.",
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
        let files = match args.opt_str("folder")? {
            Some(dir) => {
                let dir = a.resolve(dir, false)?;
                markupcraft_engine::search_more::folder_pdfs(&dir, args.bool_or("recursive", false)?)?
            }
            None => files_of(a, args)?,
        };
        let mo = args.bool_or("measurements_only", false)?;
        let out_path = args.opt_str("out")?;
        if let Some(out) = out_path.filter(|o| o.to_ascii_lowercase().ends_with(".xlsx")) {
            let out = a.resolve(out, true)?;
            let (bytes, n, errors) = markupcraft_engine::batch::batch_summary_xlsx(&files, mo);
            let tmp = out.with_extension("xlsx.markupcraft-tmp");
            std::fs::write(&tmp, &bytes).map_err(failed)?;
            std::fs::rename(&tmp, &out).map_err(failed)?;
            return Ok(
                json!({ "markups": n, "files": files.len(), "path": out.display().to_string(), "errors": errors }),
            );
        }
        let (csv, n, errors) = batch_summary_csv(&files, mo);
        match out_path {
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
    description: "Batch Link: every page label (sheet number) in the files is a target; wherever a page's text shows another page's label, a link to that page is added (same file: a page link; another file: a link to its page). Terms can instead be file names, a region's text, or a term table (`targets`: each to a page of a file, a file, a named Place in a file, or a web URL; `terms_csv` reads Term,File,Page,Place,URL). Options: relative or full paths, border, a highlight over each link (highlight_style fill / outline / highlight, flatten_highlight), places with a link already are skipped, replaced (replace_existing) or kept beside (add_overlapping). `export_terms` writes the term table as CSV; `save_run` saves the files and options as XML (or into a .pcset Set file) and `run_file` runs a saved one; `run: false` only exports / saves. Files are saved in place; the report counts links created and deleted, pages skipped (no text) and files not opened.",
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
                    "padding": { "type": "number", "minimum": 0, "maximum": 36 },
                    "terms": { "type": "string", "enum": ["page_labels", "file_names", "region", "custom"], "description": "Where the search terms come from (default page_labels)." },
                    "region": rect_arg("For terms=region: the box of each page whose text is that page's term"),
                    "custom": { "type": "array", "items": { "type": "array" }, "description": "For terms=custom: [[term, file index (0-based), page (1-based)], ...]." },
                    "terms_csv": path_arg("For terms=custom: a CSV of term,file name,page lines"),
                    "filter_char": { "type": "string", "description": "Cut each term at this character." },
                    "keep_start": { "type": "boolean", "description": "Keep the text before filter_char (default) or after it." },
                    "full_paths": { "type": "boolean" },
                    "highlight": { "type": ["string", "array"], "description": "A highlight rectangle of this colour over each new link." },
                    "replace_existing": { "type": "boolean" },
                    "add_overlapping": { "type": "boolean", "description": "Add the new link beside an existing one." },
                    "highlight_style": { "type": "string", "enum": ["fill", "outline", "highlight"] },
                    "flatten_highlight": { "type": "boolean" },
                    "targets": { "type": "array", "items": { "type": "object" }, "description": "terms=targets: [{term, file (from 1), page (from 1) | place | (neither: the file)} or {term, url}]." },
                    "export_terms": path_arg("Write the term table here as CSV"),
                    "save_run": path_arg("Save the run (files and options) as .xml, or into a .pcset Set file"),
                    "run_file": path_arg("Run a saved run (.xml or .pcset)"),
                    "run": { "type": "boolean", "description": "false: only export_terms / save_run (default true)." }
                }),
            ),
            &[],
        )
    },
    run: |a, args| {
        let loaded = match args.opt_str("run_file")? {
            Some(f) => Some(load_link_run(&a.resolve(f, false)?)?),
            None => None,
        };
        let files = match &loaded {
            Some(l) if args.get("files").is_none() && args.get("set").is_none() => l
                .files
                .iter()
                .map(|p| a.resolve(&p.display().to_string(), false))
                .collect::<Result<Vec<_>>>()?,
            _ => files_of(a, args)?,
        };
        let do_run = args.bool_or("run", true)?;
        if do_run {
            check_closed(a, &files)?;
        }
        let mut o = match &loaded {
            Some(l) => l.options.clone(),
            None => BatchLinkOptions::default(),
        };
        if let Some(v) = args.opt_bool("match_case")? {
            o.match_case = v;
        }
        if let Some(w) = args.opt_num("border_width")? {
            o.width = w;
        }
        if let Some(c) = args.opt_color("color")? {
            o.color = c;
        }
        if let Some(p) = args.opt_num("padding")? {
            o.padding = p;
        }
        let default_terms = if loaded.is_some() { "saved" } else { "page_labels" };
        o.terms = match args.opt_str("terms")?.unwrap_or(default_terms) {
            "saved" => o.terms.clone(),
            "page_labels" => LinkTerms::PageLabels,
            "targets" => {
                let mut list = Vec::new();
                if let Some(csv) = args.opt_str("terms_csv")? {
                    let text = std::fs::read_to_string(a.resolve(csv, false)?).map_err(failed)?;
                    list.extend(link_terms_from_csv(&text, &files));
                }
                for t in args.get("targets").and_then(Value::as_array).into_iter().flatten() {
                    list.push(term_target(t, files.len())?);
                }
                if list.is_empty() {
                    return Err(bad_args("terms=targets needs targets or terms_csv"));
                }
                LinkTerms::Targets(list)
            }
            "file_names" => LinkTerms::FileNames,
            "region" => LinkTerms::Region(
                args.opt_rect("region")?
                    .ok_or_else(|| bad_args("terms=region needs region"))?,
            ),
            "custom" => {
                let mut list = Vec::new();
                if let Some(csv) = args.opt_str("terms_csv")? {
                    let text = std::fs::read_to_string(a.resolve(csv, false)?).map_err(failed)?;
                    list.extend(link_terms_csv(&text, &files));
                }
                for t in args.get("custom").and_then(Value::as_array).into_iter().flatten() {
                    let row = t
                        .as_array()
                        .ok_or_else(|| bad_args("custom is [[term, file index, page], ...]"))?;
                    let term = row
                        .first()
                        .and_then(Value::as_str)
                        .ok_or_else(|| bad_args("custom terms are strings"))?;
                    let fi = row.get(1).and_then(Value::as_u64).unwrap_or(0) as usize;
                    let page = row.get(2).and_then(Value::as_u64).unwrap_or(1).max(1) as usize - 1;
                    if fi >= files.len() {
                        return Err(bad_args(format!(
                            "custom term {term:?}: file index {fi} is not one of the files"
                        )));
                    }
                    list.push((term.to_string(), fi, page));
                }
                if list.is_empty() {
                    return Err(bad_args("terms=custom needs custom or terms_csv"));
                }
                LinkTerms::Custom(list)
            }
            other => {
                return Err(bad_args(format!(
                    "terms is page_labels, file_names, region, custom or targets, not {other:?}"
                )));
            }
        };
        if let Some(c) = args.opt_str("filter_char")? {
            let mut it = c.chars();
            match (it.next(), it.next()) {
                (Some(ch), None) => o.filter_char = Some(ch),
                (None, _) => {}
                _ => return Err(bad_args("filter_char is one character")),
            }
        }
        for (k, slot) in [
            ("keep_start", &mut o.keep_start),
            ("full_paths", &mut o.full_paths),
            ("replace_existing", &mut o.replace_existing),
            ("add_overlapping", &mut o.add_overlapping),
            ("flatten_highlight", &mut o.flatten_highlight),
        ] {
            if let Some(v) = args.opt_bool(k)? {
                *slot = v;
            }
        }
        if let Some(c) = args.opt_color("highlight")? {
            o.highlight = Some(c);
        }
        if let Some(st) = args.opt_str("highlight_style")? {
            o.highlight_style = HighlightStyle::from_name(st)
                .ok_or_else(|| bad_args("highlight_style is fill, outline or highlight"))?;
            if o.highlight.is_none() {
                o.highlight = Some(markupcraft_model::Color::rgb(1.0, 1.0, 0.0));
            }
        }
        let mut out = Map::new();
        if let Some(p) = args.opt_str("export_terms")? {
            let p = a.resolve(p, true)?;
            let (terms, _) = batch_link_terms(&files, &o)?;
            std::fs::write(&p, link_terms_to_csv(&terms, &files)).map_err(failed)?;
            out.insert("terms_exported".into(), json!(terms.len()));
        }
        if let Some(p) = args.opt_str("save_run")? {
            let p = a.resolve(p, true)?;
            save_link_run(
                &p,
                &BatchLinkRun {
                    files: files.clone(),
                    options: o.clone(),
                },
            )?;
            out.insert("saved".into(), json!(p.display().to_string()));
        }
        if !do_run {
            return Ok(Value::Object(out));
        }
        let r = batch_link(&files, &o)?;
        let mut v = json!({
            "links": r.links,
            "existing": r.existing,
            "deleted": r.deleted,
            "skipped_pages": r.skipped_pages,
            "highlights": r.highlights,
            "files_not_opened": r.files_not_opened,
            "files": r.files.iter().map(|(f, n)| json!({ "file": f.display().to_string(), "links": n })).collect::<Vec<_>>(),
            "errors": r.errors,
        });
        if let Some(m) = v.as_object_mut() {
            m.extend(out);
        }
        Ok(v)
    },
};

/// One `targets` entry of batch_link.
fn term_target(t: &Value, files: usize) -> Result<TermTarget> {
    let term = t
        .get("term")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| bad_args("each target has a term"))?
        .to_string();
    if let Some(u) = t.get("url").and_then(Value::as_str) {
        return Ok(TermTarget {
            term,
            dest: TermDest::Url(u.to_string()),
        });
    }
    let file = t
        .get("file")
        .and_then(Value::as_u64)
        .and_then(|f| usize::try_from(f).ok())
        .and_then(|f| f.checked_sub(1))
        .filter(|f| *f < files)
        .ok_or_else(|| bad_args(format!("target {term:?}: file is one of the files, counted from 1")))?;
    let dest = match (
        t.get("place").and_then(Value::as_str),
        t.get("page").and_then(Value::as_u64),
    ) {
        (Some(name), _) => TermDest::Place {
            file,
            name: name.to_string(),
        },
        (None, Some(p)) => TermDest::Page {
            file,
            page: usize::try_from(p).unwrap_or(1).max(1) - 1,
        },
        (None, None) => TermDest::File { file },
    };
    Ok(TermTarget { term, dest })
}

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

pub static APPLY: Tool = Tool {
    name: "batch_apply",
    title: "Batch apply",
    description: "Run document tools on many PDFs: `operations` is a list of {\"tool\": name, \"args\": {...}} run in order on each file (markup_flatten, header_footer_add, watermark_add, bates_add, marks_remove, stamp_add, markup_paste (paste what markup_copy copied), markup_add, layer_assign, legend_add, legend_update, page_label_set, markup_unflatten, doc_repair, doc_reduce_size, doc_color_process, doc_pdfa, security_set, security_remove, ocr_pages, page_rotate, page_crop, page_resize, redact_apply, form_flatten, doc_properties_set). Each file is saved in place, or into `out_dir` under its own name. A file whose operations fail is left untouched and reported.",
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
