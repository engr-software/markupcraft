//! Slip Sheet (one document) and Batch Slip Sheet (many documents, one pool of revisions).

use std::collections::HashSet;
use std::path::PathBuf;

use markupcraft_engine::Session;
use markupcraft_engine::batch_compare::MatchBy;
use markupcraft_engine::slip::{Leftovers, SlipRun, SlipRunReport, SlipStatus, slip_report_csv, slip_report_pdf};
use serde_json::{Value, json};

use super::{Tool, path_arg, rect_arg, schema, schema_nodoc};
use crate::{Args, Automation, Result, bad_args, failed, summary};

fn run_schema() -> Value {
    json!({
        "new_file": path_arg("The revised set (or give new_files)"),
        "new_files": { "type": "array", "items": { "type": "string" }, "description": "The revised files." },
        "match": { "type": "string", "enum": ["label", "file_page", "region", "manual"], "description": "How sheets pair: page label (default), file name + page index, the text in `region` (AutoMark), or `pairs`." },
        "region": rect_arg("The title-block box whose text is the key (match: region)"),
        "pairs": { "type": "array", "items": { "type": "array" }, "description": "match: manual: [[old page, new file (from 1), new page], ...]" },
        "number_filter": { "type": "string", "description": "Match on the key part before this (e.g. \" - \")." },
        "filter": { "type": "string", "description": "Wildcard match filter: # digits, @ letters, * non-digits, ? a separator; sheets it does not match take no part." },
        "match_case": { "type": "boolean" },
        "insert_ahead": { "type": "boolean", "description": "Insert each revision ahead of its old page (kept) instead of replacing it." },
        "carry_markups": { "type": "boolean", "description": "Bring markups to the revisions (default true)." },
        "superseded": { "type": "boolean", "description": "Stamp the kept old pages SUPERSEDED (insert_ahead)." },
        "unflatten_first": { "type": "boolean" },
        "flatten_after": { "type": "boolean" },
        "redirect_links": { "type": "boolean", "description": "Links and bookmarks to an old page go to its revision (default true)." },
        "unmatched": { "type": "string", "enum": ["append", "skip", "extract"], "description": "New sheets that match nothing: appended (default), left out, or extracted to files in extract_dir." },
        "append_unmatched": { "type": "boolean", "description": "Older form of unmatched: append / skip." },
        "extract_dir": path_arg("The folder unmatched new sheets are extracted to"),
        "report_csv": path_arg("Write the report as CSV here"),
        "report_pdf": path_arg("Write the report as a PDF (with links) here")
    })
}

fn files_arg(a: &Automation, args: &Args, one: &str, many: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if let Some(f) = args.opt_str(one)? {
        out.push(a.resolve(f, false)?);
    }
    for f in args.opt_strings(many)?.unwrap_or_default() {
        out.push(a.resolve(&f, false)?);
    }
    Ok(out)
}

