//! Panel Trazar, con las opciones y el orden de la referencia.

use super::section;
use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{RichText, Ui};
use egui_material_icons::icons as mi;
use kurbo::Point;
use oc_core::shape::{ImageData, Shape, ShapeKind};
use oc_trace::{TraceMode, TraceSettings};

/// Ajustes del panel en porcentaje, como en la referencia.
#[derive(Clone, Copy, PartialEq)]
struct PanelSettings {
    threshold_pct: f64,
    speckle_pct: f64,
    invert: bool,
    color: bool,
    smooth: bool,
    corner: i32,
    high_pass_on: bool,
    high_pass: f32,
    low_pass_on: bool,
    low_pass: f32,
    scale: u32,
    quality: u8,
    outline_preview: bool,
}

impl Default for PanelSettings {
    fn default() -> Self {
        PanelSettings {
            threshold_pct: 50.0,
            speckle_pct: 0.0,
            invert: false,
            color: false,
            smooth: true,
            corner: 45,
            high_pass_on: false,
            high_pass: 5.0,
            low_pass_on: false,
            low_pass: 0.0,
            scale: 4,
            quality: 3,
            outline_preview: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Trace,
    Outer,
    Detach,
}

/// Convierte el área (mm del mundo) a coordenadas normalizadas de la imagen.
fn normalized_area(img: &Shape, area: kurbo::Rect) -> [f64; 4] {
    let inv = oc_core::edit::rotation_about(img.center(), -img.rotation);
    let a = inv * Point::new(area.x0, area.y0);
    let b = inv * Point::new(area.x1, area.y1);
    let r = img.rect;
    let u = |x: f64| ((x - r.x0) / r.width()).clamp(0.0, 1.0);
    let v = |y: f64| ((y - r.y0) / r.height()).clamp(0.0, 1.0);
    [
        u(a.x.min(b.x)),
        v(a.y.min(b.y)),
        u(a.x.max(b.x)),
        v(a.y.max(b.y)),
    ]
}

fn settings_for(
    ps: &PanelSettings,
    img: &Shape,
    area: Option<kurbo::Rect>,
    mode: TraceMode,
) -> TraceSettings {
    let (pw, ph) = match &img.kind {
        ShapeKind::Image {
            px_width,
            px_height,
            ..
        } => (*px_width as f64, *px_height as f64),
        _ => (1.0, 1.0),
    };
    TraceSettings {
        threshold: (ps.threshold_pct * 2.55).round().clamp(0.0, 255.0) as u8,
        invert: ps.invert,
        // 100 % = descartar manchas de hasta el 1 % del área de la imagen.
        filter_speckle: ((ps.speckle_pct / 100.0) * pw * ph * 0.01).round() as usize + 2,
        smooth: ps.smooth,
        corner_threshold: ps.corner,
        color: ps.color,
        area: area.map(|a| normalized_area(img, a)),
        mode,
        high_pass: if ps.high_pass_on { ps.high_pass } else { 0.0 },
        low_pass: if ps.low_pass_on { ps.low_pass } else { 0.0 },
        scale: ps.scale,
        quality: ps.quality,
    }
}

fn pct_slider(ui: &mut Ui, label: &str, value: &mut f64) {
    ui.label(RichText::new(label).color(theme::CHROME_200));
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = ui.available_width() - 104.0;
        ui.add(egui::Slider::new(value, 0.0..=100.0).show_value(false));
        ui.add(
            egui::DragValue::new(value)
                .range(0.0..=100.0)
                .fixed_decimals(1)
                .suffix(" %"),
        );
    });
}

/// Filtro con interruptor y valor, como «Paso alto (Encendido) · Valor».
fn filter_row(ui: &mut Ui, label: &str, on: &mut bool, value: &mut f32, max: f32, tip: &str) {
    let state = if *on { "Encendido" } else { "Apagado" };
    ui.checkbox(on, format!("{label} ({state})"))
        .on_hover_text(tip);
    ui.add_enabled_ui(*on, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Valor").color(theme::CHROME_200));
            ui.spacing_mut().slider_width = ui.available_width() - 70.0;
            ui.add(egui::Slider::new(value, 0.0..=max).show_value(false));
            ui.add(
                egui::DragValue::new(value)
                    .range(0.0..=max)
                    .speed(0.05)
                    .fixed_decimals(2),
            );
        });
    });
}

