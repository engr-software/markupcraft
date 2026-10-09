//! More of File > Print: what prints (document and markups, document only, markups only), a
//! region (Get Window / current view), copies with collation and reverse order, margins and a
//! manual position, emphasis (dimmed page content, dimmed filtered-out markups, Spaces and
//! visible hyperlinks printed), Batch Print, and sending the sheets to a printer.
//!
//! Printing to a printer uses the operating system's own print command on the print-ready PDF
//! (`lp` on macOS and Linux; on Windows the PDF handler's `PrintTo` verb through PowerShell),
//! so it depends on the system's PDF printing support. The print-ready PDF is always what is
//! printed, so the result can be checked by writing it to a file instead.

use std::path::{Path, PathBuf};

use markupcraft_model::{Color, Kind, Markup, Rect};
use markupcraft_revu::cos::{Object, Stream};

use crate::archive::ColorMode;
use crate::docutil::page_objs;
use crate::printout::PrintSettings;
use crate::{Result, Session, invalid};

/// What a print job holds besides the layout ([`PrintSettings`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PrintJob {
    pub settings: PrintSettings,
    /// Print only the markups (the page content left out).
    pub markups_only: bool,
    /// Only this rectangle of this page (Get Window, or the current view).
    pub region: Option<(usize, Rect)>,
    /// Copies (1 to 999), collated or each page repeated, in reverse order.
    pub copies: usize,
    pub collate: bool,
    pub reverse: bool,
    /// Margin on every side of the sheet, points (fit/reduce to margins).
    pub margin: f64,
    /// Move the printed area from the centre by this much, points (manual position).
    pub offset: (f64, f64),
    /// Emphasis: page content faded toward white.
    pub dim_content: bool,
    /// Emphasis: markups not in this list faded (the filtered-out ones).
    pub dim_except: Option<Vec<String>>,
    /// Emphasis: Space outlines and visible hyperlink boxes printed.
    pub spaces: bool,
    pub links: bool,
}

impl Default for PrintJob {
    fn default() -> Self {
        Self {
            settings: PrintSettings::default(),
            markups_only: false,
            region: None,
            copies: 1,
            collate: true,
            reverse: false,
            margin: 0.0,
            offset: (0.0, 0.0),
            dim_content: false,
            dim_except: None,
            spaces: false,
            links: false,
        }
    }
}

/// The page order a job prints: `pages` (empty = all of `count`), reversed when asked, times
/// the copies, collated or not.
pub fn print_order(pages: &[usize], count: usize, copies: usize, collate: bool, reverse: bool) -> Vec<usize> {
    let mut base: Vec<usize> = if pages.is_empty() {
        (0..count).collect()
    } else {
        pages.to_vec()
    };
    if reverse {
        base.reverse();
    }
    let copies = copies.clamp(1, 999);
    if collate {
        (0..copies).flat_map(|_| base.iter().copied()).collect()
    } else {
        base.iter().flat_map(|p| std::iter::repeat_n(*p, copies)).collect()
    }
}

