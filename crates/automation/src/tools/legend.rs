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
        o.insert(
            "pages".into(),
            json!({ "type": ["array", "string"], "description": "Pages counted (a list or range like \"2-5\"; default: the scope)." }),
        );
        o.insert(
            "ids".into(),
            json!({ "type": "array", "items": { "type": "string" }, "description": "Only these markups (a legend of a selection)." }),
        );
        o.insert("show_empty".into(), json!({ "type": "boolean", "description": "List every subject in `subjects` even with no markups (a tool set's legend)." }));
        o.insert(
            "custom_columns".into(),
            json!({ "type": "array", "items": { "type": "string" }, "description": "More Markups List columns shown after the columns, their values splitting rows: any list column by id or header (Layer, Author, Measurement, Status, Label, ...) or a custom column's id or name." }),
        );
        o.insert("border_color".into(), json!({ "type": ["string", "array"] }));
        o.insert(
            "fill_color".into(),
            json!({ "type": ["string", "array"], "description": "\"none\" = no fill." }),
        );
        o.insert(
            "opacity".into(),
            json!({ "type": "number", "minimum": 0, "maximum": 1 }),
        );
        o.insert(
            "line_width".into(),
            json!({ "type": "number", "minimum": 0, "maximum": 12 }),
        );
        o.insert(
            "symbol_scale".into(),
            json!({ "type": "number", "minimum": 0.25, "maximum": 4 }),
        );
        o.insert(
            "header".into(),
            json!({ "type": "boolean", "description": "Show the header row." }),
        );
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
    if args.get("pages").is_some() {
        o.pages = args.opt_pages("pages", 100_000)?.unwrap_or_default();
    }
    if let Some(ids) = args.opt_strings("ids")? {
        o.ids = ids;
    }
    if let Some(b) = args.opt_bool("show_empty")? {
        o.show_empty = b;
    }
    if let Some(c) = args.opt_strings("custom_columns")? {
        o.custom_columns = c;
    }
    if let Some(c) = args.opt_color("border_color")? {
        o.border_color = c;
    }
    if args.get("fill_color").and_then(Value::as_str) == Some("none") {
        o.fill_color = None;
    } else if let Some(c) = args.opt_color("fill_color")? {
        o.fill_color = Some(c);
    }
    if let Some(v) = args.opt_num("opacity")? {
        o.opacity = v;
    }
    if let Some(v) = args.opt_num("line_width")? {
        o.line_width = v;
    }
    if let Some(v) = args.opt_num("symbol_scale")? {
        o.symbol_scale = v;
    }
    if let Some(b) = args.opt_bool("header")? {
        o.header = b;
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
            "custom": r.custom,
        })).collect::<Vec<_>>(),
        "pages": l.options.pages.iter().map(|p| p + 1).collect::<Vec<_>>(),
        "show_empty": l.options.show_empty,
        "custom_columns": l.options.custom_columns,
        "border_color": l.options.border_color.hex(),
        "fill_color": l.options.fill_color.map(|c| c.hex()),
        "opacity": l.options.opacity,
        "line_width": l.options.line_width,
        "symbol_scale": l.options.symbol_scale,
        "header": l.options.header,
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

pub static COPY: Tool = Tool {
    name: "legend_copy",
    title: "Copy legend to pages",
    description: "Legend distribution: a copy of legend `id` on every other page at the same position (each counts its own page unless the legend's scope is the document). Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "id": { "type": "string" } }), &["id"]),
    run: |a, args| {
        let id = args.str("id")?.to_string();
        let (doc, s) = a.session(args)?;
        let made = s.copy_legend_to_pages(&id)?;
        Ok(json!({ "legends": made, "document": summary(doc, s) }))
    },
};

pub static FREEZE: Tool = Tool {
    name: "legend_freeze",
    title: "Snapshot legend",
    description: "Turn legend `id` into a static copy that stays as drawn and no longer follows the markups. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "id": { "type": "string" } }), &["id"]),
    run: |a, args| {
        let id = args.str("id")?.to_string();
        let (doc, s) = a.session(args)?;
        s.freeze_legend(&id)?;
        Ok(json!({ "frozen": id, "legends": s.legends().len(), "document": summary(doc, s) }))
    },
};
