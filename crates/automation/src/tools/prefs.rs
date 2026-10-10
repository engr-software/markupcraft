//! Preferences and profiles, stored in the config folder.

use markupcraft_engine::prefs::PrefStore;
use serde_json::{Value, json};

use super::{Tool, path_arg, schema_nodoc};
use crate::{Automation, Result, bad_args, failed};

fn store(a: &Automation) -> Result<PrefStore> {
    Ok(PrefStore::new(a.config_dir()?))
}

/// New markups in every open document (and documents opened later) take this author.
fn set_author(a: &mut Automation, author: &str) {
    a.author = author.to_string();
    for (_, s) in a.docs.iter_mut() {
        s.set_author(author);
    }
}

fn prefs_json(p: &markupcraft_engine::prefs::Preferences) -> Result<Value> {
    serde_json::to_value(p).map_err(failed)
}

fn profile_arg() -> Value {
    json!({ "type": "string", "description": "A profile name (default: the active profile)." })
}

pub static GET: Tool = Tool {
    name: "prefs_get",
    title: "Preferences",
    description: "The preferences of the active profile (or `profile`): author, units, precision, colors, snapping, autosave_minutes, save_mode, recent_files, theme.",
    read_only: true,
    destructive: false,
    schema: || schema_nodoc(json!({ "profile": profile_arg() }), &[]),
    run: |a, args| {
        let st = store(a)?;
        let name = args.opt_string("profile")?.unwrap_or_else(|| st.active());
        let p = st.load_profile(&name)?;
        Ok(json!({ "profile": name, "active": st.active(), "preferences": prefs_json(&p)? }))
    },
};

pub static SET: Tool = Tool {
    name: "prefs_set",
    title: "Change preferences",
    description: "Change preferences: `values` is a partial object merged into the profile, e.g. {\"author\": \"Estimator\", \"snapping\": {\"grid\": true}, \"autosave_minutes\": 5}. Values are checked; unknown keys are refused. Saved at once. When the author changes in the active profile, new markups use it.",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({ "values": { "type": "object" }, "profile": profile_arg() }),
            &["values"],
        )
    },
    run: |a, args| {
        let values = args
            .get("values")
            .cloned()
            .ok_or_else(|| bad_args("prefs_set: missing values"))?;
        let st = store(a)?;
        let name = args.opt_string("profile")?.unwrap_or_else(|| st.active());
        let p = st.load_profile(&name)?.merged(&values)?;
        st.save_profile(&name, &p)?;
        if name == st.active() && values.get("author").is_some() {
            set_author(a, &p.author);
        }
        Ok(json!({ "profile": name, "preferences": prefs_json(&p)? }))
    },
};

pub static PROFILES: Tool = Tool {
    name: "profile_list",
    title: "Profiles",
    description: "Every saved profile and the active one.",
    read_only: true,
    destructive: false,
    schema: || schema_nodoc(json!({}), &[]),
    run: |a, _args| {
        let st = store(a)?;
        Ok(json!({ "active": st.active(), "profiles": st.profiles() }))
    },
};

pub static SWITCH: Tool = Tool {
    name: "profile_switch",
    title: "Switch profile",
    description: "Make `profile` the active one. A new profile starts as a copy of the current preferences (`copy`, default true) or from the defaults. New markups take its author.",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({ "profile": { "type": "string" }, "copy": { "type": "boolean" } }),
            &["profile"],
        )
    },
    run: |a, args| {
        let name = args.str("profile")?.to_string();
        let copy = args.bool_or("copy", true)?;
        let st = store(a)?;
        let p = st.switch(&name, copy)?;
        set_author(a, &p.author);
        Ok(json!({ "active": st.active(), "preferences": prefs_json(&p)? }))
    },
};

pub static DELETE: Tool = Tool {
    name: "profile_delete",
    title: "Delete a profile",
    description: "Delete a saved profile (not the active one).",
    read_only: false,
    destructive: true,
    schema: || schema_nodoc(json!({ "profile": { "type": "string" } }), &["profile"]),
    run: |a, args| {
        let name = args.str("profile")?;
        let st = store(a)?;
        st.delete_profile(name)?;
        Ok(json!({ "profiles": st.profiles() }))
    },
};

pub static EXPORT: Tool = Tool {
    name: "prefs_export",
    title: "Export a profile",
    description: "Write a profile's preferences to a JSON file. `bundle: true` writes a profile file (.mcprofile) that also carries the profile's interface settings and keyboard shortcuts, and with `include_dependencies` the shared settings files (Tool Chest, presets, templates) so another computer gets the same tools.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "path": path_arg("The file to write"),
                "profile": profile_arg(),
                "bundle": { "type": "boolean", "description": "Write a profile bundle (default false: the preferences only)." },
                "include_dependencies": { "type": "boolean", "description": "Bundle: add the shared settings files (default false)." }
            }),
            &["path"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, true)?;
        let st = store(a)?;
        let name = args.opt_string("profile")?.unwrap_or_else(|| st.active());
        if args.bool_or("bundle", false)? {
            let n = st.export_bundle(&name, &path, args.bool_or("include_dependencies", false)?)?;
            return Ok(json!({ "profile": name, "path": path.display().to_string(), "dependencies": n }));
        }
        st.export(&name, &path)?;
        Ok(json!({ "profile": name, "path": path.display().to_string() }))
    },
};

pub static IMPORT: Tool = Tool {
    name: "prefs_import",
    title: "Import a profile",
    description: "Read a preferences JSON file into profile `profile` (created or replaced; default: the active profile). A profile bundle (prefs_export with bundle) is recognised and imported whole: preferences, interface settings, shortcuts and any shared settings files it carries; it keeps its own name unless `profile` is given.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({ "path": path_arg("The file to read"), "profile": profile_arg() }),
            &["path"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let st = store(a)?;
        let is_bundle = std::fs::read(&path)
            .ok()
            .filter(|b| b.len() < (64 << 20))
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .is_some_and(|v| v["format"] == markupcraft_engine::profiles::BUNDLE_FORMAT);
        if is_bundle {
            let got = st.import_bundle(&path, args.opt_string("profile")?.as_deref())?;
            let p = st.load_profile(&got.name)?;
            if got.name == st.active() {
                set_author(a, &p.author);
            }
            return Ok(
                json!({ "profile": got.name, "dependencies": got.dependencies, "preferences": prefs_json(&p)? }),
            );
        }
        let name = args.opt_string("profile")?.unwrap_or_else(|| st.active());
        let p = st.import(&path, &name)?;
        if name == st.active() {
            set_author(a, &p.author);
        }
        Ok(json!({ "profile": name, "preferences": prefs_json(&p)? }))
    },
};

pub static RENAME: Tool = Tool {
    name: "profile_rename",
    title: "Rename a profile",
    description: "Rename a profile (with its interface settings and shortcuts). The active profile stays active under its new name.",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({ "profile": { "type": "string" }, "to": { "type": "string" } }),
            &["profile", "to"],
        )
    },
    run: |a, args| {
        let st = store(a)?;
        st.rename_profile(args.str("profile")?, args.str("to")?)?;
        Ok(json!({ "active": st.active(), "profiles": st.profiles() }))
    },
};
