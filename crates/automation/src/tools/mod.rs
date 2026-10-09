//! The tool table. To add a tool: the engine capability first (with its tests), then a
//! `pub static FOO: Tool` in `tools/<area>.rs`, one line in [`TOOLS`], and an end-to-end test in
//! `tests/automation.rs`.
//!
//! TODO(render): a `page_render` tool (page to PNG) once `markupcraft-render` exposes rendering.

mod attachments;
mod batch;
mod batch_compare;
mod bookmarks;
mod compare;
mod convert;
mod doc;
mod docfile;
mod docprops;
mod docs5b;
mod docs6a;
mod edit;
mod export;
mod files;
mod fill;
mod flatten;
mod forms;
mod layers;
mod layout;
mod legend;
mod links;
mod marks;
mod markup_ops;
mod markups;
mod ocr;
mod pages;
mod partials;
mod prefs;
mod redact;
mod scale;
mod search;
mod security;
mod sets;
mod sign;
mod spaces;
mod spell;
mod stamps;
mod summary;
mod takeoff;
mod viewports;
mod visual;
mod xfdf;

use markupcraft_engine::{Markup, Session};
use serde_json::{Value, json};

use crate::{Args, Automation, Result};

pub struct Tool {
    /// `[a-z_]` only
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Changes no document or file.
    pub read_only: bool,
    /// May remove content or overwrite files.
    pub destructive: bool,
    /// JSON Schema of the arguments object.
    pub schema: fn() -> Value,
    pub run: fn(&mut Automation, &Args) -> Result<Value>,
}

