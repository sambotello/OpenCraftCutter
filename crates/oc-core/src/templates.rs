//! Plantillas a medida: cajas, sobres, tarjetas y etiquetas con sus líneas de corte y de
//! doblez, en mm y con la esquina superior izquierda en (0, 0).

use crate::send::Polyline;
use kurbo::Point;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Template {
    /// Caja abierta (bandeja): base `w`×`d`, paredes de `h` y pestañas para pegar.
    Tray { w: f64, d: f64, h: f64 },
    /// Sobre para una tarjeta de `w`×`h`.
    Envelope { w: f64, h: f64 },
    /// Tarjeta de `w`×`h` cerrada, doblada por la mitad (abierta mide 2·`w`).
    FoldedCard { w: f64, h: f64 },
    /// Etiqueta colgante de `w`×`h` con esquinas cortadas y agujero.
    Tag { w: f64, h: f64 },
    /// Caja con tapa: la base de `w`×`d`×`h` y una tapa que encaja por fuera.
    LidBox { w: f64, d: f64, h: f64 },
    /// Caja almohada de `w` de ancho y `h` de largo; `d` es cuánto se curvan los extremos.
    Pillow { w: f64, h: f64, d: f64 },
    /// Cono de `d` de diámetro en la base y `h` de alto.
    Cone { d: f64, h: f64 },
    /// Bolsa de regalo de `w` de ancho, `h` de alto y `d` de fondo.
    Bag { w: f64, h: f64, d: f64 },
    /// Marcapáginas de `w`×`h` con esquinas redondeadas y agujero para la cinta.
    Bookmark { w: f64, h: f64 },
    /// Tarjeta de `w`×`h` cerrada con un escalón que se levanta al abrirla; `s` es cuánto
    /// sobresale el escalón.
    PopUp { w: f64, h: f64, s: f64 },
}

impl Template {
    pub const NAMES: [&'static str; 10] = [
        "Caja abierta",
        "Sobre",
        "Tarjeta doblada",
        "Etiqueta",
        "Caja con tapa",
        "Caja almohada",
        "Cono",
        "Bolsa de regalo",
        "Marcapáginas",
        "Tarjeta pop-up",
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Template::Tray { .. } => Self::NAMES[0],
            Template::Envelope { .. } => Self::NAMES[1],
            Template::FoldedCard { .. } => Self::NAMES[2],
            Template::Tag { .. } => Self::NAMES[3],
            Template::LidBox { .. } => Self::NAMES[4],
            Template::Pillow { .. } => Self::NAMES[5],
            Template::Cone { .. } => Self::NAMES[6],
            Template::Bag { .. } => Self::NAMES[7],
            Template::Bookmark { .. } => Self::NAMES[8],
            Template::PopUp { .. } => Self::NAMES[9],
        }
    }
}

/// Líneas de una plantilla.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lines {
    pub cut: Vec<Polyline>,
    pub fold: Vec<Polyline>,
}

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn seg(a: Point, b: Point) -> Polyline {
    (vec![a, b], false)
}

