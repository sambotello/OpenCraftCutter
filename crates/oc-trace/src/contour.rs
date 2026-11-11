//! Trazado propio en blanco y negro.
//!
//! 1. Isolíneas del umbral con *marching squares* sobre la luminancia: el borde sale con
//!    precisión subpíxel en imágenes con antialias, sin la escalera de los píxeles.
//! 2. Detección de esquinas por ángulo de giro en una ventana de longitud de arco.
//! 3. Entre esquinas: recta si cabe en la tolerancia; si no, ajuste de cúbicas por mínimos
//!    cuadrados con subdivisión (curvas suaves, esquinas vivas, rectas rectas).

use kurbo::{BezPath, CubicBez, ParamCurve, ParamCurveDeriv, Point, Vec2};
use std::collections::HashMap;

/// Valor de «papel» fuera de la imagen (nunca es tinta: el umbral es ≤ 255).
pub(crate) const PAPER: f32 = 256.0;

/// Ajustes del ajuste de curvas, en píxeles de la imagen que se traza.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FitParams {
    /// Ángulo de giro (grados) a partir del cual un vértice es esquina.
    pub corner_deg: f64,
    /// Ventana de longitud de arco para medir el giro.
    pub corner_span: f64,
    /// Desviación máxima admitida entre el contorno y su ajuste.
    pub tolerance: f64,
    /// `false` = solo segmentos rectos (polígono).
    pub smooth: bool,
}

/// Isolíneas cerradas donde `value < t` es tinta. `value` es w×h por filas; los puntos
/// están en coordenadas de imagen (el centro del píxel (x, y) es (x + 0.5, y + 0.5)).
pub(crate) fn iso_loops(value: &[f32], w: usize, h: usize, t: f32) -> Vec<Vec<Point>> {
    let gw = w + 2;
    let gh = h + 2;
    let v = |x: usize, y: usize| -> f32 {
        if x == 0 || y == 0 || x > w || y > h {
            PAPER
        } else {
            value[(y - 1) * w + (x - 1)]
        }
    };
    let ink = |x: usize, y: usize| v(x, y) < t;
    let hid = |x: usize, y: usize| (2 * (y * gw + x)) as u32;
    let vid = |x: usize, y: usize| (2 * (y * gw + x) + 1) as u32;

    let mut segs: Vec<(u32, u32)> = Vec::new();
    for y in 0..gh - 1 {
        for x in 0..gw - 1 {
            let (a, b, c, d) = (ink(x, y), ink(x + 1, y), ink(x + 1, y + 1), ink(x, y + 1));
            let code = a as u8 | (b as u8) << 1 | (c as u8) << 2 | (d as u8) << 3;
            if code == 0 || code == 15 {
                continue;
            }
            let (top, right, bottom, left) = (hid(x, y), vid(x + 1, y), hid(x, y + 1), vid(x, y));
            if code == 5 || code == 10 {
                let center = (v(x, y) + v(x + 1, y) + v(x + 1, y + 1) + v(x, y + 1)) / 4.0 < t;
                // code 5: a y c son tinta; code 10: b y d.
                if (code == 5) == center {
                    segs.push((top, right));
                    segs.push((bottom, left));
                } else {
                    segs.push((left, top));
                    segs.push((right, bottom));
                }
                continue;
            }
            let mut e = [0u32; 2];
            let mut n = 0;
            for (edge, p, q) in [(top, a, b), (right, b, c), (bottom, d, c), (left, a, d)] {
                if p != q && n < 2 {
                    e[n] = edge;
                    n += 1;
                }
            }
            segs.push((e[0], e[1]));
        }
    }

    // Posición del cruce del umbral en una arista de la rejilla.
    let point = |id: u32| -> Point {
        let k = (id / 2) as usize;
        let (x, y) = (k % gw, k / gw);
        let (x2, y2) = if id % 2 == 0 { (x + 1, y) } else { (x, y + 1) };
        let (va, vb) = (v(x, y), v(x2, y2));
        let s = if (vb - va).abs() < 1e-6 {
            0.5
        } else {
            ((t - va) / (vb - va)).clamp(0.0, 1.0) as f64
        };
        let (ax, ay) = (x as f64 - 0.5, y as f64 - 0.5);
        Point::new(
            ax + s * (x2 as f64 - x as f64),
            ay + s * (y2 as f64 - y as f64),
        )
    };

    let mut by_edge: HashMap<u32, [u32; 2]> = HashMap::with_capacity(segs.len() * 2);
    for (i, &(p, q)) in segs.iter().enumerate() {
        for e in [p, q] {
            by_edge
                .entry(e)
                .and_modify(|s| s[1] = i as u32)
                .or_insert([i as u32, u32::MAX]);
        }
    }
    let mut visited = vec![false; segs.len()];
    let mut loops = Vec::new();
    for start in 0..segs.len() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let start_edge = segs[start].0;
        let mut pts = vec![point(start_edge)];
        let (mut cur, mut edge) = (start as u32, segs[start].1);
        while edge != start_edge {
            pts.push(point(edge));
            let pair = by_edge[&edge];
            let next = if pair[0] == cur { pair[1] } else { pair[0] };
            if next == u32::MAX || visited[next as usize] {
                break;
            }
            visited[next as usize] = true;
            let (p, q) = segs[next as usize];
            edge = if p == edge { q } else { p };
            cur = next;
        }
        if pts.len() >= 3 {
            loops.push(pts);
        }
    }
    loops
}

