//! Legends: a table markup of the markups on a page (or the document) by subject.

use markupcraft_engine::legend::{LegendColumn, LegendInfo, LegendOptions};
use serde_json::{Value, json};

use super::{Tool, page_arg, point_arg, schema};
use crate::{Args, Result, bad_args, summary};

fn options_schema(mut props: Value) -> Value {
    if let Some(o) = props.as_object_mut() {
        o.insert("title".into(), json!({ "type": "string" }));
        o.insert(
            "scope".into(),
            json!({ "type": "string", "enum": ["page", "document"], "description": "Markups of the legend's page (default) or of every page." }),
        );
        o.insert(
            "columns".into(),
            json!({ "type": "array", "items": { "type": "string", "enum": ["Symbol", "Subject", "Type", "Count", "Total"] } }),
        );
        o.insert(
            "font_size".into(),
            json!({ "type": "number", "minimum": 4, "maximum": 72 }),
        );
        o.insert(
            "subjects".into(),
            json!({ "type": "array", "items": { "type": "string" }, "description": "Only these subjects (default: all)." }),
        );
        o.insert("measurements_only".into(), json!({ "type": "boolean" }));
    }
    props
}

fn read_options(args: &Args, mut o: LegendOptions) -> Result<LegendOptions> {
    if let Some(t) = args.opt_string("title")? {
        o.title = t;
    }
    if let Some(sc) = args.opt_str("scope")? {
        o.document = match sc {
            "page" => false,
            "document" => true,
            other => return Err(bad_args(format!("scope is page or document, not {other:?}"))),
        };
    }
    if let Some(cols) = args.opt_strings("columns")? {
        o.columns = cols
            .iter()
            .map(|c| LegendColumn::from_name(c).ok_or_else(|| bad_args(format!("unknown legend column {c:?}"))))
            .collect::<Result<_>>()?;
    }
    if let Some(sz) = args.opt_num("font_size")? {
        o.font_size = sz;
    }
    if let Some(s) = args.opt_strings("subjects")? {
        o.subjects = s;
    }
    if let Some(m) = args.opt_bool("measurements_only")? {
        o.measurements_only = m;
    }
    Ok(o)
}

fn legend_json(l: &LegendInfo) -> Value {
    json!({
        "id": l.id,
        "page": l.page + 1,
        "title": l.options.title,
        "scope": if l.options.document { "document" } else { "page" },
        "columns": l.options.columns.iter().map(|c| c.name()).collect::<Vec<_>>(),
        "rows": l.rows.iter().map(|r| json!({
            "subject": r.subject, "type": r.kind.name(), "color": r.color.hex(),
            "markups": r.markups, "count": r.count, "total": r.total, "total_text": r.total_text,
        })).collect::<Vec<_>>(),
    })
}

pub static ADD: Tool = Tool {
    name: "legend_add",
    title: "Add a legend",
    description: "Add a legend with its top-left corner at `at` on `page`: a table of the markups by subject with a symbol in their colours, how many and their total quantity. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            options_schema(json!({ "page": page_arg("to put it on"), "at": point_arg("Its top-left corner") })),
            &["page", "at"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let at = args.point("at")?;
        let o = read_options(args, LegendOptions::default())?;
        let (doc, s) = a.session(args)?;
        let id = s.add_legend(page, at, &o)?;
        let l = s.legends().into_iter().find(|l| l.id == id).map(|l| legend_json(&l));
        Ok(json!({ "id": id, "legend": l, "document": summary(doc, s) }))
    },
};

pub static LIST: Tool = Tool {
    name: "legend_list",
    title: "List legends",
    description: "Every legend with its options and the rows it shows now.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session_ref(args)?;
        let l: Vec<Value> = s.legends().iter().map(legend_json).collect();
        Ok(json!({ "doc": doc, "legends": l }))
    },
};

pub static UPDATE: Tool = Tool {
    name: "legend_update",
    title: "Update legends",
    description: "Recompute legends from the markups as they are now. With `id`, also change that legend's options (title, scope, columns, font_size, subjects, measurements_only). Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(options_schema(json!({ "id": { "type": "string" } })), &[]),
    run: |a, args| {
        let id = args.opt_string("id")?;
        let (doc, s) = a.session(args)?;
        let n = match id {
            Some(id) => {
                let cur = s
                    .legends()
                    .into_iter()
                    .find(|l| l.id == id)
                    .ok_or_else(|| bad_args(format!("{id:?} is not a legend (legend_list shows them)")))?;
                let o = read_options(args, cur.options)?;
                s.set_legend_options(&id, &o)?;
                s.update_legends()?
            }
            None => s.update_legends()?,
        };
        Ok(json!({ "legends": n, "document": summary(doc, s) }))
    },
};
