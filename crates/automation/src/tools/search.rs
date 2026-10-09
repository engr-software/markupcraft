//! Text search across pages, and a page's text.

use markupcraft_engine::search::SearchOptions;
use markupcraft_engine::search_more::{FoundHit, HitSource, SearchTargets, folder_pdfs, search_files};
use serde_json::{Value, json};

use super::{Tool, page_arg, pages_arg, rect_arg, schema};
use crate::bad_args;

fn hit_json(h: &FoundHit) -> Value {
    let (source, id) = match &h.source {
        HitSource::PageText => ("text", None),
        HitSource::Markup(id) => ("markup", Some(id.clone())),
        HitSource::FileName => ("file_name", None),
        HitSource::Property(k) => ("property", Some(k.clone())),
        HitSource::FormField(n) => ("form_field", Some(n.clone())),
    };
    json!({
        "source": source,
        "id": id,
        "page": h.page.map(|p| p + 1),
        "text": h.text,
        "context": h.context,
        "rects": h.rects.iter().map(|r| r.as_array()).collect::<Vec<_>>(),
    })
}

pub static SEARCH: Tool = Tool {
    name: "text_search",
    title: "Search text",
    description: "Find text on the pages (default all; whitespace-insensitive). Each hit has its page, the matched text, the line around it and one rectangle per line it spans ([x0, y0, x1, y1] in PDF points), ready for a highlight markup. Also look in markups, the file name, the document properties and form field values (each hit then says its source), and in other files: files, a folder (recursive), a Set file, or every open document.",
    read_only: true,
    destructive: false,
    schema: || {
        schema(
            json!({
                "text": { "type": "string" },
                "case_sensitive": { "type": "boolean" },
                "whole_words": { "type": "boolean" },
                "pages": pages_arg("to search (default: all)"),
                "max_hits": { "type": "integer", "minimum": 1 },
                "page_text": { "type": "boolean", "description": "Search the page text (default true)." },
                "markups": { "type": "boolean", "description": "Also search markup text (comments, subjects, labels, authors)." },
                "file_names": { "type": "boolean" },
                "properties": { "type": "boolean", "description": "Document properties (title, author, subject, keywords, custom)." },
                "form_fields": { "type": "boolean", "description": "Form field values." },
                "files": { "type": "array", "items": { "type": "string" }, "description": "Search these PDFs instead of the document." },
                "folder": { "type": "string", "description": "Search every PDF in this folder." },
                "recursive": { "type": "boolean", "description": "folder: subfolders too." },
                "set": { "type": "string", "description": "Search every file of this Set (.pcset)." },
                "open_docs": { "type": "boolean", "description": "Search every open document." }
            }),
            &["text"],
        )
    },
    run: |a, args| {
        let text = args.str("text")?;
        let targets = SearchTargets {
            page_text: args.bool_or("page_text", true)?,
            markups: args.bool_or("markups", false)?,
            file_name: args.bool_or("file_names", false)?,
            properties: args.bool_or("properties", false)?,
            form_fields: args.bool_or("form_fields", false)?,
        };
        let plain = targets == SearchTargets::default();
        // Other files: a list, a folder, a Set, or the open documents.
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        if let Some(list) = args.opt_strings("files")? {
            for f in list {
                files.push(a.resolve(&f, false)?);
            }
        }
        if let Some(dir) = args.opt_str("folder")? {
            files.extend(folder_pdfs(&a.resolve(dir, false)?, args.bool_or("recursive", false)?)?);
        }
        if let Some(set) = args.opt_str("set")? {
            files.extend(markupcraft_engine::batch::load_set(&a.resolve(set, false)?)?.files);
        }
        let opts0 = SearchOptions {
            case_sensitive: args.bool_or("case_sensitive", false)?,
            whole_words: args.bool_or("whole_words", false)?,
            pages: None,
            max_hits: args.opt_u64("max_hits")?.unwrap_or(0) as usize,
        };
        if args.bool_or("open_docs", false)? {
            let mut out = Vec::new();
            for (id, s) in a.docs() {
                let hits = s.search_all(text, &opts0, &targets)?;
                out.push(json!({ "doc": id, "path": s.path().display().to_string(), "count": hits.len(), "hits": hits.iter().map(hit_json).collect::<Vec<_>>() }));
            }
            return Ok(json!({ "files": out }));
        }
        if !files.is_empty() {
            let r = search_files(&files, text, &opts0, &targets);
            let total: usize = r.iter().map(|f| f.hits.len()).sum();
            return Ok(json!({
                "count": total,
                "files": r.iter().map(|f| json!({
                    "path": f.path.display().to_string(),
                    "count": f.hits.len(),
                    "hits": f.hits.iter().map(hit_json).collect::<Vec<_>>(),
                    "error": f.error,
                })).collect::<Vec<_>>(),
            }));
        }
        let (doc, s) = a.session_ref(args)?;
        if !plain {
            let o = SearchOptions {
                pages: args.opt_pages("pages", s.page_count())?,
                ..opts0
            };
            let hits = s.search_all(text, &o, &targets)?;
            return Ok(
                json!({ "doc": doc, "count": hits.len(), "hits": hits.iter().map(hit_json).collect::<Vec<_>>() }),
            );
        }
        let opts = SearchOptions {
            case_sensitive: args.bool_or("case_sensitive", false)?,
            whole_words: args.bool_or("whole_words", false)?,
            pages: args.opt_pages("pages", s.page_count())?,
            max_hits: args.opt_u64("max_hits")?.unwrap_or(0) as usize,
        };
        let r = s.search_text(text, &opts)?;
        let hits: Vec<Value> = r
            .hits
            .iter()
            .map(|h| {
                json!({
                    "page": h.page + 1,
                    "text": h.text,
                    "context": h.context,
                    "rects": h.rects.iter().map(|r| r.as_array()).collect::<Vec<_>>(),
                })
            })
            .collect();
        Ok(json!({
            "doc": doc,
            "count": hits.len(),
            "hits": hits,
            "pages_searched": r.pages_searched,
            "unreadable_pages": r.unreadable.iter().map(|p| p + 1).collect::<Vec<_>>(),
            "truncated": r.truncated,
        }))
    },
};