/// Every tool, in a stable order, one line each.
pub static TOOLS: &[&Tool] = &[
    &doc::OPEN,
    &doc::NEW,
    &doc::LIST,
    &doc::INFO,
    &doc::SAVE,
    &doc::CLOSE,
    &markups::LIST,
    &markups::ADD,
    &markups::EDIT,
    &markups::TRANSFORM,
    &markups::DELETE,
    &markups::DUPLICATE,
    &markups::ARRANGE,
    &layout::ALIGN,
    &markups::COPY,
    &markups::PASTE,
    &markups::GROUP,
    &markups::UNGROUP,
    &markups::SELECT,
    &scale::SET,
    &scale::CALIBRATE,
    &scale::VIEWPORT_ADD,
    &scale::VIEWPORT_DELETE,
    &viewports::VIEWPORT_EDIT,
    &scale::MEASURE,
    &pages::ROTATE,
    &pages::DELETE,
    &pages::MOVE,
    &pages::INSERT_BLANK,
    &pages::INSERT_FILE,
    &pages::EXTRACT,
    &pages::LABELS,
    &export::CSV,
    &edit::UNDO,
    &edit::REDO,
    &edit::COMMAND_LIST,
    &edit::COMMAND_RUN,
    &bookmarks::LIST,
    &bookmarks::ADD,
    &bookmarks::EDIT,
    &bookmarks::MOVE,
    &bookmarks::DELETE,
    &bookmarks::CREATE,
    &bookmarks::NUMBER,
    &bookmarks::SET,
    &bookmarks::CLEAR,
    &bookmarks::FROM_BOOKMARKS,
    &search::SEARCH,
    &search::PAGE_TEXT,
    &links::LIST,
    &links::ADD,
    &links::DELETE,
    &attachments::LIST,
    &attachments::ADD,
    &attachments::EXTRACT,
    &attachments::DELETE,
    &docprops::GET,
    &docprops::SET,
    &marks::HEADER_FOOTER,
    &marks::WATERMARK,
    &marks::BATES,
    &marks::REMOVE,
    &flatten::FLATTEN,
    &xfdf::EXPORT,
    &xfdf::IMPORT,
    &security::OPEN,
    &security::INFO,
    &security::SET,
    &security::REMOVE,
    &files::REDUCE,
    &files::PRINT,
    &files::SPLIT,
    &files::COMBINE,
    &files::REPLACE,
    &files::BOXES,
    &files::CROP,
    &files::RESIZE,
    &compare::COMPARE,
    &compare::OVERLAY,
    &visual::SEARCH,
    &ocr::OCR,
    &redact::MARK,
    &redact::LIST,
    &redact::APPLY,
    &forms::LIST,
    &forms::FILL,
    &forms::ADD,
    &forms::RESET,
    &forms::FLATTEN,
    &sign::ID_CREATE,
    &sign::SIGN,
    &sign::LIST,
    &spell::CHECK,
    &takeoff::CUTOUT_ADD,
    &takeoff::CUTOUT_DELETE,
    &takeoff::CELL_SET,
    &takeoff::COLUMNS_SET,
    &layers::LIST,
    &layers::CREATE,
    &layers::RENAME,
    &layers::DELETE,
    &layers::SET,
    &layers::ASSIGN,
    &layers::MARKUPS,
    &spaces::LIST,
    &spaces::ADD,
    &spaces::EDIT,
    &spaces::DELETE,
    &spaces::TALLY,
    &spaces::EXPORT,
    &spaces::IMPORT,
    &fill::DYNAMIC_FILL,
    &fill::HATCH,
    &legend::ADD,
    &legend::LIST,
    &legend::UPDATE,
    &stamps::LIST,
    &stamps::CREATE,
    &stamps::REMOVE,
    &stamps::ADD,
    &summary::EXPORT,
    // wave 5B: export, archive, compare and overlay extras, batch compare
    &convert::EXPORT_IMAGES,
    &convert::EXPORT_DOCUMENT,
    &convert::EXPORT_REGION,
    &convert::REPAIR,
    &convert::PDFA,
    &convert::COLOR_PROCESS,
    &flatten::UNFLATTEN,
    &compare::PRESET,
    &batch_compare::MATCH,
    &batch_compare::COMPARE,
    &batch_compare::OVERLAY,
    &search::REPLACE,
    &search::REGION_TEXT,
    &links::EDIT,
    &links::FROM_URLS,
    &links::PLACES,
    &links::PLACE_SET,
    &links::PLACE_DELETE,
    &links::MARKUP_ACTION,
    &links::SNAPSHOT,
    &links::ATTACH,
    &links::CAPTURE,
    &bookmarks::STYLE,
    &bookmarks::ACTION,
    &bookmarks::COPY,
    &bookmarks::AUTOMARK,
    &bookmarks::STRUCTURE,
    &bookmarks::AUDIT,
    &bookmarks::EXPORT,
    &sets::LAYER_NEST,
    &sets::LAYER_CONFIG,
    &sets::LAYER_VIEW,
    &sets::LAYER_IMPORT,
    &sets::LAYER_EXPORT,
    &sets::SET_TAGS,
    &sets::SET_PUBLISH,
    &sets::SET_PRINT,
    &sets::PRINTERS,
    &sets::BATCH_PRINT,
    &legend::COPY,
    &legend::FREEZE,
    &redact::CODES,
    &docs5b::SECURITY_PRESET,
    &docs5b::HF_TEMPLATE,
    &docs5b::HF_KEPT,
    &docs5b::FIT_CONTENT,
    &docs5b::FORM_DATA_EXPORT,
    &docs5b::FORM_DATA_IMPORT,
    &docs5b::FORM_DATA_MERGE,
    &docs5b::FORM_TYPEWRITER,
    &docs5b::FORM_AUTO_FIELDS,
    &docs5b::COMBINE_FILES,
    &docs5b::CREATE_FROM_FILES,
    &docs5b::LAYERED_PDF,
    &docs5b::QUANTITY_LINK,
    &prefs::GET,
    &prefs::SET,
    &prefs::PROFILES,
    &prefs::SWITCH,
    &prefs::DELETE,
    &prefs::EXPORT,
    &prefs::IMPORT,
    &batch::SET_SAVE,
    &batch::SET_SHEETS,
    &batch::SUMMARY,
    &batch::LINK,
    &batch::SLIP,
    &batch::APPLY,
    &docfile::FROM_IMAGE,
    &docfile::REVISIONS,
    &docfile::PUBLISH,
    &docfile::DESKEW,
    &docfile::STANDARDS,
    &docfile::REGION_LABELS,
    &markup_ops::INK_ERASE,
    &markup_ops::COUNT_EDIT,
    &markup_ops::SELECT_LASSO,
    &markup_ops::ARC_EDIT,
    &markup_ops::CUTOUT_SPLIT,
    &markup_ops::RECALCULATE,
    &markup_ops::MEASURE_PROPS,
    &markup_ops::UNFLATTEN,
    &markup_ops::IMPORT_PDF,
    &markup_ops::ATTACH_FILE,
    &markup_ops::SAVE_ATTACHED,
    &markup_ops::REPLY,
    &markup_ops::SUMMARY_APPEND,
    &markup_ops::SCALE_TEMPORARY,
    // wave 6A
    &docs6a::PACKAGE,
    &docs6a::BOOKMARKS_FROM_SOURCE,
    &docs6a::INSERT_WITH,
    &docs6a::INSERT_LAYERED,
    &docs6a::STITCH,
    &docs6a::BATCH_SIGN,
    &docs6a::SHELL_INTEGRATION,
    &docs6a::SMART_OVERLAY,
    &docs6a::JAVASCRIPT,
    &docs6a::WEBTAB,
    &docs6a::SCAN,
    &docs6a::CAMERA,
    &docs6a::STAMP_INTERACTIVE,
    &docs6a::STAMP_SETTINGS,
    &docs6a::REDACT_TEXT,
    // wave 6B: partial rows finished
    &partials::INSERT_FILES,
    &partials::INSERT_BLANK_STYLED,
    &partials::EXTRACT_EACH,
    &partials::PAGE_SETUP,
    &partials::CREATE_EACH,
    &partials::PAGE_TEMPLATE,
    &partials::EMAIL_TEMPLATE,
    &partials::BATCH_SPLIT,
    &partials::BATCH_SCRIPT,
    &partials::LEGEND_TOOLSET,
    &partials::STATUS_REPORT,
    &partials::VIEWPORT_CALIBRATE,
    &partials::REDACT_KINDS,
    &partials::SNAPSHOT_CUT,
    &partials::ID_STORE,
    &partials::CLEAR_CERTIFICATION,
    &partials::FORM_PROPS,
    &partials::FORM_ACTION,
    &partials::XFA_LAYOUT,
    &partials::FLATTEN_EXTRAS,
];

