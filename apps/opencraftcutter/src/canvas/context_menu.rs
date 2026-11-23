//! Menús contextuales del lienzo (clic derecho): solo muestran lo que se puede hacer con lo que
//! hay bajo el puntero o seleccionado. Las acciones son las mismas de los menús Editar y Objeto
//! y del panel Editar puntos.

use super::point_edit::{self, Hit};
use super::viewport_actions;
use crate::app::{OpenCraftCutterApp, Panel, Tool, Workspace};
use crate::object_menu::Cmd;
use crate::panels::edit_points::{self, Action};
use egui::{Response, Ui};
use kurbo::Point;
use oc_core::arrange::{Align, Axis};
use oc_core::ops::BoolOp;

/// Dónde se abrió el menú contextual.
#[derive(Debug, Clone, Copy)]
pub struct ContextTarget {
    /// Punto del documento (mm) bajo el puntero.
    pub at: Point,
    /// En Editar puntos: el nodo, manejador o tramo bajo el puntero.
    pub hit: Option<Hit>,
}

/// ¿Tiene el lienzo menú contextual ahora? No mientras se escribe un texto ni fuera de Diseño.
fn available(app: &OpenCraftCutterApp) -> bool {
    app.workspace == Workspace::Design && app.canvas.editing_text.is_none()
}

/// Clic derecho: antes de abrir el menú, la selección pasa a lo que hay bajo el puntero (si no
/// formaba ya parte de ella), como en cualquier editor.
pub fn secondary_click(app: &mut OpenCraftCutterApp, pos: egui::Pos2) {
    if !available(app) {
        return;
    }
    let at = app.canvas.viewport.to_world(pos);
    let mut target = ContextTarget { at, hit: None };
    app.canvas.drag = None;
    if app.canvas.polygon.is_some() {
        app.canvas.context = Some(target);
        return;
    }
    let under = super::tools::shape_at(app, at);
    if app.tool == Tool::EditPoints {
        target.hit = point_edit::hit(app, pos);
        match target.hit {
            Some(Hit::Node(si, ni) | Hit::Handle(si, ni, _)) => {
                if !app.selected_nodes().contains(&(si, ni)) {
                    app.point_select(si, ni, false);
                }
            }
            Some(Hit::Segment(..)) => {}
            None => {
                // Sobre otra forma: pasa a editarse esa. En vacío se conserva lo que hay.
                let editing = app.canvas.point_edit.map(|pe| pe.shape);
                if let Some(id) = under.filter(|id| Some(*id) != editing) {
                    point_edit::begin(app, id);
                }
            }
        }
    } else {
        match under {
            Some(id) if !app.doc.is_selected(id) => app.doc.selected = vec![id],
            Some(_) => {}
            None => app.doc.selected.clear(),
        }
    }
    app.canvas.context = Some(target);
}

/// Menú contextual del lienzo.
pub fn show(app: &mut OpenCraftCutterApp, response: &Response) {
    if !available(app) {
        return;
    }
    response.context_menu(|ui| {
        ui.set_min_width(210.0);
        let target = app.canvas.context.unwrap_or(ContextTarget {
            at: app.canvas.cursor_mm.unwrap_or(Point::ZERO),
            hit: None,
        });
        if app.canvas.polygon.is_some() {
            polygon_menu(app, ui);
        } else if app.tool == Tool::EditPoints && app.canvas.point_edit.is_some() {
            points_menu(app, ui, target);
        } else if app.doc.selected.is_empty() {
            empty_menu(app, ui, target);
        } else {
            selection_menu(app, ui, target);
        }
    });
}

