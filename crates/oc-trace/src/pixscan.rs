//! PixScan: colocar sobre el tapete la foto (o el escaneo) de un tapete con dibujos, para cortar
//! justo alrededor de lo impreso.
//!
//! - Comprobación de resolución (mínimo 640×480, recomendado 1600×900, como la referencia).
//! - Perfiles de calibración de cámara a partir de la foto de una tarjeta de puntos: se
//!   detectan los puntos y se estima la distorsión radial de la lente.
//! - Rectificación de perspectiva por homografía a partir de las cuatro esquinas del tapete.

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use kurbo::Point;
use serde::{Deserialize, Serialize};

/// Resolución mínima admitida y recomendada (lado mayor × lado menor).
pub const MIN_RES: (u32, u32) = (640, 480);
pub const RECOMMENDED_RES: (u32, u32) = (1600, 900);

/// Separación y diámetro de los puntos de la tarjeta de calibración, y margen al borde (mm).
pub const CARD_PITCH_MM: f64 = 5.0;
pub const CARD_DOT_MM: f64 = 0.9;
pub const CARD_MARGIN_MM: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    TooSmall,
    Low,
    Ok,
}

pub fn check_resolution(w: u32, h: u32) -> Resolution {
    let (big, small) = (w.max(h), w.min(h));
    if big < MIN_RES.0 || small < MIN_RES.1 {
        Resolution::TooSmall
    } else if big < RECOMMENDED_RES.0 || small < RECOMMENDED_RES.1 {
        Resolution::Low
    } else {
        Resolution::Ok
    }
}

/// Cámara que hizo la foto (EXIF «Make Model»), si la imagen lo indica.
pub fn camera_model(bytes: &[u8]) -> Option<String> {
    let exif = exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(bytes))
        .ok()?;
    let field = |tag| {
        exif.get_field(tag, exif::In::PRIMARY).map(|f| {
            f.display_value()
                .to_string()
                .trim_matches('"')
                .trim()
                .to_string()
        })
    };
    let make = field(exif::Tag::Make).unwrap_or_default();
    let model = field(exif::Tag::Model).unwrap_or_default();
    let name = if model.starts_with(&make) {
        model
    } else {
        format!("{make} {model}")
    };
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// Perfil de calibración: distorsión radial de la lente de una cámara a una resolución.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationProfile {
    pub name: String,
    /// Cámara (EXIF) con la que se calibró; `None` si la foto no lo indicaba.
    pub camera: Option<String>,
    pub width: u32,
    pub height: u32,
    /// Coeficientes del modelo r_d = r_u·(1 + k1·r_u² + k2·r_u⁴), con r normalizado a la
    /// semidiagonal de la imagen.
    pub k1: f64,
    pub k2: f64,
    /// Error medio de ajuste (px) de la tarjeta tras corregir.
    pub rms_px: f64,
}

impl CalibrationProfile {
    /// ¿Sirve para una foto de esta cámara y tamaño (en cualquier orientación)?
    pub fn matches(&self, camera: Option<&str>, w: u32, h: u32) -> bool {
        let same_size = (self.width, self.height) == (w, h) || (self.width, self.height) == (h, w);
        same_size && camera.is_some() && self.camera.as_deref() == camera
    }

    /// Posición distorsionada (en la foto) de un punto ideal.
    pub fn distort(&self, p: Point, w: f64, h: f64) -> Point {
        distort(p, w, h, self.k1, self.k2)
    }
}

fn distort(p: Point, w: f64, h: f64, k1: f64, k2: f64) -> Point {
    let c = Point::new(w / 2.0, h / 2.0);
    let norm = (w * w + h * h).sqrt() / 2.0;
    let d = (p - c) / norm;
    let r2 = d.hypot2();
    c + d * norm * (1.0 + k1 * r2 + k2 * r2 * r2)
}

/// Inversa de `distort` por punto fijo.
fn undistort(p: Point, w: f64, h: f64, k1: f64, k2: f64) -> Point {
    let c = Point::new(w / 2.0, h / 2.0);
    let norm = (w * w + h * h).sqrt() / 2.0;
    let dd = (p - c) / norm;
    let mut u = dd;
    for _ in 0..20 {
        let r2 = u.hypot2();
        u = dd / (1.0 + k1 * r2 + k2 * r2 * r2);
    }
    c + u * norm
}

