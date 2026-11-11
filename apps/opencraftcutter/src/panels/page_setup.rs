//! Configuración de página, con las mismas opciones y orden que la referencia de diseño.

use super::{CONTROL_WIDTH, PALETTE, color_picker, row, section};
use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{Color32, RichText, Ui};
use egui_material_icons::icons as mi;
use oc_core::Color;
use oc_core::document::{GridStyle, Orientation};
use oc_core::machines::{
    FEED_TYPES_ALL, MACHINES, MEDIA_AUTO, MEDIA_CUSTOM, MachineGroup, MatId, auto_mat_label,
    auto_media_label, machine_label, mat, mats_for, media_size, media_sizes_for, resolve_mat,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Page,
    Grid,
}

/// Abre Configuración de página en la pestaña Rejilla (Ver › Opciones de retícula…).
pub fn open_grid_tab(app: &mut OpenCraftCutterApp, ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("page_setup_tab"), Tab::Grid));
    app.panel = Some(crate::app::Panel::PageSetup);
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let tab_id = egui::Id::unique("page_setup_tab");
    // En depuración, OPENCRAFTCUTTER_DEBUG_TAB=grid abre la pestaña Rejilla (capturas automatizadas).
    let default_tab = if cfg!(debug_assertions)
        && std::env::var("OPENCRAFTCUTTER_DEBUG_TAB").as_deref() == Ok("grid")
    {
        Tab::Grid
    } else {
        Tab::Page
    };
    let mut tab: Tab = ui.data(|d| d.get_temp(tab_id)).unwrap_or(default_tab);
    ui.horizontal(|ui| {
        if ui
            .selectable_label(
                tab == Tab::Page,
                mi::ICON_DESCRIPTION.rich_text().size(18.0),
            )
            .on_hover_text("Página")
            .clicked()
        {
            tab = Tab::Page;
        }
        if ui
            .selectable_label(tab == Tab::Grid, mi::ICON_GRID_ON.rich_text().size(18.0))
            .on_hover_text("Rejilla")
            .clicked()
        {
            tab = Tab::Grid;
        }
    });
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    ui.separator();
    match tab {
        Tab::Page => page_tab(app, ui),
        Tab::Grid => grid_tab(app, ui),
    }
}

