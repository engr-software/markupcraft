//! Security: open protected PDFs, set and remove passwords and permissions.

use markupcraft_engine::Session;
use markupcraft_engine::security::{Encryption, Permissions, SecuritySettings};
use serde_json::{Value, json};

use super::{Tool, path_arg, schema, schema_nodoc};
use crate::{bad_args, summary};

fn perms_json(p: &Permissions) -> Value {
    json!({
        "print": p.print,
        "print_high_quality": p.print_high_quality,
        "modify": p.modify,
        "copy": p.copy,
        "annotate": p.annotate,
        "fill_forms": p.fill_forms,
        "accessibility": p.accessibility,
        "assemble": p.assemble,
    })
}

fn info_json(s: &Session) -> Value {
    let i = s.security();
    json!({
        "encrypted": i.encrypted,
        "method": i.method,
        "owner": i.owner,
        "permissions": perms_json(&i.permissions),
        "pending_change": i.pending_change,
        "save_encrypted": i.save_encrypted,
        "status": s.security_status().label(),
    })
}

pub static OPEN: Tool = Tool {
    name: "doc_open_protected",
    title: "Open a protected PDF",
    description: "Open a password-protected PDF with its open password or its permissions password (the permissions password allows everything, including changing security).",
    read_only: true,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({ "path": path_arg("The PDF to open"), "password": { "type": "string" } }),
            &["path", "password"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let s = Session::open_with_password(&path, args.str("password")?)?;
        let id = a.add_doc(s)?;
        let (_, s) = a.session_ref(args)?;
        Ok(json!({ "document": summary(id, s), "security": info_json(s) }))
    },
};

pub static INFO: Tool = Tool {
    name: "security_info",
    title: "Security",
    description: "Whether the document is encrypted (and how), whether it was opened with the permissions password, what is permitted, and what the next save writes.",
    read_only: true,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session_ref(args)?;
        Ok(json!({ "doc": doc, "security": info_json(s) }))
    },
};

pub static SET: Tool = Tool {
    name: "security_set",
    title: "Set passwords and permissions",
    description: "Protect the document: an open password (needed to open it) and/or a permissions password (needed to change security), with permissions (all allowed unless set false): print, print_high_quality, modify, copy, annotate, fill_forms, accessibility, assemble. encryption: aes256 (default), aes128 or rc4. Applied by the next save (a full rewrite). Undoable until then.",
    read_only: false,
    destructive: false,
    schema: || {
        let b = || json!({ "type": "boolean" });
        schema(
            json!({
                "open_password": { "type": "string" },
                "permissions_password": { "type": "string" },
                "encryption": { "type": "string", "enum": ["aes256", "aes128", "rc4"] },
                "print": b(), "print_high_quality": b(), "modify": b(), "copy": b(),
                "annotate": b(), "fill_forms": b(), "accessibility": b(), "assemble": b()
            }),
            &[],
        )
    },
    run: |a, args| {
        let all = Permissions::ALL;
        let permissions = Permissions {
            print: args.bool_or("print", all.print)?,
            print_high_quality: args.bool_or("print_high_quality", all.print_high_quality)?,
            modify: args.bool_or("modify", all.modify)?,
            copy: args.bool_or("copy", all.copy)?,
            annotate: args.bool_or("annotate", all.annotate)?,
            fill_forms: args.bool_or("fill_forms", all.fill_forms)?,
            accessibility: args.bool_or("accessibility", all.accessibility)?,
            assemble: args.bool_or("assemble", all.assemble)?,
        };
        let encryption = match args.opt_str("encryption")? {
            None => Encryption::default(),
            Some(e) => Encryption::from_name(e).ok_or_else(|| bad_args(format!("unknown encryption {e:?}")))?,
        };
        let settings = SecuritySettings {
            open_password: args.opt_string("open_password")?.unwrap_or_default(),
            permissions_password: args.opt_string("permissions_password")?.unwrap_or_default(),
            permissions,
            encryption,
        };
        let (doc, s) = a.session(args)?;
        s.set_security(&settings)?;
        Ok(json!({ "encryption": encryption.name(), "security": info_json(s), "document": summary(doc, s) }))
    },
};

pub static REMOVE: Tool = Tool {
    name: "security_remove",
    title: "Remove security",
    description: "Remove password protection; the next save writes the file unencrypted. Needs the document opened with its permissions password (or unprotected). Undoable until saved.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        s.remove_security()?;
        Ok(json!({ "security": info_json(s), "document": summary(doc, s) }))
    },
};
