//! Flatten's extras: overlay text written on every flattened page (font, size, position), the
//! chosen properties of each flattened markup kept in a pop-up note at its place, and the
//! captured files of flattened File Attachment markups turned into a capture summary attached
//! to the document.

use markupcraft_model::{Kind, Markup, Point};
use markupcraft_revu::cos::{Dict, Object};

use crate::docutil::page_objs;
use crate::flatten::{FlattenFilter, FlattenOptions};
use crate::{Result, Session, invalid};

/// Where overlay text goes on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayPos {
    TopLeft,
    #[default]
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl OverlayPos {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "top_left" => Self::TopLeft,
            "top_center" => Self::TopCenter,
            "top_right" => Self::TopRight,
            "bottom_left" => Self::BottomLeft,
            "bottom_center" => Self::BottomCenter,
            "bottom_right" => Self::BottomRight,
            _ => return None,
        })
    }
}

/// Flatten's extras.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlattenExtras {
    /// Overlay text on each flattened page (empty = none).
    pub overlay: String,
    /// Helvetica, Times-Roman or Courier.
    pub font: String,
    pub size: f64,
    pub position: OverlayPos,
    /// Properties kept in a pop-up note per flattened markup: subject, author, comments,
    /// label, status, date.
    pub keep: Vec<String>,
    /// Attach a capture summary (CSV) of the flattened File Attachment markups.
    pub capture_summary: bool,
}

fn picks(f: &FlattenFilter, m: &Markup) -> bool {
    let any = |list: &[String], v: &str| list.is_empty() || list.iter().any(|x| x.eq_ignore_ascii_case(v));
    (f.ids.is_empty() || f.ids.contains(&m.id))
        && (f.pages.is_empty() || f.pages.contains(&m.page))
        && any(&f.kinds, m.kind.name())
        && any(&f.layers, &m.layer)
        && any(&f.authors, &m.author)
}

fn keep_text(m: &Markup, keep: &[String]) -> String {
    let mut lines = Vec::new();
    for k in keep {
        let v = match k.as_str() {
            "subject" => m.subject.clone(),
            "author" => m.author.clone(),
            "comments" => m.contents.clone(),
            "label" => m.label.clone(),
            "status" => m.status.clone(),
            "date" => m.modified.clone(),
            _ => continue,
        };
        if !v.trim().is_empty() {
            let name = k
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
                .into_iter()
                .chain(k.chars().skip(1));
            lines.push(format!("{}: {v}", name.collect::<String>()));
        }
    }
    lines.join("\n")
}

fn esc(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '(' | ')' | '\\' => format!("\\{c}"),
            c if (c as u32) < 128 => c.to_string(),
            _ => "?".to_string(),
        })
        .collect()
}

