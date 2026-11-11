//! Trazado de imágenes: convierte un mapa de bits en trazados de corte (vectorización).
//!
//! Usa `vtracer` (visioncortex) para agrupar píxeles y ajustar curvas; el resultado SVG se
//! importa con `oc-svg` y se escala al tamaño físico de la imagen en la página.

use image::{DynamicImage, GenericImageView};
use kurbo::{Affine, Point, Rect};
use oc_core::shape::{Shape, ShapeKind, Stroke};
use vtracer::{ColorImage, ColorMode, Config, Hierarchical};

mod contour;
pub mod pixscan;

#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    #[error("No se pudo decodificar la imagen: {0}")]
    Decode(#[from] image::ImageError),
    #[error("No se pudo trazar la imagen: {0}")]
    Trace(String),
    #[error("No se pudo interpretar el trazado: {0}")]
    Svg(#[from] oc_svg::SvgError),
    #[error("La imagen no produce ninguna forma con estos ajustes.")]
    Empty,
}

/// Ajustes de trazado expuestos en el panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceSettings {
    /// Umbral de luminancia (0-255): los píxeles más oscuros se consideran tinta.
    pub threshold: u8,
    /// Invertir: trazar lo claro en lugar de lo oscuro.
    pub invert: bool,
    /// Área mínima (px²) de las motas que se conservan.
    pub filter_speckle: usize,
    /// Suavizado de curvas: 0 = polígonos, 1 = curvas.
    pub smooth: bool,
    /// Ángulo mínimo (grados) para considerar una esquina.
    pub corner_threshold: i32,
    /// Trazar en color (capas por color) en lugar de blanco y negro.
    pub color: bool,
    /// Área de trazado en coordenadas normalizadas de la imagen (x0, y0, x1, y1); `None` = toda.
    pub area: Option<[f64; 4]>,
    /// Qué contornos conservar.
    pub mode: TraceMode,
    /// Filtro de paso alto (radio en px de la imagen); 0 = apagado. Realza bordes en fotos.
    pub high_pass: f32,
    /// Filtro de paso bajo (radio en px); 0 = apagado. Suaviza ruido antes del umbral.
    pub low_pass: f32,
    /// Factor de ampliación antes de trazar (curvas más suaves en imágenes pequeñas).
    pub scale: u32,
    /// Calidad del trazo, 0 (baja) a 4 (máxima).
    pub quality: u8,
}

/// Qué contornos produce el trazado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraceMode {
    /// Todos los bordes, agujeros incluidos.
    #[default]
    All,
    /// Solo el contorno exterior de cada mancha.
    Outer,
}

impl Default for TraceSettings {
    fn default() -> Self {
        TraceSettings {
            threshold: 128,
            invert: false,
            filter_speckle: 8,
            smooth: true,
            corner_threshold: 45,
            color: false,
            area: None,
            mode: TraceMode::All,
            high_pass: 0.0,
            low_pass: 0.0,
            scale: 1,
            quality: 3,
        }
    }
}

/// Decodifica una imagen y devuelve (ancho, alto) en píxeles.
pub fn image_size(bytes: &[u8]) -> Result<(u32, u32), TraceError> {
    let img = image::load_from_memory(bytes)?;
    Ok(img.dimensions())
}

/// Recodifica cualquier formato soportado a PNG (para guardarlo en el documento).
pub fn to_png(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), TraceError> {
    let img = image::load_from_memory(bytes)?;
    let (w, h) = img.dimensions();
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok((out, w, h))
}

/// Píxeles RGBA de una imagen PNG (para subirla como textura).
pub fn decode_rgba(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), TraceError> {
    let img = image::load_from_memory(bytes)?;
    let (w, h) = img.dimensions();
    Ok((img.to_rgba8().into_raw(), w, h))
}

/// Lado máximo de la imagen tras ampliar, para acotar memoria y tiempo.
const MAX_SCALED_SIDE: u32 = 3000;

/// Factor de escala efectivo (limitado por `MAX_SCALED_SIDE`).
fn effective_scale(w: u32, h: u32, scale: u32) -> u32 {
    let side = w.max(h).max(1);
    scale.max(1).min((MAX_SCALED_SIDE / side).max(1))
}

/// Luminancia sobre blanco (la transparencia cuenta como papel).
fn luminance(px: [u8; 4]) -> f32 {
    let [r, g, b, a] = px;
    let af = a as f32 / 255.0;
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) * af + 255.0 * (1.0 - af)
}

