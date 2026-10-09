//! More layer tools (hierarchy, configurations, page layers, export state, print/export
//! preview, import and export), Set tools (tags, categories, revisions, publish, print) and
//! printing tools (the system's printers, Batch Print).

use std::path::PathBuf;

use markupcraft_engine::batch::{SetSort, load_set, save_set};
use markupcraft_engine::layers_more::LayerPreview;
use markupcraft_engine::printing::{PrintJob, batch_print, list_printers};
use markupcraft_engine::printout::PrintLayout;
use markupcraft_engine::sets_more::{
    CategoryMode, categorize, default_categories, drawing_log, print_set, publish_combined, publish_package, revisions,
    tagged_sheets,
};
use serde_json::{Value, json};

use super::{Tool, page_arg, path_arg, schema, schema_nodoc};
use crate::{Result, bad_args, summary};

pub static LAYER_NEST: Tool = Tool {
    name: "layer_nest",
    title: "Arrange layers",
    description: "Layer hierarchy: move layer `name` under `parent` (omit for the top level) at `index` (1-based; default last). Returns the tree (name, depth, parent). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "name": { "type": "string" }, "parent": { "type": "string" }, "index": { "type": "integer", "minimum": 1 } }),
            &["name"],
        )
    },
    run: |a, args| {
        let name = args.str("name")?.to_string();
        let parent = args.opt_string("parent")?;
        let index = args.opt_u64("index")?.map(|i| (i as usize).saturating_sub(1));
        let (doc, s) = a.session(args)?;
        s.nest_layer(&name, parent.as_deref(), index)?;
        let tree: Vec<Value> = s
            .layer_tree()
            .iter()
            .map(|n| json!({ "name": n.name, "depth": n.depth, "parent": n.parent }))
            .collect();
        Ok(json!({ "tree": tree, "document": summary(doc, s) }))
    },
};

pub static LAYER_CONFIG: Tool = Tool {
    name: "layer_config",
    title: "Layer configurations",
    description: "Saved visibility presets: list (default), save the current visibility as `name`, apply `name`, or delete `name`. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "action": { "type": "string", "enum": ["list", "save", "apply", "delete"] }, "name": { "type": "string" } }),
            &[],
        )
    },
    run: |a, args| {
        let action = args.opt_str("action")?.unwrap_or("list").to_string();
        let name = args.opt_string("name")?;
        let (doc, s) = a.session(args)?;
        let need = || name.clone().ok_or_else(|| bad_args("give the configuration's name"));
        match action.as_str() {
            "list" => {}
            "save" => s.save_layer_config(&need()?)?,
            "apply" => s.apply_layer_config(&need()?)?,
            "delete" => s.delete_layer_config(&need()?)?,
            o => return Err(bad_args(format!("action {o:?}"))),
        }
        let layers: Vec<Value> = s
            .layers()
            .iter()
            .map(|l| json!({ "name": l.name, "visible": l.visible }))
            .collect();
        Ok(json!({ "configs": s.layer_configs(), "layers": layers, "document": summary(doc, s) }))
    },
};

pub static LAYER_VIEW: Tool = Tool {
    name: "layer_view",
    title: "Layer view options",
    description: "page: the layers used on that page (Show layers on page only), sorted by name; preview print|export: show only the layers set to print or export (end: back to the visibility before); export_state: with `name` and `on`, set a layer's export state.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("whose layers to list"),
                "preview": { "type": "string", "enum": ["print", "export", "end"] },
                "name": { "type": "string" },
                "export_state": { "type": "boolean" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let page = args.opt_page("page")?;
        let preview = args.opt_str("preview")?.map(str::to_string);
        let name = args.opt_string("name")?;
        let export = args.opt_bool("export_state")?;
        let (doc, s) = a.session(args)?;
        if let (Some(n), Some(on)) = (&name, export) {
            s.set_layer_export(n, on)?;
        }
        match preview.as_deref() {
            Some("print") => s.preview_layers(LayerPreview::Print)?,
            Some("export") => s.preview_layers(LayerPreview::Export)?,
            Some("end") => s.end_layer_preview()?,
            Some(o) => return Err(bad_args(format!("preview {o:?}"))),
            None => {}
        }
        let on_page = match page {
            Some(p) => Some(s.layers_on_page(p)?),
            None => None,
        };
        let layers: Vec<Value> = s
            .layers()
            .iter()
            .map(|l| json!({ "name": l.name, "visible": l.visible, "print": l.print, "export": s.layer_export(&l.name).unwrap_or(true) }))
            .collect();
        Ok(json!({ "on_page": on_page, "layers": layers, "document": summary(doc, s) }))
    },
};

