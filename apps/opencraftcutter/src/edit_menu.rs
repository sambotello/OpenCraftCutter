//! Menú Editar, con la estructura, el orden y los atajos de la referencia: portapapeles
//! (cortar, copiar, pegar, pegar delante), borrar, duplicar, selección y preferencias.

use crate::app::OpenCraftCutterApp;
use oc_core::Color;
use oc_core::shape::Shape;

/// Prefijo del texto que se deja en el portapapeles del sistema.
const CLIP_PREFIX: &str = "OPENCRAFTCUTTER-CLIP:";
/// Desplazamiento de «Pegar» (mm); «Pegar delante» pega en el mismo sitio.
const PASTE_OFFSET_MM: f64 = 5.0;

impl OpenCraftCutterApp {
    pub fn copy_selection(&mut self, ctx: &egui::Context) {
        let shapes: Vec<Shape> = self.doc.selected_shapes().into_iter().cloned().collect();
        if shapes.is_empty() {
            return;
        }
        if let Ok(json) = serde_json::to_string(&shapes) {
            let text = format!("{CLIP_PREFIX}{json}");
            ctx.copy_text(text.clone());
            self.clipboard_text = text;
        }
        self.clipboard = shapes;
    }

    pub fn cut_selection(&mut self, ctx: &egui::Context) {
        self.copy_selection(ctx);
        self.delete_selected();
    }

    /// Pega el portapapeles. `in_place` = «Pegar delante» (mismo sitio, delante de todo).
    pub fn paste(&mut self, in_place: bool) {
        if self.clipboard.is_empty() {
            return;
        }
        let d = if in_place { 0.0 } else { PASTE_OFFSET_MM };
        // Los grupos pegados forman grupos nuevos.
        let mut groups = std::collections::HashMap::new();
        let copies: Vec<Shape> = self
            .clipboard
            .iter()
            .map(|s| {
                let mut c = s.clone();
                c.id = 0;
                c.rect = c.rect + kurbo::Vec2::new(d, d);
                c.remap_groups(|g| *groups.entry(g).or_insert_with(|| self.doc.next_id()));
                c
            })
            .collect();
        self.commit();
        self.doc.selected = copies.into_iter().map(|c| self.doc.add_shape(c)).collect();
        // El siguiente «Pegar» se desplaza otra vez, como al pegar varias veces seguidas.
        if !in_place {
            for s in &mut self.clipboard {
                s.rect = s.rect + kurbo::Vec2::new(d, d);
            }
        }
    }

    /// Pegar desde el texto del portapapeles del sistema (⌘V): formas de OpenCraftCutter o SVG.
    pub fn paste_text(&mut self, ctx: &egui::Context, text: &str) {
        if let Some(json) = text.strip_prefix(CLIP_PREFIX) {
            // Si es lo que copiamos aquí, se usa el portapapeles interno (conserva el
            // desplazamiento acumulado de pegar varias veces); si no, viene de otra ventana.
            if text != self.clipboard_text {
                if let Ok(shapes) = serde_json::from_str::<Vec<Shape>>(json) {
                    self.clipboard = shapes;
                    self.clipboard_text = text.to_string();
                }
            }
            return self.paste(false);
        }
        if text.trim_start().starts_with("<svg") || text.contains("<svg ") {
            match oc_svg::import_svg(text, self.defaults.stroke) {
                Ok(r) if !r.shapes.is_empty() => {
                    self.commit();
                    self.doc.selected = r
                        .shapes
                        .into_iter()
                        .map(|s| self.doc.add_shape(s))
                        .collect();
                }
                _ => self.notify(
                    ctx,
                    "El portapapeles no contiene formas que se puedan pegar.",
                ),
            }
            return;
        }
        self.paste(false);
    }

