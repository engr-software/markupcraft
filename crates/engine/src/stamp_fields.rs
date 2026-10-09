//! Interactive stamps: stamps with form-like fields a reviewer fills in when placing the stamp
//! and changes later (check boxes, text, choices), on top of the dynamic fields of
//! [`crate::stamps`].
//!
//! ```text
//! {check:Label}  {check:Label=on}         a check box, drawn [X] Label or [ ] Label
//! {field:Label}  {field:Label=Default}    a text field, drawn Label: value (or a blank line)
//! {choice:Label=A|B|C}                    one of the options, drawn Label: value
//! ```
//!
//! The stamp keeps its template (dynamic fields already filled) and the values in its column
//! data (`/PCColumnData`: `stamp_template`, `sf_<label>`), so the fields stay editable after
//! saving and reopening.

use std::collections::BTreeMap;

use markupcraft_model::{Color, Rect};

use crate::props::MarkupPatch;
use crate::stamps::{StampPlace, StampSource, expand_fields};
use crate::{Result, Session, invalid};

/// The column holding the template.
pub const TEMPLATE_KEY: &str = "stamp_template";

/// What kind of field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StampFieldKind {
    Check,
    Text,
    Choice,
}

/// One field of an interactive stamp.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct StampField {
    pub label: String,
    pub kind: StampFieldKind,
    /// Check boxes: "on" or "". Choices: one of `options`.
    pub value: String,
    pub options: Vec<String>,
}

/// The `{...}` fields of a template: (start, end, inner).
fn spans(t: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(open) = t.get(i..).and_then(|x| x.find('{')).map(|o| o + i) {
        let Some(close) = t.get(open..).and_then(|x| x.find('}')).map(|c| c + open) else {
            break;
        };
        if let Some(inner) = t.get(open + 1..close) {
            out.push((open, close + 1, inner));
        }
        i = close + 1;
    }
    out
}

fn parse(inner: &str) -> Option<StampField> {
    let (kind, rest) = inner.split_once(':')?;
    let (label, default) = rest.split_once('=').unwrap_or((rest, ""));
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 100 {
        return None;
    }
    let (kind, value, options) = match kind {
        "check" => (
            StampFieldKind::Check,
            if matches!(default.trim(), "on" | "yes" | "true" | "1" | "x" | "X") {
                "on".to_string()
            } else {
                String::new()
            },
            Vec::new(),
        ),
        "field" => (StampFieldKind::Text, default.to_string(), Vec::new()),
        "choice" => {
            let options: Vec<String> = default
                .split('|')
                .map(|o| o.trim().to_string())
                .filter(|o| !o.is_empty())
                .take(50)
                .collect();
            (
                StampFieldKind::Choice,
                options.first().cloned().unwrap_or_default(),
                options,
            )
        }
        _ => return None,
    };
    Some(StampField {
        label: label.to_string(),
        kind,
        value,
        options,
    })
}

/// The interactive fields of a template, in order, first of each label.
pub fn interactive_fields(tmpl: &str) -> Vec<StampField> {
    let mut out: Vec<StampField> = Vec::new();
    for (_, _, inner) in spans(tmpl) {
        if let Some(f) = parse(inner)
            && !out.iter().any(|x| x.label == f.label)
        {
            out.push(f);
        }
    }
    out
}

/// Whether a template has interactive fields.
pub fn is_interactive(tmpl: &str) -> bool {
    !interactive_fields(tmpl).is_empty()
}

/// The column key of a field's value.
pub fn value_key(label: &str) -> String {
    let k: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    format!("sf_{k}")
}

/// The stamp text with every interactive field drawn with `values` (label to value; missing
/// labels take the template's default).
pub fn render_interactive(tmpl: &str, values: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut last = 0;
    for (a, b, inner) in spans(tmpl) {
        out.push_str(tmpl.get(last..a).unwrap_or_default());
        match parse(inner) {
            Some(f) => {
                let v = values.get(&f.label).cloned().unwrap_or(f.value);
                out.push_str(&match f.kind {
                    StampFieldKind::Check => {
                        format!("[{}] {}", if v == "on" { "X" } else { " " }, f.label)
                    }
                    StampFieldKind::Text | StampFieldKind::Choice => {
                        format!(
                            "{}: {}",
                            f.label,
                            if v.trim().is_empty() { "________" } else { v.trim() }
                        )
                    }
                });
            }
            None => out.push_str(tmpl.get(a..b).unwrap_or_default()),
        }
        last = b;
    }
    out.push_str(tmpl.get(last..).unwrap_or_default());
    out
}

fn check_values(fields: &[StampField], values: &BTreeMap<String, String>) -> Result<()> {
    for (k, v) in values {
        let f = fields
            .iter()
            .find(|f| f.label == *k)
            .ok_or_else(|| invalid(format!("the stamp has no field {k:?}")))?;
        if v.chars().count() > 500 {
            return Err(invalid(format!("the value of {k:?} is too long")));
        }
        if f.kind == StampFieldKind::Choice && !v.is_empty() && !f.options.contains(v) {
            return Err(invalid(format!("{k:?} is one of {}", f.options.join(", "))));
        }
        if f.kind == StampFieldKind::Check && !matches!(v.as_str(), "on" | "") {
            return Err(invalid(format!("check box {k:?} is \"on\" or \"\"")));
        }
    }
    Ok(())
}

