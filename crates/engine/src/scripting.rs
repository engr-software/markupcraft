//! JavaScript: the document's own scripts (document-level `/Names /JavaScript` and a
//! JavaScript `/OpenAction`) and the JavaScript Console (Alt+J), run in PdfCraft's sandboxed
//! interpreter (`pdfcraft-js`: no file, network or timer access; loops and recursion bounded)
//! with the form object model Acrobat scripts use (`this.getField`, `app`, `util`, `console`).
//!
//! A script's field changes are applied to the form as one undoable step; what it asks the
//! viewer to do (go to a page, open a web address, print) is returned for the interface.

use pdfcraft_js::{DocInfo, Event, FieldState, FieldType, Limits, Request};

use crate::docutil::{name_tree_entries, names_tree, text_of};
use crate::forms::FillValue;
use crate::{Result, Session, invalid};

/// Longest script run.
pub const MAX_SCRIPT: usize = 256 << 10;

/// What a script did.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct ScriptOutcome {
    /// The completion value (what a console prints), unless undefined.
    pub result: Option<String>,
    pub console: Vec<String>,
    pub alerts: Vec<String>,
    pub error: Option<String>,
    /// Fields whose value the script changed (applied to the form).
    pub changed: Vec<String>,
    /// `this.pageNum = n` (0-based).
    pub go_to_page: Option<usize>,
    /// `app.launchURL(...)`: for the interface to offer.
    pub urls: Vec<String>,
    pub print: bool,
}

fn field_type(kind: &str) -> FieldType {
    match kind {
        "checkbox" => FieldType::CheckBox,
        "radio" => FieldType::RadioButton,
        "combo" => FieldType::ComboBox,
        "list" => FieldType::ListBox,
        "button" => FieldType::Button,
        "signature" => FieldType::Signature,
        _ => FieldType::Text,
    }
}

impl Session {
    /// The document-level scripts: `(name, source)` from the JavaScript name tree, then the
    /// open action's script.
    pub fn document_scripts(&self) -> Vec<(String, String)> {
        let cos = &self.file.cos;
        let script_of = |o: &markupcraft_revu::cos::Object| -> Option<String> {
            let d = cos.dict(o)?;
            if d.name(b"S") != Some(b"JavaScript".as_slice()) {
                return None;
            }
            let js = d.get(b"JS")?;
            match &*cos.resolve(js) {
                markupcraft_revu::cos::Object::Stream(s) => s
                    .decoded_within(MAX_SCRIPT)
                    .ok()
                    .map(|b| String::from_utf8_lossy(&b).into_owned()),
                _ => Some(text_of(cos, Some(js))),
            }
        };
        let mut out: Vec<(String, String)> = names_tree(cos, b"JavaScript")
            .map(|t| name_tree_entries(cos, &t))
            .unwrap_or_default()
            .iter()
            .take(1_000)
            .filter_map(|(k, v)| {
                let name = markupcraft_revu::cos::PdfString {
                    bytes: k.clone(),
                    hex: false,
                }
                .to_text();
                script_of(v).map(|s| (name, s))
            })
            .collect();
        if let Some(open) = cos
            .root()
            .and_then(|r| cos.dict(&markupcraft_revu::cos::Object::Ref(r)))
            .and_then(|d| d.get(b"OpenAction").cloned())
            && let Some(s) = script_of(&open)
        {
            out.push(("OpenAction".into(), s));
        }
        out
    }

    fn script_fields(&self) -> Vec<FieldState> {
        self.form_fields()
            .into_iter()
            .map(|f| {
                let mut st = FieldState::new(f.name.clone(), field_type(f.kind), f.value.clone());
                st.readonly = f.read_only;
                st.required = f.required;
                st.options = f.options.iter().map(|o| (o.clone(), o.clone())).collect();
                st
            })
            .collect()
    }

