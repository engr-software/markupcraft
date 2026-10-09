//! Summary appended with links: a markup summary added at the end of the document, each row a
//! link back to its markup's page (Revu's Markups List > Summary > Append to document).

use markupcraft_model::{Group, MarkupTable, Rect, Scope, View, table::page_label};

use crate::links::{LinkLook, LinkTarget};
use crate::synthetic::{SyntheticPage, line, text};
use crate::{Result, Session, invalid};

const W: f64 = 612.0;
const H: f64 = 792.0;
const M: f64 = 36.0;
const ROW: f64 = 14.0;
/// Most rows appended (the rest are left out).
const MAX_ROWS: usize = 20_000;

/// Printable ASCII only (the summary is set in Helvetica with the standard encoding).
fn plain(s: &str, max: usize) -> String {
    let mut t: String = s
        .chars()
        .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
        .take(max)
        .collect();
    if s.chars().count() > max && max > 1 {
        t.pop();
        t.push('~');
    }
    t
}

fn rows_in(g: &Group, out: &mut Vec<usize>) {
    out.extend(g.rows.iter().copied());
    for c in &g.children {
        rows_in(c, out);
    }
}

impl Session {
    /// Append a summary of the markups (all, or the measurements) at the end of the document:
    /// subject, page, measurement and comments per row, each row linked to its markup's page.
    /// One undo step; returns how many rows.
    pub fn append_summary_with_links(&mut self, title: &str, measurements_only: bool) -> Result<usize> {
        let doc = self.doc();
        let table = MarkupTable::new(doc);
        let view = View {
            scope: Scope::AllPages,
            measurements_only,
            ..Default::default()
        };
        let mut rows = Vec::new();
        rows_in(&table.build(&view), &mut rows);
        rows.truncate(MAX_ROWS);
        if rows.is_empty() {
            return Err(invalid("no markups to summarize"));
        }
        let title = if title.trim().is_empty() {
            "Markup Summary"
        } else {
            title.trim()
        };
        // (summary page, the row's rectangle, the markup's page)
        let mut links: Vec<(usize, Rect, usize)> = Vec::new();
        let mut pages: Vec<SyntheticPage> = Vec::new();
        let mut content = String::new();
        let mut y = 0.0;
        let cols = [
            (M, "Subject", 34),
            (M + 200.0, "Page", 10),
            (M + 260.0, "Measurement", 20),
            (M + 380.0, "Comments", 36),
        ];
        for (n, &i) in rows.iter().enumerate() {
            if n == 0 || y < M + ROW {
                if n > 0 {
                    pages.push(SyntheticPage::new(W, H, std::mem::take(&mut content)));
                }
                y = H - M - 16.0;
                content.push_str(&text(M, y, 16.0, &plain(title, 60)));
                y -= 24.0;
                for (x, h, _) in cols {
                    content.push_str(&text(x, y, 9.0, h));
                }
                content.push_str(&line(M, y - 4.0, W - M, y - 4.0, 0.8));
                y -= ROW + 2.0;
            }
            let Some(m) = doc.markups.get(i) else { continue };
            let cells = [
                m.subject.clone(),
                page_label(doc, m.page),
                m.quantity_text(),
                if m.kind.is_measurement() {
                    String::new()
                } else {
                    m.contents.replace(['\r', '\n'], " ")
                },
            ];
            for ((x, _, chars), c) in cols.iter().zip(cells.iter()) {
                content.push_str(&text(*x, y, 8.5, &plain(c, *chars)));
            }
            links.push((pages.len(), Rect::new(M - 2.0, y - 3.0, W - M, y + ROW - 4.0), m.page));
            y -= ROW;
        }
        pages.push(SyntheticPage::new(W, H, content));
        let bytes = crate::synthetic::pdf(&pages);
        let file = crate::pages::ForeignPdf::from_bytes(bytes, "summary.pdf")?;
        let at = self.page_count();
        let n = rows.len();
        self.set_merge_key(Some("append-summary-with-links"));
        let r = (|| -> Result<()> {
            let mut plan = crate::pages::PagePlan::identity(at);
            plan.insert_file(at, file, None)?;
            self.apply_page_plan("Append Summary", &plan)?;
            for (k, rect, target) in links {
                self.add_link(at + k, rect, &LinkTarget::Page(target), LinkLook::default())?;
            }
            Ok(())
        })();
        self.set_merge_key(None);
        self.seal();
        r.map(|_| n)
    }
}

#[cfg(test)]
mod tests {
    use markupcraft_model::{Kind, Markup, Point};

    use super::*;

    #[test]
    fn summary_appended_links_each_row_to_its_page() {
        let dir = std::env::temp_dir().join(format!("markupcraft-sumlinks-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut s = Session::new_blank(dir.join("s.pdf"), &[(612.0, 792.0), (612.0, 792.0)]).expect("blank");
        for (page, subject) in [(0, "Door"), (1, "Window")] {
            let mut m = Markup::new(Kind::Line, page, vec![Point::new(10.0, 10.0), Point::new(90.0, 10.0)]);
            m.subject = subject.into();
            s.add_markup(m).expect("add");
        }
        let depth = s.undo_depth();
        assert_eq!(s.append_summary_with_links("", false).expect("append"), 2);
        assert_eq!(s.page_count(), 3, "one summary page");
        assert_eq!(s.undo_depth(), depth + 1, "one undo step");
        let links = s.links();
        let targets: Vec<&LinkTarget> = links.iter().filter(|l| l.page == 2).map(|l| &l.target).collect();
        assert_eq!(targets.len(), 2);
        assert!(targets.contains(&&LinkTarget::Page(0)) && targets.contains(&&LinkTarget::Page(1)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
