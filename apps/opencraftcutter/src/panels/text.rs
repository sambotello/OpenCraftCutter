//! Panel Estilo de texto, con las opciones y el orden de la referencia: fuente (búsqueda,
//! tamaño y lista con vista previa en cada fuente), negrita/cursiva/subrayado, estilo,
//! alineación (incl. justificado), dirección horizontal/vertical, y espaciado de líneas,
//! de caracteres e interletraje.

use super::section;
use crate::app::OpenCraftCutterApp;
use crate::text::{TextStyle, measure};
use crate::theme;
use egui::{Color32, RichText, Sense, Stroke as EStroke, Ui, Vec2 as EVec2};
use egui_material_icons::icons as mi;
use oc_core::shape::{ShapeKind, TextAlign, TextFx};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// Valores editables del panel (los del primer texto seleccionado).
#[derive(Clone, PartialEq)]
struct Values {
    family: String,
    size: f64,
    align: TextAlign,
    bold: bool,
    italic: bool,
    line_height: f64,
    fx: TextFx,
}

/// Estado del panel entre fotogramas.
#[derive(Clone, Default)]
struct PanelState {
    search: String,
    preview: String,
    editing: bool,
}

const SIZES: [f64; 16] = [
    6.0, 8.0, 9.0, 10.0, 12.0, 14.0, 18.0, 24.0, 36.0, 48.0, 60.0, 72.0, 96.0, 144.0, 200.0, 288.0,
];
const ROW_H: f32 = 26.0;

thread_local! {
    /// Contornos de vista previa por (fuente, texto), en mm anclados en (0,0).
    static PREVIEWS: RefCell<HashMap<(String, String), crate::text::Outlines>> =
        RefCell::new(HashMap::new());
}

fn preview_outlines(family: &str, text: &str) -> crate::text::Outlines {
    let key = (family.to_string(), text.to_string());
    if let Some(p) = PREVIEWS.with(|c| c.borrow().get(&key).cloned()) {
        return p;
    }
    let style = TextStyle {
        text,
        font_family: family,
        font_size_pt: 36.0,
        align: TextAlign::Left,
        bold: false,
        italic: false,
        line_height: 1.2,
        fx: TextFx::default(),
    };
    let paths = Arc::new(crate::text::outlines(&style));
    PREVIEWS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 2000 {
            c.clear();
        }
        c.insert(key, paths.clone());
    });
    paths
}

/// Fila de la lista de fuentes: el texto dibujado con la propia fuente.
fn font_row(ui: &mut Ui, family: &str, text: &str, selected: bool) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(EVec2::new(ui.available_width(), ROW_H), Sense::click());
    let p = ui.painter_at(rect);
    if selected {
        p.rect_filled(rect, 3.0, theme::ACCENT_STRONG);
    } else if resp.hovered() {
        p.rect_filled(rect, 3.0, theme::CHROME_700);
    }
    let color = if selected {
        Color32::WHITE
    } else {
        theme::CHROME_100
    };
    let paths = preview_outlines(family, text);
    let height = paths
        .iter()
        .flat_map(|(pts, _)| pts.iter().map(|q| q.y))
        .fold(0.0_f64, f64::max);
    if height > 0.0 {
        // Misma escala en todas las filas: las mayúsculas a 36 pt (~9 mm) ocupan ~55 % del alto.
        let k = ROW_H as f64 * 0.55 / 9.0;
        let origin = rect.left_center() + EVec2::new(6.0, -(height * k) as f32 / 2.0);
        for (pts, closed) in paths.iter() {
            let line: Vec<egui::Pos2> = pts
                .iter()
                .map(|q| origin + EVec2::new((q.x * k) as f32, (q.y * k) as f32))
                .collect();
            if *closed {
                p.add(egui::Shape::closed_line(line, EStroke::new(1.0, color)));
            } else {
                p.add(egui::Shape::line(line, EStroke::new(1.0, color)));
            }
        }
    } else {
        // Fuente sin glifos para ese texto: su nombre en la fuente de la interfaz.
        p.text(
            rect.left_center() + EVec2::new(6.0, 0.0),
            egui::Align2::LEFT_CENTER,
            family,
            egui::FontId::proportional(13.0),
            color,
        );
    }
    resp.on_hover_text(family)
}

