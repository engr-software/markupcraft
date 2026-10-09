//! Document tools: security presets, header/footer templates kept with the document, fitting
//! content inside margins, form data exchange and automatic fields, combining with options,
//! creating PDFs from other files and layered PDFs.

use std::path::PathBuf;

use markupcraft_engine::docs_more::{
    CombineOptions, HfTemplate, SecurityPreset, combine_with, create_pdf_from_files, layered_pdf, load_hf_templates,
    load_security_presets, merge_form_data, save_hf_templates, save_security_presets,
};
use markupcraft_engine::marks::HeaderFooter;
use markupcraft_engine::quantity::{
    QuantityLink, QuantityMeasure, QuantityValue, load_links, quantity_totals, save_links,
};
use serde_json::{Value, json};

use super::marks::{SLOTS, margins, margins_arg};
use super::{Tool, pages_arg, path_arg, schema, schema_nodoc};
use crate::{Args, Automation, Result, bad_args, summary};

fn files_list(a: &Automation, args: &Args) -> Result<Vec<PathBuf>> {
    let f = args.opt_strings("files")?.unwrap_or_default();
    if f.is_empty() {
        return Err(bad_args(format!("{}: give files", args.tool())));
    }
    f.iter().map(|p| a.resolve(p, false)).collect()
}

fn files_schema(extra: Value) -> Value {
    let mut p = json!({
        "files": { "type": "array", "items": { "type": "string" }, "description": "Input files in order." },
        "out": path_arg("The PDF to write")
    });
    if let (Some(o), Some(m)) = (p.as_object_mut(), extra.as_object()) {
        for (k, v) in m {
            o.insert(k.clone(), v.clone());
        }
    }
    p
}

fn name_arg(args: &Args) -> Result<String> {
    let n = args.str("name")?.trim().to_string();
    if n.is_empty() || n.chars().count() > 100 {
        return Err(bad_args("name has 1 to 100 characters"));
    }
    Ok(n)
}

