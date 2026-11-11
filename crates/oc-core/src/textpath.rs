//! Texto sobre un trazado: cada letra se coloca a lo largo de la línea según su avance y se
//! gira con la dirección del trazado en ese punto.

use kurbo::{Point, Vec2};

/// Letra ya trazada: posición en la línea de texto, avance y contornos (base en y = 0, lo de
/// arriba con y negativa).
pub struct Glyph {
    pub x_mm: f64,
    pub advance_mm: f64,
    pub outline: Vec<(Vec<Point>, bool)>,
}

/// Coloca las letras sobre `path` (polilínea en mm).
/// - `position`: 0–1, dónde cae el centro del texto a lo largo del trazado.
/// - `distance_mm`: separación de la base de las letras respecto a la línea.
/// - `flip`: las letras van del otro lado (por dentro de un círculo).
pub fn place_on_path(
    glyphs: &[Glyph],
    path: &[Point],
    closed: bool,
    position: f64,
    distance_mm: f64,
    flip: bool,
) -> Vec<(Vec<Point>, bool)> {
    let mut pts: Vec<Point> = path.to_vec();
    if closed && pts.len() > 2 && pts.first() != pts.last() {
        pts.push(pts[0]);
    }
    if flip {
        pts.reverse();
    }
    if pts.len() < 2 {
        return Vec::new();
    }
    let mut acc = vec![0.0];
    for w in pts.windows(2) {
        acc.push(acc.last().unwrap() + w[0].distance(w[1]));
    }
    let total = *acc.last().unwrap();
    if total <= 0.0 {
        return Vec::new();
    }
    let width = glyphs.last().map(|g| g.x_mm + g.advance_mm).unwrap_or(0.0);
    let start = position.clamp(0.0, 1.0) * total - width / 2.0;
    // Punto y dirección a una distancia `s` del inicio (en un trazado cerrado da la vuelta;
    // en uno abierto sigue recto por los extremos).
    let at = |s: f64| -> (Point, Vec2) {
        let s = if closed { s.rem_euclid(total) } else { s };
        let i = match acc.binary_search_by(|a| a.total_cmp(&s)) {
            Ok(i) => i.min(pts.len() - 2),
            Err(i) => i.saturating_sub(1).min(pts.len() - 2),
        };
        let (a, b) = (pts[i], pts[i + 1]);
        let seg = acc[i + 1] - acc[i];
        let dir = if seg > 0.0 {
            (b - a) / seg
        } else {
            Vec2::new(1.0, 0.0)
        };
        (a + dir * (s - acc[i]), dir)
    };
    let mut out = Vec::new();
    for g in glyphs {
        let mid = g.x_mm + g.advance_mm / 2.0;
        let (p, t) = at(start + mid);
        // Normal hacia «arriba» de las letras con el trazado de izquierda a derecha.
        let n = Vec2::new(-t.y, t.x);
        for (c, closed) in &g.outline {
            let placed = c
                .iter()
                .map(|q| {
                    let (u, v) = (q.x - mid, q.y - distance_mm);
                    p + t * u + n * v
                })
                .collect();
            out.push((placed, *closed));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Letra cuadrada de 2 mm de ancho y 3 de alto sobre la base.
    fn glyph(x: f64) -> Glyph {
        Glyph {
            x_mm: x,
            advance_mm: 2.0,
            outline: vec![(
                vec![
                    Point::new(x, 0.0),
                    Point::new(x + 2.0, 0.0),
                    Point::new(x + 2.0, -3.0),
                    Point::new(x, -3.0),
                ],
                true,
            )],
        }
    }

    #[test]
    fn straight_path_keeps_the_text_as_is_centered() {
        let glyphs = [glyph(0.0), glyph(2.0), glyph(4.0)];
        let line = [Point::new(0.0, 10.0), Point::new(100.0, 10.0)];
        let out = place_on_path(&glyphs, &line, false, 0.5, 0.0, false);
        // 6 mm de texto centrados en x = 50: de 47 a 53, sobre la línea (y de 7 a 10).
        let xs: Vec<f64> = out
            .iter()
            .flat_map(|(c, _)| c.iter().map(|p| p.x))
            .collect();
        let ys: Vec<f64> = out
            .iter()
            .flat_map(|(c, _)| c.iter().map(|p| p.y))
            .collect();
        let min = |v: &[f64]| v.iter().cloned().fold(f64::MAX, f64::min);
        let max = |v: &[f64]| v.iter().cloned().fold(f64::MIN, f64::max);
        assert!((min(&xs) - 47.0).abs() < 1e-9 && (max(&xs) - 53.0).abs() < 1e-9);
        assert!((min(&ys) - 7.0).abs() < 1e-9 && (max(&ys) - 10.0).abs() < 1e-9);
        // Del otro lado, las letras quedan por debajo de la línea.
        let flipped = place_on_path(&glyphs, &line, false, 0.5, 0.0, true);
        assert!(
            flipped
                .iter()
                .flat_map(|(c, _)| c.iter())
                .all(|p| p.y >= 10.0 - 1e-9)
        );
    }

    #[test]
    fn letters_follow_a_circle_outside() {
        // Círculo de radio 20 en el origen, recorrido en sentido horario en pantalla (y abajo).
        let circle: Vec<Point> = (0..360)
            .map(|i| {
                let a = (i as f64).to_radians();
                Point::new(20.0 * a.cos(), 20.0 * a.sin())
            })
            .collect();
        let glyphs: Vec<Glyph> = (0..5).map(|i| glyph(i as f64 * 2.0)).collect();
        let out = place_on_path(&glyphs, &circle, true, 0.75, 1.0, false);
        // Con la normal hacia fuera del círculo, las letras quedan entre r = 21 y r ≈ 24.
        for (c, _) in &out {
            for p in c {
                let r = p.to_vec2().hypot();
                assert!((20.9..24.2).contains(&r), "r = {r}");
            }
        }
        // Y por dentro al darles la vuelta.
        let inside = place_on_path(&glyphs, &circle, true, 0.75, 1.0, true);
        assert!(
            inside
                .iter()
                .flat_map(|(c, _)| c.iter())
                .all(|p| p.to_vec2().hypot() < 19.1)
        );
    }
}
