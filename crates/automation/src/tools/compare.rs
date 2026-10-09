//! Compare Documents and Overlay Pages.

use markupcraft_engine::compare::{CompareMode, CompareOptions};
use markupcraft_engine::overlay::{OverlayAlign, OverlayLayer, default_color, overlay_pages};
use markupcraft_engine::{Color, Point, props};
use serde_json::{Value, json};

use super::{Tool, path_arg, rect_arg, schema, schema_nodoc};
use crate::{Result, bad_args, failed, summary};

/// A colour argument inside a nested object: "#RRGGBB", a name, or [r, g, b] from 0 to 1.
pub(crate) fn color_value(key: &str, v: &Value) -> Result<Color> {
    if let Some(s) = v.as_str() {
        return props::parse_color(s).ok_or_else(|| bad_args(format!("{key}: unknown colour {s:?}")));
    }
    let a: Vec<f64> = v
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    match a.as_slice() {
        [r, g, b] if a.iter().all(|c| (0.0..=1.0).contains(c)) => Ok(Color::rgb(*r, *g, *b)),
        _ => Err(bad_args(format!(
            "{key} must be \"#RRGGBB\", a name, or [r, g, b] from 0 to 1"
        ))),
    }
}

fn point_value(v: &Value) -> Option<Point> {
    crate::args::point_of(v)
}

pub static COMPARE: Tool = Tool {
    name: "compare_documents",
    title: "Compare Documents",
    description: "Compare this (newer) document with an older revision and cloud every change on this one (subject \"Compare\"): text changes (words inserted, deleted, replaced) and graphic changes (a raster difference of the rendered pages, clustered into regions). The old file is only read. Returns each region with its page, rectangle, kind (added, removed, changed), source and cloud id. Undoable as one step.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "old": path_arg("The older revision (PDF)"),
                "old_doc": { "type": "integer", "minimum": 1, "description": "Or an open document as the older revision." },
                "mode": { "type": "string", "enum": ["text", "graphics", "both"], "description": "What to compare (default both)." },
                "pairs": { "type": "array", "items": { "type": "array", "items": { "type": "integer", "minimum": 1 }, "minItems": 2, "maxItems": 2 }, "description": "[[old page, new page], ...] 1-based (default: page 1 with 1, 2 with 2, ...)." },
                "sensitivity": { "type": "number", "minimum": 0, "maximum": 1, "description": "0 = only strong changes, 1 = the faintest difference (default 0.5)." },
                "dpi": { "type": "number", "description": "Rasterization resolution (default 100)." },
                "cell": { "type": "integer", "minimum": 1, "description": "Grid cell size in pixels (default 8)." },
                "density": { "type": "integer", "minimum": 1, "description": "Changed pixels a cell needs (default 4)." },
                "merge": { "type": "number", "minimum": 0, "description": "Changes closer than this many points join one cloud (default 18)." },
                "margin": { "type": "number", "minimum": 0, "description": "Ignore a band this wide (points) around each page." },
                "window": rect_arg("Only compare this rectangle of the new page"),
                "include_markups": { "type": "boolean", "description": "Include existing markups in the graphics comparison (default false)." },
                "color": { "type": ["string", "array"], "description": "Cloud colour (default orange)." },
                "subject": { "type": "string", "description": "Subject of the clouds (default Compare)." },
                "width": { "type": "number", "description": "Cloud line width in points (default 1.5)." },
                "cloud": { "type": "number", "minimum": 0, "maximum": 2, "description": "Cloud intensity; 0 draws rectangles (default 1)." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let mut o = CompareOptions::default();
        if let Some(m) = args.opt_str("mode")? {
            o.mode = CompareMode::from_name(m).ok_or_else(|| bad_args("mode must be text, graphics or both"))?;
        }
        if let Some(p) = args.get("pairs") {
            let arr = p
                .as_array()
                .ok_or_else(|| bad_args("pairs must be [[old, new], ...]"))?;
            for pair in arr {
                match pair
                    .as_array()
                    .map(|x| x.iter().filter_map(Value::as_u64).collect::<Vec<_>>())
                    .as_deref()
                {
                    Some([x, y]) if *x >= 1 && *y >= 1 => o.pairs.push((*x as usize - 1, *y as usize - 1)),
                    _ => return Err(bad_args("pairs must be [[old, new], ...] with pages from 1")),
                }
            }
        }
        if let Some(v) = args.opt_num("sensitivity")? {
            o.sensitivity = v;
        }
        if let Some(v) = args.opt_num("dpi")? {
            o.dpi = v;
        }
        if let Some(v) = args.opt_u64("cell")? {
            o.cell_px = v as usize;
        }
        if let Some(v) = args.opt_u64("density")? {
            o.min_pixels = v as usize;
        }
        if let Some(v) = args.opt_num("merge")? {
            o.merge_pt = v;
        }
        if let Some(v) = args.opt_num("margin")? {
            o.margin_pt = v;
        }
        o.window = args.opt_rect("window")?;
        o.include_markups = args.bool_or("include_markups", false)?;
        if let Some(c) = args.opt_color("color")? {
            o.color = c;
        }
        if let Some(s) = args.opt_string("subject")? {
            o.subject = s;
        }
        if let Some(w) = args.opt_num("width")? {
            o.line_width = w;
        }
        if let Some(c) = args.opt_num("cloud")? {
            o.cloud = c;
        }
        let old = match (args.opt_str("old")?, args.opt_u64("old_doc")?) {
            (Some(p), None) => markupcraft_engine::raster::read_pdf(&a.resolve(p, false)?)?,
            (None, Some(id)) => {
                let (_, s) = a
                    .docs()
                    .iter()
                    .find(|(d, _)| *d == id)
                    .ok_or_else(|| failed(format!("no open document {id}")))?;
                s.current_bytes()?
            }
            _ => {
                return Err(bad_args(
                    "give the older revision as old (a path) or old_doc (an open document)",
                ));
            }
        };
        let (doc, s) = a.session(args)?;
        let r = s.compare_with(old, &o)?;
        let regions: Vec<Value> = r
            .regions
            .iter()
            .map(|g| {
                json!({
                    "page": g.page + 1,
                    "old_page": g.old_page + 1,
                    "rect": g.rect.as_array(),
                    "kind": g.kind,
                    "source": g.source,
                    "text": g.text,
                    "id": g.markup,
                })
            })
            .collect();
        Ok(json!({
            "pairs": r.pairs.iter().map(|(x, y)| [x + 1, y + 1]).collect::<Vec<_>>(),
            "changes": regions.len(),
            "text_changes": r.text_changes,
            "graphics_changes": r.graphics_changes,
            "regions": regions,
            "document": summary(doc, s),
        }))
    },
};

