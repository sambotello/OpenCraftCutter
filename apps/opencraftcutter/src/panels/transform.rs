//! Panel Transformar: pestañas Alinear, Escalar, Rotar y Mover, con las opciones, el orden y
//! el comportamiento de la referencia (todo actúa sobre los objetos seleccionados).

use super::section;
use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{
    Color32, Rect as ERect, RichText, Sense, Stroke as EStroke, StrokeKind, Ui, Vec2 as EVec2,
};
use egui_material_icons::{MaterialIcon, icons as mi};
use kurbo::{Point, Rect, Vec2};
use oc_core::arrange::{self, Align, Anchor, Axis};
use oc_core::edit::selection_bounds;
use oc_core::shape::Shape;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Align,
    Scale,
    Rotate,
    Move,
}

/// Estado del panel (persistente mientras la app está abierta).
#[derive(Clone, Copy)]
struct State {
    tab: Tab,
    scale_pct: f64,
    lock: bool,
    width: f64,
    height: f64,
    dims_key: u64,
    rotate_by: f64,
    distance: f64,
    move_x: f64,
    move_y: f64,
    move_key: u64,
    anchor: Anchor,
}

impl Default for State {
    fn default() -> Self {
        State {
            tab: Tab::Align,
            scale_pct: 100.0,
            lock: true,
            width: 0.0,
            height: 0.0,
            dims_key: 0,
            rotate_by: 0.0,
            // 1/8 de pulgada, como en la referencia.
            distance: 3.175,
            move_x: 0.0,
            move_y: 0.0,
            move_key: 0,
            anchor: Anchor::TopLeft,
        }
    }
}

const LABEL_W: f32 = 92.0;

