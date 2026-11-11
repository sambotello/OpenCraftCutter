//! Panel Replicar: pestañas Replicar, Replicar a medida y Objeto en ruta, con las opciones,
//! el orden y el comportamiento de la referencia.

use super::section;
use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{Color32, RichText, Sense, Stroke as EStroke, StrokeKind, Ui, Vec2 as EVec2};
use egui_material_icons::{MaterialIcon, icons as mi};
use kurbo::{Point, Rect, Vec2};
use oc_core::edit::{mirror_shape, selection_bounds, shape_bounds};
use oc_core::replicate::{self, PathOptions, PathSpacing, Side};
use oc_core::shape::Shape;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Quick,
    Custom,
    Path,
}

/// Dónde se sitúa cada copia en «Replicar a medida».
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Place {
    #[default]
    Same,
    Left,
    Right,
    Up,
    Down,
    Custom,
}

#[derive(Clone, Copy)]
struct State {
    tab: Tab,
    copies: usize,
    place: Place,
    offset_x: f64,
    offset_y: f64,
    angle: f64,
    perpendicular: bool,
    start_angle: f64,
    angle_step: f64,
    start: f64,
    section: f64,
    by_count: bool,
    count: usize,
    step: f64,
}

impl Default for State {
    fn default() -> Self {
        State {
            tab: Tab::Quick,
            copies: 1,
            place: Place::Same,
            offset_x: 0.0,
            offset_y: 0.0,
            angle: 0.0,
            perpendicular: false,
            start_angle: 0.0,
            angle_step: 0.0,
            start: 0.0,
            section: 0.0,
            by_count: true,
            count: 50,
            step: 50.0,
        }
    }
}

const LABEL_W: f32 = 112.0;
const GLYPH: EVec2 = EVec2::new(40.0, 40.0);

/// Añade copias nuevas (una entrada de historial) y las deja seleccionadas.
fn add_copies(app: &mut OpenCraftCutterApp, copies: Vec<Shape>) {
    if copies.is_empty() {
        return;
    }
    app.commit();
    let ids: Vec<u64> = copies.into_iter().map(|c| app.doc.add_shape(c)).collect();
    app.doc.selected = ids;
}

fn label_cell(ui: &mut Ui, text: &str) {
    ui.allocate_ui_with_layout(
        EVec2::new(LABEL_W, GLYPH.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(LABEL_W);
            ui.label(RichText::new(text).color(theme::CHROME_200));
        },
    );
}

/// Botón cuadrado con icono y una etiqueta pequeña debajo.
fn glyph_button(ui: &mut Ui, icon: MaterialIcon, label: &str, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(GLYPH, Sense::click());
    let resp = resp.on_hover_text(tip);
    let enabled = ui.is_enabled();
    let color = if !enabled {
        theme::CHROME_500
    } else if resp.hovered() {
        Color32::WHITE
    } else {
        theme::CHROME_200
    };
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, theme::CHROME_700);
    }
    let icon_y = if label.is_empty() { 0.0 } else { 6.0 };
    ui.painter().text(
        rect.center() - EVec2::new(0.0, icon_y),
        egui::Align2::CENTER_CENTER,
        <&str>::from(icon),
        egui::FontId::proportional(19.0),
        color,
    );
    if !label.is_empty() {
        ui.painter().text(
            rect.center_bottom() - EVec2::new(0.0, 2.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(10.5),
            color,
        );
    }
    enabled && resp.clicked()
}

/// Fila «etiqueta + botones a la derecha»; devuelve el índice del botón pulsado.
fn glyph_row(ui: &mut Ui, label: &str, items: &[(MaterialIcon, &str, &str)]) -> Option<usize> {
    let mut hit = None;
    ui.horizontal(|ui| {
        label_cell(ui, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (i, (icon, text, tip)) in items.iter().enumerate().rev() {
                if glyph_button(ui, *icon, text, tip) {
                    hit = Some(i);
                }
            }
        });
    });
    hit
}

fn slider_field(
    ui: &mut Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    decimals: usize,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 100.0).max(60.0);
        changed |= ui
            .add(egui::Slider::new(value, range.clone()).show_value(false))
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(value)
                    .range(range)
                    .fixed_decimals(decimals)
                    .suffix(suffix),
            )
            .changed();
    });
    changed
}