pub fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().copied().find(|t| t.name == name)
}

/// The tool table as JSON (name, title, description, schema, hints).
pub fn list_json() -> Value {
    Value::Array(
        TOOLS
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "title": t.title,
                    "description": t.description,
                    "read_only": t.read_only,
                    "destructive": t.destructive,
                    "input_schema": (t.schema)(),
                })
            })
            .collect(),
    )
}

// ---- schema pieces ------------------------------------------------------------------------

pub(crate) fn schema(props: Value, required: &[&str]) -> Value {
    let mut props = props;
    if let Some(o) = props.as_object_mut() {
        o.insert("doc".into(), doc_arg());
    }
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

/// A schema without the `doc` argument (tools that open documents or list them).
pub(crate) fn schema_nodoc(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

fn doc_arg() -> Value {
    json!({ "type": "integer", "minimum": 1, "description": "Document id from doc_open (default: the last document used)." })
}

pub(crate) fn path_arg(what: &str) -> Value {
    json!({ "type": "string", "description": format!("{what} (relative to --root when one is set).") })
}

pub(crate) fn page_arg(what: &str) -> Value {
    json!({ "type": "integer", "minimum": 1, "description": format!("1-based page {what}.") })
}

pub(crate) fn pages_arg(what: &str) -> Value {
    json!({ "type": ["array", "string"], "items": { "type": "integer", "minimum": 1 }, "description": format!("Pages {what}: [1, 3] or a range like \"1-3, 5, 8-\".") })
}

pub(crate) fn point_arg(what: &str) -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2, "description": format!("{what}: [x, y] in PDF points, origin bottom-left.") })
}

pub(crate) fn points_arg(what: &str) -> Value {
    json!({ "type": "array", "items": point_arg("a vertex"), "description": format!("{what}: [[x, y], ...] in PDF points, origin bottom-left.") })
}

pub(crate) fn rect_arg(what: &str) -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4, "description": format!("{what}: [x0, y0, x1, y1] in PDF points.") })
}

pub(crate) fn ids_arg() -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "description": "Markup ids (markup_list shows them). Default: the selection." })
}

pub(crate) fn scale_arg() -> Value {
    json!({
        "type": "object",
        "description": "A scale: {\"kind\": \"architectural\", \"paper_inches\": 0.125, \"real_feet\": 1} (1/8\" = 1'-0\"), {\"kind\": \"engineering\", \"feet_per_inch\": 20}, {\"kind\": \"ratio\", \"ratio\": 100, \"unit\": \"m\"} (1:100 in metres) or {\"kind\": \"custom\", \"scale\": {...}}.",
        "properties": {
            "kind": { "type": "string", "enum": ["architectural", "engineering", "ratio", "custom"] },
            "paper_inches": { "type": "number" }, "real_feet": { "type": "number" },
            "feet_per_inch": { "type": "number" }, "ratio": { "type": "number" },
            "unit": { "type": "string" }, "scale": { "type": "object" }
        },
        "required": ["kind"]
    })
}

