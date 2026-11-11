//! Panel Estilo de trazo: pestañas Estilo y Color, con las opciones de la referencia.

use super::fill::{
    SW, base_palette, c32, color_fields, from32, new_palette_row, small_swatch, spectrum,
};
use super::section;
use crate::app::OpenCraftCutterApp;
use crate::canvas::EyedropperTarget;
use crate::prefs::NamedPalette;
use crate::theme;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke as EStroke, StrokeKind, Ui, Vec2};
use egui_material_icons::icons as mi;
use oc_core::Color;
use oc_core::shape::{LineCap, LineJoin, LineStyle, StrokeAlign};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Style,
    Color,
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let tab_id = egui::Id::unique("line_tab");
    let default_tab = if cfg!(debug_assertions)
        && std::env::var("OPENCRAFTCUTTER_DEBUG_TAB").as_deref() == Ok("color")
    {
        Tab::Color
    } else {
        Tab::Style
    };
    let mut tab: Tab = ui.data(|d| d.get_temp(tab_id)).unwrap_or(default_tab);
    ui.horizontal(|ui| {
        if ui
            .selectable_label(
                tab == Tab::Style,
                mi::ICON_LINE_STYLE.rich_text().size(18.0),
            )
            .on_hover_text("Estilo de trazo")
            .clicked()
        {
            tab = Tab::Style;
        }
        if ui
            .selectable_label(
                tab == Tab::Color,
                mi::ICON_CROP_SQUARE.rich_text().size(18.0),
            )
            .on_hover_text("Color de trazo")
            .clicked()
        {
            tab = Tab::Color;
        }
    });
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    ui.separator();
    let n = app.doc.selected.len();
    ui.label(
        RichText::new(if n > 0 {
            format!("Aplicando a {n} objeto{}.", if n > 1 { "s" } else { "" })
        } else {
            "Sin selección: se aplicará a las formas nuevas.".into()
        })
        .small()
        .color(theme::CHROME_300),
    );
    match tab {
        Tab::Style => style_tab(app, ui),
        Tab::Color => color_tab(app, ui),
    }
}

/// Etiqueta en la columna izquierda de ancho fijo, alineada a la izquierda.
fn label_cell(ui: &mut Ui, width: f32, text: &str) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 28.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(width);
            ui.label(RichText::new(text).color(theme::CHROME_200));
        },
    );
}

/// Botón con un icono dibujado a mano (vértices, remates, posición).
fn glyph_button(
    ui: &mut Ui,
    selected: bool,
    tooltip: &str,
    draw: impl Fn(&egui::Painter, Rect, Color32),
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 28.0), Sense::click());
    crate::theme::name_widget(&resp, tooltip);
    let bg = if selected {
        theme::ACCENT
    } else if resp.hovered() {
        theme::CHROME_600
    } else {
        theme::CHROME_700
    };
    ui.painter().rect_filled(rect, 5.0, bg);
    let fg = if selected {
        theme::CHROME_950
    } else {
        theme::CHROME_100
    };
    draw(ui.painter(), rect.shrink(7.0), fg);
    resp.on_hover_text(tooltip).clicked()
}

fn join_glyph(join: LineJoin) -> impl Fn(&egui::Painter, Rect, Color32) {
    move |p, r, c| {
        let w = 5.0;
        let a = Pos2::new(r.left() + w / 2.0, r.top());
        let corner = Pos2::new(r.left() + w / 2.0, r.bottom() - w / 2.0);
        let b = Pos2::new(r.right(), r.bottom() - w / 2.0);
        p.line_segment([a, corner], EStroke::new(w, c));
        p.line_segment([corner, b], EStroke::new(w, c));
        match join {
            LineJoin::Miter => {
                p.rect_filled(Rect::from_center_size(corner, Vec2::splat(w)), 0.0, c);
            }
            LineJoin::Round => {
                p.circle_filled(corner, w / 2.0, c);
            }
            LineJoin::Bevel => {
                p.add(egui::Shape::convex_polygon(
                    vec![
                        corner + Vec2::new(-w / 2.0, -w / 2.0),
                        corner + Vec2::new(w / 2.0, -w / 2.0),
                        corner + Vec2::new(w / 2.0, w / 2.0),
                        corner + Vec2::new(0.0, w / 2.0),
                        corner + Vec2::new(-w / 2.0, 0.0),
                    ],
                    c,
                    EStroke::NONE,
                ));
            }
        }
    }
}

