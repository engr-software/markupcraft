//! Compare Documents and Overlay Pages.

use markupcraft_engine::compare::{
    CompareAlign, CompareMode, CompareOptions, CustomPreset, load_presets, save_presets,
};
use markupcraft_engine::overlay::{
    LayerAdjust, OverlayAlign, OverlayBlend, OverlayLayer, default_color, overlay_pages_shaded,
};
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
                "cloud": { "type": "number", "minimum": 0, "maximum": 2, "description": "Cloud intensity; 0 draws rectangles (default 1)." },
                "preset": { "type": "string", "description": "same_printer, different_printer, scanned, or a saved custom preset (compare_preset); applied before the other tuning arguments." },
                "align": { "type": "string", "enum": ["page", "auto", "offset", "points"], "description": "Register the old page on the new: as stacked (default), automatically, by a known offset, or by two matching points." },
                "offset": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2, "description": "align offset: [dx, dy] points the old content moves." },
                "old_points": { "type": "array", "items": { "type": "array", "items": { "type": "number" } }, "description": "align points: two points on the old page." },
                "new_points": { "type": "array", "items": { "type": "array", "items": { "type": "number" } }, "description": "align points: the same two points on the new page." },
                "fill": { "type": ["string", "array"], "description": "Cloud fill colour (default none)." },
                "fill_opacity": { "type": "number", "minimum": 0, "maximum": 1 },
                "opacity": { "type": "number", "minimum": 0, "maximum": 1 },
                "lock": { "type": "boolean", "description": "Lock the clouds when placed." },
                "include_flattened": { "type": "boolean", "description": "Include recoverable flattened markups (default false)." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let o = compare_options(a, args)?;
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

/// The custom presets file in the config folder.
fn presets_path(a: &crate::Automation) -> Result<std::path::PathBuf> {
    Ok(a.config_dir()?.join("compare_presets.json"))
}

pub static PRESET: Tool = Tool {
    name: "compare_preset",
    title: "Comparison presets",
    description: "Compare > Advanced > Type: list the built-in presets (same_printer, different_printer, scanned) and the saved custom ones; save the given tuning (sensitivity, dpi, cell, density, merge, auto_align) as a custom preset; delete one; or restore defaults (remove every custom preset).",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({
                "action": { "type": "string", "enum": ["list", "save", "delete", "restore_defaults"] },
                "name": { "type": "string" },
                "sensitivity": { "type": "number", "minimum": 0, "maximum": 1 },
                "dpi": { "type": "number" },
                "cell": { "type": "integer", "minimum": 1 },
                "density": { "type": "integer", "minimum": 1 },
                "merge": { "type": "number", "minimum": 0 },
                "auto_align": { "type": "boolean" }
            }),
            &["action"],
        )
    },
    run: |a, args| {
        let path = presets_path(a)?;
        let mut list = load_presets(&path)?;
        match args.str("action")? {
            "list" => {}
            "save" => {
                let name = args.str("name")?.trim().to_string();
                let mut o = CompareOptions::default();
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
                if args.bool_or("auto_align", false)? {
                    o.align = CompareAlign::Auto;
                }
                list.retain(|p| !p.name.eq_ignore_ascii_case(&name));
                list.push(CustomPreset::from_options(&name, &o));
                save_presets(&path, &list)?;
            }
            "delete" => {
                let name = args.str("name")?;
                let before = list.len();
                list.retain(|p| !p.name.eq_ignore_ascii_case(name));
                if list.len() == before {
                    return Err(failed(format!("no custom preset {name:?}")));
                }
                save_presets(&path, &list)?;
            }
            "restore_defaults" => {
                list.clear();
                save_presets(&path, &list)?;
            }
            other => return Err(bad_args(format!("action {other:?}"))),
        }
        Ok(json!({
            "built_in": markupcraft_engine::compare::PRESETS.iter().map(|p| p.name).collect::<Vec<_>>(),
            "custom": list.iter().map(|p| json!({ "name": p.name, "sensitivity": p.sensitivity, "dpi": p.dpi, "cell": p.cell_px, "density": p.min_pixels, "merge": p.merge_pt, "auto_align": p.auto_align })).collect::<Vec<_>>(),
        }))
    },
};

