//! Tools that finish features which were partly built: inserting pages from several PDFs,
//! blank pages with a grid or from a template, one file per extracted page, Page Setup's
//! content placement, one PDF per source file, page templates and email templates.

use std::path::PathBuf;

use markupcraft_engine::Session;
use markupcraft_engine::finish::create::{
    EmailTemplate, create_each, email_from_template, list_templates, load_email_templates, remove_template,
    save_email_templates, save_template,
};
use markupcraft_engine::finish::pages::{GridStyle, PageSetup};
use serde_json::{Value, json};

use super::{Tool, page_arg, pages_arg, path_arg, schema, schema_nodoc};
use crate::{Result, bad_args, summary};

fn report(r: &markupcraft_engine::pages::PageReport) -> Value {
    json!({ "pages_before": r.pages_before, "pages_after": r.pages_after })
}

fn paths(v: &[PathBuf]) -> Vec<String> {
    v.iter().map(|p| p.display().to_string()).collect()
}

pub static INSERT_FILES: Tool = Tool {
    name: "page_insert_files",
    title: "Insert pages from several PDFs",
    description: "Insert pages from one or more PDFs in order (each item: path and optional pages of that file, default all) so the first inserted page becomes page `at`. One undoable step.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "at": page_arg("position of the first inserted page"),
                "files": { "type": "array", "items": { "type": "object", "properties": { "path": { "type": "string" }, "pages": pages_arg("of that file (default: all)") }, "required": ["path"] } }
            }),
            &["at", "files"],
        )
    },
    run: |a, args| {
        let at = args.position("at")?;
        let list = args.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
        if list.is_empty() {
            return Err(bad_args("files: give one or more {path, pages}"));
        }
        let mut items = Vec::new();
        for it in &list {
            let p = it
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| bad_args("each file needs a path"))?;
            let path = a.resolve(p, false)?;
            let pages = match it.get("pages") {
                None | Some(Value::Null) => None,
                Some(v) => {
                    let n = markupcraft_engine::pages::ForeignPdf::open(&path)?.page_count();
                    Some(page_list(v, n)?)
                }
            };
            items.push((path, pages));
        }
        let (doc, s) = a.session(args)?;
        let r = s.insert_files(at, &items)?;
        Ok(json!({ "report": report(&r), "document": summary(doc, s) }))
    },
};

/// A page list of an item: `[1, 3]` or `"1-3, 5"` (1-based) to 0-based.
fn page_list(v: &Value, n: usize) -> Result<Vec<usize>> {
    let out = match v {
        Value::String(t) => crate::parse_range(t, n).map_err(bad_args)?,
        Value::Array(a) => a
            .iter()
            .take(10_000)
            .map(|x| match x.as_u64().and_then(|p| usize::try_from(p).ok()) {
                Some(p) if p >= 1 && p <= n => Ok(p - 1),
                _ => Err(bad_args(format!("pages: 1 to {n}"))),
            })
            .collect::<Result<Vec<usize>>>()?,
        _ => return Err(bad_args("pages: a list or a range like \"1-3\"")),
    };
    if out.is_empty() {
        return Err(bad_args("pages: at least one page"));
    }
    Ok(out)
}

pub static INSERT_BLANK_STYLED: Tool = Tool {
    name: "page_insert_blank_styled",
    title: "Insert blank pages with a grid or from a template",
    description: "Insert `count` pages at `at`: blank (width x height points, default Letter) ruled with a grid (grid_spacing points 4-288, grid_dots, grid_gray 0-1), or copies of the first page of the PDF `template`. One undoable step.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "at": page_arg("position of the first new page"),
                "count": { "type": "integer", "minimum": 1, "maximum": 1000 },
                "width": { "type": "number" }, "height": { "type": "number" },
                "grid_spacing": { "type": "number" }, "grid_dots": { "type": "boolean" }, "grid_gray": { "type": "number" },
                "template": path_arg("A PDF whose first page is copied")
            }),
            &["at"],
        )
    },
    run: |a, args| {
        let at = args.position("at")?;
        let count = usize::try_from(args.opt_int("count")?.unwrap_or(1)).map_err(|_| bad_args("count is 1 to 1000"))?;
        let size = match (args.opt_num("width")?, args.opt_num("height")?) {
            (Some(w), Some(h)) => Some((w, h)),
            (None, None) => None,
            _ => return Err(bad_args("give both width and height")),
        };
        let grid = match args.opt_num("grid_spacing")? {
            Some(spacing) => Some(GridStyle {
                spacing,
                dots: args.bool_or("grid_dots", false)?,
                gray: args.opt_num("grid_gray")?.unwrap_or(0.75),
            }),
            None => None,
        };
        let template = match args.opt_str("template")? {
            Some(t) => Some(a.resolve(t, false)?),
            None => None,
        };
        let (doc, s) = a.session(args)?;
        let r = s.insert_blank_styled(at, count, size, grid, template.as_deref())?;
        Ok(json!({ "report": report(&r), "document": summary(doc, s) }))
    },
};

