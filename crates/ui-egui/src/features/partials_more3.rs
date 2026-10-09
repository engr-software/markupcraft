//! Document completions of partial rows: the security icon in the status bar, the PDF/A badge
//! on document tabs, Document Properties' Standards (Verify, Unlock) and page tags, previews of
//! recent files, the Digital ID manager, clearing a certification, and Number Pages.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::RichText;
use markupcraft_engine::docs_more::SecurityStatus;
use markupcraft_engine::finish::idstore;

use crate::dialogs::Purpose;
use crate::{AppState, actions};

#[derive(Default)]
pub struct MoreState3 {
    /// Document Properties > Standards > Verify: the result.
    pub verify: Option<Vec<String>>,
    /// Recent-file previews (grey pixels), by file.
    pub previews: HashMap<PathBuf, Option<egui::TextureHandle>>,
    pub ids_open: bool,
    pub id_name: String,
    pub id_person: String,
    pub id_password: String,
    pub id_new_password: String,
    pub id_selected: Option<String>,
    pub id_message: String,
    /// IDs opened with their password this session (Log Out forgets them).
    pub unlocked: Vec<String>,
    /// Number Pages: the pages, prefix, start and style.
    pub number: Option<(Vec<usize>, String, i64, usize)>,
}

/// The Digital ID store (`<settings>/ids`).
pub fn ids_dir(app: &AppState) -> Option<PathBuf> {
    super::partials::config_dir(app).map(|d| d.join("ids"))
}

/// Run one of these commands; false when `id` is not one of them.
pub fn run(app: &mut AppState, id: &str) -> bool {
    match id {
        "tools.digital_ids" => app.features.partials.more3.ids_open = true,
        "tools.clear_certification" => clear_certification(app),
        _ => return false,
    }
    true
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    ids_window(app, ctx);
    number_window(app, ctx);
}

// ---- status bar and tabs ------------------------------------------------------------------------

/// The security icon of the status bar: open or closed lock by the document's security; hover
/// for the status, click for the Security dialog.
pub fn security_icon(app: &mut AppState, ui: &mut egui::Ui) {
    let Some(st) = app.doc().map(|d| d.session.security_status()) else {
        return;
    };
    let (icon, on) = match st {
        SecurityStatus::None => ("lock-open", false),
        _ => ("lock", true),
    };
    let tip = format!("Security: {}", st.label());
    if crate::icons::button(ui, icon, 18.0, on, &tip).clicked() {
        app.queue("document.security");
    }
}

/// A short badge before a document tab's name: PDF/A documents show "PDF/A".
pub fn tab_badge(ctx: &egui::Context, d: &crate::DocTab) -> Option<String> {
    let key = egui::Id::new(("pdfa-badge", d.uid, d.session.state_version()));
    if let Some(v) = ctx.data(|m| m.get_temp::<Option<String>>(key)) {
        return v;
    }
    let v = d.session.standards().pdfa.map(|p| format!("PDF/A-{p}"));
    ctx.data_mut(|m| m.insert_temp(key, v.clone()));
    v
}

// ---- Document Properties ---------------------------------------------------------------------

/// What the Standards rows ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdAct {
    Verify,
    Unlock,
}

/// Standards (Verify, Unlock) and page tags rows of Document Properties.
pub fn properties_rows(ui: &mut egui::Ui, st: &MoreState3, pdfa: bool, tags: &[(String, String)]) -> Option<StdAct> {
    let mut act = None;
    ui.label("");
    ui.horizontal(|ui| {
        if ui.add_enabled(pdfa, egui::Button::new("Verify")).clicked() {
            act = Some(StdAct::Verify);
        }
        if ui
            .add_enabled(pdfa, egui::Button::new("Unlock"))
            .on_hover_text("Remove the PDF/A claim so pages can be edited")
            .clicked()
        {
            act = Some(StdAct::Unlock);
        }
    });
    ui.end_row();
    if let Some(v) = &st.verify {
        ui.label("Verify");
        ui.vertical(|ui| {
            if v.is_empty() {
                ui.label("Compliant: no problems found");
            }
            for line in v.iter().take(30) {
                ui.label(RichText::new(line).small());
            }
        });
        ui.end_row();
    }
    ui.label("Page tags");
    ui.vertical(|ui| {
        if tags.is_empty() {
            ui.label(RichText::new("None").weak());
        }
        for (k, v) in tags {
            ui.label(format!("{k}: {v}"));
        }
    });
    ui.end_row();
    act
}

