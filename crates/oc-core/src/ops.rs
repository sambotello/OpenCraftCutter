//! Operaciones geométricas sobre formas: booleanas (soldar, restar, intersecar, excluir, dividir),
//! desplazamiento (offset) y separar/combinar trazos compuestos. Basadas en `i_overlay`.

use crate::edit::rotation_about;
use crate::shape::{FillRule, Shape, ShapeKind};
use i_overlay::core::fill_rule::FillRule as IoFillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};
use kurbo::Point;

/// Polígonos en mm: cada forma es una lista de contornos (el primero exterior, el resto agujeros).
pub type Polygons = Vec<Vec<Vec<[f64; 2]>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    /// Unión de todas las formas.
    Weld,
    /// La forma inferior menos las superiores.
    Subtract,
    /// Intersección de todas.
    Intersect,
    /// Exclusión (xor).
    Exclude,
    /// Todas las piezas resultantes de cortar unas con otras.
    Divide,
    /// La forma superior se resta de cada una de las demás (que siguen separadas).
    SubtractFront,
    /// Cada forma inferior se queda solo con lo que cae dentro de la superior.
    Crop,
}

impl BoolOp {
    pub fn name(self) -> &'static str {
        match self {
            BoolOp::Weld => "Fusionar",
            BoolOp::Subtract => "Sustraer todo",
            BoolOp::Intersect => "Interseccionar",
            BoolOp::Exclude => "Excluir",
            BoolOp::Divide => "Dividir",
            BoolOp::SubtractFront => "Sustraer",
            BoolOp::Crop => "Recortar",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Join {
    Round,
    Miter,
    Bevel,
}

impl Join {
    pub const ALL: [Join; 3] = [Join::Round, Join::Miter, Join::Bevel];

    pub fn name(self) -> &'static str {
        match self {
            Join::Round => "Redondeado",
            Join::Miter => "En punta",
            Join::Bevel => "Biselado",
        }
    }

    fn style(self) -> LineJoin<f64> {
        match self {
            Join::Round => LineJoin::Round(0.15),
            Join::Miter => LineJoin::Miter(0.3),
            Join::Bevel => LineJoin::Bevel,
        }
    }
}

const ELLIPSE_SEGMENTS: usize = 96;

/// Contornos cerrados de una forma en mm (mundo, con rotación aplicada). Los textos no tienen contorno.
pub fn closed_contours(s: &Shape) -> Vec<Vec<[f64; 2]>> {
    let rot = rotation_about(s.center(), s.rotation);
    let w = |p: Point| {
        let q = rot * p;
        [q.x, q.y]
    };
    let r = s.rect;
    match &s.kind {
        ShapeKind::Rect { corner_radius } => {
            let rad = corner_radius.min(r.width() / 2.0).min(r.height() / 2.0);
            if rad <= 0.0 {
                return vec![vec![
                    w(Point::new(r.x0, r.y0)),
                    w(Point::new(r.x1, r.y0)),
                    w(Point::new(r.x1, r.y1)),
                    w(Point::new(r.x0, r.y1)),
                ]];
            }
            let mut out = Vec::new();
            let corners = [
                (r.x1 - rad, r.y0 + rad, -90.0),
                (r.x1 - rad, r.y1 - rad, 0.0),
                (r.x0 + rad, r.y1 - rad, 90.0),
                (r.x0 + rad, r.y0 + rad, 180.0),
            ];
            for (cx, cy, start) in corners {
                for k in 0..=12 {
                    let a = (start + 90.0 * k as f64 / 12.0_f64).to_radians();
                    out.push(w(Point::new(cx + rad * a.cos(), cy + rad * a.sin())));
                }
            }
            vec![out]
        }
        ShapeKind::Ellipse => {
            let c = r.center();
            let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
            vec![
                (0..ELLIPSE_SEGMENTS)
                    .map(|i| {
                        let a = std::f64::consts::TAU * i as f64 / ELLIPSE_SEGMENTS as f64;
                        w(Point::new(c.x + rx * a.cos(), c.y + ry * a.sin()))
                    })
                    .collect(),
            ]
        }
        ShapeKind::Path { .. } => s
            .world_subpaths()
            .into_iter()
            .filter(|(pts, closed)| *closed && pts.len() >= 3)
            .map(|(pts, _)| pts.into_iter().map(w).collect())
            .collect(),
        ShapeKind::Text { .. } | ShapeKind::Image { .. } => Vec::new(),
    }
}

/// Trazos abiertos de una forma (solo trazos): puntos en mm con rotación aplicada.
pub fn open_paths(s: &Shape) -> Vec<Vec<[f64; 2]>> {
    let rot = rotation_about(s.center(), s.rotation);
    s.world_subpaths()
        .into_iter()
        .filter(|(pts, closed)| !*closed && pts.len() >= 2)
        .map(|(pts, _)| {
            pts.into_iter()
                .map(|p| {
                    let q = rot * p;
                    [q.x, q.y]
                })
                .collect()
        })
        .collect()
}

fn fill_rule_of(s: &Shape) -> IoFillRule {
    match &s.kind {
        ShapeKind::Path {
            fill_rule: FillRule::EvenOdd,
            ..
        } => IoFillRule::EvenOdd,
        _ => IoFillRule::NonZero,
    }
}

/// Normaliza los contornos de una forma a polígonos limpios (sin autointersecciones, agujeros resueltos).
pub fn polygons_of(s: &Shape) -> Polygons {
    let contours = closed_contours(s);
    if contours.is_empty() {
        return Vec::new();
    }
    let empty: Vec<Vec<[f64; 2]>> = Vec::new();
    contours.overlay(&empty, OverlayRule::Subject, fill_rule_of(s))
}

/// Construye un trazo del documento a partir de polígonos, copiando estilo y nombre de `template`.
pub fn shape_from_polygons(template: &Shape, polys: &Polygons, name: &str) -> Option<Shape> {
    let world: Vec<(Vec<Point>, bool)> = polys
        .iter()
        .flatten()
        .filter(|c| c.len() >= 3)
        .map(|c| (c.iter().map(|p| Point::new(p[0], p[1])).collect(), true))
        .collect();
    if world.is_empty() {
        return None;
    }
    let (rect, subpaths) = Shape::normalize_subpaths(&world);
    let mut s = Shape::new(
        0,
        name,
        rect,
        ShapeKind::Path {
            subpaths,
            fill_rule: FillRule::EvenOdd,
        },
    );
    s.fill = template.fill;
    s.stroke = template.stroke;
    Some(s)
}

/// Aplica una operación booleana. `shapes` en orden de pila (la primera es la inferior).
/// Devuelve las formas resultantes (vacío si no hay nada) o `None` si la operación no aplica.
pub fn boolean(shapes: &[Shape], op: BoolOp) -> Option<Vec<Shape>> {
    let polys: Vec<Polygons> = shapes
        .iter()
        .map(polygons_of)
        .filter(|p| !p.is_empty())
        .collect();
    let template = &shapes[0];
    // Fusionar una sola forma une sus propios contornos solapados (p. ej., las letras de un
    // texto manuscrito que se tocan); los agujeros se conservan según su regla de relleno.
    if polys.len() == 1 && op == BoolOp::Weld {
        let name = format!("{} (soldado)", template.name);
        return Some(
            shape_from_polygons(template, &polys[0], &name)
                .into_iter()
                .collect(),
        );
    }
    if polys.len() < 2 {
        return None;
    }
    let rule = IoFillRule::EvenOdd;
    let combine = |rule_op: OverlayRule| -> Polygons {
        let mut acc = polys[0].clone();
        for p in &polys[1..] {
            acc = acc.overlay(p, rule_op, rule);
        }
        acc
    };
    let name = |suffix: &str| format!("{} ({suffix})", template.name);
    let result: Vec<Shape> = match op {
        BoolOp::Weld => {
            shape_from_polygons(template, &combine(OverlayRule::Union), &name("soldado"))
                .into_iter()
                .collect()
        }
        BoolOp::Subtract => shape_from_polygons(
            template,
            &combine(OverlayRule::Difference),
            &name("restado"),
        )
        .into_iter()
        .collect(),
        BoolOp::Intersect => shape_from_polygons(
            template,
            &combine(OverlayRule::Intersect),
            &name("intersección"),
        )
        .into_iter()
        .collect(),
        BoolOp::Exclude => {
            shape_from_polygons(template, &combine(OverlayRule::Xor), &name("exclusión"))
                .into_iter()
                .collect()
        }
        BoolOp::SubtractFront | BoolOp::Crop => {
            // La forma superior (la última de la pila) actúa sobre cada una de las demás.
            let top = polys.last().expect("al menos dos");
            let (rule_op, suffix) = if op == BoolOp::Crop {
                (OverlayRule::Intersect, "recortado")
            } else {
                (OverlayRule::Difference, "sustraído")
            };
            let polys_shapes: Vec<(&Shape, &Polygons)> = shapes
                .iter()
                .filter(|s| !polygons_of(s).is_empty())
                .zip(polys.iter())
                .collect();
            polys_shapes[..polys_shapes.len() - 1]
                .iter()
                .filter_map(|(sh, p)| {
                    let r = p.overlay(top, rule_op, rule);
                    shape_from_polygons(sh, &r, &format!("{} ({suffix})", sh.name))
                })
                .collect()
        }
        BoolOp::Divide => {
            // Piezas: para cada par de regiones, intersección y diferencias; con n formas, se parte
            // progresivamente cada pieza acumulada por la siguiente forma.
            let mut pieces: Vec<Polygons> = vec![polys[0].clone()];
            for p in &polys[1..] {
                let mut next: Vec<Polygons> = Vec::new();
                let mut remainder = p.clone();
                for piece in &pieces {
                    let inter = piece.overlay(p, OverlayRule::Intersect, rule);
                    let diff = piece.overlay(p, OverlayRule::Difference, rule);
                    if !inter.is_empty() {
                        next.push(inter);
                    }
                    if !diff.is_empty() {
                        next.push(diff);
                    }
                    remainder = remainder.overlay(piece, OverlayRule::Difference, rule);
                }
                if !remainder.is_empty() {
                    next.push(remainder);
                }
                pieces = next;
            }
            pieces
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    shape_from_polygons(
                        template,
                        p,
                        &format!("{} (pieza {})", template.name, i + 1),
                    )
                })
                .collect()
        }
    };
    Some(result)
}