pub static EXTRACT_EACH: Tool = Tool {
    name: "page_extract_each",
    title: "Extract one file per page",
    description: "Write each of `pages` to its own PDF in folder `dir`: <stem>_<n>.pdf, or the page label with by_label. overwrite replaces files of the same name (else ' (2)' is added); delete removes the pages afterwards (undoable).",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to extract"), "dir": path_arg("The folder"),
                "stem": { "type": "string" }, "by_label": { "type": "boolean" },
                "overwrite": { "type": "boolean" }, "delete": { "type": "boolean" },
                "update_links": { "type": "boolean", "description": "Links between the extracted pages point to their new files." }
            }),
            &["pages", "dir"],
        )
    },
    run: |a, args| {
        let dir = a.resolve(args.str("dir")?, true)?;
        let stem = args.opt_string("stem")?.unwrap_or_else(|| "page".into());
        let (by_label, overwrite, delete) = (
            args.bool_or("by_label", false)?,
            args.bool_or("overwrite", false)?,
            args.bool_or("delete", false)?,
        );
        let (doc, s) = a.session(args)?;
        let pages = args.pages("pages", s.page_count())?;
        let links = args.bool_or("update_links", false)?;
        let files = s.extract_each_linked(&pages, &dir, &stem, by_label, overwrite, delete, links)?;
        Ok(json!({ "files": paths(&files), "document": summary(doc, s) }))
    },
};

pub static PAGE_SETUP: Tool = Tool {
    name: "page_setup",
    title: "Page Setup",
    description: "Give pages a new size (width x height points) and place their content on it: scale (omit to fit inside the margins), offset [x, y] points, rotation degrees counter-clockwise (or level_line [[x,y],[x,y]] to turn a drawn line level), center, margins [left, right, top, bottom] points (binding margins) and a border line this wide. Markups move with the content. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to set up (default: all)"),
                "width": { "type": "number" }, "height": { "type": "number" },
                "scale": { "type": "number" }, "offset": { "type": "array", "items": { "type": "number" } },
                "rotation": { "type": "number" },
                "level_line": { "type": "array", "items": { "type": "array", "items": { "type": "number" } } },
                "center": { "type": "boolean" },
                "margins": { "type": "array", "items": { "type": "number" } },
                "border": { "type": "number" }
            }),
            &["width", "height"],
        )
    },
    run: |a, args| {
        let mut st = PageSetup {
            width: args.num("width")?,
            height: args.num("height")?,
            scale: args.opt_num("scale")?,
            center: args.bool_or("center", true)?,
            border: args.opt_num("border")?.unwrap_or(0.0),
            rotation: args.opt_num("rotation")?.unwrap_or(0.0),
            ..Default::default()
        };
        if let Some(o) = args.get("offset").and_then(Value::as_array) {
            st.offset = (
                o.first().and_then(Value::as_f64).unwrap_or(0.0),
                o.get(1).and_then(Value::as_f64).unwrap_or(0.0),
            );
        }
        if let Some(m) = args.get("margins").and_then(Value::as_array) {
            for (i, v) in m.iter().take(4).enumerate() {
                if let Some(slot) = st.margins.get_mut(i) {
                    *slot = v.as_f64().unwrap_or(0.0);
                }
            }
        }
        if let Some(line) = args.opt_points("level_line")? {
            let (Some(p0), Some(p1)) = (line.first(), line.get(1)) else {
                return Err(bad_args("level_line: two points"));
            };
            st.rotation = markupcraft_engine::finish::pages::level_angle(*p0, *p1);
        }
        let (doc, s) = a.session(args)?;
        let pages = args
            .opt_pages("pages", s.page_count())?
            .unwrap_or_else(|| (0..s.page_count()).collect());
        s.page_setup(&pages, &st)?;
        Ok(json!({ "rotation": st.rotation, "document": summary(doc, s) }))
    },
};

