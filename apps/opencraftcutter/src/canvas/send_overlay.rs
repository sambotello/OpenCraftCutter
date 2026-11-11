//! Vista previa de Enviar sobre el lienzo: el diseño atenuado, las líneas que se envían (en el
//! color de su portaherramientas, o en su propio color en los modos por color) y los botones de
//! asignación bajo la vista previa.

use crate::app::OpenCraftCutterApp;
use crate::shell::send::holder_color;
use crate::theme;
use egui::{Color32, Painter, RichText, Stroke, Ui};
use egui_material_icons::icons as mi;
use oc_core::send::{CutAssign, CutStyle, Holder, SendMode};

pub fn draw(app: &OpenCraftCutterApp, painter: &Painter, stage: egui::Rect) {
    painter.rect_filled(stage, 0.0, Color32::from_white_alpha(110));
    let vp = app.canvas.viewport;
    // «Elegir sitio»: dónde caería la prueba con el clic.
    if app.canvas.send.placing_test {
        if let Some(p) = app.canvas.cursor_mm {
            let o = oc_core::send::test_origin_at(p, oc_core::send::test_area(&app.doc));
            let k = oc_core::send::TEST_SIZE_MM;
            let r =
                egui::Rect::from_two_pos(vp.to_screen(o), vp.to_screen(o + kurbo::Vec2::new(k, k)));
            painter.rect_stroke(
                r,
                0.0,
                Stroke::new(1.5, theme::ACCENT),
                egui::StrokeKind::Middle,
            );
        }
    }
    let line = |paths: Vec<oc_core::send::Polyline>, stroke: Stroke| {
        for (pts, closed) in paths {
            let mut screen: Vec<egui::Pos2> = pts.iter().map(|p| vp.to_screen(*p)).collect();
            if closed {
                if let Some(first) = screen.first().copied() {
                    screen.push(first);
                }
            }
            painter.add(egui::Shape::line(screen, stroke));
        }
    };
    let s = &app.canvas.send;
    match s.mode {
        // Por color: cada línea en su color, para ver qué fila corta qué.
        SendMode::ByLine | SendMode::ByFill => {
            let by_fill = s.mode == SendMode::ByFill;
            let rows = if by_fill { &s.fill_rows } else { &s.line_rows };
            let doc = app.cut_doc();
            for (color, row) in rows.iter().filter(|(_, r)| r.enabled) {
                let one = [(*color, row.clone())];
                let c = if color.a == 0 {
                    theme::CHROME_400
                } else {
                    Color32::from_rgb(color.r, color.g, color.b)
                };
                for (_, paths) in oc_core::send::color_preview(&doc, by_fill, &one) {
                    line(paths, Stroke::new(2.0, c));
                }
            }
        }
        // Estándar y Guía: en el color del portaherramientas.
        SendMode::Standard | SendMode::Guide => {
            for (holder, paths) in app.send_preview() {
                line(paths, Stroke::new(2.0, holder_color(holder)));
            }
        }
    }
    simulation(app, painter);
    // Detalles demasiado finos para el material, en rojo encima.
    let red = Color32::from_rgb(0xef, 0x23, 0x23);
    for poly in app.thin_details().iter() {
        for c in poly {
            let pts: Vec<egui::Pos2> = c
                .iter()
                .map(|q| vp.to_screen(kurbo::Point::new(q[0], q[1])))
                .collect();
            // Lado a lado: con una línea cerrada, egui alarga las esquinas muy agudas.
            for i in 0..pts.len() {
                painter.line_segment([pts[i], pts[(i + 1) % pts.len()]], Stroke::new(2.5, red));
            }
        }
    }
}

