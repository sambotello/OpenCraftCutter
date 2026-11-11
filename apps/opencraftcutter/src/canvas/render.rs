//! Dibujo de la escena: tapete, página, rejilla, formas y reglas.

use super::Viewport;
use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Align2, Color32, FontId, Painter, Pos2, Shape as EShape, Stroke, StrokeKind};
use kurbo::{Point, Rect};
use lyon::path::Path as LPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule as LFillRule, FillTessellator, FillVertex, VertexBuffers,
};
use oc_core::edit::rotation_about;
use oc_core::machines::resolve_mat;
use oc_core::shape::{FillRule, LineStyle, Shape, ShapeKind};
use oc_core::units::{MM_PER_IN, pt_to_mm};

fn c32(c: oc_core::Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// Sube a la GPU las imágenes nuevas del documento y descarta las de formas eliminadas.
const LOGO_PNG: &[u8] = include_bytes!("../../assets/logo-with-text.png");
/// Ancho del logo sobre el tapete, en mm.
const LOGO_WIDTH_MM: f64 = 76.0;

fn ensure_logo(app: &mut OpenCraftCutterApp, ctx: &egui::Context) {
    if app.canvas.logo.is_some() {
        return;
    }
    if let Ok((mut rgba, w, h)) = oc_trace::decode_rgba(LOGO_PNG) {
        // Máscara: la tinta (oscura) se vuelve opaca y el fondo (claro) transparente, respetando
        // el alfa original; el color se pone en blanco para teñirlo al dibujar.
        for px in rgba.chunks_exact_mut(4) {
            let lum =
                (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) / 255.0;
            let ink = (1.0 - lum).clamp(0.0, 1.0);
            px[3] = (px[3] as f32 * ink).round() as u8;
            px[0] = 255;
            px[1] = 255;
            px[2] = 255;
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        let tex = ctx.load_texture("mat-logo", image, egui::TextureOptions::LINEAR);
        app.canvas.logo = Some((tex, w as f32 / h.max(1) as f32));
    }
}

pub fn ensure_textures(app: &mut OpenCraftCutterApp, ctx: &egui::Context) {
    ensure_logo(app, ctx);
    // Contornos de texto (caché por texto + estilo + caja).
    let text_ids: std::collections::HashSet<u64> = app
        .doc
        .shapes
        .iter()
        .filter(|s| s.is_text())
        .map(|s| s.id)
        .collect();
    app.canvas.text_cache.retain(&text_ids);
    app.canvas.text_outlines.clear();
    for s in &app.doc.shapes {
        if s.is_text() {
            if let Some(paths) = app.canvas.text_cache.world_outlines(s) {
                app.canvas.text_outlines.insert(s.id, paths);
            }
        }
    }
    let ids: std::collections::HashSet<u64> = app
        .doc
        .shapes
        .iter()
        .filter(|s| s.is_image())
        .map(|s| s.id)
        .collect();
    app.canvas.textures.retain(|id, _| ids.contains(id));
    for s in &app.doc.shapes {
        if let ShapeKind::Image { png, .. } = &s.kind {
            if app.canvas.textures.contains_key(&s.id) {
                continue;
            }
            match oc_trace::decode_rgba(&png.0) {
                Ok((rgba, w, h)) => {
                    let image =
                        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    let tex = ctx.load_texture(
                        format!("image-{}", s.id),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    app.canvas.textures.insert(s.id, tex);
                }
                Err(e) => log::warn!("imagen {} no decodificable: {e}", s.id),
            }
        }
    }
}

pub fn draw_scene(app: &OpenCraftCutterApp, painter: &Painter) {
    let vp = app.canvas.viewport;
    let page = &app.doc.page;
    let mat = resolve_mat(page.machine, page.mat);
    if mat.width_mm > 0.0 {
        draw_mat(painter, &vp, mat.width_mm, mat.height_mm);
        draw_mat_logo(app, painter, &vp, mat.width_mm, mat.height_mm);
    }
    draw_page(app, painter, &vp);
    for s in &app.doc.shapes {
        if app.canvas.editing_text == Some(s.id) {
            continue;
        }
        if let ShapeKind::Image { .. } = &s.kind {
            draw_image(app, painter, &vp, s);
        } else if s.is_text() {
            draw_text(app, painter, &vp, s);
        } else {
            draw_shape_cached(painter, &vp, s, Some(&app.canvas.stroke_cache));
        }
    }
    // Como la referencia, las líneas de corte se ven siempre por encima de las imágenes:
    // se vuelven a trazar (sin relleno) las de las formas que quedan debajo de alguna.
    if let Some(last_image) = app.doc.shapes.iter().rposition(|s| s.is_image()) {
        for s in app.doc.shapes[..last_image]
            .iter()
            .filter(|s| s.visible && !s.is_image() && !s.is_text())
        {
            let mut line = s.clone();
            line.fill = None;
            line.fill_gradient = None;
            draw_shape_cached(painter, &vp, &line, Some(&app.canvas.stroke_cache));
        }
    }
    // Rellenos de herramientas especiales en azul, como la referencia.
    let blue = Stroke::new(1.0, Color32::from_rgb(0x40, 0x40, 0xff));
    for s in app.doc.shapes.iter().filter(|s| s.visible) {
        let Some(f) = s.tool_fill else { continue };
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for v in [
                s.rect.x0,
                s.rect.y0,
                s.rect.x1,
                s.rect.y1,
                s.rotation,
                f.spacing_mm,
                f.angle_deg,
            ] {
                v.to_bits().hash(&mut h);
            }
            (f.pattern as u8).hash(&mut h);
            if let ShapeKind::Path { subpaths, .. } = &s.kind {
                subpaths.len().hash(&mut h);
            }
            h.finish()
        };
        let lines = {
            let mut cache = app.canvas.tool_fill_cache.borrow_mut();
            match cache.get(&s.id) {
                Some((k, l)) if *k == key => l.clone(),
                _ => {
                    let l = std::sync::Arc::new(oc_core::special::pattern_lines(
                        s,
                        f.pattern,
                        f.spacing_mm,
                        f.angle_deg,
                    ));
                    cache.insert(s.id, (key, l.clone()));
                    l
                }
            }
        };
        // Las líneas y contornos de `ops` ya vienen en mundo con la rotación aplicada.
        for l in lines.iter() {
            let pts: Vec<Pos2> = l
                .iter()
                .map(|p| vp.to_screen(Point::new(p[0], p[1])))
                .collect();
            painter.add(EShape::line(pts, blue));
        }
        // El contorno de una forma con relleno de herramienta también va en azul.
        for c in oc_core::ops::closed_contours(s) {
            let line: Vec<Pos2> = c
                .iter()
                .map(|p| vp.to_screen(Point::new(p[0], p[1])))
                .collect();
            painter.add(EShape::closed_line(line, blue));
        }
    }
    // Vista previa del desplazamiento: contorno en el color de la referencia, sin relleno.
    if app.panel == Some(crate::app::Panel::Offset) {
        for s in &app.canvas.offset_preview {
            for (pts, closed) in s.world_subpaths() {
                let rot = rotation_about(s.center(), s.rotation);
                let line: Vec<Pos2> = pts.iter().map(|p| vp.to_screen(rot * *p)).collect();
                let st = Stroke::new(1.0, theme::CUT_AREA);
                if closed {
                    painter.add(EShape::closed_line(line, st));
                } else {
                    painter.add(EShape::line(line, st));
                }
            }
        }
    }
}

/// Dibuja una textura sobre un rectángulo del mundo (girado `rotation` sobre su centro).
pub(crate) fn textured_quad(
    painter: &Painter,
    vp: &Viewport,
    tex: egui::TextureId,
    r: Rect,
    rotation: f64,
    tint: Color32,
) {
    let rot = rotation_about(r.center(), rotation);
    let corners = [
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y1),
    ];
    let uvs = [
        Pos2::new(0.0, 0.0),
        Pos2::new(1.0, 0.0),
        Pos2::new(1.0, 1.0),
        Pos2::new(0.0, 1.0),
    ];
    let mut mesh = Mesh::with_texture(tex);
    for (c, uv) in corners.iter().zip(uvs) {
        mesh.vertices.push(Vertex {
            pos: vp.to_screen(rot * *c),
            uv,
            color: tint,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(EShape::mesh(mesh));
}

fn draw_image(app: &OpenCraftCutterApp, painter: &Painter, vp: &Viewport, s: &Shape) {
    let Some(tex) = app.canvas.textures.get(&s.id) else {
        return;
    };
    textured_quad(painter, vp, tex.id(), s.rect, s.rotation, Color32::WHITE);
    // Vista previa del umbral de trazado (amarillo) mientras el panel Trazar está abierto y
    // la imagen sigue seleccionada (tras trazar, la selección pasa a las formas nuevas).
    if app.panel == Some(crate::app::Panel::Trace) && app.doc.selected.contains(&s.id) {
        if let Some((_, id, prev)) = &app.canvas.trace_preview {
            if *id == s.id {
                textured_quad(painter, vp, prev.id(), s.rect, s.rotation, Color32::WHITE);
            }
        }
    }
}

fn hair(color: Color32) -> Stroke {
    Stroke::new(1.0, color)
}

fn draw_mat(painter: &Painter, vp: &Viewport, w: f64, h: f64) {
    let border = 12.0;
    let top = 30.0;
    let outer = vp.rect_to_screen(Rect::new(-border, -top, w + border, h + border));
    painter.rect(
        outer,
        4.0 * vp.scale as f32,
        theme::MAT,
        hair(theme::MAT_BORDER),
        StrokeKind::Inside,
    );
    // Flecha de alimentación
    let cx = w / 2.0;
    let aw = 16.0;
    let pts = [
        Point::new(cx, -top + 6.0),
        Point::new(cx + aw / 2.0, -top + 14.0),
        Point::new(cx + aw / 4.0, -top + 14.0),
        Point::new(cx + aw / 4.0, -top + 22.0),
        Point::new(cx - aw / 4.0, -top + 22.0),
        Point::new(cx - aw / 4.0, -top + 14.0),
        Point::new(cx - aw / 2.0, -top + 14.0),
    ];
    painter.add(EShape::convex_polygon(
        pts.iter().map(|p| vp.to_screen(*p)).collect(),
        Color32::from_gray(0x4a),
        Stroke::NONE,
    ));
    // Rejilla de pulgadas
    let cols = (w / MM_PER_IN).round() as i32;
    let rows = (h / MM_PER_IN).round() as i32;
    let label = FontId::proportional((3.2 * vp.scale).clamp(6.0, 14.0) as f32);
    for i in 0..=cols {
        let x = i as f64 * MM_PER_IN;
        painter.line_segment(
            [
                vp.to_screen(Point::new(x, 0.0)),
                vp.to_screen(Point::new(x, h)),
            ],
            hair(theme::MAT_GRID),
        );
        if i > 0 {
            painter.text(
                vp.to_screen(Point::new(x - 1.2, -1.5)),
                Align2::RIGHT_BOTTOM,
                i.to_string(),
                label.clone(),
                theme::MAT_LABEL,
            );
        }
    }
    for j in 0..=rows {
        let y = j as f64 * MM_PER_IN;
        painter.line_segment(
            [
                vp.to_screen(Point::new(0.0, y)),
                vp.to_screen(Point::new(w, y)),
            ],
            hair(theme::MAT_GRID),
        );
        if j > 0 {
            painter.text(
                vp.to_screen(Point::new(-1.5, y - 1.2)),
                Align2::RIGHT_BOTTOM,
                j.to_string(),
                label.clone(),
                theme::MAT_LABEL,
            );
        }
    }
}

/// Logo en la esquina inferior derecha del tapete.
fn draw_mat_logo(app: &OpenCraftCutterApp, painter: &Painter, vp: &Viewport, w: f64, h: f64) {
    let Some((tex, aspect)) = &app.canvas.logo else {
        return;
    };
    let lw = LOGO_WIDTH_MM.min(w * 0.4);
    let lh = lw / *aspect as f64;
    textured_quad(
        painter,
        vp,
        tex.id(),
        Rect::new(w - 2.0 - lw, h - 2.5 - lh, w - 2.0, h - 2.5),
        0.0,
        theme::MAT_LABEL,
    );
}

fn draw_page(app: &OpenCraftCutterApp, painter: &Painter, vp: &Viewport) {
    let page = &app.doc.page;
    let (w, h) = (page.width_mm, page.height_mm);
    let r = vp.rect_to_screen(Rect::new(0.0, 0.0, w, h));
    // Sombra suave
    painter.rect_filled(
        r.translate(egui::vec2(0.0, 2.0)).expand(2.0),
        2.0,
        Color32::from_black_alpha(40),
    );
    let pc = page.page_color;
    let alpha = ((1.0 - page.page_transparency.clamp(0.0, 1.0)) * pc.a as f64).round() as u8;
    painter.rect_filled(
        r,
        0.0,
        Color32::from_rgba_unmultiplied(pc.r, pc.g, pc.b, alpha),
    );
    if page.show_grid && page.grid_spacing_mm > 0.0 {
        let primary = c32(page.grid_primary_color);
        let secondary = c32(page.grid_secondary_color);
        let divisions = page.grid_divisions.max(1);
        let div = divisions as f64;
        let minor = page.grid_spacing_mm / div;
        let show_minor = minor * vp.scale >= 4.0;
        match page.grid_style {
            oc_core::document::GridStyle::Square => {
                let mut i = 0;
                let mut x = 0.0;
                while x <= w + 1e-6 {
                    let major = i % divisions == 0;
                    if major || show_minor {
                        painter.line_segment(
                            [
                                vp.to_screen(Point::new(x, 0.0)),
                                vp.to_screen(Point::new(x, h)),
                            ],
                            hair(if major { primary } else { secondary }),
                        );
                    }
                    x += minor;
                    i += 1;
                }
                let mut j = 0;
                let mut y = 0.0;
                while y <= h + 1e-6 {
                    let major = j % divisions == 0;
                    if major || show_minor {
                        painter.line_segment(
                            [
                                vp.to_screen(Point::new(0.0, y)),
                                vp.to_screen(Point::new(w, y)),
                            ],
                            hair(if major { primary } else { secondary }),
                        );
                    }
                    y += minor;
                    j += 1;
                }
            }
            oc_core::document::GridStyle::Isometric => {
                // Verticales + diagonales a ±30° (triángulos equiláteros de lado = espaciado).
                let step_x = page.grid_spacing_mm * 3f64.sqrt() / 2.0;
                let mut i = 0;
                let mut x = 0.0;
                while x <= w + 1e-6 {
                    let major = i % divisions == 0;
                    if major || show_minor {
                        painter.line_segment(
                            [
                                vp.to_screen(Point::new(x, 0.0)),
                                vp.to_screen(Point::new(x, h)),
                            ],
                            hair(if major { primary } else { secondary }),
                        );
                    }
                    x += step_x / div;
                    i += 1;
                }
                let span = w * 30f64.to_radians().tan();
                let mut j = 0;
                let mut y = -span;
                while y <= h + span + 1e-6 {
                    let major = j % divisions == 0;
                    if major || show_minor {
                        for dir in [1.0, -1.0] {
                            if let Some((p, q)) = clip_segment_to_rect(
                                Point::new(0.0, y),
                                Point::new(w, y + dir * span),
                                Rect::new(0.0, 0.0, w, h),
                            ) {
                                painter.line_segment(
                                    [vp.to_screen(p), vp.to_screen(q)],
                                    hair(if major { primary } else { secondary }),
                                );
                            }
                        }
                    }
                    y += minor;
                    j += 1;
                }
            }
        }
    }
    if page.show_print_margins {
        let pm = 6.35;
        if w > 2.0 * pm && h > 2.0 * pm {
            let r = vp.rect_to_screen(Rect::new(pm, pm, w - pm, h - pm));
            let pts = [
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
                r.left_top(),
            ];
            painter.add(EShape::dashed_line(
                &pts,
                hair(Color32::from_gray(0x9a)),
                5.0,
                4.0,
            ));
        }
    }
    let marks = &page.reg_marks;
    if marks.enabled {
        draw_reg_marks(painter, vp, marks, w, h);
        if page.show_cut_area {
            painter.rect_stroke(
                vp.rect_to_screen(marks.cut_area(w, h)),
                0.0,
                hair(theme::CUT_AREA),
                StrokeKind::Middle,
            );
        }
        return;
    }
    let m = page.cut_margin_mm;
    if page.show_cut_area && m > 0.0 && w > 2.0 * m && h > 2.0 * m {
        painter.rect_stroke(
            vp.rect_to_screen(Rect::new(m, m, w - m, h - m)),
            0.0,
            hair(theme::CUT_AREA),
            StrokeKind::Middle,
        );
    }
}

/// Marcas de registro y sus zonas reservadas (rayadas, sin diseño).
fn draw_reg_marks(
    painter: &Painter,
    vp: &Viewport,
    marks: &oc_core::regmarks::RegMarks,
    w: f64,
    h: f64,
) {
    let hatch = Stroke::new(1.0, Color32::from_gray(0xc4));
    for z in marks.keep_out_zones(w, h) {
        let r = vp.rect_to_screen(z);
        let p = painter.with_clip_rect(r);
        p.rect_filled(r, 0.0, Color32::from_gray(0xf2));
        // Rayado cruzado cada 3 mm.
        let step = (3.0 * vp.scale as f32).max(4.0);
        let span = r.width() + r.height();
        let mut t = 0.0;
        while t <= span {
            p.line_segment(
                [
                    r.left_top() + egui::vec2(t, 0.0),
                    r.left_top() + egui::vec2(0.0, t),
                ],
                hatch,
            );
            p.line_segment(
                [
                    r.right_top() - egui::vec2(t, 0.0),
                    r.right_top() + egui::vec2(0.0, t),
                ],
                hatch,
            );
            t += step;
        }
    }
    for m in marks.mark_rects(w, h) {
        painter.rect_filled(vp.rect_to_screen(m), 0.0, Color32::BLACK);
    }
}

/// Grosor de línea en px: las líneas de corte (0 pt) se dibujan siempre finas.
fn stroke_px(s: &Shape, vp: &Viewport) -> f32 {
    let mm = pt_to_mm(s.stroke.width_pt);
    if mm < 0.02 {
        1.0
    } else {
        (mm * vp.scale).max(1.0) as f32
    }
}

fn stroke_polyline(
    painter: &Painter,
    pts: Vec<Pos2>,
    closed: bool,
    stroke: Stroke,
    style: LineStyle,
) {
    if pts.len() < 2 {
        return;
    }
    // Un trazo cerrado de dos nodos es una recta: egui no dibuja líneas cerradas de 2 puntos.
    let closed = closed && pts.len() > 2;
    match style {
        LineStyle::Solid => {
            painter.add(if closed {
                EShape::closed_line(pts, stroke)
            } else {
                EShape::line(pts, stroke)
            });
        }
        _ => {
            let mut p = pts;
            if closed {
                p.push(p[0]);
            }
            let unit = stroke.width.max(1.5);
            let (d, g) = style.pattern().unwrap_or((4.0, 3.0));
            painter.add(EShape::dashed_line(
                &p,
                stroke,
                unit * d as f32,
                unit * g as f32,
            ));
        }
    }
}

/// Polígono (mm, local) de un rectángulo con esquinas redondeadas.
fn rect_polygon(r: Rect, radius: f64) -> Vec<Point> {
    let rad = radius.min(r.width() / 2.0).min(r.height() / 2.0);
    if rad <= 0.0 {
        return vec![
            Point::new(r.x0, r.y0),
            Point::new(r.x1, r.y0),
            Point::new(r.x1, r.y1),
            Point::new(r.x0, r.y1),
        ];
    }
    let mut out = Vec::new();
    let corners = [
        (r.x1 - rad, r.y0 + rad, -90.0),
        (r.x1 - rad, r.y1 - rad, 0.0),
        (r.x0 + rad, r.y1 - rad, 90.0),
        (r.x0 + rad, r.y0 + rad, 180.0),
    ];
    for (cx, cy, start) in corners {
        for k in 0..=8 {
            let a = (start + 90.0 * k as f64 / 8.0_f64).to_radians();
            out.push(Point::new(cx + rad * a.cos(), cy + rad * a.sin()));
        }
    }
    out
}

fn ellipse_polygon(r: Rect, scale: f64) -> Vec<Point> {
    let c = r.center();
    let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
    let n = ((rx.max(ry) * scale * 0.8).clamp(24.0, 200.0)) as usize;
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            Point::new(c.x + rx * a.cos(), c.y + ry * a.sin())
        })
        .collect()
}

/// Cómo se pinta un relleno en pantalla.
pub enum FillPaint {
    Solid(Color32),
    Gradient {
        g: oc_core::shape::Gradient,
        origin: Pos2,
        dir: egui::Vec2,
        length: f32,
        opacity: f32,
    },
}

impl FillPaint {
    fn color_at(&self, p: Pos2) -> Color32 {
        match self {
            FillPaint::Solid(c) => *c,
            FillPaint::Gradient {
                g,
                origin,
                dir,
                length,
                opacity,
            } => {
                let t = ((p - *origin).dot(*dir) / length.max(1e-3) + 0.5) as f64;
                with_opacity(g.sample(t), *opacity)
            }
        }
    }

    fn t_at(&self, p: Pos2) -> f32 {
        match self {
            FillPaint::Solid(_) => 0.0,
            FillPaint::Gradient {
                origin,
                dir,
                length,
                ..
            } => (p - *origin).dot(*dir) / length.max(1e-3),
        }
    }
}

fn with_opacity(c: oc_core::Color, opacity: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        c.r,
        c.g,
        c.b,
        (c.a as f32 * opacity.clamp(0.0, 1.0)).round() as u8,
    )
}

/// Pintura del relleno de una forma, o `None` si no tiene relleno.
pub fn fill_paint(s: &Shape, vp: &Viewport) -> Option<FillPaint> {
    let f = s.fill?;
    let opacity = s.fill_opacity as f32;
    let Some(g) = &s.fill_gradient else {
        return Some(FillPaint::Solid(with_opacity(f, opacity)));
    };
    let r = s.rect;
    let a = g.angle_deg.to_radians();
    let (sin, cos) = a.sin_cos();
    let length_mm = (r.width() * cos).abs() + (r.height() * sin).abs();
    let rot = rotation_about(r.center(), s.rotation);
    let c = rot * r.center();
    let tip = rot * (r.center() + kurbo::Vec2::new(cos, sin));
    let origin = vp.to_screen(c);
    let dir = (vp.to_screen(tip) - origin).normalized();
    Some(FillPaint::Gradient {
        g: g.clone(),
        origin,
        dir,
        length: (length_mm * vp.scale) as f32,
        opacity,
    })
}

/// Subdivide triángulos hasta que el degradado varíe poco en cada uno (la GPU interpola linealmente).
fn subdivide_for_gradient(paint: &FillPaint, positions: &[Pos2], indices: &[u32]) -> Mesh {
    let mut mesh = Mesh::default();
    fn emit(paint: &FillPaint, mesh: &mut Mesh, tri: [Pos2; 3], depth: u32) {
        let ts = tri.map(|p| paint.t_at(p));
        let span = ts.iter().cloned().fold(f32::MIN, f32::max)
            - ts.iter().cloned().fold(f32::MAX, f32::min);
        if span > 0.02 && depth < 10 {
            // Partir por el punto medio del lado más largo.
            let l = |i: usize, j: usize| tri[i].distance_sq(tri[j]);
            let (i, j, k) = [(0, 1, 2), (1, 2, 0), (2, 0, 1)]
                .into_iter()
                .max_by(|a, b| {
                    l(a.0, a.1)
                        .partial_cmp(&l(b.0, b.1))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or((0, 1, 2));
            let m = tri[i] + (tri[j] - tri[i]) * 0.5;
            emit(paint, mesh, [tri[i], m, tri[k]], depth + 1);
            emit(paint, mesh, [m, tri[j], tri[k]], depth + 1);
            return;
        }
        let base = mesh.vertices.len() as u32;
        for p in tri {
            mesh.vertices.push(Vertex {
                pos: p,
                uv: WHITE_UV,
                color: paint.color_at(p),
            });
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
    for t in indices.chunks_exact(3) {
        let tri = [
            positions[t[0] as usize],
            positions[t[1] as usize],
            positions[t[2] as usize],
        ];
        emit(paint, &mut mesh, tri, 0);
    }
    mesh
}

/// Rellena polilíneas (en pantalla) con teselación de lyon: admite cóncavos, agujeros y degradados.
pub fn fill_polygons(painter: &Painter, polys: &[Vec<Pos2>], rule: FillRule, paint: &FillPaint) {
    let mut builder = LPath::builder();
    for poly in polys {
        if poly.len() < 3 {
            continue;
        }
        builder.begin(lyon::math::point(poly[0].x, poly[0].y));
        for p in &poly[1..] {
            builder.line_to(lyon::math::point(p.x, p.y));
        }
        builder.close();
    }
    let path = builder.build();
    let mut buffers: VertexBuffers<Pos2, u32> = VertexBuffers::new();
    let mut tess = FillTessellator::new();
    let opts = FillOptions::tolerance(0.3).with_fill_rule(match rule {
        FillRule::NonZero => LFillRule::NonZero,
        FillRule::EvenOdd => LFillRule::EvenOdd,
    });
    let ok = tess
        .tessellate_path(
            &path,
            &opts,
            &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| {
                Pos2::new(v.position().x, v.position().y)
            }),
        )
        .is_ok();
    if !ok || buffers.indices.is_empty() {
        return;
    }
    let mesh = match paint {
        FillPaint::Solid(color) => Mesh {
            vertices: buffers
                .vertices
                .iter()
                .map(|p| Vertex {
                    pos: *p,
                    uv: WHITE_UV,
                    color: *color,
                })
                .collect(),
            indices: buffers.indices,
            texture_id: egui::TextureId::default(),
        },
        FillPaint::Gradient { .. } => {
            subdivide_for_gradient(paint, &buffers.vertices, &buffers.indices)
        }
    };
    painter.add(EShape::Mesh(std::sync::Arc::new(mesh)));
}

/// Caché de contornos de trazos gruesos: id → (huella de la geometría, polígonos en mm).
pub type StrokeCache = std::cell::RefCell<
    std::collections::HashMap<u64, (u64, std::sync::Arc<oc_core::ops::Polygons>)>,
>;

fn stroke_fingerprint(s: &Shape) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(&(&s.rect, s.rotation, &s.stroke, &s.kind))
        .unwrap_or_default()
        .hash(&mut h);
    h.finish()
}

/// Trazo de una forma: líneas finas con el pincel de egui; gruesas como polígono con vértices,
/// remates, posición y guiones exactos.
fn draw_stroke(
    painter: &Painter,
    vp: &Viewport,
    s: &Shape,
    screen: &[(Vec<Pos2>, bool)],
    extra: Option<&[(Vec<Point>, bool)]>,
    cache: Option<&StrokeCache>,
) {
    let color = with_opacity(s.stroke.color, s.stroke.opacity as f32);
    let px = stroke_px(s, vp);
    if s.stroke.width_pt <= 0.0 || px < 1.5 {
        let stroke = Stroke::new(px, color);
        for (pts, closed) in screen {
            stroke_polyline(painter, pts.clone(), *closed, stroke, s.stroke.style);
        }
        return;
    }
    let polys = match cache {
        Some(c) => {
            let fp = stroke_fingerprint(s);
            let hit = c
                .borrow()
                .get(&s.id)
                .filter(|(f, _)| *f == fp)
                .map(|(_, p)| p.clone());
            hit.unwrap_or_else(|| {
                let p = std::sync::Arc::new(oc_core::ops::stroke_polygons(s, extra));
                c.borrow_mut().insert(s.id, (fp, p.clone()));
                p
            })
        }
        None => std::sync::Arc::new(oc_core::ops::stroke_polygons(s, extra)),
    };
    let screen_polys: Vec<Vec<Pos2>> = polys
        .iter()
        .flatten()
        .map(|c| {
            c.iter()
                .map(|p| vp.to_screen(Point::new(p[0], p[1])))
                .collect()
        })
        .collect();
    if !screen_polys.is_empty() {
        fill_polygons(
            painter,
            &screen_polys,
            FillRule::NonZero,
            &FillPaint::Solid(color),
        );
    }
}

pub fn draw_shape(painter: &Painter, vp: &Viewport, s: &Shape) {
    draw_shape_cached(painter, vp, s, None);
}

pub fn draw_shape_cached(painter: &Painter, vp: &Viewport, s: &Shape, cache: Option<&StrokeCache>) {
    if !s.visible {
        return;
    }
    let rot = rotation_about(s.center(), s.rotation);
    let to_screen = |p: Point| vp.to_screen(rot * p);
    match &s.kind {
        ShapeKind::Rect { corner_radius } => {
            let pts: Vec<Pos2> = rect_polygon(s.rect, *corner_radius)
                .into_iter()
                .map(to_screen)
                .collect();
            if let Some(paint) = fill_paint(s, vp) {
                fill_polygons(
                    painter,
                    std::slice::from_ref(&pts),
                    FillRule::NonZero,
                    &paint,
                );
            }
            draw_stroke(painter, vp, s, &[(pts, true)], None, cache);
        }
        ShapeKind::Ellipse => {
            let pts: Vec<Pos2> = ellipse_polygon(s.rect, vp.scale)
                .into_iter()
                .map(to_screen)
                .collect();
            if let Some(paint) = fill_paint(s, vp) {
                fill_polygons(
                    painter,
                    std::slice::from_ref(&pts),
                    FillRule::NonZero,
                    &paint,
                );
            }
            draw_stroke(painter, vp, s, &[(pts, true)], None, cache);
        }
        ShapeKind::Path { fill_rule, .. } => {
            let subpaths = s.world_subpaths();
            let screen: Vec<(Vec<Pos2>, bool)> = subpaths
                .iter()
                .map(|(pts, closed)| (pts.iter().map(|p| to_screen(*p)).collect(), *closed))
                .collect();
            if let Some(paint) = fill_paint(s, vp) {
                let closed: Vec<Vec<Pos2>> = screen
                    .iter()
                    .filter(|(_, c)| *c)
                    .map(|(p, _)| p.clone())
                    .collect();
                if !closed.is_empty() {
                    fill_polygons(painter, &closed, *fill_rule, &paint);
                }
            }
            draw_stroke(painter, vp, s, &screen, None, cache);
        }
        ShapeKind::Text { .. } => {}
        ShapeKind::Image { .. } => {}
    }
}

fn draw_text(app: &OpenCraftCutterApp, painter: &Painter, vp: &Viewport, s: &Shape) {
    let Some(paths) = app.canvas.text_outlines.get(&s.id) else {
        return;
    };
    let rot = rotation_about(s.center(), s.rotation);
    let to_screen = |p: Point| vp.to_screen(rot * p);
    let screen: Vec<(Vec<Pos2>, bool)> = paths
        .iter()
        .map(|(pts, closed)| (pts.iter().map(|p| to_screen(*p)).collect(), *closed))
        .collect();
    if let Some(paint) = fill_paint(s, vp) {
        let closed: Vec<Vec<Pos2>> = screen
            .iter()
            .filter(|(_, c)| *c)
            .map(|(p, _)| p.clone())
            .collect();
        if !closed.is_empty() {
            fill_polygons(painter, &closed, FillRule::NonZero, &paint);
        }
    }
    draw_stroke(
        painter,
        vp,
        s,
        &screen,
        Some(paths.as_slice()),
        Some(&app.canvas.stroke_cache),
    );
    if paths.is_empty() {
        // Texto sin tinta (vacío): caja punteada para poder encontrarlo.
        let pts: Vec<Pos2> = rect_polygon(s.rect, 0.0)
            .into_iter()
            .map(to_screen)
            .collect();
        stroke_polyline(
            painter,
            pts,
            true,
            Stroke::new(1.0, theme::CHROME_300),
            LineStyle::Dashed,
        );
    }
}

const RULER_STEPS: [f64; 11] = [0.1, 0.2, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0];

pub fn draw_rulers(app: &OpenCraftCutterApp, painter: &Painter, full: egui::Rect) {
    let vp = app.canvas.viewport;
    let unit = app.doc.page.unit;
    let f = unit.factor_mm();
    let px_per_unit = f * vp.scale;
    let major = RULER_STEPS
        .iter()
        .copied()
        .find(|s| s * px_per_unit >= 48.0)
        .unwrap_or(100.0);
    let subdiv = if unit == oc_core::units::Unit::In {
        4.0
    } else if major * px_per_unit / 5.0 >= 6.0 {
        5.0
    } else {
        2.0
    };
    let minor = major / subdiv;
    let size = super::RULER_PX;
    let top = egui::Rect::from_min_max(
        Pos2::new(full.min.x + size, full.min.y),
        Pos2::new(full.max.x, full.min.y + size),
    );
    let left = egui::Rect::from_min_max(
        Pos2::new(full.min.x, full.min.y + size),
        Pos2::new(full.min.x + size, full.max.y),
    );
    painter.rect_filled(top, 0.0, theme::CHROME_900);
    painter.rect_filled(left, 0.0, theme::CHROME_900);
    painter.rect_filled(
        egui::Rect::from_min_size(full.min, egui::vec2(size, size)),
        0.0,
        theme::CHROME_900,
    );
    painter.text(
        full.min + egui::vec2(size / 2.0, size / 2.0),
        Align2::CENTER_CENTER,
        unit.label(),
        FontId::proportional(8.0),
        theme::CHROME_300,
    );
    let font = FontId::proportional(9.0);
    let tick = hair(theme::CHROME_400);

    let ((_, sx), (_, sy)) = vp.ruler_axes();
    // Regla superior: recorre el eje del mundo que va a lo largo de x en pantalla.
    let (lo, hi) = (
        (full.min.x + size - vp.origin.x) as f64 / px_per_unit,
        (full.max.x - vp.origin.x) as f64 / px_per_unit,
    );
    let (lo, hi) = if sx < 0.0 { (-hi, -lo) } else { (lo, hi) };
    let start = (lo / minor).floor() * minor;
    let end = hi;
    let mut u = start;
    while u <= end {
        let x = vp.origin.x + (sx * u * px_per_unit) as f32;
        let is_major = (u / major - (u / major).round()).abs() < 1e-6;
        let len = if is_major { size } else { size * 0.4 };
        if x >= top.min.x {
            painter.line_segment(
                [Pos2::new(x, top.max.y - len), Pos2::new(x, top.max.y)],
                tick,
            );
            if is_major {
                painter.text(
                    Pos2::new(x + 2.0, top.min.y + 1.0),
                    Align2::LEFT_TOP,
                    format!("{}", (u * 1000.0).round() / 1000.0),
                    font.clone(),
                    theme::CHROME_300,
                );
            }
        }
        u += minor;
    }
    let (lo, hi) = (
        (full.min.y + size - vp.origin.y) as f64 / px_per_unit,
        (full.max.y - vp.origin.y) as f64 / px_per_unit,
    );
    let (lo, hi) = if sy < 0.0 { (-hi, -lo) } else { (lo, hi) };
    let start = (lo / minor).floor() * minor;
    let end = hi;
    let mut u = start;
    while u <= end {
        let y = vp.origin.y + (sy * u * px_per_unit) as f32;
        let is_major = (u / major - (u / major).round()).abs() < 1e-6;
        let len = if is_major { size } else { size * 0.4 };
        if y >= left.min.y {
            painter.line_segment(
                [Pos2::new(left.max.x - len, y), Pos2::new(left.max.x, y)],
                tick,
            );
            if is_major {
                painter.text(
                    Pos2::new(left.min.x + 1.5, y + 1.0),
                    Align2::LEFT_TOP,
                    format!("{}", (u * 1000.0).round() / 1000.0),
                    FontId::proportional(8.0),
                    theme::CHROME_300,
                );
            }
        }
        u += minor;
    }
    if let Some(c) = app.canvas.cursor_mm {
        let p = vp.to_screen(c);
        painter.line_segment(
            [Pos2::new(p.x, top.min.y), Pos2::new(p.x, top.max.y)],
            hair(theme::ACCENT),
        );
        painter.line_segment(
            [Pos2::new(left.min.x, p.y), Pos2::new(left.max.x, p.y)],
            hair(theme::ACCENT),
        );
    }
}

/// Recorta el segmento a-b al rectángulo (Liang–Barsky). None si queda fuera.
fn clip_segment_to_rect(a: Point, b: Point, r: Rect) -> Option<(Point, Point)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut t0 = 0.0;
    let mut t1 = 1.0;
    for (p, q) in [
        (-dx, a.x - r.x0),
        (dx, r.x1 - a.x),
        (-dy, a.y - r.y0),
        (dy, r.y1 - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                if t > t1 {
                    return None;
                }
                if t > t0 {
                    t0 = t;
                }
            } else {
                if t < t0 {
                    return None;
                }
                if t < t1 {
                    t1 = t;
                }
            }
        }
    }
    Some((
        Point::new(a.x + dx * t0, a.y + dy * t0),
        Point::new(a.x + dx * t1, a.y + dy * t1),
    ))
}
