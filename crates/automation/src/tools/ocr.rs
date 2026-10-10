//! OCR.

use markupcraft_engine::ocr::{OcrAccuracy, OcrDocType, OcrOptions, recognizer};
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
                "skip_text": { "type": "boolean", "description": "Leave pages that already have text alone (default true)." },
                "deskew": { "type": "boolean", "description": "Correct skewed scans (default false)." },
                "detect_orientation": { "type": "boolean", "description": "Read pages scanned on their side or upside down, and vertical text (default false)." },
                "skip_vector": { "type": "boolean", "description": "Leave pages without images alone (default false)." },
                "accuracy": { "type": "string", "enum": ["speed", "balanced", "accuracy"], "description": "Accuracy against speed: sets the resolution pages are read at (with doc_type) unless `dpi` is given." },
                "doc_type": { "type": "string", "enum": ["drawing", "text_document"], "description": "A CAD drawing (small scattered text, default) or a text document (larger type, read at a lower resolution)." },
                "chunk_pages": { "type": "integer", "minimum": 0, "description": "Pages read at a time (0 = all): bounds memory on long documents." },
                "max_vector_kb": { "type": "integer", "minimum": 0, "description": "Leave pages alone whose vector content is larger than this many KB (0 = no limit)." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let accuracy = match args.opt_str("accuracy")? {
            Some(n) => OcrAccuracy::from_name(n)
                .ok_or_else(|| crate::bad_args(format!("accuracy {n:?}: speed, balanced or accuracy")))?,
            None => OcrAccuracy::default(),
        };
        let doc_type = match args.opt_str("doc_type")? {
            Some(n) => OcrDocType::from_name(n)
                .ok_or_else(|| crate::bad_args(format!("doc_type {n:?}: drawing or text_document")))?,
            None => OcrDocType::default(),
        };
        let mut o = OcrOptions::default();
        if args.has("accuracy") || args.has("doc_type") {
            o = o.with_preset(accuracy, doc_type);
        }
        if let Some(d) = args.opt_num("dpi")? {
            o.dpi = d;
        }
        o.chunk_pages = args.opt_u64("chunk_pages")?.unwrap_or(0).min(100_000) as usize;
        o.max_vector_kb = args.opt_u64("max_vector_kb")?.unwrap_or(0).min(1 << 30) as usize;
        o.skip_text_pages = args.bool_or("skip_text", true)?;
        o.deskew = args.bool_or("deskew", false)?;
        o.detect_orientation = args.bool_or("detect_orientation", false)?;
        o.skip_vector_pages = args.bool_or("skip_vector", false)?;
        let (doc, s) = a.session(args)?;
        if let Some(p) = args.opt_pages("pages", s.page_count())? {
            o.pages = p;
        }
        let rec = recognizer()?;
        let pages = s.ocr(&o, rec.as_ref())?;
        let chunks = o.chunks(pages.len());
        let list: Vec<Value> = pages
            .iter()
            .map(|p| json!({ "page": p.page + 1, "words": p.words, "text": p.text, "skipped": p.skipped }))
            .collect();
        let words: usize = pages.iter().map(|p| p.words).sum();
        Ok(json!({ "words": words, "pages": list, "dpi": o.dpi, "chunks": chunks, "document": summary(doc, s) }))
    },
};