/// Área con signo (fórmula del cordón).
pub(crate) fn signed_area(pts: &[Point]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

/// Ajusta un contorno cerrado y devuelve el trazado (en las mismas coordenadas).
pub(crate) fn fit_loop(pts: &[Point], p: &FitParams) -> BezPath {
    let pts = dedup(pts);
    let n = pts.len();
    let mut out = BezPath::new();
    if n < 3 {
        return out;
    }
    let corners = find_corners(&pts, p);
    let corner_at: HashMap<usize, Point> = corners
        .iter()
        .map(|&c| (c, sharpen_corner(&pts, c, p.corner_span)))
        .collect();
    let zone = p.corner_span * 0.6;
    // Tramos entre esquinas (cada uno incluye sus dos extremos).
    let runs: Vec<(Vec<Point>, bool)> = if corners.is_empty() {
        let mut run = pts.clone();
        run.push(pts[0]);
        vec![(run, true)]
    } else {
        let mut runs = Vec::new();
        for (ci, &c) in corners.iter().enumerate() {
            let next = corners[(ci + 1) % corners.len()];
            let (a, b) = (corner_at[&c], corner_at[&next]);
            let mut run = vec![a];
            let mut i = (c + 1) % n;
            while i != next {
                // Fuera la zona redondeada alrededor de cada esquina.
                if pts[i].distance(a) > zone && pts[i].distance(b) > zone {
                    run.push(pts[i]);
                }
                i = (i + 1) % n;
            }
            run.push(b);
            runs.push((run, false));
        }
        runs
    };
    out.move_to(runs[0].0[0]);
    for (run, cyclic) in runs {
        let run = if p.smooth {
            smooth_run(&run, cyclic)
        } else {
            run
        };
        emit_run(&run, cyclic, p, &mut out);
    }
    out.close_path();
    out
}

/// Ajusta una polilínea abierta (de extremo a extremo).
pub(crate) fn fit_open(pts: &[Point], p: &FitParams) -> BezPath {
    let mut pts = pts.to_vec();
    pts.dedup_by(|a, b| a.distance(*b) <= 1e-9);
    let mut out = BezPath::new();
    if pts.len() < 2 {
        return out;
    }
    out.move_to(pts[0]);
    let run = if p.smooth {
        smooth_run(&pts, false)
    } else {
        pts
    };
    emit_run(&run, false, p, &mut out);
    out
}

fn dedup(pts: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    for &q in pts {
        if out.last().is_none_or(|l| l.distance(q) > 1e-9) {
            out.push(q);
        }
    }
    while out.len() > 1 && out[0].distance(*out.last().unwrap()) <= 1e-9 {
        out.pop();
    }
    out
}

/// Índice del punto a distancia de arco ≥ `d` hacia delante (`dir = 1`) o atrás (`-1`).
fn walk(pts: &[Point], i: usize, d: f64, dir: isize) -> usize {
    let n = pts.len() as isize;
    let mut j = i as isize;
    let mut acc = 0.0;
    for _ in 0..n / 2 {
        let k = (j + dir).rem_euclid(n);
        acc += pts[j as usize].distance(pts[k as usize]);
        j = k;
        if acc >= d {
            break;
        }
    }
    j as usize
}

fn turn_deg(a: Point, b: Point, c: Point) -> f64 {
    let (u, v) = (b - a, c - b);
    v.cross(u).atan2(u.dot(v)).abs().to_degrees()
}

/// Esquinas: vértices cuyo giro en la ventana supera el umbral y es máximo local.
fn find_corners(pts: &[Point], p: &FitParams) -> Vec<usize> {
    let n = pts.len();
    if n < 6 {
        return (0..n).collect();
    }
    let angle: Vec<f64> = (0..n)
        .map(|i| {
            let b = walk(pts, i, p.corner_span, -1);
            let f = walk(pts, i, p.corner_span, 1);
            turn_deg(pts[b], pts[i], pts[f])
        })
        .collect();
    let mut corners = Vec::new();
    for i in 0..n {
        if angle[i] <= p.corner_deg {
            continue;
        }
        // Máximo local dentro de la ventana (empates: el primer índice).
        let mut is_max = true;
        for dir in [-1isize, 1] {
            let mut j = i;
            let mut acc = 0.0;
            for _ in 0..n / 2 {
                let k = (j as isize + dir).rem_euclid(n as isize) as usize;
                acc += pts[j].distance(pts[k]);
                j = k;
                if acc > p.corner_span {
                    break;
                }
                if angle[j] > angle[i] || (angle[j] == angle[i] && j < i) {
                    is_max = false;
                    break;
                }
            }
        }
        if is_max {
            corners.push(i);
        }
    }
    corners
}

/// Recta que mejor ajusta (mínimos cuadrados) un conjunto de puntos: (punto, dirección).
fn fit_line(q: &[Point]) -> Option<(Point, Vec2)> {
    if q.len() < 2 {
        return None;
    }
    let n = q.len() as f64;
    let c = q.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / n;
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for p in q {
        let d = p.to_vec2() - c;
        sxx += d.x * d.x;
        sxy += d.x * d.y;
        syy += d.y * d.y;
    }
    let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    Some((c.to_point(), Vec2::new(theta.cos(), theta.sin())))
}

/// Lleva la esquina al cruce de las rectas de sus dos lados (deshace el redondeo del
/// antialias y el chaflán de la rejilla). Si los lados no son rectos, la deja igual.
fn sharpen_corner(pts: &[Point], c: usize, span: f64) -> Point {
    let n = pts.len();
    let side = |dir: isize| -> Vec<Point> {
        let near = walk(pts, c, span * 0.6, dir);
        let far = walk(pts, c, span * 3.0, dir);
        let mut out = Vec::new();
        let mut i = near;
        loop {
            out.push(pts[i]);
            if i == far {
                break;
            }
            i = (i as isize + dir).rem_euclid(n as isize) as usize;
        }
        out
    };
    let (back, fwd) = (side(-1), side(1));
    let (Some((p1, d1)), Some((p2, d2))) = (fit_line(&back), fit_line(&fwd)) else {
        return pts[c];
    };
    // Los lados deben ser rectos de verdad.
    let straight = |q: &[Point], p: Point, d: Vec2| {
        q.iter()
            .all(|x| (*x - p).cross(d).abs() <= 0.35 * span.max(1.0))
    };
    if !straight(&back, p1, d1) || !straight(&fwd, p2, d2) {
        return pts[c];
    }
    let den = d1.cross(d2);
    if den.abs() < 1e-6 {
        return pts[c];
    }
    let t = (p2 - p1).cross(d2) / den;
    let x = p1 + d1 * t;
    if x.distance(pts[c]) <= span {
        x
    } else {
        pts[c]
    }
}

/// Suavizado laplaciano ligero (extremos fijos salvo en contornos sin esquinas).
fn smooth_run(run: &[Point], cyclic: bool) -> Vec<Point> {
    let mut cur = run.to_vec();
    let m = cur.len();
    if m < 3 {
        return cur;
    }
    for _ in 0..2 {
        let prev = cur.clone();
        if cyclic {
            // run[0] == run[m-1]: tratamos m-1 puntos cíclicos.
            let k = m - 1;
            for i in 0..k {
                let a = prev[(i + k - 1) % k];
                let c = prev[(i + 1) % k];
                cur[i] = ((a.to_vec2() + prev[i].to_vec2() * 2.0 + c.to_vec2()) / 4.0).to_point();
            }
            cur[m - 1] = cur[0];
        } else {
            for i in 1..m - 1 {
                cur[i] =
                    ((prev[i - 1].to_vec2() + prev[i].to_vec2() * 2.0 + prev[i + 1].to_vec2())
                        / 4.0)
                        .to_point();
            }
        }
    }
    cur
}

fn dist_to_segment(q: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.hypot2();
    if len2 < 1e-12 {
        return q.distance(a);
    }
    let t = ((q - a).dot(ab) / len2).clamp(0.0, 1.0);
    q.distance(a + ab * t)
}

/// Dirección media desde `q[0]` hacia los puntos siguientes (en una ventana de arco).
fn end_tangent(q: &[Point], span: f64) -> Vec2 {
    let mut acc = 0.0;
    let mut j = 1;
    while j < q.len() - 1 {
        acc += q[j - 1].distance(q[j]);
        if acc >= span {
            break;
        }
        j += 1;
    }
    let v = q[j] - q[0];
    if v.hypot() < 1e-9 {
        Vec2::new(1.0, 0.0)
    } else {
        v.normalize()
    }
}

/// Tramos rectos largos dentro de un tramo: (inicio, fin) en índices.
/// Una recta debe medir al menos `min_len`, caber en `tol` y no estar «combada»: la flecha
/// estimada con un ajuste parabólico de las desviaciones (`bow`) distingue un arco suave de
/// una recta con ruido.
fn line_pieces(run: &[Point], tol: f64, bow: f64, min_len: f64) -> Vec<(usize, usize)> {
    let m = run.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < m {
        let mut best = None;
        let mut j = i + 1;
        while j < m {
            let (a, b) = (run[i], run[j]);
            let d = b - a;
            let len = d.hypot();
            if len < 1e-9 {
                j += 1;
                continue;
            }
            let nrm = d / len;
            let (mut ok, mut num, mut den) = (true, 0.0, 0.0);
            for q in &run[i + 1..j] {
                let rel = *q - a;
                let off = rel.cross(nrm);
                if off.abs() > tol {
                    ok = false;
                    break;
                }
                // Forma de un arco sobre la cuerda: 0 en los extremos, 1 en el centro.
                let t = (rel.dot(nrm) / len).clamp(0.0, 1.0);
                let wgt = 4.0 * t * (1.0 - t);
                num += off * wgt;
                den += wgt * wgt;
            }
            if !ok {
                break;
            }
            let sagitta = if den > 0.0 { (num / den).abs() } else { 0.0 };
            if len >= min_len && sagitta <= bow {
                best = Some(j);
            }
            j += 1;
        }
        match best {
            Some(b) => {
                out.push((i, b));
                i = b;
            }
            None => i += 1,
        }
    }
    out
}

fn emit_run(run: &[Point], cyclic: bool, p: &FitParams, out: &mut BezPath) {
    let m = run.len();
    if m < 2 {
        return;
    }
    if !p.smooth {
        for q in douglas_peucker(run, p.tolerance).into_iter().skip(1) {
            out.line_to(q);
        }
        return;
    }
    let span = p.corner_span;
    // Escala de la imagen respecto al original (corner_span = 1.5 px originales).
    let k = span / 1.5;
    let line_tol = p.tolerance.max(0.6 * k);
    let (bow, min_len) = (0.05 * k, 20.0 * k);
    let mut lines = line_pieces(run, line_tol, bow, min_len);
    // Recta si todo el tramo entre esquinas cabe en la tolerancia.
    if !cyclic
        && lines.is_empty()
        && run
            .iter()
            .all(|&q| dist_to_segment(q, run[0], run[m - 1]) <= p.tolerance)
    {
        out.line_to(run[m - 1]);
        return;
    }
    if cyclic && lines.is_empty() {
        // Contorno sin esquinas ni rectas: tangente central en el punto de unión.
        let n = m - 1;
        let fwd = end_tangent(run, span);
        let rev: Vec<Point> = (0..n).map(|i| run[(n - i) % n]).chain([run[0]]).collect();
        let back = end_tangent(&rev, span);
        let tc = (fwd - back).normalize();
        fit_cubic(run, tc, -tc, p.tolerance, span, out, 0);
        return;
    }
    // En un contorno cíclico con rectas, empezamos en la primera recta (el trazado ya
    // hizo move_to en run[0], así que solo rotamos si no lo hemos emitido aún).
    let rotated: Vec<Point>;
    let run: &[Point] = if cyclic {
        let s0 = lines[0].0;
        let n = m - 1;
        rotated = (0..=n).map(|i| run[(s0 + i) % n]).collect();
        if let Some(kurbo::PathEl::MoveTo(_)) = out.elements().last() {
            let n = out.elements().len();
            out.truncate(n - 1);
            out.move_to(rotated[0]);
        }
        lines = line_pieces(&rotated, line_tol, bow, min_len);
        &rotated
    } else {
        run
    };
    let m = run.len();
    // Piezas alternas: rectas y curvas entre ellas.
    let mut pieces: Vec<(usize, usize, bool)> = Vec::new();
    let mut at = 0;
    for &(a, b) in &lines {
        if a > at {
            pieces.push((at, a, false));
        }
        pieces.push((a, b, true));
        at = b;
    }
    if at < m - 1 {
        pieces.push((at, m - 1, false));
    }
    let dir = |(a, b, _): (usize, usize, bool)| (run[b] - run[a]).normalize();
    let np = pieces.len();
    for (idx, &piece) in pieces.iter().enumerate() {
        let (a, b, is_line) = piece;
        if is_line {
            out.line_to(run[b]);
            continue;
        }
        let q = &run[a..=b];
        if q.len() < 2 {
            continue;
        }
        let prev = if idx > 0 {
            Some(pieces[idx - 1])
        } else if cyclic {
            Some(pieces[np - 1])
        } else {
            None
        };
        let next = if idx + 1 < np {
            Some(pieces[idx + 1])
        } else if cyclic {
            Some(pieces[0])
        } else {
            None
        };
        let t1 = match prev {
            Some(pc) if pc.2 => dir(pc),
            _ => end_tangent(q, span),
        };
        let t2 = match next {
            Some(pc) if pc.2 => -dir(pc),
            _ => {
                let rev: Vec<Point> = q.iter().rev().copied().collect();
                end_tangent(&rev, span)
            }
        };
        fit_cubic(q, t1, t2, p.tolerance, span, out, 0);
    }
}

fn douglas_peucker(pts: &[Point], tol: f64) -> Vec<Point> {
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (mut best, mut idx) = (0.0, 0);
        for i in a + 1..b {
            let d = dist_to_segment(pts[i], pts[a], pts[b]);
            if d > best {
                best = d;
                idx = i;
            }
        }
        if best > tol {
            keep[idx] = true;
            stack.push((a, idx));
            stack.push((idx, b));
        }
    }
    pts.iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

fn chord_params(q: &[Point]) -> Vec<f64> {
    let mut u = vec![0.0; q.len()];
    for i in 1..q.len() {
        u[i] = u[i - 1] + q[i - 1].distance(q[i]);
    }
    let total = *u.last().unwrap();
    if total > 0.0 {
        for v in &mut u {
            *v /= total;
        }
    }
    u
}

/// Cúbica por mínimos cuadrados con tangentes fijas en los extremos.
fn least_squares(q: &[Point], u: &[f64], t1: Vec2, t2: Vec2) -> CubicBez {
    let (p0, p3) = (q[0], q[q.len() - 1]);
    let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (qi, &t) in q.iter().zip(u) {
        let mt = 1.0 - t;
        let (b0, b1, b2, b3) = (mt * mt * mt, 3.0 * t * mt * mt, 3.0 * t * t * mt, t * t * t);
        let (a0, a1) = (t1 * b1, t2 * b2);
        c00 += a0.dot(a0);
        c01 += a0.dot(a1);
        c11 += a1.dot(a1);
        let tmp = qi.to_vec2() - (p0.to_vec2() * (b0 + b1) + p3.to_vec2() * (b2 + b3));
        x0 += a0.dot(tmp);
        x1 += a1.dot(tmp);
    }
    let det = c00 * c11 - c01 * c01;
    let seg = p0.distance(p3);
    let (mut al, mut ar) = if det.abs() > 1e-12 {
        ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det)
    } else {
        (0.0, 0.0)
    };
    let eps = 1e-6 * seg.max(1e-6);
    if al < eps || ar < eps || al > 3.0 * seg || ar > 3.0 * seg {
        al = seg / 3.0;
        ar = seg / 3.0;
    }
    CubicBez::new(p0, p0 + t1 * al, p3 + t2 * ar, p3)
}

