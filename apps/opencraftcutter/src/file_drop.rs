//! Soltar archivos desde Finder: en Diseño se añaden al tapete donde se sueltan (los
//! documentos `.opencraftcutter` se abren en pestaña nueva); en Biblioteca se copian a la carpeta.

use crate::app::{OpenCraftCutterApp, Workspace};
use oc_core::arrange::{Anchor, move_anchor_to};
use oc_core::document::DOCUMENT_EXTENSION;
use std::path::{Path, PathBuf};

const IMAGE_EXT: [&str; 8] = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff"];
/// Separación entre archivos soltados a la vez (mm).
const CASCADE_MM: f64 = 10.0;

fn ext(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default()
}

pub fn supported(p: &Path) -> bool {
    let e = ext(p);
    e == DOCUMENT_EXTENSION
        || e == "studio3"
        || e == "svg"
        || e == "svgz"
        || IMAGE_EXT.contains(&e.as_str())
}

/// Posición actual del ratón en puntos de egui, relativa a la ventana (solo macOS).
#[cfg(target_os = "macos")]
fn system_pointer(ctx: &egui::Context) -> Option<egui::Pos2> {
    use std::ffi::c_void;
    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreate(source: *const c_void) -> *mut c_void;
        fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const c_void);
    }
    // Coordenadas globales en puntos, con origen arriba a la izquierda (como winit).
    // SAFETY: CGEventCreate(NULL) crea un evento con el estado actual del ratón; se
    // libera tras leer la posición.
    let global = unsafe {
        let ev = CGEventCreate(std::ptr::null());
        if ev.is_null() {
            return None;
        }
        let p = CGEventGetLocation(ev);
        CFRelease(ev);
        p
    };
    let (inner, native_ppp) = ctx.input(|i| {
        (
            i.viewport().inner_rect,
            i.viewport().native_pixels_per_point,
        )
    });
    // egui mide la ventana en píxeles físicos / pixels_per_point (incluye el zoom).
    let k = native_ppp.unwrap_or(1.0) / ctx.pixels_per_point();
    let p = egui::pos2(global.x as f32 * k, global.y as f32 * k);
    Some(p - inner?.min.to_vec2())
}

#[cfg(not(target_os = "macos"))]
fn system_pointer(_ctx: &egui::Context) -> Option<egui::Pos2> {
    None
}

impl OpenCraftCutterApp {
    /// Procesa los archivos soltados este frame y dibuja el aviso mientras se arrastran.
    pub fn handle_file_drop(&mut self, ctx: &egui::Context) {
        let (hovering, dropped, pointer) = ctx.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .filter(|p| !p.as_os_str().is_empty())
                    .collect::<Vec<PathBuf>>(),
                i.pointer.latest_pos(),
            )
        });
        if hovering {
            self.draw_drop_hint(ctx);
        }
        if dropped.is_empty() {
            return;
        }
        let (ok, skipped): (Vec<PathBuf>, Vec<PathBuf>) =
            dropped.into_iter().partition(|p| supported(p));
        if !skipped.is_empty() {
            self.notify(
                ctx,
                format!(
                    "Formato no admitido: {}. Se aceptan documentos, SVG e imágenes.",
                    skipped
                        .iter()
                        .filter_map(|p| p.file_name())
                        .map(|n| n.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
        if ok.is_empty() {
            return;
        }
        if self.workspace == Workspace::Library {
            let n = self.library.add_files(&ok);
            return self.notify(ctx, format!("{n} archivo(s) añadidos a la biblioteca."));
        }
        self.workspace = Workspace::Design;
        // Punto del tapete bajo el puntero; si no está sobre el lienzo, el centro visible.
        // Durante un arrastre del sistema la ventana no recibe movimientos del ratón, así
        // que se pregunta su posición al sistema.
        let stage = self.canvas.stage;
        let system = system_pointer(ctx);
        log::debug!(
            "soltar: puntero del sistema {system:?}, de egui {pointer:?}, lienzo {stage:?}"
        );
        let screen = system
            .or(pointer)
            .filter(|p| stage.contains(*p))
            .unwrap_or(stage.center());
        let mut target = self.canvas.viewport.to_world(screen);
        for path in ok {
            if ext(&path) == DOCUMENT_EXTENSION {
                self.open_path(ctx, &path);
                continue;
            }
            self.add_file_at(ctx, &path, target);
            target += kurbo::Vec2::new(CASCADE_MM, CASCADE_MM);
        }
    }

    /// Importa un SVG o una imagen al documento actual, centrado en `target` (mm).
    fn add_file_at(&mut self, ctx: &egui::Context, path: &Path, target: kurbo::Point) {
        let before = self.doc.selected.clone();
        let e = ext(path);
        if e == "svg" || e == "svgz" {
            self.import_svg_path(ctx, path);
        } else if e == "studio3" {
            self.import_studio3_path(ctx, path);
        } else {
            self.import_image_path(ctx, path);
        }
        if self.doc.selected == before || self.doc.selected.is_empty() {
            return;
        }
        let shapes: Vec<_> = self.doc.selected_shapes().into_iter().cloned().collect();
        for moved in move_anchor_to(&shapes, Anchor::Center, target) {
            if let Some(s) = self.doc.shape_mut(moved.id) {
                *s = moved;
            }
        }
        self.remember_recent(path);
    }

    fn draw_drop_hint(&self, ctx: &egui::Context) {
        let (rect, text) = if self.workspace == Workspace::Library {
            (ctx.content_rect(), "Suelta para añadir a la biblioteca")
        } else {
            (self.canvas.stage, "Suelta para añadir al tapete")
        };
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("file_drop_hint"),
        ));
        let accent = crate::theme::ACCENT;
        painter.rect(
            rect.shrink(4.0),
            8.0,
            accent.gamma_multiply(0.12),
            egui::Stroke::new(2.0, accent),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(20.0),
            egui::Color32::WHITE,
        );
    }
}
