//! Panel Herramientas especiales, con las pestañas, opciones, valores y orden de la referencia:
//! Relieve · Puntuar y grabar en relieve · Grabador eléctrico · Lápiz térmico · Punzón de cuero ·
//! Tallador de cuero · Enviar a S3D.

use crate::app::{OpenCraftCutterApp, Panel};
use crate::theme;
use egui::{Color32, Pos2, RichText, Sense, Stroke as EStroke, StrokeKind, Ui, Vec2 as EVec2};
use egui_material_icons::{MaterialIcon, icons as mi};
use kurbo::Point;
use oc_core::shape::Shape;
use oc_core::special::{self, Pattern, Tool, ToolFill};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Emboss,
    ScoreEmboss,
    Engraver,
    HeatPen,
    LeatherPunch,
    LeatherCarver,
    S3d,
}

/// Ajustes del panel (los de la referencia por defecto).
#[derive(Clone, Copy)]
struct State {
    tab: Tab,
    score_wide: bool,
    score_passes_on: bool,
    score_passes: u32,
    score_fill: bool,
    punch_size: usize,
    punch_spacing_mm: f64,
}

impl Default for State {
    fn default() -> Self {
        State {
            tab: Tab::Emboss,
            score_wide: false,
            score_passes_on: true,
            score_passes: 1,
            score_fill: false,
            punch_size: 1,
            punch_spacing_mm: 3.5,
        }
    }
}

/// Espaciado por defecto de cada herramienta (mm): 0,200 cm en relieve, 0,040 cm en el resto.
fn default_spacing(tool: Tool) -> f64 {
    match tool {
        Tool::Emboss | Tool::ScoreEmboss => 2.0,
        _ => 0.4,
    }
}

fn tool_of(tab: Tab) -> Option<Tool> {
    match tab {
        Tab::Emboss => Some(Tool::Emboss),
        Tab::Engraver => Some(Tool::Engraver),
        Tab::HeatPen => Some(Tool::HeatPen),
        Tab::LeatherCarver => Some(Tool::LeatherCarver),
        _ => None,
    }
}

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::CHROME_200));
}

fn heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(14.5).color(theme::CHROME_100));
}

fn wide_button(ui: &mut Ui, icon: MaterialIcon, text: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(format!("{}   {text}", <&str>::from(icon)))
            .min_size(EVec2::new(ui.available_width(), 34.0)),
    )
    .clicked()
}

/// Dibujo propio de cada patrón (miniatura de 22 px).
fn pattern_glyph(p: &egui::Painter, r: egui::Rect, pat: Pattern, color: Color32) {
    let st = EStroke::new(1.2, color);
    let c = r.center();
    let h = r.width() / 2.0;
    match pat {
        Pattern::None => {
            p.rect_stroke(r, 1.0, st, StrokeKind::Inside);
            p.line_segment([r.left_top(), r.right_bottom()], st);
            p.line_segment([r.right_top(), r.left_bottom()], st);
        }
        Pattern::CrossHatch | Pattern::Lined => {
            p.circle_stroke(c, h, st);
            for i in -3..=3 {
                let o = i as f32 * h / 3.5;
                let w = (h * h - o * o).max(0.0).sqrt();
                p.line_segment([c + EVec2::new(-w, o), c + EVec2::new(w, o)], st);
                if pat == Pattern::CrossHatch {
                    p.line_segment([c + EVec2::new(o, -w), c + EVec2::new(o, w)], st);
                }
            }
        }
        Pattern::Concentric => {
            for k in 1..=4 {
                p.circle_stroke(c, h * k as f32 / 4.0, st);
            }
        }
        Pattern::Spiral => {
            p.rect_stroke(r, 1.0, st, StrokeKind::Inside);
            let pts: Vec<Pos2> = (0..120)
                .map(|i| {
                    let t = i as f32 / 120.0 * 4.0 * std::f32::consts::TAU;
                    let rr = h * 0.9 * t / (4.0 * std::f32::consts::TAU);
                    c + EVec2::new(rr * t.cos(), rr * t.sin())
                })
                .collect();
            p.add(egui::Shape::line(pts, st));
        }
    }
}

