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
    /// Suave y además con los dos manejadores de la misma longitud.
    #[serde(default, skip_serializing_if = "is_false")]
    pub symmetric: bool,
    /// Radio de redondeo de la esquina, en mm (0 = esquina viva). No se normaliza con la caja:
    /// al escalar la forma el radio se conserva.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub radius: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    pub p: Point,
    pub h_in: Option<Point>,
    pub h_out: Option<Point>,
    pub smooth: bool,
    /// Simétrico: suave y con los dos manejadores de la misma longitud.
    pub symmetric: bool,
    /// Radio de redondeo de la esquina, en mm (0 = esquina viva). El nodo sigue en el vértice;
    /// el arco aparece al resolver el trazado ([`NodePath::resolved`]).
    pub radius: f64,
}

impl Node {
    pub fn line(p: Point) -> Node {
        Node {
            p,
            h_in: None,
            h_out: None,
            smooth: false,
            symmetric: false,
            radius: 0.0,
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
            symmetric: self.symmetric,
            radius: self.radius,
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

    /// ¿Tiene alguna esquina con radio de redondeo?
    pub fn has_rounding(&self) -> bool {
        self.nodes.iter().any(|n| n.radius > 0.0)
    }

    /// El trazado tal como se dibuja y se corta: cada esquina con radio sustituida por su arco.
    /// Los nodos del resultado ya no llevan radio.
    pub fn resolved(&self) -> NodePath {
        if !self.has_rounding() {
            return self.clone();
        }
        let corners: Vec<(usize, f64)> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.radius > 0.0)
            .map(|(i, n)| (i, n.radius))
            .collect();
        let mut out = fillet_corners_each(self, &corners).0;
        for n in &mut out.nodes {
            n.radius = 0.0;
        }
        out
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
                    symmetric: false,
                    radius: 0.0,
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
        self.nodes[i].symmetric = false;
    }

    /// Nodo simétrico: suave y con los dos manejadores de la misma longitud (la media).
    pub fn make_symmetric(&mut self, i: usize) {
        self.make_smooth(i);
        let node = &mut self.nodes[i];
        if !node.smooth {
            return;
        }
        node.symmetric = true;
        if let (Some(a), Some(b)) = (node.h_in, node.h_out) {
            let len = (a.distance(node.p) + b.distance(node.p)) / 2.0;
            let u = (b - a).normalize();
            node.h_in = Some(node.p - u * len);
            node.h_out = Some(node.p + u * len);
        }
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
        node.symmetric = false;
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
    /// conserva su longitud; en uno simétrico, además la iguala.
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
                let symmetric = node.symmetric;
                let opposite = if out { &mut node.h_in } else { &mut node.h_out };
                if let Some(h) = *opposite {
                    let len = if symmetric { d.hypot() } else { h.distance(p) };
                    *opposite = Some(p - u * len);
                }
            }
        }
    }

    /// Invierte el sentido del trazado sin cambiar su forma. Uno cerrado conserva su nodo
    /// inicial.
    pub fn reverse(&mut self) {
        let mut r = self.reversed();
        if r.closed && !r.nodes.is_empty() {
            r.nodes.rotate_right(1);
        }
        *self = r;
    }

    /// Cierra un trazado abierto con un tramo recto del último nodo al primero. Si los dos
    /// extremos coinciden se funden en un solo nodo.
    pub fn close(&mut self) {
        if self.closed || self.nodes.len() < 3 {
            return;
        }
        let last = *self.nodes.last().expect("hay nodos");
        if last.p.distance(self.nodes[0].p) < 1e-6 {
            self.nodes.pop();
            self.nodes[0].h_in = last.h_in;
        } else {
            self.nodes[0].h_in = None;
            self.nodes.last_mut().expect("hay nodos").h_out = None;
        }
        self.closed = true;
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
                        symmetric: false,
                        radius: 0.0,
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
                        symmetric: false,
                        radius: 0.0,
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
                symmetric: false,
                radius: 0.0,
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
        symmetric: false,
        radius: 0.0,
    }
}

/// ¿Es el nodo `ni` un extremo libre del trazado `si`?
pub fn is_free_end(paths: &[NodePath], si: usize, ni: usize) -> bool {
    paths
        .get(si)
        .is_some_and(|p| !p.closed && p.nodes.len() >= 2 && (ni == 0 || ni + 1 == p.nodes.len()))
}

/// Une el extremo `ni` del trazado `si` con su [`join_target`]: los dos extremos se funden en
/// un solo nodo, en su punto medio. Devuelve el nodo de la unión.
pub fn join_paths(paths: &mut Vec<NodePath>, si: usize, ni: usize) -> Option<(usize, usize)> {
    let target = join_target(paths, si, ni)?;
    join_ends(paths, (si, ni), target)
}