/// Desplaza el contorno de una forma `distance_mm` hacia fuera (positivo) o dentro (negativo).
/// Los trazos abiertos producen el contorno de una línea de grosor `2·|distance|`.
pub fn offset(s: &Shape, distance_mm: f64, join: Join) -> Option<Shape> {
    offset_with_limit(s, distance_mm, join, 4.0)
}

/// Ángulo interior mínimo (rad) para una punta con límite `limit` (como `stroke-miterlimit`
/// de SVG: longitud de la punta ÷ grosor). Por debajo, la esquina se bisela.
fn miter_min_angle(limit: f64) -> f64 {
    let m = limit.max(1.0);
    (2.0 * (1.0 / m).asin()).clamp(0.01 * std::f64::consts::PI, 0.99 * std::f64::consts::PI)
}

/// Como [`offset`], con el límite de las esquinas en punta.
pub fn offset_with_limit(
    s: &Shape,
    distance_mm: f64,
    join: Join,
    miter_limit: f64,
) -> Option<Shape> {
    let line_join = match join {
        Join::Miter => LineJoin::Miter(miter_min_angle(miter_limit)),
        other => other.style(),
    };
    if distance_mm == 0.0 {
        return None;
    }
    let mut result: Polygons = Vec::new();
    let polys = polygons_of(s);
    if !polys.is_empty() {
        let style = OutlineStyle {
            outer_offset: distance_mm,
            inner_offset: distance_mm,
            join: line_join.clone(),
            ..OutlineStyle::new(distance_mm)
        };
        result.extend(polys.outline(&style));
    }
    for path in open_paths(s) {
        let style = StrokeStyle {
            width: 2.0 * distance_mm.abs(),
            start_cap: LineCap::Round(0.15),
            end_cap: LineCap::Round(0.15),
            join: line_join.clone(),
            ..StrokeStyle::new(2.0 * distance_mm.abs())
        };
        result.extend(path.stroke(style, false));
    }
    let name = format!("{} (desplazamiento)", s.name);
    let mut out = shape_from_polygons(s, &result, &name)?;
    out.fill = None;
    Some(out)
}