pub static LAYER_IMPORT: Tool = Tool {
    name: "layer_import",
    title: "Import a layer",
    description: "Bring page `src_page` of the PDF at `path` in as layer `name` on `page` (lower-left corners aligned). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "path": path_arg("The PDF to import"),
                "src_page": page_arg("of that PDF (default 1)"),
                "page": page_arg("to draw it on"),
                "name": { "type": "string" }
            }),
            &["path", "page", "name"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let src_page = args.opt_page("src_page")?.unwrap_or(0);
        let page = args.page("page")?;
        let name = args.str("name")?.to_string();
        let (doc, s) = a.session(args)?;
        s.import_layer(&path, src_page, page, &name)?;
        Ok(json!({ "document": summary(doc, s) }))
    },
};

pub static LAYER_EXPORT: Tool = Tool {
    name: "layer_export",
    title: "Export a layer",
    description: "Write a PDF at `out` holding only layer `name`: its page content and its markups.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "name": { "type": "string" }, "out": path_arg("The PDF to write") }),
            &["name", "out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let name = args.str("name")?.to_string();
        let (doc, s) = a.session_ref(args)?;
        let n = s.export_layer(&name, &out)?;
        Ok(json!({ "doc": doc, "out": out.display().to_string(), "pages": n }))
    },
};

fn set_path(a: &crate::Automation, args: &crate::Args) -> Result<PathBuf> {
    a.resolve(args.str("set")?, false)
}