/// Simulación: lo ya cortado en negro grueso, los desplazamientos en vacío en gris fino y la
/// cuchilla como un punto.
fn simulation(app: &OpenCraftCutterApp, painter: &Painter) {
    let Some(sim) = &app.canvas.send.simulation else {
        return;
    };
    let vp = app.canvas.viewport;
    let ctx = painter.ctx();
    let t = sim.at(ctx.input(|i| i.time));
    let cut = Stroke::new(3.0, Color32::from_rgb(0x11, 0x18, 0x27));
    let travel = Stroke::new(1.0, Color32::from_gray(0x80));
    let mut pos = None;
    for m in sim.moves.iter().take_while(|m| m.start_s < t) {
        let k = if m.secs > 0.0 {
            ((t - m.start_s) / m.secs).min(1.0)
        } else {
            1.0
        };
        let end = m.from.lerp(m.to, k);
        let seg = [vp.to_screen(m.from), vp.to_screen(end)];
        if m.cutting {
            painter.line_segment(seg, cut);
        } else {
            painter.line_segment(seg, travel);
        }
        pos = Some(seg[1]);
    }
    if let Some(p) = pos {
        painter.circle(p, 6.0, theme::ACCENT, Stroke::new(2.0, Color32::WHITE));
    }
    if t < sim.total_s {
        ctx.request_repaint();
    }
}

fn assign_button(
    ui: &mut Ui,
    active: bool,
    enabled: bool,
    holder: Option<Holder>,
    edge: bool,
    label: &str,
) -> bool {
    let icon = if edge {
        mi::ICON_BORDER_OUTER
    } else {
        mi::ICON_FILTER_NONE
    };
    let color = holder.map(holder_color).unwrap_or(theme::CHROME_200);
    let text = RichText::new(format!("{}  {label}", <&str>::from(icon))).color(if active {
        Color32::WHITE
    } else {
        theme::CHROME_200
    });
    let mut b = egui::Button::new(text)
        .min_size(egui::vec2(250.0, 26.0))
        .stroke(Stroke::new(1.0, color));
    if active {
        b = b.fill(color.gamma_multiply(0.55));
    }
    ui.add_enabled(enabled, b).clicked()
}

/// Botones para elegir portaherramientas y líneas de los objetos seleccionados (modo Estándar).
pub fn assign_buttons(app: &mut OpenCraftCutterApp, ui: &mut Ui, stage: egui::Rect) {
    if !matches!(app.canvas.send.mode, SendMode::Standard | SendMode::Guide) {
        return;
    }
    let sel: Vec<CutAssign> = app.doc.selected_shapes().iter().map(|s| s.cut).collect();
    let enabled = !sel.is_empty();
    let current = sel.first().copied().filter(|c| sel.iter().all(|s| s == c));
    let mut chosen: Option<CutAssign> = None;
    egui::Area::new(egui::Id::unique("send_assign_buttons"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -16.0))
        .constrain_to(stage)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                if !enabled {
                    ui.label(
                        RichText::new("Selecciona objetos para elegir cómo se envían.")
                            .small()
                            .color(theme::CHROME_300),
                    );
                }
                for h in Holder::ALL {
                    ui.horizontal(|ui| {
                        for (style, label) in [
                            (CutStyle::All, "Usar todas las líneas seleccionadas"),
                            (CutStyle::Edge, "Usar el contorno de la selección"),
                        ] {
                            let a = CutAssign { holder: h, style };
                            if assign_button(
                                ui,
                                current == Some(a),
                                enabled,
                                Some(h),
                                style == CutStyle::Edge,
                                &format!("{} · {label}", h.number()),
                            ) {
                                chosen = Some(a);
                            }
                        }
                    });
                }
                ui.vertical_centered(|ui| {
                    let skip = current.is_some_and(|c| c.style == CutStyle::Skip);
                    let b = egui::Button::new(format!(
                        "{}  Omitir las líneas seleccionadas",
                        <&str>::from(mi::ICON_DO_NOT_DISTURB_ON)
                    ))
                    .min_size(egui::vec2(250.0, 26.0))
                    .selected(skip);
                    if ui.add_enabled(enabled, b).clicked() {
                        let holder = current.map(|c| c.holder).unwrap_or_default();
                        chosen = Some(CutAssign {
                            holder,
                            style: CutStyle::Skip,
                        });
                    }
                });
            });
        });
    if let Some(a) = chosen {
        app.commit();
        let ids = app.doc.selected.clone();
        for id in ids {
            if let Some(s) = app.doc.shape_mut(id) {
                s.cut = a;
            }
        }
    }
}
