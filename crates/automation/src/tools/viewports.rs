//! Viewport management: rename, rescale, clear, copy to other pages.

use serde_json::{Value, json};

use super::{Tool, page_arg, pages_arg, scale_arg, schema};
use crate::{Result, bad_args, summary};

pub static VIEWPORT_EDIT: Tool = Tool {
    name: "viewport_edit",
    title: "Rename, rescale, clear or copy viewports",
    description: "One change per call on a page's viewports: rename {index, name}; scale {index, scale, update_markups} (measurements inside follow unless update_markups is false); clear (every viewport but the page scale; with all: true the page scale too); copy_to pages (the page's partial viewports, or just index, at the same place on those pages; \"all\" = every other page). index is 1-based as doc_info lists them. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("the viewports are on"),
                "index": { "type": "integer", "minimum": 1 },
                "rename": { "type": "string" },
                "scale": scale_arg(),
                "update_markups": { "type": "boolean" },
                "clear": { "type": "boolean" },
                "all": { "type": "boolean" },
                "copy_to": pages_arg("to copy to, or \"all\"")
            }),
            &["page"],
        )
    },
    run: |a, args| {
        let ops = ["rename", "scale", "clear", "copy_to"];
        let given: Vec<&str> = ops.iter().copied().filter(|k| args.has(k)).collect();
        if given.len() != 1 {
            return Err(bad_args(format!(
                "viewport_edit takes exactly one of {}",
                ops.join(", ")
            )));
        }
        let page = args.page("page")?;
        let index = if args.has("index") {
            Some(args.position("index")?)
        } else {
            None
        };
        let need_index = || -> Result<usize> { index.ok_or_else(|| bad_args("index is required")) };
        let (doc, s) = a.session(args)?;
        let changed: Value = match given.first().copied().unwrap_or_default() {
            "rename" => {
                s.rename_viewport(page, need_index()?, args.str("rename")?)?;
                json!(1)
            }
            "scale" => {
                let sc = args.opt_scale("scale")?.ok_or_else(|| bad_args("missing scale"))?;
                let n = s.set_viewport_scale(page, need_index()?, &sc, args.bool_or("update_markups", true)?)?;
                json!({ "measurements_updated": n })
            }
            "clear" => {
                let keep = !args.bool_or("all", false)?;
                json!(s.clear_viewports(page, keep)?)
            }
            _ => {
                let all = args
                    .get("copy_to")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.eq_ignore_ascii_case("all"));
                let pages = if all {
                    (0..s.page_count()).collect()
                } else {
                    args.pages("copy_to", s.page_count())?
                };
                json!(s.copy_viewports(page, index, &pages)?)
            }
        };
        Ok(json!({ "changed": changed, "document": summary(doc, s) }))
    },
};
