//! Capa de interfaz sobre la escena: marco de selección, asas, marco de arrastre y polígono en curso.

use super::Viewport;
use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{Color32, Painter, Pos2, Rect as ERect, Shape as EShape, Stroke, StrokeKind};
use kurbo::{Point, Rect};
use oc_core::edit::{Handle, rotation_about, selection_bounds};

pub const HANDLE_PX: f32 = 7.0;
/// Colores del marco de selección (estilo de la referencia).
const FRAME_COLOR: Color32 = Color32::from_gray(0x3c);
const HANDLE_BORDER: Color32 = Color32::from_gray(0x70);
const ROTATE_COLOR: Color32 = Color32::from_rgb(0x1c, 0xe0, 0x1c);
pub const VERTEX_PX: f32 = 6.0;
pub const ROTATE_OFFSET_PX: f32 = 32.0;
/// Separación (px de pantalla) entre el objeto y el marco de selección, como en la referencia.
pub const FRAME_PAD_PX: f32 = 8.0;

/// Ejes del marco en pantalla (unitarios, girando con el objeto): x hacia la derecha e y
/// hacia abajo del objeto.
fn frame_axes(frame: &SelectionFrame, vp: &Viewport) -> (egui::Vec2, egui::Vec2) {
    let rot = rotation_about(frame.rect.center(), frame.rotation);
    let o = vp.to_screen(rot * Point::new(0.0, 0.0));
    let ux = (vp.to_screen(rot * Point::new(1.0, 0.0)) - o).normalized();
    let uy = (vp.to_screen(rot * Point::new(0.0, 1.0)) - o).normalized();
    let ok = |v: egui::Vec2| v.is_finite() && v.length() > 0.5;
    (
        if ok(ux) { ux } else { egui::vec2(1.0, 0.0) },
        if ok(uy) { uy } else { egui::vec2(0.0, 1.0) },
    )
}

/// Posición en pantalla de un punto del marco separado del objeto (`sx`, `sy` ∈ {−1, 0, 1}).
fn padded(base: Pos2, ax: (egui::Vec2, egui::Vec2), sx: f32, sy: f32) -> Pos2 {
    base + (ax.0 * sx + ax.1 * sy) * FRAME_PAD_PX
}

fn handle_signs(h: Handle) -> (f32, f32) {
    let sx = if h.west() {
        -1.0
    } else if h.east() {
        1.0
    } else {
        0.0
    };
    let sy = if h.north() {
        -1.0
    } else if h.south() {
        1.0
    } else {
        0.0
    };
    (sx, sy)
}

/// Marco de selección en mm (local, sin rotar) y su rotación.
pub struct SelectionFrame {
    pub rect: Rect,
    pub rotation: f64,
    pub locked: bool,
}

pub fn selection_frame(app: &OpenCraftCutterApp) -> Option<SelectionFrame> {
    let sel = app.doc.selected_shapes();
    match sel.as_slice() {
        [] => None,
        [one] => Some(SelectionFrame {
            rect: one.rect,
            rotation: one.rotation,
            locked: one.locked,
        }),
        many => selection_bounds(many.iter().copied()).map(|b| SelectionFrame {
            rect: b,
            rotation: 0.0,
            locked: many.iter().all(|s| s.locked),
        }),
    }
}

/// Posición en pantalla de cada asa (y del asa de rotación) del marco.
pub fn handle_positions(frame: &SelectionFrame, vp: &Viewport) -> (Vec<(Handle, Pos2)>, Pos2) {
    let rot = rotation_about(frame.rect.center(), frame.rotation);
    let ax = frame_axes(frame, vp);
    let handles = Handle::ALL
        .iter()
        .map(|h| {
            let (sx, sy) = handle_signs(*h);
            (
                *h,
                padded(vp.to_screen(rot * h.position(frame.rect)), ax, sx, sy),
            )
        })
        .collect();
    let top = padded(
        vp.to_screen(rot * Point::new(frame.rect.center().x, frame.rect.y0)),
        ax,
        0.0,
        -1.0,
    );
    (handles, top - ax.1 * ROTATE_OFFSET_PX)
}