/// Separa en islas: cada contorno exterior con sus agujeros pasa a ser una forma.
pub fn break_apart(s: &Shape) -> Vec<Shape> {
    let polys = polygons_of(s);
    if polys.len() <= 1 {
        return vec![s.clone()];
    }
    polys
        .into_iter()
        .enumerate()
        .filter_map(|(i, island)| {
            shape_from_polygons(s, &vec![island], &format!("{} ({})", s.name, i + 1))
        })
        .collect()
}

/// Separa un trazo compuesto en una forma por subtrazo.
pub fn separate(s: &Shape) -> Vec<Shape> {
    let subpaths = s.world_subpaths();
    if subpaths.len() <= 1 {
        return vec![s.clone()];
    }
    let rot = rotation_about(s.center(), s.rotation);
    subpaths
        .into_iter()
        .enumerate()
        .map(|(i, (pts, closed))| {
            let world: Vec<Point> = pts.into_iter().map(|p| rot * p).collect();
            let (rect, sp) = Shape::normalize_subpaths(&[(world, closed)]);
            let mut n = Shape::new(
                0,
                format!("{} ({})", s.name, i + 1),
                rect,
                ShapeKind::Path {
                    subpaths: sp,
                    fill_rule: fill_rule_kind(s),
                },
            );
            n.fill = s.fill;
            n.stroke = s.stroke;
            n
        })
        .collect()
}

fn fill_rule_kind(s: &Shape) -> FillRule {
    match &s.kind {
        ShapeKind::Path { fill_rule, .. } => *fill_rule,
        _ => FillRule::NonZero,
    }
}