pub static CREATE_EACH: Tool = Tool {
    name: "doc_create_each",
    title: "Create one PDF per file",
    description: "Stapler, one PDF per source: convert each file (images, text, Word .docx, Excel .xlsx, DXF) to <stem>.pdf in `out_dir`, or beside its source when out_dir is omitted. PDFs are skipped.",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({ "files": { "type": "array", "items": { "type": "string" } }, "out_dir": path_arg("Output folder (default: each source's folder)") }),
            &["files"],
        )
    },
    run: |a, args| {
        let files: Vec<PathBuf> = args
            .opt_strings("files")?
            .unwrap_or_default()
            .iter()
            .map(|p| a.resolve(p, false))
            .collect::<Result<_>>()?;
        let out = match args.opt_str("out_dir")? {
            Some(d) => Some(a.resolve(d, true)?),
            None => None,
        };
        let written = create_each(&files, out.as_deref())?;
        Ok(json!({ "files": paths(&written) }))
    },
};

pub static PAGE_TEMPLATE: Tool = Tool {
    name: "page_template",
    title: "Page templates",
    description: "Page templates kept as PDFs in folder `dir`: action list; save (name: the current document becomes the template); remove (name); new (name + path: open a new unsaved document from the template that saves to `path`).",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "action": { "type": "string", "enum": ["list", "save", "remove", "new"] },
                "dir": path_arg("The templates folder"), "name": { "type": "string" },
                "path": path_arg("For new: where the new document saves")
            }),
            &["action", "dir"],
        )
    },
    run: |a, args| {
        let dir = a.resolve(args.str("dir")?, true)?;
        let names =
            |dir: &std::path::Path| -> Vec<String> { list_templates(dir).into_iter().map(|(n, _)| n).collect() };
        match args.str("action")? {
            "list" => Ok(json!({ "templates": names(&dir) })),
            "save" => {
                let name = args.str("name")?.to_string();
                let (_, s) = a.session(args)?;
                let bytes = s.current_bytes()?.to_vec();
                save_template(&dir, &name, &bytes)?;
                Ok(json!({ "templates": names(&dir) }))
            }
            "remove" => {
                remove_template(&dir, args.str("name")?)?;
                Ok(json!({ "templates": names(&dir) }))
            }
            "new" => {
                let name = args.str("name")?;
                let t = list_templates(&dir)
                    .into_iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .ok_or_else(|| bad_args(format!("no template {name}")))?;
                let path = a.resolve(args.str("path")?, true)?;
                let s = Session::from_template(&t.1, &path)?;
                let id = a.add_doc(s)?;
                let s = a
                    .docs()
                    .iter()
                    .find(|(d, _)| *d == id)
                    .map(|(_, s)| s)
                    .ok_or_else(|| crate::failed("document vanished"))?;
                Ok(summary(id, s))
            }
            o => Err(bad_args(format!("unknown action {o}"))),
        }
    },
};

pub static EMAIL_TEMPLATE: Tool = Tool {
    name: "email_template",
    title: "Email templates",
    description: "Email templates kept in a JSON file (`path`): action list; save (name, to, cc, subject, body; {file} is replaced by the document's name); delete (name); draft (name + out: write the unsent .eml message with this document attached, as File > Email opens it).",
    read_only: false,
    destructive: false,
    schema: || {
        let t = || json!({ "type": "string" });
        schema(
            json!({
                "action": { "type": "string", "enum": ["list", "save", "delete", "draft"] },
                "path": path_arg("The templates file"), "name": t(), "to": t(), "cc": t(), "subject": t(), "body": t(),
                "out": path_arg("For draft: the .eml file")
            }),
            &["action", "path"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, true)?;
        let mut list = load_email_templates(&path)?;
        let names = |l: &[EmailTemplate]| l.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
        match args.str("action")? {
            "list" => Ok(
                json!({ "templates": list.iter().map(|t| json!({ "name": t.name, "to": t.to, "cc": t.cc, "subject": t.subject, "body": t.body })).collect::<Vec<_>>() }),
            ),
            "save" => {
                let t = EmailTemplate {
                    name: args.str("name")?.trim().to_string(),
                    to: args.opt_string("to")?.unwrap_or_default(),
                    cc: args.opt_string("cc")?.unwrap_or_default(),
                    subject: args.opt_string("subject")?.unwrap_or_default(),
                    body: args.opt_string("body")?.unwrap_or_default(),
                };
                list.retain(|o| !o.name.eq_ignore_ascii_case(&t.name));
                list.push(t);
                save_email_templates(&path, &list)?;
                Ok(json!({ "templates": names(&list) }))
            }
            "delete" => {
                let n = args.str("name")?;
                list.retain(|o| !o.name.eq_ignore_ascii_case(n));
                save_email_templates(&path, &list)?;
                Ok(json!({ "templates": names(&list) }))
            }
            "draft" => {
                let n = args.str("name")?;
                let t = list
                    .iter()
                    .find(|o| o.name.eq_ignore_ascii_case(n))
                    .cloned()
                    .ok_or_else(|| bad_args(format!("no template {n}")))?;
                let out = a.resolve(args.str("out")?, true)?;
                let (_, s) = a.session(args)?;
                let name = s
                    .path()
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "document.pdf".into());
                let bytes = s.current_bytes()?.to_vec();
                let eml = email_from_template(&t, &name, &bytes);
                markupcraft_engine::finish::write_file(&out, &eml)?;
                Ok(json!({ "out": out.display().to_string(), "bytes": eml.len() }))
            }
            o => Err(bad_args(format!("unknown action {o}"))),
        }
    },
};