fn cap_glyph(cap: LineCap) -> impl Fn(&egui::Painter, Rect, Color32) {
    move |p, r, c| {
        let h = 8.0;
        let y = r.center().y;
        let x0 = r.left() + 4.0;
        let x1 = r.right();
        p.rect_filled(
            Rect::from_min_max(Pos2::new(x0, y - h / 2.0), Pos2::new(x1, y + h / 2.0)),
            0.0,
            c,
        );
        match cap {
            LineCap::Butt => {}
            LineCap::Round => {
                p.circle_filled(Pos2::new(x0, y), h / 2.0, c);
            }
            LineCap::Square => {
                p.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(x0 - h / 2.0, y - h / 2.0),
                        Pos2::new(x0, y + h / 2.0),
                    ),
                    0.0,
                    c,
                );
            }
        }
        // Marca del extremo real de la línea.
        p.line_segment(
            [
                Pos2::new(x0, y - h / 2.0 - 2.0),
                Pos2::new(x0, y + h / 2.0 + 2.0),
            ],
            EStroke::new(1.0, theme::CUT_AREA),
        );
    }
}

fn align_glyph(align: StrokeAlign) -> impl Fn(&egui::Painter, Rect, Color32) {
    move |p, r, c| {
        let edge = r.shrink(2.0);
        match align {
            StrokeAlign::Center => {
                p.rect_stroke(edge, 0.0, EStroke::new(4.0, c), StrokeKind::Middle);
            }
            StrokeAlign::Inside => {
                p.rect_stroke(edge, 0.0, EStroke::new(4.0, c), StrokeKind::Inside);
            }
        }
        p.rect_stroke(
            edge,
            0.0,
            EStroke::new(1.0, theme::CUT_AREA),
            StrokeKind::Middle,
        );
    }
}

/// Vista previa de un estilo de línea para el desplegable.
fn style_preview(ui: &mut Ui, style: LineStyle, selected: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(150.0, 22.0), Sense::click());
    if selected || resp.hovered() {
        ui.painter().rect_filled(
            rect,
            3.0,
            if selected {
                theme::ACCENT_SOFT
            } else {
                theme::CHROME_600
            },
        );
    }
    let y = rect.center().y;
    let pts = [
        Pos2::new(rect.left() + 8.0, y),
        Pos2::new(rect.right() - 8.0, y),
    ];
    let stroke = EStroke::new(2.0, theme::CHROME_100);
    match style.pattern() {
        None => {
            ui.painter().line_segment(pts, stroke);
        }
        Some((d, g)) => {
            ui.painter().add(egui::Shape::dashed_line(
                &pts,
                stroke,
                2.0 * d as f32,
                2.0 * g as f32,
            ));
        }
    }
    resp.on_hover_text(style.name()).clicked()
}

fn style_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let current = app
        .doc
        .selected_shapes()
        .first()
        .map(|s| s.stroke)
        .unwrap_or(app.defaults.stroke);
    let label_w = super::LABEL_COLUMN_WIDTH;

    // Estilo
    ui.horizontal(|ui| {
        label_cell(ui, label_w, "Estilo");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let resp = egui::ComboBox::from_id_salt("line_style_combo")
                .width(super::CONTROL_WIDTH)
                .selected_text(current.style.name())
                .show_ui(ui, |ui| {
                    for st in LineStyle::ALL {
                        if style_preview(ui, st, current.style == st) {
                            app.apply_stroke(move |s| s.style = st);
                        }
                    }
                });
            let _ = resp;
        });
    });

    // Grosor
    ui.label(RichText::new("Grosor").color(theme::CHROME_200));
    let mut w = current.width_pt;
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = ui.available_width() - 104.0;
        let r = ui.add(egui::Slider::new(&mut w, 0.0..=20.0).show_value(false));
        if r.drag_started() {
            app.apply_stroke(move |s| s.width_pt = w);
        } else if r.changed() {
            app.apply_stroke_live(move |s| s.width_pt = w);
        }
        let mut wv = current.width_pt;
        let r = ui.add(
            egui::DragValue::new(&mut wv)
                .speed(0.1)
                .range(0.0..=100.0)
                .fixed_decimals(2)
                .suffix(" pt"),
        );
        if r.drag_started() || (r.changed() && !r.dragged()) {
            app.apply_stroke(move |s| s.width_pt = wv);
        } else if r.changed() {
            app.apply_stroke_live(move |s| s.width_pt = wv);
        }
    });

    // Vértice
    ui.horizontal(|ui| {
        label_cell(ui, label_w, "Vértice");
        for j in LineJoin::ALL {
            if glyph_button(ui, current.join == j, j.name(), join_glyph(j)) {
                app.apply_stroke(move |s| s.join = j);
            }
        }
    });
    // Remates
    ui.horizontal(|ui| {
        label_cell(ui, label_w, "Remates");
        for c in LineCap::ALL {
            if glyph_button(ui, current.cap == c, c.name(), cap_glyph(c)) {
                app.apply_stroke(move |s| s.cap = c);
            }
        }
    });
    // Posicionar
    ui.horizontal(|ui| {
        label_cell(ui, label_w, "Posicionar");
        for (a, tip) in [
            (StrokeAlign::Center, "Centrado en el borde"),
            (StrokeAlign::Inside, "Hacia dentro de la forma"),
        ] {
            if glyph_button(ui, current.align == a, tip, align_glyph(a)) {
                app.apply_stroke(move |s| s.align = a);
            }
        }
    });

    ui.separator();
    let mut print = current.print_hairline;
    if ui
        .checkbox(&mut print, "Imprimir líneas de las formas seleccionadas")
        .changed()
    {
        app.apply_stroke(move |s| s.print_hairline = print);
    }
    ui.label(
        RichText::new("Muestra las líneas de corte de 0 pt como líneas de 0,1 pt al imprimir y en los SVG exportados.")
            .small()
            .italics()
            .color(theme::CHROME_300),
    );
}

