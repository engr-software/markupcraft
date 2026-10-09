//! Markup Summary reports (CSV, XML, Excel .xlsx, PDF).

use markupcraft_engine::summary::{SummaryFormat, SummaryOptions};
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
                "title": { "type": "string" }
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
        let (doc, s) = a.session_ref(args)?;
        let o = SummaryOptions {
            columns: args.opt_strings("columns")?.unwrap_or_default(),
            pages: args.opt_pages("pages", s.page_count())?.unwrap_or_default(),
            measurements_only: args.opt_bool("measurements_only")?.unwrap_or(false),
            group_by: args.opt_strings("group_by")?.unwrap_or_default(),
            sort: args.opt_string("sort")?.unwrap_or_default(),
            descending: args.opt_bool("descending")?.unwrap_or(false),
            title: args.opt_string("title")?.unwrap_or_default(),
            ..Default::default()
        };
        let n = s.export_summary(&out, format, &o)?;
        Ok(json!({ "doc": doc, "markups": n, "out": out.display().to_string() }))
    },
};
