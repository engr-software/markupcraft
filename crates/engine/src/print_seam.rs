//! A dry-run seam for printing: while it is on (tests, `--dry-run` style tool calls), the
//! system print command is recorded instead of run, so nothing ever reaches a device. The
//! print-ready PDF is still written, so what would print can be checked.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// One print command that would have run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintRecord {
    pub pdf: PathBuf,
    pub program: String,
    pub args: Vec<String>,
}

static DRY_RUN: AtomicBool = AtomicBool::new(false);
static RECORDS: Mutex<Vec<PrintRecord>> = Mutex::new(Vec::new());

/// Most commands kept (the oldest go first).
const MAX_RECORDS: usize = 1_000;

/// Record print commands instead of running them, for the rest of this process (`true`), or
/// run them again (`false`).
pub fn set_dry_run(on: bool) {
    DRY_RUN.store(on, Ordering::SeqCst);
}

/// Whether print commands are being recorded instead of run.
pub fn dry_run() -> bool {
    DRY_RUN.load(Ordering::SeqCst)
}

/// The commands recorded so far (oldest first).
pub fn recorded() -> Vec<PrintRecord> {
    RECORDS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Called by the print path: records the command and returns true when printing is a dry run.
pub(crate) fn record(pdf: &Path, program: &str, args: &[String]) -> bool {
    if !dry_run() {
        return false;
    }
    let mut r = RECORDS.lock().unwrap_or_else(|e| e.into_inner());
    if r.len() >= MAX_RECORDS {
        r.remove(0);
    }
    r.push(PrintRecord {
        pdf: pdf.to_path_buf(),
        program: program.to_string(),
        args: args.to_vec(),
    });
    true
}
