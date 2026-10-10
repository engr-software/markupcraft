//! Dynamic Fill (trace a closed region of page linework) and hatch patterns.

use markupcraft_engine::Markup;
use markupcraft_engine::fill::{FillOptions, FillOutput};
use markupcraft_engine::hatch::{Hatch, HatchStyle};
use serde_json::{Value, json};

use super::{Tool, ids_arg, markup_json, markup_props, page_arg, point_arg, points_arg, schema, target_ids};
use crate::{bad_args, summary};

fn pts_json(p: &[markupcraft_engine::Point]) -> Value {
    json!(p.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>())
}

pub static DYNAMIC_FILL: Tool = Tool {
    name: "dynamic_fill",
    title: "Dynamic Fill",
    description: "Trace the closed region of the page's vector linework around `point` (like clicking inside a room) and make it an Area (default, islands inside become cutouts), a Polygon, a Perimeter or a Space (`output`). Gaps up to `gap` points between lines are closed; `boundaries` adds extra lines (Add Boundary). `preview` only reports the outline. Markup properties (subject, color, fill, ...) apply to the new markup. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            markup_props(json!({
                "page": page_arg("to fill on"),
                "point": point_arg("A point inside the region"),
                "output": { "type": "string", "enum": ["area", "polygon", "perimeter", "polylength", "volume", "space"] },
                "depth": { "type": "number", "minimum": 0, "description": "Output volume: the depth (scale units)." },
                "path": points_arg("Fill by dragging: one fill covering every region this path passes through (instead of point)"),
                "space_name": { "type": "string", "description": "The new space's name (output space)." },
                "gap": { "type": "number", "minimum": 0, "maximum": 72, "description": "Close gaps up to this many points (default 0.5)." },
                "cutouts": { "type": "boolean", "description": "Islands inside become cutouts (default true)." },
                "boundaries": { "type": "array", "items": points_arg("A boundary polyline") },
                "preview": { "type": "boolean" },
                "detect": { "type": "string", "enum": ["vector", "raster"], "description": "raster: detect on the rendered page image (scans), with dpi, sensitivity and hide_markups." },
                "dpi": { "type": "number", "minimum": 36, "maximum": 300, "description": "Raster detection resolution (default 100)." },
                "sensitivity": { "type": "integer", "minimum": 1, "maximum": 254, "description": "Raster edge sensitivity: pixels darker than this grey level are walls (default 160)." },
                "hide_markups": { "type": "boolean", "description": "Raster: markups are not walls (default true)." }
            })),
            &["page"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let path = args.opt_points("path")?;
        let seed = match &path {
            Some(p) => p.first().copied().ok_or_else(|| bad_args("path needs points"))?,
            None => args.point("point")?,
        };
        let mut opts = FillOptions::default();
        if let Some(g) = args.opt_num("gap")? {
            opts.gap = g;
        }
        opts.cutouts = args.bool_or("cutouts", true)?;
        if args.opt_str("detect")? == Some("raster") {
            let d = markupcraft_engine::finish::rasterfill::RasterFill::default();
            opts.raster = Some(markupcraft_engine::finish::rasterfill::RasterFill {
                dpi: args.opt_num("dpi")?.unwrap_or(d.dpi),
                sensitivity: u8::try_from(args.opt_int("sensitivity")?.unwrap_or(i64::from(d.sensitivity)))
                    .map_err(|_| bad_args("sensitivity is 1 to 254"))?,
                gap: opts.gap.max(d.gap),
                hide_markups: args.bool_or("hide_markups", true)?,
                cutouts: opts.cutouts,
            });
        }
        if let Some(b) = args.get("boundaries") {
            let lines = b
                .as_array()
                .ok_or_else(|| bad_args("boundaries must be a list of point lists"))?;
            for l in lines {
                opts.boundaries.push(
                    crate::args::points_of(l).ok_or_else(|| bad_args("boundaries must be a list of point lists"))?,
                );
            }
        }
        let output = match args.opt_str("output")?.unwrap_or("area") {
            "area" => FillOutput::Area,
            "polygon" => FillOutput::Polygon,
            "perimeter" => FillOutput::Perimeter,
            "polylength" => FillOutput::Polylength,
            "volume" => FillOutput::Volume(
                args.opt_num("depth")?
                    .ok_or_else(|| bad_args("dynamic_fill: output volume needs depth"))?,
            ),
            "space" => FillOutput::Space(
                args.opt_string("space_name")?
                    .ok_or_else(|| bad_args("dynamic_fill: output space needs space_name"))?,
            ),
            other => return Err(bad_args(format!("dynamic_fill: unknown output {other:?}"))),
        };
        let patch = args.patch()?;
        let preview = args.bool_or("preview", false)?;
        let (doc, s) = a.session(args)?;
        if preview {
            let r = s.dynamic_fill(page, seed, &opts)?;
            return Ok(json!({
                "doc": doc, "outline": pts_json(&r.outer),
                "cutouts": r.holes.iter().map(|h| pts_json(h)).collect::<Vec<_>>(),
                "area_pt2": r.area,
            }));
        }
        let kind = match output {
            FillOutput::Polygon => markupcraft_engine::Kind::Polygon,
            FillOutput::Perimeter => markupcraft_engine::Kind::Perimeter,
            FillOutput::Polylength => markupcraft_engine::Kind::Polylength,
            FillOutput::Volume(_) => markupcraft_engine::Kind::Volume,
            _ => markupcraft_engine::Kind::Area,
        };
        let mut look = Markup {
            kind,
            line_width: 2.0,
            ..Default::default()
        };
        patch.apply(&mut look).map_err(crate::failed)?;
        if let Some(path) = path {
            // every region the drag passed through, joined into one fill
            let regions = s.dynamic_fill_path(page, &path, &opts)?.len();
            let ids = s.dynamic_fill_path_create(page, &path, &opts, &output, Some(look))?;
            let markups: Vec<Value> = ids.iter().filter_map(|id| s.markup(id).ok()).map(markup_json).collect();
            return Ok(json!({
                "created": ids, "regions": regions, "markups": markups, "document": summary(doc, s),
            }));
        }
        let (id, r) = s.dynamic_fill_create(page, seed, &opts, &output, Some(look))?;
        let created = match output {
            FillOutput::Space(_) => json!({ "space": id }),
            _ => json!({ "id": id, "markup": markup_json(s.markup(&id)?) }),
        };
        Ok(json!({
            "created": created, "outline": pts_json(&r.outer), "cutouts": r.holes.len(),
            "area_pt2": r.area, "document": summary(doc, s),
        }))
    },
};

