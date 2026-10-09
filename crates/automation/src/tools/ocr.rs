//! OCR.

use markupcraft_engine::ocr::{OcrOptions, recognizer};
use serde_json::{Value, json};

use super::{Tool, pages_arg, schema};
use crate::summary;

pub static OCR: Tool = Tool {
    name: "ocr_pages",
    title: "OCR",
    description: "Recognise text on scanned pages (default all) and add it as an invisible, searchable text layer over each word (the page image is untouched). Pages that already have text are skipped unless skip_text is false. Needs the OCR models (`cargo xtask models`, or MARKUPCRAFT_OCR_MODELS). Undoable as one step.",
    read_only: false,
    destructive: false,
    schema: || {
        schema(
            json!({
                "pages": pages_arg("to read (default: all)"),
                "dpi": { "type": "number", "description": "Resolution the pages are read at (default 300)." },
                "skip_text": { "type": "boolean", "description": "Leave pages that already have text alone (default true)." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let mut o = OcrOptions::default();
        if let Some(d) = args.opt_num("dpi")? {
            o.dpi = d;
        }
        o.skip_text_pages = args.bool_or("skip_text", true)?;
        let (doc, s) = a.session(args)?;
        if let Some(p) = args.opt_pages("pages", s.page_count())? {
            o.pages = p;
        }
        let rec = recognizer()?;
        let pages = s.ocr(&o, rec.as_ref())?;
        let list: Vec<Value> = pages
            .iter()
            .map(|p| json!({ "page": p.page + 1, "words": p.words, "text": p.text, "skipped": p.skipped }))
            .collect();
        let words: usize = pages.iter().map(|p| p.words).sum();
        Ok(json!({ "words": words, "pages": list, "document": summary(doc, s) }))
    },
};