/// Transformación pendiente de aplicar a la selección.
type Action = Box<dyn FnOnce(&[Shape]) -> Vec<Shape>>;
/// Fila de tres botones de alineación: (icono, ayuda, modo).
type AlignRow = [(MaterialIcon, &'static str, Align); 3];

/// Clave que cambia cuando cambia la selección o su caja (para refrescar los campos).
fn selection_key(shapes: &[Shape], extra: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    extra.hash(&mut h);
    for s in shapes {
        s.id.hash(&mut h);
    }
    if let Some(b) = selection_bounds(shapes) {
        for v in [b.x0, b.y0, b.x1, b.y1] {
            ((v * 1000.0).round() as i64).hash(&mut h);
        }
    }
    h.finish()
}

/// Sustituye las formas seleccionadas por su versión transformada (una entrada de historial).
fn apply(app: &mut OpenCraftCutterApp, shapes: &[Shape], f: impl FnOnce(&[Shape]) -> Vec<Shape>) {
    if shapes.is_empty() {
        return;
    }
    app.commit();
    for r in f(shapes) {
        if let Some(s) = app.doc.shape_mut(r.id) {
            *s = r;
        }
    }
}

fn label_cell(ui: &mut Ui, text: &str) {
    ui.allocate_ui_with_layout(
        EVec2::new(LABEL_W, 30.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(LABEL_W);
            ui.label(RichText::new(text).color(theme::CHROME_200));
        },
    );
}

fn icon_button(ui: &mut Ui, icon: MaterialIcon, tip: &str, enabled: bool) -> bool {
    let r = ui
        .add_enabled(
            enabled,
            egui::Button::new(icon.rich_text().size(18.0)).min_size(EVec2::new(34.0, 30.0)),
        )
        .on_hover_text(tip)
        .on_disabled_hover_text(tip);
    crate::theme::name_widget(&r, tip);
    r.clicked()
}

/// Deslizador con su campo numérico al lado. Devuelve si cambió.
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

/// Ancho de cada uno de `n` elementos que llenan la fila.
fn row_width(ui: &Ui, n: usize) -> f32 {
    ((ui.available_width() - (n as f32 - 1.0) * ui.spacing().item_spacing.x) / n as f32).max(30.0)
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
    let id = egui::Id::unique("transform_panel_state");
    let mut st: State = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let shapes: Vec<Shape> = app
        .doc
        .selected_shapes()
        .into_iter()
        .filter(|s| !s.locked)
        .cloned()
        .collect();

    ui.horizontal(|ui| {
        for (tab, icon, tip) in [
            (Tab::Align, mi::ICON_ALIGN_HORIZONTAL_CENTER, "Alinear"),
            (Tab::Scale, mi::ICON_OPEN_IN_FULL, "Escalar"),
            (Tab::Rotate, mi::ICON_ROTATE_RIGHT, "Rotar"),
            (Tab::Move, mi::ICON_OPEN_WITH, "Mover"),
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
        Tab::Align => align_tab(app, ui, &shapes),
        Tab::Scale => scale_tab(app, ui, &shapes, &mut st),
        Tab::Rotate => rotate_tab(app, ui, &shapes, &mut st),
        Tab::Move => move_tab(app, ui, &shapes, &mut st),
    });
    ui.data_mut(|d| d.insert_temp(id, st));
}

fn align_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui, shapes: &[Shape]) {
    let page = &app.doc.page;
    let m = page.cut_margin_mm;
    let page_rect = Rect::new(0.0, 0.0, page.width_mm, page.height_mm);
    let cut_area = Rect::new(m, m, page.width_mm - m, page.height_mm - m);
    // Con un objeto: el área de corte de la página; con varios: la caja de la selección.
    let reference = if shapes.len() >= 2 {
        selection_bounds(shapes).unwrap_or(cut_area)
    } else {
        cut_area
    };
    let mut action: Option<Action> = None;
    let row = |ui: &mut Ui, label: &str, add: &mut dyn FnMut(&mut Ui)| {
        ui.horizontal(|ui| {
            label_cell(ui, label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                add(ui)
            });
        });
    };
    ui.spacing_mut().item_spacing.y = 6.0;
    row(ui, "Centro", &mut |ui| {
        // De derecha a izquierda: el último añadido queda a la izquierda.
        if icon_button(
            ui,
            mi::ICON_FILTER_CENTER_FOCUS,
            "Centrar los objetos entre sí",
            true,
        ) {
            action = Some(Box::new(move |s| {
                arrange::align(s, Align::Center, reference)
            }));
        }
        if icon_button(
            ui,
            mi::ICON_CENTER_FOCUS_STRONG,
            "Centrar en la página",
            true,
        ) {
            let c = page_rect.center();
            action = Some(Box::new(move |s| arrange::center_group_on(s, c)));
        }
    });
    let buttons: [(&str, AlignRow); 2] = [
        (
            "Horizontal",
            [
                (
                    mi::ICON_ALIGN_HORIZONTAL_RIGHT,
                    "Alinear a la derecha",
                    Align::Right,
                ),
                (
                    mi::ICON_ALIGN_HORIZONTAL_CENTER,
                    "Centrar horizontalmente",
                    Align::HCenter,
                ),
                (
                    mi::ICON_ALIGN_HORIZONTAL_LEFT,
                    "Alinear a la izquierda",
                    Align::Left,
                ),
            ],
        ),
        (
            "Vertical",
            [
                (
                    mi::ICON_ALIGN_VERTICAL_BOTTOM,
                    "Alinear abajo",
                    Align::Bottom,
                ),
                (
                    mi::ICON_ALIGN_VERTICAL_CENTER,
                    "Centrar verticalmente",
                    Align::VCenter,
                ),
                (mi::ICON_ALIGN_VERTICAL_TOP, "Alinear arriba", Align::Top),
            ],
        ),
    ];
    for (label, items) in buttons {
        row(ui, label, &mut |ui| {
            for (icon, tip, mode) in items {
                if icon_button(ui, icon, tip, true) {
                    action = Some(Box::new(move |s| arrange::align(s, mode, reference)));
                }
            }
        });
    }
    ui.add_space(4.0);
    let can_distribute = shapes.len() >= 3;
    row(ui, "Distribuir", &mut |ui| {
        if icon_button(
            ui,
            mi::ICON_VERTICAL_DISTRIBUTE,
            "Distribuir verticalmente (3 objetos o más)",
            can_distribute,
        ) {
            action = Some(Box::new(|s| arrange::distribute(s, Axis::Vertical)));
        }
        if icon_button(
            ui,
            mi::ICON_HORIZONTAL_DISTRIBUTE,
            "Distribuir horizontalmente (3 objetos o más)",
            can_distribute,
        ) {
            action = Some(Box::new(|s| arrange::distribute(s, Axis::Horizontal)));
        }
    });
    ui.add_space(4.0);
    hint(
        ui,
        if shapes.len() >= 2 {
            "Alinea los objetos seleccionados entre sí."
        } else {
            "Con un solo objeto se alinea con el área de corte de la página."
        },
    );
    if let Some(f) = action {
        apply(app, shapes, f);
    }
}

fn scale_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui, shapes: &[Shape], st: &mut State) {
    let unit = app.doc.page.unit;
    let bounds = selection_bounds(shapes);
    // Escalados rápidos.
    ui.horizontal(|ui| {
        let w = row_width(ui, 5);
        for pct in [33.0, 50.0, 100.0, 200.0, 300.0] {
            let enabled = pct != 100.0;
            let side = 8.0 + 14.0 * (pct as f32 / 300.0).sqrt();
            let (rect, resp) = ui.allocate_exact_size(EVec2::new(w, 46.0), Sense::click());
            let resp = resp.on_hover_text(format!("Escalar al {pct} %"));
            let hovered = enabled && resp.hovered();
            let color = if !enabled || !ui.is_enabled() {
                theme::CHROME_500
            } else if hovered {
                Color32::WHITE
            } else {
                theme::CHROME_200
            };
            if hovered {
                ui.painter().rect_filled(rect, 4.0, theme::CHROME_700);
            }
            let sq =
                ERect::from_center_size(rect.center() - EVec2::new(0.0, 7.0), EVec2::splat(side));
            ui.painter()
                .rect_stroke(sq, 1.0, EStroke::new(1.5, color), StrokeKind::Inside);
            ui.painter().text(
                rect.center_bottom() - EVec2::new(0.0, 4.0),
                egui::Align2::CENTER_BOTTOM,
                format!("{pct}%"),
                egui::FontId::proportional(11.0),
                color,
            );
            if enabled && resp.clicked() {
                if let Some(b) = bounds {
                    let k = pct / 100.0;
                    apply(app, shapes, |s| arrange::scale_about(s, k, k, b.center()));
                }
            }
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Escalar").color(theme::CHROME_200));
    slider_field(ui, &mut st.scale_pct, 1.0..=1000.0, " %", 1);
    if wide_button(ui, "Aplicar", st.scale_pct != 100.0) {
        if let Some(b) = bounds {
            let k = st.scale_pct / 100.0;
            apply(app, shapes, |s| arrange::scale_about(s, k, k, b.center()));
        }
    }

    ui.add_space(4.0);
    ui.separator();
    section(ui, "Especificar dimensiones");
    let key = selection_key(shapes, 0);
    if key != st.dims_key {
        st.dims_key = key;
        if let Some(b) = bounds {
            st.width = b.width();
            st.height = b.height();
        }
    }
    let (bw, bh) = bounds
        .map(|b| (b.width(), b.height()))
        .unwrap_or((1.0, 1.0));
    let max = unit.from_mm(app.doc.page.width_mm.max(app.doc.page.height_mm) * 2.0);
    let suffix = format!(" {}", unit.label());
    let mut changed_w = false;
    let mut changed_h = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 34.0);
            ui.label(RichText::new("Anchura").color(theme::CHROME_200));
            let mut w = unit.from_mm(st.width);
            if slider_field(ui, &mut w, 0.01..=max, &suffix, unit.decimals()) {
                st.width = unit.to_mm(w).max(0.01);
                changed_w = true;
            }
            ui.label(RichText::new("Altura").color(theme::CHROME_200));
            let mut h = unit.from_mm(st.height);
            if slider_field(ui, &mut h, 0.01..=max, &suffix, unit.decimals()) {
                st.height = unit.to_mm(h).max(0.01);
                changed_h = true;
            }
        });
        let icon = if st.lock {
            mi::ICON_LOCK
        } else {
            mi::ICON_LOCK_OPEN
        };
        if ui
            .add(
                egui::Button::selectable(st.lock, icon.rich_text().size(18.0))
                    .min_size(EVec2::new(28.0, 60.0)),
            )
            .on_hover_text("Mantener la proporción")
            .clicked()
        {
            st.lock = !st.lock;
        }
    });
    if st.lock && bw > 0.0 && bh > 0.0 {
        if changed_w {
            st.height = st.width * bh / bw;
        } else if changed_h {
            st.width = st.height * bw / bh;
        }
    }
    let differs = bounds.is_some_and(|b| {
        (b.width() - st.width).abs() > 1e-6 || (b.height() - st.height).abs() > 1e-6
    });
    if wide_button(ui, "Aplicar", differs) {
        let (w, h) = (st.width, st.height);
        apply(app, shapes, |s| arrange::resize_group(s, w, h));
    }
}