fn max_error(q: &[Point], u: &[f64], bez: &CubicBez) -> (f64, usize) {
    let (mut best, mut idx) = (0.0, q.len() / 2);
    for i in 1..q.len() - 1 {
        let d = bez.eval(u[i]).distance(q[i]);
        if d > best {
            best = d;
            idx = i;
        }
    }
    (best, idx)
}

/// Una iteración de Newton para acercar cada parámetro a su punto más cercano.
fn reparametrize(q: &[Point], u: &[f64], bez: &CubicBez) -> Vec<f64> {
    let d1 = bez.deriv();
    let d2 = d1.deriv();
    q.iter()
        .zip(u)
        .map(|(qi, &t)| {
            let diff = bez.eval(t) - *qi;
            let (p1, p2) = (d1.eval(t).to_vec2(), d2.eval(t).to_vec2());
            let den = p1.dot(p1) + diff.dot(p2);
            if den.abs() < 1e-12 {
                t
            } else {
                (t - diff.dot(p1) / den).clamp(0.0, 1.0)
            }
        })
        .collect()
}

fn fit_cubic(
    q: &[Point],
    t1: Vec2,
    t2: Vec2,
    tol: f64,
    span: f64,
    out: &mut BezPath,
    depth: usize,
) {
    let m = q.len();
    if m <= 2 || depth > 32 {
        for &p in &q[1..] {
            out.line_to(p);
        }
        return;
    }
    let mut u = chord_params(q);
    let mut bez = least_squares(q, &u, t1, t2);
    let (mut err, mut split) = max_error(q, &u, &bez);
    if err <= tol {
        out.curve_to(bez.p1, bez.p2, bez.p3);
        return;
    }
    if err <= tol * 4.0 {
        for _ in 0..4 {
            u = reparametrize(q, &u, &bez);
            bez = least_squares(q, &u, t1, t2);
            (err, split) = max_error(q, &u, &bez);
            if err <= tol {
                out.curve_to(bez.p1, bez.p2, bez.p3);
                return;
            }
        }
    }
    let split = split.clamp(1, m - 2);
    // Tangente central en el punto de corte, medida en una ventana para filtrar ruido.
    let back: Vec<Point> = q[..=split].iter().rev().copied().collect();
    let fwd = end_tangent(&q[split..], span * 2.0);
    let bwd = end_tangent(&back, span * 2.0);
    let tc = (fwd - bwd).normalize();
    let tc = if tc.is_finite() { tc } else { fwd };
    fit_cubic(&q[..=split], t1, -tc, tol, span, out, depth + 1);
    fit_cubic(&q[split..], tc, t2, tol, span, out, depth + 1);
}

