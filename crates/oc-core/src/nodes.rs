//! Nodos Bézier de los trazados (Editar puntos, como la referencia).
//!
//! Un trazado es una lista de nodos; cada nodo tiene su ancla y, opcionalmente, un manejador
//! de entrada y otro de salida. El tramo entre dos nodos es recto si ninguno de los dos
//! manejadores que lo afectan existe; si no, es una cúbica. Los nodos «suaves» mantienen sus
//! dos manejadores alineados.

use kurbo::{BezPath, CubicBez, ParamCurve, ParamCurveNearest, PathEl, Point};
use serde::{Deserialize, Serialize};

/// Tolerancia (mm) al aplanar curvas para dibujo y geometría.
pub const FLATTEN_TOL_MM: f64 = 0.02;

/// Manejadores de un nodo (en las mismas coordenadas que su ancla).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Handles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h_in: Option<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h_out: Option<Point>,
    #[serde(default)]
    pub smooth: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    pub p: Point,
    pub h_in: Option<Point>,
    pub h_out: Option<Point>,
    pub smooth: bool,
}

impl Node {
    pub fn line(p: Point) -> Node {
        Node {
            p,
            h_in: None,
            h_out: None,
            smooth: false,
        }
    }

    /// ¿Es un nodo de Bézier (con algún manejador)?
    pub fn is_bezier(&self) -> bool {
        self.h_in.is_some() || self.h_out.is_some()
    }