fn rotate_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui, shapes: &[Shape], st: &mut State) {
    section(ui, "Rotar a");
    ui.horizontal(|ui| {
        let w = row_width(ui, 5);
        for (angle, icon) in [
            (0.0, mi::ICON_ARROW_UPWARD),
            (90.0, mi::ICON_ARROW_FORWARD),
            (180.0, mi::ICON_ARROW_DOWNWARD),
            (270.0, mi::ICON_ARROW_BACK),
        ] {
            if angle_button(
                ui,
                w,
                icon,
                &format!("{angle}°"),
                "Girar cada objeto a este ángulo",
            ) {
                apply(app, shapes, |s| arrange::rotate_each_to(s, angle));
            }
        }
    });
    ui.label(RichText::new("A medida").color(theme::CHROME_200));
    // Ángulo actual del primer objeto; al moverlo, cada objeto gira hasta ese ángulo.
    let mut current = shapes.first().map(|s| s.rotation).unwrap_or(0.0);
    let id = egui::Id::unique("transform_rotate_to_drag");
    let editing: bool = ui.data(|d| d.get_temp(id)).unwrap_or(false);
    if slider_field(ui, &mut current, 0.0..=360.0, " °", 1) {
        if !editing {
            app.commit();
        }
        for r in arrange::rotate_each_to(shapes, current) {
            if let Some(s) = app.doc.shape_mut(r.id) {
                *s = r;
            }
        }
        ui.data_mut(|d| d.insert_temp(id, true));
    } else if !ui.ctx().input(|i| i.pointer.any_down()) {
        ui.data_mut(|d| d.insert_temp(id, false));
    }

    ui.add_space(4.0);
    ui.separator();
    section(ui, "Rotar");
    ui.horizontal(|ui| {
        let w = row_width(ui, 5);
        for (delta, icon) in [
            (45.0, mi::ICON_ROTATE_RIGHT),
            (90.0, mi::ICON_ROTATE_RIGHT),
            (180.0, mi::ICON_ROTATE_RIGHT),
            (-90.0, mi::ICON_ROTATE_LEFT),
            (-45.0, mi::ICON_ROTATE_LEFT),
        ] {
            if angle_button(
                ui,
                w,
                icon,
                &format!("{delta}°"),
                "Girar la selección como grupo",
            ) {
                apply(app, shapes, |s| arrange::rotate_group(s, delta));
            }
        }
    });
    ui.label(RichText::new("A medida").color(theme::CHROME_200));
    slider_field(ui, &mut st.rotate_by, -180.0..=180.0, " °", 1);
    if wide_button(ui, "Aplicar", st.rotate_by != 0.0) {
        let d = st.rotate_by;
        apply(app, shapes, |s| arrange::rotate_group(s, d));
    }
}

