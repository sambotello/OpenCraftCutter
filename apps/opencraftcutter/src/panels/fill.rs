//! Panel Relleno: pestañas Color y Degradado (los motivos quedan fuera por ahora).

use super::section;
use crate::app::OpenCraftCutterApp;
use crate::prefs::{NamedGradients, NamedPalette};
use crate::theme;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};
use egui_material_icons::icons as mi;
use oc_core::Color;
use oc_core::shape::{Gradient, GradientStop};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Color,
    Gradient,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Hex,
    Rgb,
    Hsl,
}

impl Format {
    fn name(self) -> &'static str {
        match self {
            Format::Hex => "HEX",
            Format::Rgb => "RGB",
            Format::Hsl => "HSL",
        }
    }
}

pub(super) const SW: f32 = 17.0;

/// HSV (h en 0..1) → Color32.
fn hsv32(h: f32, s: f32, v: f32) -> Color32 {
    let rgb = egui::ecolor::Hsva::new(h, s, v, 1.0);
    Color32::from(rgb)
}

/// Espectro de color: tono en horizontal, saturación en vertical y barra de brillo a la derecha.
/// El HSV se guarda entre frames para no perder el tono con blancos o grises.
pub(super) fn spectrum(ui: &mut Ui, id: egui::Id, c: &mut Color32) -> bool {
    let hsv_id = id.with("hsv");
    let stored: Option<[f32; 3]> = ui.data(|d| d.get_temp(hsv_id));
    let mut hsv = match stored {
        Some([h, s, v]) if hsv32(h, s, v) == *c => [h, s, v],
        _ => {
            let x = egui::ecolor::Hsva::from(*c);
            [x.h, x.s, x.v]
        }
    };
    let mut changed = false;
    ui.horizontal(|ui| {
        let w = ui.available_width() - 30.0;
        let (area, resp) = ui.allocate_exact_size(Vec2::new(w, 110.0), Sense::click_and_drag());
        // Malla con colores por vértice: columnas = tono, filas = saturación (arriba plena).
        let (cols, rows) = (36, 6);
        let mut mesh = egui::Mesh::default();
        for j in 0..=rows {
            for i in 0..=cols {
                let (u, t) = (i as f32 / cols as f32, j as f32 / rows as f32);
                let pos = Pos2::new(
                    area.left() + u * area.width(),
                    area.top() + t * area.height(),
                );
                mesh.colored_vertex(pos, hsv32(u, 1.0 - t, hsv[2]));
            }
        }
        for j in 0..rows {
            for i in 0..cols {
                let a = (j * (cols + 1) + i) as u32;
                let b = a + 1;
                let c2 = a + (cols + 1) as u32;
                let d = c2 + 1;
                mesh.add_triangle(a, b, d);
                mesh.add_triangle(a, d, c2);
            }
        }
        ui.painter().add(egui::Shape::mesh(mesh));
        ui.painter().rect_stroke(
            area,
            2.0,
            Stroke::new(1.0, theme::CHROME_500),
            StrokeKind::Inside,
        );
        if let Some(p) = resp.interact_pointer_pos() {
            if resp.dragged() || resp.clicked() || resp.drag_started() {
                hsv[0] = ((p.x - area.left()) / area.width()).clamp(0.0, 1.0);
                hsv[1] = 1.0 - ((p.y - area.top()) / area.height()).clamp(0.0, 1.0);
                changed = true;
            }
        }
        let marker = Pos2::new(
            area.left() + hsv[0] * area.width(),
            area.top() + (1.0 - hsv[1]) * area.height(),
        );
        ui.painter()
            .circle_stroke(marker, 5.0, Stroke::new(2.0, Color32::WHITE));
        ui.painter()
            .circle_stroke(marker, 6.5, Stroke::new(1.0, Color32::BLACK));

        // Barra de brillo
        let (bar, bresp) = ui.allocate_exact_size(Vec2::new(18.0, 110.0), Sense::click_and_drag());
        let mut m = egui::Mesh::default();
        m.colored_vertex(bar.left_top(), hsv32(hsv[0], hsv[1], 1.0));
        m.colored_vertex(bar.right_top(), hsv32(hsv[0], hsv[1], 1.0));
        m.colored_vertex(bar.right_bottom(), Color32::BLACK);
        m.colored_vertex(bar.left_bottom(), Color32::BLACK);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        ui.painter().add(egui::Shape::mesh(m));
        ui.painter().rect_stroke(
            bar,
            2.0,
            Stroke::new(1.0, theme::CHROME_500),
            StrokeKind::Inside,
        );
        if let Some(p) = bresp.interact_pointer_pos() {
            if bresp.dragged() || bresp.clicked() || bresp.drag_started() {
                hsv[2] = 1.0 - ((p.y - bar.top()) / bar.height()).clamp(0.0, 1.0);
                changed = true;
            }
        }
        let y = bar.top() + (1.0 - hsv[2]) * bar.height();
        ui.painter().rect_stroke(
            Rect::from_min_max(
                Pos2::new(bar.left() - 2.0, y - 2.0),
                Pos2::new(bar.right() + 2.0, y + 2.0),
            ),
            1.0,
            Stroke::new(1.5, Color32::WHITE),
            StrokeKind::Middle,
        );
    });
    if changed {
        *c = hsv32(hsv[0], hsv[1], hsv[2]);
    }
    ui.data_mut(|d| d.insert_temp(hsv_id, hsv));
    changed
}

