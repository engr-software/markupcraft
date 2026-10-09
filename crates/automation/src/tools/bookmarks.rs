//! Bookmarks and page labels. Bookmark paths are 1-based child positions from the top level:
//! `[2, 1]` is the first child of the second top-level bookmark.

use markupcraft_engine::bookmarks::BookmarkTitles;
use markupcraft_engine::labels::LabelStyle;
use serde_json::{Value, json};

use super::{Tool, page_arg, pages_arg, schema};
use crate::{Args, Result, bad_args, summary};

fn path_arg(what: &str) -> Value {
    json!({ "type": "array", "items": { "type": "integer", "minimum": 1 }, "description": format!("{what}: 1-based positions from the top level, e.g. [2, 1] = first child of the second bookmark (bookmark_list shows them).") })
}

/// A 1-based bookmark path argument → 0-based.
fn path(a: &Args, key: &str, required: bool) -> Result<Option<Vec<usize>>> {
    let Some(v) = a.get(key) else {
        return if required {
            Err(bad_args(format!("{}: missing argument {key}", a.tool())))
        } else {
            Ok(None)
        };
    };
    let arr = v
        .as_array()
        .ok_or_else(|| bad_args(format!("{key} must be a list of positions from 1")))?;
    if arr.len() > 64 {
        return Err(bad_args(format!("{key} is too deep")));
    }
    arr.iter()
        .map(|x| match x.as_u64() {
            Some(p) if p >= 1 => Ok(p as usize - 1),
            _ => Err(bad_args(format!("{key} must be a list of positions from 1"))),
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn one_based(p: &[usize]) -> Vec<usize> {
    p.iter().map(|i| i + 1).collect()
}

fn list_json(s: &markupcraft_engine::Session) -> Value {
    Value::Array(
        s.bookmarks()
            .iter()
            .map(|b| {
                json!({
                    "path": one_based(&b.path),
                    "title": b.title,
                    "page": b.page.map(|p| p + 1),
                    "depth": b.path.len().saturating_sub(1),
                    "open": b.open,
                    "children": b.children,
                })
            })
            .collect(),
    )
}

pub static LIST: Tool = Tool {
    name: "bookmark_list",
    title: "List bookmarks",
    description: "Every bookmark, parents before their children, with its path, title, depth and the page it goes to.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session_ref(args)?;
        Ok(json!({ "doc": doc, "bookmarks": list_json(s) }))
    },
};

pub static ADD: Tool = Tool {
    name: "bookmark_add",
    title: "Add a bookmark",
    description: "Add a bookmark that goes to `page`, under `parent` (default: the top level) at `index` (1-based; default: last). Title defaults to the page label. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("the bookmark goes to"),
                "title": { "type": "string" },
                "parent": path_arg("The parent bookmark"),
                "index": { "type": "integer", "minimum": 1, "description": "1-based position among the parent's children." }
            }),
            &["page"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let parent = path(args, "parent", false)?.unwrap_or_default();
        let index = args.opt_page("index")?;
        let title = args.opt_string("title")?;
        let (doc, s) = a.session(args)?;
        let title = match title {
            Some(t) => t,
            None => {
                let label = s.page(page)?.label.trim().to_string();
                if label.is_empty() {
                    format!("Page {}", page + 1)
                } else {
                    label
                }
            }
        };
        let p = s.add_bookmark(&parent, index, &title, page)?;
        Ok(json!({ "path": one_based(&p), "title": title, "document": summary(doc, s) }))
    },
};

pub static EDIT: Tool = Tool {
    name: "bookmark_edit",
    title: "Edit a bookmark",
    description: "Rename a bookmark, point it at another page, or expand/collapse it. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "path": path_arg("The bookmark"),
                "title": { "type": "string" },
                "page": page_arg("it goes to"),
                "open": { "type": "boolean", "description": "Expanded (true) or collapsed." }
            }),
            &["path"],
        )
    },
    run: |a, args| {
        let p = path(args, "path", true)?.unwrap_or_default();
        let title = args.opt_string("title")?;
        let page = args.opt_page("page")?;
        let open = args.opt_bool("open")?;
        if title.is_none() && page.is_none() && open.is_none() {
            return Err(bad_args("bookmark_edit: give title, page or open"));
        }
        let (doc, s) = a.session(args)?;
        if let Some(t) = title {
            s.rename_bookmark(&p, &t)?;
        }
        if let Some(pg) = page {
            s.set_bookmark_page(&p, pg)?;
        }
        if let Some(o) = open {
            s.set_bookmark_open(&p, o)?;
        }
        Ok(json!({ "bookmarks": list_json(s), "document": summary(doc, s) }))
    },
};

pub static MOVE: Tool = Tool {
    name: "bookmark_move",
    title: "Move or nest a bookmark",
    description: "Move a bookmark (with its children) under `parent` (default: the top level) at `index` (1-based, counted after taking it out; default: last). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "path": path_arg("The bookmark to move"),
                "parent": path_arg("The new parent"),
                "index": { "type": "integer", "minimum": 1 }
            }),
            &["path"],
        )
    },
    run: |a, args| {
        let from = path(args, "path", true)?.unwrap_or_default();
        let parent = path(args, "parent", false)?.unwrap_or_default();
        let index = args.opt_page("index")?;
        let (doc, s) = a.session(args)?;
        let p = s.move_bookmark(&from, &parent, index)?;
        Ok(json!({ "path": one_based(&p), "document": summary(doc, s) }))
    },
};

