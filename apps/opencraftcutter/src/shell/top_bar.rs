use crate::app::{OpenCraftCutterApp, Tool, Workspace};
use crate::canvas::viewport_actions;
use crate::shell::icons::{self, bar_button as icon_button};
use crate::theme;
use egui::{Color32, Frame, RichText, Ui};

fn divider(ui: &mut Ui) {
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
}

pub fn top_bar(app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let ctx = &ctx;
    egui::Panel::top("top_bar")
        .exact_size(40.0)
        .frame(
            Frame::NONE
                .fill(theme::CHROME_900)
                .inner_margin(egui::Margin::symmetric(6, 4)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                menu(app, ui);
                divider(ui);
                if icon_button(ui, icons::NEW, "Nuevo (⌘N)", true, false) {
                    app.new_document(ctx);
                }
                if icon_button(ui, icons::OPEN, "Abrir (⌘O)", true, false) {
                    app.open_document(ctx);
                }
                if icon_button(ui, icons::SAVE, "Guardar (⌘S)", true, false) {
                    app.save_document(ctx, false);
                }
                if icon_button(ui, icons::IMPORT, "Importar SVG (⇧⌘I)", true, false) {
                    app.import_svg(ctx);
                }
                if icon_button(
                    ui,
                    icons::EXPORT,
                    "Exportar como SVG (⌥⌘E)",
                    !app.doc.shapes.is_empty(),
                    false,
                ) {
                    app.export_svg(ctx);
                }
                divider(ui);
                let has_sel = !app.doc.selected.is_empty();
                if icon_button(ui, icons::DUPLICATE, "Duplicar (⌘D)", has_sel, false) {
                    app.commit();
                    app.doc.duplicate_selected();
                }
                if icon_button(ui, icons::DELETE, "Eliminar (⌫)", has_sel, false) {
                    app.delete_selected();
                }
                divider(ui);
                if icon_button(
                    ui,
                    icons::UNDO,
                    "Deshacer (⌘Z)",
                    app.history.can_undo(),
                    false,
                ) {
                    app.undo();
                }
                if icon_button(
                    ui,
                    icons::REDO,
                    "Rehacer (⇧⌘Z)",
                    app.history.can_redo(),
                    false,
                ) {
                    app.redo();
                }
                divider(ui);
                if icon_button(ui, icons::ZOOM_IN, "Acercar (⌘+)", true, false) {
                    viewport_actions::zoom_by(app, 1.25);
                }
                if icon_button(ui, icons::ZOOM_OUT, "Alejar (⌘-)", true, false) {
                    viewport_actions::zoom_by(app, 1.0 / 1.25);
                }
                if icon_button(
                    ui,
                    icons::ZOOM_SELECTION,
                    "Zoom a la selección (⌘2)",
                    has_sel,
                    false,
                ) {
                    viewport_actions::zoom_to_selection(app);
                }
                if icon_button(
                    ui,
                    icons::ZOOM_DRAG,
                    "Zoom de arrastre (Z)",
                    true,
                    app.tool == Tool::ZoomDrag,
                ) {
                    app.tool = Tool::ZoomDrag;
                }
                if icon_button(
                    ui,
                    icons::PAN,
                    "Desplazar (H, o mantén Espacio)",
                    true,
                    app.tool == Tool::Pan,
                ) {
                    app.tool = Tool::Pan;
                }
                if icon_button(ui, icons::FIT, "Ajustar a la página (⌘0)", true, false) {
                    viewport_actions::fit_to_page(app);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (ws, label) in [
                        (Workspace::Send, "ENVIAR"),
                        (Workspace::Library, "BIBLIOTECA"),
                        (Workspace::Design, "DISEÑO"),
                    ] {
                        let active = app.workspace == ws;
                        let text = RichText::new(label).strong().size(12.0).color(if active {
                            theme::CHROME_950
                        } else {
                            theme::CHROME_200
                        });
                        let btn = egui::Button::new(text)
                            .min_size(egui::vec2(96.0, 32.0))
                            .fill(if active {
                                theme::ACCENT
                            } else {
                                Color32::TRANSPARENT
                            })
                            .corner_radius(0.0)
                            .frame(false);
                        if ui.add(btn).clicked() {
                            app.workspace = ws;
                        }
                    }
                });
            });
        });
}