impl Session {
    /// A print job written as a print-ready PDF at `out`. Returns the number of sheets.
    pub fn print_job_to_pdf(&self, out: &Path, job: &PrintJob) -> Result<usize> {
        if !(job.margin.is_finite() && (0.0..=144.0).contains(&job.margin)) {
            return Err(invalid("the margin is 0 to 144 points"));
        }
        if !(job.offset.0.is_finite()
            && job.offset.1.is_finite()
            && job.offset.0.abs() <= 1_000.0
            && job.offset.1.abs() <= 1_000.0)
        {
            return Err(invalid("the position offset is at most 1000 points"));
        }
        if !(1..=999).contains(&job.copies) {
            return Err(invalid("copies are 1 to 999"));
        }
        for p in &job.settings.pages {
            self.page(*p)?;
        }
        // A copy to prepare: emphasis, markups only, the region.
        let mut t = Session::from_bytes(self.current_bytes()?.as_ref().clone(), self.path())?;
        let mut extra: Vec<Markup> = Vec::new();
        if job.spaces {
            for (page, sp) in self.spaces(None) {
                if sp.pts.len() >= 3 {
                    let mut m = Markup::new(Kind::Polygon, page, sp.pts.clone());
                    m.color = sp.color;
                    m.line_width = 2.0;
                    m.subject = format!("Space {}", sp.name);
                    extra.push(m);
                }
            }
        }
        if job.links {
            for l in self.links() {
                let mut m = Markup::new(Kind::Rectangle, l.page, l.rect.corners().to_vec());
                m.color = Color::rgb(0.0, 0.3, 1.0);
                m.line_width = 1.0;
                m.subject = "Hyperlink".into();
                extra.push(m);
            }
        }
        if !extra.is_empty() {
            t.add_new_markups("Print emphasis", extra)?;
        }
        if let Some(keep) = &job.dim_except {
            let ids: Vec<String> = t
                .doc()
                .markups
                .iter()
                .filter(|m| !keep.contains(&m.id))
                .map(|m| m.id.clone())
                .collect();
            t.edit("Dim markups", |s| {
                for m in s.doc.markups.iter_mut().filter(|m| ids.contains(&m.id)) {
                    m.opacity = (m.opacity * 0.3).clamp(0.05, 1.0);
                    m.fill_opacity = (m.fill_opacity * 0.3).clamp(0.0, 1.0);
                    m.dirty = true;
                }
                Ok(((), !ids.is_empty()))
            })?;
        }
        if job.dim_content && !job.markups_only {
            t.color_process(&[], ColorMode::Lighten(0.6))?;
        }
        if job.markups_only {
            t.graph_edit("Markups only", |cos, _| {
                for p in page_objs(cos)? {
                    let empty = cos.add(Object::Stream(Stream::from_raw(Default::default(), Vec::new())));
                    cos.update_dict(p, |d| d.set(b"Contents".to_vec(), Object::Ref(empty)))?;
                }
                Ok(())
            })?;
        }
        let mut settings = job.settings.clone();
        if let Some((page, r)) = job.region {
            t.page(page)?;
            let r = r.normalized();
            if !(r.width() >= 1.0 && r.height() >= 1.0) {
                return Err(invalid("drag a region to print"));
            }
            t.graph_edit("Print region", |cos, _| {
                let pages = page_objs(cos)?;
                let pr = *pages.get(page).ok_or_else(|| invalid("no such page"))?;
                let arr = Object::Array(r.as_array().iter().map(|v| Object::Real(*v)).collect());
                cos.update_dict(pr, |d| {
                    d.set(b"CropBox".to_vec(), arr.clone());
                    d.set(b"MediaBox".to_vec(), arr);
                })?;
                Ok(())
            })?;
            settings.pages = vec![page];
        }
        settings.pages = print_order(&settings.pages, t.page_count(), job.copies, job.collate, job.reverse);
        // Margins: lay out on the paper less the margins, then give the sheet its margins back.
        let (pw, ph) = settings.paper;
        let m = job.margin;
        if m > 0.0 {
            settings.paper = (pw - 2.0 * m, ph - 2.0 * m);
        }
        let n = t.print_to_pdf(out, &settings)?;
        if m > 0.0 || job.offset != (0.0, 0.0) {
            let mut s = Session::open(out)?;
            let (dx, dy) = job.offset;
            s.graph_edit("Margins", |cos, _| {
                for p in page_objs(cos)? {
                    let mb = cos
                        .dict(&Object::Ref(p))
                        .and_then(|d| d.get(b"MediaBox").map(|b| cos.resolve(b)))
                        .and_then(|b| b.as_array().cloned())
                        .unwrap_or_default();
                    let v: Vec<f64> = mb.iter().filter_map(|x| cos.resolve(x).as_f64()).collect();
                    let [x0, y0, x1, y1] = match v.as_slice() {
                        [a, b, c, d] => [*a, *b, *c, *d],
                        _ => continue,
                    };
                    let nb = [x0 - m - dx, y0 - m - dy, x1 + m - dx, y1 + m - dy];
                    let arr = Object::Array(nb.iter().map(|x| Object::Real(*x)).collect());
                    cos.update_dict(p, |d| {
                        d.set(b"MediaBox".to_vec(), arr.clone());
                        d.remove(b"CropBox");
                    })?;
                }
                Ok(())
            })?;
            s.save_as(out, true)?;
        }
        Ok(n)
    }
}