fn page_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let mut page = app.doc.page.clone();
    let unit = page.unit;
    let current_mat = resolve_mat(page.machine, page.mat);
    let max_w = if page.restrict_to_mat && current_mat.width_mm > 0.0 {
        current_mat.width_mm
    } else {
        1000.0
    };
    let max_h = if page.restrict_to_mat && current_mat.height_mm > 0.0 {
        current_mat.height_mm
    } else {
        3000.0
    };

    // 1. Guía
    if ui
        .add(
            egui::Button::new("Guía de configuración de la página")
                .min_size(egui::vec2(ui.available_width(), 28.0)),
        )
        .clicked()
    {
        app.show_page_guide = true;
    }
    ui.separator();

    // 2. Máquina (con grupos "Máquinas heredadas" y "Otros tipos de máquina")
    row(ui, "Máquina", |ui| {
        egui::ComboBox::from_id_salt("machine")
            .width(CONTROL_WIDTH)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .selected_text(machine_label(page.machine))
            .height(520.0)
            .show_ui(ui, |ui| {
                let mut last_group = MachineGroup::Current;
                for mm in MACHINES.iter() {
                    if mm.group != last_group {
                        last_group = mm.group;
                        ui.separator();
                        if let Some(t) = mm.group.title() {
                            ui.label(RichText::new(t).size(13.5).color(theme::CHROME_100));
                        }
                    }
                    if ui
                        .selectable_label(page.machine == mm.id, machine_label(mm.id))
                        .clicked()
                    {
                        page.machine = mm.id;
                        if page.mat != MatId::Auto
                            && !mats_for(mm.id).iter().any(|x| x.id == page.mat)
                        {
                            page.mat = MatId::Auto;
                        }
                    }
                }
            });
    });
    // 3. Tipo de alimentación
    row(ui, "Tipo de alimentación", |ui| {
        egui::ComboBox::from_id_salt("feed")
            .width(CONTROL_WIDTH)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .selected_text(page.feed.name())
            .show_ui(ui, |ui| {
                for f in FEED_TYPES_ALL {
                    ui.selectable_value(&mut page.feed, f, f.name());
                }
            });
    });
    // 4. Tapete de corte (nombre + dimensiones en dos líneas)
    let dims = |w: f64, h: f64| {
        format!(
            "{:.1} × {:.1} {}",
            unit.from_mm(w),
            unit.from_mm(h),
            unit.label()
        )
    };
    row(ui, "Tapete de corte", |ui| {
        let label = if page.mat == MatId::Auto {
            auto_mat_label(page.machine)
        } else {
            mat(page.mat)
                .map(|x| {
                    if x.width_mm > 0.0 {
                        format!("{} ({})", x.name, dims(x.width_mm, x.height_mm))
                    } else {
                        x.name.to_string()
                    }
                })
                .unwrap_or_default()
        };
        egui::ComboBox::from_id_salt("mat")
            .width(CONTROL_WIDTH)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .selected_text(label)
            .height(520.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut page.mat, MatId::Auto, auto_mat_label(page.machine));
                for x in mats_for(page.machine) {
                    let text = if x.width_mm > 0.0 {
                        format!("{}\n{}", x.name, dims(x.width_mm, x.height_mm))
                    } else {
                        x.name.to_string()
                    };
                    ui.selectable_value(&mut page.mat, x.id, text);
                }
            });
    });
    // 5. Tamaño de medios
    let dims3 = |w: f64, h: f64| {
        format!(
            "({:.3} × {:.3} {})",
            unit.from_mm(w),
            unit.from_mm(h),
            unit.label()
        )
    };
    let current_mat_for_media = resolve_mat(page.machine, page.mat);
    row(ui, "Tamaño de medios", |ui| {
        let selected = match media_size(&page.media_size) {
            Some(s) if s.id == MEDIA_AUTO => auto_media_label(page.machine, page.mat),
            Some(s) if s.width_mm > 0.0 => format!("{} {}", s.name, dims3(s.width_mm, s.height_mm)),
            _ => format!("A medida {}", dims3(page.width_mm, page.height_mm)),
        };
        egui::ComboBox::from_id_salt("media")
            .width(CONTROL_WIDTH)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .selected_text(selected)
            .height(560.0)
            .show_ui(ui, |ui| {
                for s in media_sizes_for(page.machine) {
                    let text = match s.id {
                        MEDIA_AUTO => auto_media_label(page.machine, page.mat),
                        MEDIA_CUSTOM => s.name.to_string(),
                        _ => format!("{}\n{}", s.name, dims3(s.width_mm, s.height_mm)),
                    };
                    if ui.selectable_label(page.media_size == s.id, text).clicked() {
                        page.media_size = s.id.to_string();
                        let (w, h) = match s.id {
                            MEDIA_AUTO => (
                                current_mat_for_media.width_mm,
                                current_mat_for_media.height_mm,
                            ),
                            MEDIA_CUSTOM => (0.0, 0.0),
                            _ => (s.width_mm, s.height_mm),
                        };
                        if w > 0.0 {
                            let landscape = page.orientation == Orientation::Landscape;
                            page.width_mm = if landscape { h } else { w };
                            page.height_mm = if landscape { w } else { h };
                        }
                    }
                }
            });
    });

    // 6. Personalización de medios
    ui.add_space(2.0);
    ui.separator();
    section(ui, "Personalización de medios");
    let mut w = unit.from_mm(page.width_mm);
    let mut h = unit.from_mm(page.height_mm);
    ui.label("Ancho personalizado");
    ui.horizontal(|ui| {
        ui.add(
            egui::Slider::new(&mut w, unit.from_mm(10.0)..=unit.from_mm(max_w)).show_value(false),
        );
        ui.add(
            egui::DragValue::new(&mut w)
                .speed(unit.step())
                .range(unit.from_mm(10.0)..=unit.from_mm(max_w))
                .fixed_decimals(unit.decimals())
                .suffix(format!(" {}", unit.label())),
        );
    });
    ui.label("Altura personalizada");
    ui.horizontal(|ui| {
        ui.add(
            egui::Slider::new(&mut h, unit.from_mm(10.0)..=unit.from_mm(max_h)).show_value(false),
        );
        ui.add(
            egui::DragValue::new(&mut h)
                .speed(unit.step())
                .range(unit.from_mm(10.0)..=unit.from_mm(max_h))
                .fixed_decimals(unit.decimals())
                .suffix(format!(" {}", unit.label())),
        );
    });
    if (unit.to_mm(w) - page.width_mm).abs() > 1e-6 || (unit.to_mm(h) - page.height_mm).abs() > 1e-6
    {
        page.width_mm = unit.to_mm(w);
        page.height_mm = unit.to_mm(h);
        page.media_size = "custom".into();
    }
    // 7. Restringir al tapete
    ui.checkbox(
        &mut page.restrict_to_mat,
        "Restringir los medios a la alfombrilla de corte",
    );
    if page.restrict_to_mat {
        if current_mat.width_mm > 0.0 && page.width_mm > current_mat.width_mm {
            page.width_mm = current_mat.width_mm;
        }
        if current_mat.height_mm > 0.0 && page.height_mm > current_mat.height_mm {
            page.height_mm = current_mat.height_mm;
        }
    }
    // 8. Orientación
    ui.horizontal(|ui| {
        let w2 = (ui.available_width() - 6.0) / 2.0;
        for (o, icon, label) in [
            (Orientation::Portrait, mi::ICON_CROP_PORTRAIT, "Vertical"),
            (Orientation::Landscape, mi::ICON_CROP_LANDSCAPE, "Apaisado"),
        ] {
            let selected = page.orientation == o;
            let text = egui::RichText::new(format!("{}  {label}", <&str>::from(icon)))
                .strong()
                .color(if selected {
                    theme::CHROME_950
                } else {
                    theme::CHROME_100
                });
            if ui
                .add_sized([w2, 28.0], egui::Button::selectable(selected, text))
                .clicked()
                && !selected
            {
                page.orientation = o;
                std::mem::swap(&mut page.width_mm, &mut page.height_mm);
            }
        }
    });

    // 9. Transparencia
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
    ui.label(RichText::new("Transparencia").color(theme::CHROME_200));
    let mut t = page.page_transparency * 100.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = ui.available_width() - 104.0;
        ui.add(egui::Slider::new(&mut t, 0.0..=100.0).show_value(false));
        ui.add(
            egui::DragValue::new(&mut t)
                .speed(1.0)
                .range(0.0..=100.0)
                .fixed_decimals(1)
                .suffix(" %"),
        );
    });
    page.page_transparency = (t / 100.0).clamp(0.0, 1.0);

    // 10. Color de la página
    ui.horizontal_wrapped(|ui| {
        if super::swatch(ui, Some(Color::WHITE), page.page_color == Color::WHITE) {
            page.page_color = Color::WHITE;
        }
        for c in PALETTE.iter().skip(1) {
            if super::swatch(ui, Some(*c), page.page_color == *c) {
                page.page_color = *c;
            }
        }
        if let Some(c) = color_picker(ui, page.page_color) {
            page.page_color = c;
        }
        if super::swatch(ui, None, page.page_transparency >= 1.0) {
            page.page_transparency = 1.0;
        }
    });

    // 11. Rotar previsualización
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(super::LABEL_COLUMN_WIDTH, 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(super::LABEL_COLUMN_WIDTH);
                ui.add(
                    egui::Label::new(
                        RichText::new("Rotar previsualización").color(theme::CHROME_200),
                    )
                    .wrap(),
                );
            },
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (deg, icon, tip) in [
                (270u16, mi::ICON_TEXT_ROTATE_VERTICAL, "270°"),
                (180, mi::ICON_TEXT_ROTATION_DOWN, "180°"),
                (90, mi::ICON_TEXT_ROTATE_UP, "90°"),
                (0, mi::ICON_TEXT_ROTATION_NONE, "Sin rotar"),
            ] {
                if ui
                    .selectable_label(page.preview_rotation == deg, icon.rich_text().size(16.0))
                    .on_hover_text(tip)
                    .clicked()
                {
                    page.preview_rotation = deg;
                }
            }
        });
    });

    if page != app.doc.page {
        app.commit();
        app.doc.page = page;
    }
    let _ = Color32::WHITE;
    let _ = theme::CHROME_300;
}

