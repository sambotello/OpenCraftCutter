//! Importación y exportación de SVG.
//!
//! La importación delega en `usvg`, que resuelve unidades, viewBox, transformaciones, estilos
//! heredados y convierte el texto a trazados con las fuentes del sistema. Aquí solo aplanamos
//! las curvas a polilíneas en mm y las convertimos en formas del documento.

use kurbo::{Affine, BezPath, PathEl, Point};
use oc_core::shape::{FillRule, LineStyle, Shape, ShapeKind, Stroke};
use oc_core::units::{MM_PER_IN, mm_to_pt, pt_to_mm};
use oc_core::{Color, Document};

/// usvg normaliza todo a píxeles CSS (96 dpi).
const MM_PER_PX: f64 = MM_PER_IN / 96.0;
/// Desviación máxima al aplanar curvas, en mm.
const FLATTEN_TOLERANCE_MM: f64 = 0.05;

#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    #[error("El archivo no es un SVG válido: {0}")]
    Parse(#[from] usvg::Error),
    #[error("El SVG no contiene formas importables.")]
    Empty,
}

pub struct ImportResult {
    pub shapes: Vec<Shape>,
    pub warnings: Vec<String>,
}

fn usvg_options() -> usvg::Options<'static> {
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    opt
}

/// Degradado lineal de usvg → nuestro modelo (ángulo en el espacio del mundo).
fn gradient_of(
    paint: &usvg::Paint,
    path_transform: usvg::Transform,
) -> Option<oc_core::shape::Gradient> {
    let usvg::Paint::LinearGradient(lg) = paint else {
        return None;
    };
    let t = affine_of(path_transform) * affine_of(lg.transform());
    let a = t * Point::new(lg.x1() as f64, lg.y1() as f64);
    let b = t * Point::new(lg.x2() as f64, lg.y2() as f64);
    let angle_deg = (b.y - a.y).atan2(b.x - a.x).to_degrees();
    let stops = lg
        .stops()
        .iter()
        .map(|st| {
            let c = st.color();
            oc_core::shape::GradientStop {
                pos: st.offset().get() as f64,
                color: Color::rgba(
                    c.red,
                    c.green,
                    c.blue,
                    (st.opacity().get() * 255.0).round() as u8,
                ),
            }
        })
        .collect::<Vec<_>>();
    if stops.len() < 2 {
        return None;
    }
    Some(oc_core::shape::Gradient { stops, angle_deg })
}

fn color_of(paint: &usvg::Paint) -> Color {
    match paint {
        usvg::Paint::Color(c) => Color::rgb(c.red, c.green, c.blue),
        _ => Color::rgb(0x80, 0x80, 0x80),
    }
}

fn affine_of(t: usvg::Transform) -> Affine {
    Affine::new([
        t.sx as f64,
        t.ky as f64,
        t.kx as f64,
        t.sy as f64,
        t.tx as f64,
        t.ty as f64,
    ])
}

fn mean_scale(a: Affine) -> f64 {
    a.determinant().abs().sqrt()
}

/// Convierte un trazado de usvg (coordenadas locales + transformación absoluta) en subtrazos en mm.
fn flatten_path(path: &usvg::Path) -> Vec<(Vec<Point>, bool)> {
    let to_mm = Affine::scale(MM_PER_PX) * affine_of(path.abs_transform());
    let mut bez = BezPath::new();
    for seg in path.data().segments() {
        use usvg::tiny_skia_path::PathSegment as S;
        let p = |q: usvg::tiny_skia_path::Point| Point::new(q.x as f64, q.y as f64);
        match seg {
            S::MoveTo(a) => bez.move_to(p(a)),
            S::LineTo(a) => bez.line_to(p(a)),
            S::QuadTo(a, b) => bez.quad_to(p(a), p(b)),
            S::CubicTo(a, b, c) => bez.curve_to(p(a), p(b), p(c)),
            S::Close => bez.close_path(),
        }
    }
    let bez = to_mm * bez;
    let mut out: Vec<(Vec<Point>, bool)> = Vec::new();
    kurbo::flatten(bez.iter(), FLATTEN_TOLERANCE_MM, |el| match el {
        PathEl::MoveTo(p) => out.push((vec![p], false)),
        PathEl::LineTo(p) => {
            if let Some(last) = out.last_mut() {
                last.0.push(p);
            }
        }
        PathEl::ClosePath => {
            if let Some(last) = out.last_mut() {
                last.1 = true;
            }
        }
        _ => {}
    });
    out.retain(|(pts, closed)| pts.len() > 1 || (*closed && !pts.is_empty()));
    out
}

fn stroke_of(path: &usvg::Path, defaults: &Stroke) -> Stroke {
    match path.stroke() {
        Some(st) => {
            let width_mm =
                st.width().get() as f64 * mean_scale(affine_of(path.abs_transform())) * MM_PER_PX;
            Stroke {
                color: color_of(st.paint()),
                width_pt: (mm_to_pt(width_mm) * 100.0).round() / 100.0,
                style: if st.dasharray().is_some() {
                    LineStyle::Dashed
                } else {
                    LineStyle::Solid
                },
                join: match st.linejoin() {
                    usvg::LineJoin::Round => oc_core::shape::LineJoin::Round,
                    usvg::LineJoin::Bevel => oc_core::shape::LineJoin::Bevel,
                    _ => oc_core::shape::LineJoin::Miter,
                },
                cap: match st.linecap() {
                    usvg::LineCap::Round => oc_core::shape::LineCap::Round,
                    usvg::LineCap::Square => oc_core::shape::LineCap::Square,
                    usvg::LineCap::Butt => oc_core::shape::LineCap::Butt,
                },
                opacity: st.opacity().get() as f64,
                ..*defaults
            }
        }
        None => *defaults,
    }
}

