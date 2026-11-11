//! Diseños más grandes que el tapete: se reparten en partes del tamaño del área útil, con un
//! solape para unirlas al aplicarlas.

use kurbo::{Point, Rect};

/// Rectángulos de las partes (en coordenadas del diseño), por filas. Cada parte mide como
/// mucho `w`×`h` y se solapa `overlap` con sus vecinas.
pub fn tile_grid(design: Rect, w: f64, h: f64, overlap: f64) -> Vec<Rect> {
    if w <= overlap || h <= overlap {
        return Vec::new();
    }
    let count = |len: f64, size: f64| ((len - overlap) / (size - overlap)).ceil().max(1.0) as usize;
    let (cols, rows) = (count(design.width(), w), count(design.height(), h));
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let x0 = design.x0 + c as f64 * (w - overlap);
            let y0 = design.y0 + r as f64 * (h - overlap);
            out.push(Rect::new(
                x0,
                y0,
                (x0 + w).min(design.x1),
                (y0 + h).min(design.y1),
            ));
        }
    }
    out
}

/// Recorta una polilínea abierta por un rectángulo: los tramos que quedan dentro.
pub fn clip_polyline(pts: &[Point], r: Rect) -> Vec<Vec<Point>> {
    let mut out: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    for w in pts.windows(2) {
        match clip_segment(w[0], w[1], r) {
            Some((a, b)) => {
                if cur.last().is_none_or(|l| l.distance(a) > 1e-9) {
                    if cur.len() >= 2 {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur = vec![a];
                }
                cur.push(b);
                // Si el segmento sale del rectángulo, el tramo termina aquí.
                if b.distance(w[1]) > 1e-9 {
                    out.push(std::mem::take(&mut cur));
                }
            }
            None => {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// Parte de un segmento dentro de un rectángulo (Liang-Barsky).
fn clip_segment(a: Point, b: Point, r: Rect) -> Option<(Point, Point)> {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-d.x, a.x - r.x0),
        (d.x, r.x1 - a.x),
        (-d.y, a.y - r.y0),
        (d.y, r.y1 - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return None;
            }
        }
    }
    (t1 - t0 > 1e-12).then(|| (a + d * t0, a + d * t1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_covers_the_design_with_overlap() {
        // 500 × 250 mm en partes de 280 × 280 con 10 mm de solape: 2 columnas, 1 fila.
        let d = Rect::new(0.0, 0.0, 500.0, 250.0);
        let t = tile_grid(d, 280.0, 280.0, 10.0);
        assert_eq!(
            t,
            vec![
                Rect::new(0.0, 0.0, 280.0, 250.0),
                Rect::new(270.0, 0.0, 500.0, 250.0)
            ]
        );
        // Si cabe, una sola parte.
        assert_eq!(
            tile_grid(Rect::new(5.0, 5.0, 100.0, 100.0), 280.0, 280.0, 10.0).len(),
            1
        );
        // 3 × 2.
        assert_eq!(
            tile_grid(Rect::new(0.0, 0.0, 700.0, 400.0), 280.0, 280.0, 10.0).len(),
            6
        );
    }

    #[test]
    fn open_lines_are_clipped() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        // Entra, sale y vuelve a entrar.
        let line = [
            Point::new(-5.0, 5.0),
            Point::new(5.0, 5.0),
            Point::new(5.0, 15.0),
            Point::new(8.0, 15.0),
            Point::new(8.0, 2.0),
        ];
        let parts = clip_polyline(&line, r);
        assert_eq!(
            parts,
            vec![
                vec![
                    Point::new(0.0, 5.0),
                    Point::new(5.0, 5.0),
                    Point::new(5.0, 10.0)
                ],
                vec![Point::new(8.0, 10.0), Point::new(8.0, 2.0)],
            ]
        );
        assert!(clip_polyline(&[Point::new(20.0, 0.0), Point::new(30.0, 0.0)], r).is_empty());
    }
}