impl Session {
    /// Flatten `filter`'s markups with `opts`, and the extras. Returns how many were flattened.
    /// One undo step per part.
    pub fn flatten_with_extras(
        &mut self,
        filter: &FlattenFilter,
        opts: &FlattenOptions,
        x: &FlattenExtras,
    ) -> Result<usize> {
        let font = match x.font.as_str() {
            "" | "Helvetica" => "Helvetica",
            "Times-Roman" | "Times" => "Times-Roman",
            "Courier" => "Courier",
            o => {
                return Err(invalid(format!(
                    "overlay font: Helvetica, Times-Roman or Courier (got {o})"
                )));
            }
        };
        if !x.overlay.is_empty() && !(x.size.is_finite() && (4.0..=144.0).contains(&x.size)) {
            return Err(invalid("overlay text size is 4 to 144"));
        }
        let chosen: Vec<Markup> = self.doc.markups.iter().filter(|m| picks(filter, m)).cloned().collect();
        let mut pages: Vec<usize> = chosen.iter().map(|m| m.page).collect();
        pages.sort_unstable();
        pages.dedup();
        // the capture summary is read before the attachments are flattened away
        let summary = if x.capture_summary && chosen.iter().any(|m| m.kind == Kind::Attachment) {
            let ids: Vec<&str> = chosen.iter().map(|m| m.id.as_str()).collect();
            let csv = self.capture_summary_csv();
            let mut out = String::new();
            for (i, line) in csv.lines().enumerate() {
                let keep = i == 0
                    || self
                        .attachment_markups()
                        .iter()
                        .any(|a| ids.contains(&a.id.as_str()) && line.contains(&a.file));
                if keep {
                    out.push_str(line);
                    out.push_str("\r\n");
                }
            }
            Some(out)
        } else {
            None
        };
        let n = self.flatten_markups_with(filter, opts)?;
        if !x.keep.is_empty() {
            for m in &chosen {
                let text = keep_text(m, &x.keep);
                if text.is_empty() {
                    continue;
                }
                let at = m.rect.normalized();
                let mut note = Markup::new(Kind::Note, m.page, vec![Point::new(at.x0, at.y1 - 20.0)]);
                note.contents = text;
                note.subject = "Flattened markup".into();
                note.color = m.color;
                self.add_markup(note)?;
            }
        }
        if !x.overlay.trim().is_empty() {
            let size = x.size;
            let text = esc(&x.overlay);
            let pos = x.position;
            self.cos_edit("Flatten Overlay Text", |cos| {
                let objs = page_objs(cos)?;
                for p in &pages {
                    let Some(pref) = objs.get(*p).copied() else { continue };
                    let pd = cos.dict(&Object::Ref(pref)).unwrap_or_default();
                    let media = pd
                        .get(b"MediaBox")
                        .and_then(|b| cos.resolve(b).as_array().cloned())
                        .map(|a| a.iter().filter_map(|v| cos.resolve(v).as_f64()).collect::<Vec<f64>>())
                        .filter(|v| v.len() == 4)
                        .unwrap_or_else(|| vec![0.0, 0.0, 612.0, 792.0]);
                    let (x0, y0, x1, y1) = (media[0], media[1], media[2], media[3]);
                    let w = 0.55 * size * text.chars().count() as f64;
                    let tx = match pos {
                        OverlayPos::TopLeft | OverlayPos::BottomLeft => x0 + 36.0,
                        OverlayPos::TopCenter | OverlayPos::BottomCenter => (x0 + x1 - w) / 2.0,
                        OverlayPos::TopRight | OverlayPos::BottomRight => x1 - 36.0 - w,
                    };
                    let ty = match pos {
                        OverlayPos::TopLeft | OverlayPos::TopCenter | OverlayPos::TopRight => y1 - 24.0 - size,
                        _ => y0 + 24.0,
                    };
                    // a standard font in the page's resources
                    let mut res = pd.get(b"Resources").and_then(|r| cos.dict(r)).unwrap_or_default();
                    let mut fonts = res.get(b"Font").and_then(|f| cos.dict(f)).unwrap_or_default();
                    let mut fd = Dict::new();
                    fd.set(b"Type".to_vec(), Object::Name(b"Font".to_vec()));
                    fd.set(b"Subtype".to_vec(), Object::Name(b"Type1".to_vec()));
                    fd.set(b"BaseFont".to_vec(), Object::Name(font.as_bytes().to_vec()));
                    fd.set(b"Encoding".to_vec(), Object::Name(b"WinAnsiEncoding".to_vec()));
                    let fr = cos.add(Object::Dict(fd));
                    fonts.set(b"PCOverlay".to_vec(), Object::Ref(fr));
                    res.set(b"Font".to_vec(), Object::Dict(fonts));
                    cos.update_dict(pref, |d| d.set(b"Resources".to_vec(), Object::Dict(res)))?;
                    let post = format!("q BT /PCOverlay {size:.2} Tf 0 g {tx:.2} {ty:.2} Td ({text}) Tj ET Q\n");
                    super::pages::wrap_content(cos, pref, "", &post)?;
                }
                Ok(((), true))
            })?;
        }
        if let Some(csv) = summary {
            let p = std::env::temp_dir().join(format!("markupcraft-capture-{}.csv", std::process::id()));
            crate::write_atomic(&p, csv.as_bytes())?;
            let r = self.add_attachment(
                &p,
                Some("Capture Summary.csv"),
                "Files captured by the flattened markups",
            );
            let _ = std::fs::remove_file(&p);
            r?;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf};

    #[test]
    fn overlay_kept_properties_and_capture_summary() {
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, String::new())]), "f.pdf").unwrap();
        let mut r = Markup::new(
            Kind::Rectangle,
            0,
            vec![Point::new(100.0, 100.0), Point::new(200.0, 200.0)],
        );
        r.subject = "Hold".into();
        r.contents = "Check with the engineer".into();
        s.add_markup(r).unwrap();
        let file = std::env::temp_dir().join(format!("mc-cap-{}.txt", std::process::id()));
        std::fs::write(&file, b"site notes").unwrap();
        s.add_file_attachment(
            0,
            Point::new(300.0, 300.0),
            &file,
            crate::capture::AttachIcon::Paperclip,
            "notes",
        )
        .unwrap();
        let x = FlattenExtras {
            overlay: "FLATTENED FOR ISSUE".into(),
            font: "Courier".into(),
            size: 10.0,
            position: OverlayPos::BottomRight,
            keep: vec!["subject".into(), "comments".into()],
            capture_summary: true,
        };
        let n = s
            .flatten_with_extras(&FlattenFilter::default(), &FlattenOptions::default(), &x)
            .unwrap();
        assert_eq!(n, 2);
        assert!(s.page_text(0).unwrap().contains("FLATTENED FOR ISSUE"));
        let notes: Vec<&Markup> = s.doc().markups.iter().filter(|m| m.kind == Kind::Note).collect();
        assert!(
            notes
                .iter()
                .any(|n| n.contents.contains("Subject: Hold") && n.contents.contains("Comments: Check")),
            "{notes:?}"
        );
        let att = s.attachments();
        assert!(att.iter().any(|a| a.name == "Capture Summary.csv"), "{att:?}");
        let csv = String::from_utf8(s.attachment_data("Capture Summary.csv").unwrap()).unwrap();
        assert!(csv.contains("mc-cap-"), "{csv}");
        let _ = std::fs::remove_file(file);
        assert!(
            s.flatten_with_extras(
                &FlattenFilter::default(),
                &FlattenOptions::default(),
                &FlattenExtras {
                    font: "Comic".into(),
                    ..x
                }
            )
            .is_err()
        );
    }
}
