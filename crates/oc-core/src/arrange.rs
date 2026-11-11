//! Organización de objetos (panel Transformar): alinear, centrar, distribuir, escalar,
//! rotar y mover, con el mismo comportamiento que la referencia:
//!
//! - Con varios objetos, alinear usa la caja de la selección; con uno solo, el área de
//!   corte de la página.
//! - «Centrar» junta los centros de todos los objetos; «Centrar en la página» mueve el
//!   grupo entero.
//! - Distribuir deja huecos iguales entre objetos (ordenados por su borde inicial).
//! - Escalar y «Rotar» (relativo) actúan sobre el grupo desde su centro; «Rotar a» lleva
//!   cada objeto a un ángulo absoluto sobre su propio centro.

use crate::edit::{apply_rect, rotate_shapes, selection_bounds, shape_bounds};
use crate::shape::Shape;
use kurbo::{Point, Rect, Vec2};

/// Alineación respecto a una caja de referencia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    HCenter,
    Right,
    Top,
    VCenter,
    Bottom,
    /// Centro en ambos ejes.
    Center,
}

/// Eje de distribución.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// Punto de referencia de una caja (rejilla 3×3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    pub const ALL: [Anchor; 9] = [
        Anchor::TopLeft,
        Anchor::Top,
        Anchor::TopRight,
        Anchor::Left,
        Anchor::Center,
        Anchor::Right,
        Anchor::BottomLeft,
        Anchor::Bottom,
        Anchor::BottomRight,
    ];

    /// Fracción (0, 0.5 o 1) en x e y dentro de la caja.
    pub fn fraction(self) -> (f64, f64) {
        let i = Anchor::ALL.iter().position(|a| *a == self).unwrap_or(0);
        ((i % 3) as f64 / 2.0, (i / 3) as f64 / 2.0)
    }

    pub fn point(self, r: Rect) -> Point {
        let (fx, fy) = self.fraction();
        Point::new(r.x0 + r.width() * fx, r.y0 + r.height() * fy)
    }
}

fn moved(s: &Shape, d: Vec2) -> Shape {
    let mut n = s.clone();
    n.rect = s.rect + d;
    n
}

/// Alinea cada objeto con `reference` (su caja girada es la que se alinea).
pub fn align(shapes: &[Shape], mode: Align, reference: Rect) -> Vec<Shape> {
    let rc = reference.center();
    shapes
        .iter()
        .map(|s| {
            let b = shape_bounds(s);
            let bc = b.center();
            let d = match mode {
                Align::Left => Vec2::new(reference.x0 - b.x0, 0.0),
                Align::HCenter => Vec2::new(rc.x - bc.x, 0.0),
                Align::Right => Vec2::new(reference.x1 - b.x1, 0.0),
                Align::Top => Vec2::new(0.0, reference.y0 - b.y0),
                Align::VCenter => Vec2::new(0.0, rc.y - bc.y),
                Align::Bottom => Vec2::new(0.0, reference.y1 - b.y1),
                Align::Center => rc - bc,
            };
            moved(s, d)
        })
        .collect()
}

/// Mueve el grupo entero para que el centro de su caja quede en `target`.
pub fn center_group_on(shapes: &[Shape], target: Point) -> Vec<Shape> {
    let Some(b) = selection_bounds(shapes) else {
        return shapes.to_vec();
    };
    let d = target - b.center();
    shapes.iter().map(|s| moved(s, d)).collect()
}

