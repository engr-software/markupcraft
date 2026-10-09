//! Tools for the drawing and takeoff operations of `markupcraft_engine::markup_ops`: the
//! Eraser, Count series, lasso selection, arcs, cutouts as measurements, Recalculate, and the
//! measurement properties the Properties panel sets (slope, caption contents and leader,
//! centroid, a Dimension's offset).

use markupcraft_engine::MarkupPatch;
use serde_json::{Value, json};

use super::{Tool, ids_arg, markup_json, page_arg, pages_arg, path_arg, point_arg, points_arg, schema, target_ids};
use crate::{Result, bad_args, summary};

pub static ATTACH_FILE: Tool = Tool {
    name: "markup_attach_file",
    title: "Add a File Attachment markup",
    description: "A File Attachment markup on `page` with its top-left corner at `point`, embedding the file at `path` (at most 50 MB), shown as `icon` (PushPin, Paperclip, Graph, Tag). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("The page (1-based)"),
                "point": point_arg("Top-left corner of the icon"),
                "path": path_arg("The file to attach"),
                "icon": { "type": "string", "enum": ["PushPin", "Paperclip", "Graph", "Tag"] }
            }),
            &["page", "point", "path"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let at = args.point("point")?;
        let path = a.resolve(args.str("path")?, false)?;
        let icon = args.opt_str("icon")?.unwrap_or("PushPin").to_string();
        let (doc, s) = a.session(args)?;
        let id = s.attach_file_markup(page, at, &path, &icon)?;
        let m = s.markup(&id)?;
        Ok(json!({ "id": id, "markup": markup_json(m), "document": summary(doc, s) }))
    },
};

pub static SAVE_ATTACHED: Tool = Tool {
    name: "markup_attachment_save",
    title: "Save a File Attachment's file",
    description: "Write the file a File Attachment markup carries to `out` (atomic).",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "id": { "type": "string", "description": "The File Attachment markup." },
                "out": path_arg("Where to write the file")
            }),
            &["id", "out"],
        )
    },
    run: |a, args| {
        let id = args.str("id")?.to_string();
        let out = a.resolve(args.str("out")?, true)?;
        let (_, s) = a.session_ref(args)?;
        let (name, bytes) = s.attachment_file(&id)?;
        let tmp = out.with_extension("mctmp");
        std::fs::write(&tmp, &bytes).map_err(|e| bad_args(format!("{}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, &out).map_err(|e| bad_args(format!("{}: {e}", out.display())))?;
        Ok(json!({ "name": name, "bytes": bytes.len(), "out": out.display().to_string() }))
    },
};

pub static REPLY: Tool = Tool {
    name: "markup_reply",
    title: "Reply to a markup",
    description: "One change to a markup's replies (id, or the selected one): `add` a reply by the session author, `edit` reply {index (1-based), text}, or `delete` reply index (1-based). Returns the replies. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "id": { "type": "string", "description": "The markup (default: the selected one)." },
                "add": { "type": "string", "description": "The reply's text." },
                "edit": { "type": "object", "properties": { "index": { "type": "integer", "minimum": 1 }, "text": { "type": "string" } }, "required": ["index", "text"] },
                "delete": { "type": "integer", "minimum": 1, "description": "The reply to delete." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let id = one_id(args, s)?;
        if let Some(t) = args.opt_str("add")? {
            s.add_reply(&id, t)?;
        } else if let Some(e) = args.get("edit") {
            let index = e["index"]
                .as_u64()
                .filter(|i| *i >= 1)
                .ok_or_else(|| bad_args("edit.index must be a reply number from 1"))?;
            let text = e["text"].as_str().ok_or_else(|| bad_args("edit.text must be text"))?;
            s.edit_reply(&id, index as usize - 1, text)?;
        } else if args.has("delete") {
            s.delete_reply(&id, args.position("delete")?)?;
        } else {
            return Err(bad_args("markup_reply takes add, edit or delete"));
        }
        let m = s.markup(&id)?;
        let replies: Vec<Value> = m
            .replies
            .iter()
            .map(|r| json!({ "author": r.author, "date": r.date, "text": r.text }))
            .collect();
        Ok(json!({ "replies": replies, "document": summary(doc, s) }))
    },
};

pub static SUMMARY_APPEND: Tool = Tool {
    name: "summary_append",
    title: "Append a linked markup summary",
    description: "Add a markup summary (subject, page, measurement, comments) at the end of the document, each row a link to its markup's page; `measurements_only` limits it to measurements. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "title": { "type": "string", "description": "Heading (default Markup Summary)." },
                "measurements_only": { "type": "boolean" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let title = args.opt_string("title")?.unwrap_or_default();
        let only = args.bool_or("measurements_only", false)?;
        let (doc, s) = a.session(args)?;
        let n = s.append_summary_with_links(&title, only)?;
        Ok(json!({ "rows": n, "pages": s.page_count(), "document": summary(doc, s) }))
    },
};

pub static SCALE_TEMPORARY: Tool = Tool {
    name: "scale_set_temporary",
    title: "Set a temporary page scale",
    description: "Measure `pages` (default all) with `scale` for this session without saving it: the file keeps its own scale, which comes back when the document is opened again. `apply_to_markups` recalculates the measurements there. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("Pages (default all)"),
                "scale": super::scale_arg(),
                "apply_to_markups": { "type": "boolean" }
            }),
            &["scale"],
        )
    },
    run: |a, args| {
        let sc = args
            .opt_scale("scale")?
            .ok_or_else(|| bad_args("missing argument scale"))?;
        let apply = args.bool_or("apply_to_markups", false)?;
        let (doc, s) = a.session(args)?;
        let count = s.page_count();
        let pages = args.opt_pages("pages", count)?.unwrap_or_else(|| (0..count).collect());
        let n = s.set_page_scale_temporary(&pages, &sc, apply)?;
        Ok(json!({ "scale": sc.ratio, "markups_updated": n, "document": summary(doc, s) }))
    },
};

