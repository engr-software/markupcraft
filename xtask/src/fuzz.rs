//! `cargo xtask fuzz`: a mutation fuzzer on stable Rust, adapted from PdfCraft's (MIT OR
//! Apache-2.0).
//!
//! Seeds are small PDFs: the synthetic ones the `fuzz_one` example writes (the sample plan, a
//! set of Revu-style measurements saved by MarkupCraft, a plain page), plus every PDF up to
//! 4 MB in the folder `MARKUPCRAFT_FUZZ_CORPUS` names, when it is set. Each iteration mutates a
//! seed, writes it to `fuzz-out/work/`, and runs `fuzz_one <file> <dir>` in a child process with
//! a timeout: open (model, Markups, renderer), render every page, read text and search, build
//! and export the Markups List, edit, save full and incremental, reopen and list again.
//!
//! A child that dies (a panic, an abort, a stack overflow) is a **crash**, one that runs past
//! the timeout is a **hang**. Crashes are minimized (chunks removed while it still fails the
//! same way) and kept in `fuzz-out/findings/` with a note. `fuzz-out/` is git-ignored; inputs
//! derived from a local corpus are never committed: each real bug becomes a small synthetic
//! regression test instead.
//!
//! ```text
//! cargo xtask fuzz [--time 300] [--iterations N] [--seed 1] [--jobs N] [--timeout 20] [--debug]
//! ```
//! It exits non-zero when it finds a crash (hangs are reported but do not fail the run).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::bench::build_example;
use crate::gates::root;

const CORPUS_ENV: &str = "MARKUPCRAFT_FUZZ_CORPUS";
const MAX_SEED_BYTES: u64 = 4 * 1024 * 1024;

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// xorshift64*: small, deterministic, good enough for mutation choices.
#[derive(Clone)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

/// Bytes that matter to PDF syntax, and tokens worth splicing in (PDF syntax plus the keys
/// MarkupCraft and Revu read on markups).
const SYNTAX: &[u8] = b"0123456789 /<>[]()R\n\r%-.+#\\";
const TOKENS: &[&[u8]] = &[
    b" 0 R",
    b" 99999 0 R",
    b" -1 ",
    b" 2147483648 ",
    b" 1e308 ",
    b" -1e308 ",
    b" NaN ",
    b"<<",
    b">>",
    b"[",
    b"]",
    b"(",
    b")",
    b"[]",
    b"[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[",
    b"stream\n",
    b"\nendstream",
    b"endobj",
    b" obj ",
    b"/Length 0",
    b"/Length 99999999",
    b"/Filter /FlateDecode",
    b"/Kids [1 0 R]",
    b"/Parent 1 0 R",
    b"/Prev 0",
    b"trailer << /Root 1 0 R >>",
    b"/Count -5",
    b"/MediaBox [0 0 0 0]",
    b"/Rotate 45",
    b"/Rect [0 0 0 0]",
    b"/Vertices []",
    b"/Vertices [1e308 1e308 -1e308 0]",
    b"/L [0 0 0 0]",
    b"/QuadPoints [1 2 3]",
    b"/InkList [[]]",
    b"/IT /PolygonDimension",
    b"/IT /PolyLineDimension",
    b"/IT /LineDimension",
    b"/MeasurementTypes 129",
    b"/MeasurementTypes 128",
    b"/Measure << /Type /Measure /Subtype /RL /R (1 in = 0 ft) /X [<< /U (ft) /C 0 >>] >>",
    b"/X [<< /U (ft) /C 1e308 /D 0 /F /F >>]",
    b"/D [<< /U (in) /C -1 /D 99999999 >>]",
    b"/VP [<< /Type /Viewport /BBox [0 0 0 0] >>]",
    b"/CO [1e308 -1e308]",
    b"/LL -1e308",
    b"/PCCutouts [[1 2 3] []]",
    b"/PCColumns [<< /ID (x) /Type /Formula /Formula ([x] * [x]) >>]",
    b"/PCColumnData << /x (1e308) >>",
    b"/PCArcs [[0 0 0]]",
    b"/PCCountSymbol /Nope",
    b"/IRT 1 0 R",
    b"/Popup 1 0 R",
    b"/RC (<body><p><span style=\"font-size:1e308pt\">x</span></p></body>)",
    b"/DS (font: 99999999pt Helvetica)",
    b"/BS << /W -5 /D [0 0] >>",
    b"/C [2 -1]",
    b"/CA -3",
    b"/BE << /S /C /I 99999 >>",
    b"/RD [9999 9999 9999 9999]",
    b"/PageLabels << /Nums [0 << /S /r /St -1 >>] >>",
];

