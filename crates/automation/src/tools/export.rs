//! Exports.

use serde_json::json;

use super::{Tool, page_arg, path_arg, schema};

pub static CSV: Tool = Tool {
    name: "export_csv",
    title: "Export the Markups List as CSV",
    description: "The Markups List as CSV (page, label, id, type, subject, label, author, layer, status, contents, measurement, value, unit, colour, custom columns): written to `out` (atomic) or returned as text.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({ "out": path_arg("CSV file to write"), "page": page_arg("only") }),
            &[],
        )
    },
    run: |a, args| {
        let page = args.opt_page("page")?;
        let out = match args.opt_str("out")? {
            Some(p) => Some(a.resolve(p, true)?),
            None => None,
        };
        let (doc, s) = a.session_ref(args)?;
        if let Some(p) = page {
            s.page(p)?;
        }
        let csv = markupcraft_engine::export::markups_csv(s.doc(), page);
        let rows = csv.lines().count().saturating_sub(1);
        match out {
            Some(path) => {
                write_atomic(&path, csv.as_bytes())?;
                Ok(json!({ "doc": doc, "rows": rows, "out": path.display().to_string() }))
            }
            None => Ok(json!({ "doc": doc, "rows": rows, "csv": csv })),
        }
    },
};

fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> crate::Result<()> {
    let err = |e: std::io::Error| crate::failed(format!("{}: {e}", path.display()));
    if path.is_dir() {
        return Err(crate::failed(format!("{} is a folder, not a file", path.display())));
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".markupcraft-tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(err)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        err(e)
    })
}