fn toggle_glyph(ui: &mut Ui, on: bool, icon: egui_material_icons::MaterialIcon, tip: &str) -> bool {
    let r = ui
        .add(
            egui::Button::selectable(on, icon.rich_text().size(18.0))
                .frame(true)
                .min_size(EVec2::new(32.0, 30.0)),
        )
        .on_hover_text(tip);
    crate::theme::name_widget(&r, tip);
    r.clicked()
}

fn pct_slider(
    ui: &mut Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
) -> bool {
    ui.label(RichText::new(label).color(theme::CHROME_200));
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 100.0).max(60.0);
        changed |= ui
            .add(egui::Slider::new(value, range.clone()).show_value(false))
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(value)
                    .range(range)
                    .fixed_decimals(1)
                    .suffix(" %"),
            )
            .changed();
    });
    changed
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let ids: Vec<u64> = app
        .doc
        .selected_shapes()
        .iter()
        .filter(|s| s.is_text())
        .map(|s| s.id)
        .collect();
    let Some(first) = ids.first().and_then(|id| app.doc.shape(*id)).cloned() else {
        ui.label(
            RichText::new("Selecciona un texto o crea uno con la herramienta Texto (T).")
                .small()
                .color(theme::CHROME_300),
        );
        return;
    };
    let ShapeKind::Text {
        font_family,
        font_size_pt,
        align,
        bold,
        italic,
        line_height,
        fx,
        ..
    } = first.kind.clone()
    else {
        return;
    };
    let before = Values {
        family: font_family,
        size: font_size_pt,
        align,
        bold,
        italic,
        line_height,
        fx,
    };
    let mut v = before.clone();
    let state_id = egui::Id::unique("text_panel_state");
    let mut st: PanelState = ui.data(|d| d.get_temp(state_id)).unwrap_or_default();

    section(ui, "Estilo de texto");
    // Fuente (búsqueda) · tamaño · unidad.
    ui.horizontal(|ui| {
        let w = ui.available_width() - 120.0;
        ui.add(
            egui::TextEdit::singleline(&mut st.search)
                .hint_text(v.family.as_str())
                .desired_width(w),
        )
        .on_hover_text("Buscar fuente");
        ui.add(
            egui::DragValue::new(&mut v.size)
                .speed(0.5)
                .range(1.0..=1000.0)
                .fixed_decimals(2),
        );
        ui.menu_button(mi::ICON_ARROW_DROP_DOWN.rich_text(), |ui| {
            for s in SIZES {
                if ui.selectable_label(v.size == s, format!("{s}")).clicked() {
                    v.size = s;
                    ui.close();
                }
            }
        });
        ui.label(RichText::new("pt").color(theme::CHROME_200));
    });

    // Lista de fuentes con vista previa.
    let families = app.font_families();
    let query = st.search.to_lowercase();
    let shown: Vec<&String> = families
        .iter()
        .filter(|f| query.is_empty() || f.to_lowercase().contains(&query))
        .collect();
    let sample = if st.preview.trim().is_empty() {
        None
    } else {
        Some(st.preview.clone())
    };
    egui::Frame::new()
        .fill(theme::CHROME_900)
        .corner_radius(4.0)
        .inner_margin(2.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("font_list")
                .max_height(ROW_H * 9.0)
                .auto_shrink([false, true])
                .show_rows(ui, ROW_H, shown.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for f in &shown[range] {
                        let text = sample.as_deref().unwrap_or(f.as_str());
                        if font_row(ui, f, text, **f == v.family).clicked() {
                            v.family = (*f).clone();
                        }
                    }
                });
        });
    // Fuente actual.
    egui::Frame::new()
        .fill(theme::CHROME_700)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(&v.family).strong().color(Color32::WHITE));
        });
    // Texto de vista previa.
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut st.preview)
                .hint_text("Introduce el texto de la vista previa aquí…")
                .desired_width(ui.available_width() - 36.0),
        );
        if ui
            .add(egui::Button::new(mi::ICON_BACKSPACE.rich_text()).min_size(EVec2::new(30.0, 24.0)))
            .on_hover_text("Borrar la vista previa")
            .clicked()
        {
            st.preview.clear();
        }
    });

    // Negrita, cursiva, subrayado · estilo.
    ui.horizontal(|ui| {
        if toggle_glyph(ui, v.bold, mi::ICON_FORMAT_BOLD, "Negrita") {
            v.bold = !v.bold;
        }
        if toggle_glyph(ui, v.italic, mi::ICON_FORMAT_ITALIC, "Cursiva") {
            v.italic = !v.italic;
        }
        if toggle_glyph(ui, v.fx.underline, mi::ICON_FORMAT_UNDERLINED, "Subrayado") {
            v.fx.underline = !v.fx.underline;
        }
        let label = match (v.bold, v.italic) {
            (false, false) => "Normal",
            (true, false) => "Negrita",
            (false, true) => "Cursiva",
            (true, true) => "Negrita cursiva",
        };
        egui::ComboBox::from_id_salt("text_face")
            .width(ui.available_width())
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (name, b, i) in [
                    ("Normal", false, false),
                    ("Negrita", true, false),
                    ("Cursiva", false, true),
                    ("Negrita cursiva", true, true),
                ] {
                    if ui
                        .selectable_label(v.bold == b && v.italic == i, name)
                        .clicked()
                    {
                        v.bold = b;
                        v.italic = i;
                    }
                }
            });
    });
    // Alineación · dirección.
    ui.horizontal(|ui| {
        for (a, icon, tip) in [
            (
                TextAlign::Left,
                mi::ICON_FORMAT_ALIGN_LEFT,
                "Alinear a la izquierda",
            ),
            (TextAlign::Center, mi::ICON_FORMAT_ALIGN_CENTER, "Centrar"),
            (
                TextAlign::Right,
                mi::ICON_FORMAT_ALIGN_RIGHT,
                "Alinear a la derecha",
            ),
            (
                TextAlign::Justify,
                mi::ICON_FORMAT_ALIGN_JUSTIFY,
                "Justificar",
            ),
        ] {
            if toggle_glyph(ui, v.align == a, icon, tip) {
                v.align = a;
            }
        }
        ui.add_space(8.0);
        if toggle_glyph(
            ui,
            !v.fx.vertical,
            mi::ICON_TEXT_ROTATION_NONE,
            "Texto horizontal",
        ) {
            v.fx.vertical = false;
        }
        if toggle_glyph(
            ui,
            v.fx.vertical,
            mi::ICON_TEXT_ROTATE_VERTICAL,
            "Texto vertical",
        ) {
            v.fx.vertical = true;
        }
    });

    ui.add_space(4.0);
    ui.separator();
    section(ui, "Espaciado de caracteres");
    let mut lh_pct = v.line_height / 1.2 * 100.0;
    let slid_lh = pct_slider(ui, "Espaciado de líneas", &mut lh_pct, 10.0..=400.0);
    if slid_lh {
        v.line_height = lh_pct / 100.0 * 1.2;
    }
    let slid_cs = pct_slider(
        ui,
        "Espaciado de caracteres",
        &mut v.fx.char_spacing,
        0.0..=400.0,
    );
    ui.horizontal(|ui| {
        ui.label(RichText::new("Interletraje").color(theme::CHROME_200));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (on, label, tip) in [
                (
                    true,
                    "AV",
                    "Interletraje activado (pares como «AV» más juntos)",
                ),
                (false, "A V", "Interletraje desactivado"),
            ] {
                if ui
                    .add(
                        egui::Button::selectable(
                            v.fx.kerning == on,
                            RichText::new(label).size(16.0),
                        )
                        .frame(true)
                        .min_size(EVec2::new(40.0, 28.0)),
                    )
                    .on_hover_text(tip)
                    .clicked()
                {
                    v.fx.kerning = on;
                }
            }
        });
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new("Doble clic sobre un texto en el lienzo para editarlo.")
            .small()
            .color(theme::CHROME_400),
    );

    // Aplicar a todos los textos seleccionados (una entrada de historial por gesto).
    let dragging = (slid_lh || slid_cs) && ui.ctx().input(|i| i.pointer.any_down());
    if v != before {
        if !st.editing {
            app.commit();
        }
        st.editing = dragging;
        for id in ids {
            if let Some(s) = app.doc.shape_mut(id) {
                if let ShapeKind::Text {
                    font_family,
                    font_size_pt,
                    align,
                    bold,
                    italic,
                    line_height,
                    fx,
                    ..
                } = &mut s.kind
                {
                    *font_family = v.family.clone();
                    *font_size_pt = v.size;
                    *align = v.align;
                    *bold = v.bold;
                    *italic = v.italic;
                    *line_height = v.line_height;
                    *fx = v.fx;
                }
                if let Some(ts) = TextStyle::from_kind(&s.kind) {
                    let (w, h) = measure(&ts);
                    s.rect = kurbo::Rect::new(s.rect.x0, s.rect.y0, s.rect.x0 + w, s.rect.y0 + h);
                }
            }
        }
    } else if !ui.ctx().input(|i| i.pointer.any_down()) {
        st.editing = false;
    }
    ui.data_mut(|d| d.insert_temp(state_id, st));
}

