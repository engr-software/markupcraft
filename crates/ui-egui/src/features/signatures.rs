//! Digital signatures: the Signatures panel's list (validated against trusted certificates),
//! signing with a digital ID (a .p12 file and its password, an optional visible box), and
//! creating a self-signed digital ID.

use std::path::{Path, PathBuf};

use egui::{Color32, RichText};
use markupcraft_engine::signatures::{
    IdentityInfo, SignRequest, SignatureSummary, create_digital_id, load_certificates, open_digital_id,
};
use markupcraft_geom::{Point, Rect};

use super::{Ask, Mark, Pick};
use crate::dialogs::{PDF, Purpose};
use crate::{AppState, DocTab};

pub struct SignaturesState {
    /// The list and the document it is of (refreshed on demand: validation is not free).
    pub list: Vec<SignatureSummary>,
    pub list_doc: Option<u64>,
    pub list_error: String,
    pub selected: Option<usize>,
    /// Certificates trusted when validating (file bytes, PEM or DER).
    pub trust: Vec<Vec<u8>>,
    pub sign_open: bool,
    pub id_path: Option<PathBuf>,
    pub password: String,
    pub reason: String,
    pub location: String,
    pub certify: bool,
    pub rect: Option<(usize, Rect)>,
    pub new_id_open: bool,
    pub who: IdentityInfo,
    pub years: u32,
    pub new_password: String,
    pub message: String,
}

impl Default for SignaturesState {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            list_doc: None,
            list_error: String::new(),
            selected: None,
            trust: Vec::new(),
            sign_open: false,
            id_path: None,
            password: String::new(),
            reason: String::new(),
            location: String::new(),
            certify: false,
            rect: None,
            new_id_open: false,
            who: IdentityInfo::default(),
            years: 5,
            new_password: String::new(),
            message: String::new(),
        }
    }
}

/// Colour of a signature status.
pub fn status_color(s: &SignatureSummary) -> Color32 {
    match (s.signed, s.status) {
        (false, _) => Color32::from_gray(140),
        (true, "valid") => Color32::from_rgb(30, 150, 60),
        (true, "invalid") => Color32::from_rgb(200, 40, 40),
        _ => Color32::from_rgb(220, 150, 0),
    }
}

/// A short status text (and the icon character the panel shows).
pub fn status_text(s: &SignatureSummary) -> (&'static str, &'static str) {
    match (s.signed, s.status) {
        (false, _) => ("o", "Not signed"),
        (true, "valid") => ("+", "Valid"),
        (true, "invalid") => ("x", "Invalid"),
        _ => ("?", "Validity unknown"),
    }
}

impl SignaturesState {
    pub fn marks(&self, d: &DocTab, out: &mut Vec<Mark>) {
        if self.list_doc == Some(d.uid)
            && let Some(s) = self.selected.and_then(|i| self.list.get(i))
            && let (Some(p), Some(r)) = (s.page, s.rect)
        {
            out.push(Mark::rect(p, r, super::canvas::CURRENT_FILL, status_color(s)));
        }
        if let Some((p, r)) = self.rect
            && self.sign_open
        {
            out.push(Mark::rect(
                p,
                r,
                super::canvas::CURRENT_FILL,
                super::canvas::CURRENT_STROKE,
            ));
        }
    }
}

/// Validate the active document's signatures again.
pub fn refresh(app: &mut AppState) {
    let Some(d) = app.docs.get(app.active) else { return };
    let s = &mut app.features.signatures;
    let mut certs = Vec::new();
    for b in &s.trust {
        if let Ok(c) = load_certificates(b) {
            certs.extend(c);
        }
    }
    s.list_doc = Some(d.uid);
    match d.session.signatures(&certs) {
        Ok(l) => {
            s.list = l;
            s.list_error.clear();
        }
        Err(e) => {
            s.list.clear();
            s.list_error = e.to_string();
        }
    }
}

pub fn rect_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    app.features.signatures.rect = super::rect_of(pts).map(|r| (page, r));
    app.features.signatures.sign_open = true;
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    sign_window(app, ctx);
    new_id_window(app, ctx);
}

fn sign_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.signatures.sign_open || !app.has_doc() {
        return;
    }
    let name = app
        .doc()
        .map(|d| d.name.trim_end_matches(".pdf").to_string())
        .unwrap_or_default();
    let (mut open, mut browse, mut pick, mut sign) = (true, false, false, false);
    super::window("Sign Document").open(&mut open).show(ctx, |ui| {
        let s = &mut app.features.signatures;
        egui::Grid::new("sign").num_columns(2).show(ui, |ui| {
            ui.label("Digital ID");
            ui.horizontal(|ui| {
                ui.label(
                    s.id_path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map_or_else(|| "(none)".to_string(), |n| n.to_string_lossy().into_owned()),
                );
                if ui.button("Browse...").clicked() {
                    browse = true;
                }
            });
            ui.end_row();
            ui.label("Password");
            ui.add(egui::TextEdit::singleline(&mut s.password).password(true));
            ui.end_row();
            ui.label("Reason");
            ui.text_edit_singleline(&mut s.reason);
            ui.end_row();
            ui.label("Location");
            ui.text_edit_singleline(&mut s.location);
            ui.end_row();
            ui.label("Appearance");
            ui.horizontal(|ui| {
                ui.label(match s.rect {
                    Some((p, _)) => format!("box on page {}", p + 1),
                    None => "invisible".into(),
                });
                if ui.button("Draw Box...").clicked() {
                    pick = true;
                }
                if s.rect.is_some() && ui.small_button("Clear").clicked() {
                    s.rect = None;
                }
            });
            ui.end_row();
            ui.label("Certify");
            ui.checkbox(&mut s.certify, "Certify (no changes allowed after)");
            ui.end_row();
        });
        if !s.message.is_empty() {
            ui.label(RichText::new(&s.message).small());
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(s.id_path.is_some() && !s.password.is_empty(), |ui| {
                if ui.button("Sign and Save As...").clicked() {
                    sign = true;
                }
            });
            if ui.button("Cancel").clicked() {
                s.sign_open = false;
            }
        });
    });
    if !open {
        app.features.signatures.sign_open = false;
    }
    if browse {
        app.dialogs.open(Purpose::Feature(Ask::SignId), super::P12, false);
    }
    if pick {
        super::start_pick(app, Pick::Signature, "Drag the signature's box");
    }
    if sign {
        app.dialogs
            .save(Purpose::Feature(Ask::SignOut), PDF, &format!("{name} signed.pdf"));
    }
}

