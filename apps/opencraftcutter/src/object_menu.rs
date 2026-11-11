//! Menú Objeto, con los submenús, el orden y los atajos de la referencia.

use crate::app::OpenCraftCutterApp;
use kurbo::Rect;
use oc_core::arrange::{self, Align, Axis};
use oc_core::ops::{self, BoolOp};
use oc_core::replicate::{self, Side};
use oc_core::shape::Shape;

#[derive(Clone, Copy, PartialEq)]
pub enum Cmd {
    FlipH,
    FlipV,
    Reorder(i32),
    Align(Align),
    Distribute(Axis),
    CenterOnPage,
    /// Acomodar las piezas en el área útil para gastar menos material.
    Pack,
    RotateBy(f64),
    RotateTo(f64),
    Duplicate(Side),
    Mirror(Side),
    RotateCopies(usize),
    Row(Side, usize),
    FillPage,
    Bool(BoolOp),
    BreakApart,
    Group,
    Ungroup,
    ToPath,
    MakeCompound,
    ReleaseCompound,
    Simplify,
}

/// Separación entre piezas al acomodarlas (mm).
pub const PACK_GAP_MM: f64 = 3.0;

impl OpenCraftCutterApp {
    fn unlocked_selection(&self) -> Vec<Shape> {
        self.doc
            .selected_shapes()
            .into_iter()
            .filter(|s| !s.locked)
            .cloned()
            .collect()
    }

    /// Sustituye las formas por su versión transformada (una entrada de historial).
    fn apply_transformed(&mut self, out: Vec<Shape>) {
        if out.is_empty() {
            return;
        }
        self.commit();
        for r in out {
            if let Some(s) = self.doc.shape_mut(r.id) {
                *s = r;
            }
        }
    }

    fn add_new(&mut self, copies: Vec<Shape>) {
        if copies.is_empty() {
            return;
        }
        self.commit();
        let ids: Vec<u64> = copies.into_iter().map(|c| self.doc.add_shape(c)).collect();
        self.doc.selected = ids;
    }

    /// ¿Se puede ejecutar ahora?
    pub fn object_enabled(&self, c: Cmd) -> bool {
        let n = self.doc.selected.len();
        let shapes = self.doc.selected_shapes();
        match c {
            Cmd::Distribute(_) => n >= 3,
            // Fusionar también sirve con una sola forma (letras que se tocan en un texto).
            Cmd::Bool(BoolOp::Weld) => n >= 1,
            Cmd::Bool(_) => n >= 2,
            Cmd::MakeCompound => n >= 2,
            Cmd::Group => n >= 2,
            Cmd::Ungroup => self.doc.selection_has_group(),
            Cmd::BreakApart | Cmd::ReleaseCompound => {
                shapes.iter().any(|s| s.world_subpaths().len() > 1)
            }
            Cmd::ToPath => shapes.iter().any(|s| {
                matches!(
                    s.kind,
                    oc_core::ShapeKind::Rect { .. }
                        | oc_core::ShapeKind::Ellipse
                        | oc_core::ShapeKind::Text { .. }
                )
            }),
            _ => n >= 1,
        }
    }