pub fn draw_overlay(app: &OpenCraftCutterApp, painter: &Painter) {
    let vp = app.canvas.viewport;
    draw_tool_feedback(app, painter, &vp);
    if let Some(poly) = &app.canvas.polygon {
        let mut pts: Vec<Pos2> = poly.iter().map(|p| vp.to_screen(*p)).collect();
        if let Some(c) = app.canvas.cursor_mm {
            pts.push(vp.to_screen(c));
        }
        if pts.len() >= 2 {
            painter.add(EShape::dashed_line(
                &pts,
                Stroke::new(1.0, theme::SELECTION),
                4.0,
                3.0,
            ));
        }
    }
    if app.canvas.editing_text.is_none() {
        if let Some(frame) = selection_frame(app) {
            let rot = rotation_about(frame.rect.center(), frame.rotation);
            let r = frame.rect;
            let corners = [
                Point::new(r.x0, r.y0),
                Point::new(r.x1, r.y0),
                Point::new(r.x1, r.y1),
                Point::new(r.x0, r.y1),
            ];
            let pts: Vec<Pos2> = corners.iter().map(|p| vp.to_screen(rot * *p)).collect();
            draw_dimensions(app, painter, &frame, &pts);
            // Marco al estilo de la referencia: tramos grises entre asas (sin tocarlas),
            // asas cuadradas blancas con borde gris y asa de giro verde.
            let (handles, rotate_pos) = handle_positions(&frame, &vp);
            let frame_stroke = Stroke::new(1.0, FRAME_COLOR);
            let gap = HANDLE_PX / 2.0 + 4.0;
            for i in 0..handles.len() {
                let (a, b) = (handles[i].1, handles[(i + 1) % handles.len()].1);
                let d = b - a;
                let len = d.length();
                if len > 2.0 * gap + 2.0 {
                    let u = d / len;
                    painter.line_segment([a + u * gap, b - u * gap], frame_stroke);
                }
            }
            if !frame.locked {
                let top = handles[1].1;
                let d = rotate_pos - top;
                if d.length() > 2.0 * gap {
                    let u = d.normalized();
                    painter.line_segment(
                        [top + u * gap, rotate_pos - u * (HANDLE_PX * 0.7 + 3.0)],
                        frame_stroke,
                    );
                }
                painter.circle(
                    rotate_pos,
                    HANDLE_PX * 0.7,
                    ROTATE_COLOR,
                    Stroke::new(1.0, Color32::BLACK),
                );
            }
            for (_, p) in &handles {
                painter.rect(
                    ERect::from_center_size(*p, egui::vec2(HANDLE_PX, HANDLE_PX)),
                    0.0,
                    Color32::WHITE,
                    Stroke::new(1.0, HANDLE_BORDER),
                    StrokeKind::Inside,
                );
            }
        }
    }
    if let Some(m) = app.canvas.marquee {
        let r = vp.rect_to_screen(m);
        painter.rect(
            r,
            0.0,
            theme::ACCENT_SOFT,
            Stroke::new(1.0, theme::SELECTION),
            StrokeKind::Inside,
        );
    }
}

const DIM_COLOR: Color32 = Color32::from_gray(0x5a);
/// Separación de las cotas respecto al marco de selección.
const DIM_OFFSET_PX: f32 = FRAME_PAD_PX + 14.0;

/// Cotas de anchura (debajo) y altura (a la izquierda) del marco, como en la referencia.
/// `pts` son las esquinas del marco en pantalla: sup-izq, sup-der, inf-der, inf-izq.
fn draw_dimensions(
    app: &OpenCraftCutterApp,
    painter: &Painter,
    frame: &SelectionFrame,
    pts: &[Pos2],
) {
    let unit = app.doc.page.unit;
    let label = |mm: f64| {
        format!(
            "{:.*} {}",
            unit.decimals() + 1,
            unit.from_mm(mm),
            unit.label()
        )
    };
    let (tl, br, bl) = (pts[0], pts[2], pts[3]);
    // Normales hacia fuera del marco (abajo e izquierda, girando con el objeto).
    let down = (bl - tl).normalized();
    let left = (bl - br).normalized();
    if !down.is_finite() || !left.is_finite() {
        return;
    }
    dimension(
        painter,
        bl + down * DIM_OFFSET_PX,
        br + down * DIM_OFFSET_PX,
        down,
        &label(frame.rect.width()),
    );
    // La altura se lee de abajo arriba, como en la referencia.
    dimension(
        painter,
        bl + left * DIM_OFFSET_PX,
        tl + left * DIM_OFFSET_PX,
        left,
        &label(frame.rect.height()),
    );
}

