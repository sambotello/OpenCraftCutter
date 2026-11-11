use super::{row, section};
use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{RichText, Ui};
use oc_core::ops::{self, StoneLayout};
use oc_core::shape::Shape;

#[derive(Clone, Copy)]
struct Settings {
    diameter_mm: f64,
    gap_mm: f64,
    layout: StoneLayout,
    replace: bool,
}

/// Tamaños comerciales habituales (SS = stone size).
const SIZES: [(&str, f64); 6] = [
    ("SS6 (2,0 mm)", 2.0),
    ("SS8 (2,4 mm)", 2.4),
    ("SS10 (2,8 mm)", 2.8),
    ("SS12 (3,1 mm)", 3.1),
    ("SS16 (3,9 mm)", 3.9),
    ("SS20 (4,7 mm)", 4.7),
];

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let id = egui::Id::unique("rhinestone_settings");
    let mut st: Settings = ui.data(|d| d.get_temp(id)).unwrap_or(Settings {
        diameter_mm: 2.8,
        gap_mm: 0.5,
        layout: StoneLayout::Outline,
        replace: true,
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
    let ids: Vec<u64> = selected.iter().map(|s| s.id).collect();

    ui.label(
        RichText::new(if selected.is_empty() {
            "Selecciona una forma para rellenarla de pedrería."
        } else {
            "Genera círculos del tamaño de la piedra para cortar una plantilla."
        })
        .small()
        .color(theme::CHROME_300),
    );
    section(ui, "Piedra");
    row(ui, "Tamaño", |ui| {
        let label = SIZES
            .iter()
            .find(|(_, d)| (*d - st.diameter_mm).abs() < 1e-9)
            .map(|(n, _)| n.to_string())
            .unwrap_or_else(|| format!("{:.2} mm", st.diameter_mm));
        egui::ComboBox::from_id_salt("stone_size")
            .width(super::CONTROL_WIDTH)
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (n, d) in SIZES {
                    ui.selectable_value(&mut st.diameter_mm, d, n);
                }
            });
    });
    row(ui, "Diámetro", |ui| {
        let mut d = unit.from_mm(st.diameter_mm);
        if ui
            .add(
                egui::DragValue::new(&mut d)
                    .speed(unit.step() / 2.0)
                    .range(unit.from_mm(0.5)..=unit.from_mm(20.0))
                    .fixed_decimals(unit.decimals())
                    .suffix(format!(" {}", unit.label())),
            )
            .changed()
        {
            st.diameter_mm = unit.to_mm(d);
        }
    });
    row(ui, "Separación", |ui| {
        let mut g = unit.from_mm(st.gap_mm);
        if ui
            .add(
                egui::DragValue::new(&mut g)
                    .speed(unit.step() / 2.0)
                    .range(0.0..=unit.from_mm(20.0))
                    .fixed_decimals(unit.decimals())
                    .suffix(format!(" {}", unit.label())),
            )
            .changed()
        {
            st.gap_mm = unit.to_mm(g);
        }
    });
    section(ui, "Disposición");
    ui.horizontal(|ui| {
        for (l, name) in [
            (StoneLayout::Outline, "Contorno"),
            (StoneLayout::Fill, "Relleno"),
            (StoneLayout::Both, "Ambos"),
        ] {
            if ui.selectable_label(st.layout == l, name).clicked() {
                st.layout = l;
            }
        }
    });
    ui.checkbox(&mut st.replace, "Sustituir la forma original");
    ui.add_space(6.0);
    if ui
        .add_enabled(
            !selected.is_empty(),
            egui::Button::new("Generar pedrería").min_size(egui::vec2(ui.available_width(), 28.0)),
        )
        .clicked()
    {
        let out: Vec<Shape> = selected
            .iter()
            .filter_map(|s| ops::rhinestones(s, st.diameter_mm, st.gap_mm, st.layout))
            .collect();
        if out.is_empty() {
            let ctx = ui.ctx().clone();
            app.notify(&ctx, "No cabe ninguna piedra con estos ajustes.");
        } else if st.replace {
            app.replace_shapes(&ids, out);
        } else {
            app.commit();
            let new_ids: Vec<u64> = out.into_iter().map(|s| app.doc.add_shape(s)).collect();
            app.doc.selected = new_ids;
        }
    }
    ui.data_mut(|d| d.insert_temp(id, st));
}