/// Homografía 3×3 (fila mayor) que lleva puntos `src` a `dst` (4 o más pares).
pub fn homography(src: &[Point], dst: &[Point]) -> Option<[f64; 9]> {
    if src.len() < 4 || src.len() != dst.len() {
        return None;
    }
    // Normal equations del sistema DLT con h33 = 1 (8 incógnitas).
    let mut ata = [[0.0f64; 8]; 8];
    let mut atb = [0.0f64; 8];
    for (s, d) in src.iter().zip(dst) {
        let (x, y, u, v) = (s.x, s.y, d.x, d.y);
        let rows = [
            ([x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y], u),
            ([0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y], v),
        ];
        for (r, b) in rows {
            for i in 0..8 {
                for j in 0..8 {
                    ata[i][j] += r[i] * r[j];
                }
                atb[i] += r[i] * b;
            }
        }
    }
    let h = solve(ata, atb)?;
    Some([h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], 1.0])
}

/// Gauss con pivoteo parcial.
fn solve<const N: usize>(mut a: [[f64; N]; N], mut b: [f64; N]) -> Option<[f64; N]> {
    for col in 0..N {
        let piv = (col..N).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        for row in col + 1..N {
            let f = a[row][col] / a[col][col];
            let pivot_row = a[col];
            for (k, v) in a[row].iter_mut().enumerate().skip(col) {
                *v -= f * pivot_row[k];
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = [0.0; N];
    for i in (0..N).rev() {
        let s: f64 = (i + 1..N).map(|k| a[i][k] * x[k]).sum();
        x[i] = (b[i] - s) / a[i][i];
    }
    Some(x)
}

pub fn apply_h(h: &[f64; 9], p: Point) -> Point {
    let w = h[6] * p.x + h[7] * p.y + h[8];
    Point::new(
        (h[0] * p.x + h[1] * p.y + h[2]) / w,
        (h[3] * p.x + h[4] * p.y + h[5]) / w,
    )
}

fn bilinear(img: &RgbaImage, x: f64, y: f64) -> Rgba<u8> {
    let (w, h) = img.dimensions();
    if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
        return Rgba([255, 255, 255, 0]);
    }
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let p = |xx, yy| img.get_pixel(xx, yy).0;
    let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
    let mut out = [0u8; 4];
    for i in 0..4 {
        let top = a[i] as f64 * (1.0 - fx) + b[i] as f64 * fx;
        let bot = c[i] as f64 * (1.0 - fx) + d[i] as f64 * fx;
        out[i] = (top * (1.0 - fy) + bot * fy).round() as u8;
    }
    Rgba(out)
}

/// Rectifica la foto: `corners` son las esquinas del tapete en la foto (px; sup-izq,
/// sup-der, inf-der, inf-izq) y el resultado mide `out_w × out_h` px. Si hay perfil de
/// calibración, se corrige también la distorsión de la lente.
pub fn rectify(
    img: &DynamicImage,
    corners: [Point; 4],
    out_w: u32,
    out_h: u32,
    calibration: Option<&CalibrationProfile>,
) -> Option<RgbaImage> {
    let src = img.to_rgba8();
    let (w, h) = (src.width() as f64, src.height() as f64);
    // Las esquinas se marcan sobre la foto tal cual; la homografía se calcula en el espacio
    // sin distorsión y cada píxel de salida se vuelve a distorsionar para muestrear.
    let ideal: Vec<Point> = corners
        .iter()
        .map(|c| match calibration {
            Some(cal) => undistort(*c, w, h, cal.k1, cal.k2),
            None => *c,
        })
        .collect();
    let out_corners = [
        Point::new(0.0, 0.0),
        Point::new(out_w as f64, 0.0),
        Point::new(out_w as f64, out_h as f64),
        Point::new(0.0, out_h as f64),
    ];
    let hm = homography(&out_corners, &ideal)?;
    let mut out = RgbaImage::new(out_w, out_h);
    for (x, y, px) in out.enumerate_pixels_mut() {
        let p = apply_h(&hm, Point::new(x as f64 + 0.5, y as f64 + 0.5));
        let p = match calibration {
            Some(cal) => cal.distort(p, w, h),
            None => p,
        };
        *px = bilinear(&src, p.x - 0.5, p.y - 0.5);
    }
    Some(out)
}

/// Centros de las manchas oscuras pequeñas y redondas (puntos de la tarjeta), en px.
pub fn detect_dots(img: &DynamicImage) -> Vec<Point> {
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();
    // Umbral de Otsu.
    let mut hist = [0u64; 256];
    for p in gray.pixels() {
        hist[p.0[0] as usize] += 1;
    }
    let total = (w as u64 * h as u64) as f64;
    let sum: f64 = hist
        .iter()
        .enumerate()
        .map(|(i, c)| i as f64 * *c as f64)
        .sum();
    let (mut wb, mut sb, mut best, mut thr) = (0.0, 0.0, 0.0, 128u8);
    for (t, c) in hist.iter().enumerate() {
        wb += *c as f64;
        if wb == 0.0 || wb == total {
            continue;
        }
        sb += t as f64 * *c as f64;
        let (mb, mf) = (sb / wb, (sum - sb) / (total - wb));
        let between = wb * (total - wb) * (mb - mf).powi(2);
        if between > best {
            best = between;
            thr = t as u8;
        }
    }
    let dark: Vec<bool> = gray.pixels().map(|p| p.0[0] <= thr).collect();
    let mut seen = vec![false; dark.len()];
    let mut dots = Vec::new();
    let mut stack = Vec::new();
    for start in 0..dark.len() {
        if !dark[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut n, mut sx, mut sy) = (0usize, 0.0, 0.0);
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        while let Some(i) = stack.pop() {
            let (x, y) = ((i as u32) % w, (i as u32) / w);
            n += 1;
            sx += x as f64;
            sy += y as f64;
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let j = (ny as u32 * w + nx as u32) as usize;
                if dark[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        let (bw, bh) = ((x1 - x0 + 1) as f64, (y1 - y0 + 1) as f64);
        let fill = n as f64 / (bw * bh);
        let round = bw.max(bh) / bw.min(bh) < 1.6 && fill > 0.55;
        if n >= 4 && n < (w as usize * h as usize) / 500 && round {
            dots.push(Point::new(sx / n as f64 + 0.5, sy / n as f64 + 0.5));
        }
    }
    dots
}

/// Asigna coordenadas enteras de rejilla a los puntos a partir del más central.
fn grid_indices(dots: &[Point]) -> Vec<Option<(i32, i32)>> {
    let n = dots.len();
    let mut out = vec![None; n];
    if n < 9 {
        return out;
    }
    let cx = dots.iter().map(|p| p.x).sum::<f64>() / n as f64;
    let cy = dots.iter().map(|p| p.y).sum::<f64>() / n as f64;
    let c = Point::new(cx, cy);
    let start = (0..n)
        .min_by(|&a, &b| dots[a].distance(c).total_cmp(&dots[b].distance(c)))
        .unwrap();
    let nearest = |p: Point, exclude: usize| {
        (0..n)
            .filter(|&j| j != exclude)
            .min_by(|&a, &b| dots[a].distance(p).total_cmp(&dots[b].distance(p)))
            .unwrap()
    };
    // Ejes locales: vecino más cercano y el más cercano aproximadamente perpendicular.
    let s = dots[start];
    let mut neigh: Vec<usize> = (0..n).filter(|&j| j != start).collect();
    neigh.sort_by(|&a, &b| dots[a].distance(s).total_cmp(&dots[b].distance(s)));
    let ax = dots[neigh[0]] - s;
    let ay = neigh
        .iter()
        .map(|&j| dots[j] - s)
        .find(|v| (v.dot(ax) / (v.hypot() * ax.hypot())).abs() < 0.4)
        .unwrap_or(kurbo::Vec2::new(-ax.y, ax.x));
    let (ax, ay) = if ax.x.abs() >= ay.x.abs() {
        (ax, ay)
    } else {
        (ay, ax)
    };
    let ax = if ax.x < 0.0 { -ax } else { ax };
    let ay = if ay.y < 0.0 { -ay } else { ay };
    let mut step: Vec<Option<(kurbo::Vec2, kurbo::Vec2)>> = vec![None; n];
    out[start] = Some((0, 0));
    step[start] = Some((ax, ay));
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(i) = queue.pop_front() {
        let (gi, gj) = out[i].unwrap();
        let (sx, sy) = step[i].unwrap();
        for (di, dj, v) in [(1, 0, sx), (-1, 0, -sx), (0, 1, sy), (0, -1, -sy)] {
            let guess = dots[i] + v;
            let j = nearest(guess, i);
            if dots[j].distance(guess) > 0.35 * v.hypot() || out[j].is_some() {
                continue;
            }
            out[j] = Some((gi + di, gj + dj));
            // Actualizar el paso local (la distorsión lo hace variar suavemente).
            let real = dots[j] - dots[i];
            let (nsx, nsy) = if di != 0 {
                (real * di as f64, sy)
            } else {
                (sx, real * dj as f64)
            };
            step[j] = Some((nsx, nsy));
            queue.push_back(j);
        }
    }
    out
}

/// Error RMS (px) de ajustar una homografía rejilla→puntos corregidos con (k1, k2).
fn fit_error(grid: &[(Point, Point)], w: f64, h: f64, k1: f64, k2: f64) -> f64 {
    let (src, dst): (Vec<Point>, Vec<Point>) = grid
        .iter()
        .map(|(g, p)| (*g, undistort(*p, w, h, k1, k2)))
        .unzip();
    let Some(hm) = homography(&src, &dst) else {
        return f64::MAX;
    };
    let se: f64 = src
        .iter()
        .zip(&dst)
        .map(|(g, p)| apply_h(&hm, *g).distance_squared(*p))
        .sum();
    (se / src.len() as f64).sqrt()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalibrationError {
    NotEnoughDots,
    Quality,
}

/// Calibra a partir de la foto de la tarjeta de puntos.
pub fn calibrate(bytes: &[u8], name: &str) -> Result<CalibrationProfile, CalibrationError> {
    let img = image::load_from_memory(bytes).map_err(|_| CalibrationError::NotEnoughDots)?;
    let (w, h) = img.dimensions();
    let dots = detect_dots(&img);
    let idx = grid_indices(&dots);
    let grid: Vec<(Point, Point)> = dots
        .iter()
        .zip(&idx)
        .filter_map(|(p, g)| g.map(|(i, j)| (Point::new(i as f64, j as f64), *p)))
        .collect();
    if grid.len() < 60 {
        return Err(CalibrationError::NotEnoughDots);
    }
    let (wf, hf) = (w as f64, h as f64);
    // Búsqueda de k1 (y luego k2) por sección áurea.
    let golden = |f: &dyn Fn(f64) -> f64, mut a: f64, mut b: f64| {
        let g = (5f64.sqrt() - 1.0) / 2.0;
        for _ in 0..60 {
            let (c, d) = (b - g * (b - a), a + g * (b - a));
            if f(c) < f(d) {
                b = d;
            } else {
                a = c;
            }
        }
        (a + b) / 2.0
    };
    let k1 = golden(&|k| fit_error(&grid, wf, hf, k, 0.0), -0.5, 0.5);
    let k2 = golden(&|k| fit_error(&grid, wf, hf, k1, k), -0.3, 0.3);
    let rms = fit_error(&grid, wf, hf, k1, k2);
    // Paso de rejilla medio en px, para juzgar la calidad relativa.
    let pitch = grid
        .windows(2)
        .filter(|w| (w[0].0 - w[1].0).hypot() == 1.0)
        .map(|w| w[0].1.distance(w[1].1))
        .sum::<f64>()
        / grid.len().max(1) as f64;
    if rms > (0.08 * pitch).max(1.5) {
        return Err(CalibrationError::Quality);
    }
    Ok(CalibrationProfile {
        name: name.to_string(),
        camera: camera_model(bytes),
        width: w,
        height: h,
        k1,
        k2,
        rms_px: rms,
    })
}

/// Tamaños de la tarjeta de calibración (nombre, ancho, alto en mm).
pub const CARD_SIZES: [(&str, f64, f64); 4] = [
    ("Carta", 215.9, 279.4),
    ("A4", 210.0, 297.0),
    ("Doble carta", 279.4, 431.8),
    ("A3", 297.0, 420.0),
];

/// Centros de los puntos de la tarjeta para una hoja de `w × h` mm.
pub fn card_dots(w: f64, h: f64) -> Vec<Point> {
    let cols = ((w - 2.0 * CARD_MARGIN_MM) / CARD_PITCH_MM).floor() as usize + 1;
    let rows = ((h - 2.0 * CARD_MARGIN_MM) / CARD_PITCH_MM).floor() as usize + 1;
    let ox = (w - (cols - 1) as f64 * CARD_PITCH_MM) / 2.0;
    let oy = (h - (rows - 1) as f64 * CARD_PITCH_MM) / 2.0;
    (0..rows)
        .flat_map(|r| {
            (0..cols).map(move |c| {
                Point::new(ox + c as f64 * CARD_PITCH_MM, oy + r as f64 * CARD_PITCH_MM)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_thresholds() {
        assert_eq!(check_resolution(512, 512), Resolution::TooSmall);
        assert_eq!(check_resolution(2048, 512), Resolution::Low);
        assert_eq!(check_resolution(4032, 3024), Resolution::Ok);
        assert_eq!(check_resolution(900, 1600), Resolution::Ok);
    }

    #[test]
    fn homography_maps_corners() {
        let src = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let dst = [
            Point::new(5.0, 3.0),
            Point::new(52.0, 8.0),
            Point::new(47.0, 61.0),
            Point::new(2.0, 55.0),
        ];
        let h = homography(&src, &dst).unwrap();
        for (s, d) in src.iter().zip(&dst) {
            assert!(apply_h(&h, *s).distance(*d) < 1e-6);
        }
    }

    /// Imagen sintética de la tarjeta vista en perspectiva y con distorsión de lente.
    fn synthetic_card(k1: f64) -> (Vec<u8>, u32, u32) {
        let (w, h) = (1600u32, 1200u32);
        let dots_mm = card_dots(215.9, 279.4);
        // Hoja de 215,9 × 279,4 mm a ~4 px/mm, algo inclinada.
        let card = [
            Point::new(0.0, 0.0),
            Point::new(215.9, 0.0),
            Point::new(215.9, 279.4),
            Point::new(0.0, 279.4),
        ];
        let photo = [
            Point::new(380.0, 40.0),
            Point::new(1250.0, 70.0),
            Point::new(1290.0, 1160.0),
            Point::new(330.0, 1130.0),
        ];
        let hm = homography(&card, &photo).unwrap();
        let mut img = RgbaImage::from_pixel(w, h, Rgba([245, 245, 245, 255]));
        let r = 1.8;
        for d in dots_mm {
            let p = distort(apply_h(&hm, d), w as f64, h as f64, k1, 0.0);
            for y in (p.y - r).floor() as i64..=(p.y + r).ceil() as i64 {
                for x in (p.x - r).floor() as i64..=(p.x + r).ceil() as i64 {
                    if x >= 0
                        && y >= 0
                        && (x as u32) < w
                        && (y as u32) < h
                        && Point::new(x as f64 + 0.5, y as f64 + 0.5).distance(p) <= r
                    {
                        img.put_pixel(x as u32, y as u32, Rgba([10, 10, 10, 255]));
                    }
                }
            }
        }
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        (out, w, h)
    }

    #[test]
    fn calibration_recovers_lens_distortion() {
        let (png, w, h) = synthetic_card(-0.08);
        let prof = calibrate(&png, "prueba").unwrap();
        assert_eq!((prof.width, prof.height), (w, h));
        assert!((prof.k1 + 0.08).abs() < 0.02, "k1 = {}", prof.k1);
        assert!(prof.rms_px < 1.0, "rms = {}", prof.rms_px);
    }

    #[test]
    fn calibration_rejects_non_card() {
        let img = RgbaImage::from_pixel(1600, 1200, Rgba([250, 250, 250, 255]));
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        assert_eq!(calibrate(&out, "x"), Err(CalibrationError::NotEnoughDots));
    }

    #[test]
    fn rectify_straightens_a_skewed_square() {
        // Cuadrado negro en una foto en perspectiva: tras rectificar ocupa el centro.
        let (w, h) = (800u32, 600u32);
        let corners = [
            Point::new(100.0, 80.0),
            Point::new(700.0, 120.0),
            Point::new(650.0, 560.0),
            Point::new(60.0, 500.0),
        ];
        let mat = [
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(100.0, 100.0),
            Point::new(0.0, 100.0),
        ];
        let hm = homography(&mat, &corners).unwrap();
        let inv = homography(&corners, &mat).unwrap();
        let mut img = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));
        for (x, y, p) in img.enumerate_pixels_mut() {
            let m = apply_h(&inv, Point::new(x as f64 + 0.5, y as f64 + 0.5));
            if (25.0..75.0).contains(&m.x) && (25.0..75.0).contains(&m.y) {
                *p = Rgba([0, 0, 0, 255]);
            }
        }
        let _ = hm;
        let out = rectify(&DynamicImage::ImageRgba8(img), corners, 200, 200, None).unwrap();
        let dark = |x, y| out.get_pixel(x, y).0[0] < 100;
        assert!(dark(100, 100) && dark(55, 55) && dark(145, 145));
        assert!(!dark(40, 100) && !dark(160, 100) && !dark(100, 40));
    }

    #[test]
    fn card_has_5mm_grid() {
        let d = card_dots(215.9, 279.4);
        assert_eq!(d.len(), 42 * 54);
        assert!((d[1].x - d[0].x - 5.0).abs() < 1e-9);
    }
}
