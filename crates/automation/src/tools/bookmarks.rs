//! Bookmarks and page labels. Bookmark paths are 1-based child positions from the top level:
//! `[2, 1]` is the first child of the second top-level bookmark.

use markupcraft_engine::bookmarks::BookmarkTitles;
use markupcraft_engine::bookmarks_more::{
    BookmarkExport, BookmarkStructure, BookmarkStyle, StructureNode, export_bookmarks, load_structure, save_structure,
};
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

// ---- bookmark properties, actions, copy, AutoMark, structures, audit, export ------------------

pub static STYLE: Tool = Tool {
    name: "bookmark_style",
    title: "Bookmark properties",
    description: "Text colour (\"\" or omitted = black), bold and italic of one or many bookmarks (`paths`). Returns each one's details (title, look, action). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "paths": { "type": "array", "items": path_arg("A bookmark") },
                "color": { "type": ["string", "array"] },
                "bold": { "type": "boolean" },
                "italic": { "type": "boolean" }
            }),
            &["paths"],
        )
    },
    run: |a, args| {
        let mut paths = Vec::new();
        for v in args
            .get("paths")
            .and_then(Value::as_array)
            .ok_or_else(|| bad_args("paths is a list of bookmark paths"))?
        {
            let p: Vec<usize> = v
                .as_array()
                .ok_or_else(|| bad_args("each path is a list of positions from 1"))?
                .iter()
                .map(|x| match x.as_u64() {
                    Some(n) if n >= 1 => Ok(n as usize - 1),
                    _ => Err(bad_args("positions are from 1")),
                })
                .collect::<Result<_>>()?;
            paths.push(p);
        }
        let style = BookmarkStyle {
            color: args.opt_color("color")?,
            bold: args.bool_or("bold", false)?,
            italic: args.bool_or("italic", false)?,
        };
        let (doc, s) = a.session(args)?;
        s.set_bookmark_style(&paths, style)?;
        let details: Vec<Value> = paths
            .iter()
            .filter_map(|p| s.bookmark_details(p).ok())
            .map(|d| details_json(&d))
            .collect();
        Ok(json!({ "bookmarks": details, "document": summary(doc, s) }))
    },
};

fn details_json(d: &markupcraft_engine::bookmarks_more::BookmarkDetails) -> Value {
    json!({
        "path": one_based(&d.path),
        "title": d.title,
        "color": d.style.color.map(|c| c.hex()),
        "bold": d.style.bold,
        "italic": d.style.italic,
        "action": d.target.as_ref().map(super::links::target_json),
    })
}

pub static ACTION: Tool = Tool {
    name: "bookmark_action",
    title: "Bookmark action",
    description: "Where a bookmark goes: the target arguments of link_add (to_page with zoom or view, place, space, url, file with file_page/view, relative). Without a target, returns the bookmark's details. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        let mut p = json!({ "path": path_arg("The bookmark") });
        if let Some(o) = p.as_object_mut() {
            super::links::target_props(o);
        }
        schema(p, &["path"])
    },
    run: |a, args| {
        let p = path(args, "path", true)?.unwrap_or_default();
        let has = ["to_page", "place", "space", "url", "file"].iter().any(|k| args.has(k));
        let (doc, s) = a.session(args)?;
        if has {
            let t = super::links::target_from(s, args)?;
            s.set_bookmark_action(&p, &t)?;
        }
        let d = s.bookmark_details(&p)?;
        Ok(json!({ "bookmark": details_json(&d), "document": summary(doc, s) }))
    },
};

pub static COPY: Tool = Tool {
    name: "bookmark_copy",
    title: "Copy a bookmark",
    description: "Copy a bookmark with its children, look and action to child `index` (1-based; default last) of `parent` (default the top level). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "path": path_arg("The bookmark to copy"),
                "parent": path_arg("The new parent (default: top level)"),
                "index": { "type": "integer", "minimum": 1 }
            }),
            &["path"],
        )
    },
    run: |a, args| {
        let from = path(args, "path", true)?.unwrap_or_default();
        let parent = path(args, "parent", false)?.unwrap_or_default();
        let index = args.opt_u64("index")?.map(|i| (i as usize).saturating_sub(1));
        let (doc, s) = a.session(args)?;
        let p = s.copy_bookmark(&from, &parent, index)?;
        Ok(json!({ "path": one_based(&p), "document": summary(doc, s) }))
    },
};