    pub fn object_run(&mut self, c: Cmd) {
        if !self.object_enabled(c) {
            return;
        }
        let shapes = self.unlocked_selection();
        let Some(b) = oc_core::edit::selection_bounds(shapes.iter()) else {
            return;
        };
        let page = Rect::new(0.0, 0.0, self.doc.page.width_mm, self.doc.page.height_mm);
        match c {
            Cmd::FlipH | Cmd::FlipV => {
                // Volteo en el sitio sobre el centro de la selección.
                let horizontal = c == Cmd::FlipH;
                let axis = if horizontal {
                    b.center().x
                } else {
                    b.center().y
                };
                let out = crate::text::paths_for_ops(&shapes)
                    .iter()
                    .map(|s| oc_core::edit::mirror_shape(s, horizontal, axis))
                    .collect();
                self.apply_transformed(out);
            }
            Cmd::Reorder(d) => {
                self.commit();
                self.doc.reorder_selected(d);
            }
            Cmd::Align(mode) => {
                let m = self.doc.page.cut_margin_mm;
                let reference = if shapes.len() >= 2 {
                    b
                } else {
                    Rect::new(m, m, page.x1 - m, page.y1 - m)
                };
                self.apply_transformed(arrange::align(&shapes, mode, reference));
            }
            Cmd::Distribute(axis) => self.apply_transformed(arrange::distribute(&shapes, axis)),
            Cmd::CenterOnPage => {
                self.apply_transformed(arrange::center_group_on(&shapes, page.center()))
            }
            Cmd::Pack => {
                let area = oc_core::send::safe_area(&self.doc);
                let (out, missing) = arrange::pack_shapes(&shapes, area, PACK_GAP_MM);
                self.apply_transformed(out);
                if missing > 0 {
                    self.pending_notice = Some(if missing == 1 {
                        "1 pieza no cabe en la página: se queda donde estaba.".into()
                    } else {
                        format!("{missing} piezas no caben en la página: se quedan donde estaban.")
                    });
                }
            }
            Cmd::RotateBy(d) => self.apply_transformed(arrange::rotate_group(&shapes, d)),
            Cmd::RotateTo(a) => self.apply_transformed(arrange::rotate_each_to(&shapes, a)),
            Cmd::Duplicate(side) => self.add_new(replicate::adjacent_copies(&shapes, side, 1)),
            Cmd::Mirror(side) => {
                let (horizontal, axis) = match side {
                    Side::Left => (true, b.x0),
                    Side::Right => (true, b.x1),
                    Side::Up => (false, b.y0),
                    Side::Down => (false, b.y1),
                };
                let copies = crate::text::paths_for_ops(&shapes)
                    .iter()
                    .map(|s| {
                        let mut m = oc_core::edit::mirror_shape(s, horizontal, axis);
                        m.id = 0;
                        m
                    })
                    .collect();
                self.add_new(copies);
            }
            Cmd::RotateCopies(n) => self.add_new(replicate::rotate_copies(&shapes, n)),
            Cmd::Row(side, n) => self.add_new(replicate::adjacent_copies(&shapes, side, n - 1)),
            Cmd::FillPage => {
                let m = replicate::FILL_MARGIN_MM;
                let area = Rect::new(m, m, page.x1 - m, page.y1 - m);
                let (first, copies) =
                    replicate::fill_area(&shapes, area, replicate::FILL_MIN_GAP_MM);
                self.commit();
                for s in first {
                    if let Some(t) = self.doc.shape_mut(s.id) {
                        *t = s;
                    }
                }
                for c in copies {
                    self.doc.add_shape(c);
                }
            }
            Cmd::Bool(op) => {
                let paths = crate::text::paths_for_ops(
                    &shapes
                        .iter()
                        .filter(|s| !s.is_image())
                        .cloned()
                        .collect::<Vec<_>>(),
                );
                let ids: Vec<u64> = paths.iter().map(|s| s.id).collect();
                if let Some(out) = ops::boolean(&paths, op).filter(|o| !o.is_empty()) {
                    self.replace_shapes(&ids, out);
                }
            }
            Cmd::BreakApart => {
                let ids: Vec<u64> = shapes.iter().map(|s| s.id).collect();
                let out = crate::text::paths_for_ops(&shapes)
                    .iter()
                    .flat_map(ops::break_apart)
                    .collect();
                self.replace_shapes(&ids, out);
            }
            Cmd::ReleaseCompound => {
                let ids: Vec<u64> = shapes.iter().map(|s| s.id).collect();
                let out = crate::text::paths_for_ops(&shapes)
                    .iter()
                    .flat_map(ops::separate)
                    .collect();
                self.replace_shapes(&ids, out);
            }
            Cmd::MakeCompound => {
                let paths = crate::text::paths_for_ops(&shapes);
                let ids: Vec<u64> = paths.iter().map(|s| s.id).collect();
                if let Some(out) = ops::combine(&paths) {
                    self.replace_shapes(&ids, vec![out]);
                }
            }
            Cmd::Group => {
                self.commit();
                self.doc.group_selected();
            }
            Cmd::Ungroup => {
                self.commit();
                self.doc.ungroup_selected();
            }
            Cmd::ToPath => self.point_convert_to_path(),
            Cmd::Simplify => self.point_simplify(),
        }
    }
}

/// Entradas del menú: `None` = separador.
type Entry = Option<(&'static str, &'static str, Cmd)>;

fn entries(list: &[Entry], app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    for e in list {
        match e {
            None => {
                ui.separator();
            }
            Some((label, keys, cmd)) => {
                let enabled = app.object_enabled(*cmd);
                if ui
                    .add_enabled(enabled, egui::Button::new(*label).shortcut_text(*keys))
                    .clicked()
                {
                    app.object_run(*cmd);
                    ui.close();
                }
            }
        }
    }
}

