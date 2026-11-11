//! Ventana de preferencias.

use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{RichText, Ui};
use oc_core::machines::MACHINES;
use oc_core::units::Unit;

pub fn preferences_window(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    if !app.show_prefs {
        return;
    }
    let ctx = ui.ctx().clone();
    let mut open = true;
    let mut prefs = app.prefs.clone();
    egui::Window::new("Preferencias")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(360.0)
        .show(&ctx, |ui| {
            ui.label(RichText::new("Documentos nuevos").strong());
            egui::Grid::new("prefs_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Unidades");
                    egui::ComboBox::from_id_salt("prefs_unit")
                        .width(100.0)
                        .selected_text(prefs.unit.label())
                        .show_ui(ui, |ui| {
                            for u in Unit::ALL {
                                ui.selectable_value(&mut prefs.unit, u, u.label());
                            }
                        });
                    ui.end_row();
                    ui.label("Máquina");
                    egui::ComboBox::from_id_salt("prefs_machine")
                        .width(160.0)
                        .selected_text(oc_core::machines::machine_label(prefs.machine))
                        .show_ui(ui, |ui| {
                            for m in MACHINES.iter() {
                                ui.selectable_value(
                                    &mut prefs.machine,
                                    m.id,
                                    oc_core::machines::machine_label(m.id),
                                );
                            }
                        });
                    ui.end_row();
                    ui.label("Mostrar rejilla");
                    ui.checkbox(&mut prefs.show_grid, "");
                    ui.end_row();
                    ui.label("Espaciado de rejilla");
                    let u = prefs.unit;
                    let mut g = u.from_mm(prefs.grid_spacing_mm);
                    if ui
                        .add(
                            egui::DragValue::new(&mut g)
                                .speed(u.step())
                                .range(u.from_mm(1.0)..=u.from_mm(100.0))
                                .fixed_decimals(u.decimals())
                                .suffix(format!(" {}", u.label())),
                        )
                        .changed()
                    {
                        prefs.grid_spacing_mm = u.to_mm(g);
                    }
                    ui.end_row();
                    ui.label("Margen de corte");
                    let mut m = u.from_mm(prefs.cut_margin_mm);
                    if ui
                        .add(
                            egui::DragValue::new(&mut m)
                                .speed(u.step())
                                .range(0.0..=u.from_mm(50.0))
                                .fixed_decimals(u.decimals())
                                .suffix(format!(" {}", u.label())),
                        )
                        .changed()
                    {
                        prefs.cut_margin_mm = u.to_mm(m);
                    }
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.label(RichText::new("Biblioteca").strong());
            ui.horizontal(|ui| {
                let shown = prefs
                    .library_root
                    .clone()
                    .unwrap_or_else(|| "~/Documents/OpenCraftCutter (por defecto)".into());
                ui.label(RichText::new(shown).small().color(theme::CHROME_300));
            });
            ui.horizontal(|ui| {
                if ui.button("Elegir carpeta…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        prefs.library_root = Some(dir.display().to_string());
                    }
                }
                if prefs.library_root.is_some() && ui.button("Restablecer").clicked() {
                    prefs.library_root = None;
                }
            });
            ui.add_space(8.0);
            ui.label(RichText::new("Herramientas").strong());
            ui.horizontal(|ui| {
                ui.label("Grosor del borrador");
                let u = prefs.unit;
                let mut w = u.from_mm(prefs.eraser_width_mm);
                if ui
                    .add(
                        egui::DragValue::new(&mut w)
                            .speed(u.step() / 2.0)
                            .range(u.from_mm(0.5)..=u.from_mm(50.0))
                            .fixed_decimals(u.decimals())
                            .suffix(format!(" {}", u.label())),
                    )
                    .changed()
                {
                    prefs.eraser_width_mm = u.to_mm(w);
                }
            });
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Los cambios se guardan automáticamente y se aplican a los documentos nuevos.",
                )
                .small()
                .color(theme::CHROME_300),
            );
        });
    if prefs != app.prefs {
        let root_changed = prefs.library_root != app.prefs.library_root;
        app.canvas.eraser_width_mm = prefs.eraser_width_mm;
        app.prefs = prefs;
        if root_changed {
            app.library.set_root(app.prefs.library_root.as_deref());
        }
    }
    if !open {
        app.show_prefs = false;
    }
}
