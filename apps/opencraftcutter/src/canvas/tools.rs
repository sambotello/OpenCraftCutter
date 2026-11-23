//! Interpretación de la entrada del puntero según la herramienta activa.

use super::overlay::{HANDLE_PX, handle_positions, selection_frame};
use super::{Drag, viewport_actions};
use crate::app::{OpenCraftCutterApp, Panel, Tool};
use crate::text::{DEFAULT_FONT_FAMILY, TextStyle, measure};
use egui::{CursorIcon, Key, PointerButton, Response, Ui};
use kurbo::{Point, Rect, Vec2};
use oc_core::edit::{
    Handle, angle_deg, hit_test, rotate_shapes, scale_group_by_handle, scale_shape_by_handle,
    selection_bounds, shape_bounds,
};
use oc_core::shape::{Shape, ShapeKind, TextAlign};

const DRAG_THRESHOLD_PX: f64 = 3.0;

fn snap(app: &OpenCraftCutterApp, p: Point) -> Point {
    let page = &app.doc.page;
    if !page.snap_to_grid {
        return p;
    }
    let step = page.grid_spacing_mm / page.grid_divisions.max(1) as f64;
    Point::new((p.x / step).round() * step, (p.y / step).round() * step)
}

fn new_shape(app: &mut OpenCraftCutterApp, name: &str, rect: Rect, kind: ShapeKind) -> Shape {
    let n = app
        .doc
        .shapes
        .iter()
        .filter(|s| s.name.starts_with(name))
        .count()
        + 1;
    let mut s = Shape::new(0, format!("{name} {n}"), rect, kind);
    s.fill = app.defaults.fill;
    s.stroke = app.defaults.stroke;
    s
}

fn path_kind(points: &[Point], closed: bool) -> (Rect, ShapeKind) {
    let (rect, subpaths) = Shape::normalize_subpaths(&[(points.to_vec(), closed)]);
    (
        rect,
        ShapeKind::Path {
            subpaths,
            fill_rule: Default::default(),
        },
    )
}

/// Forma bajo el puntero (la de más arriba en la pila).
pub(super) fn shape_at(app: &OpenCraftCutterApp, p: Point) -> Option<u64> {
    shapes_at(app, p).first().copied()
}

/// Todas las formas bajo el puntero, de la de más arriba a la de más abajo.
pub(super) fn shapes_at(app: &OpenCraftCutterApp, p: Point) -> Vec<u64> {
    let tol = 6.0 / app.canvas.viewport.scale;
    app.doc
        .shapes
        .iter()
        .rev()
        .filter(|s| hit_test(s, p, tol))
        .map(|s| s.id)
        .collect()
}

/// La forma que está justo debajo de la seleccionada en el punto `p` (Alt + clic). Desde la
/// de más abajo se vuelve a la de arriba; si la selección no está bajo el puntero, la de arriba.
pub(super) fn shape_below_selection(app: &OpenCraftCutterApp, p: Point) -> Option<u64> {
    let under = shapes_at(app, p);
    let current = under.iter().rposition(|id| app.doc.is_selected(*id));
    match current {
        Some(i) => under.get((i + 1) % under.len()).copied(),
        None => under.first().copied(),
    }
}

/// Asa bajo el puntero (en px de pantalla).
fn handle_at(app: &OpenCraftCutterApp, pos: egui::Pos2) -> Option<Option<Handle>> {
    let frame = selection_frame(app)?;
    if frame.locked {
        return None;
    }
    let (handles, rotate) = handle_positions(&frame, &app.canvas.viewport);
    // Área de clic algo mayor que el asa dibujada.
    let r = HANDLE_PX + 3.0;
    if rotate.distance(pos) <= r + 2.0 {
        return Some(None);
    }
    handles
        .into_iter()
        .filter(|(h, _)| super::overlay::handle_usable(&frame, &app.canvas.viewport, *h))
        .find(|(_, hp)| (hp.x - pos.x).abs() <= r && (hp.y - pos.y).abs() <= r)
        .map(|(h, _)| Some(h))
}

pub fn finish_polygon(app: &mut OpenCraftCutterApp, close: bool) {
    let Some(poly) = app.canvas.polygon.take() else {
        return;
    };
    if poly.len() >= 2 {
        let (rect, kind) = path_kind(&poly, close && poly.len() >= 3);
        let s = new_shape(app, "Polígono", rect, kind);
        app.commit();
        let id = app.doc.add_shape(s);
        app.doc.selected = vec![id];
    }
}

/// Distancia (px) a la que un clic sobre el primer nodo cierra el trazado de la Pluma.
pub const PEN_CLOSE_PX: f64 = 8.0;

/// Termina el trazado de la Pluma: lo añade al documento como una forma y la selecciona. Con
/// menos de dos puntos no queda nada.
pub fn finish_pen(app: &mut OpenCraftCutterApp, close: bool) {
    app.canvas.drag = None;
    let Some(mut nodes) = app.canvas.pen.take() else {
        return;
    };
    // Dos clics en el mismo sitio (un doble clic para terminar) no son dos nodos.
    nodes.dedup_by(|b, a| a.p.distance(b.p) < 1e-6);
    let closed = close && nodes.len() >= 3;
    if nodes.len() < 2 {
        return;
    }
    if !closed {
        // En un trazado abierto esos manejadores no dibujan nada.
        nodes[0].h_in = None;
        if let Some(last) = nodes.last_mut() {
            last.h_out = None;
        }
    }
    let kind = ShapeKind::Path {
        subpaths: Vec::new(),
        fill_rule: Default::default(),
    };
    let mut s = new_shape(app, "Trazo", Rect::ZERO, kind);
    s.set_world_node_paths(&[oc_core::nodes::NodePath { nodes, closed }]);
    app.commit();
    let id = app.doc.add_shape(s);
    app.doc.selected = vec![id];
}