fn grid_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let mut page = app.doc.page.clone();
    let unit = page.unit;
    let tol_id = egui::Id::unique("grid_lines_tab");
    let mut secondary: bool = ui.data(|d| d.get_temp(tol_id)).unwrap_or(false);

    // 1-2. Bordes
    ui.checkbox(&mut page.show_print_margins, "Mostrar bordes de impresión");
    ui.checkbox(&mut page.show_cut_area, "Mostrar bordes de corte");
    ui.separator();
    // 3. Retícula y ajuste
    ui.horizontal(|ui| {
        ui.checkbox(&mut page.show_grid, "Mostrar retícula");
        ui.add_space(8.0);
        ui.checkbox(&mut page.snap_to_grid, "Ajustar a retícula");
    });
    ui.separator();
    // 4-6. Corte inteligente
    ui.checkbox(&mut page.smart_snap, "Habilitar corte inteligente");
    ui.add_enabled(
        page.smart_snap,
        egui::Checkbox::new(
            &mut page.smart_snap_within_media,
            "Acoplado inteligente solo dentro de los medios",
        ),
    );
    ui.add_space(2.0);
    ui.label(RichText::new("Tolerancia").color(theme::CHROME_200));
    let mut tol = unit.from_mm(page.snap_tolerance_mm);
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = ui.available_width() - 104.0;
        ui.add_enabled(
            page.smart_snap,
            egui::Slider::new(&mut tol, 0.0..=unit.from_mm(10.0)).show_value(false),
        );
        ui.add_enabled(
            page.smart_snap,
            egui::DragValue::new(&mut tol)
                .speed(unit.step() / 5.0)
                .range(0.0..=unit.from_mm(10.0))
                .fixed_decimals(unit.decimals() + 1)
                .suffix(format!(" {}", unit.label())),
        );
    });
    page.snap_tolerance_mm = unit.to_mm(tol);

    // 7. Ajustes de cuadrícula
    ui.add_space(2.0);
    ui.separator();
    egui::CollapsingHeader::new(
        RichText::new("Ajustes de cuadrícula")
            .strong()
            .size(13.0)
            .color(theme::CHROME_100),
    )
    .default_open(true)
    .show(ui, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 9.0);
        ui.add_space(2.0);
        // 8. Estilo
        ui.horizontal(|ui| {
            let w2 = (ui.available_width() - 6.0) / 2.0;
            for (style, icon, label) in [
                (GridStyle::Square, mi::ICON_GRID_ON, "Cuadrada"),
                (GridStyle::Isometric, mi::ICON_CHANGE_HISTORY, "Isométrica"),
            ] {
                let selected = page.grid_style == style;
                let text = RichText::new(format!("{}  {label}", <&str>::from(icon)))
                    .strong()
                    .color(if selected {
                        theme::CHROME_950
                    } else {
                        theme::CHROME_100
                    });
                if ui
                    .add_sized([w2, 28.0], egui::Button::selectable(selected, text))
                    .clicked()
                {
                    page.grid_style = style;
                }
            }
        });
        // 9. Espaciado
        ui.label(RichText::new("Espaciado").color(theme::CHROME_200));
        let mut sp = unit.from_mm(page.grid_spacing_mm);
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = ui.available_width() - 104.0;
            ui.add(
                egui::Slider::new(&mut sp, unit.from_mm(1.0)..=unit.from_mm(100.0))
                    .show_value(false),
            );
            ui.add(
                egui::DragValue::new(&mut sp)
                    .speed(unit.step())
                    .range(unit.from_mm(1.0)..=unit.from_mm(100.0))
                    .fixed_decimals(unit.decimals())
                    .suffix(format!(" {}", unit.label())),
            );
        });
        page.grid_spacing_mm = unit.to_mm(sp);
        // 10. Divisiones
        ui.label(RichText::new("Divisiones").color(theme::CHROME_200));
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = ui.available_width() - 104.0;
            ui.add(egui::Slider::new(&mut page.grid_divisions, 1..=10).show_value(false));
            ui.add(egui::DragValue::new(&mut page.grid_divisions).range(1..=10));
        });
        // 11. Colores de líneas
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let w2 = (ui.available_width() - 6.0) / 2.0;
            for (is_secondary, label) in [(false, "Líneas primarias"), (true, "Líneas secundarias")]
            {
                let selected = secondary == is_secondary;
                let text = RichText::new(label).color(if selected {
                    theme::CHROME_950
                } else {
                    theme::CHROME_100
                });
                if ui
                    .add_sized([w2, 26.0], egui::Button::selectable(selected, text))
                    .clicked()
                {
                    secondary = is_secondary;
                }
            }
        });
        let current = if secondary {
            page.grid_secondary_color
        } else {
            page.grid_primary_color
        };
        let mut chosen: Option<Color> = None;
        ui.horizontal_wrapped(|ui| {
            for c in [
                Color::WHITE,
                Color::rgb(0xd8, 0xd8, 0xd8),
                Color::rgb(0xec, 0xec, 0xec),
                Color::rgb(0xb4, 0xb4, 0xb4),
            ] {
                if super::swatch(ui, Some(c), current == c) {
                    chosen = Some(c);
                }
            }
            for c in PALETTE.iter().skip(1) {
                if super::swatch(ui, Some(*c), current == *c) {
                    chosen = Some(*c);
                }
            }
            if let Some(c) = color_picker(ui, current) {
                chosen = Some(c);
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("HEX").small().color(theme::CHROME_300));
            let hex_id = egui::Id::unique(("grid_hex", secondary));
            let mut hex: String = ui
                .data(|d| d.get_temp(hex_id))
                .unwrap_or_else(|| current.to_hex());
            let resp = ui.add(
                egui::TextEdit::singleline(&mut hex)
                    .desired_width(90.0)
                    .font(egui::TextStyle::Monospace),
            );
            if resp.changed() {
                if let Some(c) = Color::from_hex(&hex) {
                    chosen = Some(c);
                }
            }
            if resp.lost_focus() || !resp.has_focus() {
                hex = current.to_hex();
            }
            ui.data_mut(|d| d.insert_temp(hex_id, hex));
        });
        if let Some(c) = chosen {
            if secondary {
                page.grid_secondary_color = c;
            } else {
                page.grid_primary_color = c;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(tol_id, secondary));
    if page != app.doc.page {
        app.commit();
        app.doc.page = page;
    }
}

