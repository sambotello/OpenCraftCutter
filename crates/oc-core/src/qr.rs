//! Códigos QR como formas: los módulos oscuros contiguos se funden en un solo contorno, para
//! cortar o dibujar pocas líneas en vez de cada cuadradito.

use crate::shape::{FillRule, Shape, ShapeKind};
use i_overlay::core::fill_rule::FillRule as IoFillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use kurbo::Point;
use qrcodegen::{QrCode, QrCodeEcc};

/// Código QR de `text` de `size_mm` de lado (sin zona de silencio) con su esquina en
/// `origin`. `None` si el texto no cabe en un QR.
pub fn qr_shape(text: &str, size_mm: f64, origin: Point) -> Option<Shape> {
    let qr = QrCode::encode_text(text, QrCodeEcc::Medium).ok()?;
    let n = qr.size();
    let m = size_mm / n as f64;
    let squares: Vec<Vec<[f64; 2]>> = (0..n)
        .flat_map(|y| (0..n).map(move |x| (x, y)))
        .filter(|&(x, y)| qr.get_module(x, y))
        .map(|(x, y)| {
            let (x0, y0) = (origin.x + x as f64 * m, origin.y + y as f64 * m);
            vec![[x0, y0], [x0 + m, y0], [x0 + m, y0 + m], [x0, y0 + m]]
        })
        .collect();
    let empty: Vec<Vec<[f64; 2]>> = Vec::new();
    let merged = squares.overlay(&empty, OverlayRule::Subject, IoFillRule::NonZero);
    let polys: Vec<(Vec<Point>, bool)> = merged
        .iter()
        .flatten()
        .map(|c| (c.iter().map(|p| Point::new(p[0], p[1])).collect(), true))
        .collect();
    let (rect, subpaths) = Shape::normalize_subpaths(&polys);
    let mut s = Shape::new(
        0,
        format!("Código QR ({})", short(text)),
        rect,
        ShapeKind::Path {
            subpaths,
            fill_rule: FillRule::EvenOdd,
        },
    );
    s.fill = Some(crate::Color::rgb(0, 0, 0));
    s.stroke.color = crate::Color::rgb(0, 0, 0);
    Some(s)
}

fn short(t: &str) -> String {
    let mut s: String = t.chars().take(24).collect();
    if t.chars().count() > 24 {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_is_one_merged_shape_of_the_right_size() {
        let s = qr_shape("https://example.com", 30.0, Point::new(10.0, 20.0)).unwrap();
        // Los tres localizadores llegan a las esquinas: la caja es todo el código.
        assert!((s.rect.width() - 30.0).abs() < 1e-6 && (s.rect.height() - 30.0).abs() < 1e-6);
        assert!((s.rect.x0 - 10.0).abs() < 1e-6 && (s.rect.y0 - 20.0).abs() < 1e-6);
        // Fundido: muchos menos contornos que módulos oscuros.
        let qr = QrCode::encode_text("https://example.com", QrCodeEcc::Medium).unwrap();
        let dark = (0..qr.size())
            .flat_map(|y| (0..qr.size()).map(move |x| (x, y)))
            .filter(|&(x, y)| qr.get_module(x, y))
            .count();
        let contours = s.world_subpaths().len();
        assert!(
            contours * 3 < dark,
            "{contours} contornos para {dark} módulos"
        );
        assert!(s.name.starts_with("Código QR (https://example.com"));
    }

    #[test]
    fn too_long_text_is_rejected() {
        assert!(qr_shape(&"x".repeat(5000), 30.0, Point::ZERO).is_none());
    }
}
