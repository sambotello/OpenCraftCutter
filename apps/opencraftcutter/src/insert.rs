//! Diálogos de «Insertar» del menú Objeto: código QR y plantillas a medida.

use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::RichText;

/// Diálogo abierto.
#[derive(Clone, PartialEq)]
pub enum InsertDialog {
    Qr {
        text: String,
        size_mm: f64,
    },
    /// Plantilla elegida (índice en `Template::NAMES`) y sus medidas en mm.
    Template {
        kind: usize,
        dims: [f64; 3],
    },
}

/// Medidas por defecto de cada plantilla.
pub fn template_defaults(kind: usize) -> [f64; 3] {
    match kind {
        0 => [60.0, 40.0, 25.0],
        1 | 2 => [105.0, 148.0, 0.0],
        _ => [50.0, 30.0, 0.0],
    }
}

fn template_of(kind: usize, d: [f64; 3]) -> oc_core::templates::Template {
    use oc_core::templates::Template;
    match kind {
        0 => Template::Tray {
            w: d[0],
            d: d[1],
            h: d[2],
        },
        1 => Template::Envelope { w: d[0], h: d[1] },
        2 => Template::FoldedCard { w: d[0], h: d[1] },
        _ => Template::Tag { w: d[0], h: d[1] },
    }
}

/// Nombres de las medidas de cada plantilla.
fn dim_labels(kind: usize) -> &'static [&'static str] {
    match kind {
        0 => &["Ancho", "Fondo", "Alto"],
        1 => &["Ancho de la tarjeta", "Alto de la tarjeta"],
        2 => &["Ancho (cerrada)", "Alto"],
        _ => &["Ancho", "Alto"],
    }
}

impl OpenCraftCutterApp {
    /// Inserta una plantilla en la esquina del área útil: corte en rojo y dobleces en azul,
    /// agrupados. Devuelve si tiene dobleces.
    pub fn insert_template(&mut self, kind: usize, dims: [f64; 3]) -> bool {
        use oc_core::shape::{FillRule, LineStyle, Shape, ShapeKind};
        let t = template_of(kind, dims);
        let lines = oc_core::templates::lines(t);
        let origin = oc_core::send::safe_area(&self.doc).origin().to_vec2();
        let shift = |v: &Vec<(Vec<kurbo::Point>, bool)>| -> Vec<(Vec<kurbo::Point>, bool)> {
            v.iter()
                .map(|(p, c)| (p.iter().map(|q| *q + origin).collect(), *c))
                .collect()
        };
        let make = |polys: &[(Vec<kurbo::Point>, bool)], name: String| {
            let (rect, subpaths) = Shape::normalize_subpaths(polys);
            let mut s = Shape::new(
                0,
                name,
                rect,
                ShapeKind::Path {
                    subpaths,
                    fill_rule: FillRule::EvenOdd,
                },
            );
            s.fill = None;
            s
        };
        self.commit();
        let g = self.doc.next_id();
        let mut cut = make(&shift(&lines.cut), t.name().to_string());
        cut.stroke.color = oc_core::Color::rgb(0xe3, 0x34, 0x3f);
        cut.group = Some(g);
        let mut ids = vec![self.doc.add_shape(cut)];
        let folds = !lines.fold.is_empty();
        if folds {
            let mut f = make(&shift(&lines.fold), format!("{}: dobleces", t.name()));
            f.stroke.color = oc_core::Color::rgb(0x20, 0x30, 0xdf);
            f.stroke.style = LineStyle::Dashed;
            f.group = Some(g);
            ids.push(self.doc.add_shape(f));
        }
        self.doc.selected = ids;
        folds
    }

    /// Inserta un código QR centrado en la página y lo selecciona. `false` si no cabe el texto.
    pub fn insert_qr(&mut self, text: &str, size_mm: f64) -> bool {
        let page = &self.doc.page;
        let origin = kurbo::Point::new(
            (page.width_mm - size_mm) / 2.0,
            (page.height_mm - size_mm) / 2.0,
        );
        let Some(s) = oc_core::qr::qr_shape(text, size_mm, origin) else {
            return false;
        };
        self.commit();
        let id = self.doc.add_shape(s);
        self.doc.selected = vec![id];
        true
    }
}

pub fn window(app: &mut OpenCraftCutterApp, ctx: &egui::Context) {
    let Some(dialog) = app.insert_dialog.clone() else {
        return;
    };
    let mut open = true;
    let next = match dialog {
        InsertDialog::Template { mut kind, mut dims } => {
            let unit = app.doc.page.unit;
            let mut insert = false;
            egui::Window::new("Insertar plantilla")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    egui::ComboBox::from_label("Plantilla")
                        .selected_text(oc_core::templates::Template::NAMES[kind])
                        .show_ui(ui, |ui| {
                            for (i, n) in oc_core::templates::Template::NAMES.iter().enumerate() {
                                if ui.selectable_label(kind == i, *n).clicked() && kind != i {
                                    kind = i;
                                    dims = template_defaults(i);
                                }
                            }
                        });
                    for (i, label) in dim_labels(kind).iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(*label);
                            let mut v = unit.from_mm(dims[i]);
                            if ui
                                .add(
                                    egui::DragValue::new(&mut v)
                                        .range(unit.from_mm(5.0)..=unit.from_mm(600.0))
                                        .speed(unit.step())
                                        .fixed_decimals(unit.decimals())
                                        .suffix(format!(" {}", unit.label())),
                                )
                                .changed()
                            {
                                dims[i] = unit.to_mm(v);
                            }
                        });
                    }
                    insert = ui.button("Insertar").clicked();
                });
            if insert {
                if app.insert_template(kind, dims) {
                    app.notify(
                        ctx,
                        "Los dobleces van en azul: en Enviar › Por color de línea, dales «Marcar» o poca profundidad.",
                    );
                }
                None
            } else {
                Some(InsertDialog::Template { kind, dims })
            }
        }
        InsertDialog::Qr {
            mut text,
            mut size_mm,
        } => {
            let unit = app.doc.page.unit;
            let mut insert = false;
            egui::Window::new("Insertar código QR")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Texto o enlace");
                    ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .desired_rows(2)
                            .desired_width(300.0),
                    );
                    ui.horizontal(|ui| {
                        ui.label("Tamaño");
                        let mut v = unit.from_mm(size_mm);
                        if ui
                            .add(
                                egui::DragValue::new(&mut v)
                                    .range(unit.from_mm(10.0)..=unit.from_mm(300.0))
                                    .speed(unit.step())
                                    .fixed_decimals(unit.decimals())
                                    .suffix(format!(" {}", unit.label())),
                            )
                            .changed()
                        {
                            size_mm = unit.to_mm(v);
                        }
                    });
                    ui.label(
                        RichText::new(
                            "Para cortarlo en vinil, mejor 25 mm o más: los módulos pequeños se \
                             despegan al pelar.",
                        )
                        .small()
                        .color(theme::CHROME_300),
                    );
                    ui.horizontal(|ui| {
                        insert = ui
                            .add_enabled(!text.trim().is_empty(), egui::Button::new("Insertar"))
                            .clicked();
                    });
                });
            if insert && app.insert_qr(text.trim(), size_mm) {
                None
            } else {
                if insert {
                    app.notify(ctx, "El texto es demasiado largo para un código QR.");
                }
                Some(InsertDialog::Qr { text, size_mm })
            }
        }
    };
    app.insert_dialog = if open { next } else { None };
}
