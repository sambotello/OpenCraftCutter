//! Pestañas de documentos abiertos.

use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{Color32, Frame, RichText, Ui};
use egui_material_icons::icons as mi;

pub fn document_tabs(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    egui::Panel::top("document_tabs")
        .exact_size(28.0)
        .frame(
            Frame::NONE
                .fill(theme::CHROME_900)
                .inner_margin(egui::Margin {
                    left: 6,
                    right: 6,
                    top: 4,
                    bottom: 0,
                }),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let mut switch: Option<usize> = None;
                let mut close: Option<usize> = None;
                for (i, (name, dirty)) in app.tab_titles().into_iter().enumerate() {
                    let active = i == app.active_tab();
                    let label = format!("{name}{}", if dirty { " •" } else { "" });
                    Frame::NONE
                        .fill(if active {
                            theme::CHROME_700
                        } else {
                            Color32::TRANSPARENT
                        })
                        .corner_radius(egui::CornerRadius {
                            nw: 6,
                            ne: 6,
                            sw: 0,
                            se: 0,
                        })
                        .inner_margin(egui::Margin {
                            left: 10,
                            right: 4,
                            top: 2,
                            bottom: 2,
                        })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let r = ui.add(
                                    egui::Label::new(RichText::new(label).size(11.5).color(
                                        if active {
                                            theme::CHROME_100
                                        } else {
                                            theme::CHROME_300
                                        },
                                    ))
                                    .sense(egui::Sense::click()),
                                );
                                if r.clicked() {
                                    switch = Some(i);
                                }
                                if ui
                                    .add(
                                        egui::Button::new(mi::ICON_CLOSE.rich_text().size(11.0))
                                            .frame(false)
                                            .min_size(egui::vec2(16.0, 16.0)),
                                    )
                                    .on_hover_text("Cerrar documento")
                                    .clicked()
                                {
                                    close = Some(i);
                                }
                            });
                        });
                }
                if ui
                    .add(
                        egui::Button::new(mi::ICON_ADD.rich_text().size(14.0))
                            .frame(false)
                            .min_size(egui::vec2(22.0, 22.0)),
                    )
                    .on_hover_text("Nuevo documento (⌘N)")
                    .clicked()
                {
                    app.new_tab();
                }
                if let Some(i) = switch {
                    app.switch_tab(i);
                }
                if let Some(i) = close {
                    app.close_tab(i);
                }
            });
        });
}
