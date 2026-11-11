//! Acciones de documento: nuevo, abrir, guardar, importar y exportar. Diálogos nativos vía `rfd`.

use crate::app::{OpenCraftCutterApp, Workspace};
use oc_core::document::DOCUMENT_EXTENSION;
use oc_core::{Document, History};
use std::path::{Path, PathBuf};

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Sin título".into())
}

impl OpenCraftCutterApp {
    /// Pide confirmación para descartar cambios. Devuelve true si se puede continuar.
    pub fn confirm_discard(&self) -> bool {
        rfd::MessageDialog::new()
            .set_title("Cambios sin guardar")
            .set_description("Hay cambios sin guardar. ¿Continuar y descartarlos?")
            .set_buttons(rfd::MessageButtons::OkCancel)
            .show()
            == rfd::MessageDialogResult::Ok
    }

    fn show_error(&mut self, ctx: &egui::Context, msg: String) {
        log::error!("{msg}");
        self.notify(ctx, msg);
    }

    pub fn new_document(&mut self, _ctx: &egui::Context) {
        self.new_tab();
    }

    pub fn open_document(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Todos los compatibles",
                &[
                    DOCUMENT_EXTENSION,
                    "studio3",
                    "svg",
                    "svgz",
                    "png",
                    "jpg",
                    "jpeg",
                    "gif",
                    "bmp",
                    "webp",
                    "tif",
                    "tiff",
                ],
            )
            .add_filter("Documento de OpenCraftCutter", &[DOCUMENT_EXTENSION])
            .add_filter("Silhouette Studio", &["studio3"])
            .add_filter("SVG", &["svg", "svgz"])
            .add_filter(
                "Imágenes",
                &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff"],
            )
            .pick_file()
        else {
            return;
        };
        self.open_path(ctx, &path);
    }

    /// Abre un archivo: documentos de OpenCraftCutter tal cual; SVG e imágenes, como la referencia,
    /// en un documento nuevo con el nombre del archivo.
    pub fn open_path(&mut self, ctx: &egui::Context, path: &Path) {
        let path = path.to_path_buf();
        self.remember_recent(&path);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default();
        if ext != DOCUMENT_EXTENSION {
            if !(self.doc.shapes.is_empty() && !self.dirty && self.file_path.is_none()) {
                self.new_tab();
            }
            self.doc.name = stem(&path);
            self.workspace = Workspace::Design;
            if ext == "svg" || ext == "svgz" {
                self.import_svg_path(ctx, &path);
            } else if ext == "studio3" {
                self.import_studio3_path(ctx, &path);
            } else {
                self.import_image_path(ctx, &path);
            }
            // Recién abierto: sin historial ni cambios pendientes.
            self.history = History::default();
            self.dirty = false;
            self.canvas.request_fit();
            return;
        }
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| Document::from_json(&t).map_err(|e| e.to_string()))
        {
            Ok(mut doc) => {
                doc.name = stem(&path);
                // Un documento vacío y sin cambios se sustituye; si no, pestaña nueva.
                if self.doc.shapes.is_empty() && !self.dirty && self.file_path.is_none() {
                    self.doc = doc;
                    self.history = History::default();
                    self.file_path = Some(path);
                    self.dirty = false;
                    self.canvas.request_fit();
                } else {
                    self.open_in_new_tab(doc, Some(path));
                }
                self.workspace = Workspace::Design;
            }
            Err(e) => self.show_error(ctx, format!("No se pudo abrir el documento: {e}")),
        }
    }

    pub fn save_document(&mut self, ctx: &egui::Context, save_as: bool) -> bool {
        let path: Option<PathBuf> = if save_as || self.file_path.is_none() {
            rfd::FileDialog::new()
                .set_file_name(format!("{}.{}", self.doc.name, DOCUMENT_EXTENSION))
                .add_filter("Documento de OpenCraftCutter", &[DOCUMENT_EXTENSION])
                .save_file()
        } else {
            self.file_path.clone()
        };
        let Some(path) = path else { return false };
        self.save_to(ctx, &path)
    }

    /// Guarda el documento en `path` y lo toma como su archivo.
    pub fn save_to(&mut self, ctx: &egui::Context, path: &Path) -> bool {
        let path = path.to_path_buf();
        match std::fs::write(&path, self.doc.to_json()) {
            Ok(()) => {
                self.doc.name = stem(&path);
                self.remember_recent(&path);
                self.file_path = Some(path);
                self.dirty = false;
                true
            }
            Err(e) => {
                self.show_error(ctx, format!("No se pudo guardar: {e}"));
                false
            }
        }
    }

    pub fn import_svg(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("SVG", &["svg", "svgz"])
            .pick_file()
        else {
            return;
        };
        self.import_svg_path(ctx, &path);
    }

    /// Importa un documento de Silhouette Studio (.studio3) al documento actual.
    pub fn import_studio3_path(&mut self, ctx: &egui::Context, path: &Path) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return self.show_error(ctx, format!("No se pudo leer el archivo: {e}")),
        };
        match oc_studio3::parse(&bytes) {
            Ok(doc) => {
                self.commit();
                // Al abrir (documento vacío), se toman los medios y las marcas de registro.
                if self.doc.shapes.is_empty() {
                    if let Some(r) = doc.reg_marks {
                        let m = &mut self.doc.page.reg_marks;
                        m.enabled = true;
                        [
                            m.inset_left_mm,
                            m.inset_top_mm,
                            m.inset_right_mm,
                            m.inset_bottom_mm,
                        ] = r.insets;
                        m.length_mm = r.length;
                        m.thickness_mm = r.thickness;
                    }
                    if let Some(m) = doc
                        .media
                        .as_deref()
                        .and_then(oc_core::machines::media_size)
                        .filter(|m| m.width_mm > 0.0)
                    {
                        let page = &mut self.doc.page;
                        page.media_size = m.id.to_string();
                        let landscape =
                            page.orientation == oc_core::document::Orientation::Landscape;
                        (page.width_mm, page.height_mm) = if landscape {
                            (m.height_mm, m.width_mm)
                        } else {
                            (m.width_mm, m.height_mm)
                        };
                    }
                }
                // Los grupos del archivo pasan a ser grupos del documento.
                let mut groups = std::collections::HashMap::new();
                let ids: Vec<_> = doc
                    .to_shapes(self.defaults.stroke)
                    .into_iter()
                    .map(|mut s| {
                        s.remap_groups(|g| *groups.entry(g).or_insert_with(|| self.doc.next_id()));
                        self.doc.add_shape(s)
                    })
                    .collect();
                let count = ids.len();
                self.doc.selected = ids;
                self.workspace = Workspace::Design;
                let mut msg = format!("Importadas {count} formas de {}.", stem(path));
                for w in doc.warnings() {
                    msg.push(' ');
                    msg.push_str(&w);
                }
                self.notify(ctx, msg);
            }
            Err(e) => self.show_error(ctx, e.to_string()),
        }
    }

    pub fn import_svg_path(&mut self, ctx: &egui::Context, path: &Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => return self.show_error(ctx, format!("No se pudo leer el archivo: {e}")),
        };
        match oc_svg::import_svg(&text, self.defaults.stroke) {
            Ok(result) => {
                self.commit();
                let ids: Vec<_> = result
                    .shapes
                    .into_iter()
                    .map(|s| self.doc.add_shape(s))
                    .collect();
                let count = ids.len();
                self.doc.selected = ids;
                self.workspace = Workspace::Design;
                let mut msg = format!("Importadas {count} formas de {}.", stem(path));
                for w in result.warnings {
                    msg.push(' ');
                    msg.push_str(&w);
                }
                self.notify(ctx, msg);
            }
            Err(e) => self.show_error(ctx, e.to_string()),
        }
    }

    /// Importa una imagen (PNG, JPG, GIF, BMP, WebP, TIFF) como forma de imagen centrada en la página.
    pub fn import_image(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Imágenes",
                &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff"],
            )
            .pick_file()
        else {
            return;
        };
        self.import_image_path(ctx, &path);
    }

    pub fn import_image_path(&mut self, ctx: &egui::Context, path: &Path) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return self.show_error(ctx, format!("No se pudo leer la imagen: {e}")),
        };
        match oc_trace::to_png(&bytes) {
            Ok((png, w, h)) => {
                // Tamaño físico a 96 dpi, limitado al 80 % de la página.
                let px_mm = oc_core::units::MM_PER_IN / 96.0;
                let (mut wmm, mut hmm) = (w as f64 * px_mm, h as f64 * px_mm);
                let page = &self.doc.page;
                let k = (page.width_mm * 0.8 / wmm)
                    .min(page.height_mm * 0.8 / hmm)
                    .min(1.0);
                wmm *= k;
                hmm *= k;
                let x0 = (page.width_mm - wmm) / 2.0;
                let y0 = (page.height_mm - hmm) / 2.0;
                let kind = oc_core::shape::ShapeKind::Image {
                    png: oc_core::shape::ImageData(std::sync::Arc::new(png)),
                    px_width: w,
                    px_height: h,
                };
                let mut s = oc_core::Shape::new(
                    0,
                    stem(path),
                    kurbo::Rect::new(x0, y0, x0 + wmm, y0 + hmm),
                    kind,
                );
                s.stroke.width_pt = 0.0;
                self.commit();
                let id = self.doc.add_shape(s);
                self.doc.selected = vec![id];
                self.workspace = Workspace::Design;
                self.panel = Some(crate::app::Panel::Trace);
            }
            Err(e) => self.show_error(ctx, e.to_string()),
        }
    }

    /// Imprimir y cortar: guarda la página lista para imprimir (diseño, marcas de registro y
    /// sangrado, sin las líneas de corte) y la abre con la aplicación del sistema.
    pub fn print_page(&mut self, ctx: &egui::Context) {
        // PDF (no SVG): se abre en el visor de PDF del sistema, que imprime con el diálogo
        // nativo. En macOS se fuerza Vista Previa para no depender de la app asociada al SVG.
        let pdf = match oc_svg::print_pdf(&self.doc) {
            Ok(p) => p,
            Err(e) => {
                return self.show_error(ctx, format!("No se pudo preparar la impresión: {e}"));
            }
        };
        let path = std::env::temp_dir().join(format!("{} - imprimir.pdf", self.doc.name));
        if let Err(e) = std::fs::write(&path, pdf) {
            return self.show_error(ctx, format!("No se pudo preparar la impresión: {e}"));
        }
        let opened = if cfg!(target_os = "macos") {
            std::process::Command::new("open")
                .args(["-a", "Preview"])
                .arg(&path)
                .spawn()
        } else if cfg!(target_os = "windows") {
            std::process::Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(&path)
                .spawn()
        } else {
            std::process::Command::new("xdg-open").arg(&path).spawn()
        };
        match opened {
            Ok(_) => self.notify(
                ctx,
                "Página lista para imprimir abierta en Vista Previa (⌘P para imprimir).",
            ),
            Err(e) => self.show_error(ctx, format!("No se pudo abrir {}: {e}", path.display())),
        }
    }

    pub fn export_svg(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("{}.svg", self.doc.name))
            .add_filter("SVG", &["svg"])
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, oc_svg::export_svg(&self.doc)) {
            Ok(()) => self.notify(ctx, format!("Exportado a {}.", path.display())),
            Err(e) => self.show_error(ctx, format!("No se pudo exportar: {e}")),
        }
    }

    /// Exporta para Silhouette Studio: formas, grupos, imágenes y textos (como trazados).
    pub fn export_studio3(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("{}.studio3", self.doc.name))
            .add_filter("Silhouette Studio", &["studio3"])
            .save_file()
        else {
            return;
        };
        let (bytes, skipped) = studio3_export(&self.doc);
        match std::fs::write(&path, bytes) {
            Ok(()) if skipped > 0 => self.notify(
                ctx,
                format!(
                    "Exportado a {}. No se pudieron convertir {skipped} textos (se omitieron).",
                    path.display()
                ),
            ),
            Ok(()) => self.notify(ctx, format!("Exportado a {}.", path.display())),
            Err(e) => self.show_error(ctx, format!("No se pudo exportar: {e}")),
        }
    }
}

