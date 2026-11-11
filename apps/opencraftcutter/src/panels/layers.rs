use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{RichText, Ui};
use egui_material_icons::icons as mi;

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    if app.doc.shapes.is_empty() {
        ui.label("El documento está vacío.");
        return;
    }
    let has_sel = !app.doc.selected.is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                has_sel,
                egui::Button::new(mi::ICON_ARROW_UPWARD.rich_text()),
            )
            .on_hover_text("Traer adelante (⌘])")
            .clicked()
        {
            app.commit();
            app.doc.reorder_selected(1);
        }
        if ui
            .add_enabled(
                has_sel,
                egui::Button::new(mi::ICON_ARROW_DOWNWARD.rich_text()),
            )
            .on_hover_text("Enviar atrás (⌘[)")
            .clicked()
        {
            app.commit();
            app.doc.reorder_selected(-1);
        }
    });
    let shift = ui.input(|i| i.modifiers.shift);
    let rows: Vec<(u64, String, &'static str, bool, bool)> = app
        .doc
        .shapes
        .iter()
        .rev()
        .map(|s| (s.id, s.name.clone(), s.kind_name(), s.visible, s.locked))
        .collect();
    for (id, name, kind, visible, locked) in rows {
        let selected = app.doc.is_selected(id);
        let frame = egui::Frame::NONE
            .fill(if selected {
                theme::ACCENT_SOFT
            } else {
                egui::Color32::TRANSPARENT
            })
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(6, 2));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let label =
                    ui.add(egui::Label::new(format!("{name}  ")).sense(egui::Sense::click()));
                ui.label(RichText::new(kind).small().color(theme::CHROME_300));
                if label.clicked() {
                    if shift {
                        app.doc.toggle_selected(id);
                    } else {
                        app.doc.selected = vec![id];
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if state_button(
                        ui,
                        locked,
                        mi::ICON_LOCK,
                        mi::ICON_LOCK_OPEN,
                        "Bloqueado: clic para desbloquear",
                        "Desbloqueado: clic para bloquear",
                    ) {
                        app.commit();
                        if let Some(s) = app.doc.shape_mut(id) {
                            s.locked = !s.locked;
                        }
                    }
                    // El ojo resalta el estado «oculto» (el poco habitual).
                    if state_button(
                        ui,
                        !visible,
                        mi::ICON_VISIBILITY_OFF,
                        mi::ICON_VISIBILITY,
                        "Oculto: clic para mostrar",
                        "Visible: clic para ocultar",
                    ) {
                        app.commit();
                        if let Some(s) = app.doc.shape_mut(id) {
                            s.visible = !s.visible;
                        }
                    }
                });
            });
        });
    }
}

/// Botón de estado: activo = icono `on` en ámbar sobre fondo; inactivo = icono `off` atenuado.
fn state_button(
    ui: &mut Ui,
    active: bool,
    on: egui_material_icons::MaterialIcon,
    off: egui_material_icons::MaterialIcon,
    tip_on: &str,
    tip_off: &str,
) -> bool {
    const AMBER: egui::Color32 = egui::Color32::from_rgb(0xff, 0xb3, 0x2e);
    let (icon, color, fill) = if active {
        (
            on,
            AMBER,
            egui::Color32::from_rgba_unmultiplied(0xff, 0xb3, 0x2e, 40),
        )
    } else {
        (off, theme::CHROME_500, egui::Color32::TRANSPARENT)
    };
    ui.add(
        egui::Button::new(icon.rich_text().size(16.0).color(color))
            .fill(fill)
            .min_size(egui::vec2(24.0, 22.0)),
    )
    .on_hover_text(if active { tip_on } else { tip_off })
    .clicked()
}
