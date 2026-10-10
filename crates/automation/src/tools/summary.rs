//! Markup Summary reports (CSV, XML, Excel .xlsx, PDF).

use markupcraft_engine::summary::{PdfLayout, SummaryContent, SummaryFormat, SummaryOptions};
use serde_json::Value;
use serde_json::json;

use super::{Tool, pages_arg, path_arg, schema};

pub static EXPORT: Tool = Tool {
    name: "summary_export",
    title: "Markup Summary",
    description: "Write the Markup Summary (the Markups List as a report) to `out`: CSV, XML, an Excel workbook (.xlsx, numbers as numbers) or a PDF report, chosen by `format` or the file extension. `columns` are Markups List column ids (default: the list's default columns), `pages` limits it, `group_by` groups with subtotals, `sort` orders the rows, `measurements_only` keeps measurements.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "out": path_arg("file to write (.csv, .xml, .xlsx or .pdf)"),
                "format": { "type": "string", "enum": ["csv", "xml", "xlsx", "pdf"] },
                "columns": { "type": "array", "items": { "type": "string" } },
                "pages": pages_arg("pages to include (default all)"),
                "group_by": { "type": "array", "items": { "type": "string" } },
                "sort": { "type": "string" },
                "descending": { "type": "boolean" },
                "measurements_only": { "type": "boolean" },
                "title": { "type": "string" },
                "then_by": { "type": "array", "items": { "type": "array" }, "description": "More sort keys: [[column id, descending], ...]." },
                "filters": { "type": "object", "description": "{column id: [allowed values]}." },
                "date_in_title": { "type": "boolean" },
                "content": { "type": "string", "enum": ["both", "markups", "totals"], "description": "CSV/XML: what rows go in." },
                "headers": { "type": "boolean", "description": "CSV: the header row (default true)." },
                "per_value": { "type": "boolean", "description": "One report per value of the first column, named <out stem> - <value>." },
                "pdf_flow": { "type": "boolean", "description": "PDF: each markup as a block of lines instead of a table." },
                "pdf_paper": { "type": "string", "description": "PDF page size name (default letter, landscape)." },
                "pdf_break_per_group": { "type": "boolean" },
                "pdf_links": { "type": "boolean", "description": "PDF: link each markup to its source page (default true)." },
                "pdf_totals": { "type": "boolean" },
                "pdf_padding": { "type": "number" },
                "pdf_logo": path_arg("PDF: a PNG logo at the top right"),
                "pdf_spaces_cover": { "type": "boolean", "description": "PDF: a cover sheet of the Spaces and their markup counts." },
                "pdf_status_history": { "type": "boolean", "description": "PDF: each markup's status history (review replies)." },
                "pdf_thumbnails": { "type": "integer", "minimum": 48, "maximum": 400, "description": "PDF: markup thumbnails of this many pixels, on contact sheets with an index." },
                "pdf_page_content": { "type": "boolean", "description": "PDF: the summarized pages themselves after the report." },
                "column_config": { "type": "string", "description": "Use a saved column configuration (summary_columns) instead of `columns`." },
                "include_empty": { "type": "boolean", "description": "Keep columns that are empty in every row (default true; a saved configuration says its own)." }
            }),
            &["out"],
        )
    },
    run: |a, args| {
        let out = a.resolve(args.opt_str("out")?.unwrap_or_default(), true)?;
        let format = match args.opt_str("format")? {
            Some(f) => Some(SummaryFormat::from_name(f).ok_or_else(|| crate::failed("format: csv, xml, xlsx or pdf"))?),
            None => None,
        };
        let count = a.session_ref(args)?.1.page_count();
        let o = SummaryOptions {
            columns: args.opt_strings("columns")?.unwrap_or_default(),
            pages: args.opt_pages("pages", count)?.unwrap_or_default(),
            measurements_only: args.opt_bool("measurements_only")?.unwrap_or(false),
            group_by: args.opt_strings("group_by")?.unwrap_or_default(),
            sort: args.opt_string("sort")?.unwrap_or_default(),
            descending: args.opt_bool("descending")?.unwrap_or(false),
            title: args.opt_string("title")?.unwrap_or_default(),
            ..Default::default()
        };
        let mut o = o;
        if let Some(v) = args.get("then_by") {
            for k in v
                .as_array()
                .ok_or_else(|| crate::bad_args("then_by: [[column, descending], ...]"))?
            {
                let col = k
                    .get(0)
                    .and_then(Value::as_str)
                    .ok_or_else(|| crate::bad_args("then_by: a column id first"))?;
                let desc = k.get(1).and_then(Value::as_bool).unwrap_or(false);
                o.then_by.push((col.to_string(), desc));
            }
        }
        if let Some(Value::Object(f)) = args.get("filters") {
            for (k, v) in f {
                let vals = v
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                o.filters.insert(k.clone(), vals);
            }
        }
        o.date_in_title = args.bool_or("date_in_title", false)?;
        o.content = match args.opt_str("content")?.unwrap_or("both") {
            "markups" => SummaryContent::Markups,
            "totals" => SummaryContent::Totals,
            _ => SummaryContent::Both,
        };
        o.no_headers = !args.bool_or("headers", true)?;
        let mut layout = PdfLayout {
            flow: args.bool_or("pdf_flow", false)?,
            break_per_group: args.bool_or("pdf_break_per_group", false)?,
            links: args.bool_or("pdf_links", true)?,
            totals: args.bool_or("pdf_totals", true)?,
            ..Default::default()
        };
        if let Some(p) = args.opt_str("pdf_paper")? {
            let (w, h) = markupcraft_engine::printout::paper_size(p)
                .ok_or_else(|| crate::bad_args(format!("unknown paper {p:?}")))?;
            layout.page_size = (w.max(h), w.min(h));
        }
        if let Some(p) = args.opt_num("pdf_padding")? {
            layout.padding = p;
        }
        if let Some(l) = args.opt_str("pdf_logo")? {
            layout.logo = Some(a.resolve(l, false)?);
        }
        o.layout = layout;
        let config = match args.opt_str("column_config")? {
            Some(n) => Some(markupcraft_engine::summary_cols::find_config(&a.config_dir()?, n)?),
            None => None,
        };
        let (doc, s) = a.session_ref(args)?;
        if let Some(c) = &config {
            o.use_columns(s.doc(), c);
        }
        if !args.bool_or("include_empty", true)? {
            o.drop_empty_columns(s.doc());
        }
        if args.bool_or("per_value", false)? {
            let files = s.export_summary_per_value(&out, format, &o)?;
            return Ok(
                json!({ "doc": doc, "files": files.iter().map(|f| f.display().to_string()).collect::<Vec<_>>() }),
            );
        }
        let x = markupcraft_engine::finish::summary_more::SummaryExtras {
            spaces_cover: args.bool_or("pdf_spaces_cover", false)?,
            status_history: args.bool_or("pdf_status_history", false)?,
            thumbnails: args
                .opt_u64("pdf_thumbnails")?
                .map(|v| u32::try_from(v).unwrap_or(u32::MAX)),
            page_content: args.bool_or("pdf_page_content", false)?,
        };
        let pdf = format == Some(markupcraft_engine::summary::SummaryFormat::Pdf)
            || (format.is_none() && out.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")));
        let n = if pdf && x != Default::default() {
            s.export_summary_extras(&out, &o, &x)?
        } else {
            s.export_summary(&out, format, &o)?
        };
        Ok(json!({ "doc": doc, "markups": n, "out": out.display().to_string() }))
    },
};