pub fn menu(app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    ui.menu_button("Espejo", |ui| {
        entries(
            &[
                Some(("Voltear horizontalmente", "", Cmd::FlipH)),
                Some(("Voltear verticalmente", "", Cmd::FlipV)),
            ],
            app,
            ui,
        );
    });
    ui.menu_button("Organizar", |ui| {
        entries(
            &[
                Some(("Traer al frente", "⇧⌘]", Cmd::Reorder(i32::MAX))),
                Some(("Enviar detrás", "⇧⌘[", Cmd::Reorder(i32::MIN))),
                Some(("Hacia delante", "⌘]", Cmd::Reorder(1))),
                Some(("Hacia atrás", "⌘[", Cmd::Reorder(-1))),
            ],
            app,
            ui,
        );
    });
    ui.menu_button("Alinear", |ui| {
        entries(
            &[
                Some(("Alinear al centro hor+vert", "", Cmd::Align(Align::Center))),
                None,
                Some(("Alinear a la izquierda", "", Cmd::Align(Align::Left))),
                Some(("Alinear al centro", "", Cmd::Align(Align::HCenter))),
                Some(("Alinear a la derecha", "", Cmd::Align(Align::Right))),
                None,
                Some(("Alinear arriba", "", Cmd::Align(Align::Top))),
                Some(("Alinear al centro vert", "", Cmd::Align(Align::VCenter))),
                Some(("Alinear abajo", "", Cmd::Align(Align::Bottom))),
                None,
                Some((
                    "Distribuir horizontalmente",
                    "",
                    Cmd::Distribute(Axis::Horizontal),
                )),
                Some((
                    "Distribuir verticalmente",
                    "",
                    Cmd::Distribute(Axis::Vertical),
                )),
                None,
                Some(("Centrar en página", "", Cmd::CenterOnPage)),
                Some(("Acomodar en la página", "", Cmd::Pack)),
            ],
            app,
            ui,
        );
    });
    ui.menu_button("Rotar", |ui| {
        entries(
            &[
                Some(("Rotar 45° en sentido horario", "", Cmd::RotateBy(45.0))),
                Some(("Rotar 90° en sentido horario", "", Cmd::RotateBy(90.0))),
                Some(("Rotar 180°", "", Cmd::RotateBy(180.0))),
                Some(("Rotar 45° en sentido antihorario", "", Cmd::RotateBy(-45.0))),
                Some(("Rotar 90° en sentido antihorario", "", Cmd::RotateBy(-90.0))),
                None,
                Some(("Rotar a 0°", "", Cmd::RotateTo(0.0))),
                Some(("Rotar a 90°", "", Cmd::RotateTo(90.0))),
                Some(("Rotar a 180°", "", Cmd::RotateTo(180.0))),
                Some(("Rotar a 270°", "", Cmd::RotateTo(270.0))),
            ],
            app,
            ui,
        );
    });
    ui.menu_button("Replicar", |ui| {
        entries(
            &[
                Some(("Duplicar a la izquierda", "", Cmd::Duplicate(Side::Left))),
                Some(("Duplicar a la derecha", "", Cmd::Duplicate(Side::Right))),
                Some(("Duplicar arriba", "", Cmd::Duplicate(Side::Up))),
                Some(("Duplicar abajo", "", Cmd::Duplicate(Side::Down))),
                None,
                Some(("Reflejar a la izquierda", "", Cmd::Mirror(Side::Left))),
                Some(("Reflejar a la derecha", "", Cmd::Mirror(Side::Right))),
                Some(("Reflejar arriba", "", Cmd::Mirror(Side::Up))),
                Some(("Reflejar abajo", "", Cmd::Mirror(Side::Down))),
                None,
                Some(("Rotar una copia", "", Cmd::RotateCopies(2))),
                Some(("Rotar dos copias", "", Cmd::RotateCopies(3))),
                Some(("Rotar tres copias", "", Cmd::RotateCopies(4))),
                Some(("Rotar cinco copias", "", Cmd::RotateCopies(6))),
                None,
                Some(("Hilera de tres", "", Cmd::Row(Side::Right, 3))),
                Some(("Fila de cuatro", "", Cmd::Row(Side::Right, 4))),
                Some(("Columna de tres", "", Cmd::Row(Side::Down, 3))),
                Some(("Columna de cuatro", "", Cmd::Row(Side::Down, 4))),
                Some(("Llenar página", "⇧⌘F", Cmd::FillPage)),
            ],
            app,
            ui,
        );
    });
    ui.menu_button("Modificar", |ui| {
        entries(
            &[
                Some(("Fusionar", "⇧⌘W", Cmd::Bool(BoolOp::Weld))),
                Some(("Interseccionar", "", Cmd::Bool(BoolOp::Intersect))),
                Some(("Sustraer", "", Cmd::Bool(BoolOp::SubtractFront))),
                Some(("Recortar", "", Cmd::Bool(BoolOp::Crop))),
                Some(("Sustraer todo", "", Cmd::Bool(BoolOp::Subtract))),
                Some(("Dividir", "", Cmd::Bool(BoolOp::Divide))),
                None,
                Some(("Separar trazos", "", Cmd::BreakApart)),
            ],
            app,
            ui,
        );
    });
    ui.separator();
    entries(
        &[
            Some(("Agrupar", "⌘G", Cmd::Group)),
            Some(("Desagrupar", "⇧⌘G", Cmd::Ungroup)),
            None,
            Some(("Convertir en trazado", "", Cmd::ToPath)),
            Some(("Crear trazado compuesto", "⌘E", Cmd::MakeCompound)),
            Some(("Soltar trazado compuesto", "⇧⌘E", Cmd::ReleaseCompound)),
            None,
            Some(("Simplificar", "⌘J", Cmd::Simplify)),
        ],
        app,
        ui,
    );
    ui.separator();
    if ui.button("Insertar código QR…").clicked() {
        app.insert_dialog = Some(crate::insert::InsertDialog::Qr {
            text: String::new(),
            size_mm: 30.0,
        });
        ui.close();
    }
    if ui.button("Insertar plantilla…").clicked() {
        app.insert_dialog = Some(crate::insert::InsertDialog::Template {
            kind: 0,
            dims: crate::insert::template_defaults(0),
        });
        ui.close();
    }
}
