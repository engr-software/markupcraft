//! Status reports of counts (and any markup with a review status): how many items of each
//! subject are in each status (None, Accepted, Installed ...), and the visual report, a copy of
//! the document with every markup recoloured by its status so progress shows on the drawings.

use std::collections::BTreeMap;
use std::path::Path;

use markupcraft_model::{Color, Kind, Markup};

use crate::{MarkupPatch, Result, Session};

/// One line of a status report.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusRow {
    pub subject: String,
    pub status: String,
    /// Count items (a Count's points), or markups for other kinds.
    pub items: usize,
    pub markups: usize,
}

/// The colour a status is shown in by the visual report: the review statuses have fixed
/// colours, the user's own ones a colour from a palette by their name.
pub fn status_color(status: &str) -> Color {
    match status.trim() {
        "" | "None" => Color::rgb(0.55, 0.55, 0.55),
        "Accepted" => Color::rgb(0.13, 0.6, 0.2),
        "Rejected" => Color::rgb(0.85, 0.12, 0.12),
        "Cancelled" => Color::rgb(0.3, 0.3, 0.3),
        "Completed" => Color::rgb(0.1, 0.35, 0.85),
        other => {
            const PALETTE: [(f64, f64, f64); 6] = [
                (0.95, 0.55, 0.05),
                (0.55, 0.2, 0.75),
                (0.0, 0.6, 0.6),
                (0.8, 0.2, 0.55),
                (0.45, 0.6, 0.1),
                (0.6, 0.4, 0.2),
            ];
            let h = other
                .bytes()
                .fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(u32::from(b)));
            let (r, g, b) = PALETTE[(h as usize) % PALETTE.len()];
            Color::rgb(r, g, b)
        }
    }
}

fn status_of(m: &Markup) -> String {
    if m.status.trim().is_empty() {
        "None".into()
    } else {
        m.status.trim().to_string()
    }
}

/// The report over `markups` (only Counts when `counts_only`), by subject then status.
pub fn status_report(markups: &[Markup], counts_only: bool) -> Vec<StatusRow> {
    let mut map: BTreeMap<(String, String), (usize, usize)> = BTreeMap::new();
    for m in markups {
        if counts_only && m.kind != Kind::Count {
            continue;
        }
        let items = if m.kind == Kind::Count { m.pts.len() } else { 1 };
        let e = map.entry((m.subject.clone(), status_of(m))).or_insert((0, 0));
        e.0 += items;
        e.1 += 1;
    }
    map.into_iter()
        .map(|((subject, status), (items, markups))| StatusRow {
            subject,
            status,
            items,
            markups,
        })
        .collect()
}

/// The report as CSV (Subject, Status, Items, Markups).
pub fn status_csv(rows: &[StatusRow]) -> String {
    let q = |s: &str| {
        if s.contains([',', '"', '\n']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let mut out = String::from("Subject,Status,Items,Markups\n");
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{}\n",
            q(&r.subject),
            q(&r.status),
            r.items,
            r.markups
        ));
    }
    out
}

impl Session {
    /// The visual status report: a copy of this document (as it is now) at `out` with every
    /// markup (only Counts when `counts_only`) drawn in its status colour. This document is
    /// not changed. Returns the rows of the report.
    pub fn visual_status_report(&self, out: &Path, counts_only: bool) -> Result<Vec<StatusRow>> {
        let bytes = self.current_bytes()?;
        let mut copy = Session::from_bytes(bytes.to_vec(), out)?;
        let targets: Vec<(String, String)> = copy
            .doc
            .markups
            .iter()
            .filter(|m| !counts_only || m.kind == Kind::Count)
            .map(|m| (m.id.clone(), status_of(m)))
            .collect();
        for (id, st) in &targets {
            let c = status_color(st);
            let p = MarkupPatch {
                color: Some(c),
                ..Default::default()
            };
            copy.set_properties(std::slice::from_ref(id), &p)?;
        }
        copy.save_as(out, true)?;
        Ok(status_report(&self.doc.markups, counts_only))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markupcraft_model::Point;

    #[test]
    fn status_rows_colours_and_visual_copy() {
        let mut a = Markup::new(Kind::Count, 0, vec![Point::new(1.0, 1.0), Point::new(2.0, 2.0)]);
        a.subject = "Door".into();
        a.status = "Installed".into();
        let mut b = Markup::new(Kind::Count, 0, vec![Point::new(3.0, 3.0)]);
        b.subject = "Door".into();
        let rows = status_report(&[a.clone(), b.clone()], true);
        assert_eq!(rows.len(), 2);
        assert!(rows.contains(&StatusRow {
            subject: "Door".into(),
            status: "Installed".into(),
            items: 2,
            markups: 1
        }));
        assert_eq!(status_color("Installed"), status_color("Installed"));
        assert_ne!(status_color("Accepted"), status_color("Rejected"));
        assert!(status_csv(&rows).starts_with("Subject,Status,Items,Markups\nDoor,"));

        let mut s = Session::from_bytes(
            crate::synthetic::pdf(&[crate::synthetic::SyntheticPage::new(612.0, 792.0, String::new())]),
            "s.pdf",
        )
        .unwrap();
        let ia = s.add_markup(a).unwrap();
        s.add_markup(b).unwrap();
        let out = std::env::temp_dir().join(format!("mc-status-{}.pdf", std::process::id()));
        let rows = s.visual_status_report(&out, true).unwrap();
        assert_eq!(rows.len(), 2);
        let back = Session::open(&out).unwrap();
        let m = back.doc().find(&ia).unwrap();
        assert_eq!(m.color, status_color("Installed"));
        assert_ne!(
            s.doc().find(&ia).unwrap().color,
            status_color("Installed"),
            "the document is not changed"
        );
        let _ = std::fs::remove_file(out);
    }
}