fn mutate(rng: &mut Rng, seed: &[u8], other: &[u8]) -> Vec<u8> {
    let mut d = seed.to_vec();
    let rounds = 1 + rng.below(4);
    for _ in 0..rounds {
        if d.is_empty() {
            d.extend_from_slice(b"%PDF-1.7\n");
        }
        let at = rng.below(d.len());
        let rest = d.len() - at;
        match rng.below(10) {
            0 => {
                if let Some(b) = d.get_mut(at) {
                    *b ^= 1 << rng.below(8);
                }
            }
            1 => {
                let c = SYNTAX.get(rng.below(SYNTAX.len())).copied().unwrap_or(b' ');
                if let Some(b) = d.get_mut(at) {
                    *b = c;
                }
            }
            2 => {
                let n = 1 + rng.below(64.min(rest));
                d.drain(at..at + n);
            }
            3 => {
                let n = 1 + rng.below(256.min(rest));
                let chunk = d[at..at + n].to_vec();
                let to = rng.below(d.len());
                d.splice(to..to, chunk);
            }
            4 | 9 => {
                let t = TOKENS.get(rng.below(TOKENS.len())).copied().unwrap_or(b" ");
                // Most markup keys sit right after a dictionary key: aim at a `/`.
                let to = if rng.below(2) == 0 {
                    d[at..].iter().position(|&c| c == b'/').map_or(at, |p| p + at)
                } else {
                    at
                };
                d.splice(to..to, t.iter().copied());
            }
            5 => d.truncate(at.max(1)),
            6 if !other.is_empty() => {
                let from = rng.below(other.len());
                let n = 1 + rng.below(512.min(other.len() - from));
                d.splice(at..at, other[from..from + n].iter().copied());
            }
            7 => {
                // Replace a run of digits with an extreme number.
                if let Some(start) = d[at..].iter().position(u8::is_ascii_digit).map(|p| p + at) {
                    let end = d[start..]
                        .iter()
                        .position(|c| !c.is_ascii_digit())
                        .map_or(d.len(), |p| p + start);
                    let v: &[u8] = [
                        &b"0"[..],
                        b"4294967295",
                        b"9223372036854775807",
                        b"65535",
                        b"1",
                        b"99999999999999999999",
                    ][rng.below(6)];
                    d.splice(start..end, v.iter().copied());
                }
            }
            _ => {
                // Corrupt compressed data: flip bytes inside a stream body.
                if let Some(s) = find(&d[at..], b"stream").map(|p| p + at + 7) {
                    for _ in 0..1 + rng.below(8) {
                        if s < d.len() {
                            let i = s + rng.below((d.len() - s).min(512));
                            if let Some(b) = d.get_mut(i) {
                                *b = rng.next() as u8;
                            }
                        }
                    }
                }
            }
        }
    }
    d
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn seeds(exe: &Path, out: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let dir = out.join("seeds");
    let status = Command::new(exe)
        .args(["--write-seeds", &dir.to_string_lossy()])
        .status()?;
    if !status.success() {
        bail!("fuzz_one --write-seeds failed ({status})");
    }
    let mut out = Vec::new();
    let mut dirs = vec![(dir, "synthetic")];
    if let Some(c) = std::env::var_os(CORPUS_ENV).filter(|v| !v.is_empty()) {
        dirs.push((PathBuf::from(c), "corpus"));
    }
    for (d, kind) in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
            .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() <= MAX_SEED_BYTES))
            .collect();
        files.sort();
        for (i, f) in files.iter().enumerate() {
            if let Ok(b) = std::fs::read(f) {
                // Corpus seeds are named by number only: their names stay local too.
                let name = if kind == "corpus" {
                    format!("corpus-{i}")
                } else {
                    format!("{kind}-{}", f.file_stem().unwrap_or_default().to_string_lossy())
                };
                out.push((name, b));
            }
        }
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Ok,
    Crash,
    Hang,
}

