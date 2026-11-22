//! Panel Editar puntos, con las secciones, el orden y los textos de la referencia.

use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{RichText, Ui, Vec2 as EVec2};
use egui_material_icons::{MaterialIcon, icons as mi};

/// Acciones de Editar puntos (compartidas por el panel y la barra superior).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Delete,
    Break,
    Join,
    Corner,
    Smooth,
    Symmetric,
    ToLine,
    ToCurve,
    Midpoint,
    Reverse,
    Close,
    Simplify,
    ToPath,
}

impl Action {
    pub fn icon(self) -> MaterialIcon {
        match self {
            Action::Delete => mi::ICON_CLOSE,
            Action::Break => mi::ICON_CONTENT_CUT,
            Action::Join => mi::ICON_LINK,
            Action::Corner => mi::ICON_TURN_RIGHT,
            Action::Smooth => mi::ICON_GESTURE,
            Action::Symmetric => mi::ICON_BALANCE,
            Action::Midpoint => mi::ICON_COMMIT,
            Action::Reverse => mi::ICON_SWAP_HORIZ,
            Action::Close => mi::ICON_LOOP,
            Action::ToLine => mi::ICON_NORTH_EAST,
            Action::ToCurve => mi::ICON_SHOW_CHART,
            Action::Simplify => mi::ICON_ROUTE,
            Action::ToPath => mi::ICON_PENTAGON,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::Delete => "Borrar punto",
            Action::Break => "Romper trazado",
            Action::Join => "Unir trazados",
            Action::Corner => "Vértice",
            Action::Smooth => "Suavizar",
            Action::Symmetric => "Simétrico",
            Action::Midpoint => "Añadir punto medio",
            Action::Reverse => "Invertir sentido",
            Action::Close => "Cerrar trazado",
            Action::ToLine => "Convertir en recta",
            Action::ToCurve => "Convertir en curva",
            Action::Simplify => "Simplificar",
            Action::ToPath => "Convertir en trazado",
        }
    }

    pub const ALL: [Action; 13] = [
        Action::Delete,
        Action::Break,
        Action::Join,
        Action::Corner,
        Action::Smooth,
        Action::Symmetric,
        Action::ToLine,
        Action::ToCurve,
        Action::Midpoint,
        Action::Reverse,
        Action::Close,
        Action::Simplify,
        Action::ToPath,
    ];
}

/// ¿Está disponible la acción? (`active` = resaltada: tipo del nodo seleccionado).
pub fn state(app: &OpenCraftCutterApp, a: Action) -> (bool, bool) {
    let sel = app.canvas.point_edit.and_then(|pe| pe.selected);
    let node = sel.and_then(|(si, ni)| {
        crate::canvas::point_edit::edited_paths(app)
            .and_then(|(_, p)| p.get(si).map(|np| (np.clone(), ni)))
            .filter(|(np, ni)| *ni < np.nodes.len())
    });
    let has_seg = node
        .as_ref()
        .is_some_and(|(np, ni)| crate::canvas::point_edit::highlighted_segment(np, *ni).is_some());
    match a {
        Action::Delete | Action::Break => (node.is_some(), false),
        // Dos extremos libres elegidos, o uno con otro extremo al que unirse.
        Action::Join => (app.can_join(), false),
        Action::Corner => (
            node.is_some(),
            node.as_ref().is_some_and(|(np, ni)| !np.nodes[*ni].smooth),
        ),
        Action::Smooth => (
            node.is_some(),
            node.as_ref()
                .is_some_and(|(np, ni)| np.nodes[*ni].smooth && !np.nodes[*ni].symmetric),
        ),
        Action::Symmetric => (
            node.is_some(),
            node.as_ref()
                .is_some_and(|(np, ni)| np.nodes[*ni].symmetric),
        ),
        Action::ToLine | Action::ToCurve | Action::Midpoint => {
            (has_seg || !app.midpoint_segments().is_empty(), false)
        }
        Action::Reverse => (node.is_some(), false),
        Action::Close => (app.can_close(), false),
        Action::Simplify => (!app.doc.selected.is_empty(), false),
        Action::ToPath => (
            app.doc.selected_shapes().iter().any(|s| {
                matches!(
                    s.kind,
                    oc_core::ShapeKind::Rect { .. }
                        | oc_core::ShapeKind::Ellipse
                        | oc_core::ShapeKind::Text { .. }
                )
            }),
            false,
        ),
    }
}

