//! Background jobs: start a long batch job (Stapler one PDF per file or all in one, batch
//! flatten / unflatten) on the worker thread, list the jobs with their progress, wait for one,
//! cancel one. The same queue the app's Window > Jobs dialog shows.

use std::path::PathBuf;
use std::time::Duration;

use markupcraft_engine::jobs::{create_combined_job, create_each_job, flatten_job};
use serde_json::{Value, json};

use super::{Tool, path_arg, schema_nodoc};
use crate::{Result, bad_args};

fn info(j: &markupcraft_engine::jobs::JobInfo) -> Value {
    serde_json::to_value(j).unwrap_or(Value::Null)
}

pub static START: Tool = Tool {
    name: "job_start",
    title: "Start a background job",
    description: "Queue a long batch job on the background worker and return its id at once: kind create_each (one PDF per file, into out_dir or beside each source), create (every file in one PDF at out), flatten or unflatten (files saved in place). Jobs run one after another; job_list shows progress, job_wait waits, job_cancel stops one.",
    read_only: false,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({
                "kind": { "type": "string", "enum": ["create_each", "create", "flatten", "unflatten"] },
                "files": { "type": "array", "items": { "type": "string" } },
                "out": path_arg("For create: the PDF to write"),
                "out_dir": path_arg("For create_each: the output folder (default: beside each source)"),
                "picture_dpi": { "type": "number", "description": "create / create_each: pictures' resolution on the page, pixels per inch (36 to 1200, default 72)." },
                "grayscale": { "type": "boolean", "description": "create / create_each: pictures stored in grey." }
            }),
            &["kind", "files"],
        )
    },
    run: |a, args| {
        let kind = args.opt_str("kind")?.unwrap_or_default().to_string();
        let files: Vec<PathBuf> = args
            .opt_strings("files")?
            .unwrap_or_default()
            .iter()
            .map(|p| a.resolve(p, kind == "flatten" || kind == "unflatten"))
            .collect::<Result<_>>()?;
        let pictures = super::docs5b::picture_options(args)?;
        let (name, (total, work)) = match kind.as_str() {
            "create_each" => {
                let out = match args.opt_str("out_dir")? {
                    Some(d) => Some(a.resolve(d, true)?),
                    None => None,
                };
                (
                    "Create PDF from Files (one per file)",
                    create_each_job(files, out, pictures)?,
                )
            }
            "create" => {
                let out = args.opt_str("out")?.ok_or_else(|| bad_args("create needs out"))?;
                let out = a.resolve(out, true)?;
                ("Create PDF from Files", create_combined_job(files, out, pictures)?)
            }
            "flatten" => ("Batch Flatten", flatten_job(files, false)?),
            "unflatten" => ("Batch Unflatten", flatten_job(files, true)?),
            other => return Err(bad_args(format!("unknown job kind {other:?}"))),
        };
        let id = a.jobs().submit(name, total, false, work);
        Ok(json!({ "job": id }))
    },
};

pub static LIST: Tool = Tool {
    name: "job_list",
    title: "List background jobs",
    description: "Every background job with its state (queued, running, done, failed, cancelled), progress (done of total), message and output files. clear_finished: forget the finished ones first.",
    read_only: true,
    destructive: false,
    schema: || schema_nodoc(json!({ "clear_finished": { "type": "boolean" } }), &[]),
    run: |a, args| {
        if args.opt_bool("clear_finished")?.unwrap_or(false) {
            a.jobs().clear_finished();
        }
        let list: Vec<Value> = a.jobs().list().iter().map(info).collect();
        Ok(json!({ "jobs": list, "active": a.jobs().active() }))
    },
};

pub static WAIT: Tool = Tool {
    name: "job_wait",
    title: "Wait for a background job",
    description: "Wait up to timeout_secs (default 60, at most 3600) for job `job` to finish; returns its entry (its state says whether it finished).",
    read_only: true,
    destructive: false,
    schema: || {
        schema_nodoc(
            json!({ "job": { "type": "integer", "minimum": 1 }, "timeout_secs": { "type": "number", "minimum": 0 } }),
            &["job"],
        )
    },
    run: |a, args| {
        let id = args.opt_u64("job")?.ok_or_else(|| bad_args("job is required"))?;
        let secs = args.opt_num("timeout_secs")?.unwrap_or(60.0);
        if !secs.is_finite() || secs < 0.0 {
            return Err(bad_args("timeout_secs: 0 to 3600"));
        }
        let j = a
            .jobs()
            .wait(id, Duration::from_secs_f64(secs.min(3600.0)))
            .ok_or_else(|| bad_args(format!("no job {id}")))?;
        Ok(info(&j))
    },
};

pub static CANCEL: Tool = Tool {
    name: "job_cancel",
    title: "Cancel a background job",
    description: "Stop job `job`: a queued job never starts, a running one stops after its current file (files it finished stay written).",
    read_only: false,
    destructive: false,
    schema: || schema_nodoc(json!({ "job": { "type": "integer", "minimum": 1 } }), &["job"]),
    run: |a, args| {
        let id = args.opt_u64("job")?.ok_or_else(|| bad_args("job is required"))?;
        Ok(json!({ "cancelled": a.jobs().cancel(id) }))
    },
};