/// Combina varias formas en un solo trazo compuesto (sin operar: conserva todos los subtrazos).
pub fn combine(shapes: &[Shape]) -> Option<Shape> {
    let template = shapes.first()?;
    let mut world: Vec<(Vec<Point>, bool)> = Vec::new();
    for s in shapes {
        for c in closed_contours(s) {
            world.push((c.iter().map(|p| Point::new(p[0], p[1])).collect(), true));
        }
        for c in open_paths(s) {
            world.push((c.iter().map(|p| Point::new(p[0], p[1])).collect(), false));
        }
    }
    if world.is_empty() {
        return None;
    }
    let (rect, subpaths) = Shape::normalize_subpaths(&world);
    let mut out = Shape::new(
        0,
        format!("{} (combinado)", template.name),
        rect,
        ShapeKind::Path {
            subpaths,
            fill_rule: FillRule::EvenOdd,
        },
    );
    out.fill = template.fill;
    out.stroke = template.stroke;
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn miter_limit_bevels_sharp_corners() {
        // Triángulo muy agudo: con límite bajo se bisela (caja más pequeña) y con límite alto
        // la punta llega más lejos.
        let tri = shape_from_polygons(
            &sq(0.0, 0.0, 1.0, 1.0),
            &vec![vec![vec![[0.0, 0.0], [40.0, 3.0], [0.0, 6.0]]]],
            "t",
        )
        .unwrap();
        let w = |limit: f64| {
            let o = offset_with_limit(&tri, 2.0, Join::Miter, limit).unwrap();
            crate::edit::shape_bounds(&o).x1
        };
        assert!(w(20.0) > w(1.5) + 5.0, "{} vs {}", w(20.0), w(1.5));
    }

    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Shape {
        Shape::new(
            1,
            "c",
            kurbo::Rect::new(x0, y0, x1, y1),
            ShapeKind::Rect { corner_radius: 0.0 },
        )
    }

    fn area_of(s: &Shape) -> f64 {
        polygons_of(s)
            .iter()
            .flatten()
            .map(|c| {
                let n = c.len();
                (0..n)
                    .map(|i| c[i][0] * c[(i + 1) % n][1] - c[(i + 1) % n][0] * c[i][1])
                    .sum::<f64>()
                    / 2.0
            })
            .sum::<f64>()
            .abs()
    }

    #[test]
    fn subtract_front_and_crop_act_on_each_lower_shape() {
        // Dos cuadrados abajo, separados, y uno arriba que pisa la mitad de cada uno.
        let shapes = vec![
            sq(0.0, 0.0, 10.0, 10.0),
            sq(20.0, 0.0, 30.0, 10.0),
            sq(5.0, 0.0, 25.0, 10.0),
        ];
        let sub = boolean(&shapes, BoolOp::SubtractFront).unwrap();
        assert_eq!(sub.len(), 2);
        assert!(sub.iter().all(|s| (area_of(s) - 50.0).abs() < 1e-6));
        let crop = boolean(&shapes, BoolOp::Crop).unwrap();
        assert_eq!(crop.len(), 2);
        assert!(crop.iter().all(|s| (area_of(s) - 50.0).abs() < 1e-6));
        // Sustraer todo: la inferior menos todas las demás → una sola forma.
        let all = boolean(&shapes, BoolOp::Subtract).unwrap();
        assert_eq!(all.len(), 1);
        assert!((area_of(&all[0]) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn break_apart_keeps_holes_with_their_island() {
        let donut = combine(&[
            sq(0.0, 0.0, 10.0, 10.0),
            sq(3.0, 3.0, 7.0, 7.0),
            sq(20.0, 0.0, 25.0, 5.0),
        ])
        .unwrap();
        let parts = break_apart(&donut);
        assert_eq!(parts.len(), 2);
        let mut areas: Vec<f64> = parts.iter().map(area_of).collect();
        areas.sort_by(f64::total_cmp);
        assert!(
            (areas[0] - 25.0).abs() < 1e-6 && (areas[1] - 84.0).abs() < 1e-6,
            "{areas:?}"
        );
    }

    use super::*;
    use kurbo::Rect;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Shape {
        Shape::new(
            0,
            "r",
            Rect::new(x0, y0, x1, y1),
            ShapeKind::Rect { corner_radius: 0.0 },
        )
    }

    fn area(s: &Shape) -> f64 {
        // Área con signo por subtrazo (shoelace); los agujeros restan por orientación opuesta.
        s.world_subpaths()
            .iter()
            .map(|(pts, _)| {
                let n = pts.len();
                (0..n)
                    .map(|i| {
                        let (a, b) = (pts[i], pts[(i + 1) % n]);
                        a.x * b.y - b.x * a.y
                    })
                    .sum::<f64>()
                    / 2.0
            })
            .sum::<f64>()
            .abs()
    }

    #[test]
    fn weld_two_overlapping_squares() {
        let out = boolean(
            &[rect(0.0, 0.0, 10.0, 10.0), rect(5.0, 0.0, 15.0, 10.0)],
            BoolOp::Weld,
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        assert!((out[0].rect.width() - 15.0).abs() < 1e-6);
        assert!((area(&out[0]) - 150.0).abs() < 1e-6);
    }

    #[test]
    fn subtract_makes_a_hole() {
        let out = boolean(
            &[rect(0.0, 0.0, 10.0, 10.0), rect(3.0, 3.0, 7.0, 7.0)],
            BoolOp::Subtract,
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        match &out[0].kind {
            ShapeKind::Path { subpaths, .. } => assert_eq!(subpaths.len(), 2),
            _ => panic!(),
        }
        assert!((area(&out[0]) - 84.0).abs() < 1e-6);
    }

    #[test]
    fn intersect_and_divide() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 0.0, 15.0, 10.0);
        let inter = boolean(&[a.clone(), b.clone()], BoolOp::Intersect).unwrap();
        assert!((area(&inter[0]) - 50.0).abs() < 1e-6);
        let pieces = boolean(&[a, b], BoolOp::Divide).unwrap();
        assert_eq!(pieces.len(), 3);
        let total: f64 = pieces.iter().map(area).sum();
        assert!((total - 150.0).abs() < 1e-6);
    }

    #[test]
    fn offset_grows_and_shrinks() {
        let s = rect(0.0, 0.0, 10.0, 10.0);
        let out = offset(&s, 1.0, Join::Miter).unwrap();
        assert!(
            (out.rect.width() - 12.0).abs() < 0.05,
            "{}",
            out.rect.width()
        );
        let inner = offset(&s, -1.0, Join::Miter).unwrap();
        assert!(
            (inner.rect.width() - 8.0).abs() < 0.05,
            "{}",
            inner.rect.width()
        );
    }

    #[test]
    fn offset_of_open_line_is_a_capsule() {
        let mut s = Shape::new(
            0,
            "l",
            Rect::ZERO,
            ShapeKind::Path {
                subpaths: vec![],
                fill_rule: FillRule::NonZero,
            },
        );
        s.set_world_subpaths(&[(vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)], false)]);
        let out = offset(&s, 1.0, Join::Round).unwrap();
        assert!((out.rect.width() - 12.0).abs() < 0.1);
        assert!((out.rect.height() - 2.0).abs() < 0.1);
    }

    #[test]
    fn separate_and_combine_round_trip() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(20.0, 0.0, 30.0, 10.0);
        let c = combine(&[a, b]).unwrap();
        let parts = separate(&c);
        assert_eq!(parts.len(), 2);
        assert!((parts[1].rect.x0 - 20.0).abs() < 1e-9);
    }
}

// ---------------------------------------------------------------------------------------------
// Cuchilla, borrador, sombreado y pedrería.
// ---------------------------------------------------------------------------------------------

use i_overlay::float::clip::FloatClip;
use i_overlay::float::slice::FloatSlice;
use i_overlay::string::clip::ClipRule;

/// Polilíneas abiertas en mm.
pub type Lines = Vec<Vec<[f64; 2]>>;

fn pt(p: [f64; 2]) -> Point {
    Point::new(p[0], p[1])
}

