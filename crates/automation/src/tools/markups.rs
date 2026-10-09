//! Markups: list, add, edit, transform, delete, duplicate, arrange, copy, paste, group,
//! ungroup, select.

use markupcraft_engine::{Arrange, Markup, props};
use serde_json::{Value, json};

use super::{Tool, ids_arg, markup_json, markup_props, page_arg, point_arg, points_arg, rect_arg, schema, target_ids};
use crate::{Result, bad_args, summary};

pub static LIST: Tool = Tool {
    name: "markup_list",
    title: "List markups",
    description: "The Markups List: every markup (or those on `page`, of `kind`, or with `subject`) with its id, page, kind, subject, label, author, look, points, quantity and unit.",
    read_only: true,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("to list"),
                "kind": { "type": "string", "description": "Only this kind (Area, Length, Count, Polygon, ...)." },
                "subject": { "type": "string", "description": "Only this subject." },
                "limit": { "type": "integer", "minimum": 1, "description": "At most this many (default all)." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let page = args.opt_page("page")?;
        let kind = if args.has("kind") {
            Some(args.kind("kind")?)
        } else {
            None
        };
        let subject = args.opt_str("subject")?;
        let limit = args.opt_u64("limit")?.map_or(usize::MAX, |l| l as usize);
        let (id, s) = a.session_ref(args)?;
        if let Some(p) = page {
            s.page(p)?;
        }
        let list: Vec<Value> = s
            .doc()
            .markups
            .iter()
            .filter(|m| page.is_none_or(|p| m.page == p))
            .filter(|m| kind.is_none_or(|k| m.kind == k))
            .filter(|m| subject.is_none_or(|t| m.subject == t))
            .take(limit)
            .map(markup_json)
            .collect();
        Ok(json!({ "doc": id, "count": list.len(), "markups": list }))
    },
};

pub static ADD: Tool = Tool {
    name: "markup_add",
    title: "Add a markup",
    description: "Add a markup of any creatable kind on a page. Measurements take the page's scale at their first point unless `scale` is given. Rectangle and Ellipse take two opposite corners. Returns the new markup (it becomes the selection). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            markup_props(json!({
                "page": page_arg("to add it to"),
                "kind": { "type": "string", "description": "Area, Perimeter, Length, Polylength, Count, Polygon, Polyline, Line, Rectangle, Ellipse, Pen ..." },
                "points": points_arg("The vertices"),
                "holes": { "type": "array", "items": points_arg("A cutout"), "description": "Area cutouts." }
            })),
            &["page", "kind", "points"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let kind = args.kind("kind")?;
        let pts = args.points("points")?;
        let patch = args.patch()?;
        let mut m = Markup::new(kind, page, pts);
        if let Some(h) = args.get("holes") {
            let rings = h
                .as_array()
                .ok_or_else(|| bad_args("holes must be a list of point lists"))?;
            for r in rings {
                m.holes
                    .push(crate::args::points_of(r).ok_or_else(|| bad_args("holes must be a list of point lists"))?);
            }
        }
        // Defaults a new markup gets before the given properties.
        if kind.is_measurement() {
            m.line_width = 2.0;
        }
        let locked = patch.locked;
        let mut p = patch.clone();
        p.locked = None;
        if props::can_create(kind) {
            // (otherwise add_markup explains which kinds can be created)
            p.apply(&mut m).map_err(crate::failed)?;
        }
        let (doc, s) = a.session(args)?;
        let id = s.add_markup(m)?;
        if locked == Some(true) {
            s.set_locked(std::slice::from_ref(&id), true)?;
        }
        Ok(json!({ "id": id, "markup": markup_json(s.markup(&id)?), "document": summary(doc, s) }))
    },
};

pub static EDIT: Tool = Tool {
    name: "markup_edit",
    title: "Edit markup properties",
    description: "Change properties of markups (ids, or the selection): colour, fill, opacity, width, dash, subject, label, contents, layer, status, lock, font, custom columns, scale. A locked markup takes only locked: false (with or without other changes). Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(markup_props(json!({ "ids": ids_arg() })), &[]),
    run: |a, args| {
        let patch = args.patch()?;
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let n = s.set_properties(&ids, &patch)?;
        let list: Vec<Value> = ids.iter().filter_map(|id| s.markup(id).ok()).map(markup_json).collect();
        Ok(json!({ "changed": n, "markups": list, "document": summary(doc, s) }))
    },
};