fn create_text(app: &mut OpenCraftCutterApp, _ctx: &egui::Context, at: Point) {
    let kind = ShapeKind::Text {
        text: String::new(),
        font_family: DEFAULT_FONT_FAMILY.into(),
        font_size_pt: 36.0,
        align: TextAlign::Left,
        bold: false,
        italic: false,
        line_height: 1.2,
        fx: Default::default(),
    };
    let (w, h) = TextStyle::from_kind(&kind)
        .map(|st| measure(&st))
        .unwrap_or((10.0, 10.0));
    let mut s = new_shape(
        app,
        "Texto",
        Rect::new(at.x, at.y, at.x + w, at.y + h),
        kind,
    );
    // Como la referencia: el texto nuevo no lleva relleno, solo la línea de corte.
    s.fill = None;
    app.commit();
    let id = app.doc.add_shape(s);
    app.doc.selected = vec![id];
    app.canvas.editing_text = Some(id);
    app.panel = Some(Panel::Text);
}

fn placing_test(app: &OpenCraftCutterApp) -> bool {
    app.canvas.send.placing_test && app.workspace == crate::app::Workspace::Send
}

pub fn handle_input(app: &mut OpenCraftCutterApp, ui: &mut Ui, response: &Response) {
    let ctx = ui.ctx().clone();
    let vp = app.canvas.viewport;
    app.canvas.space_held = ctx.input(|i| i.key_down(Key::Space))
        && app.canvas.editing_text.is_none()
        && !ctx.egui_wants_keyboard_input();
    let tool = if app.canvas.space_held {
        Tool::Pan
    } else {
        app.tool
    };

    // Cursor en mm para reglas y barra de estado
    let hover = response.hover_pos().or(response.interact_pointer_pos());
    app.canvas.cursor_mm = hover.map(|p| vp.to_world(p));

    // Rueda: ⌘/Ctrl (o pinch) = zoom; resto = desplazamiento.
    if response.hovered() {
        let (scroll, zoom_delta, mods) =
            ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers));
        if let Some(pos) = hover {
            if zoom_delta != 1.0 {
                app.canvas.viewport.zoom_at(zoom_delta as f64, pos);
            } else if mods.command || mods.ctrl {
                if scroll.y != 0.0 {
                    app.canvas
                        .viewport
                        .zoom_at((-scroll.y as f64 * 0.01).exp(), pos);
                }
            } else if scroll != egui::Vec2::ZERO {
                app.canvas.viewport.pan(scroll);
            }
        }
    }

    // Cursor
    ui.output_mut(|o| {
        o.cursor_icon = match (&app.canvas.drag, tool) {
            (Some(Drag::Pan), _) => CursorIcon::Grabbing,
            (_, Tool::Pan) => CursorIcon::Grab,
            (_, Tool::Text) => CursorIcon::Text,
            _ if app.canvas.eyedropper.is_some()
                || app.canvas.trace_area_select.is_some()
                || placing_test(app) =>
            {
                CursorIcon::Crosshair
            }
            (_, Tool::ZoomDrag) => CursorIcon::ZoomIn,
            (_, Tool::Knife | Tool::Eraser) => CursorIcon::Crosshair,
            (_, Tool::EditPoints) => {
                if hover.is_some_and(|pos| super::point_edit::hit(app, pos).is_some()) {
                    CursorIcon::PointingHand
                } else {
                    CursorIcon::Default
                }
            }
            (
                _,
                Tool::Rect
                | Tool::Ellipse
                | Tool::Line
                | Tool::Polygon
                | Tool::Pen
                | Tool::Freehand,
            ) => CursorIcon::Crosshair,
            _ => {
                if let Some(pos) = hover {
                    match handle_at(app, pos) {
                        Some(None) => CursorIcon::Grab,
                        Some(Some(h)) => match h {
                            Handle::NW | Handle::SE => CursorIcon::ResizeNwSe,
                            Handle::NE | Handle::SW => CursorIcon::ResizeNeSw,
                            Handle::N | Handle::S => CursorIcon::ResizeVertical,
                            Handle::E | Handle::W => CursorIcon::ResizeHorizontal,
                        },
                        None => CursorIcon::Default,
                    }
                } else {
                    CursorIcon::Default
                }
            }
        };
    });

    if app.canvas.editing_text.is_some() {
        return;
    }

    // «Elegir sitio» de la forma de prueba: el clic la coloca; Esc cancela.
    if placing_test(app) {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            app.canvas.send.placing_test = false;
        } else if response.clicked_by(PointerButton::Primary) {
            if let Some(p) = app.canvas.cursor_mm {
                app.place_test_shape_at(p);
            }
        }
        if tool != Tool::Pan {
            return;
        }
    }

    let shift = ctx.input(|i| i.modifiers.shift);
    let alt = ctx.input(|i| i.modifiers.alt);

    // Doble clic: cerrar polígono o editar texto.
    if response.double_clicked() {
        // Pluma: el doble clic pone el último punto (lo hizo el primer clic) y termina.
        if app.canvas.pen.is_some() {
            finish_pen(app, false);
            return;
        }
        if tool == Tool::EditPoints {
            return;
        }
        if app.canvas.polygon.is_some() {
            finish_polygon(app, true);
            return;
        }
        if let Some(p) = app.canvas.cursor_mm {
            if let Some(id) = shape_at(app, p) {
                if let Some(s) = app.doc.shape(id) {
                    if s.is_text() && !s.locked {
                        app.doc.selected = vec![id];
                        app.canvas.editing_text = Some(id);
                        app.panel = Some(Panel::Text);
                        return;
                    }
                    // Como la referencia: doble clic en una forma entra en Editar puntos.
                    if tool == Tool::Select && !s.locked && !s.is_image() && s.punch_mm.is_none() {
                        app.doc.selected = vec![id];
                        app.canvas.drag = None;
                        app.tool = Tool::EditPoints;
                        return;
                    }
                }
            }
        }
    }

    // Cuentagotas: toma el relleno (o, si no tiene, el color de línea) de la forma bajo el puntero.
    if let Some(target) = app
        .canvas
        .eyedropper
        .filter(|_| response.clicked_by(PointerButton::Primary))
    {
        app.canvas.eyedropper = None;
        if let Some(p) = app.canvas.cursor_mm {
            if let Some(id) = shape_at(app, p) {
                if let Some(s) = app.doc.shape(id) {
                    match target {
                        super::EyedropperTarget::Fill => {
                            let c = s.fill.unwrap_or(s.stroke.color);
                            app.apply_fill(Some(c));
                        }
                        super::EyedropperTarget::Line => {
                            let c = s.stroke.color;
                            app.apply_stroke(|st| st.color = c);
                            app.remember_line_color(c);
                        }
                    }
                }
            }
        }
        return;
    }

    // Inicio de gesto
    let primary_down = response.drag_started_by(PointerButton::Primary)
        || response.clicked_by(PointerButton::Primary);
    let middle_down = response.drag_started_by(PointerButton::Middle);
    if app.canvas.drag.is_none() && (primary_down || middle_down) {
        // El gesto empieza donde se pulsó, no donde está el puntero al superar el umbral de
        // arrastre (si no, un arrastre rápido sale del asa antes de detectarla).
        let Some(pos) = ui
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos())
        else {
            return;
        };
        let p = vp.to_world(pos);
        let sp = snap(app, p);
        if middle_down || tool == Tool::Pan {
            app.canvas.drag = Some(Drag::Pan);
            return;
        }
        // Dibujar puntos de perforación: cada clic añade un orificio al trazado de puntos
        // seleccionado (o crea uno nuevo).
        if let Some(d) = app.canvas.punch_draw {
            let circle = (oc_core::special::circle(p, d), true);
            let target = app
                .doc
                .selected_shapes()
                .iter()
                .find(|s| s.punch_mm == Some(d))
                .map(|s| s.id);
            if let Some(id) = target {
                if let Some(s) = app.doc.shape(id).cloned() {
                    let mut subs = s.world_subpaths();
                    subs.push(circle);
                    let (rect, subpaths) = Shape::normalize_subpaths(&subs);
                    app.commit();
                    if let Some(m) = app.doc.shape_mut(id) {
                        m.rect = rect;
                        m.rotation = 0.0;
                        m.kind = ShapeKind::Path {
                            subpaths,
                            fill_rule: oc_core::FillRule::NonZero,
                        };
                    }
                }
            } else {
                app.commit();
                app.add_punch_shape(vec![circle], d);
            }
            return;
        }
        if let Some(image) = app.canvas.trace_area_select {
            app.canvas.drag = Some(Drag::TraceArea { image, start: p });
            app.canvas.trace_area = Some((image, Rect::from_points(p, p)));
            return;
        }
        match tool {
            Tool::Knife => {
                app.canvas.drag = Some(Drag::Knife { start: p, end: p });
            }
            Tool::Eraser => {
                app.canvas.drag = Some(Drag::Erase { path: vec![p] });
            }
            Tool::EditPoints => {
                use super::point_edit::Hit;
                let shape = app.canvas.point_edit.map(|pe| pe.shape).unwrap_or(0);
                match super::point_edit::hit(app, pos) {
                    Some(Hit::Node(si, ni)) => {
                        app.point_select(si, ni, shift);
                        app.canvas.drag = move_point_drag(app, shape, si, ni, p, false);
                    }
                    Some(Hit::Handle(si, ni, out)) => {
                        // Con Alt el manejador se mueve solo: el nodo pasa a ser un vértice.
                        let committed = alt && app.point_edit_break_handles(si, ni);
                        app.canvas.drag = Some(Drag::MoveHandle {
                            shape,
                            sub: si,
                            idx: ni,
                            out,
                            committed,
                        });
                    }
                    Some(Hit::Segment(si, seg, t)) => {
                        // Un clic en la línea añade un nodo, como la referencia.
                        app.point_insert(si, seg, t);
                        if let Some((si, ni)) = app.canvas.point_edit.and_then(|pe| pe.selected) {
                            app.canvas.point_multi.clear();
                            app.canvas.drag = move_point_drag(app, shape, si, ni, p, true);
                        }
                    }
                    None => {
                        let editing = app.canvas.point_edit.is_some();
                        match shape_at(app, p) {
                            Some(id) if Some(id) != app.canvas.point_edit.map(|pe| pe.shape) => {
                                super::point_edit::begin(app, id);
                            }
                            // En vacío o sobre la propia forma: recuadro de selección de nodos.
                            _ if editing => {
                                app.canvas.drag = Some(Drag::NodeMarquee {
                                    start: p,
                                    additive: shift,
                                });
                                app.canvas.marquee = Some(Rect::from_points(p, p));
                            }
                            _ => {
                                app.canvas.point_edit = None;
                                app.doc.selected.clear();
                            }
                        }
                    }
                }
            }
            Tool::Select | Tool::Note => {
                // Con Alt el clic es «seleccionar debajo»: no se agarran las asas, que pueden
                // quedar justo sobre la forma que se busca.
                let handle = if alt { None } else { handle_at(app, pos) };
                match handle {
                    Some(None) => {
                        let originals: Vec<Shape> =
                            app.doc.selected_shapes().into_iter().cloned().collect();
                        let center = if originals.len() == 1 {
                            originals[0].center()
                        } else {
                            selection_bounds(originals.iter())
                                .map(|b| b.center())
                                .unwrap_or(p)
                        };
                        app.canvas.drag = Some(Drag::Rotate {
                            originals,
                            center,
                            start_angle: angle_deg(center, p),
                            committed: false,
                        });
                        return;
                    }
                    Some(Some(h)) => {
                        let originals: Vec<Shape> =
                            app.doc.selected_shapes().into_iter().cloned().collect();
                        let group = if originals.len() > 1 {
                            selection_bounds(originals.iter())
                        } else {
                            None
                        };
                        let grab = selection_frame(app)
                            .map(|f| {
                                let rot =
                                    oc_core::edit::rotation_about(f.rect.center(), f.rotation);
                                p - rot * h.position(f.rect)
                            })
                            .unwrap_or_default();
                        app.canvas.drag = Some(Drag::Scale {
                            handle: h,
                            grab,
                            originals,
                            group,
                            committed: false,
                        });
                        return;
                    }
                    None => {}
                }
                // Alt + clic: la forma de debajo de la seleccionada (para llegar a las tapadas).
                let picked = if alt {
                    shape_below_selection(app, p)
                } else {
                    shape_at(app, p)
                };
                if let Some(id) = picked {
                    // Un objeto agrupado se selecciona con todo su grupo.
                    let members = app.doc.with_group(id);
                    if alt {
                        app.doc.selected = members;
                    } else if shift {
                        for m in members {
                            app.doc.toggle_selected(m);
                        }
                    } else if !app.doc.is_selected(id) {
                        app.doc.selected = members;
                    }
                    let originals: Vec<Shape> = app
                        .doc
                        .selected_shapes()
                        .into_iter()
                        .filter(|s| !s.locked)
                        .cloned()
                        .collect();
                    app.canvas.drag = Some(Drag::Move {
                        start: p,
                        originals,
                        committed: false,
                    });
                } else {
                    if !shift {
                        app.doc.selected.clear();
                    }
                    app.canvas.drag = Some(Drag::Marquee {
                        start: p,
                        additive: shift,
                    });
                    app.canvas.marquee = Some(Rect::from_points(p, p));
                }
            }
            Tool::Rect | Tool::Ellipse => {
                let (name, kind) = if tool == Tool::Rect {
                    ("Rectángulo", ShapeKind::Rect { corner_radius: 0.0 })
                } else {
                    ("Elipse", ShapeKind::Ellipse)
                };
                let s = new_shape(app, name, Rect::from_points(sp, sp), kind);
                app.commit();
                let id = app.doc.add_shape(s);
                app.doc.selected = vec![id];
                app.canvas.drag = Some(Drag::Create {
                    id,
                    start: sp,
                    line: false,
                });
            }
            Tool::Line => {
                let (rect, kind) = path_kind(&[sp, sp], false);
                let s = new_shape(app, "Línea", rect, kind);
                app.commit();
                let id = app.doc.add_shape(s);
                app.doc.selected = vec![id];
                app.canvas.drag = Some(Drag::Create {
                    id,
                    start: sp,
                    line: true,
                });
            }
            Tool::Freehand => {
                let (rect, kind) = path_kind(&[p], false);
                let s = new_shape(app, "Mano alzada", rect, kind);
                app.commit();
                let id = app.doc.add_shape(s);
                app.doc.selected = vec![id];
                app.canvas.drag = Some(Drag::Freehand {
                    id,
                    points: vec![p],
                });
            }
            Tool::Polygon => {
                let close_now = app
                    .canvas
                    .polygon
                    .as_ref()
                    .and_then(|poly| poly.first().copied())
                    .is_some_and(|first| {
                        app.canvas
                            .polygon
                            .as_ref()
                            .is_some_and(|poly| poly.len() >= 3)
                            && first.distance(p) * vp.scale < 8.0
                    });
                if close_now {
                    finish_polygon(app, true);
                } else {
                    app.canvas.polygon.get_or_insert_with(Vec::new).push(sp);
                }
            }
            Tool::Pen => {
                let nodes = app.canvas.pen.get_or_insert_with(Vec::new);
                let on_first = nodes.len() >= 3 && nodes[0].p.distance(p) * vp.scale < PEN_CLOSE_PX;
                if on_first {
                    finish_pen(app, true);
                } else {
                    // Mayús: el tramo nuevo, en horizontal, vertical o diagonal.
                    let at = match nodes.last().filter(|_| shift) {
                        Some(prev) => oc_core::snap::constrain_angle(prev.p, sp, 45.0).0,
                        None => sp,
                    };
                    nodes.push(oc_core::nodes::Node::line(at));
                    app.canvas.drag = Some(Drag::PenHandle);
                }
            }
            Tool::Text => {
                if let Some(id) = shape_at(app, p) {
                    if app.doc.shape(id).is_some_and(|s| s.is_text()) {
                        app.doc.selected = vec![id];
                        app.canvas.editing_text = Some(id);
                        return;
                    }
                }
                create_text(app, &ctx, sp);
            }
            Tool::ZoomDrag => {
                app.canvas.drag = Some(Drag::ZoomBox { start: p });
                app.canvas.marquee = Some(Rect::from_points(p, p));
            }
            Tool::Pan => {}
        }
        return;
    }

    // Gesto en curso
    let Some(drag) = app.canvas.drag.clone() else {
        return;
    };
    let Some(pos) = response.interact_pointer_pos().or(hover) else {
        return;
    };
    let p = vp.to_world(pos);
    let sp = snap(app, p);
    let dragging = response.dragged();

    if dragging {
        match drag {
            Drag::Pan => {
                app.canvas.viewport.pan(response.drag_delta());
            }
            Drag::Move {
                start,
                originals,
                mut committed,
            } => {
                let mut d = p - start;
                if d.hypot() * vp.scale < DRAG_THRESHOLD_PX && !committed {
                    return;
                }
                if shift {
                    if d.x.abs() > d.y.abs() {
                        d.y = 0.0;
                    } else {
                        d.x = 0.0;
                    }
                }
                app.canvas.move_guides.clear();
                if app.doc.page.smart_snap {
                    let (snapped, guides) = smart_snap_delta(app, &originals, d);
                    d = snapped;
                    app.canvas.move_guides = guides;
                }
                if app.doc.page.snap_to_grid {
                    let step =
                        app.doc.page.grid_spacing_mm / app.doc.page.grid_divisions.max(1) as f64;
                    d = Vec2::new((d.x / step).round() * step, (d.y / step).round() * step);
                    // La retícula manda: las guías del ajuste ya no describen dónde quedó.
                    app.canvas.move_guides.clear();
                }
                if !committed {
                    app.commit();
                    committed = true;
                }
                for o in &originals {
                    if let Some(s) = app.doc.shape_mut(o.id) {
                        s.rect = o.rect + d;
                    }
                }
                app.canvas.drag = Some(Drag::Move {
                    start,
                    originals,
                    committed,
                });
            }
            Drag::Marquee { start, additive } => {
                app.canvas.marquee = Some(Rect::from_points(start, p));
                app.canvas.drag = Some(Drag::Marquee { start, additive });
            }
            Drag::NodeMarquee { start, additive } => {
                app.canvas.marquee = Some(Rect::from_points(start, p));
                app.canvas.drag = Some(Drag::NodeMarquee { start, additive });
            }
            Drag::ZoomBox { start } => {
                app.canvas.marquee = Some(Rect::from_points(start, p));
            }
            Drag::Scale {
                handle,
                grab,
                originals,
                group,
                mut committed,
            } => {
                if !committed {
                    app.commit();
                    committed = true;
                }
                let p = p - grab;
                let results = if originals.len() == 1 {
                    vec![scale_shape_by_handle(&originals[0], handle, p, shift)]
                } else {
                    let g = group
                        .or_else(|| selection_bounds(originals.iter()))
                        .unwrap_or(Rect::new(0.0, 0.0, 1.0, 1.0));
                    scale_group_by_handle(&originals, g, handle, p, shift)
                };
                for r in results {
                    if let Some(s) = app.doc.shape_mut(r.id) {
                        *s = r;
                    }
                }
                app.canvas.drag = Some(Drag::Scale {
                    handle,
                    grab,
                    originals,
                    group,
                    committed,
                });
            }
            Drag::Rotate {
                originals,
                center,
                start_angle,
                mut committed,
            } => {
                if !committed {
                    app.commit();
                    committed = true;
                }
                let mut delta = angle_deg(center, p) - start_angle;
                if shift {
                    delta = (delta / 15.0).round() * 15.0;
                }
                for r in rotate_shapes(&originals, center, delta) {
                    if let Some(s) = app.doc.shape_mut(r.id) {
                        *s = r;
                    }
                }
                app.canvas.drag = Some(Drag::Rotate {
                    originals,
                    center,
                    start_angle,
                    committed,
                });
            }
            Drag::Create { id, start, line } => {
                if line {
                    let mut end = sp;
                    if shift {
                        let ang = ((angle_deg(start, sp) / 45.0).round() * 45.0).to_radians();
                        let len = start.distance(sp);
                        end = Point::new(start.x + ang.cos() * len, start.y + ang.sin() * len);
                    }
                    if let Some(s) = app.doc.shape_mut(id) {
                        s.set_world_subpaths(&[(vec![start, end], false)]);
                    }
                } else {
                    let mut r = Rect::from_points(start, sp);
                    if shift {
                        let side = r.width().max(r.height());
                        let x0 = if sp.x < start.x {
                            start.x - side
                        } else {
                            start.x
                        };
                        let y0 = if sp.y < start.y {
                            start.y - side
                        } else {
                            start.y
                        };
                        r = Rect::new(x0, y0, x0 + side, y0 + side);
                    }
                    if let Some(s) = app.doc.shape_mut(id) {
                        s.rect = r;
                    }
                }
            }
            Drag::TraceArea { image, start } => {
                app.canvas.trace_area = Some((image, Rect::from_points(start, p)));
                app.canvas.drag = Some(Drag::TraceArea { image, start });
            }
            Drag::Knife { start, .. } => {
                let mut end = p;
                if shift {
                    let ang = ((angle_deg(start, p) / 45.0).round() * 45.0).to_radians();
                    let len = start.distance(p);
                    end = Point::new(start.x + ang.cos() * len, start.y + ang.sin() * len);
                }
                app.canvas.drag = Some(Drag::Knife { start, end });
            }
            Drag::Erase { mut path } => {
                if path.last().is_none_or(|l| l.distance(p) >= 0.3) {
                    path.push(p);
                }
                app.canvas.drag = Some(Drag::Erase { path });
            }
            Drag::MovePoint {
                shape,
                sub,
                idx,
                mut committed,
                grab,
                origin,
                targets,
            } => {
                let mut to = p - grab;
                if (to - origin).hypot() * vp.scale < DRAG_THRESHOLD_PX && !committed {
                    return;
                }
                if !committed {
                    app.commit();
                    committed = true;
                }
                // Mayús: solo en horizontal, vertical o diagonal desde donde estaba.
                let mut axes = oc_core::snap::Axes::BOTH;
                if shift {
                    (to, axes) = oc_core::snap::constrain_angle(origin, to, 45.0);
                }
                // ⌘/Ctrl: sin ajuste mientras se mantiene pulsada.
                app.canvas.snap_guides.clear();
                if !ctx.input(|i| i.modifiers.command) {
                    let page = &app.doc.page;
                    let none: &[Point] = &[];
                    let snapped = oc_core::snap::snap_point(
                        to,
                        if page.smart_snap { &targets } else { none },
                        super::point_edit::SNAP_PX / vp.scale,
                        page.snap_to_grid
                            .then(|| page.grid_spacing_mm / page.grid_divisions.max(1) as f64),
                        axes,
                    );
                    to = snapped.point;
                    app.canvas.snap_guides.extend(
                        [snapped.guide_x, snapped.guide_y]
                            .into_iter()
                            .flatten()
                            .map(|t| (t, to)),
                    );
                }
                app.point_move_node(sub, idx, to);
                app.canvas.drag = Some(Drag::MovePoint {
                    shape,
                    sub,
                    idx,
                    committed,
                    grab,
                    origin,
                    targets,
                });
            }
            Drag::MoveHandle {
                shape,
                sub,
                idx,
                out,
                mut committed,
            } => {
                if !committed {
                    app.commit();
                    committed = true;
                }
                // Mayús: el manejador gira en pasos de 15° alrededor de su nodo.
                let mut to = p;
                if shift {
                    let node = super::point_edit::edited_paths(app)
                        .and_then(|(_, paths)| paths.get(sub)?.nodes.get(idx).map(|n| n.p));
                    if let Some(node) = node {
                        to = oc_core::snap::constrain_angle(node, p, 15.0).0;
                    }
                }
                app.point_move_handle(sub, idx, out, to);
                app.canvas.drag = Some(Drag::MoveHandle {
                    shape,
                    sub,
                    idx,
                    out,
                    committed,
                });
            }
            Drag::PenHandle => {
                // Arrastrar tras poner el nodo le saca dos manejadores simétricos: el de
                // salida sigue al puntero.
                if let Some(node) = app.canvas.pen.as_mut().and_then(|n| n.last_mut()) {
                    let at = node.p;
                    if at.distance(p) * vp.scale >= DRAG_THRESHOLD_PX {
                        let to = if shift {
                            oc_core::snap::constrain_angle(at, p, 45.0).0
                        } else {
                            p
                        };
                        node.h_out = Some(to);
                        node.h_in = Some(at - (to - at));
                        node.smooth = true;
                        node.symmetric = true;
                    }
                }
                app.canvas.drag = Some(Drag::PenHandle);
            }
            Drag::Freehand { id, mut points } => {
                if points.last().is_none_or(|l| l.distance(p) >= 0.4) {
                    points.push(p);
                    if let Some(s) = app.doc.shape_mut(id) {
                        s.set_world_subpaths(&[(points.clone(), false)]);
                    }
                }
                app.canvas.drag = Some(Drag::Freehand { id, points });
            }
        }
        return;
    }

    // Fin de gesto
    if response.drag_stopped() || (!response.is_pointer_button_down_on() && !dragging) {
        app.canvas.drag = None;
        app.canvas.snap_guides.clear();
        app.canvas.move_guides.clear();
        match drag {
            Drag::NodeMarquee { additive, .. } => {
                if let Some(m) = app.canvas.marquee.take() {
                    if m.width() * vp.scale >= DRAG_THRESHOLD_PX
                        || m.height() * vp.scale >= DRAG_THRESHOLD_PX
                    {
                        app.point_select_rect(m, additive);
                    } else if !additive {
                        // Clic simple en vacío: deseleccionar nodos (o salir si fue fuera).
                        app.canvas.point_multi.clear();
                        if let Some(pe) = app.canvas.point_edit.as_mut() {
                            pe.selected = None;
                        }
                        if shape_at(app, m.origin()).is_none() {
                            app.canvas.point_edit = None;
                            app.doc.selected.clear();
                        }
                    }
                }
            }
            Drag::Marquee { additive, .. } => {
                let m = app.canvas.marquee.take();
                if let Some(m) = m {
                    if m.width() * vp.scale >= DRAG_THRESHOLD_PX
                        || m.height() * vp.scale >= DRAG_THRESHOLD_PX
                    {
                        let hit: Vec<u64> = app
                            .doc
                            .shapes
                            .iter()
                            .filter(|s| s.visible && m.overlaps(shape_bounds(s)))
                            .map(|s| s.id)
                            .collect();
                        if additive {
                            for id in hit {
                                if !app.doc.is_selected(id) {
                                    app.doc.selected.push(id);
                                }
                            }
                        } else {
                            app.doc.selected = hit;
                        }
                    }
                }
            }
            Drag::ZoomBox { .. } => {
                let m = app.canvas.marquee.take();
                match m {
                    Some(m) if m.width() * vp.scale > 8.0 && m.height() * vp.scale > 8.0 => {
                        app.canvas.viewport = super::Viewport::fit(m, app.canvas.stage, 20.0);
                    }
                    _ => app
                        .canvas
                        .viewport
                        .zoom_at(if alt { 0.5 } else { 2.0 }, pos),
                }
            }
            Drag::Create { id, line, .. } => {
                let tiny = app.doc.shape(id).is_some_and(|s| {
                    if line {
                        s.rect.width().hypot(s.rect.height()) < 0.5
                    } else {
                        s.rect.width() < 0.5 || s.rect.height() < 0.5
                    }
                });
                if tiny {
                    cancel_add(app, id);
                }
            }
            Drag::TraceArea { .. } => {
                app.canvas.trace_area_select = None;
                if app
                    .canvas
                    .trace_area
                    .is_some_and(|(_, r)| r.width() * vp.scale < 4.0 || r.height() * vp.scale < 4.0)
                {
                    app.canvas.trace_area = None;
                }
            }
            Drag::Freehand { id, points } if points.len() < 2 => {
                cancel_add(app, id);
            }
            Drag::Knife { start, end } => apply_knife(app, start, end),
            Drag::Erase { path } => apply_eraser(app, &path),
            _ => {}
        }
    }
    let _ = viewport_actions::page_frame;
}

