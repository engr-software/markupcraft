//! Flatten markups into the page content.

use markupcraft_engine::flatten::FlattenFilter;
use serde_json::json;

use super::{Tool, pages_arg, schema};
use crate::summary;

pub static FLATTEN: Tool = Tool {
    name: "markup_flatten",
    title: "Flatten markups",
    description: "Burn markups into the page content (they stop being markups). Which: all, or those matching every filter given: ids, pages, kinds (as markup_list names them), layers, authors. The next save rewrites the file in full. Undoable until saved.",
    read_only: false,
    destructive: true,
    schema: || {
        let strings = |d: &str| json!({ "type": "array", "items": { "type": "string" }, "description": d });
        schema(
            json!({
                "ids": strings("Markup ids."),
                "pages": pages_arg("whose markups to flatten"),
                "kinds": strings("Kinds, e.g. [\"Cloud\", \"Text\"]."),
                "layers": strings("Layer names."),
                "authors": strings("Authors."),
                "all": { "type": "boolean", "description": "true to flatten every markup when no filter is given." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let filter = FlattenFilter {
            ids: args.opt_strings("ids")?.unwrap_or_default(),
            pages: args.opt_pages("pages", s.page_count())?.unwrap_or_default(),
            kinds: args.opt_strings("kinds")?.unwrap_or_default(),
            layers: args.opt_strings("layers")?.unwrap_or_default(),
            authors: args.opt_strings("authors")?.unwrap_or_default(),
        };
        if filter == FlattenFilter::default() && !args.bool_or("all", false)? {
            return Err(crate::bad_args(
                "markup_flatten: give a filter (ids, pages, kinds, layers, authors) or all: true",
            ));
        }
        let before = s.doc().markups.len();
        let n = s.flatten_markups(&filter)?;
        Ok(json!({ "flattened": n, "markups_before": before, "document": summary(doc, s) }))
    },
};
