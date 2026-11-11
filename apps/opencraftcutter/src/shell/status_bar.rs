use crate::app::OpenCraftCutterApp;
use crate::canvas::viewport_actions;
use crate::theme;
use egui::{Frame, RichText};
use oc_core::edit::selection_bounds;
use oc_core::units::Unit;

/// 100 % = tamaño real aproximado en una pantalla de 96 dpi.
pub const PX_PER_MM_AT_100: f64 = 96.0 / 25.4;

pub fn status_bar(app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let ctx = &ctx;
    egui::Panel::bottom("status_bar")
        .exact_size(24.0)
        .frame(
            Frame::NONE
                .fill(theme::CHROME_900)
                .inner_margin(egui::Margin::symmetric(10, 3)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let unit = app.doc.page.unit;
                let small = |t: String| RichText::new(t).small().color(theme::CHROME_300);
                let cursor = app
                    .canvas
                    .cursor_mm
                    .map(|p| format!("X {}   Y {}", unit.format(p.x), unit.format(p.y)))
                    .unwrap_or_default();
                ui.add_sized([170.0, 16.0], egui::Label::new(small(cursor)));
                if let Some(b) = selection_bounds(app.doc.selected_shapes()) {
                    ui.label(small(format!(
                        "{} obj · {} × {}",
                        app.doc.selected.len(),
                        unit.format(b.width()),
                        unit.format(b.height())
                    )));
                }
                if app.tool.in_development() {
                    ui.label(
                        RichText::new("Herramienta en desarrollo")
                            .small()
                            .color(egui::Color32::from_rgb(0xfb, 0xbf, 0x24)),
                    );
                }
                if let Some((msg, until)) = &app.toast {
                    if ctx.input(|i| i.time) < *until {
                        ui.label(RichText::new(msg).small().color(theme::ACCENT));
                        ctx.request_repaint_after(std::time::Duration::from_millis(500));
                    } else {
                        app.toast = None;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let pct = (app.canvas.viewport.scale / PX_PER_MM_AT_100 * 100.0).round() as i32;
                    egui::ComboBox::from_id_salt("zoom")
                        .width(70.0)
                        .selected_text(format!("{pct} %"))
                        .show_ui(ui, |ui| {
                            for z in [25, 50, 75, 100, 150, 200, 400, 800] {
                                if ui.selectable_label(pct == z, format!("{z} %")).clicked() {
                                    viewport_actions::zoom_to(
                                        app,
                                        z as f64 / 100.0 * PX_PER_MM_AT_100,
                                    );
                                }
                            }
                        });
                    let mut u = unit;
                    egui::ComboBox::from_id_salt("unit")
                        .width(56.0)
                        .selected_text(u.label())
                        .show_ui(ui, |ui| {
                            for x in Unit::ALL {
                                ui.selectable_value(&mut u, x, x.label());
                            }
                        });
                    if u != unit {
                        app.doc.page.unit = u;
                    }
                });
            });
        });
}