pub static SET_TAGS: Tool = Tool {
    name: "set_tags",
    title: "Set sheets and tags",
    description: "Every sheet of the Set (`set`, a .pcset) with its tags: Sheet Number, Revision (from the file name), Discipline and Sheet Type (from the sheet number), and custom tags. tag: {\"sheet\": \"<file name>#<page>\", \"name\": ..., \"value\": ...} sets a custom tag (\"\" removes it) and saves the set. categories: off | file_name | sheet_number groups the sheets; revisions: true lists each sheet's versions (latest last) with an optional wildcard `filter`.",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({
                "set": path_arg("The set file"),
                "tag": { "type": "object" },
                "categories": { "type": "string", "enum": ["off", "file_name", "sheet_number"] },
                "revisions": { "type": "boolean" },
                "filter": { "type": "string" }
            }),
            &["set"],
        )
    },
    run: |a, args| {
        let path = set_path(a, args)?;
        let mut set = load_set(&path)?;
        if let Some(t) = args.get("tag") {
            let get = |k: &str| t.get(k).and_then(Value::as_str).map(str::to_string);
            let sheet = get("sheet").ok_or_else(|| bad_args("tag: sheet is \"<file name>#<page>\""))?;
            let name = get("name").ok_or_else(|| bad_args("tag: name"))?;
            let value = get("value").unwrap_or_default();
            let entry = set.tags.entry(sheet.clone()).or_default();
            if value.is_empty() {
                entry.remove(&name);
            } else {
                entry.insert(name, value);
            }
            if entry.is_empty() {
                set.tags.remove(&sheet);
            }
            save_set(&path, &set)?;
        }
        let rules = default_categories();
        let (sheets, errors) = tagged_sheets(&set, SetSort::FileOrder, &rules);
        let sheet_json = |i: usize| {
            sheets.get(i).map(|t| {
                json!({
                    "file": t.file.display().to_string(),
                    "page": t.sheet.page + 1,
                    "label": t.sheet.label,
                    "tags": t.tags,
                })
            })
        };
        let mut out = json!({
            "sheets": (0..sheets.len()).filter_map(sheet_json).collect::<Vec<_>>(),
            "errors": errors,
        });
        if let Some(c) = args.opt_str("categories")? {
            let mode = match c {
                "file_name" => CategoryMode::FileName,
                "sheet_number" => CategoryMode::SheetNumber,
                _ => CategoryMode::Off,
            };
            out["categories"] = Value::Array(
                categorize(&sheets, mode, &rules)
                    .iter()
                    .map(|(k, list)| json!({ "category": k, "sheets": list.iter().map(|t| t.tags.get("Sheet Number").cloned().unwrap_or_default()).collect::<Vec<_>>() }))
                    .collect(),
            );
        }
        if args.bool_or("revisions", false)? {
            let filter = args.opt_string("filter")?.unwrap_or_default();
            out["revisions"] = Value::Array(
                revisions(&sheets, &filter)
                    .iter()
                    .map(|g| {
                        json!({
                            "sheet": g.key,
                            "versions": g.versions.iter().filter_map(|i| sheets.get(*i).map(|t| t.file.display().to_string())).collect::<Vec<_>>(),
                            "latest": g.latest().and_then(|i| sheets.get(i)).map(|t| t.file.display().to_string()),
                        })
                    })
                    .collect(),
            );
        }
        Ok(out)
    },
};

pub static SET_PUBLISH: Tool = Tool {
    name: "set_publish",
    title: "Publish a Set",
    description: "Sets > Publish: combine (`out`: one PDF with a bookmark per sheet; latest_only keeps the newest version of each sheet, versions found by sheet number or wildcard `filter`), package (`dir`: the files copied with the set file and a drawing log), or log (`log`: the drawing log CSV).",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "set": path_arg("The set file"),
                "out": path_arg("The combined PDF"),
                "latest_only": { "type": "boolean" },
                "filter": { "type": "string" },
                "dir": path_arg("Package into this folder"),
                "log": path_arg("Write the drawing log here")
            }),
            &["set"],
        )
    },
    run: |a, args| {
        let set = load_set(&set_path(a, args)?)?;
        let mut out = json!({});
        if let Some(p) = args.opt_str("out")? {
            let p = a.resolve(p, true)?;
            let r = publish_combined(
                &set,
                &p,
                args.bool_or("latest_only", false)?,
                &args.opt_string("filter")?.unwrap_or_default(),
            )?;
            out["pages"] = json!(r.pages);
        }
        if let Some(d) = args.opt_str("dir")? {
            let r = publish_package(&set, &a.resolve(d, true)?)?;
            out["package"] = json!(r.files.iter().map(|f| f.display().to_string()).collect::<Vec<_>>());
        }
        if let Some(l) = args.opt_str("log")? {
            let (csv, n) = drawing_log(&set);
            std::fs::write(a.resolve(l, true)?, csv).map_err(crate::failed)?;
            out["log_rows"] = json!(n);
        }
        if out.as_object().is_some_and(serde_json::Map::is_empty) {
            return Err(bad_args("set_publish: give out, dir or log"));
        }
        Ok(out)
    },
};

