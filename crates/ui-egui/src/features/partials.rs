//! Features finished from partial rows: File > New PDF from Template (page templates kept as
//! PDFs in the settings folder), Email Templates (File > Email with saved recipients, subject
//! and body), and the windows of the other completions (`features::partials_more`).

use std::path::PathBuf;

use egui::RichText;
use markupcraft_engine::finish::create::{
    EmailTemplate, email_from_template, list_templates, load_email_templates, remove_template, save_email_templates,
    save_template,
};

use crate::{AppState, actions};

/// State of these windows.
#[derive(Default)]
pub struct PartialsState {
    /// The settings folder (tests set it; else the app's settings folder).
    pub config: Option<PathBuf>,
    pub templates_open: bool,
    pub template_name: String,
    pub template_pick: Option<String>,
    pub email_open: bool,
    pub email: EmailTemplate,
    pub message: String,
    pub more: super::partials_more::MoreState,
    pub more2: super::partials_more2::MoreState2,
    pub more3: super::partials_more3::MoreState3,
    /// Extract Pages, one file per page: the dialog waiting for its folder.
    pub extract_each: Option<crate::shell::pages::PageDialog>,
}

/// The settings folder, or why there is none.
pub fn config_dir(app: &AppState) -> Option<PathBuf> {
    app.features
        .partials
        .config
        .clone()
        .or_else(|| app.shell.store.as_ref().map(|s| s.dir.clone()))
}

fn templates_dir(app: &AppState) -> Option<PathBuf> {
    config_dir(app).map(|d| d.join("templates"))
}

fn email_file(app: &AppState) -> Option<PathBuf> {
    // Preferences > Admin: a shared email template folder.
    crate::shell::admin_prefs::email_templates_file(app)
        .or_else(|| config_dir(app).map(|d| d.join("email_templates.json")))
}

/// Run one of these commands; false when `id` is not one of them.
pub fn run(app: &mut AppState, id: &str) -> bool {
    match id {
        "file.new_from_template" => {
            app.features.partials.templates_open = true;
            app.features.partials.message.clear();
        }
        "file.email_templates" => {
            app.features.partials.email_open = true;
            app.features.partials.message.clear();
        }
        "tools.add_shared_toolset" => app.dialogs.open(
            crate::dialogs::Purpose::Feature(super::Ask::SharedToolSet),
            crate::dialogs::TOOLSET,
            false,
        ),
        _ => return super::partials_more::run(app, id),
    }
    true
}

/// A new untitled document from the template called `name`.
pub fn new_from_template(app: &mut AppState, name: &str) {
    let Some(dir) = templates_dir(app) else {
        app.status = "No settings folder for templates".into();
        return;
    };
    let Some((_, path)) = list_templates(&dir).into_iter().find(|(n, _)| n == name) else {
        app.status = format!("No template {name}");
        return;
    };
    match std::fs::read(&path) {
        Ok(bytes) => {
            app.shell.untitled += 1;
            let title = format!("Untitled {}.pdf", app.shell.untitled);
            match app.open_bytes(&title, None, bytes) {
                Ok(()) => app.status = format!("New PDF from {name}"),
                Err(e) => app.status = format!("New PDF from Template: {e}"),
            }
        }
        Err(e) => app.status = format!("{}: {e}", path.display()),
    }
}

/// Email the active document with template `t` (an unsent message in the mail program).
pub fn email_with(app: &mut AppState, t: &EmailTemplate) -> Result<PathBuf, String> {
    let d = app.doc().ok_or("No document open")?;
    let bytes = d.session.current_bytes().map_err(|e| e.to_string())?;
    let name = if d.name.to_lowercase().ends_with(".pdf") {
        d.name.clone()
    } else {
        format!("{}.pdf", d.name)
    };
    let eml = email_from_template(t, &name, &bytes);
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "markupcraft-email-{}-{}.eml",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    crate::chest::write_atomic(&path, &eml).map_err(|e| e.to_string())?;
    app.shell.extra.files.last_email = Some(path.clone());
    if app.shell.extra.launch {
        crate::shell::files::launch(&path);
    }
    Ok(path)
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    templates_window(app, ctx);
    email_window(app, ctx);
    super::partials_more::window(app, ctx);
}