pub static TRANSFORM: Tool = Tool {
    name: "markup_transform",
    title: "Move, rotate, resize or reshape markups",
    description: "One geometry change per call: move [dx, dy] (ids or selection); rotate degrees counter-clockwise about center or each markup's own centre; resize one markup's box to [x0, y0, x1, y1]; points replaces one markup's vertices; move_vertex / insert_vertex {index (1-based), point}; delete_vertex index. Locked markups are refused. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        let vertex = json!({ "type": "object", "properties": { "index": { "type": "integer", "minimum": 1 }, "point": point_arg("Where") }, "required": ["index", "point"] });
        schema(
            json!({
                "ids": ids_arg(),
                "move": point_arg("Offset [dx, dy]"),
                "rotate": { "type": "number", "description": "Degrees, counter-clockwise." },
                "center": point_arg("Centre of rotation"),
                "resize": rect_arg("New box of the markup's points"),
                "points": points_arg("New vertices"),
                "move_vertex": vertex.clone(),
                "insert_vertex": vertex,
                "delete_vertex": { "type": "integer", "minimum": 1, "description": "1-based vertex to remove." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let ops = [
            "move",
            "rotate",
            "resize",
            "points",
            "move_vertex",
            "insert_vertex",
            "delete_vertex",
        ];
        let given: Vec<&str> = ops.iter().copied().filter(|k| args.has(k)).collect();
        if given.len() != 1 {
            return Err(bad_args(format!(
                "markup_transform takes exactly one of {} (got {})",
                ops.join(", "),
                if given.is_empty() {
                    "none".into()
                } else {
                    given.join(", ")
                }
            )));
        }
        let center = args.opt_point("center")?;
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let one = || -> Result<String> {
            match ids.as_slice() {
                [id] => Ok(id.clone()),
                _ => Err(bad_args("this change takes exactly one markup id")),
            }
        };
        let vertex = |key: &str| -> Result<(usize, markupcraft_engine::Point)> {
            let v = args.get(key).ok_or_else(|| bad_args(format!("missing {key}")))?;
            let index = v["index"]
                .as_u64()
                .filter(|i| *i >= 1)
                .ok_or_else(|| bad_args(format!("{key}.index must be a vertex number from 1")))?;
            let p =
                crate::args::point_of(&v["point"]).ok_or_else(|| bad_args(format!("{key}.point must be [x, y]")))?;
            Ok((index as usize - 1, p))
        };
        match given.first().copied().unwrap_or_default() {
            "move" => {
                let d = args.point("move")?;
                s.move_markups(&ids, d.x, d.y)?;
            }
            "rotate" => {
                s.rotate_markups(&ids, args.num("rotate")?, center)?;
            }
            "resize" => {
                let r = args
                    .opt_rect("resize")?
                    .ok_or_else(|| bad_args("resize must be [x0, y0, x1, y1]"))?;
                s.resize_markup(&one()?, r)?;
            }
            "points" => s.set_points(&one()?, args.points("points")?)?,
            "move_vertex" => {
                let (i, p) = vertex("move_vertex")?;
                s.move_vertex(&one()?, i, p)?;
            }
            "insert_vertex" => {
                let (i, p) = vertex("insert_vertex")?;
                s.insert_vertex(&one()?, i, p)?;
            }
            _ => {
                let i = args
                    .opt_u64("delete_vertex")?
                    .filter(|i| *i >= 1)
                    .ok_or_else(|| bad_args("delete_vertex must be a vertex number from 1"))?;
                s.delete_vertex(&one()?, i as usize - 1)?;
            }
        }
        let list: Vec<Value> = ids.iter().filter_map(|id| s.markup(id).ok()).map(markup_json).collect();
        Ok(json!({ "markups": list, "document": summary(doc, s) }))
    },
};

pub static DELETE: Tool = Tool {
    name: "markup_delete",
    title: "Delete markups",
    description: "Delete markups (ids, or the selection). Locked markups are refused unless skip_locked is true, which leaves them. Undoable until saved.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "ids": ids_arg(), "skip_locked": { "type": "boolean", "description": "Leave locked markups instead of failing." } }),
            &[],
        )
    },
    run: |a, args| {
        let skip = args.bool_or("skip_locked", false)?;
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let n = s.delete_markups(&ids, skip)?;
        Ok(json!({ "deleted": n, "document": summary(doc, s) }))
    },
};

pub static DUPLICATE: Tool = Tool {
    name: "markup_duplicate",
    title: "Duplicate markups",
    description: "Copy markups (ids, or the selection) on their own pages, moved by offset (default [10, -10]). The copies become the selection. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "ids": ids_arg(), "offset": point_arg("Offset [dx, dy]") }), &[]),
    run: |a, args| {
        let off = args
            .opt_point("offset")?
            .unwrap_or(markupcraft_engine::Point::new(10.0, -10.0));
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let new = s.duplicate_markups(&ids, off.x, off.y)?;
        Ok(json!({ "ids": new, "document": summary(doc, s) }))
    },
};

pub static ARRANGE: Tool = Tool {
    name: "markup_arrange",
    title: "Change z-order",
    description: "Bring markups (ids, or the selection) to the front, send them to the back, or one step forward/backward within their pages. Saved as the page's annotation order. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "ids": ids_arg(), "order": { "type": "string", "enum": ["front", "back", "forward", "backward"] } }),
            &["order"],
        )
    },
    run: |a, args| {
        let order = args.str("order")?;
        let how =
            Arrange::from_name(order).ok_or_else(|| bad_args("order must be front, back, forward or backward"))?;
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let moved = s.arrange(&ids, how)?;
        Ok(json!({ "moved": moved, "document": summary(doc, s) }))
    },
};