    fn run_script_event(
        &mut self,
        script: &str,
        event: &Event,
        page: usize,
        with_doc_scripts: bool,
    ) -> Result<ScriptOutcome> {
        if script.len() > MAX_SCRIPT {
            return Err(invalid("the script is too long"));
        }
        let fields = self.script_fields();
        let info = self
            .doc_properties()
            .standard
            .into_iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v))
            .collect();
        let doc = DocInfo {
            file_name: self
                .path()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            num_pages: self.page_count(),
            page: page.min(self.page_count().saturating_sub(1)),
            info,
        };
        let doc_scripts: Vec<String> = if with_doc_scripts {
            self.document_scripts()
                .into_iter()
                .filter(|(n, _)| n != "OpenAction")
                .map(|(_, s)| s)
                .collect()
        } else {
            Vec::new()
        };
        let o = pdfcraft_js::run(script, event, &doc, &fields, &doc_scripts, Limits::default());
        let mut out = ScriptOutcome {
            result: o.result,
            console: o.console,
            alerts: o.alerts,
            error: o.error,
            ..Default::default()
        };
        // Field values the script changed.
        let mut fills = Vec::new();
        for c in &o.changed {
            let before = fields.iter().find(|f| f.name == c.name);
            if before.is_some_and(|b| b.value == c.value) {
                continue;
            }
            let v = match c.kind {
                FieldType::CheckBox | FieldType::RadioButton => {
                    FillValue::Bool(c.value.first().is_some_and(|v| !v.is_empty() && v != "Off"))
                }
                FieldType::ComboBox | FieldType::ListBox => FillValue::Choice(c.value.clone()),
                _ => FillValue::Text(c.value.first().cloned().unwrap_or_default()),
            };
            out.changed.push(c.name.clone());
            fills.push((c.name.clone(), v));
        }
        let mut resets: Option<Vec<String>> = None;
        for r in o.requests {
            match r {
                Request::GoToPage(p) => out.go_to_page = Some(p.min(self.page_count().saturating_sub(1))),
                Request::LaunchUrl(u) => out.urls.push(u),
                Request::Print => out.print = true,
                Request::Reset(names) => resets = Some(names),
                _ => {}
            }
        }
        if !fills.is_empty() || resets.is_some() {
            self.set_merge_key(Some("javascript"));
            let r = (|| -> Result<()> {
                if let Some(names) = &resets {
                    self.form_reset(if names.is_empty() { None } else { Some(names) })?;
                }
                if !fills.is_empty() {
                    self.form_fill(&fills)?;
                }
                Ok(())
            })();
            self.set_merge_key(None);
            self.seal();
            r?;
        }
        Ok(out)
    }

    /// The JavaScript Console: run `script` with the document's scripts defined, `page`
    /// (0-based) as `this.pageNum`.
    pub fn run_javascript(&mut self, script: &str, page: usize) -> Result<ScriptOutcome> {
        self.run_script_event(script, &Event::doc("Console"), page, true)
    }

    /// Run the document's own scripts as Acrobat does on opening (the document-level scripts,
    /// then the open action's).
    pub fn run_document_scripts(&mut self) -> Result<ScriptOutcome> {
        let open = self
            .document_scripts()
            .into_iter()
            .find(|(n, _)| n == "OpenAction")
            .map(|(_, s)| s)
            .unwrap_or_default();
        self.run_script_event(&open, &Event::doc("Open"), 0, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::NewFieldKind;
    use crate::synthetic::{SyntheticPage, pdf};
    use markupcraft_model::Rect;

    #[test]
    fn the_console_reads_and_sets_fields() {
        let p = std::env::temp_dir().join(format!("markupcraft-js-{}.pdf", std::process::id()));
        let mut s = Session::from_bytes(
            pdf(&[
                SyntheticPage::new(612.0, 792.0, ""),
                SyntheticPage::new(612.0, 792.0, ""),
            ]),
            &p,
        )
        .unwrap();
        s.form_add_field(
            0,
            Rect::new(72.0, 700.0, 300.0, 720.0),
            &NewFieldKind::Text { multiline: false },
            Some("Qty"),
        )
        .unwrap();
        s.form_add_field(
            0,
            Rect::new(72.0, 660.0, 300.0, 680.0),
            &NewFieldKind::Text { multiline: false },
            Some("Total"),
        )
        .unwrap();
        s.form_fill(&[("Qty".into(), FillValue::Text("4".into()))]).unwrap();
        let o = s.run_javascript("1 + 2", 0).unwrap();
        assert_eq!(o.result.as_deref(), Some("3"));
        let o = s
            .run_javascript(
                "var q = this.getField('Qty').value; this.getField('Total').value = q * 2.5; console.println('n=' + this.numPages); this.pageNum = 1; app.launchURL('https://example.com');",
                0,
            )
            .unwrap();
        assert_eq!(o.error, None);
        assert_eq!(o.changed, ["Total"]);
        assert_eq!(s.form_values().get("Total").map(String::as_str), Some("10"));
        assert!(o.console.iter().any(|l| l.contains("n=2")), "{:?}", o.console);
        assert_eq!(o.go_to_page, Some(1));
        assert_eq!(o.urls, ["https://example.com"]);
        // One undo step.
        s.undo().unwrap();
        assert_ne!(s.form_values().get("Total").map(String::as_str), Some("10"));
        // Errors come back, not panics; loops are bounded.
        assert!(s.run_javascript("throw new Error('nope')", 0).unwrap().error.is_some());
        assert!(s.run_javascript("while (true) {}", 0).unwrap().error.is_some());
        assert!(s.document_scripts().is_empty());
    }
}
