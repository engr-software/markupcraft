//! Embedded file attachments.

use serde_json::{Value, json};

use super::{Tool, path_arg, schema};
use crate::summary;

pub static LIST: Tool = Tool {
    name: "attachment_list",
    title: "List attachments",
    description: "Files embedded in the document: name, file name, description and size.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session_ref(args)?;
        let list: Vec<Value> = s
            .attachments()
            .iter()
            .map(|x| json!({ "name": x.name, "file": x.file, "description": x.description, "size": x.size }))
            .collect();
        Ok(json!({ "doc": doc, "attachments": list }))
    },
};

pub static ADD: Tool = Tool {
    name: "attachment_add",
    title: "Attach a file",
    description: "Embed the file at `path` in the document under `name` (default: its file name; an attachment of that name is replaced). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "path": path_arg("The file to attach"), "name": { "type": "string" }, "description": { "type": "string" } }),
            &["path"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let name = args.opt_string("name")?;
        let desc = args.opt_string("description")?.unwrap_or_default();
        let (doc, s) = a.session(args)?;
        let name = s.add_attachment(&path, name.as_deref(), &desc)?;
        Ok(json!({ "name": name, "document": summary(doc, s) }))
    },
};

pub static EXTRACT: Tool = Tool {
    name: "attachment_extract",
    title: "Save an attachment",
    description: "Write attachment `name` to the file `out` (atomic).",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "name": { "type": "string" }, "out": path_arg("Where to write it") }),
            &["name", "out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let name = args.str("name")?;
        let (doc, s) = a.session_ref(args)?;
        let n = s.extract_attachment(name, &out)?;
        Ok(json!({ "doc": doc, "out": out.display().to_string(), "bytes": n }))
    },
};

pub static DELETE: Tool = Tool {
    name: "attachment_delete",
    title: "Remove an attachment",
    description: "Remove attachment `name` from the document. Undoable.",
    read_only: false,
    destructive: true,
    schema: || schema(json!({ "name": { "type": "string" } }), &["name"]),
    run: |a, args| {
        let name = args.str("name")?.to_string();
        let (doc, s) = a.session(args)?;
        s.delete_attachment(&name)?;
        Ok(json!({ "deleted": name, "document": summary(doc, s) }))
    },
};
