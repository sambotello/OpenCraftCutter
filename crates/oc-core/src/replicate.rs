//! Replicar (panel Replicar), con el comportamiento de la referencia:
//!
//! - Duplicar, filas y columnas: copias pegadas a la caja de la selección, sin separación.
//! - Reflejar: copia espejo pegada al lado indicado.
//! - Rotar copias: `n` objetos girados en pasos de 180°/n sobre el centro de la selección.
//! - Llenar página: cuadrícula de copias que cubre el área de corte.
//! - Replicar a medida: `n` copias desplazadas y giradas de forma acumulada.
//! - Objeto en ruta: copias repartidas a lo largo de una figura guía.

use crate::arrange::{rotate_group, translate};
use crate::edit::selection_bounds;
use crate::shape::Shape;
use kurbo::{Point, Rect, Vec2};

/// Copias nuevas (id 0) de todo el grupo.
fn fresh(shapes: Vec<Shape>) -> Vec<Shape> {
    shapes
        .into_iter()
        .map(|mut s| {
            s.id = 0;
            s
        })
        .collect()
}

/// `count` copias del grupo; la copia `i` (1..=count) se desplaza `i·step` y gira `i·angle`
/// grados sobre su propio centro.
pub fn offset_copies(shapes: &[Shape], step: Vec2, count: usize, angle: f64) -> Vec<Shape> {
    let mut out = Vec::new();
    for i in 1..=count {
        let moved = translate(shapes, step * i as f64);
        let turned = if angle != 0.0 {
            rotate_group(&moved, angle * i as f64)
        } else {
            moved
        };
        out.extend(fresh(turned));
    }
    out
}

/// Lado al que se pega una copia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Up,
    Down,
}

impl Side {
    /// Desplazamiento de una copia pegada a la caja `b` por este lado.
    pub fn step(self, b: Rect) -> Vec2 {
        match self {
            Side::Left => Vec2::new(-b.width(), 0.0),
            Side::Right => Vec2::new(b.width(), 0.0),
            Side::Up => Vec2::new(0.0, -b.height()),
            Side::Down => Vec2::new(0.0, b.height()),
        }
    }
}

/// `count` copias pegadas en fila o columna hacia `side`.
pub fn adjacent_copies(shapes: &[Shape], side: Side, count: usize) -> Vec<Shape> {
    let Some(b) = selection_bounds(shapes) else {
        return Vec::new();
    };
    offset_copies(shapes, side.step(b), count, 0.0)
}

/// `n` objetos en total, girados 180°/n entre sí sobre el centro de la selección (devuelve
/// las `n - 1` copias nuevas).
pub fn rotate_copies(shapes: &[Shape], n: usize) -> Vec<Shape> {
    let step = 180.0 / n.max(1) as f64;
    (1..n)
        .flat_map(|k| fresh(rotate_group(shapes, step * k as f64)))
        .collect()
}

/// Margen al borde de la página de «Llenar página» (3/16 de pulgada, como la referencia).
pub const FILL_MARGIN_MM: f64 = 4.7625;
/// Separación mínima entre copias de «Llenar página» (1/4 de pulgada).
pub const FILL_MIN_GAP_MM: f64 = 6.35;

/// Cuántas piezas de tamaño `size` caben en `span` con al menos `gap` entre ellas.
fn fit_count(span: f64, size: f64, gap: f64) -> usize {
    if size <= 0.0 || size > span {
        return 0;
    }
    ((span + gap) / (size + gap)).floor().max(1.0) as usize
}

/// Posiciones (inicio de cada pieza) repartiendo el sobrante por igual entre piezas.
fn spread(start: f64, span: f64, size: f64, n: usize) -> Vec<f64> {
    if n <= 1 {
        return vec![start];
    }
    let gap = (span - n as f64 * size) / (n - 1) as f64;
    (0..n).map(|i| start + i as f64 * (size + gap)).collect()
}