/// Properties shared by markup_add and markup_edit.
pub(crate) fn markup_props(mut extra: Value) -> Value {
    let color =
        || json!({ "type": ["string", "array"], "description": "\"#RRGGBB\", a name, or [r, g, b] from 0 to 1." });
    let text = |d: &str| json!({ "type": "string", "description": d });
    let num = |d: &str| json!({ "type": "number", "description": d });
    let b = |d: &str| json!({ "type": "boolean", "description": d });
    if let Some(o) = extra.as_object_mut() {
        o.insert("color".into(), color());
        o.insert(
            "fill".into(),
            json!({ "type": ["string", "array", "null"], "description": "Fill colour; null or \"none\" for no fill." }),
        );
        o.insert("opacity".into(), num("Line opacity, 0 to 1."));
        o.insert("fill_opacity".into(), num("Fill opacity, 0 to 1."));
        o.insert("width".into(), num("Line width in points."));
        o.insert(
            "dash".into(),
            json!({ "type": "array", "items": { "type": "number" }, "description": "Dash lengths in points; [] = solid." }),
        );
        o.insert("subject".into(), text("Subject (Markups List)."));
        o.insert("label".into(), text("Label."));
        o.insert("author".into(), text("Author."));
        o.insert(
            "contents".into(),
            text("Comment text (measurements show their quantity here on save)."),
        );
        o.insert("layer".into(), text("Layer name."));
        o.insert(
            "status".into(),
            text("Review status: Accepted, Rejected, Cancelled, Completed, or None."),
        );
        o.insert("checked".into(), b("Markups List check mark."));
        o.insert("locked".into(), b("Lock (true) or unlock (false)."));
        o.insert(
            "line_start".into(),
            text("Line start ending name (None, OpenArrow, ...)."),
        );
        o.insert("line_end".into(), text("Line end ending name."));
        o.insert("cloud".into(), num("Cloud intensity, 0 (straight) to 2."));
        o.insert("font".into(), text("Font family (Helvetica, Times, Courier)."));
        o.insert("font_size".into(), num("Font size in points."));
        o.insert("text_color".into(), color());
        o.insert("bold".into(), b("Bold text."));
        o.insert("italic".into(), b("Italic text."));
        o.insert("underline".into(), b("Underlined text."));
        o.insert("multiply".into(), b("Blend mode Multiply (true) or Normal (false)."));
        o.insert("hidden".into(), b("Hidden flag (not shown or printed)."));
        o.insert("print".into(), b("Print flag (printed with the page)."));
        o.insert("no_view".into(), b("NoView flag (printed but not shown)."));
        o.insert("show_caption".into(), b("Show a measurement's value caption."));
        o.insert("depth".into(), num("Volume depth, in the scale's first unit."));
        o.insert(
            "rise_drop".into(),
            num("Polylength rise/drop, in the scale's first unit."),
        );
        o.insert("scale".into(), scale_arg());
        o.insert(
            "columns".into(),
            json!({ "type": "object", "description": "Custom column values: {column id: value}; \"\" removes one." }),
        );
    }
    extra
}

// ---- results --------------------------------------------------------------------------------

fn hex(c: &markupcraft_engine::Color) -> String {
    c.hex()
}

/// A markup as tools report it (pages 1-based).
pub(crate) fn markup_json(m: &Markup) -> Value {
    json!({
        "id": m.id,
        "page": m.page + 1,
        "kind": m.kind.name(),
        "subject": m.subject,
        "label": m.label,
        "author": m.author,
        "contents": m.contents,
        "layer": m.layer,
        "status": m.status,
        "checked": m.checked,
        "locked": m.locked(),
        "group": m.group,
        "color": hex(&m.color),
        "fill": m.fill.as_ref().map(hex),
        "opacity": m.opacity,
        "fill_opacity": m.fill_opacity,
        "width": m.line_width,
        "dash": m.dash,
        "points": m.pts.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>(),
        "holes": m.holes.iter().map(|h| h.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "rect": m.rect.as_array(),
        "quantity": m.quantity(),
        "quantity_text": m.quantity_text(),
        "unit": m.unit(),
        "scale": m.scale.as_ref().map(|s| s.ratio.clone()),
        "columns": m.column_data,
        "hatch": m.hatch.map(|h| h.style.name()),
        "unsaved": m.dirty,
    })
}

/// The markups `ids` names, or the selection.
pub(crate) fn target_ids(s: &Session, a: &Args) -> Result<Vec<String>> {
    match a.opt_strings("ids")? {
        Some(ids) if !ids.is_empty() => Ok(ids),
        _ if !s.selection().is_empty() => Ok(s.selection().to_vec()),
        _ => Err(crate::bad_args(format!(
            "{}: pass ids, or select markups first (markup_select)",
            a.tool()
        ))),
    }
}