fn new_id_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.signatures.new_id_open {
        return;
    }
    let (mut open, mut save) = (true, false);
    super::window("Create Digital ID").open(&mut open).show(ctx, |ui| {
        let s = &mut app.features.signatures;
        egui::Grid::new("new-id").num_columns(2).show(ui, |ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut s.who.name);
            ui.end_row();
            ui.label("Organization");
            ui.text_edit_singleline(&mut s.who.organization);
            ui.end_row();
            ui.label("Unit");
            ui.text_edit_singleline(&mut s.who.unit);
            ui.end_row();
            ui.label("Email");
            ui.text_edit_singleline(&mut s.who.email);
            ui.end_row();
            ui.label("Country");
            ui.add(
                egui::TextEdit::singleline(&mut s.who.country)
                    .char_limit(2)
                    .desired_width(30.0),
            );
            ui.end_row();
            ui.label("Valid for");
            ui.add(egui::DragValue::new(&mut s.years).range(1..=50).suffix(" years"));
            ui.end_row();
            ui.label("Password");
            ui.add(egui::TextEdit::singleline(&mut s.new_password).password(true));
            ui.end_row();
        });
        if !s.message.is_empty() {
            ui.label(RichText::new(&s.message).small());
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!s.who.name.trim().is_empty() && !s.new_password.is_empty(), |ui| {
                if ui.button("Save ID As...").clicked() {
                    save = true;
                }
            });
            if ui.button("Cancel").clicked() {
                s.new_id_open = false;
            }
        });
    });
    if !open {
        app.features.signatures.new_id_open = false;
    }
    if save {
        let name = format!("{}.p12", app.features.signatures.who.name.trim());
        app.dialogs.save(Purpose::Feature(Ask::NewIdOut), super::P12, &name);
    }
}

/// Sign the active document with the dialog's ID into `out`, and continue on the signed file.
pub fn sign_to(app: &mut AppState, out: &Path) -> Result<(), String> {
    let s = &app.features.signatures;
    let path = s.id_path.clone().ok_or("Choose a digital ID")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let id = open_digital_id(&bytes, &s.password).map_err(|e| e.to_string())?;
    let req = SignRequest {
        page: s.rect.map_or(0, |r| r.0),
        rect: s.rect.map(|r| r.1),
        reason: (!s.reason.trim().is_empty()).then(|| s.reason.trim().to_string()),
        location: (!s.location.trim().is_empty()).then(|| s.location.trim().to_string()),
        certify: s.certify.then_some(2),
        ..Default::default()
    };
    let threads = app.threads;
    let d = app.doc_mut().ok_or("No document open")?;
    d.session.sign(&id, &req, out).map_err(|e| e.to_string())?;
    d.path = Some(out.to_path_buf());
    d.name = out
        .file_name()
        .map_or_else(|| d.name.clone(), |n| n.to_string_lossy().into_owned());
    d.rerender(threads);
    Ok(())
}

pub fn file(app: &mut AppState, ask: &Ask, path: &Path) {
    match ask {
        Ask::SignId => {
            app.features.signatures.id_path = Some(path.to_path_buf());
            app.features.signatures.sign_open = true;
        }
        Ask::SignOut => match sign_to(app, path) {
            Ok(()) => {
                let s = &mut app.features.signatures;
                s.sign_open = false;
                s.password.clear();
                s.rect = None;
                app.status = format!("Signed and saved as {}", path.display());
                refresh(app);
                app.show_panel("signatures");
            }
            Err(e) => app.features.signatures.message = e,
        },
        Ask::NewIdOut => {
            let s = &mut app.features.signatures;
            let r = create_digital_id(&s.who, s.years, &s.new_password)
                .map_err(|e| e.to_string())
                .and_then(|b| crate::chest::write_atomic(path, &b).map_err(|e| e.to_string()));
            match r {
                Ok(()) => {
                    s.new_id_open = false;
                    s.new_password.clear();
                    s.id_path = Some(path.to_path_buf());
                    s.message = format!("Digital ID saved to {}", path.display());
                    app.status = s.message.clone();
                }
                Err(e) => s.message = e,
            }
        }
        Ask::TrustCerts => match std::fs::read(path) {
            Ok(b) => {
                if load_certificates(&b).is_ok() {
                    app.features.signatures.trust.push(b);
                    refresh(app);
                } else {
                    app.features.signatures.list_error = "That file has no certificate".into();
                }
            }
            Err(e) => app.features.signatures.list_error = e.to_string(),
        },
        _ => {}
    }
}
