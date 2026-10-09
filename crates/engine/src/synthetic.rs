//! Small PDFs built from content streams: test inputs for the review features (compare,
//! visual search, OCR, redaction, forms), so no drawing has to be committed.

/// One page: its size in points and its content stream. The stream may use the font `/F1`
/// (Helvetica, WinAnsi).
#[derive(Debug, Clone, PartialEq)]
pub struct SyntheticPage {
    pub width: f64,
    pub height: f64,
    pub content: String,
}

impl SyntheticPage {
    pub fn new(width: f64, height: f64, content: impl Into<String>) -> Self {
        Self {
            width,
            height,
            content: content.into(),
        }
    }
}

/// A complete PDF (with a correct cross-reference table) of these pages.
pub fn pdf(pages: &[SyntheticPage]) -> Vec<u8> {
    let n = pages.len();
    // 1 catalog, 2 pages, 3 font, then (page, content) pairs.
    let mut objs: Vec<String> = Vec::with_capacity(3 + 2 * n);
    objs.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")));
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into());
    for (i, p) in pages.iter().enumerate() {
        objs.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            p.width,
            p.height,
            5 + 2 * i
        ));
        objs.push(format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            p.content.len() + 1,
            p.content
        ));
    }
    let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objs.len());
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// Content that writes `text` at `(x, y)` in 12 pt Helvetica.
pub fn text(x: f64, y: f64, size: f64, text: &str) -> String {
    let esc = text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    format!("BT /F1 {size} Tf {x} {y} Td ({esc}) Tj ET\n")
}

/// Content that fills a black rectangle.
pub fn rect(x: f64, y: f64, w: f64, h: f64) -> String {
    format!("0 g {x} {y} {w} {h} re f\n")
}

/// Content that strokes a black line.
pub fn line(x0: f64, y0: f64, x1: f64, y1: f64, width: f64) -> String {
    format!("0 G {width} w {x0} {y0} m {x1} {y1} l S\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_pdf_opens_with_its_text() {
        let bytes = pdf(&[SyntheticPage::new(
            300.0,
            200.0,
            text(20.0, 100.0, 12.0, "Hello (world)"),
        )]);
        let s = crate::Session::from_bytes(bytes, "synthetic.pdf").unwrap();
        assert_eq!(s.page_count(), 1);
        let r = s.renderable(false).unwrap();
        let t = r.text(0).unwrap();
        assert!(t.plain_text().contains("Hello (world)"), "{:?}", t.plain_text());
    }
}