/// Aplica escala, paso bajo y paso alto. Devuelve la imagen lista para el umbral.
fn preprocess(img: &DynamicImage, settings: &TraceSettings, apply_scale: bool) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    let mut rgba = img.to_rgba8();
    let k = if apply_scale {
        effective_scale(w, h, settings.scale)
    } else {
        1
    };
    if k > 1 {
        rgba = image::imageops::resize(&rgba, w * k, h * k, image::imageops::FilterType::Triangle);
    }
    if settings.low_pass > 0.0 {
        rgba = image::imageops::blur(&rgba, settings.low_pass * k as f32);
    }
    if settings.high_pass > 0.0 {
        // Paso alto sobre la luminancia: original − desenfocado, centrado en gris medio.
        let blurred = image::imageops::blur(&rgba, settings.high_pass * k as f32);
        for (p, b) in rgba.pixels_mut().zip(blurred.pixels()) {
            let v = (luminance(p.0) - luminance(b.0) + 128.0).clamp(0.0, 255.0) as u8;
            p.0 = [v, v, v, 255];
        }
    }
    rgba
}

/// ¿El píxel (x, y) cae dentro del área de trazado?
fn in_area(settings: &TraceSettings, x: u32, y: u32, w: u32, h: u32) -> bool {
    match settings.area {
        None => true,
        Some([x0, y0, x1, y1]) => {
            let (u, v) = ((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
            u >= x0.min(x1) && u <= x0.max(x1) && v >= y0.min(y1) && v <= y0.max(y1)
        }
    }
}

/// ¿El píxel cuenta como tinta con estos ajustes?
fn is_ink(px: [u8; 4], settings: &TraceSettings) -> bool {
    (luminance(px) < settings.threshold as f32) != settings.invert
}

/// Máscara de previsualización: amarillo donde hay tinta (o solo en su borde si `outline`),
/// transparente en el resto (RGBA, w, h).
pub fn threshold_preview(
    png: &[u8],
    settings: &TraceSettings,
    outline: bool,
) -> Result<(Vec<u8>, u32, u32), TraceError> {
    let img = image::load_from_memory(png)?;
    let rgba = preprocess(&img, settings, false);
    let (w, h) = rgba.dimensions();
    let ink: Vec<bool> = rgba
        .enumerate_pixels()
        .map(|(x, y, p)| in_area(settings, x, y, w, h) && is_ink(p.0, settings))
        .collect();
    let at = |x: i64, y: i64| {
        x >= 0 && y >= 0 && x < w as i64 && y < h as i64 && ink[(y * w as i64 + x) as usize]
    };
    // Grosor del borde proporcional a la imagen, para que se vea a cualquier resolución.
    let r = (w.max(h) as i64 / 400).max(1);
    let near_paper = |x: i64, y: i64| {
        (1..=r).any(|d| {
            !(at(x - d, y) && at(x + d, y) && at(x, y - d) && at(x, y + d))
                || !(at(x - d, y - d) && at(x + d, y + d) && at(x + d, y - d) && at(x - d, y + d))
        })
    };
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let inside = at(x, y);
            let show = if outline {
                inside && near_paper(x, y)
            } else {
                inside
            };
            out.extend_from_slice(if show {
                &[0xff, 0xd8, 0x00, 0xb0]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    Ok((out, w, h))
}

/// Parámetros de ajuste de curvas de vtracer según la calidad (0 baja … 4 máxima):
/// (longitud mínima de segmento, umbral de empalme, iteraciones).
fn quality_params(quality: u8) -> (f64, i32, usize) {
    match quality.min(4) {
        0 => (10.0, 70, 4),
        1 => (7.0, 55, 6),
        2 => (5.0, 45, 8),
        3 => (4.0, 45, 10),
        _ => (3.5, 30, 16),
    }
}

/// Envuelve el contenido del SVG en `<g transform="scale(sx sy)">` y amplía el lienzo.
fn prescale_svg(svg: &str, sx: f64, sy: f64) -> String {
    let Some(open_end) = svg
        .find("<svg")
        .and_then(|i| svg[i..].find('>').map(|j| i + j + 1))
    else {
        return svg.to_string();
    };
    let Some(close) = svg.rfind("</svg>") else {
        return svg.to_string();
    };
    let head = &svg[..open_end];
    format!(
        "{head}<g transform=\"scale({sx} {sy})\">{}</g></svg>",
        &svg[open_end..close]
    )
}

/// Tolerancia del ajuste de curvas (px de la imagen original) según la calidad.
fn fit_tolerance(quality: u8) -> f64 {
    [1.5, 1.0, 0.7, 0.5, 0.3][quality.min(4) as usize]
}

/// Trazado en blanco y negro con el motor propio (`contour`): bordes subpíxel, esquinas
/// vivas, rectas rectas y curvas suaves.
fn trace_binary(
    img: &DynamicImage,
    target: Rect,
    settings: &TraceSettings,
    stroke: Stroke,
) -> Result<Vec<Shape>, TraceError> {
    let (ow, _) = img.dimensions();
    let rgba = preprocess(img, settings, true);
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return Err(TraceError::Empty);
    }
    let k = w as f64 / ow.max(1) as f64;
    let thr = settings.threshold as f32;
    let value: Vec<f32> = rgba
        .enumerate_pixels()
        .map(|(x, y, p)| {
            if !in_area(settings, x, y, w, h) {
                contour::PAPER
            } else if settings.invert {
                255.0 - luminance(p.0)
            } else {
                luminance(p.0)
            }
        })
        .collect();
    let t = if settings.invert { 255.5 - thr } else { thr };
    let min_area = settings.filter_speckle as f64 * k * k;
    let loops: Vec<Vec<Point>> = contour::iso_loops(&value, w as usize, h as usize, t)
        .into_iter()
        .filter(|l| contour::signed_area(l).abs() >= min_area.max(0.5 * k * k))
        .collect();
    if loops.is_empty() {
        return Err(TraceError::Empty);
    }
    let depth = contour::nesting(&loops);
    let params = contour::FitParams {
        corner_deg: settings.corner_threshold as f64,
        corner_span: 1.5 * k,
        tolerance: fit_tolerance(settings.quality) * k,
        smooth: settings.smooth,
    };
    let to_mm = Affine::translate(target.origin().to_vec2())
        * Affine::scale_non_uniform(target.width() / w as f64, target.height() / h as f64);
    // Como la referencia, el trazado conserva las curvas Bézier (pocos nodos).
    let world: Vec<oc_core::nodes::NodePath> = loops
        .iter()
        .map(|l| {
            oc_core::nodes::from_bez(&(to_mm * contour::fit_loop(l, &params)))
                .into_iter()
                .next()
                .unwrap_or(oc_core::nodes::NodePath {
                    nodes: Vec::new(),
                    closed: true,
                })
        })
        .collect();
    // Cada contorno exterior (profundidad par) forma una figura con sus agujeros directos.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut group_of = vec![usize::MAX; loops.len()];
    for i in (0..loops.len()).filter(|&i| depth[i] % 2 == 0) {
        group_of[i] = groups.len();
        groups.push(vec![i]);
    }
    if settings.mode == TraceMode::All {
        for i in (0..loops.len()).filter(|&i| depth[i] % 2 == 1) {
            let parent = (0..loops.len()).find(|&j| {
                depth[j] + 1 == depth[i] && oc_core::edit::point_in_polygon(loops[i][0], &loops[j])
            });
            match parent {
                Some(j) => groups[group_of[j]].push(i),
                None => groups.push(vec![i]),
            }
        }
    }
    let out: Vec<Shape> = groups
        .into_iter()
        .filter_map(|g| {
            let subs: Vec<oc_core::nodes::NodePath> = g
                .into_iter()
                .filter(|&i| world[i].nodes.len() >= 2)
                .map(|i| world[i].clone())
                .collect();
            if subs.is_empty() {
                return None;
            }
            let mut s = Shape::new(
                0,
                "Trazado",
                Rect::ZERO,
                ShapeKind::Path {
                    subpaths: Vec::new(),
                    fill_rule: oc_core::shape::FillRule::EvenOdd,
                },
            );
            s.set_world_node_paths(&subs);
            s.stroke = stroke;
            s.fill = None;
            Some(s)
        })
        .collect();
    if out.is_empty() {
        return Err(TraceError::Empty);
    }
    Ok(out)
}

/// Silueta de una imagen: contornos de la zona no transparente (alfa ≥ 50 %), en mm dentro de
/// `target` (caja de la imagen sin girar). Incluye los huecos transparentes encerrados (como la
/// referencia, que también corta por dentro). Si la imagen es opaca, es su rectángulo.
/// Los contornos se combinan con la regla par-impar.
/// Alfa mínimo (0–255) de un píxel para formar parte de la silueta de una imagen.
const SILHOUETTE_MIN_ALPHA: f32 = 12.0;

pub fn image_silhouette(png: &[u8], target: Rect) -> Result<Vec<Vec<Point>>, TraceError> {
    let img = image::load_from_memory(png)?;
    let (ow, oh) = img.dimensions();
    if ow == 0 || oh == 0 {
        return Err(TraceError::Empty);
    }
    // Trabajar como mucho a ~1500 px de lado: suficiente para el contorno y rápido.
    let k = (1500.0 / ow.max(oh) as f64).min(4.0);
    let (w, h) = (
        ((ow as f64 * k).round() as u32).max(1),
        ((oh as f64 * k).round() as u32).max(1),
    );
    let rgba =
        image::imageops::resize(&img.to_rgba8(), w, h, image::imageops::FilterType::Triangle);
    let value: Vec<f32> = rgba.pixels().map(|p| 255.0 - p.0[3] as f32).collect();
    // Como la referencia, cuenta como silueta todo lo que no es casi transparente: los trazos
    // finos (pelos, líneas sueltas) se dibujan con el borde suavizado y su alfa queda muy por
    // debajo del 50 %.
    let loops = contour::iso_loops(&value, w as usize, h as usize, 255.0 - SILHOUETTE_MIN_ALPHA);
    let min_area = (w as f64 * h as f64) * 1e-4;
    let loops: Vec<Vec<Point>> = loops
        .into_iter()
        .filter(|l| contour::signed_area(l).abs() >= min_area)
        .collect();
    if loops.is_empty() {
        return Err(TraceError::Empty);
    }
    let scale = w as f64 / ow as f64;
    let params = contour::FitParams {
        corner_deg: 45.0,
        corner_span: 1.5 * scale,
        tolerance: 0.5 * scale,
        smooth: true,
    };
    let to_mm = Affine::translate(target.origin().to_vec2())
        * Affine::scale_non_uniform(target.width() / w as f64, target.height() / h as f64);
    let out: Vec<Vec<Point>> = loops
        .iter()
        .map(|l| {
            let path = to_mm * contour::fit_loop(l, &params);
            let mut pts = Vec::new();
            kurbo::flatten(path.iter(), 0.02, |el| match el {
                kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => pts.push(p),
                _ => {}
            });
            pts
        })
        .filter(|p| p.len() >= 3)
        .collect();
    if out.is_empty() {
        return Err(TraceError::Empty);
    }
    Ok(out)
}

/// Simplificar (Editar puntos): reajusta una polilínea en mm con curvas Bézier de pocos nodos,
/// con desviación máxima `tol_mm`.
pub fn simplify_polyline(pts: &[Point], closed: bool, tol_mm: f64) -> kurbo::BezPath {
    let params = contour::FitParams {
        corner_deg: 45.0,
        corner_span: 0.6,
        tolerance: tol_mm,
        smooth: true,
    };
    if closed {
        contour::fit_loop(pts, &params)
    } else {
        contour::fit_open(pts, &params)
    }
}

/// Traza `png` y devuelve formas en mm encajadas en `target` (la caja de la imagen en la página).
pub fn trace(
    png: &[u8],
    target: Rect,
    settings: &TraceSettings,
    stroke: Stroke,
) -> Result<Vec<Shape>, TraceError> {
    let img = image::load_from_memory(png)?;
    if !settings.color {
        return trace_binary(&img, target, settings, stroke);
    }
    // En color se usa vtracer (capas por color).
    let (ow, oh) = img.dimensions();
    let color_image = ColorImage {
        pixels: img.to_rgba8().into_raw(),
        width: ow as usize,
        height: oh as usize,
    };
    // Tamaño real de lo que se traza (puede estar ampliado por `scale`).
    let (w, h) = (color_image.width as u32, color_image.height as u32);
    let k2 = ((w as f64 / ow.max(1) as f64).powi(2)).max(1.0);
    let (length_threshold, splice_threshold, max_iterations) = quality_params(settings.quality);
    let config = Config {
        color_mode: ColorMode::Color,
        hierarchical: Hierarchical::Stacked,
        filter_speckle: (settings.filter_speckle as f64 * k2) as usize,
        mode: if settings.smooth {
            visioncortex::PathSimplifyMode::Spline
        } else {
            visioncortex::PathSimplifyMode::Polygon
        },
        corner_threshold: settings.corner_threshold,
        path_precision: Some(3),
        length_threshold,
        splice_threshold,
        max_iterations,
        ..Config::default()
    };
    let svg = vtracer::convert(color_image, config).map_err(TraceError::Trace)?;
    if w == 0 || h == 0 {
        return Err(TraceError::Empty);
    }
    // El SVG de vtracer está en píxeles → oc-svg lo deja en mm a 96 dpi. Lo escalamos al
    // tamaño final dentro del propio SVG para que el aplanado de curvas se haga a escala real.
    let px_mm = oc_core::units::MM_PER_IN / 96.0;
    let sx = target.width() / (w as f64 * px_mm);
    let sy = target.height() / (h as f64 * px_mm);
    let text = prescale_svg(&svg.to_string(), sx, sy);
    let imported = oc_svg::import_svg(&text, stroke)?;
    if imported.shapes.is_empty() {
        return Err(TraceError::Empty);
    }
    let to_target = Affine::translate(target.origin().to_vec2());
    let mut out = Vec::new();
    for mut s in imported.shapes {
        let world: Vec<(Vec<Point>, bool)> = s
            .world_subpaths()
            .into_iter()
            .map(|(pts, c)| (pts.into_iter().map(|p| to_target * p).collect(), c))
            .collect();
        if let ShapeKind::Path { .. } = s.kind {
            s.set_world_subpaths(&world);
            s.name = "Trazado".into();
            s.stroke = stroke;
            if !settings.color {
                s.fill = None;
            }
            out.push(s);
        }
    }
    if settings.mode == TraceMode::Outer {
        out = out
            .into_iter()
            .filter_map(|s| {
                let outer: oc_core::ops::Polygons = oc_core::ops::polygons_of(&s)
                    .into_iter()
                    .filter_map(|shape| shape.into_iter().next().map(|c| vec![c]))
                    .collect();
                oc_core::ops::shape_from_polygons(&s, &outer, &s.name).map(|mut n| {
                    n.fill = s.fill;
                    n
                })
            })
            .collect();
    }
    if out.is_empty() {
        return Err(TraceError::Empty);
    }
    Ok(out)
}

/// Recorta la imagen a lo que queda dentro de `shapes` (mm, en la caja `target` de la imagen):
/// los píxeles de fuera pasan a transparentes. Devuelve el PNG nuevo.
pub fn mask_to_shapes(png: &[u8], target: Rect, shapes: &[Shape]) -> Result<Vec<u8>, TraceError> {
    let img = image::load_from_memory(png)?;
    let mut rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 || target.width() <= 0.0 || target.height() <= 0.0 {
        return Err(TraceError::Empty);
    }
    // Aristas de todos los contornos en píxeles.
    let to_px = |p: [f64; 2]| {
        (
            (p[0] - target.x0) / target.width() * w as f64,
            (p[1] - target.y0) / target.height() * h as f64,
        )
    };
    let mut edges: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for s in shapes {
        for shape in oc_core::ops::polygons_of(s) {
            for contour in shape {
                let n = contour.len();
                for i in 0..n {
                    edges.push((to_px(contour[i]), to_px(contour[(i + 1) % n])));
                }
            }
        }
    }
    // Relleno par-impar por líneas de barrido.
    let mut xs: Vec<f64> = Vec::new();
    for y in 0..h {
        let fy = y as f64 + 0.5;
        xs.clear();
        for ((x0, y0), (x1, y1)) in &edges {
            if (*y0 > fy) != (*y1 > fy) {
                xs.push(x0 + (fy - y0) / (y1 - y0) * (x1 - x0));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut inside_spans: Vec<(f64, f64)> = Vec::new();
        for pair in xs.chunks(2) {
            if let [a, b] = pair {
                inside_spans.push((*a, *b));
            }
        }
        for x in 0..w {
            let fx = x as f64 + 0.5;
            let inside = inside_spans.iter().any(|(a, b)| fx >= *a && fx <= *b);
            if !inside {
                rgba.get_pixel_mut(x, y).0[3] = 0;
            }
        }
    }
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc_png() -> Vec<u8> {
        // Círculo negro de radio 30 sobre fondo blanco, 100×100.
        let mut img = image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 255, 255, 255]));
        for (x, y, p) in img.enumerate_pixels_mut() {
            let dx = x as f64 - 50.0;
            let dy = y as f64 - 50.0;
            if dx * dx + dy * dy <= 30.0 * 30.0 {
                *p = image::Rgba([0, 0, 0, 255]);
            }
        }
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn traces_a_disc_to_one_closed_shape_in_target_rect() {
        let png = disc_png();
        let target = Rect::new(10.0, 20.0, 60.0, 70.0); // 50 mm para 100 px → 0.5 mm/px
        let shapes = trace(&png, target, &TraceSettings::default(), Stroke::default()).unwrap();
        assert_eq!(shapes.len(), 1);
        let r = shapes[0].rect;
        // Círculo de radio 30 px = 15 mm centrado en (35, 45)
        assert!((r.center().x - 35.0).abs() < 1.0, "{r:?}");
        assert!((r.center().y - 45.0).abs() < 1.0, "{r:?}");
        assert!((r.width() - 30.0).abs() < 2.0, "{r:?}");
    }

    #[test]
    fn invert_traces_the_background() {
        let png = disc_png();
        let settings = TraceSettings {
            invert: true,
            ..Default::default()
        };
        let shapes = trace(
            &png,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &settings,
            Stroke::default(),
        )
        .unwrap();
        assert!(!shapes.is_empty());
        let r = shapes[0].rect;
        assert!(r.width() > 90.0, "{r:?}");
    }

    #[test]
    fn scale_and_quality_keep_the_shape_in_the_target_rect() {
        let png = disc_png();
        let target = Rect::new(10.0, 20.0, 60.0, 70.0);
        for quality in [0, 4] {
            let settings = TraceSettings {
                scale: 4,
                quality,
                ..Default::default()
            };
            let shapes = trace(&png, target, &settings, Stroke::default()).unwrap();
            assert_eq!(shapes.len(), 1);
            let r = shapes[0].rect;
            assert!((r.center().x - 35.0).abs() < 1.0, "{r:?}");
            assert!((r.width() - 30.0).abs() < 2.0, "{r:?}");
        }
    }

    fn png_of(img: image::RgbaImage) -> Vec<u8> {
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn big_circle_stays_round_without_facets() {
        // Círculo binario (sin antialias) de radio 150 px: 1 px = 1 mm.
        let mut img = image::RgbaImage::from_pixel(400, 400, image::Rgba([255, 255, 255, 255]));
        for (x, y, p) in img.enumerate_pixels_mut() {
            let (dx, dy) = (x as f64 + 0.5 - 200.0, y as f64 + 0.5 - 200.0);
            if dx * dx + dy * dy <= 150.0 * 150.0 {
                *p = image::Rgba([0, 0, 0, 255]);
            }
        }
        let target = Rect::new(0.0, 0.0, 400.0, 400.0);
        for scale in [1, 4] {
            let st = TraceSettings {
                scale,
                ..Default::default()
            };
            let shapes = trace(&png_of(img.clone()), target, &st, Stroke::default()).unwrap();
            assert_eq!(shapes.len(), 1);
            let (pts, _) = &shapes[0].world_subpaths()[0];
            let mut worst: f64 = 0.0;
            for w in pts.windows(2) {
                // Desviación del punto medio de cada segmento respecto al círculo.
                let m = w[0].midpoint(w[1]);
                worst = worst.max(((m - Point::new(200.0, 200.0)).hypot() - 150.0).abs());
            }
            assert!(worst < 0.6, "escala {scale}: desviación {worst}");
        }
    }

    #[test]
    fn triangle_keeps_three_sharp_corners() {
        let mut img = image::RgbaImage::from_pixel(200, 200, image::Rgba([255, 255, 255, 255]));
        let (a, b, c) = ((100.0, 20.0), (180.0, 170.0), (20.0, 170.0));
        let side = |p: (f64, f64), q: (f64, f64), x: f64, y: f64| {
            (q.0 - p.0) * (y - p.1) - (q.1 - p.1) * (x - p.0)
        };
        for (x, y, px) in img.enumerate_pixels_mut() {
            let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
            if side(a, b, fx, fy) >= 0.0 && side(b, c, fx, fy) >= 0.0 && side(c, a, fx, fy) >= 0.0 {
                *px = image::Rgba([0, 0, 0, 255]);
            }
        }
        let target = Rect::new(0.0, 0.0, 200.0, 200.0);
        let shapes = trace(
            &png_of(img),
            target,
            &TraceSettings::default(),
            Stroke::default(),
        )
        .unwrap();
        let (pts, _) = &shapes[0].world_subpaths()[0];
        // Cada vértice del triángulo tiene un punto del trazado a menos de 1 px.
        for v in [a, b, c] {
            let d = pts
                .iter()
                .map(|p| (*p - Point::new(v.0, v.1)).hypot())
                .fold(f64::MAX, f64::min);
            assert!(d < 1.0, "vértice {v:?} a {d}");
        }
        // Lados rectos: pocos puntos en total.
        assert!(pts.len() <= 12, "{} puntos", pts.len());
    }

    #[test]
    fn area_cut_edge_is_a_straight_line() {
        // Disco recortado por un área que empieza a la mitad: el corte debe ser una recta.
        let png = disc_png();
        let st = TraceSettings {
            area: Some([0.5, 0.0, 1.0, 1.0]),
            scale: 4,
            corner_threshold: 30,
            ..Default::default()
        };
        let shapes = trace(
            &png,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &st,
            Stroke::default(),
        )
        .unwrap();
        let (pts, _) = &shapes[0].world_subpaths()[0];
        // Hay un único segmento recto sobre x = 50 que cubre casi todo el corte (~60 mm).
        let n = pts.len();
        let cut = (0..n).any(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            (a.x - 50.0).abs() < 0.3 && (b.x - 50.0).abs() < 0.3 && (a.y - b.y).abs() > 55.0
        });
        assert!(cut, "{pts:?}");
    }

    #[test]
    fn silhouette_follows_alpha_not_colour() {
        // Disco opaco con interior blanco (no debe perderse) sobre fondo transparente.
        let mut img = image::RgbaImage::from_pixel(200, 200, image::Rgba([0, 0, 0, 0]));
        for (x, y, p) in img.enumerate_pixels_mut() {
            let d = ((x as f64 + 0.5 - 100.0).powi(2) + (y as f64 + 0.5 - 100.0).powi(2)).sqrt();
            if d <= 80.0 {
                *p = if d < 60.0 {
                    image::Rgba([255, 255, 255, 255])
                } else {
                    image::Rgba([200, 30, 30, 255])
                };
            }
        }
        let loops =
            image_silhouette(&png_of(img.clone()), Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
        assert_eq!(loops.len(), 1);
        // Un hueco transparente encerrado también se conserva.
        let mut ring = img;
        for (x, y, p) in ring.enumerate_pixels_mut() {
            let d = ((x as f64 + 0.5 - 100.0).powi(2) + (y as f64 + 0.5 - 100.0).powi(2)).sqrt();
            if d < 30.0 {
                *p = image::Rgba([0, 0, 0, 0]);
            }
        }
        assert_eq!(
            image_silhouette(&png_of(ring), Rect::new(0.0, 0.0, 100.0, 100.0))
                .unwrap()
                .len(),
            2
        );
        let far = loops[0]
            .iter()
            .map(|p| (*p - Point::new(50.0, 50.0)).hypot())
            .fold(0.0, f64::max);
        assert!((far - 40.0).abs() < 1.0, "{far}");
    }

    #[test]
    fn noisy_image_traces_quickly() {
        let mut img = image::RgbaImage::new(1200, 1200);
        let mut seed: u32 = 1;
        for p in img.pixels_mut() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let v = (seed >> 24) as u8;
            *p = image::Rgba([v, v, v, 255]);
        }
        let start = std::time::Instant::now();
        let st = TraceSettings {
            filter_speckle: 4,
            ..Default::default()
        };
        let _ = trace(
            &png_of(img),
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &st,
            Stroke::default(),
        );
        assert!(
            start.elapsed().as_secs_f64() < 20.0,
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn outline_preview_marks_only_the_border() {
        let png = disc_png();
        let st = TraceSettings::default();
        let count = |rgba: &[u8]| rgba.chunks(4).filter(|p| p[3] > 0).count();
        let (solid, _, _) = threshold_preview(&png, &st, false).unwrap();
        let (outline, w, _) = threshold_preview(&png, &st, true).unwrap();
        assert!(count(&outline) * 5 < count(&solid));
        // El centro del disco no se marca en modo contorneado.
        let c = ((50 * w + 50) * 4 + 3) as usize;
        assert!(solid[c] > 0 && outline[c] == 0);
    }

    #[test]
    fn filters_still_trace_a_disc() {
        let png = disc_png();
        let settings = TraceSettings {
            low_pass: 1.5,
            ..Default::default()
        };
        let shapes = trace(
            &png,
            Rect::new(0.0, 0.0, 50.0, 50.0),
            &settings,
            Stroke::default(),
        );
        assert_eq!(shapes.unwrap().len(), 1);
        let settings = TraceSettings {
            high_pass: 5.0,
            ..Default::default()
        };
        assert!(
            trace(
                &png,
                Rect::new(0.0, 0.0, 50.0, 50.0),
                &settings,
                Stroke::default()
            )
            .is_ok()
        );
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    fn ring_png() -> Vec<u8> {
        // Anillo negro (radio 30, agujero 15) sobre blanco, 100×100.
        let mut img = image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 255, 255, 255]));
        for (x, y, p) in img.enumerate_pixels_mut() {
            let d = ((x as f64 - 50.0).powi(2) + (y as f64 - 50.0).powi(2)).sqrt();
            if (15.0..=30.0).contains(&d) {
                *p = image::Rgba([0, 0, 0, 255]);
            }
        }
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn outer_mode_drops_holes() {
        let png = ring_png();
        let target = Rect::new(0.0, 0.0, 100.0, 100.0);
        let all = trace(&png, target, &TraceSettings::default(), Stroke::default()).unwrap();
        let outer = trace(
            &png,
            target,
            &TraceSettings {
                mode: TraceMode::Outer,
                ..Default::default()
            },
            Stroke::default(),
        )
        .unwrap();
        let count = |v: &Vec<Shape>| v.iter().map(|s| s.world_subpaths().len()).sum::<usize>();
        assert!(count(&all) >= 2, "{}", count(&all));
        assert_eq!(count(&outer), 1);
    }

    #[test]
    fn area_limits_the_trace() {
        let png = ring_png();
        let target = Rect::new(0.0, 0.0, 100.0, 100.0);
        // Área de la mitad izquierda: el resultado no pasa del centro.
        let settings = TraceSettings {
            area: Some([0.0, 0.0, 0.5, 1.0]),
            ..Default::default()
        };
        let shapes = trace(&png, target, &settings, Stroke::default()).unwrap();
        for s in &shapes {
            assert!(s.rect.x1 <= 52.0, "{:?}", s.rect);
        }
    }

    #[test]
    fn preview_and_mask() {
        let png = ring_png();
        let (rgba, w, h) = threshold_preview(&png, &TraceSettings::default(), false).unwrap();
        assert_eq!((w, h), (100, 100));
        let ink_px = rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(ink_px > 1500 && ink_px < 2500, "{ink_px}");
        let target = Rect::new(0.0, 0.0, 100.0, 100.0);
        let outer = trace(
            &png,
            target,
            &TraceSettings {
                mode: TraceMode::Outer,
                ..Default::default()
            },
            Stroke::default(),
        )
        .unwrap();
        let masked = mask_to_shapes(&png, target, &outer).unwrap();
        let img = image::load_from_memory(&masked).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(2, 2).0[3], 0, "fuera queda transparente");
        assert_eq!(
            img.get_pixel(50, 50).0[3],
            255,
            "dentro del contorno exterior se conserva"
        );
    }
}

#[cfg(test)]
mod silhouette_dump {
    #[test]
    fn dump_silhouette_if_requested() {
        // OPENCRAFTCUTTER_SIL_DUMP=imagen.png:salida.svg
        let Ok(spec) = std::env::var("OPENCRAFTCUTTER_SIL_DUMP") else {
            return;
        };
        let (inp, out) = spec.split_once(':').unwrap();
        let png = std::fs::read(inp).unwrap();
        let loops = super::image_silhouette(&png, kurbo::Rect::new(0.0, 0.0, 68.0, 93.0)).unwrap();
        let mut svg = String::from(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='-5 -5 78 103' width='780' height='1030'><rect x='-5' y='-5' width='78' height='103' fill='white'/>",
        );
        svg.push_str(&format!(
            "<image href='{inp}' x='0' y='0' width='68' height='93' preserveAspectRatio='none'/>"
        ));
        for l in &loops {
            let d: Vec<String> = l.iter().map(|p| format!("{:.2},{:.2}", p.x, p.y)).collect();
            svg.push_str(&format!(
                "<polygon points='{}' fill='none' stroke='red' stroke-width='0.15'/>",
                d.join(" ")
            ));
        }
        svg.push_str("</svg>");
        std::fs::write(out, svg).unwrap();
        eprintln!("{} contornos", loops.len());
    }
}
