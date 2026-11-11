//! Panel Desbarbado: caja alrededor del diseño y líneas por los huecos entre piezas, para
//! pelar el vinil sobrante a trozos.

use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{RichText, Ui, Vec2 as EVec2};
use egui_material_icons::icons as mi;
use oc_core::shape::{FillRule, Shape, ShapeKind};

pub const BOX_NAME: &str = "Caja de desbarbado";
pub const LINES_NAME: &str = "Líneas de desbarbado";
pub const MARKS_NAME: &str = "Marcas de alineación";

#[derive(Clone, Copy, PartialEq)]
struct Settings {
    margin_mm: f64,
    lines: bool,
    min_gap_mm: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            margin_mm: 5.0,
            lines: true,
            min_gap_mm: 2.0,
        }
    }
}

fn is_weeding(s: &Shape) -> bool {
    s.name == BOX_NAME || s.name == LINES_NAME || s.name == MARKS_NAME
}

impl OpenCraftCutterApp {
    /// Formas a las que se añade el desbarbado: la selección o, sin selección, todo el diseño.
    fn weeding_targets(&self) -> Vec<Shape> {
        let test_ids = oc_core::send::test_shape_ids(&self.doc);
        let usable =
            |s: &&Shape| s.visible && !s.is_image() && !is_weeding(s) && !test_ids.contains(&s.id);
        let sel: Vec<Shape> = self
            .doc
            .selected_shapes()
            .into_iter()
            .filter(usable)
            .cloned()
            .collect();
        if !sel.is_empty() {
            return sel;
        }
        self.doc.shapes.iter().filter(usable).cloned().collect()
    }

    /// Añade la caja (y las líneas) de desbarbado alrededor de `weeding_targets`.
    fn add_weeding(&mut self, margin: f64, lines: bool, min_gap: f64) -> bool {
        let targets = self.weeding_targets();
        let color = targets.first().map(|s| s.stroke.color);
        // Cada letra o isla por separado, para que las líneas pasen entre ellas.
        let pieces: Vec<kurbo::Rect> = crate::text::paths_for_ops(&targets)
            .iter()
            .flat_map(oc_core::ops::break_apart)
            .map(|s| oc_core::edit::shape_bounds(&s))
            .collect();
        let Some(w) = oc_core::weeding::weeding(&pieces, margin, lines, min_gap) else {
            return false;
        };
        self.commit();
        let mut new_ids = Vec::new();
        let mut frame = Shape::new(0, BOX_NAME, w.frame, ShapeKind::Rect { corner_radius: 0.0 });
        frame.fill = None;
        if let Some(c) = color {
            frame.stroke.color = c;
        }
        new_ids.push(self.doc.add_shape(frame));
        if !w.lines.is_empty() {
            let polys: Vec<(Vec<kurbo::Point>, bool)> =
                w.lines.iter().map(|l| (l.to_vec(), false)).collect();
            let (rect, subpaths) = Shape::normalize_subpaths(&polys);
            let mut s = Shape::new(
                0,
                LINES_NAME,
                rect,
                ShapeKind::Path {
                    subpaths,
                    fill_rule: FillRule::NonZero,
                },
            );
            s.fill = None;
            if let Some(c) = color {
                s.stroke.color = c;
            }
            new_ids.push(self.doc.add_shape(s));
        }
        self.doc.selected = new_ids;
        true
    }

    /// Marcas de alineación para capas: las mismas dos cruces en cada color del diseño (línea
    /// y relleno de ese color), para superponer las capas al aplicarlas. Sustituyen a las que
    /// hubiera.
    fn add_alignment_marks(&mut self, gap: f64) -> bool {
        let targets = self.weeding_targets();
        let bounds = oc_core::edit::selection_bounds(crate::text::paths_for_ops(&targets).iter());
        let Some(design) = bounds else {
            return false;
        };
        // Capas: por color de línea, o por relleno si el diseño distingue más colores así (vinil
        // por capas suele ir con el mismo trazo y rellenos distintos). Cada capa: (línea, relleno).
        let mut by_stroke: Vec<(oc_core::Color, oc_core::Color)> = Vec::new();
        let mut by_fill: Vec<(oc_core::Color, oc_core::Color)> = Vec::new();
        for s in &targets {
            if !by_stroke.iter().any(|(c, _)| *c == s.stroke.color) {
                by_stroke.push((s.stroke.color, s.stroke.color));
            }
            if let Some(f) = s.fill
                && !by_fill.iter().any(|(_, c)| *c == f)
            {
                by_fill.push((s.stroke.color, f));
            }
        }
        let layers = if by_fill.len() > by_stroke.len() {
            by_fill
        } else {
            by_stroke
        };
        let marks = oc_core::weeding::alignment_marks(design, gap, 6.0);
        let polys: Vec<(Vec<kurbo::Point>, bool)> = marks.into_iter().map(|m| (m, true)).collect();
        self.commit();
        let old: Vec<u64> = self
            .doc
            .shapes
            .iter()
            .filter(|s| s.name == MARKS_NAME)
            .map(|s| s.id)
            .collect();
        self.doc.remove_shapes(&old);
        let mut ids = Vec::new();
        for (stroke, fill) in layers {
            let (rect, subpaths) = Shape::normalize_subpaths(&polys);
            let mut s = Shape::new(
                0,
                MARKS_NAME,
                rect,
                ShapeKind::Path {
                    subpaths,
                    fill_rule: FillRule::NonZero,
                },
            );
            s.stroke.color = stroke;
            s.fill = Some(fill);
            ids.push(self.doc.add_shape(s));
        }
        self.doc.selected = ids;
        true
    }

