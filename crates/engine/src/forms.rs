//! Forms: list and fill AcroForm fields, create text, check box, radio, combo box, list box
//! and button fields, reset them, and flatten them into the page.
//!
//! Field handling is PdfCraft's `pdfcraft-forms` (values with regenerated appearances, new
//! fields with Acrobat's naming and look); flattening is `pdfcraft-edit`. Each operation is
//! one undoable step.

use markupcraft_geom::Rect;
use pdfcraft_forms::{FieldKind, FieldValue, NewField};

use crate::{Result, Session, invalid};

/// Most values one fill may set.
pub const MAX_VALUES: usize = 10_000;

/// A form field as listed.
#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    pub name: String,
    /// text, checkbox, radio, button, combo, list, signature
    pub kind: &'static str,
    /// Text, the checked state (none when off), or the selected export values.
    pub value: Vec<String>,
    /// Choice options' export values; check box / radio: the "on" states of its widgets.
    pub options: Vec<String>,
    pub page: Option<usize>,
    pub rect: Option<Rect>,
    pub read_only: bool,
    pub required: bool,
}

/// A value to fill in.
#[derive(Debug, Clone, PartialEq)]
pub enum FillValue {
    Text(String),
    /// Check boxes; for a radio group, `true` is not enough (name the button).
    Bool(bool),
    /// Combo and list boxes (several only for multi-select lists).
    Choice(Vec<String>),
}

/// The kind of field to create.
#[derive(Debug, Clone, PartialEq)]
pub enum NewFieldKind {
    Text {
        multiline: bool,
    },
    CheckBox,
    Radio {
        group: Option<String>,
        export: String,
    },
    Combo {
        options: Vec<String>,
        editable: bool,
    },
    List {
        options: Vec<String>,
        multi: bool,
    },
    Button {
        caption: String,
    },
    /// An empty signature field, to be signed later.
    Signature,
}

fn kind_name(k: FieldKind) -> &'static str {
    match k {
        FieldKind::Text => "text",
        FieldKind::CheckBox => "checkbox",
        FieldKind::Radio => "radio",
        FieldKind::PushButton => "button",
        FieldKind::Combo => "combo",
        FieldKind::List => "list",
        FieldKind::Signature => "signature",
    }
}

fn form_err(e: impl std::fmt::Display) -> crate::EngineError {
    invalid(e.to_string())
}

impl Session {
    /// Every terminal form field.
    pub fn form_fields(&self) -> Vec<FormField> {
        pdfcraft_forms::fields(&self.file.cos)
            .into_iter()
            .map(|f| {
                let w = f.widgets.first();
                let options = match f.kind {
                    FieldKind::CheckBox | FieldKind::Radio => {
                        let mut v: Vec<String> = f.widgets.iter().filter_map(|w| w.on_state.clone()).collect();
                        v.dedup();
                        v
                    }
                    _ => f.options.iter().map(|(e, _)| e.clone()).collect(),
                };
                FormField {
                    kind: kind_name(f.kind),
                    value: f.value.clone(),
                    options,
                    page: w.and_then(|w| w.page),
                    rect: w.map(|w| Rect::new(w.rect[0], w.rect[1], w.rect[2], w.rect[3])),
                    read_only: f.read_only(),
                    required: f.has(pdfcraft_forms::flags::REQUIRED),
                    name: f.name,
                }
            })
            .collect()
    }

    /// Fill fields by name, as one undoable step. Returns how many were set.
    pub fn form_fill(&mut self, values: &[(String, FillValue)]) -> Result<usize> {
        if values.is_empty() {
            return Err(invalid("no values given"));
        }
        if values.len() > MAX_VALUES {
            return Err(invalid(format!("at most {MAX_VALUES} values at once")));
        }
        let fields = pdfcraft_forms::fields(&self.file.cos);
        if fields.is_empty() {
            return Err(invalid("the document has no form fields"));
        }
        let mut set = Vec::with_capacity(values.len());
        for (name, v) in values {
            let f = fields
                .iter()
                .find(|f| &f.name == name)
                .ok_or_else(|| invalid(format!("there is no field named {name:?} (form_list shows them)")))?;
            let value = match (f.kind, v) {
                (FieldKind::Text, FillValue::Text(t)) => FieldValue::Text(t.clone()),
                (FieldKind::CheckBox, FillValue::Bool(b)) => FieldValue::Check(*b),
                (FieldKind::CheckBox, FillValue::Text(t)) => FieldValue::Check(!t.is_empty() && t != "Off"),
                (FieldKind::Radio, FillValue::Text(t)) => {
                    FieldValue::Radio((!t.is_empty() && t != "Off").then(|| t.clone()))
                }
                (FieldKind::Radio, FillValue::Bool(false)) => FieldValue::Radio(None),
                (FieldKind::Combo | FieldKind::List, FillValue::Choice(c)) => FieldValue::Choice(c.clone()),
                (FieldKind::Combo | FieldKind::List, FillValue::Text(t)) => FieldValue::Choice(vec![t.clone()]),
                (k, v) => {
                    return Err(invalid(format!(
                        "{name:?} is a {} field; {v:?} does not fit it",
                        kind_name(k)
                    )));
                }
            };
            set.push((name.clone(), value));
        }
        self.edit("Fill Form", |s| {
            for (name, value) in &set {
                pdfcraft_forms::set_value(&mut s.file.cos, name, value).map_err(form_err)?;
            }
            Ok((set.len(), true))
        })
    }