/// The command that prints `pdf` on `printer` (`None` = the default printer) `copies` times:
/// program and arguments.
pub fn print_command(pdf: &Path, printer: Option<&str>, copies: usize) -> (String, Vec<String>) {
    let copies = copies.clamp(1, 999);
    if cfg!(windows) {
        // PowerShell quoting: single quotes, doubled inside.
        let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
        let file = q(&pdf.display().to_string());
        let verb = match printer {
            Some(p) => format!("-Verb PrintTo -ArgumentList {}", q(&format!("\"{p}\""))),
            None => "-Verb Print".to_string(),
        };
        let one = format!("Start-Process -FilePath {file} {verb} -WindowStyle Hidden");
        let script = (0..copies).map(|_| one.clone()).collect::<Vec<_>>().join("; ");
        (
            "powershell".into(),
            vec!["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), script],
        )
    } else {
        let mut args = Vec::new();
        if let Some(p) = printer {
            args.push("-d".into());
            args.push(p.to_string());
        }
        args.push("-n".into());
        args.push(copies.to_string());
        args.push(pdf.display().to_string());
        ("lp".into(), args)
    }
}

/// The printers the system knows (empty when it cannot say).
pub fn list_printers() -> Vec<String> {
    let out = if cfg!(windows) {
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-CimInstance Win32_Printer | ForEach-Object { $_.Name }",
            ])
            .output()
    } else {
        std::process::Command::new("lpstat").arg("-e").output()
    };
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .take(500)
            .collect(),
        _ => Vec::new(),
    }
}

/// Send a print-ready PDF to a printer with the system's print command. The command runs in
/// the background; an error is returned when it cannot start.
pub fn send_to_printer(pdf: &Path, printer: Option<&str>, copies: usize) -> Result<()> {
    if !pdf.is_file() {
        return Err(invalid(format!("{} is not a file", pdf.display())));
    }
    let (prog, args) = print_command(pdf, printer, copies);
    std::process::Command::new(&prog)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| invalid(format!("the print command ({prog}) could not start: {e}")))
}

