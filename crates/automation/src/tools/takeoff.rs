//! Takeoff editing the app's panels do: cutouts in Area measurements, Markups List cells, and
//! the document's custom columns.

use markupcraft_model::{ChoiceItem, ColumnType, CustomColumn, make_column_id};
use serde_json::{Value, json};

use super::{Tool, points_arg, schema};
use crate::{Result, bad_args, summary};

/// Most custom columns accepted in one call.
const MAX_COLUMNS: usize = 256;

pub static CUTOUT_ADD: Tool = Tool {
    name: "cutout_add",
    title: "Cut a hole in an area",
    description: "Add a cutout (deduction) to an Area or Volume measurement: `points` is the hole's outline, inside the area. The measured value drops by the hole's area. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "id": { "type": "string", "description": "The Area measurement (markup_list shows ids)." },
                "points": points_arg("The cutout outline, at least 3 points")
            }),
            &["id", "points"],
        )
    },
    run: |a, args| {
        let id = args.str("id")?.to_string();
        let ring = args.points("points")?;
        let (doc, s) = a.session(args)?;
        s.add_cutout(&id, ring)?;
        let m = s.markup(&id)?;
        Ok(json!({ "cutouts": m.holes.len(), "quantity": m.quantity_text(), "document": summary(doc, s) }))
    },
};

pub static CUTOUT_DELETE: Tool = Tool {
    name: "cutout_delete",
    title: "Remove a cutout",
    description: "Remove cutout `index` (1-based) from an Area or Volume measurement. Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "id": { "type": "string", "description": "The Area measurement." },
                "index": { "type": "integer", "minimum": 1, "description": "Which cutout (1 = the first)." }
            }),
            &["id", "index"],
        )
    },
    run: |a, args| {
        let id = args.str("id")?.to_string();
        let index = args.position("index")?;
        let (doc, s) = a.session(args)?;
        s.remove_cutout(&id, index)?;
        let m = s.markup(&id)?;
        Ok(json!({ "cutouts": m.holes.len(), "quantity": m.quantity_text(), "document": summary(doc, s) }))
    },
};

pub static CELL_SET: Tool = Tool {
    name: "list_cell_set",
    title: "Set a Markups List cell",
    description: "Change one Markups List cell of a markup from text, as typing in the list does: Subject, Label, Comments, Status (None, Accepted, Rejected, Cancelled, Completed), Checkmark and Lock (true/false), or a custom column (`c:<id>`). Fails when the column is read-only or the value does not fit it. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "id": { "type": "string", "description": "The markup." },
                "column": { "type": "string", "description": "Column id: subject, label, comments, status, checkmark, lock, or c:<custom id>." },
                "value": { "type": "string", "description": "The cell text (\"\" clears it)." }
            }),
            &["id", "column", "value"],
        )
    },
    run: |a, args| {
        let id = args.str("id")?.to_string();
        let column = args.str("column")?.to_string();
        let value = args.str("value")?.to_string();
        let (doc, s) = a.session(args)?;
        if !s.set_cell(&id, &column, &value)? {
            return Err(bad_args(format!(
                "column {column:?} is read-only or does not take {value:?}"
            )));
        }
        Ok(json!({ "document": summary(doc, s) }))
    },
};