/// The run options from the arguments (no files).
fn run_of(a: &Automation, args: &Args, batch: bool) -> Result<(SlipRun, Option<PathBuf>, Option<PathBuf>)> {
    let matching = match args.opt_str("match")?.unwrap_or("label") {
        "label" | "page_label" => MatchBy::PageLabel,
        "file_page" | "file" | "file_name" => MatchBy::FileAndPage,
        "region" => {
            let r = args
                .opt_rect("region")?
                .ok_or_else(|| bad_args("match: region needs region [x0, y0, x1, y1]"))?;
            MatchBy::Region {
                rect: [r.x0, r.y0, r.x1, r.y1],
            }
        }
        "manual" => MatchBy::Manual,
        o => {
            return Err(bad_args(format!(
                "match is label, file_page, region or manual, not {o:?}"
            )));
        }
    };
    let mut pairs = Vec::new();
    for p in args.get("pairs").and_then(Value::as_array).into_iter().flatten() {
        let n = |i: usize| -> Result<usize> {
            let v = p
                .as_array()
                .and_then(|x| x.get(i))
                .and_then(Value::as_u64)
                .ok_or_else(|| bad_args("pairs are [[old page, new file, new page], ...] counted from 1"))?;
            usize::try_from(v)
                .ok()
                .and_then(|v| v.checked_sub(1))
                .ok_or_else(|| bad_args("pairs count from 1"))
        };
        pairs.push((n(0)?, n(1)?, n(2)?));
    }
    let unmatched = match (args.opt_str("unmatched")?, args.opt_bool("append_unmatched")?) {
        (Some("append"), _) | (None, Some(true)) => "append",
        (Some("skip"), _) | (None, Some(false)) => "skip",
        (Some("extract"), _) => "extract",
        (None, None) => {
            if batch {
                "skip"
            } else {
                "append"
            }
        }
        (Some(o), _) => return Err(bad_args(format!("unmatched is append, skip or extract, not {o:?}"))),
    };
    let leftovers = match unmatched {
        "append" if batch => return Err(bad_args("batch_slip_sheet: unmatched is skip or extract")),
        "append" => Leftovers::Append,
        "skip" => Leftovers::Skip,
        _ => Leftovers::Extract(
            a.resolve(
                args.opt_str("extract_dir")?
                    .ok_or_else(|| bad_args("unmatched: extract needs extract_dir"))?,
                true,
            )?,
        ),
    };
    let report = |k: &str| -> Result<Option<PathBuf>> {
        match args.opt_str(k)? {
            Some(p) => Ok(Some(a.resolve(p, true)?)),
            None => Ok(None),
        }
    };
    Ok((
        SlipRun {
            new_files: Vec::new(),
            matching,
            pairs,
            number_filter: args.opt_string("number_filter")?.unwrap_or_default(),
            filter: args.opt_string("filter")?.unwrap_or_default(),
            match_case: args.bool_or("match_case", false)?,
            insert_ahead: args.bool_or("insert_ahead", false)?,
            carry_markups: args.bool_or("carry_markups", true)?,
            superseded: args.bool_or("superseded", false)?,
            unflatten_first: args.bool_or("unflatten_first", false)?,
            flatten_after: args.bool_or("flatten_after", false)?,
            redirect_links: args.bool_or("redirect_links", true)?,
            leftovers,
        },
        report("report_csv")?,
        report("report_pdf")?,
    ))
}

fn write_reports(r: &SlipRunReport, csv: Option<&PathBuf>, pdf: Option<&PathBuf>) -> Result<()> {
    if let Some(p) = csv {
        std::fs::write(p, slip_report_csv(r)).map_err(failed)?;
    }
    if let Some(p) = pdf {
        slip_report_pdf(r, p)?;
    }
    Ok(())
}

fn report_json(r: &SlipRunReport) -> Value {
    let n = |v: Option<usize>| v.map(|p| p + 1);
    let rows = |s: SlipStatus| -> Vec<Value> {
        r.rows
            .iter()
            .filter(|x| x.status == s)
            .map(|x| {
                json!({
                    "key": x.key, "old_page": n(x.old_page), "label": if x.old_label.is_empty() { &x.new_label } else { &x.old_label },
                    "new_file": x.new_file.as_ref().map(|p| p.display().to_string()), "new_page": n(x.new_page),
                    "result_page": n(x.result_page), "markups": x.markups,
                    "extracted": x.extracted.as_ref().map(|p| p.display().to_string()),
                })
            })
            .collect()
    };
    json!({
        "matched": rows(SlipStatus::Matched),
        "unmatched_old": r.rows.iter().filter(|x| x.status == SlipStatus::OldUnmatched).filter_map(|x| n(x.old_page)).collect::<Vec<_>>(),
        "unmatched_new": r.rows.iter().filter(|x| x.status == SlipStatus::NewUnmatched).filter_map(|x| n(x.new_page)).collect::<Vec<_>>(),
        "unmatched_new_sheets": rows(SlipStatus::NewUnmatched),
        "appended": r.appended,
        "superseded": r.superseded,
        "unflattened": r.unflattened,
        "flattened": r.flattened,
        "links_redirected": r.links_redirected,
        "bookmarks_redirected": r.bookmarks_redirected,
    })
}

