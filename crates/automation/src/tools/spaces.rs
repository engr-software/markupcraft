//! Spaces: named regions of a page; markups inside one show its name in the Space column.

use markupcraft_engine::spaces::{SpaceMatch, SpacePatch};
use serde_json::{Value, json};

use super::{Tool, page_arg, path_arg, points_arg, schema};
use crate::{bad_args, summary};

fn space_json(page: usize, s: &markupcraft_engine::spaces::Space) -> Value {
    json!({
        "id": s.id, "page": page + 1, "name": s.name,
        "points": s.pts.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>(),
        "color": s.color.hex(), "opacity": s.opacity, "area": s.area(),
    })
}

fn color_arg() -> Value {
    json!({ "type": ["string", "array"], "description": "Highlight colour: \"#RRGGBB\", a name, or [r, g, b] from 0 to 1." })
}

pub static LIST: Tool = Tool {
    name: "space_list",
    title: "List spaces",
    description: "Spaces (named page regions) on `page` or every page: id, name, outline, colour, area in PDF points², and the markups whose innermost space it is.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({ "page": page_arg("to list (default: every page)") }), &[]),
    run: |a, args| {
        let page = args.opt_page("page")?;
        let (doc, s) = a.session_ref(args)?;
        let mut out = Vec::new();
        for (p, sp) in s.spaces(page) {
            let mut v = space_json(p, &sp);
            v["markups"] = json!(s.markups_in_space(&sp.id)?);
            out.push(v);
        }
        Ok(json!({ "doc": doc, "spaces": out }))
    },
};

pub static ADD: Tool = Tool {
    name: "space_add",
    title: "Add a space",
    description: "Add a space: a named polygon region on `page`. Markups inside it get its name in the Markups List Space column. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("the space is on"),
                "name": { "type": "string" },
                "points": points_arg("The outline (3 or more points)"),
                "color": color_arg(),
                "opacity": { "type": "number", "minimum": 0, "maximum": 1 }
            }),
            &["page", "name", "points"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let name = args.str("name")?.to_string();
        let pts = args.points("points")?;
        let color = args.opt_color("color")?;
        let opacity = args.opt_num("opacity")?;
        let (doc, s) = a.session(args)?;
        let id = s.add_space(page, &name, pts, color, opacity)?;
        Ok(json!({ "id": id, "document": summary(doc, s) }))
    },
};

pub static EDIT: Tool = Tool {
    name: "space_edit",
    title: "Edit a space",
    description: "Rename a space, change its outline, colour or opacity. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "id": { "type": "string" },
                "name": { "type": "string" },
                "points": points_arg("The new outline"),
                "color": color_arg(),
                "opacity": { "type": "number", "minimum": 0, "maximum": 1 }
            }),
            &["id"],
        )
    },
    run: |a, args| {
        let id = args.str("id")?.to_string();
        let patch = SpacePatch {
            name: args.opt_string("name")?,
            pts: args.opt_points("points")?,
            color: args.opt_color("color")?,
            opacity: args.opt_num("opacity")?,
        };
        if patch == SpacePatch::default() {
            return Err(bad_args("space_edit: give name, points, color or opacity"));
        }
        let (doc, s) = a.session(args)?;
        s.edit_space(&id, &patch)?;
        Ok(json!({ "document": summary(doc, s) }))
    },
};

pub static DELETE: Tool = Tool {
    name: "space_delete",
    title: "Delete spaces",
    description: "Delete spaces by id (space_list shows them). Markups stay. Undoable.",
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
        let n = s.delete_spaces(&ids)?;
        Ok(json!({ "deleted": n, "document": summary(doc, s) }))
    },
};

pub static TALLY: Tool = Tool {
    name: "space_tally",
    title: "Markups per space",
    description: "Markups per space and counted items per subject per space (each point of a Count measurement counts in the space it is in), on `page` or every page. Space \"\" = outside every space.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({ "page": page_arg("to tally (default: every page)") }), &[]),
    run: |a, args| {
        let page = args.opt_page("page")?;
        let (doc, s) = a.session_ref(args)?;
        let rows: Vec<Value> = s
            .space_tallies(page)
            .iter()
            .map(|t| json!({ "page": t.page + 1, "space": t.space, "markups": t.markups, "counts": t.counts }))
            .collect();
        Ok(json!({ "doc": doc, "spaces": rows }))
    },
};

pub static SPLIT_COUNTS: Tool = Tool {
    name: "space_split_counts",
    title: "Split counts by space",
    description: "Every Count measurement (of `ids`, or all) whose items lie in more than one space becomes one Count per space, keeping its subject and look (Preferences > Measure > Split counts by space does this as counts are placed). One undoable step. Returns the new Counts' ids.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "ids": { "type": "array", "items": { "type": "string" } } }),
            &[],
        )
    },
    run: |a, args| {
        let ids: Option<Vec<String>> = args
            .get("ids")
            .and_then(Value::as_array)
            .map(|v| v.iter().filter_map(Value::as_str).map(str::to_string).collect());
        let (doc, s) = a.session(args)?;
        let made = s.split_counts_by_space(ids.as_deref())?;
        Ok(json!({ "made": made, "document": summary(doc, s) }))
    },
};

pub static EXPORT: Tool = Tool {
    name: "space_export",
    title: "Export spaces",
    description: "Write every space to a JSON spaces file (page number, page label, name, outline, colour).",
    read_only: false,
    destructive: true,
    schema: || schema(json!({ "path": path_arg("The spaces file to write") }), &["path"]),
    run: |a, args| {
        let path = a.resolve(args.str("path")?, true)?;
        let (doc, s) = a.session_ref(args)?;
        let n = s.export_spaces(&path)?;
        Ok(json!({ "doc": doc, "spaces": n, "path": path.display().to_string() }))
    },
};

pub static IMPORT: Tool = Tool {
    name: "space_import",
    title: "Import spaces",
    description: "Add the spaces of a JSON spaces file (space_export), matching pages by label (default; falls back to the page number) or by number. A space whose name the page already has is replaced. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "path": path_arg("The spaces file"),
                "match": { "type": "string", "enum": ["label", "number"] }
            }),
            &["path"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let by = match args.opt_str("match")?.unwrap_or("label") {
            "label" => SpaceMatch::Label,
            "number" => SpaceMatch::Number,
            other => {
                return Err(bad_args(format!(
                    "space_import: match is label or number, not {other:?}"
                )));
            }
        };
        let (doc, s) = a.session(args)?;
        let n = s.import_spaces(&path, by)?;
        Ok(json!({ "imported": n, "document": summary(doc, s) }))
    },
};
