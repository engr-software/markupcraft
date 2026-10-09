//! Non-blocking file dialogs. Each request runs the platform dialog (rfd's async dialog) on its
//! own thread and the frame loop polls for the answer, so the window keeps painting and other
//! documents keep rendering while a dialog is open.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

/// What a dialog's answer is for.
#[derive(Debug, Clone, PartialEq)]
pub enum Purpose {
    Open,
    /// Save the document (by its position when asked) under a new name; then close it.
    SaveAs {
        doc: usize,
        then_close: bool,
    },
    /// Markups List exports
    ExportCsv,
    ExportTotals,
    ExportXml,
    /// Insert the pages of a PDF before `at` in the active document
    InsertPages {
        at: usize,
    },
    /// Extract these pages (0-based) of the active document to a new file
    ExtractPages {
        pages: Vec<usize>,
    },
    /// An answer for the shell (`shell::dialog_answer`): page dialogs, profiles, File Access.
    Shell {
        tag: String,
    },
}

struct Pending {
    purpose: Purpose,
    rx: Receiver<Vec<PathBuf>>,
}

/// Dialogs waiting for the user.
#[derive(Default)]
pub struct Dialogs {
    pending: Vec<Pending>,
    /// Tests: answer every request at once with these paths instead of asking.
    pub scripted: Option<Vec<PathBuf>>,
    answered: Vec<(Purpose, Vec<PathBuf>)>,
}

/// A file type filter: name and extensions.
pub type Filter = (&'static str, &'static [&'static str]);

pub const PDF: Filter = ("PDF", &["pdf"]);
pub const CSV: Filter = ("CSV", &["csv"]);
pub const XML: Filter = ("XML", &["xml"]);

impl Dialogs {
    /// A dialog is open.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Ask for files to open (`many`) or one file.
    pub fn open(&mut self, purpose: Purpose, filter: Filter, many: bool) {
        if let Some(a) = self.scripted.clone() {
            self.answered.push((purpose, a));
            return;
        }
        self.spawn(purpose, move || {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let d = rfd::AsyncFileDialog::new().add_filter(filter.0, filter.1);
                if many {
                    block_on(d.pick_files())
                        .unwrap_or_default()
                        .into_iter()
                        .map(|h| h.path().to_path_buf())
                        .collect()
                } else {
                    block_on(d.pick_file())
                        .map(|h| h.path().to_path_buf())
                        .into_iter()
                        .collect()
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = (filter, many);
                Vec::new()
            }
        });
    }

    /// Ask where to save, suggesting `name`.
    pub fn save(&mut self, purpose: Purpose, filter: Filter, name: &str) {
        if let Some(a) = self.scripted.clone() {
            self.answered.push((purpose, a));
            return;
        }
        let name = name.to_string();
        self.spawn(purpose, move || {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let d = rfd::AsyncFileDialog::new()
                    .add_filter(filter.0, filter.1)
                    .set_file_name(&name);
                block_on(d.save_file())
                    .map(|h| h.path().to_path_buf())
                    .into_iter()
                    .collect()
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = (filter, name);
                Vec::new()
            }
        });
    }

    fn spawn(&mut self, purpose: Purpose, ask: impl FnOnce() -> Vec<PathBuf> + Send + 'static) {
        let (tx, rx) = channel();
        let started = std::thread::Builder::new()
            .name("markupcraft-dialog".into())
            .spawn(move || {
                let _ = tx.send(ask());
            });
        if started.is_ok() {
            self.pending.push(Pending { purpose, rx });
        }
    }

    /// Answers that came in since the last call (an empty list = cancelled).
    pub fn poll(&mut self) -> Vec<(Purpose, Vec<PathBuf>)> {
        let mut out = std::mem::take(&mut self.answered);
        self.pending.retain(|p| match p.rx.try_recv() {
            Ok(paths) => {
                out.push((p.purpose.clone(), paths));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                out.push((p.purpose.clone(), Vec::new()));
                false
            }
        });
        out
    }
}

/// Run a future to completion on this thread (the dialog thread), parking between polls.
#[cfg(not(target_arch = "wasm32"))]
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_answers_come_back_on_poll() {
        let mut d = Dialogs {
            scripted: Some(vec![PathBuf::from("x.csv")]),
            ..Default::default()
        };
        d.save(Purpose::ExportCsv, CSV, "list.csv");
        assert!(!d.busy());
        assert_eq!(d.poll(), vec![(Purpose::ExportCsv, vec![PathBuf::from("x.csv")])]);
        assert!(d.poll().is_empty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn block_on_runs_a_ready_future() {
        assert_eq!(block_on(async { 41 + 1 }), 42);
    }
}