/// Ajustes de «Texto sobre trazado».
#[derive(Clone, Copy)]
struct PathSettings {
    position: f64,
    distance_mm: f64,
    flip: bool,
}

impl Default for PathSettings {
    fn default() -> Self {
        PathSettings {
            position: 0.5,
            distance_mm: 1.0,
            flip: false,
        }
    }
}

/// Letras de un texto con una fuente del sistema, cada una en su posición de la línea.
fn system_glyphs(style: &TextStyle) -> Result<Vec<oc_core::textpath::Glyph>, &'static str> {
    let run = oc_svg::glyph_run(
        style.font_family,
        style.font_size_pt,
        style.bold,
        style.italic,
        style.text,
    )
    .ok_or("No se pudo leer la fuente de este texto.")?;
    Ok(run
        .glyphs
        .iter()
        .map(|g| oc_core::textpath::Glyph {
            x_mm: g.x_mm,
            advance_mm: g.advance_mm,
            // Los contornos vienen relativos a cada letra: se llevan a su sitio en la línea.
            outline: g
                .outline
                .iter()
                .map(|np| {
                    let pts = np
                        .flatten(0.02)
                        .into_iter()
                        .map(|p| p + kurbo::Vec2::new(g.x_mm, 0.0))
                        .collect();
                    (pts, np.closed)
                })
                .collect(),
        })
        .collect())
}

