//! Geometría de edición: cajas rotadas, asas de escala, rotación y pruebas de impacto.

use crate::shape::{Shape, ShapeKind};
use crate::units::pt_to_mm;
use kurbo::{Affine, Point, Rect, Vec2};

/// Asas de escala del marco de selección.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handle {
    NW,
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
}

impl Handle {
    pub const ALL: [Handle; 8] = [
        Handle::NW,
        Handle::N,
        Handle::NE,
        Handle::E,
        Handle::SE,
        Handle::S,
        Handle::SW,
        Handle::W,
    ];

    pub fn is_corner(self) -> bool {
        matches!(self, Handle::NW | Handle::NE | Handle::SE | Handle::SW)
    }
    pub fn west(self) -> bool {
        matches!(self, Handle::NW | Handle::W | Handle::SW)
    }
    pub fn east(self) -> bool {
        matches!(self, Handle::NE | Handle::E | Handle::SE)
    }
    pub fn north(self) -> bool {
        matches!(self, Handle::NW | Handle::N | Handle::NE)
    }
    pub fn south(self) -> bool {
        matches!(self, Handle::SW | Handle::S | Handle::SE)
    }

    pub fn position(self, r: Rect) -> Point {
        let c = r.center();
        match self {
            Handle::NW => Point::new(r.x0, r.y0),
            Handle::N => Point::new(c.x, r.y0),
            Handle::NE => Point::new(r.x1, r.y0),
            Handle::E => Point::new(r.x1, c.y),
            Handle::SE => Point::new(r.x1, r.y1),
            Handle::S => Point::new(c.x, r.y1),
            Handle::SW => Point::new(r.x0, r.y1),
            Handle::W => Point::new(r.x0, c.y),
        }
    }
}

const MIN_SIZE_MM: f64 = 0.5;

pub fn rotation_about(center: Point, deg: f64) -> Affine {
    Affine::translate(center.to_vec2())
        * Affine::rotate(deg.to_radians())
        * Affine::translate(-center.to_vec2())
}

/// Esquinas de una caja rotada sobre su centro.
pub fn rotated_corners(r: Rect, deg: f64) -> [Point; 4] {
    let a = rotation_about(r.center(), deg);
    [
        a * Point::new(r.x0, r.y0),
        a * Point::new(r.x1, r.y0),
        a * Point::new(r.x1, r.y1),
        a * Point::new(r.x0, r.y1),
    ]
}

/// Caja alineada a los ejes de una caja rotada.
pub fn rotated_bounds(r: Rect, deg: f64) -> Rect {
    if deg == 0.0 {
        return r;
    }
    bounds_of(rotated_corners(r, deg).iter().copied()).unwrap_or(r)
}

pub fn bounds_of(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
    let mut out: Option<Rect> = None;
    for p in points {
        let r = Rect::from_points(p, p);
        out = Some(out.map_or(r, |b| b.union(r)));
    }
    out
}

pub fn shape_bounds(s: &Shape) -> Rect {
    rotated_bounds(s.rect, s.rotation)
}

pub fn selection_bounds<'a>(shapes: impl IntoIterator<Item = &'a Shape>) -> Option<Rect> {
    let mut out: Option<Rect> = None;
    for s in shapes {
        let b = shape_bounds(s);
        out = Some(out.map_or(b, |o| o.union(b)));
    }
    out
}

/// Redimensiona `r` arrastrando `handle` hasta `p` (coordenadas locales sin rotar).
pub fn resize_rect(r: Rect, handle: Handle, p: Point, keep_aspect: bool) -> Rect {
    let (mut left, mut right, mut top, mut bottom) = (r.x0, r.x1, r.y0, r.y1);
    if handle.west() {
        left = p.x.min(right - MIN_SIZE_MM);
    }
    if handle.east() {
        right = p.x.max(left + MIN_SIZE_MM);
    }
    if handle.north() {
        top = p.y.min(bottom - MIN_SIZE_MM);
    }
    if handle.south() {
        bottom = p.y.max(top + MIN_SIZE_MM);
    }
    let mut width = right - left;
    let mut height = bottom - top;

    if keep_aspect && r.width() > 0.0 && r.height() > 0.0 {
        let aspect = r.width() / r.height();
        if handle.is_corner() {
            // Proyecta el desplazamiento del puntero sobre la diagonal: el asa sigue al puntero.
            let sx = if handle.west() { -1.0 } else { 1.0 };
            let sy = if handle.north() { -1.0 } else { 1.0 };
            let ax = if handle.west() { r.x1 } else { r.x0 };
            let ay = if handle.north() { r.y1 } else { r.y0 };
            let dx = (p.x - ax) * sx;
            let dy = (p.y - ay) * sy;
            let k = ((dx * r.width() + dy * r.height()) / (r.width().powi(2) + r.height().powi(2)))
                .max(MIN_SIZE_MM / r.width().max(r.height()));
            width = r.width() * k;
            height = r.height() * k;
        } else if matches!(handle, Handle::N | Handle::S) {
            width = height * aspect;
        } else {
            height = width / aspect;
        }
        if handle.west() {
            left = right - width;
        }
        if handle.north() {
            top = bottom - height;
        }
        if matches!(handle, Handle::N | Handle::S) {
            left = r.center().x - width / 2.0;
        }
        if matches!(handle, Handle::E | Handle::W) {
            top = r.center().y - height / 2.0;
        }
    }
    Rect::new(left, top, left + width, top + height)
}