pub(super) fn c32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

pub(super) fn from32(c: Color32) -> Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color::rgba(r, g, b, a)
}

/// Muestra pequeña clicable; `None` = sin relleno (tachada).
pub(super) fn small_swatch(
    ui: &mut Ui,
    color: Option<Color>,
    selected: bool,
    tooltip: &str,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(SW), Sense::click());
    crate::theme::name_widget(&resp, tooltip);
    let p = ui.painter();
    match color {
        Some(c) => {
            p.rect_filled(rect, 2.0, c32(c));
        }
        None => {
            p.rect_filled(rect, 2.0, Color32::WHITE);
            p.line_segment(
                [rect.left_bottom(), rect.right_top()],
                Stroke::new(1.5, Color32::from_rgb(0xe3, 0x34, 0x3f)),
            );
        }
    }
    let stroke = if selected {
        Stroke::new(2.0, theme::ACCENT)
    } else if resp.hovered() {
        Stroke::new(1.0, Color32::WHITE)
    } else {
        Stroke::new(1.0, theme::CHROME_500)
    };
    p.rect_stroke(rect, 2.0, stroke, StrokeKind::Inside);
    resp.on_hover_text(tooltip).clicked()
}

/// Paleta base: fila de grises y cinco filas de tonos de más intenso a más claro.
pub(super) fn base_palette() -> Vec<Vec<Color>> {
    let mut rows = Vec::new();
    rows.push(
        (0..12)
            .map(|i| Color::from_hsl(0.0, 0.0, 1.0 - i as f64 / 11.0))
            .collect(),
    );
    let hues = [
        0.0, 330.0, 280.0, 235.0, 205.0, 180.0, 145.0, 90.0, 55.0, 40.0, 25.0, 15.0,
    ];
    for (s, l) in [
        (0.75, 0.50),
        (0.70, 0.62),
        (0.65, 0.72),
        (0.60, 0.82),
        (0.55, 0.91),
    ] {
        rows.push(hues.iter().map(|h| Color::from_hsl(*h, s, l)).collect());
    }
    rows
}

