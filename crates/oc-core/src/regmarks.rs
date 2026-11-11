//! Marcas de registro para Imprimir y cortar (como la referencia): un cuadrado relleno en
//! una esquina y dos escuadras en las esquinas contiguas, metidas `inset` desde el borde.
//! La máquina las lee con su sensor óptico para alinear el corte con lo impreso.

use kurbo::{Point, Rect};
use serde::{Deserialize, Serialize};

/// 5/8 de pulgada, la inserción por defecto de la referencia.
pub const DEFAULT_INSET_MM: f64 = 15.875;

/// Ajustes de marcas de registro y sangrado de impresión.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegMarks {
    pub enabled: bool,
    /// Longitud de los brazos de las escuadras.
    pub length_mm: f64,
    /// Grosor de las líneas de las escuadras.
    pub thickness_mm: f64,
    pub inset_left_mm: f64,
    pub inset_top_mm: f64,
    pub inset_right_mm: f64,
    pub inset_bottom_mm: f64,
    /// Marcas invertidas en vertical (el cuadrado abajo a la izquierda).
    pub inverted: bool,
    /// Sangrado de impresión: los rellenos se imprimen un poco más grandes que el corte.
    pub bleed: bool,
    pub bleed_mm: f64,
}

impl Default for RegMarks {
    fn default() -> Self {
        RegMarks {
            enabled: false,
            length_mm: 20.0,
            thickness_mm: 0.5,
            inset_left_mm: DEFAULT_INSET_MM,
            inset_top_mm: DEFAULT_INSET_MM,
            inset_right_mm: DEFAULT_INSET_MM,
            inset_bottom_mm: DEFAULT_INSET_MM,
            inverted: false,
            bleed: false,
            bleed_mm: 1.27,
        }
    }
}

/// Refleja un rectángulo en vertical dentro de una página de alto `h`.
fn flip_v(r: Rect, h: f64) -> Rect {
    Rect::new(r.x0, h - r.y1, r.x1, h - r.y0)
}

/// Lado del cuadrado de registro.
pub const SQUARE_MM: f64 = 5.0;

impl RegMarks {
    /// Restaura las marcas por defecto (conserva si están activadas y el sangrado).
    pub fn reset_marks(&mut self) {
        let d = RegMarks::default();
        *self = RegMarks {
            enabled: self.enabled,
            bleed: self.bleed,
            bleed_mm: self.bleed_mm,
            ..d
        };
    }

    /// Fija las cuatro inserciones a la vez.
    pub fn set_inset(&mut self, mm: f64) {
        self.inset_left_mm = mm;
        self.inset_top_mm = mm;
        self.inset_right_mm = mm;
        self.inset_bottom_mm = mm;
    }

    /// Inserción común si las cuatro coinciden.
    pub fn uniform_inset(&self) -> Option<f64> {
        let v = self.inset_left_mm;
        [self.inset_top_mm, self.inset_right_mm, self.inset_bottom_mm]
            .iter()
            .all(|x| (x - v).abs() < 1e-9)
            .then_some(v)
    }

    /// Rectángulo delimitado por las marcas (las esquinas interiores de las escuadras).
    pub fn frame(&self, w: f64, h: f64) -> Rect {
        let r = Rect::new(
            self.inset_left_mm,
            self.inset_top_mm,
            w - self.inset_right_mm,
            h - self.inset_bottom_mm,
        );
        if self.inverted {
            // Reflejo vertical: las inserciones superior e inferior se cambian.
            Rect::new(r.x0, h - r.y1, r.x1, h - r.y0)
        } else {
            r
        }
    }

    /// Área de corte: dentro de las marcas.
    pub fn cut_area(&self, w: f64, h: f64) -> Rect {
        self.frame(w, h)
    }

    /// Rectángulos rellenos que forman las marcas (cuadrado + dos escuadras), en mm.
    pub fn mark_rects(&self, w: f64, h: f64) -> Vec<Rect> {
        // Se construyen sin invertir y luego se reflejan en vertical si hace falta.
        let f = RegMarks {
            inverted: false,
            ..*self
        }
        .frame(w, h);
        let (l, t) = (self.length_mm, self.thickness_mm);
        let mut out = vec![Rect::new(f.x0, f.y0, f.x0 + SQUARE_MM, f.y0 + SQUARE_MM)];
        // Escuadra superior derecha: esquina en (x1, y0), brazos hacia la izquierda y abajo.
        out.push(Rect::new(f.x1 - l, f.y0, f.x1, f.y0 + t));
        out.push(Rect::new(f.x1 - t, f.y0, f.x1, f.y0 + l));
        // Escuadra inferior izquierda: esquina en (x0, y1), brazos hacia arriba y la derecha.
        out.push(Rect::new(f.x0, f.y1 - l, f.x0 + t, f.y1));
        out.push(Rect::new(f.x0, f.y1 - t, f.x0 + l, f.y1));
        if self.inverted {
            out = out.into_iter().map(|r| flip_v(r, h)).collect();
        }
        out
    }