/// Aplica una caja nueva a una forma; los textos escalan su tamaño de fuente.
pub(crate) fn apply_rect(s: &mut Shape, nr: Rect) {
    if let ShapeKind::Text { font_size_pt, .. } = &mut s.kind {
        if s.rect.height() > 0.0 {
            let k = nr.height() / s.rect.height();
            *font_size_pt = (*font_size_pt * k).max(1.0);
        }
    }
    s.rect = nr;
}

/// Escala una forma (posiblemente rotada) arrastrando un asa hasta `pointer` (mm, mundo).
pub fn scale_shape_by_handle(
    original: &Shape,
    handle: Handle,
    pointer: Point,
    keep_aspect: bool,
) -> Shape {
    let c = original.center();
    let local = rotation_about(c, -original.rotation) * pointer;
    let nr = resize_rect(
        original.rect,
        handle,
        local,
        keep_aspect || original.is_text(),
    );
    let wc = rotation_about(c, original.rotation) * nr.center();
    let mut s = original.clone();
    apply_rect(&mut s, Rect::from_center_size(wc, nr.size()));
    s
}

/// Escala un grupo proporcionalmente a su caja común.
pub fn scale_group_by_handle(
    originals: &[Shape],
    group: Rect,
    handle: Handle,
    pointer: Point,
    keep_aspect: bool,
) -> Vec<Shape> {
    let ng = resize_rect(group, handle, pointer, keep_aspect);
    let kx = if group.width() > 0.0 {
        ng.width() / group.width()
    } else {
        1.0
    };
    let ky = if group.height() > 0.0 {
        ng.height() / group.height()
    } else {
        1.0
    };
    originals
        .iter()
        .map(|o| {
            let mut s = o.clone();
            let x = ng.x0 + (o.rect.x0 - group.x0) * kx;
            let y = ng.y0 + (o.rect.y0 - group.y0) * ky;
            apply_rect(
                &mut s,
                Rect::new(x, y, x + o.rect.width() * kx, y + o.rect.height() * ky),
            );
            s
        })
        .collect()
}

pub fn rotate_shapes(originals: &[Shape], center: Point, delta_deg: f64) -> Vec<Shape> {
    let a = rotation_about(center, delta_deg);
    originals
        .iter()
        .map(|o| {
            let mut s = o.clone();
            s.rotation = (o.rotation + delta_deg).rem_euclid(360.0);
            s.rect = Rect::from_center_size(a * o.center(), o.rect.size());
            s
        })
        .collect()
}

pub fn translate_shapes(originals: &[Shape], d: Vec2) -> Vec<Shape> {
    originals
        .iter()
        .map(|o| {
            let mut s = o.clone();
            s.rect = o.rect + d;
            s
        })
        .collect()
}

pub fn angle_deg(center: Point, p: Point) -> f64 {
    (p.y - center.y).atan2(p.x - center.x).to_degrees()
}

/// Distancia de `p` al segmento `ab`.
fn dist_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.hypot2();
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