fn file_list(a: &crate::Automation, args: &crate::Args) -> Result<Vec<PathBuf>> {
    let f = args.opt_strings("files")?.unwrap_or_default();
    if f.is_empty() {
        return Err(bad_args(format!("{}: give files", args.tool())));
    }
    f.iter().map(|p| a.resolve(p, false)).collect()
}

pub static BATCH_SPLIT: Tool = Tool {
    name: "batch_split",
    title: "Batch split",
    description: "Split every PDF in `files` into parts in folder `dir`: every `pages_per_file` pages, or at each top-level bookmark (by: \"bookmarks\"). Parts are named <file>-<part>.pdf; the files are not changed.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "files": { "type": "array", "items": { "type": "string" } }, "dir": path_arg("The folder for the parts"),
                "pages_per_file": { "type": "integer", "minimum": 1 }, "by": { "type": "string", "enum": ["pages", "bookmarks"] }
            }),
            &["files", "dir"],
        )
    },
    run: |a, args| {
        use markupcraft_engine::combine::SplitBy;
        let files = file_list(a, args)?;
        let dir = a.resolve(args.str("dir")?, true)?;
        let by = match (args.opt_str("by")?, args.opt_u64("pages_per_file")?) {
            (Some("bookmarks"), None) => SplitBy::Bookmarks,
            (None | Some("pages"), Some(n)) => {
                SplitBy::Pages(usize::try_from(n).map_err(|_| bad_args("pages_per_file"))?)
            }
            _ => return Err(bad_args("batch_split: give pages_per_file or by: \"bookmarks\"")),
        };
        let mut out = Vec::new();
        for f in &files {
            let r = Session::open(f).and_then(|s| s.split_document(&dir, &by));
            out.push(match r {
                Ok(parts) => json!({ "file": f.display().to_string(), "ok": true, "parts": parts.len() }),
                Err(e) => json!({ "file": f.display().to_string(), "ok": false, "error": e.to_string() }),
            });
        }
        Ok(json!({ "files": out }))
    },
};

pub static BATCH_SCRIPT: Tool = Tool {
    name: "batch_script",
    title: "Batch script",
    description: "Run a script file (`script`, JSON: [{\"tool\": name, \"params\": {...}}], the format of markupcraft-cli run --script) on every PDF in `files`, as batch_apply does: each file saved in place or into `out_dir`.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "files": { "type": "array", "items": { "type": "string" } }, "script": path_arg("The script (.json)"),
                "out_dir": path_arg("Write the results here instead of in place")
            }),
            &["files", "script"],
        )
    },
    run: |a, args| {
        let script = a.resolve(args.str("script")?, false)?;
        let len = std::fs::metadata(&script).map_err(crate::failed)?.len();
        if len > 4 << 20 {
            return Err(bad_args("the script is too large"));
        }
        let text = std::fs::read_to_string(&script).map_err(crate::failed)?;
        let steps: Value = serde_json::from_str(&text).map_err(|e| bad_args(format!("script: {e}")))?;
        let ops: Vec<Value> = steps
            .as_array()
            .ok_or_else(|| bad_args("a script is a list of {\"tool\", \"params\"} steps"))?
            .iter()
            .map(|s| {
                let p = s
                    .get("params")
                    .or_else(|| s.get("args"))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                json!({ "tool": s.get("tool").cloned().unwrap_or(Value::Null), "args": p })
            })
            .collect();
        let mut call = json!({ "files": args.opt_strings("files")?.unwrap_or_default(), "operations": ops });
        if let (Some(d), Some(o)) = (args.opt_str("out_dir")?, call.as_object_mut()) {
            o.insert("out_dir".into(), json!(d));
        }
        a.call("batch_apply", &call)
    },
};

