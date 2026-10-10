//! Apply Stamp: one stamp on many pages at one spot (and on many files through batch_apply).

use std::collections::BTreeMap;

use markupcraft_engine::Color;
use markupcraft_engine::extras6::StampSettings;
use markupcraft_engine::stamp_apply::{Anchor, ApplyStamp, PageFilter};
use markupcraft_engine::stamps::{StampLibrary, StampSource};
use serde_json::json;

use super::{Tool, pages_arg, path_arg, schema};
use crate::{bad_args, summary};

pub static APPLY_STAMP: Tool = Tool {
    name: "stamp_apply",
    title: "Apply Stamp",
    description: "Apply one stamp (a library `stamp` id, `text`, or an `image` file) at the same spot on many pages: `pages` (default all) narrowed by `filter` (all, odd, even, portrait, landscape); `anchor` on the page's 3 x 3 grid (top_left ... bottom_right, default center) with `offset_x` / `offset_y` in points (towards the middle from an edge anchor); `scale` (0.1 to 10) and `rotation` (degrees counter-clockwise); opacity, blend and lock default to the stamp settings'. In batch_apply it stamps many files. One undo step.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "stamp": { "type": "string", "description": "A stamp id (stamp_list)." },
                "text": { "type": "string" },
                "color": { "type": ["string", "array"] },
                "image": path_arg("A .png or .pdf file"),
                "pages": pages_arg("to stamp (default all)"),
                "filter": { "type": "string", "enum": ["all", "odd", "even", "portrait", "landscape"] },
                "anchor": { "type": "string", "enum": ["top_left", "top", "top_right", "left", "center", "right", "bottom_left", "bottom", "bottom_right"] },
                "offset_x": { "type": "number" },
                "offset_y": { "type": "number" },
                "scale": { "type": "number", "minimum": 0.1, "maximum": 10 },
                "rotation": { "type": "number" },
                "answers": { "type": "object" },
                "time": { "type": "integer" },
                "opacity": { "type": "number" },
                "blend": { "type": "string", "enum": ["normal", "multiply"] },
                "lock": { "type": "boolean" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let source = match (
            args.opt_string("stamp")?,
            args.opt_string("text")?,
            args.opt_str("image")?,
        ) {
            (Some(id), None, None) => StampSource::Library(id),
            (None, Some(text), None) => StampSource::Text {
                text,
                color: args.opt_color("color")?.unwrap_or(Color::RED),
            },
            (None, None, Some(img)) => StampSource::File {
                path: a.resolve(img, false)?,
                page: 0,
            },
            _ => return Err(bad_args("stamp_apply: give exactly one of stamp, text or image")),
        };
        let mut job = ApplyStamp::new(source);
        if let Some(f) = args.opt_str("filter")? {
            job.filter =
                PageFilter::from_name(f).ok_or_else(|| bad_args("filter is all, odd, even, portrait or landscape"))?;
        }
        if let Some(n) = args.opt_str("anchor")? {
            job.anchor =
                Anchor::from_name(n).ok_or_else(|| bad_args("anchor is top_left ... bottom_right or center"))?;
        }
        job.offset = (
            args.opt_num("offset_x")?.unwrap_or(0.0),
            args.opt_num("offset_y")?.unwrap_or(0.0),
        );
        job.scale = args.opt_num("scale")?.unwrap_or(1.0);
        job.rotation = args.opt_num("rotation")?.unwrap_or(0.0);
        let mut answers = BTreeMap::new();
        if let Some(o) = args.get("answers") {
            let o = o.as_object().ok_or_else(|| bad_args("answers is an object of texts"))?;
            for (k, v) in o {
                let v = v.as_str().ok_or_else(|| bad_args("answers is an object of texts"))?;
                answers.insert(k.clone(), v.to_string());
            }
        }
        job.answers = answers;
        job.when = args.opt_int("time")?;
        let config = a.config_dir().ok();
        let mut st = config
            .as_deref()
            .and_then(|d| StampSettings::load(d).ok())
            .unwrap_or_default();
        let lib: Option<StampLibrary> = config.as_deref().map(|d| st.library(d));
        if let Some(v) = args.opt_num("opacity")? {
            st.opacity = v;
        }
        if let Some(v) = args.opt_string("blend")? {
            st.blend = v;
        }
        if let Some(v) = args.opt_bool("lock")? {
            st.lock = v;
        }
        st.validate()?;
        job.opacity = st.opacity;
        job.multiply = st.blend == "multiply";
        job.lock = st.lock;
        let (doc, s) = a.session(args)?;
        job.pages = args.opt_pages("pages", s.page_count())?.unwrap_or_default();
        let ids = s.apply_stamp(&job, lib.as_ref())?;
        Ok(json!({
            "ids": ids,
            "pages": ids.iter().filter_map(|i| s.markup(i).ok()).map(|m| m.page + 1).collect::<Vec<_>>(),
            "document": summary(doc, s),
        }))
    },
};
