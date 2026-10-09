//! Document properties (information dictionary and XMP).

use serde_json::{Map, Value, json};

use super::{Tool, schema};
use crate::{bad_args, summary};

fn props_json(s: &markupcraft_engine::Session) -> Value {
    let p = s.doc_properties();
    let obj = |list: &[(String, String)]| {
        Value::Object(
            list.iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect::<Map<_, _>>(),
        )
    };
    json!({
        "standard": obj(&p.standard),
        "custom": obj(&p.custom),
        "pdf_version": p.pdf_version,
        "pages": p.pages,
        "encrypted": p.encrypted,
        "has_xmp": p.has_xmp,
        "file_size": p.file_size,
    })
}

pub static GET: Tool = Tool {
    name: "doc_properties",
    title: "Document properties",
    description: "Title, Author, Subject, Keywords, Creator, Producer, dates, custom properties, PDF version, encryption and XMP presence. xmp: true also returns the XMP packet.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({ "xmp": { "type": "boolean" } }), &[]),
    run: |a, args| {
        let want_xmp = args.bool_or("xmp", false)?;
        let (doc, s) = a.session_ref(args)?;
        let mut v = props_json(s);
        if want_xmp && let Some(o) = v.as_object_mut() {
            o.insert("xmp".into(), json!(s.xmp()));
        }
        Ok(json!({ "doc": doc, "properties": v }))
    },
};

pub static SET: Tool = Tool {
    name: "doc_properties_set",
    title: "Set document properties",
    description: "Set properties: {\"properties\": {\"Title\": \"...\", \"Project\": \"...\"}}. Standard keys (Title, Author, Subject, Keywords, Creator, Producer) or custom names; \"\" or null removes one. The XMP metadata is updated to match. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "properties": { "type": "object" } }), &["properties"]),
    run: |a, args| {
        let map = args
            .get("properties")
            .and_then(Value::as_object)
            .ok_or_else(|| bad_args("properties must be an object of name: text"))?;
        let mut changes = Vec::with_capacity(map.len());
        for (k, v) in map {
            let v = match v {
                Value::Null => None,
                Value::String(s) if s.is_empty() => None,
                Value::String(s) => Some(s.clone()),
                _ => return Err(bad_args(format!("the value of {k} must be text or null"))),
            };
            changes.push((k.clone(), v));
        }
        let (doc, s) = a.session(args)?;
        s.set_doc_properties(&changes)?;
        Ok(json!({ "properties": props_json(s), "document": summary(doc, s) }))
    },
};
