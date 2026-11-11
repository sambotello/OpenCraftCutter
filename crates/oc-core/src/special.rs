//! Herramientas especiales (como la referencia): rellenos de patrón para relieve, grabador,
//! lápiz térmico y tallador de cuero, y puntos de perforación del punzón de cuero.

use crate::ops::{self, Lines, Polygons};
use crate::shape::Shape;
use i_overlay::core::fill_rule::FillRule as IoFillRule;
use i_overlay::float::clip::FloatClip;
use i_overlay::string::clip::ClipRule;
use kurbo::Point;
use serde::{Deserialize, Serialize};

/// Patrón de relleno, en el orden de la referencia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Pattern {
    /// Sin relleno: solo el contorno.
    #[default]
    None,
    CrossHatch,
    Lined,
    Concentric,
    Spiral,
}

impl Pattern {
    pub const ALL: [Pattern; 5] = [
        Pattern::None,
        Pattern::CrossHatch,
        Pattern::Lined,
        Pattern::Concentric,
        Pattern::Spiral,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Pattern::None => "Sin relleno",
            Pattern::CrossHatch => "Trama cruzada",
            Pattern::Lined => "Líneas",
            Pattern::Concentric => "Concéntrico",
            Pattern::Spiral => "Espiral",
        }
    }
}

/// Herramienta a la que pertenece un relleno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tool {
    Emboss,
    ScoreEmboss,
    Engraver,
    HeatPen,
    LeatherCarver,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Emboss => "Relieve",
            Tool::ScoreEmboss => "Puntuar y grabar en relieve",
            Tool::Engraver => "Grabador eléctrico",
            Tool::HeatPen => "Lápiz térmico",
            Tool::LeatherCarver => "Tallador de cuero",
        }
    }
}

/// Relleno de herramienta especial aplicado a una forma (editable hasta «liberarlo»).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToolFill {
    pub tool: Tool,
    pub pattern: Pattern,
    pub spacing_mm: f64,
    pub angle_deg: f64,
    /// Solo relieve: `true` = relieve (se envía reflejado), `false` = bajo relieve.
    pub emboss: bool,
    /// Pasadas de la herramienta (puntuar y grabar en relieve).
    pub passes: u32,
}

/// Líneas del patrón dentro de la forma (mm, mundo).
pub fn pattern_lines(s: &Shape, pattern: Pattern, spacing_mm: f64, angle_deg: f64) -> Lines {
    let polys = ops::polygons_of(s);
    if polys.is_empty() || spacing_mm <= 0.0 {
        return Vec::new();
    }
    match pattern {
        Pattern::None => Vec::new(),
        Pattern::Lined => hatch_lines(&polys, angle_deg, spacing_mm, false),
        Pattern::CrossHatch => hatch_lines(&polys, angle_deg, spacing_mm, true),
        Pattern::Concentric => concentric_lines(s, spacing_mm),
        Pattern::Spiral => spiral_lines(&polys, spacing_mm, angle_deg),
    }
}

fn bounds(polys: &Polygons) -> Option<kurbo::Rect> {
    crate::edit::bounds_of(
        polys
            .iter()
            .flatten()
            .flatten()
            .map(|p| Point::new(p[0], p[1])),
    )
}

fn clip(lines: Lines, polys: &Polygons) -> Lines {
    let rule = ClipRule {
        invert: false,
        boundary_included: false,
    };
    lines.clip_by(polys, IoFillRule::EvenOdd, rule)
}

fn hatch_lines(polys: &Polygons, angle: f64, spacing: f64, cross: bool) -> Lines {
    let Some(b) = bounds(polys) else {
        return Vec::new();
    };
    let diag = b.width().hypot(b.height()) + spacing;
    let c = b.center();
    let mut lines: Lines = Vec::new();
    let angles: &[f64] = if cross { &[0.0, 90.0] } else { &[0.0] };
    for extra in angles {
        let (sin, cos) = (angle + extra).to_radians().sin_cos();
        let n = (diag / spacing).ceil() as i64;
        for i in -n..=n {
            let off = i as f64 * spacing;
            let o = [c.x - sin * off, c.y + cos * off];
            lines.push(vec![
                [o[0] - cos * diag, o[1] - sin * diag],
                [o[0] + cos * diag, o[1] + sin * diag],
            ]);
        }
    }
    clip(lines, polys)
}

/// Contornos paralelos hacia dentro cada `spacing` (el exterior incluido).
fn concentric_lines(s: &Shape, spacing: f64) -> Lines {
    let mut out = Vec::new();
    let mut k = 1;
    while let Some(inner) = ops::offset(s, -spacing * k as f64, ops::Join::Miter) {
        let contours = ops::closed_contours(&inner);
        if contours.is_empty() || k > 2000 {
            break;
        }
        for mut c in contours {
            if let Some(&first) = c.first() {
                c.push(first);
            }
            out.push(c);
        }
        k += 1;
    }
    out
}