/// Reparte los objetos con huecos iguales entre el primero y el último del eje.
pub fn distribute(shapes: &[Shape], axis: Axis) -> Vec<Shape> {
    if shapes.len() < 3 {
        return shapes.to_vec();
    }
    let key = |r: Rect| match axis {
        Axis::Horizontal => (r.x0, r.x1),
        Axis::Vertical => (r.y0, r.y1),
    };
    let mut order: Vec<usize> = (0..shapes.len()).collect();
    order.sort_by(|&a, &b| {
        key(shape_bounds(&shapes[a]))
            .0
            .total_cmp(&key(shape_bounds(&shapes[b])).0)
    });
    let bounds: Vec<Rect> = shapes.iter().map(shape_bounds).collect();
    let start = key(bounds[order[0]]).0;
    let end = order
        .iter()
        .map(|&i| key(bounds[i]).1)
        .fold(f64::MIN, f64::max);
    let total: f64 = order
        .iter()
        .map(|&i| {
            let (a, b) = key(bounds[i]);
            b - a
        })
        .sum();
    let gap = (end - start - total) / (shapes.len() - 1) as f64;
    let mut out = shapes.to_vec();
    let mut at = start;
    for &i in &order {
        let (a, b) = key(bounds[i]);
        let d = at - a;
        out[i] = moved(
            &shapes[i],
            match axis {
                Axis::Horizontal => Vec2::new(d, 0.0),
                Axis::Vertical => Vec2::new(0.0, d),
            },
        );
        at += (b - a) + gap;
    }
    out
}

/// Escala el grupo con factores `kx`, `ky` desde `center` (los textos escalan su fuente).
pub fn scale_about(shapes: &[Shape], kx: f64, ky: f64, center: Point) -> Vec<Shape> {
    shapes
        .iter()
        .map(|s| {
            let c = s.center();
            let nc = Point::new(
                center.x + (c.x - center.x) * kx,
                center.y + (c.y - center.y) * ky,
            );
            let size = kurbo::Size::new(s.rect.width() * kx, s.rect.height() * ky);
            let mut n = s.clone();
            apply_rect(&mut n, Rect::from_center_size(nc, size));
            n
        })
        .collect()
}

/// Escala el grupo para que su caja mida `width` × `height`, desde su centro.
pub fn resize_group(shapes: &[Shape], width: f64, height: f64) -> Vec<Shape> {
    let Some(b) = selection_bounds(shapes) else {
        return shapes.to_vec();
    };
    let kx = if b.width() > 0.0 {
        width / b.width()
    } else {
        1.0
    };
    let ky = if b.height() > 0.0 {
        height / b.height()
    } else {
        1.0
    };
    scale_about(shapes, kx, ky, b.center())
}

/// Gira el grupo `delta` grados sobre el centro de su caja.
pub fn rotate_group(shapes: &[Shape], delta: f64) -> Vec<Shape> {
    let Some(b) = selection_bounds(shapes) else {
        return shapes.to_vec();
    };
    rotate_shapes(shapes, b.center(), delta)
}

/// Lleva cada objeto al ángulo absoluto `angle` sobre su propio centro.
pub fn rotate_each_to(shapes: &[Shape], angle: f64) -> Vec<Shape> {
    shapes
        .iter()
        .map(|s| {
            let mut n = s.clone();
            n.rotation = angle.rem_euclid(360.0);
            n
        })
        .collect()
}

/// Desplaza todos los objetos.
pub fn translate(shapes: &[Shape], d: Vec2) -> Vec<Shape> {
    shapes.iter().map(|s| moved(s, d)).collect()
}

/// Mueve el grupo para que su punto `anchor` quede en `target`.
pub fn move_anchor_to(shapes: &[Shape], anchor: Anchor, target: Point) -> Vec<Shape> {
    let Some(b) = selection_bounds(shapes) else {
        return shapes.to_vec();
    };
    translate(shapes, target - anchor.point(b))
}