/// Une dos extremos libres concretos (de trazados distintos o los dos del mismo, que queda
/// cerrado) fundiéndolos en un solo nodo. Devuelve el nodo de la unión.
pub fn join_ends(
    paths: &mut Vec<NodePath>,
    (si, ni): (usize, usize),
    (ti, tj): (usize, usize),
) -> Option<(usize, usize)> {
    if !is_free_end(paths, si, ni) || !is_free_end(paths, ti, tj) {
        return None;
    }
    if ti == si && (tj == ni || paths[si].nodes.len() < 3) {
        return None;
    }
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

/// Un nodo: (trazado, índice del nodo).
type NodeRef = (usize, usize);

/// Cuánto de un tramo puede ocupar un redondeo: la mitad si el nodo del otro extremo también
/// se redondea, casi todo si no.
const FILLET_SHARED: f64 = 0.5;
const FILLET_ALONE: f64 = 0.95;

/// Dirección (unitaria) con la que el tramo `seg` llega a su nodo final (`at_end`) o sale de
/// su nodo inicial.
fn segment_direction(np: &NodePath, seg: usize, at_end: bool) -> Option<kurbo::Vec2> {
    let d = match np.segment(seg) {
        (a, None, b) => b - a,
        (_, Some(c), _) => {
            // Si el manejador coincide con su nodo, la tangente la da el siguiente punto.
            let pts = if at_end {
                [c.p3 - c.p2, c.p3 - c.p1, c.p3 - c.p0]
            } else {
                [c.p1 - c.p0, c.p2 - c.p0, c.p3 - c.p0]
            };
            pts.into_iter().find(|v| v.hypot() > 1e-9)?
        }
    };
    (d.hypot() > 1e-9).then(|| d.normalize())
}

fn segment_length(np: &NodePath, seg: usize) -> f64 {
    use kurbo::ParamCurveArclen;
    match np.segment(seg) {
        (a, None, b) => a.distance(b),
        (_, Some(c), _) => c.arclen(1e-6),
    }
}

/// ¿Es el nodo `i` una esquina que se puede redondear? Tiene que tener tramo a cada lado y que
/// estos formen ángulo (un nodo suave o un extremo de un trazado abierto no lo son).
pub fn is_corner(np: &NodePath, i: usize) -> bool {
    corner_turn(np, i).is_some()
}

/// Tramos que llegan y salen del nodo `i` y el ángulo que gira el trazado en él (rad).
fn corner_turn(np: &NodePath, i: usize) -> Option<(usize, usize, f64)> {
    let n = np.nodes.len();
    if i >= n || n < 3 || (!np.closed && (i == 0 || i + 1 == n)) {
        return None;
    }
    let (seg_in, seg_out) = ((i + n - 1) % n, i);
    let a = segment_direction(np, seg_in, true)?;
    let b = segment_direction(np, seg_out, false)?;
    let turn = a.dot(b).clamp(-1.0, 1.0).acos();
    (turn > 0.02 && turn < std::f64::consts::PI - 0.02).then_some((seg_in, seg_out, turn))
}

/// Redondea las esquinas `corners` de un trazado con arcos de radio `radius` tangentes a los
/// dos tramos de cada esquina (cada esquina pasa a ser dos nodos unidos por una curva). Si el
/// radio no cabe en los tramos, se usa el mayor que cabe. Devuelve el trazado y los nodos nuevos.
pub fn fillet_corners(np: &NodePath, corners: &[usize], radius: f64) -> (NodePath, Vec<usize>) {
    let each: Vec<(usize, f64)> = corners.iter().map(|i| (*i, radius)).collect();
    fillet_corners_each(np, &each)
}

/// Como [`fillet_corners`], con un radio por esquina: (nodo, radio).
pub fn fillet_corners_each(np: &NodePath, corners: &[(usize, f64)]) -> (NodePath, Vec<usize>) {
    use kurbo::ParamCurveArclen;
    let mut out = np.clone();
    let mut added: Vec<usize> = Vec::new();
    let n = np.nodes.len();
    let mut each: Vec<(usize, f64)> = corners
        .iter()
        .copied()
        .filter(|(i, r)| *r > 0.0 && is_corner(np, *i))
        .collect();
    each.sort_by_key(|c| c.0);
    each.dedup_by_key(|c| c.0);
    let list: Vec<usize> = each.iter().map(|c| c.0).collect();
    // Recorte de cada esquina (distancia del vértice a los puntos de tangencia), calculado
    // sobre el trazado original para que no dependa del orden.
    let trims: Vec<(usize, f64, f64)> = each
        .iter()
        .filter_map(|&(i, radius)| {
            let (seg_in, seg_out, turn) = corner_turn(np, i)?;
            let share = |other: usize| {
                if list.contains(&other) {
                    FILLET_SHARED
                } else {
                    FILLET_ALONE
                }
            };
            let room_in = segment_length(np, seg_in) * share((i + n - 1) % n);
            let room_out = segment_length(np, seg_out) * share((i + 1) % n);
            let d = (radius * (turn / 2.0).tan()).min(room_in).min(room_out);
            (d > 1e-6).then_some((i, d, turn))
        })
        .collect();
    // De atrás hacia delante: cada esquina añade un nodo y no mueve los índices anteriores.
    for &(i, d, turn) in trims.iter().rev() {
        let len = out.nodes.len();
        let (prev, next) = ((i + len - 1) % len, (i + 1) % len);
        let corner = out.nodes[i].p;
        // Punto de tangencia del tramo de entrada (y manejadores del trozo que queda).
        let (p1, prev_out, a_in) = match out.segment(prev) {
            (a, None, b) => (b + (a - b).normalize() * d, None, None),
            (_, Some(c), _) => {
                let total = c.arclen(1e-6);
                let l = c.subsegment(0.0..c.inv_arclen((total - d).max(0.0), 1e-6));
                (l.p3, Some(l.p1), Some(l.p2))
            }
        };
        let (p2, b_out, next_in) = match out.segment(i) {
            (a, None, b) => (a + (b - a).normalize() * d, None, None),
            (_, Some(c), _) => {
                let r = c.subsegment(c.inv_arclen(d, 1e-6)..1.0);
                (r.p0, Some(r.p1), Some(r.p2))
            }
        };
        // Arco como cúbica: manejadores sobre las tangentes, hacia el vértice.
        let k = 4.0 / 3.0 * (turn / 4.0).tan() * d / (turn / 2.0).tan();
        let toward = |from: Point, handle: Option<Point>| {
            let v = handle.map(|h| from - h).filter(|v| v.hypot() > 1e-9);
            v.unwrap_or(corner - from).normalize()
        };
        let a = Node {
            p: p1,
            h_in: a_in,
            h_out: Some(p1 + toward(p1, a_in) * k),
            smooth: true,
            symmetric: false,
            radius: 0.0,
        };
        let b = Node {
            p: p2,
            h_in: Some(p2 + toward(p2, b_out) * k),
            h_out: b_out,
            smooth: true,
            symmetric: false,
            radius: 0.0,
        };
        out.nodes[prev].h_out = prev_out;
        out.nodes[next].h_in = next_in;
        out.nodes[i] = a;
        out.nodes.insert(i + 1, b);
        // Los nodos ya añadidos detrás se desplazan uno.
        for k in &mut added {
            *k += 1;
        }
        added.extend([i, i + 1]);
    }
    added.sort_unstable();
    (out, added)
}

/// Conecta dos extremos libres con un tramo recto, sin mover ningún nodo (de trazados
/// distintos, que pasan a ser uno, o los dos del mismo, que queda cerrado). Devuelve el nodo
/// del primer extremo en el trazado resultante.
pub fn connect_ends(
    paths: &mut Vec<NodePath>,
    (si, ni): (usize, usize),
    (ti, tj): (usize, usize),
) -> Option<(usize, usize)> {
    if !is_free_end(paths, si, ni) || !is_free_end(paths, ti, tj) {
        return None;
    }
    if ti == si {
        if tj == ni || paths[si].nodes.len() < 3 {
            return None;
        }
        paths[si].close();
        return Some((si, ni));
    }
    let mut a = if ni == 0 {
        paths[si].reversed()
    } else {
        paths[si].clone()
    };
    let mut b = if tj == 0 {
        paths[ti].clone()
    } else {
        paths[ti].reversed()
    };
    let joint = a.nodes.len() - 1;
    a.nodes[joint].h_out = None;
    b.nodes[0].h_in = None;
    a.nodes.extend(b.nodes);
    paths[si] = a;
    paths.remove(ti);
    Some((if ti < si { si - 1 } else { si }, joint))
}

/// Une entre sí todos los trazados abiertos: una y otra vez junta los dos extremos libres más
/// cercanos de trazados distintos, hasta que queda uno solo. Los extremos a `fuse_tol` o menos
/// se funden en un nodo; los más separados se conectan con un tramo recto, sin moverlos. Al
/// final, un trazado cuyos dos extremos coinciden (a `fuse_tol`) se cierra. Los trazados
/// cerrados no se tocan. Devuelve cuántas uniones hizo.
pub fn join_open_paths(paths: &mut Vec<NodePath>, fuse_tol: f64) -> usize {
    let ends = |paths: &[NodePath]| -> Vec<(usize, usize)> {
        paths
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.closed && p.nodes.len() >= 2)
            .flat_map(|(k, p)| [(k, 0), (k, p.nodes.len() - 1)])
            .collect()
    };
    let mut joins = 0;
    loop {
        let e = ends(paths);
        let mut best: Option<(NodeRef, NodeRef, f64)> = None;
        for (i, a) in e.iter().enumerate() {
            for b in &e[i + 1..] {
                if a.0 == b.0 {
                    continue;
                }
                let d = paths[a.0].nodes[a.1].p.distance(paths[b.0].nodes[b.1].p);
                if best.is_none_or(|(.., bd)| d < bd) {
                    best = Some((*a, *b, d));
                }
            }
        }
        let Some((a, b, d)) = best else {
            break;
        };
        let done = if d <= fuse_tol {
            join_ends(paths, a, b)
        } else {
            connect_ends(paths, a, b)
        };
        if done.is_none() {
            break;
        }
        joins += 1;
    }
    for si in 0..paths.len() {
        let p = &paths[si];
        if p.closed || p.nodes.len() < 3 {
            continue;
        }
        let last = p.nodes.len() - 1;
        if p.nodes[0].p.distance(p.nodes[last].p) <= fuse_tol
            && join_ends(paths, (si, last), (si, 0)).is_some()
        {
            joins += 1;
        }
    }
    joins
}