pub static UNFLATTEN: Tool = Tool {
    name: "unflatten",
    title: "Unflatten markups",
    description: "Bring back the markups MarkupCraft flattened on `pages` (default all) as annotations, taking their drawings out of the page content. Content another program flattened cannot be unflattened. Undoable; the next save rewrites the file in full.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "pages": pages_arg("Pages (default all)") }), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let count = s.page_count();
        let pages = args.opt_pages("pages", count)?.unwrap_or_default();
        let n = s.unflatten_flattened(&pages)?;
        Ok(json!({ "unflattened": n, "document": summary(doc, s) }))
    },
};

pub static IMPORT_PDF: Tool = Tool {
    name: "markup_import_pdf",
    title: "Import markups from a PDF",
    description: "Bring the markups of another PDF onto the same page numbers here (pages past the end are skipped) as new annotations with their look, subject, status and column values. Kinds MarkupCraft cannot write and snapshots are skipped. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "path": path_arg("The PDF to take markups from") }), &["path"]),
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let (doc, s) = a.session(args)?;
        let (n, skipped) = s.import_markups_from_pdf(&path)?;
        Ok(json!({ "imported": n, "skipped": skipped, "document": summary(doc, s) }))
    },
};

fn one_id(a: &crate::Args, s: &markupcraft_engine::Session) -> Result<String> {
    match a.opt_string("id")? {
        Some(id) => Ok(id),
        None => match s.selection() {
            [id] => Ok(id.clone()),
            _ => Err(bad_args(format!("{}: pass id, or select one markup", a.tool()))),
        },
    }
}

fn positions(a: &crate::Args, key: &str) -> Result<Vec<usize>> {
    let v = a.get(key).ok_or_else(|| bad_args(format!("missing argument {key}")))?;
    let list = v
        .as_array()
        .ok_or_else(|| bad_args(format!("{key} must be a list of item numbers from 1")))?;
    if list.len() > 100_000 {
        return Err(bad_args(format!("{key}: too many items")));
    }
    list.iter()
        .map(|x| match x.as_u64() {
            Some(i) if i >= 1 => Ok(i as usize - 1),
            _ => Err(bad_args(format!("{key} must be item numbers from 1"))),
        })
        .collect()
}

pub static INK_ERASE: Tool = Tool {
    name: "ink_erase",
    title: "Erase ink",
    description: "The Eraser: drag `path` across Pen and Highlight strokes on `page`; every point within `radius` points (default 6) is rubbed out, strokes split where they were cut and emptied markups deleted. Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "page": page_arg("The page (1-based)"),
                "path": points_arg("The eraser's path"),
                "radius": { "type": "number", "description": "Reach in points (default 6)." }
            }),
            &["page", "path"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let path = args.points("path")?;
        let radius = args.opt_num("radius")?.unwrap_or(6.0);
        let (doc, s) = a.session(args)?;
        let n = s.erase_ink(page, &path, radius)?;
        Ok(json!({ "changed": n, "document": summary(doc, s) }))
    },
};