/// The current page's tags: the Set's custom tags for this file and page.
pub fn page_tags(app: &AppState) -> Vec<(String, String)> {
    let Some(d) = app.doc() else { return Vec::new() };
    let Some(name) = d
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
    else {
        return Vec::new();
    };
    let key = format!("{name}#{}", d.view.current + 1);
    app.features
        .sets
        .set
        .tags
        .get(&key)
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

pub fn properties_act(app: &mut AppState, act: StdAct) {
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    match act {
        StdAct::Verify => {
            let level = match d.session.pdfa_declared() {
                Some((3, _)) => markupcraft_engine::archive::PdfaLevel::A3b,
                _ => markupcraft_engine::archive::PdfaLevel::A2b,
            };
            let issues = d.session.pdfa_verify(level);
            app.features.partials.more3.verify = Some(
                issues
                    .iter()
                    .map(|i| {
                        format!(
                            "{} {}{}",
                            i.clause,
                            i.message,
                            i.page.map(|p| format!(" (page {})", p + 1)).unwrap_or_default()
                        )
                    })
                    .collect(),
            );
        }
        StdAct::Unlock => {
            let r = d.session.unlock_pdfa();
            d.rerender(threads);
            app.status = actions::report(r, |had| {
                if had {
                    "PDF/A claim removed: pages can be edited".into()
                } else {
                    "Not a PDF/A document".into()
                }
            });
        }
    }
}

// ---- recent file previews ----------------------------------------------------------------------

/// The preview of `path` (made once, at most 160 pixels).
pub fn preview(app: &mut AppState, ctx: &egui::Context, path: &std::path::Path) -> Option<egui::TextureHandle> {
    let cache = &mut app.features.partials.more3.previews;
    if let Some(t) = cache.get(path) {
        return t.clone();
    }
    if cache.len() > 200 {
        cache.clear();
    }
    let tex = markupcraft_engine::finish::preview::first_page_preview(path, 160.0)
        .ok()
        .map(|(w, h, px)| {
            let img = egui::ColorImage::from_gray([w, h], &px);
            ctx.load_texture(
                format!("recent-preview-{}", path.display()),
                img,
                egui::TextureOptions::LINEAR,
            )
        });
    cache.insert(path.to_path_buf(), tex.clone());
    tex
}

// ---- Digital IDs -------------------------------------------------------------------------------

fn ids_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.partials.more3.ids_open {
        return;
    }
    let Some(dir) = ids_dir(app) else {
        app.features.partials.more3.ids_open = false;
        app.status = "Digital IDs are kept in the settings folder, which is not available".into();
        return;
    };
    let list = idstore::list_ids(&dir);
    #[derive(PartialEq)]
    enum A {
        Create,
        Import,
        Export,
        Password,
        Delete,
        LogIn,
        LogOut,
    }
    let mut act = None;
    let mut open = true;
    super::window("Digital IDs").open(&mut open).show(ctx, |ui| {
        let st = &mut app.features.partials.more3;
        if list.is_empty() {
            ui.label(RichText::new("No digital IDs yet.").weak());
        }
        for i in &list {
            let unlocked = st.unlocked.contains(&i.name);
            let label = format!(
                "{}  ({}){}",
                i.name,
                i.subject,
                if unlocked { "  logged in" } else { "" }
            );
            if ui
                .selectable_label(st.id_selected.as_deref() == Some(i.name.as_str()), label)
                .on_hover_text(format!("SHA-256 {}", i.fingerprint))
                .clicked()
            {
                st.id_selected = Some(i.name.clone());
            }
        }
        ui.separator();
        egui::Grid::new("ids-fields").num_columns(2).show(ui, |ui| {
            ui.label("ID name");
            ui.text_edit_singleline(&mut st.id_name);
            ui.end_row();
            ui.label("Person");
            ui.text_edit_singleline(&mut st.id_person);
            ui.end_row();
            ui.label("Password");
            ui.add(egui::TextEdit::singleline(&mut st.id_password).password(true));
            ui.end_row();
            ui.label("New password");
            ui.add(egui::TextEdit::singleline(&mut st.id_new_password).password(true));
            ui.end_row();
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Create ID").clicked() {
                act = Some(A::Create);
            }
            if ui.button("Import ID...").clicked() {
                act = Some(A::Import);
            }
            let sel = st.id_selected.is_some();
            for (a, label) in [
                (A::Export, "Export Certificate..."),
                (A::Password, "Change Password"),
                (A::LogIn, "Log In"),
                (A::LogOut, "Log Out"),
                (A::Delete, "Delete ID"),
            ] {
                if ui.add_enabled(sel, egui::Button::new(label)).clicked() {
                    act = Some(a);
                }
            }
        });
        if !st.id_message.is_empty() {
            ui.label(RichText::new(&st.id_message).small());
        }
    });
    app.features.partials.more3.ids_open = open;
    let st = &mut app.features.partials.more3;
    let sel = st.id_selected.clone().unwrap_or_default();
    let msg = match act {
        Some(A::Create) => {
            let who = markupcraft_engine::signatures::IdentityInfo {
                name: st.id_person.clone(),
                ..Default::default()
            };
            actions::report(idstore::create_id(&dir, &st.id_name, &who, 5, &st.id_password), |i| {
                format!("Created {} for {}", i.name, i.subject)
            })
        }
        Some(A::Password) => actions::report(
            idstore::change_password(&dir, &sel, &st.id_password, &st.id_new_password),
            |_| "Password changed".into(),
        ),
        Some(A::Delete) => {
            st.unlocked.retain(|n| *n != sel);
            actions::report(idstore::delete_id(&dir, &sel), |_| format!("Deleted {sel}"))
        }
        Some(A::LogIn) => match idstore::open_id(&dir, &sel, &st.id_password) {
            Ok(_) => {
                if !st.unlocked.contains(&sel) {
                    st.unlocked.push(sel.clone());
                }
                format!("Logged in to {sel}")
            }
            Err(e) => e.to_string(),
        },
        Some(A::LogOut) => {
            st.unlocked.retain(|n| *n != sel);
            format!("Logged out of {sel}")
        }
        Some(A::Import) => {
            app.dialogs
                .open(Purpose::Feature(super::Ask::IdImport), super::P12, false);
            return;
        }
        Some(A::Export) => {
            app.dialogs.save(
                Purpose::Feature(super::Ask::IdExportCert),
                super::CERTS,
                &format!("{sel}.pem"),
            );
            return;
        }
        None => return,
    };
    app.features.partials.more3.id_message = msg;
}