/// Acomoda cajas de tamaño `(ancho, alto)` en `area` por filas, de mayor a menor alto, con
/// `gap` entre ellas. Devuelve la esquina de cada una (en el orden de entrada) o `None` si
/// no cabe.
pub fn pack(sizes: &[(f64, f64)], area: Rect, gap: f64) -> Vec<Option<Point>> {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| {
        sizes[b]
            .1
            .total_cmp(&sizes[a].1)
            .then(sizes[b].0.total_cmp(&sizes[a].0))
    });
    let mut out = vec![None; sizes.len()];
    // Filas abiertas: (y, alto, x libre).
    let mut rows: Vec<(f64, f64, f64)> = Vec::new();
    let mut next_y = area.y0;
    for i in order {
        let (w, h) = sizes[i];
        if w > area.width() + 1e-9 {
            continue;
        }
        // La primera fila donde quepa a lo ancho (y de alto, por ser más baja que la fila).
        if let Some(r) = rows
            .iter_mut()
            .find(|r| h <= r.1 + 1e-9 && r.2 + w <= area.x1 + 1e-9)
        {
            out[i] = Some(Point::new(r.2, r.0));
            r.2 += w + gap;
            continue;
        }
        if next_y + h > area.y1 + 1e-9 {
            continue;
        }
        out[i] = Some(Point::new(area.x0, next_y));
        rows.push((next_y, h, area.x0 + w + gap));
        next_y += h + gap;
    }
    out
}