/// Trazo con subtrazos abiertos (líneas) copiando el estilo de `template`, sin relleno.
pub fn shape_from_lines(template: &Shape, lines: &Lines, name: &str) -> Option<Shape> {
    let world: Vec<(Vec<Point>, bool)> = lines
        .iter()
        .filter(|l| l.len() >= 2)
        .map(|l| (l.iter().map(|p| pt(*p)).collect(), false))
        .collect();
    if world.is_empty() {
        return None;
    }
    let (rect, subpaths) = Shape::normalize_subpaths(&world);
    let mut s = Shape::new(
        0,
        name,
        rect,
        ShapeKind::Path {
            subpaths,
            fill_rule: FillRule::NonZero,
        },
    );
    s.fill = None;
    s.stroke = template.stroke;
    Some(s)
}

/// Corta una forma con una polilínea (cuchilla). Devuelve las piezas (o la forma intacta si no la cruza).
pub fn slice(s: &Shape, line: &[[f64; 2]]) -> Vec<Shape> {
    if line.len() < 2 {
        return vec![s.clone()];
    }
    let polys = polygons_of(s);
    let mut out = Vec::new();
    if !polys.is_empty() {
        let pieces = polys.slice_by(&line.to_vec(), IoFillRule::EvenOdd);
        if pieces.len() <= 1 {
            // Sin corte efectivo en la parte cerrada.
            if let Some(whole) = shape_from_polygons(s, &polys, &s.name) {
                out.push(whole);
            }
        } else {
            for (i, piece) in pieces.iter().enumerate() {
                if let Some(p) =
                    shape_from_polygons(s, &vec![piece.clone()], &format!("{} ({})", s.name, i + 1))
                {
                    out.push(p);
                }
            }
        }
    }
    // Trazos abiertos: dividir en los cruces con el segmento de la cuchilla.
    let open = open_paths(s);
    if !open.is_empty() {
        let mut lines: Lines = Vec::new();
        for path in open {
            let mut current: Vec<[f64; 2]> = vec![path[0]];
            for w in path.windows(2) {
                let (a, b) = (w[0], w[1]);
                let mut cuts: Vec<f64> = Vec::new();
                for lw in line.windows(2) {
                    if let Some(t) = segment_intersection_t(a, b, lw[0], lw[1]) {
                        cuts.push(t);
                    }
                }
                cuts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
                for t in cuts {
                    let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                    current.push(p);
                    lines.push(std::mem::replace(&mut current, vec![p]));
                }
                current.push(b);
            }
            lines.push(current);
        }
        if let Some(sh) = shape_from_lines(s, &lines, &s.name) {
            // Cada polilínea como forma propia para poder manipularlas por separado.
            for (i, (pts, _)) in sh.world_subpaths().into_iter().enumerate() {
                let (rect, subpaths) = Shape::normalize_subpaths(&[(pts, false)]);
                let mut piece = Shape::new(
                    0,
                    format!("{} ({})", s.name, i + 1),
                    rect,
                    ShapeKind::Path {
                        subpaths,
                        fill_rule: FillRule::NonZero,
                    },
                );
                piece.stroke = s.stroke;
                out.push(piece);
            }
        }
    }
    if out.is_empty() { vec![s.clone()] } else { out }
}

/// Parámetro t (0..1) sobre `ab` del cruce con `cd`, si existe y es interior.
fn segment_intersection_t(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> Option<f64> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [d[0] - c[0], d[1] - c[1]];
    let denom = r[0] * s[1] - r[1] * s[0];
    if denom.abs() < 1e-12 {
        return None;
    }
    let ac = [c[0] - a[0], c[1] - a[1]];
    let t = (ac[0] * s[1] - ac[1] * s[0]) / denom;
    let u = (ac[0] * r[1] - ac[1] * r[0]) / denom;
    (t > 1e-9 && t < 1.0 - 1e-9 && (0.0..=1.0).contains(&u)).then_some(t)
}

/// Polígono de una "pincelada" (recorrido con grosor `width`), para el borrador.
pub fn brush_polygons(path: &[[f64; 2]], width: f64) -> Polygons {
    if path.is_empty() || width <= 0.0 {
        return Vec::new();
    }
    let path: Vec<[f64; 2]> = if path.len() == 1 {
        vec![path[0], [path[0][0] + 1e-3, path[0][1]]]
    } else {
        path.to_vec()
    };
    let style = StrokeStyle {
        width,
        start_cap: LineCap::Round(0.15),
        end_cap: LineCap::Round(0.15),
        join: LineJoin::Round(0.15),
        ..StrokeStyle::new(width)
    };
    path.stroke(style, false)
}

/// Borra de una forma todo lo cubierto por `brush`. Devuelve lo que queda (puede ser vacío).
pub fn erase(s: &Shape, brush: &Polygons) -> Vec<Shape> {
    if brush.is_empty() {
        return vec![s.clone()];
    }
    let mut out = Vec::new();
    let polys = polygons_of(s);
    if !polys.is_empty() {
        let rest = polys.overlay(brush, OverlayRule::Difference, IoFillRule::EvenOdd);
        if let Some(sh) = shape_from_polygons(s, &rest, &s.name) {
            out.push(sh);
        }
    }
    let open = open_paths(s);
    if !open.is_empty() {
        let rule = ClipRule {
            invert: true,
            boundary_included: false,
        };
        let kept: Lines = open.clip_by(brush, IoFillRule::EvenOdd, rule);
        if let Some(sh) = shape_from_lines(s, &kept, &s.name) {
            out.push(sh);
        }
    }
    out
}