pub static OVERLAY: Tool = Tool {
    name: "overlay_pages",
    title: "Overlay Pages",
    description: "Stack pages of two or more PDFs in a new PDF at `out`: each layer is recoloured (ink takes the layer colour, paper stays white) and kept as a toggleable PDF layer; where all layers have ink it shows dark. Alignment per layer: \"page\" (as positioned), \"bounds\" (stretched to the first layer's page) or two matching points. Writes the file (atomic); open it with doc_open.",
    read_only: false,
    destructive: true,
    schema: || {
        let pt = json!({ "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 });
        schema_nodoc(
            json!({
                "layers": {
                    "type": "array",
                    "minItems": 2,
                    "description": "The layers, first = base page boxes.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": path_arg("The PDF"),
                            "pages": { "type": "array", "items": { "type": "integer", "minimum": 1 }, "description": "Its page for output page 1, 2, ... (default all, in order)." },
                            "color": { "type": ["string", "array"] },
                            "opacity": { "type": "number", "minimum": 0, "maximum": 1 },
                            "name": { "type": "string" },
                            "align": { "type": "string", "enum": ["page", "bounds", "points"] },
                            "from": { "type": "array", "items": pt.clone(), "description": "align points: two points on this layer's page." },
                            "to": { "type": "array", "items": pt, "description": "align points: where they land on the first layer's page." }
                        },
                        "required": ["path"]
                    }
                },
                "out": path_arg("The overlay PDF to write")
            }),
            &["layers", "out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let list = args
            .get("layers")
            .and_then(Value::as_array)
            .ok_or_else(|| bad_args("layers must be a list"))?;
        let mut layers = Vec::new();
        for (i, l) in list.iter().enumerate() {
            let o = l.as_object().ok_or_else(|| bad_args("each layer must be an object"))?;
            const KEYS: [&str; 8] = ["path", "pages", "color", "opacity", "name", "align", "from", "to"];
            if let Some(k) = o.keys().find(|k| !KEYS.contains(&k.as_str())) {
                return Err(bad_args(format!("layer {}: unknown key {k:?}", i + 1)));
            }
            let path = o
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| bad_args(format!("layer {}: path is required", i + 1)))?;
            let path = a.resolve(path, false)?;
            let pages: Vec<usize> = match o.get("pages") {
                None | Some(Value::Null) => Vec::new(),
                Some(v) => v
                    .as_array()
                    .ok_or_else(|| bad_args("pages must be a list"))?
                    .iter()
                    .map(|p| match p.as_u64() {
                        Some(n) if n >= 1 => Ok(n as usize - 1),
                        _ => Err(bad_args("pages are numbered from 1")),
                    })
                    .collect::<Result<_>>()?,
            };
            let color = match o.get("color") {
                Some(c) if !c.is_null() => color_value("color", c)?,
                _ => default_color(i),
            };
            let opacity = o.get("opacity").and_then(Value::as_f64).unwrap_or(1.0);
            let name = o.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let align = match o.get("align").and_then(Value::as_str).unwrap_or("page") {
                "page" => OverlayAlign::Page,
                "bounds" => OverlayAlign::Bounds,
                "points" => {
                    let two = |k: &str| -> Result<[Point; 2]> {
                        let v: Vec<Point> = o
                            .get(k)
                            .and_then(Value::as_array)
                            .map(|a| a.iter().filter_map(point_value).collect())
                            .unwrap_or_default();
                        match v.as_slice() {
                            [p, q] => Ok([*p, *q]),
                            _ => Err(bad_args(format!("align points needs {k}: [[x, y], [x, y]]"))),
                        }
                    };
                    OverlayAlign::Points {
                        from: two("from")?,
                        to: two("to")?,
                    }
                }
                other => return Err(bad_args(format!("align {other:?}: use page, bounds or points"))),
            };
            layers.push(OverlayLayer {
                bytes: markupcraft_engine::raster::read_pdf(&path)?,
                pages,
                color,
                opacity,
                name,
                align,
            });
        }
        let r = overlay_pages(&layers, &out)?;
        Ok(json!({ "out": out.display().to_string(), "pages": r.pages, "layers": r.layers }))
    },
};