fn color_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let stroke = app
        .doc
        .selected_shapes()
        .first()
        .map(|s| s.stroke)
        .unwrap_or(app.defaults.stroke);
    let current = stroke.color;
    let set_color = |app: &mut OpenCraftCutterApp, c: Color| {
        app.apply_stroke(move |s| s.color = c);
        app.remember_line_color(c);
    };

    section(ui, "Usada recientemente");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let recent = app.prefs.recent_line_colors.clone();
        if recent.is_empty() {
            ui.label(
                RichText::new("Aún no hay colores recientes.")
                    .small()
                    .color(theme::CHROME_400),
            );
        }
        for c in recent {
            if small_swatch(ui, Some(c), current == c, &c.to_hex()) {
                set_color(app, c);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
            ui.painter().rect_filled(rect, 3.0, c32(current));
            ui.painter().rect_stroke(
                rect,
                3.0,
                EStroke::new(1.0, theme::CHROME_500),
                StrokeKind::Inside,
            );
            let active = app.canvas.eyedropper == Some(EyedropperTarget::Line);
            if ui
                .add(egui::Button::selectable(
                    active,
                    mi::ICON_COLORIZE.rich_text().size(16.0),
                ))
                .on_hover_text("Cuentagotas: haz clic en una forma para tomar su color de línea")
                .clicked()
            {
                app.canvas.eyedropper = if active {
                    None
                } else {
                    Some(EyedropperTarget::Line)
                };
            }
        });
    });

    section(ui, "Color de trazo");
    ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
    for row in base_palette() {
        ui.horizontal(|ui| {
            ui.add_space(SW + 3.0);
            for c in row {
                if small_swatch(ui, Some(c), current == c, &c.to_hex()) {
                    set_color(app, c);
                }
            }
        });
    }
    let mut remove: Option<usize> = None;
    let mut add_to: Option<usize> = None;
    for (pi, pal) in app.prefs.line_palettes.clone().iter().enumerate() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(&pal.name).small().color(theme::CHROME_200));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(mi::ICON_DELETE.rich_text())
                    .on_hover_text("Eliminar paleta")
                    .clicked()
                {
                    remove = Some(pi);
                }
                if ui
                    .small_button(mi::ICON_ADD.rich_text())
                    .on_hover_text("Añadir el color actual a esta paleta")
                    .clicked()
                {
                    add_to = Some(pi);
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            for c in &pal.colors {
                if small_swatch(ui, Some(*c), current == *c, &c.to_hex()) {
                    set_color(app, *c);
                }
            }
        });
    }
    if let Some(pi) = add_to {
        if let Some(p) = app.prefs.line_palettes.get_mut(pi) {
            if !p.colors.contains(&current) {
                p.colors.push(current);
            }
        }
    }
    if let Some(pi) = remove {
        app.prefs.line_palettes.remove(pi);
    }
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 9.0);
    new_palette_row(ui, egui::Id::unique("line_new_palette"), |name| {
        app.prefs.line_palettes.push(NamedPalette {
            name,
            colors: vec![current],
        });
    });

    // Transparencia
    ui.add_space(2.0);
    ui.label(RichText::new("Transparencia").color(theme::CHROME_200));
    let mut t = (1.0 - stroke.opacity) * 100.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = ui.available_width() - 104.0;
        let r = ui.add(egui::Slider::new(&mut t, 0.0..=100.0).show_value(false));
        let op = 1.0 - t / 100.0;
        if r.drag_started() {
            app.apply_stroke(move |s| s.opacity = op);
        } else if r.changed() {
            app.apply_stroke_live(move |s| s.opacity = op);
        }
        let mut tv = (1.0 - stroke.opacity) * 100.0;
        if ui
            .add(
                egui::DragValue::new(&mut tv)
                    .range(0.0..=100.0)
                    .fixed_decimals(1)
                    .suffix(" %"),
            )
            .changed()
        {
            let op = 1.0 - tv / 100.0;
            app.apply_stroke(move |s| s.opacity = op);
        }
    });

    ui.separator();
    section(ui, "Color personalizado");
    let mut c = c32(current);
    if spectrum(ui, egui::Id::unique("line_spectrum"), &mut c) {
        let col = from32(c);
        if ui.input(|i| i.pointer.primary_pressed()) {
            app.apply_stroke(move |s| s.color = col);
        } else {
            app.apply_stroke_live(move |s| s.color = col);
        }
        app.remember_line_color(col);
    }
    if let Some(col) = color_fields(ui, egui::Id::unique("line_color_fields"), current) {
        set_color(app, col);
    }
}