/// Gesto de mover el nodo (`si`, `ni`) pulsado en `p` (mm), con sus objetivos de ajuste.
fn move_point_drag(
    app: &OpenCraftCutterApp,
    shape: u64,
    si: usize,
    ni: usize,
    p: Point,
    committed: bool,
) -> Option<Drag> {
    let (_, paths) = super::point_edit::edited_paths(app)?;
    let origin = paths.get(si)?.nodes.get(ni)?.p;
    // Los nodos que se mueven con él no son objetivos.
    let mut moving = app.selected_nodes();
    if !moving.contains(&(si, ni)) {
        moving = vec![(si, ni)];
    }
    Some(Drag::MovePoint {
        shape,
        sub: si,
        idx: ni,
        committed,
        // Un nodo recién insertado nace bajo el puntero: se arrastra desde ahí.
        grab: if committed { Vec2::ZERO } else { p - origin },
        origin,
        targets: std::sync::Arc::new(super::point_edit::snap_targets(app, &moving)),
    })
}

/// Deshace la creación de una forma recién añadida sin dejar rastro en el historial.
fn cancel_add(app: &mut OpenCraftCutterApp, id: u64) {
    app.undo();
    app.doc.selected.retain(|x| *x != id);
    app.history.clear_future();
}

// ---------------------------------------------------------------------------------------------
// Cuchilla, borrador y edición de puntos
// ---------------------------------------------------------------------------------------------