/// Red de una caja abierta (bandeja) con la esquina superior izquierda en (0, 0).
fn tray(w: f64, d: f64, h: f64) -> Lines {
    // Cruz: base en el centro y una pared a cada lado. Las paredes de arriba y de abajo
    // llevan pestañas en sus extremos (en trapecio) que se pegan a las laterales.
    let tab = (h * 0.6).min(15.0);
    let t = tab * 0.3;
    let (x0, y0) = (h, h);
    let (x1, y1) = (h + w, h + d);
    let outline = vec![
        p(x0, 0.0),
        p(x1, 0.0),
        p(x1 + tab, t),
        p(x1 + tab, y0 - t),
        p(x1, y0),
        p(x1 + h, y0),
        p(x1 + h, y1),
        p(x1, y1),
        p(x1 + tab, y1 + t),
        p(x1 + tab, y1 + h - t),
        p(x1, y1 + h),
        p(x0, y1 + h),
        p(x0 - tab, y1 + h - t),
        p(x0 - tab, y1 + t),
        p(x0, y1),
        p(x0 - h, y1),
        p(x0 - h, y0),
        p(x0, y0),
        p(x0 - tab, y0 - t),
        p(x0 - tab, t),
    ];
    Lines {
        cut: vec![(outline, true)],
        fold: vec![
            // Bordes de la base.
            seg(p(x0, y0), p(x1, y0)),
            seg(p(x1, y0), p(x1, y1)),
            seg(p(x0, y1), p(x1, y1)),
            seg(p(x0, y0), p(x0, y1)),
            // Pestañas de las paredes de arriba y de abajo.
            seg(p(x1, 0.0), p(x1, y0)),
            seg(p(x0, 0.0), p(x0, y0)),
            seg(p(x1, y1), p(x1, y1 + h)),
            seg(p(x0, y1), p(x0, y1 + h)),
        ],
    }
}

/// Las mismas líneas desplazadas.
fn shifted(l: Lines, dx: f64, dy: f64) -> Lines {
    let mv = |v: Vec<Polyline>| -> Vec<Polyline> {
        v.into_iter()
            .map(|(pts, c)| (pts.into_iter().map(|q| p(q.x + dx, q.y + dy)).collect(), c))
            .collect()
    };
    Lines {
        cut: mv(l.cut),
        fold: mv(l.fold),
    }
}

/// Arco de `a` a `b` que se separa `sagitta` de la cuerda (hacia la izquierda del sentido
/// a→b si es positiva), en `n` tramos. Incluye los dos extremos.
fn arc(a: Point, b: Point, sagitta: f64, n: usize) -> Vec<Point> {
    let chord = b - a;
    let len = chord.hypot();
    if len < 1e-9 || sagitta.abs() < 1e-9 {
        return vec![a, b];
    }
    let s = sagitta.abs();
    let r = (len * len / 4.0 + s * s) / (2.0 * s);
    let u = chord / len;
    // Normal hacia el lado al que se abomba el arco.
    let nrm = kurbo::Vec2::new(u.y, -u.x) * sagitta.signum();
    let mid = a.midpoint(b);
    let center = mid - nrm * (r - s);
    let a0 = (a - center).atan2();
    let mut sweep = (b - center).atan2() - a0;
    // Se recorre por el lado del abombamiento (el arco corto, o el largo si la flecha supera
    // al radio).
    let via = (mid + nrm * s - center).atan2() - a0;
    let wrap = |x: f64| {
        (x + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
    };
    let via = wrap(via);
    sweep = wrap(sweep);
    if via.signum() != sweep.signum() {
        sweep -= sweep.signum() * std::f64::consts::TAU;
    }
    (0..=n)
        .map(|i| {
            let t = a0 + sweep * i as f64 / n as f64;
            match i {
                0 => a,
                i if i == n => b,
                _ => p(center.x + r * t.cos(), center.y + r * t.sin()),
            }
        })
        .collect()
}

/// Círculo de radio `r` como polilínea cerrada.
fn circle(center: Point, r: f64) -> Polyline {
    let pts = (0..32)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 32.0;
            p(center.x + r * a.cos(), center.y + r * a.sin())
        })
        .collect();
    (pts, true)
}

/// Lleva las líneas a que su caja empiece en (0, 0).
fn at_origin(l: Lines) -> Lines {
    let all = l.cut.iter().chain(&l.fold).flat_map(|(q, _)| q.iter());
    let (x0, y0) = all.fold((f64::INFINITY, f64::INFINITY), |(x, y), q| {
        (x.min(q.x), y.min(q.y))
    });
    if x0.is_finite() && y0.is_finite() {
        shifted(l, -x0, -y0)
    } else {
        l
    }
}

