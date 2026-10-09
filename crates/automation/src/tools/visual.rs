//! Visual Search.

use markupcraft_engine::visual::{VisualAction, VisualSearchOptions};
use serde_json::{Value, json};

use super::{Tool, page_arg, pages_arg, rect_arg, schema};
use crate::{bad_args, summary};

pub static SEARCH: Tool = Tool {
    name: "visual_search",
    title: "Visual Search",
    description: "Box a symbol (rect on page) and find every graphically similar instance on the searched pages (default all): normalized cross-correlation of the rendered region, also turned 90/180/270 degrees unless rotations is false. Returns hit rectangles with scores. action \"count\" adds one Count measurement per page with a point on each hit; \"highlight\" adds a translucent rectangle on each (one undoable step).",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("the symbol is on"),
                "rect": rect_arg("The symbol's box"),
                "pages": pages_arg("to search (default: all)"),
                "sensitivity": { "type": "number", "minimum": 0, "maximum": 1, "description": "0 = near-exact matches only, 1 = loose (default 0.5)." },
                "rotations": { "type": "boolean", "description": "Also find the symbol turned by 90, 180, 270 degrees (default true)." },
                "fine_rotations": { "type": "boolean", "description": "With rotations: in 45-degree steps (default false)." },
                "color_filter": { "type": "boolean", "description": "Only hits in the symbol's colour (default false)." },
                "limit_to_selection": { "type": "boolean", "description": "Ignore linework that runs out of the box (default false)." },
                "thumbnails": { "type": "integer", "minimum": 16, "maximum": 512, "description": "Also return each hit as a PNG thumbnail (base64) at most this many pixels across." },
                "dpi": { "type": "number", "description": "Rendering resolution (default 100)." },
                "max_hits": { "type": "integer", "minimum": 1, "description": "At most this many hits (default 1000)." },
                "action": { "type": "string", "enum": ["none", "count", "highlight"], "description": "What to do with the hits (default none)." },
                "color": { "type": ["string", "array"], "description": "Colour of the markups." },
                "subject": { "type": "string", "description": "Subject of the markups (default Visual Search)." }
            }),
            &["page", "rect"],
        )
    },
    run: |a, args| {
        let mut o = VisualSearchOptions {
            page: args.page("page")?,
            region: args.opt_rect("rect")?.ok_or_else(|| bad_args("rect is required"))?,
            ..Default::default()
        };
        if let Some(v) = args.opt_num("sensitivity")? {
            o.sensitivity = v;
        }
        o.rotations = args.bool_or("rotations", true)?;
        o.fine_rotations = args.bool_or("fine_rotations", false)?;
        o.color_filter = args.bool_or("color_filter", false)?;
        o.limit_to_selection = args.bool_or("limit_to_selection", false)?;
        let thumbs = args.opt_u64("thumbnails")?;
        if let Some(v) = args.opt_num("dpi")? {
            o.dpi = v;
        }
        if let Some(v) = args.opt_u64("max_hits")? {
            o.max_hits = v as usize;
        }
        if let Some(s) = args.opt_str("action")? {
            o.action = VisualAction::from_name(s).ok_or_else(|| bad_args("action must be none, count or highlight"))?;
        }
        if let Some(c) = args.opt_color("color")? {
            o.color = c;
        }
        if let Some(s) = args.opt_string("subject")? {
            o.subject = s;
        }
        let (doc, s) = a.session(args)?;
        if let Some(p) = args.opt_pages("pages", s.page_count())? {
            o.pages = p;
        }
        let r = s.visual_search(&o)?;
        let mut hits: Vec<Value> = Vec::new();
        for h in &r.hits {
            let mut v = json!({ "page": h.page + 1, "rect": h.rect.as_array(), "score": (h.score * 1000.0).round() / 1000.0, "rotation": h.rotation });
            if let Some(px) = thumbs
                && hits.len() < 200
            {
                let png = s.region_png(h.page, h.rect.padded(2.0), px as u32)?;
                v["thumbnail"] = json!(super::convert::base64(&png));
            }
            hits.push(v);
        }
        Ok(json!({ "count": hits.len(), "hits": hits, "markups": r.markups, "document": summary(doc, s) }))
    },
};