fn visit(group: &usvg::Group, out: &mut Vec<Shape>, skipped: &mut usize, defaults: &Stroke) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => visit(g, out, skipped, defaults),
            usvg::Node::Text(t) => visit(t.flattened(), out, skipped, defaults),
            usvg::Node::Image(_) => *skipped += 1,
            usvg::Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                let world = flatten_path(p);
                if world.is_empty() {
                    continue;
                }
                let (rect, subpaths) = Shape::normalize_subpaths(&world);
                let fill_rule = match p.fill().map(|f| f.rule()) {
                    Some(usvg::FillRule::EvenOdd) => FillRule::EvenOdd,
                    _ => FillRule::NonZero,
                };
                let name = if p.id().is_empty() {
                    "Trazo".to_string()
                } else {
                    p.id().to_string()
                };
                let mut shape = Shape::new(
                    0,
                    name,
                    rect,
                    ShapeKind::Path {
                        subpaths,
                        fill_rule,
                    },
                );
                shape.fill = p.fill().map(|f| color_of(f.paint()));
                if let Some(f) = p.fill() {
                    shape.fill_opacity = f.opacity().get() as f64;
                    if let Some(g) = gradient_of(f.paint(), p.abs_transform()) {
                        shape.fill = g.sorted().first().map(|s| s.color);
                        shape.fill_gradient = Some(g);
                    }
                }
                shape.stroke = stroke_of(p, defaults);
                out.push(shape);
            }
        }
    }
}

/// Importa un documento SVG como formas en mm (origen en la esquina superior izquierda del SVG).
pub fn import_svg(text: &str, default_stroke: Stroke) -> Result<ImportResult, SvgError> {
    let tree = usvg::Tree::from_str(text, &usvg_options())?;
    let mut shapes = Vec::new();
    let mut skipped = 0;
    visit(tree.root(), &mut shapes, &mut skipped, &default_stroke);
    let mut warnings = Vec::new();
    if skipped > 0 {
        warnings.push(format!("Se omitieron {skipped} imágenes de mapa de bits."));
    }
    if shapes.is_empty() {
        return Err(SvgError::Empty);
    }
    Ok(ImportResult { shapes, warnings })
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn n(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".into()
    } else {
        s.to_string()
    }
}

