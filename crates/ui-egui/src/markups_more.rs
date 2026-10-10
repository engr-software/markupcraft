//! More markup commands: Apply to Pages with Revu's page subsets (odd, even, portrait,
//! landscape), the Markup Layer (Layers panel > Set as Markup Layer), and Reply and Flatten
//! from a markup's right-click menu. Their command rows join the table through
//! `commands::all`; `more.rs` reaches the rest through one-line hooks.

use markupcraft_engine::align::PageFilter;

use crate::AppState;
use crate::actions;
use crate::commands::Command;

/// State of these commands (kept in `MoreState`).
#[derive(Debug, Clone, Default)]
pub struct State {
    /// The Apply to Pages window is open.
    pub apply_open: bool,
    /// Its page range ("" or "all" = every page).
    pub apply_pages: String,
    /// Its page subset.
    pub apply_filter: PageFilter,
    /// The Reply window: (markup id, text being typed).
    pub reply: Option<(String, String)>,
}

const fn c(id: &'static str, label: &'static str, menu: &'static str, group: u8, icon: &'static str) -> Command {
    Command {
        id,
        label,
        menu,
        group,
        keys: None,
        alias: None,
        icon,
        built: true,
    }
}

#[rustfmt::skip]
pub static COMMANDS: &[Command] = &[
    c("markup.apply_to_pages", "Apply to Pages...", "Markup", 32, "copy-plus"),
    c("markup.reply", "Reply...", "Markup", 32, ""),
    c("markup.flatten_selected", "Flatten Selected Markups...", "Markup", 32, ""),
    c("layers.set_markup_layer", "Set as Markup Layer", "", 0, ""),
    c("layers.clear_markup_layer", "Clear Markup Layer", "", 0, ""),
];

pub fn handles(id: &str) -> bool {
    COMMANDS.iter().any(|c| c.id == id)
}

pub fn enabled(app: &AppState, id: &str) -> bool {
    let Some(d) = app.doc() else { return false };
    match id {
        "markup.apply_to_pages" | "markup.flatten_selected" => !d.selection().is_empty(),
        "markup.reply" => d.selection().len() == 1,
        "layers.set_markup_layer" => app.features.layers.selected.is_some(),
        "layers.clear_markup_layer" => d.session.markup_layer().is_some(),
        _ => true,
    }
}

pub fn run(app: &mut AppState, id: &str) {
    match id {
        "markup.apply_to_pages" => {
            let st = &mut app.edit.more.g2;
            st.apply_open = true;
            st.apply_pages.clear();
            st.apply_filter = PageFilter::All;
        }
        "markup.reply" => {
            let one = app.doc().and_then(|d| match d.selection() {
                [id] => Some(id.clone()),
                _ => None,
            });
            match one {
                Some(id) => app.edit.more.g2.reply = Some((id, String::new())),
                None => app.status = "Reply: select one markup".into(),
            }
        }
        "markup.flatten_selected" => {
            app.features.docops.open(crate::features::docops::Tab::Flatten);
            app.features.docops.flatten_selected = true;
        }
        "layers.set_markup_layer" => {
            let Some(name) = app.features.layers.selected.clone() else {
                app.status = "Set as Markup Layer: pick a layer in the Layers panel".into();
                return;
            };
            if let Some(d) = app.doc_mut() {
                let r = d.session.set_markup_layer(Some(&name));
                app.status = actions::report(r, |_| format!("New markups go on layer {name}"));
            }
        }
        "layers.clear_markup_layer" => {
            if let Some(d) = app.doc_mut() {
                let r = d.session.set_markup_layer(None);
                app.status = actions::report(r, |_| "New markups go on no layer".into());
            }
        }
        _ => {}
    }
}

/// The windows these commands open (once a frame).
pub fn frame(app: &mut AppState, ctx: &egui::Context) {
    apply_window(app, ctx);
    reply_window(app, ctx);
}

/// Apply to Pages: copies of the selected markups at the same place on the chosen pages,
/// optionally only the odd, even, portrait or landscape ones.
fn apply_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.edit.more.g2.apply_open {
        return;
    }
    let mut st = app.edit.more.g2.clone();
    let mut open = true;
    let mut apply = false;
    egui::Window::new("Apply to Pages")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Copy the selected markups to the same place on:");
            crate::features::pages_field(ui, &mut st.apply_pages);
            ui.horizontal(|ui| {
                ui.label("Subset:");
                egui::ComboBox::from_id_salt("apply-pages-subset")
                    .selected_text(st.apply_filter.name())
                    .show_ui(ui, |ui| {
                        for f in PageFilter::ALL {
                            ui.selectable_value(&mut st.apply_filter, f, f.name());
                        }
                    });
            });
            if ui.button("Apply").clicked() {
                apply = true;
            }
        });
    st.apply_open = open && !apply;
    let (pages_text, filter) = (st.apply_pages.clone(), st.apply_filter);
    app.edit.more.g2 = st;
    if !apply {
        return;
    }
    let Some(d) = app.doc_mut() else { return };
    let count = d.session.page_count();
    let Some(pages) = crate::features::parse_pages(&pages_text, count) else {
        app.status = format!("Apply to Pages: {pages_text:?} is not a page range");
        app.edit.more.g2.apply_open = true;
        return;
    };
    let ids = d.selection().to_vec();
    let r = d.session.copy_to_pages_filtered(&ids, &pages, filter);
    app.status = actions::report(r, |v| {
        format!(
            "Copied to {} ({})",
            actions::plural(v.len(), "markup"),
            filter.name().to_lowercase()
        )
    });
}

/// Reply (right-click > Reply): a reply to the markup, listed under it in the Markups List.
fn reply_window(app: &mut AppState, ctx: &egui::Context) {
    let Some((id, mut text)) = app.edit.more.g2.reply.clone() else {
        return;
    };
    let mut open = true;
    let mut send = false;
    egui::Window::new("Reply")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .desired_rows(3)
                    .desired_width(260.0)
                    .hint_text("Your reply"),
            );
            if ui.button("Add Reply").clicked() {
                send = true;
            }
        });
    app.edit.more.g2.reply = (open && !send).then(|| (id.clone(), text.clone()));
    if !send {
        return;
    }
    let Some(d) = app.doc_mut() else { return };
    let r = d.session.add_reply(&id, &text);
    app.status = actions::report(r, |_| "Reply added".into());
}
