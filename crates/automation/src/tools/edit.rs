//! Undo, redo, and the engine's command table.

use markupcraft_engine::commands;
use serde_json::{Value, json};

use super::{Tool, schema};
use crate::{Result, summary};

pub static UNDO: Tool = Tool {
    name: "edit_undo",
    title: "Undo",
    description: "Undo the last change (markup edits, scales and page operations alike).",
    read_only: false,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let label = s.undo()?;
        Ok(json!({ "undone": label, "document": summary(doc, s) }))
    },
};

pub static REDO: Tool = Tool {
    name: "edit_redo",
    title: "Redo",
    description: "Redo the last undone change.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let label = s.redo()?;
        Ok(json!({ "redone": label, "document": summary(doc, s) }))
    },
};

pub static COMMAND_LIST: Tool = Tool {
    name: "command_list",
    title: "List commands",
    description: "The engine's command table (menu and shortcut commands that act on the selection), with ids for command_run.",
    read_only: true,
    destructive: false,
    schema: || super::schema_nodoc(json!({}), &[]),
    run: |_, _| -> Result<Value> {
        let list: Vec<Value> = commands::COMMANDS
            .iter()
            .map(|c| json!({ "id": c.id, "label": c.label, "needs_selection": c.needs_selection }))
            .collect();
        Ok(json!({ "commands": list }))
    },
};

pub static COMMAND_RUN: Tool = Tool {
    name: "command_run",
    title: "Run a command",
    description: "Run a command from command_list (e.g. edit.delete, arrange.bring_to_front, markup.group) on the document's selection.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "id": { "type": "string", "description": "Command id from command_list." } }),
            &["id"],
        )
    },
    run: |a, args| {
        let id = args.str("id")?;
        // The session works on the shared clipboard, so copy here and paste in another document.
        let shared = a.clipboard().to_vec();
        let (doc, s) = a.session(args)?;
        s.set_clipboard(shared);
        let report = commands::run(s, id)?;
        let clip = s.clipboard().to_vec();
        let out = json!({ "report": report, "document": summary(doc, s) });
        a.set_clipboard(clip);
        Ok(out)
    },
};