/// Degradados predefinidos (propios).
fn preset_gradients() -> Vec<Gradient> {
    let mut out = Vec::new();
    for h in [
        0.0, 25.0, 50.0, 90.0, 130.0, 170.0, 205.0, 235.0, 275.0, 320.0,
    ] {
        out.push(Gradient::two(
            Color::from_hsl(h, 0.8, 0.45),
            Color::from_hsl(h, 0.8, 0.85),
        ));
    }
    for (a, b) in [
        (0.0, 50.0),
        (200.0, 280.0),
        (90.0, 180.0),
        (320.0, 30.0),
        (170.0, 230.0),
        (40.0, 340.0),
    ] {
        out.push(Gradient::two(
            Color::from_hsl(a, 0.8, 0.55),
            Color::from_hsl(b, 0.8, 0.55),
        ));
    }
    let metal = |dark: Color, light: Color| Gradient {
        stops: vec![
            GradientStop {
                pos: 0.0,
                color: dark,
            },
            GradientStop {
                pos: 0.5,
                color: light,
            },
            GradientStop {
                pos: 1.0,
                color: dark,
            },
        ],
        angle_deg: 0.0,
    };
    out.push(metal(
        Color::rgb(0x8a, 0x6a, 0x1c),
        Color::rgb(0xf7, 0xe2, 0x8b),
    ));
    out.push(metal(
        Color::rgb(0x6b, 0x6b, 0x6b),
        Color::rgb(0xf0, 0xf0, 0xf0),
    ));
    out.push(metal(
        Color::rgb(0x7a, 0x3e, 0x1d),
        Color::rgb(0xf2, 0xb2, 0x7e),
    ));
    out.push(metal(
        Color::rgb(0x1d, 0x3d, 0x7a),
        Color::rgb(0xbf, 0xd8, 0xff),
    ));
    out.push(Gradient {
        stops: [0.0, 50.0, 120.0, 200.0, 270.0, 320.0]
            .iter()
            .enumerate()
            .map(|(i, h)| GradientStop {
                pos: i as f64 / 5.0,
                color: Color::from_hsl(*h, 0.85, 0.55),
            })
            .collect(),
        angle_deg: 0.0,
    });
    out.push(Gradient::two(Color::BLACK, Color::WHITE));
    out
}

fn gradient_swatch(ui: &mut Ui, g: &Gradient, size: Vec2, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    paint_gradient(ui, rect, g);
    let stroke = if selected {
        Stroke::new(2.0, theme::ACCENT)
    } else if resp.hovered() {
        Stroke::new(1.0, Color32::WHITE)
    } else {
        Stroke::new(1.0, theme::CHROME_500)
    };
    ui.painter()
        .rect_stroke(rect, 2.0, stroke, StrokeKind::Inside);
    resp
}

fn paint_gradient(ui: &Ui, rect: Rect, g: &Gradient) {
    let n = 48;
    for i in 0..n {
        let t0 = i as f32 / n as f32;
        let t1 = (i + 1) as f32 / n as f32;
        let r = Rect::from_min_max(
            Pos2::new(rect.left() + rect.width() * t0, rect.top()),
            Pos2::new(rect.left() + rect.width() * t1 + 0.5, rect.bottom()),
        );
        ui.painter()
            .rect_filled(r, 0.0, c32(g.sample(((t0 + t1) / 2.0) as f64)));
    }
}

/// Campo de color con formato elegible. Devuelve el nuevo color si el usuario lo cambió.
pub(super) fn color_fields(ui: &mut Ui, id: egui::Id, current: Color) -> Option<Color> {
    let fmt_id = id.with("fmt");
    let mut fmt: Format = ui.data(|d| d.get_temp(fmt_id)).unwrap_or(Format::Hex);
    let mut out = None;
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(id.with("combo"))
            .width(64.0)
            .selected_text(fmt.name())
            .show_ui(ui, |ui| {
                for f in [Format::Hex, Format::Rgb, Format::Hsl] {
                    ui.selectable_value(&mut fmt, f, f.name());
                }
            });
        match fmt {
            Format::Hex => {
                ui.label(RichText::new("#").color(theme::CHROME_300));
                let text_id = id.with("hex");
                let mut text: String = ui
                    .data(|d| d.get_temp(text_id))
                    .unwrap_or_else(|| current.to_hex().trim_start_matches('#').to_uppercase());
                let r = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(90.0)
                        .font(egui::TextStyle::Monospace),
                );
                if r.changed() {
                    if let Some(c) = Color::from_hex(&text) {
                        out = Some(c);
                    }
                }
                if !r.has_focus() {
                    text = current.to_hex().trim_start_matches('#').to_uppercase();
                }
                ui.data_mut(|d| d.insert_temp(text_id, text));
            }
            Format::Rgb => {
                let (mut r, mut g, mut b) = (current.r, current.g, current.b);
                let mut changed = false;
                for (label, v) in [("R", &mut r), ("G", &mut g), ("B", &mut b)] {
                    ui.label(RichText::new(label).small().color(theme::CHROME_300));
                    changed |= ui.add(egui::DragValue::new(v).range(0..=255)).changed();
                }
                if changed {
                    out = Some(Color::rgba(r, g, b, current.a));
                }
            }
            Format::Hsl => {
                let (h, s, l) = current.to_hsl();
                let (mut h, mut s, mut l) = (h.round(), (s * 100.0).round(), (l * 100.0).round());
                let mut changed = false;
                ui.label(RichText::new("H").small().color(theme::CHROME_300));
                changed |= ui
                    .add(egui::DragValue::new(&mut h).range(0.0..=360.0).suffix("°"))
                    .changed();
                ui.label(RichText::new("S").small().color(theme::CHROME_300));
                changed |= ui
                    .add(egui::DragValue::new(&mut s).range(0.0..=100.0).suffix("%"))
                    .changed();
                ui.label(RichText::new("L").small().color(theme::CHROME_300));
                changed |= ui
                    .add(egui::DragValue::new(&mut l).range(0.0..=100.0).suffix("%"))
                    .changed();
                if changed {
                    out = Some(Color::from_hsl(h, s / 100.0, l / 100.0));
                }
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(fmt_id, fmt));
    out
}