impl OpenCraftCutterApp {
    /// Convierte el texto `text_id` en letras colocadas sobre la forma `path_id`. El texto pasa
    /// a ser un trazado (deshacer lo recupera).
    pub fn text_on_path(
        &mut self,
        text_id: u64,
        path_id: u64,
        position: f64,
        distance_mm: f64,
        flip: bool,
    ) -> Result<(), &'static str> {
        let (Some(text), Some(target)) = (
            self.doc.shape(text_id).cloned(),
            self.doc.shape(path_id).cloned(),
        ) else {
            return Err("Selecciona un texto y una forma.");
        };
        let Some(style) = TextStyle::from_kind(&text.kind) else {
            return Err("Selecciona un texto y una forma.");
        };
        if style.text.contains('\n') {
            return Err("El texto sobre un trazado tiene que ser de una sola línea.");
        }
        let glyphs = if style.font_family == oc_core::singleline::FAMILY {
            // La fuente de un trazo no es del sistema: sus letras salen de `singleline`.
            oc_core::singleline::path_glyphs(
                style.text,
                oc_core::units::pt_to_mm(style.font_size_pt),
            )
        } else {
            system_glyphs(&style)?
        };
        let to_pts = |c: &Vec<[f64; 2]>| c.iter().map(|p| kurbo::Point::new(p[0], p[1])).collect();
        let (line, closed): (Vec<kurbo::Point>, bool) =
            match oc_core::ops::closed_contours(&target).first() {
                Some(c) => (to_pts(c), true),
                None => match oc_core::ops::open_paths(&target).first() {
                    Some(c) => (to_pts(c), false),
                    None => return Err("La forma no tiene un trazado que seguir."),
                },
            };
        let placed =
            oc_core::textpath::place_on_path(&glyphs, &line, closed, position, distance_mm, flip);
        if placed.is_empty() {
            return Err("El texto no tiene letras visibles.");
        }
        let (rect, subpaths) = oc_core::Shape::normalize_subpaths(&placed);
        let mut s = oc_core::Shape::new(
            0,
            format!("{} (sobre trazado)", text.name),
            rect,
            ShapeKind::Path {
                subpaths,
                fill_rule: oc_core::FillRule::NonZero,
            },
        );
        s.fill = text.fill;
        s.stroke = text.stroke;
        s.cut = text.cut;
        self.commit();
        self.doc.remove_shapes(&[text_id]);
        let id = self.doc.add_shape(s);
        self.doc.selected = vec![id, path_id];
        Ok(())
    }
}