fn path_d(shape: &Shape) -> String {
    shape
        .world_subpaths()
        .iter()
        .filter(|(pts, _)| !pts.is_empty())
        .map(|(pts, closed)| {
            let body: Vec<String> = pts
                .iter()
                .map(|p| format!("{} {}", n(p.x), n(p.y)))
                .collect();
            format!("M {}{}", body.join(" L "), if *closed { " Z" } else { "" })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Exporta el documento como SVG en milímetros (1 unidad = 1 mm).
/// SVG para imprimir (Imprimir y cortar): sin las líneas que solo son de corte, con las
/// marcas de registro y, si está activado, con sangrado en los rellenos.
/// PDF para imprimir (Imprimir y cortar): el mismo contenido que [`print_svg`], a tamaño real.
pub fn print_pdf(doc: &Document) -> Result<Vec<u8>, String> {
    let svg = print_svg(doc);
    let mut opt = svg2pdf::usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = svg2pdf::usvg::Tree::from_str(&svg, &opt).map_err(|e| e.to_string())?;
    // usvg pasa los mm a px a 96 ppp; con dpi = 96 el PDF sale a tamaño real.
    let page = svg2pdf::PageOptions { dpi: 96.0 };
    svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), page).map_err(|e| e.to_string())
}

pub fn print_svg(doc: &Document) -> String {
    let mut d = doc.clone();
    let bleed = d.page.reg_marks.bleed.then_some(d.page.reg_marks.bleed_mm);
    d.shapes.retain(|s| {
        s.visible
            && (s.fill.is_some()
                || s.fill_gradient.is_some()
                || s.is_image()
                || s.is_text()
                || s.stroke.width_pt > 0.0
                || s.stroke.print_hairline)
    });
    if let Some(b) = bleed {
        for s in &mut d.shapes {
            if let (Some(fill), true) = (s.fill, s.stroke.width_pt <= 0.0) {
                // Trazo del color del relleno: la mancha impresa sobresale `b` del corte.
                s.stroke.color = fill;
                s.stroke.width_pt = oc_core::units::mm_to_pt(2.0 * b);
                s.stroke.align = oc_core::shape::StrokeAlign::Center;
                s.stroke.join = oc_core::shape::LineJoin::Round;
                s.stroke.style = oc_core::shape::LineStyle::Solid;
                s.stroke.opacity = s.fill_opacity;
            }
        }
    }
    export_svg(&d)
}

pub fn export_svg(doc: &Document) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n",
        w = n(doc.page.width_mm),
        h = n(doc.page.height_mm)
    ));
    out.push_str("  <!-- Exportado por OpenCraftCutter. Unidades: milímetros. -->\n");
    for s in doc.shapes.iter().filter(|s| s.visible) {
        let r = s.rect;
        let c = r.center();
        // Las líneas de corte de 0 pt se exportan a 0,1 pt si se pidió imprimirlas.
        let sw = if s.stroke.width_pt <= 0.0 && s.stroke.print_hairline {
            pt_to_mm(0.1)
        } else {
            pt_to_mm(s.stroke.width_pt)
        };
        let inside = s.stroke.align == oc_core::shape::StrokeAlign::Inside && sw > 0.0;
        let svg_sw = if inside { sw * 2.0 } else { sw };
        let dash = match oc_core::ops::dash_pattern(s.stroke.style, sw) {
            Some((d, g)) => format!(" stroke-dasharray=\"{} {}\"", n(d), n(g)),
            None => String::new(),
        };
        let mut stroke_extra = format!(
            " stroke-linejoin=\"{}\" stroke-linecap=\"{}\"",
            s.stroke.join.svg(),
            s.stroke.cap.svg()
        );
        if s.stroke.opacity < 1.0 {
            stroke_extra.push_str(&format!(" stroke-opacity=\"{}\"", n(s.stroke.opacity)));
        }
        let transform = if s.rotation != 0.0 {
            format!(
                " transform=\"rotate({} {} {})\"",
                n(s.rotation),
                n(c.x),
                n(c.y)
            )
        } else {
            String::new()
        };
        let (fill_attr, defs) = match (&s.fill_gradient, s.fill) {
            (Some(g), Some(_)) => {
                let gid = format!("oc-grad-{}", s.id);
                let (sin, cos) = g.angle_deg.to_radians().sin_cos();
                let half = ((r.width() * cos).abs() + (r.height() * sin).abs()) / 2.0;
                let stops: String = g
                    .sorted()
                    .iter()
                    .map(|st| {
                        let c = st.color;
                        format!("<stop offset=\"{}\" stop-color=\"#{:02x}{:02x}{:02x}\" stop-opacity=\"{}\"/>", n(st.pos), c.r, c.g, c.b, n(c.a as f64 / 255.0))
                    })
                    .collect();
                let defs = format!(
                    "  <defs><linearGradient id=\"{gid}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">{stops}</linearGradient></defs>\n",
                    n(c.x - cos * half),
                    n(c.y - sin * half),
                    n(c.x + cos * half),
                    n(c.y + sin * half)
                );
                (format!("url(#{gid})"), defs)
            }
            (_, Some(f)) => (f.to_hex(), String::new()),
            _ => ("none".to_string(), String::new()),
        };
        out.push_str(&defs);
        let opacity_attr = if s.fill_opacity < 1.0 {
            format!(" fill-opacity=\"{}\"", n(s.fill_opacity))
        } else {
            String::new()
        };
        let common = format!(
            "id=\"oc-{}\" fill=\"{}\"{} stroke=\"{}\" stroke-width=\"{}\"{}{}{}{} data-name=\"{}\"",
            s.id,
            fill_attr,
            opacity_attr,
            s.stroke.color.to_hex(),
            n(svg_sw),
            stroke_extra,
            if inside {
                format!(" clip-path=\"url(#oc-clip-{})\"", s.id)
            } else {
                String::new()
            },
            dash,
            transform,
            esc(&s.name)
        );
        let el = match &s.kind {
            ShapeKind::Image { .. } => continue,
            ShapeKind::Rect { corner_radius } => format!(
                "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{} {}/>",
                n(r.x0),
                n(r.y0),
                n(r.width()),
                n(r.height()),
                if *corner_radius > 0.0 {
                    format!(" rx=\"{}\"", n(*corner_radius))
                } else {
                    String::new()
                },
                common
            ),
            ShapeKind::Ellipse => format!(
                "  <ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" {}/>",
                n(c.x),
                n(c.y),
                n(r.width() / 2.0),
                n(r.height() / 2.0),
                common
            ),
            ShapeKind::Path { fill_rule, .. } => format!(
                "  <path d=\"{}\" fill-rule=\"{}\" {}/>",
                path_d(s),
                match fill_rule {
                    FillRule::NonZero => "nonzero",
                    FillRule::EvenOdd => "evenodd",
                },
                common
            ),
            ShapeKind::Text {
                text,
                font_family,
                font_size_pt,
                align,
                bold,
                italic,
                line_height,
                ..
            } => {
                let size_mm = pt_to_mm(*font_size_pt);
                let (anchor, ax) = match align {
                    oc_core::TextAlign::Left | oc_core::TextAlign::Justify => ("start", r.x0),
                    oc_core::TextAlign::Center => ("middle", c.x),
                    oc_core::TextAlign::Right => ("end", r.x1),
                };
                let tspans: String = text
                    .lines()
                    .enumerate()
                    .map(|(i, line)| {
                        format!(
                            "<tspan x=\"{}\" y=\"{}\">{}</tspan>",
                            n(ax),
                            n(r.y0 + size_mm * line_height * i as f64 + size_mm * 0.8),
                            esc(line)
                        )
                    })
                    .collect();
                format!(
                    "  <text text-anchor=\"{anchor}\" font-family=\"{}\" font-size=\"{}\"{}{} {common}>{tspans}</text>",
                    esc(&css_family(font_family)),
                    n(size_mm),
                    if *bold { " font-weight=\"bold\"" } else { "" },
                    if *italic {
                        " font-style=\"italic\""
                    } else {
                        ""
                    },
                )
            }
        };
        if inside {
            // Trazo hacia dentro: el doble de grosor recortado por la propia forma.
            let mut clip_el = el.replacen(&format!("id=\"oc-{}\" ", s.id), "", 1);
            clip_el = clip_el.replacen(&format!(" clip-path=\"url(#oc-clip-{})\"", s.id), "", 1);
            out.push_str(&format!(
                "  <defs><clipPath id=\"oc-clip-{}\">{}</clipPath></defs>\n",
                s.id,
                clip_el.trim()
            ));
        }
        out.push_str(&el);
        out.push('\n');
    }
    let marks = &doc.page.reg_marks;
    if marks.enabled {
        out.push_str("  <g id=\"marcas-registro\" fill=\"#000000\" stroke=\"none\">\n");
        for r in marks.mark_rects(doc.page.width_mm, doc.page.height_mm) {
            out.push_str(&format!(
                "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>\n",
                n(r.x0),
                n(r.y0),
                n(r.width()),
                n(r.height())
            ));
        }
        out.push_str("  </g>\n");
    }
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Rect;

    #[test]
    fn imports_physical_units_and_holes() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm" viewBox="0 0 200 100">
            <rect x="20" y="10" width="40" height="20" fill="#ff0000"/>
            <path fill-rule="evenodd" d="M0 0 H10 V10 H0 Z M3 3 H7 V7 H3 Z"/>
        </svg>"##;
        let r = import_svg(svg, Stroke::default()).unwrap();
        assert_eq!(r.shapes.len(), 2);
        let rect = &r.shapes[0];
        assert!((rect.rect.x0 - 10.0).abs() < 1e-3 && (rect.rect.width() - 20.0).abs() < 1e-3);
        assert_eq!(rect.fill, Some(Color::rgb(255, 0, 0)));
        assert_eq!(rect.stroke, Stroke::default());
        match &r.shapes[1].kind {
            ShapeKind::Path {
                subpaths,
                fill_rule,
            } => {
                assert_eq!(subpaths.len(), 2);
                assert_eq!(*fill_rule, FillRule::EvenOdd);
            }
            _ => panic!("esperaba un trazo"),
        }
    }

    #[test]
    fn imports_stroke_width_scaled_by_transform() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10mm">
            <g transform="scale(2)"><circle cx="2" cy="2" r="1" fill="none" stroke="#0000ff" stroke-width="0.5"/></g>
        </svg>"##;
        let r = import_svg(svg, Stroke::default()).unwrap();
        let s = &r.shapes[0];
        // 0.5 unidades × escala 2 = 1 unidad = 1 mm (10 unidades = 10 mm) ≈ 2.83 pt
        assert!(
            (s.stroke.width_pt - 2.83).abs() < 0.05,
            "{}",
            s.stroke.width_pt
        );
        assert!((s.rect.width() - 4.0).abs() < 0.05);
    }

    #[test]
    fn export_round_trips_through_import() {
        let mut doc = Document::default();
        let mut s = Shape::new(
            0,
            "caja",
            Rect::new(10.0, 20.0, 40.0, 50.0),
            ShapeKind::Rect { corner_radius: 0.0 },
        );
        s.fill = Some(Color::BLACK);
        doc.add_shape(s);
        let svg = export_svg(&doc);
        let back = import_svg(&svg, Stroke::default()).unwrap();
        let r = back.shapes[0].rect;
        assert!(
            (r.x0 - 10.0).abs() < 1e-3 && (r.x1 - 40.0).abs() < 1e-3 && (r.y1 - 50.0).abs() < 1e-3
        );
    }

    #[test]
    fn rejects_non_svg() {
        assert!(import_svg("<html></html>", Stroke::default()).is_err());
    }
}

