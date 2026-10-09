//! Help > Shortcut Reference: every command with its keys (the user's own, as Tools >
//! Customize Keyboard set them), plus the mouse and modifier keys, as a printable PDF.

use std::path::Path;

use markupcraft_engine::synthetic::{SyntheticPage, line, pdf, text};

use crate::AppState;

/// Mouse and modifier keys (Revu's conventions, as MarkupCraft follows them).
pub const MOUSE: &[(&str, &str)] = &[
    ("Wheel", "Zoom (or scroll, by preference); Ctrl swaps"),
    ("Middle drag / Space + drag", "Pan"),
    ("Right click", "Context menu; finishes a measurement"),
    ("Right drag (Select)", "Select markups in a box"),
    ("Shift while drawing", "Square, circle, straight lines"),
    ("Shift + drag a markup", "Move in a straight line"),
    ("Ctrl + drag a markup", "Copy it; Ctrl+Shift copies in a straight line"),
    ("Rotate handle", "Turns in 15 degree steps; Shift for whole degrees"),
    ("Ctrl while placing a point", "Place it without snapping"),
    ("Shift + click a vertex or segment", "Remove or add a vertex"),
];

/// The rows: (what, keys), commands first.
pub fn rows(app: &AppState) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = app
        .keys
        .bindings()
        .into_iter()
        .rev()
        .map(|(k, id)| {
            let label = crate::commands::describe(&id).map_or(id, |d| d.0);
            (label, k.label())
        })
        .collect();
    out.sort_by_key(|a| a.0.to_lowercase());
    out.dedup();
    out
}

fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii() && !c.is_control() { c } else { '?' })
        .collect()
}

/// Write the reference to `out`; returns how many shortcuts it lists.
pub fn write(app: &AppState, out: &Path) -> Result<usize, String> {
    let rows = rows(app);
    let (w, h) = (612.0, 792.0);
    let per_col = 52;
    let mut pages = Vec::new();
    let mut content = String::new();
    let mut i = 0;
    let header = |content: &mut String, page: usize| {
        content.push_str(&text(
            40.0,
            756.0,
            14.0,
            &format!("MarkupCraft keyboard shortcuts ({})", page),
        ));
        content.push_str(&line(40.0, 748.0, 572.0, 748.0, 0.5));
    };
    header(&mut content, 1);
    let mut all: Vec<(String, String)> = rows.clone();
    all.push((String::new(), String::new()));
    all.push(("MOUSE AND MODIFIER KEYS".into(), String::new()));
    all.extend(MOUSE.iter().map(|(a, b)| (a.to_string(), b.to_string())));
    for (label, keys) in &all {
        let col = (i / per_col) % 2;
        let row = i % per_col;
        let x = 40.0 + col as f64 * 270.0;
        let y = 730.0 - row as f64 * 13.5;
        content.push_str(&text(x, y, 8.0, &ascii(label)));
        content.push_str(&text(x + 150.0, y, 8.0, &ascii(keys)));
        i += 1;
        if i % (2 * per_col) == 0 {
            pages.push(SyntheticPage::new(w, h, std::mem::take(&mut content)));
            header(&mut content, pages.len() + 1);
        }
        if pages.len() > 50 {
            break;
        }
    }
    pages.push(SyntheticPage::new(w, h, content));
    crate::chest::write_atomic(out, &pdf(&pages)).map_err(|e| e.to_string())?;
    Ok(rows.len())
}