fn wide_button(ui: &mut Ui, text: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(text).min_size(EVec2::new(ui.available_width(), 30.0)),
    )
    .clicked()
}

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(theme::CHROME_400));
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let id = egui::Id::unique("replicate_panel_state");
    let mut st: State = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let shapes: Vec<Shape> = app.doc.selected_shapes().into_iter().cloned().collect();

    ui.horizontal(|ui| {
        for (tab, icon, tip) in [
            (Tab::Quick, mi::ICON_AUTO_AWESOME_MOTION, "Replicar"),
            (Tab::Custom, mi::ICON_TUNE, "Replicar a medida"),
            (Tab::Path, mi::ICON_ROUTE, "Objeto en ruta"),
        ] {
            if ui
                .selectable_label(st.tab == tab, icon.rich_text().size(18.0))
                .on_hover_text(tip)
                .clicked()
            {
                st.tab = tab;
            }
        }
    });
    ui.separator();
    if shapes.is_empty() {
        hint(ui, "Selecciona uno o más objetos.");
    }
    ui.add_enabled_ui(!shapes.is_empty(), |ui| match st.tab {
        Tab::Quick => quick_tab(app, ui, &shapes),
        Tab::Custom => custom_tab(app, ui, &shapes, &mut st),
        Tab::Path => path_tab(app, ui, &shapes, &mut st),
    });
    ui.data_mut(|d| d.insert_temp(id, st));
}

fn quick_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui, shapes: &[Shape]) {
    let Some(b) = selection_bounds(shapes) else {
        return;
    };
    ui.spacing_mut().item_spacing.y = 2.0;
    if let Some(i) = glyph_row(
        ui,
        "Duplicar",
        &[
            (mi::ICON_ARROW_BACK, "", "Duplicar a la izquierda"),
            (mi::ICON_ARROW_FORWARD, "", "Duplicar a la derecha"),
            (mi::ICON_ARROW_DOWNWARD, "", "Duplicar abajo"),
            (mi::ICON_ARROW_UPWARD, "", "Duplicar arriba"),
        ],
    ) {
        let side = [Side::Left, Side::Right, Side::Down, Side::Up][i];
        add_copies(app, replicate::adjacent_copies(shapes, side, 1));
    }
    if let Some(i) = glyph_row(
        ui,
        "Filas y columnas",
        &[
            (mi::ICON_VIEW_COLUMN, "3", "Fila de 3"),
            (mi::ICON_VIEW_COLUMN, "4", "Fila de 4"),
            (mi::ICON_TABLE_ROWS, "3", "Columna de 3"),
            (mi::ICON_TABLE_ROWS, "4", "Columna de 4"),
        ],
    ) {
        let (side, n) = [
            (Side::Right, 2),
            (Side::Right, 3),
            (Side::Down, 2),
            (Side::Down, 3),
        ][i];
        add_copies(app, replicate::adjacent_copies(shapes, side, n));
    }
    if glyph_row(
        ui,
        "Llenar página",
        &[(
            mi::ICON_APPS,
            "",
            "Llenar la página con el mayor número de copias",
        )],
    )
    .is_some()
    {
        let page = &app.doc.page;
        let m = replicate::FILL_MARGIN_MM;
        let area = Rect::new(m, m, page.width_mm - m, page.height_mm - m);
        let (first, copies) = replicate::fill_area(shapes, area, replicate::FILL_MIN_GAP_MM);
        app.commit();
        for s in first {
            if let Some(t) = app.doc.shape_mut(s.id) {
                *t = s;
            }
        }
        for c in copies {
            app.doc.add_shape(c);
        }
    }
    ui.add_space(4.0);
    ui.separator();
    if let Some(i) = glyph_row(
        ui,
        "Reflejar",
        &[
            (
                mi::ICON_FLIP,
                <&str>::from(mi::ICON_ARROW_DOWNWARD),
                "Reflejar abajo",
            ),
            (
                mi::ICON_FLIP,
                <&str>::from(mi::ICON_ARROW_UPWARD),
                "Reflejar arriba",
            ),
            (
                mi::ICON_FLIP,
                <&str>::from(mi::ICON_ARROW_BACK),
                "Reflejar a la izquierda",
            ),
            (
                mi::ICON_FLIP,
                <&str>::from(mi::ICON_ARROW_FORWARD),
                "Reflejar a la derecha",
            ),
        ],
    ) {
        // Eje en el borde de la caja: la copia queda pegada.
        let (horizontal, axis) = [(false, b.y1), (false, b.y0), (true, b.x0), (true, b.x1)][i];
        let copies: Vec<Shape> = crate::text::paths_for_ops(shapes)
            .iter()
            .map(|s| {
                let mut m = mirror_shape(s, horizontal, axis);
                m.id = 0;
                m
            })
            .collect();
        add_copies(app, copies);
    }
    if let Some(i) = glyph_row(
        ui,
        "Rotar copias",
        &[
            (mi::ICON_FILTER_VINTAGE, "2", "2 copias giradas"),
            (mi::ICON_FILTER_VINTAGE, "3", "3 copias giradas"),
            (mi::ICON_FILTER_VINTAGE, "4", "4 copias giradas"),
            (mi::ICON_FILTER_VINTAGE, "6", "6 copias giradas"),
        ],
    ) {
        add_copies(app, replicate::rotate_copies(shapes, [2, 3, 4, 6][i]));
    }
}

