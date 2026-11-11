//! Menú Archivo, con la estructura, el orden y los atajos de la referencia, y las acciones
//! que solo usa este menú: recientes, combinar, guardar selección, biblioteca y archivos
//! recuperados.

use crate::app::{OpenCraftCutterApp, Panel, Workspace};
use oc_core::Document;
use oc_core::document::DOCUMENT_EXTENSION;
use std::path::{Path, PathBuf};

const MAX_RECENT: usize = 15;
/// Cada cuánto se guarda una copia de recuperación de los documentos con cambios (s).
const AUTOSAVE_SECS: f64 = 120.0;

/// Carpeta de archivos recuperados (copias automáticas).
pub fn recovery_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let base = if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("OpenCraftCutter")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("OpenCraftCutter")
    } else {
        home.join(".local").join("share").join("opencraftcutter")
    };
    base.join("Recuperados")
}

fn file_name_safe(name: &str) -> String {
    name.chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect()
}

/// Ruta libre en `dir` para `name.ext` (añade « 2», « 3»… si ya existe).
fn free_path(dir: &Path, name: &str, ext: &str) -> PathBuf {
    let base = file_name_safe(name);
    let mut p = dir.join(format!("{base}.{ext}"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{base} {n}.{ext}"));
        n += 1;
    }
    p
}

impl OpenCraftCutterApp {
    /// Anota un archivo en «Abrir reciente».
    pub fn remember_recent(&mut self, path: &Path) {
        let s = path.to_string_lossy().into_owned();
        self.prefs.recent_files.retain(|p| p != &s);
        self.prefs.recent_files.insert(0, s);
        self.prefs.recent_files.truncate(MAX_RECENT);
    }

    /// Copia de recuperación periódica de los documentos con cambios.
    pub fn autosave_tick(&mut self, now: f64) {
        // Las pruebas nunca escriben en la carpeta de recuperación del usuario.
        if cfg!(test) || now - self.last_autosave < AUTOSAVE_SECS {
            return;
        }
        self.last_autosave = now;
        self.autosave_into(&recovery_dir());
    }

    /// Escribe la copia de recuperación en `dir` si hay cambios sin guardar.
    pub fn autosave_into(&mut self, dir: &Path) {
        if !self.dirty || self.doc.shapes.is_empty() {
            return;
        }
        if std::fs::create_dir_all(dir).is_ok() {
            let p = dir.join(format!(
                "{}.{DOCUMENT_EXTENSION}",
                file_name_safe(&self.doc.name)
            ));
            let _ = std::fs::write(p, self.doc.to_json());
        }
    }

    pub fn open_recovered(&mut self, ctx: &egui::Context) {
        let dir = recovery_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Some(p) = rfd::FileDialog::new()
            .set_title("Abrir un archivo recuperado")
            .set_directory(&dir)
            .add_filter("Documento de OpenCraftCutter", &[DOCUMENT_EXTENSION])
            .pick_file()
        {
            self.open_path(ctx, &p);
        }
    }

    /// Nuevo asistente de proyecto: documento nuevo con la guía de configuración de página.
    pub fn new_with_wizard(&mut self) {
        self.new_tab();
        self.panel = Some(Panel::PageSetup);
        self.show_page_guide = true;
    }

    /// Combinar: añade al documento actual el contenido de otro archivo.
    pub fn merge_document(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Combinar")
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
            .pick_file()
        else {
            return;
        };
        self.merge_path(ctx, &path);
    }

    /// Combinar: añade al documento actual el contenido del archivo `path`.
    pub fn merge_path(&mut self, ctx: &egui::Context, path: &Path) {
        let path = path.to_path_buf();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default();
        match ext.as_str() {
            e if e == DOCUMENT_EXTENSION => {
                match std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|t| Document::from_json(&t).map_err(|e| e.to_string()))
                {
                    Ok(other) => {
                        self.commit();
                        let ids: Vec<u64> = other
                            .shapes
                            .into_iter()
                            .map(|mut s| {
                                s.id = 0;
                                self.doc.add_shape(s)
                            })
                            .collect();
                        self.doc.selected = ids;
                    }
                    Err(e) => self.notify(ctx, format!("No se pudo combinar: {e}")),
                }
            }
            "svg" | "svgz" => self.import_svg_path(ctx, &path),
            "studio3" => self.import_studio3_path(ctx, &path),
            _ => self.import_image_path(ctx, &path),
        }
        self.workspace = Workspace::Design;
    }

    /// Documento con solo la selección (misma página).
    fn selection_document(&self) -> Option<Document> {
        if self.doc.selected.is_empty() {
            return None;
        }
        let mut d = Document::default();
        d.name = format!("{} (selección)", self.doc.name);
        d.page = self.doc.page.clone();
        for s in self.doc.selected_shapes() {
            let mut c = s.clone();
            c.id = 0;
            d.add_shape(c);
        }
        Some(d)
    }

    /// Guardar selección en el disco duro.
    pub fn save_selection(&mut self, ctx: &egui::Context) {
        let Some(d) = self.selection_document() else {
            return;
        };
        if let Some(p) = rfd::FileDialog::new()
            .set_file_name(format!("{}.{DOCUMENT_EXTENSION}", d.name))
            .add_filter("Documento de OpenCraftCutter", &[DOCUMENT_EXTENSION])
            .save_file()
        {
            match std::fs::write(&p, d.to_json()) {
                Ok(()) => self.notify(ctx, format!("Selección guardada en {}.", p.display())),
                Err(e) => self.notify(ctx, format!("No se pudo guardar: {e}")),
            }
        }
    }

    /// Guarda el documento (o la selección) en la carpeta de la biblioteca.
    pub fn save_to_library(&mut self, ctx: &egui::Context, selection_only: bool) {
        let doc = if selection_only {
            match self.selection_document() {
                Some(d) => d,
                None => return,
            }
        } else {
            self.doc.clone()
        };
        let dir = self.library.root_dir();
        let p = free_path(&dir, &doc.name, DOCUMENT_EXTENSION);
        match std::fs::write(&p, doc.to_json()) {
            Ok(()) => {
                self.library.rescan();
                self.notify(
                    ctx,
                    format!(
                        "Guardado en la biblioteca: {}.",
                        p.file_name().unwrap_or_default().to_string_lossy()
                    ),
                );
            }
            Err(e) => self.notify(ctx, format!("No se pudo guardar en la biblioteca: {e}")),
        }
    }

    /// Importar biblioteca: copia a la biblioteca el contenido compatible de una carpeta.
    pub fn import_library_folder(&mut self, ctx: &egui::Context) {
        let Some(src) = rfd::FileDialog::new()
            .set_title("Importar biblioteca")
            .pick_folder()
        else {
            return;
        };
        let n = self.copy_into_library(walk_files(&src));
        self.notify(ctx, format!("Importados {n} archivos a la biblioteca."));
    }

    /// Importar a biblioteca: copia archivos sueltos a la biblioteca.
    pub fn import_files_to_library(&mut self, ctx: &egui::Context) {
        let Some(files) = rfd::FileDialog::new()
            .set_title("Importar a biblioteca")
            .add_filter(
                "Todos los compatibles",
                &[DOCUMENT_EXTENSION, "studio3", "svg", "png", "jpg", "jpeg"],
            )
            .pick_files()
        else {
            return;
        };
        let n = self.copy_into_library(files);
        self.notify(ctx, format!("Importados {n} archivos a la biblioteca."));
    }

    fn copy_into_library(&mut self, files: Vec<PathBuf>) -> usize {
        let dir = self.library.root_dir();
        let mut n = 0;
        for f in files {
            let ext = f
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_lowercase)
                .unwrap_or_default();
            if ![DOCUMENT_EXTENSION, "svg", "svgz", "png", "jpg", "jpeg"].contains(&ext.as_str()) {
                continue;
            }
            let stem = f
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if std::fs::copy(&f, free_path(&dir, &stem, &ext)).is_ok() {
                n += 1;
            }
        }
        self.library.rescan();
        n
    }

    pub fn close_active_tab(&mut self) {
        let i = self.active_tab();
        self.close_tab(i);
    }
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p);
                }
            }
        }
    }
    out
}