use oc_core::ops;

/// Formas editables por las herramientas de corte (visibles, desbloqueadas, no imágenes).
fn editable_shapes(app: &OpenCraftCutterApp) -> Vec<Shape> {
    let all: Vec<Shape> = app
        .doc
        .shapes
        .iter()
        .filter(|s| s.visible && !s.locked && !s.is_image())
        .cloned()
        .collect();
    let sel: Vec<Shape> = all
        .iter()
        .filter(|s| app.doc.is_selected(s.id))
        .cloned()
        .collect();
    crate::text::paths_for_ops(if sel.is_empty() { &all } else { &sel })
}

/// Sustituye `old_id` por `pieces` en su misma posición de pila; selecciona las piezas si procede.
fn replace_in_place(app: &mut OpenCraftCutterApp, old_id: u64, pieces: Vec<Shape>, select: bool) {
    let pos = app
        .doc
        .shapes
        .iter()
        .position(|x| x.id == old_id)
        .unwrap_or(app.doc.shapes.len());
    app.doc.remove_shapes(&[old_id]);
    for (i, piece) in pieces.into_iter().enumerate() {
        let id = app.doc.add_shape(piece);
        let last = app.doc.shapes.len() - 1;
        app.doc.shapes.swap(last, (pos + i).min(last));
        if select {
            app.doc.selected.push(id);
        }
    }
}