pub static LEGEND_TOOLSET: Tool = Tool {
    name: "legend_from_toolset",
    title: "Legend of a tool set",
    description: "Add a legend tied to a Tool Chest set: every subject the set's tools make (a .mctools file at `path`) is listed, even with none drawn yet, with live counts and totals; markups of those subjects drawn later appear on the next update. At `at` (top-left) on `page`; title defaults to the set's title.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "path": path_arg("The tool set (.mctools)"), "page": page_arg("to put it on"), "at": super::point_arg("Its top-left corner"), "title": { "type": "string" } }),
            &["path", "page", "at"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let len = std::fs::metadata(&path).map_err(crate::failed)?.len();
        if len > 16 << 20 {
            return Err(bad_args("the tool set file is too large"));
        }
        let v: Value = serde_json::from_slice(&std::fs::read(&path).map_err(crate::failed)?)
            .map_err(|e| bad_args(format!("not a tool set: {e}")))?;
        let set = v.get("set").ok_or_else(|| bad_args("not a MarkupCraft tool set"))?;
        let mut subjects: Vec<String> = Vec::new();
        for it in set.get("items").and_then(Value::as_array).into_iter().flatten() {
            let subj = it
                .get("markup")
                .and_then(|m| m.get("subject"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let s = if subj.is_empty() {
                it.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            } else {
                subj
            };
            if !s.is_empty() && !subjects.contains(&s) {
                subjects.push(s);
            }
        }
        if subjects.is_empty() {
            return Err(bad_args("the tool set has no tools"));
        }
        let title = match args.opt_string("title")? {
            Some(t) => t,
            None => set.get("title").and_then(Value::as_str).unwrap_or("Legend").to_string(),
        };
        let o = markupcraft_engine::legend::LegendOptions {
            title,
            subjects: subjects.clone(),
            show_empty: true,
            ..Default::default()
        };
        let page = args.page("page")?;
        let at = args.point("at")?;
        let (doc, s) = a.session(args)?;
        let id = s.add_legend(page, at, &o)?;
        Ok(json!({ "id": id, "subjects": subjects, "document": summary(doc, s) }))
    },
};

pub static STATUS_REPORT: Tool = Tool {
    name: "count_status_report",
    title: "Count status report",
    description: "How many count items (and markups) of each subject are in each status (None, Accepted, Installed ...). counts_only (default true) limits it to Counts. With `out` (.pdf), also write the visual report: a copy of the document with every markup drawn in its status's colour; with `csv`, the table.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "counts_only": { "type": "boolean" },
                "out": path_arg("The visual report (.pdf)"),
                "csv": path_arg("The table (.csv)")
            }),
            &[],
        )
    },
    run: |a, args| {
        use markupcraft_engine::finish::status::{status_color, status_csv, status_report};
        let counts_only = args.bool_or("counts_only", true)?;
        let out = match args.opt_str("out")? {
            Some(p) => Some(a.resolve(p, true)?),
            None => None,
        };
        let csv = match args.opt_str("csv")? {
            Some(p) => Some(a.resolve(p, true)?),
            None => None,
        };
        let (doc, s) = a.session(args)?;
        let rows = match &out {
            Some(o) => s.visual_status_report(o, counts_only)?,
            None => status_report(&s.doc().markups, counts_only),
        };
        if let Some(c) = &csv {
            markupcraft_engine::finish::write_file(c, status_csv(&rows).as_bytes())?;
        }
        Ok(json!({
            "doc": doc,
            "rows": rows.iter().map(|r| json!({ "subject": r.subject, "status": r.status, "items": r.items, "markups": r.markups, "color": status_color(&r.status).hex() })).collect::<Vec<_>>()
        }))
    },
};

pub static VIEWPORT_CALIBRATE: Tool = Tool {
    name: "viewport_calibrate",
    title: "Calibrate a viewport",
    description: "Give viewport `index` (1-based, as viewport_edit counts them) on `page` a calibrated scale: two points `from` and `to` that are `length` `unit`s apart. y_scale (a scale object) sets a separate Y scale instead or as well. Measurements inside it update. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("of the viewport"), "index": { "type": "integer", "minimum": 1 },
                "from": super::point_arg("First point"), "to": super::point_arg("Second point"),
                "length": { "type": "number" }, "unit": { "type": "string" },
                "y_scale": super::scale_arg()
            }),
            &["page", "index"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let index = usize::try_from(args.int("index")?.saturating_sub(1)).map_err(|_| bad_args("index from 1"))?;
        let (doc, s) = a.session(args)?;
        let vp = s
            .doc()
            .pages
            .get(page)
            .and_then(|p| p.viewports.get(index))
            .cloned()
            .ok_or_else(|| bad_args("no such viewport"))?;
        let mut sc = vp.scale.clone();
        if let (Some(f), Some(t)) = (args.opt_point("from")?, args.opt_point("to")?) {
            let length = args.num("length")?;
            let unit = args
                .opt_unit("unit")?
                .unwrap_or(markupcraft_measure::units::LengthUnit::Foot);
            sc = markupcraft_engine::calibrated_scale(f.dist(t), length, unit)?;
        }
        if let Some(ys) = args.opt_scale("y_scale")? {
            sc.y = ys.x.clone();
        }
        let n = s.set_viewport_scale(page, index, &sc, true)?;
        Ok(json!({ "scale": sc.ratio, "x": sc.x_conv(), "y": sc.y_conv(), "updated": n, "document": summary(doc, s) }))
    },
};