pub static SECURITY_PRESET: Tool = Tool {
    name: "security_preset",
    title: "Security presets",
    description: "Saved security policies in a presets file (`path`, JSON): action list; save (name + open_password, permissions_password, encryption, and the permissions print, print_high_quality, modify, copy, annotate, fill_forms, accessibility, assemble, all allowed unless false); apply (name: set this document's security as the preset says, applied by the next save); delete (name).",
    read_only: false,
    destructive: false,
    schema: || {
        let b = || json!({ "type": "boolean" });
        schema(
            json!({
                "action": { "type": "string", "enum": ["list", "save", "apply", "delete"] },
                "path": path_arg("The presets file"),
                "name": { "type": "string" },
                "open_password": { "type": "string" },
                "permissions_password": { "type": "string" },
                "encryption": { "type": "string", "enum": ["aes256", "aes128", "rc4"] },
                "print": b(), "print_high_quality": b(), "modify": b(), "copy": b(),
                "annotate": b(), "fill_forms": b(), "accessibility": b(), "assemble": b()
            }),
            &["action", "path"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, true)?;
        let mut list = if path.exists() {
            load_security_presets(&path)?
        } else {
            Vec::new()
        };
        let names = |l: &[SecurityPreset]| l.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
        match args.str("action")? {
            "list" => Ok(json!({ "presets": list.iter().map(|p| json!({
                "name": p.name, "open_password": !p.open_password.is_empty(),
                "permissions_password": !p.permissions_password.is_empty(), "encryption": p.encryption,
                "print": p.print, "modify": p.modify, "copy": p.copy, "annotate": p.annotate,
                "fill_forms": p.fill_forms, "assemble": p.assemble,
            })).collect::<Vec<_>>() })),
            "save" => {
                let name = name_arg(args)?;
                let encryption = args.opt_string("encryption")?.unwrap_or_else(|| "aes256".into());
                if !["aes256", "aes128", "rc4"].contains(&encryption.as_str()) {
                    return Err(bad_args("encryption is aes256, aes128 or rc4"));
                }
                let p = SecurityPreset {
                    name: name.clone(),
                    open_password: args.opt_string("open_password")?.unwrap_or_default(),
                    permissions_password: args.opt_string("permissions_password")?.unwrap_or_default(),
                    print: args.bool_or("print", true)?,
                    print_high_quality: args.bool_or("print_high_quality", true)?,
                    modify: args.bool_or("modify", true)?,
                    copy: args.bool_or("copy", true)?,
                    annotate: args.bool_or("annotate", true)?,
                    fill_forms: args.bool_or("fill_forms", true)?,
                    accessibility: args.bool_or("accessibility", true)?,
                    assemble: args.bool_or("assemble", true)?,
                    encryption,
                };
                list.retain(|x| x.name != name);
                list.push(p);
                save_security_presets(&path, &list)?;
                Ok(json!({ "presets": names(&list) }))
            }
            "apply" => {
                let name = name_arg(args)?;
                let p = list
                    .iter()
                    .find(|x| x.name == name)
                    .ok_or_else(|| bad_args(format!("no preset named {name:?}")))?
                    .settings();
                let (doc, s) = a.session(args)?;
                s.set_security(&p)?;
                Ok(json!({ "applied": name, "status": s.security_status().label(), "document": summary(doc, s) }))
            }
            "delete" => {
                let name = name_arg(args)?;
                let before = list.len();
                list.retain(|x| x.name != name);
                if list.len() == before {
                    return Err(bad_args(format!("no preset named {name:?}")));
                }
                save_security_presets(&path, &list)?;
                Ok(json!({ "presets": names(&list) }))
            }
            other => Err(bad_args(format!(
                "action is list, save, apply or delete, not {other:?}"
            ))),
        }
    },
};

fn hf_props() -> Value {
    let mut p = json!({
        "font_size": { "type": "number", "minimum": 1, "maximum": 144 },
        "color": { "type": ["string", "array"] },
        "underline": { "type": "boolean" },
        "margins": margins_arg(),
        "start_number": { "type": "integer", "minimum": 1 },
        "fit_content": { "type": "boolean", "description": "Shrink the page content so the header and footer do not overlap it." }
    });
    if let Some(o) = p.as_object_mut() {
        for s in SLOTS {
            o.insert(s.into(), json!({ "type": "string" }));
        }
    }
    p
}

fn hf_from(args: &Args, name: &str) -> Result<HfTemplate> {
    let mut hf = HeaderFooter::default();
    for (i, s) in SLOTS.iter().enumerate() {
        if let (Some(t), Some(slot)) = (args.opt_string(s)?, hf.text.get_mut(i)) {
            *slot = t;
        }
    }
    if let Some(f) = args.opt_num("font_size")? {
        if !(1.0..=144.0).contains(&f) {
            return Err(bad_args("font_size is 1 to 144"));
        }
        hf.font_size = f;
    }
    if let Some(c) = args.opt_color("color")? {
        hf.color = c;
    }
    hf.underline = args.bool_or("underline", false)?;
    hf.margins = margins(args, hf.margins)?;
    if let Some(n) = args.opt_u64("start_number")? {
        hf.start_number = u32::try_from(n).map_err(|_| bad_args("start_number is too large"))?;
    }
    let mut t = HfTemplate::from_settings(name, &hf);
    t.fit_content = args.bool_or("fit_content", false)?;
    Ok(t)
}

fn hf_json(t: &HfTemplate) -> Value {
    let mut v = json!({
        "name": t.name, "font_size": t.font_size, "color": t.color.map(|c| c.hex()), "underline": t.underline,
        "margins": t.margins, "start_number": t.start_number, "fit_content": t.fit_content,
        "pages": t.pages.iter().map(|p| p + 1).collect::<Vec<_>>(),
    });
    if let Some(o) = v.as_object_mut() {
        for (s, text) in SLOTS.iter().zip(t.text.iter()) {
            o.insert((*s).into(), json!(text));
        }
    }
    v
}

pub static HF_TEMPLATE: Tool = Tool {
    name: "header_footer_template",
    title: "Header & Footer templates",
    description: "Saved header/footer templates in a file (`path`, JSON): action list; save (name + the six slot texts, font_size, color, underline, margins, start_number, fit_content); apply (name, pages default all: adds it, replacing earlier headers/footers, and keeps its settings with the document so header_footer_kept can edit or update it); delete (name).",
    read_only: false,
    destructive: false,
    schema: || {
        let mut p = hf_props();
        if let Some(o) = p.as_object_mut() {
            o.insert(
                "action".into(),
                json!({ "type": "string", "enum": ["list", "save", "apply", "delete"] }),
            );
            o.insert("path".into(), path_arg("The templates file"));
            o.insert("name".into(), json!({ "type": "string" }));
            o.insert("pages".into(), pages_arg("to apply it to (default: all)"));
        }
        schema(p, &["action", "path"])
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, true)?;
        let mut list = if path.exists() {
            load_hf_templates(&path)?
        } else {
            Vec::new()
        };
        match args.str("action")? {
            "list" => Ok(json!({ "templates": list.iter().map(hf_json).collect::<Vec<_>>() })),
            "save" => {
                let name = name_arg(args)?;
                let t = hf_from(args, &name)?;
                list.retain(|x| x.name != name);
                list.push(t);
                save_hf_templates(&path, &list)?;
                Ok(json!({ "templates": list.iter().map(|t| t.name.clone()).collect::<Vec<_>>() }))
            }
            "apply" => {
                let name = name_arg(args)?;
                let t = list
                    .iter()
                    .find(|x| x.name == name)
                    .cloned()
                    .ok_or_else(|| bad_args(format!("no template named {name:?}")))?;
                let (doc, s) = a.session(args)?;
                let pages = args.opt_pages("pages", s.page_count())?.unwrap_or_default();
                s.apply_header_footer_kept(&pages, &t)?;
                Ok(json!({ "applied": name, "document": summary(doc, s) }))
            }
            "delete" => {
                let name = name_arg(args)?;
                let before = list.len();
                list.retain(|x| x.name != name);
                if list.len() == before {
                    return Err(bad_args(format!("no template named {name:?}")));
                }
                save_hf_templates(&path, &list)?;
                Ok(json!({ "templates": list.iter().map(|t| t.name.clone()).collect::<Vec<_>>() }))
            }
            other => Err(bad_args(format!(
                "action is list, save, apply or delete, not {other:?}"
            ))),
        }
    },
};