fn apply_knife(app: &mut OpenCraftCutterApp, start: Point, end: Point) {
    if start.distance(end) < 0.2 {
        return;
    }
    let line = [[start.x, start.y], [end.x, end.y]];
    let mut committed = false;
    for s in editable_shapes(app) {
        let pieces = ops::slice(&s, &line);
        if pieces.len() > 1 {
            if !committed {
                app.commit();
                committed = true;
            }
            replace_in_place(app, s.id, pieces, true);
        }
    }
}

fn apply_eraser(app: &mut OpenCraftCutterApp, path: &[Point]) {
    let pts: Vec<[f64; 2]> = path.iter().map(|p| [p.x, p.y]).collect();
    let brush = ops::brush_polygons(&pts, app.canvas.eraser_width_mm);
    if brush.is_empty() {
        return;
    }
    let brush_bounds = oc_core::edit::bounds_of(
        brush
            .iter()
            .flatten()
            .flatten()
            .map(|p| Point::new(p[0], p[1])),
    );
    let candidates: Vec<Shape> = editable_shapes(app)
        .into_iter()
        .filter(|s| brush_bounds.is_some_and(|b| b.overlaps(shape_bounds(s))))
        .collect();
    let mut committed = false;
    for s in candidates {
        let rest = ops::erase(&s, &brush);
        let unchanged = rest.len() == 1
            && rest[0].world_subpaths() == s.world_subpaths()
            && rest[0].rect == s.rect;
        if unchanged {
            continue;
        }
        if !committed {
            app.commit();
            committed = true;
        }
        let was_selected = app.doc.is_selected(s.id);
        replace_in_place(app, s.id, rest, was_selected);
    }
}

