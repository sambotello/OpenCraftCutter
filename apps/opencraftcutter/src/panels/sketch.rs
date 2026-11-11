use super::{row, section};
use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{RichText, Ui};
use oc_core::ops;
use oc_core::shape::Shape;

#[derive(Clone, Copy)]
struct Settings {
    angle: f64,
    spacing_mm: f64,
    cross: bool,
    keep_outline: bool,
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let id = egui::Id::unique("sketch_settings");
    let mut st: Settings = ui.data(|d| d.get_temp(id)).unwrap_or(Settings {
        angle: 45.0,
        spacing_mm: 2.0,
        cross: false,
        keep_outline: true,
    });
    let unit = app.doc.page.unit;
    let raw: Vec<Shape> = app
        .doc
        .selected_shapes()
        .into_iter()
        .filter(|s| !s.is_image())
        .cloned()
        .collect();
    let selected = crate::text::paths_for_ops(&raw);

    ui.label(
        RichText::new(if selected.is_empty() {
            "Selecciona una forma para rellenarla con líneas de dibujo."
        } else {
            "Relleno de líneas para plumas o grabado."
        })
        .small()
        .color(theme::CHROME_300),
    );
    section(ui, "Líneas");
    row(ui, "Ángulo", |ui| {
        ui.add(
            egui::DragValue::new(&mut st.angle)
                .speed(1.0)
                .range(-90.0..=90.0)
                .suffix(" °"),
        );
    });
    row(ui, "Separación", |ui| {
        let mut g = unit.from_mm(st.spacing_mm);
        if ui
            .add(
                egui::DragValue::new(&mut g)
                    .speed(unit.step() / 2.0)
                    .range(unit.from_mm(0.2)..=unit.from_mm(50.0))
                    .fixed_decimals(unit.decimals())
                    .suffix(format!(" {}", unit.label())),
            )
            .changed()
        {
            st.spacing_mm = unit.to_mm(g);
        }
    });
    ui.checkbox(&mut st.cross, "Cruzado (dos direcciones)");
    ui.checkbox(&mut st.keep_outline, "Conservar la forma original");
    ui.add_space(6.0);
    if ui
        .add_enabled(
            !selected.is_empty(),
            egui::Button::new("Generar sombreado").min_size(egui::vec2(ui.available_width(), 28.0)),
        )
        .clicked()
    {
        let out: Vec<Shape> = selected
            .iter()
            .filter_map(|s| ops::hatch(s, st.angle, st.spacing_mm, st.cross))
            .collect();
        if out.is_empty() {
            let ctx = ui.ctx().clone();
            app.notify(&ctx, "No cabe ninguna línea con estos ajustes.");
        } else if st.keep_outline {
            app.commit();
            let new_ids: Vec<u64> = out.into_iter().map(|s| app.doc.add_shape(s)).collect();
            app.doc.selected = new_ids;
        } else {
            let ids: Vec<u64> = selected.iter().map(|s| s.id).collect();
            app.replace_shapes(&ids, out);
        }
    }
    ui.data_mut(|d| d.insert_temp(id, st));
}