/// Compare options from tool arguments (shared with the batch tools).
pub(crate) fn compare_options(a: &crate::Automation, args: &crate::Args) -> Result<CompareOptions> {
    let mut o = CompareOptions::default();
    if let Some(name) = args.opt_str("preset")?
        && o.apply_preset(name).is_err()
    {
        let custom = load_presets(&presets_path(a)?)?;
        let p = custom
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| bad_args(format!("unknown preset {name:?} (compare_preset lists them)")))?;
        p.apply(&mut o);
    }
    if let Some(al) = args.opt_str("align")? {
        o.align = match al {
            "page" => CompareAlign::Page,
            "auto" => CompareAlign::Auto,
            "offset" => {
                let v: Vec<f64> = args
                    .get("offset")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_f64).collect())
                    .unwrap_or_default();
                match v.as_slice() {
                    [dx, dy] => CompareAlign::Offset { dx: *dx, dy: *dy },
                    _ => return Err(bad_args("align offset needs offset: [dx, dy]")),
                }
            }
            "points" => {
                let two = |k: &str| -> Result<[Point; 2]> {
                    let v: Vec<Point> = args
                        .get(k)
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(point_value).collect())
                        .unwrap_or_default();
                    match v.as_slice() {
                        [p, q] => Ok([*p, *q]),
                        _ => Err(bad_args(format!("align points needs {k}: [[x, y], [x, y]]"))),
                    }
                };
                CompareAlign::Points {
                    old: two("old_points")?,
                    new: two("new_points")?,
                }
            }
            other => return Err(bad_args(format!("align {other:?}"))),
        };
    }
    o.fill = args.opt_color("fill")?;
    if let Some(v) = args.opt_num("fill_opacity")? {
        o.fill_opacity = v;
    }
    if let Some(v) = args.opt_num("opacity")? {
        o.opacity = v;
    }
    o.lock = args.bool_or("lock", false)?;
    o.include_flattened = args.bool_or("include_flattened", false)?;
    compare_tuning(args, o)
}

