//! Paleta y estilo visual: gris carbón con acento cian, en línea con la referencia de diseño.

use egui::{Color32, CornerRadius, Stroke, Visuals, style::WidgetVisuals};

pub const CHROME_950: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x1f);
pub const CHROME_900: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
pub const CHROME_800: Color32 = Color32::from_rgb(0x2e, 0x2e, 0x2e);
pub const CHROME_700: Color32 = Color32::from_rgb(0x38, 0x38, 0x38);
pub const CHROME_600: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);
pub const CHROME_500: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x5a);
pub const CHROME_400: Color32 = Color32::from_rgb(0x7a, 0x7a, 0x7a);
pub const CHROME_300: Color32 = Color32::from_rgb(0xa0, 0xa0, 0xa0);
pub const CHROME_200: Color32 = Color32::from_rgb(0xc8, 0xc8, 0xc8);
pub const CHROME_100: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);
pub const ACCENT: Color32 = Color32::from_rgb(0x22, 0xb8, 0xd8);
pub const ACCENT_STRONG: Color32 = Color32::from_rgb(0x12, 0xa2, 0xc2);
pub const ACCENT_SOFT: Color32 =
    Color32::from_rgba_premultiplied(0x22 / 5, 0xb8 / 5, 0xd8 / 5, 0x33);

pub const MAT: Color32 = Color32::from_rgb(0xd6, 0xd6, 0xd6);
pub const MAT_BORDER: Color32 = Color32::from_rgb(0xc2, 0xc2, 0xc2);
pub const MAT_GRID: Color32 = Color32::from_rgb(0xb4, 0xb4, 0xb4);
pub const MAT_LABEL: Color32 = Color32::from_rgb(0x8a, 0x8a, 0x8a);
pub const CUT_AREA: Color32 = Color32::from_rgb(0xe3, 0x34, 0x3f);
pub const SELECTION: Color32 = ACCENT;

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = CHROME_900;
    v.window_fill = CHROME_800;
    v.extreme_bg_color = CHROME_950;
    v.faint_bg_color = CHROME_800;
    v.code_bg_color = CHROME_950;
    v.override_text_color = Some(CHROME_100);
    v.weak_text_color = Some(CHROME_300);
    v.weak_text_alpha = 1.0;
    v.selection.bg_fill = ACCENT_STRONG;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.window_corner_radius = CornerRadius::same(8);
    v.window_stroke = Stroke::new(1.0, CHROME_600);
    v.window_shadow.color = Color32::from_black_alpha(120);

    let radius = CornerRadius::same(5);
    let w = |bg: Color32, border: Color32, fg: Color32| WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: Stroke::new(1.0, border),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0, fg),
        expansion: 0.0,
    };
    v.widgets.noninteractive = w(CHROME_800, CHROME_600, CHROME_200);
    v.widgets.inactive = w(CHROME_700, CHROME_600, CHROME_200);
    v.widgets.hovered = w(CHROME_600, CHROME_500, Color32::WHITE);
    v.widgets.active = w(ACCENT, ACCENT_STRONG, CHROME_950);
    v.widgets.open = w(CHROME_600, CHROME_500, Color32::WHITE);

    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.interact_size.y = 24.0;
        s.spacing.slider_width = 140.0;
        s.interaction.selectable_labels = false;
        for (style, font) in s.text_styles.iter_mut() {
            match style {
                egui::TextStyle::Body => font.size = 12.5,
                egui::TextStyle::Button => font.size = 12.5,
                egui::TextStyle::Small => font.size = 10.5,
                egui::TextStyle::Heading => font.size = 15.0,
                _ => {}
            }
        }
    });
}

/// Nombre accesible de un botón de solo icono (lo leen VoiceOver y las pruebas de interfaz).
pub fn name_widget(r: &egui::Response, name: &str) {
    let enabled = r.enabled();
    r.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Button, enabled, name));
}