pub static PAGE_TEXT: Tool = Tool {
    name: "page_text",
    title: "Page text",
    description: "The text of a page, lines separated by line breaks.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({ "page": page_arg("to read") }), &["page"]),
    run: |a, args| {
        let page = args.page("page")?;
        let (doc, s) = a.session_ref(args)?;
        let text = s.page_text(page)?;
        Ok(json!({ "doc": doc, "page": page + 1, "text": text }))
    },
};

pub static REPLACE: Tool = Tool {
    name: "text_replace",
    title: "Replace text",
    description: "Search results > Replace: replace `text` with `with` in the page content (not in markups) on the pages given (default all), optionally only in the lines touching `only` ([{\"page\": 1, \"rect\": [...]}], the checked results). A line keeps its font when the font can show the new text; otherwise it is set in Helvetica (fallback true, default) or skipped. One undoable step.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "text": { "type": "string" },
                "with": { "type": "string" },
                "case_sensitive": { "type": "boolean" },
                "whole_words": { "type": "boolean" },
                "pages": pages_arg("to replace on (default: all)"),
                "only": { "type": "array", "items": { "type": "object" } },
                "fallback": { "type": "boolean" }
            }),
            &["text", "with"],
        )
    },
    run: |a, args| {
        let text = args.str("text")?;
        let with = args.str("with")?;
        let mut only = Vec::new();
        if let Some(v) = args.get("only") {
            for o in v.as_array().ok_or_else(|| bad_args("only is a list"))? {
                let page = o
                    .get("page")
                    .and_then(Value::as_u64)
                    .filter(|p| *p >= 1)
                    .ok_or_else(|| bad_args("only: page from 1"))? as usize
                    - 1;
                let rect = o
                    .get("rect")
                    .and_then(crate::args::rect_of)
                    .ok_or_else(|| bad_args("only: rect [x0, y0, x1, y1]"))?;
                only.push((page, rect));
            }
        }
        let (doc, s) = a.session(args)?;
        let opts = SearchOptions {
            case_sensitive: args.bool_or("case_sensitive", false)?,
            whole_words: args.bool_or("whole_words", false)?,
            pages: args.opt_pages("pages", s.page_count())?,
            max_hits: 0,
        };
        let only = (!only.is_empty()).then_some(only);
        let r = s.replace_text(text, with, &opts, only.as_deref(), args.bool_or("fallback", true)?)?;
        Ok(json!({
            "replaced": r.replaced,
            "lines": r.lines,
            "substituted": r.substituted,
            "skipped": r.skipped,
            "document": crate::summary(doc, s),
        }))
    },
};

pub static REGION_TEXT: Tool = Tool {
    name: "region_text",
    title: "Text in a box",
    description: "The text inside `rect` on `page` (the Select Text tool's selection), words in reading order; search for it with text_search.",
    read_only: true,
    destructive: false,
    schema: || {
        schema(
            json!({ "page": page_arg("to read"), "rect": rect_arg("The box") }),
            &["page", "rect"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let rect = args.opt_rect("rect")?.ok_or_else(|| bad_args("rect is required"))?;
        let (doc, s) = a.session_ref(args)?;
        Ok(json!({ "doc": doc, "page": page + 1, "text": s.text_in_rect(page, rect)? }))
    },
};