pub fn run(app: &mut OpenCraftCutterApp, a: Action) {
    match a {
        Action::Delete => {
            app.delete_selected_vertex();
        }
        Action::Break => app.point_break(),
        Action::Join => app.point_join(),
        Action::Corner => app.point_set_smooth(false),
        Action::Smooth => app.point_set_smooth(true),
        Action::Symmetric => app.point_set_symmetric(),
        Action::Midpoint => app.point_midpoints(),
        Action::Reverse => app.point_reverse(),
        Action::Close => app.point_close(),
        Action::ToLine => app.point_segment_kind(false),
        Action::ToCurve => app.point_segment_kind(true),
        Action::Simplify => app.point_simplify(),
        Action::ToPath => app.point_convert_to_path(),
    }
}

fn group(ui: &mut Ui, app: &mut OpenCraftCutterApp, actions: &[Action]) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, theme::CHROME_600))
        .corner_radius(6.0)
        .inner_margin(2.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for &a in actions {
                let (enabled, active) = state(app, a);
                let text = RichText::new(format!("{}   {}", <&str>::from(a.icon()), a.label()));
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::selectable(active, text)
                            .min_size(EVec2::new(ui.available_width(), 30.0)),
                    )
                    .clicked()
                {
                    run(app, a);
                }
            }
        });
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

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(theme::CHROME_400));
}

/// Campos X e Y del nodo principal, en la unidad de la página. Un arrastre o una edición
/// completa del campo es una sola entrada de historial.
fn position(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let node = app
        .canvas
        .point_edit
        .and_then(|pe| pe.selected)
        .and_then(|(si, ni)| {
            crate::canvas::point_edit::edited_paths(app)
                .and_then(|(_, p)| p.get(si)?.nodes.get(ni).map(|n| n.p))
        });
    let unit = app.doc.page.unit;
    let at = node.unwrap_or(kurbo::Point::ZERO);
    let (mut x, mut y) = (unit.from_mm(at.x), unit.from_mm(at.y));
    let editing_id = egui::Id::unique("editar-puntos-posicion-en-edicion");
    let mut changed = false;
    let mut active = false;
    super::row(ui, "Posición", |ui| {
        // De derecha a izquierda: primero Y, luego X.
        for (value, prefix) in [(&mut y, "Y "), (&mut x, "X ")] {
            let r = ui.add_enabled(
                node.is_some(),
                egui::DragValue::new(value)
                    .speed(unit.from_mm(0.1))
                    .fixed_decimals(unit.decimals() + 1)
                    .prefix(prefix)
                    .suffix(format!(" {}", unit.label())),
            );
            crate::theme::name_widget(&r, &format!("Posición {}", prefix.trim()));
            changed |= r.changed();
            active |= r.dragged() || r.has_focus();
        }
    });
    let editing = ui.data(|d| d.get_temp::<bool>(editing_id).unwrap_or(false));
    if changed && node.is_some() {
        app.point_set_position(kurbo::Point::new(unit.to_mm(x), unit.to_mm(y)), !editing);
        ui.data_mut(|d| d.insert_temp(editing_id, true));
    } else if editing && !active {
        ui.data_mut(|d| d.insert_temp(editing_id, false));
    }
}

/// Radio de redondeo de las esquinas seleccionadas, en la unidad de la página. Arrastrar o
/// escribir el valor ajusta el mismo redondeo; 0 devuelve la esquina viva.
fn fillet(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let unit = app.doc.page.unit;
    let enabled = app.can_fillet();
    let shown = unit.from_mm(app.fillet_radius());
    let mut v = shown;
    let mut changed = false;
    super::row(ui, "Radio", |ui| {
        let r = ui.add_enabled(
            enabled,
            egui::DragValue::new(&mut v)
                .speed(unit.from_mm(0.1))
                .range(0.0..=unit.from_mm(1000.0))
                .fixed_decimals(unit.decimals() + 1)
                .suffix(format!(" {}", unit.label())),
        );
        crate::theme::name_widget(&r, "Radio de redondeo");
        changed = r.changed() && v != shown;
    });
    if changed {
        app.point_fillet(unit.to_mm(v));
    }
    hint(
        ui,
        if enabled {
            "Sube el radio para redondear las esquinas seleccionadas; 0 las deja en punta. \
             Si no cabe en los tramos, se usa el mayor radio posible."
        } else {
            "Selecciona un punto en esquina (no un extremo ni un punto suave)."
        },
    );
}

