//! Batch > Compare Documents and Batch > Overlay Pages, with their saved batch files.

use std::path::PathBuf;

use markupcraft_engine::batch_compare::{
    BatchJob, BatchReport, MatchBy, SheetPair, SheetRef, batch_compare, batch_overlay, batch_overlay_colors,
    collect_pdfs, load_job, match_sheets, report_csv, report_pdf, save_job, stamp_now,
};
use markupcraft_engine::overlay::OverlayAlign;
use serde_json::{Value, json};

use super::compare::compare_options;
use super::{Tool, path_arg, rect_arg, schema_nodoc};
use crate::{Args, Automation, Result, bad_args};

fn job_props() -> Value {
    let paths = |d: &str| json!({ "type": "array", "items": { "type": "string" }, "description": d });
    json!({
        "job": path_arg("A saved batch file (.pcbatch) to start from"),
        "current": paths("Current (older) PDFs or folders."),
        "revised": paths("Revised (newer) PDFs or folders."),
        "recursive": { "type": "boolean", "description": "Search folders' subfolders too." },
        "match": { "type": "string", "enum": ["file", "label", "region", "manual"], "description": "Pair by file name + page index (default), page label, the text in `region`, or the `pairs` given." },
        "region": rect_arg("match region: the title-block box holding the sheet number"),
        "filter": { "type": "string", "description": "Wildcard filter for the keys: # digits, @ letters, * non-digits, ? a separator, \\ escapes; e.g. \"@?#\" keeps A-101 of \"A-101 rev 2\"." },
        "pairs": { "type": "array", "items": { "type": "object" }, "description": "match manual (or to re-pair): [{\"current\": path, \"current_page\": 1, \"revised\": path, \"revised_page\": 1}, ...]." },
        "save_job": path_arg("Save the job (lists, matching and pairs) here")
    })
}

fn sheet_json(s: &SheetRef) -> Value {
    json!({ "file": s.file.display().to_string(), "page": s.page + 1, "key": s.key })
}

/// The batch job from the arguments (a saved job first, arguments override).
fn job_from(a: &Automation, args: &Args) -> Result<BatchJob> {
    let mut job = match args.opt_str("job")? {
        Some(p) => load_job(&a.resolve(p, false)?)?,
        None => BatchJob::default(),
    };
    let recursive = args.bool_or("recursive", false)?;
    let list = |k: &str| -> Result<Option<Vec<PathBuf>>> {
        match args.opt_strings(k)? {
            Some(v) => {
                let resolved: Vec<PathBuf> = v.iter().map(|p| a.resolve(p, false)).collect::<Result<_>>()?;
                Ok(Some(collect_pdfs(&resolved, recursive)?))
            }
            None => Ok(None),
        }
    };
    if let Some(c) = list("current")? {
        job.current = c;
    }
    if let Some(r) = list("revised")? {
        job.revised = r;
    }
    if let Some(m) = args.opt_str("match")? {
        job.matching = match m {
            "file" => MatchBy::FileAndPage,
            "label" => MatchBy::PageLabel,
            "region" => {
                let r = args
                    .opt_rect("region")?
                    .ok_or_else(|| bad_args("match region needs region"))?;
                MatchBy::Region { rect: r.as_array() }
            }
            "manual" => MatchBy::Manual,
            o => return Err(bad_args(format!("match {o:?}"))),
        };
    }
    if let Some(f) = args.opt_string("filter")? {
        job.filter = f;
    }
    if let Some(v) = args.get("pairs") {
        let arr = v.as_array().ok_or_else(|| bad_args("pairs must be a list"))?;
        let mut pairs = Vec::new();
        for p in arr {
            let o = p.as_object().ok_or_else(|| bad_args("each pair is an object"))?;
            let file = |k: &str| -> Result<PathBuf> {
                let s = o
                    .get(k)
                    .and_then(Value::as_str)
                    .ok_or_else(|| bad_args(format!("pair: {k} is required")))?;
                a.resolve(s, false)
            };
            let page = |k: &str| -> Result<usize> {
                match o.get(k).map(Value::as_u64) {
                    None => Ok(0),
                    Some(Some(n)) if n >= 1 => Ok(n as usize - 1),
                    _ => Err(bad_args(format!("pair: {k} is a page from 1"))),
                }
            };
            pairs.push(SheetPair {
                current: SheetRef {
                    file: file("current")?,
                    page: page("current_page")?,
                    key: String::new(),
                },
                revised: SheetRef {
                    file: file("revised")?,
                    page: page("revised_page")?,
                    key: String::new(),
                },
            });
        }
        job.pairs = pairs;
        if args.opt_str("match")?.is_none() {
            job.matching = MatchBy::Manual;
        }
    }
    Ok(job)
}

/// Pair the job, save it when asked; returns the pairs and the unmatched sheets.
/// A job, its pairs, and the current and revised sheets that matched nothing.
type Paired = (BatchJob, Vec<SheetPair>, Vec<SheetRef>, Vec<SheetRef>);

fn paired(a: &Automation, args: &Args) -> Result<Paired> {
    let mut job = job_from(a, args)?;
    let (pairs, lc, lr) = match_sheets(&job)?;
    job.pairs = pairs.clone();
    if let Some(p) = args.opt_str("save_job")? {
        save_job(&a.resolve(p, true)?, &job)?;
    }
    Ok((job, pairs, lc, lr))
}

