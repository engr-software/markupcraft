//! Redaction: mark areas or search hits for redaction, apply the marks (the content under them
//! is removed for good), and verify that nothing readable is left under them.
//!
//! Marks are standard Redact annotations (ISO 32000-2 §12.5.6.23) made with `pdfcraft-annot`;
//! applying is PdfCraft's `pdfcraft-redact`, which removes text glyphs, images and paths under
//! the marks (and comments, links and fields overlapping them), draws the fill boxes, and
//! re-reads every redacted page, failing (and changing nothing) if anything is still found.
//! After applying, the next save rewrites the whole file so no earlier revision keeps the
//! removed content.

use markupcraft_geom::Rect;
use markupcraft_model::Color;
use pdfcraft_annot::{Meta, NewAnnotation, OverlayLook, Shape, Style, add_annotation, rect_quad};

use crate::raster::Renderable;
use crate::{Result, Session, invalid};

/// Most marks one call may add.
pub const MAX_MARKS: usize = 10_000;

/// A redaction mark in the document.
#[derive(Debug, Clone, PartialEq)]
pub struct RedactMark {
    pub page: usize,
    pub rects: Vec<Rect>,
    pub overlay: String,
    pub fill: Option<Color>,
}

/// What applying removed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RedactReport {
    pub marks: usize,
    pub pages: usize,
    pub glyphs: usize,
    pub images: usize,
    pub paths: usize,
    pub annotations: usize,
    pub fields: usize,
    /// Text still readable under the applied marks (none when the redaction is complete).
    pub residue: Vec<String>,
}

/// How a new mark looks.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkStyle {
    /// Box colour once applied; `None` leaves no box.
    pub fill: Option<Color>,
    /// Outline while marked.
    pub outline: Color,
    /// Text drawn on the box once applied.
    pub overlay: String,
    /// Overlay text font (Helvetica, Times, Courier), size (0 = fit the box), colour,
    /// alignment (0 left, 1 centre, 2 right) and whether it repeats to fill the box.
    pub font: String,
    pub font_size: f64,
    pub text_color: Color,
    pub align: u8,
    pub repeat: bool,
}

/// Redaction codes for the overlay text: the US FOIA exemptions and DOD codes.
pub const REDACTION_CODES: &[(&str, &str)] = &[
    (
        "(b)(1)",
        "FOIA: classified national defense or foreign relations information",
    ),
    ("(b)(2)", "FOIA: internal agency rules and practices"),
    ("(b)(3)", "FOIA: information exempted by other laws"),
    ("(b)(4)", "FOIA: trade secrets and confidential business information"),
    ("(b)(5)", "FOIA: inter- or intra-agency communications"),
    ("(b)(6)", "FOIA: personal privacy"),
    ("(b)(7)(A)", "FOIA: law enforcement: interfere with proceedings"),
    ("(b)(7)(C)", "FOIA: law enforcement: personal privacy"),
    ("(b)(8)", "FOIA: financial institution supervision"),
    ("(b)(9)", "FOIA: geological and geophysical information"),
    ("(b)(3):10 USC 130", "DOD: critical infrastructure security information"),
    (
        "(b)(3):10 USC 424",
        "DOD: organization and functions of defense intelligence agencies",
    ),
];

impl Default for MarkStyle {
    fn default() -> Self {
        Self {
            fill: Some(Color::BLACK),
            outline: Color::rgb(0.89, 0.13, 0.13),
            overlay: String::new(),
            font: "Helvetica".into(),
            font_size: 0.0,
            text_color: Color::rgb(1.0, 0.0, 0.0),
            align: 1,
            repeat: false,
        }
    }
}

impl MarkStyle {
    fn look(&self) -> OverlayLook {
        let font = match self.font.to_ascii_lowercase().as_str() {
            "times" | "times-roman" => pdfcraft_annot::OverlayFont::Times,
            "courier" => pdfcraft_annot::OverlayFont::Courier,
            _ => pdfcraft_annot::OverlayFont::Helvetica,
        };
        OverlayLook {
            font,
            size: if self.font_size.is_finite() {
                self.font_size.clamp(0.0, 200.0)
            } else {
                0.0
            },
            color: rgb(self.text_color),
            align: self.align.min(2),
            repeat: self.repeat,
        }
    }
}

