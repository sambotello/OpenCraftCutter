//! Panel Desplazamiento, con las opciones y el orden de la referencia: desplazamiento exterior
//! o interior, distancia, vértice (esquina o redondeado), límite de esquina, y Cancelar /
//! Aplicar con vista previa en el lienzo.

use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{RichText, Ui, Vec2 as EVec2};
use egui_material_icons::icons as mi;
use oc_core::ops::{self, Join};
use oc_core::shape::Shape;

#[derive(Clone, Copy, PartialEq)]
struct Settings {
    /// `None` = aún no se ha elegido tipo (sin vista previa).
    inner: Option<bool>,
    distance_mm: f64,
    round: bool,
    corner_limit: f64,
}

impl Default for Settings {
    fn default() -> Self {
        // 0,150 cm y vértice redondeado, como la referencia.
        Settings {
            inner: None,
            distance_mm: 1.5,
            round: true,
            corner_limit: 4.0,
        }
    }
}

fn slider(ui: &mut Ui, label: &str, enabled: bool, add: impl FnOnce(&mut Ui)) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.label(RichText::new(label).color(theme::CHROME_200));
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 110.0).max(60.0);
            add(ui);
        });
    });
}

fn kind_button(
    ui: &mut Ui,
    selected: bool,
    icon: egui_material_icons::MaterialIcon,
    label: &str,
) -> bool {
    let w = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
    ui.add(
        egui::Button::selectable(
            selected,
            RichText::new(format!("{}  {label}", <&str>::from(icon))).size(13.0),
        )
        .frame(true)
        .min_size(EVec2::new(w, 40.0)),
    )
    .clicked()
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let id = egui::Id::unique("offset_settings");
    let mut st: Settings = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let unit = app.doc.page.unit;
    let raw: Vec<Shape> = app
        .doc
        .selected_shapes()
        .into_iter()
        .filter(|s| !s.is_image())
        .cloned()
        .collect();
    let mut selected: Vec<Shape> = crate::text::paths_for_ops(&raw);
    // Imágenes: como la referencia, se desplaza la silueta de su parte no transparente.
    for img in app
        .doc
        .selected_shapes()
        .into_iter()
        .filter(|s| s.is_image())
    {
        if let Some(sil) = image_silhouette(app, img) {
            selected.push(sil);
        }
    }
    if selected.is_empty() {
        ui.label(
            RichText::new("Selecciona una o más formas.")
                .small()
                .color(theme::CHROME_300),
        );
        st.inner = None;
    }
    let before = st;

    ui.add_enabled_ui(!selected.is_empty(), |ui| {
        ui.horizontal(|ui| {
            if kind_button(ui, st.inner == Some(false), mi::ICON_STAR, "Desplazamiento") {
                st.inner = Some(false);
            }
            if kind_button(ui, st.inner == Some(true), mi::ICON_STAR_HALF, "Interno") {
                st.inner = Some(true);
            }
        });
        ui.separator();
        let active = st.inner.is_some();
        slider(ui, "Distancia", active, |ui| {
            let mut d = unit.from_mm(st.distance_mm);
            let max = unit.from_mm(50.0);
            let a = ui.add(egui::Slider::new(&mut d, 0.0..=max).show_value(false)).changed();
            let b = ui
                .add(
                    egui::DragValue::new(&mut d)
                        .speed(unit.step() / 2.0)
                        .range(0.0..=unit.from_mm(200.0))
                        .fixed_decimals(unit.decimals() + 1)
                        .suffix(format!(" {}", unit.label())),
                )
                .changed();
            if a || b {
                st.distance_mm = unit.to_mm(d);
            }
        });
        ui.add_enabled_ui(active, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Vértice").color(theme::CHROME_200));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::selectable(st.round, mi::ICON_ROUNDED_CORNER.rich_text().size(20.0)).frame(true).min_size(EVec2::splat(34.0)))
                        .on_hover_text("Redondeado")
                        .clicked()
                    {
                        st.round = true;
                    }
                    if ui
                        .add(egui::Button::selectable(!st.round, mi::ICON_CHECK_BOX_OUTLINE_BLANK.rich_text().size(20.0)).frame(true).min_size(EVec2::splat(34.0)))
                        .on_hover_text("Esquina")
                        .clicked()
                    {
                        st.round = false;
                    }
                });
            });
        });
        slider(ui, "Límite de esquina", active && !st.round, |ui| {
            ui.add(egui::Slider::new(&mut st.corner_limit, 1.0..=10.0).show_value(false));
            ui.add(egui::DragValue::new(&mut st.corner_limit).range(1.0..=10.0).speed(0.05).fixed_decimals(3));
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let w = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            if ui.add_enabled(active, egui::Button::new("Cancelar").min_size(EVec2::new(w, 30.0))).clicked() {
                st.inner = None;
            }
            if ui.add_enabled(active, egui::Button::new("Aplicar").min_size(EVec2::new(w, 30.0))).clicked() {
                let out = std::mem::take(&mut app.canvas.offset_preview);
                if out.is_empty() {
                    let ctx = ui.ctx().clone();
                    app.notify(&ctx, "El desplazamiento no deja ninguna forma (quizá la distancia interior es mayor que la forma).");
                } else {
                    app.commit();
                    let ids: Vec<u64> = out.into_iter().map(|s| app.doc.add_shape(s)).collect();
                    app.doc.selected = ids;
                }
                st.inner = None;
            }
        });
    });

    // Recalcular la vista previa cuando cambian los ajustes o la selección.
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for s in &selected {
            s.id.hash(&mut h);
            for v in [s.rect.x0, s.rect.y0, s.rect.x1, s.rect.y1, s.rotation] {
                v.to_bits().hash(&mut h);
            }
        }
        h.finish()
    };
    let key_id = egui::Id::unique("offset_preview_key");
    let last: Option<(u64, Settings)> = ui.data(|d| d.get_temp(key_id));
    if st.inner.is_none() {
        app.canvas.offset_preview.clear();
    } else if last != Some((key, st)) || st != before {
        let signed = if st.inner == Some(true) {
            -st.distance_mm
        } else {
            st.distance_mm
        };
        let join = if st.round { Join::Round } else { Join::Miter };
        app.canvas.offset_preview = if signed == 0.0 {
            Vec::new()
        } else {
            selected
                .iter()
                .filter_map(|s| ops::offset_with_limit(s, signed, join, st.corner_limit))
                .collect()
        };
    }
    ui.data_mut(|d| {
        d.insert_temp(key_id, (key, st));
        d.insert_temp(id, st);
    });
}