    /// Zonas junto a cada marca donde no debe haber diseño (de la esquina de la página
    /// hasta la inserción más la longitud de la marca).
    pub fn keep_out_zones(&self, w: f64, h: f64) -> Vec<Rect> {
        let reach = |inset: f64| inset + self.length_mm.max(SQUARE_MM);
        let (rl, rt, rr, rb) = (
            reach(self.inset_left_mm),
            reach(self.inset_top_mm),
            reach(self.inset_right_mm),
            reach(self.inset_bottom_mm),
        );
        let zones = [
            Rect::new(0.0, 0.0, rl, rt),
            Rect::new(w - rr, 0.0, w, rt),
            Rect::new(0.0, h - rb, rl, h),
        ];
        if self.inverted {
            zones.iter().map(|z| flip_v(*z, h)).collect()
        } else {
            zones.to_vec()
        }
    }
}

/// Trazos que dan problemas con Imprimir y cortar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkConflicts {
    /// Fuera del marco entre marcas: la máquina no los corta.
    pub outside: usize,
    /// Dentro del marco pero en la zona de una marca: pueden estorbar la lectura o cortarla.
    pub near_marks: usize,
}

impl MarkConflicts {
    pub fn any(&self) -> bool {
        self.outside + self.near_marks > 0
    }
}

impl RegMarks {
    /// Cuenta los trazos (polilíneas en mm) fuera del marco o sobre las zonas de las marcas.
    pub fn conflicts<'a>(
        &self,
        w: f64,
        h: f64,
        paths: impl IntoIterator<Item = &'a [Point]>,
    ) -> MarkConflicts {
        const EPS: f64 = 0.01;
        let frame = self.cut_area(w, h).inflate(EPS, EPS);
        let zones = self.keep_out_zones(w, h);
        let mut c = MarkConflicts::default();
        for pts in paths {
            if pts.iter().any(|p| !frame.contains(*p)) {
                c.outside += 1;
            } else if pts
                .windows(2)
                .any(|s| zones.iter().any(|z| segment_hits(s[0], s[1], *z)))
                || (pts.len() == 1 && zones.iter().any(|z| z.contains(pts[0])))
            {
                c.near_marks += 1;
            }
        }
        c
    }
}

/// ¿Toca el segmento `a`–`b` el rectángulo? (recorte de Liang-Barsky)
fn segment_hits(a: Point, b: Point, r: Rect) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-d.x, a.x - r.x0),
        (d.x, r.x1 - a.x),
        (-d.y, a.y - r.y0),
        (d.y, r.y1 - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflicts_outside_frame_and_near_marks() {
        let m = RegMarks {
            enabled: true,
            ..Default::default()
        };
        let (w, h) = (215.9, 279.4);
        let sq = |x: f64, y: f64, s: f64| {
            vec![
                Point::new(x, y),
                Point::new(x + s, y),
                Point::new(x + s, y + s),
                Point::new(x, y + s),
                Point::new(x, y),
            ]
        };
        let inside = sq(80.0, 100.0, 30.0);
        let outside = sq(5.0, 100.0, 30.0); // empieza antes del marco (x0 = 15,875)
        let on_square = sq(20.0, 20.0, 10.0); // dentro del marco, junto al cuadrado
        // Una línea que solo cruza la zona de la escuadra de arriba a la derecha.
        let crossing = vec![Point::new(150.0, 25.0), Point::new(195.0, 25.0)];
        let paths = [&inside, &outside, &on_square, &crossing];
        let c = m.conflicts(w, h, paths.iter().map(|p| p.as_slice()));
        assert_eq!(
            c,
            MarkConflicts {
                outside: 1,
                near_marks: 2
            }
        );
        let ok = m.conflicts(w, h, [inside.as_slice()]);
        assert!(!ok.any());
    }

    #[test]
    fn default_marks_on_letter_page() {
        let m = RegMarks {
            enabled: true,
            ..Default::default()
        };
        let (w, h) = (215.9, 279.4);
        let rects = m.mark_rects(w, h);
        assert_eq!(rects.len(), 5);
        // Cuadrado arriba a la izquierda, a 5/8" de los bordes.
        assert!((rects[0].x0 - 15.875).abs() < 1e-9 && (rects[0].y0 - 15.875).abs() < 1e-9);
        // Área de corte dentro de las marcas.
        let c = m.cut_area(w, h);
        assert!((c.x1 - (w - 15.875)).abs() < 1e-9 && (c.y1 - (h - 15.875)).abs() < 1e-9);
        // Zonas libres de 15,875 + 20 mm, como se mide en la referencia (~36 mm).
        let z = m.keep_out_zones(w, h);
        assert!((z[0].width() - 35.875).abs() < 1e-9);
    }

    #[test]
    fn inverted_flips_marks_vertically() {
        let m = RegMarks {
            enabled: true,
            inverted: true,
            ..Default::default()
        };
        let (w, h) = (215.9, 279.4);
        let r = m.mark_rects(w, h);
        // Como en la referencia: cuadrado abajo a la izquierda…
        assert!((r[0].x0 - 15.875).abs() < 1e-9 && (r[0].y1 - (h - 15.875)).abs() < 1e-9);
        // …y escuadras abajo a la derecha y arriba a la izquierda.
        assert!((r[1].x1 - (w - 15.875)).abs() < 1e-9 && (r[1].y1 - (h - 15.875)).abs() < 1e-9);
        assert!((r[3].x0 - 15.875).abs() < 1e-9 && (r[3].y0 - 15.875).abs() < 1e-9);
    }
}