fn compare_tuning(args: &crate::Args, mut o: CompareOptions) -> Result<CompareOptions> {
    if let Some(m) = args.opt_str("mode")? {
        o.mode = CompareMode::from_name(m).ok_or_else(|| bad_args("mode must be text, graphics or both"))?;
    }
    if let Some(p) = args.get("pairs").filter(|_| args.tool() == "compare_documents") {
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
    Ok(o)
}

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
                            "align": { "type": "string", "enum": ["page", "bounds", "points", "auto"], "description": "points: two or three matching points (three: a full affine map)." },
                            "from": { "type": "array", "items": pt.clone(), "description": "align points: two or three points on this layer's page." },
                            "to": { "type": "array", "items": pt, "description": "align points: where they land on the first layer's page." },
                            "background": { "type": ["string", "array"], "description": "Colour of the layer's whitespace (default transparent)." },
                            "blend": { "type": "string", "enum": ["multiply", "darken", "normal", "screen", "difference"] },
                            "region": rect_arg("Only this region of the layer's page"),
                            "rotation": { "type": "number" },
                            "scale": { "type": "number" },
                            "dx": { "type": "number" },
                            "dy": { "type": "number" }
                        },
                        "required": ["path"]
                    }
                },
                "out": path_arg("The overlay PDF to write"),
                "defaults": { "type": "object", "description": "Edit Defaults: blend, rotation, scale, dx, dy for every layer that does not set its own." },
                "include_flattened": { "type": "boolean", "description": "Include recoverable flattened markups (default false)." },
                "advanced_shading": { "type": "boolean", "description": "Advanced Color Shading: layer colours laid on with Screen instead of Lighten, so grey linework, fills and hatching keep their tone (default false)." }
            }),
            &["layers", "out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let include_flattened = args.bool_or("include_flattened", false)?;
        let empty = serde_json::Map::new();
        let defaults = match args.get("defaults") {
            Some(Value::Object(d)) => d,
            Some(_) => return Err(bad_args("defaults must be an object")),
            None => &empty,
        };
        if let Some(k) = defaults
            .keys()
            .find(|k| !["blend", "rotation", "scale", "dx", "dy"].contains(&k.as_str()))
        {
            return Err(bad_args(format!("defaults: unknown key {k:?}")));
        }
        let list = args
            .get("layers")
            .and_then(Value::as_array)
            .ok_or_else(|| bad_args("layers must be a list"))?;
        let mut layers = Vec::new();
        for (i, l) in list.iter().enumerate() {
            let o = l.as_object().ok_or_else(|| bad_args("each layer must be an object"))?;
            const KEYS: [&str; 15] = [
                "path",
                "pages",
                "color",
                "opacity",
                "name",
                "align",
                "from",
                "to",
                "background",
                "blend",
                "region",
                "rotation",
                "scale",
                "dx",
                "dy",
            ];
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
                "auto" => OverlayAlign::Auto,
                "points" => {
                    let pts = |k: &str| -> Vec<Point> {
                        o.get(k)
                            .and_then(Value::as_array)
                            .map(|a| a.iter().filter_map(point_value).collect())
                            .unwrap_or_default()
                    };
                    match (pts("from").as_slice(), pts("to").as_slice()) {
                        ([a, b], [c, d]) => OverlayAlign::Points {
                            from: [*a, *b],
                            to: [*c, *d],
                        },
                        ([a, b, c], [d, e, f]) => OverlayAlign::Three {
                            from: [*a, *b, *c],
                            to: [*d, *e, *f],
                        },
                        _ => return Err(bad_args("align points needs from and to: two or three [x, y] each")),
                    }
                }
                other => return Err(bad_args(format!("align {other:?}: use page, bounds or points"))),
            };
            let num = |k: &str| {
                o.get(k)
                    .and_then(Value::as_f64)
                    .or_else(|| defaults.get(k).and_then(Value::as_f64))
            };
            let blend = match o.get("blend").or_else(|| defaults.get("blend")).and_then(Value::as_str) {
                Some(b) => OverlayBlend::from_name(b).ok_or_else(|| bad_args(format!("unknown blend {b:?}")))?,
                None => OverlayBlend::Multiply,
            };
            let background = match o.get("background") {
                Some(c) if !c.is_null() => Some(color_value("background", c)?),
                _ => None,
            };
            let region = match o.get("region") {
                Some(r) if !r.is_null() => {
                    Some(crate::args::rect_of(r).ok_or_else(|| bad_args("region must be [x0, y0, x1, y1]"))?)
                }
                _ => None,
            };
            let bytes = markupcraft_engine::raster::read_pdf(&path)?;
            let bytes = if include_flattened {
                bytes
            } else {
                markupcraft_engine::flatten::without_flattened(bytes)
            };
            layers.push(OverlayLayer {
                pages,
                color,
                opacity,
                name,
                align,
                background,
                blend,
                region,
                adjust: LayerAdjust {
                    rotation: num("rotation").unwrap_or(0.0),
                    scale: num("scale").unwrap_or(1.0),
                    dx: num("dx").unwrap_or(0.0),
                    dy: num("dy").unwrap_or(0.0),
                },
                ..OverlayLayer::new(bytes)
            });
        }
        let shaded = args.bool_or("advanced_shading", false)?;
        let r = overlay_pages_shaded(&layers, &out, shaded)?;
        Ok(json!({ "out": out.display().to_string(), "pages": r.pages, "layers": r.layers }))
    },
};