fn templates_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.partials.templates_open {
        return;
    }
    let dir = templates_dir(app);
    let list = dir.as_deref().map(list_templates).unwrap_or_default();
    let has_doc = app.has_doc();
    let (mut open, mut create, mut save, mut remove) = (true, None, false, None);
    super::window("New PDF from Template").open(&mut open).show(ctx, |ui| {
        let st = &mut app.features.partials;
        if dir.is_none() {
            ui.label("Templates are kept in the settings folder, which is not available.");
        }
        if list.is_empty() {
            ui.label(RichText::new("No templates yet: save a document as one below.").weak());
        }
        for (name, _) in &list {
            ui.horizontal(|ui| {
                let r = ui.selectable_label(st.template_pick.as_deref() == Some(name.as_str()), name);
                if r.clicked() {
                    st.template_pick = Some(name.clone());
                }
                if r.double_clicked() {
                    create = Some(name.clone());
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(name.clone());
                }
            });
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("template-pick")
                .selected_text(st.template_pick.clone().unwrap_or_else(|| "Choose a template".into()))
                .show_ui(ui, |ui| {
                    for (name, _) in &list {
                        ui.selectable_value(&mut st.template_pick, Some(name.clone()), name);
                    }
                });
            if ui
                .add_enabled(st.template_pick.is_some(), egui::Button::new("New PDF"))
                .clicked()
            {
                create = st.template_pick.clone();
            }
        });
        ui.separator();
        ui.add_enabled_ui(has_doc && dir.is_some(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Template name");
                ui.text_edit_singleline(&mut st.template_name);
                if ui.button("Save Current Document as Template").clicked() {
                    save = true;
                }
            });
        });
        if !st.message.is_empty() {
            ui.label(RichText::new(&st.message).small());
        }
    });
    app.features.partials.templates_open = open;
    let Some(dir) = dir else { return };
    if let Some(name) = remove {
        app.features.partials.message = actions::report(remove_template(&dir, &name), |_| format!("Removed {name}"));
    }
    if save {
        let name = app.features.partials.template_name.trim().to_string();
        let r = app
            .doc()
            .ok_or_else(|| markupcraft_engine::EngineError::Invalid("No document open".into()))
            .and_then(|d| d.session.current_bytes())
            .and_then(|b| save_template(&dir, &name, &b));
        app.features.partials.message = actions::report(r, |_| format!("Saved template {name}"));
    }
    if let Some(name) = create {
        new_from_template(app, &name);
        app.features.partials.templates_open = false;
    }
}

fn email_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.partials.email_open {
        return;
    }
    let Some(file) = email_file(app) else {
        app.features.partials.email_open = false;
        app.status = "Email templates are kept in the settings folder, which is not available".into();
        return;
    };
    let list = load_email_templates(&file).unwrap_or_default();
    let has_doc = app.has_doc();
    let (mut open, mut save, mut delete, mut send) = (true, false, false, false);
    super::window("Email Templates").open(&mut open).show(ctx, |ui| {
        let st = &mut app.features.partials;
        ui.horizontal_wrapped(|ui| {
            for t in &list {
                if ui.selectable_label(st.email.name == t.name, &t.name).clicked() {
                    st.email = t.clone();
                }
            }
        });
        egui::Grid::new("email-template").num_columns(2).show(ui, |ui| {
            let e = &mut st.email;
            for (label, v) in [
                ("Name", &mut e.name),
                ("To", &mut e.to),
                ("Cc", &mut e.cc),
                ("Subject", &mut e.subject),
            ] {
                ui.label(label);
                ui.text_edit_singleline(v);
                ui.end_row();
            }
            ui.label("Message");
            ui.text_edit_multiline(&mut e.body);
            ui.end_row();
        });
        ui.label(
            RichText::new("{file} is replaced by the document's name.")
                .weak()
                .small(),
        );
        ui.horizontal(|ui| {
            if ui.button("Save Template").clicked() {
                save = true;
            }
            if ui.button("Delete Template").clicked() {
                delete = true;
            }
            if ui.add_enabled(has_doc, egui::Button::new("Email Document")).clicked() {
                send = true;
            }
        });
        if !st.message.is_empty() {
            ui.label(RichText::new(&st.message).small());
        }
    });
    app.features.partials.email_open = open;
    let cur = app.features.partials.email.clone();
    if save || delete {
        let mut list = list;
        list.retain(|t| !t.name.eq_ignore_ascii_case(cur.name.trim()));
        if save {
            let mut t = cur.clone();
            t.name = t.name.trim().to_string();
            list.push(t);
        }
        app.features.partials.message = actions::report(save_email_templates(&file, &list), |_| {
            if save {
                format!("Saved {}", cur.name.trim())
            } else {
                format!("Deleted {}", cur.name.trim())
            }
        });
    }
    if send {
        let r = email_with(app, &cur);
        app.features.partials.message = match r {
            Ok(_) => format!("Email draft from {} with the document attached", cur.name),
            Err(e) => e,
        };
        app.status = app.features.partials.message.clone();
    }
}