/// Clave de la caché de siluetas: imagen y caja (redondeada).
type SilKey = (u64, [i64; 5]);

/// Silueta (mm, mundo, ya girada) de una imagen, cacheada por imagen y caja.
pub(super) fn image_silhouette(app: &OpenCraftCutterApp, img: &Shape) -> Option<Shape> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static CACHE: RefCell<HashMap<SilKey, Option<Shape>>> = RefCell::new(HashMap::new());
    }
    let oc_core::ShapeKind::Image { png, .. } = &img.kind else {
        return None;
    };
    let r = img.rect;
    let key = (
        img.id ^ (png.0.len() as u64).rotate_left(17),
        [r.x0, r.y0, r.x1, r.y1, img.rotation].map(|v| (v * 1000.0).round() as i64),
    );
    if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let out = oc_trace::image_silhouette(&png.0, r).ok().map(|loops| {
        let rot = oc_core::edit::rotation_about(r.center(), img.rotation);
        let world: Vec<(Vec<kurbo::Point>, bool)> = loops
            .into_iter()
            .map(|l| (l.into_iter().map(|p| rot * p).collect(), true))
            .collect();
        let (rect, subpaths) = Shape::normalize_subpaths(&world);
        let mut s = Shape::new(
            0,
            format!("{} (contorno)", img.name),
            rect,
            oc_core::ShapeKind::Path {
                subpaths,
                fill_rule: oc_core::FillRule::EvenOdd,
            },
        );
        s.stroke = oc_core::shape::Stroke::default();
        s
    });
    let _ = app;
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 64 {
            c.clear();
        }
        c.insert(key, out.clone());
    });
    out
}