pub static HF_KEPT: Tool = Tool {
    name: "header_footer_kept",
    title: "Edit or update header & footer",
    description: "The header/footer kept with the document: action get (its settings); set (apply new settings: the six slot texts, font_size, color, underline, margins, start_number, fit_content, pages; replacing the old ones and keeping them); update (re-apply the kept settings so page numbers follow the pages as they are now). Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        let mut p = hf_props();
        if let Some(o) = p.as_object_mut() {
            o.insert(
                "action".into(),
                json!({ "type": "string", "enum": ["get", "set", "update"] }),
            );
            o.insert("pages".into(), pages_arg("for set (default: all)"));
        }
        schema(p, &["action"])
    },
    run: |a, args| {
        let action = args.str("action")?.to_string();
        let (doc, s) = a.session(args)?;
        match action.as_str() {
            "get" => Ok(json!({ "kept": s.kept_header_footer().as_ref().map(hf_json) })),
            "set" => {
                let t = hf_from(args, "kept")?;
                let pages = args.opt_pages("pages", s.page_count())?.unwrap_or_default();
                s.apply_header_footer_kept(&pages, &t)?;
                Ok(json!({ "kept": s.kept_header_footer().as_ref().map(hf_json), "document": summary(doc, s) }))
            }
            "update" => {
                s.update_header_footer()?;
                Ok(json!({ "kept": s.kept_header_footer().as_ref().map(hf_json), "document": summary(doc, s) }))
            }
            other => Err(bad_args(format!("action is get, set or update, not {other:?}"))),
        }
    },
};

pub static FIT_CONTENT: Tool = Tool {
    name: "page_fit_content",
    title: "Fit content in margins",
    description: "Shrink the content of `pages` (default all) to fit inside `margins` [top, bottom, left, right] points, centred; an earlier fit is replaced. Undoable.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({ "pages": pages_arg("to fit (default: all)"), "margins": margins_arg() }),
            &["margins"],
        )
    },
    run: |a, args| {
        let m = margins(args, [0.0; 4])?;
        let (doc, s) = a.session(args)?;
        let pages = args.opt_pages("pages", s.page_count())?.unwrap_or_default();
        s.fit_content_in_margins(&pages, m)?;
        Ok(json!({ "pages": if pages.is_empty() { s.page_count() } else { pages.len() }, "document": summary(doc, s) }))
    },
};

pub static FORM_DATA_EXPORT: Tool = Tool {
    name: "form_data_export",
    title: "Export form data",
    description: "Write the form's field values to `out`: .xfdf (XFDF), .csv (one header row and one value row) or .json.",
    read_only: true,
    destructive: false,
    schema: || {
        schema(
            json!({ "out": path_arg("The data file (.xfdf, .csv or .json)") }),
            &["out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.str("out")?, true)?;
        let (doc, s) = a.session_ref(args)?;
        let n = s.export_form_data(&out)?;
        Ok(json!({ "doc": doc, "fields": n, "path": out.display().to_string() }))
    },
};

pub static FORM_DATA_IMPORT: Tool = Tool {
    name: "form_data_import",
    title: "Import form data",
    description: "Fill the form from a data file (`path`: .xfdf, .csv or .json, as form_data_export writes). Fields not in the form are ignored. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "path": path_arg("The data file") }), &["path"]),
    run: |a, args| {
        let path = a.resolve(args.str("path")?, false)?;
        let (doc, s) = a.session(args)?;
        let n = s.import_form_data(&path)?;
        Ok(json!({ "filled": n, "values": s.form_values(), "document": summary(doc, s) }))
    },
};

pub static FORM_DATA_MERGE: Tool = Tool {
    name: "form_data_merge",
    title: "Merge form data",
    description: "Merge the form data of many filled PDFs (`files`) into one CSV at `out`: a File column then one column per field name, one row per file.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "files": { "type": "array", "items": { "type": "string" } },
                "out": path_arg("The CSV")
            }),
            &["files", "out"],
        )
    },
    run: |a, args| {
        let files = files_list(a, args)?;
        let out = a.resolve(args.str("out")?, true)?;
        let rows = merge_form_data(&files, &out)?;
        Ok(json!({ "rows": rows, "path": out.display().to_string() }))
    },
};

