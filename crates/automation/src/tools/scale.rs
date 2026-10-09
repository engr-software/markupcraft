//! Scales: page scale, calibration, viewports, measuring.

use markupcraft_measure::units::LengthUnit;
use serde_json::{Value, json};

use super::{Tool, page_arg, pages_arg, point_arg, points_arg, rect_arg, scale_arg, schema};
use crate::{bad_args, summary};

fn apply_arg() -> Value {
    json!({ "type": "boolean", "description": "Also give existing measurements on those pages the new scale (default false)." })
}

pub static SET: Tool = Tool {
    name: "scale_set",
    title: "Set the page scale",
    description: "Set the scale of pages (default all): a page-wide viewport, the way Revu stores a page scale. New measurements on those pages take it. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "pages": pages_arg("to set (default: all)"), "scale": scale_arg(), "apply_to_markups": apply_arg() }),
            &["scale"],
        )
    },
    run: |a, args| {
        let sc = args.opt_scale("scale")?.ok_or_else(|| bad_args("missing scale"))?;
        let apply = args.bool_or("apply_to_markups", false)?;
        let (doc, s) = a.session(args)?;
        let pages = match args.opt_pages("pages", s.page_count())? {
            Some(p) => p,
            None => (0..s.page_count()).collect(),
        };
        let updated = s.set_page_scale(&pages, &sc, apply)?;
        Ok(json!({ "scale": sc.ratio, "pages": pages.len(), "markups_updated": updated, "document": summary(doc, s) }))
    },
};

pub static CALIBRATE: Tool = Tool {
    name: "scale_calibrate",
    title: "Calibrate a scale",
    description: "Calibrate from two points on the sheet that are `length` `unit`s apart in the real world. Feet and inches give a feet-inches scale; metric units a decimal one. Applies to `pages` (default: the page the points are on). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("the points are on"),
                "from": point_arg("First point"),
                "to": point_arg("Second point"),
                "length": { "type": "number", "exclusiveMinimum": 0, "description": "Real distance between the points." },
                "unit": { "type": "string", "description": "Unit of length: in, ft, yd, mi, mm, cm, m, km (default ft)." },
                "pages": pages_arg("that get the scale (default: page)"),
                "apply_to_markups": apply_arg()
            }),
            &["page", "from", "to", "length"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let (from, to) = (args.point("from")?, args.point("to")?);
        let length = args.num("length")?;
        let unit = args.opt_unit("unit")?.unwrap_or(LengthUnit::Foot);
        let apply = args.bool_or("apply_to_markups", false)?;
        let (doc, s) = a.session(args)?;
        s.page(page)?;
        let pages = args.opt_pages("pages", s.page_count())?.unwrap_or_else(|| vec![page]);
        let sc = s.calibrate(&pages, from, to, length, unit, apply)?;
        Ok(
            json!({ "scale": sc.ratio, "pages": pages.iter().map(|p| p + 1).collect::<Vec<_>>(), "document": summary(doc, s) }),
        )
    },
};

pub static VIEWPORT_ADD: Tool = Tool {
    name: "viewport_add",
    title: "Add a viewport",
    description: "Give part of a page its own scale (a detail at another scale). Measurements inside the box use it. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "page": page_arg("to add it to"), "box": rect_arg("The viewport"), "name": { "type": "string" }, "scale": scale_arg() }),
            &["page", "box", "scale"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let bbox = args
            .opt_rect("box")?
            .ok_or_else(|| bad_args("box must be [x0, y0, x1, y1]"))?;
        let name = args.opt_str("name")?.unwrap_or("").to_string();
        let sc = args.opt_scale("scale")?.ok_or_else(|| bad_args("missing scale"))?;
        let (doc, s) = a.session(args)?;
        let id = s.add_viewport(page, bbox, &name, &sc)?;
        Ok(json!({ "id": id, "document": summary(doc, s) }))
    },
};

pub static VIEWPORT_DELETE: Tool = Tool {
    name: "viewport_delete",
    title: "Delete a viewport",
    description: "Remove viewport `index` (1-based, as doc_info lists them) from a page; removing the page-wide one removes the page scale. Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "page": page_arg("it is on"), "index": { "type": "integer", "minimum": 1 } }),
            &["page", "index"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let index = args.position("index")?;
        let (doc, s) = a.session(args)?;
        s.delete_viewport(page, index)?;
        Ok(json!({ "document": summary(doc, s) }))
    },
};

pub static MEASURE: Tool = Tool {
    name: "measure",
    title: "Measure points",
    description: "The quantity of a set of points as a Length, Polylength, Area, Perimeter or Count, with the page's scale at the first point (or `scale`). Changes nothing.",
    read_only: true,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("the points are on"),
                "kind": { "type": "string", "description": "Length, Polylength, Area, Perimeter or Count (default Polylength, or Length for 2 points)." },
                "points": points_arg("The points"),
                "scale": scale_arg()
            }),
            &["page", "points"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let pts = args.points("points")?;
        let kind = if args.has("kind") {
            args.kind("kind")?
        } else if pts.len() == 2 {
            markupcraft_engine::Kind::Length
        } else {
            markupcraft_engine::Kind::Polylength
        };
        let sc = args.opt_scale("scale")?;
        let (_, s) = a.session_ref(args)?;
        let m = s.measure(page, kind, &pts, sc.as_ref())?;
        Ok(json!({ "kind": m.kind.name(), "value": m.value, "text": m.text, "unit": m.unit, "scale": m.scale }))
    },
};
