//! Hyperlinks (to a page with a zoom, a Place, a view rectangle or a Space, a web address, or
//! another file), Places, links from text and from written URLs, markup actions, snapshots and
//! File Attachment markups.

use markupcraft_engine::Session;
use markupcraft_engine::capture::AttachIcon;
use markupcraft_engine::links::{LinkLook, LinkTarget, Zoom};
use serde_json::{Value, json};

use super::{Tool, page_arg, pages_arg, path_arg, point_arg, rect_arg, schema};
use crate::{Args, Result, bad_args, summary};

pub(crate) fn target_json(t: &LinkTarget) -> Value {
    match t {
        LinkTarget::Page(p) => json!({ "page": p + 1 }),
        LinkTarget::Zoomed { page, zoom } => json!({ "page": page + 1, "zoom": zoom.name() }),
        LinkTarget::View { page, rect } => json!({ "page": page + 1, "view": rect.as_array() }),
        LinkTarget::Place(n) => json!({ "place": n }),
        LinkTarget::Url(u) => json!({ "url": u }),
        LinkTarget::File { path, page } => json!({ "file": path, "file_page": page.map(|p| p + 1) }),
        LinkTarget::FileView { path, page, rect } => {
            json!({ "file": path, "file_page": page + 1, "view": rect.as_array() })
        }
        LinkTarget::FilePlace { path, name } => json!({ "file": path, "place": name }),
        LinkTarget::Other(o) => json!({ "other": o }),
    }
}

/// The target arguments every link, bookmark and markup action takes.
pub(crate) fn target_props(o: &mut serde_json::Map<String, Value>) {
    o.insert("to_page".into(), page_arg("of this document it goes to"));
    o.insert(
        "zoom".into(),
        json!({ "type": "string", "enum": ["fit_page", "fit_width", "actual", "inherit"], "description": "to_page: how the page shows (default fit_page)." }),
    );
    o.insert(
        "view".into(),
        rect_arg("to_page (or file_page): show exactly this rectangle (a snapshot view)"),
    );
    o.insert(
        "place".into(),
        json!({ "type": "string", "description": "A Place (named destination) of this document." }),
    );
    o.insert(
        "space".into(),
        json!({ "type": "string", "description": "A Space id: shows its box." }),
    );
    o.insert("url".into(), json!({ "type": "string" }));
    o.insert(
        "file".into(),
        json!({ "type": "string", "description": "A file path or name, stored as typed (not resolved)." }),
    );
    o.insert("file_page".into(), page_arg("of that PDF to open"));
    o.insert(
        "relative".into(),
        json!({ "type": "boolean", "description": "file: store it relative to this document's folder." }),
    );
}

/// The target the arguments name.
pub(crate) fn target_from(s: &Session, args: &Args) -> Result<LinkTarget> {
    let to_page = args.opt_page("to_page")?;
    let place = args.opt_string("place")?;
    let space = args.opt_string("space")?;
    let url = args.opt_string("url")?;
    let file = args.opt_string("file")?;
    let given = [
        to_page.is_some(),
        place.is_some(),
        space.is_some(),
        url.is_some(),
        file.is_some(),
    ]
    .iter()
    .filter(|b| **b)
    .count();
    if given != 1 {
        return Err(bad_args(format!(
            "{}: give exactly one of to_page, place, space, url or file",
            args.tool()
        )));
    }
    let view = args.opt_rect("view")?;
    if let Some(p) = to_page {
        return Ok(match (view, args.opt_str("zoom")?) {
            (Some(rect), _) => LinkTarget::View { page: p, rect },
            (None, Some(z)) => LinkTarget::Zoomed {
                page: p,
                zoom: Zoom::from_name(z).ok_or_else(|| bad_args(format!("unknown zoom {z:?}")))?,
            },
            (None, None) => LinkTarget::Page(p),
        });
    }
    if let Some(n) = place {
        return Ok(LinkTarget::Place(n));
    }
    if let Some(id) = space {
        let (page, sp) = s
            .spaces(None)
            .into_iter()
            .find(|(_, x)| x.id == id)
            .ok_or_else(|| bad_args(format!("no space {id:?} (space_list shows them)")))?;
        let first = sp.pts.first().ok_or_else(|| bad_args("the space has no outline"))?;
        let mut rect = markupcraft_engine::Rect::new(first.x, first.y, first.x, first.y);
        for p in &sp.pts {
            rect =
                markupcraft_engine::Rect::new(rect.x0.min(p.x), rect.y0.min(p.y), rect.x1.max(p.x), rect.y1.max(p.y));
        }
        return Ok(LinkTarget::View { page, rect });
    }
    if let Some(u) = url {
        return Ok(LinkTarget::Url(u));
    }
    let mut path = file.unwrap_or_default();
    if args.bool_or("relative", false)?
        && let Some(base) = s.path().parent()
        && let Ok(rel) = std::path::Path::new(&path).strip_prefix(base)
    {
        path = rel.to_string_lossy().replace('\\', "/");
    }
    let fp = args.opt_page("file_page")?;
    Ok(match (fp, view) {
        (Some(page), Some(rect)) => LinkTarget::FileView { path, page, rect },
        (fp, _) => LinkTarget::File { path, page: fp },
    })
}