/// ¿Está `p` dentro del polígono `poly` (regla par-impar)?
fn point_in_polygon(p: Point, poly: &[Point]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

/// Lado hacia el que sale una pestaña del tramo recto `seg` (unitario): hacia fuera de un
/// trazado cerrado; en uno abierto, hacia el lado contrario al resto del trazado.
fn tab_side(np: &NodePath, seg: usize) -> Option<kurbo::Vec2> {
    let (a, curve, b) = np.segment(seg);
    if curve.is_some() || a.distance(b) < 1e-9 {
        return None;
    }
    let u = (b - a).normalize();
    let n = kurbo::Vec2::new(u.y, -u.x);
    let mid = a.midpoint(b);
    let flat = np.flatten(FLATTEN_TOL_MM);
    let outward = if np.closed {
        // Un punto muy cerca del tramo, del lado de `n`: si cae dentro, la pestaña va al otro.
        let probe = (a.distance(b) * 1e-3).max(1e-4);
        !point_in_polygon(mid + n * probe, &flat)
    } else {
        // Hacia donde no está el centro del resto del trazado.
        let (sx, sy) = flat.iter().fold((0.0, 0.0), |(x, y), q| (x + q.x, y + q.y));
        let center = Point::new(sx / flat.len() as f64, sy / flat.len() as f64);
        (center - mid).dot(n) <= 0.0
    };
    Some(if outward { n } else { -n })
}

/// Añade una pestaña de pegado al tramo recto `seg`: un trapecio de `width` mm de fondo cuyos
/// lados cierran `angle_deg` con el tramo (90° = rectángulo). El tramo pasa a ser tres (los dos
/// lados y el borde de la pestaña). Devuelve el doblez (los dos extremos del tramo original), o
/// `None` si el tramo es curvo o demasiado corto.
pub fn add_glue_tab(
    np: &mut NodePath,
    seg: usize,
    width: f64,
    angle_deg: f64,
) -> Option<(Point, Point)> {
    if seg >= np.segment_count() || width <= 0.0 {
        return None;
    }
    let n = tab_side(np, seg)?;
    let (a, _, b) = np.segment(seg);
    let len = a.distance(b);
    let u = (b - a) / len;
    // Cuánto se mete cada lado: nunca más de un 40 % del tramo, para que quede borde.
    let angle = angle_deg.clamp(20.0, 90.0).to_radians();
    let inset = (width / angle.tan()).min(len * 0.4);
    let (p1, p2) = (a + n * width + u * inset, b + n * width - u * inset);
    let count = np.nodes.len();
    let j = (seg + 1) % count;
    // Los extremos dejan de ser puntos suaves hacia la pestaña.
    np.nodes[seg].h_out = None;
    np.nodes[j].h_in = None;
    np.nodes.insert(seg + 1, Node::line(p1));
    np.nodes.insert(seg + 2, Node::line(p2));
    Some((a, b))
}

/// Alinea las anclas de los nodos `sel` (trazado, nodo) entre sí; los manejadores se mueven
/// con su nodo. Hacen falta al menos dos nodos.
pub fn align_nodes(paths: &mut [NodePath], sel: &[(usize, usize)], mode: crate::arrange::Align) {
    use crate::arrange::Align;
    let pts: Vec<Point> = sel
        .iter()
        .filter_map(|&(si, ni)| paths.get(si)?.nodes.get(ni).map(|n| n.p))
        .collect();
    if pts.len() < 2 {
        return;
    }
    let (x0, x1) = min_max(pts.iter().map(|p| p.x));
    let (y0, y1) = min_max(pts.iter().map(|p| p.y));
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (tx, ty) = match mode {
        Align::Left => (Some(x0), None),
        Align::HCenter => (Some(cx), None),
        Align::Right => (Some(x1), None),
        Align::Top => (None, Some(y0)),
        Align::VCenter => (None, Some(cy)),
        Align::Bottom => (None, Some(y1)),
        Align::Center => (Some(cx), Some(cy)),
    };
    for &(si, ni) in sel {
        if let Some(p) = paths.get(si).and_then(|np| np.nodes.get(ni)).map(|n| n.p) {
            paths[si].move_node(ni, Point::new(tx.unwrap_or(p.x), ty.unwrap_or(p.y)));
        }
    }
}

/// Reparte los nodos `sel` a intervalos iguales en un eje, entre los dos de los extremos.
/// Hacen falta al menos tres nodos.
pub fn distribute_nodes(
    paths: &mut [NodePath],
    sel: &[(usize, usize)],
    axis: crate::arrange::Axis,
) {
    let coord = |p: Point| match axis {
        crate::arrange::Axis::Horizontal => p.x,
        crate::arrange::Axis::Vertical => p.y,
    };
    let mut items: Vec<((usize, usize), Point)> = sel
        .iter()
        .filter_map(|&(si, ni)| Some(((si, ni), paths.get(si)?.nodes.get(ni)?.p)))
        .collect();
    if items.len() < 3 {
        return;
    }
    items.sort_by(|a, b| coord(a.1).total_cmp(&coord(b.1)));
    let (first, last) = (coord(items[0].1), coord(items[items.len() - 1].1));
    let step = (last - first) / (items.len() - 1) as f64;
    for (k, ((si, ni), p)) in items.into_iter().enumerate() {
        let v = first + step * k as f64;
        let to = match axis {
            crate::arrange::Axis::Horizontal => Point::new(v, p.y),
            crate::arrange::Axis::Vertical => Point::new(p.x, v),
        };
        paths[si].move_node(ni, to);
    }
}

fn min_max(v: impl Iterator<Item = f64>) -> (f64, f64) {
    v.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
        (lo.min(x), hi.max(x))
    })
}