/// An Import or Export Certificate file was chosen.
pub fn ids_file(app: &mut AppState, import: bool, path: &std::path::Path) {
    let Some(dir) = ids_dir(app) else { return };
    let st = &mut app.features.partials.more3;
    st.id_message = if import {
        let name = if st.id_name.trim().is_empty() {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            st.id_name.clone()
        };
        actions::report(idstore::import_id(&dir, path, &name, &st.id_password), |i| {
            format!("Imported {} ({})", i.name, i.subject)
        })
    } else {
        let sel = st.id_selected.clone().unwrap_or_default();
        actions::report(idstore::export_certificate(&dir, &sel, path), |_| {
            format!("Certificate written to {}", path.display())
        })
    };
}

/// Tools > Clear Certification with the ID selected in Digital IDs and its password.
fn clear_certification(app: &mut AppState) {
    let Some(dir) = ids_dir(app) else { return };
    let st = &app.features.partials.more3;
    let sel = st.id_selected.clone().unwrap_or_default();
    let id = match idstore::open_id(&dir, &sel, &st.id_password) {
        Ok(id) => id,
        Err(e) => {
            app.status = format!("Choose the certifier's ID in Tools > Digital IDs and type its password: {e}");
            return;
        }
    };
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let r = d.session.clear_certification(&id);
    d.rerender(threads);
    app.status = actions::report(r, |f| format!("Certification cleared ({f})"));
}

// ---- Number Pages ------------------------------------------------------------------------------

pub fn open_number(app: &mut AppState, pages: Vec<usize>) {
    app.features.partials.more3.number = Some((pages, String::new(), 1, 0));
}

fn number_window(app: &mut AppState, ctx: &egui::Context) {
    let Some((pages, mut prefix, mut start, mut style)) = app.features.partials.more3.number.clone() else {
        return;
    };
    const STYLES: [&str; 6] = [
        "1, 2, 3",
        "I, II, III",
        "i, ii, iii",
        "A, B, C",
        "a, b, c",
        "None (prefix only)",
    ];
    let (mut open, mut ok) = (true, false);
    super::window("Number Pages").open(&mut open).show(ctx, |ui| {
        ui.label(actions::plural(pages.len(), "page"));
        ui.horizontal(|ui| {
            ui.label("Prefix");
            ui.text_edit_singleline(&mut prefix);
        });
        ui.horizontal(|ui| {
            ui.label("Start at");
            ui.add(egui::DragValue::new(&mut start).range(1..=100_000));
            egui::ComboBox::from_id_salt("number-style")
                .selected_text(STYLES.get(style).copied().unwrap_or_default())
                .show_ui(ui, |ui| {
                    for (i, s) in STYLES.iter().enumerate() {
                        ui.selectable_value(&mut style, i, *s);
                    }
                });
        });
        if ui.button("Number").clicked() {
            ok = true;
        }
    });
    app.features.partials.more3.number = (open && !ok).then(|| (pages.clone(), prefix.clone(), start, style));
    if ok {
        use markupcraft_engine::labels::LabelStyle;
        let st = [
            Some(LabelStyle::Decimal),
            Some(LabelStyle::UpperRoman),
            Some(LabelStyle::LowerRoman),
            Some(LabelStyle::UpperAlpha),
            Some(LabelStyle::LowerAlpha),
            None,
        ]
        .get(style)
        .copied()
        .flatten();
        let Some(d) = app.doc_mut() else { return };
        let r = d.session.number_pages(&pages, st, &prefix, start);
        app.status = actions::report(r, |_| format!("Numbered {}", actions::plural(pages.len(), "page")));
    }
}
