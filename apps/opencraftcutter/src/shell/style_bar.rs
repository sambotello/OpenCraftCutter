use crate::app::{OpenCraftCutterApp, Panel};
use crate::theme;
use egui::{Color32, Frame, RichText};
use oc_core::shape::LineStyle;

/// Muestra de color clicable; `None` se dibuja como tablero con raya roja.
pub fn color_swatch(
    ui: &mut egui::Ui,
    color: Option<oc_core::Color>,
    tooltip: &str,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    let p = ui.painter();
    match color {
        Some(c) => {
            p.rect_filled(
                rect,
                4.0,
                Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a),
            );
        }
        None => {
            p.rect_filled(rect, 4.0, Color32::from_gray(0xe0));
            p.line_segment(
                [rect.left_bottom(), rect.right_top()],
                egui::Stroke::new(1.5, Color32::RED),
            );
        }
    }
    p.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, theme::CHROME_500),
        egui::StrokeKind::Inside,
    );
    resp.on_hover_text(tooltip)
}

pub fn style_bar(app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    egui::Panel::top("style_bar")
        .exact_size(30.0)
        .frame(
            Frame::NONE
                .fill(theme::CHROME_800)
                .inner_margin(egui::Margin::symmetric(8, 4)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                // Editar puntos: la barra muestra sus acciones, como la referencia.
                if app.tool == crate::app::Tool::EditPoints {
                    use crate::panels::edit_points::{Action, run, state};
                    for a in Action::ALL {
                        let (enabled, active) = state(app, a);
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::selectable(active, a.icon().rich_text().size(17.0)),
                            )
                            .on_hover_text(a.label())
                            .on_disabled_hover_text(a.label())
                            .clicked()
                        {
                            run(app, a);
                        }
                    }
                    ui.separator();
                    if ui.button("Mostrar panel").clicked() {
                        app.panel = Some(Panel::EditPoints);
                    }
                    return;
                }
                let first = app
                    .doc
                    .selected_shapes()
                    .first()
                    .map(|s| (s.fill, s.stroke));
                let (fill, stroke) = first.unwrap_or((app.defaults.fill, app.defaults.stroke));
                if color_swatch(ui, fill, "Relleno").clicked() {
                    app.toggle_panel(Panel::Fill);
                }
                if color_swatch(ui, Some(stroke.color), "Color de línea").clicked() {
                    app.toggle_panel(Panel::Line);
                }
                let mut style = stroke.style;
                egui::ComboBox::from_id_salt("line_style")
                    .width(100.0)
                    .selected_text(style.name())
                    .show_ui(ui, |ui| {
                        for s in LineStyle::ALL {
                            ui.selectable_value(&mut style, s, s.name());
                        }
                    });
                let mut width = stroke.width_pt;
                ui.add(
                    egui::DragValue::new(&mut width)
                        .range(0.0..=100.0)
                        .speed(0.1)
                        .suffix(" pt")
                        .fixed_decimals(2),
                );
                if style != stroke.style || width != stroke.width_pt {
                    app.apply_stroke(|st| {
                        st.style = style;
                        st.width_pt = width;
                    });
                }
                if app.tool == crate::app::Tool::Eraser {
                    ui.separator();
                    ui.label("Borrador");
                    let unit = app.doc.page.unit;
                    let mut w = unit.from_mm(app.canvas.eraser_width_mm);
                    if ui
                        .add(
                            egui::DragValue::new(&mut w)
                                .speed(unit.step() / 2.0)
                                .range(unit.from_mm(0.5)..=unit.from_mm(50.0))
                                .fixed_decimals(unit.decimals())
                                .suffix(format!(" {}", unit.label())),
                        )
                        .changed()
                    {
                        app.canvas.eraser_width_mm = unit.to_mm(w);
                    }
                }
                if !app.doc.selected.is_empty() {
                    selection_tools(app, ui);
                }
            });
        });
}

impl OpenCraftCutterApp {
    /// Aplica un cambio de línea a la selección (con historial) y a los valores por defecto.
    pub fn apply_stroke(&mut self, f: impl Fn(&mut oc_core::shape::Stroke)) {
        if !self.doc.selected.is_empty() {
            self.commit();
            let ids = self.doc.selected.clone();
            for id in ids {
                if let Some(s) = self.doc.shape_mut(id) {
                    f(&mut s.stroke);
                }
            }
        }
        f(&mut self.defaults.stroke);
    }