pub static REDACT_KINDS: Tool = Tool {
    name: "redact_apply_kinds",
    title: "Apply redactions to text or images only",
    description: "Apply the redaction marks (on `pages`, or all) removing only `kinds`: text (text under the marks goes; images and vector graphics stay) or images (images go; text and graphics stay), or all. scrub removes document metadata too. Content inside form XObjects is not split by kind. The next save is a full rewrite.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to apply on (default: all)"),
                "kinds": { "type": "string", "enum": ["all", "text", "images"] },
                "scrub": { "type": "boolean" }
            }),
            &["kinds"],
        )
    },
    run: |a, args| {
        use markupcraft_engine::finish::redact_kinds::RedactKinds;
        let kinds = RedactKinds::from_name(args.str("kinds")?).ok_or_else(|| bad_args("kinds: all, text or images"))?;
        let scrub = args.bool_or("scrub", false)?;
        let (doc, s) = a.session(args)?;
        let pages = args.opt_pages("pages", s.page_count())?;
        let r = s.redact_apply_kinds(pages.as_deref(), kinds, scrub)?;
        Ok(
            json!({ "marks": r.marks, "glyphs": r.glyphs, "images": r.images, "paths": r.paths, "residue": r.residue, "document": summary(doc, s) }),
        )
    },
};

pub static SNAPSHOT_CUT: Tool = Tool {
    name: "snapshot_cut",
    title: "Cut a snapshot",
    description: "Snapshot Content > Cut: the page content of `rect` on `page` goes to the clipboard as a snapshot (kept vector; markup_paste places it) and is removed from the page (no box is drawn; other redaction marks are left alone). Undoable; the next save is a full rewrite.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "page": page_arg("to cut from"), "rect": super::rect_arg("The region") }),
            &["page", "rect"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let rect = args
            .opt_rect("rect")?
            .ok_or_else(|| bad_args("rect: [x0, y0, x1, y1]"))?;
        let (doc, s) = a.session(args)?;
        let r = s.snapshot_cut(page, rect)?;
        let clip = s.clipboard().to_vec();
        a.set_clipboard(clip);
        Ok(json!({ "doc": doc, "rect": r.as_array(), "clipboard": 1 }))
    },
};

pub static ID_STORE: Tool = Tool {
    name: "digital_id_store",
    title: "Digital ID manager",
    description: "Digital IDs kept in a folder (`dir`): action list; create (name, person, password, years); import (file .p12/.pfx, name, password); export (name + out: the public certificate, PEM); password (name, password, new_password); delete (name).",
    read_only: false,
    destructive: true,
    schema: || {
        let t = || json!({ "type": "string" });
        schema_nodoc(
            json!({
                "action": { "type": "string", "enum": ["list", "create", "import", "export", "password", "delete"] },
                "dir": path_arg("The ID store folder"), "name": t(), "person": t(), "password": t(), "new_password": t(),
                "years": { "type": "integer", "minimum": 1, "maximum": 50 },
                "file": path_arg("For import: the .p12 / .pfx"), "out": path_arg("For export: the .pem")
            }),
            &["action", "dir"],
        )
    },
    run: |a, args| {
        use markupcraft_engine::finish::idstore;
        let dir = a.resolve(args.str("dir")?, true)?;
        let pw = || args.opt_string("password").map(Option::unwrap_or_default);
        let done = match args.str("action")? {
            "list" => None,
            "create" => {
                let who = markupcraft_engine::signatures::IdentityInfo {
                    name: args.str("person")?.to_string(),
                    ..Default::default()
                };
                let years = u32::try_from(args.opt_int("years")?.unwrap_or(5)).map_err(|_| bad_args("years"))?;
                Some(idstore::create_id(&dir, args.str("name")?, &who, years, &pw()?)?.name)
            }
            "import" => {
                let file = a.resolve(args.str("file")?, false)?;
                Some(idstore::import_id(&dir, &file, args.str("name")?, &pw()?)?.name)
            }
            "export" => {
                let out = a.resolve(args.str("out")?, true)?;
                idstore::export_certificate(&dir, args.str("name")?, &out)?;
                Some(out.display().to_string())
            }
            "password" => {
                idstore::change_password(&dir, args.str("name")?, &pw()?, args.str("new_password")?)?;
                Some(args.str("name")?.to_string())
            }
            "delete" => {
                idstore::delete_id(&dir, args.str("name")?)?;
                Some(args.str("name")?.to_string())
            }
            o => return Err(bad_args(format!("unknown action {o}"))),
        };
        let ids: Vec<Value> = idstore::list_ids(&dir)
            .iter()
            .map(|i| json!({ "name": i.name, "subject": i.subject, "fingerprint": i.fingerprint }))
            .collect();
        Ok(json!({ "done": done, "ids": ids }))
    },
};