pub static SLIP: Tool = Tool {
    name: "slip_sheet",
    title: "Slip Sheet",
    description: "Slip Sheet: pair this document's sheets with the revised sheets of `new_file` / `new_files` (by page label, file name + page index, a title-block region's text, or manual pairs; `number_filter` keeps the key part before a separator, `filter` is a wildcard match filter). Matched pages are replaced in place (their markups, links and bookmarks stay), or with insert_ahead the revision goes ahead of the old page, the markups are copied forward and the old page can be stamped SUPERSEDED (unflatten_first / flatten_after; links and bookmarks redirected). New sheets that match nothing are appended, skipped or extracted to files. Optional CSV and PDF reports (with links). Undoable.",
    read_only: false,
    destructive: true,
    schema: || schema(run_schema(), &[]),
    run: |a, args| {
        let (mut run, csv, pdf) = run_of(a, args, false)?;
        run.new_files = files_arg(a, args, "new_file", "new_files")?;
        if run.new_files.is_empty() {
            return Err(bad_args("slip_sheet: give new_file or new_files"));
        }
        let (doc, s) = a.session(args)?;
        let r = s.slip_sheet_run(&run)?;
        write_reports(&r, csv.as_ref(), pdf.as_ref())?;
        let mut v = report_json(&r);
        if let Some(o) = v.as_object_mut() {
            o.insert("document".into(), summary(doc, s));
        }
        Ok(v)
    },
};

pub static BATCH_SLIP: Tool = Tool {
    name: "batch_slip_sheet",
    title: "Batch Slip Sheet",
    description: "Slip Sheet across many files: each of `files` (closed PDFs) is slip-sheeted against one pool of revised sheets from `new_files` (a revision is used once) with the options of slip_sheet, then saved in place (or into `out_dir`). New sheets that match nothing are skipped or extracted (unmatched: extract, extract_dir). One report (report_csv / report_pdf) for the run.",
    read_only: false,
    destructive: true,
    schema: || {
        let mut s = run_schema();
        if let Some(o) = s.as_object_mut() {
            o.insert(
                "files".into(),
                json!({ "type": "array", "items": { "type": "string" } }),
            );
            o.insert("out_dir".into(), path_arg("Write the slip-sheeted files here"));
        }
        schema_nodoc(s, &["files", "new_files"])
    },
    run: |a, args| {
        let (mut base, csv, pdf) = run_of(a, args, true)?;
        let files = files_arg(a, args, "file", "files")?;
        base.new_files = files_arg(a, args, "new_file", "new_files")?;
        if files.is_empty() || base.new_files.is_empty() {
            return Err(bad_args("batch_slip_sheet: give files and new_files"));
        }
        let out_dir = match args.opt_str("out_dir")? {
            Some(d) => {
                let d = a.resolve(d, true)?;
                std::fs::create_dir_all(&d).map_err(failed)?;
                Some(d)
            }
            None => None,
        };
        let leftovers = std::mem::take(&mut base.leftovers);
        base.leftovers = Leftovers::Skip;
        let mut used = HashSet::new();
        let mut all = SlipRunReport::default();
        let mut results = Vec::new();
        for f in &files {
            let r = (|| -> markupcraft_engine::Result<SlipRunReport> {
                let mut s = Session::open(f)?;
                let r = s.slip_sheet_run_with(&base, &mut used)?;
                match &out_dir {
                    Some(d) => s.save_as(d.join(f.file_name().unwrap_or_default()), true)?,
                    None => s.save(true)?,
                }
                Ok(r)
            })();
            match r {
                Ok(r) => {
                    results.push(
                        json!({ "file": f.display().to_string(), "ok": true, "matched": r.count(SlipStatus::Matched) }),
                    );
                    all.document = r.document.clone();
                    all.rows
                        .extend(r.rows.into_iter().filter(|x| x.status != SlipStatus::NewUnmatched));
                    all.superseded += r.superseded;
                    all.flattened += r.flattened;
                    all.unflattened += r.unflattened;
                }
                Err(e) => results.push(json!({ "file": f.display().to_string(), "ok": false, "error": e.to_string() })),
            }
        }
        // The revisions no file took: one more pass for the leftovers only.
        let left = markupcraft_engine::slip::leftover_sheets(&base.new_files, &used, &leftovers)?;
        all.rows.extend(left);
        write_reports(&all, csv.as_ref(), pdf.as_ref())?;
        let mut v = report_json(&all);
        if let Some(o) = v.as_object_mut() {
            o.insert("files".into(), json!(results));
        }
        Ok(v)
    },
};