    /// Igual que `apply_stroke`, pero sin historial (para arrastres; el primer paso usa `apply_stroke`).
    pub fn apply_stroke_live(&mut self, f: impl Fn(&mut oc_core::shape::Stroke)) {
        let ids = self.doc.selected.clone();
        for id in ids {
            if let Some(s) = self.doc.shape_mut(id) {
                f(&mut s.stroke);
            }
        }
        self.dirty = true;
        f(&mut self.defaults.stroke);
    }

    pub fn remember_line_color(&mut self, c: oc_core::Color) {
        let r = &mut self.prefs.recent_line_colors;
        r.retain(|x| *x != c);
        r.insert(0, c);
        r.truncate(10);
    }

    /// Aplica un relleno sólido (o ninguno) a la selección y a los valores por defecto.
    pub fn apply_fill(&mut self, fill: Option<oc_core::Color>) {
        if !self.doc.selected.is_empty() {
            self.commit();
            let ids = self.doc.selected.clone();
            for id in ids {
                if let Some(s) = self.doc.shape_mut(id) {
                    s.fill = fill;
                    s.fill_gradient = None;
                }
            }
        }
        self.defaults.fill = fill;
        if let Some(c) = fill {
            self.remember_fill_color(c);
        }
    }

    /// Como `apply_fill`, sin historial (arrastres por el espectro).
    pub fn apply_fill_live(&mut self, fill: Option<oc_core::Color>) {
        let ids = self.doc.selected.clone();
        for id in ids {
            if let Some(s) = self.doc.shape_mut(id) {
                s.fill = fill;
                s.fill_gradient = None;
            }
        }
        self.dirty = true;
        self.defaults.fill = fill;
        if let Some(c) = fill {
            self.remember_fill_color(c);
        }
    }

    /// Aplica un degradado a la selección.
    pub fn apply_fill_gradient(&mut self, g: oc_core::shape::Gradient, commit: bool) {
        if self.doc.selected.is_empty() {
            return;
        }
        if commit {
            self.commit();
        } else {
            self.dirty = true;
        }
        let first = g
            .sorted()
            .first()
            .map(|s| s.color)
            .unwrap_or(oc_core::Color::BLACK);
        let ids = self.doc.selected.clone();
        for id in ids {
            if let Some(s) = self.doc.shape_mut(id) {
                s.fill = Some(first);
                s.fill_gradient = Some(g.clone());
            }
        }
    }

    /// Cambia la opacidad del relleno de la selección (0..1). Sin historial por cada paso de arrastre.
    pub fn apply_fill_opacity(&mut self, opacity: f64, commit: bool) {
        if self.doc.selected.is_empty() {
            return;
        }
        if commit {
            self.commit();
        }
        let ids = self.doc.selected.clone();
        for id in ids {
            if let Some(s) = self.doc.shape_mut(id) {
                s.fill_opacity = opacity.clamp(0.0, 1.0);
            }
        }
        self.dirty = true;
    }

    pub fn remember_fill_color(&mut self, c: oc_core::Color) {
        let r = &mut self.prefs.recent_fill_colors;
        r.retain(|x| *x != c);
        r.insert(0, c);
        r.truncate(10);
    }
}