/// Transparencia 0..100 % ↔ opacidad del relleno de la selección.
fn transparency_row(app: &mut OpenCraftCutterApp, ui: &mut Ui, opacity: f64) {
    ui.label(RichText::new("Transparencia").color(theme::CHROME_200));
    let mut t = (1.0 - opacity) * 100.0;
    let mut changed = false;
    let mut started = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = ui.available_width() - 104.0;
        let r = ui.add_enabled(
            !app.doc.selected.is_empty(),
            egui::Slider::new(&mut t, 0.0..=100.0).show_value(false),
        );
        changed |= r.changed();
        started |= r.drag_started();
        let r = ui.add_enabled(
            !app.doc.selected.is_empty(),
            egui::DragValue::new(&mut t)
                .range(0.0..=100.0)
                .fixed_decimals(1)
                .suffix(" %"),
        );
        changed |= r.changed();
        started |= r.drag_started() || r.lost_focus();
    });
    if changed || started {
        app.apply_fill_opacity(1.0 - t / 100.0, started);
    }
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let tab_id = egui::Id::unique("fill_tab");
    let default_tab = if cfg!(debug_assertions)
        && std::env::var("OPENCRAFTCUTTER_DEBUG_TAB").as_deref() == Ok("gradient")
    {
        Tab::Gradient
    } else {
        Tab::Color
    };
    let mut tab: Tab = ui.data(|d| d.get_temp(tab_id)).unwrap_or(default_tab);
    ui.horizontal(|ui| {
        if ui
            .selectable_label(tab == Tab::Color, mi::ICON_SQUARE.rich_text().size(18.0))
            .on_hover_text("Color de relleno")
            .clicked()
        {
            tab = Tab::Color;
        }
        if ui
            .selectable_label(
                tab == Tab::Gradient,
                mi::ICON_GRADIENT.rich_text().size(18.0),
            )
            .on_hover_text("Degradado de relleno")
            .clicked()
        {
            tab = Tab::Gradient;
        }
    });
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    ui.separator();
    let n = app.doc.selected.len();
    ui.label(
        RichText::new(if n > 0 {
            format!("Aplicando a {n} objeto{}.", if n > 1 { "s" } else { "" })
        } else {
            "Sin selección: el color se usará en las formas nuevas.".into()
        })
        .small()
        .color(theme::CHROME_300),
    );
    match tab {
        Tab::Color => color_tab(app, ui),
        Tab::Gradient => gradient_tab(app, ui),
    }
}