/// Sombreado: líneas paralelas con `angle_deg` y `spacing_mm` recortadas al interior de la forma.
pub fn hatch(s: &Shape, angle_deg: f64, spacing_mm: f64, cross: bool) -> Option<Shape> {
    let polys = polygons_of(s);
    if polys.is_empty() || spacing_mm <= 0.0 {
        return None;
    }
    let b = crate::edit::shape_bounds(s);
    let diag = b.width().hypot(b.height()) + spacing_mm;
    let c = b.center();
    let mut lines: Lines = Vec::new();
    let mut angles = vec![angle_deg];
    if cross {
        angles.push(angle_deg + 90.0);
    }
    for a in angles {
        let (sin, cos) = a.to_radians().sin_cos();
        let dir = [cos, sin];
        let normal = [-sin, cos];
        let n = (diag / spacing_mm).ceil() as i64;
        for i in -n..=n {
            let off = i as f64 * spacing_mm;
            let o = [c.x + normal[0] * off, c.y + normal[1] * off];
            let p0 = [o[0] - dir[0] * diag, o[1] - dir[1] * diag];
            let p1 = [o[0] + dir[0] * diag, o[1] + dir[1] * diag];
            lines.push(vec![p0, p1]);
        }
    }
    let rule = ClipRule {
        invert: false,
        boundary_included: false,
    };
    let inside: Lines = lines.clip_by(&polys, IoFillRule::EvenOdd, rule);
    shape_from_lines(s, &inside, &format!("{} (sombreado)", s.name))
}

pub fn point_in_polygons(p: [f64; 2], polys: &Polygons) -> bool {
    // Par-impar sobre todos los contornos.
    let mut inside = false;
    for shape in polys {
        for contour in shape {
            if crate::edit::point_in_polygon(
                Point::new(p[0], p[1]),
                &contour
                    .iter()
                    .map(|q| Point::new(q[0], q[1]))
                    .collect::<Vec<_>>(),
            ) {
                inside = !inside;
            }
        }
    }
    inside
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoneLayout {
    /// Piedras a lo largo del contorno.
    Outline,
    /// Piedras en rejilla dentro de la forma.
    Fill,
    /// Contorno y relleno.
    Both,
}

fn circle(center: [f64; 2], diameter: f64) -> Vec<Point> {
    let r = diameter / 2.0;
    (0..24)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 24.0;
            Point::new(center[0] + r * a.cos(), center[1] + r * a.sin())
        })
        .collect()
}

/// Pedrería: círculos de `diameter_mm` separados `gap_mm` sobre el contorno y/o el interior.
pub fn rhinestones(s: &Shape, diameter_mm: f64, gap_mm: f64, layout: StoneLayout) -> Option<Shape> {
    let polys = polygons_of(s);
    if polys.is_empty() || diameter_mm <= 0.0 {
        return None;
    }
    let step = diameter_mm + gap_mm;
    let mut centers: Vec<[f64; 2]> = Vec::new();
    if matches!(layout, StoneLayout::Outline | StoneLayout::Both) {
        for shape in &polys {
            for contour in shape {
                let n = contour.len();
                let mut length = 0.0;
                for i in 0..n {
                    let (a, b) = (contour[i], contour[(i + 1) % n]);
                    length += ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                }
                let count = (length / step).floor().max(1.0);
                let spacing = length / count;
                let mut next = 0.0;
                let mut walked = 0.0;
                for i in 0..n {
                    let (a, b) = (contour[i], contour[(i + 1) % n]);
                    let seg = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                    while next <= walked + seg && centers.len() < 20000 {
                        let t = if seg > 0.0 {
                            (next - walked) / seg
                        } else {
                            0.0
                        };
                        centers.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
                        next += spacing;
                    }
                    walked += seg;
                }
            }
        }
    }
    if matches!(layout, StoneLayout::Fill | StoneLayout::Both) {
        // Interior reducido para que las piedras no pisen el contorno.
        let inset = if layout == StoneLayout::Both {
            diameter_mm * 1.5
        } else {
            diameter_mm * 0.6
        };
        let style = OutlineStyle {
            outer_offset: -inset,
            inner_offset: -inset,
            join: LineJoin::Round(0.2),
            ..OutlineStyle::new(-inset)
        };
        let inner = polys.outline(&style);
        let b = crate::edit::shape_bounds(s);
        let rows = (b.height() / (step * 0.866)).ceil() as i64;
        let cols = (b.width() / step).ceil() as i64 + 1;
        for r in 0..=rows {
            let y = b.y0 + r as f64 * step * 0.866;
            let shift = if r % 2 == 1 { step / 2.0 } else { 0.0 };
            for c in 0..=cols {
                let x = b.x0 + c as f64 * step + shift;
                if point_in_polygons([x, y], &inner) && centers.len() < 20000 {
                    centers.push([x, y]);
                }
            }
        }
    }
    if centers.is_empty() {
        return None;
    }
    let world: Vec<(Vec<Point>, bool)> = centers
        .iter()
        .map(|c| (circle(*c, diameter_mm), true))
        .collect();
    let (rect, subpaths) = Shape::normalize_subpaths(&world);
    let mut out = Shape::new(
        0,
        format!("{} (pedrería)", s.name),
        rect,
        ShapeKind::Path {
            subpaths,
            fill_rule: FillRule::NonZero,
        },
    );
    out.fill = None;
    out.stroke = s.stroke;
    Some(out)
}

#[cfg(test)]
mod tool_tests {
    use super::*;
    use kurbo::Rect;

    fn square() -> Shape {
        Shape::new(
            0,
            "r",
            Rect::new(0.0, 0.0, 10.0, 10.0),
            ShapeKind::Rect { corner_radius: 0.0 },
        )
    }

