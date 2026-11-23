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
    // Forma cuyo nombre se está editando en la lista, con el texto en curso.
    let rename_id = egui::Id::unique("capas_renombrar");
    let mut renaming: Option<(u64, String)> = ui.data(|d| d.get_temp(rename_id)).flatten();
    let was_renaming = renaming.as_ref().map(|r| r.0);
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
                if let Some((_, text)) = renaming.as_mut().filter(|r| r.0 == id) {
                    let field = ui.add(
                        egui::TextEdit::singleline(text)
                            .desired_width(130.0)
                            .id(egui::Id::unique(("capas_renombrar_campo", id))),
                    );
                    crate::theme::name_widget(&field, "Nombre de la forma");
                    // Recién abierto: toma el foco para escribir directamente.
                    if was_renaming != Some(id) || !field.has_focus() && !field.lost_focus() {
                        field.request_focus();
                    }
                    if field.lost_focus() {
                        let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                        let new = text.trim().to_string();
                        if !cancelled && !new.is_empty() && new != name {
                            app.rename_shape(id, &new);
                        }
                        renaming = None;
                    }
                    return;
                }
                let label =
                    ui.add(egui::Label::new(format!("{name}  ")).sense(egui::Sense::click()));
                ui.label(RichText::new(kind).small().color(theme::CHROME_300));
                // Doble clic en el nombre: renombrar.
                if label.double_clicked() {
                    app.doc.selected = vec![id];
                    renaming = Some((id, name.clone()));
                    return;
                }
                if label.clicked() {
                    if shift {
                        app.doc.toggle_selected(id);
                    } else {
                        app.doc.selected = vec![id];
                    }
                }
                // Clic derecho: la fila pasa a estar seleccionada (si no lo estaba ya) y el
                // menú actúa sobre la selección.
                if label.secondary_clicked() && !app.doc.is_selected(id) {
                    app.doc.selected = vec![id];
                }
                label.context_menu(|ui| {
                    if row_menu(app, ui) {
                        renaming = Some((id, name.clone()));
                    }
                });
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
    ui.data_mut(|d| d.insert_temp(rename_id, renaming));
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

impl OpenCraftCutterApp {
    /// Cambia el nombre de una forma (una entrada de historial).
    pub fn rename_shape(&mut self, id: u64, name: &str) {
        if self.doc.shape(id).is_none_or(|s| s.name == name) {
            return;
        }
        self.commit();
        if let Some(s) = self.doc.shape_mut(id) {
            s.name = name.to_string();
        }
    }
}

/// Menú contextual de una fila: actúa sobre las formas seleccionadas. Devuelve si se pidió
/// renombrar la fila.
fn row_menu(app: &mut OpenCraftCutterApp, ui: &mut Ui) -> bool {
    use crate::canvas::context_menu;
    ui.set_min_width(190.0);
    let ctx = ui.ctx().clone();
    let all_locked = app.doc.selected_shapes().iter().all(|s| s.locked);
    let item = |ui: &mut Ui, label: &str, keys: &str| {
        let clicked = ui
            .add(egui::Button::new(label).shortcut_text(keys))
            .clicked();
        if clicked {
            ui.close();
        }
        clicked
    };
    let mut rename = false;
    if app.doc.selected.len() == 1 && item(ui, "Renombrar", "") {
        rename = true;
    }
    if item(ui, "Copiar", "⌘C") {
        app.copy_selection(&ctx);
    }
    if item(ui, "Duplicar", "⌘D") {
        app.commit();
        app.doc.duplicate_selected();
    }
    if !all_locked && item(ui, "Borrar", "⌫") {
        app.delete_selected();
    }
    ui.separator();
    if !all_locked {
        context_menu::arrange_menus(app, ui);
        ui.separator();
    }
    context_menu::lock_and_hide(app, ui);
    ui.separator();
    if item(ui, "Zoom a la selección", "⌘2") {
        crate::canvas::viewport_actions::zoom_to_selection(app);
    }
    rename
}