fn color_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let first = app
        .doc
        .selected_shapes()
        .first()
        .map(|s| (s.fill, s.fill_opacity, s.fill_gradient.is_some()));
    let (current, opacity, has_gradient) = first.unwrap_or((app.defaults.fill, 1.0, false));
    let current_solid = if has_gradient { None } else { current };

    // Usada recientemente + cuentagotas + color actual
    section(ui, "Usada recientemente");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let recent = app.prefs.recent_fill_colors.clone();
        if recent.is_empty() {
            ui.label(
                RichText::new("Aún no hay colores recientes.")
                    .small()
                    .color(theme::CHROME_400),
            );
        }
        for c in recent {
            if small_swatch(ui, Some(c), current_solid == Some(c), &c.to_hex()) {
                app.apply_fill(Some(c));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
            match current {
                Some(c) => {
                    ui.painter().rect_filled(rect, 3.0, c32(c));
                }
                None => {
                    ui.painter().rect_filled(rect, 3.0, Color32::WHITE);
                    ui.painter().line_segment(
                        [rect.left_bottom(), rect.right_top()],
                        Stroke::new(1.5, Color32::from_rgb(0xe3, 0x34, 0x3f)),
                    );
                }
            }
            ui.painter().rect_stroke(
                rect,
                3.0,
                Stroke::new(1.0, theme::CHROME_500),
                StrokeKind::Inside,
            );
            let active = app.canvas.eyedropper == Some(crate::canvas::EyedropperTarget::Fill);
            if ui
                .add(egui::Button::selectable(
                    active,
                    mi::ICON_COLORIZE.rich_text().size(16.0),
                ))
                .on_hover_text("Cuentagotas: haz clic en una forma del lienzo para tomar su color")
                .clicked()
            {
                app.canvas.eyedropper = if active {
                    None
                } else {
                    Some(crate::canvas::EyedropperTarget::Fill)
                };
            }
        });
    });

    // Paleta
    section(ui, "Color de relleno");
    let rows = base_palette();
    ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
    for (i, row) in rows.iter().enumerate() {
        ui.horizontal(|ui| {
            if i == 0 {
                if small_swatch(ui, None, current.is_none(), "Sin relleno") {
                    app.apply_fill(None);
                }
            } else {
                ui.add_space(SW + 3.0);
            }
            for c in row {
                if small_swatch(ui, Some(*c), current_solid == Some(*c), &c.to_hex()) {
                    app.apply_fill(Some(*c));
                }
            }
        });
    }
    // Paletas guardadas
    let mut remove: Option<usize> = None;
    let mut add_to: Option<usize> = None;
    for (pi, pal) in app.prefs.fill_palettes.clone().iter().enumerate() {
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
                if small_swatch(ui, Some(*c), current_solid == Some(*c), &c.to_hex()) {
                    app.apply_fill(Some(*c));
                }
            }
        });
    }
    if let (Some(pi), Some(c)) = (add_to, current_solid) {
        if let Some(p) = app.prefs.fill_palettes.get_mut(pi) {
            if !p.colors.contains(&c) {
                p.colors.push(c);
            }
        }
    }
    if let Some(pi) = remove {
        app.prefs.fill_palettes.remove(pi);
    }
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 9.0);
    new_palette_row(ui, egui::Id::unique("fill_new_palette"), |name| {
        let colors = current_solid.into_iter().collect();
        app.prefs.fill_palettes.push(NamedPalette { name, colors });
    });

    ui.add_space(2.0);
    transparency_row(app, ui, opacity);

    // Color personalizado
    ui.separator();
    section(ui, "Color personalizado");
    let mut c = c32(current_solid.unwrap_or(Color::WHITE));
    if spectrum(ui, egui::Id::unique("fill_spectrum"), &mut c) {
        if ui.input(|i| i.pointer.primary_pressed()) {
            app.apply_fill(Some(from32(c)));
        } else {
            app.apply_fill_live(Some(from32(c)));
        }
    }
    if let Some(c) = color_fields(
        ui,
        egui::Id::unique("fill_color_fields"),
        current_solid.unwrap_or(Color::WHITE),
    ) {
        app.apply_fill(Some(c));
    }
}

/// Fila "Nombre de paleta nueva" + botón de crear.
pub(super) fn new_palette_row(ui: &mut Ui, id: egui::Id, create: impl FnOnce(String)) {
    let mut name: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let mut created = false;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text("Nombre de paleta nueva")
                .desired_width(ui.available_width() - 34.0),
        );
        if ui
            .add_enabled(
                !name.trim().is_empty(),
                egui::Button::new(mi::ICON_ADD.rich_text()),
            )
            .on_hover_text("Crear paleta")
            .clicked()
        {
            created = true;
        }
    });
    if created {
        create(name.trim().to_string());
        name.clear();
    }
    ui.data_mut(|d| d.insert_temp(id, name));
}