pub static COPY: Tool = Tool {
    name: "markup_copy",
    title: "Copy markups",
    description: "Copy markups (ids, or the selection) to the clipboard, which every open document shares. cut: true also deletes them (locked ones stay).",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "ids": ids_arg(), "cut": { "type": "boolean" } }), &[]),
    run: |a, args| {
        let cut = args.bool_or("cut", false)?;
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let n = if cut {
            s.cut_markups(&ids)?
        } else {
            s.copy_markups(&ids)?
        };
        let items = s.clipboard().to_vec();
        let out = json!({ "copied": items.len(), "cut": if cut { n } else { 0 }, "document": summary(doc, s) });
        a.set_clipboard(items);
        Ok(out)
    },
};

pub static PASTE: Tool = Tool {
    name: "markup_paste",
    title: "Paste markups",
    description: "Paste the clipboard onto `page` (default: each copy on the page it came from). With `at`, the clipboard's centre lands on that point; without, copies keep their positions (paste in place). The copies become the selection. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "page": page_arg("to paste on"), "at": point_arg("Centre of the pasted markups") }),
            &[],
        )
    },
    run: |a, args| {
        let page = args.opt_page("page")?;
        let at = args.opt_point("at")?;
        let items = a.clipboard().to_vec();
        let (doc, s) = a.session(args)?;
        s.set_clipboard(items);
        let ids = s.paste(page, at)?;
        Ok(json!({ "ids": ids, "document": summary(doc, s) }))
    },
};

pub static GROUP: Tool = Tool {
    name: "markup_group",
    title: "Group markups",
    description: "Group markups on one page (ids, or the selection; at least two). Returns the group id. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "ids": ids_arg() }), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let g = s.group(&ids)?;
        Ok(json!({ "group": g, "document": summary(doc, s) }))
    },
};

pub static UNGROUP: Tool = Tool {
    name: "markup_ungroup",
    title: "Ungroup markups",
    description: "Dissolve the groups the markups (ids, or the selection) belong to. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "ids": ids_arg() }), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let n = s.ungroup(&ids)?;
        Ok(json!({ "ungrouped": n, "document": summary(doc, s) }))
    },
};

pub static SELECT: Tool = Tool {
    name: "markup_select",
    title: "Select markups",
    description: "Set the selection that markup tools act on when they get no ids: these ids, all markups (all: true, optionally on `page`), or none (ids: []).",
    read_only: true,
    destructive: false,
    schema: || {
        schema(
            json!({ "ids": { "type": "array", "items": { "type": "string" } }, "all": { "type": "boolean" }, "page": page_arg("for all") }),
            &[],
        )
    },
    run: |a, args| {
        let all = args.bool_or("all", false)?;
        let page = args.opt_page("page")?;
        let ids = args.opt_strings("ids")?;
        let (doc, s) = a.session(args)?;
        match (all, ids) {
            (true, None) => {
                if let Some(p) = page {
                    s.page(p)?;
                }
                s.select_all(page)
            }
            (false, Some(ids)) => s.select(&ids)?,
            _ => return Err(bad_args("markup_select takes either ids or all: true")),
        }
        Ok(json!({ "selection": s.selection(), "document": summary(doc, s) }))
    },
};
