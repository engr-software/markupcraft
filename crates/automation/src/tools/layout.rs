//! Arranging markups against each other: align, distribute, flip, copy to other pages and
//! remove from a group (Revu's Markup > Arrange and Apply to All Pages).

use markupcraft_engine::align::Align;
use serde_json::{Value, json};

use super::{Tool, ids_arg, markup_json, pages_arg, schema, target_ids};
use crate::{Result, bad_args, summary};

pub static ALIGN: Tool = Tool {
    name: "markup_align",
    title: "Align, distribute or flip markups",
    description: "One change per call on markups (ids, or the selection): align left|center|right|top|middle|bottom (to the joint extent; two or more markups), distribute horizontal|vertical (equal gaps; three or more), flip horizontal|vertical (about the joint centre), to_pages (copies at the same place on these pages; \"all\" = every other page), or remove_from_group. Locked markups are refused. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "ids": ids_arg(),
                "align": { "type": "string", "enum": ["left", "center", "right", "top", "middle", "bottom"] },
                "distribute": { "type": "string", "enum": ["horizontal", "vertical"] },
                "flip": { "type": "string", "enum": ["horizontal", "vertical"] },
                "to_pages": pages_arg("to copy the markups to, or \"all\""),
                "remove_from_group": { "type": "boolean" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let ops = ["align", "distribute", "flip", "to_pages", "remove_from_group"];
        let given: Vec<&str> = ops.iter().copied().filter(|k| args.has(k)).collect();
        if given.len() != 1 {
            return Err(bad_args(format!(
                "markup_align takes exactly one of {} (got {})",
                ops.join(", "),
                if given.is_empty() {
                    "none".into()
                } else {
                    given.join(", ")
                }
            )));
        }
        let horizontal = |key: &str| -> Result<bool> {
            match args.str(key)? {
                "horizontal" => Ok(true),
                "vertical" => Ok(false),
                o => Err(bad_args(format!("{key} must be horizontal or vertical (got {o})"))),
            }
        };
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let mut new_ids: Vec<String> = Vec::new();
        let changed = match given.first().copied().unwrap_or_default() {
            "align" => {
                let how = Align::from_name(args.str("align")?)
                    .ok_or_else(|| bad_args("align must be left, center, right, top, middle or bottom"))?;
                s.align_markups(&ids, how)?
            }
            "distribute" => s.distribute_markups(&ids, horizontal("distribute")?)?,
            "flip" => s.flip_markups(&ids, horizontal("flip")?)?,
            "to_pages" => {
                let all = args
                    .get("to_pages")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.eq_ignore_ascii_case("all"));
                let pages = if all {
                    Vec::new()
                } else {
                    args.pages("to_pages", s.page_count())?
                };
                new_ids = s.copy_to_pages(&ids, &pages)?;
                new_ids.len()
            }
            _ => {
                if !args.bool_or("remove_from_group", false)? {
                    return Err(bad_args("remove_from_group must be true"));
                }
                s.remove_from_group(&ids)?
            }
        };
        let list: Vec<Value> = ids
            .iter()
            .chain(&new_ids)
            .filter_map(|id| s.markup(id).ok())
            .map(markup_json)
            .collect();
        Ok(json!({ "changed": changed, "new_ids": new_ids, "markups": list, "document": summary(doc, s) }))
    },
};