/// Tramos (trazado, tramo) cuyos dos nodos están en `sel`.
pub fn selected_segments(paths: &[NodePath], sel: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let has = |si: usize, ni: usize| sel.contains(&(si, ni));
    let mut out = Vec::new();
    for (si, np) in paths.iter().enumerate() {
        let n = np.nodes.len();
        for seg in 0..np.segment_count() {
            if has(si, seg) && has(si, (seg + 1) % n) {
                out.push((si, seg));
            }
        }
    }
    out
}

/// Añade un nodo en el punto medio de cada tramo de `segs` sin cambiar la forma. Devuelve los
/// nodos nuevos (trazado, nodo), con los índices ya definitivos.
pub fn insert_midpoints(paths: &mut [NodePath], segs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut segs: Vec<(usize, usize)> = segs.to_vec();
    segs.sort();
    segs.dedup();
    let mut out = Vec::new();
    // De atrás hacia delante: insertar no mueve los tramos anteriores del mismo trazado.
    for &(si, seg) in segs.iter().rev() {
        let Some(np) = paths.get_mut(si) else {
            continue;
        };
        if seg >= np.segment_count() {
            continue;
        }
        let k = np.insert_at(seg, 0.5);
        for o in out.iter_mut().filter(|o: &&mut (usize, usize)| o.0 == si) {
            o.1 += 1;
        }
        out.push((si, k));
    }
    out.sort();
    out
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

    #[test]
    fn symmetric_handles_keep_the_same_length() {
        let mut s = square();
        s.make_symmetric(1);
        let n = s.nodes[1];
        assert!(n.smooth && n.symmetric);
        let (a, b) = (n.h_in.unwrap(), n.h_out.unwrap());
        assert!((a.distance(n.p) - b.distance(n.p)).abs() < 1e-9);
        s.move_handle(1, true, Point::new(16.0, 0.0));
        let n = s.nodes[1];
        assert!((n.h_in.unwrap().x - 4.0).abs() < 1e-9 && n.h_in.unwrap().y.abs() < 1e-9);
        // Suavizar o hacer vértice quita la simetría.
        s.make_smooth(1);
        assert!(s.nodes[1].smooth && !s.nodes[1].symmetric);
        s.make_symmetric(1);
        s.make_corner(1);
        assert!(!s.nodes[1].smooth && !s.nodes[1].symmetric);
    }

    #[test]
    fn reverse_and_close_keep_the_shape() {
        let e = ellipse(kurbo::Rect::new(0.0, 0.0, 20.0, 10.0));
        let mut r = e.clone();
        r.reverse();
        assert_eq!(r.nodes[0].p, e.nodes[0].p, "conserva el nodo inicial");
        assert_eq!(r.nodes[1].p, e.nodes[3].p, "recorre al revés");
        let (a, b) = (e.flatten(0.01), r.flatten(0.01));
        assert_eq!(a.len(), b.len());
        assert!(b.iter().all(|p| e.nearest_segment(*p).unwrap().2 < 1e-3));
        // Cerrar: con extremos separados añade un tramo recto; si coinciden, los funde.
        let mut open = NodePath::from_points(
            &[
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
            ],
            false,
        );
        open.close();
        assert!(open.closed && open.nodes.len() == 3 && open.segment_count() == 3);
        let mut dup = square().break_at(0).remove(0);
        assert_eq!(dup.nodes.len(), 5);
        dup.close();
        assert!(dup.closed && dup.nodes.len() == 4);
    }

    #[test]
    fn align_and_distribute_nodes() {
        use crate::arrange::{Align, Axis};
        let mut p = vec![NodePath::from_points(
            &[
                Point::new(0.0, 0.0),
                Point::new(3.0, 5.0),
                Point::new(10.0, 9.0),
                Point::new(20.0, 2.0),
            ],
            false,
        )];
        p[0].segment_to_curve(1);
        let h = p[0].nodes[1].h_out.unwrap() - p[0].nodes[1].p;
        let sel = [(0, 0), (0, 1), (0, 2)];
        align_nodes(&mut p, &sel, Align::Bottom);
        assert!(p[0].nodes[..3].iter().all(|n| n.p.y == 9.0));
        assert_eq!(p[0].nodes[3].p.y, 2.0, "los no seleccionados no se mueven");
        let n = p[0].nodes[1];
        assert!(
            (n.h_out.unwrap() - n.p - h).hypot() < 1e-9,
            "el manejador va con su nodo"
        );
        let all = [(0, 0), (0, 1), (0, 2), (0, 3)];
        distribute_nodes(&mut p, &all, Axis::Horizontal);
        let xs: Vec<f64> = p[0].nodes.iter().map(|n| n.p.x).collect();
        for (x, want) in xs.iter().zip([0.0, 20.0 / 3.0, 40.0 / 3.0, 20.0]) {
            assert!((x - want).abs() < 1e-9, "{xs:?}");
        }
        // Con un solo nodo no hay nada que alinear.
        let before = p.clone();
        align_nodes(&mut p, &[(0, 0)], Align::Left);
        assert_eq!(p, before);
    }

    #[test]
    fn midpoints_on_selected_segments() {
        let mut p = vec![square()];
        // Nodos 0, 1 y 2 seleccionados: tramos 0 y 1.
        let segs = selected_segments(&p, &[(0, 0), (0, 1), (0, 2)]);
        assert_eq!(segs, vec![(0, 0), (0, 1)]);
        let new = insert_midpoints(&mut p, &segs);
        assert_eq!(new, vec![(0, 1), (0, 3)]);
        assert_eq!(p[0].nodes.len(), 6);
        assert_eq!(p[0].nodes[1].p, Point::new(5.0, 0.0));
        assert_eq!(p[0].nodes[3].p, Point::new(10.0, 5.0));
        // En un trazado cerrado, el último tramo une el último nodo con el primero.
        let segs = selected_segments(&[square()], &[(0, 3), (0, 0)]);
        assert_eq!(segs, vec![(0, 3)]);
    }

    #[test]
    fn join_open_paths_fuses_close_ends_and_bridges_far_ones() {
        let p = Point::new;
        // Cuatro lados de un cuadrado dibujados por separado y en cualquier sentido.
        let mut sides = vec![
            NodePath::from_points(&[p(0.0, 0.0), p(10.0, 0.0)], false),
            NodePath::from_points(&[p(10.0, 10.0), p(10.0, 0.0)], false),
            NodePath::from_points(&[p(0.0, 10.0), p(10.0, 10.0)], false),
            NodePath::from_points(&[p(0.0, 10.0), p(0.0, 0.0)], false),
        ];
        assert_eq!(join_open_paths(&mut sides, 0.1), 4);
        assert_eq!(sides.len(), 1);
        assert!(sides[0].closed && sides[0].nodes.len() == 4, "{sides:?}");
        // Dos líneas separadas: se conectan con un tramo recto y ningún nodo se mueve.
        let mut far = vec![
            NodePath::from_points(&[p(0.0, 0.0), p(10.0, 0.0)], false),
            NodePath::from_points(&[p(30.0, 5.0), p(14.0, 0.0)], false),
            ellipse(kurbo::Rect::new(50.0, 50.0, 60.0, 60.0)),
        ];
        assert_eq!(join_open_paths(&mut far, 0.1), 1);
        assert_eq!(far.len(), 2, "el trazado cerrado se queda como estaba");
        let open = far.iter().find(|np| !np.closed).unwrap();
        let xs: Vec<f64> = open.nodes.iter().map(|n| n.p.x).collect();
        assert_eq!(xs, vec![0.0, 10.0, 14.0, 30.0]);
        // Con un solo trazado abierto no hay nada que unir.
        assert_eq!(join_open_paths(&mut far, 0.1), 0);
    }

    #[test]
    fn fillet_rounds_a_right_angle_with_a_tangent_arc() {
        let s = square();
        // Una esquina de 90° con radio 2: los puntos de tangencia quedan a 2 del vértice.
        let (f, new) = fillet_corners(&s, &[1], 2.0);
        assert_eq!(new, vec![1, 2]);
        assert_eq!(f.nodes.len(), 5);
        assert!(f.nodes[1].p.distance(Point::new(8.0, 0.0)) < 1e-9);
        assert!(f.nodes[2].p.distance(Point::new(10.0, 2.0)) < 1e-9);
        // El arco es (casi) un cuarto de círculo de centro (8, 2) y radio 2.
        let arc = f.segment(1).1.expect("curva");
        for t in [0.25, 0.5, 0.75] {
            let r = arc.eval(t).distance(Point::new(8.0, 2.0));
            assert!((r - 2.0).abs() < 0.002, "radio {r}");
        }
        // Los tramos vecinos siguen rectos y el resto del cuadrado no se toca.
        assert!(f.segment(0).1.is_none() && f.segment(2).1.is_none());
        assert_eq!(f.nodes[0].p, s.nodes[0].p);
        assert_eq!(f.nodes[4].p, s.nodes[3].p);
        // Radio 0 o un nodo que no es esquina: sin cambios.
        assert_eq!(fillet_corners(&s, &[1], 0.0).0, s);
        let e = ellipse(kurbo::Rect::new(0.0, 0.0, 20.0, 10.0));
        assert!(!is_corner(&e, 0));
        assert_eq!(fillet_corners(&e, &[0, 1], 3.0).0, e);
        let open = NodePath::from_points(
            &[
                Point::new(0.0, 0.0),
                Point::new(5.0, 0.0),
                Point::new(5.0, 5.0),
            ],
            false,
        );
        assert!(!is_corner(&open, 0) && is_corner(&open, 1) && !is_corner(&open, 2));
    }

    #[test]
    fn fillet_all_corners_and_clamp_to_the_room() {
        let s = square();
        // Las cuatro esquinas a la vez, también la que cierra el trazado.
        let (f, new) = fillet_corners(&s, &[0, 1, 2, 3], 3.0);
        assert_eq!(f.nodes.len(), 8);
        assert_eq!(new, (0..8).collect::<Vec<_>>());
        assert!(f.closed);
        let (lo, hi) = f
            .flatten(0.01)
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.x.min(p.y)), hi.max(p.x.max(p.y)))
            });
        assert!(lo > -1e-9 && hi < 10.0 + 1e-9, "no se sale del cuadrado");
        // Un radio enorme se queda en el que cabe: medio lado por esquina (un círculo).
        let (f, _) = fillet_corners(&s, &[0, 1, 2, 3], 100.0);
        assert!(
            f.nodes
                .iter()
                .all(|n| n.p.x.is_finite() && n.p.y.is_finite())
        );
        for p in f.flatten(0.01) {
            let r = p.distance(Point::new(5.0, 5.0));
            assert!((r - 5.0).abs() < 0.01, "radio {r}");
        }
    }

    #[test]
    fn fillet_between_a_line_and_a_curve() {
        // Recta que llega a (10, 0) y curva que sale hacia arriba.
        let mut p = NodePath::from_points(
            &[
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(14.0, 12.0),
            ],
            false,
        );
        p.nodes[1].h_out = Some(Point::new(10.0, 4.0));
        p.nodes[2].h_in = Some(Point::new(12.0, 9.0));
        assert!(is_corner(&p, 1));
        let (f, new) = fillet_corners(&p, &[1], 1.5);
        assert_eq!(new, vec![1, 2]);
        assert_eq!(f.nodes.len(), 4);
        // El primer punto de tangencia está sobre la recta; el segundo, sobre la curva original.
        assert!(f.nodes[1].p.y.abs() < 1e-9 && f.nodes[1].p.x < 10.0);
        assert!(p.nearest_segment(f.nodes[2].p).unwrap().2 < 1e-6);
        // El final de la curva original se conserva.
        assert_eq!(f.nodes[3].p, p.nodes[2].p);
        let tail = f.segment(2).1.expect("curva");
        assert!(p.nearest_segment(tail.eval(0.5)).unwrap().2 < 1e-3);
    }

    #[test]
    fn stored_radius_is_resolved_into_arcs() {
        let mut s = square();
        assert!(!s.has_rounding());
        assert_eq!(s.resolved(), s, "sin radios, el trazado es el mismo");
        s.nodes[1].radius = 2.0;
        s.nodes[3].radius = 3.0;
        assert!(s.has_rounding());
        let r = s.resolved();
        // Los nodos siguen siendo cuatro; lo que se dibuja tiene seis, sin radios.
        assert_eq!(s.nodes.len(), 4);
        assert_eq!(r.nodes.len(), 6);
        assert!(r.nodes.iter().all(|n| n.radius == 0.0));
        assert!(
            r.nodes
                .iter()
                .any(|n| n.p.distance(Point::new(8.0, 0.0)) < 1e-9)
        );
        assert!(
            r.nodes
                .iter()
                .any(|n| n.p.distance(Point::new(0.0, 7.0)) < 1e-9)
        );
        // Cada esquina con su radio.
        let (each, _) = fillet_corners_each(&square(), &[(1, 2.0), (3, 3.0)]);
        assert_eq!(each, r);
        // Un radio en un nodo que no es esquina no hace nada.
        let mut e = ellipse(kurbo::Rect::new(0.0, 0.0, 20.0, 10.0));
        e.nodes[0].radius = 4.0;
        assert_eq!(e.resolved().nodes.len(), 4);
        // El radio viaja con su nodo al invertir o insertar.
        let mut t = square();
        t.nodes[1].radius = 2.0;
        t.insert_at(0, 0.5);
        assert_eq!(t.nodes[2].radius, 2.0);
        t.reverse();
        assert_eq!(t.nodes.iter().filter(|n| n.radius == 2.0).count(), 1);
    }

    #[test]
    fn glue_tab_goes_outside_and_keeps_the_fold() {
        // Cuadrado de 10: pestaña de 3 mm a 45° en el lado de arriba (y = 0).
        let mut s = square();
        let fold = add_glue_tab(&mut s, 0, 3.0, 45.0).unwrap();
        assert_eq!(fold, (Point::new(0.0, 0.0), Point::new(10.0, 0.0)));
        assert_eq!(s.nodes.len(), 6);
        // Sale hacia fuera (y negativa) y se estrecha 3 mm por cada lado.
        assert!(
            s.nodes[1].p.distance(Point::new(3.0, -3.0)) < 1e-9,
            "{:?}",
            s.nodes[1]
        );
        assert!(s.nodes[2].p.distance(Point::new(7.0, -3.0)) < 1e-9);
        // Da igual el sentido del trazado: sigue saliendo hacia fuera.
        let mut r = square();
        r.reverse();
        let seg = (0..4)
            .find(|i| {
                let (a, _, b) = r.segment(*i);
                a.y == 0.0 && b.y == 0.0
            })
            .unwrap();
        add_glue_tab(&mut r, seg, 3.0, 45.0).unwrap();
        assert!(r.nodes.iter().any(|n| n.p.y < -2.9));
        assert!(r.nodes.iter().all(|n| n.p.y <= 10.0 + 1e-9));
        // A 90° es un rectángulo; en un tramo corto los lados no se cruzan.
        let mut q = square();
        add_glue_tab(&mut q, 1, 2.0, 90.0).unwrap();
        assert!(q.nodes[2].p.distance(Point::new(12.0, 0.0)) < 1e-9);
        let mut t = square();
        add_glue_tab(&mut t, 0, 30.0, 30.0).unwrap();
        assert!(
            t.nodes[1].p.x < t.nodes[2].p.x,
            "queda borde: {:?}",
            &t.nodes[1..3]
        );
        // En el último tramo de un trazado cerrado (del último nodo al primero).
        let mut c = square();
        add_glue_tab(&mut c, 3, 3.0, 45.0).unwrap();
        assert_eq!(c.nodes.len(), 6);
        assert!(c.nodes[4].p.x < -2.9 && c.nodes[5].p.x < -2.9);
        // Un tramo curvo no admite pestaña.
        let mut e = ellipse(kurbo::Rect::new(0.0, 0.0, 20.0, 10.0));
        assert!(add_glue_tab(&mut e, 0, 3.0, 45.0).is_none());
        // Trazado abierto: la pestaña sale hacia el lado contrario al resto.
        let mut o = NodePath::from_points(
            &[
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
            ],
            false,
        );
        add_glue_tab(&mut o, 0, 2.0, 60.0).unwrap();
        assert!(o.nodes[1].p.y < 0.0);
    }

    #[test]
    fn join_two_chosen_ends() {
        let a = NodePath::from_points(&[Point::new(0.0, 0.0), Point::new(4.0, 0.0)], false);
        let b = NodePath::from_points(&[Point::new(5.0, 0.0), Point::new(9.0, 0.0)], false);
        let c = NodePath::from_points(&[Point::new(30.0, 0.0), Point::new(40.0, 0.0)], false);
        // El más cercano a (4, 0) es `b`, pero se pide unir con `c`.
        let mut p = vec![a, b, c];
        let sel = join_ends(&mut p, (0, 1), (2, 0)).unwrap();
        assert_eq!(p.len(), 2);
        let xs: Vec<f64> = p[sel.0].nodes.iter().map(|n| n.p.x).collect();
        assert_eq!(xs, vec![0.0, 17.0, 40.0]);
        // Un nodo intermedio o el mismo extremo no se pueden unir.
        assert!(join_ends(&mut p, (0, 1), (1, 0)).is_none());
        assert!(join_ends(&mut p, (1, 0), (1, 0)).is_none());
    }
}
