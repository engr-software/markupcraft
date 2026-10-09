//! Signatures (Alt+4): every signature field with its status (valid, invalid, unknown, not
//! signed), signer, date and reason; validate against trusted certificates; sign.

use egui::RichText;

use super::{PanelDef, Slot};
use crate::AppState;
use crate::commands::alt;
use crate::dialogs::Purpose;
use crate::features::{self, Ask, signatures};

pub static PANEL: PanelDef = PanelDef {
    id: "signatures",
    title: "Signatures",
    icon: "pen-line",
    slot: Slot::Left,
    keys: alt(egui::Key::Num4),
    ui,
};

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let Some(uid) = app.doc().map(|d| d.uid) else {
        super::empty(ui, "No document open.");
        return;
    };
    if app.features.signatures.list_doc != Some(uid) {
        signatures::refresh(app);
    }
    let (mut validate, mut trust, mut sign, mut select) = (false, false, false, None);
    {
        let s = &app.features.signatures;
        ui.horizontal(|ui| {
            if ui.button("Sign...").clicked() {
                sign = true;
            }
            if ui.button("Validate").clicked() {
                validate = true;
            }
            if ui.button("Trust Certificate...").clicked() {
                trust = true;
            }
        });
        ui.separator();
        if !s.list_error.is_empty() {
            ui.label(RichText::new(&s.list_error).small());
        }
        if s.list.is_empty() {
            super::empty(ui, "This document has no signature fields.");
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, sig) in s.list.iter().enumerate() {
                let (icon, text) = signatures::status_text(sig);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("[{icon}]"))
                            .monospace()
                            .strong()
                            .color(signatures::status_color(sig)),
                    );
                    let who = sig.signer.clone().unwrap_or_else(|| sig.field.clone());
                    if ui.selectable_label(s.selected == Some(i), who).clicked() {
                        select = Some(i);
                    }
                });
                if s.selected == Some(i) {
                    ui.indent(("sig", i), |ui| {
                        ui.label(RichText::new(format!("{text}: {}", sig.summary)).small());
                        for (k, v) in [
                            ("Field", Some(sig.field.clone())),
                            ("Date", sig.date.clone()),
                            ("Reason", sig.reason.clone()),
                            ("Location", sig.location.clone()),
                            ("Page", sig.page.map(|p| format!("{}", p + 1))),
                        ] {
                            if let Some(v) = v.filter(|v| !v.is_empty()) {
                                ui.label(RichText::new(format!("{k}: {v}")).small());
                            }
                        }
                        if sig.certify.is_some() {
                            ui.label(RichText::new("Certifying signature").small());
                        }
                        for m in &sig.modifications {
                            ui.label(RichText::new(format!("Changed after signing: {m}")).small());
                        }
                        for d in &sig.details {
                            ui.label(RichText::new(d).small().weak());
                        }
                    });
                }
            }
        });
    }
    if validate {
        signatures::refresh(app);
    }
    if trust {
        app.dialogs
            .open(Purpose::Feature(Ask::TrustCerts), features::CERTS, false);
    }
    if sign {
        app.features.signatures.sign_open = true;
    }
    if let Some(i) = select {
        app.features.signatures.selected = Some(i);
        let page = app.features.signatures.list.get(i).and_then(|s| s.page);
        if let (Some(p), Some(d)) = (page, app.doc_mut()) {
            let n = d.session.page_count();
            d.view.go_to_page(p, n);
        }
    }
}