fn menu(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    ui.menu_button("Archivo", |ui| crate::file_menu::menu(app, ui));
    ui.menu_button("Editar", |ui| crate::edit_menu::menu(app, ui));
    ui.menu_button("Objeto", |ui| crate::object_menu::menu(app, ui));
    ui.menu_button("Ver", |ui| view_menu(app, ui));
}

/// Menú Ver, con la estructura y los atajos de la referencia.
fn view_menu(app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    use crate::shell::status_bar::PX_PER_MM_AT_100;
    let pct = (app.canvas.viewport.scale / PX_PER_MM_AT_100 * 100.0).round() as i32;
    ui.menu_button("Zoom", |ui| {
        for z in [25, 33, 50, 75, 0, 100, 0, 150, 200, 400] {
            if z == 0 {
                ui.separator();
                continue;
            }
            if ui
                .add(egui::Button::selectable(pct == z, format!("{z}%")))
                .clicked()
            {
                viewport_actions::zoom_to(app, z as f64 / 100.0 * PX_PER_MM_AT_100);
                ui.close();
            }
        }
        ui.separator();
        if ui
            .add(egui::Button::new("Ajustar a ventana").shortcut_text("⌘1"))
            .clicked()
        {
            viewport_actions::fit_to_page(app);
            ui.close();
        }
        if ui
            .add(egui::Button::new("Acercar").shortcut_text("⌘+"))
            .clicked()
        {
            viewport_actions::zoom_by(app, 1.25);
            ui.close();
        }
        if ui
            .add(egui::Button::new("Alejar").shortcut_text("⌘-"))
            .clicked()
        {
            viewport_actions::zoom_by(app, 1.0 / 1.25);
            ui.close();
        }
        if ui
            .add_enabled(
                !app.doc.selected.is_empty(),
                egui::Button::new("Zoom a la selección").shortcut_text("⌘2"),
            )
            .clicked()
        {
            viewport_actions::zoom_to_selection(app);
            ui.close();
        }
    });
    ui.menu_button("Rotar página", |ui| {
        for deg in [0u16, 90, 180, 270] {
            if ui
                .add(egui::Button::selectable(
                    app.doc.page.preview_rotation == deg,
                    format!("{deg}°"),
                ))
                .clicked()
            {
                app.doc.page.preview_rotation = deg;
                app.dirty = true;
                ui.close();
            }
        }
    });
    ui.separator();
    if ui.button("Opciones de retícula…").clicked() {
        let ctx = ui.ctx().clone();
        crate::panels::open_grid_tab(app, &ctx);
        ui.close();
    }
    let page = &mut app.doc.page;
    let mut changed = false;
    changed |= ui
        .checkbox(&mut page.show_grid, "Mostrar retícula")
        .changed();
    changed |= ui
        .checkbox(&mut page.snap_to_grid, "Ajustar a retícula")
        .changed();
    changed |= ui
        .checkbox(&mut page.smart_snap, "Habilitar ajuste inteligente")
        .changed();
    ui.separator();
    ui.horizontal(|ui| {
        changed |= ui
            .checkbox(&mut page.show_print_margins, "Mostrar bordes de impresión")
            .changed();
        ui.label(egui::RichText::new("⇧⌘P").weak());
    });
    ui.horizontal(|ui| {
        changed |= ui
            .checkbox(&mut page.show_cut_area, "Mostrar bordes de corte")
            .changed();
        ui.label(egui::RichText::new("⇧⌘L").weak());
    });
    if changed {
        app.dirty = true;
    }
    if ui.button("Marcas de registro…").clicked() {
        app.panel = Some(crate::app::Panel::PrintCut);
        ui.close();
    }
    if ui
        .button("Mostrar ajustes de la página de diseño…")
        .clicked()
    {
        app.panel = Some(crate::app::Panel::PageSetup);
        ui.close();
    }
    ui.separator();
    for (ws, label, keys) in [
        (Workspace::Design, "Mostrar página de diseño", "⌥⌘D"),
        (Workspace::Library, "Mostrar biblioteca", "⌥⌘L"),
        (Workspace::Send, "Mostrar ajustes de corte", ""),
    ] {
        if ui
            .add_enabled(
                app.workspace != ws,
                egui::Button::new(label).shortcut_text(keys),
            )
            .clicked()
        {
            app.workspace = ws;
            ui.close();
        }
    }
    ui.separator();
    if ui
        .add(egui::Button::new("Preferencias…").shortcut_text("⌘,"))
        .clicked()
    {
        app.show_prefs = true;
        ui.close();
    }
}