pub static CLEAR_CERTIFICATION: Tool = Tool {
    name: "certification_clear",
    title: "Clear a certification",
    description: "Clear the document's certification with the certifier's digital ID (`id` .p12 + password): the certifying signature is removed (its field stays, unsigned) and the DocMDP permission goes. Only the certifier's ID is accepted. Undoable until saved.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "id": path_arg("The certifier's digital ID (.p12)"), "password": { "type": "string" } }),
            &["id", "password"],
        )
    },
    run: |a, args| {
        let p = a.resolve(args.str("id")?, false)?;
        let bytes = std::fs::read(&p).map_err(crate::failed)?;
        let id = markupcraft_engine::signatures::open_digital_id(&bytes, args.str("password")?)?;
        let (doc, s) = a.session(args)?;
        let field = s.clear_certification(&id)?;
        Ok(json!({ "field": field, "document": summary(doc, s) }))
    },
};

pub static FORM_PROPS: Tool = Tool {
    name: "form_field_props",
    title: "Form field properties",
    description: "Change a form field's properties (`name` = the field): new_name, tooltip, required, read_only, multiline, max_len (0 = no limit), font_size (0 = auto), align (0 left, 1 centre, 2 right), default_value, locked, options (choice fields). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        let b = || json!({ "type": "boolean" });
        schema(
            json!({
                "name": { "type": "string" }, "new_name": { "type": "string" }, "tooltip": { "type": "string" },
                "required": b(), "read_only": b(), "multiline": b(), "locked": b(),
                "max_len": { "type": "integer", "minimum": 0 }, "font_size": { "type": "number" },
                "align": { "type": "integer", "minimum": 0, "maximum": 2 }, "default_value": { "type": "string" },
                "options": { "type": "array", "items": { "type": "string" } }
            }),
            &["name"],
        )
    },
    run: |a, args| {
        use markupcraft_engine::finish::forms_more::FieldEdit;
        let e = FieldEdit {
            name: args.opt_string("new_name")?,
            tooltip: args.opt_string("tooltip")?,
            required: args.opt_bool("required")?,
            read_only: args.opt_bool("read_only")?,
            multiline: args.opt_bool("multiline")?,
            locked: args.opt_bool("locked")?,
            max_len: args
                .opt_u64("max_len")?
                .map(|n| (n > 0).then(|| usize::try_from(n).unwrap_or(usize::MAX))),
            font_size: args.opt_num("font_size")?,
            align: args.opt_int("align")?,
            default_value: args.opt_string("default_value")?.map(Some),
            options: args.opt_strings("options")?,
        };
        let name = args.str("name")?.to_string();
        let (doc, s) = a.session(args)?;
        let n = s.form_set_props(&name, &e)?;
        Ok(json!({ "name": n, "document": summary(doc, s) }))
    },
};

