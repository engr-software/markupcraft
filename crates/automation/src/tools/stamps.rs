//! Stamps: our built-in designs, the user's stamp library, dynamic fields, image stamps.

use std::collections::BTreeMap;

use markupcraft_engine::extras6::StampSettings;
use markupcraft_engine::stamps::{StampLibrary, StampPlace, StampSource, stamp_prompts};
use markupcraft_engine::{Color, MarkupPatch};
use serde_json::{Value, json};

use super::{Tool, markup_json, page_arg, path_arg, point_arg, rect_arg, schema, schema_nodoc};
use crate::{Automation, Result, bad_args, summary};

fn library(a: &Automation) -> Result<StampLibrary> {
    Ok(StampLibrary::in_config(&a.config_dir()?))
}

pub static LIST: Tool = Tool {
    name: "stamp_list",
    title: "Stamps",
    description: "Every stamp: our built-in designs (Approved, Approved As Noted, Reviewed, Revise and Resubmit, Rejected, Draft, Void, For Construction, Not For Construction, For Information Only) and the user's library (text and image stamps), with their text templates and the {prompt:...} fields they ask for.",
    read_only: true,
    destructive: false,
    schema: || schema_nodoc(json!({}), &[]),
    run: |a, _args| {
        let lib = library(a)?;
        let list: Vec<Value> = lib
            .all()?
            .iter()
            .map(|e| {
                json!({
                    "id": e.id, "name": e.name, "text": e.text, "color": e.color,
                    "image": e.image.is_some(), "builtin": e.builtin,
                    "prompts": stamp_prompts(&e.text).iter().map(|p| json!({ "label": p.label, "default": p.default })).collect::<Vec<_>>(),
                })
            })
            .collect();
        Ok(json!({ "stamps": list }))
    },
};

pub static CREATE: Tool = Tool {
    name: "stamp_create",
    title: "Create a stamp",
    description: "Add a stamp to the user's library: a text stamp (`text`, first line big, may use dynamic fields like {user} {date} {date:MMM d, yyyy} {time} {page} {pages} {file} {prompt:Label=Default}; `color`), or an image stamp from a PNG or a PDF (`image`, copied into the library).",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({
                "name": { "type": "string" },
                "text": { "type": "string" },
                "color": { "type": ["string", "array"] },
                "image": path_arg("A .png or .pdf file")
            }),
            &["name"],
        )
    },
    run: |a, args| {
        let name = args.str("name")?.to_string();
        let lib = library(a)?;
        let id = match (args.opt_string("text")?, args.opt_str("image")?) {
            (Some(t), None) => {
                let c = args.opt_color("color")?.unwrap_or(Color::rgb(0.05, 0.3, 0.8));
                lib.add_text(&name, &t, c)?
            }
            (None, Some(img)) => {
                let p = a.resolve(img, false)?;
                lib.add_image(&name, &p)?
            }
            _ => return Err(bad_args("stamp_create: give text or image")),
        };
        Ok(json!({ "id": id }))
    },
};

pub static REMOVE: Tool = Tool {
    name: "stamp_remove",
    title: "Remove a stamp",
    description: "Remove a user stamp from the library (built-in designs stay).",
    read_only: false,
    destructive: true,
    schema: || schema_nodoc(json!({ "id": { "type": "string" } }), &["id"]),
    run: |a, args| {
        let id = args.str("id")?;
        library(a)?.remove(id)?;
        Ok(json!({ "removed": id }))
    },
};

pub static ADD: Tool = Tool {
    name: "stamp_add",
    title: "Place a stamp",
    description: "Place a stamp on `page`, in `rect` or centred at `at`: a library stamp (`stamp` id), free `text` (dynamic fields filled), or an `image` file (PNG, or `image_page` of a PDF). `answers` fill {prompt:...} fields; `time` (seconds since 1970) fixes the date and time fields. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("to stamp"),
                "rect": rect_arg("The stamp's box"),
                "at": point_arg("The stamp's centre (default size)"),
                "stamp": { "type": "string", "description": "A stamp id (stamp_list)." },
                "text": { "type": "string" },
                "color": { "type": ["string", "array"] },
                "image": path_arg("A .png or .pdf file"),
                "image_page": page_arg("of the PDF to use (default 1)"),
                "answers": { "type": "object", "description": "{prompt label: text}" },
                "time": { "type": "integer" },
                "opacity": { "type": "number", "description": "0.05 to 1 (default: the stamp settings')." },
                "blend": { "type": "string", "enum": ["normal", "multiply"], "description": "Default: the stamp settings'." },
                "lock": { "type": "boolean", "description": "Default: the stamp settings'." }
            }),
            &["page"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let at = match (args.opt_rect("rect")?, args.opt_point("at")?) {
            (Some(r), None) => StampPlace::Rect(r),
            (None, Some(p)) => StampPlace::Center(p),
            _ => return Err(bad_args("stamp_add: give rect or at")),
        };
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
                page: args.opt_page("image_page")?.unwrap_or(0),
            },
            _ => return Err(bad_args("stamp_add: give exactly one of stamp, text or image")),
        };
        let mut answers = BTreeMap::new();
        if let Some(o) = args.get("answers") {
            let o = o.as_object().ok_or_else(|| bad_args("answers is an object of texts"))?;
            for (k, v) in o {
                let v = v.as_str().ok_or_else(|| bad_args("answers is an object of texts"))?;
                answers.insert(k.clone(), v.to_string());
            }
        }
        let when = args.opt_int("time")?;
        // The stamp settings (Tools > Stamp settings) give every placed stamp its library,
        // opacity, blend mode and lock; the arguments override them for this stamp.
        let config = a.config_dir().ok();
        let mut st = config
            .as_deref()
            .and_then(|d| StampSettings::load(d).ok())
            .unwrap_or_default();
        let lib = config.as_deref().map(|d| st.library(d));
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
        let (doc, s) = a.session(args)?;
        s.set_merge_key(Some("stamp-add"));
        let placed = (|| -> markupcraft_engine::Result<String> {
            let id = s.place_stamp(page, at, &source, lib.as_ref(), &answers, when)?;
            if st.opacity < 1.0 || st.blend == "multiply" || st.lock {
                let patch = MarkupPatch {
                    opacity: Some(st.opacity),
                    multiply: Some(st.blend == "multiply"),
                    locked: Some(st.lock),
                    ..Default::default()
                };
                s.set_properties(std::slice::from_ref(&id), &patch)?;
            }
            Ok(id)
        })();
        s.set_merge_key(None);
        s.seal();
        let id = placed?;
        Ok(json!({ "id": id, "markup": markup_json(s.markup(&id)?), "document": summary(doc, s) }))
    },
};
