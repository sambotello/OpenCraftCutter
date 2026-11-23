//! Menú Editar, con la estructura, el orden y los atajos de la referencia: portapapeles
//! (cortar, copiar, pegar, pegar delante), borrar, duplicar, selección y preferencias.

use crate::app::OpenCraftCutterApp;
use crate::clipboard::Pasted;
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
        // Además del texto, el portapapeles puede traer algo mejor: «Copiar imagen» en un
        // navegador deja la imagen y, como texto, solo su dirección.
        if let Some(content) = crate::clipboard::read() {
            return self.paste_content(ctx, content);
        }
        if crate::clipboard::looks_like_svg(text) {
            return self.paste_content(ctx, Pasted::Svg(text.to_string()));
        }
        self.paste(false);
    }

    /// «Pegar» desde un menú: lo que haya en el portapapeles del sistema (formas copiadas
    /// aquí, vectores SVG, imágenes o archivos) o, si no hay nada, el portapapeles interno.
    pub fn paste_system(&mut self, ctx: &egui::Context) {
        match crate::clipboard::read_text() {
            Some(text) => self.paste_text(ctx, &text),
            None => match crate::clipboard::read() {
                Some(content) => self.paste_content(ctx, content),
                None => self.paste(false),
            },
        }
    }

    /// ¿Hay algo que pegar (en el portapapeles interno o en el del sistema)?
    pub fn can_paste(&self) -> bool {
        !self.clipboard.is_empty() || crate::clipboard::has_content()
    }

    /// Imagen pegada sin más formato (lo que entrega egui cuando no hay texto).
    pub fn paste_color_image(&mut self, ctx: &egui::Context, img: &egui::ColorImage) {
        if let Some(content) = crate::clipboard::read() {
            return self.paste_content(ctx, content);
        }
        let [w, h] = img.size;
        let mut png = Vec::new();
        let ok = image::RgbaImage::from_raw(w as u32, h as u32, img.as_raw().to_vec())
            .and_then(|rgba| {
                rgba.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                    .ok()
            })
            .is_some();
        if ok {
            self.paste_content(ctx, Pasted::Image(png));
        }
    }

    /// Añade al documento lo leído del portapapeles del sistema.
    pub fn paste_content(&mut self, ctx: &egui::Context, content: Pasted) {
        match content {
            Pasted::Svg(text) => {
                let empty = oc_svg::import_svg(&text, self.defaults.stroke)
                    .map_or(true, |r| r.shapes.is_empty());
                if empty {
                    return self.notify(
                        ctx,
                        "El portapapeles no contiene formas que se puedan pegar.",
                    );
                }
                self.import_svg_text(ctx, &text, "el portapapeles");
            }
            Pasted::Image(bytes) => self.add_image_bytes(ctx, "Imagen pegada", &bytes),
            Pasted::Files(files) => {
                let (ok, skipped): (Vec<_>, Vec<_>) = files
                    .into_iter()
                    .partition(|p| crate::file_drop::supported(p));
                if ok.is_empty() {
                    let n = skipped.len();
                    return self.notify(
                        ctx,
                        format!(
                            "{n} archivo(s) copiados no se pueden pegar: se aceptan documentos, SVG e imágenes."
                        ),
                    );
                }
                // Como al soltarlos en el lienzo, pero en el centro de lo que se ve.
                let center = self.canvas.viewport.to_world(self.canvas.stage.center());
                self.add_files_at(ctx, &ok, center);
            }
            Pasted::Pdf { pdf, image } => {
                // Vectores del PDF; si no trae ninguno (por ejemplo, es una foto envuelta en
                // un PDF), la imagen que venía con él.
                match oc_svg::pdf_to_svg(&pdf) {
                    Ok(svg)
                        if oc_svg::import_svg(&svg, self.defaults.stroke)
                            .is_ok_and(|r| !r.shapes.is_empty()) =>
                    {
                        self.import_svg_text(ctx, &svg, "el portapapeles");
                    }
                    _ => match image {
                        Some(bytes) => self.add_image_bytes(ctx, "Imagen pegada", &bytes),
                        None => self.notify(
                            ctx,
                            "El PDF del portapapeles no contiene trazados que se puedan pegar.",
                        ),
                    },
                }
            }
        }
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
    let can_paste = app.can_paste();
    if item(ui, "Pegar", "⌘V", can_paste) {
        app.paste_system(&ctx);
        ui.close();
    }
    if item(ui, "Pegar delante", "⌘F", !app.clipboard.is_empty()) {
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
