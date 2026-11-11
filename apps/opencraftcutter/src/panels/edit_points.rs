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
    ToLine,
    ToCurve,
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
            Action::ToLine => "Convertir en recta",
            Action::ToCurve => "Convertir en curva",
            Action::Simplify => "Simplificar",
            Action::ToPath => "Convertir en trazado",
        }
    }

    pub const ALL: [Action; 9] = [
        Action::Delete,
        Action::Break,
        Action::Join,
        Action::Corner,
        Action::Smooth,
        Action::ToLine,
        Action::ToCurve,
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
    });
    let has_seg = node
        .as_ref()
        .is_some_and(|(np, ni)| crate::canvas::point_edit::highlighted_segment(np, *ni).is_some());
    match a {
        Action::Delete | Action::Break => (node.is_some(), false),
        // Solo en un extremo libre con otro extremo al que unirse.
        Action::Join => (
            sel.is_some_and(|(si, ni)| {
                crate::canvas::point_edit::edited_paths(app)
                    .is_some_and(|(_, p)| oc_core::nodes::join_target(&p, si, ni).is_some())
            }),
            false,
        ),
        Action::Corner => (
            node.is_some(),
            node.as_ref().is_some_and(|(np, ni)| !np.nodes[*ni].smooth),
        ),
        Action::Smooth => (
            node.is_some(),
            node.as_ref().is_some_and(|(np, ni)| np.nodes[*ni].smooth),
        ),
        Action::ToLine | Action::ToCurve => (has_seg, false),
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

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    super::section(ui, "Punto seleccionado");
    group(
        ui,
        app,
        &[
            Action::Delete,
            Action::Break,
            Action::Join,
            Action::Corner,
            Action::Smooth,
        ],
    );
    super::section(ui, "Trazo seleccionado");
    group(ui, app, &[Action::ToLine, Action::ToCurve]);
    group(ui, app, &[Action::Simplify]);
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