fn run_one(exe: &Path, file: &Path, work: &Path, timeout: Duration) -> (Outcome, String) {
    let child = Command::new(exe)
        .args([file.as_os_str(), work.as_os_str()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return (Outcome::Ok, String::new());
    };
    // Drain stderr on a thread so a chatty child never blocks on a full pipe.
    let reader = child.stderr.take().map(|mut e| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut s = String::new();
            let _ = e.read_to_string(&mut s);
            s
        })
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let err = reader.and_then(|r| r.join().ok()).unwrap_or_default();
                if status.success() {
                    return (Outcome::Ok, String::new());
                }
                // Keep the first panic location or abort message: it identifies the bug.
                let what = err
                    .lines()
                    .find(|l| {
                        l.contains("panicked at")
                            || l.contains("overflow")
                            || l.contains("fatal")
                            || l.contains("memory allocation")
                    })
                    .unwrap_or("")
                    .trim();
                // Drop the per-process thread id ("thread 'main' (12345)") so runs compare equal.
                let what: String = what
                    .split(" (")
                    .map(|part| match part.split_once(')') {
                        Some((id, rest)) if id.chars().all(|c| c.is_ascii_digit()) => rest.to_string(),
                        _ => format!(" ({part}"),
                    })
                    .collect::<String>()
                    .trim_start_matches(" (")
                    .to_string();
                return (Outcome::Crash, format!("{status}; {what}"));
            }
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return (Outcome::Hang, format!("over {} s", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => return (Outcome::Ok, e.to_string()),
        }
    }
}