/// Profundidad de anidamiento de cada contorno (0 = exterior, 1 = agujero, …).
pub(crate) fn nesting(loops: &[Vec<Point>]) -> Vec<usize> {
    let boxes: Vec<kurbo::Rect> = loops
        .iter()
        .map(|l| {
            l.iter()
                .fold(kurbo::Rect::from_points(l[0], l[0]), |r, p| r.union_pt(*p))
        })
        .collect();
    (0..loops.len())
        .map(|i| {
            let probe = loops[i][0];
            (0..loops.len())
                .filter(|&j| {
                    j != i
                        && boxes[j].contains_rect(boxes[i])
                        && oc_core::edit::point_in_polygon(probe, &loops[j])
                })
                .count()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_field(w: usize, h: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> Vec<f32> {
        let mut v = vec![255.0; w * h];
        for y in y0..y1 {
            for x in x0..x1 {
                v[y * w + x] = 0.0;
            }
        }
        v
    }

    #[test]
    fn square_traces_to_four_straight_sides() {
        let f = square_field(40, 40, 10, 10, 30, 30);
        let loops = iso_loops(&f, 40, 40, 128.0);
        assert_eq!(loops.len(), 1);
        let params = FitParams {
            corner_deg: 60.0,
            corner_span: 1.5,
            tolerance: 0.35,
            smooth: true,
        };
        let path = fit_loop(&loops[0], &params);
        let lines = path
            .elements()
            .iter()
            .filter(|e| matches!(e, kurbo::PathEl::LineTo(_)))
            .count();
        let curves = path
            .elements()
            .iter()
            .filter(|e| matches!(e, kurbo::PathEl::CurveTo(..)))
            .count();
        assert_eq!((lines, curves), (4, 0), "{path:?}");
    }

    #[test]
    fn hole_has_depth_one() {
        let mut f = square_field(40, 40, 5, 5, 35, 35);
        for y in 15..25 {
            for x in 15..25 {
                f[y * 40 + x] = 255.0;
            }
        }
        let loops = iso_loops(&f, 40, 40, 128.0);
        assert_eq!(loops.len(), 2);
        let mut depth = nesting(&loops);
        depth.sort();
        assert_eq!(depth, vec![0, 1]);
    }
}