pub fn point_in_polygon(p: Point, poly: &[Point]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// ¿El punto (mm, mundo) toca la forma? `tolerance_mm` es el margen para líneas.
pub fn hit_test(s: &Shape, p: Point, tolerance_mm: f64) -> bool {
    if !s.visible {
        return false;
    }
    let local = rotation_about(s.center(), -s.rotation) * p;
    let r = s.rect;
    let half_stroke = pt_to_mm(s.stroke.width_pt) / 2.0 + tolerance_mm;
    match &s.kind {
        ShapeKind::Rect { .. } | ShapeKind::Text { .. } | ShapeKind::Image { .. } => {
            r.inflate(half_stroke, half_stroke).contains(local)
        }
        ShapeKind::Ellipse => {
            let c = r.center();
            let rx = r.width() / 2.0 + half_stroke;
            let ry = r.height() / 2.0 + half_stroke;
            if rx <= 0.0 || ry <= 0.0 {
                return false;
            }
            ((local.x - c.x) / rx).powi(2) + ((local.y - c.y) / ry).powi(2) <= 1.0
        }
        ShapeKind::Path { .. } => {
            let subpaths = s.world_subpaths();
            for (pts, closed) in &subpaths {
                if *closed && s.fill.is_some() && point_in_polygon(local, pts) {
                    return true;
                }
                let n = pts.len();
                for i in 0..n {
                    let j = if i + 1 < n {
                        i + 1
                    } else if *closed {
                        0
                    } else {
                        continue;
                    };
                    if dist_to_segment(local, pts[i], pts[j]) <= half_stroke {
                        return true;
                    }
                }
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::{Shape, ShapeKind};

    fn rect_shape() -> Shape {
        Shape::new(
            1,
            "r",
            Rect::new(10.0, 10.0, 30.0, 20.0),
            ShapeKind::Rect { corner_radius: 0.0 },
        )
    }

    #[test]
    fn resize_se_corner_free() {
        let r = resize_rect(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Handle::SE,
            Point::new(20.0, 15.0),
            false,
        );
        assert_eq!(r, Rect::new(0.0, 0.0, 20.0, 15.0));
    }

    #[test]
    fn resize_keeps_aspect_on_corner() {
        let r = resize_rect(
            Rect::new(0.0, 0.0, 20.0, 10.0),
            Handle::SE,
            Point::new(40.0, 12.0),
            true,
        );
        assert!((r.width() / r.height() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn resize_never_negative() {
        let r = resize_rect(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Handle::E,
            Point::new(-50.0, 0.0),
            false,
        );
        assert!(r.width() > 0.0 && r.x0 == 0.0);
    }

    #[test]
    fn scale_rotated_shape_keeps_center_axis() {
        let mut s = rect_shape();
        s.rotation = 90.0;
        let out = scale_shape_by_handle(&s, Handle::E, Point::new(20.0, 35.0), false);
        assert_eq!(out.rotation, 90.0);
        assert!((out.rect.height() - 10.0).abs() < 1e-9);
        assert!((out.rect.width() - 30.0).abs() < 1e-6);
        let c = out.center();
        assert!((c.x - 20.0).abs() < 1e-6 && (c.y - 20.0).abs() < 1e-6);
    }

    #[test]
    fn rotate_group_moves_centers() {
        let s = Shape::new(1, "a", Rect::new(0.0, 0.0, 10.0, 10.0), ShapeKind::Ellipse);
        let out = rotate_shapes(&[s], Point::new(20.0, 5.0), 180.0);
        assert_eq!(out[0].rotation, 180.0);
        assert!((out[0].rect.x0 - 30.0).abs() < 1e-6 && out[0].rect.y0.abs() < 1e-6);
    }

    #[test]
    fn hit_tests_lines_and_fills() {
        let mut s = Shape::new(
            1,
            "p",
            Rect::ZERO,
            ShapeKind::Path {
                subpaths: vec![],
                fill_rule: Default::default(),
            },
        );
        s.set_world_subpaths(&[(
            vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
            ],
            true,
        )]);
        assert!(hit_test(&s, Point::new(5.0, 0.2), 0.5));
        assert!(!hit_test(&s, Point::new(2.0, 8.0), 0.5)); // dentro de la caja pero fuera del triángulo
        s.fill = Some(crate::Color::BLACK);
        assert!(hit_test(&s, Point::new(8.0, 5.0), 0.5)); // relleno: dentro del triángulo
    }
}

/// Refleja una forma respecto a un eje vertical (`x = axis`) o horizontal (`y = axis`).
/// Rectángulos, elipses e imágenes solo cambian de posición; los trazos invierten sus puntos.
pub fn mirror_shape(s: &Shape, horizontal: bool, axis: f64) -> Shape {
    let mut out = s.clone();
    let r = s.rect;
    out.rect = if horizontal {
        Rect::new(2.0 * axis - r.x1, r.y0, 2.0 * axis - r.x0, r.y1)
    } else {
        Rect::new(r.x0, 2.0 * axis - r.y1, r.x1, 2.0 * axis - r.y0)
    };
    out.rotation = (-s.rotation).rem_euclid(360.0);
    if let ShapeKind::Path { subpaths, .. } = &mut out.kind {
        for sp in subpaths.iter_mut() {
            let flip = |p: &mut Point| {
                if horizontal {
                    p.x = 1.0 - p.x;
                } else {
                    p.y = 1.0 - p.y;
                }
            };
            sp.points.iter_mut().for_each(flip);
            if let Some(hs) = sp.handles.as_mut() {
                for h in hs.iter_mut() {
                    if let Some(p) = h.h_in.as_mut() {
                        flip(p);
                    }
                    if let Some(p) = h.h_out.as_mut() {
                        flip(p);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod mirror_tests {
    use super::*;
    use crate::shape::{Shape, ShapeKind};

    #[test]
    fn mirror_path_reflects_points_and_box() {
        let mut s = Shape::new(
            1,
            "p",
            Rect::ZERO,
            ShapeKind::Path {
                subpaths: vec![],
                fill_rule: Default::default(),
            },
        );
        s.set_world_subpaths(&[(
            vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 5.0),
            ],
            true,
        )]);
        let m = mirror_shape(&s, true, 10.0);
        assert_eq!(m.rect, Rect::new(10.0, 0.0, 20.0, 5.0));
        let pts = m.world_subpaths()[0].0.clone();
        assert!((pts[0].x - 20.0).abs() < 1e-9 && (pts[1].x - 10.0).abs() < 1e-9);
    }
}
