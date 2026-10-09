//! Form field properties and actions (Revu's field Properties: General, Options, Actions), and
//! dynamic XFA forms laid out as ordinary pages and fields (pdfcraft-xfa), so they can be
//! filled like any AcroForm.

pub use pdfcraft_forms::{FieldAction, Trigger};

use crate::{Result, Session, invalid};

/// What a field's Properties change (`None` = unchanged).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldEdit {
    /// A new name.
    pub name: Option<String>,
    pub tooltip: Option<String>,
    pub read_only: Option<bool>,
    pub required: Option<bool>,
    pub multiline: Option<bool>,
    /// Most characters (`Some(None)` = no limit).
    pub max_len: Option<Option<usize>>,
    /// Choice options.
    pub options: Option<Vec<String>>,
    /// Font size, 0 = auto.
    pub font_size: Option<f64>,
    /// Alignment: 0 left, 1 centre, 2 right.
    pub align: Option<i64>,
    /// The value Reset restores.
    pub default_value: Option<Option<String>>,
    pub locked: Option<bool>,
}

fn err(e: impl std::fmt::Display) -> crate::EngineError {
    invalid(e.to_string())
}

/// A trigger by its id (mouse_up, mouse_down, mouse_enter, mouse_exit, on_focus, on_blur).
pub fn trigger(id: &str) -> Option<Trigger> {
    Trigger::from_id(id)
}

impl Session {
    /// Change a field's properties; returns its (possibly new) name. Undoable.
    pub fn form_set_props(&mut self, name: &str, e: &FieldEdit) -> Result<String> {
        if let Some(n) = &e.name
            && (n.trim().is_empty() || n.chars().count() > 200)
        {
            return Err(invalid("a field name has 1 to 200 characters"));
        }
        if let Some(Some(m)) = e.max_len
            && m > 100_000
        {
            return Err(invalid("at most 100,000 characters"));
        }
        if let Some(s) = e.font_size
            && !(s.is_finite() && (0.0..=144.0).contains(&s))
        {
            return Err(invalid("font size is 0 (auto) to 144"));
        }
        let props = pdfcraft_forms::FieldProps {
            name: e.name.clone(),
            tooltip: e.tooltip.clone(),
            read_only: e.read_only,
            required: e.required,
            multiline: e.multiline,
            max_len: e.max_len,
            options: e.options.clone(),
            font_size: e.font_size,
            quadding: e.align.map(|q| q.clamp(0, 2)),
            default_value: e.default_value.clone(),
            locked: e.locked,
            ..Default::default()
        };
        self.edit("Field Properties", |s| {
            let n = pdfcraft_forms::set_props(&mut s.file.cos, name, &props).map_err(err)?;
            Ok((n, true))
        })
    }

    /// A field's actions by trigger.
    pub fn form_actions(&self, name: &str) -> Result<Vec<(Trigger, FieldAction)>> {
        pdfcraft_forms::field_actions(&self.file.cos, name).map_err(err)
    }

    /// Set (or with `None` remove) the action a field runs on `trigger`. Undoable.
    pub fn form_set_action(&mut self, name: &str, trigger: Trigger, action: Option<FieldAction>) -> Result<usize> {
        let mut list = self.form_actions(name)?;
        list.retain(|(t, _)| *t != trigger);
        if let Some(a) = action {
            list.push((trigger, a));
        }
        let n = list.len();
        self.edit("Field Actions", |s| {
            pdfcraft_forms::set_field_actions(&mut s.file.cos, name, &list).map_err(err)?;
            Ok((n, true))
        })
    }

    /// A dynamic XFA form still showing its placeholder pages.
    pub fn is_xfa_form(&self) -> bool {
        pdfcraft_xfa::is_dynamic(&self.file.cos)
    }

    /// Lay a dynamic XFA form out as pages and AcroForm fields (holding its data), so it can
    /// be filled. Returns (pages, fields, warnings). Undoable.
    pub fn xfa_render(&mut self) -> Result<(usize, usize, Vec<String>)> {
        if !self.is_xfa_form() {
            return Err(invalid("the document is not a dynamic XFA form"));
        }
        self.graph_edit("Lay Out XFA Form", |cos, _| {
            let r = pdfcraft_xfa::render_into(cos).map_err(err)?;
            cos.require_full_save();
            Ok((r.pages, r.fields, r.warnings))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::NewFieldKind;
    use crate::synthetic::{SyntheticPage, pdf};
    use markupcraft_geom::Rect;

    #[test]
    fn properties_actions_and_xfa() {
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, String::new())]), "f.pdf").unwrap();
        let name = s
            .form_add_field(
                0,
                Rect::new(100.0, 700.0, 300.0, 720.0),
                &NewFieldKind::Text { multiline: false },
                None,
            )
            .unwrap();
        let e = FieldEdit {
            name: Some("Project".into()),
            tooltip: Some("The project name".into()),
            required: Some(true),
            max_len: Some(Some(40)),
            ..Default::default()
        };
        let n = s.form_set_props(&name, &e).unwrap();
        assert_eq!(n, "Project");
        let f = s.form_fields().into_iter().find(|f| f.name == "Project").unwrap();
        assert!(f.required);
        s.form_set_action(
            "Project",
            Trigger::MouseUp,
            Some(FieldAction::Uri("https://example.com".into())),
        )
        .unwrap();
        let a = s.form_actions("Project").unwrap();
        assert_eq!(
            a,
            vec![(Trigger::MouseUp, FieldAction::Uri("https://example.com".into()))]
        );
        s.form_set_action("Project", Trigger::MouseUp, None).unwrap();
        assert!(s.form_actions("Project").unwrap().is_empty());
        assert!(
            s.form_set_props(
                "Project",
                &FieldEdit {
                    font_size: Some(500.0),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(!s.is_xfa_form());
        assert!(s.xfa_render().is_err());
        assert_eq!(trigger("on_blur"), Some(Trigger::OnBlur));

        // a dynamic XFA form becomes pages and fields that fill like any form
        let shell = pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::template(3));
        let mut x = Session::from_bytes(shell, "x.pdf").unwrap();
        assert!(x.is_xfa_form());
        let (pages, fields, _) = x.xfa_render().unwrap();
        assert!(pages >= 1 && fields >= 1, "{pages} {fields}");
        assert!(!x.is_xfa_form());
        let first = x.form_fields().into_iter().find(|f| f.kind == "text").unwrap();
        x.form_fill(&[(first.name.clone(), crate::forms::FillValue::Text("filled".into()))])
            .unwrap();
    }
}