    /// Create a field on `page` (0-based) at `rect`; returns its name.
    pub fn form_add_field(
        &mut self,
        page: usize,
        rect: Rect,
        kind: &NewFieldKind,
        name: Option<&str>,
    ) -> Result<String> {
        self.page(page)?;
        let r = rect.normalized();
        if !([r.x0, r.y0, r.x1, r.y1]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= crate::geometry::MAX_COORD)
            && r.width() >= 1.0
            && r.height() >= 1.0)
        {
            return Err(invalid(
                "a field needs a finite rectangle at least 1 point on each side",
            ));
        }
        if let Some(n) = name
            && (n.is_empty() || n.chars().count() > 256 || n.contains('.'))
        {
            return Err(invalid("a field name is 1 to 256 characters without dots"));
        }
        let check_options = |o: &[String]| -> Result<()> {
            if o.is_empty() || o.len() > 10_000 || o.iter().any(|x| x.chars().count() > 1000) {
                return Err(invalid("a choice field needs 1 to 10000 options"));
            }
            Ok(())
        };
        let nf = match kind {
            NewFieldKind::Text { multiline } => NewField::Text { multiline: *multiline },
            NewFieldKind::CheckBox => NewField::CheckBox,
            NewFieldKind::Radio { group, export } => {
                if export.trim().is_empty() {
                    return Err(invalid("a radio button needs an export value (its choice name)"));
                }
                NewField::Radio {
                    group: group.clone(),
                    export: export.clone(),
                }
            }
            NewFieldKind::Combo { options, editable } => {
                check_options(options)?;
                NewField::Combo {
                    options: options.clone(),
                    editable: *editable,
                }
            }
            NewFieldKind::List { options, multi } => {
                check_options(options)?;
                NewField::List {
                    options: options.clone(),
                    multi: *multi,
                }
            }
            NewFieldKind::Button { caption } => NewField::Button {
                caption: caption.clone(),
            },
            NewFieldKind::Signature => NewField::Signature,
        };
        self.edit("Add Form Field", |s| {
            let vp = std::mem::take(&mut s.vp_changed);
            Session::flush(&mut s.file.cos, &mut s.doc, &vp);
            let name = pdfcraft_forms::add_field(&mut s.file.cos, page, [r.x0, r.y0, r.x1, r.y1], &nf, name)
                .map_err(form_err)?;
            s.reload();
            Ok((name, true))
        })
    }

    /// Reset fields (all when `names` is `None`) to their default values.
    pub fn form_reset(&mut self, names: Option<&[String]>) -> Result<usize> {
        self.edit("Reset Form", |s| {
            let n = pdfcraft_forms::reset(&mut s.file.cos, names).map_err(form_err)?;
            Ok((n, n > 0))
        })
    }

    /// Flatten form fields on `pages` (0-based; empty = all) into page content: they keep
    /// their look and stop being fields. Returns how many widgets were flattened.
    pub fn form_flatten(&mut self, pages: &[usize]) -> Result<usize> {
        let pages: Vec<usize> = if pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            pages.to_vec()
        };
        for p in &pages {
            self.page(*p)?;
        }
        if pdfcraft_forms::fields(&self.file.cos).is_empty() {
            return Err(invalid("the document has no form fields to flatten"));
        }
        self.edit("Flatten Form Fields", |s| {
            let vp = std::mem::take(&mut s.vp_changed);
            Session::flush(&mut s.file.cos, &mut s.doc, &vp);
            let n = pdfcraft_edit::flatten(&mut s.file.cos, &pages, false, true).map_err(form_err)?;
            s.file.cos.require_full_save();
            s.reload();
            Ok((n, n > 0))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};
    use markupcraft_render::text::TextExtractor;

    #[test]
    fn create_fill_reset_and_flatten_fields() {
        let bytes = pdf(&[SyntheticPage::new(612.0, 792.0, text(50.0, 740.0, 12.0, "Request"))]);
        let mut s = Session::from_bytes(bytes, "form.pdf").unwrap();
        assert!(s.form_fields().is_empty());
        let r = |x: f64, y: f64| Rect::new(x, y, x + 150.0, y + 20.0);
        let name = s
            .form_add_field(
                0,
                r(50.0, 700.0),
                &NewFieldKind::Text { multiline: false },
                Some("Project"),
            )
            .unwrap();
        assert_eq!(name, "Project");
        assert_eq!(
            s.form_add_field(0, Rect::new(50.0, 660.0, 66.0, 676.0), &NewFieldKind::CheckBox, None)
                .unwrap(),
            "Check Box1"
        );
        for (i, export) in ["Yes", "No"].iter().enumerate() {
            let radio = NewFieldKind::Radio {
                group: Some("Approved".into()),
                export: export.to_string(),
            };
            s.form_add_field(
                0,
                Rect::new(50.0 + 30.0 * i as f64, 620.0, 66.0 + 30.0 * i as f64, 636.0),
                &radio,
                None,
            )
            .unwrap();
        }
        let combo = NewFieldKind::Combo {
            options: vec!["Draft".into(), "Final".into()],
            editable: false,
        };
        s.form_add_field(0, r(50.0, 580.0), &combo, Some("Stage")).unwrap();
        let list = NewFieldKind::List {
            options: vec!["A".into(), "B".into(), "C".into()],
            multi: true,
        };
        s.form_add_field(0, Rect::new(50.0, 480.0, 200.0, 560.0), &list, Some("Trades"))
            .unwrap();
        s.form_add_field(
            0,
            r(300.0, 700.0),
            &NewFieldKind::Button {
                caption: "Submit".into(),
            },
            None,
        )
        .unwrap();
        let fields = s.form_fields();
        let kinds: Vec<&str> = fields.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            ["text", "checkbox", "radio", "combo", "list", "button"],
            "{fields:#?}"
        );
        let radio = fields.iter().find(|f| f.kind == "radio").unwrap();
        assert_eq!(
            (radio.name.as_str(), radio.options.clone()),
            ("Approved", vec!["Yes".into(), "No".into()])
        );

        let n = s
            .form_fill(&[
                ("Project".into(), FillValue::Text("Tower B".into())),
                ("Check Box1".into(), FillValue::Bool(true)),
                ("Approved".into(), FillValue::Text("No".into())),
                ("Stage".into(), FillValue::Text("Final".into())),
                ("Trades".into(), FillValue::Choice(vec!["A".into(), "C".into()])),
            ])
            .unwrap();
        assert_eq!(n, 5);
        let v = |s: &Session, n: &str| s.form_fields().into_iter().find(|f| f.name == n).unwrap().value;
        assert_eq!(v(&s, "Project"), ["Tower B"]);
        assert_eq!(v(&s, "Approved"), ["No"]);
        assert_eq!(v(&s, "Trades"), ["A", "C"]);
        assert!(!v(&s, "Check Box1").is_empty());
        // A wrong value or name changes nothing.
        assert!(s.form_fill(&[("Project".into(), FillValue::Bool(true))]).is_err());
        assert!(s.form_fill(&[("Nope".into(), FillValue::Text("x".into()))]).is_err());
        assert_eq!(v(&s, "Project"), ["Tower B"]);

        // Flatten: the text stays on the page, the fields are gone.
        assert!(s.form_flatten(&[]).unwrap() >= 7);
        assert!(s.form_fields().is_empty());
        let mut t = TextExtractor::new(s.current_bytes().unwrap());
        assert_eq!(t.find("Tower B"), vec![(0, 1)]);
        s.undo().unwrap();
        assert_eq!(s.form_fields().len(), 6);
        assert_eq!(s.form_reset(None).unwrap(), 5, "every field with a value");
        assert!(v(&s, "Project").iter().all(|x| x.is_empty()));
        // Errors.
        assert!(s.form_add_field(9, r(0.0, 0.0), &NewFieldKind::CheckBox, None).is_err());
        assert!(
            s.form_add_field(0, Rect::new(0.0, 0.0, 0.0, 5.0), &NewFieldKind::CheckBox, None)
                .is_err()
        );
        let empty = NewFieldKind::Combo {
            options: vec![],
            editable: true,
        };
        assert!(s.form_add_field(0, r(0.0, 0.0), &empty, None).is_err());
    }
}