fn rgb(c: Color) -> [f64; 3] {
    [c.r.clamp(0.0, 1.0), c.g.clamp(0.0, 1.0), c.b.clamp(0.0, 1.0)]
}

impl Session {
    /// Mark rectangles of `page` for redaction (one mark per rectangle). Undoable.
    pub fn redact_mark(&mut self, page: usize, rects: &[Rect], style: &MarkStyle) -> Result<usize> {
        self.redact_mark_many(&rects.iter().map(|r| (page, vec![*r])).collect::<Vec<_>>(), style)
    }

    /// Mark several areas: each `(page, rects)` becomes one mark. Undoable as one step.
    pub fn redact_mark_many(&mut self, marks: &[(usize, Vec<Rect>)], style: &MarkStyle) -> Result<usize> {
        if marks.is_empty() {
            return Err(invalid("nothing to mark"));
        }
        if marks.len() > MAX_MARKS {
            return Err(invalid(format!("at most {MAX_MARKS} marks at once")));
        }
        if style.overlay.chars().count() > 1000 {
            return Err(invalid("overlay text is too long"));
        }
        for (page, rects) in marks {
            self.page(*page)?;
            if rects.is_empty() {
                return Err(invalid("a mark needs at least one rectangle"));
            }
            for r in rects {
                let r = r.normalized();
                if ![r.x0, r.y0, r.x1, r.y1]
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= crate::geometry::MAX_COORD)
                    || r.width() <= 0.0
                    || r.height() <= 0.0
                {
                    return Err(invalid("redaction rectangles must be finite with a width and a height"));
                }
            }
        }
        let author = self.author.clone();
        self.edit("Mark for Redaction", |s| {
            let vp = std::mem::take(&mut s.vp_changed);
            Session::flush(&mut s.file.cos, &mut s.doc, &vp);
            for (page, rects) in marks {
                let shape = Shape::Redact {
                    quads: rects
                        .iter()
                        .map(|r| {
                            let r = r.normalized();
                            rect_quad([r.x0, r.y0, r.x1, r.y1])
                        })
                        .collect(),
                    overlay: style.overlay.clone(),
                    look: style.look(),
                };
                let mut st = Style::default_for(&shape);
                st.fill = style.fill.map(rgb);
                st.color = rgb(style.outline);
                let meta = Meta {
                    date: Some(markupcraft_revu::pdf_date_now()),
                    id: markupcraft_revu::new_markup_id(),
                };
                let new = NewAnnotation {
                    page: *page,
                    shape,
                    style: st,
                    contents: String::new(),
                    author: author.clone(),
                };
                add_annotation(&mut s.file.cos, &new, &meta).map_err(|e| invalid(e.to_string()))?;
            }
            s.reload();
            Ok(((), true))
        })?;
        Ok(marks.len())
    }

    /// Mark every occurrence of `needle` in the page text (pages: 0-based, empty = all) for
    /// redaction. Returns how many were marked.
    pub fn redact_search(
        &mut self,
        needle: &str,
        case_sensitive: bool,
        whole_words: bool,
        pages: &[usize],
        style: &MarkStyle,
    ) -> Result<usize> {
        if needle.trim().is_empty() {
            return Err(invalid("give the text to find"));
        }
        let doc = self.renderable(true)?;
        let pages: Vec<usize> = if pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            pages.to_vec()
        };
        let mut marks = Vec::new();
        for page in pages {
            self.page(page)?;
            let geom = doc.geom(page)?.clone();
            let Some(text) = doc.text(page) else { continue };
            for hit in text.find_opts(needle, case_sensitive, whole_words) {
                let rects: Vec<Rect> = text
                    .line_rects(hit)
                    .into_iter()
                    .map(|r| {
                        let a = geom.view_to_user(r[0], r[1]);
                        let b = geom.view_to_user(r[2], r[3]);
                        Rect::new(
                            a[0].min(b[0]) as f64,
                            a[1].min(b[1]) as f64,
                            a[0].max(b[0]) as f64,
                            a[1].max(b[1]) as f64,
                        )
                        .padded(0.5)
                    })
                    .collect();
                if !rects.is_empty() {
                    marks.push((page, rects));
                }
            }
        }
        if marks.is_empty() {
            return Ok(0);
        }
        self.redact_mark_many(&marks, style)
    }

    /// The redaction marks in the document.
    pub fn redact_marks(&self) -> Vec<RedactMark> {
        pdfcraft_redact::marks(&self.file.cos)
            .into_iter()
            .map(|m| RedactMark {
                page: m.page,
                rects: m.rects.iter().map(|r| Rect::new(r[0], r[1], r[2], r[3])).collect(),
                overlay: m.overlay,
                fill: m.fill.map(|c| Color::rgb(c[0], c[1], c[2])),
            })
            .collect()
    }

    /// Apply the redaction marks (on `pages`, or all): remove what is under them, draw their
    /// boxes, then check that no text is readable under them. Undoable until saved; the next
    /// save rewrites the whole file.
    pub fn redact_apply(&mut self, pages: Option<&[usize]>) -> Result<RedactReport> {
        if let Some(p) = pages {
            for page in p {
                self.page(*page)?;
            }
        }
        let marks: Vec<RedactMark> = self
            .redact_marks()
            .into_iter()
            .filter(|m| pages.is_none_or(|p| p.contains(&m.page)))
            .collect();
        let report = self.edit("Apply Redactions", |s| {
            let vp = std::mem::take(&mut s.vp_changed);
            Session::flush(&mut s.file.cos, &mut s.doc, &vp);
            let r = pdfcraft_redact::apply(&mut s.file.cos, pages).map_err(|e| invalid(e.to_string()))?;
            s.file.cos.require_full_save();
            s.reload();
            Ok((
                RedactReport {
                    marks: r.marks,
                    pages: r.pages,
                    glyphs: r.glyphs,
                    images: r.images_removed + r.images_cleared,
                    paths: r.paths_removed + r.paths_clipped,
                    annotations: r.annotations,
                    fields: r.fields,
                    residue: Vec::new(),
                },
                true,
            ))
        })?;
        let residue = self.redact_residue(&marks)?;
        Ok(RedactReport { residue, ..report })
    }

    /// Apply redactions and, with `scrub_metadata`, remove the document's metadata too (Info,
    /// XMP and other applications' private data), as Revu's Apply Redactions offers.
    pub fn redact_apply_with(&mut self, pages: Option<&[usize]>, scrub_metadata: bool) -> Result<RedactReport> {
        let r = self.redact_apply(pages)?;
        if scrub_metadata {
            self.cos_edit("Remove Metadata", |cos| {
                if let Some(root) = cos.root() {
                    cos.update_dict(root, |d| {
                        d.remove(b"Metadata");
                        d.remove(b"PieceInfo");
                    })?;
                }
                cos.trailer_mut().remove(b"Info");
                cos.require_full_save();
                Ok(((), true))
            })?;
        }
        Ok(r)
    }

    /// Text that can still be read under `marks`, apart from their own overlay text (empty
    /// when they are clean).
    pub fn redact_residue(&self, marks: &[RedactMark]) -> Result<Vec<String>> {
        let doc: Renderable = self.renderable(true)?;
        let mut out = Vec::new();
        for m in marks {
            let Some(text) = doc.text(m.page) else { continue };
            let geom = doc.geom(m.page)?;
            let mut under = String::new();
            for g in &text.glyphs {
                let cx = (g.rect[0] + g.rect[2]) / 2.0;
                let cy = (g.rect[1] + g.rect[3]) / 2.0;
                let [ux, uy] = geom.view_to_user(cx, cy);
                let p = markupcraft_geom::Point::new(ux as f64, uy as f64);
                if m.rects.iter().any(|r| r.normalized().contains(p)) {
                    under.extend(g.text.chars().filter(|c| !c.is_whitespace()));
                }
            }
            let overlay: String = m.overlay.chars().filter(|c| !c.is_whitespace()).collect();
            if !overlay.is_empty() {
                under = under.replace(&overlay, "");
            }
            if !under.is_empty() {
                out.push(format!("page {}: {under:?}", m.page + 1));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, rect, text};
    use markupcraft_render::text::TextExtractor;

    fn doc() -> Session {
        let p1 = format!(
            "{}{}{}",
            text(50.0, 700.0, 12.0, "Owner: SECRET NAME here"),
            text(50.0, 650.0, 12.0, "Public line"),
            rect(400.0, 400.0, 60.0, 60.0)
        );
        let p2 = text(50.0, 700.0, 12.0, "Another SECRET NAME");
        let bytes = pdf(&[
            SyntheticPage::new(612.0, 792.0, p1),
            SyntheticPage::new(612.0, 792.0, p2),
        ]);
        Session::from_bytes(bytes, "redact.pdf").unwrap()
    }

    #[test]
    fn overlay_look_codes_and_metadata_scrub() {
        let mut s = doc();
        s.set_doc_properties(&[("Title".into(), Some("Case file".into()))])
            .unwrap();
        let style = MarkStyle {
            overlay: REDACTION_CODES[5].0.into(),
            font: "Courier".into(),
            font_size: 9.0,
            text_color: Color::rgb(1.0, 1.0, 1.0),
            align: 0,
            repeat: true,
            ..Default::default()
        };
        s.redact_mark(0, &[Rect::new(45.0, 695.0, 250.0, 715.0)], &style)
            .unwrap();
        let m = &pdfcraft_redact::marks(&s.file.cos)[0];
        assert_eq!(m.overlay, "(b)(6)");
        assert!(m.look.repeat && m.look.align == 0 && m.look.font == pdfcraft_annot::OverlayFont::Courier);
        s.redact_apply_with(None, true).unwrap();
        assert!(
            s.doc_properties()
                .standard
                .iter()
                .all(|(k, v)| k != "Title" || v.is_empty()),
            "metadata scrubbed"
        );
        assert!(!s.page_text(0).unwrap().contains("SECRET"));
    }

    #[test]
    fn marks_search_hits_and_areas_then_applies_and_verifies() {
        let mut s = doc();
        let n = s
            .redact_search("secret name", false, false, &[], &MarkStyle::default())
            .unwrap();
        assert_eq!(n, 2);
        s.redact_mark(0, &[Rect::new(390.0, 390.0, 470.0, 470.0)], &MarkStyle::default())
            .unwrap();
        let marks = s.redact_marks();
        assert_eq!(marks.len(), 3);
        assert_eq!(marks.iter().filter(|m| m.page == 0).count(), 2);
        let r = s.redact_apply(None).unwrap();
        assert_eq!(r.marks, 3);
        assert!(r.glyphs >= 20, "{r:?}");
        assert!(r.paths >= 1, "{r:?}");
        assert!(r.residue.is_empty(), "{:?}", r.residue);
        assert!(s.redact_marks().is_empty(), "the marks are consumed");
        let mut t = TextExtractor::new(s.current_bytes().unwrap());
        assert!(t.find("secret").is_empty());
        assert_eq!(t.find("public"), vec![(0, 1)]);
        assert_eq!(t.find("owner"), vec![(0, 1)], "the rest of the line stays");
        // Undo brings the content back.
        s.undo().unwrap();
        assert_eq!(s.redact_marks().len(), 3);
        // Errors.
        let mut clean = doc();
        assert!(clean.redact_apply(None).is_err(), "nothing to apply");
        assert!(
            clean
                .redact_mark(5, &[Rect::new(0.0, 0.0, 1.0, 1.0)], &MarkStyle::default())
                .is_err()
        );
        assert!(
            clean
                .redact_mark(0, &[Rect::new(0.0, 0.0, 0.0, 1.0)], &MarkStyle::default())
                .is_err()
        );
        assert_eq!(
            clean
                .redact_search("nowhere", true, true, &[], &MarkStyle::default())
                .unwrap(),
            0
        );
    }
}
