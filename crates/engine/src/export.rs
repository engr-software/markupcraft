//! The Markups List as CSV.

use markupcraft_model::Document;

fn q(s: &str) -> String {
    // A leading = + - @ would run as a formula in a spreadsheet: quote it as text.
    let s = if s.starts_with(['=', '+', '-', '@']) && s.parse::<f64>().is_err() {
        format!("'{s}")
    } else {
        s.to_string()
    };
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// One row per markup (optionally only on `page`): page (1-based), page label, id, type,
/// subject, label, author, layer, status, contents, measurement, value, unit, color, then one
/// column per custom column.
pub fn markups_csv(doc: &Document, page: Option<usize>) -> String {
    let mut out =
        String::from("Page,Page Label,Id,Type,Subject,Label,Author,Layer,Status,Contents,Measurement,Value,Unit,Color");
    for c in &doc.columns {
        out.push(',');
        out.push_str(&q(&c.name));
    }
    out.push('\n');
    for m in doc.markups.iter().filter(|m| page.is_none_or(|p| m.page == p)) {
        let label = doc.pages.get(m.page).map(|p| p.label.as_str()).unwrap_or("");
        let value = m.quantity().map(|v| format!("{v}")).unwrap_or_default();
        let row = [
            (m.page + 1).to_string(),
            q(label),
            q(&m.id),
            q(m.kind.name()),
            q(&m.subject),
            q(&m.label),
            q(&m.author),
            q(&m.layer),
            q(&m.status),
            q(&m.contents),
            q(&m.quantity_text()),
            value,
            q(&m.unit()),
            q(&m.color.hex()),
        ];
        out.push_str(&row.join(","));
        for c in &doc.columns {
            out.push(',');
            out.push_str(&q(m.column_data.get(&c.id).map(String::as_str).unwrap_or("")));
        }
        out.push('\n');
    }
    out
}