/// Llena `area` con el mayor número posible de copias de la selección: prueba la orientación
/// original y girada 90°, deja al menos `min_gap` entre copias y reparte el sobrante por
/// igual (las copias tocan los bordes del área). Devuelve (originales colocados en la
/// primera celda, copias nuevas).
pub fn fill_area(shapes: &[Shape], area: Rect, min_gap: f64) -> (Vec<Shape>, Vec<Shape>) {
    let Some(b) = selection_bounds(shapes) else {
        return (shapes.to_vec(), Vec::new());
    };
    if b.width() <= 0.0 || b.height() <= 0.0 {
        return (shapes.to_vec(), Vec::new());
    }
    let layout = |w: f64, h: f64| {
        let cols = fit_count(area.width(), w, min_gap);
        let rows = fit_count(area.height(), h, min_gap);
        (cols, rows)
    };
    let (c0, r0) = layout(b.width(), b.height());
    let (c1, r1) = layout(b.height(), b.width());
    // Girar solo si caben más (en empate se conserva la orientación original).
    let base = if c1 * r1 > c0 * r0 {
        rotate_group(shapes, 90.0)
    } else {
        shapes.to_vec()
    };
    let Some(bb) = selection_bounds(&base) else {
        return (shapes.to_vec(), Vec::new());
    };
    let (cols, rows) = layout(bb.width(), bb.height());
    if cols == 0 || rows == 0 {
        return (shapes.to_vec(), Vec::new());
    }
    let xs = spread(area.x0, area.width(), bb.width(), cols);
    let ys = spread(area.y0, area.height(), bb.height(), rows);
    let first = translate(&base, Point::new(xs[0], ys[0]) - bb.origin());
    let mut copies = Vec::new();
    for (r, y) in ys.iter().enumerate() {
        for (c, x) in xs.iter().enumerate() {
            if r == 0 && c == 0 {
                continue;
            }
            copies.extend(fresh(translate(&first, Vec2::new(x - xs[0], y - ys[0]))));
        }
    }
    (first, copies)
}

/// Cómo se reparten las copias a lo largo de la guía.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSpacing {
    /// Número total de copias, repartidas por igual en el tramo.
    Count(usize),
    /// Distancia fija entre copias (mm).
    Step(f64),
}

/// Opciones de «Objeto en ruta».
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathOptions {
    /// Girar cada copia según la dirección de la guía.
    pub perpendicular: bool,
    /// Giro inicial (grados).
    pub start_angle: f64,
    /// Giro añadido en cada copia sucesiva (grados).
    pub angle_step: f64,
    /// Distancia desde el inicio de la guía (mm).
    pub start: f64,
    /// Longitud del tramo usado (mm); 0 = hasta el final.
    pub section: f64,
    pub spacing: PathSpacing,
}

/// Longitud de una polilínea (cerrada si `closed`).
pub fn polyline_length(pts: &[Point], closed: bool) -> f64 {
    let mut len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    if closed && pts.len() > 1 {
        len += pts[pts.len() - 1].distance(pts[0]);
    }
    len
}

/// Punto y dirección (grados) a la distancia `s` sobre la polilínea.
pub fn point_at(pts: &[Point], closed: bool, s: f64) -> Option<(Point, f64)> {
    if pts.len() < 2 {
        return None;
    }
    let total = polyline_length(pts, closed);
    let mut s = if closed {
        s.rem_euclid(total.max(1e-9))
    } else {
        s.clamp(0.0, total)
    };
    let n = pts.len();
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let l = a.distance(b);
        if s < l || i == segs - 1 {
            let t = if l > 0.0 {
                (s / l).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = b - a;
            return Some((a + d * t, d.y.atan2(d.x).to_degrees()));
        }
        s -= l;
    }
    None
}