/// Fila de tres botones de alineación: (icono, ayuda, modo).
type AlignRow = [(MaterialIcon, &'static str, oc_core::arrange::Align); 3];

/// Botones de alinear y distribuir los nodos seleccionados.
fn align(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    use oc_core::arrange::{Align, Axis};
    let n = app.selected_nodes().len();
    let rows: [(&str, AlignRow); 2] = [
        (
            "Horizontal",
            [
                (
                    mi::ICON_ALIGN_HORIZONTAL_RIGHT,
                    "Alinear puntos a la derecha",
                    Align::Right,
                ),
                (
                    mi::ICON_ALIGN_HORIZONTAL_CENTER,
                    "Centrar puntos horizontalmente",
                    Align::HCenter,
                ),
                (
                    mi::ICON_ALIGN_HORIZONTAL_LEFT,
                    "Alinear puntos a la izquierda",
                    Align::Left,
                ),
            ],
        ),
        (
            "Vertical",
            [
                (
                    mi::ICON_ALIGN_VERTICAL_BOTTOM,
                    "Alinear puntos abajo",
                    Align::Bottom,
                ),
                (
                    mi::ICON_ALIGN_VERTICAL_CENTER,
                    "Centrar puntos verticalmente",
                    Align::VCenter,
                ),
                (
                    mi::ICON_ALIGN_VERTICAL_TOP,
                    "Alinear puntos arriba",
                    Align::Top,
                ),
            ],
        ),
    ];
    for (label, items) in rows {
        super::row(ui, label, |ui| {
            for (icon, tip, mode) in items {
                if icon_button(ui, icon, tip, n >= 2) {
                    app.point_align(mode);
                }
            }
        });
    }
    super::row(ui, "Distribuir", |ui| {
        for (icon, tip, axis) in [
            (
                mi::ICON_VERTICAL_DISTRIBUTE,
                "Distribuir puntos verticalmente (3 o más)",
                Axis::Vertical,
            ),
            (
                mi::ICON_HORIZONTAL_DISTRIBUTE,
                "Distribuir puntos horizontalmente (3 o más)",
                Axis::Horizontal,
            ),
        ] {
            if icon_button(ui, icon, tip, n >= 3) {
                app.point_distribute(axis);
            }
        }
    });
    if n < 2 {
        hint(
            ui,
            "Selecciona varios puntos con Mayús + clic, con un recuadro o con ⌘A.",
        );
    }
}

/// Opciones de ajuste al arrastrar nodos (las mismas de la página).
fn snapping(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let page = &mut app.doc.page;
    let mut changed = ui
        .checkbox(&mut page.smart_snap, "Ajustar a puntos y guías")
        .changed();
    changed |= ui
        .checkbox(&mut page.snap_to_grid, "Ajustar a retícula")
        .changed();
    if changed {
        app.dirty = true;
    }
    hint(
        ui,
        "Mayús: mover en horizontal, vertical o diagonal (y girar manejadores de 15° en 15°). \
         ⌘: sin ajuste. Alt: mover un manejador solo. Flechas: 1 mm (Mayús 10, Alt 0,1).",
    );
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    super::section(ui, "Punto seleccionado");
    position(app, ui);
    group(ui, app, &[Action::Delete, Action::Break, Action::Join]);
    super::section(ui, "Tipo de punto");
    group(
        ui,
        app,
        &[Action::Corner, Action::Smooth, Action::Symmetric],
    );
    super::section(ui, "Redondear esquina");
    fillet(app, ui);
    super::section(ui, "Alinear puntos");
    align(app, ui);
    super::section(ui, "Trazo seleccionado");
    group(
        ui,
        app,
        &[Action::ToLine, Action::ToCurve, Action::Midpoint],
    );
    group(ui, app, &[Action::Reverse, Action::Close]);
    group(ui, app, &[Action::Simplify]);
    super::section(ui, "Ajuste");
    snapping(app, ui);
    super::section(ui, "Convertir en trazado");
    group(ui, app, &[Action::ToPath]);
    super::section(ui, "Recuento de nodos");
    let (shapes, contours, nodes, linear, bezier) = crate::canvas::point_edit::node_counts(app);
    for (icon, label, v) in [
        (mi::ICON_CROP_FREE, "Formas seleccionadas", shapes),
        (mi::ICON_SYNC, "Contornos", contours),
        (mi::ICON_POLYLINE, "Recuento de nodos", nodes),
        (mi::ICON_TURN_RIGHT, "Nodos lineales", linear),
        (mi::ICON_GESTURE, "Nodos de Bézier", bezier),
    ] {
        ui.horizontal(|ui| {
            ui.label(icon.rich_text().size(16.0).color(theme::CHROME_200));
            ui.label(RichText::new(label).color(theme::CHROME_200));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(v.to_string()).color(theme::CHROME_100));
            });
        });
    }
    if app.canvas.point_edit.is_none() {
        ui.label(
            RichText::new("Haz clic en una forma para editar sus puntos. Clic en la línea para añadir un nodo.")
                .small()
                .color(theme::CHROME_400),
        );
    }
}