pub static COUNT_EDIT: Tool = Tool {
    name: "count_edit",
    title: "Edit a Count series",
    description: "One change to a Count measurement (id, or the selected one): `add` points (Resume Count), `delete_item` (1-based; the series renumbers, the last item deletes the count), `split` item numbers off into a new count, or `merge` with other count ids (all on the same page). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "id": { "type": "string", "description": "The Count (default: the selected one)." },
                "add": points_arg("Items to add"),
                "delete_item": { "type": "integer", "minimum": 1, "description": "Item to delete." },
                "split": { "type": "array", "items": { "type": "integer", "minimum": 1 }, "description": "Items to split off." },
                "merge": { "type": "array", "items": { "type": "string" }, "description": "Counts merged into this one." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let ops = ["add", "delete_item", "split", "merge"];
        let given: Vec<&str> = ops.iter().copied().filter(|k| args.has(k)).collect();
        if given.len() != 1 {
            return Err(bad_args(format!("count_edit takes exactly one of {}", ops.join(", "))));
        }
        let (doc, s) = a.session(args)?;
        let id = one_id(args, s)?;
        let out: Value = match given.first().copied().unwrap_or_default() {
            "add" => json!({ "items": s.add_count_items(&id, &args.points("add")?)? }),
            "delete_item" => json!({ "items": s.delete_count_item(&id, args.position("delete_item")?)? }),
            "split" => json!({ "new_id": s.split_count(&id, &positions(args, "split")?)? }),
            _ => {
                let mut ids = vec![id.clone()];
                ids.extend(args.opt_strings("merge")?.unwrap_or_default());
                json!({ "items": s.merge_counts(&ids)? })
            }
        };
        let list: Vec<Value> = s
            .doc()
            .markups
            .iter()
            .filter(|m| m.kind == markupcraft_engine::Kind::Count)
            .map(markup_json)
            .collect();
        Ok(json!({ "result": out, "counts": list, "document": summary(doc, s) }))
    },
};

pub static SELECT_LASSO: Tool = Tool {
    name: "select_lasso",
    title: "Lasso select",
    description: "Select the markups on `page` whose whole extent lies inside the closed `points` loop (added to the selection with `add`). Returns the selection.",
    read_only: true,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("The page (1-based)"),
                "points": points_arg("The lasso loop"),
                "add": { "type": "boolean", "description": "Add to the selection." }
            }),
            &["page", "points"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let ring = args.points("points")?;
        let add = args.bool_or("add", false)?;
        let (doc, s) = a.session(args)?;
        s.select_lasso(page, &ring, add)?;
        Ok(json!({ "selection": s.selection(), "document": summary(doc, s) }))
    },
};

pub static ARC_EDIT: Tool = Tool {
    name: "arc_edit",
    title: "Curve or straighten a segment",
    description: "On a Polylength, Perimeter, Area, Volume, Polygon or Polyline (id, or the selected one): `convert` the segment starting at vertex N (1-based) into an arc, `straighten` arc N back to a line, or `bend` arc N toward a point. The measured value follows. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "id": { "type": "string", "description": "The markup (default: the selected one)." },
                "convert": { "type": "integer", "minimum": 1, "description": "Vertex the segment starts at." },
                "straighten": { "type": "integer", "minimum": 1, "description": "Arc number." },
                "bend": { "type": "integer", "minimum": 1, "description": "Arc number." },
                "toward": point_arg("Where the bent arc passes")
            }),
            &[],
        )
    },
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let id = one_id(args, s)?;
        if args.has("convert") {
            s.convert_segment_to_arc(&id, args.position("convert")?)?;
        } else if args.has("straighten") {
            s.straighten_arc(&id, args.position("straighten")?)?;
        } else if args.has("bend") {
            s.bend_arc(&id, args.position("bend")?, args.point("toward")?)?;
        } else {
            return Err(bad_args("arc_edit takes convert, straighten or bend"));
        }
        let m = s.markup(&id)?;
        Ok(json!({ "arcs": m.arcs.len(), "markup": markup_json(m), "document": summary(doc, s) }))
    },
};