/// Fila de los cinco patrones; devuelve el elegido si cambia.
fn pattern_row(ui: &mut Ui, current: Pattern, enabled: bool) -> Option<Pattern> {
    let mut out = None;
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 4.0 * ui.spacing().item_spacing.x) / 5.0;
        for pat in Pattern::ALL {
            let (rect, resp) = ui.allocate_exact_size(EVec2::new(w, 40.0), Sense::click());
            let resp = resp.on_hover_text(pat.name());
            if !ui.is_enabled() || !enabled {
                pattern_glyph(
                    ui.painter(),
                    egui::Rect::from_center_size(rect.center(), EVec2::splat(22.0)),
                    pat,
                    theme::CHROME_500,
                );
                continue;
            }
            if pat == current {
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(rect.center(), EVec2::splat(36.0)),
                    6.0,
                    theme::CHROME_900,
                );
            } else if resp.hovered() {
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(rect.center(), EVec2::splat(36.0)),
                    6.0,
                    theme::CHROME_700,
                );
            }
            pattern_glyph(
                ui.painter(),
                egui::Rect::from_center_size(rect.center(), EVec2::splat(22.0)),
                pat,
                Color32::WHITE,
            );
            if resp.clicked() && pat != current {
                out = Some(pat);
            }
        }
    });
    out
}

fn slider(ui: &mut Ui, label: &str, enabled: bool, add: impl FnOnce(&mut Ui) -> bool) -> bool {
    let mut changed = false;
    ui.add_enabled_ui(enabled, |ui| {
        ui.label(RichText::new(label).color(theme::CHROME_200));
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 112.0).max(60.0);
            changed = add(ui);
        });
    });
    changed
}

impl OpenCraftCutterApp {
    /// Formas seleccionadas a las que se puede aplicar un relleno (cerradas).
    fn special_targets(&self) -> Vec<Shape> {
        let raw: Vec<Shape> = self
            .doc
            .selected_shapes()
            .into_iter()
            .filter(|s| !s.is_image() && s.punch_mm.is_none())
            .cloned()
            .collect();
        crate::text::paths_for_ops(&raw)
    }

    /// Aplica (o actualiza) el relleno de herramienta a la selección. `commit` crea una
    /// entrada de historial (no al arrastrar deslizadores).
    fn set_tool_fill(&mut self, f: impl Fn(&mut ToolFill), tool: Tool, commit: bool) {
        let ids: Vec<u64> = self.doc.selected.clone();
        if ids.is_empty() {
            return;
        }
        if commit {
            self.commit();
        }
        // Los textos se convierten en trazado para poder rellenarlos.
        let texts: Vec<Shape> = self
            .doc
            .selected_shapes()
            .into_iter()
            .filter(|s| s.is_text())
            .cloned()
            .collect();
        if !texts.is_empty() {
            let paths = crate::text::paths_for_ops(&texts);
            let tids: Vec<u64> = texts.iter().map(|s| s.id).collect();
            for (t, p) in tids.iter().zip(paths) {
                if let Some(s) = self.doc.shape_mut(*t) {
                    let id = s.id;
                    *s = p;
                    s.id = id;
                }
            }
        }
        for id in ids {
            if let Some(s) = self.doc.shape_mut(id) {
                if s.is_image() || s.punch_mm.is_some() {
                    continue;
                }
                let mut tf = s.tool_fill.filter(|t| t.tool == tool).unwrap_or(ToolFill {
                    tool,
                    pattern: Pattern::None,
                    spacing_mm: default_spacing(tool),
                    angle_deg: 0.0,
                    emboss: false,
                    passes: 1,
                });
                f(&mut tf);
                s.tool_fill = Some(tf);
            }
        }
    }