fn action_button(
    ui: &mut Ui,
    icon: egui_material_icons::MaterialIcon,
    label: &str,
    tip: &str,
    enabled: bool,
) -> bool {
    let text = RichText::new(format!("{}   {label}", <&str>::from(icon)));
    ui.add_enabled(
        enabled,
        egui::Button::new(text).min_size(egui::vec2(ui.available_width(), 30.0)),
    )
    .on_hover_text(tip)
    .clicked()
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let id = egui::Id::unique("trace_panel_settings");
    let mut ps: PanelSettings = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let image: Option<Shape> = app
        .doc
        .selected_shapes()
        .into_iter()
        .find(|s| s.is_image())
        .cloned();
    let area = match (&image, app.canvas.trace_area) {
        (Some(img), Some((aid, r))) if aid == img.id => Some(r),
        _ => None,
    };

    if image.is_none() {
        ui.label(
            RichText::new("Selecciona una imagen. Impórtala con Archivo → Importar imagen…")
                .small()
                .color(theme::CHROME_300),
        );
        if ui.button("Importar imagen…").clicked() {
            let ctx = ui.ctx().clone();
            app.import_image(&ctx);
        }
        ui.separator();
    }

    // 1. Seleccionar área de trazado
    let selecting = app.canvas.trace_area_select.is_some();
    let label = if selecting {
        "Arrastra sobre la imagen…"
    } else if area.is_some() {
        "Cambiar área de trazado"
    } else {
        "Seleccionar área de trazado"
    };
    ui.horizontal(|ui| {
        let w = ui.available_width() - if area.is_some() { 34.0 } else { 0.0 };
        let text = RichText::new(format!("{}   {label}", <&str>::from(mi::ICON_CROP_FREE)));
        if ui
            .add_enabled(
                image.is_some(),
                egui::Button::selectable(selecting, text)
                    .frame(true)
                    .min_size(egui::vec2(w, 30.0)),
            )
            .on_hover_text("Arrastra un rectángulo sobre la imagen para trazar solo esa parte")
            .clicked()
        {
            app.canvas.trace_area_select = if selecting {
                None
            } else {
                image.as_ref().map(|i| i.id)
            };
        }
        if area.is_some()
            && ui
                .add(egui::Button::new(mi::ICON_CLOSE.rich_text()).min_size(egui::vec2(28.0, 30.0)))
                .on_hover_text("Quitar el área: trazar toda la imagen")
                .clicked()
        {
            app.canvas.trace_area = None;
        }
    });

    // 2-3. Umbrales
    ui.add_space(2.0);
    pct_slider(ui, "Umbral", &mut ps.threshold_pct);
    pct_slider(ui, "Umbral de eliminación de manchas", &mut ps.speckle_pct);

    // 4. Avanzado
    egui::CollapsingHeader::new(RichText::new("Avanzado").strong().color(theme::CHROME_100))
        // Depuración: OPENCRAFTCUTTER_DEBUG_OPEN=1 lo abre al arrancar (capturas de la interfaz).
        .default_open(
            cfg!(debug_assertions) && std::env::var_os("OPENCRAFTCUTTER_DEBUG_OPEN").is_some(),
        )
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 9.0);
            filter_row(
                ui,
                "Filtro de paso alto",
                &mut ps.high_pass_on,
                &mut ps.high_pass,
                20.0,
                "Realza los bordes: útil en fotos o imágenes con degradados",
            );
            filter_row(
                ui,
                "Filtro de paso bajo",
                &mut ps.low_pass_on,
                &mut ps.low_pass,
                10.0,
                "Suaviza el ruido antes de aplicar el umbral",
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Escala").color(theme::CHROME_200));
                ui.spacing_mut().slider_width = ui.available_width() - 70.0;
                ui.add(egui::Slider::new(&mut ps.scale, 1..=10).show_value(false))
                    .on_hover_text("Amplía la imagen antes de trazar: curvas más finas, más lento");
                ui.add(egui::DragValue::new(&mut ps.scale).range(1..=10));
            });
            ui.label(RichText::new("Calidad del trazo").color(theme::CHROME_200));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Baja").small().color(theme::CHROME_300));
                ui.spacing_mut().slider_width = ui.available_width() - 40.0;
                ui.add(egui::Slider::new(&mut ps.quality, 0..=4).show_value(false))
                    .on_hover_text("Más calidad sigue mejor los detalles, con más nodos");
                ui.label(RichText::new("Máx").small().color(theme::CHROME_300));
            });
            ui.label(RichText::new("Vista previa del trazado").color(theme::CHROME_200));
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 8.0) / 2.0;
                for (outline, text) in [(false, "Relleno sólido"), (true, "Contorneadas")] {
                    if ui
                        .add(
                            egui::Button::selectable(ps.outline_preview == outline, text)
                                .frame(true)
                                .min_size(egui::vec2(w, 26.0)),
                        )
                        .clicked()
                    {
                        ps.outline_preview = outline;
                    }
                }
            });
            ui.separator();
            ui.checkbox(&mut ps.invert, "Invertir (trazar lo claro)");
            ui.checkbox(&mut ps.color, "Por colores (una forma por color)");
            ui.checkbox(&mut ps.smooth, "Suavizar curvas");
            super::row(ui, "Ángulo de esquina", |ui| {
                ui.add(
                    egui::DragValue::new(&mut ps.corner)
                        .range(0..=180)
                        .suffix(" °"),
                );
            });
        });

    // Vista previa del umbral sobre la imagen
    if let Some(img) = &image {
        if let ShapeKind::Image { png, .. } = &img.kind {
            let st = settings_for(&ps, img, area, TraceMode::All);
            let key = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (img.id, st.threshold, st.invert, png.0.len()).hash(&mut h);
                (
                    st.high_pass.to_bits(),
                    st.low_pass.to_bits(),
                    ps.outline_preview,
                )
                    .hash(&mut h);
                st.area.map(|a| a.map(|v| (v * 1e4) as i64)).hash(&mut h);
                h.finish()
            };
            if app
                .canvas
                .trace_preview
                .as_ref()
                .is_none_or(|(k, _, _)| *k != key)
            {
                if let Ok((rgba, w, h)) =
                    oc_trace::threshold_preview(&png.0, &st, ps.outline_preview)
                {
                    let image =
                        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    let tex =
                        ui.ctx()
                            .load_texture("trace-preview", image, egui::TextureOptions::LINEAR);
                    app.canvas.trace_preview = Some((key, img.id, tex));
                }
            }
        }
    }

    // 5-7. Acciones
    ui.separator();
    section(ui, "Trazar");
    let enabled = image.is_some();
    let mut action = None;
    if action_button(
        ui,
        mi::ICON_AUTO_FIX_HIGH,
        "Trazar",
        "Crea líneas de corte en todos los bordes, agujeros incluidos",
        enabled,
    ) {
        action = Some(Action::Trace);
    }
    if action_button(
        ui,
        mi::ICON_SHAPE_LINE,
        "Trazar contorno externo",
        "Solo el contorno exterior de cada zona, sin agujeros",
        enabled,
    ) {
        action = Some(Action::Outer);
    }
    if action_button(
        ui,
        mi::ICON_CONTENT_CUT,
        "Trazar y separar",
        "Traza el contorno exterior y recorta la imagen a esa forma",
        enabled,
    ) {
        action = Some(Action::Detach);
    }
    ui.label(
        RichText::new("En amarillo, lo que se trazará con el umbral actual.")
            .small()
            .color(theme::CHROME_400),
    );

    if let (Some(act), Some(img)) = (action, image) {
        let ctx = ui.ctx().clone();
        if let ShapeKind::Image {
            png,
            px_width,
            px_height,
        } = &img.kind
        {
            let mode = if act == Action::Trace {
                TraceMode::All
            } else {
                TraceMode::Outer
            };
            let st = settings_for(&ps, &img, area, mode);
            match oc_trace::trace(&png.0, img.rect, &st, app.defaults.stroke) {
                Ok(mut shapes) => {
                    for s in &mut shapes {
                        s.rotation = img.rotation;
                    }
                    let count = shapes.len();
                    app.commit();
                    if act == Action::Detach {
                        match oc_trace::mask_to_shapes(&png.0, img.rect, &shapes) {
                            Ok(masked) => {
                                if let Some(s) = app.doc.shape_mut(img.id) {
                                    s.kind = ShapeKind::Image {
                                        png: ImageData(std::sync::Arc::new(masked)),
                                        px_width: *px_width,
                                        px_height: *px_height,
                                    };
                                }
                                app.canvas.textures.remove(&img.id);
                            }
                            Err(e) => app.notify(&ctx, e.to_string()),
                        }
                    }
                    let ids: Vec<u64> = shapes.into_iter().map(|s| app.doc.add_shape(s)).collect();
                    app.doc.selected = ids;
                    app.canvas.trace_area = None;
                    app.notify(
                        &ctx,
                        format!(
                            "Trazadas {count} forma{}.",
                            if count == 1 { "" } else { "s" }
                        ),
                    );
                }
                Err(e) => app.notify(&ctx, e.to_string()),
            }
        }
    }
    ui.data_mut(|d| d.insert_temp(id, ps));
}