pub static FORM_TYPEWRITER: Tool = Tool {
    name: "form_typewriter_to_fields",
    title: "Migrate Typewriter text to fields",
    description: "Each Typewriter markup sitting on a text field fills that field and is removed. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({}), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let n = s.typewriter_to_fields()?;
        Ok(json!({ "moved": n, "values": s.form_values(), "document": summary(doc, s) }))
    },
};

pub static FORM_AUTO_FIELDS: Tool = Tool {
    name: "form_auto_fields",
    title: "Automatically create form fields",
    description: "Find blanks on `pages` (default all): a text field over each run of underscores and each wide empty box, a check box on each small square, named after the word to their left. Undoable.",
    read_only: false,
    destructive: false,
    schema: || schema(json!({ "pages": pages_arg("to search (default: all)") }), &[]),
    run: |a, args| {
        let (doc, s) = a.session(args)?;
        let pages = args.opt_pages("pages", s.page_count())?.unwrap_or_default();
        let made = s.auto_create_fields(&pages)?;
        Ok(json!({ "fields": made, "document": summary(doc, s) }))
    },
};

pub static COMBINE_FILES: Tool = Tool {
    name: "doc_combine_files",
    title: "Combine with options",
    description: "Combine PDFs (`files`, in order) into `out` with options: bookmarks (one per file), attachments (carry every file's), properties (merge document properties), layers (keep every file's), labels_from_names (page labels from the file names). Signatures do not survive (reported in warnings).",
    read_only: false,
    destructive: true,
    schema: || {
        let b = || json!({ "type": "boolean" });
        schema_nodoc(
            files_schema(json!({
                "bookmarks": b(), "attachments": b(), "properties": b(), "layers": b(), "labels_from_names": b()
            })),
            &["files", "out"],
        )
    },
    run: |a, args| {
        let files = files_list(a, args)?;
        let out = a.resolve(args.str("out")?, true)?;
        let o = CombineOptions {
            bookmarks: args.bool_or("bookmarks", true)?,
            attachments: args.bool_or("attachments", false)?,
            properties: args.bool_or("properties", false)?,
            layers: args.bool_or("layers", false)?,
            labels_from_names: args.bool_or("labels_from_names", false)?,
        };
        let (pages, warnings) = combine_with(&files, &out, &o)?;
        Ok(json!({ "pages": pages, "warnings": warnings, "path": out.display().to_string() }))
    },
};

pub static CREATE_FROM_FILES: Tool = Tool {
    name: "doc_create_from_files",
    title: "Create PDF from files",
    description: "Create one PDF at `out` from `files` in order (the Stapler): images (PNG, JPEG, TIFF, BMP) become a page each at their size, text files are set in pages, PDFs are appended.",
    read_only: false,
    destructive: true,
    schema: || schema_nodoc(files_schema(json!({})), &["files", "out"]),
    run: |a, args| {
        let files = files_list(a, args)?;
        let out = a.resolve(args.str("out")?, true)?;
        let pages = create_pdf_from_files(&files, &out)?;
        Ok(json!({ "pages": pages, "path": out.display().to_string() }))
    },
};

pub static LAYERED_PDF: Tool = Tool {
    name: "doc_layered",
    title: "Create layered PDF",
    description: "Create a layered PDF at `out`: the first page of each of `files` drawn on one page, each in its own layer named after its file.",
    read_only: false,
    destructive: true,
    schema: || schema_nodoc(files_schema(json!({})), &["files", "out"]),
    run: |a, args| {
        let files = files_list(a, args)?;
        let out = a.resolve(args.str("out")?, true)?;
        let layers = layered_pdf(&files, &out)?;
        Ok(json!({ "layers": layers, "path": out.display().to_string() }))
    },
};

