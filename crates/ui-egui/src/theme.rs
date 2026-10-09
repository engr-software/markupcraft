//! Design tokens and the egui style: neutral grey chrome, white panels, one blue accent, a mid
//! grey workspace behind the pages (a drawing reads best on grey). Every custom widget reads
//! [`Tokens::get`]. Token layout adapted from PdfCraft's `theme.rs` (MIT OR Apache-2.0).

use egui::{Color32, CornerRadius, Stroke, Visuals};

#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    /// Menu bar and toolbars.
    pub chrome: Color32,
    /// Docked panels.
    pub panel: Color32,
    /// Workspace behind the pages.
    pub workspace: Color32,
    pub border: Color32,
    pub divider: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub icon: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub selected: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub accent_soft: Color32,
    pub page_shadow: Color32,
    /// Selection outline and handles on the canvas.
    pub select: Color32,
    pub radius: u8,
}

impl Tokens {
    pub const LIGHT: Tokens = Tokens {
        chrome: Color32::from_rgb(0xF3, 0xF3, 0xF5),
        panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        workspace: Color32::from_rgb(0x8E, 0x92, 0x98),
        border: Color32::from_rgb(0xD2, 0xD3, 0xD8),
        divider: Color32::from_rgb(0xE2, 0xE3, 0xE7),
        text: Color32::from_rgb(0x20, 0x21, 0x25),
        text_muted: Color32::from_rgb(0x55, 0x57, 0x5E),
        text_faint: Color32::from_rgb(0x7A, 0x7C, 0x84),
        icon: Color32::from_rgb(0x3A, 0x3C, 0x42),
        hover: Color32::from_rgb(0xE6, 0xE7, 0xEB),
        pressed: Color32::from_rgb(0xD9, 0xDB, 0xE1),
        selected: Color32::from_rgb(0xDC, 0xE8, 0xFC),
        accent: Color32::from_rgb(0x1B, 0x63, 0xE0),
        accent_text: Color32::from_rgb(0x17, 0x55, 0xC4),
        accent_soft: Color32::from_rgb(0xDE, 0xE9, 0xFC),
        page_shadow: Color32::from_black_alpha(60),
        select: Color32::from_rgb(0x1B, 0x63, 0xE0),
        radius: 4,
    };

    pub fn get(ctx: &egui::Context) -> Self {
        ctx.data(|d| d.get_temp::<Tokens>(egui::Id::new("markupcraft-theme")))
            .unwrap_or(Self::LIGHT)
    }
}

/// Install the tokens and the matching egui visuals.
pub fn apply(ctx: &egui::Context) {
    let t = Tokens::LIGHT;
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("markupcraft-theme"), t));
    let mut v = Visuals::light();
    v.panel_fill = t.panel;
    v.window_fill = t.panel;
    v.extreme_bg_color = Color32::WHITE;
    v.faint_bg_color = Color32::from_rgb(0xF6, 0xF7, 0xF9);
    v.selection.bg_fill = t.selected;
    v.selection.stroke = Stroke::new(1.0, t.accent_text);
    v.hyperlink_color = t.accent_text;
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.divider);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.active.weak_bg_fill = t.pressed;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 3.0);
        s.spacing.interact_size.y = 22.0;
    });
}

/// An egui colour from a model colour and an opacity in 0..=1.
pub fn color32(c: &markupcraft_model::Color, alpha: f64) -> Color32 {
    let ch = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(ch(c.r), ch(c.g), ch(c.b), ch(alpha))
}