/// Borde o centro al que se puede alinear la selección: su coordenada en el eje que se ajusta
/// y lo que ocupa en el otro (para dibujar la guía).
struct SnapTarget {
    value: f64,
    lo: f64,
    hi: f64,
}

/// Ajuste inteligente: corrige el desplazamiento para que los bordes o el centro de la selección
/// caigan sobre bordes/centros de la página (y de otros objetos, si no se limita a los medios).
/// Devuelve también las guías (mm) que muestran con qué se alineó.
fn smart_snap_delta(
    app: &OpenCraftCutterApp,
    originals: &[Shape],
    d: Vec2,
) -> (Vec2, Vec<(Point, Point)>) {
    let Some(b) = selection_bounds(originals.iter()) else {
        return (d, Vec::new());
    };
    let page = &app.doc.page;
    let tol = page.snap_tolerance_mm.max(0.0);
    let moving = b + d;
    let (w, h) = (page.width_mm, page.height_mm);
    let mut xs: Vec<SnapTarget> = [0.0, w / 2.0, w]
        .map(|value| SnapTarget {
            value,
            lo: 0.0,
            hi: h,
        })
        .into();
    let mut ys: Vec<SnapTarget> = [0.0, h / 2.0, h]
        .map(|value| SnapTarget {
            value,
            lo: 0.0,
            hi: w,
        })
        .into();
    if !page.smart_snap_within_media {
        let moving_ids: std::collections::HashSet<u64> = originals.iter().map(|s| s.id).collect();
        for s in app
            .doc
            .shapes
            .iter()
            .filter(|s| s.visible && !moving_ids.contains(&s.id))
        {
            let sb = shape_bounds(s);
            xs.extend([sb.x0, sb.center().x, sb.x1].map(|value| SnapTarget {
                value,
                lo: sb.y0,
                hi: sb.y1,
            }));
            ys.extend([sb.y0, sb.center().y, sb.y1].map(|value| SnapTarget {
                value,
                lo: sb.x0,
                hi: sb.x1,
            }));
        }
    }
    let best = |edges: [f64; 3], targets: &[SnapTarget]| -> Option<(f64, usize)> {
        let mut best: Option<(f64, usize)> = None;
        for e in edges {
            for (i, t) in targets.iter().enumerate() {
                let diff = t.value - e;
                if diff.abs() <= tol && best.is_none_or(|(b, _)| diff.abs() < b.abs()) {
                    best = Some((diff, i));
                }
            }
        }
        best
    };
    let bx = best([moving.x0, moving.center().x, moving.x1], &xs);
    let by = best([moving.y0, moving.center().y, moving.y1], &ys);
    let out = Vec2::new(d.x + bx.map_or(0.0, |b| b.0), d.y + by.map_or(0.0, |b| b.0));
    // Cada guía cubre el objetivo y la selección ya colocada.
    let placed = b + out;
    let mut guides = Vec::new();
    if let Some((_, i)) = bx {
        let t = &xs[i];
        guides.push((
            Point::new(t.value, t.lo.min(placed.y0)),
            Point::new(t.value, t.hi.max(placed.y1)),
        ));
    }
    if let Some((_, i)) = by {
        let t = &ys[i];
        guides.push((
            Point::new(t.lo.min(placed.x0), t.value),
            Point::new(t.hi.max(placed.x1), t.value),
        ));
    }
    (out, guides)
}