    /// «Liberar»: convierte las líneas del relleno en un trazado y quita el relleno editable.
    fn release_tool_fill(&mut self, tool: Tool) {
        let sel: Vec<Shape> = self.doc.selected_shapes().into_iter().cloned().collect();
        let with: Vec<&Shape> = sel
            .iter()
            .filter(|s| s.tool_fill.is_some_and(|t| t.tool == tool))
            .collect();
        if with.is_empty() {
            return;
        }
        self.commit();
        let mut new_ids = Vec::new();
        for s in with {
            let f = s.tool_fill.unwrap();
            let lines = special::pattern_lines(s, f.pattern, f.spacing_mm, f.angle_deg);
            if let Some(mut p) = oc_core::ops::shape_from_lines(
                s,
                &lines,
                &format!("{} ({})", s.name, tool.name().to_lowercase()),
            ) {
                p.fill = None;
                p.stroke.color = oc_core::Color::rgb(0x40, 0x40, 0xff);
                new_ids.push(self.doc.add_shape(p));
            }
            if let Some(m) = self.doc.shape_mut(s.id) {
                m.tool_fill = None;
            }
        }
        self.doc.selected.extend(new_ids);
    }

    fn add_punch_points(&mut self, diameter: f64, spacing: f64) {
        let targets: Vec<Shape> = self
            .doc
            .selected_shapes()
            .into_iter()
            .filter(|s| s.punch_mm.is_none() && !s.is_image())
            .cloned()
            .collect();
        let paths = crate::text::paths_for_ops(&targets);
        let mut circles: Vec<(Vec<Point>, bool)> = Vec::new();
        for s in &paths {
            for c in special::punch_points(s, spacing) {
                circles.push((special::circle(c, diameter), true));
            }
        }
        if circles.is_empty() {
            return;
        }
        self.commit();
        let ids: Vec<u64> = targets.iter().map(|s| s.id).collect();
        self.doc.remove_shapes(&ids);
        self.add_punch_shape(circles, diameter);
    }

    /// Añade un trazado de puntos de perforación (azul) y lo selecciona.
    pub fn add_punch_shape(&mut self, circles: Vec<(Vec<Point>, bool)>, diameter: f64) {
        let (rect, subpaths) = Shape::normalize_subpaths(&circles);
        let mut s = Shape::new(
            0,
            "Puntos de perforación",
            rect,
            oc_core::ShapeKind::Path {
                subpaths,
                fill_rule: oc_core::FillRule::NonZero,
            },
        );
        s.stroke.color = oc_core::Color::rgb(0x40, 0x40, 0xff);
        s.punch_mm = Some(diameter);
        let id = self.doc.add_shape(s);
        self.doc.selected = vec![id];
    }

    fn remove_punch_points(&mut self) {
        let ids: Vec<u64> = self
            .doc
            .selected_shapes()
            .into_iter()
            .filter(|s| s.punch_mm.is_some())
            .map(|s| s.id)
            .collect();
        if !ids.is_empty() {
            self.commit();
            self.doc.remove_shapes(&ids);
        }
    }
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let id = egui::Id::unique("special_tools_state");
    let mut st: State = ui.data(|d| d.get_temp(id)).unwrap_or_default();

