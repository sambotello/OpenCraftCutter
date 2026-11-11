//! Textos del documento: contornos con las fuentes del sistema (vía usvg), medidos y cacheados.

use kurbo::{Point, Rect};
use oc_core::shape::{Shape, ShapeKind, TextAlign};
use oc_svg::{TextOutlineStyle, text_outlines};
use std::collections::HashMap;

pub const DEFAULT_FONT_FAMILY: &str = "Helvetica Neue";

/// Contornos de un texto en mm, compartidos entre caché y dibujo.
pub type Outlines = std::sync::Arc<Vec<(Vec<Point>, bool)>>;

pub struct TextStyle<'a> {
    pub text: &'a str,
    pub font_family: &'a str,
    pub font_size_pt: f64,
    pub align: TextAlign,
    pub bold: bool,
    pub italic: bool,
    pub line_height: f64,
    pub fx: oc_core::shape::TextFx,
}

impl<'a> TextStyle<'a> {
    pub fn from_kind(kind: &'a ShapeKind) -> Option<TextStyle<'a>> {
        match kind {
            ShapeKind::Text {
                text,
                font_family,
                font_size_pt,
                align,
                bold,
                italic,
                line_height,
                fx,
            } => Some(TextStyle {
                text,
                font_family,
                font_size_pt: *font_size_pt,
                align: *align,
                bold: *bold,
                italic: *italic,
                line_height: *line_height,
                fx: *fx,
            }),
            _ => None,
        }
    }

    fn key(&self) -> String {
        format!(
            "{}|{}|{}|{:?}|{}|{}|{}|{:?}",
            self.text,
            self.font_family,
            self.font_size_pt,
            self.align,
            self.bold,
            self.italic,
            self.line_height,
            self.fx
        )
    }

    fn outline_style(&self) -> TextOutlineStyle<'_> {
        TextOutlineStyle {
            text: self.text,
            font_family: self.font_family,
            font_size_pt: self.font_size_pt,
            align: self.align,
            bold: self.bold,
            italic: self.italic,
            line_height: self.line_height,
            fx: self.fx,
        }
    }
}

/// Contornos de un texto anclados en (0,0), en mm. Vacío si no hay tinta o la fuente falla.
/// Con la fuente de un trazo son líneas abiertas (para rotulador).
pub fn outlines(style: &TextStyle) -> Vec<(Vec<Point>, bool)> {
    if style.font_family == oc_core::singleline::FAMILY {
        return oc_core::singleline::text_strokes(
            style.text,
            oc_core::units::pt_to_mm(style.font_size_pt),
            style.line_height,
        );
    }
    text_outlines(&style.outline_style()).unwrap_or_default()
}

fn bounds(paths: &[(Vec<Point>, bool)]) -> Option<Rect> {
    oc_core::edit::bounds_of(paths.iter().flat_map(|(p, _)| p.iter().copied()))
}

/// Tamaño de la caja de tinta en mm. Un texto vacío ocupa un cuerpo mínimo para poder seleccionarlo.
pub fn measure(style: &TextStyle) -> (f64, f64) {
    let size_mm = oc_core::units::pt_to_mm(style.font_size_pt);
    match bounds(&outlines(style)) {
        Some(b) if b.width() > 0.0 && b.height() > 0.0 => (b.width(), b.height()),
        _ => (size_mm * 0.6, size_mm * 0.8),
    }
}

/// Caché de contornos por forma de texto (clave = texto + estilo).
#[derive(Default)]
pub struct TextCache {
    entries: HashMap<u64, (String, Outlines)>,
}

impl TextCache {
    /// Contornos del texto `s` escalados y colocados en su caja (mm, sin rotar).
    pub fn world_outlines(&mut self, s: &Shape) -> Option<Outlines> {
        let style = TextStyle::from_kind(&s.kind)?;
        let key = format!(
            "{}|{:?}",
            style.key(),
            (s.rect.x0, s.rect.y0, s.rect.width(), s.rect.height())
        );
        if let Some((k, paths)) = self.entries.get(&s.id) {
            if *k == key {
                return Some(paths.clone());
            }
        }
        let raw = outlines(&style);
        let fitted = fit_to_rect(&raw, s.rect);
        let arc = std::sync::Arc::new(fitted);
        self.entries.insert(s.id, (key, arc.clone()));
        Some(arc)
    }

    pub fn retain(&mut self, ids: &std::collections::HashSet<u64>) {
        self.entries.retain(|id, _| ids.contains(id));
    }
}

/// Escala contornos anclados en (0,0) para que su caja coincida con `rect`.
pub fn fit_to_rect(paths: &[(Vec<Point>, bool)], rect: Rect) -> Vec<(Vec<Point>, bool)> {
    let Some(b) = bounds(paths) else {
        return Vec::new();
    };
    let sx = if b.width() > 0.0 {
        rect.width() / b.width()
    } else {
        1.0
    };
    let sy = if b.height() > 0.0 {
        rect.height() / b.height()
    } else {
        1.0
    };
    paths
        .iter()
        .map(|(pts, c)| {
            (
                pts.iter()
                    .map(|p| Point::new(rect.x0 + (p.x - b.x0) * sx, rect.y0 + (p.y - b.y0) * sy))
                    .collect(),
                *c,
            )
        })
        .collect()
}

/// Convierte un texto en un trazo compuesto equivalente (misma caja, rotación y estilo).
pub fn text_to_path(s: &Shape) -> Option<Shape> {
    let style = TextStyle::from_kind(&s.kind)?;
    let fitted = fit_to_rect(&outlines(&style), s.rect);
    if fitted.is_empty() {
        return None;
    }
    let (rect, subpaths) = Shape::normalize_subpaths(&fitted);
    let mut out = Shape::new(
        0,
        format!("{} (trazado)", s.name),
        rect,
        // Regla de relleno no nula, como las fuentes: las letras que se tocan se ven unidas y
        // Fusionar las une en un solo contorno (con par-impar, el solape sería un agujero).
        ShapeKind::Path {
            subpaths,
            fill_rule: oc_core::FillRule::NonZero,
        },
    );
    out.rotation = s.rotation;
    out.fill = s.fill;
    out.stroke = s.stroke;
    out.locked = s.locked;
    out.visible = s.visible;
    Some(out)
}

/// Sustituye los textos de una lista por su trazado (los demás se devuelven tal cual).
pub fn paths_for_ops(shapes: &[Shape]) -> Vec<Shape> {
    shapes
        .iter()
        .map(|s| {
            if s.is_text() {
                text_to_path(s)
                    .map(|mut p| {
                        p.id = s.id;
                        p
                    })
                    .unwrap_or_else(|| s.clone())
            } else {
                s.clone()
            }
        })
        .collect()
}