pub fn lines(t: Template) -> Lines {
    match t {
        Template::Tray { w, d, h } => tray(w, d, h),
        Template::Envelope { w, h } => {
            // Hueco de 3 mm alrededor de la tarjeta; solapas: laterales en trapecio, inferior
            // de 3/4 del alto y superior en pico.
            let (cw, ch) = (w + 6.0, h + 6.0);
            let side = cw * 0.25;
            let bottom = ch * 0.75;
            let top = ch * 0.45;
            let (x0, y0) = (side, top);
            let (x1, y1) = (side + cw, top + ch);
            let outline = vec![
                p(x0, y0),
                p(x0 + cw / 2.0, 0.0),
                p(x1, y0),
                p(x1 + side, y0 + ch * 0.2),
                p(x1 + side, y1 - ch * 0.2),
                p(x1, y1),
                p(x1 - cw * 0.1, y1 + bottom),
                p(x0 + cw * 0.1, y1 + bottom),
                p(x0, y1),
                p(0.0, y1 - ch * 0.2),
                p(0.0, y0 + ch * 0.2),
            ];
            Lines {
                cut: vec![(outline, true)],
                fold: vec![
                    seg(p(x0, y0), p(x1, y0)),
                    seg(p(x1, y0), p(x1, y1)),
                    seg(p(x0, y1), p(x1, y1)),
                    seg(p(x0, y0), p(x0, y1)),
                ],
            }
        }
        Template::FoldedCard { w, h } => Lines {
            cut: vec![(
                vec![p(0.0, 0.0), p(2.0 * w, 0.0), p(2.0 * w, h), p(0.0, h)],
                true,
            )],
            fold: vec![seg(p(w, 0.0), p(w, h))],
        },
        Template::Tag { w, h } => {
            let c = (w.min(h) * 0.2).min(12.0);
            let outline = vec![
                p(c, 0.0),
                p(w, 0.0),
                p(w, h),
                p(c, h),
                p(0.0, h - c),
                p(0.0, c),
            ];
            // El agujero va a 2 mm del chaflán y cabe con al menos 1 mm de borde: en etiquetas
            // estrechas se encoge, y si no cabe ni de 0,5 mm de radio no se pone.
            let r = (h * 0.08)
                .clamp(2.0, 4.0)
                .min((w - c - 3.0) / 2.0)
                .min(h / 2.0 - 1.0);
            let mut cut = vec![(outline, true)];
            if r >= 0.5 {
                let center = p(c + r + 2.0, h / 2.0);
                let hole: Vec<Point> = (0..32)
                    .map(|i| {
                        let a = std::f64::consts::TAU * i as f64 / 32.0;
                        p(center.x + r * a.cos(), center.y + r * a.sin())
                    })
                    .collect();
                cut.push((hole, true));
            }
            Lines {
                cut,
                fold: Vec::new(),
            }
        }
        Template::LidBox { w, d, h } => {
            // La base, y a su derecha una tapa algo mayor (1 mm de holgura por lado) y más
            // baja, que encaja por fuera.
            let base = tray(w, d, h);
            let lid_h = (h * 0.5).clamp(5.0, 30.0).min(h);
            let lid = tray(w + 2.0, d + 2.0, lid_h);
            let gap = 8.0;
            let base_w = w + 2.0 * h;
            let mut out = base;
            let lid = shifted(lid, base_w + gap, 0.0);
            out.cut.extend(lid.cut);
            out.fold.extend(lid.fold);
            out
        }
        Template::Pillow { w, h, d } => {
            // Dos caras unidas por un doblez, una pestaña para pegar a la izquierda y, en cada
            // extremo de cada cara, una lente: el corte se abomba hacia fuera y el doblez hacia
            // dentro.
            let d = d.min(w * 0.45);
            let tab = (w * 0.25).min(12.0);
            let c = tab * 0.5;
            let (xa, xb, xc) = (tab, tab + w, tab + 2.0 * w);
            let (y0, y1) = (d, d + h);
            let n = 20;
            let mut outline = Vec::new();
            outline.extend(arc(p(xa, y0), p(xb, y0), d, n));
            outline.extend(arc(p(xb, y0), p(xc, y0), d, n).into_iter().skip(1));
            outline.extend(arc(p(xc, y1), p(xb, y1), d, n));
            outline.extend(arc(p(xb, y1), p(xa, y1), d, n).into_iter().skip(1));
            outline.push(p(0.0, y1 - c));
            outline.push(p(0.0, y0 + c));
            Lines {
                cut: vec![(outline, true)],
                fold: vec![
                    seg(p(xa, y0), p(xa, y1)),
                    seg(p(xb, y0), p(xb, y1)),
                    (arc(p(xa, y0), p(xb, y0), -d, n), false),
                    (arc(p(xb, y0), p(xc, y0), -d, n), false),
                    (arc(p(xc, y1), p(xb, y1), -d, n), false),
                    (arc(p(xb, y1), p(xa, y1), -d, n), false),
                ],
            }
        }
        Template::Cone { d, h } => {
            // Sector de radio la generatriz y arco igual al perímetro de la base, con una
            // pestaña a lo largo de uno de sus lados rectos.
            let r = d / 2.0;
            let l = r.hypot(h);
            let theta = std::f64::consts::TAU * r / l;
            let n = ((theta / (std::f64::consts::PI / 36.0)).ceil() as usize).max(8);
            let at = |radius: f64, a: f64| p(radius * a.cos(), radius * a.sin());
            let mut outline = vec![p(0.0, 0.0)];
            outline.extend((0..=n).map(|i| at(l, theta * i as f64 / n as f64)));
            // Pestaña sobre el lado final, hacia fuera del sector, si queda sitio.
            let free = std::f64::consts::TAU - theta;
            let tw = 10.0_f64.min(l * 0.15).min(free * l * 0.2);
            let mut fold = Vec::new();
            if tw >= 1.5 {
                let nrm = kurbo::Vec2::new(-theta.sin(), theta.cos());
                outline.push(at(l * 0.92, theta) + nrm * tw);
                outline.push(at(l * 0.3, theta) + nrm * tw);
                fold.push(seg(p(0.0, 0.0), at(l, theta)));
            }
            at_origin(Lines {
                cut: vec![(outline, true)],
                fold,
            })
        }
        Template::Bag { w, h, d } => {
            // Paneles en fila: pestaña | frente | lado | detrás | lado. Arriba, un dobladillo;
            // abajo, las solapas del fondo, separadas por cortes.
            let tab = 12.0_f64.min(d.max(8.0));
            let hem = 15.0_f64.min(h * 0.2);
            let bottom = d * 0.75;
            let xs = [
                tab,
                tab + w,
                tab + w + d,
                tab + 2.0 * w + d,
                tab + 2.0 * (w + d),
            ];
            let (y0, y1, y2) = (hem, hem + h, hem + h + bottom);
            let c = tab * 0.4;
            let outline = vec![
                p(xs[0], 0.0),
                p(xs[4], 0.0),
                p(xs[4], y2),
                p(xs[0], y2),
                p(xs[0], y1),
                p(0.0, y1 - c),
                p(0.0, c),
            ];
            let mut cut = vec![(outline, true)];
            // Cortes entre las solapas del fondo.
            for x in &xs[1..4] {
                cut.push(seg(p(*x, y1), p(*x, y2)));
            }
            // Agujeros para las asas, en el frente y detrás.
            if w >= 40.0 && h >= 50.0 {
                for x0 in [xs[0], xs[2]] {
                    for k in [0.3, 0.7] {
                        cut.push(circle(p(x0 + w * k, y0 + 12.0), 2.5));
                    }
                }
            }
            let mut fold = vec![
                seg(p(xs[0], y0), p(xs[4], y0)),
                seg(p(xs[0], y1), p(xs[4], y1)),
                seg(p(xs[0], 0.0), p(xs[0], y1)),
            ];
            for x in &xs[1..4] {
                fold.push(seg(p(*x, 0.0), p(*x, y1)));
            }
            // Fuelles: el centro de cada lado.
            for x0 in [xs[1], xs[3]] {
                fold.push(seg(p(x0 + d / 2.0, 0.0), p(x0 + d / 2.0, y1)));
            }
            Lines { cut, fold }
        }
        Template::Bookmark { w, h } => {
            let r = (w.min(h) * 0.2).min(6.0);
            let k = r * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
            let mut outline = Vec::new();
            // Cuatro esquinas redondeadas (cuartos de círculo).
            for (a, b) in [
                (p(0.0, r), p(r, 0.0)),
                (p(w - r, 0.0), p(w, r)),
                (p(w, h - r), p(w - r, h)),
                (p(r, h), p(0.0, h - r)),
            ] {
                outline.extend(arc(a, b, k, 6));
            }
            let mut cut = vec![(outline, true)];
            let hole = 2.5_f64.min(w / 2.0 - 2.0);
            if hole >= 1.0 && h >= 30.0 {
                cut.push(circle(p(w / 2.0, hole + 4.0), hole));
            }
            Lines {
                cut,
                fold: Vec::new(),
            }
        }
        Template::PopUp { w, h, s } => {
            // Tarjeta abierta de 2w × h con el doblez en vertical. El escalón: dos cortes
            // horizontales que cruzan el doblez y tres dobleces (los dos extremos del escalón
            // en un sentido y el del centro en el contrario).
            let s = s.min(w * 0.8);
            let (y1, y2) = (h * 0.3, h * 0.7);
            Lines {
                cut: vec![
                    (
                        vec![p(0.0, 0.0), p(2.0 * w, 0.0), p(2.0 * w, h), p(0.0, h)],
                        true,
                    ),
                    seg(p(w - s, y1), p(w + s, y1)),
                    seg(p(w - s, y2), p(w + s, y2)),
                ],
                fold: vec![
                    seg(p(w, 0.0), p(w, y1)),
                    seg(p(w, y2), p(w, h)),
                    seg(p(w - s, y1), p(w - s, y2)),
                    seg(p(w + s, y1), p(w + s, y2)),
                    seg(p(w, y1), p(w, y2)),
                ],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(l: &Lines) -> kurbo::Rect {
        crate::edit::bounds_of(
            l.cut
                .iter()
                .chain(&l.fold)
                .flat_map(|(p, _)| p.iter().copied()),
        )
        .unwrap()
    }

    #[test]
    fn tray_net_has_base_walls_and_folds() {
        let l = lines(Template::Tray {
            w: 60.0,
            d: 40.0,
            h: 20.0,
        });
        // Abierta: ancho w + 2h, alto d + 2h.
        assert_eq!(bounds(&l), kurbo::Rect::new(0.0, 0.0, 100.0, 80.0));
        assert_eq!(l.cut.len(), 1);
        assert!(l.cut[0].1, "contorno cerrado");
        assert_eq!(l.fold.len(), 8, "4 bordes de la base + 4 pestañas");
        // El contorno no se cruza consigo mismo.
        let c = &l.cut[0].0;
        let n = c.len();
        let cross = |a: Point, b: Point, c: Point, d: Point| {
            let o = |p: Point, q: Point, r: Point| (q - p).cross(r - p);
            o(a, b, c) * o(a, b, d) < 0.0 && o(c, d, a) * o(c, d, b) < 0.0
        };
        for i in 0..n {
            for j in i + 2..n {
                if i == 0 && j == n - 1 {
                    continue;
                }
                assert!(
                    !cross(c[i], c[(i + 1) % n], c[j], c[(j + 1) % n]),
                    "lados {i} y {j} se cruzan"
                );
            }
        }
        // Cada doblez está sobre el borde entre dos piezas: dentro de la caja abierta.
        for (f, _) in &l.fold {
            assert!(f.iter().all(|q| q.x >= 20.0 - 1e-9 && q.x <= 80.0 + 1e-9));
        }
    }

    #[test]
    fn other_templates_have_the_requested_size() {
        let card = lines(Template::FoldedCard { w: 105.0, h: 148.0 });
        assert_eq!(bounds(&card), kurbo::Rect::new(0.0, 0.0, 210.0, 148.0));
        assert_eq!(card.fold, vec![seg(p(105.0, 0.0), p(105.0, 148.0))]);
        let env = lines(Template::Envelope { w: 105.0, h: 148.0 });
        let b = bounds(&env);
        // La tarjeta (con 3 mm de holgura) cabe en el rectángulo de los dobleces.
        let fold =
            crate::edit::bounds_of(env.fold.iter().flat_map(|(p, _)| p.iter().copied())).unwrap();
        assert!((fold.width() - 111.0).abs() < 1e-9 && (fold.height() - 154.0).abs() < 1e-9);
        assert!(b.contains(fold.origin()));
        let tag = lines(Template::Tag { w: 50.0, h: 30.0 });
        assert_eq!(tag.cut.len(), 2, "contorno y agujero");
        assert!(tag.fold.is_empty());
    }

    #[test]
    fn tag_hole_always_fits_inside_the_tag() {
        // El diálogo permite desde 5 mm: antes, una etiqueta de 6 × 30 dejaba el agujero fuera.
        let pts = |l: &(Vec<Point>, bool)| crate::edit::bounds_of(l.0.iter().copied()).unwrap();
        for w in [5.0, 6.0, 8.0, 10.0, 20.0, 50.0, 600.0] {
            for h in [5.0, 6.0, 10.0, 30.0, 600.0] {
                let tag = lines(Template::Tag { w, h });
                let body = pts(&tag.cut[0]);
                assert_eq!(body, kurbo::Rect::new(0.0, 0.0, w, h));
                if let Some(hole) = tag.cut.get(1) {
                    let b = pts(hole);
                    assert!(
                        b.x0 > 0.0
                            && b.y0 >= 1.0 - 1e-9
                            && b.x1 <= w - 1.0 + 1e-9
                            && b.y1 <= h - 1.0 + 1e-9,
                        "{w} × {h}: agujero {b:?} fuera de la etiqueta"
                    );
                    assert!(
                        b.width() >= 1.0 - 1e-9,
                        "{w} × {h}: agujero de al menos 1 mm"
                    );
                }
            }
        }
        // Las de tamaño normal conservan su agujero de siempre (radio 2,4 mm en 50 × 30).
        let tag = lines(Template::Tag { w: 50.0, h: 30.0 });
        let b = pts(&tag.cut[1]);
        assert!((b.width() - 4.8).abs() < 1e-6);
        // 6 × 30: el agujero se encoge pero sigue ahí.
        assert_eq!(lines(Template::Tag { w: 6.0, h: 30.0 }).cut.len(), 2);
    }

    /// ¿Se cruza consigo mismo el contorno cerrado?
    fn self_crossing(c: &[Point]) -> Option<(usize, usize)> {
        let n = c.len();
        let cross = |a: Point, b: Point, c: Point, d: Point| {
            let o = |p: Point, q: Point, r: Point| (q - p).cross(r - p);
            o(a, b, c) * o(a, b, d) < 0.0 && o(c, d, a) * o(c, d, b) < 0.0
        };
        for i in 0..n {
            for j in i + 2..n {
                if i == 0 && j == n - 1 {
                    continue;
                }
                if cross(c[i], c[(i + 1) % n], c[j], c[(j + 1) % n]) {
                    return Some((i, j));
                }
            }
        }
        None
    }

    /// Comprobaciones comunes: empieza en (0, 0), contornos cerrados sin cruces y dobleces
    /// dentro de la caja.
    fn check(name: &str, l: &Lines) -> kurbo::Rect {
        let b = bounds(l);
        assert!(b.x0.abs() < 1e-9 && b.y0.abs() < 1e-9, "{name}: {b:?}");
        assert!(l.cut[0].1, "{name}: el contorno es cerrado");
        for (pts, closed) in &l.cut {
            assert!(
                pts.iter().all(|q| q.x.is_finite() && q.y.is_finite()),
                "{name}"
            );
            if *closed {
                assert_eq!(self_crossing(pts), None, "{name}: el contorno se cruza");
            }
        }
        for (f, _) in &l.fold {
            assert!(
                f.iter().all(|q| b.inflate(1e-9, 1e-9).contains(*q)),
                "{name}: doblez fuera de la plantilla"
            );
        }
        b
    }

    #[test]
    fn arcs_bulge_to_the_requested_side() {
        // De izquierda a derecha con flecha positiva: se abomba hacia arriba (y menor).
        let a = arc(p(0.0, 10.0), p(20.0, 10.0), 4.0, 16);
        assert_eq!((a[0], a[16]), (p(0.0, 10.0), p(20.0, 10.0)));
        assert!(
            (a[8].y - 6.0).abs() < 1e-9 && (a[8].x - 10.0).abs() < 1e-9,
            "{:?}",
            a[8]
        );
        assert!(a.iter().all(|q| q.y <= 10.0 + 1e-9));
        // Flecha negativa: hacia abajo.
        let b = arc(p(0.0, 10.0), p(20.0, 10.0), -4.0, 16);
        assert!((b[8].y - 14.0).abs() < 1e-9);
        // Todos los puntos están sobre el mismo círculo.
        let r = (20.0_f64 * 20.0 / 4.0 + 16.0) / 8.0;
        let c = p(10.0, 6.0 + r);
        assert!(a.iter().all(|q| (q.distance(c) - r).abs() < 1e-9));
        // Sin flecha es un tramo recto.
        assert_eq!(arc(p(0.0, 0.0), p(5.0, 0.0), 0.0, 8).len(), 2);
    }

    #[test]
    fn lid_box_has_a_base_and_a_bigger_lid() {
        let l = lines(Template::LidBox {
            w: 60.0,
            d: 40.0,
            h: 20.0,
        });
        let b = check("caja con tapa", &l);
        assert_eq!(l.cut.len(), 2, "base y tapa");
        let part = |i: usize| crate::edit::bounds_of(l.cut[i].0.iter().copied()).unwrap();
        // Base abierta: 60 + 2·20 de ancho. La tapa, con paredes de 10 y 2 mm más de base.
        assert!((part(0).width() - 100.0).abs() < 1e-9);
        assert!((part(1).width() - 82.0).abs() < 1e-9 && (part(1).height() - 62.0).abs() < 1e-9);
        assert!(part(1).x0 > part(0).x1, "no se solapan");
        assert_eq!(l.fold.len(), 16);
        assert!((b.width() - (100.0 + 8.0 + 82.0)).abs() < 1e-9);
    }

    #[test]
    fn pillow_box_has_two_faces_and_lens_ends() {
        let l = lines(Template::Pillow {
            w: 70.0,
            h: 110.0,
            d: 12.0,
        });
        let b = check("caja almohada", &l);
        // Pestaña de 12 + dos caras de 70; alto: el largo más la curva en cada extremo.
        assert!(
            (b.width() - 152.0).abs() < 1e-9 && (b.height() - 134.0).abs() < 1e-9,
            "{b:?}"
        );
        assert_eq!(l.fold.len(), 6, "dos rectos y cuatro curvos");
        // Los dobleces curvos van hacia dentro: entre y = 12 y y = 122.
        for (f, _) in &l.fold[2..] {
            assert!(f.iter().all(|q| q.y >= 12.0 - 1e-9 && q.y <= 122.0 + 1e-9));
            assert!(f.len() > 10, "es una curva");
        }
        // Una curva exagerada se limita para que las lentes no se toquen.
        check(
            "caja almohada muy curva",
            &lines(Template::Pillow {
                w: 30.0,
                h: 40.0,
                d: 500.0,
            }),
        );
    }

    #[test]
    fn cone_sector_matches_the_base() {
        let (d, h) = (60.0_f64, 80.0_f64);
        let l = lines(Template::Cone { d, h });
        check("cono", &l);
        let slant = (d / 2.0).hypot(h);
        // El doblez de la pestaña mide la generatriz.
        let (f, _) = &l.fold[0];
        assert!((f[0].distance(f[1]) - slant).abs() < 1e-9);
        // El arco del sector mide lo que el perímetro de la base.
        let apex = f[0];
        let on_arc: Vec<Point> = l.cut[0]
            .0
            .iter()
            .copied()
            .filter(|q| (q.distance(apex) - slant).abs() < 1e-6)
            .collect();
        let len: f64 = on_arc.windows(2).map(|w| w[0].distance(w[1])).sum();
        let base = std::f64::consts::PI * d;
        assert!((len - base).abs() / base < 0.01, "arco {len}, base {base}");
        // Conos muy chatos o muy agudos tampoco se cruzan.
        for (d, h) in [(200.0, 5.0), (10.0, 300.0), (5.0, 5.0), (600.0, 600.0)] {
            check("cono extremo", &lines(Template::Cone { d, h }));
        }
    }

    #[test]
    fn bag_bookmark_and_pop_up() {
        let bag = lines(Template::Bag {
            w: 80.0,
            h: 120.0,
            d: 40.0,
        });
        let b = check("bolsa", &bag);
        // Pestaña 12 + (80 + 40)·2 de ancho; dobladillo 15 + 120 + fondo 30 de alto.
        assert!(
            (b.width() - 252.0).abs() < 1e-9 && (b.height() - 165.0).abs() < 1e-9,
            "{b:?}"
        );
        assert_eq!(
            bag.cut.iter().filter(|c| !c.1).count(),
            3,
            "cortes del fondo"
        );
        assert_eq!(
            bag.cut.iter().filter(|c| c.1).count(),
            5,
            "contorno y cuatro agujeros"
        );
        check(
            "bolsa pequeña",
            &lines(Template::Bag {
                w: 20.0,
                h: 30.0,
                d: 5.0,
            }),
        );
        let mark = lines(Template::Bookmark { w: 45.0, h: 150.0 });
        assert_eq!(
            check("marcapáginas", &mark),
            kurbo::Rect::new(0.0, 0.0, 45.0, 150.0)
        );
        assert_eq!(mark.cut.len(), 2, "contorno y agujero");
        assert!(mark.fold.is_empty());
        // Las esquinas son redondeadas: ninguna punta en (0, 0).
        assert!(mark.cut[0].0.iter().all(|q| q.distance(p(0.0, 0.0)) > 1.0));
        check(
            "marcapáginas mínimo",
            &lines(Template::Bookmark { w: 5.0, h: 5.0 }),
        );
        let pop = lines(Template::PopUp {
            w: 105.0,
            h: 148.0,
            s: 25.0,
        });
        assert_eq!(
            check("pop-up", &pop),
            kurbo::Rect::new(0.0, 0.0, 210.0, 148.0)
        );
        assert_eq!(pop.cut.len(), 3, "contorno y dos cortes del escalón");
        // Los cortes cruzan el doblez central, y el doblez central se interrumpe en el escalón.
        assert_eq!(pop.cut[1].0, vec![p(80.0, 44.4), p(130.0, 44.4)]);
        assert_eq!(pop.fold.len(), 5);
        // Un saliente mayor que la tarjeta se limita.
        let big = lines(Template::PopUp {
            w: 50.0,
            h: 80.0,
            s: 500.0,
        });
        assert!(big.cut[1].0.iter().all(|q| q.x > 0.0 && q.x < 100.0));
    }
}
