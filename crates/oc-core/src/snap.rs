//! Ajuste de un punto que se arrastra: a otros puntos (por eje, con sus guías), a la retícula y
//! a ángulos fijos.

use kurbo::Point;

/// Resultado de ajustar un punto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snapped {
    pub point: Point,
    /// Punto con el que se alineó en vertical (misma x), si lo hubo.
    pub guide_x: Option<Point>,
    /// Punto con el que se alineó en horizontal (misma y), si lo hubo.
    pub guide_y: Option<Point>,
}

/// Qué ejes puede mover el ajuste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Axes {
    pub x: bool,
    pub y: bool,
}

impl Axes {
    pub const BOTH: Axes = Axes { x: true, y: true };
    pub const NONE: Axes = Axes { x: false, y: false };
}

/// Ajusta `p` a los `targets`: en cada eje libre toma la coordenada del objetivo más cercano en
/// ese eje si está a `tol` o menos (a igual distancia, el más próximo en el otro eje). Donde no
/// hay objetivo y `grid` es un paso de retícula, redondea a la retícula.
pub fn snap_point(p: Point, targets: &[Point], tol: f64, grid: Option<f64>, axes: Axes) -> Snapped {
    snap_point_with_lines(p, targets, &[], &[], tol, grid, axes)
}

/// Como [`snap_point`], con líneas guía además de puntos: `xs` son guías verticales (su x) e
/// `ys` horizontales (su y). Una guía equivale a un objetivo justo a la altura del punto.
pub fn snap_point_with_lines(
    p: Point,
    targets: &[Point],
    xs: &[f64],
    ys: &[f64],
    tol: f64,
    grid: Option<f64>,
    axes: Axes,
) -> Snapped {
    let on_lines: Vec<Point> = xs
        .iter()
        .map(|x| Point::new(*x, p.y))
        .chain(ys.iter().map(|y| Point::new(p.x, *y)))
        .collect();
    let (x_lines, y_lines) = on_lines.split_at(xs.len());
    let best = |along: fn(Point) -> f64, across: fn(Point) -> f64, lines: &[Point]| {
        targets
            .iter()
            .chain(lines)
            .filter(|t| (along(**t) - along(p)).abs() <= tol)
            .min_by(|a, b| {
                let key =
                    |t: &Point| ((along(*t) - along(p)).abs(), (across(*t) - across(p)).abs());
                let (ka, kb) = (key(a), key(b));
                ka.0.total_cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
            })
            .copied()
    };
    let round = |v: f64| match grid {
        Some(step) if step > 0.0 => (v / step).round() * step,
        _ => v,
    };
    let gx = if axes.x {
        best(|t| t.x, |t| t.y, x_lines)
    } else {
        None
    };
    let gy = if axes.y {
        best(|t| t.y, |t| t.x, y_lines)
    } else {
        None
    };
    let x = match gx {
        Some(t) => t.x,
        None if axes.x => round(p.x),
        None => p.x,
    };
    let y = match gy {
        Some(t) => t.y,
        None if axes.y => round(p.y),
        None => p.y,
    };
    Snapped {
        point: Point::new(x, y),
        guide_x: gx,
        guide_y: gy,
    }
}