fn look_from(args: &Args) -> Result<LinkLook> {
    let mut look = LinkLook::default();
    if let Some(w) = args.opt_num("border_width")? {
        look.width = w;
    }
    if let Some(c) = args.opt_color("color")? {
        look.color = c;
    }
    Ok(look)
}

pub static LIST: Tool = Tool {
    name: "link_list",
    title: "List links",
    description: "Every hyperlink: id, page, rectangle and where it goes (page and zoom, view, Place, url or file).",
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
    description: "Add a hyperlink over `rect` on `page` (or, with on_text, over the words inside rect) that goes to one of: `to_page` (with `zoom`, or `view` a rectangle), `place` (a Place), `space` (a Space's box), `url`, or `file` (with `file_page` and `view`, a rectangle of another PDF; `relative` stores the path relative to this document). Invisible unless `border_width` > 0. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        let mut p = json!({
            "page": page_arg("the link is on"),
            "rect": rect_arg("The clickable area"),
            "on_text": { "type": "boolean", "description": "Link the words inside rect (the box fits them)." },
            "border_width": { "type": "number", "minimum": 0 },
            "color": { "type": ["string", "array"], "description": "Border colour." }
        });
        if let Some(o) = p.as_object_mut() {
            target_props(o);
        }
        schema(p, &["page", "rect"])
    },
    run: |a, args| {
        let page = args.page("page")?;
        let rect = args
            .opt_rect("rect")?
            .ok_or_else(|| bad_args("link_add: missing argument rect"))?;
        let look = look_from(args)?;
        let on_text = args.bool_or("on_text", false)?;
        let (doc, s) = a.session(args)?;
        let target = target_from(s, args)?;
        let id = if on_text {
            s.add_link_on_text(page, rect, &target, look)?
        } else {
            s.add_link(page, rect, &target, look)?
        };
        Ok(json!({ "id": id, "document": summary(doc, s) }))
    },
};

pub static EDIT: Tool = Tool {
    name: "link_edit",
    title: "Edit a link",
    description: "Edit Action on a link: change where it goes (the target arguments of link_add), its rect, or its look. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        let mut p = json!({
            "id": { "type": "string" },
            "rect": rect_arg("The new clickable area"),
            "border_width": { "type": "number", "minimum": 0 },
            "color": { "type": ["string", "array"] }
        });
        if let Some(o) = p.as_object_mut() {
            target_props(o);
            // `view` without to_page/file_page would be ambiguous here; it rides along with them.
        }
        schema(p, &["id"])
    },
    run: |a, args| {
        let id = args.str("id")?.to_string();
        let rect = args.opt_rect("rect")?;
        let has_target = ["to_page", "place", "space", "url", "file"].iter().any(|k| args.has(k));
        let look = (args.has("border_width") || args.has("color"))
            .then(|| look_from(args))
            .transpose()?;
        let (doc, s) = a.session(args)?;
        let target = if has_target { Some(target_from(s, args)?) } else { None };
        s.edit_link(&id, target.as_ref(), rect, look)?;
        Ok(json!({ "id": id, "document": summary(doc, s) }))
    },
};