/// Remove chunks while the input still fails with the same outcome and message.
fn minimize(exe: &Path, work: &Path, data: Vec<u8>, what: &str, timeout: Duration) -> Vec<u8> {
    let mut best = data;
    let mut chunk = best.len() / 2;
    let mut tries = 0;
    let probe = work.join("minimize.pdf");
    let probe_dir = work.join("minimize");
    while chunk >= 1 && tries < 300 {
        let mut i = 0;
        let mut progressed = false;
        while i < best.len() && tries < 300 {
            let mut cand = best.clone();
            cand.drain(i..(i + chunk).min(cand.len()));
            tries += 1;
            if std::fs::write(&probe, &cand).is_err() {
                break;
            }
            let (o, w) = run_one(exe, &probe, &probe_dir, timeout);
            if o == Outcome::Crash && w == what {
                best = cand;
                progressed = true;
            } else {
                i += chunk;
            }
        }
        if !progressed {
            chunk /= 2;
        }
    }
    let _ = std::fs::remove_file(probe);
    best
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn run(args: &[String]) -> Result<()> {
    let time = Duration::from_secs(flag(args, "--time").unwrap_or("300").parse()?);
    let iterations: usize = flag(args, "--iterations")
        .map(str::parse)
        .transpose()?
        .unwrap_or(usize::MAX);
    let seed: u64 = flag(args, "--seed").unwrap_or("1").parse()?;
    // Half the cores by default: a saturated machine turns slow inputs into false hangs.
    let jobs: usize = flag(args, "--jobs")
        .map(str::parse)
        .transpose()?
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(1)));
    let timeout = Duration::from_secs(flag(args, "--timeout").unwrap_or("20").parse()?);
    let exe = build_example("fuzz_one", args.iter().any(|a| a == "--debug"))?;

    let out = root().join("fuzz-out");
    let work = out.join("work");
    let findings = out.join("findings");
    std::fs::create_dir_all(&work)?;
    std::fs::create_dir_all(&findings)?;
    let seeds = Arc::new(seeds(&exe, &out)?);
    if seeds.is_empty() {
        bail!("no seeds");
    }
    println!(
        "fuzz: {} seeds, {jobs} jobs, {} s, timeout {} s, seed {seed}",
        seeds.len(),
        time.as_secs(),
        timeout.as_secs()
    );

    let done = Arc::new(AtomicUsize::new(0));
    let found: Arc<Mutex<Vec<(Outcome, String, String)>>> = Arc::default();
    let solo = Arc::new(Mutex::new(()));
    let start = Instant::now();
    std::thread::scope(|s| {
        for job in 0..jobs {
            let (seeds, done, found, exe, work, findings, solo) = (
                seeds.clone(),
                done.clone(),
                found.clone(),
                exe.clone(),
                work.clone(),
                findings.clone(),
                solo.clone(),
            );
            s.spawn(move || {
                let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(job as u64 + 1) | 1);
                let file = work.join(format!("job{job}.pdf"));
                let job_dir = work.join(format!("job{job}"));
                while start.elapsed() < time {
                    let n = done.fetch_add(1, Ordering::Relaxed);
                    if n >= iterations {
                        break;
                    }
                    let Some((name, base)) = seeds.get(rng.below(seeds.len())) else { break };
                    let other = seeds.get(rng.below(seeds.len())).map_or(&[][..], |s| &s.1[..]);
                    let data = mutate(&mut rng, base, other);
                    if std::fs::write(&file, &data).is_err() {
                        continue;
                    }
                    let (outcome, what) = run_one(&exe, &file, &job_dir, timeout);
                    if outcome == Outcome::Ok {
                        continue;
                    }
                    let key = |o: Outcome, w: &str, s: &str| {
                        if o == Outcome::Hang {
                            format!("Hang:{s}")
                        } else {
                            format!("{o:?}:{w}")
                        }
                    };
                    let k = key(outcome, &what, name);
                    // Claim the finding before the slow confirm / minimize, so two jobs that
                    // hit the same bug at once record it once.
                    let fresh = found.lock().is_ok_and(|mut f| {
                        let new = !f.iter().any(|(o, w, s)| key(*o, w, s) == k);
                        if new {
                            f.push((outcome, what.clone(), name.clone()));
                        }
                        new
                    });
                    if !fresh {
                        continue;
                    }
                    // A hang under full load may be a slow machine: confirm it alone with three
                    // times the time before recording it. Hangs are not minimized.
                    let small = if outcome == Outcome::Hang {
                        let _guard = solo.lock();
                        if run_one(&exe, &file, &job_dir, timeout * 3).0 != Outcome::Hang {
                            if let Ok(mut f) = found.lock() {
                                f.retain(|(o, w, s)| key(*o, w, s) != k);
                            }
                            continue;
                        }
                        data.clone()
                    } else {
                        minimize(&exe, &work.join(format!("min{job}")), data.clone(), &what, timeout)
                    };
                    let id = format!("{:?}-{:016x}", outcome, fxhash(&small)).to_lowercase();
                    let _ = std::fs::write(findings.join(format!("{id}.pdf")), &small);
                    let note = format!(
                        "{{\n  \"outcome\": \"{outcome:?}\",\n  \"detail\": \"{}\",\n  \"seed\": \"{}\",\n  \"bytes\": {},\n  \"original_bytes\": {}\n}}\n",
                        escape(&what),
                        escape(name),
                        small.len(),
                        data.len()
                    );
                    let _ = std::fs::write(findings.join(format!("{id}.json")), note);
                    println!(
                        "  {outcome:?} from {name}: {what} -> fuzz-out/findings/{id}.pdf ({} bytes)",
                        small.len()
                    );
                }
            });
        }
    });
    let n = done.load(Ordering::Relaxed).min(iterations);
    let found = found.lock().map(|f| f.clone()).unwrap_or_default();
    let crashes = found.iter().filter(|f| f.0 == Outcome::Crash).count();
    let hangs = found.len() - crashes;
    println!(
        "fuzz: {n} runs in {:.0} s ({:.1}/s): {crashes} distinct crash(es), {hangs} hang(s)",
        start.elapsed().as_secs_f64(),
        n as f64 / start.elapsed().as_secs_f64().max(0.001)
    );
    if crashes > 0 {
        bail!("{crashes} crash(es): see fuzz-out/findings/");
    }
    Ok(())
}

fn fxhash(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, x| {
        (h ^ u64::from(*x)).wrapping_mul(0x100_0000_01b3)
    })
}