/// Espiral de Arquímedes centrada en la forma (paso = `spacing`), recortada a la forma.
fn spiral_lines(polys: &Polygons, spacing: f64, angle: f64) -> Lines {
    let Some(b) = bounds(polys) else {
        return Vec::new();
    };
    let c = b.center();
    let r_max = (b.width().hypot(b.height())) / 2.0 + spacing;
    let turns = r_max / spacing;
    // Segmentos de ~0,5 mm.
    let mut pts = Vec::new();
    let mut theta: f64 = 0.0;
    let theta_max = turns * std::f64::consts::TAU;
    let a0 = angle.to_radians();
    while theta <= theta_max {
        let r = spacing * theta / std::f64::consts::TAU;
        pts.push([c.x + r * (theta + a0).cos(), c.y + r * (theta + a0).sin()]);
        let step = if r < 0.5 { 0.2 } else { (0.5 / r).min(0.2) };
        theta += step;
    }
    clip(vec![pts], polys)
}

/// Centros de los puntos de perforación a lo largo de cada contorno, cada `spacing` mm.
pub fn punch_points(s: &Shape, spacing_mm: f64) -> Vec<Point> {
    if spacing_mm <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut paths: Vec<(Vec<[f64; 2]>, bool)> = ops::closed_contours(s)
        .into_iter()
        .map(|c| (c, true))
        .collect();
    paths.extend(ops::open_paths(s).into_iter().map(|c| (c, false)));
    for (c, closed) in paths {
        let pts: Vec<Point> = c.iter().map(|p| Point::new(p[0], p[1])).collect();
        let total = crate::replicate::polyline_length(&pts, closed);
        if total <= 0.0 {
            continue;
        }
        // Reparto uniforme: el paso real se ajusta para cerrar sin huecos.
        let n = if closed {
            (total / spacing_mm).round().max(1.0) as usize
        } else {
            (total / spacing_mm).round() as usize + 1
        };
        let step = if closed {
            total / n as f64
        } else {
            total / (n.max(2) - 1) as f64
        };
        for i in 0..n {
            if let Some((p, _)) = crate::replicate::point_at(&pts, closed, step * i as f64) {
                out.push(p);
            }
        }
    }
    out
}

/// Diámetros de punta del punzón de cuero (mm).
pub const PUNCH_SIZES: [f64; 3] = [1.0, 3.0, 5.0];

/// Círculo (polilínea cerrada) de diámetro `d` centrado en `c`.
pub fn circle(c: Point, d: f64) -> Vec<Point> {
    (0..24)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 24.0;
            Point::new(c.x + d / 2.0 * a.cos(), c.y + d / 2.0 * a.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::ShapeKind;

    fn rect() -> Shape {
        Shape::new(
            1,
            "r",
            kurbo::Rect::new(0.0, 0.0, 60.0, 50.0),
            ShapeKind::Rect { corner_radius: 0.0 },
        )
    }

    fn inside(lines: &Lines) -> bool {
        lines
            .iter()
            .flatten()
            .all(|p| p[0] >= -1e-6 && p[0] <= 60.0 + 1e-6 && p[1] >= -1e-6 && p[1] <= 50.0 + 1e-6)
    }

    #[test]
    fn patterns_stay_inside_the_shape() {
        for p in Pattern::ALL {
            let l = pattern_lines(&rect(), p, 2.0, 0.0);
            assert!(inside(&l), "{p:?}");
            assert_eq!(l.is_empty(), p == Pattern::None, "{p:?}");
        }
    }

    #[test]
    fn line_counts_follow_spacing() {
        let lined = pattern_lines(&rect(), Pattern::Lined, 2.0, 0.0);
        // 50 mm de alto / 2 mm ≈ 25 líneas horizontales.
        assert!((24..=26).contains(&lined.len()), "{}", lined.len());
        let cross = pattern_lines(&rect(), Pattern::CrossHatch, 2.0, 0.0);
        assert!((53..=57).contains(&cross.len()), "{}", cross.len());
        let conc = pattern_lines(&rect(), Pattern::Concentric, 2.0, 0.0);
        // Contornos hasta el centro: 25 mm / 2 mm ≈ 12.
        assert!((11..=13).contains(&conc.len()), "{}", conc.len());
    }

    #[test]
    fn punch_points_evenly_spaced_on_outline() {
        let pts = punch_points(&rect(), 3.5);
        // Perímetro 220 mm / 3,5 mm ≈ 63 puntos.
        assert!((62..=64).contains(&pts.len()), "{}", pts.len());
        let d = pts[0].distance(pts[1]);
        assert!((d - 220.0 / pts.len() as f64).abs() < 1e-6);
    }
}