fn job_from(args: &crate::Args) -> Result<PrintJob> {
    let mut job = PrintJob::default();
    if let Some(p) = args.opt_str("paper")? {
        job.settings.paper =
            markupcraft_engine::printout::paper_size(p).ok_or_else(|| bad_args(format!("unknown paper {p:?}")))?;
    }
    job.settings.layout = match args.opt_str("layout")?.unwrap_or("fit") {
        "fit" => PrintLayout::Fit,
        "actual" => PrintLayout::ActualSize,
        "shrink" => PrintLayout::Shrink,
        o => return Err(bad_args(format!("layout {o:?}: fit, actual or shrink"))),
    };
    job.settings.markups = args.bool_or("markups", true)?;
    job.copies = args.opt_u64("copies")?.unwrap_or(1) as usize;
    job.reverse = args.bool_or("reverse", false)?;
    Ok(job)
}

fn job_props() -> Value {
    json!({
        "paper": { "type": "string" },
        "layout": { "type": "string", "enum": ["fit", "actual", "shrink"] },
        "markups": { "type": "boolean" },
        "copies": { "type": "integer", "minimum": 1, "maximum": 999 },
        "reverse": { "type": "boolean" },
        "printer": { "type": "string", "description": "Also send to this printer (\"\" = the default) with the system's print command." }
    })
}

pub static SET_PRINT: Tool = Tool {
    name: "set_print",
    title: "Print a Set",
    description: "Print every sheet of the Set (`set`) with one job into a print-ready PDF at `out` (and to a printer when `printer` is given).",
    read_only: false,
    destructive: true,
    schema: || {
        let mut p = job_props();
        if let Some(o) = p.as_object_mut() {
            o.insert("set".into(), path_arg("The set file"));
            o.insert("out".into(), path_arg("The print-ready PDF"));
        }
        schema_nodoc(p, &["set", "out"])
    },
    run: |a, args| {
        let set = load_set(&set_path(a, args)?)?;
        let out = a.resolve(args.str("out")?, true)?;
        let job = job_from(args)?;
        let n = print_set(&set, &job, &out)?;
        if let Some(p) = args.opt_str("printer")? {
            markupcraft_engine::printing::send_to_printer(&out, (!p.is_empty()).then_some(p), 1)?;
        }
        Ok(json!({ "out": out.display().to_string(), "sheets": n }))
    },
};

pub static PRINTERS: Tool = Tool {
    name: "printer_list",
    title: "Printers",
    description: "The printers the system knows (empty when it cannot say).",
    read_only: true,
    destructive: false,
    schema: || schema_nodoc(json!({}), &[]),
    run: |_, _| Ok(json!({ "printers": list_printers() })),
};

pub static BATCH_PRINT: Tool = Tool {
    name: "batch_print",
    title: "Batch Print",
    description: "Print many PDFs (`files`) in list order with one job: each written as a print-ready PDF (<name>_print.pdf) in `out_dir`, and sent to `printer` when given.",
    read_only: false,
    destructive: true,
    schema: || {
        let mut p = job_props();
        if let Some(o) = p.as_object_mut() {
            o.insert(
                "files".into(),
                json!({ "type": "array", "items": { "type": "string" } }),
            );
            o.insert("out_dir".into(), path_arg("Folder for the print-ready PDFs"));
        }
        schema_nodoc(p, &["files", "out_dir"])
    },
    run: |a, args| {
        let files: Vec<PathBuf> = args
            .opt_strings("files")?
            .unwrap_or_default()
            .iter()
            .map(|f| a.resolve(f, false))
            .collect::<Result<_>>()?;
        let dir = a.resolve(args.str("out_dir")?, true)?;
        std::fs::create_dir_all(&dir).map_err(crate::failed)?;
        let job = job_from(args)?;
        let printer = args.opt_string("printer")?;
        let printer_ref = printer.as_deref().map(|p| (!p.is_empty()).then_some(p));
        let r = batch_print(&files, &job, &dir, printer_ref)?;
        Ok(json!({
            "results": r.iter().map(|(f, res)| match res {
                Ok(n) => json!({ "out": f.display().to_string(), "sheets": n }),
                Err(e) => json!({ "out": f.display().to_string(), "error": e }),
            }).collect::<Vec<_>>()
        }))
    },
};
