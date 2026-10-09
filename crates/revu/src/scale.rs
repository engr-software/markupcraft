//! `/Measure /RL` dictionaries and `/NumberFormat` arrays.

use markupcraft_measure::{Fmt, FormatArray, NumberFormat, Scale};
use pdfcraft_cos::{Dict, Document as CosDoc, Object};

use crate::pdf::{self, real, s};

fn formats(doc: &CosDoc, o: Option<&Object>) -> FormatArray {
    let Some(o) = o else { return Vec::new() };
    let arr = doc.resolve(o);
    let Some(arr) = arr.as_array() else { return Vec::new() };
    let mut out = Vec::new();
    for item in arr {
        let d = doc.resolve(item);
        let Some(d) = d.as_dict() else { continue };
        let fmt = Fmt::from_name(&pdf::name(d.get(b"F")));
        let mut f = NumberFormat {
            unit: pdf::text(d.get(b"U")),
            conv: pdf::num_or(d.get(b"C"), 1.0),
            fmt,
            den: pdf::num(d.get(b"D"))
                .map(|v| v as i64)
                .unwrap_or(if fmt == Fmt::Fraction { 16 } else { 100 }),
            no_reduce: pdf::boolean(d.get(b"FD")).unwrap_or(false),
            label_first: pdf::name(d.get(b"O")) == "P",
            ..Default::default()
        };
        if d.contains(b"RT") {
            f.thousands = pdf::text(d.get(b"RT"));
        }
        if d.contains(b"RD") {
            f.decimal = pdf::text(d.get(b"RD"));
        }
        if d.contains(b"PS") {
            f.prefix = pdf::text(d.get(b"PS"));
        }
        if d.contains(b"SS") {
            f.suffix = pdf::text(d.get(b"SS"));
        }
        out.push(f);
    }
    out
}

/// Read a `/Measure` dictionary; `None` unless it is a valid rectilinear scale.
pub fn read(doc: &CosDoc, o: Option<&Object>) -> Option<Scale> {
    let m = doc.resolve(o?);
    let m = m.as_dict()?;
    if m.contains(b"Subtype") && m.name(b"Subtype") != Some(b"RL") {
        return None;
    }
    let s = Scale {
        ratio: pdf::text(m.get(b"R")),
        x: formats(doc, m.get(b"X")),
        y: formats(doc, m.get(b"Y")),
        dist: formats(doc, m.get(b"D")),
        area: formats(doc, m.get(b"A")),
        volume: formats(doc, m.get(b"V")),
        target_unit_conversion: pdf::num(m.get(b"TargetUnitConversion")),
    };
    s.valid().then_some(s)
}

fn formats_obj(fa: &FormatArray) -> Object {
    Object::Array(
        fa.iter()
            .map(|f| {
                let mut d = Dict::new();
                pdf::set(&mut d, "Type", pdf::n("NumberFormat"));
                pdf::set(&mut d, "U", s(&f.unit));
                pdf::set(&mut d, "C", real(f.conv));
                pdf::set(&mut d, "F", pdf::n(f.fmt.name()));
                pdf::set(&mut d, "D", Object::Int(f.den));
                pdf::set(&mut d, "FD", Object::Bool(f.no_reduce));
                if f.thousands != "," {
                    pdf::set(&mut d, "RT", s(&f.thousands));
                }
                if f.decimal != "." {
                    pdf::set(&mut d, "RD", s(&f.decimal));
                }
                if f.prefix != " " {
                    pdf::set(&mut d, "PS", s(&f.prefix));
                }
                if f.suffix != " " {
                    pdf::set(&mut d, "SS", s(&f.suffix));
                }
                if f.label_first {
                    pdf::set(&mut d, "O", pdf::n("P"));
                }
                Object::Dict(d)
            })
            .collect(),
    )
}

/// A `/Measure /RL` dictionary for `sc`.
pub fn object(sc: &Scale) -> Object {
    let mut m = Dict::new();
    pdf::set(&mut m, "Type", pdf::n("Measure"));
    pdf::set(&mut m, "Subtype", pdf::n("RL"));
    pdf::set(&mut m, "R", s(&sc.ratio));
    pdf::set(&mut m, "X", formats_obj(&sc.x));
    if !sc.y.is_empty() {
        pdf::set(&mut m, "Y", formats_obj(&sc.y));
    }
    pdf::set(&mut m, "D", formats_obj(&sc.dist));
    pdf::set(&mut m, "A", formats_obj(&sc.area));
    if !sc.volume.is_empty() {
        pdf::set(&mut m, "V", formats_obj(&sc.volume));
    }
    if let Some(t) = sc.target_unit_conversion {
        pdf::set(&mut m, "TargetUnitConversion", real(t));
    }
    Object::Dict(m)
}