fn gradient_tab(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let has_sel = !app.doc.selected.is_empty();
    let sel_gradient = app
        .doc
        .selected_shapes()
        .first()
        .and_then(|s| s.fill_gradient.clone());
    let opacity = app
        .doc
        .selected_shapes()
        .first()
        .map(|s| s.fill_opacity)
        .unwrap_or(1.0);
    let work_id = egui::Id::unique("fill_gradient_work");
    let mut g: Gradient = sel_gradient.clone().unwrap_or_else(|| {
        ui.data(|d| d.get_temp(work_id)).unwrap_or_else(|| {
            Gradient::two(
                Color::from_hsl(0.0, 0.8, 0.45),
                Color::from_hsl(50.0, 0.9, 0.6),
            )
        })
    });
    if !has_sel {
        ui.label(
            RichText::new("Selecciona una forma para aplicarle un degradado.")
                .small()
                .color(theme::CHROME_400),
        );
    }

    section(ui, "Degradado de relleno");
    let mut picked: Option<Gradient> = None;
    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
    ui.horizontal_wrapped(|ui| {
        for p in preset_gradients() {
            let selected = sel_gradient.as_ref().is_some_and(|s| s.stops == p.stops);
            if gradient_swatch(ui, &p, Vec2::new(27.0, 22.0), selected).clicked() {
                picked = Some(Gradient {
                    angle_deg: g.angle_deg,
                    ..p
                });
            }
        }
    });
    let mut remove: Option<usize> = None;
    let mut add_to: Option<usize> = None;
    for (pi, pal) in app.prefs.gradient_palettes.clone().iter().enumerate() {
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
                    .on_hover_text("Añadir el degradado actual a esta paleta")
                    .clicked()
                {
                    add_to = Some(pi);
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            for p in &pal.gradients {
                if gradient_swatch(
                    ui,
                    p,
                    Vec2::new(27.0, 22.0),
                    sel_gradient.as_ref() == Some(p),
                )
                .clicked()
                {
                    picked = Some(p.clone());
                }
            }
        });
    }
    if let Some(pi) = add_to {
        if let Some(p) = app.prefs.gradient_palettes.get_mut(pi) {
            p.gradients.push(g.clone());
        }
    }
    if let Some(pi) = remove {
        app.prefs.gradient_palettes.remove(pi);
    }
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 9.0);
    let current_for_palette = g.clone();
    new_palette_row(ui, egui::Id::unique("gradient_new_palette"), |name| {
        app.prefs.gradient_palettes.push(NamedGradients {
            name,
            gradients: vec![current_for_palette],
        });
    });
    if let Some(p) = picked {
        g = p;
        app.apply_fill_gradient(g.clone(), true);
    }

    ui.add_space(2.0);
    transparency_row(app, ui, opacity);

    // Editor de paradas
    ui.separator();
    section(ui, "Colores del degradado");
    let sel_id = egui::Id::unique("fill_gradient_sel_stop");
    let mut sel: usize = ui
        .data(|d| d.get_temp(sel_id))
        .unwrap_or(0)
        .min(g.stops.len().saturating_sub(1));
    let (edited, started) = stops_editor(ui, &mut g, &mut sel);
    let mut changed = edited;
    let mut commit = started;
    // Color de la parada seleccionada
    if let Some(stop) = g.stops.get(sel).copied() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
            for c in super::PALETTE {
                if small_swatch(ui, Some(c), stop.color == c, &c.to_hex()) {
                    g.stops[sel].color = c;
                    changed = true;
                    commit = true;
                }
            }
        });
        let mut c = c32(stop.color);
        if spectrum(ui, egui::Id::unique(("gradient_spectrum", sel)), &mut c) {
            g.stops[sel].color = from32(c);
            changed = true;
            commit |= ui.input(|i| i.pointer.primary_pressed());
        }
        if let Some(c) = color_fields(ui, egui::Id::unique("gradient_stop_fields"), stop.color) {
            g.stops[sel].color = c;
            changed = true;
            commit = true;
        }
    }

    // Girar degradado
    ui.separator();
    section(ui, "Girar degradado");
    ui.label(RichText::new("Ángulo").color(theme::CHROME_200));
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = ui.available_width() - 104.0;
        let r = ui.add(egui::Slider::new(&mut g.angle_deg, -180.0..=180.0).show_value(false));
        changed |= r.changed();
        commit |= r.drag_started();
        let r = ui.add(
            egui::DragValue::new(&mut g.angle_deg)
                .range(-180.0..=180.0)
                .fixed_decimals(1)
                .suffix(" °"),
        );
        changed |= r.changed();
        commit |= r.drag_started();
    });

    if changed && has_sel {
        app.apply_fill_gradient(g.clone(), commit);
    }
    ui.data_mut(|d| {
        d.insert_temp(work_id, g);
        d.insert_temp(sel_id, sel);
    });
}

