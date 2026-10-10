//! The Preferences sections of `extra`'s settings, drawn under each page's own: startup
//! (General), the locked-file prompt and Rotate Pages default (Document), scrollbars, Alt menu
//! accelerators and split-view synchronization (Navigation), snap targets and colour (Grid &
//! Snap), Sketch to Scale angles (Tools), line weights and PDF/A (Advanced), and connecting an
//! AI assistant over MCP (Admin).

use egui::RichText;

use super::UiPrefs;

/// The MCP configuration an AI assistant reads to start MarkupCraft's tool server (opt-in,
/// local, over stdio).
pub const MCP_CONFIG: &str = r#"{
  "mcpServers": {
    "markupcraft": {
      "command": "markupcraft-cli",
      "args": ["mcp", "--root", "<folder with your PDFs>"]
    }
  }
}"#;

/// Draw `page`'s extra section. Returns an action for the window ("startup-file", "copy-mcp").
pub fn section(ui: &mut egui::Ui, page: &str, u: &mut UiPrefs) -> Option<&'static str> {
    let x = &mut u.extra;
    let mut out = None;
    match page {
        "General" => {
            ui.add_space(6.0);
            ui.label(RichText::new("Document").strong());
            ui.checkbox(&mut x.reorder_bookmarks, "Reorder bookmarks when pages move");
            ui.checkbox(
                &mut x.detect_urls,
                "Detect web addresses in page text (Ctrl+click opens them)",
            );
            crate::spell_prefs::page_ui(ui, &mut x.spell);
            ui.add_space(6.0);
            ui.label(RichText::new("Startup").strong());
            ui.horizontal(|ui| {
                ui.label("Start in");
                ui.radio_value(&mut x.startup_mode, "last".to_string(), "Last used");
                ui.radio_value(&mut x.startup_mode, "markup".to_string(), "Markup mode");
                ui.radio_value(&mut x.startup_mode, "view".to_string(), "View mode");
            });
            ui.horizontal(|ui| {
                ui.label("Open on start");
                ui.add(
                    egui::TextEdit::singleline(&mut x.startup_file)
                        .hint_text("No file")
                        .desired_width(220.0),
                );
                if ui.button("Browse...").clicked() {
                    out = Some("startup-file");
                }
            });
            ui.checkbox(&mut x.startup_full_screen, "Start in full screen");
        }
        "Document" => {
            ui.add_space(6.0);
            ui.label(RichText::new("Files").strong());
            ui.checkbox(
                &mut x.locked_prompt,
                "Offer a read-only copy when a file is in use or read-only",
            );
            ui.checkbox(&mut x.rotate_all_pages, "Rotate Pages acts on every page by default");
        }
        "Navigation" => {
            ui.add_space(6.0);
            ui.label(RichText::new("Scrollbars").strong());
            ui.checkbox(&mut x.scrollbars, "Show scrollbars on the document");
            ui.add_enabled(
                x.scrollbars,
                egui::Checkbox::new(&mut x.scrollbars_left, "Vertical scrollbar on the left"),
            );
            ui.add_space(6.0);
            ui.label(RichText::new("Keyboard and views").strong());
            ui.checkbox(&mut x.alt_menus, "Alt + a menu's first letter opens the menu");
            ui.horizontal(|ui| {
                ui.label("New split views synchronize");
                ui.radio_value(&mut x.sync_default, "off".to_string(), "Off");
                ui.radio_value(&mut x.sync_default, "document".to_string(), "Document");
                ui.radio_value(&mut x.sync_default, "page".to_string(), "Page");
            });
        }
        "Grid & Snap" => {
            ui.add_space(6.0);
            ui.label(RichText::new("Snap to").strong());
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut x.snap_endpoints, "Endpoints");
                ui.checkbox(&mut x.snap_midpoints, "Midpoints");
                ui.checkbox(&mut x.snap_intersections, "Intersections");
                ui.checkbox(&mut x.snap_nearest, "Nearest point");
                ui.checkbox(&mut x.snap_centers, "Centers");
            });
            ui.horizontal(|ui| {
                let mut custom = x.snap_color.is_some();
                if ui
                    .checkbox(&mut custom, "Snap indicator and crosshair colour")
                    .changed()
                {
                    x.snap_color = custom.then_some([0xE0, 0x10, 0xC0]);
                }
                if let Some(c) = &mut x.snap_color {
                    ui.color_edit_button_srgb(c);
                }
            });
        }
        "Tools" => {
            ui.add_space(6.0);
            ui.label(RichText::new("Sketch to Scale").strong());
            ui.horizontal(|ui| {
                ui.label("Angles are");
                ui.radio_value(&mut x.sketch_relative, false, "absolute (0 = right)");
                ui.radio_value(&mut x.sketch_relative, true, "relative to the last segment");
            });
            ui.horizontal(|ui| {
                ui.label("Ellipses are typed as");
                ui.radio_value(&mut x.sketch_radius, false, "width x height");
                ui.radio_value(&mut x.sketch_radius, true, "a radius from the centre");
            });
        }
        "Advanced" => {
            ui.add_space(6.0);
            ui.checkbox(&mut x.thin_lines, "Disable line weights (all linework thin)");
            ui.label(
                RichText::new("Hairlines always draw at least one pixel wide; fills are anti-aliased.")
                    .weak()
                    .size(11.0),
            );
            ui.add_space(6.0);
            ui.label(RichText::new("PDF/A").strong());
            ui.checkbox(&mut x.pdfa_locked, "Open PDF/A documents locked for page edits");
            ui.label(
                RichText::new("Document JavaScript runs sandboxed, only when allowed below.")
                    .weak()
                    .size(11.0),
            );
        }
        "Admin" => {
            ui.add_space(6.0);
            ui.label(RichText::new("AI assistants (MCP)").strong());
            ui.label(
                "An AI assistant can use MarkupCraft's tools (open, mark up, measure, export) through \
                 its MCP server. The server is off unless the assistant starts it; it reads and writes \
                 only the folder given with --root.",
            );
            if ui.button("Copy MCP Configuration").clicked() {
                out = Some("copy-mcp");
            }
        }
        _ => {}
    }
    out
}