pub static CUTOUT_SPLIT: Tool = Tool {
    name: "cutout_to_measurement",
    title: "Cutout to its own measurement",
    description: "Make cutout `index` (1-based) of an Area or Volume a measurement of its own, with the same look and scale; the hole stays in the parent. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "id": { "type": "string", "description": "The Area or Volume." },
                "index": { "type": "integer", "minimum": 1, "description": "Which cutout." }
            }),
            &["id", "index"],
        )
    },
    run: |a, args| {
        let id = args.str("id")?.to_string();
        let index = args.position("index")?;
        let (doc, s) = a.session(args)?;
        let new_id = s.cutout_to_measurement(&id, index)?;
        let m = s.markup(&new_id)?;
        Ok(json!({ "markup": markup_json(m), "document": summary(doc, s) }))
    },
};

pub static RECALCULATE: Tool = Tool {
    name: "measure_recalculate",
    title: "Recalculate measurements",
    description: "Give every measurement on `pages` (default all) the scale now in effect where it sits (page scale or viewport), keeping its units and precision. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "pages": pages_arg("Pages (default all)") }), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let count = s.page_count();
        let pages = args.opt_pages("pages", count)?.unwrap_or_else(|| (0..count).collect());
        let n = s.recalculate_measurements(&pages)?;
        Ok(json!({ "changed": n, "document": summary(doc, s) }))
    },
};

pub static MEASURE_PROPS: Tool = Tool {
    name: "measure_props_set",
    title: "Set measurement caption, slope and dimension properties",
    description: "Set what the Properties panel sets on measurements and dimensions (ids, or the selection): `slope_type` (none, pitch, degrees, grade) with `slope` (pitch: rise in 12); `caption` contents with fields like {value}, {subject}, {all}, {area}, {c:<column id>} (\"\" = the value); `caption_leader`; `show_centroid`; a Dimension's `offset` and `extension` (points). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "ids": ids_arg(),
                "slope_type": { "type": "string", "enum": ["none", "pitch", "degrees", "grade"] },
                "slope": { "type": "number", "description": "Pitch rise in 12, degrees, or grade %." },
                "caption": { "type": "string", "description": "Caption contents template." },
                "caption_leader": { "type": "boolean" },
                "caption_last_segment": { "type": "boolean", "description": "Perimeter, Area, Volume or Polylength caption along the last segment instead of the centre." },
                "caption_bold": { "type": "boolean" },
                "caption_italic": { "type": "boolean" },
                "caption_underline": { "type": "boolean" },
                "caption_strike": { "type": "boolean", "description": "Strike-through caption text." },
                "caption_script": { "type": "string", "enum": ["normal", "superscript", "subscript"] },
                "show_centroid": { "type": "boolean" },
                "offset": { "type": "number", "description": "Dimension line offset, points." },
                "extension": { "type": "number", "description": "Extension past the dimension line, points." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let mut p = MarkupPatch {
            caption_template: args.opt_string("caption")?,
            caption_leader: args.opt_bool("caption_leader")?,
            caption_last_segment: args.opt_bool("caption_last_segment")?,
            bold: args.opt_bool("caption_bold")?,
            italic: args.opt_bool("caption_italic")?,
            underline: args.opt_bool("caption_underline")?,
            strike: args.opt_bool("caption_strike")?,
            show_centroid: args.opt_bool("show_centroid")?,
            leader: args.opt_num("offset")?,
            leader_ext: args.opt_num("extension")?,
            ..Default::default()
        };
        if let Some(t) = args.opt_str("slope_type")? {
            let code = match t {
                "none" => 0,
                "pitch" => 1,
                "degrees" => 2,
                "grade" => 3,
                o => return Err(bad_args(format!("slope_type: none, pitch, degrees or grade (got {o})"))),
            };
            p.slope = Some((code, args.opt_num("slope")?.unwrap_or(0.0)));
        }
        if let Some(t) = args.opt_str("caption_script")? {
            p.script = Some(match t {
                "normal" => 0,
                "superscript" => 1,
                "subscript" => -1,
                o => {
                    return Err(bad_args(format!(
                        "caption_script: normal, superscript or subscript (got {o})"
                    )));
                }
            });
        }
        p.validate().map_err(|e| bad_args(e.to_string()))?;
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let n = s.set_properties(&ids, &p)?;
        let list: Vec<Value> = ids
            .iter()
            .filter_map(|id| s.markup(id).ok())
            .map(|m| {
                let mut v = markup_json(m);
                v["caption"] = json!(markupcraft_model::caption::caption_text(m));
                v
            })
            .collect();
        Ok(json!({ "changed": n, "markups": list, "document": summary(doc, s) }))
    },
};