pub static AUTOMARK: Tool = Tool {
    name: "bookmark_automark",
    title: "AutoMark",
    description: "Create bookmarks from the text inside `region` (a title block's sheet number or title, PDF points) on each page (default all); replace: clear the bookmarks first. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "region": super::rect_arg("The title-block region"),
                "pages": pages_arg("to bookmark (default all)"),
                "replace": { "type": "boolean" }
            }),
            &["region"],
        )
    },
    run: |a, args| {
        let region = args.opt_rect("region")?.ok_or_else(|| bad_args("region is required"))?;
        let replace = args.bool_or("replace", false)?;
        let (doc, s) = a.session(args)?;
        let pages = args.opt_pages("pages", s.page_count())?.unwrap_or_default();
        let n = s.bookmarks_from_region(&pages, region, replace)?;
        Ok(json!({ "added": n, "bookmarks": list_json(s), "document": summary(doc, s) }))
    },
};

pub static STRUCTURE: Tool = Tool {
    name: "bookmark_structure",
    title: "Bookmark structures",
    description: "Bookmarks > Structures: save this document's folder tree as a structure file (`save`), or apply one (`apply`: a structure file, or `folders` [{title, prefixes: [\"A-\"], children}]): its folders are made and every top-level bookmark whose title starts with a folder's prefix is filed into it. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "save": super::path_arg("Write the structure here"),
                "apply": super::path_arg("A structure file to apply"),
                "name": { "type": "string" },
                "folders": { "type": "array", "items": { "type": "object" } }
            }),
            &[],
        )
    },
    run: |a, args| {
        let save = args.opt_str("save")?.map(|p| a.resolve(p, true)).transpose()?;
        let apply = args.opt_str("apply")?.map(|p| a.resolve(p, false)).transpose()?;
        let inline: Option<BookmarkStructure> = match args.get("folders") {
            Some(v) => Some(BookmarkStructure {
                name: args.opt_string("name")?.unwrap_or_default(),
                folders: serde_json::from_value::<Vec<StructureNode>>(v.clone())
                    .map_err(|e| bad_args(format!("folders: {e}")))?,
            }),
            None => None,
        };
        let (doc, s) = a.session(args)?;
        let mut filed = None;
        if let Some(p) = &save {
            let st = s.bookmark_structure(&args.opt_string("name")?.unwrap_or_else(|| "Structure".into()));
            save_structure(p, &st)?;
        }
        let st = match (&apply, inline) {
            (Some(p), _) => Some(load_structure(p)?),
            (None, Some(st)) => Some(st),
            _ => None,
        };
        if let Some(st) = st {
            filed = Some(s.apply_bookmark_structure(&st)?);
        }
        if save.is_none() && filed.is_none() {
            return Err(bad_args("bookmark_structure: give save, apply or folders"));
        }
        Ok(json!({ "filed": filed, "bookmarks": list_json(s), "document": summary(doc, s) }))
    },
};

pub static AUDIT: Tool = Tool {
    name: "bookmark_audit",
    title: "Audit bookmarks",
    description: "Bookmarks that go to a page no longer in the file, a missing Place, or nowhere.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session_ref(args)?;
        let broken: Vec<Value> = s
            .audit_bookmarks()
            .iter()
            .map(|b| json!({ "path": one_based(&b.path), "title": b.title, "reason": b.reason }))
            .collect();
        Ok(json!({ "doc": doc, "broken": broken }))
    },
};

pub static EXPORT: Tool = Tool {
    name: "bookmark_export",
    title: "Export bookmarks",
    description: "Export the bookmarks of `files` (default: this document) to `out`: CSV, or a PDF report (.pdf) with each bookmark linked to its page; tree (indented) or flat, top level only, date stamp, page size by name.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "out": super::path_arg("The CSV or PDF report"),
                "files": { "type": "array", "items": { "type": "string" } },
                "tree": { "type": "boolean" },
                "top_level_only": { "type": "boolean" },
                "links": { "type": "boolean" },
                "date_stamp": { "type": "boolean" },
                "paper": { "type": "string" }
            }),
            &["out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let files: Vec<std::path::PathBuf> = match args.opt_strings("files")? {
            Some(list) => list.iter().map(|f| a.resolve(f, false)).collect::<Result<_>>()?,
            None => {
                let (_, s) = a.session_ref(args)?;
                vec![s.path().to_path_buf()]
            }
        };
        let mut o = BookmarkExport {
            tree: args.bool_or("tree", true)?,
            top_level_only: args.bool_or("top_level_only", false)?,
            links: args.bool_or("links", true)?,
            date_stamp: args.bool_or("date_stamp", true)?,
            ..Default::default()
        };
        if let Some(p) = args.opt_str("paper")? {
            o.page_size =
                markupcraft_engine::printout::paper_size(p).ok_or_else(|| bad_args(format!("unknown paper {p:?}")))?;
        }
        let n = export_bookmarks(&files, &out, &o)?;
        Ok(json!({ "out": out.display().to_string(), "bookmarks": n }))
    },
};