pub static MATCH: Tool = Tool {
    name: "batch_match",
    title: "Batch: match sheets",
    description: "Pair current sheets with their revisions for Batch Compare or Batch Overlay (add files or folders, match by file name + page, page label or a title-block region, with an optional wildcard filter). Returns the pairs and what matched nothing; save_job writes the batch file to reuse (and re-pair by editing its pairs).",
    read_only: false,
    destructive: false,
    schema: || schema_nodoc(job_props(), &[]),
    run: |a, args| {
        let (_, pairs, lc, lr) = paired(a, args)?;
        Ok(json!({
            "pairs": pairs.iter().map(|p| json!({ "current": sheet_json(&p.current), "revised": sheet_json(&p.revised) })).collect::<Vec<_>>(),
            "unmatched_current": lc.iter().map(sheet_json).collect::<Vec<_>>(),
            "unmatched_revised": lr.iter().map(sheet_json).collect::<Vec<_>>(),
        }))
    },
};

fn report_json(a: &Automation, args: &Args, r: &BatchReport) -> Result<Value> {
    let stamp = args.bool_or("date_stamp", true)?.then(stamp_now);
    if let Some(p) = args.opt_str("report")? {
        let path = a.resolve(p, true)?;
        let is_pdf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        if is_pdf {
            let n = args.opt_str("report_paper")?.unwrap_or("letter");
            let size =
                markupcraft_engine::printout::paper_size(n).ok_or_else(|| bad_args(format!("unknown paper {n:?}")))?;
            report_pdf(r, &path, (size.1.max(size.0), size.1.min(size.0)), stamp.as_deref())?;
        } else {
            std::fs::write(&path, report_csv(r, stamp.as_deref())).map_err(crate::failed)?;
        }
    }
    Ok(json!({
        "results": r.results.iter().map(|l| json!({
            "current": sheet_json(&l.current),
            "revised": sheet_json(&l.revised),
            "differences": l.differences,
            "output": l.output.display().to_string(),
            "error": l.error,
        })).collect::<Vec<_>>(),
        "outputs": r.outputs.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "total_differences": r.total_differences(),
    }))
}

fn run_props(extra: Value) -> Value {
    let mut p = job_props();
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
        o.insert("out_dir".into(), path_arg("Folder for the results"));
        o.insert("suffix".into(), json!({ "type": "string", "description": "Added to result file names (default \"_compared\" / \"_overlay\")." }));
        o.insert(
            "report".into(),
            path_arg("Write a report here (.csv, or .pdf with a link to each result)"),
        );
        o.insert(
            "report_paper".into(),
            json!({ "type": "string", "description": "Report page size name (default letter, landscape)." }),
        );
        o.insert(
            "date_stamp".into(),
            json!({ "type": "boolean", "description": "Date and time on the report (default true)." }),
        );
    }
    p
}

pub static COMPARE: Tool = Tool {
    name: "batch_compare",
    title: "Batch Compare Documents",
    description: "Batch > Compare Documents: pair the sheets (see batch_match), compare every pair and write one copy of each revised file to out_dir with its changes clouded, plus an optional report (CSV or PDF with links). Takes compare_documents' tuning (mode, preset, sensitivity, align ...). A pair that fails is reported, not fatal.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            run_props(json!({
                "mode": { "type": "string", "enum": ["text", "graphics", "both"] },
                "preset": { "type": "string" },
                "sensitivity": { "type": "number", "minimum": 0, "maximum": 1 },
                "dpi": { "type": "number" },
                "align": { "type": "string", "enum": ["page", "auto"] },
                "color": { "type": ["string", "array"] },
                "subject": { "type": "string" }
            })),
            &["out_dir"],
        )
    },
    run: |a, args| {
        let out_dir = a.resolve(args.str("out_dir")?, true)?;
        let opts = compare_options(a, args)?;
        let (_, pairs, _, _) = paired(a, args)?;
        let suffix = args.opt_string("suffix")?.unwrap_or_else(|| "_compared".into());
        let r = batch_compare(&pairs, &opts, &out_dir, &suffix)?;
        report_json(a, args, &r)
    },
};

pub static OVERLAY: Tool = Tool {
    name: "batch_overlay",
    title: "Batch Overlay Pages",
    description: "Batch > Overlay Pages: pair the sheets (see batch_match) and write one overlay PDF per pair to out_dir (current red, revised blue, each a toggleable layer), plus an optional report (CSV or PDF with links). align page (default) or auto.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            run_props(json!({
                "align": { "type": "string", "enum": ["page", "auto", "bounds"] }
            })),
            &["out_dir"],
        )
    },
    run: |a, args| {
        let out_dir = a.resolve(args.str("out_dir")?, true)?;
        let align = match args.opt_str("align")?.unwrap_or("page") {
            "page" => OverlayAlign::Page,
            "auto" => OverlayAlign::Auto,
            "bounds" => OverlayAlign::Bounds,
            o => return Err(bad_args(format!("align {o:?}"))),
        };
        let (_, pairs, _, _) = paired(a, args)?;
        let suffix = args.opt_string("suffix")?.unwrap_or_else(|| "_overlay".into());
        let r = batch_overlay(&pairs, batch_overlay_colors(), align, &out_dir, &suffix)?;
        report_json(a, args, &r)
    },
};