impl Session {
    /// Place an interactive stamp: `text` is a stamp template (dynamic fields, `{prompt:}`
    /// answered from `answers`, and interactive fields filled from `values`). Returns its id.
    /// One undo step.
    pub fn place_interactive_stamp(
        &mut self,
        page: usize,
        at: StampPlace,
        text: &str,
        color: Color,
        answers: &BTreeMap<String, String>,
        values: &BTreeMap<String, String>,
    ) -> Result<String> {
        let fields = interactive_fields(text);
        if fields.is_empty() {
            return Err(invalid("the stamp has no {check:}, {field:} or {choice:} fields"));
        }
        check_values(&fields, values)?;
        let ctx = self.stamp_context(page)?;
        let stored = expand_fields(text, &ctx, answers);
        let shown = render_interactive(&stored, values);
        self.set_merge_key(Some("place-interactive-stamp"));
        let r = (|| -> Result<String> {
            let id = self.place_stamp(
                page,
                at,
                &StampSource::Text { text: shown, color },
                None,
                &BTreeMap::new(),
                None,
            )?;
            let mut columns = BTreeMap::new();
            columns.insert(TEMPLATE_KEY.to_string(), stored.clone());
            for f in &fields {
                let v = values.get(&f.label).cloned().unwrap_or_else(|| f.value.clone());
                columns.insert(value_key(&f.label), v);
            }
            let patch = MarkupPatch {
                columns,
                subject: Some("Interactive Stamp".into()),
                ..Default::default()
            };
            self.set_properties(std::slice::from_ref(&id), &patch)?;
            Ok(id)
        })();
        self.set_merge_key(None);
        self.seal();
        r
    }

    /// The fields of interactive stamp `id`, with their current values.
    pub fn stamp_fields(&self, id: &str) -> Result<Vec<StampField>> {
        let m = self.markup(id)?;
        let tmpl = m
            .column_data
            .get(TEMPLATE_KEY)
            .ok_or_else(|| invalid("this markup is not an interactive stamp"))?;
        Ok(interactive_fields(tmpl)
            .into_iter()
            .map(|mut f| {
                if let Some(v) = m.column_data.get(&value_key(&f.label)) {
                    f.value = v.clone();
                }
                f
            })
            .collect())
    }

    /// Fill in fields of interactive stamp `id` (label to value; check boxes take "on" or "").
    /// The stamp is redrawn. Undoable.
    pub fn set_stamp_fields(&mut self, id: &str, values: &BTreeMap<String, String>) -> Result<()> {
        let fields = self.stamp_fields(id)?;
        check_values(&fields, values)?;
        let tmpl = self
            .markup(id)?
            .column_data
            .get(TEMPLATE_KEY)
            .cloned()
            .unwrap_or_default();
        let mut all: BTreeMap<String, String> = fields.iter().map(|f| (f.label.clone(), f.value.clone())).collect();
        for (k, v) in values {
            all.insert(k.clone(), v.clone());
        }
        let mut columns = BTreeMap::new();
        for (k, v) in &all {
            columns.insert(value_key(k), v.clone());
        }
        let patch = MarkupPatch {
            contents: Some(render_interactive(&tmpl, &all)),
            columns,
            ..Default::default()
        };
        self.set_properties(&[id.to_string()], &patch)?;
        Ok(())
    }
}

/// A box for a stamp centred at a point (the default text stamp size).
pub fn default_box(cx: f64, cy: f64) -> Rect {
    Rect::new(cx - 85.0, cy - 30.0, cx + 85.0, cy + 30.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf};

    #[test]
    fn interactive_stamp_fields_fill_on_placing_and_stay_editable() {
        let tmpl = "REVIEWED\n{check:No Exceptions} {check:Revise=on}\n{choice:Action=Resubmit|Record}\n{field:By} {date:yyyy}";
        let f = interactive_fields(tmpl);
        assert_eq!(f.len(), 4);
        assert_eq!(f.get(1).map(|x| x.value.as_str()), Some("on"));
        assert_eq!(f.get(2).map(|x| x.options.len()), Some(2));
        let p = std::env::temp_dir().join(format!("markupcraft-istamp-{}.pdf", std::process::id()));
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, "")]), &p).unwrap();
        let mut v = BTreeMap::new();
        v.insert("By".to_string(), "QA".to_string());
        let id = s
            .place_interactive_stamp(
                0,
                StampPlace::Center(markupcraft_model::Point::new(300.0, 400.0)),
                tmpl,
                Color::RED,
                &BTreeMap::new(),
                &v,
            )
            .unwrap();
        let c = s.markup(&id).unwrap().contents.clone();
        assert!(
            c.contains("[ ] No Exceptions")
                && c.contains("[X] Revise")
                && c.contains("Action: Resubmit")
                && c.contains("By: QA"),
            "{c}"
        );
        assert!(!c.contains("{date"), "dynamic fields are filled: {c}");
        // Change them later.
        let mut v = BTreeMap::new();
        v.insert("No Exceptions".to_string(), "on".to_string());
        v.insert("Action".to_string(), "Record".to_string());
        s.set_stamp_fields(&id, &v).unwrap();
        let c = s.markup(&id).unwrap().contents.clone();
        assert!(
            c.contains("[X] No Exceptions") && c.contains("Action: Record") && c.contains("By: QA"),
            "{c}"
        );
        v.insert("Action".to_string(), "Nope".to_string());
        assert!(s.set_stamp_fields(&id, &v).is_err());
        // They survive a save.
        s.save(true).unwrap();
        let t = Session::open(&p).unwrap();
        let id2 = t.doc().markups.first().unwrap().id.clone();
        let f = t.stamp_fields(&id2).unwrap();
        assert_eq!(
            f.iter().find(|x| x.label == "Action").map(|x| x.value.as_str()),
            Some("Record")
        );
        // One undo removes the placed stamp entirely (placing is one step).
        let mut u = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, "")]), &p).unwrap();
        u.place_interactive_stamp(
            0,
            StampPlace::Rect(default_box(300.0, 300.0)),
            tmpl,
            Color::RED,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        u.undo().unwrap();
        assert!(u.doc().markups.is_empty());
    }
}