// ---------------------------------------------------------------------------------------------
// Texto → contornos, con las fuentes del sistema (vía usvg).
// ---------------------------------------------------------------------------------------------

/// Subtrazos en mm: puntos y si el contorno está cerrado.
pub type WorldPaths = Vec<(Vec<Point>, bool)>;

/// Estilo de un bloque de texto a convertir en contornos.
#[derive(Debug, Clone)]
pub struct TextOutlineStyle<'a> {
    pub text: &'a str,
    pub font_family: &'a str,
    pub font_size_pt: f64,
    pub align: oc_core::TextAlign,
    pub bold: bool,
    pub italic: bool,
    pub line_height: f64,
    pub fx: oc_core::shape::TextFx,
}

/// Nombre de familia entre comillas para CSS (los nombres que empiezan por número o llevan
/// espacios no son identificadores válidos sin comillas).
fn css_family(name: &str) -> String {
    format!("'{}'", name.replace('\'', ""))
}

/// Espaciado extra entre letras (mm) para un porcentaje de la referencia (100 % = normal).
/// El 100 % equivale al avance medio de un carácter (≈ medio cuerpo).
pub fn letter_spacing_mm(char_spacing_pct: f64, size_mm: f64) -> f64 {
    (char_spacing_pct / 100.0 - 1.0) * size_mm * 0.5
}