/// Botón con icono y ángulo debajo (Rotar a / Rotar).
fn angle_button(ui: &mut Ui, w: f32, icon: MaterialIcon, label: &str, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(EVec2::new(w, 46.0), Sense::click());
    let resp = resp.on_hover_text(tip);
    let color = if !ui.is_enabled() {
        theme::CHROME_500
    } else if resp.hovered() {
        Color32::WHITE
    } else {
        theme::CHROME_200
    };
    if resp.hovered() && ui.is_enabled() {
        ui.painter().rect_filled(rect, 4.0, theme::CHROME_700);
    }
    ui.painter().text(
        rect.center() - EVec2::new(0.0, 7.0),
        egui::Align2::CENTER_CENTER,
        <&str>::from(icon),
        egui::FontId::proportional(20.0),
        color,
    );
    ui.painter().text(
        rect.center_bottom() - EVec2::new(0.0, 4.0),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(11.0),
        color,
    );
    resp.clicked()
}

fn move_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui, shapes: &[Shape], st: &mut State) {
    let unit = app.doc.page.unit;
    let suffix = format!(" {}", unit.label());
    section(ui, "Mover");
    ui.horizontal(|ui| {
        let d = st.distance;
        let w = row_width(ui, 4);
        for (icon, tip, v) in [
            (
                mi::ICON_ARROW_BACK,
                "Mover a la izquierda",
                Vec2::new(-d, 0.0),
            ),
            (
                mi::ICON_ARROW_FORWARD,
                "Mover a la derecha",
                Vec2::new(d, 0.0),
            ),
            (
                mi::ICON_ARROW_UPWARD,
                "Mover hacia arriba",
                Vec2::new(0.0, -d),
            ),
            (
                mi::ICON_ARROW_DOWNWARD,
                "Mover hacia abajo",
                Vec2::new(0.0, d),
            ),
        ] {
            if ui
                .add(
                    egui::Button::new(icon.rich_text().size(18.0))
                        .min_size(EVec2::new(w.max(34.0), 30.0)),
                )
                .on_hover_text(tip)
                .clicked()
            {
                apply(app, shapes, |s| arrange::translate(s, v));
            }
        }
    });
    ui.label(RichText::new("Distancia").color(theme::CHROME_200));
    let mut dist = unit.from_mm(st.distance);
    if slider_field(
        ui,
        &mut dist,
        0.0..=unit.from_mm(100.0),
        &suffix,
        unit.decimals(),
    ) {
        st.distance = unit.to_mm(dist);
    }

    ui.add_space(4.0);
    ui.separator();
    section(ui, "Mover hasta");
    let bounds = selection_bounds(shapes);
    let key = selection_key(shapes, st.anchor as u64);
    if key != st.move_key {
        st.move_key = key;
        if let Some(b) = bounds {
            let p = st.anchor.point(b);
            st.move_x = p.x;
            st.move_y = p.y;
        }
    }
    let (pw, ph) = (app.doc.page.width_mm, app.doc.page.height_mm);
    for (label, v, max) in [("X", &mut st.move_x, pw), ("Y", &mut st.move_y, ph)] {
        ui.label(RichText::new(label).color(theme::CHROME_200));
        let mut shown = unit.from_mm(*v);
        if slider_field(
            ui,
            &mut shown,
            unit.from_mm(-max)..=unit.from_mm(max * 2.0),
            &suffix,
            unit.decimals(),
        ) {
            *v = unit.to_mm(shown);
        }
    }
    ui.horizontal(|ui| {
        label_cell(ui, "Punto central");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            anchor_picker(ui, &mut st.anchor);
        });
    });
    let target = Point::new(st.move_x, st.move_y);
    let differs = bounds.is_some_and(|b| (st.anchor.point(b) - target).hypot() > 1e-6);
    if wide_button(ui, "Aplicar", differs) {
        let anchor = st.anchor;
        apply(app, shapes, |s| arrange::move_anchor_to(s, anchor, target));
    }
}