fn item(ui: &mut Ui, label: &str, keys: &str) -> bool {
    let clicked = ui
        .add(egui::Button::new(label).shortcut_text(keys))
        .clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// Entrada de una orden del menú Objeto; no aparece si ahora no se puede ejecutar.
fn cmd(app: &mut OpenCraftCutterApp, ui: &mut Ui, label: &str, keys: &str, c: Cmd) {
    if app.object_enabled(c) && item(ui, label, keys) {
        app.object_run(c);
    }
}

/// Submenú con órdenes del menú Objeto; no aparece si ninguna se puede ejecutar. `None` es un
/// separador.
fn submenu(
    app: &mut OpenCraftCutterApp,
    ui: &mut Ui,
    title: &str,
    entries: &[Option<(&str, &str, Cmd)>],
) {
    if !entries.iter().flatten().any(|e| app.object_enabled(e.2)) {
        return;
    }
    ui.menu_button(title, |ui| {
        let mut pending_separator = false;
        let mut any = false;
        for e in entries {
            match e {
                None => pending_separator = any,
                Some((label, keys, c)) => {
                    if !app.object_enabled(*c) {
                        continue;
                    }
                    if pending_separator {
                        ui.separator();
                        pending_separator = false;
                    }
                    any = true;
                    if item(ui, label, keys) {
                        app.object_run(*c);
                    }
                }
            }
        }
    });
}

/// Pega el portapapeles (formas copiadas aquí o lo que traiga el del sistema) centrado en `at`.
pub fn paste_at(app: &mut OpenCraftCutterApp, ctx: &egui::Context, at: Point) {
    let before = app.doc.selected.clone();
    let count = app.doc.shapes.len();
    app.paste_system(ctx);
    // Sin formas nuevas (nada que pegar, o un documento que se abrió en otra pestaña) no hay
    // nada que colocar.
    if app.doc.shapes.len() <= count || app.doc.selected == before {
        return;
    }
    let shapes: Vec<oc_core::Shape> = app.doc.selected_shapes().into_iter().cloned().collect();
    for moved in oc_core::arrange::center_group_on(&shapes, at) {
        if let Some(s) = app.doc.shape_mut(moved.id) {
            *s = moved;
        }
    }
}

/// Submenú con todas las formas que hay bajo el punto donde se abrió el menú, de arriba abajo,
/// para llegar a las que quedan tapadas. Solo aparece si hay más de una.
fn shapes_under_menu(app: &mut OpenCraftCutterApp, ui: &mut Ui, at: Point) {
    let under = super::tools::shapes_at(app, at);
    if under.len() < 2 {
        return;
    }
    let editing = (app.tool == Tool::EditPoints)
        .then(|| app.canvas.point_edit.map(|pe| pe.shape))
        .flatten();
    let title = if app.tool == Tool::EditPoints {
        "Editar otra forma de aquí"
    } else {
        "Seleccionar debajo"
    };
    ui.menu_button(title, |ui| {
        for id in under {
            let Some((name, kind)) = app.doc.shape(id).map(|s| (s.name.clone(), s.kind_name()))
            else {
                continue;
            };
            let current = if app.tool == Tool::EditPoints {
                editing == Some(id)
            } else {
                app.doc.is_selected(id)
            };
            let mark = if current { "✔ " } else { "" };
            if item(ui, &format!("{mark}{name}"), kind) {
                if app.tool == Tool::EditPoints {
                    point_edit::begin(app, id);
                } else {
                    app.doc.selected = app.doc.with_group(id);
                }
            }
        }
    });
}

/// Órdenes de organizar, alinear, voltear y rotar (comunes al lienzo y al panel Capas).
pub fn arrange_menus(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    submenu(
        app,
        ui,
        "Organizar",
        &[
            Some(("Traer al frente", "⇧⌘]", Cmd::Reorder(i32::MAX))),
            Some(("Hacia delante", "⌘]", Cmd::Reorder(1))),
            Some(("Hacia atrás", "⌘[", Cmd::Reorder(-1))),
            Some(("Enviar detrás", "⇧⌘[", Cmd::Reorder(i32::MIN))),
        ],
    );
    // Con una sola forma, alinear es respecto a la página.
    let title = if app.doc.selected.len() >= 2 {
        "Alinear"
    } else {
        "Alinear en la página"
    };
    submenu(
        app,
        ui,
        title,
        &[
            Some(("A la izquierda", "", Cmd::Align(Align::Left))),
            Some(("Al centro", "", Cmd::Align(Align::HCenter))),
            Some(("A la derecha", "", Cmd::Align(Align::Right))),
            None,
            Some(("Arriba", "", Cmd::Align(Align::Top))),
            Some(("Al centro vertical", "", Cmd::Align(Align::VCenter))),
            Some(("Abajo", "", Cmd::Align(Align::Bottom))),
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
            Some(("Centrar en la página", "", Cmd::CenterOnPage)),
        ],
    );
    submenu(
        app,
        ui,
        "Voltear y rotar",
        &[
            Some(("Voltear horizontalmente", "", Cmd::FlipH)),
            Some(("Voltear verticalmente", "", Cmd::FlipV)),
            None,
            Some(("Rotar 90° en sentido horario", "", Cmd::RotateBy(90.0))),
            Some(("Rotar 90° en sentido antihorario", "", Cmd::RotateBy(-90.0))),
            Some(("Rotar 180°", "", Cmd::RotateBy(180.0))),
        ],
    );
}

/// Bloquear / desbloquear y ocultar la selección (comunes al lienzo y al panel Capas).
pub fn lock_and_hide(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let (any_locked, any_unlocked) = {
        let sel = app.doc.selected_shapes();
        (sel.iter().any(|s| s.locked), sel.iter().any(|s| !s.locked))
    };
    let ids = app.doc.selected.clone();
    if any_unlocked && item(ui, "Bloquear", "") {
        app.commit();
        for id in &ids {
            if let Some(s) = app.doc.shape_mut(*id) {
                s.locked = true;
            }
        }
    }
    if any_locked && item(ui, "Desbloquear", "") {
        app.commit();
        for id in &ids {
            if let Some(s) = app.doc.shape_mut(*id) {
                s.locked = false;
            }
        }
    }
    let (any_hidden, any_visible) = {
        let sel = app.doc.selected_shapes();
        (
            sel.iter().any(|s| !s.visible),
            sel.iter().any(|s| s.visible),
        )
    };
    if any_hidden && item(ui, "Mostrar", "") {
        app.commit();
        for id in &ids {
            if let Some(s) = app.doc.shape_mut(*id) {
                s.visible = true;
            }
        }
    }
    if any_visible && item(ui, "Ocultar", "") {
        app.commit();
        for id in &ids {
            if let Some(s) = app.doc.shape_mut(*id) {
                s.visible = false;
            }
        }
    }
}

/// Con una o varias formas seleccionadas.
fn selection_menu(app: &mut OpenCraftCutterApp, ui: &mut Ui, target: ContextTarget) {
    let ctx = ui.ctx().clone();
    let all_locked = app.doc.selected_shapes().iter().all(|s| s.locked);
    if !all_locked && item(ui, "Cortar", "⌘X") {
        app.cut_selection(&ctx);
    }
    if item(ui, "Copiar", "⌘C") {
        app.copy_selection(&ctx);
    }
    if app.can_paste() && item(ui, "Pegar aquí", "") {
        paste_at(app, &ctx, target.at);
    }
    if item(ui, "Duplicar", "⌘D") {
        app.commit();
        app.doc.duplicate_selected();
    }
    if !all_locked && item(ui, "Borrar", "⌫") {
        app.delete_selected();
    }
    ui.separator();
    // Una sola forma: entrar a editarla.
    let single = match app.doc.selected_shapes().as_slice() {
        [s] if !s.locked => Some((s.id, s.is_text(), s.is_image(), s.punch_mm.is_some())),
        _ => None,
    };
    if let Some((id, is_text, is_image, is_punch)) = single {
        if is_text && item(ui, "Editar texto", "") {
            app.canvas.editing_text = Some(id);
            app.panel = Some(Panel::Text);
        }
        if !is_text && !is_image && !is_punch && item(ui, "Editar puntos", "A") {
            app.tool = Tool::EditPoints;
        }
    }
    cmd(app, ui, "Convertir en trazado", "", Cmd::ToPath);
    if !all_locked {
        arrange_menus(app, ui);
        ui.separator();
        cmd(app, ui, "Agrupar", "⌘G", Cmd::Group);
        cmd(app, ui, "Desagrupar", "⇧⌘G", Cmd::Ungroup);
        if app.doc.selected.len() >= 2 {
            submenu(
                app,
                ui,
                "Modificar",
                &[
                    Some(("Fusionar", "⇧⌘W", Cmd::Bool(BoolOp::Weld))),
                    Some(("Interseccionar", "", Cmd::Bool(BoolOp::Intersect))),
                    Some(("Sustraer", "", Cmd::Bool(BoolOp::SubtractFront))),
                    Some(("Recortar", "", Cmd::Bool(BoolOp::Crop))),
                    Some(("Sustraer todo", "", Cmd::Bool(BoolOp::Subtract))),
                    Some(("Dividir", "", Cmd::Bool(BoolOp::Divide))),
                ],
            );
        }
        cmd(app, ui, "Crear trazado compuesto", "⌘E", Cmd::MakeCompound);
        cmd(
            app,
            ui,
            "Soltar trazado compuesto",
            "⇧⌘E",
            Cmd::ReleaseCompound,
        );
        cmd(app, ui, "Unir trazados", "⇧⌘J", Cmd::JoinPaths);
        ui.separator();
    }
    lock_and_hide(app, ui);
    ui.separator();
    shapes_under_menu(app, ui, target.at);
    if item(ui, "Zoom a la selección", "⌘2") {
        viewport_actions::zoom_to_selection(app);
    }
}

/// En vacío, sin nada seleccionado.
fn empty_menu(app: &mut OpenCraftCutterApp, ui: &mut Ui, target: ContextTarget) {
    let ctx = ui.ctx().clone();
    if app.can_paste() {
        if item(ui, "Pegar aquí", "") {
            paste_at(app, &ctx, target.at);
        }
        if !app.clipboard.is_empty() && item(ui, "Pegar delante", "⌘F") {
            app.paste(true);
        }
        ui.separator();
    }
    if !app.doc.shapes.is_empty() && item(ui, "Seleccionar todo", "⌘A") {
        app.doc.select_all();
    }
    let (any_locked, any_hidden) = (
        app.doc.shapes.iter().any(|s| s.locked),
        app.doc.shapes.iter().any(|s| !s.visible),
    );
    if any_locked && item(ui, "Desbloquear todo", "") {
        app.commit();
        for s in &mut app.doc.shapes {
            s.locked = false;
        }
    }
    if any_hidden && item(ui, "Mostrar todo", "") {
        app.commit();
        for s in &mut app.doc.shapes {
            s.visible = true;
        }
    }
    ui.separator();
    if item(ui, "Ajustar a la página", "⌘0") {
        viewport_actions::fit_to_page(app);
    }
    let page = &mut app.doc.page;
    let mut changed = ui
        .checkbox(&mut page.show_grid, "Mostrar retícula")
        .changed();
    changed |= ui
        .checkbox(&mut page.snap_to_grid, "Ajustar a retícula")
        .changed();
    changed |= ui
        .checkbox(&mut page.smart_snap, "Ajuste inteligente")
        .changed();
    if changed {
        app.dirty = true;
        ui.close();
    }
}

/// Mientras se dibuja un polígono.
fn polygon_menu(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let n = app.canvas.polygon.as_ref().map_or(0, Vec::len);
    if n >= 3 && item(ui, "Cerrar polígono", "") {
        super::tools::finish_polygon(app, true);
    }
    if n >= 2 && item(ui, "Terminar sin cerrar", "") {
        super::tools::finish_polygon(app, false);
    }
    if item(ui, "Cancelar", "Esc") {
        app.canvas.polygon = None;
    }
}

/// Acción del panel Editar puntos; no aparece si ahora no se puede ejecutar. Las de tipo de
/// punto llevan una marca cuando es el tipo del nodo seleccionado.
fn point_action(app: &mut OpenCraftCutterApp, ui: &mut Ui, a: Action) {
    let (enabled, active) = edit_points::state(app, a);
    if !enabled {
        return;
    }
    let label = if active {
        format!("✔ {}", a.label())
    } else {
        a.label().to_string()
    };
    if item(ui, &label, "") {
        edit_points::run(app, a);
    }
}

/// En Editar puntos: sobre un tramo, sobre un nodo o en vacío.
fn points_menu(app: &mut OpenCraftCutterApp, ui: &mut Ui, target: ContextTarget) {
    if let Some(Hit::Segment(si, seg, t)) = target.hit {
        let curve = point_edit::edited_paths(app)
            .and_then(|(_, p)| {
                p.get(si)
                    .filter(|np| seg < np.segment_count())
                    .map(|np| np.segment(seg).1.is_some())
            })
            .unwrap_or(false);
        if item(ui, "Añadir punto aquí", "") {
            app.point_insert(si, seg, t);
        }
        if item(ui, "Añadir punto medio", "") {
            app.point_insert(si, seg, 0.5);
        }
        if curve && item(ui, "Convertir en recta", "") {
            app.point_segment_set(si, seg, false);
        }
        if !curve && item(ui, "Convertir en curva", "") {
            app.point_segment_set(si, seg, true);
        }
        ui.separator();
    } else if !app.selected_nodes().is_empty() {
        for a in [Action::Delete, Action::Break, Action::Join] {
            point_action(app, ui, a);
        }
        ui.separator();
        for a in [Action::Corner, Action::Smooth, Action::Symmetric] {
            point_action(app, ui, a);
        }
        let n = app.selected_nodes().len();
        if n >= 2 {
            ui.separator();
            ui.menu_button("Alinear puntos", |ui| {
                for (label, mode) in [
                    ("A la izquierda", Align::Left),
                    ("Al centro", Align::HCenter),
                    ("A la derecha", Align::Right),
                    ("Arriba", Align::Top),
                    ("Al centro vertical", Align::VCenter),
                    ("Abajo", Align::Bottom),
                ] {
                    if item(ui, label, "") {
                        app.point_align(mode);
                    }
                }
                if n >= 3 {
                    ui.separator();
                    if item(ui, "Distribuir horizontalmente", "") {
                        app.point_distribute(Axis::Horizontal);
                    }
                    if item(ui, "Distribuir verticalmente", "") {
                        app.point_distribute(Axis::Vertical);
                    }
                }
            });
        }
        ui.separator();
        for a in [
            Action::ToLine,
            Action::ToCurve,
            Action::Midpoint,
            Action::Reverse,
            Action::Close,
        ] {
            point_action(app, ui, a);
        }
        ui.separator();
    }
    shapes_under_menu(app, ui, target.at);
    if item(ui, "Seleccionar todos los puntos", "⌘A") {
        app.point_select_all();
    }
    point_action(app, ui, Action::Simplify);
    ui.separator();
    if item(ui, "Salir de Editar puntos", "Esc") {
        app.tool = Tool::Select;
    }
}
