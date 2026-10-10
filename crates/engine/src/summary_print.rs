//! Markup Summary straight to the printer: the summary is laid out as a print-ready PDF report
//! and handed to the system print command, the same path File > Print uses.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::printing::{print_command, send_to_printer};
use crate::summary::{SummaryFormat, SummaryOptions};
use crate::{Result, Session};

/// What printing the summary did.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryPrint {
    /// The print-ready PDF that was printed.
    pub pdf: PathBuf,
    /// Markups in the report.
    pub markups: usize,
    /// The print command (program and arguments).
    pub program: String,
    pub args: Vec<String>,
    /// Whether the command was handed to the system (false for a dry run).
    pub sent: bool,
}

/// A fresh path in the temporary folder for a print-ready summary.
fn temp_pdf() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    std::env::temp_dir().join(format!(
        "markupcraft-summary-print-{}-{}.pdf",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

impl Session {
    /// Print the Markup Summary on `printer` (`None` = the default printer), `copies` times.
    /// With `dry_run` the report is written and the command worked out, but nothing is sent.
    pub fn print_summary(
        &self,
        o: &SummaryOptions,
        printer: Option<&str>,
        copies: usize,
        dry_run: bool,
    ) -> Result<SummaryPrint> {
        let pdf = temp_pdf();
        let markups = self.export_summary(&pdf, Some(SummaryFormat::Pdf), o)?;
        let copies = copies.clamp(1, 999);
        let (program, args) = print_command(&pdf, printer, copies);
        let sent = if dry_run {
            false
        } else {
            send_to_printer(&pdf, printer, copies)?;
            !crate::print_seam::dry_run()
        };
        Ok(SummaryPrint {
            pdf,
            markups,
            program,
            args,
            sent,
        })
    }
}