fn link_json(l: &QuantityLink, v: Option<&QuantityValue>) -> Value {
    json!({
        "name": l.name, "sheet": l.sheet, "cell": l.cell, "measure": l.measure.name(),
        "files": l.files.iter().map(|f| f.display().to_string()).collect::<Vec<_>>(),
        "subjects": l.subjects, "layers": l.layers, "labels": l.labels, "authors": l.authors,
        "colors": l.colors, "pages": l.pages,
        "value": v.map(|v| v.value), "units": v.map(|v| v.units.clone()), "markups": v.map(|v| v.markups),
        "errors": v.map(|v| v.errors.clone()),
    })
}

pub static QUANTITY_LINK: Tool = Tool {
    name: "quantity_link",
    title: "Quantity Link",
    description: "Quantity Links kept in a links file (`path`, JSON): each links a workbook cell (`sheet`, `cell` like B4) to a total over `files`: measure count, length, area, volume, quantity or column:<custom column id>, filtered by subjects, layers, labels, authors, colors (#rrggbb) and pages (page labels). action list (with current totals); save (name + the rest); delete (name); update (recompute every link and write the workbook `out` .xlsx: each total at its sheet and cell, plus a Links sheet).",
    read_only: false,
    destructive: true,
    schema: || {
        let strs = || json!({ "type": "array", "items": { "type": "string" } });
        schema_nodoc(
            json!({
                "action": { "type": "string", "enum": ["list", "save", "delete", "update"] },
                "path": path_arg("The links file"),
                "name": { "type": "string" },
                "sheet": { "type": "string" },
                "cell": { "type": "string" },
                "measure": { "type": "string", "description": "count, length, area, volume, quantity or column:<id>" },
                "files": strs(), "subjects": strs(), "layers": strs(), "labels": strs(),
                "authors": strs(), "colors": strs(), "pages": strs(),
                "out": path_arg("For update: the workbook (.xlsx)")
            }),
            &["action", "path"],
        )
    },
    run: |a, args| {
        let path = a.resolve(args.str("path")?, true)?;
        let mut list = if path.exists() { load_links(&path)? } else { Vec::new() };
        match args.str("action")? {
            "list" => {
                let values = if list.is_empty() {
                    Vec::new()
                } else {
                    quantity_totals(&list)?
                };
                Ok(
                    json!({ "links": list.iter().enumerate().map(|(i, l)| link_json(l, values.get(i))).collect::<Vec<_>>() }),
                )
            }
            "save" => {
                let name = name_arg(args)?;
                let measure_name = args.opt_str("measure")?.unwrap_or("count");
                let measure = QuantityMeasure::from_name(measure_name)
                    .ok_or_else(|| bad_args(format!("unknown measure {measure_name:?}")))?;
                let s = |k: &str| -> Result<Vec<String>> { Ok(args.opt_strings(k)?.unwrap_or_default()) };
                let l = QuantityLink {
                    name: name.clone(),
                    sheet: args.opt_string("sheet")?.unwrap_or_else(|| "Quantities".into()),
                    cell: args.str("cell")?.to_string(),
                    files: files_list(a, args)?,
                    measure,
                    subjects: s("subjects")?,
                    layers: s("layers")?,
                    labels: s("labels")?,
                    authors: s("authors")?,
                    colors: s("colors")?,
                    pages: s("pages")?,
                };
                list.retain(|x| x.name != name);
                list.push(l);
                save_links(&path, &list)?;
                Ok(json!({ "links": list.iter().map(|l| l.name.clone()).collect::<Vec<_>>() }))
            }
            "delete" => {
                let name = name_arg(args)?;
                let before = list.len();
                list.retain(|x| x.name != name);
                if list.len() == before {
                    return Err(bad_args(format!("no Quantity Link named {name:?}")));
                }
                save_links(&path, &list)?;
                Ok(json!({ "links": list.iter().map(|l| l.name.clone()).collect::<Vec<_>>() }))
            }
            "update" => {
                if list.is_empty() {
                    return Err(bad_args("the links file has no links (save some first)"));
                }
                let out = a.resolve(args.str("out")?, true)?;
                let values = markupcraft_engine::finish::xlsx_edit::update_quantity_workbook_in_place(&list, &out)?;
                Ok(json!({
                    "path": out.display().to_string(),
                    "links": list.iter().zip(&values).map(|(l, v)| link_json(l, Some(v))).collect::<Vec<_>>(),
                }))
            }
            other => Err(bad_args(format!(
                "action is list, save, delete or update, not {other:?}"
            ))),
        }
    },
};