pub static FROM_URLS: Tool = Tool {
    name: "link_from_urls",
    title: "Create hyperlinks from URLs",
    description: "Turn every web address written on the pages (default all; http://, https:// or www.) into a link, skipping text already linked. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to scan (default all)"),
                "border_width": { "type": "number", "minimum": 0 },
                "color": { "type": ["string", "array"] }
            }),
            &[],
        )
    },
    run: |a, args| {
        let look = look_from(args)?;
        let (doc, s) = a.session(args)?;
        let pages = args.opt_pages("pages", s.page_count())?.unwrap_or_default();
        let n = s.links_from_urls(&pages, look)?;
        Ok(json!({ "added": n, "document": summary(doc, s) }))
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

pub static PLACES: Tool = Tool {
    name: "place_list",
    title: "List Places",
    description: "The Links panel's Places: named destinations with their page and view (left, top, zoom).",
    read_only: true,
    destructive: false,
    schema: || schema(json!({ "page": page_arg("to list (default all)") }), &[]),
    run: |a, args| {
        let page = args.opt_page("page")?;
        let (doc, s) = a.session_ref(args)?;
        let list: Vec<Value> = s
            .places()
            .iter()
            .filter(|p| page.is_none_or(|x| p.page == Some(x)))
            .map(|p| json!({ "name": p.name, "page": p.page.map(|x| x + 1), "left": p.left, "top": p.top, "zoom": p.zoom }))
            .collect();
        Ok(json!({ "doc": doc, "places": list }))
    },
};

pub static PLACE_SET: Tool = Tool {
    name: "place_set",
    title: "Add or move a Place",
    description: "Add a Place (a named destination) or move one: `name` goes to `page` with the view's `left`, `top` (PDF points) and `zoom` (1 = 100%; omitted = the reader's). Links and bookmarks that go to the Place follow it. rename: give `new_name`. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "name": { "type": "string" },
                "new_name": { "type": "string" },
                "page": page_arg("it goes to"),
                "left": { "type": "number" },
                "top": { "type": "number" },
                "zoom": { "type": "number" }
            }),
            &["name"],
        )
    },
    run: |a, args| {
        let name = args.str("name")?.to_string();
        let (doc, s) = a.session(args)?;
        if let Some(n) = args.opt_str("new_name")? {
            s.rename_place(&name, n)?;
        } else {
            let page = args.page("page")?;
            s.set_place(
                &name,
                page,
                args.opt_num("left")?,
                args.opt_num("top")?,
                args.opt_num("zoom")?,
            )?;
        }
        Ok(json!({ "places": s.places().len(), "document": summary(doc, s) }))
    },
};

pub static PLACE_DELETE: Tool = Tool {
    name: "place_delete",
    title: "Delete Places",
    description: "Delete Places by name (links to them stop working). Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "names": { "type": "array", "items": { "type": "string" } } }),
            &["names"],
        )
    },
    run: |a, args| {
        let names = args.opt_strings("names")?.unwrap_or_default();
        let (doc, s) = a.session(args)?;
        let n = s.delete_places(&names)?;
        Ok(json!({ "deleted": n, "document": summary(doc, s) }))
    },
};

