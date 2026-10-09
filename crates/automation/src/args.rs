//! Typed access to a tool's JSON arguments, with messages that say what was expected.

use markupcraft_engine::props::{self, MarkupPatch};
use markupcraft_engine::{Color, Kind, Point, Rect, Scale};
use markupcraft_measure::units::LengthUnit;
use serde_json::Value;

use crate::tools::Tool;
use crate::{Result, ToolError};

/// Most points or pages in one argument.
const MAX_ITEMS: usize = 100_000;

pub struct Args<'a> {
    tool: &'static str,
    v: &'a Value,
}

fn wrong(key: &str, what: &str) -> ToolError {
    ToolError::InvalidArgs(format!("{key} must be {what}"))
}

impl<'a> Args<'a> {
    /// Check `v` against the tool's schema: an object, no unknown keys, required keys present.
    pub fn checked(tool: &'static Tool, v: &'a Value) -> Result<Self> {
        let obj = v
            .as_object()
            .ok_or_else(|| ToolError::InvalidArgs(format!("{}: arguments must be a JSON object", tool.name)))?;
        let schema = (tool.schema)();
        let props = schema["properties"].as_object().cloned().unwrap_or_default();
        if let Some(k) = obj.keys().find(|k| !props.contains_key(*k)) {
            let known: Vec<&str> = props.keys().map(String::as_str).collect();
            return Err(ToolError::InvalidArgs(format!(
                "{}: unknown argument {k:?} (expected: {})",
                tool.name,
                known.join(", ")
            )));
        }
        for r in schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if obj.get(r).is_none_or(Value::is_null) {
                return Err(ToolError::InvalidArgs(format!("{}: missing argument {r}", tool.name)));
            }
        }
        Ok(Self { tool: tool.name, v })
    }

    pub fn tool(&self) -> &'static str {
        self.tool
    }

    pub fn get(&self, key: &str) -> Option<&'a Value> {
        self.v.get(key).filter(|v| !v.is_null())
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    fn missing(&self, key: &str) -> ToolError {
        ToolError::InvalidArgs(format!("{}: missing argument {key}", self.tool))
    }

    pub fn str(&self, key: &str) -> Result<&'a str> {
        self.opt_str(key)?.ok_or_else(|| self.missing(key))
    }

    pub fn opt_str(&self, key: &str) -> Result<Option<&'a str>> {
        self.get(key)
            .map(|v| v.as_str().ok_or_else(|| wrong(key, "a string")))
            .transpose()
    }

    pub fn opt_string(&self, key: &str) -> Result<Option<String>> {
        Ok(self.opt_str(key)?.map(str::to_string))
    }

    pub fn num(&self, key: &str) -> Result<f64> {
        self.opt_num(key)?.ok_or_else(|| self.missing(key))
    }

    pub fn opt_num(&self, key: &str) -> Result<Option<f64>> {
        self.get(key)
            .map(|v| {
                v.as_f64()
                    .filter(|f| f.is_finite())
                    .ok_or_else(|| wrong(key, "a number"))
            })
            .transpose()
    }

    pub fn int(&self, key: &str) -> Result<i64> {
        self.opt_int(key)?.ok_or_else(|| self.missing(key))
    }

    pub fn opt_int(&self, key: &str) -> Result<Option<i64>> {
        self.get(key)
            .map(|v| v.as_i64().ok_or_else(|| wrong(key, "an integer")))
            .transpose()
    }

    pub fn opt_u64(&self, key: &str) -> Result<Option<u64>> {
        self.get(key)
            .map(|v| v.as_u64().ok_or_else(|| wrong(key, "a positive integer")))
            .transpose()
    }

    pub fn opt_bool(&self, key: &str) -> Result<Option<bool>> {
        self.get(key)
            .map(|v| v.as_bool().ok_or_else(|| wrong(key, "true or false")))
            .transpose()
    }

    pub fn bool_or(&self, key: &str, dflt: bool) -> Result<bool> {
        Ok(self.opt_bool(key)?.unwrap_or(dflt))
    }

    /// A 1-based page number → 0-based.
    pub fn page(&self, key: &str) -> Result<usize> {
        self.opt_page(key)?.ok_or_else(|| self.missing(key))
    }

    pub fn opt_page(&self, key: &str) -> Result<Option<usize>> {
        match self.opt_int(key)? {
            None => Ok(None),
            Some(p) if p >= 1 => Ok(Some(p as usize - 1)),
            Some(p) => Err(wrong(key, &format!("a page number from 1 (got {p})"))),
        }
    }

    /// A 1-based position → 0-based.
    pub fn position(&self, key: &str) -> Result<usize> {
        self.page(key)
    }

    /// Pages as `[1, 3, 4]` or `"1-3, 5, 8-"` (to the end) → 0-based, sorted.
    pub fn pages(&self, key: &str, count: usize) -> Result<Vec<usize>> {
        self.opt_pages(key, count)?.ok_or_else(|| self.missing(key))
    }

    pub fn opt_pages(&self, key: &str, count: usize) -> Result<Option<Vec<usize>>> {
        let Some(v) = self.get(key) else { return Ok(None) };
        let mut out = match v {
            Value::String(s) => parse_range(s, count).map_err(|e| wrong(key, &e))?,
            Value::Array(a) => {
                if a.len() > MAX_ITEMS {
                    return Err(wrong(key, "a shorter list"));
                }
                a.iter()
                    .map(|x| match x.as_i64() {
                        Some(p) if p >= 1 => Ok(p as usize - 1),
                        _ => Err(wrong(key, "page numbers from 1")),
                    })
                    .collect::<Result<Vec<usize>>>()?
            }
            _ => return Err(wrong(key, "a list of page numbers or a range like \"1-3, 5\"")),
        };
        if out.is_empty() {
            return Err(wrong(key, "at least one page"));
        }
        out.sort_unstable();
        out.dedup();
        Ok(Some(out))
    }

    pub fn point(&self, key: &str) -> Result<Point> {
        self.opt_point(key)?.ok_or_else(|| self.missing(key))
    }

    pub fn opt_point(&self, key: &str) -> Result<Option<Point>> {
        self.get(key)
            .map(|v| point_of(v).ok_or_else(|| wrong(key, "[x, y]")))
            .transpose()
    }

    pub fn points(&self, key: &str) -> Result<Vec<Point>> {
        self.opt_points(key)?.ok_or_else(|| self.missing(key))
    }

    pub fn opt_points(&self, key: &str) -> Result<Option<Vec<Point>>> {
        let Some(v) = self.get(key) else { return Ok(None) };
        points_of(v)
            .map(Some)
            .ok_or_else(|| wrong(key, "a list of [x, y] points"))
    }

    pub fn opt_rect(&self, key: &str) -> Result<Option<Rect>> {
        self.get(key)
            .map(|v| rect_of(v).ok_or_else(|| wrong(key, "[x0, y0, x1, y1]")))
            .transpose()
    }

    pub fn opt_strings(&self, key: &str) -> Result<Option<Vec<String>>> {
        let Some(v) = self.get(key) else { return Ok(None) };
        let a = v.as_array().ok_or_else(|| wrong(key, "a list of strings"))?;
        a.iter()
            .map(|x| {
                x.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| wrong(key, "a list of strings"))
            })
            .collect::<Result<Vec<_>>>()
            .map(Some)
    }

    pub fn kind(&self, key: &str) -> Result<Kind> {
        let s = self.str(key)?;
        props::kind_from_name(s).ok_or_else(|| {
            ToolError::InvalidArgs(format!(
                "unknown kind {s:?}; creatable kinds: {}",
                props::creatable_kinds().join(", ")
            ))
        })
    }

    pub fn opt_color(&self, key: &str) -> Result<Option<Color>> {
        self.get(key).map(|v| color_of(key, v)).transpose()
    }

    pub fn opt_unit(&self, key: &str) -> Result<Option<LengthUnit>> {
        self.opt_str(key)?
            .map(|u| {
                LengthUnit::from_label(u).ok_or_else(|| wrong(key, "a length unit: in, ft, yd, mi, mm, cm, m, km, pt"))
            })
            .transpose()
    }

    pub fn opt_scale(&self, key: &str) -> Result<Option<Scale>> {
        self.get(key).map(|v| scale_of(key, v)).transpose()
    }

    /// The markup property arguments shared by markup_add and markup_edit.
    pub fn patch(&self) -> Result<MarkupPatch> {
        let mut p = MarkupPatch {
            color: self.opt_color("color")?,
            opacity: self.opt_num("opacity")?,
            fill_opacity: self.opt_num("fill_opacity")?,
            line_width: self.opt_num("width")?,
            subject: self.opt_string("subject")?,
            label: self.opt_string("label")?,
            author: self.opt_string("author")?,
            contents: self.opt_string("contents")?,
            layer: self.opt_string("layer")?,
            status: self.opt_string("status")?,
            checked: self.opt_bool("checked")?,
            locked: self.opt_bool("locked")?,
            line_start: self.opt_string("line_start")?,
            line_end: self.opt_string("line_end")?,
            cloud: self.opt_num("cloud")?,
            font: self.opt_string("font")?,
            font_size: self.opt_num("font_size")?,
            text_color: self.opt_color("text_color")?,
            bold: self.opt_bool("bold")?,
            italic: self.opt_bool("italic")?,
            depth: self.opt_num("depth")?,
            rise_drop: self.opt_num("rise_drop")?,
            scale: self.opt_scale("scale")?,
            ..Default::default()
        };
        // fill: a colour, or null / "none" for no fill. Present-and-null means "remove".
        if let Some(v) = self.v.get("fill") {
            p.fill = Some(match v {
                Value::Null => None,
                Value::String(s) if s.eq_ignore_ascii_case("none") => None,
                v => Some(color_of("fill", v)?),
            });
        }
        if let Some(d) = self.get("dash") {
            let a = d
                .as_array()
                .ok_or_else(|| wrong("dash", "a list of lengths ([] = solid)"))?;
            p.dash = Some(
                a.iter()
                    .map(|x| {
                        x.as_f64()
                            .filter(|f| f.is_finite())
                            .ok_or_else(|| wrong("dash", "numbers"))
                    })
                    .collect::<Result<Vec<f64>>>()?,
            );
        }
        if let Some(c) = self.get("columns") {
            let o = c
                .as_object()
                .ok_or_else(|| wrong("columns", "an object of column id -> value"))?;
            for (k, v) in o {
                let s = match v {
                    Value::String(s) => s.clone(),
                    Value::Null => String::new(),
                    v => v.to_string(),
                };
                p.columns.insert(k.clone(), s);
            }
        }
        p.validate().map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        Ok(p)
    }
}