pub static HATCH: Tool = Tool {
    name: "markup_hatch",
    title: "Hatch pattern",
    description: "Hatch closed markups (Area, Polygon, Rectangle, Ellipse, Cloud, Volume): `style` Diagonal, BackDiagonal, Horizontal, Vertical, Cross, DiagonalCross or None (removes it); `spacing` and `width` in points; `color` (default: the line colour). Saved as /PCHatch and drawn into the appearance. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "ids": ids_arg(),
                "style": { "type": "string" },
                "spacing": { "type": "number", "minimum": 0.5 },
                "width": { "type": "number", "minimum": 0 },
                "color": { "type": ["string", "array"] }
            }),
            &["style"],
        )
    },
    run: |a, args| {
        let style = args.str("style")?;
        let hatch = if style.eq_ignore_ascii_case("none") {
            None
        } else {
            let st = HatchStyle::from_name(style).ok_or_else(|| {
                bad_args(format!(
                    "markup_hatch: style is one of {}, or None",
                    HatchStyle::ALL.map(HatchStyle::name).join(", ")
                ))
            })?;
            let mut h = Hatch {
                style: st,
                ..Default::default()
            };
            if let Some(sp) = args.opt_num("spacing")? {
                h.spacing = sp;
            }
            if let Some(w) = args.opt_num("width")? {
                h.width = w;
            }
            h.color = args.opt_color("color")?;
            Some(h)
        };
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let n = s.set_hatch(&ids, hatch)?;
        Ok(json!({ "changed": n, "document": summary(doc, s) }))
    },
};