/// Restringe `p` a la dirección múltiplo de `step_deg` más cercana vista desde `origin`
/// (proyección sobre esa dirección). Devuelve el punto y los ejes que siguen libres para el
/// ajuste: en horizontal solo x, en vertical solo y, en diagonal ninguno.
pub fn constrain_angle(origin: Point, p: Point, step_deg: f64) -> (Point, Axes) {
    let d = p - origin;
    if d.hypot() < 1e-12 || step_deg <= 0.0 {
        return (p, Axes::BOTH);
    }
    let step = step_deg.to_radians();
    let ang = (d.y.atan2(d.x) / step).round() * step;
    let (sin, cos) = ang.sin_cos();
    let len = d.x * cos + d.y * sin;
    let (horizontal, vertical) = (sin.abs() < 1e-9, cos.abs() < 1e-9);
    // En los ejes la coordenada fija es exacta (sin el error del seno o el coseno).
    let q = if horizontal {
        Point::new(origin.x + len * cos.signum(), origin.y)
    } else if vertical {
        Point::new(origin.x, origin.y + len * sin.signum())
    } else {
        Point::new(origin.x + len * cos, origin.y + len * sin)
    };
    (
        q,
        Axes {
            x: horizontal,
            y: vertical,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_each_axis_to_its_nearest_target() {
        let targets = [Point::new(10.0, 50.0), Point::new(40.0, 20.0)];
        let s = snap_point(Point::new(10.4, 19.7), &targets, 0.5, None, Axes::BOTH);
        assert_eq!(s.point, Point::new(10.0, 20.0));
        assert_eq!(s.guide_x, Some(targets[0]));
        assert_eq!(s.guide_y, Some(targets[1]));
        // Fuera de la tolerancia no se ajusta.
        let s = snap_point(Point::new(11.0, 30.0), &targets, 0.5, None, Axes::BOTH);
        assert_eq!(s.point, Point::new(11.0, 30.0));
        assert!(s.guide_x.is_none() && s.guide_y.is_none());
    }

    #[test]
    fn prefers_the_closest_target_on_ties() {
        // Dos objetivos con la misma x: la guía sale del más cercano.
        let targets = [Point::new(10.0, 90.0), Point::new(10.0, 12.0)];
        let s = snap_point(Point::new(10.2, 15.0), &targets, 0.5, None, Axes::BOTH);
        assert_eq!(s.guide_x, Some(targets[1]));
    }

    #[test]
    fn grid_fills_in_where_there_is_no_target() {
        let targets = [Point::new(10.3, 0.0)];
        let s = snap_point(Point::new(10.4, 7.4), &targets, 0.5, Some(5.0), Axes::BOTH);
        assert_eq!(
            s.point,
            Point::new(10.3, 5.0),
            "el objetivo gana a la retícula"
        );
        // Un eje bloqueado no se toca.
        let s = snap_point(
            Point::new(10.4, 7.4),
            &targets,
            0.5,
            Some(5.0),
            Axes { x: false, y: true },
        );
        assert_eq!(s.point, Point::new(10.4, 5.0));
    }

    #[test]
    fn guide_lines_snap_one_axis_each() {
        let p = Point::new(50.3, 79.8);
        let s = snap_point_with_lines(p, &[], &[50.0], &[80.0, 120.0], 0.5, None, Axes::BOTH);
        assert_eq!(s.point, Point::new(50.0, 80.0));
        // Una guía vertical no ajusta la y, ni una lejana ajusta nada.
        let s = snap_point_with_lines(p, &[], &[50.0], &[120.0], 0.5, None, Axes::BOTH);
        assert_eq!(s.point, Point::new(50.0, 79.8));
        // Con un punto y una guía a la misma distancia en x, gana la guía (está a su altura).
        let s = snap_point_with_lines(
            Point::new(50.0, 79.8),
            &[Point::new(50.2, 10.0)],
            &[49.8],
            &[],
            0.5,
            None,
            Axes::BOTH,
        );
        assert_eq!(s.point.x, 49.8);
    }

    #[test]
    fn constrains_to_axes_and_diagonals() {
        let o = Point::new(10.0, 10.0);
        let (p, axes) = constrain_angle(o, Point::new(30.0, 12.0), 45.0);
        assert_eq!(p, Point::new(30.0, 10.0));
        assert_eq!(axes, Axes { x: true, y: false });
        let (p, axes) = constrain_angle(o, Point::new(9.0, -20.0), 45.0);
        assert_eq!(p, Point::new(10.0, -20.0));
        assert_eq!(axes, Axes { x: false, y: true });
        let (p, axes) = constrain_angle(o, Point::new(20.0, 22.0), 45.0);
        assert!((p.x - 21.0).abs() < 1e-9 && (p.y - 21.0).abs() < 1e-9);
        assert_eq!(axes, Axes::NONE);
    }
}