fn item(ui: &mut egui::Ui, label: &str, keys: &str, enabled: bool) -> bool {
    ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(keys))
        .clicked()
}

pub fn menu(app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let has_sel = !app.doc.selected.is_empty();
    if item(ui, "Nuevo", "⌘N", true) {
        app.new_document(&ctx);
        ui.close();
    }
    if item(ui, "Nuevo asistente de proyecto", "⇧⌘N", true) {
        app.new_with_wizard();
        ui.close();
    }
    if item(ui, "Abrir…", "⌘O", true) {
        app.open_document(&ctx);
        ui.close();
    }
    let recent = app.prefs.recent_files.clone();
    ui.add_enabled_ui(!recent.is_empty(), |ui| {
        ui.menu_button("Abrir reciente…", |ui| {
            for p in &recent {
                let path = PathBuf::from(p);
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.clone());
                if ui
                    .add_enabled(path.exists(), egui::Button::new(name))
                    .on_hover_text(p)
                    .clicked()
                {
                    app.open_path(&ctx, &path);
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Borrar la lista").clicked() {
                app.prefs.recent_files.clear();
                ui.close();
            }
        });
    });
    if item(ui, "Abre un archivo recuperado…", "", true) {
        app.open_recovered(&ctx);
        ui.close();
    }
    if item(ui, "Combinar…", "⇧⌘O", true) {
        app.merge_document(&ctx);
        ui.close();
    }
    ui.separator();
    if item(ui, "Guardar", "⌘S", true) {
        app.save_document(&ctx, false);
        ui.close();
    }
    ui.menu_button("Guardar como", |ui| {
        if item(ui, "Guardar en disco duro…", "⇧⌘S", true) {
            app.save_document(&ctx, true);
            ui.close();
        }
        if item(ui, "Guardar en la biblioteca…", "⌃⌥⌘R", true) {
            app.save_to_library(&ctx, false);
            ui.close();
        }
    });
    ui.add_enabled_ui(has_sel, |ui| {
        ui.menu_button("Guardar selección", |ui| {
            if item(ui, "Guardar en disco duro…", "", true) {
                app.save_selection(&ctx);
                ui.close();
            }
            if item(ui, "Guardar en la biblioteca…", "", true) {
                app.save_to_library(&ctx, true);
                ui.close();
            }
        });
    });
    ui.separator();
    ui.menu_button("Biblioteca", |ui| {
        if item(ui, "Biblioteca…", "⌥⌘L", true) {
            app.workspace = Workspace::Library;
            ui.close();
        }
        ui.separator();
        if item(ui, "Importar biblioteca…", "", true) {
            app.import_library_folder(&ctx);
            ui.close();
        }
        ui.separator();
        if item(ui, "Importar a biblioteca…", "", true) {
            app.import_files_to_library(&ctx);
            ui.close();
        }
    });
    if item(ui, "Actualizar la biblioteca…", "", true) {
        app.library.rescan();
        ui.close();
    }
    ui.separator();
    if item(ui, "Escanear…", "", true) {
        app.workspace = Workspace::Design;
        app.panel = Some(Panel::PixScan);
        app.pixscan.tab = crate::panels::PixScanTab::Scanner;
        ui.close();
    }
    ui.separator();
    if item(ui, "Imprimir…", "⌘P", true) {
        app.print_page(&ctx);
        ui.close();
    }
    if item(ui, "Configurar página para imprimir…", "", true) {
        app.workspace = Workspace::Design;
        app.panel = Some(Panel::PageSetup);
        ui.close();
    }
    if item(ui, "Cerrar pestaña", "⌘W", true) {
        app.close_active_tab();
        ui.close();
    }
    ui.separator();
    if item(ui, "Importar SVG…", "⇧⌘I", true) {
        app.import_svg(&ctx);
        ui.close();
    }
    if item(ui, "Importar imagen…", "⇧⌘M", true) {
        app.import_image(&ctx);
        ui.close();
    }
    if item(ui, "Exportar como SVG…", "⌥⌘E", true) {
        app.export_svg(&ctx);
        ui.close();
    }
    if item(ui, "Exportar como .studio3…", "", true) {
        app.export_studio3(&ctx);
        ui.close();
    }
}
