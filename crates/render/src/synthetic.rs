//! A small synthetic drawing set, generated in code (no project files in the repository): a
//! tabloid floor plan with a page scale and a few markups of different kinds, plus a letter
//! notes page and a two-entry outline. Tests and the headless screenshot use it.

use std::fmt::Write as _;

/// The filled red square annotation on page 1 (user space 900..1000 x 600..700).
pub const SQUARE_OBJ: (u32, u16) = (8, 0);

const MEASURE: &str = "<< /Type /Measure /Subtype /RL /R (1/8 in = 1 ft) /X [<< /U (ft) /C 0.111111 /D 100 >>] \
/D [<< /U (ft) /C 1 /D 100 >>] /A [<< /U (sf) /C 1 /D 100 >>] >>";

fn plan_content() -> String {
    let mut s = String::new();
    // Title block and border.
    s.push_str("0.2 0.2 0.2 RG 2 w 24 24 1176 744 re S\n");
    s.push_str("1 w 960 24 m 960 120 l S 24 120 m 1200 120 l S\n");
    // Grid lines and bubbles.
    s.push_str("0.55 0.55 0.6 RG 0.5 w [6 3] 0 d\n");
    for i in 0..6 {
        let x = 120 + i * 180;
        let _ = writeln!(s, "{x} 150 m {x} 720 l S");
    }
    for j in 0..4 {
        let y = 180 + j * 170;
        let _ = writeln!(s, "60 {y} m 1150 {y} l S");
    }
    s.push_str("[] 0 d 0 0 0 RG 3 w\n");
    // Exterior walls and rooms.
    s.push_str("120 180 900 510 re S\n");
    s.push_str("1.5 w 120 350 m 600 350 l S 600 180 m 600 690 l S 600 520 m 1020 520 l S 300 350 m 300 690 l S\n");
    // Door swings.
    s.push_str("0.8 w 600 400 m 640 400 l S 600 440 m 622 440 640 422 640 400 c S\n");
    // Text.
    s.push_str("BT /F1 22 Tf 0 0 0 rg 990 80 Td (SAMPLE PLAN) Tj ET\n");
    s.push_str("BT /F1 10 Tf 990 60 Td (SCALE 1/8 IN = 1 FT) Tj ET\n");
    s.push_str("BT /F1 12 Tf 150 640 Td (LIVING) Tj ET BT /F1 12 Tf 650 640 Td (KITCHEN) Tj ET\n");
    s.push_str("BT /F1 12 Tf 150 300 Td (BEDROOM) Tj ET BT /F1 12 Tf 650 300 Td (BATH) Tj ET\n");
    for (i, c) in ["A", "B", "C", "D", "E", "F"].iter().enumerate() {
        let x = 120 + i * 180;
        let _ = writeln!(s, "0.5 w {x} 735 12 0 360 arc");
        let _ = writeln!(s, "BT /F1 12 Tf {} 731 Td ({c}) Tj ET", x - 4);
    }
    s
}

