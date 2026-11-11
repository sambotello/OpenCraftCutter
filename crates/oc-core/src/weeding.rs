//! Desbarbado de vinil: una caja alrededor del diseño y líneas que parten el sobrante en
//! trozos para pelarlo sin arrastrar las piezas pequeñas. Las líneas solo pasan por los
//! huecos entre piezas, nunca por encima.

use kurbo::{Point, Rect};

/// Caja de desbarbado y líneas (segmentos en mm).
#[derive(Debug, Clone, PartialEq)]
pub struct Weeding {
    pub frame: Rect,
    pub lines: Vec<[Point; 2]>,
}

/// `pieces`: rectángulos que ocupan las piezas del diseño (cada letra o isla por separado).
/// `margin`: distancia de la caja al diseño. `min_gap`: hueco mínimo para trazar una línea.
pub fn weeding(pieces: &[Rect], margin: f64, lines: bool, min_gap: f64) -> Option<Weeding> {
    let all = pieces.iter().copied().reduce(|a, b| a.union(b))?;
    let frame = all.inflate(margin, margin);
    let mut out = Vec::new();
    if lines {
        // Líneas verticales en los huecos entre columnas de piezas, de arriba abajo de la caja.
        for x in gaps(pieces.iter().map(|r| (r.x0, r.x1)), min_gap) {
            out.push([Point::new(x, frame.y0), Point::new(x, frame.y1)]);
        }
        // Y horizontales entre filas.
        for y in gaps(pieces.iter().map(|r| (r.y0, r.y1)), min_gap) {
            out.push([Point::new(frame.x0, y), Point::new(frame.x1, y)]);
        }
    }
    Some(Weeding { frame, lines: out })
}

/// Centros de los huecos de al menos `min_gap` entre intervalos (una vez fundidos los que se
/// solapan).
fn gaps(intervals: impl Iterator<Item = (f64, f64)>, min_gap: f64) -> Vec<f64> {
    let mut v: Vec<(f64, f64)> = intervals.collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (a, b) in v {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    merged
        .windows(2)
        .filter(|w| w[1].0 - w[0].1 >= min_gap)
        .map(|w| (w[0].1 + w[1].0) / 2.0)
        .collect()
}

/// Cruz «+» cerrada de `size` de lado y brazos de `arm` de grosor, centrada en `c`.
pub fn plus(c: Point, size: f64, arm: f64) -> Vec<Point> {
    let (h, a) = (size / 2.0, arm / 2.0);
    [
        (-a, -h),
        (a, -h),
        (a, -a),
        (h, -a),
        (h, a),
        (a, a),
        (a, h),
        (-a, h),
        (-a, a),
        (-h, a),
        (-h, -a),
        (-a, -a),
    ]
    .iter()
    .map(|&(x, y)| Point::new(c.x + x, c.y + y))
    .collect()
}

/// Marcas de alineación para capas: una cruz a cada lado del diseño, a `gap` de su caja y a
/// media altura. Las mismas en cada capa de color.
pub fn alignment_marks(design: Rect, gap: f64, size: f64) -> Vec<Vec<Point>> {
    let y = design.center().y;
    // Brazos de 1,5 mm o más: más finos se romperían al pelar (ver el aviso de detalles).
    let arm = (size / 4.0).max(1.5);
    vec![
        plus(Point::new(design.x0 - gap - size / 2.0, y), size, arm),
        plus(Point::new(design.x1 + gap + size / 2.0, y), size, arm),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_surrounds_the_design_with_margin() {
        let w = weeding(&[Rect::new(10.0, 10.0, 20.0, 30.0)], 3.0, true, 1.0).unwrap();
        assert_eq!(w.frame, Rect::new(7.0, 7.0, 23.0, 33.0));
        assert!(w.lines.is_empty(), "una sola pieza: sin líneas");
        assert!(weeding(&[], 3.0, true, 1.0).is_none());
    }

    #[test]
    fn lines_go_between_pieces_never_across() {
        // Tres «letras» en fila; las dos primeras se solapan en x (no hay hueco entre ellas).
        let pieces = [
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Rect::new(8.0, 0.0, 14.0, 10.0),
            Rect::new(20.0, 0.0, 30.0, 10.0),
            // Una segunda fila debajo.
            Rect::new(0.0, 16.0, 30.0, 20.0),
        ];
        let w = weeding(&pieces, 2.0, true, 1.0).unwrap();
        let verticals: Vec<f64> = w
            .lines
            .iter()
            .filter(|l| l[0].x == l[1].x)
            .map(|l| l[0].x)
            .collect();
        let horizontals: Vec<f64> = w
            .lines
            .iter()
            .filter(|l| l[0].y == l[1].y)
            .map(|l| l[0].y)
            .collect();
        // En x, la fila de abajo ocupa todo: no hay hueco vertical libre de arriba abajo.
        assert!(verticals.is_empty(), "{verticals:?}");
        assert_eq!(horizontals, vec![13.0]);
        // Sin la fila de abajo sí hay una vertical, en el centro del hueco 14–20.
        let w = weeding(&pieces[..3], 2.0, true, 1.0).unwrap();
        assert_eq!(w.lines.len(), 1);
        assert_eq!(w.lines[0][0].x, 17.0);
        // Ninguna línea toca una pieza.
        for l in &w.lines {
            for p in &pieces[..3] {
                assert!(!(l[0].x > p.x0 && l[0].x < p.x1));
            }
        }
        // Huecos menores que el mínimo no llevan línea.
        assert!(
            weeding(&pieces[..3], 2.0, true, 7.0)
                .unwrap()
                .lines
                .is_empty()
        );
    }

    #[test]
    fn alignment_marks_sit_outside_on_both_sides() {
        let d = Rect::new(20.0, 10.0, 80.0, 50.0);
        let m = alignment_marks(d, 5.0, 6.0);
        assert_eq!(m.len(), 2);
        let bx = |c: &Vec<Point>| crate::edit::bounds_of(c.iter().copied()).unwrap();
        assert_eq!(bx(&m[0]), Rect::new(9.0, 27.0, 15.0, 33.0));
        assert_eq!(bx(&m[1]), Rect::new(85.0, 27.0, 91.0, 33.0));
        assert_eq!(m[0].len(), 12, "cruz cerrada de 12 vértices");
    }
}