/// Ventana de ayuda de la configuración de página.
pub fn page_guide_window(app: &mut OpenCraftCutterApp, ctx: &egui::Context) {
    if !app.show_page_guide {
        return;
    }
    let mut open = true;
    egui::Window::new("Guía de configuración de la página").open(&mut open).collapsible(false).resizable(false).default_width(420.0).show(ctx, |ui| {
        let items = [
            ("Máquina", "Modelo de cortadora. Determina el ancho máximo de corte y los tapetes disponibles."),
            ("Tipo de alimentación", "Auto (Manual) carga el material a mano; Bandeja y Alimentador de rollo preparan la página para esos accesorios."),
            ("Tapete de corte", "Tapete que se usará. Auto elige el habitual de la máquina. Sin tapete solo para materiales con soporte propio (vinilo, por ejemplo)."),
            ("Tamaño de medios", "Tamaño del material. Elige uno estándar o personalízalo con los controles de ancho y alto."),
            ("Restringir al tapete", "Impide que la página sea mayor que el área de corte del tapete."),
            ("Vertical / Apaisado", "Orientación de la página. Intercambia ancho y alto."),
            ("Transparencia y color", "Aspecto de la hoja en pantalla, útil para ver el tapete o imitar el color del material. No afecta al corte."),
            ("Rotar previsualización", "Gira la vista del tapete para que coincida con cómo lo cargas en la máquina. No gira el diseño."),
        ];
        for (title, body) in items {
            ui.label(RichText::new(title).strong());
            ui.label(RichText::new(body).color(theme::CHROME_300));
            ui.add_space(6.0);
        }
    });
    if !open {
        app.show_page_guide = false;
    }
}
