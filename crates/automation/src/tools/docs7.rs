//! More document tools: header/footer tokens, saved summary column configurations, the
//! summary straight to the printer, links from search results, File Access favourites and the
//! print preview.

use markupcraft_engine::favorites::{add_favorite, load_favorites, remove_favorite};
use markupcraft_engine::search::SearchOptions;
use markupcraft_engine::summary::SummaryOptions;
use markupcraft_engine::summary_cols::{ColumnConfig, delete_config, find_config, load_configs, save_config};
use serde_json::{Value, json};

use super::{Tool, pages_arg, path_arg, schema, schema_nodoc};
use crate::{bad_args, summary};

pub static HF_TOKENS: Tool = Tool {
    name: "header_footer_tokens",
    title: "Header and footer tokens",
    description: "The tokens a header or footer text can hold (page number, page count, dates, Bates number, and file data: file name, file path, author, title, subject, keywords, creator, producer), each with what it shows; with a document open, also what each file-data token becomes for it.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let tokens: Vec<Value> = markupcraft_engine::hf_tokens::all_tokens()
            .into_iter()
            .map(|(t, l)| json!({ "token": t, "label": l }))
            .collect();
        let values = match a.session_ref(args) {
            Ok((_, s)) => s
                .file_token_values()
                .into_iter()
                .map(|(t, v)| (t.to_string(), Value::String(v)))
                .collect::<serde_json::Map<_, _>>(),
            Err(_) => Default::default(),
        };
        Ok(json!({ "tokens": tokens, "values": values }))
    },
};

fn config_json(c: &ColumnConfig) -> Value {
    json!({ "name": c.name, "columns": c.columns, "include_empty": c.include_empty })
}

pub static SUMMARY_COLUMNS: Tool = Tool {
    name: "summary_columns",
    title: "Saved summary column configurations",
    description: "Markup Summary column configurations kept in the config folder: action list (default), save (`name`, `columns` in report order, `include_empty`: keep columns empty in every row, default true), load (`name`: its columns) or delete (`name`). summary_export and summary_print use one with `column_config`.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "action": { "type": "string", "enum": ["list", "save", "load", "delete"] },
                "name": { "type": "string" },
                "columns": { "type": "array", "items": { "type": "string" }, "description": "Markups List column ids in report order." },
                "include_empty": { "type": "boolean" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let dir = a.config_dir()?;
        let name = || args.str("name");
        match args.opt_str("action")?.unwrap_or("list") {
            "list" => {
                let v = load_configs(&dir)?;
                Ok(json!({ "configs": v.iter().map(config_json).collect::<Vec<_>>() }))
            }
            "save" => {
                let c = ColumnConfig {
                    name: name()?.to_string(),
                    columns: args
                        .opt_strings("columns")?
                        .ok_or_else(|| bad_args("summary_columns save: give `columns`"))?,
                    include_empty: args.bool_or("include_empty", true)?,
                };
                save_config(&dir, &c)?;
                Ok(json!({ "saved": config_json(&c) }))
            }
            "load" => Ok(config_json(&find_config(&dir, name()?)?)),
            "delete" => Ok(json!({ "deleted": delete_config(&dir, name()?)? })),
            o => Err(bad_args(format!("action {o:?}: list, save, load or delete"))),
        }
    },
};

pub static SUMMARY_PRINT: Tool = Tool {
    name: "summary_print",
    title: "Print the Markup Summary",
    description: "Print the Markup Summary straight to a printer: the report is laid out as a print-ready PDF and handed to the system's print command (`printer`, default the default printer; `copies`). `columns` or a saved `column_config`, `pages`, `group_by`, `sort`, `descending`, `title` and `measurements_only` as in summary_export. dry_run: write the report and return the command without printing.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "printer": { "type": "string", "description": "Printer name (\"\" or absent = the default printer)." },
                "copies": { "type": "integer", "minimum": 1, "maximum": 999 },
                "dry_run": { "type": "boolean" },
                "columns": { "type": "array", "items": { "type": "string" } },
                "column_config": { "type": "string" },
                "include_empty": { "type": "boolean" },
                "pages": pages_arg("to include (default all)"),
                "group_by": { "type": "array", "items": { "type": "string" } },
                "sort": { "type": "string" },
                "descending": { "type": "boolean" },
                "measurements_only": { "type": "boolean" },
                "title": { "type": "string" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let config = match args.opt_str("column_config")? {
            Some(n) => Some(find_config(&a.config_dir()?, n)?),
            None => None,
        };
        let (doc, s) = a.session_ref(args)?;
        let mut o = SummaryOptions {
            columns: args.opt_strings("columns")?.unwrap_or_default(),
            pages: args.opt_pages("pages", s.page_count())?.unwrap_or_default(),
            measurements_only: args.bool_or("measurements_only", false)?,
            group_by: args.opt_strings("group_by")?.unwrap_or_default(),
            sort: args.opt_string("sort")?.unwrap_or_default(),
            descending: args.bool_or("descending", false)?,
            title: args.opt_string("title")?.unwrap_or_default(),
            ..Default::default()
        };
        if let Some(c) = &config {
            o.use_columns(s.doc(), c);
        }
        if !args.bool_or("include_empty", true)? {
            o.drop_empty_columns(s.doc());
        }
        let printer = args.opt_str("printer")?.filter(|p| !p.is_empty());
        let copies = args.opt_u64("copies")?.unwrap_or(1) as usize;
        let r = s.print_summary(&o, printer, copies, args.bool_or("dry_run", false)?)?;
        Ok(json!({
            "doc": doc,
            "markups": r.markups,
            "pdf": r.pdf.display().to_string(),
            "command": r.program,
            "args": r.args,
            "sent": r.sent,
        }))
    },
};