/// Acomoda las formas en `area` tratando cada grupo como una pieza. Devuelve las formas
/// movidas y cuántas piezas no cupieron (esas no se mueven).
pub fn pack_shapes(shapes: &[Shape], area: Rect, gap: f64) -> (Vec<Shape>, usize) {
    // Piezas: índices de las formas de cada grupo (o sueltas).
    let mut units: Vec<Vec<usize>> = Vec::new();
    let mut by_group: std::collections::HashMap<u64, usize> = Default::default();
    for (i, s) in shapes.iter().enumerate() {
        match s.group {
            Some(g) => match by_group.get(&g) {
                Some(&u) => units[u].push(i),
                None => {
                    by_group.insert(g, units.len());
                    units.push(vec![i]);
                }
            },
            None => units.push(vec![i]),
        }
    }
    let bounds: Vec<Rect> = units
        .iter()
        .map(|u| {
            u.iter()
                .map(|&i| shape_bounds(&shapes[i]))
                .reduce(|a, b| a.union(b))
                .unwrap_or(Rect::ZERO)
        })
        .collect();
    let sizes: Vec<(f64, f64)> = bounds.iter().map(|b| (b.width(), b.height())).collect();
    let mut out = shapes.to_vec();
    let mut missing = 0;
    for (u, at) in pack(&sizes, area, gap).into_iter().enumerate() {
        let Some(at) = at else {
            missing += 1;
            continue;
        };
        let d = at - bounds[u].origin();
        for &i in &units[u] {
            out[i].rect = out[i].rect + d;
        }
    }
    (out, missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::ShapeKind;

    fn rect(id: u64, x0: f64, y0: f64, x1: f64, y1: f64) -> Shape {
        Shape::new(
            id,
            "r",
            Rect::new(x0, y0, x1, y1),
            ShapeKind::Rect { corner_radius: 0.0 },
        )
    }

    fn three() -> Vec<Shape> {
        vec![
            rect(1, 10.0, 10.0, 30.0, 20.0),
            rect(2, 50.0, 40.0, 60.0, 70.0),
            rect(3, 15.0, 90.0, 45.0, 100.0),
        ]
    }

    #[test]
    fn align_left_uses_selection_box() {
        let out = align(&three(), Align::Left, selection_bounds(&three()).unwrap());
        assert!(out.iter().all(|s| (s.rect.x0 - 10.0).abs() < 1e-9));
        // El eje vertical no cambia.
        assert_eq!(out[1].rect.y0, 40.0);
    }

    #[test]
    fn align_center_stacks_all_centers() {
        let b = selection_bounds(&three()).unwrap();
        let out = align(&three(), Align::Center, b);
        for s in &out {
            assert!((s.center() - b.center()).hypot() < 1e-9);
        }
    }

    #[test]
    fn center_on_page_moves_group_rigidly() {
        let out = center_group_on(&three(), Point::new(100.0, 150.0));
        let b = selection_bounds(&out).unwrap();
        assert!((b.center() - Point::new(100.0, 150.0)).hypot() < 1e-9);
        // Las distancias relativas se conservan.
        assert!(((out[1].rect.x0 - out[0].rect.x0) - 40.0).abs() < 1e-9);
    }

    #[test]
    fn distribute_leaves_equal_gaps() {
        // Igual que la prueba en la referencia: huecos negativos si los objetos se solapan.
        let out = distribute(&three(), Axis::Vertical);
        let mut r: Vec<Rect> = out.iter().map(|s| s.rect).collect();
        r.sort_by(|a, b| a.y0.total_cmp(&b.y0));
        let g1 = r[1].y0 - r[0].y1;
        let g2 = r[2].y0 - r[1].y1;
        assert!((g1 - g2).abs() < 1e-9, "{g1} {g2}");
        assert_eq!(r[0].y0, 10.0);
        assert_eq!(r[2].y1, 100.0);
    }

    #[test]
    fn scale_half_about_group_center() {
        let shapes = three();
        let b = selection_bounds(&shapes).unwrap();
        let out = scale_about(&shapes, 0.5, 0.5, b.center());
        let nb = selection_bounds(&out).unwrap();
        assert!((nb.width() - b.width() / 2.0).abs() < 1e-9);
        assert!((nb.center() - b.center()).hypot() < 1e-9);
    }

    #[test]
    fn rotate_group_vs_rotate_each() {
        let shapes = three();
        let g = rotate_group(&shapes, 90.0);
        let b = selection_bounds(&shapes).unwrap();
        let nb = selection_bounds(&g).unwrap();
        // El grupo girado 90° intercambia ancho y alto de su caja.
        assert!((nb.width() - b.height()).abs() < 1e-6);
        let each = rotate_each_to(&shapes, 90.0);
        for (a, s) in shapes.iter().zip(&each) {
            assert_eq!(a.center(), s.center());
            assert_eq!(s.rotation, 90.0);
        }
    }

    #[test]
    fn move_anchor() {
        let out = move_anchor_to(&three(), Anchor::BottomRight, Point::new(200.0, 200.0));
        let b = selection_bounds(&out).unwrap();
        assert!((b.x1 - 200.0).abs() < 1e-9 && (b.y1 - 200.0).abs() < 1e-9);
    }

    #[test]
    fn pack_fills_rows_without_overlap() {
        let area = Rect::new(0.0, 0.0, 100.0, 60.0);
        let sizes = [
            (40.0, 20.0),
            (40.0, 30.0),
            (40.0, 20.0),
            (30.0, 10.0),
            (90.0, 50.0),
        ];
        let at = pack(&sizes, area, 2.0);
        // La de 90×50 no cabe con las demás… pero es la más alta: va primero y llena.
        assert_eq!(at[4], Some(Point::new(0.0, 0.0)));
        let placed: Vec<Rect> = at
            .iter()
            .zip(sizes)
            .filter_map(|(p, (w, h))| p.map(|p| Rect::from_origin_size(p, (w, h))))
            .collect();
        for (i, a) in placed.iter().enumerate() {
            assert!(area.contains(a.origin()) && a.x1 <= 100.0 && a.y1 <= 60.0);
            for b in &placed[i + 1..] {
                assert!(a.intersect(*b).area() == 0.0, "{a:?} {b:?}");
            }
        }
        // Sin la grande caben todas.
        assert!(pack(&sizes[..4], area, 2.0).iter().all(|p| p.is_some()));
    }

    #[test]
    fn pack_shapes_moves_groups_together() {
        let mut a = rect(1, 0.0, 0.0, 10.0, 10.0);
        let mut b = rect(2, 12.0, 0.0, 17.0, 5.0);
        a.group = Some(99);
        b.group = Some(99);
        let c = rect(3, 50.0, 50.0, 70.0, 70.0);
        let (out, missing) = pack_shapes(&[a, b, c], Rect::new(100.0, 100.0, 200.0, 200.0), 3.0);
        assert_eq!(missing, 0);
        // La más alta (c) arriba a la izquierda; el grupo a su derecha, sin separarse.
        assert_eq!(out[2].rect.origin(), Point::new(100.0, 100.0));
        assert_eq!(out[0].rect.origin(), Point::new(123.0, 100.0));
        assert_eq!(out[1].rect.x0 - out[0].rect.x0, 12.0);
    }
}
