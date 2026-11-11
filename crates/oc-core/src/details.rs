//! Detalles demasiado finos para cortar: zonas de las piezas más estrechas que el mínimo que
//! aguanta el material (se rompen al cortar o al pelar). Se hallan con una apertura
//! morfológica: se encoge la pieza media anchura y se vuelve a agrandar; lo que no vuelve es
//! más fino que el mínimo.

use crate::ops::Polygons;
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::style::{LineJoin, OutlineStyle};

/// Zonas de las piezas (contornos cerrados en mm, par-impar) más finas que `min_mm`. Se
/// descartan las virutas de las esquinas, que son de esperar en cualquier forma con picos.
pub fn thin_parts(contours: &[Vec<[f64; 2]>], min_mm: f64) -> Polygons {
    if contours.is_empty() || min_mm <= 0.0 {
        return Vec::new();
    }
    let r = min_mm / 2.0;
    let empty: Vec<Vec<[f64; 2]>> = Vec::new();
    let pieces = contours.overlay(&empty, OverlayRule::Subject, FillRule::EvenOdd);
    let style = |d: f64| OutlineStyle {
        outer_offset: d,
        inner_offset: d,
        join: LineJoin::Round(0.1),
        ..OutlineStyle::new(d)
    };
    let shrunk = pieces.outline(&style(-r));
    let opened = if shrunk.is_empty() {
        Vec::new()
    } else {
        shrunk.outline(&style(r))
    };
    // Piezas que desaparecen enteras (puntos, trocitos): siempre cuentan.
    let mut out: Polygons = Vec::new();
    let mut kept: Polygons = Vec::new();
    for p in pieces {
        let left =
            std::slice::from_ref(&p).overlay(&opened, OverlayRule::Intersect, FillRule::NonZero);
        if left.is_empty() {
            out.push(p);
        } else {
            kept.push(p);
        }
    }
    // En el resto, las zonas que no vuelven, sin las virutas de las esquinas.
    let min_area = 1.5 * r * r;
    out.extend(
        kept.overlay(&opened, OverlayRule::Difference, FillRule::NonZero)
            .into_iter()
            .filter(|p| p.first().map(|c| area(c).abs()).unwrap_or(0.0) > min_area),
    );
    out
}

fn area(c: &[[f64; 2]]) -> f64 {
    let n = c.len();
    (0..n)
        .map(|i| {
            let (a, b) = (c[i], c[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    #[test]
    fn wide_shapes_have_no_thin_parts() {
        assert!(thin_parts(&[rect(0.0, 0.0, 20.0, 20.0)], 1.0).is_empty());
        // Un anillo de 3 mm de grosor tampoco.
        let ring = [rect(0.0, 0.0, 20.0, 20.0), rect(3.0, 3.0, 17.0, 17.0)];
        assert!(thin_parts(&ring, 1.0).is_empty());
    }

    #[test]
    fn thin_bar_and_tiny_dot_are_found() {
        // Barra de 0,5 mm × 10 mm y punto de 0,6 mm, junto a una pieza buena.
        let shapes = [
            rect(0.0, 0.0, 20.0, 20.0),
            rect(30.0, 0.0, 30.5, 10.0),
            rect(40.0, 0.0, 40.6, 0.6),
        ];
        let thin = thin_parts(&shapes, 1.0);
        assert_eq!(thin.len(), 2, "{thin:?}");
        assert!(thin.iter().all(|p| p[0].iter().all(|q| q[0] >= 29.9)));
        // Con un mínimo de 0,4 mm ya no son demasiado finas.
        assert!(thin_parts(&shapes, 0.4).is_empty());
    }

    #[test]
    fn thin_ring_is_found() {
        // Anillo de 0,5 mm: el hueco es par-impar.
        let ring = [rect(0.0, 0.0, 20.0, 20.0), rect(0.5, 0.5, 19.5, 19.5)];
        assert!(!thin_parts(&ring, 1.0).is_empty());
    }
}
