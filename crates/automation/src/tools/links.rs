//! Hyperlinks: to a page, a web address or another file.

use markupcraft_engine::links::{LinkLook, LinkTarget};
use serde_json::{Value, json};

use super::{Tool, page_arg, rect_arg, schema};
use crate::{bad_args, summary};

fn target_json(t: &LinkTarget) -> Value {
    match t {
        LinkTarget::Page(p) => json!({ "page": p + 1 }),
        LinkTarget::Url(u) => json!({ "url": u }),
        LinkTarget::File { path, page } => json!({ "file": path, "file_page": page.map(|p| p + 1) }),
        LinkTarget::Other(o) => json!({ "other": o }),
    }
}

pub static LIST: Tool = Tool {
    name: "link_list",
    title: "List links",
    description: "Every hyperlink: id, page, rectangle and where it goes (page, url or file).",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session_ref(args)?;
        let links: Vec<Value> = s
            .links()
            .iter()
            .map(|l| {
                json!({
                    "id": l.id,
                    "page": l.page + 1,
                    "rect": l.rect.as_array(),
                    "target": target_json(&l.target),
                })
            })
            .collect();
        Ok(json!({ "doc": doc, "links": links }))
    },
};

pub static ADD: Tool = Tool {
    name: "link_add",
    title: "Add a link",
    description: "Add a hyperlink over `rect` on `page` that goes to one of: `to_page` (this document), `url` (a web address), or `file` (another file; with `file_page`, a page of another PDF). Invisible unless `border_width` > 0. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("the link is on"),
                "rect": rect_arg("The clickable area"),
                "to_page": page_arg("of this document it goes to"),
                "url": { "type": "string" },
                "file": { "type": "string", "description": "A file path or name, stored as typed (not resolved)." },
                "file_page": page_arg("of that PDF to open"),
                "border_width": { "type": "number", "minimum": 0 },
                "color": { "type": ["string", "array"], "description": "Border colour." }
            }),
            &["page", "rect"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let rect = args
            .opt_rect("rect")?
            .ok_or_else(|| bad_args("link_add: missing argument rect"))?;
        let to_page = args.opt_page("to_page")?;
        let url = args.opt_string("url")?;
        let file = args.opt_string("file")?;
        let target = match (to_page, url, file) {
            (Some(p), None, None) => LinkTarget::Page(p),
            (None, Some(u), None) => LinkTarget::Url(u),
            (None, None, Some(f)) => LinkTarget::File {
                path: f,
                page: args.opt_page("file_page")?,
            },
            _ => return Err(bad_args("link_add: give exactly one of to_page, url or file")),
        };
        let mut look = LinkLook::default();
        if let Some(w) = args.opt_num("border_width")? {
            look.width = w;
        }
        if let Some(c) = args.opt_color("color")? {
            look.color = c;
        }
        let (doc, s) = a.session(args)?;
        let id = s.add_link(page, rect, &target, look)?;
        Ok(json!({ "id": id, "document": summary(doc, s) }))
    },
};

pub static DELETE: Tool = Tool {
    name: "link_delete",
    title: "Delete links",
    description: "Delete links by id (link_list shows them). Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "ids": { "type": "array", "items": { "type": "string" } } }),
            &["ids"],
        )
    },
    run: |a, args| {
        let ids = args.opt_strings("ids")?.unwrap_or_default();
        let (doc, s) = a.session(args)?;
        let n = s.delete_links(&ids)?;
        Ok(json!({ "deleted": n, "document": summary(doc, s) }))
    },
};
