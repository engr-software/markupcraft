//! Text search across pages, and a page's text.

use markupcraft_engine::search::SearchOptions;
use serde_json::{Value, json};

use super::{Tool, page_arg, pages_arg, schema};

pub static SEARCH: Tool = Tool {
    name: "text_search",
    title: "Search text",
    description: "Find text on the pages (default all; whitespace-insensitive). Each hit has its page, the matched text, the line around it and one rectangle per line it spans ([x0, y0, x1, y1] in PDF points), ready for a highlight markup.",
    read_only: true,
    destructive: false,
    schema: || {
        schema(
            json!({
                "text": { "type": "string" },
                "case_sensitive": { "type": "boolean" },
                "whole_words": { "type": "boolean" },
                "pages": pages_arg("to search (default: all)"),
                "max_hits": { "type": "integer", "minimum": 1 }
            }),
            &["text"],
        )
    },
    run: |a, args| {
        let text = args.str("text")?;
        let (doc, s) = a.session_ref(args)?;
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