fn custom_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui, shapes: &[Shape], st: &mut State) {
    let unit = app.doc.page.unit;
    let suffix = format!(" {}", unit.label());
    section(ui, "Número de copias");
    ui.label(RichText::new("Número").color(theme::CHROME_200));
    let mut n = st.copies as f64;
    if slider_field(ui, &mut n, 1.0..=100.0, "", 0) {
        st.copies = n.round().max(1.0) as usize;
    }
    ui.label(RichText::new("Situar cada copia").color(theme::CHROME_200));
    ui.horizontal(|ui| {
        let w = ((ui.available_width() - 5.0 * ui.spacing().item_spacing.x) / 6.0).max(30.0);
        for (place, icon, tip) in [
            (Place::Same, mi::ICON_CROP_SQUARE, "En el mismo sitio"),
            (Place::Left, mi::ICON_ARROW_BACK, "A la izquierda"),
            (Place::Right, mi::ICON_ARROW_FORWARD, "A la derecha"),
            (Place::Up, mi::ICON_ARROW_UPWARD, "Arriba"),
            (Place::Down, mi::ICON_ARROW_DOWNWARD, "Abajo"),
            (
                Place::Custom,
                mi::ICON_TURN_RIGHT,
                "Desplazamiento a medida",
            ),
        ] {
            if ui
                .add(
                    egui::Button::selectable(st.place == place, icon.rich_text().size(18.0))
                        .frame(true)
                        .min_size(EVec2::new(w, 30.0)),
                )
                .on_hover_text(tip)
                .clicked()
            {
                st.place = place;
            }
        }
    });
    if st.place == Place::Custom {
        let (pw, ph) = (app.doc.page.width_mm, app.doc.page.height_mm);
        for (label, v, max) in [
            ("Desplazamiento X", &mut st.offset_x, pw),
            ("Desplazamiento Y", &mut st.offset_y, ph),
        ] {
            ui.label(RichText::new(label).color(theme::CHROME_200));
            let mut shown = unit.from_mm(*v);
            if slider_field(
                ui,
                &mut shown,
                unit.from_mm(-max)..=unit.from_mm(max),
                &suffix,
                unit.decimals(),
            ) {
                *v = unit.to_mm(shown);
            }
        }
    }
    ui.label(RichText::new("Giro de cada copia").color(theme::CHROME_200));
    ui.horizontal(|ui| {
        angle_preview(ui, st.angle);
        ui.vertical(|ui| {
            ui.add_space(24.0);
            ui.add(
                egui::DragValue::new(&mut st.angle)
                    .range(-360.0..=360.0)
                    .speed(1.0)
                    .fixed_decimals(1)
                    .suffix(" °"),
            );
        });
    });
    if wide_button(ui, "Aplicar", true) {
        if let Some(b) = selection_bounds(shapes) {
            let step = match st.place {
                Place::Same => Vec2::ZERO,
                Place::Left => Side::Left.step(b),
                Place::Right => Side::Right.step(b),
                Place::Up => Side::Up.step(b),
                Place::Down => Side::Down.step(b),
                Place::Custom => Vec2::new(st.offset_x, st.offset_y),
            };
            add_copies(
                app,
                replicate::offset_copies(shapes, step, st.copies, st.angle),
            );
        }
    }
}