/// Batch Print: every file printed with one job, in list order, each written as a
/// print-ready PDF in `out_dir` (`<name>_print.pdf`) and, with `printer`, sent to it.
/// Returns (file, sheets or error) per file.
pub fn batch_print(
    files: &[PathBuf],
    job: &PrintJob,
    out_dir: &Path,
    printer: Option<Option<&str>>,
) -> Result<Vec<(PathBuf, std::result::Result<usize, String>)>> {
    if !out_dir.is_dir() {
        return Err(invalid(format!("{} is not a folder", out_dir.display())));
    }
    if files.is_empty() || files.len() > crate::batch::MAX_FILES {
        return Err(invalid("print 1 to 2000 files"));
    }
    let mut out = Vec::new();
    for f in files {
        let stem = f
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());
        let target = out_dir.join(format!("{stem}_print.pdf"));
        let r = Session::open(f)
            .and_then(|s| {
                let mut j = job.clone();
                // Page lists are per document; a batch prints every page.
                j.settings.pages.clear();
                j.region = None;
                s.print_job_to_pdf(&target, &j)
            })
            .and_then(|n| {
                if let Some(p) = printer {
                    send_to_printer(&target, p, job.copies)?;
                }
                Ok(n)
            })
            .map_err(|e| e.to_string());
        out.push((target, r));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, rect};

    #[test]
    fn order_copies_collate_reverse() {
        assert_eq!(print_order(&[], 3, 2, true, false), vec![0, 1, 2, 0, 1, 2]);
        assert_eq!(print_order(&[0, 2], 3, 2, false, false), vec![0, 0, 2, 2]);
        assert_eq!(print_order(&[], 3, 1, true, true), vec![2, 1, 0]);
    }

    #[test]
    fn commands_name_the_printer_and_copies() {
        let (prog, args) = print_command(Path::new("a b.pdf"), Some("Plotter 1"), 2);
        let all = args.join(" ");
        if cfg!(windows) {
            assert_eq!(prog, "powershell");
            assert!(
                all.contains("PrintTo") && all.contains("Plotter 1") && all.matches("Start-Process").count() == 2,
                "{all}"
            );
        } else {
            assert_eq!(prog, "lp");
            assert_eq!(args, vec!["-d", "Plotter 1", "-n", "2", "a b.pdf"]);
        }
        assert!(send_to_printer(Path::new("does-not-exist.pdf"), None, 1).is_err());
    }

    #[test]
    fn jobs_print_regions_markups_only_margins_and_emphasis() {
        let d = std::env::temp_dir().join(format!("markupcraft-printing-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let mut s = Session::from_bytes(
            pdf(&[
                SyntheticPage::new(612.0, 792.0, rect(100.0, 100.0, 200.0, 200.0)),
                SyntheticPage::new(612.0, 792.0, ""),
            ]),
            d.join("p.pdf"),
        )
        .unwrap();
        let m = Markup::new(
            Kind::Rectangle,
            0,
            Rect::new(400.0, 400.0, 500.0, 500.0).corners().to_vec(),
        );
        s.add_markup(m).unwrap();
        let out = d.join("copies.pdf");
        let job = PrintJob {
            copies: 3,
            collate: false,
            ..Default::default()
        };
        assert_eq!(s.print_job_to_pdf(&out, &job).unwrap(), 6);
        // Markups only: the black square is gone, the markup stays.
        let mo = d.join("markups.pdf");
        let job = PrintJob {
            markups_only: true,
            settings: PrintSettings {
                pages: vec![0],
                layout: crate::printout::PrintLayout::ActualSize,
                ..Default::default()
            },
            ..Default::default()
        };
        s.print_job_to_pdf(&mo, &job).unwrap();
        let img = Session::open(&mo)
            .unwrap()
            .renderable(false)
            .unwrap()
            .render_rgba(0, 0.5)
            .unwrap();
        assert!(img.pixel(200.0, 200.0)[0] > 240, "page content left out");
        // A region with a margin: one sheet.
        let ro = d.join("region.pdf");
        let job = PrintJob {
            region: Some((0, Rect::new(50.0, 50.0, 350.0, 350.0))),
            margin: 36.0,
            dim_content: true,
            spaces: true,
            links: true,
            dim_except: Some(Vec::new()),
            ..Default::default()
        };
        assert_eq!(s.print_job_to_pdf(&ro, &job).unwrap(), 1);
        let r = Session::open(&ro).unwrap();
        let mb = r.page(0).unwrap().media;
        assert!((mb.width() - 612.0).abs() < 1.0, "the full paper: {mb:?}");
        assert!(
            s.print_job_to_pdf(
                &ro,
                &PrintJob {
                    copies: 0,
                    ..Default::default()
                }
            )
            .is_err()
        );
        // Batch.
        let f = d.join("p.pdf");
        s.save_as(&f, true).unwrap();
        let res = batch_print(&[f], &PrintJob::default(), &d, None).unwrap();
        assert_eq!(res[0].1, Ok(2));
    }
}
