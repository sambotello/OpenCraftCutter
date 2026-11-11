//! Paneles flotantes del lado derecho (propiedades).

pub mod edit_points;
mod fill;
mod layers;
mod line;
mod modify;
mod offset;
mod page_setup;
mod pixscan;
pub use pixscan::{PixScanState, Tab as PixScanTab, corners_window as pixscan_corners_window};
pub mod print_cut;
pub use page_setup::{open_grid_tab, page_guide_window};
mod replicate;
mod rhinestone;
mod sketch;
mod special_tools;
mod text;
mod trace;
mod transform;
pub mod weeding;

use crate::app::{OpenCraftCutterApp, Panel};
use crate::theme;
use egui::{Frame, RichText, Ui};

pub fn show_floating(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let Some(panel) = app.panel else { return };
    let stage = app.canvas.stage;
    let pos = egui::pos2(stage.max.x - 292.0, stage.min.y + 12.0);
    let ctx = ui.ctx().clone();
    egui::Area::new(egui::Id::unique("floating_panel"))
        .fixed_pos(pos)
        .order(egui::Order::Middle)
        .show(&ctx, |ui| {
            Frame::NONE
                .fill(theme::CHROME_800)
                .stroke(egui::Stroke::new(1.0, theme::CHROME_600))
                .corner_radius(8.0)
                .shadow(egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 16,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(110),
                })
                .show(ui, |ui| {
                    ui.set_width(280.0);
                    ui.set_max_height(stage.height() - 24.0);
                    Frame::NONE
                        .fill(theme::CHROME_700)
                        .inner_margin(egui::Margin::symmetric(8, 5))
                        .show(ui, |ui| {
                            ui.set_width(264.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(panel.title())
                                        .strong()
                                        .size(13.0)
                                        .color(theme::CHROME_100),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button(
                                                egui_material_icons::icons::ICON_CLOSE.rich_text(),
                                            )
                                            .clicked()
                                        {
                                            app.panel = None;
                                        }
                                    },
                                );
                            });
                        });
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        Frame::NONE.inner_margin(10.0).show(ui, |ui| {
                            ui.set_width(260.0);
                            ui.spacing_mut().item_spacing = egui::vec2(8.0, 9.0);
                            match panel {
                                Panel::PageSetup => {
                                    page_setup::show(app, ui);
                                    crate::parts::section(app, ui);
                                }
                                Panel::Transform => transform::show(app, ui),
                                Panel::Fill => fill::show(app, ui),
                                Panel::Line => line::show(app, ui),
                                Panel::Text => {
                                    text::show(app, ui);
                                    text::path_section(app, ui);
                                }
                                Panel::PrintCut => print_cut::show(app, ui),
                                Panel::PixScan => pixscan::show(app, ui),
                                Panel::SpecialTools => special_tools::show(app, ui),
                                Panel::EditPoints => edit_points::show(app, ui),
                                Panel::Layers => layers::show(app, ui),
                                Panel::Modify => modify::show(app, ui),
                                Panel::Offset => offset::show(app, ui),
                                Panel::Trace => trace::show(app, ui),
                                Panel::Replicate => replicate::show(app, ui),
                                Panel::Rhinestone => rhinestone::show(app, ui),
                                Panel::Sketch => sketch::show(app, ui),
                                Panel::Weeding => weeding::show(app, ui),
                                other => coming_soon(ui, other),
                            }
                        });
                    });
                });
        });
}

fn coming_soon(ui: &mut Ui, panel: Panel) {
    ui.vertical_centered(|ui| {
        ui.add_space(18.0);
        ui.label(
            crate::shell::panel_icon(panel)
                .rich_text()
                .size(30.0)
                .color(theme::CHROME_300),
        );
        ui.add_space(6.0);
        ui.label(panel.description().unwrap_or(""));
        ui.add_space(6.0);
        ui.label(
            RichText::new("EN DESARROLLO")
                .small()
                .color(theme::CHROME_300),
        );
        ui.add_space(12.0);
    });
}

/// Etiqueta de sección dentro de un panel.
pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(
        RichText::new(title)
            .strong()
            .size(13.0)
            .color(theme::CHROME_100),
    );
    ui.add_space(1.0);
}

/// Fila etiqueta + control a la derecha.
pub fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_COLUMN_WIDTH, 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(LABEL_COLUMN_WIDTH);
                ui.add(egui::Label::new(RichText::new(label).color(theme::CHROME_200)).wrap());
            },
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
    });
}

/// Ancho de la columna de etiquetas en las filas etiqueta/control.
pub const LABEL_COLUMN_WIDTH: f32 = 104.0;
/// Ancho habitual de un desplegable alineado a la derecha de una fila.
pub const CONTROL_WIDTH: f32 = 148.0;

/// Paleta de colores compartida por los paneles de relleno y línea.
pub const PALETTE: [oc_core::Color; 11] = [
    oc_core::Color::WHITE,
    oc_core::Color::BLACK,
    oc_core::Color::rgb(0x7a, 0x7a, 0x7a),
    oc_core::Color::CUT_RED,
    oc_core::Color::rgb(0xff, 0x7f, 0x27),
    oc_core::Color::rgb(0xff, 0xd2, 0x3f),
    oc_core::Color::rgb(0x3c, 0xb4, 0x4b),
    oc_core::Color::rgb(0x22, 0xb8, 0xd8),
    oc_core::Color::rgb(0x1f, 0x5f, 0xbf),
    oc_core::Color::rgb(0x8e, 0x44, 0xad),
    oc_core::Color::rgb(0xff, 0x69, 0xb4),
];

pub fn swatch(ui: &mut Ui, color: Option<oc_core::Color>, selected: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
    let p = ui.painter();
    match color {
        Some(c) => {
            p.rect_filled(
                rect,
                4.0,
                egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a),
            );
        }
        None => {
            p.rect_filled(rect, 4.0, egui::Color32::from_gray(0xe0));
            p.line_segment(
                [rect.left_bottom(), rect.right_top()],
                egui::Stroke::new(1.5, egui::Color32::RED),
            );
        }
    }
    p.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected {
                theme::ACCENT
            } else {
                theme::CHROME_500
            },
        ),
        egui::StrokeKind::Inside,
    );
    resp.clicked()
}

/// Selector de color personalizado (egui) que devuelve el nuevo color si cambió.
pub fn color_picker(ui: &mut Ui, current: oc_core::Color) -> Option<oc_core::Color> {
    let mut c = egui::Color32::from_rgba_unmultiplied(current.r, current.g, current.b, current.a);
    if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque)
        .changed()
    {
        return Some(oc_core::Color::rgba(c.r(), c.g(), c.b(), c.a()));
    }
    None
}
