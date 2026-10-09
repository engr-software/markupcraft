//! Forms: list, fill, add fields, reset, flatten.

use markupcraft_engine::forms::{FillValue, NewFieldKind};
use serde_json::{Value, json};

use super::{Tool, page_arg, pages_arg, rect_arg, schema};
use crate::{Result, bad_args, summary};

fn strings(v: Option<&Value>, key: &str) -> Result<Vec<String>> {
    match v {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| {
                x.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| bad_args(format!("{key} must be a list of strings")))
            })
            .collect(),
        _ => Err(bad_args(format!("{key} must be a list of strings"))),
    }
}

pub static LIST: Tool = Tool {
    name: "form_list",
    title: "List form fields",
    description: "Every form field: name, kind (text, checkbox, radio, combo, list, button, signature), value, options (choices, or a check box / radio group's on-states), page, rectangle, read-only and required.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session_ref(args)?;
        let fields: Vec<Value> = s
            .form_fields()
            .iter()
            .map(|f| {
                json!({
                    "name": f.name,
                    "kind": f.kind,
                    "value": f.value,
                    "options": f.options,
                    "page": f.page.map(|p| p + 1),
                    "rect": f.rect.map(|r| r.as_array()),
                    "read_only": f.read_only,
                    "required": f.required,
                })
            })
            .collect();
        Ok(json!({ "doc": doc, "count": fields.len(), "fields": fields }))
    },
};

pub static FILL: Tool = Tool {
    name: "form_fill",
    title: "Fill form fields",
    description: "Fill fields by name: {\"values\": {\"Name\": \"text\", \"Check Box1\": true, \"Group1\": \"Choice2\", \"List Box1\": [\"A\", \"C\"]}}. Text fields take a string, check boxes true/false, radio groups the button's export value (false = none), combo and list boxes an export value or a list. Appearances are regenerated. Undoable as one step.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "values": { "type": "object", "description": "{field name: value}" } }),
            &["values"],
        )
    },
    run: |a, args| {
        let obj = args
            .get("values")
            .and_then(Value::as_object)
            .ok_or_else(|| bad_args("values must be an object {field name: value}"))?;
        let mut values = Vec::with_capacity(obj.len());
        for (k, v) in obj {
            let fv = match v {
                Value::String(s) => FillValue::Text(s.clone()),
                Value::Bool(b) => FillValue::Bool(*b),
                Value::Number(n) => FillValue::Text(n.to_string()),
                Value::Array(_) => FillValue::Choice(strings(Some(v), k)?),
                _ => {
                    return Err(bad_args(format!(
                        "{k}: a value is a string, true/false, or a list of strings"
                    )));
                }
            };
            values.push((k.clone(), fv));
        }
        let (doc, s) = a.session(args)?;
        let n = s.form_fill(&values)?;
        Ok(json!({ "filled": n, "document": summary(doc, s) }))
    },
};

pub static ADD: Tool = Tool {
    name: "form_add_field",
    title: "Add a form field",
    description: "Create a form field on a page: type text (multiline), checkbox, radio (group + export value; buttons of one group share the group name), combo (options, editable), list (options, multi) button (caption) or signature (an empty field for signature_sign). Names default to Acrobat's (Text1, Check Box1, Group1, Dropdown1, List Box1, Button1, Signature1). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "page": page_arg("to put it on"),
                "rect": rect_arg("Its box"),
                "type": { "type": "string", "enum": ["text", "checkbox", "radio", "combo", "list", "button", "signature"] },
                "name": { "type": "string", "description": "Field name (radio: ignored, see group)." },
                "multiline": { "type": "boolean" },
                "group": { "type": "string", "description": "Radio: the group's name." },
                "export": { "type": "string", "description": "Radio: this button's value." },
                "options": { "type": "array", "items": { "type": "string" }, "description": "Combo / list choices." },
                "editable": { "type": "boolean", "description": "Combo: allow typing a value." },
                "multi": { "type": "boolean", "description": "List: allow several selections." },
                "caption": { "type": "string", "description": "Button caption." }
            }),
            &["page", "rect", "type"],
        )
    },
    run: |a, args| {
        let page = args.page("page")?;
        let rect = args.opt_rect("rect")?.ok_or_else(|| bad_args("rect is required"))?;
        let kind = match args.str("type")? {
            "text" => NewFieldKind::Text {
                multiline: args.bool_or("multiline", false)?,
            },
            "checkbox" => NewFieldKind::CheckBox,
            "radio" => NewFieldKind::Radio {
                group: args.opt_string("group")?,
                export: args
                    .opt_string("export")?
                    .ok_or_else(|| bad_args("a radio button needs export (its value)"))?,
            },
            "combo" => NewFieldKind::Combo {
                options: strings(args.get("options"), "options")?,
                editable: args.bool_or("editable", false)?,
            },
            "list" => NewFieldKind::List {
                options: strings(args.get("options"), "options")?,
                multi: args.bool_or("multi", false)?,
            },
            "button" => NewFieldKind::Button {
                caption: args.opt_string("caption")?.unwrap_or_default(),
            },
            "signature" => NewFieldKind::Signature,
            other => return Err(bad_args(format!("unknown field type {other:?}"))),
        };
        let name = args.opt_string("name")?;
        let name = if matches!(kind, NewFieldKind::Radio { .. }) {
            None
        } else {
            name
        };
        let (doc, s) = a.session(args)?;
        let n = s.form_add_field(page, rect, &kind, name.as_deref())?;
        Ok(json!({ "name": n, "document": summary(doc, s) }))
    },
};

pub static RESET: Tool = Tool {
    name: "form_reset",
    title: "Reset form",
    description: "Clear fields (names, default all) back to their default values. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "names": { "type": "array", "items": { "type": "string" } } }),
            &[],
        )
    },
    run: |a, args| {
        let names = args.opt_strings("names")?;
        let (doc, s) = a.session(args)?;
        let n = s.form_reset(names.as_deref())?;
        Ok(json!({ "reset": n, "document": summary(doc, s) }))
    },
};

pub static FLATTEN: Tool = Tool {
    name: "form_flatten",
    title: "Flatten form fields",
    description: "Flatten form fields on pages (default all) into page content: they keep their look and stop being fields. The next save rewrites the file. Undoable until saved.",
    read_only: false,
    destructive: true,
    schema: || schema(json!({ "pages": pages_arg("to flatten (default: all)") }), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let pages = args.opt_pages("pages", s.page_count())?.unwrap_or_default();
        let n = s.form_flatten(&pages)?;
        Ok(json!({ "flattened": n, "document": summary(doc, s) }))
    },
};
