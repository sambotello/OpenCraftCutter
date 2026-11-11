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
}

impl Template {
    pub const NAMES: [&'static str; 4] = ["Caja abierta", "Sobre", "Tarjeta doblada", "Etiqueta"];

    pub fn name(&self) -> &'static str {
        match self {
            Template::Tray { .. } => Self::NAMES[0],
            Template::Envelope { .. } => Self::NAMES[1],
            Template::FoldedCard { .. } => Self::NAMES[2],
            Template::Tag { .. } => Self::NAMES[3],
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

pub fn lines(t: Template) -> Lines {
    match t {
        Template::Tray { w, d, h } => {
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
}