/// Barra del degradado con paradas arrastrables. Clic en la barra añade una parada; × quita la seleccionada.
/// Devuelve (cambió, empezó un gesto que merece historial).
fn stops_editor(ui: &mut Ui, g: &mut Gradient, sel: &mut usize) -> (bool, bool) {
    let mut changed = false;
    let mut started = false;
    let width = ui.available_width() - 30.0;
    ui.horizontal(|ui| {
        let (rect, bar) = ui.allocate_exact_size(Vec2::new(width, 40.0), Sense::click());
        let bar_rect = Rect::from_min_size(rect.min, Vec2::new(width, 24.0));
        paint_gradient(ui, bar_rect, g);
        ui.painter().rect_stroke(
            bar_rect,
            2.0,
            Stroke::new(1.0, theme::CHROME_500),
            StrokeKind::Inside,
        );
        // Paradas
        for i in 0..g.stops.len() {
            let x = bar_rect.left() + bar_rect.width() * g.stops[i].pos as f32;
            let marker = Rect::from_center_size(
                Pos2::new(x, bar_rect.bottom() + 8.0),
                Vec2::new(12.0, 14.0),
            );
            let id = ui.scope_id().with(("stop", i));
            let resp = ui.interact(marker.expand(2.0), id, Sense::click_and_drag());
            if resp.drag_started() || resp.clicked() {
                *sel = i;
            }
            if resp.drag_started() {
                started = true;
            }
            if resp.dragged() {
                let nx = (resp.interact_pointer_pos().map(|p| p.x).unwrap_or(x) - bar_rect.left())
                    / bar_rect.width();
                g.stops[i].pos = nx.clamp(0.0, 1.0) as f64;
                changed = true;
            }
            let selected = *sel == i;
            let p = ui.painter();
            p.line_segment(
                [
                    Pos2::new(x, bar_rect.top()),
                    Pos2::new(x, bar_rect.bottom()),
                ],
                Stroke::new(
                    1.0,
                    if selected {
                        theme::ACCENT
                    } else {
                        Color32::from_black_alpha(140)
                    },
                ),
            );
            p.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(x, bar_rect.bottom()),
                    Pos2::new(x + 6.0, marker.top() + 4.0),
                    Pos2::new(x - 6.0, marker.top() + 4.0),
                ],
                if selected {
                    theme::ACCENT
                } else {
                    Color32::WHITE
                },
                Stroke::NONE,
            ));
            let swatch = Rect::from_min_max(
                Pos2::new(x - 6.0, marker.top() + 4.0),
                Pos2::new(x + 6.0, marker.bottom()),
            );
            p.rect(
                swatch,
                1.0,
                c32(g.stops[i].color),
                Stroke::new(
                    1.5,
                    if selected {
                        theme::ACCENT
                    } else {
                        Color32::WHITE
                    },
                ),
                StrokeKind::Inside,
            );
        }
        if bar.clicked() {
            if let Some(pos) = bar.interact_pointer_pos() {
                if bar_rect.contains(pos) {
                    let t = ((pos.x - bar_rect.left()) / bar_rect.width()).clamp(0.0, 1.0) as f64;
                    let color = g.sample(t);
                    g.stops.push(GradientStop { pos: t, color });
                    *sel = g.stops.len() - 1;
                    changed = true;
                    started = true;
                }
            }
        }
        let can_remove = g.stops.len() > 2;
        if ui
            .add_enabled(
                can_remove,
                egui::Button::new(mi::ICON_CLOSE.rich_text().size(14.0))
                    .min_size(Vec2::splat(22.0)),
            )
            .on_hover_text("Quitar la parada seleccionada")
            .clicked()
        {
            g.stops.remove(*sel);
            *sel = sel.saturating_sub(1);
            changed = true;
            started = true;
        }
    });
    ui.label(
        RichText::new(
            "Clic en la barra para añadir un color; arrastra los marcadores para moverlos.",
        )
        .small()
        .color(theme::CHROME_400),
    );
    (changed, started)
}
