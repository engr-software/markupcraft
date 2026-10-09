//! The JavaScript Console (Window > JavaScript Console, Alt+J) and Tools > Run Document
//! JavaScript: scripts run in PdfCraft's sandboxed interpreter against the active document's
//! form (`markupcraft_engine::scripting`). Output, alerts and errors go to the console; field
//! changes are one undo step; a page change is followed and web addresses are offered.

use crate::AppState;

#[derive(Default)]
pub struct ScriptState {
    pub open: bool,
    pub code: String,
    /// The console's output, oldest first.
    pub log: Vec<String>,
    /// Web addresses scripts asked to open (the user clicks to open them).
    pub urls: Vec<String>,
}

const MAX_LOG: usize = 2_000;

fn push(st: &mut ScriptState, line: String) {
    st.log.push(line);
    if st.log.len() > MAX_LOG {
        let extra = st.log.len() - MAX_LOG;
        st.log.drain(..extra);
    }
}

/// Apply what a script asked for and log what it said.
fn outcome(app: &mut AppState, o: markupcraft_engine::scripting::ScriptOutcome) {
    let threads = app.threads;
    if let Some(d) = app.doc_mut() {
        if !o.changed.is_empty() {
            d.rerender(threads);
        }
        if let Some(p) = o.go_to_page {
            let n = d.session.page_count();
            d.view.go_to_page(p, n);
        }
    }
    let st = &mut app.features.more6.script;
    for l in o.console {
        push(st, l);
    }
    for a in o.alerts {
        push(st, format!("alert: {a}"));
    }
    if !o.changed.is_empty() {
        push(st, format!("fields changed: {}", o.changed.join(", ")));
    }
    if o.print {
        push(st, "the script asked to print: use File > Print".into());
    }
    for u in o.urls {
        if !st.urls.contains(&u) {
            st.urls.push(u);
        }
    }
    match (o.error, o.result) {
        (Some(e), _) => push(st, format!("error: {e}")),
        (None, Some(r)) => push(st, r),
        (None, None) => {}
    }
}

/// Run the console's code.
pub fn run_console(app: &mut AppState) {
    let code = app.features.more6.script.code.clone();
    if code.trim().is_empty() {
        return;
    }
    push(&mut app.features.more6.script, format!("> {}", code.trim()));
    let Some(d) = app.doc_mut() else {
        push(&mut app.features.more6.script, "error: open a document first".into());
        return;
    };
    let page = d.view.current;
    match d.session.run_javascript(&code, page) {
        Ok(o) => outcome(app, o),
        Err(e) => push(&mut app.features.more6.script, format!("error: {e}")),
    }
}

/// Tools > Run Document JavaScript: the document's own scripts, as on opening.
pub fn run_document(app: &mut AppState) {
    let Some(d) = app.doc_mut() else { return };
    let n = d.session.document_scripts().len();
    match d.session.run_document_scripts() {
        Ok(o) => {
            outcome(app, o);
            app.status = format!("Ran {}", crate::actions::plural(n, "document script"));
        }
        Err(e) => app.status = e.to_string(),
    }
    app.features.more6.script.open = true;
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.more6.script.open {
        return;
    }
    let mut open = true;
    let (mut run, mut clear, mut doc_scripts) = (false, false, false);
    let mut go: Option<String> = None;
    let st = &mut app.features.more6.script;
    egui::Window::new("JavaScript Console")
        .id(egui::Id::new("js-console"))
        .open(&mut open)
        .default_size([520.0, 360.0])
        .default_pos(egui::pos2(560.0, 300.0))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for l in &st.log {
                        ui.monospace(l);
                    }
                });
            ui.separator();
            let r = ui.add(
                egui::TextEdit::multiline(&mut st.code)
                    .id(egui::Id::new("js-code"))
                    .code_editor()
                    .desired_rows(4)
                    .desired_width(f32::INFINITY)
                    .hint_text("this.getField(\"Total\").value = 12;  (Ctrl+Enter runs)"),
            );
            if r.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) {
                run = true;
            }
            ui.horizontal(|ui| {
                run |= ui.button("Run").clicked();
                clear = ui.button("Clear").clicked();
                doc_scripts = ui.button("Run Document Scripts").clicked();
            });
            for u in &st.urls {
                if ui.link(format!("Open {u}")).clicked() {
                    go = Some(u.clone());
                }
            }
            ui.weak("Scripts run sandboxed: no files, network or timers; endless loops are stopped.");
        });
    app.features.more6.script.open = open;
    if clear {
        app.features.more6.script.log.clear();
        app.features.more6.script.urls.clear();
    }
    if run {
        run_console(app);
    }
    if doc_scripts {
        run_document(app);
    }
    if let Some(u) = go {
        super::web::open_in_browser(app, ctx, &u);
    }
}