/// Bytes del `.studio3` de un documento y cuántos textos no se pudieron exportar. Los textos
/// de una línea sin efectos van como textos editables; los demás, convertidos en trazados.
pub fn studio3_export(doc: &Document) -> (Vec<u8>, usize) {
    let mut doc = doc.clone();
    let mut runs = oc_studio3::write::TextRuns::new();
    for s in doc.shapes.iter_mut().filter(|s| s.is_text()) {
        if let Some(run) = editable_text(s) {
            runs.insert(s.id, run);
        } else if let Some(mut p) = crate::text::text_to_path(s) {
            p.id = s.id;
            p.name = s.name.clone();
            p.group = s.group;
            p.inner_groups = s.inner_groups.clone();
            *s = p;
        }
    }
    let skipped = doc
        .shapes
        .iter()
        .filter(|s| s.visible && s.is_text() && !runs.contains_key(&s.id))
        .count();
    (oc_studio3::write::write_with_texts(&doc, &runs), skipped)
}

/// Letras de un texto colocadas en su caja (la del dibujo), si se puede exportar como texto
/// editable: una línea, sin subrayado, interletrado ni vertical, y sin deformar.
fn editable_text(s: &oc_core::Shape) -> Option<oc_studio3::text_write::TextRun> {
    let oc_core::ShapeKind::Text {
        text,
        font_family,
        font_size_pt,
        align: _,
        bold,
        italic,
        line_height: _,
        fx,
    } = &s.kind
    else {
        return None;
    };
    if fx.underline || fx.vertical || (fx.char_spacing - 100.0).abs() > 0.5 {
        return None;
    }
    let run = oc_svg::glyph_run(font_family, *font_size_pt, *bold, *italic, text)?;
    // Caja de tinta del texto maquetado (origen en el lápiz sobre la línea base).
    let ink = run
        .glyphs
        .iter()
        .flat_map(|g| {
            g.outline.iter().flat_map(move |np| {
                np.nodes
                    .iter()
                    .flat_map(|n| [Some(n.p), n.h_in, n.h_out])
                    .flatten()
                    .map(move |p| kurbo::Point::new(p.x + g.x_mm, p.y))
            })
        })
        .fold(None, |r: Option<kurbo::Rect>, p| {
            Some(r.map_or(kurbo::Rect::from_points(p, p), |r| r.union_pt(p)))
        })?;
    let r = s.rect;
    let k = r.width() / ink.width().max(1e-9);
    // La referencia solo escala el texto por igual en los dos ejes.
    if ((r.height() / ink.height().max(1e-9)) / k - 1.0).abs() > 0.03 {
        return None;
    }
    let origin = kurbo::Point::new(r.x0 - ink.x0 * k, r.y0 - ink.y0 * k);
    let place =
        |p: kurbo::Point, x: f64| kurbo::Point::new(origin.x + (p.x + x) * k, origin.y + p.y * k);
    Some(oc_studio3::text_write::TextRun {
        family: font_family.clone(),
        size_pt: font_size_pt * k,
        origin,
        ascent_mm: run.ascent_mm * k,
        descent_mm: run.descent_mm * k,
        glyphs: run
            .glyphs
            .iter()
            .map(|g| oc_studio3::text_write::TextGlyph {
                ch: g.ch,
                x_mm: origin.x + g.x_mm * k,
                advance_mm: g.advance_mm * k,
                outline: g
                    .outline
                    .iter()
                    .map(|np| oc_core::nodes::NodePath {
                        nodes: np
                            .nodes
                            .iter()
                            .map(|n| oc_core::nodes::Node {
                                p: place(n.p, g.x_mm),
                                h_in: n.h_in.map(|h| place(h, g.x_mm)),
                                h_out: n.h_out.map(|h| place(h, g.x_mm)),
                                smooth: n.smooth,
                            })
                            .collect(),
                        closed: np.closed,
                    })
                    .collect(),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::{Color, Rect, Shape, ShapeKind, TextAlign};

    fn text(t: &str, r: Rect) -> Shape {
        let mut s = Shape::new(
            0,
            "Texto",
            r,
            ShapeKind::Text {
                text: t.to_string(),
                font_family: crate::text::DEFAULT_FONT_FAMILY.into(),
                font_size_pt: 36.0,
                align: TextAlign::Left,
                bold: false,
                italic: false,
                line_height: 1.2,
                fx: Default::default(),
            },
        );
        // Caja como la que da la app al crear el texto: la de su maquetación.
        let (w, h) = crate::text::TextStyle::from_kind(&s.kind)
            .map(|st| crate::text::measure(&st))
            .unwrap();
        s.rect = Rect::new(r.x0, r.y0, r.x0 + w, r.y0 + h);
        s.fill = Some(Color::BLACK);
        s
    }

    #[test]
    fn texts_are_exported_editable() {
        let mut doc = Document::default();
        doc.add_shape(text("Hola", Rect::new(20.0, 20.0, 80.0, 40.0)));
        let mut grouped = text("OpenCraftCutter", Rect::new(20.0, 60.0, 140.0, 80.0));
        grouped.group = Some(7);
        grouped.rotation = 10.0;
        grouped.fill = Some(Color::rgb(200, 30, 60));
        let mut frame = Shape::new(
            0,
            "Marco",
            Rect::new(15.0, 55.0, 145.0, 85.0),
            ShapeKind::Rect { corner_radius: 3.0 },
        );
        frame.fill = None;
        frame.group = Some(7);
        doc.add_shape(grouped);
        doc.add_shape(frame);
        let (bytes, skipped) = studio3_export(&doc);
        if let Ok(out) = std::env::var("OPENCRAFTCUTTER_STUDIO3_WRITE") {
            std::fs::write(out, &bytes).unwrap();
        }
        assert_eq!(skipped, 0);
        let back = oc_studio3::parse(&bytes).expect("se lee");
        // Los dos textos son de una línea: van como textos editables; el marco, como forma.
        assert_eq!(back.texts.len(), 2);
        assert_eq!(back.texts[0].text, "Hola");
        assert_eq!(back.texts[1].text, "OpenCraftCutter");
        let hola = &back.texts[0].bounds;
        assert!(
            (hola.x0 - 20.0).abs() < 0.3 && (hola.y0 - 20.0).abs() < 0.3,
            "{hola:?}"
        );
        assert_eq!(back.shapes.len(), 1);
    }

    #[test]
    fn weld_joins_touching_script_letters() {
        // Fuente manuscrita de macOS: las letras se tocan. Fusionar el texto solo debe dejar
        // menos contornos que letras sueltas y ninguno de los solapes como agujero.
        let mut t = text("minimo", Rect::new(0.0, 0.0, 1.0, 1.0));
        if let ShapeKind::Text { font_family, .. } = &mut t.kind {
            *font_family = "Snell Roundhand".into();
        }
        let Some(path) = crate::text::text_to_path(&t) else {
            return; // Sin la fuente en el sistema.
        };
        let before = oc_core::ops::closed_contours(&path).len();
        let out = oc_core::ops::boolean(std::slice::from_ref(&path), oc_core::ops::BoolOp::Weld)
            .expect("se fusiona");
        let after = oc_core::ops::closed_contours(&out[0]).len();
        assert!(after < before, "antes {before} contornos, después {after}");
    }
}