/// Copias del grupo `objects` repartidas a lo largo de la guía (su centro sobre la línea).
pub fn along_path(objects: &[Shape], guide: &[Point], closed: bool, o: &PathOptions) -> Vec<Shape> {
    let Some(b) = selection_bounds(objects) else {
        return Vec::new();
    };
    let total = polyline_length(guide, closed);
    if total <= 0.0 {
        return Vec::new();
    }
    let start = o.start.clamp(0.0, total);
    let section = if o.section > 0.0 {
        o.section.min(total - start)
    } else {
        total - start
    };
    let positions: Vec<f64> = match o.spacing {
        PathSpacing::Count(n) if n >= 1 => {
            // En una guía cerrada completa, la última copia no repite la primera.
            let whole_loop = closed && (section - total).abs() < 1e-6;
            let div = if n == 1 {
                1.0
            } else if whole_loop {
                n as f64
            } else {
                (n - 1) as f64
            };
            (0..n).map(|i| start + section * i as f64 / div).collect()
        }
        PathSpacing::Step(step) if step > 0.0 => {
            let n = (section / step).floor() as usize + 1;
            (0..n.min(10_000))
                .map(|i| start + step * i as f64)
                .collect()
        }
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for (i, s) in positions.into_iter().enumerate() {
        let Some((p, dir)) = point_at(guide, closed, s) else {
            continue;
        };
        let angle =
            if o.perpendicular { dir } else { 0.0 } + o.start_angle + o.angle_step * i as f64;
        let placed = translate(objects, p - b.center());
        out.extend(fresh(if angle != 0.0 {
            rotate_group(&placed, angle)
        } else {
            placed
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::ShapeKind;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Shape {
        Shape::new(
            1,
            "r",
            Rect::new(x0, y0, x1, y1),
            ShapeKind::Rect { corner_radius: 0.0 },
        )
    }

    #[test]
    fn adjacent_copies_touch_without_gap() {
        let s = vec![rect(10.0, 10.0, 30.0, 20.0)];
        let c = adjacent_copies(&s, Side::Right, 2);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].rect.x0, 30.0);
        assert_eq!(c[1].rect.x0, 50.0);
        assert!(c.iter().all(|s| s.id == 0));
        let up = adjacent_copies(&s, Side::Up, 1);
        assert_eq!(up[0].rect.y1, 10.0);
    }

    #[test]
    fn rotate_copies_use_half_turn_steps() {
        let s = vec![rect(0.0, 0.0, 40.0, 10.0)];
        let c = rotate_copies(&s, 4);
        let angles: Vec<f64> = c.iter().map(|s| s.rotation).collect();
        assert_eq!(angles, vec![45.0, 90.0, 135.0]);
        assert!(c.iter().all(|x| x.center() == s[0].center()));
    }

    #[test]
    fn fill_area_tiles_the_page() {
        let s = vec![rect(50.0, 50.0, 70.0, 60.0)];
        let (first, copies) = fill_area(&s, Rect::new(0.0, 0.0, 100.0, 35.0), 0.0);
        assert_eq!(first[0].rect.origin(), Point::new(0.0, 0.0));
        // 5 columnas × 3 filas = 15 objetos.
        assert_eq!(copies.len() + 1, 15);
    }

    #[test]
    fn fill_page_like_the_reference() {
        // El caso medido en la referencia: rectángulo de 29,8 × 23 mm en una página
        // de 210 × 297 mm. Girado caben 7 × 8 = 56; sin girar, solo 5 × 10 = 50.
        let s = vec![rect(14.77, 13.63, 44.57, 36.63)];
        let m = FILL_MARGIN_MM;
        let area = Rect::new(m, m, 210.0 - m, 297.0 - m);
        let (first, copies) = fill_area(&s, area, FILL_MIN_GAP_MM);
        assert_eq!(copies.len() + 1, 56);
        assert_eq!(first[0].rotation, 90.0);
        let all: Vec<Shape> = first.iter().chain(&copies).cloned().collect();
        let b = selection_bounds(&all).unwrap();
        // Las copias tocan los bordes del área.
        assert!((b.x0 - area.x0).abs() < 1e-6 && (b.x1 - area.x1).abs() < 1e-6);
        assert!((b.y0 - area.y0).abs() < 1e-6 && (b.y1 - area.y1).abs() < 1e-6);
    }

    #[test]
    fn offset_copies_accumulate_rotation() {
        let s = vec![rect(0.0, 0.0, 10.0, 10.0)];
        let c = offset_copies(&s, Vec2::new(5.0, 0.0), 3, 30.0);
        let r: Vec<f64> = c.iter().map(|s| s.rotation).collect();
        assert_eq!(r, vec![30.0, 60.0, 90.0]);
        assert_eq!(c[2].center().x, 5.0 + 15.0);
    }

    #[test]
    fn along_square_path() {
        let guide = vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(100.0, 100.0),
            Point::new(0.0, 100.0),
        ];
        let obj = vec![rect(0.0, 0.0, 4.0, 2.0)];
        let o = PathOptions {
            perpendicular: true,
            start_angle: 0.0,
            angle_step: 0.0,
            start: 0.0,
            section: 0.0,
            spacing: PathSpacing::Count(4),
        };
        let c = along_path(&obj, &guide, true, &o);
        assert_eq!(c.len(), 4);
        // Una copia por lado, en las esquinas, girada según la dirección del lado.
        assert!((c[1].center() - Point::new(100.0, 0.0)).hypot() < 1e-9);
        assert_eq!(c[1].rotation, 90.0);
        let step = along_path(
            &obj,
            &guide,
            true,
            &PathOptions {
                spacing: PathSpacing::Step(50.0),
                perpendicular: false,
                ..o
            },
        );
        assert_eq!(step.len(), 9);
        assert!(step.iter().all(|s| s.rotation == 0.0));
    }
}