    // Pestañas (iconos propios de OpenCraftCutter, en el orden de la referencia).
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (tab, icon, tip) in [
            (Tab::Emboss, mi::ICON_TEXTURE, "Relieve"),
            (
                Tab::ScoreEmboss,
                mi::ICON_APPROVAL,
                "Puntuar y grabar en relieve",
            ),
            (
                Tab::Engraver,
                mi::ICON_PRECISION_MANUFACTURING,
                "Grabador eléctrico",
            ),
            (
                Tab::HeatPen,
                mi::ICON_LOCAL_FIRE_DEPARTMENT,
                "Lápiz térmico",
            ),
            (Tab::LeatherPunch, mi::ICON_SCATTER_PLOT, "Punzón de cuero"),
            (Tab::LeatherCarver, mi::ICON_CARPENTER, "Tallador de cuero"),
            (Tab::S3d, mi::ICON_VIEW_IN_AR, "Enviar a S3D"),
        ] {
            let r = ui
                .add(
                    egui::Button::selectable(st.tab == tab, icon.rich_text().size(19.0))
                        .min_size(EVec2::splat(31.0)),
                )
                .on_hover_text(tip);
            crate::theme::name_widget(&r, tip);
            if r.clicked() {
                st.tab = tab;
                if tab != Tab::LeatherPunch {
                    app.canvas.punch_draw = None;
                }
            }
        }
    });
    ui.separator();

    let targets = app.special_targets();
    let has_sel = !targets.is_empty();
    let unit = app.doc.page.unit;
    let suffix = format!(" {}", unit.label());

    match st.tab {
        Tab::Emboss | Tab::Engraver | Tab::HeatPen | Tab::LeatherCarver => {
            let tool = tool_of(st.tab).unwrap();
            match st.tab {
                Tab::Engraver => {
                    heading(ui, "Rellenos para grabador eléctrico");
                    hint(
                        ui,
                        "Usa el grabador giratorio para grabar patrones en materiales sólidos, como la madera.",
                    );
                }
                Tab::HeatPen => {
                    heading(ui, "Rellenos para lápices térmicos");
                    hint(
                        ui,
                        "Cómo usar el lápiz térmico con papel de aluminio activado por calor para añadir efectos dorados a los diseños.",
                    );
                }
                Tab::LeatherCarver => {
                    heading(ui, "Tallador de cuero");
                    hint(
                        ui,
                        "Usa la herramienta de tallado de cuero para grabar patrones en cuero fino y cuero sintético.",
                    );
                }
                _ => {}
            }
            // Valores de la primera forma con este relleno (o los por defecto).
            let current = targets
                .iter()
                .find_map(|s| {
                    app.doc
                        .shape(s.id)
                        .and_then(|s| s.tool_fill)
                        .filter(|t| t.tool == tool)
                })
                .unwrap_or(ToolFill {
                    tool,
                    pattern: Pattern::None,
                    spacing_mm: default_spacing(tool),
                    angle_deg: 0.0,
                    emboss: false,
                    passes: 1,
                });
            ui.add_enabled_ui(has_sel, |ui| {
                if st.tab == Tab::Emboss {
                    // Bajo relieve / Relieve.
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                        for (emboss, icon, label, tip) in [
                            (false, mi::ICON_STAR, "Bajo relieve", "Presiona el diseño hacia dentro, tal como se ve (material boca arriba)"),
                            (true, mi::ICON_STAR_HALF, "Relieve", "El diseño sale en alto: se envía reflejado (material boca abajo)"),
                        ] {
                            if ui
                                .add(egui::Button::selectable(current.emboss == emboss, format!("{}   {label}", <&str>::from(icon))).frame(true).min_size(EVec2::new(w, 36.0)))
                                .on_hover_text(tip)
                                .clicked()
                                && current.emboss != emboss
                            {
                                app.set_tool_fill(|t| t.emboss = emboss, tool, true);
                            }
                        }
                    });
                    ui.add_space(4.0);
                    hint(ui, "Patrones de estampado");
                }
                if let Some(p) = pattern_row(ui, current.pattern, true) {
                    app.set_tool_fill(|t| t.pattern = p, tool, true);
                }
                let patterned = current.pattern != Pattern::None;
                let pressed = ui.ctx().input(|i| i.pointer.primary_pressed());
                let mut sp = unit.from_mm(current.spacing_mm);
                let max_sp = unit.from_mm(if tool == Tool::Emboss { 6.0 } else { 3.0 });
                if slider(ui, "Espaciado", patterned, |ui| {
                    let a = ui.add(egui::Slider::new(&mut sp, unit.from_mm(0.1)..=max_sp).show_value(false)).changed();
                    let b = ui
                        .add(egui::DragValue::new(&mut sp).range(unit.from_mm(0.1)..=unit.from_mm(20.0)).speed(unit.step() / 4.0).fixed_decimals(unit.decimals() + 1).suffix(suffix.clone()))
                        .changed();
                    a || b
                }) {
                    let v = unit.to_mm(sp);
                    app.set_tool_fill(|t| t.spacing_mm = v, tool, pressed);
                }
                let mut ang = current.angle_deg;
                if slider(ui, "Ángulo", patterned, |ui| {
                    let a = ui.add(egui::Slider::new(&mut ang, -90.0..=90.0).show_value(false)).changed();
                    let b = ui.add(egui::DragValue::new(&mut ang).range(-360.0..=360.0).fixed_decimals(1).suffix(" °")).changed();
                    a || b
                }) {
                    app.set_tool_fill(|t| t.angle_deg = ang, tool, pressed);
                }
                let label = match tool {
                    Tool::Emboss => "Efectos de liberación",
                    Tool::Engraver => "Quitar el relleno del grabado",
                    Tool::HeatPen => "Quitar el relleno con lápiz térmico",
                    _ => "Liberar el relleno del tallador",
                };
                let releasable = targets.iter().any(|s| app.doc.shape(s.id).and_then(|s| s.tool_fill).is_some_and(|t| t.tool == tool && t.pattern != Pattern::None));
                if wide_button(ui, mi::ICON_GRID_4X4, label, releasable) {
                    app.release_tool_fill(tool);
                }
            });
            if !has_sel {
                ui.label(
                    RichText::new("Selecciona una o más formas.")
                        .small()
                        .color(theme::CHROME_400),
                );
            }
        }
        Tab::ScoreEmboss => {
            heading(ui, "Puntuar y grabar en relieve");
            ui.add_enabled_ui(has_sel, |ui| {
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                    for (wide, label, tip) in [
                        (false, "Fina", "Punta fina de 1 mm"),
                        (true, "Ancha", "Punta ancha de 3 mm"),
                    ] {
                        if ui
                            .add(
                                egui::Button::selectable(
                                    st.score_wide == wide,
                                    format!("{}   {label}", <&str>::from(mi::ICON_PEN_SIZE_2)),
                                )
                                .frame(true)
                                .min_size(EVec2::new(w, 36.0)),
                            )
                            .on_hover_text(tip)
                            .clicked()
                        {
                            st.score_wide = wide;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut st.score_passes_on, "Pasadas");
                    ui.add_enabled_ui(st.score_passes_on, |ui| {
                        ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(40.0);
                        ui.add(egui::Slider::new(&mut st.score_passes, 1..=10).show_value(false));
                        ui.add(egui::DragValue::new(&mut st.score_passes).range(1..=10));
                    });
                });
                ui.checkbox(&mut st.score_fill, "Relleno");
                if wide_button(
                    ui,
                    mi::ICON_GRID_4X4,
                    "Aplicar muescas y líneas de gofrado",
                    has_sel,
                ) {
                    // El contorno queda como corte (soporte 1, cuchilla) y una copia hacia dentro
                    // (media punta) como línea de relieve (soporte 2).
                    let tip = if st.score_wide { 3.0 } else { 1.0 };
                    let passes = if st.score_passes_on {
                        st.score_passes
                    } else {
                        1
                    };
                    let fill = st.score_fill;
                    let mut out = Vec::new();
                    for s in &targets {
                        if let Some(mut inner) =
                            oc_core::ops::offset(s, -tip / 2.0, oc_core::ops::Join::Round)
                        {
                            inner.name = format!("{} (relieve)", s.name);
                            inner.fill = None;
                            inner.stroke.color = oc_core::Color::rgb(0x40, 0x40, 0xff);
                            inner.tool_fill = Some(ToolFill {
                                tool: Tool::ScoreEmboss,
                                pattern: if fill {
                                    Pattern::Concentric
                                } else {
                                    Pattern::None
                                },
                                spacing_mm: tip * 0.8,
                                angle_deg: 0.0,
                                emboss: false,
                                passes,
                            });
                            out.push(inner);
                        }
                    }
                    if !out.is_empty() {
                        app.commit();
                        let ids: Vec<u64> = out.into_iter().map(|s| app.doc.add_shape(s)).collect();
                        app.doc.selected.extend(ids);
                    }
                }
            });
            ui.label(RichText::new("La cuchilla (soporte 1) marca el contorno y la herramienta de relieve (soporte 2) presiona por dentro.").small().color(theme::CHROME_400));
        }
        Tab::LeatherPunch => {
            heading(ui, "Punzón de cuero");
            hint(
                ui,
                "Perfora orificios en cuero sintético con facilidad con la herramienta para perforar cuero.",
            );
            hint(
                ui,
                "Convierte una línea en perforaciones con el botón «Añadir puntos de perforación». El círculo azul muestra el radio del orificio de perforación.",
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Tamaño del punzón").color(theme::CHROME_200));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for i in (0..special::PUNCH_SIZES.len()).rev() {
                        let d = special::PUNCH_SIZES[i];
                        let (rect, resp) =
                            ui.allocate_exact_size(EVec2::new(48.0, 44.0), Sense::click());
                        if st.punch_size == i {
                            ui.painter().rect_filled(rect, 6.0, theme::CHROME_900);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, 6.0, theme::CHROME_700);
                        }
                        ui.painter().circle_stroke(
                            rect.center() - EVec2::new(0.0, 8.0),
                            2.0 + d as f32 * 1.3,
                            EStroke::new(1.4, Color32::WHITE),
                        );
                        ui.painter().text(
                            rect.center_bottom() - EVec2::new(0.0, 4.0),
                            egui::Align2::CENTER_BOTTOM,
                            format!("{} mm", d),
                            egui::FontId::proportional(11.5),
                            theme::CHROME_100,
                        );
                        if resp.clicked() {
                            st.punch_size = i;
                            if app.canvas.punch_draw.is_some() {
                                app.canvas.punch_draw = Some(d);
                            }
                        }
                    }
                });
            });
            let d = special::PUNCH_SIZES[st.punch_size];
            let drawing = app.canvas.punch_draw.is_some();
            if ui
                .add(
                    egui::Button::selectable(
                        drawing,
                        format!(
                            "{}   Dibujar puntos de perforación",
                            <&str>::from(mi::ICON_DRAW)
                        ),
                    )
                    .frame(true)
                    .min_size(EVec2::new(ui.available_width(), 34.0)),
                )
                .on_hover_text("Haz clic en el lienzo para colocar cada punto; Esc para terminar")
                .clicked()
            {
                app.canvas.punch_draw = if drawing { None } else { Some(d) };
            }
            let can_add = targets.iter().any(|s| s.punch_mm.is_none());
            if wide_button(
                ui,
                mi::ICON_SCATTER_PLOT,
                "Añadir puntos de perforación",
                can_add,
            ) {
                app.add_punch_points(d, st.punch_spacing_mm);
            }
            let mut sp = unit.from_mm(st.punch_spacing_mm);
            if slider(ui, "Espaciado", true, |ui| {
                let a = ui
                    .add(
                        egui::Slider::new(&mut sp, unit.from_mm(d + 0.5)..=unit.from_mm(30.0))
                            .show_value(false),
                    )
                    .changed();
                let b = ui
                    .add(
                        egui::DragValue::new(&mut sp)
                            .range(unit.from_mm(d + 0.5)..=unit.from_mm(100.0))
                            .speed(unit.step() / 4.0)
                            .fixed_decimals(unit.decimals() + 1)
                            .suffix(suffix.clone()),
                    )
                    .changed();
                a || b
            }) {
                st.punch_spacing_mm = unit.to_mm(sp).max(d + 0.5);
            }
            let has_punch = app
                .doc
                .selected_shapes()
                .iter()
                .any(|s| s.punch_mm.is_some());
            if ui
                .add_enabled(
                    has_punch,
                    egui::Button::new("Quitar puntos de perforación")
                        .min_size(EVec2::new(ui.available_width(), 34.0)),
                )
                .clicked()
            {
                app.remove_punch_points();
            }
        }
        Tab::S3d => {
            heading(ui, "Enviar a S3D");
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("Tienes más información sobre la serie Alta en")
                        .color(theme::CHROME_200),
                );
                ui.hyperlink_to(
                    "www.Silhouette101.com/alta",
                    "https://www.silhouette101.com/alta",
                );
            });
            if wide_button(ui, mi::ICON_VIEW_IN_AR, "Enviar a S3D", true) {
                // Sin la aplicación S3D, se exporta el diseño en SVG para abrirlo allí.
                let ctx = ui.ctx().clone();
                app.export_svg(&ctx);
            }
            if wide_button(
                ui,
                mi::ICON_DESCRIPTION,
                "Abrir Configuración de la página",
                true,
            ) {
                app.panel = Some(Panel::PageSetup);
            }
        }
    }
    ui.data_mut(|d| d.insert_temp(id, st));
}