pub fn point_of(v: &Value) -> Option<Point> {
    let a = v.as_array()?;
    if a.len() != 2 {
        return None;
    }
    let x = a.first()?.as_f64()?;
    let y = a.get(1)?.as_f64()?;
    (x.is_finite() && y.is_finite()).then_some(Point::new(x, y))
}

pub fn points_of(v: &Value) -> Option<Vec<Point>> {
    let a = v.as_array()?;
    if a.len() > MAX_ITEMS {
        return None;
    }
    a.iter().map(point_of).collect()
}

pub fn rect_of(v: &Value) -> Option<Rect> {
    let a = v.as_array()?;
    if a.len() != 4 {
        return None;
    }
    let n: Vec<f64> = a.iter().filter_map(Value::as_f64).filter(|f| f.is_finite()).collect();
    match n.as_slice() {
        [x0, y0, x1, y1] => Some(Rect::new(*x0, *y0, *x1, *y1).normalized()),
        _ => None,
    }
}

fn color_of(key: &str, v: &Value) -> Result<Color> {
    let what = "a colour: \"#RRGGBB\", a name (red, blue, ...) or [r, g, b] from 0 to 1";
    match v {
        Value::String(s) => props::parse_color(s).ok_or_else(|| wrong(key, what)),
        Value::Array(a) if a.len() == 3 => {
            let c: Vec<f64> = a
                .iter()
                .filter_map(Value::as_f64)
                .filter(|f| (0.0..=1.0).contains(f))
                .collect();
            match c.as_slice() {
                [r, g, b] => Ok(Color::rgb(*r, *g, *b)),
                _ => Err(wrong(key, what)),
            }
        }
        _ => Err(wrong(key, what)),
    }
}