/// «Texto sobre trazado»: con un texto y una forma seleccionados.
pub fn path_section(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let sel = app.doc.selected_shapes();
    let texts: Vec<u64> = sel.iter().filter(|s| s.is_text()).map(|s| s.id).collect();
    let others: Vec<u64> = sel
        .iter()
        .filter(|s| !s.is_text() && !s.is_image())
        .map(|s| s.id)
        .collect();
    ui.separator();
    section(ui, "Texto sobre trazado");
    let ([t], [p]) = (texts.as_slice(), others.as_slice()) else {
        ui.label(
            RichText::new("Selecciona un texto y una forma (círculo, línea, curva…) para colocar las letras siguiendo su contorno.")
                .small()
                .color(theme::CHROME_300),
        );
        return;
    };
    let (t, p) = (*t, *p);
    let id = egui::Id::unique("text_on_path");
    let mut st: PathSettings = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let unit = app.doc.page.unit;
    ui.label(RichText::new("Posición").color(theme::CHROME_200));
    ui.add(
        egui::Slider::new(&mut st.position, 0.0..=1.0)
            .custom_formatter(|v, _| format!("{:.0} %", v * 100.0))
            .custom_parser(|s| {
                s.trim_end_matches('%')
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|v| v / 100.0)
            }),
    );
    ui.horizontal(|ui| {
        ui.label(RichText::new("Distancia").color(theme::CHROME_200));
        let mut d = unit.from_mm(st.distance_mm);
        if ui
            .add(
                egui::DragValue::new(&mut d)
                    .speed(unit.step() / 2.0)
                    .range(unit.from_mm(-20.0)..=unit.from_mm(50.0))
                    .fixed_decimals(unit.decimals() + 1)
                    .suffix(format!(" {}", unit.label())),
            )
            .changed()
        {
            st.distance_mm = unit.to_mm(d);
        }
    });
    ui.checkbox(&mut st.flip, "Del otro lado (por dentro)");
    if ui
        .add(
            egui::Button::new(format!(
                "{}  Colocar sobre el trazado",
                <&str>::from(mi::ICON_ROUTE)
            ))
            .min_size(EVec2::new(ui.available_width(), 30.0)),
        )
        .clicked()
    {
        if let Err(e) = app.text_on_path(t, p, st.position, st.distance_mm, st.flip) {
            let ctx = ui.ctx().clone();
            app.notify(&ctx, e);
        }
    }
    ui.label(
        RichText::new("El texto pasa a ser un trazado: edita el texto antes de colocarlo.")
            .small()
            .color(theme::CHROME_300),
    );
    ui.data_mut(|d| d.insert_temp(id, st));
}