/// Contornos de cada línea (índice, trazos) con un espaciado extra por línea (mm).
fn render_lines(
    style: &TextOutlineStyle,
    lines: &[&str],
    extra: &[f64],
) -> Result<Vec<(usize, WorldPaths)>, SvgError> {
    let size_mm = pt_to_mm(style.font_size_pt);
    let line_mm = size_mm * style.line_height;
    let base = letter_spacing_mm(style.fx.char_spacing, size_mm);
    let mut body = String::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let spacing = base + extra.get(i).copied().unwrap_or(0.0);
        let attrs = format!(
            "font-family=\"{}\" font-size=\"{}\"{}{}{}",
            esc(&css_family(style.font_family)),
            n(size_mm),
            if style.bold {
                " font-weight=\"bold\""
            } else {
                ""
            },
            if style.italic {
                " font-style=\"italic\""
            } else {
                ""
            },
            if style.fx.kerning {
                ""
            } else {
                " style=\"font-kerning:none\""
            },
        );
        if style.fx.vertical {
            // Columnas hacia la derecha; letras derechas apiladas y centradas en la columna.
            let x = size_mm * 0.5 + line_mm * i as f64;
            let advance = size_mm * 0.95 + spacing;
            for (j, ch) in line.chars().enumerate() {
                if ch.is_whitespace() {
                    continue;
                }
                body.push_str(&format!(
                    "<text id=\"line-{i}-c{j}\" x=\"{}\" y=\"{}\" text-anchor=\"middle\" {attrs} xml:space=\"preserve\">{}</text>\n",
                    n(x),
                    n(size_mm * 0.8 + advance * j as f64),
                    esc(&ch.to_string())
                ));
            }
        } else {
            body.push_str(&format!(
                "<text id=\"line-{i}\" x=\"0\" y=\"{}\" {attrs}{} xml:space=\"preserve\">{}</text>\n",
                n(size_mm * 0.8 + line_mm * i as f64),
                if spacing.abs() > 1e-9 { format!(" letter-spacing=\"{}\"", n(spacing)) } else { String::new() },
                esc(line)
            ));
        }
    }
    if body.is_empty() {
        return Ok(Vec::new());
    }
    // 1 unidad = 1 mm (ancho físico igual al viewBox).
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1000mm\" height=\"1000mm\" viewBox=\"0 0 1000 1000\">\n{body}</svg>"
    );
    let tree = usvg::Tree::from_str(&svg, &usvg_options())?;
    fn collect(group: &usvg::Group, out: &mut Vec<(Vec<Point>, bool)>) {
        for node in group.children() {
            match node {
                usvg::Node::Group(g) => collect(g, out),
                usvg::Node::Text(t) => collect(t.flattened(), out),
                usvg::Node::Path(p) => out.extend(flatten_path(p)),
                usvg::Node::Image(_) => {}
            }
        }
    }
    fn visit(node: &usvg::Node, out: &mut Vec<(usize, WorldPaths)>) {
        match node {
            usvg::Node::Text(t) => {
                // «line-3» o «line-3-c7» (una letra de una columna vertical).
                let idx: usize = t
                    .id()
                    .strip_prefix("line-")
                    .and_then(|s| s.split('-').next())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let mut paths = Vec::new();
                collect(t.flattened(), &mut paths);
                match out.iter_mut().find(|(i, _)| *i == idx) {
                    Some((_, acc)) => acc.extend(paths),
                    None => out.push((idx, paths)),
                }
            }
            // usvg puede envolver el texto en un grupo; buscamos dentro.
            usvg::Node::Group(g) => {
                for inner in g.children() {
                    visit(inner, out);
                }
            }
            _ => {}
        }
    }
    let mut per_line = Vec::new();
    for node in tree.root().children() {
        visit(node, &mut per_line);
    }
    Ok(per_line)
}

/// Extremos horizontales de unos trazos.
fn x_extent(paths: &WorldPaths) -> (f64, f64) {
    paths
        .iter()
        .flat_map(|(pts, _)| pts.iter().map(|p| p.x))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| {
            (a.min(x), b.max(x))
        })
}

/// Contornos del texto en mm, con el bloque anclado en (0, 0) (esquina superior izquierda de la
/// caja de tinta). Devuelve subtrazos cerrados (agujeros incluidos) listos para `Shape`.
pub fn text_outlines(style: &TextOutlineStyle) -> Result<Vec<(Vec<Point>, bool)>, SvgError> {
    let lines: Vec<&str> = style.text.split('\n').collect();
    let mut per_line = render_lines(style, &lines, &[])?;
    let horizontal = !style.fx.vertical;
    // Justificado: segunda pasada con espaciado extra para que cada línea (salvo la última)
    // ocupe el ancho de la más larga.
    if horizontal && style.align == oc_core::TextAlign::Justify && per_line.len() > 1 {
        let widths: Vec<(usize, f64)> = per_line
            .iter()
            .map(|(i, p)| {
                let (a, b) = x_extent(p);
                (*i, b - a)
            })
            .collect();
        let block_w = widths.iter().map(|w| w.1).fold(0.0, f64::max);
        let last = widths.iter().map(|w| w.0).max().unwrap_or(0);
        let mut extra = vec![0.0; lines.len()];
        for (i, w) in widths {
            let gaps = lines[i].chars().count().saturating_sub(1);
            if i != last && gaps > 0 {
                extra[i] = (block_w - w) / gaps as f64;
            }
        }
        per_line = render_lines(style, &lines, &extra)?;
    }
    let widths: Vec<(f64, f64)> = per_line.iter().map(|(_, p)| x_extent(p)).collect();
    let block_min = widths.iter().map(|w| w.0).fold(f64::INFINITY, f64::min);
    let block_max = widths.iter().map(|w| w.1).fold(f64::NEG_INFINITY, f64::max);
    let block_w = block_max - block_min;
    let mut all: Vec<(Vec<Point>, bool)> = Vec::new();
    let size_mm = pt_to_mm(style.font_size_pt);
    let line_mm = size_mm * style.line_height;
    for ((idx, paths), (lmin, lmax)) in per_line.into_iter().zip(widths) {
        // Alineación horizontal relativa a la línea más ancha (en vertical no se desplaza).
        let shift = if !horizontal {
            0.0
        } else {
            match style.align {
                oc_core::TextAlign::Left | oc_core::TextAlign::Justify => 0.0,
                oc_core::TextAlign::Center => (block_w - (lmax - lmin)) / 2.0 - (lmin - block_min),
                oc_core::TextAlign::Right => block_w - (lmax - lmin) - (lmin - block_min),
            }
        };
        for (pts, closed) in paths {
            all.push((
                pts.into_iter()
                    .map(|p| Point::new(p.x + shift, p.y))
                    .collect(),
                closed,
            ));
        }
        // Subrayado: barra bajo la línea base, del ancho de la línea.
        if style.fx.underline && horizontal && lmax > lmin {
            let y0 = size_mm * 0.8 + line_mm * idx as f64 + size_mm * 0.1;
            let y1 = y0 + size_mm * 0.06;
            let (x0, x1) = (lmin + shift, lmax + shift);
            all.push((
                vec![
                    Point::new(x0, y0),
                    Point::new(x1, y0),
                    Point::new(x1, y1),
                    Point::new(x0, y1),
                ],
                true,
            ));
        }
    }
    // Anclar el bloque en (0,0).
    let min_x = all
        .iter()
        .flat_map(|(p, _)| p.iter().map(|q| q.x))
        .fold(f64::INFINITY, f64::min);
    let min_y = all
        .iter()
        .flat_map(|(p, _)| p.iter().map(|q| q.y))
        .fold(f64::INFINITY, f64::min);
    for (pts, _) in &mut all {
        for p in pts.iter_mut() {
            p.x -= min_x;
            p.y -= min_y;
        }
    }
    Ok(all)
}