pub static SEARCH_LINK: Tool = Tool {
    name: "search_results_link",
    title: "Hyperlinks from search results",
    description: "Search the page text for `text` (case_sensitive, whole_words, pages as in text_search) and put a hyperlink over each result, or only the checked ones (`checked`: 1-based result numbers in text_search order). The links go to one target: to_page (this document), url, file (with file_page), place or space. One undo step.",
    read_only: false,
    destructive: false,
    schema: || {
        let mut v = json!({
            "text": { "type": "string" },
            "case_sensitive": { "type": "boolean" },
            "whole_words": { "type": "boolean" },
            "pages": pages_arg("to search (default all)"),
            "checked": { "type": "array", "items": { "type": "integer", "minimum": 1 }, "description": "Result numbers (1-based) to link; default every result." },
            "border_width": { "type": "number" },
            "color": { "type": ["string", "array"] }
        });
        if let Some(o) = v.as_object_mut() {
            super::links::target_props(o);
        }
        schema(v, &["text"])
    },
    run: |a, args| {
        let text = args.str("text")?;
        let (doc, s) = a.session(args)?;
        let opts = SearchOptions {
            case_sensitive: args.bool_or("case_sensitive", false)?,
            whole_words: args.bool_or("whole_words", false)?,
            pages: args.opt_pages("pages", s.page_count())?,
            max_hits: 0,
        };
        let found = s.search_text(text, &opts)?;
        let mut hits: Vec<(usize, Vec<markupcraft_engine::Rect>)> =
            found.hits.iter().map(|h| (h.page, h.rects.clone())).collect();
        if let Some(v) = args.get("checked") {
            let picks = v
                .as_array()
                .ok_or_else(|| bad_args("checked: [1, 3, ...]"))?
                .iter()
                .map(|x| match x.as_u64() {
                    Some(n) if n >= 1 && (n as usize) <= hits.len() => Ok(n as usize - 1),
                    _ => Err(bad_args(format!("checked: result numbers 1 to {}", hits.len()))),
                })
                .collect::<crate::Result<Vec<usize>>>()?;
            hits = picks.iter().filter_map(|i| hits.get(*i).cloned()).collect();
        }
        if hits.is_empty() {
            return Err(crate::failed(format!("no results for {text:?} to link")));
        }
        let target = super::links::target_from(s, args)?;
        let mut look = markupcraft_engine::links::LinkLook::default();
        if let Some(w) = args.opt_num("border_width")? {
            look.width = w;
        }
        if let Some(c) = args.opt_color("color")? {
            look.color = c;
        }
        let ids = s.link_hits(&hits, &target, look)?;
        Ok(json!({ "links": ids.len(), "ids": ids, "document": summary(doc, s) }))
    },
};

pub static FAVORITES: Tool = Tool {
    name: "file_favorites",
    title: "File Access favourites",
    description: "The File Access Explorer's favourite folders, kept in the config folder: action list (default), add (`path`, a folder) or remove (`path`).",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({
                "action": { "type": "string", "enum": ["list", "add", "remove"] },
                "path": path_arg("The folder")
            }),
            &[],
        )
    },
    run: |a, args| {
        let dir = a.config_dir()?;
        let v = match args.opt_str("action")?.unwrap_or("list") {
            "list" => load_favorites(&dir)?,
            "add" => add_favorite(&dir, &a.resolve(args.str("path")?, false)?)?,
            "remove" => remove_favorite(&dir, &a.resolve(args.str("path")?, false)?)?,
            o => return Err(bad_args(format!("action {o:?}: list, add or remove"))),
        };
        Ok(json!({ "favorites": v.iter().map(|p| p.display().to_string()).collect::<Vec<_>>() }))
    },
};

pub static PRINT_PREVIEW: Tool = Tool {
    name: "print_preview",
    title: "Print preview",
    description: "Lay the print job out exactly as print_pdf would (same arguments, `out` not needed) and render one sheet (`sheet`, 1-based, default 1) as the Print dialog's preview: the sheet count, the sheet size and margins, and with `png` the sheet as a PNG image (`size`: longer side in pixels, default 800).",
    read_only: false,
    destructive: false,
    schema: || {
        let mut v = super::files::print_props();
        if let Some(o) = v.as_object_mut() {
            o.remove("out");
            o.remove("printer");
            o.insert("sheet".into(), json!({ "type": "integer", "minimum": 1 }));
            o.insert("png".into(), path_arg("Write the sheet here as a PNG"));
            o.insert(
                "size".into(),
                json!({ "type": "integer", "minimum": 64, "maximum": 4000 }),
            );
        }
        schema(v, &[])
    },
    run: |a, args| {
        let png = match args.opt_str("png")? {
            Some(p) => Some(a.resolve(p, true)?),
            None => None,
        };
        let (doc, s) = a.session_ref(args)?;
        let job = super::files::print_job(args, s)?;
        let sheet = args.opt_u64("sheet")?.unwrap_or(1).saturating_sub(1) as usize;
        let size = u32::try_from(args.opt_u64("size")?.unwrap_or(800)).unwrap_or(4_000);
        let p = s.print_preview(&job, sheet, size)?;
        if let Some(out) = &png {
            p.write_png(out)?;
        }
        let (w, h) = p.paper;
        let m = p.margin;
        Ok(json!({
            "doc": doc,
            "sheets": p.sheets,
            "sheet": p.sheet + 1,
            "paper": [w, h],
            "margin": m,
            "printable": [m, m, w - m, h - m],
            "image": [p.width, p.height],
            "png": png.map(|p| p.display().to_string()),
        }))
    },
};