/// Tiny helper: a full circle path as four Bezier arcs (PDF has no arc operator).
fn circles(content: &str) -> String {
    let mut out = String::new();
    for line in content.lines() {
        if let Some(rest) = line.strip_suffix(" 0 360 arc") {
            let v: Vec<f64> = rest.split_whitespace().filter_map(|t| t.parse().ok()).collect();
            if let [w, x, y, r] = v.as_slice() {
                let k = 0.5523 * r;
                let _ = writeln!(
                    out,
                    "{w} w {} {y} m {} {} {} {} {x} {} c {} {} {} {} {} {y} c {} {} {} {} {x} {} c {} {} {} {} {} {y} c S",
                    x + r,
                    x + r,
                    y + k,
                    x + k,
                    y + r,
                    y + r,
                    x - k,
                    y + r,
                    x - r,
                    y + k,
                    x - r,
                    x - r,
                    y - k,
                    x - k,
                    y - r,
                    y - r,
                    x + k,
                    y - r,
                    x + r,
                    y - k,
                    x + r,
                );
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn stream(dict: &str, data: &str) -> String {
    format!("<< {dict} /Length {} >>\nstream\n{data}\nendstream", data.len() + 1)
}

/// The sample PDF bytes.
pub fn sample_pdf() -> Vec<u8> {
    let plan = circles(&plan_content());
    let notes = "BT /F1 18 Tf 72 720 Td (GENERAL NOTES) Tj ET\n\
BT /F1 11 Tf 72 690 Td (1. Verify all dimensions in the field.) Tj ET\n\
BT /F1 11 Tf 72 672 Td (2. This sheet was generated for tests.) Tj ET\n";
    let res = "/Resources << /Font << /F1 7 0 R >> >>";
    let objs: Vec<(u32, String)> = vec![
        (1, "<< /Type /Catalog /Pages 2 0 R /Outlines 10 0 R >>".into()),
        (2, "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".into()),
        (
            3,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1224 792] /Contents 5 0 R {res} \
/Annots [8 0 R 9 0 R 11 0 R 12 0 R 13 0 R 14 0 R] /VP [<< /Type /Viewport /BBox [0 0 1224 792] /Name (Plan) /Measure {MEASURE} >>] >>"
            ),
        ),
        (
            4,
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R {res} >>"),
        ),
        (5, stream("", &plan)),
        (6, stream("", notes)),
        (
            7,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
        ),
        (
            8,
            "<< /Type /Annot /Subtype /Square /Rect [900 600 1000 700] /NM (SAMPLESQUAREAAAA) /Subj (Rectangle) \
/T (Estimator) /C [1 0 0] /IC [1 0 0] /CA 1 /BS << /W 2 >> /F 4 /AP << /N 15 0 R >> >>"
                .into(),
        ),
        (
            9,
            "<< /Type /Annot /Subtype /Circle /Rect [700 560 860 680] /NM (SAMPLECIRCLEAAAA) /Subj (Ellipse) \
/T (Estimator) /C [0 0 1] /BS << /W 3 >> /F 4 >>"
                .into(),
        ),
        (
            10,
            "<< /Type /Outlines /First 16 0 R /Last 17 0 R /Count 2 >>".into(),
        ),
        (
            11,
            format!(
                "<< /Type /Annot /Subtype /Polygon /IT /PolygonDimension /MeasurementTypes 129 /Rect [300 180 600 350] \
/Vertices [300 180 600 180 600 350 300 350] /NM (SAMPLEAREAAAAAAA) /Subj (Area) /T (Estimator) \
/C [0 0.5 0] /IC [0.3 0.85 0.3] /CA 1 /FillOpacity 0.35 /BS << /W 2 >> /F 4 /Measure {MEASURE} >>"
            ),
        ),
        (
            12,
            "<< /Type /Annot /Subtype /PolyLine /Rect [700 230 1000 420] /Vertices [700 240 820 410 900 260 1000 400] \
/NM (SAMPLEPOLYLINEAA) /Subj (Polyline) /T (Estimator) /C [0.9 0.5 0] /BS << /W 2 >> /F 4 >>"
                .into(),
        ),
        (
            13,
            "<< /Type /Annot /Subtype /FreeText /Rect [640 200 900 236] /NM (SAMPLETEXTAAAAAA) /Subj (Text Box) \
/T (Estimator) /Contents (Verify sink location) /DA (1 0 0 rg /Helv 14 Tf) /C [1 0 0] /F 4 >>"
                .into(),
        ),
        (
            14,
            "<< /Type /Annot /Subtype /Line /IT /LineArrow /Rect [120 130 600 170] /L [140 150 580 150] /LE [/None /OpenArrow] \
/NM (SAMPLEARROWAAAAA) /Subj (Arrow) /T (Estimator) /C [0.6 0 0.6] /BS << /W 2 >> /F 4 >>"
                .into(),
        ),
        (
            15,
            stream(
                "/Type /XObject /Subtype /Form /BBox [0 0 100 100]",
                "1 0 0 rg 0 0 100 100 re f",
            ),
        ),
        (
            16,
            "<< /Title (Floor Plan) /Parent 10 0 R /Next 17 0 R /Dest [3 0 R /Fit] >>".into(),
        ),
        (
            17,
            "<< /Title (General Notes) /Parent 10 0 R /Prev 16 0 R /Dest [4 0 R /Fit] >>".into(),
        ),
    ];
    let mut out = String::from("%PDF-1.7\n");
    let max = objs.iter().map(|(n, _)| *n).max().unwrap_or(0);
    let mut offsets = vec![0usize; max as usize + 1];
    for (n, body) in &objs {
        if let Some(slot) = offsets.get_mut(*n as usize) {
            *slot = out.len();
        }
        let _ = write!(out, "{n} 0 obj\n{body}\nendobj\n");
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", max + 1);
    for off in offsets.iter().skip(1) {
        // Each xref entry is exactly 20 bytes: "nnnnnnnnnn ggggg n" + space + newline.
        let _ = writeln!(out, "{off:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        max + 1
    );
    out.into_bytes()
}