/// Familias de fuentes instaladas en el sistema, ordenadas y sin repetir.
pub fn system_font_families() -> Vec<String> {
    let opt = usvg_options();
    let mut names: Vec<String> = opt
        .fontdb
        .faces()
        .flat_map(|f| f.families.iter().map(|(name, _)| name.clone()))
        // Las familias con punto inicial son internas del sistema (no seleccionables).
        .filter(|n| !n.starts_with('.') && !n.trim().is_empty())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

#[cfg(test)]
mod print_tests {
    use super::*;
    use oc_core::shape::ShapeKind;

    #[test]
    fn print_svg_drops_cut_lines_and_adds_marks() {
        let mut doc = Document::default();
        let cut = oc_core::Shape::new(
            0,
            "corte",
            kurbo::Rect::new(40.0, 40.0, 60.0, 60.0),
            ShapeKind::Ellipse,
        );
        let mut ink = oc_core::Shape::new(
            0,
            "tinta",
            kurbo::Rect::new(80.0, 80.0, 100.0, 100.0),
            ShapeKind::Ellipse,
        );
        ink.fill = Some(oc_core::Color::rgb(255, 0, 0));
        doc.add_shape(cut);
        doc.add_shape(ink);
        doc.page.reg_marks.enabled = true;
        doc.page.reg_marks.bleed = true;
        let svg = print_svg(&doc);
        assert_eq!(
            svg.matches("<ellipse").count() + svg.matches("<path").count(),
            1,
            "{svg}"
        );
        assert!(svg.contains("marcas-registro"));
        // PDF a tamaño real: A4 = 595 × 842 pt.
        let pdf = print_pdf(&doc).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(pdf.starts_with(b"%PDF"));
        assert!(
            text.contains("595.27") && text.contains("841.88"),
            "MediaBox"
        );
        assert_eq!(svg.matches("<rect").count(), 5);
        // Sangrado: trazo rojo alrededor del relleno.
        assert!(svg.contains("stroke=\"#ff0000\""), "{svg}");
    }
}

#[cfg(test)]
mod text_tests {
    use super::*;

    #[test]
    fn text_outlines_have_ink_and_anchor_at_origin() {
        let style = TextOutlineStyle {
            text: "Hola",
            font_family: "Helvetica",
            font_size_pt: 36.0,
            align: oc_core::TextAlign::Left,
            bold: false,
            italic: false,
            line_height: 1.2,
            fx: Default::default(),
        };
        let paths = text_outlines(&style).unwrap();
        assert!(paths.len() >= 4, "{}", paths.len());
        let min_x = paths
            .iter()
            .flat_map(|(p, _)| p.iter().map(|q| q.x))
            .fold(f64::INFINITY, f64::min);
        let max_y = paths
            .iter()
            .flat_map(|(p, _)| p.iter().map(|q| q.y))
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(min_x.abs() < 1e-6);
        // 36 pt ≈ 12.7 mm de cuerpo; la tinta de "Hola" mide del orden de 9 mm de alto.
        assert!(max_y > 6.0 && max_y < 14.0, "{max_y}");
    }

    fn ink_size(style: &TextOutlineStyle) -> (f64, f64) {
        let paths = text_outlines(style).unwrap();
        let b =
            oc_core::edit::bounds_of(paths.iter().flat_map(|(p, _)| p.iter().copied())).unwrap();
        (b.width(), b.height())
    }

    #[test]
    fn text_fx_change_the_outlines() {
        let base = TextOutlineStyle {
            text: "AVAV",
            font_family: "Helvetica",
            font_size_pt: 36.0,
            align: oc_core::TextAlign::Left,
            bold: false,
            italic: false,
            line_height: 1.2,
            fx: Default::default(),
        };
        let (w0, h0) = ink_size(&base);
        let wide = TextOutlineStyle {
            fx: oc_core::shape::TextFx {
                char_spacing: 200.0,
                ..Default::default()
            },
            ..base
        };
        assert!(ink_size(&wide).0 > w0 + 10.0, "espaciado");
        let vertical = TextOutlineStyle {
            fx: oc_core::shape::TextFx {
                vertical: true,
                ..Default::default()
            },
            ..base
        };
        let (vw, vh) = ink_size(&vertical);
        assert!(vh > vw && vh > h0 * 2.0, "vertical {vw}×{vh}");
        let no_kern = TextOutlineStyle {
            fx: oc_core::shape::TextFx {
                kerning: false,
                ..Default::default()
            },
            ..base
        };
        assert!(
            ink_size(&no_kern).0 > w0 + 0.3,
            "interletraje {} vs {w0}",
            ink_size(&no_kern).0
        );
        let under = TextOutlineStyle {
            fx: oc_core::shape::TextFx {
                underline: true,
                ..Default::default()
            },
            ..base
        };
        assert!(ink_size(&under).1 > h0, "subrayado");
    }

    #[test]
    fn justify_stretches_lines_to_block_width() {
        let style = TextOutlineStyle {
            text: "IIII\nWWWWWW\nI",
            font_family: "Helvetica",
            font_size_pt: 24.0,
            align: oc_core::TextAlign::Justify,
            bold: false,
            italic: false,
            line_height: 1.4,
            fx: Default::default(),
        };
        let paths = text_outlines(&style).unwrap();
        // La primera línea (y pequeña) llega al borde derecho del bloque.
        let block_max = paths
            .iter()
            .flat_map(|(p, _)| p.iter().map(|q| q.x))
            .fold(0.0, f64::max);
        let first_max = paths
            .iter()
            .filter(|(p, _)| p.iter().all(|q| q.y < 9.0))
            .flat_map(|(p, _)| p.iter().map(|q| q.x))
            .fold(0.0, f64::max);
        assert!(
            (block_max - first_max).abs() < 1.0,
            "{first_max} de {block_max}"
        );
    }

    #[test]
    fn centered_lines_share_a_center() {
        let style = TextOutlineStyle {
            text: "I\nWWW",
            font_family: "Helvetica",
            font_size_pt: 24.0,
            align: oc_core::TextAlign::Center,
            bold: false,
            italic: false,
            line_height: 1.2,
            fx: Default::default(),
        };
        let paths = text_outlines(&style).unwrap();
        let (mut top, mut bottom): (Vec<f64>, Vec<f64>) = (vec![], vec![]);
        for (pts, _) in &paths {
            let y = pts.iter().map(|p| p.y).sum::<f64>() / pts.len() as f64;
            let xs = pts.iter().map(|p| p.x);
            if y < 9.0 {
                top.extend(xs)
            } else {
                bottom.extend(xs)
            }
        }
        let center = |v: &Vec<f64>| {
            (v.iter().cloned().fold(f64::INFINITY, f64::min)
                + v.iter().cloned().fold(f64::NEG_INFINITY, f64::max))
                / 2.0
        };
        assert!(
            (center(&top) - center(&bottom)).abs() < 0.8,
            "{} vs {}",
            center(&top),
            center(&bottom)
        );
    }

    #[test]
    fn lists_system_fonts() {
        assert!(system_font_families().len() > 5);
    }
}

#[cfg(test)]
mod gradient_svg_tests {
    use super::*;
    use kurbo::Rect;

    #[test]
    fn gradient_round_trips_through_svg() {
        let mut doc = Document::default();
        let mut s = Shape::new(
            0,
            "caja",
            Rect::new(0.0, 0.0, 40.0, 20.0),
            ShapeKind::Rect { corner_radius: 0.0 },
        );
        s.fill = Some(Color::rgb(255, 0, 0));
        s.fill_gradient = Some(oc_core::shape::Gradient::two(
            Color::rgb(255, 0, 0),
            Color::rgb(0, 0, 255),
        ));
        s.fill_opacity = 0.5;
        doc.add_shape(s);
        let svg = export_svg(&doc);
        assert!(svg.contains("linearGradient"), "{svg}");
        let back = import_svg(&svg, Stroke::default()).unwrap();
        let g = back.shapes[0]
            .fill_gradient
            .as_ref()
            .expect("degradado importado");
        assert_eq!(g.stops.len(), 2);
        assert!(g.angle_deg.abs() < 1e-6);
        assert!((back.shapes[0].fill_opacity - 0.5).abs() < 1e-3);
    }
}

#[cfg(test)]
mod stroke_svg_tests {
    use super::*;
    use kurbo::Rect;

    #[test]
    fn exports_join_cap_inside_and_hairline() {
        let mut doc = Document::default();
        let mut s = Shape::new(
            0,
            "a",
            Rect::new(0.0, 0.0, 10.0, 10.0),
            ShapeKind::Rect { corner_radius: 0.0 },
        );
        s.stroke.width_pt = 2.0;
        s.stroke.join = oc_core::shape::LineJoin::Round;
        s.stroke.cap = oc_core::shape::LineCap::Square;
        s.stroke.align = oc_core::shape::StrokeAlign::Inside;
        doc.add_shape(s);
        let mut h = Shape::new(0, "b", Rect::new(20.0, 0.0, 30.0, 10.0), ShapeKind::Ellipse);
        h.stroke.print_hairline = true;
        doc.add_shape(h);
        let svg = export_svg(&doc);
        assert!(svg.contains("stroke-linejoin=\"round\""), "{svg}");
        assert!(svg.contains("stroke-linecap=\"square\""));
        assert!(svg.contains("clipPath id=\"oc-clip-1\""));
        assert!(svg.contains(&format!("stroke-width=\"{}\"", n(pt_to_mm(0.1)))));
        // Sigue siendo un SVG válido que podemos volver a importar.
        assert_eq!(import_svg(&svg, Stroke::default()).unwrap().shapes.len(), 2);
    }
}

/// Letra colocada de una línea de texto: posición del lápiz y avance (mm) y contorno en mm
/// con el origen en el lápiz sobre la línea base (y hacia abajo).
#[derive(Debug, Clone)]
pub struct PlacedGlyph {
    pub ch: char,
    pub x_mm: f64,
    pub advance_mm: f64,
    pub outline: Vec<oc_core::nodes::NodePath>,
}

/// Línea de texto letra a letra, con el ascendente y el descendente de la fuente (mm).
#[derive(Debug, Clone)]
pub struct GlyphRun {
    pub ascent_mm: f64,
    pub descent_mm: f64,
    pub glyphs: Vec<PlacedGlyph>,
}

/// Recoge el contorno de una letra como trazados de nodos (las cuadráticas pasan a cúbicas).
struct NodeOutline {
    scale: f64,
    paths: Vec<oc_core::nodes::NodePath>,
}

impl NodeOutline {
    fn pt(&self, x: f32, y: f32) -> Point {
        Point::new(x as f64 * self.scale, -(y as f64) * self.scale)
    }

    fn current(&mut self) -> &mut Vec<oc_core::nodes::Node> {
        &mut self.paths.last_mut().expect("move_to antes").nodes
    }
}

impl ttf_parser::OutlineBuilder for NodeOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.paths.push(oc_core::nodes::NodePath {
            nodes: vec![oc_core::nodes::Node::line(p)],
            closed: false,
        });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.current().push(oc_core::nodes::Node::line(p));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let q = self.pt(x1, y1);
        let p = self.pt(x, y);
        let nodes = self.current();
        let last = nodes.last_mut().expect("nodo anterior");
        let p0 = last.p;
        last.h_out = Some(p0 + (q - p0) * (2.0 / 3.0));
        nodes.push(oc_core::nodes::Node {
            p,
            h_in: Some(p + (q - p) * (2.0 / 3.0)),
            h_out: None,
            smooth: false,
        });
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1, c2, p) = (self.pt(x1, y1), self.pt(x2, y2), self.pt(x, y));
        let nodes = self.current();
        nodes.last_mut().expect("nodo anterior").h_out = Some(c1);
        nodes.push(oc_core::nodes::Node {
            p,
            h_in: Some(c2),
            h_out: None,
            smooth: false,
        });
    }

    fn close(&mut self) {
        let Some(np) = self.paths.last_mut() else {
            return;
        };
        np.closed = true;
        // El último nodo repite el primero: se fusionan (su manejador de entrada pasa al primero).
        if np.nodes.len() > 1 {
            let (first, last) = (np.nodes[0].p, np.nodes[np.nodes.len() - 1].p);
            if (first - last).hypot() < 1e-9 {
                let last = np.nodes.pop().expect("hay nodos");
                np.nodes[0].h_in = last.h_in;
            }
        }
    }
}