/// A scale: `{"kind": "architectural", "paper_inches": 0.125, "real_feet": 1}`,
/// `{"kind": "engineering", "feet_per_inch": 20}`, `{"kind": "ratio", "ratio": 100, "unit": "m"}`
/// (1:100, measured in metres) or `{"kind": "custom", "scale": {...}}` (the model's Scale).
fn scale_of(key: &str, v: &Value) -> Result<Scale> {
    let o = v.as_object().ok_or_else(|| wrong(key, "a scale object with a kind"))?;
    let num = |k: &str| {
        o.get(k)
            .and_then(Value::as_f64)
            .filter(|f| f.is_finite() && *f > 0.0)
            .ok_or_else(|| wrong(&format!("{key}.{k}"), "a positive number"))
    };
    let sc = match o.get("kind").and_then(Value::as_str).unwrap_or("") {
        "architectural" => Scale::architectural(num("paper_inches")?, num("real_feet")?),
        "engineering" => Scale::engineering(num("feet_per_inch")?),
        "ratio" => {
            let unit = o
                .get("unit")
                .and_then(Value::as_str)
                .and_then(LengthUnit::from_label)
                .ok_or_else(|| wrong(&format!("{key}.unit"), "a length unit: in, ft, mm, cm, m ..."))?;
            let pt = LengthUnit::Point.meters() / unit.meters();
            markupcraft_engine::decimal_scale(num("ratio")? * pt, unit)
        }
        "custom" => serde_json::from_value::<Scale>(o.get("scale").cloned().unwrap_or(Value::Null))
            .map_err(|e| wrong(&format!("{key}.scale"), &format!("the model's Scale object ({e})")))?,
        _ => {
            return Err(wrong(
                &format!("{key}.kind"),
                "architectural, engineering, ratio or custom",
            ));
        }
    };
    if !sc.valid() {
        return Err(wrong(key, "a scale with a usable conversion"));
    }
    Ok(sc)
}

/// `"1-3, 5, 8-"` → 0-based pages; `8-` runs to the end.
pub fn parse_range(text: &str, count: usize) -> std::result::Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for part in text.split([',', ';']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let num = |s: &str| -> std::result::Result<usize, String> {
            s.trim()
                .parse::<usize>()
                .ok()
                .filter(|n| *n >= 1)
                .ok_or_else(|| format!("a page range like \"1-3, 5\" (bad part {part:?})"))
        };
        let (a, b) = match part.split_once('-') {
            Some((a, b)) if b.trim().is_empty() => (num(a)?, count),
            Some((a, b)) => (num(a)?, num(b)?),
            None => (num(part)?, num(part)?),
        };
        let (a, b) = (a.min(b), a.max(b));
        if a == 0 || b > count {
            return Err(format!("pages within 1-{count} (got {b})"));
        }
        if out.len() + (b - a) > MAX_ITEMS {
            return Err("a shorter range".into());
        }
        out.extend(a - 1..b);
    }
    Ok(out)
}
