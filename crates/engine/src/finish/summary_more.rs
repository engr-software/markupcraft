//! Markup Summary PDF extras: a Spaces cover sheet (each space with how many markups are in
//! it), the status history of each markup (its review replies), markup thumbnails (the page
//! around each markup, on contact sheets with a numbered index) and the page content (the
//! summarized pages themselves, with their markups) after the report.

use std::path::{Path, PathBuf};

use markupcraft_model::spaces::space_path;

use crate::summary::{SummaryFormat, SummaryOptions};
use crate::{Result, Session, invalid};

/// What the summary PDF adds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SummaryExtras {
    pub spaces_cover: bool,
    pub status_history: bool,
    /// Markup thumbnails, this many pixels on a side (48 to 400).
    pub thumbnails: Option<u32>,
    pub page_content: bool,
}

/// Thumbnails on one contact sheet.
const PER_SHEET: usize = 12;

impl Session {
    /// The Markup Summary as a PDF at `out` with `x`'s extras. Returns the summary's markup
    /// count.
    pub fn export_summary_extras(&self, out: &Path, o: &SummaryOptions, x: &SummaryExtras) -> Result<usize> {
        let tmp = markupcraft_revu::fsio::temp_dir().join(format!(
            "markupcraft-summary-{}-{}",
            markupcraft_revu::fsio::process_id(),
            web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp).map_err(|e| invalid(format!("{}: {e}", tmp.display())))?;
        let r = self.summary_parts(&tmp, out, o, x);
        let _ = std::fs::remove_dir_all(&tmp);
        r
    }

    fn summary_parts(&self, tmp: &Path, out: &Path, o: &SummaryOptions, x: &SummaryExtras) -> Result<usize> {
        let write = |name: &str, bytes: &[u8]| -> Result<PathBuf> {
            let p = tmp.join(name);
            crate::write_atomic(&p, bytes)?;
            Ok(p)
        };
        let summary = tmp.join("summary.pdf");
        let n = self.export_summary(&summary, Some(SummaryFormat::Pdf), o)?;
        let doc = &self.doc;
        let chosen: Vec<&markupcraft_model::Markup> = doc
            .markups
            .iter()
            .filter(|m| o.pages.is_empty() || o.pages.contains(&m.page))
            .filter(|m| !o.measurements_only || m.kind.is_measurement())
            .take(5_000)
            .collect();
        let mut parts: Vec<PathBuf> = Vec::new();
        if x.spaces_cover {
            let mut t = String::from("SPACES\n\n");
            for (page, sp) in self.spaces(None) {
                let k = chosen.iter().filter(|m| space_path(doc, m).contains(&sp.name)).count();
                t.push_str(&format!("{}  (page {})  {} markups\n", sp.name, page + 1, k));
            }
            let outside = chosen.iter().filter(|m| space_path(doc, m).is_empty()).count();
            t.push_str(&format!("\nNot in a space: {outside} markups\n"));
            parts.push(write("cover.txt", t.as_bytes())?);
        }
        parts.push(summary);
        if x.status_history {
            let mut t = String::from("STATUS HISTORY\n\n");
            for m in &chosen {
                let hist: Vec<String> = m
                    .replies
                    .iter()
                    .filter(|r| r.state_model == "Review" && !r.state.is_empty())
                    .map(|r| format!("    {} by {} {}", r.state, r.author, r.date))
                    .collect();
                if hist.is_empty() && m.status.is_empty() {
                    continue;
                }
                t.push_str(&format!(
                    "{} (page {}): {}\n",
                    if m.subject.is_empty() {
                        m.kind.name()
                    } else {
                        &m.subject
                    },
                    m.page + 1,
                    if m.status.is_empty() { "None" } else { &m.status }
                ));
                for h in hist {
                    t.push_str(&h);
                    t.push('\n');
                }
            }
            parts.push(write("history.txt", t.as_bytes())?);
        }
        if let Some(side) = x.thumbnails {
            if !(48..=400).contains(&side) {
                return Err(invalid("thumbnail size is 48 to 400 pixels"));
            }
            let render = self.renderable(false)?;
            let mut cache: std::collections::HashMap<usize, crate::raster::PageImage> = Default::default();
            let mut index = String::from("MARKUP THUMBNAILS\n\n");
            let (cols, rows) = (3u32, 4u32);
            let mut sheet = image::GrayImage::from_pixel(cols * (side + 8), rows * (side + 8), image::Luma([255]));
            let mut on_sheet = 0usize;
            let mut sheets = 0usize;
            for (i, m) in chosen.iter().enumerate() {
                let img = match cache.entry(m.page) {
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                    std::collections::hash_map::Entry::Vacant(e) => e.insert(render.render(m.page, 2.0, 4_000.0)?),
                };
                let r = m.rect.normalized();
                let pad = 12.0;
                let px = img.rect_to_px(markupcraft_geom::Rect::new(
                    r.x0 - pad,
                    r.y0 - pad,
                    r.x1 + pad,
                    r.y1 + pad,
                ));
                let (x0, x1) = (px[0].min(px[2]).max(0.0), px[0].max(px[2]).min(img.gray.w as f64));
                let (y0, y1) = (px[1].min(px[3]).max(0.0), px[1].max(px[3]).min(img.gray.h as f64));
                if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
                    continue;
                }
                let cell = (on_sheet % PER_SHEET) as u32;
                let (cx, cy) = ((cell % cols) * (side + 8) + 4, (cell / cols) * (side + 8) + 4);
                let k = (f64::from(side) / (x1 - x0)).min(f64::from(side) / (y1 - y0));
                for ty in 0..side {
                    for tx in 0..side {
                        let sx = x0 + f64::from(tx) / k;
                        let sy = y0 + f64::from(ty) / k;
                        if sx >= x1 || sy >= y1 {
                            continue;
                        }
                        let v = img.gray.get(sx as usize, sy as usize);
                        sheet.put_pixel(cx + tx, cy + ty, image::Luma([v]));
                    }
                }
                index.push_str(&format!(
                    "{}. {} (page {}) {}\n",
                    i + 1,
                    if m.subject.is_empty() {
                        m.kind.name()
                    } else {
                        &m.subject
                    },
                    m.page + 1,
                    m.contents.lines().next().unwrap_or_default()
                ));
                on_sheet += 1;
                if on_sheet.is_multiple_of(PER_SHEET) {
                    let mut buf = std::io::Cursor::new(Vec::new());
                    sheet
                        .write_to(&mut buf, image::ImageFormat::Png)
                        .map_err(|e| invalid(e.to_string()))?;
                    parts.push(write(&format!("thumbs{sheets}.png"), &buf.into_inner())?);
                    sheets += 1;
                    sheet = image::GrayImage::from_pixel(cols * (side + 8), rows * (side + 8), image::Luma([255]));
                }
            }
            if !on_sheet.is_multiple_of(PER_SHEET) {
                let mut buf = std::io::Cursor::new(Vec::new());
                sheet
                    .write_to(&mut buf, image::ImageFormat::Png)
                    .map_err(|e| invalid(e.to_string()))?;
                parts.push(write(&format!("thumbs{sheets}.png"), &buf.into_inner())?);
            }
            parts.push(write("thumbs.txt", index.as_bytes())?);
        }
        if x.page_content {
            let pages: Vec<usize> = if o.pages.is_empty() {
                (0..self.page_count()).collect()
            } else {
                o.pages.clone()
            };
            let mut copy = Session::from_bytes(self.current_bytes()?.to_vec(), tmp.join("all.pdf"))?;
            let p = tmp.join("content.pdf");
            copy.extract_pages(&pages, &p, false)?;
            parts.push(p);
        }
        crate::docs_more::create_pdf_from_files(&parts, out)?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, rect};
    use markupcraft_model::{Kind, Markup, Point};