/// Letras de una línea de texto con la fuente del sistema, sin interletrado ni kerning (como
/// las guarda la referencia). `None` si la fuente no se encuentra o hay saltos de línea.
pub fn glyph_run(
    family: &str,
    size_pt: f64,
    bold: bool,
    italic: bool,
    text: &str,
) -> Option<GlyphRun> {
    if text.contains('\n') || text.is_empty() {
        return None;
    }
    let opt = usvg_options();
    let db = opt.fontdb.as_ref();
    let query = usvg::fontdb::Query {
        families: &[usvg::fontdb::Family::Name(family)],
        weight: if bold {
            usvg::fontdb::Weight::BOLD
        } else {
            usvg::fontdb::Weight::NORMAL
        },
        style: if italic {
            usvg::fontdb::Style::Italic
        } else {
            usvg::fontdb::Style::Normal
        },
        ..Default::default()
    };
    let id = db.query(&query)?;
    db.with_face_data(id, |data, index| {
        let face = ttf_parser::Face::parse(data, index).ok()?;
        let scale = pt_to_mm(size_pt) / face.units_per_em() as f64;
        let mut x = 0.0;
        let mut glyphs = Vec::new();
        for ch in text.chars() {
            let gid = face.glyph_index(ch)?;
            let advance = face.glyph_hor_advance(gid).unwrap_or(0) as f64 * scale;
            let mut b = NodeOutline {
                scale,
                paths: Vec::new(),
            };
            face.outline_glyph(gid, &mut b);
            glyphs.push(PlacedGlyph {
                ch,
                x_mm: x,
                advance_mm: advance,
                outline: b.paths,
            });
            x += advance;
        }
        Some(GlyphRun {
            ascent_mm: face.ascender() as f64 * scale,
            descent_mm: -(face.descender() as f64) * scale,
            glyphs,
        })
    })?
}

#[cfg(test)]
mod glyph_tests {
    #[test]
    fn arial_metrics_match_reference() {
        // La referencia guarda «Abc» en Arial a 72 pt: avances 16,94 / 14,13 / 12,70 mm y caja
        // de ascendente + descendente de 28,375 mm.
        let Some(run) = super::glyph_run("Arial", 72.0, false, false, "Abc") else {
            return; // Sin Arial en el sistema.
        };
        let adv: Vec<f64> = run.glyphs.iter().map(|g| g.advance_mm).collect();
        assert!((adv[0] - 16.94).abs() < 0.02, "{adv:?}");
        assert!((adv[1] - 14.13).abs() < 0.02, "{adv:?}");
        assert!((adv[2] - 12.70).abs() < 0.02, "{adv:?}");
        assert!((run.ascent_mm + run.descent_mm - 28.375).abs() < 0.05);
        assert!(
            run.glyphs[0].outline.len() == 2,
            "la A tiene contorno y agujero"
        );
    }
}