    #[test]
    fn knife_splits_a_square_in_two() {
        let out = slice(&square(), &[[-1.0, 5.0], [11.0, 5.0]]);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|s| (s.rect.height() - 5.0).abs() < 1e-6));
    }

    #[test]
    fn knife_splits_open_lines() {
        let mut s = Shape::new(
            0,
            "l",
            Rect::ZERO,
            ShapeKind::Path {
                subpaths: vec![],
                fill_rule: FillRule::NonZero,
            },
        );
        s.set_world_subpaths(&[(vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)], false)]);
        let out = slice(&s, &[[5.0, -1.0], [5.0, 1.0]]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn eraser_removes_a_bite() {
        let brush = brush_polygons(&[[5.0, -2.0], [5.0, 12.0]], 2.0);
        let out = erase(&square(), &brush);
        // El cuadrado queda partido en dos mitades (una forma compuesta de dos contornos o dos formas).
        let contours: usize = out.iter().map(|s| s.world_subpaths().len()).sum();
        assert!(contours >= 2, "{contours}");
    }

    #[test]
    fn hatch_lines_stay_inside() {
        let h = hatch(&square(), 0.0, 2.0, false).unwrap();
        let lines = h.world_subpaths();
        assert!(lines.len() >= 4, "{}", lines.len());
        for (pts, closed) in lines {
            assert!(!closed);
            for p in pts {
                assert!(
                    p.x >= -1e-6 && p.x <= 10.0 + 1e-6 && p.y >= -1e-6 && p.y <= 10.0 + 1e-6,
                    "{p:?}"
                );
            }
        }
    }

    #[test]
    fn rhinestones_outline_and_fill() {
        let o = rhinestones(&square(), 2.0, 0.5, StoneLayout::Outline).unwrap();
        let n_outline = o.world_subpaths().len();
        assert!((14..=18).contains(&n_outline), "{n_outline}");
        let f = rhinestones(&square(), 2.0, 0.5, StoneLayout::Fill).unwrap();
        assert!(f.world_subpaths().len() >= 6);
    }
}

// ---------------------------------------------------------------------------------------------
// Contorno del trazo (para dibujar líneas gruesas con vértices, remates, posición y guiones).
// ---------------------------------------------------------------------------------------------

/// Patrón de guiones (trazo, hueco) en mm para un estilo y grosor.
pub fn dash_pattern(style: crate::shape::LineStyle, width_mm: f64) -> Option<(f64, f64)> {
    let unit = width_mm.max(0.35);
    style.pattern().map(|(d, g)| (unit * d, unit * g))
}

/// Divide una polilínea en tramos según un patrón de guiones.
pub fn dash_polyline(pts: &[[f64; 2]], closed: bool, dash: f64, gap: f64) -> Lines {
    let mut path: Vec<[f64; 2]> = pts.to_vec();
    if closed {
        if let Some(f) = pts.first() {
            path.push(*f);
        }
    }
    let mut out: Lines = Vec::new();
    let mut current: Vec<[f64; 2]> = Vec::new();
    let mut on = true;
    let mut left = dash;
    for w in path.windows(2) {
        let (mut a, b) = (w[0], w[1]);
        let mut seg = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        while seg > 1e-9 {
            let step = left.min(seg);
            let t = step / seg;
            let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            if on {
                if current.is_empty() {
                    current.push(a);
                }
                current.push(p);
            }
            seg -= step;
            left -= step;
            a = p;
            if left <= 1e-9 {
                if on && current.len() >= 2 {
                    out.push(std::mem::take(&mut current));
                }
                current.clear();
                on = !on;
                left = if on { dash } else { gap };
            }
        }
    }
    if on && current.len() >= 2 {
        out.push(current);
    }
    out
}

fn io_join(join: crate::shape::LineJoin) -> LineJoin<f64> {
    match join {
        crate::shape::LineJoin::Miter => LineJoin::Miter(0.3),
        crate::shape::LineJoin::Round => LineJoin::Round(0.15),
        crate::shape::LineJoin::Bevel => LineJoin::Bevel,
    }
}

fn io_cap(cap: crate::shape::LineCap) -> LineCap<[f64; 2]> {
    match cap {
        crate::shape::LineCap::Butt => LineCap::Butt,
        crate::shape::LineCap::Round => LineCap::Round(0.15),
        crate::shape::LineCap::Square => LineCap::Square,
    }
}