fn column_of(v: &Value, existing: &[CustomColumn]) -> Result<CustomColumn> {
    let name = v["name"]
        .as_str()
        .filter(|n| !n.trim().is_empty())
        .ok_or_else(|| bad_args("every column needs a name"))?;
    let kind = match v["type"].as_str() {
        Some(t) => ColumnType::from_name(t).ok_or_else(|| {
            bad_args(format!(
                "column type {t:?} is not one of Text, Number, Currency, Percent, Date, Choice, Formula, Checkmark"
            ))
        })?,
        None => ColumnType::Text,
    };
    let id = match v["id"].as_str() {
        Some(id) => id.to_string(),
        None => make_column_id(name, existing),
    };
    let mut c = CustomColumn {
        id,
        name: name.to_string(),
        kind,
        ..Default::default()
    };
    if let Some(d) = v["decimals"].as_i64() {
        c.decimals = d.clamp(0, 6) as i32;
    }
    if let Some(t) = v["total"].as_bool() {
        c.total = t;
    }
    if let Some(f) = v["formula"].as_str() {
        c.formula = f.to_string();
    }
    // how a Formula column shows its number: Number, Currency or Percent
    if let Some(d) = v["display"].as_str() {
        c.display = match ColumnType::from_name(d) {
            Some(t @ (ColumnType::Number | ColumnType::Currency | ColumnType::Percent)) => t,
            _ => {
                return Err(bad_args(format!(
                    "display {d:?} is not one of Number, Currency, Percent"
                )));
            }
        };
    }
    if let Some(sym) = v["symbol"].as_str() {
        c.symbol = sym.to_string();
    }
    if let Some(ac) = v["allow_custom"].as_bool() {
        c.allow_custom = ac;
    }
    if let Some(items) = v["items"].as_array() {
        // A choice is its text, or {item, subject, value} (the item offered for that subject,
        // with the number formulas use).
        let mut out = Vec::with_capacity(items.len());
        for it in items {
            let item = match it {
                Value::String(t) => ChoiceItem {
                    text: t.to_string(),
                    ..Default::default()
                },
                Value::Object(_) => ChoiceItem {
                    text: it["item"]
                        .as_str()
                        .or_else(|| it["text"].as_str())
                        .ok_or_else(|| bad_args("a choice item needs its item text"))?
                        .to_string(),
                    subject: it["subject"].as_str().unwrap_or_default().to_string(),
                    value: it["value"].as_f64().filter(|x| x.is_finite()),
                },
                _ => return Err(bad_args("choice items are text or {item, subject, value}")),
            };
            out.push(item);
        }
        c.items = out;
    }
    if let Some(d) = v["default"].as_str() {
        c.default_value = d.to_string();
    }
    if let Some(f) = v["date_format"].as_str() {
        if !markupcraft_model::columns::DATE_FORMATS.contains(&f) {
            return Err(bad_args(format!(
                "date_format {f:?} is not one of {:?}",
                markupcraft_model::columns::DATE_FORMATS
            )));
        }
        c.date_format = f.to_string();
    }
    Ok(c)
}

pub static COLUMNS_SET: Tool = Tool {
    name: "columns_set",
    title: "Set the custom columns",
    description: "Replace the document's Markups List custom columns (the Manage Columns dialog). Each column: name, type (Text, Number, Currency, Percent, Date, Choice, Formula, Checkmark), and optionally id, decimals, total, formula (e.g. \"Measurement * [Unit Cost]\"; a column name with spaces goes in brackets), display (a Formula shown as Number, Currency or Percent), symbol, items (choices: text, or {item, subject, value}), allow_custom, default (the value new markups get), date_format. Values already on markups are kept. Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "columns": {
                    "type": "array",
                    "items": { "type": "object" },
                    "description": "The custom columns, in order."
                }
            }),
            &["columns"],
        )
    },
    run: |a, args| {
        let list = args
            .get("columns")
            .and_then(Value::as_array)
            .ok_or_else(|| bad_args("columns must be a list of column objects"))?;
        if list.len() > MAX_COLUMNS {
            return Err(bad_args(format!("at most {MAX_COLUMNS} custom columns")));
        }
        let mut cols: Vec<CustomColumn> = Vec::with_capacity(list.len());
        for v in list {
            let c = column_of(v, &cols)?;
            cols.push(c);
        }
        let ids: Vec<String> = cols.iter().map(|c| format!("c:{}", c.id)).collect();
        let (doc, s) = a.session(args)?;
        s.set_custom_columns(cols)?;
        Ok(json!({ "columns": ids, "document": summary(doc, s) }))
    },
};
