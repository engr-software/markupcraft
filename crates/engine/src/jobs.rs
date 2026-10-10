//! Background jobs: long batch work (the Stapler's conversions, batch flatten and unflatten)
//! runs on a worker thread, one job after another, while the interface stays live. Each job
//! reports its progress (`done` of `total` steps) and can be cancelled: a queued job never
//! starts, a running one stops at its next step (the files it finished stay written).
//!
//! [`JobQueue`] is cheap to clone (it is a handle); the interface keeps one, and so does the
//! automation table (tools `job_start`, `job_list`, `job_wait`, `job_cancel`).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::flatten::FlattenFilter;
use crate::{Result, Session, invalid};

/// Most jobs listed at once (finished ones are forgotten first).
pub const MAX_JOBS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn finished(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::Done => "Done",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// One job as the jobs list shows it.
#[derive(Debug, Clone, Serialize)]
pub struct JobInfo {
    pub id: u64,
    pub name: String,
    pub state: JobState,
    /// Steps done and steps in all (files, usually).
    pub done: usize,
    pub total: usize,
    /// The result (or the error) once finished.
    pub message: String,
    /// The files it wrote.
    pub outputs: Vec<PathBuf>,
    /// A file to open when it finishes (a combined PDF).
    pub open: Option<PathBuf>,
    #[serde(skip)]
    reported: bool,
}

impl JobInfo {
    /// Share done, 0 to 1.
    pub fn fraction(&self) -> f32 {
        if self.state == JobState::Done {
            return 1.0;
        }
        if self.total == 0 {
            return 0.0;
        }
        (self.done as f32 / self.total as f32).clamp(0.0, 1.0)
    }
}

/// What a finished job hands back.
#[derive(Debug, Clone, Default)]
pub struct JobOutcome {
    pub message: String,
    pub open: Option<PathBuf>,
}

/// The running job's handle on its own entry: progress, outputs, cancellation.
pub struct JobCtx {
    id: u64,
    shared: Arc<Shared>,
    cancel: Arc<AtomicBool>,
}

impl JobCtx {
    /// `done` of `total` steps finished.
    pub fn progress(&self, done: usize, total: usize) {
        let mut g = self.shared.lock();
        if let Some(j) = g.jobs.iter_mut().find(|j| j.id == self.id) {
            j.done = done.min(total);
            j.total = total;
        }
    }

    /// A file the job wrote.
    pub fn output(&self, p: &Path) {
        let mut g = self.shared.lock();
        if let Some(j) = g.jobs.iter_mut().find(|j| j.id == self.id)
            && j.outputs.len() < 10_000
        {
            j.outputs.push(p.to_path_buf());
        }
    }

    /// Whether the job was asked to stop (check between steps).
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// The work of a job.
pub type Work = Box<dyn FnOnce(&JobCtx) -> Result<JobOutcome> + Send + 'static>;

struct Pending {
    id: u64,
    work: Work,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct Inner {
    jobs: Vec<JobInfo>,
    pending: VecDeque<Pending>,
    /// The running job's stop flag.
    running: Option<(u64, Arc<AtomicBool>)>,
    worker: bool,
    next: u64,
}

#[derive(Default)]
struct Shared {
    inner: Mutex<Inner>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A job that panicked while holding the lock leaves plain data behind: keep using it.
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The job queue (a handle; clones share it).
#[derive(Clone, Default)]
pub struct JobQueue {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for JobQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobQueue").field("jobs", &self.list().len()).finish()
    }
}

impl JobQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue `work` as job `name` with `total` steps. With `inline` it runs now, on this
    /// thread (no worker threads: headless tests, single-threaded builds). Returns its id.
    pub fn submit(&self, name: &str, total: usize, inline: bool, work: Work) -> u64 {
        let cancel = Arc::new(AtomicBool::new(false));
        let id = {
            let mut g = self.shared.lock();
            g.next = g.next.saturating_add(1);
            let id = g.next;
            // Forget the oldest finished jobs beyond the cap.
            while g.jobs.len() >= MAX_JOBS {
                match g.jobs.iter().position(|j| j.state.finished()) {
                    Some(i) => {
                        g.jobs.remove(i);
                    }
                    None => break,
                }
            }
            g.jobs.push(JobInfo {
                id,
                name: name.chars().take(200).collect(),
                state: JobState::Queued,
                done: 0,
                total,
                message: String::new(),
                outputs: Vec::new(),
                open: None,
                reported: false,
            });
            if !inline {
                g.pending.push_back(Pending {
                    id,
                    work,
                    cancel: cancel.clone(),
                });
                if !g.worker {
                    g.worker = true;
                    drop(g);
                    self.spawn_worker();
                }
                return id;
            }
            id
        };
        // Inline: `work` was not queued, so it is still ours to run.
        run_one(&self.shared, id, cancel, work);
        id
    }

    fn spawn_worker(&self) {
        let shared = self.shared.clone();
        let spawned = std::thread::Builder::new()
            .name("markupcraft-jobs".into())
            .spawn(move || worker(shared));
        if let Err(e) = spawned {
            // No thread: fail what is queued rather than leave it waiting forever.
            let mut g = self.shared.lock();
            g.worker = false;
            let ids: Vec<u64> = g.pending.drain(..).map(|p| p.id).collect();
            for j in g.jobs.iter_mut().filter(|j| ids.contains(&j.id)) {
                j.state = JobState::Failed;
                j.message = format!("could not start a worker thread: {e}");
            }
        }
    }

    /// Every job, oldest first.
    pub fn list(&self) -> Vec<JobInfo> {
        self.shared.lock().jobs.clone()
    }

    pub fn get(&self, id: u64) -> Option<JobInfo> {
        self.shared.lock().jobs.iter().find(|j| j.id == id).cloned()
    }

    /// Jobs queued or running.
    pub fn active(&self) -> usize {
        self.shared.lock().jobs.iter().filter(|j| !j.state.finished()).count()
    }

    /// Stop job `id`: a queued job is dropped, a running one stops at its next step. False when
    /// there is no such job or it has finished.
    pub fn cancel(&self, id: u64) -> bool {
        let mut g = self.shared.lock();
        if let Some(i) = g.pending.iter().position(|p| p.id == id) {
            g.pending.remove(i);
            if let Some(j) = g.jobs.iter_mut().find(|j| j.id == id) {
                j.state = JobState::Cancelled;
                j.message = "Cancelled before it started".into();
            }
            return true;
        }
        match &g.running {
            Some((rid, flag)) if *rid == id => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }

    /// Forget the finished jobs.
    pub fn clear_finished(&self) {
        self.shared.lock().jobs.retain(|j| !j.state.finished());
    }

    /// Jobs finished since the last call (each reported once).
    pub fn take_finished(&self) -> Vec<JobInfo> {
        let mut g = self.shared.lock();
        let mut out = Vec::new();
        for j in g.jobs.iter_mut().filter(|j| j.state.finished() && !j.reported) {
            j.reported = true;
            out.push(j.clone());
        }
        out
    }

    /// Wait up to `timeout` for job `id` to finish; its entry (finished or not).
    pub fn wait(&self, id: u64, timeout: Duration) -> Option<JobInfo> {
        let start = Instant::now();
        loop {
            let j = self.get(id)?;
            if j.state.finished() || start.elapsed() >= timeout {
                return Some(j);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn worker(shared: Arc<Shared>) {
    loop {
        let next = {
            let mut g = shared.lock();
            match g.pending.pop_front() {
                Some(p) => p,
                None => {
                    g.worker = false;
                    return;
                }
            }
        };
        run_one(&shared, next.id, next.cancel, next.work);
    }
}

fn run_one(shared: &Arc<Shared>, id: u64, cancel: Arc<AtomicBool>, work: Work) {
    {
        let mut g = shared.lock();
        g.running = Some((id, cancel.clone()));
        if let Some(j) = g.jobs.iter_mut().find(|j| j.id == id) {
            j.state = JobState::Running;
        }
    }
    let ctx = JobCtx {
        id,
        shared: shared.clone(),
        cancel: cancel.clone(),
    };
    // A job must never take the worker (or the interface) down with it.
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&ctx)));
    let mut g = shared.lock();
    g.running = None;
    if let Some(j) = g.jobs.iter_mut().find(|j| j.id == id) {
        match r {
            Ok(Ok(out)) => {
                j.state = if cancel.load(Ordering::Relaxed) {
                    JobState::Cancelled
                } else {
                    JobState::Done
                };
                j.message = out.message;
                j.open = out.open;
            }
            Ok(Err(e)) => {
                j.state = if cancel.load(Ordering::Relaxed) {
                    JobState::Cancelled
                } else {
                    JobState::Failed
                };
                j.message = e.to_string();
            }
            Err(_) => {
                j.state = JobState::Failed;
                j.message = "the job stopped unexpectedly".into();
            }
        }
    }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())
}

fn plural(n: usize, what: &str) -> String {
    if n == 1 {
        format!("1 {what}")
    } else {
        format!("{n} {what}s")
    }
}

fn check_files(files: &[PathBuf]) -> Result<()> {
    if files.is_empty() || files.len() > crate::batch::MAX_FILES {
        return Err(invalid("give 1 to 2000 files"));
    }
    Ok(())
}

/// Stapler, one PDF per source, as a job: a step per file (PDFs are skipped), stoppable
/// between files.
pub fn create_each_job(
    files: Vec<PathBuf>,
    out_dir: Option<PathBuf>,
    pictures: crate::docs_more::ImageToPdf,
) -> Result<(usize, Work)> {
    check_files(&files)?;
    if let Some(d) = &out_dir
        && !d.is_dir()
    {
        return Err(invalid(format!("{} is not a folder", d.display())));
    }
    let total = files.len();
    let work: Work = Box::new(move |ctx: &JobCtx| {
        let mut made = 0;
        for (i, f) in files.iter().enumerate() {
            if ctx.cancelled() {
                return Ok(JobOutcome {
                    message: format!("Cancelled after {}", plural(made, "PDF")),
                    open: None,
                });
            }
            let written =
                crate::finish::create::create_each_with(std::slice::from_ref(f), out_dir.as_deref(), &pictures)
                    .map_err(|e| invalid(format!("{}: {e}", file_name(f))))?;
            for w in &written {
                ctx.output(w);
            }
            made += written.len();
            ctx.progress(i + 1, total);
        }
        let place = out_dir
            .as_ref()
            .map_or_else(|| "beside their sources".to_string(), |d| format!("in {}", d.display()));
        Ok(JobOutcome {
            message: format!("Created {} {place}", plural(made, "PDF")),
            open: None,
        })
    });
    Ok((total, work))
}

/// Stapler, every file into one PDF, as a job (one step); the PDF opens when it is done.
pub fn create_combined_job(
    files: Vec<PathBuf>,
    out: PathBuf,
    pictures: crate::docs_more::ImageToPdf,
) -> Result<(usize, Work)> {
    check_files(&files)?;
    let work: Work = Box::new(move |ctx: &JobCtx| {
        if ctx.cancelled() {
            return Ok(JobOutcome {
                message: "Cancelled before it wrote anything".into(),
                open: None,
            });
        }
        let pages = crate::docs_more::create_pdf_from_files_with(&files, &out, &pictures)?;
        ctx.output(&out);
        ctx.progress(1, 1);
        Ok(JobOutcome {
            message: format!("Created {} ({})", out.display(), plural(pages, "page")),
            open: Some(out),
        })
    });
    Ok((1, work))
}

/// Batch Flatten (or Unflatten, restoring markups flattened with recovery) as a job: a step per
/// file, each saved in place.
pub fn flatten_job(files: Vec<PathBuf>, unflatten: bool) -> Result<(usize, Work)> {
    check_files(&files)?;
    let total = files.len();
    let work: Work = Box::new(move |ctx: &JobCtx| {
        let (mut done, mut errors) = (0usize, Vec::new());
        for (i, f) in files.iter().enumerate() {
            if ctx.cancelled() {
                break;
            }
            let r = Session::open(f).and_then(|mut s| {
                let n = if unflatten {
                    s.unflatten(&[])?
                } else {
                    s.flatten_markups(&FlattenFilter::default())?
                };
                if n > 0 || unflatten {
                    s.save(unflatten)?;
                }
                Ok(n)
            });
            match r {
                Ok(n) => {
                    done += n;
                    ctx.output(f);
                }
                Err(e) => errors.push(format!("{}: {e}", file_name(f))),
            }
            ctx.progress(i + 1, total);
        }
        let mut message = format!(
            "{} {}",
            if unflatten { "Unflattened" } else { "Flattened" },
            plural(done, "markup")
        );
        if !errors.is_empty() {
            message.push_str(&format!("; {}", errors.join("; ")));
        }
        Ok(JobOutcome { message, open: None })
    });
    Ok((total, work))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_all(q: &JobQueue) {
        for j in q.list() {
            q.wait(j.id, Duration::from_secs(20));
        }
    }

    #[test]
    fn jobs_run_in_order_off_the_calling_thread_with_progress() {
        let q = JobQueue::new();
        let caller = std::thread::current().id();
        let seen = Arc::new(Mutex::new(Vec::new()));
        for k in 0..3 {
            let seen = seen.clone();
            q.submit(
                &format!("job {k}"),
                2,
                false,
                Box::new(move |ctx: &JobCtx| {
                    assert_ne!(std::thread::current().id(), caller);
                    ctx.progress(1, 2);
                    seen.lock().unwrap().push(k);
                    ctx.progress(2, 2);
                    Ok(JobOutcome {
                        message: format!("ok {k}"),
                        open: None,
                    })
                }),
            );
        }
        wait_all(&q);
        assert_eq!(*seen.lock().unwrap(), vec![0, 1, 2], "one after another, in order");
        let l = q.list();
        assert!(
            l.iter()
                .all(|j| j.state == JobState::Done && j.done == 2 && j.fraction() == 1.0)
        );
        assert_eq!(q.take_finished().len(), 3);
        assert!(q.take_finished().is_empty(), "each finished job is reported once");
        q.clear_finished();
        assert!(q.list().is_empty());
    }

    #[test]
    fn cancel_stops_a_running_job_and_drops_a_queued_one() {
        let q = JobQueue::new();
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let rx = Mutex::new(rx);
        let a = q.submit(
            "long",
            100,
            false,
            Box::new(move |ctx: &JobCtx| {
                for i in 0..100 {
                    if ctx.cancelled() {
                        return Ok(JobOutcome {
                            message: format!("stopped at {i}"),
                            open: None,
                        });
                    }
                    if i == 1 {
                        // Wait for the test to cancel.
                        let _ = rx.lock().unwrap().recv_timeout(Duration::from_secs(10));
                    }
                    ctx.progress(i + 1, 100);
                }
                Ok(JobOutcome::default())
            }),
        );
        let b = q.submit("next", 1, false, Box::new(|_| Ok(JobOutcome::default())));
        // Wait until the first one runs.
        let t = Instant::now();
        while q.get(a).unwrap().state != JobState::Running && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(q.cancel(b), "the queued job is dropped");
        assert!(q.cancel(a), "the running job is asked to stop");
        tx.send(()).unwrap();
        let ja = q.wait(a, Duration::from_secs(10)).unwrap();
        assert_eq!(ja.state, JobState::Cancelled);
        assert!(ja.done < 100, "it stopped early");
        assert_eq!(q.get(b).unwrap().state, JobState::Cancelled);
        assert!(!q.cancel(a), "a finished job cannot be cancelled");
    }

    #[test]
    fn failures_and_panics_are_reported_not_fatal() {
        let q = JobQueue::new();
        let a = q.submit("bad", 1, false, Box::new(|_| Err(invalid("no such file"))));
        #[allow(clippy::panic)]
        let b = q.submit("worse", 1, false, Box::new(|_| panic!("boom")));
        let c = q.submit("fine", 1, true, Box::new(|_| Ok(JobOutcome::default())));
        assert_eq!(q.get(c).unwrap().state, JobState::Done, "inline jobs finish at once");
        assert_eq!(q.wait(a, Duration::from_secs(10)).unwrap().state, JobState::Failed);
        let jb = q.wait(b, Duration::from_secs(10)).unwrap();
        assert_eq!(jb.state, JobState::Failed);
        // The worker survives: a new job still runs.
        let d = q.submit("after", 1, false, Box::new(|_| Ok(JobOutcome::default())));
        assert_eq!(q.wait(d, Duration::from_secs(10)).unwrap().state, JobState::Done);
    }

    #[test]
    fn stapler_jobs_write_each_file() {
        let dir = std::env::temp_dir().join(format!("mc-jobs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let files: Vec<PathBuf> = (0..3)
            .map(|i| {
                let p = dir.join(format!("n{i}.txt"));
                std::fs::write(&p, format!("note {i}")).unwrap();
                p
            })
            .collect();
        let q = JobQueue::new();
        let (total, work) = create_each_job(files.clone(), None, Default::default()).unwrap();
        let id = q.submit("Create", total, false, work);
        let j = q.wait(id, Duration::from_secs(30)).unwrap();
        assert_eq!(j.state, JobState::Done, "{}", j.message);
        assert_eq!(j.outputs.len(), 3);
        assert!(dir.join("n2.pdf").is_file());
        let (total, work) = create_combined_job(files, dir.join("all.pdf"), Default::default()).unwrap();
        let id = q.submit("Create", total, false, work);
        let j = q.wait(id, Duration::from_secs(30)).unwrap();
        assert_eq!(j.open.as_deref(), Some(dir.join("all.pdf").as_path()));
        assert!(create_each_job(Vec::new(), None, Default::default()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
