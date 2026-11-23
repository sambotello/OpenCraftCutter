//! Guías de regla: se sacan arrastrando desde una regla, se mueven con la herramienta
//! Seleccionar y se borran devolviéndolas a la regla. Lo que se dibuja o se mueve se ajusta a
//! ellas (con el ajuste inteligente activo).

use super::Viewport;
use crate::app::{OpenCraftCutterApp, Workspace};
use egui::{Color32, Painter, Pos2, Rect as ERect, Stroke, Ui};
use kurbo::Point;
use oc_core::document::Guide;

/// Color de las guías de regla.
const GUIDE_COLOR: Color32 = Color32::from_rgb(0x00, 0xb4, 0xd8);
/// Distancia (px) a la que el puntero «toca» una guía.
pub const GUIDE_HIT_PX: f32 = 4.0;

/// ¿Se pueden usar las guías ahora? Solo en Diseño y con la vista sin girar (con la vista
/// girada las reglas no corresponden a los ejes del documento).
fn available(app: &OpenCraftCutterApp) -> bool {
    app.workspace == Workspace::Design && app.canvas.viewport.rotation == 0
}

/// Coordenada de una guía (su x si es vertical, su y si no) para un punto del documento.
fn coordinate(vertical: bool, p: Point) -> f64 {
    if vertical { p.x } else { p.y }
}

/// Arrastrar desde la regla superior saca una guía horizontal; desde la izquierda, una
/// vertical. Al soltarla sobre el lienzo queda puesta; fuera de él, se descarta.
pub fn handle_rulers(app: &mut OpenCraftCutterApp, ui: &mut Ui, top: ERect, left: ERect) {
    if !available(app) {
        app.canvas.new_guide = None;
        return;
    }
    for (rect, vertical, id) in [
        (top, false, "regla_superior"),
        (left, true, "regla_izquierda"),
    ] {
        let r = ui.interact(rect, egui::Id::unique(id), egui::Sense::drag());
        let pointer = r.interact_pointer_pos();
        if r.hovered() || r.dragged() {
            ui.output_mut(|o| {
                o.cursor_icon = if vertical {
                    egui::CursorIcon::ResizeHorizontal
                } else {
                    egui::CursorIcon::ResizeVertical
                };
            });
        }
        if r.drag_started() || r.dragged() {
            if let Some(pos) = pointer {
                let p = app.canvas.viewport.to_world(pos);
                app.canvas.new_guide = Some(Guide {
                    vertical,
                    pos: coordinate(vertical, p),
                });
            }
        }
        if r.drag_stopped() {
            let inside = pointer.is_some_and(|pos| app.canvas.stage.contains(pos));
            if let Some(g) = app.canvas.new_guide.take().filter(|_| inside) {
                app.commit();
                app.doc.page.guides.push(g);
                app.doc.page.show_guides = true;
            }
        }
    }
}

/// Guía bajo el puntero (px de pantalla), si las guías se ven.
pub fn guide_at(app: &OpenCraftCutterApp, pos: Pos2) -> Option<usize> {
    if !available(app) || !app.doc.page.show_guides {
        return None;
    }
    let vp = app.canvas.viewport;
    app.doc
        .page
        .guides
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let s = vp.to_screen(if g.vertical {
                Point::new(g.pos, 0.0)
            } else {
                Point::new(0.0, g.pos)
            });
            (i, if g.vertical { s.x - pos.x } else { s.y - pos.y }.abs())
        })
        .filter(|(_, d)| *d <= GUIDE_HIT_PX)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Mueve la guía `index` al punto `p` (mm).
pub fn move_guide(app: &mut OpenCraftCutterApp, index: usize, p: Point) {
    if let Some(g) = app.doc.page.guides.get_mut(index) {
        g.pos = coordinate(g.vertical, p);
    }
}

/// Fin del arrastre de una guía: si se soltó fuera del lienzo (sobre la regla), se borra.
pub fn drop_guide(app: &mut OpenCraftCutterApp, index: usize, pos: Pos2) {
    if !app.canvas.stage.contains(pos) && index < app.doc.page.guides.len() {
        app.doc.page.guides.remove(index);
    }
}

/// Quita todas las guías (una entrada de historial).
pub fn clear_guides(app: &mut OpenCraftCutterApp) {
    if !app.doc.page.guides.is_empty() {
        app.commit();
        app.doc.page.guides.clear();
    }
}

/// Dibuja las guías puestas y la que se está sacando de la regla.
pub fn draw(app: &OpenCraftCutterApp, painter: &Painter, vp: &Viewport) {
    if !available(app) {
        return;
    }
    let stage = app.canvas.stage;
    let line = |g: &Guide, stroke: Stroke| {
        if g.vertical {
            let x = vp.to_screen(Point::new(g.pos, 0.0)).x;
            painter.line_segment(
                [Pos2::new(x, stage.top()), Pos2::new(x, stage.bottom())],
                stroke,
            );
        } else {
            let y = vp.to_screen(Point::new(0.0, g.pos)).y;
            painter.line_segment(
                [Pos2::new(stage.left(), y), Pos2::new(stage.right(), y)],
                stroke,
            );
        }
    };
    if app.doc.page.show_guides {
        for g in &app.doc.page.guides {
            line(g, Stroke::new(1.0, GUIDE_COLOR));
        }
    }
    if let Some(g) = &app.canvas.new_guide {
        line(g, Stroke::new(1.0, GUIDE_COLOR.gamma_multiply(0.6)));
    }
}