pub static MARKUP_ACTION: Tool = Tool {
    name: "markup_action",
    title: "Markup action",
    description: "Edit Action on a markup: what clicking it does (the target arguments of link_add), or remove: true. Without a target or remove, returns the current action. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        let mut p = json!({
            "id": { "type": "string" },
            "remove": { "type": "boolean" }
        });
        if let Some(o) = p.as_object_mut() {
            target_props(o);
        }
        schema(p, &["id"])
    },
    run: |a, args| {
        let id = args.str("id")?.to_string();
        let has_target = ["to_page", "place", "space", "url", "file"].iter().any(|k| args.has(k));
        let remove = args.bool_or("remove", false)?;
        let (doc, s) = a.session(args)?;
        if has_target {
            let t = target_from(s, args)?;
            s.set_markup_action(&id, Some(&t))?;
        } else if remove {
            s.set_markup_action(&id, None)?;
        }
        let now = s.markup_action(&id)?;
        Ok(json!({ "id": id, "action": now.as_ref().map(target_json), "document": summary(doc, s) }))
    },
};

pub static SNAPSHOT: Tool = Tool {
    name: "snapshot_copy",
    title: "Snapshot to the clipboard",
    description: "Copy a snapshot (the page content of a region, kept vector) to the clipboard: `rect` on `page`, the whole page (Copy Page to Snapshot: no rect), or a Space's box (`space`). markup_paste places it.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("to capture"),
                "rect": rect_arg("The region (default: the whole page)"),
                "space": { "type": "string", "description": "A Space id: its box." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let page = args.opt_page("page")?.unwrap_or(0);
        let rect = args.opt_rect("rect")?;
        let space = args.opt_string("space")?;
        let (doc, s) = a.session(args)?;
        let r = s.snapshot_to_clipboard(page, rect, space.as_deref())?;
        let clip = s.clipboard().to_vec();
        a.set_clipboard(clip);
        Ok(json!({ "doc": doc, "rect": r.as_array(), "clipboard": 1 }))
    },
};

pub static ATTACH: Tool = Tool {
    name: "attachment_markup_add",
    title: "File Attachment markup",
    description: "Tools > File Attachment: embed the file at `path` as a markup shown as a paperclip (or pin) icon at `at` on `page`. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("for the icon"),
                "at": point_arg("The icon's lower-left corner"),
                "path": path_arg("The file to embed"),
                "icon": { "type": "string", "enum": ["paperclip", "pushpin"] },
                "description": { "type": "string" }
            }),
            &["page", "at", "path"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let at = args.point("at")?;
        let path = a.resolve(args.str("path")?, false)?;
        let icon = match args.opt_str("icon")? {
            Some(i) => AttachIcon::from_name(i).ok_or_else(|| bad_args(format!("unknown icon {i:?}")))?,
            None => AttachIcon::Paperclip,
        };
        let desc = args.opt_string("description")?.unwrap_or_default();
        let (doc, s) = a.session(args)?;
        let id = s.add_file_attachment(page, at, &path, icon, &desc)?;
        Ok(json!({ "id": id, "document": summary(doc, s) }))
    },
};

pub static CAPTURE: Tool = Tool {
    name: "capture_summary",
    title: "Capture summary",
    description: "Every File Attachment markup (page, file, size, description); `csv` writes the summary, `dir` saves every attached file into that folder (Export Capture Media).",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "csv": path_arg("Write the summary here"),
                "dir": path_arg("Save the attached files into this folder")
            }),
            &[],
        )
    },
    run: |a, args| {
        let csv = args.opt_str("csv")?.map(|p| a.resolve(p, true)).transpose()?;
        let dir = args.opt_str("dir")?.map(|p| a.resolve(p, true)).transpose()?;
        let (doc, s) = a.session_ref(args)?;
        let list: Vec<Value> = s
            .attachment_markups()
            .iter()
            .map(|m| json!({ "id": m.id, "page": m.page + 1, "file": m.file, "size": m.size, "description": m.description }))
            .collect();
        if let Some(p) = &csv {
            std::fs::write(p, s.capture_summary_csv()).map_err(crate::failed)?;
        }
        let files = match &dir {
            Some(d) => s
                .export_attachment_markups(d)?
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            None => Vec::new(),
        };
        Ok(json!({ "doc": doc, "attachments": list, "files": files }))
    },
};
