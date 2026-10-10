//! Layers: PDF optional content groups, and putting markups on them.

use markupcraft_engine::layers::LayerState;
use serde_json::{Value, json};

use super::{Tool, ids_arg, schema, target_ids};
use crate::{bad_args, summary};

fn name_arg(what: &str) -> Value {
    json!({ "type": "string", "description": what })
}

pub static LIST: Tool = Tool {
    name: "layer_list",
    title: "List layers",
    description: "Every layer (PDF optional content group): name, visible, print, locked, how many markups are on it, and whether the file lists it yet.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session_ref(args)?;
        let layers: Vec<Value> = s
            .layers()
            .iter()
            .map(|l| {
                json!({
                    "name": l.name, "visible": l.visible, "print": l.print, "print_set": l.print_set,
                    "locked": l.locked, "markups": l.markups, "in_file": l.in_file,
                })
            })
            .collect();
        Ok(json!({ "doc": doc, "layers": layers }))
    },
};

pub static CREATE: Tool = Tool {
    name: "layer_create",
    title: "New layer",
    description: "Add an empty layer (visible and printed). Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "name": name_arg("The layer's name.") }), &["name"]),
    run: |a, args| {
        let name = args.str("name")?;
        let (doc, s) = a.session(args)?;
        s.create_layer(name)?;
        Ok(json!({ "document": summary(doc, s) }))
    },
};

pub static RENAME: Tool = Tool {
    name: "layer_rename",
    title: "Rename a layer",
    description: "Rename a layer; its markups stay on it. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "name": name_arg("The layer to rename."), "new_name": name_arg("Its new name.") }),
            &["name", "new_name"],
        )
    },
    run: |a, args| {
        let (old, new) = (args.str("name")?, args.str("new_name")?);
        let (doc, s) = a.session(args)?;
        let n = s.rename_layer(old, new)?;
        Ok(json!({ "markups": n, "document": summary(doc, s) }))
    },
};

pub static DELETE: Tool = Tool {
    name: "layer_delete",
    title: "Delete a layer",
    description: "Delete a layer. Its markups are left on no layer, or deleted with it when delete_markups is true. Undoable.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "name": name_arg("The layer to delete."),
                "delete_markups": { "type": "boolean", "description": "Delete the layer's markups too (default false)." }
            }),
            &["name"],
        )
    },
    run: |a, args| {
        let name = args.str("name")?;
        let del = args.bool_or("delete_markups", false)?;
        let (doc, s) = a.session(args)?;
        let n = s.delete_layer(name, del)?;
        Ok(json!({ "markups": n, "markups_deleted": del, "document": summary(doc, s) }))
    },
};

pub static SET: Tool = Tool {
    name: "layer_set",
    title: "Layer states",
    description: "Change layer states: `visible`, `print`, `locked` for layer `name`; or `isolate` (show only `name`) or `show_all` (every layer visible). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "name": name_arg("The layer."),
                "visible": { "type": "boolean" },
                "print": { "type": "boolean" },
                "locked": { "type": "boolean" },
                "isolate": { "type": "boolean", "description": "Show only this layer." },
                "show_all": { "type": "boolean", "description": "Show every layer (no name needed)." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let name = args.opt_string("name")?;
        let st = LayerState {
            visible: args.opt_bool("visible")?,
            print: args.opt_bool("print")?,
            locked: args.opt_bool("locked")?,
        };
        let isolate = args.bool_or("isolate", false)?;
        let show_all = args.bool_or("show_all", false)?;
        let (doc, s) = a.session(args)?;
        if show_all {
            s.show_all_layers()?;
        }
        if isolate || st != LayerState::default() {
            let name = name
                .as_deref()
                .ok_or_else(|| bad_args("layer_set: give the layer's name"))?;
            if isolate {
                s.isolate_layer(name)?;
            }
            if st != LayerState::default() {
                s.set_layer_state(name, st)?;
            }
        } else if !show_all {
            return Err(bad_args("layer_set: give visible, print, locked, isolate or show_all"));
        }
        Ok(json!({ "document": summary(doc, s) }))
    },
};

pub static MARKUP_LAYER: Tool = Tool {
    name: "layer_markup_layer",
    title: "Markup Layer",
    description: "The Markup Layer: every new markup is drawn on it (saved as /OC). `name` sets it (the layer is created when new), \"\" clears it; without `name` it is only reported.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "name": name_arg("The layer new markups go on; \"\" = none.") }),
            &[],
        )
    },
    run: |a, args| {
        let name = args.opt_string("name")?;
        let (doc, s) = a.session(args)?;
        if let Some(n) = &name {
            s.set_markup_layer(Some(n.as_str()))?;
        }
        let now = s.markup_layer().map(String::from);
        Ok(json!({ "markup_layer": now, "document": summary(doc, s) }))
    },
};

pub static ASSIGN: Tool = Tool {
    name: "layer_assign",
    title: "Put markups on a layer",
    description: "Put markups on layer `layer` (created when new; \"\" = no layer). Saved as /OC on each annotation. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "ids": ids_arg(), "layer": name_arg("The layer; \"\" removes the markups from their layer.") }),
            &["layer"],
        )
    },
    run: |a, args| {
        let layer = args.str("layer")?.to_string();
        let (doc, s) = a.session(args)?;
        let ids = target_ids(s, args)?;
        let n = s.assign_layer(&ids, &layer)?;
        Ok(json!({ "changed": n, "document": summary(doc, s) }))
    },
};

pub static MARKUPS: Tool = Tool {
    name: "layer_markups",
    title: "Markups by layer",
    description: "Ids of the markups on any of `layers` (\"\" = on no layer; omit for every markup); `visible_only` leaves out markups on hidden layers. `select` makes them the selection.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "layers": { "type": "array", "items": { "type": "string" } },
                "visible_only": { "type": "boolean" },
                "select": { "type": "boolean" }
            }),
            &[],
        )
    },
    run: |a, args| {
        let layers = args.opt_strings("layers")?.unwrap_or_default();
        let visible = args.bool_or("visible_only", false)?;
        let select = args.bool_or("select", false)?;
        let (doc, s) = a.session(args)?;
        let ids = s.layer_markups(&layers, visible);
        if select {
            s.select(&ids)?;
        }
        Ok(json!({ "doc": doc, "ids": ids }))
    },
};