    /// Colores de línea y de relleno usados en el documento (sin repetir).
    pub fn document_colors(&self) -> (Vec<Color>, Vec<Color>) {
        let (mut lines, mut fills) = (Vec::new(), Vec::new());
        for s in self
            .doc
            .shapes
            .iter()
            .filter(|s| s.visible && !s.is_image())
        {
            if !lines.contains(&s.stroke.color) {
                lines.push(s.stroke.color);
            }
            if let Some(f) = s.fill {
                if !fills.contains(&f) {
                    fills.push(f);
                }
            }
        }
        (lines, fills)
    }

    /// Selecciona las formas con ese color de línea (`fill = false`) o de relleno.
    pub fn select_by_color(&mut self, c: Color, fill: bool) {
        self.doc.selected = self
            .doc
            .shapes
            .iter()
            .filter(|s| {
                s.visible
                    && !s.locked
                    && if fill {
                        s.fill == Some(c)
                    } else {
                        s.stroke.color == c
                    }
            })
            .map(|s| s.id)
            .collect();
    }
}

fn item(ui: &mut egui::Ui, label: &str, keys: &str, enabled: bool) -> bool {
    ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(keys))
        .clicked()
}

fn color_row(ui: &mut egui::Ui, c: Color) -> bool {
    let text = format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b);
    let r = ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 3.0, egui::Color32::from_rgb(c.r, c.g, c.b));
        ui.painter().rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(1.0, crate::theme::CHROME_500),
            egui::StrokeKind::Inside,
        );
        ui.button(text).clicked()
    });
    r.inner
}

pub fn menu(app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let has_sel = !app.doc.selected.is_empty();
    if item(ui, "Deshacer", "⌘Z", app.history.can_undo()) {
        app.undo();
        ui.close();
    }
    if item(ui, "Rehacer", "⇧⌘Z", app.history.can_redo()) {
        app.redo();
        ui.close();
    }
    ui.separator();
    if item(ui, "Cortar", "⌘X", has_sel) {
        app.cut_selection(&ctx);
        ui.close();
    }
    if item(ui, "Copiar", "⌘C", has_sel) {
        app.copy_selection(&ctx);
        ui.close();
    }
    let can_paste = !app.clipboard.is_empty();
    if item(ui, "Pegar", "⌘V", can_paste) {
        app.paste(false);
        ui.close();
    }
    if item(ui, "Pegar delante", "⌘F", can_paste) {
        app.paste(true);
        ui.close();
    }
    ui.separator();
    if item(ui, "Borrar", "⌫", has_sel) {
        app.delete_selected();
        ui.close();
    }
    if item(ui, "Duplicar", "⌘D", has_sel) {
        app.commit();
        app.doc.duplicate_selected();
        ui.close();
    }
    ui.separator();
    if item(ui, "Seleccionar todo", "⌘A", !app.doc.shapes.is_empty()) {
        app.doc.select_all();
        ui.close();
    }
    if item(ui, "Deseleccionar todo", "⇧⌘A", has_sel) {
        app.doc.selected.clear();
        ui.close();
    }
    let (lines, fills) = app.document_colors();
    ui.add_enabled_ui(!lines.is_empty() || !fills.is_empty(), |ui| {
        ui.menu_button("Seleccionar por color…", |ui| {
            if !lines.is_empty() {
                ui.label(
                    egui::RichText::new("Color de línea")
                        .small()
                        .color(crate::theme::CHROME_300),
                );
                for c in &lines {
                    if color_row(ui, *c) {
                        app.select_by_color(*c, false);
                        ui.close();
                    }
                }
            }
            if !fills.is_empty() {
                ui.separator();
                ui.label(
                    egui::RichText::new("Color de relleno")
                        .small()
                        .color(crate::theme::CHROME_300),
                );
                for c in &fills {
                    if color_row(ui, *c) {
                        app.select_by_color(*c, true);
                        ui.close();
                    }
                }
            }
        });
    });
    ui.separator();
    if item(ui, "Preferencias…", "⌘K", true) {
        app.show_prefs = true;
        ui.close();
    }
}