/// Una cota de `a` a `b`: línea con flechas, líneas de referencia hacia el objeto y el valor
/// centrado en un hueco de la línea.
fn dimension(painter: &Painter, a: Pos2, b: Pos2, out: egui::Vec2, text: &str) {
    let d = b - a;
    let len = d.length();
    if len < 8.0 {
        return;
    }
    let dir = d / len;
    let stroke = Stroke::new(1.0, DIM_COLOR);
    // Líneas de referencia (perpendiculares, cruzando la cota).
    for p in [a, b] {
        painter.line_segment([p - out * 6.0, p + out * 4.0], stroke);
    }
    let font = egui::FontId::proportional(12.0);
    let galley = painter.layout_no_wrap(text.to_string(), font, DIM_COLOR);
    let gap = galley.size().x + 12.0;
    let mid = a + d / 2.0;
    let arrow = |tip: Pos2, back: egui::Vec2| {
        let n = egui::vec2(-back.y, back.x);
        painter.add(EShape::convex_polygon(
            vec![tip, tip + back * 8.0 + n * 3.5, tip + back * 8.0 - n * 3.5],
            Color32::WHITE,
            stroke,
        ));
    };
    let fits = len > gap + 24.0;
    if fits {
        painter.line_segment([a, mid - dir * gap / 2.0], stroke);
        painter.line_segment([mid + dir * gap / 2.0, b], stroke);
        arrow(a, dir);
        arrow(b, -dir);
    } else {
        painter.line_segment([a, b], stroke);
    }
    // Texto centrado y alineado con la cota, siempre legible (nunca boca abajo).
    // Una cota vertical (−90°) se lee de abajo arriba, como en la referencia.
    let mut angle = dir.y.atan2(dir.x);
    let eps = 1e-3;
    if angle > std::f32::consts::FRAC_PI_2 + eps || angle < -std::f32::consts::FRAC_PI_2 - eps {
        angle += std::f32::consts::PI;
    }
    let (sin, cos) = angle.sin_cos();
    let half = galley.size() / 2.0;
    // Esquina superior izquierda del texto girado para que su centro caiga en `mid`.
    let offset = egui::vec2(half.x * cos - half.y * sin, half.x * sin + half.y * cos);
    // Si no cabe en el hueco, el valor va por fuera de la línea.
    let center = if fits {
        mid
    } else {
        mid + out * (galley.size().y / 2.0 + 4.0)
    };
    let pos = center - offset;
    painter.add(egui::epaint::TextShape::new(pos, galley, DIM_COLOR).with_angle(angle));
}

fn draw_tool_feedback(app: &OpenCraftCutterApp, painter: &Painter, vp: &Viewport) {
    if app.panel == Some(crate::app::Panel::Trace) {
        if let Some((_, r)) = app.canvas.trace_area {
            let sr = vp.rect_to_screen(r);
            let pts = [
                sr.left_top(),
                sr.right_top(),
                sr.right_bottom(),
                sr.left_bottom(),
                sr.left_top(),
            ];
            painter.add(EShape::dashed_line(
                &pts,
                Stroke::new(1.5, Color32::from_rgb(0xff, 0xb0, 0x00)),
                6.0,
                4.0,
            ));
        }
    }
    match &app.canvas.drag {
        Some(super::Drag::Knife { start, end }) => {
            painter.add(EShape::dashed_line(
                &[vp.to_screen(*start), vp.to_screen(*end)],
                Stroke::new(1.5, theme::CUT_AREA),
                6.0,
                4.0,
            ));
        }
        Some(super::Drag::Erase { path }) => {
            let pts: Vec<Pos2> = path.iter().map(|p| vp.to_screen(*p)).collect();
            let w = (app.canvas.eraser_width_mm * vp.scale) as f32;
            if pts.len() >= 2 {
                painter.add(EShape::line(
                    pts,
                    Stroke::new(w, Color32::from_rgba_unmultiplied(0xe3, 0x34, 0x3f, 90)),
                ));
            } else if let Some(p) = pts.first() {
                painter.circle_filled(
                    *p,
                    w / 2.0,
                    Color32::from_rgba_unmultiplied(0xe3, 0x34, 0x3f, 90),
                );
            }
        }
        _ => {}
    }
    if app.tool == crate::app::Tool::Eraser {
        if let Some(c) = app.canvas.cursor_mm {
            let r = (app.canvas.eraser_width_mm * vp.scale / 2.0) as f32;
            painter.circle_stroke(vp.to_screen(c), r, Stroke::new(1.0, theme::CUT_AREA));
        }
    }
    if app.tool == crate::app::Tool::EditPoints {
        draw_point_edit(app, painter, vp);
    }
}

