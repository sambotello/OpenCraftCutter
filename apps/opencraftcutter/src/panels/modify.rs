use super::section;
use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{RichText, Ui};
use egui_material_icons::icons as mi;
use oc_core::ops::{self, BoolOp};
use oc_core::shape::Shape;

impl OpenCraftCutterApp {
    /// Sustituye las formas `remove` por `add` (con historial) y selecciona las nuevas.
    pub fn replace_shapes(&mut self, remove: &[u64], add: Vec<Shape>) {
        self.commit();
        // Insertar en la posición de pila de la primera forma eliminada.
        let pos = self
            .doc
            .shapes
            .iter()
            .position(|s| remove.contains(&s.id))
            .unwrap_or(self.doc.shapes.len());
        self.doc.remove_shapes(remove);
        let mut ids = Vec::new();
        for (i, s) in add.into_iter().enumerate() {
            let id = self.doc.add_shape(s);
            let last = self.doc.shapes.len() - 1;
            self.doc.shapes.swap(last, (pos + i).min(last));
            ids.push(id);
        }
        self.doc.selected = ids;
    }
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let raw: Vec<Shape> = app
        .doc
        .selected_shapes()
        .into_iter()
        .filter(|s| !s.is_image())
        .cloned()
        .collect();
    let selected: Vec<Shape> = crate::text::paths_for_ops(&raw);
    let ids: Vec<u64> = selected.iter().map(|s| s.id).collect();
    let n = selected.len();
    let has_text = raw.iter().any(|s| s.is_text());
    ui.label(
        RichText::new(if n == 0 {
            "Selecciona dos o más formas.".to_string()
        } else {
            format!(
                "{n} objeto{} seleccionado{}.",
                if n > 1 { "s" } else { "" },
                if n > 1 { "s" } else { "" }
            )
        })
        .small()
        .color(theme::CHROME_300),
    );
    if has_text {
        ui.label(
            RichText::new(
                "Los textos se convierten en trazado al aplicar la operación. Fusionar un texto solo une las letras que se tocan.",
            )
            .small()
            .color(egui::Color32::from_rgb(0xfb, 0xbf, 0x24)),
        );
    }

    // Operaciones (como la referencia, en dos columnas).
    let mut run: Option<BoolOp> = None;
    let grid = [
        (
            BoolOp::Weld,
            mi::ICON_JOIN_FULL,
            "Une las formas en una sola. Con una sola forma (p. ej., un texto), une sus partes que se tocan.",
        ),
        (
            BoolOp::SubtractFront,
            mi::ICON_JOIN_LEFT,
            "Resta la forma de delante de cada una de las de detrás.",
        ),
        (
            BoolOp::Subtract,
            mi::ICON_FILTER_NONE,
            "Resta todas las formas de delante de la de más atrás.",
        ),
        (
            BoolOp::Intersect,
            mi::ICON_JOIN_INNER,
            "Conserva solo la zona común a todas.",
        ),
        (
            BoolOp::Divide,
            mi::ICON_VERTICAL_SPLIT,
            "Corta las formas en todas sus piezas.",
        ),
        (
            BoolOp::Crop,
            mi::ICON_CROP,
            "Recorta las formas de detrás a la forma de delante.",
        ),
    ];
    egui::Grid::new("modify_grid")
        .num_columns(2)
        .spacing([6.0, 6.0])
        .show(ui, |ui| {
            for (i, (op, icon, tip)) in grid.iter().enumerate() {
                let enabled = if *op == BoolOp::Weld { n >= 1 } else { n >= 2 };
                if op_button(ui, *icon, op.name(), tip, enabled) {
                    run = Some(*op);
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    if let Some(op) = run {
        match ops::boolean(&selected, op) {
            Some(result) if !result.is_empty() => app.replace_shapes(&ids, result),
            Some(_) => {
                let ctx = ui.ctx().clone();
                app.notify(&ctx, "La operación no deja ninguna forma.");
            }
            None => {
                let ctx = ui.ctx().clone();
                app.notify(&ctx, "Hacen falta al menos dos formas cerradas.");
            }
        }
    }

    ui.add_space(2.0);
    ui.separator();
    section(ui, "Trazados compuestos");
    let compound = selected.iter().any(|s| s.world_subpaths().len() > 1);
    let mut action = None;
    egui::Grid::new("compound_grid")
        .num_columns(2)
        .spacing([6.0, 6.0])
        .show(ui, |ui| {
            if op_button(
                ui,
                mi::ICON_DONUT_LARGE,
                "Crear",
                "Une las formas en un trazado compuesto: los solapes se vuelven agujeros.",
                n >= 2,
            ) {
                action = Some(0);
            }
            if op_button(
                ui,
                mi::ICON_LAYERS_CLEAR,
                "Soltar",
                "Convierte cada trazo del compuesto en una forma (los agujeros también).",
                compound,
            ) {
                action = Some(1);
            }
            ui.end_row();
        });
    let w = ui.available_width();
    if ui
        .add_enabled(
            compound,
            egui::Button::new(format!(
                "{}   Separar trazos",
                <&str>::from(mi::ICON_CALL_SPLIT)
            ))
            .min_size(egui::vec2(w, 32.0)),
        )
        .on_hover_text("Separa las islas en formas; cada una conserva sus agujeros.")
        .clicked()
    {
        action = Some(2);
    }
    match action {
        Some(0) => {
            if let Some(out) = ops::combine(&selected) {
                app.replace_shapes(&ids, vec![out]);
            }
        }
        Some(1) => {
            let out: Vec<Shape> = selected.iter().flat_map(ops::separate).collect();
            app.replace_shapes(&ids, out);
        }
        Some(2) => {
            let out: Vec<Shape> = selected.iter().flat_map(ops::break_apart).collect();
            app.replace_shapes(&ids, out);
        }
        _ => {}
    }

    ui.add_space(2.0);
    ui.separator();
    section(ui, "Otras");
    if op_button(
        ui,
        mi::ICON_JOIN_RIGHT,
        BoolOp::Exclude.name(),
        "Conserva lo que no se solapa.",
        n >= 2,
    ) {
        match ops::boolean(&selected, BoolOp::Exclude) {
            Some(result) if !result.is_empty() => app.replace_shapes(&ids, result),
            _ => {}
        }
    }
}

/// Botón con icono y texto (media anchura del panel).
fn op_button(
    ui: &mut Ui,
    icon: egui_material_icons::MaterialIcon,
    label: &str,
    tip: &str,
    enabled: bool,
) -> bool {
    let w = (ui.available_width().min(260.0) - 6.0) / 2.0;
    ui.add_enabled(
        enabled,
        egui::Button::new(format!("{}   {label}", <&str>::from(icon)))
            .min_size(egui::vec2(w.max(118.0), 32.0)),
    )
    .on_hover_text(tip)
    .on_disabled_hover_text(tip)
    .clicked()
}