/// Medidas, posición y acciones rápidas de la selección (como la barra de la referencia).
fn selection_tools(app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    use egui_material_icons::icons as mi;
    use oc_core::arrange::{self, Anchor};
    let shapes: Vec<oc_core::Shape> = app
        .doc
        .selected_shapes()
        .into_iter()
        .filter(|s| !s.locked)
        .cloned()
        .collect();
    let Some(b) = oc_core::edit::selection_bounds(shapes.iter()) else {
        return;
    };
    let unit = app.doc.page.unit;
    let id = egui::Id::unique("selection_bar");
    let (mut lock, mut anchor): (bool, Anchor) = ui
        .data(|d| d.get_temp(id))
        .unwrap_or((true, Anchor::TopLeft));
    let field = |ui: &mut egui::Ui, label: &str, mm: f64, min: f64| -> Option<f64> {
        ui.label(RichText::new(label).color(theme::CHROME_300));
        let mut v = unit.from_mm(mm);
        let r = ui.add_sized(
            [62.0, 20.0],
            egui::DragValue::new(&mut v)
                .speed(unit.step())
                .range(unit.from_mm(min)..=f64::MAX)
                .fixed_decimals(unit.decimals() + 1),
        );
        r.changed().then(|| unit.to_mm(v))
    };
    let apply = |app: &mut OpenCraftCutterApp, out: Vec<oc_core::Shape>| {
        app.commit();
        for r in out {
            if let Some(s) = app.doc.shape_mut(r.id) {
                *s = r;
            }
        }
    };
    ui.separator();
    // Tamaño (an × al) con candado de proporción; fijo el punto de referencia.
    let w = field(ui, "An", b.width(), 0.01);
    let h = field(ui, "Al", b.height(), 0.01);
    ui.label(RichText::new(unit.label()).color(theme::CHROME_300));
    let lock_icon = if lock {
        mi::ICON_LOCK
    } else {
        mi::ICON_LOCK_OPEN
    };
    if ui
        .selectable_label(lock, lock_icon.rich_text())
        .on_hover_text("Mantener la proporción")
        .clicked()
    {
        lock = !lock;
    }
    if w.is_some() || h.is_some() {
        let (mut kx, mut ky) = (
            w.map_or(1.0, |v| v / b.width().max(1e-9)),
            h.map_or(1.0, |v| v / b.height().max(1e-9)),
        );
        if lock {
            let k = if w.is_some() { kx } else { ky };
            kx = k;
            ky = k;
        }
        let fixed = anchor.point(b);
        apply(app, arrange::scale_about(&shapes, kx, ky, fixed));
    }
    ui.separator();
    // Posición del punto de referencia.
    let p = anchor.point(b);
    let x = field(ui, "X", p.x, -10_000.0);
    let y = field(ui, "Y", p.y, -10_000.0);
    ui.label(RichText::new(unit.label()).color(theme::CHROME_300));
    if x.is_some() || y.is_some() {
        let target = kurbo::Point::new(x.unwrap_or(p.x), y.unwrap_or(p.y));
        apply(app, arrange::move_anchor_to(&shapes, anchor, target));
    }
    ui.menu_button(mi::ICON_CENTER_FOCUS_WEAK.rich_text(), |ui| {
        ui.label(
            RichText::new("Punto de referencia")
                .small()
                .color(theme::CHROME_300),
        );
        egui::Grid::new("anchor_grid")
            .spacing([2.0, 2.0])
            .show(ui, |ui| {
                for (i, a) in Anchor::ALL.iter().enumerate() {
                    let icon = if *a == anchor {
                        mi::ICON_SQUARE
                    } else {
                        mi::ICON_CROP_SQUARE
                    };
                    if ui
                        .selectable_label(*a == anchor, icon.rich_text())
                        .clicked()
                    {
                        anchor = *a;
                        ui.close();
                    }
                    if i % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
    })
    .response
    .on_hover_text("Punto de referencia de X, Y y del tamaño");
    ui.separator();
    // Acciones rápidas.
    let btn =
        |ui: &mut egui::Ui, icon: egui_material_icons::MaterialIcon, tip: &str, enabled: bool| {
            ui.add_enabled(enabled, egui::Button::new(icon.rich_text()).frame(false))
                .on_hover_text(tip)
                .on_disabled_hover_text(tip)
                .clicked()
        };
    if btn(
        ui,
        mi::ICON_ALIGN_HORIZONTAL_CENTER,
        "Alinear (panel Transformar)",
        true,
    ) {
        app.panel = Some(Panel::Transform);
    }
    if btn(
        ui,
        mi::ICON_CENTER_FOCUS_STRONG,
        "Centrar en la página",
        true,
    ) {
        let c = kurbo::Point::new(app.doc.page.width_mm / 2.0, app.doc.page.height_mm / 2.0);
        apply(app, arrange::center_group_on(&shapes, c));
    }
    let n = app.doc.selected.len();
    if btn(ui, mi::ICON_GROUP_WORK, "Agrupar", n >= 2) {
        app.commit();
        app.doc.group_selected();
    }
    if btn(
        ui,
        mi::ICON_WORKSPACES,
        "Desagrupar",
        app.doc.selection_has_group(),
    ) {
        app.commit();
        app.doc.ungroup_selected();
    }
    ui.separator();
    for (icon, tip, delta) in [
        (mi::ICON_FLIP_TO_FRONT, "Traer al frente", i32::MAX),
        (mi::ICON_ARROW_UPWARD, "Traer adelante", 1),
        (mi::ICON_ARROW_DOWNWARD, "Enviar atrás", -1),
        (mi::ICON_FLIP_TO_BACK, "Enviar al fondo", i32::MIN),
    ] {
        if btn(ui, icon, tip, true) {
            app.commit();
            app.doc.reorder_selected(delta);
        }
    }
    ui.separator();
    if btn(ui, mi::ICON_STAR, "Desplazamiento (panel)", true) {
        app.panel = Some(Panel::Offset);
    }
    if btn(ui, mi::ICON_CLOSE, "Eliminar la selección", true) {
        app.delete_selected();
    }
    ui.data_mut(|d| d.insert_temp(id, (lock, anchor)));
}