    pub fn map(&self, f: impl Fn(Point) -> Point) -> Node {
        Node {
            p: f(self.p),
            h_in: self.h_in.map(&f),
            h_out: self.h_out.map(&f),
            smooth: self.smooth,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodePath {
    pub nodes: Vec<Node>,
    pub closed: bool,
}

impl NodePath {
    pub fn from_points(pts: &[Point], closed: bool) -> NodePath {
        NodePath {
            nodes: pts.iter().map(|p| Node::line(*p)).collect(),
            closed,
        }
    }

    pub fn map(&self, f: impl Fn(Point) -> Point + Copy) -> NodePath {
        NodePath {
            nodes: self.nodes.iter().map(|n| n.map(f)).collect(),
            closed: self.closed,
        }
    }

    /// Número de tramos.
    pub fn segment_count(&self) -> usize {
        let n = self.nodes.len();
        if n < 2 {
            0
        } else if self.closed {
            n
        } else {
            n - 1
        }
    }

    /// Tramo `i` (del nodo `i` al siguiente): `None` si es recto.
    pub fn segment(&self, i: usize) -> (Point, Option<CubicBez>, Point) {
        let n = self.nodes.len();
        let (a, b) = (self.nodes[i], self.nodes[(i + 1) % n]);
        if a.h_out.is_none() && b.h_in.is_none() {
            (a.p, None, b.p)
        } else {
            (
                a.p,
                Some(CubicBez::new(
                    a.p,
                    a.h_out.unwrap_or(a.p),
                    b.h_in.unwrap_or(b.p),
                    b.p,
                )),
                b.p,
            )
        }
    }

    pub fn to_bez(&self) -> BezPath {
        let mut b = BezPath::new();
        if self.nodes.is_empty() {
            return b;
        }
        b.move_to(self.nodes[0].p);
        for i in 0..self.segment_count() {
            match self.segment(i) {
                (_, Some(c), _) => b.curve_to(c.p1, c.p2, c.p3),
                (_, None, end) => b.line_to(end),
            }
        }
        if self.closed {
            b.close_path();
        }
        b
    }

    /// Polilínea equivalente (las curvas se aplanan con `tol`).
    pub fn flatten(&self, tol: f64) -> Vec<Point> {
        if !self.nodes.iter().any(Node::is_bezier) {
            return self.nodes.iter().map(|n| n.p).collect();
        }
        let mut pts = Vec::new();
        kurbo::flatten(self.to_bez(), tol, |el| match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => pts.push(p),
            _ => {}
        });
        if self.closed && pts.len() > 1 && pts[0].distance(*pts.last().unwrap()) < 1e-9 {
            pts.pop();
        }
        pts
    }

    /// Tramo más cercano a `p`: (índice, parámetro t, distancia).
    pub fn nearest_segment(&self, p: Point) -> Option<(usize, f64, f64)> {
        (0..self.segment_count())
            .map(|i| match self.segment(i) {
                (_, Some(c), _) => {
                    let n = c.nearest(p, 1e-6);
                    (i, n.t, n.distance_sq.sqrt())
                }
                (a, None, b) => {
                    let ab = b - a;
                    let t = if ab.hypot2() > 0.0 {
                        ((p - a).dot(ab) / ab.hypot2()).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    (i, t, p.distance(a + ab * t))
                }
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
    }

    /// Inserta un nodo en el tramo `i` en `t` sin cambiar la forma. Devuelve su índice.
    pub fn insert_at(&mut self, i: usize, t: f64) -> usize {
        let n = self.nodes.len();
        let j = (i + 1) % n;
        match self.segment(i) {
            (a, None, b) => {
                let p = a.lerp(b, t);
                self.nodes.insert(i + 1, Node::line(p));
            }
            (_, Some(c), _) => {
                let (l, r) = (c.subsegment(0.0..t), c.subsegment(t..1.0));
                self.nodes[i].h_out = Some(l.p1);
                self.nodes[j].h_in = Some(r.p2);
                let node = Node {
                    p: l.p3,
                    h_in: Some(l.p2),
                    h_out: Some(r.p1),
                    smooth: true,
                };
                self.nodes.insert(i + 1, node);
            }
        }
        i + 1
    }

    /// Borra el nodo `i`; los tramos vecinos se unen en una curva si alguno lo era.
    pub fn delete(&mut self, i: usize) {
        if self.nodes.len() <= 2 || i >= self.nodes.len() {
            return;
        }
        self.nodes.remove(i);
    }

    /// Rompe el trazado en el nodo `i`. Un trazado cerrado se abre ahí (el nodo se duplica en
    /// los dos extremos); uno abierto se parte en dos. Devuelve el o los trazados resultantes.
    pub fn break_at(&self, i: usize) -> Vec<NodePath> {
        let n = self.nodes.len();
        if self.closed {
            let mut nodes: Vec<Node> = (0..=n).map(|k| self.nodes[(i + k) % n]).collect();
            nodes[0].h_in = None;
            nodes[n].h_out = None;
            vec![NodePath {
                nodes,
                closed: false,
            }]
        } else {
            if i == 0 || i + 1 >= n {
                return vec![self.clone()];
            }
            let mut a: Vec<Node> = self.nodes[..=i].to_vec();
            let mut b: Vec<Node> = self.nodes[i..].to_vec();
            a.last_mut().unwrap().h_out = None;
            b[0].h_in = None;
            vec![
                NodePath {
                    nodes: a,
                    closed: false,
                },
                NodePath {
                    nodes: b,
                    closed: false,
                },
            ]
        }
    }

    /// El mismo trazado recorrido al revés (los manejadores de entrada y salida se cambian).
    pub fn reversed(&self) -> NodePath {
        NodePath {
            nodes: self
                .nodes
                .iter()
                .rev()
                .map(|n| Node {
                    h_in: n.h_out,
                    h_out: n.h_in,
                    ..*n
                })
                .collect(),
            closed: self.closed,
        }
    }

    /// Nodo de vértice: los manejadores se mueven por separado.
    pub fn make_corner(&mut self, i: usize) {
        self.nodes[i].smooth = false;
    }

    /// Nodo suave: manejadores alineados (si no tenía, se crean a un tercio de los vecinos).
    pub fn make_smooth(&mut self, i: usize) {
        let n = self.nodes.len();
        let p = self.nodes[i].p;
        let prev = if i > 0 || self.closed {
            Some(self.nodes[(i + n - 1) % n].p)
        } else {
            None
        };
        let next = if i + 1 < n || self.closed {
            Some(self.nodes[(i + 1) % n].p)
        } else {
            None
        };
        let dir = match (prev, next) {
            (Some(a), Some(b)) => b - a,
            (Some(a), None) => p - a,
            (None, Some(b)) => b - p,
            _ => return,
        };
        if dir.hypot() < 1e-12 {
            return;
        }
        let u = dir.normalize();
        let len_in = prev
            .map(|a| {
                self.nodes[i]
                    .h_in
                    .map_or(p.distance(a) / 3.0, |h| h.distance(p))
            })
            .unwrap_or(0.0);
        let len_out = next
            .map(|b| {
                self.nodes[i]
                    .h_out
                    .map_or(p.distance(b) / 3.0, |h| h.distance(p))
            })
            .unwrap_or(0.0);
        let node = &mut self.nodes[i];
        node.smooth = true;
        if prev.is_some() {
            node.h_in = Some(p - u * len_in);
        }
        if next.is_some() {
            node.h_out = Some(p + u * len_out);
        }
    }

    /// Convierte el tramo `i` en recta.
    pub fn segment_to_line(&mut self, i: usize) {
        let n = self.nodes.len();
        self.nodes[i].h_out = None;
        self.nodes[(i + 1) % n].h_in = None;
    }

    /// Convierte el tramo `i` en curva (manejadores a un tercio, sobre la recta).
    pub fn segment_to_curve(&mut self, i: usize) {
        let n = self.nodes.len();
        let j = (i + 1) % n;
        let (a, b) = (self.nodes[i].p, self.nodes[j].p);
        if self.nodes[i].h_out.is_none() {
            self.nodes[i].h_out = Some(a.lerp(b, 1.0 / 3.0));
        }
        if self.nodes[j].h_in.is_none() {
            self.nodes[j].h_in = Some(a.lerp(b, 2.0 / 3.0));
        }
    }

    /// Mueve un manejador (`out` = de salida). En un nodo suave, el opuesto gira con él y
    /// conserva su longitud.
    pub fn move_handle(&mut self, i: usize, out: bool, to: Point) {
        let node = &mut self.nodes[i];
        let p = node.p;
        if out {
            node.h_out = Some(to);
        } else {
            node.h_in = Some(to);
        }
        if node.smooth {
            let d = to - p;
            if d.hypot() > 1e-12 {
                let u = d.normalize();
                if out {
                    if let Some(h) = node.h_in {
                        node.h_in = Some(p - u * h.distance(p));
                    }
                } else if let Some(h) = node.h_out {
                    node.h_out = Some(p - u * h.distance(p));
                }
            }
        }
    }

    /// Mueve un nodo con sus manejadores.
    pub fn move_node(&mut self, i: usize, to: Point) {
        let d = to - self.nodes[i].p;
        let node = &mut self.nodes[i];
        node.p = to;
        node.h_in = node.h_in.map(|h| h + d);
        node.h_out = node.h_out.map(|h| h + d);
    }
}

/// Trazados de nodos a partir de un `BezPath` (las cuádricas se elevan a cúbicas).
pub fn from_bez(path: &BezPath) -> Vec<NodePath> {
    let mut out: Vec<NodePath> = Vec::new();
    let mut cur: Option<NodePath> = None;
    let finish = |cur: &mut Option<NodePath>, out: &mut Vec<NodePath>, closed: bool| {
        if let Some(mut p) = cur.take() {
            if closed && p.nodes.len() > 1 {
                // El último nodo repite el primero: se funden sus manejadores.
                let last = *p.nodes.last().unwrap();
                if last.p.distance(p.nodes[0].p) < 1e-9 {
                    p.nodes.pop();
                    p.nodes[0].h_in = last.h_in;
                }
            }
            p.closed = closed;
            if p.nodes.len() >= 2 {
                out.push(p);
            }
        }
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                finish(&mut cur, &mut out, false);
                cur = Some(NodePath {
                    nodes: vec![Node::line(p)],
                    closed: false,
                });
            }
            PathEl::LineTo(p) => {
                if let Some(c) = cur.as_mut() {
                    c.nodes.push(Node::line(p));
                }
            }
            PathEl::QuadTo(q, p) => {
                if let Some(c) = cur.as_mut() {
                    let a = c.nodes.last().unwrap().p;
                    c.nodes.last_mut().unwrap().h_out = Some(a + (q - a) * (2.0 / 3.0));
                    c.nodes.push(Node {
                        p,
                        h_in: Some(p + (q - p) * (2.0 / 3.0)),
                        h_out: None,
                        smooth: false,
                    });
                }
            }
            PathEl::CurveTo(c1, c2, p) => {
                if let Some(c) = cur.as_mut() {
                    c.nodes.last_mut().unwrap().h_out = Some(c1);
                    c.nodes.push(Node {
                        p,
                        h_in: Some(c2),
                        h_out: None,
                        smooth: false,
                    });
                }
            }
            PathEl::ClosePath => finish(&mut cur, &mut out, true),
        }
    }
    finish(&mut cur, &mut out, false);
    // Marcar como suaves los nodos cuyos manejadores están alineados.
    for p in &mut out {
        for n in &mut p.nodes {
            if let (Some(a), Some(b)) = (n.h_in, n.h_out) {
                let (u, v) = (n.p - a, b - n.p);
                if u.hypot() > 1e-9 && v.hypot() > 1e-9 {
                    n.smooth = (u.normalize().cross(v.normalize())).abs() < 0.02 && u.dot(v) > 0.0;
                }
            }
        }
    }
    out
}

/// Elipse como 4 nodos suaves (constante κ).
pub fn ellipse(r: kurbo::Rect) -> NodePath {
    let k = 0.552_284_749_830_793_6;
    let (c, rx, ry) = (r.center(), r.width() / 2.0, r.height() / 2.0);
    let pts = [
        (Point::new(c.x, r.y0), kurbo::Vec2::new(rx, 0.0)),
        (Point::new(r.x1, c.y), kurbo::Vec2::new(0.0, ry)),
        (Point::new(c.x, r.y1), kurbo::Vec2::new(-rx, 0.0)),
        (Point::new(r.x0, c.y), kurbo::Vec2::new(0.0, -ry)),
    ];
    NodePath {
        nodes: pts
            .iter()
            .map(|(p, t)| Node {
                p: *p,
                h_in: Some(*p - *t * k),
                h_out: Some(*p + *t * k),
                smooth: true,
            })
            .collect(),
        closed: true,
    }
}

/// Extremo libre más cercano al nodo `ni` del trazado abierto `si` (que debe ser un extremo):
/// (trazado, nodo). Puede ser el otro extremo del mismo trazado (unirlos lo cierra).
pub fn join_target(paths: &[NodePath], si: usize, ni: usize) -> Option<(usize, usize)> {
    let p = paths.get(si)?;
    let last = p.nodes.len().checked_sub(1)?;
    if p.closed || (ni != 0 && ni != last) || last == 0 {
        return None;
    }
    let at = p.nodes[ni].p;
    paths
        .iter()
        .enumerate()
        .filter(|(_, q)| !q.closed && q.nodes.len() >= 2)
        .flat_map(|(k, q)| [(k, 0), (k, q.nodes.len() - 1)])
        // Consigo mismo solo vale el otro extremo, y con al menos tres nodos.
        .filter(|&(k, j)| k != si || (j != ni && p.nodes.len() >= 3))
        .min_by(|a, b| {
            let d = |(k, j): (usize, usize)| (paths[k].nodes[j].p - at).hypot();
            d(*a).total_cmp(&d(*b))
        })
}

/// Nodo que funde dos extremos en el punto medio: conserva el manejador de entrada de `a` y el
/// de salida de `b`, desplazados con el nodo.
fn fuse(a: &Node, b: &Node) -> Node {
    let p = a.p.midpoint(b.p);
    Node {
        p,
        h_in: a.h_in.map(|h| h + (p - a.p)),
        h_out: b.h_out.map(|h| h + (p - b.p)),
        smooth: false,
    }
}

/// Une el extremo `ni` del trazado `si` con su [`join_target`]: los dos extremos se funden en
/// un solo nodo, en su punto medio. Devuelve el nodo de la unión.
pub fn join_paths(paths: &mut Vec<NodePath>, si: usize, ni: usize) -> Option<(usize, usize)> {
    let (ti, tj) = join_target(paths, si, ni)?;
    if ti == si {
        let p = &mut paths[si];
        let end = p.nodes.pop().expect("hay nodos");
        p.nodes[0] = fuse(&end, &p.nodes[0]);
        p.closed = true;
        return Some((si, 0));
    }
    // El trazado seleccionado acaba en la unión y el otro empieza en ella.
    let mut a = if ni == 0 {
        paths[si].reversed()
    } else {
        paths[si].clone()
    };
    let b = if tj == 0 {
        paths[ti].clone()
    } else {
        paths[ti].reversed()
    };
    let joint = a.nodes.len() - 1;
    a.nodes[joint] = fuse(&a.nodes[joint], &b.nodes[0]);
    a.nodes.extend(b.nodes.into_iter().skip(1));
    paths[si] = a;
    paths.remove(ti);
    Some((if ti < si { si - 1 } else { si }, joint))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> NodePath {
        NodePath::from_points(
            &[
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 10.0),
            ],
            true,
        )
    }

    #[test]
    fn insert_on_line_and_curve_keeps_shape() {
        let mut s = square();
        let (i, t, d) = s.nearest_segment(Point::new(5.0, 0.3)).unwrap();
        assert_eq!(i, 0);
        assert!(d < 0.31 && (t - 0.5).abs() < 1e-9);
        s.insert_at(i, t);
        assert_eq!(s.nodes.len(), 5);
        assert_eq!(s.nodes[1].p, Point::new(5.0, 0.0));
        // En una curva: el punto partido está sobre la curva original.
        let mut e = ellipse(kurbo::Rect::new(0.0, 0.0, 20.0, 10.0));
        let before = e.segment(0).1.unwrap().eval(0.3);
        let k = e.insert_at(0, 0.3);
        assert!(e.nodes[k].p.distance(before) < 1e-9);
        assert_eq!(e.nodes.len(), 5);
    }

    #[test]
    fn break_closed_opens_and_open_splits() {
        let s = square();
        let b = s.break_at(2);
        assert_eq!(b.len(), 1);
        assert!(!b[0].closed);
        assert_eq!(b[0].nodes.len(), 5);
        assert_eq!(b[0].nodes[0].p, b[0].nodes[4].p);
        let parts = b[0].break_at(2);
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn smooth_handles_stay_aligned() {
        let mut s = square();
        s.make_smooth(1);
        assert!(s.nodes[1].smooth && s.nodes[1].h_in.is_some() && s.nodes[1].h_out.is_some());
        s.move_handle(1, true, Point::new(14.0, 4.0));
        let n = s.nodes[1];
        let (u, v) = (n.p - n.h_in.unwrap(), n.h_out.unwrap() - n.p);
        assert!(u.normalize().cross(v.normalize()).abs() < 1e-9);
        // Vértice: el opuesto no se mueve.
        s.make_corner(1);
        let keep = s.nodes[1].h_in;
        s.move_handle(1, true, Point::new(12.0, -3.0));
        assert_eq!(s.nodes[1].h_in, keep);
    }

    #[test]
    fn line_curve_round_trip_and_bez() {
        let mut s = square();
        s.segment_to_curve(0);
        assert!(s.segment(0).1.is_some());
        s.segment_to_line(0);
        assert!(s.segment(0).1.is_none());
        let e = ellipse(kurbo::Rect::new(0.0, 0.0, 20.0, 10.0));
        let back = from_bez(&e.to_bez());
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].nodes.len(), 4);
        assert!(back[0].closed && back[0].nodes.iter().all(|n| n.smooth));
    }

    #[test]
    fn join_reverses_break() {
        let sq = NodePath::from_points(
            &[
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 10.0),
            ],
            true,
        );
        // Cerrado roto en un nodo: unir lo vuelve a cerrar con los mismos cuatro nodos.
        let mut p = sq.break_at(1);
        assert_eq!(p[0].nodes.len(), 5);
        let sel = super::join_paths(&mut p, 0, 0);
        assert!(sel.is_some() && p.len() == 1 && p[0].closed && p[0].nodes.len() == 4);
        // Abierto partido en dos: unir deja un solo trazado con el nodo común fundido.
        let open = NodePath::from_points(
            &[
                Point::new(0.0, 0.0),
                Point::new(5.0, 0.0),
                Point::new(9.0, 3.0),
            ],
            false,
        );
        let mut p = open.break_at(1);
        assert_eq!(p.len(), 2);
        let sel = super::join_paths(&mut p, 1, 0).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].nodes.len(), 3);
        assert_eq!(p[0].nodes[sel.1].p, Point::new(5.0, 0.0));
        // Extremos separados: se funden en un nodo en el punto medio.
        let a = NodePath::from_points(&[Point::new(0.0, 0.0), Point::new(4.0, 0.0)], false);
        let b = NodePath::from_points(&[Point::new(20.0, 0.0), Point::new(6.0, 0.0)], false);
        let mut p = vec![a, b];
        let sel = super::join_paths(&mut p, 0, 1).unwrap();
        assert_eq!(p.len(), 1);
        let xs: Vec<f64> = p[0].nodes.iter().map(|n| n.p.x).collect();
        assert_eq!(xs, vec![0.0, 5.0, 20.0]);
        assert_eq!(sel, (0, 1));
        // Cerrar un trazado con los extremos separados también los funde.
        let mut p = vec![NodePath::from_points(
            &[
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 2.0),
            ],
            false,
        )];
        super::join_paths(&mut p, 0, 3).unwrap();
        assert!(p[0].closed && p[0].nodes.len() == 3);
        assert_eq!(p[0].nodes[0].p, Point::new(0.0, 1.0));
        // Un nodo intermedio no es un extremo.
        let mut p = vec![open.clone()];
        assert!(super::join_paths(&mut p, 0, 1).is_none());
    }
}