/// Polígonos (mm, mundo) que ocupa el trazo de una forma, o vacío si es una línea fina.
/// `extra` permite pasar contornos ya calculados (los textos), en coordenadas del mundo sin rotar.
pub fn stroke_polygons(s: &Shape, extra: Option<&[(Vec<Point>, bool)]>) -> Polygons {
    let width = crate::units::pt_to_mm(s.stroke.width_pt);
    if width <= 0.0 {
        return Vec::new();
    }
    let rot = rotation_about(s.center(), s.rotation);
    let w = |p: Point| {
        let q = rot * p;
        [q.x, q.y]
    };
    let mut closed: Vec<Vec<[f64; 2]>> = closed_contours(s);
    let mut open: Vec<Vec<[f64; 2]>> = open_paths(s);
    if let Some(paths) = extra {
        for (pts, c) in paths {
            let v: Vec<[f64; 2]> = pts.iter().map(|p| w(*p)).collect();
            if *c {
                closed.push(v);
            } else {
                open.push(v);
            }
        }
    }
    let inside = s.stroke.align == crate::shape::StrokeAlign::Inside;
    let line_width = if inside { width * 2.0 } else { width };
    let style = StrokeStyle {
        width: line_width,
        start_cap: io_cap(s.stroke.cap),
        end_cap: io_cap(s.stroke.cap),
        join: io_join(s.stroke.join),
        ..StrokeStyle::new(line_width)
    };
    let dash = dash_pattern(s.stroke.style, width);
    let mut result: Polygons = Vec::new();
    // Líneas abiertas (y tramos de guiones): siempre centradas.
    let mut open_lines: Lines = Vec::new();
    for p in &open {
        match dash {
            Some((d, g)) => open_lines.extend(dash_polyline(p, false, d, g)),
            None => open_lines.push(p.clone()),
        }
    }
    let mut closed_as_lines: Lines = Vec::new();
    let mut closed_solid: Vec<Vec<[f64; 2]>> = Vec::new();
    for c in &closed {
        match dash {
            Some((d, g)) => closed_as_lines.extend(dash_polyline(c, true, d, g)),
            None => closed_solid.push(c.clone()),
        }
    }
    for l in open_lines.iter().chain(closed_as_lines.iter()) {
        result.extend(l.stroke(style.clone(), false));
    }
    for c in &closed_solid {
        result.extend(c.stroke(style.clone(), true));
    }
    if inside && !closed.is_empty() {
        // Conservar solo la mitad interior del trazo.
        let interior = polygons_of(s);
        let interior = if interior.is_empty() {
            closed.overlay(
                &Vec::<Vec<[f64; 2]>>::new(),
                OverlayRule::Subject,
                IoFillRule::EvenOdd,
            )
        } else {
            interior
        };
        result = result.overlay(&interior, OverlayRule::Intersect, IoFillRule::NonZero);
    }
    result
}

#[cfg(test)]
mod stroke_tests {
    use super::*;
    use crate::shape::{LineStyle, StrokeAlign};
    use kurbo::Rect;

    fn area(polys: &Polygons) -> f64 {
        polys
            .iter()
            .flatten()
            .map(|c| {
                let n = c.len();
                (0..n)
                    .map(|i| c[i][0] * c[(i + 1) % n][1] - c[(i + 1) % n][0] * c[i][1])
                    .sum::<f64>()
                    / 2.0
            })
            .sum::<f64>()
            .abs()
    }

    fn square(width_pt: f64) -> Shape {
        let mut s = Shape::new(
            0,
            "r",
            Rect::new(0.0, 0.0, 10.0, 10.0),
            ShapeKind::Rect { corner_radius: 0.0 },
        );
        s.stroke.width_pt = width_pt;
        s
    }

    #[test]
    fn hairline_has_no_polygons() {
        assert!(stroke_polygons(&square(0.0), None).is_empty());
    }

    #[test]
    fn centered_and_inside_stroke_areas() {
        let w = crate::units::pt_to_mm(28.3465); // 10 mm
        let s = square(28.3465);
        let centered = area(&stroke_polygons(&s, None));
        // Marco de 10 mm centrado sobre un cuadrado de 10: (20² − 0²) = 400 con esquinas en ángulo.
        assert!((centered - 400.0).abs() < 1.0, "{centered}");
        let mut inside = s.clone();
        inside.stroke.align = StrokeAlign::Inside;
        let a = area(&stroke_polygons(&inside, None));
        // Hacia dentro, 10 mm ocupan todo el cuadrado.
        assert!((a - 100.0).abs() < 1.0, "{a} (w={w})");
    }

    #[test]
    fn dashes_split_the_outline() {
        let mut s = square(1.0);
        s.stroke.style = LineStyle::Dashed;
        let pieces = stroke_polygons(&s, None);
        assert!(pieces.len() > 4, "{}", pieces.len());
    }

    #[test]
    fn dash_polyline_lengths() {
        let parts = dash_polyline(&[[0.0, 0.0], [10.0, 0.0]], false, 2.0, 1.0);
        assert_eq!(parts.len(), 4);
        assert!((parts[0][1][0] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn weld_single_shape_merges_overlapping_contours() {
        // Dos cuadrados solapados en una misma forma, con la misma orientación (como dos letras
        // que se tocan): fusionar la forma sola los une en un único contorno.
        let sq = |x: f64| {
            vec![
                Point::new(x, 0.0),
                Point::new(x + 10.0, 0.0),
                Point::new(x + 10.0, 10.0),
                Point::new(x, 10.0),
            ]
        };
        let mut s = Shape::new(
            1,
            "Texto",
            Rect::new(0.0, 0.0, 1.0, 1.0),
            ShapeKind::Path {
                subpaths: Vec::new(),
                fill_rule: FillRule::NonZero,
            },
        );
        s.set_world_subpaths(&[(sq(0.0), true), (sq(6.0), true)]);
        assert_eq!(closed_contours(&s).len(), 2);
        let out = boolean(std::slice::from_ref(&s), BoolOp::Weld).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(
            closed_contours(&out[0]).len(),
            1,
            "un solo contorno tras fusionar"
        );
        let b = oc_core_bounds(&out[0]);
        assert!((b.x1 - 16.0).abs() < 1e-6 && b.x0.abs() < 1e-6);
        // Otras operaciones siguen necesitando dos formas.
        assert!(boolean(std::slice::from_ref(&s), BoolOp::Intersect).is_none());
    }

    fn oc_core_bounds(s: &Shape) -> Rect {
        let pts: Vec<Point> = s
            .world_subpaths()
            .into_iter()
            .flat_map(|(p, _)| p)
            .collect();
        pts.iter()
            .skip(1)
            .fold(Rect::from_points(pts[0], pts[0]), |r, p| r.union_pt(*p))
    }
}