pub static DELETE: Tool = Tool {
    name: "bookmark_delete",
    title: "Delete bookmarks",
    description: "Delete a bookmark and everything under it, or every bookmark with all: true. Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "path": path_arg("The bookmark"), "all": { "type": "boolean" } }),
            &[],
        )
    },
    run: |a, args| {
        let p = path(args, "path", false)?;
        let all = args.bool_or("all", false)?;
        let (doc, s) = a.session(args)?;
        let deleted = match (p, all) {
            (Some(p), false) => {
                s.delete_bookmark(&p)?;
                1
            }
            (None, true) => s.clear_bookmarks()?,
            _ => return Err(bad_args("bookmark_delete: give path, or all: true")),
        };
        Ok(json!({ "deleted": deleted, "document": summary(doc, s) }))
    },
};

pub static CREATE: Tool = Tool {
    name: "bookmark_create",
    title: "Create bookmarks from pages",
    description: "One top-level bookmark per page (default all), titled by its page label (titles: \"labels\") or \"Page N\" (titles: \"numbers\"). replace: true deletes the existing bookmarks first; otherwise the new ones follow them. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to bookmark (default: all)"),
                "titles": { "type": "string", "enum": ["labels", "numbers"] },
                "replace": { "type": "boolean" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let titles = match args.opt_str("titles")?.unwrap_or("labels") {
            "labels" => BookmarkTitles::Labels,
            "numbers" => BookmarkTitles::PageNumbers,
            other => return Err(bad_args(format!("titles must be labels or numbers, not {other:?}"))),
        };
        let replace = args.bool_or("replace", false)?;
        let (doc, s) = a.session(args)?;
        let pages = args
            .opt_pages("pages", s.page_count())?
            .unwrap_or_else(|| (0..s.page_count()).collect());
        let n = s.bookmarks_from_pages(&pages, titles, replace)?;
        Ok(json!({ "created": n, "bookmarks": list_json(s), "document": summary(doc, s) }))
    },
};

pub static NUMBER: Tool = Tool {
    name: "page_label_number",
    title: "Number pages",
    description: "Number pages (Number Pages): the pages get labels prefix + number counting up from `start` in a style: decimal (1, 2), roman (I, II), roman_lower, alpha (A, B), alpha_lower, or none (the prefix alone). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to number (default: all)"),
                "style": { "type": "string", "enum": ["decimal", "roman", "roman_lower", "alpha", "alpha_lower", "none"] },
                "prefix": { "type": "string" },
                "start": { "type": "integer", "minimum": 1 }
            }),
            &[],
        )
    },
    run: |a, args| {
        let style = match args.opt_str("style")?.unwrap_or("decimal") {
            "decimal" => Some(LabelStyle::Decimal),
            "roman" => Some(LabelStyle::UpperRoman),
            "roman_lower" => Some(LabelStyle::LowerRoman),
            "alpha" => Some(LabelStyle::UpperAlpha),
            "alpha_lower" => Some(LabelStyle::LowerAlpha),
            "none" => None,
            other => return Err(bad_args(format!("unknown style {other:?}"))),
        };
        let prefix = args.opt_string("prefix")?.unwrap_or_default();
        let start = args.opt_int("start")?.unwrap_or(1);
        let (doc, s) = a.session(args)?;
        let pages = args
            .opt_pages("pages", s.page_count())?
            .unwrap_or_else(|| (0..s.page_count()).collect());
        s.number_pages(&pages, style, &prefix, start)?;
        Ok(json!({ "labels": s.page_labels(), "document": summary(doc, s) }))
    },
};

pub static SET: Tool = Tool {
    name: "page_label_set",
    title: "Set page labels",
    description: "Type labels for pages: {\"labels\": {\"3\": \"A-101\", \"4\": \"A-102\"}} (1-based page: text). \"\" puts the page number back. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "labels": { "type": "object", "description": "1-based page number (as a string key) to label text." } }),
            &["labels"],
        )
    },
    run: |a, args| {
        let map = args
            .get("labels")
            .and_then(Value::as_object)
            .ok_or_else(|| bad_args("labels must be an object of page: text"))?;
        let mut set = Vec::with_capacity(map.len());
        for (k, v) in map {
            let page = k
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|p| *p >= 1)
                .ok_or_else(|| bad_args(format!("{k:?} is not a page number from 1")))?;
            let text = v
                .as_str()
                .ok_or_else(|| bad_args(format!("the label for page {k} must be text")))?;
            set.push((page - 1, text.to_string()));
        }
        let (doc, s) = a.session(args)?;
        s.set_page_labels(&set)?;
        Ok(json!({ "labels": s.page_labels(), "document": summary(doc, s) }))
    },
};

pub static CLEAR: Tool = Tool {
    name: "page_label_clear",
    title: "Clear page labels",
    description: "Remove every page label (pages then show their numbers). Undoable.",
    read_only: false,
    destructive: true,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let had = s.clear_page_labels()?;
        Ok(json!({ "cleared": had, "labels": s.page_labels(), "document": summary(doc, s) }))
    },
};

pub static FROM_BOOKMARKS: Tool = Tool {
    name: "page_label_from_bookmarks",
    title: "Page labels from bookmarks",
    description: "Label each page with the title of the first bookmark that goes to it; other pages keep their labels. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let n = s.labels_from_bookmarks()?;
        Ok(json!({ "labelled": n, "labels": s.page_labels(), "document": summary(doc, s) }))
    },
};