/// Cuadro con una línea roja al ángulo indicado (como la vista previa de la referencia).
fn angle_preview(ui: &mut Ui, angle: f64) {
    let (rect, _) = ui.allocate_exact_size(EVec2::splat(84.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, Color32::WHITE);
    p.rect_stroke(
        rect,
        3.0,
        EStroke::new(1.0, theme::CHROME_500),
        StrokeKind::Inside,
    );
    let c = rect.center();
    let a = angle.to_radians() as f32;
    let d = EVec2::new(a.cos(), a.sin()) * (rect.width() * 0.48);
    p.line_segment([c - d, c + d], EStroke::new(2.0, theme::CUT_AREA));
    let tick = |pos: egui::Pos2, v: EVec2| {
        p.line_segment([pos, pos + v], EStroke::new(1.0, Color32::BLACK))
    };
    tick(rect.center_top(), EVec2::new(0.0, 5.0));
    tick(rect.center_bottom(), EVec2::new(0.0, -5.0));
}

fn path_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui, shapes: &[Shape], st: &mut State) {
    let unit = app.doc.page.unit;
    let suffix = format!(" {}", unit.label());
    section(ui, "Objeto en ruta");
    // La figura guía es la de mayor tamaño; el resto son los objetos a repetir.
    let guide_idx = shapes
        .iter()
        .enumerate()
        .max_by(|a, b| {
            let area = |s: &Shape| {
                let r = shape_bounds(s);
                r.width() * r.height()
            };
            area(a.1).total_cmp(&area(b.1))
        })
        .map(|(i, _)| i);
    let ready = shapes.len() >= 2;
    let (guide, objects): (Option<&Shape>, Vec<Shape>) = match guide_idx {
        Some(g) if ready => (
            Some(&shapes[g]),
            shapes
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != g)
                .map(|(_, s)| s.clone())
                .collect(),
        ),
        _ => (None, Vec::new()),
    };
    let guide_line: Option<(Vec<Point>, bool)> = guide.and_then(|g| {
        let to_pts =
            |v: &Vec<[f64; 2]>| v.iter().map(|p| Point::new(p[0], p[1])).collect::<Vec<_>>();
        oc_core::ops::closed_contours(g)
            .first()
            .map(|c| (to_pts(c), true))
            .or_else(|| {
                oc_core::ops::open_paths(g)
                    .first()
                    .map(|c| (to_pts(c), false))
            })
    });
    let total = guide_line
        .as_ref()
        .map(|(p, c)| replicate::polyline_length(p, *c))
        .unwrap_or(0.0);
    hint(
        ui,
        if guide_line.is_some() {
            "La figura más grande de la selección es la guía; las demás se repiten sobre ella."
        } else {
            "Selecciona el objeto a repetir y la figura guía (la más grande)."
        },
    );
    ui.add_space(4.0);
    ui.separator();
    section(ui, "Ángulo");
    ui.checkbox(&mut st.perpendicular, "Perpendicular");
    ui.label(RichText::new("Ángulo de inicio").color(theme::CHROME_200));
    slider_field(ui, &mut st.start_angle, -180.0..=180.0, " °", 1);
    ui.label(RichText::new("Incrementar ángulo").color(theme::CHROME_200));
    slider_field(ui, &mut st.angle_step, -180.0..=180.0, " °", 1);
    ui.add_space(4.0);
    ui.separator();
    section(ui, "Posicionar");
    let max = unit.from_mm(total.max(1.0));
    for (label, v) in [
        ("Posición de inicio", &mut st.start),
        ("Longitud de sección (0 = toda)", &mut st.section),
    ] {
        ui.label(RichText::new(label).color(theme::CHROME_200));
        let mut shown = unit.from_mm(*v).min(max);
        if slider_field(ui, &mut shown, 0.0..=max, &suffix, unit.decimals()) {
            *v = unit.to_mm(shown);
        }
    }
    ui.horizontal(|ui| {
        ui.radio_value(&mut st.by_count, true, "Número de repeticiones");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled(
                st.by_count,
                egui::DragValue::new(&mut st.count).range(1..=1000),
            );
        });
    });
    ui.horizontal(|ui| {
        ui.radio_value(&mut st.by_count, false, "Longitud del paso");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut s = unit.from_mm(st.step);
            if ui
                .add_enabled(
                    !st.by_count,
                    egui::DragValue::new(&mut s)
                        .range(unit.from_mm(0.5)..=unit.from_mm(1000.0))
                        .speed(unit.step())
                        .fixed_decimals(unit.decimals())
                        .suffix(suffix.clone()),
                )
                .changed()
            {
                st.step = unit.to_mm(s);
            }
        });
    });
    let opts = PathOptions {
        perpendicular: st.perpendicular,
        start_angle: st.start_angle,
        angle_step: st.angle_step,
        start: st.start,
        section: st.section,
        spacing: if st.by_count {
            PathSpacing::Count(st.count)
        } else {
            PathSpacing::Step(st.step)
        },
    };
    if wide_button(ui, "Publicar copias", guide_line.is_some()) {
        if let Some((pts, closed)) = &guide_line {
            add_copies(app, replicate::along_path(&objects, pts, *closed, &opts));
        }
    }
}