    #[test]
    fn cover_history_thumbnails_and_pages() {
        let mut s = Session::from_bytes(
            pdf(&[SyntheticPage::new(612.0, 792.0, rect(100.0, 100.0, 300.0, 300.0))]),
            "s.pdf",
        )
        .unwrap();
        let mut m = Markup::new(
            Kind::Rectangle,
            0,
            vec![Point::new(120.0, 120.0), Point::new(200.0, 200.0)],
        );
        m.subject = "RFI".into();
        m.status = "Accepted".into();
        s.add_markup(m).unwrap();
        s.add_space(
            0,
            "Lobby",
            vec![
                Point::new(0.0, 0.0),
                Point::new(400.0, 0.0),
                Point::new(400.0, 400.0),
                Point::new(0.0, 400.0),
            ],
            None,
            None,
        )
        .unwrap();
        let out = std::env::temp_dir().join(format!("mc-sumx-{}.pdf", std::process::id()));
        let x = SummaryExtras {
            spaces_cover: true,
            status_history: true,
            thumbnails: Some(96),
            page_content: true,
        };
        s.export_summary_extras(&out, &SummaryOptions::default(), &x).unwrap();
        let back = Session::open(&out).unwrap();
        // cover, summary, history, a contact sheet, its index, the page
        assert!(back.page_count() >= 5, "{}", back.page_count());
        let first = back.page_text(0).unwrap();
        assert!(first.contains("Lobby"), "{first}");
        let all: String = (0..back.page_count())
            .map(|p| back.page_text(p).unwrap_or_default())
            .collect();
        assert!(
            all.contains("STATUS HISTORY") && all.contains("RFI (page 1): Accepted"),
            "{all}"
        );
        let _ = std::fs::remove_file(out);
    }
}