/// Rejilla 3×3 para elegir el punto de referencia de «Mover hasta».
fn anchor_picker(ui: &mut Ui, anchor: &mut Anchor) {
    let size = 56.0;
    let (rect, resp) = ui.allocate_exact_size(EVec2::splat(size), Sense::click());
    let p = ui.painter();
    let inner = rect.shrink(8.0);
    p.rect_stroke(
        inner,
        2.0,
        EStroke::new(1.0, theme::CHROME_300),
        StrokeKind::Inside,
    );
    let pos = |a: Anchor| {
        let (fx, fy) = a.fraction();
        inner.min + EVec2::new(inner.width() * fx as f32, inner.height() * fy as f32)
    };
    let hover = resp.hover_pos();
    for a in Anchor::ALL {
        let c = pos(a);
        let hot = hover.is_some_and(|h| h.distance(c) < 8.0);
        if a == *anchor {
            p.rect_filled(
                ERect::from_center_size(c, EVec2::splat(8.0)),
                1.0,
                theme::ACCENT,
            );
        } else {
            p.circle_filled(
                c,
                if hot { 3.5 } else { 2.0 },
                if hot {
                    Color32::WHITE
                } else {
                    theme::CHROME_300
                },
            );
        }
    }
    if resp.clicked() {
        if let Some(h) = resp.interact_pointer_pos() {
            if let Some(a) = Anchor::ALL
                .into_iter()
                .min_by(|a, b| h.distance(pos(*a)).total_cmp(&h.distance(pos(*b))))
            {
                *anchor = a;
            }
        }
    }
    resp.on_hover_text("Punto de la selección que se coloca en X, Y");
}