/// Edición de puntos al estilo de la referencia: nodos grises, el seleccionado hueco, el tramo
/// que termina en él resaltado en rojo y sus manejadores en azul.
fn draw_point_edit(app: &OpenCraftCutterApp, painter: &Painter, vp: &Viewport) {
    let Some(pe) = app.canvas.point_edit else {
        return;
    };
    let Some((_, paths)) = super::point_edit::edited_paths(app) else {
        return;
    };
    let red = Color32::from_rgb(0xe0, 0x10, 0x10);
    let node_fill = Color32::from_gray(0x5a);
    let handle_col = Color32::from_rgb(0x8c, 0xa6, 0xff);
    if let Some((si, ni)) = pe.selected {
        if let Some(np) = paths.get(si) {
            if let Some(seg) = super::point_edit::highlighted_segment(np, ni) {
                let mut b = kurbo::BezPath::new();
                match np.segment(seg) {
                    (a, Some(c), _) => {
                        b.move_to(a);
                        b.curve_to(c.p1, c.p2, c.p3);
                    }
                    (a, None, e) => {
                        b.move_to(a);
                        b.line_to(e);
                    }
                }
                let mut pts = Vec::new();
                kurbo::flatten(b, 0.02, |el| match el {
                    kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => {
                        pts.push(vp.to_screen(p))
                    }
                    _ => {}
                });
                painter.add(EShape::line(pts, Stroke::new(2.0, red)));
            }
        }
    }
    for (si, np) in paths.iter().enumerate() {
        let n = np.nodes.len();
        for (ni, node) in np.nodes.iter().enumerate() {
            let p = vp.to_screen(node.p);
            let primary = pe.selected == Some((si, ni));
            let selected = primary || app.canvas.point_multi.contains(&(si, ni));
            // Extremos de un trazo abierto: círculos (rojo el inicio), como la referencia.
            let is_end = !np.closed && (ni == 0 || ni + 1 == n);
            if selected {
                let handles = if primary {
                    [node.h_in, node.h_out]
                } else {
                    [None, None]
                };
                for h in handles.into_iter().flatten() {
                    let hp = vp.to_screen(h);
                    painter.line_segment([p, hp], Stroke::new(1.0, Color32::from_gray(0x70)));
                    painter.rect(
                        ERect::from_center_size(hp, egui::vec2(VERTEX_PX, VERTEX_PX)),
                        0.0,
                        handle_col,
                        Stroke::new(1.0, Color32::from_rgb(0x50, 0x6c, 0xe0)),
                        StrokeKind::Inside,
                    );
                }
                painter.rect(
                    ERect::from_center_size(p, egui::vec2(VERTEX_PX, VERTEX_PX)),
                    1.0,
                    Color32::WHITE,
                    Stroke::new(1.2, Color32::from_gray(0x30)),
                    StrokeKind::Inside,
                );
            } else if is_end && ni == 0 {
                painter.circle_filled(p, VERTEX_PX * 0.6, red);
            } else if is_end {
                painter.circle(
                    p,
                    VERTEX_PX * 0.6,
                    Color32::WHITE,
                    Stroke::new(1.0, Color32::from_gray(0x30)),
                );
            } else {
                painter.rect_filled(
                    ERect::from_center_size(p, egui::vec2(VERTEX_PX, VERTEX_PX)),
                    0.0,
                    node_fill,
                );
            }
        }
    }
}
