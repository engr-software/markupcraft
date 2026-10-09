//! A content-stream writer for appearance streams (`/AP /N`).
//!
//! Numbers are written with three decimals, like the C++ writer, which Revu reads back.

use std::fmt::Write;

use markupcraft_geom::Point;
use markupcraft_geom::path::{Path, Seg};
use markupcraft_model::Color;

#[derive(Default, Debug, Clone)]
pub struct Ap {
    pub s: String,
}

/// `v` with three fixed decimals (`-0.000` becomes `0.000`).
pub fn f3(v: f64) -> String {
    if !v.is_finite() {
        return "0.000".into();
    }
    let t = format!("{v:.3}");
    if t == "-0.000" { "0.000".into() } else { t }
}

impl Ap {
    pub fn new() -> Self {
        Self::default()
    }
    /// Raw operator text.
    pub fn op(&mut self, t: &str) -> &mut Self {
        self.s.push_str(t);
        self
    }
    /// Numbers separated by spaces, then the operator and a space.
    pub fn nums(&mut self, vals: &[f64], op: &str) -> &mut Self {
        for v in vals {
            self.s.push_str(&f3(*v));
            self.s.push(' ');
        }
        self.s.push_str(op);
        self.s.push(' ');
        self
    }
    pub fn move_to(&mut self, p: Point) -> &mut Self {
        self.nums(&[p.x, p.y], "m")
    }
    pub fn line_to(&mut self, p: Point) -> &mut Self {
        self.nums(&[p.x, p.y], "l")
    }
    pub fn curve_to(&mut self, a: Point, b: Point, c: Point) -> &mut Self {
        self.nums(&[a.x, a.y, b.x, b.y, c.x, c.y], "c")
    }
    pub fn stroke_rgb(&mut self, c: &Color) -> &mut Self {
        self.nums(&[c.r, c.g, c.b], "RG")
    }
    pub fn fill_rgb(&mut self, c: &Color) -> &mut Self {
        self.nums(&[c.r, c.g, c.b], "rg")
    }
    /// A polyline through `pts`, optionally closed, without painting.
    pub fn path(&mut self, pts: &[Point], closed: bool) -> &mut Self {
        if let Some(first) = pts.first() {
            self.move_to(*first);
            for p in pts.iter().skip(1) {
                self.line_to(*p);
            }
            if closed {
                self.op("h ");
            }
        }
        self
    }
    /// Path operators for `p`, without painting.
    pub fn segs(&mut self, p: &Path) -> &mut Self {
        for s in p {
            match *s {
                Seg::Move(a) => self.move_to(a),
                Seg::Line(a) => self.line_to(a),
                Seg::Curve(a, b, c) => self.curve_to(a, b, c),
                Seg::Close => self.op("h "),
            };
        }
        self
    }
    pub fn newline(&mut self) -> &mut Self {
        self.s.push('\n');
        self
    }
    /// A literal string escaped for `( ... ) Tj`.
    pub fn text_literal(t: &str) -> String {
        let mut out = String::with_capacity(t.len() + 2);
        for c in t.chars() {
            match c {
                '(' | ')' | '\\' => {
                    out.push('\\');
                    out.push(c);
                }
                c if (c as u32) < 256 => out.push(c),
                _ => out.push('?'),
            }
        }
        out
    }
    pub fn write_fmt_str(&mut self, args: std::fmt::Arguments<'_>) {
        let _ = self.s.write_fmt(args);
    }
    /// Bytes for the stream, Latin-1 (WinAnsi for the base-14 fonts).
    pub fn bytes(&self) -> Vec<u8> {
        self.s
            .chars()
            .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
            .collect()
    }
}