pub static FORM_ACTION: Tool = Tool {
    name: "form_field_action",
    title: "Form field actions",
    description: "List (no trigger) or set a form field's action on `trigger` (mouse_up, mouse_down, mouse_enter, mouse_exit, on_focus, on_blur): `action` uri (value: the link), goto (value: 1-based page), named (value: Print, NextPage, PrevPage, FirstPage, LastPage), reset (value: field names, comma separated, empty = all), hide / show (value: field names), javascript (value: the script; document JavaScript never runs in MarkupCraft), none (remove). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "name": { "type": "string" }, "trigger": { "type": "string" },
                "action": { "type": "string", "enum": ["uri", "goto", "named", "reset", "hide", "show", "javascript", "none"] },
                "value": { "type": "string" }
            }),
            &["name"],
        )
    },
    run: |a, args| {
        use markupcraft_engine::finish::forms_more::{FieldAction, trigger};
        let name = args.str("name")?.to_string();
        let (doc, s) = a.session(args)?;
        if let Some(t) = args.opt_str("trigger")? {
            let t = trigger(t).ok_or_else(|| {
                bad_args("trigger: mouse_up, mouse_down, mouse_enter, mouse_exit, on_focus or on_blur")
            })?;
            let v = args.opt_string("value")?.unwrap_or_default();
            let list = |s: &str| -> Vec<String> {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            };
            let act = match args.opt_str("action")?.unwrap_or("none") {
                "uri" => Some(FieldAction::Uri(v)),
                "goto" => Some(FieldAction::GoTo(
                    v.trim()
                        .parse::<usize>()
                        .map_err(|_| bad_args("goto: a page number"))?
                        .saturating_sub(1),
                )),
                "named" => Some(FieldAction::Named(v)),
                "reset" => Some(FieldAction::Reset(list(&v))),
                "hide" => Some(FieldAction::ShowHide {
                    fields: list(&v),
                    hide: true,
                }),
                "show" => Some(FieldAction::ShowHide {
                    fields: list(&v),
                    hide: false,
                }),
                "javascript" => Some(FieldAction::JavaScript(v)),
                "none" => None,
                o => return Err(bad_args(format!("unknown action {o}"))),
            };
            s.form_set_action(&name, t, act)?;
        }
        let acts: Vec<Value> = s
            .form_actions(&name)?
            .iter()
            .map(|(t, a)| json!({ "trigger": t.id(), "action": a.describe() }))
            .collect();
        Ok(json!({ "actions": acts, "document": summary(doc, s) }))
    },
};

pub static XFA_LAYOUT: Tool = Tool {
    name: "form_xfa_layout",
    title: "Lay out an XFA form",
    description: "A dynamic XFA form (which shows only a placeholder page in most viewers) is laid out as ordinary pages and form fields holding its data (pdfcraft-xfa), so form_list / form_fill work on it. Reports pages, fields and what was approximated. Undoable; the next save is a full rewrite.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let (pages, fields, warnings) = s.xfa_render()?;
        Ok(json!({ "pages": pages, "fields": fields, "warnings": warnings, "document": summary(doc, s) }))
    },
};

pub static FLATTEN_EXTRAS: Tool = Tool {
    name: "markup_flatten_extras",
    title: "Flatten with extras",
    description: "Flatten markups (ids, pages, kinds, layers to choose; default all) onto `layer`, recoverable, with overlay text written on each flattened page (overlay, font Helvetica / Times-Roman / Courier, size, position top_left ... bottom_right), the chosen properties (keep: subject, author, comments, label, status, date) kept in a pop-up note per flattened markup, and capture_summary: a CSV of flattened File Attachments attached to the document. Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        let list = || json!({ "type": "array", "items": { "type": "string" } });
        schema(
            json!({
                "ids": list(), "pages": pages_arg("to flatten (default: all)"), "kinds": list(), "layers": list(),
                "layer": { "type": "string" }, "recoverable": { "type": "boolean" },
                "overlay": { "type": "string" }, "font": { "type": "string" }, "size": { "type": "number" },
                "position": { "type": "string", "enum": ["top_left", "top_center", "top_right", "bottom_left", "bottom_center", "bottom_right"] },
                "keep": list(), "capture_summary": { "type": "boolean" }
            }),
            &[],
        )
    },
    run: |a, args| {
        use markupcraft_engine::finish::flatten_more::{FlattenExtras, OverlayPos};
        use markupcraft_engine::flatten::{FlattenFilter, FlattenOptions};
        let x = FlattenExtras {
            overlay: args.opt_string("overlay")?.unwrap_or_default(),
            font: args.opt_string("font")?.unwrap_or_default(),
            size: args.opt_num("size")?.unwrap_or(10.0),
            position: match args.opt_str("position")? {
                Some(p) => OverlayPos::from_name(p).ok_or_else(|| bad_args("unknown position"))?,
                None => OverlayPos::default(),
            },
            keep: args.opt_strings("keep")?.unwrap_or_default(),
            capture_summary: args.bool_or("capture_summary", false)?,
        };
        let opts = FlattenOptions {
            recoverable: args.bool_or("recoverable", false)?,
            layer: args.opt_string("layer")?,
        };
        let (doc, s) = a.session(args)?;
        let filter = FlattenFilter {
            ids: args.opt_strings("ids")?.unwrap_or_default(),
            pages: args.opt_pages("pages", s.page_count())?.unwrap_or_default(),
            kinds: args.opt_strings("kinds")?.unwrap_or_default(),
            layers: args.opt_strings("layers")?.unwrap_or_default(),
            authors: Vec::new(),
        };
        let n = s.flatten_with_extras(&filter, &opts, &x)?;
        Ok(json!({ "flattened": n, "document": summary(doc, s) }))
    },
};