    fn remove_weeding(&mut self) {
        let ids: Vec<u64> = self
            .doc
            .shapes
            .iter()
            .filter(|s| is_weeding(s))
            .map(|s| s.id)
            .collect();
        if !ids.is_empty() {
            self.commit();
            self.doc.remove_shapes(&ids);
        }
    }
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let id = egui::Id::unique("weeding_settings");
    let mut st: Settings = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let unit = app.doc.page.unit;
    ui.label(
        RichText::new(
            "Una caja alrededor del diseño y líneas por los huecos entre piezas: el vinil sobrante \
             se pela a trozos sin arrastrar las letras pequeñas. Sin selección se usa todo el diseño.",
        )
        .small()
        .color(theme::CHROME_300),
    );
    ui.separator();
    let length = |ui: &mut Ui, label: &str, v: &mut f64, max: f64| {
        ui.label(RichText::new(label).color(theme::CHROME_200));
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 110.0).max(60.0);
            let mut d = unit.from_mm(*v);
            let a = ui
                .add(egui::Slider::new(&mut d, 0.0..=unit.from_mm(max)).show_value(false))
                .changed();
            let b = ui
                .add(
                    egui::DragValue::new(&mut d)
                        .speed(unit.step() / 2.0)
                        .range(0.0..=unit.from_mm(max * 4.0))
                        .fixed_decimals(unit.decimals() + 1)
                        .suffix(format!(" {}", unit.label())),
                )
                .changed();
            if a || b {
                *v = unit.to_mm(d);
            }
        });
    };
    length(ui, "Margen de la caja", &mut st.margin_mm, 20.0);
    ui.checkbox(&mut st.lines, "Líneas de desbarbado");
    ui.add_enabled_ui(st.lines, |ui| {
        length(ui, "Hueco mínimo entre piezas", &mut st.min_gap_mm, 10.0);
    });
    ui.add_space(6.0);
    let can = !app.weeding_targets().is_empty();
    if ui
        .add_enabled(
            can,
            egui::Button::new(format!(
                "{}  Añadir desbarbado",
                <&str>::from(mi::ICON_CROP_SQUARE)
            ))
            .min_size(EVec2::new(ui.available_width(), 32.0)),
        )
        .on_disabled_hover_text("No hay formas a las que añadirlo")
        .clicked()
    {
        app.add_weeding(st.margin_mm, st.lines, st.min_gap_mm);
    }
    ui.add_space(4.0);
    ui.separator();
    ui.label(RichText::new("Capas de varios colores").color(theme::CHROME_100));
    ui.label(
        RichText::new(
            "Añade las mismas dos cruces en cada color: al aplicar el vinil, haz coincidir las \
             cruces de una capa con las de la otra.",
        )
        .small()
        .color(theme::CHROME_300),
    );
    if ui
        .add_enabled(
            can,
            egui::Button::new(format!(
                "{}  Añadir marcas de alineación",
                <&str>::from(mi::ICON_ADD)
            ))
            .min_size(EVec2::new(ui.available_width(), 30.0)),
        )
        .clicked()
    {
        app.add_alignment_marks(st.margin_mm);
    }
    ui.add_space(4.0);
    let has = app.doc.shapes.iter().any(is_weeding);
    if ui
        .add_enabled(
            has,
            egui::Button::new("Quitar desbarbado y marcas")
                .min_size(EVec2::new(ui.available_width(), 28.0)),
        )
        .clicked()
    {
        app.remove_weeding();
    }
    ui.data_mut(|d| d.insert_temp(id, st));
}
