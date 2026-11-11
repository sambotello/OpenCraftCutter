//! Biblioteca local: una carpeta del usuario con documentos, SVG e imágenes, con miniaturas.
//!
//! - Secciones: Recientes, Todos los diseños y las carpetas de la biblioteca.
//! - Búsqueda por nombre en toda la biblioteca (subcarpetas incluidas).
//! - Orden por nombre o fecha; vista en cuadrícula o lista.
//! - Organizar: nueva carpeta, renombrar, mover y enviar a la Papelera.
//! - Abrir (pestaña nueva) o añadir al documento actual.

use crate::app::{OpenCraftCutterApp, Workspace};
use crate::canvas::Viewport;
use crate::theme;
use egui::{Color32, Frame, RichText, Ui};
use egui_material_icons::icons as mi;
use oc_core::Document;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const EMPTY_HINT: &str = "Guarda documentos en la biblioteca (Archivo › Guardar como) \
    o impórtalos con el botón de la izquierda.";
const FOLDER_TIP: &str = "Clic para abrir · clic derecho para más opciones";
const FILE_TIP: &str = "Doble clic para abrir en pestaña nueva · clic derecho para más opciones";
const IMAGE_EXT: [&str; 8] = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Document,
    Svg,
    /// Documento de Silhouette Studio.
    Studio3,
    Image,
}

impl Kind {
    fn of(p: &Path) -> Option<Kind> {
        if p.is_dir() {
            return Some(Kind::Folder);
        }
        let ext = p.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "opencraftcutter" => Some(Kind::Document),
            "svg" | "svgz" => Some(Kind::Svg),
            "studio3" => Some(Kind::Studio3),
            e if IMAGE_EXT.contains(&e) => Some(Kind::Image),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Folder => "carpeta",
            Kind::Document => "documento",
            Kind::Svg => "svg",
            Kind::Studio3 => "silhouette studio",
            Kind::Image => "imagen",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    path: PathBuf,
    kind: Kind,
    modified: Option<SystemTime>,
    size: u64,
}

impl Entry {
    fn new(path: PathBuf) -> Option<Entry> {
        let kind = Kind::of(&path)?;
        let meta = std::fs::metadata(&path).ok();
        Some(Entry {
            modified: meta.as_ref().and_then(|m| m.modified().ok()),
            size: meta.map(|m| m.len()).unwrap_or(0),
            path,
            kind,
        })
    }

    fn name(&self) -> String {
        let n = if self.kind == Kind::Folder {
            self.path.file_name()
        } else {
            self.path.file_stem()
        };
        n.map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Miniatura: formas en mm (vector) o textura (imagen).
pub enum Thumb {
    Vector {
        shapes: Vec<oc_core::Shape>,
        bounds: kurbo::Rect,
        /// Texturas reducidas de las imágenes, por índice en `shapes`.
        images: HashMap<usize, egui::TextureHandle>,
    },
    Raster(egui::TextureHandle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    Recent,
    #[default]
    Browse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortBy {
    #[default]
    Name,
    Date,
}

/// Diálogo pendiente (nueva carpeta o renombrar).
#[derive(Debug, Clone)]
enum Dialog {
    NewFolder(String),
    Rename(PathBuf, String),
}

#[derive(Default)]
pub struct LibraryState {
    pub root: Option<PathBuf>,
    pub current: Option<PathBuf>,
    section: Section,
    search: String,
    sort: SortBy,
    list_view: bool,
    entries: Vec<Entry>,
    scanned_for: Option<(PathBuf, String)>,
    thumbs: HashMap<PathBuf, Option<Thumb>>,
    dialog: Option<Dialog>,
}

impl LibraryState {
    /// Carpeta raíz de la biblioteca (se crea si no existe).
    pub fn root_dir(&mut self) -> PathBuf {
        self.root()
    }

    fn root(&mut self) -> PathBuf {
        if let Some(r) = &self.root {
            return r.clone();
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let r = home.join("Documents").join("OpenCraftCutter");
        let _ = std::fs::create_dir_all(&r);
        self.root = Some(r.clone());
        r
    }

    /// Cambia la carpeta raíz (None = la carpeta por defecto).
    pub fn set_root(&mut self, root: Option<&str>) {
        self.root = root.map(PathBuf::from);
        if let Some(r) = &self.root {
            let _ = std::fs::create_dir_all(r);
        }
        self.current = None;
        self.thumbs.clear();
        self.rescan();
    }

    pub fn rescan(&mut self) {
        self.scanned_for = None;
    }

    fn current_dir(&mut self) -> PathBuf {
        let root = self.root();
        self.current
            .clone()
            .filter(|c| c.starts_with(&root) && c.is_dir())
            .unwrap_or(root)
    }

    /// Lista la carpeta actual o, si hay búsqueda, toda la biblioteca.
    fn scan(&mut self, recent: &[String]) {
        let dir = self.current_dir();
        let key = (
            dir.clone(),
            format!(
                "{:?}|{:?}|{}|{}",
                self.section,
                self.sort,
                self.search,
                recent.join("\n")
            ),
        );
        if self.scanned_for.as_ref() == Some(&key) {
            return;
        }
        let query = self.search.trim().to_lowercase();
        let mut entries: Vec<Entry> = if self.section == Section::Recent {
            recent
                .iter()
                .filter_map(|p| Entry::new(PathBuf::from(p)))
                .filter(|e| e.kind != Kind::Folder)
                .collect()
        } else if !query.is_empty() {
            walk(&self.root())
                .into_iter()
                .filter_map(Entry::new)
                .filter(|e| e.name().to_lowercase().contains(&query))
                .collect()
        } else {
            std::fs::read_dir(&dir)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .filter_map(Entry::new)
                        .collect()
                })
                .unwrap_or_default()
        };
        entries.retain(|e| !e.name().starts_with('.'));
        if self.section != Section::Recent {
            let sort = self.sort;
            entries.sort_by(|a, b| {
                let folder_first = (a.kind != Kind::Folder).cmp(&(b.kind != Kind::Folder));
                folder_first.then_with(|| match sort {
                    SortBy::Name => a.name().to_lowercase().cmp(&b.name().to_lowercase()),
                    SortBy::Date => b.modified.cmp(&a.modified),
                })
            });
        }
        self.entries = entries;
        self.scanned_for = Some(key);
    }

    fn thumb(&mut self, ctx: &egui::Context, entry: &Entry) -> Option<&Thumb> {
        let path = entry.path.clone();
        if !self.thumbs.contains_key(&path) {
            let loaded = (|| -> Option<Thumb> {
                if entry.kind == Kind::Image {
                    let img = image::open(&path).ok()?.thumbnail(256, 256).to_rgba8();
                    let ci = egui::ColorImage::from_rgba_unmultiplied(
                        [img.width() as usize, img.height() as usize],
                        img.as_raw(),
                    );
                    return Some(Thumb::Raster(ctx.load_texture(
                        path.to_string_lossy(),
                        ci,
                        egui::TextureOptions::LINEAR,
                    )));
                }
                let shapes: Vec<oc_core::Shape> = match entry.kind {
                    Kind::Document => {
                        Document::from_json(&std::fs::read_to_string(&path).ok()?)
                            .ok()?
                            .shapes
                    }
                    Kind::Svg => {
                        oc_svg::import_svg(
                            &std::fs::read_to_string(&path).ok()?,
                            Default::default(),
                        )
                        .ok()?
                        .shapes
                    }
                    Kind::Studio3 => oc_studio3::parse(&std::fs::read(&path).ok()?)
                        .ok()?
                        .to_shapes(Default::default()),
                    _ => return None,
                };
                let shapes = crate::text::paths_for_ops(&shapes);
                let bounds = oc_core::edit::selection_bounds(shapes.iter())?;
                let images = shapes
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| {
                        let oc_core::ShapeKind::Image { png, .. } = &s.kind else {
                            return None;
                        };
                        let img = image::load_from_memory(&png.0)
                            .ok()?
                            .thumbnail(256, 256)
                            .to_rgba8();
                        let ci = egui::ColorImage::from_rgba_unmultiplied(
                            [img.width() as usize, img.height() as usize],
                            img.as_raw(),
                        );
                        let name = format!("{}#{i}", path.to_string_lossy());
                        Some((i, ctx.load_texture(name, ci, egui::TextureOptions::LINEAR)))
                    })
                    .collect();
                Some(Thumb::Vector {
                    shapes,
                    bounds,
                    images,
                })
            })();
            self.thumbs.insert(path.clone(), loaded);
        }
        self.thumbs.get(&path).and_then(|t| t.as_ref())
    }

    /// Copia archivos a la carpeta abierta (sin sobrescribir); devuelve cuántos copió.
    pub fn add_files(&mut self, files: &[PathBuf]) -> usize {
        let dir = if self.section == Section::Recent {
            self.root()
        } else {
            self.current_dir()
        };
        let mut n = 0;
        for f in files {
            let Some(name) = f.file_name() else { continue };
            let mut to = dir.join(name);
            let stem = f
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let ext = f
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let mut i = 2;
            while to.exists() {
                to = dir.join(format!("{stem} {i}{ext}"));
                i += 1;
            }
            if f != &to && std::fs::copy(f, &to).is_ok() {
                n += 1;
            }
        }
        self.rescan();
        n
    }

    fn forget(&mut self, path: &Path) {
        self.thumbs.retain(|p, _| !p.starts_with(path));
        self.rescan();
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                {
                    continue;
                }
                if p.is_dir() {
                    stack.push(p.clone());
                }
                out.push(p);
            }
        }
    }
    out
}

/// Fecha y hora locales «dd/mm/aaaa hh:mm».
fn format_date(t: Option<SystemTime>) -> String {
    t.map(|t| {
        chrono::DateTime::<chrono::Local>::from(t)
            .format("%d/%m/%Y %H:%M")
            .to_string()
    })
    .unwrap_or_default()
}

fn format_size(n: u64) -> String {
    match n {
        0 => String::new(),
        n if n < 1024 => format!("{n} B"),
        n if n < 1024 * 1024 => format!("{:.0} KB", n as f64 / 1024.0),
        n => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}

/// Acción pedida desde una tarjeta o fila.
enum Act {
    Open(Entry),
    Insert(Entry),
    Rename(Entry),
    Move(Entry, PathBuf),
    Trash(Entry),
    Reveal(Entry),
}

impl OpenCraftCutterApp {
    /// Añade el contenido de un archivo de la biblioteca al documento actual.
    fn library_insert(&mut self, ctx: &egui::Context, e: &Entry) {
        self.workspace = Workspace::Design;
        match e.kind {
            Kind::Document => {
                if let Ok(other) = std::fs::read_to_string(&e.path)
                    .map_err(|e| e.to_string())
                    .and_then(|t| Document::from_json(&t).map_err(|e| e.to_string()))
                {
                    self.commit();
                    self.doc.selected = other
                        .shapes
                        .into_iter()
                        .map(|mut s| {
                            s.id = 0;
                            self.doc.add_shape(s)
                        })
                        .collect();
                }
            }
            Kind::Svg => self.import_svg_path(ctx, &e.path),
            Kind::Studio3 => self.import_studio3_path(ctx, &e.path),
            Kind::Image => self.import_image_path(ctx, &e.path),
            Kind::Folder => {}
        }
    }

    fn library_act(&mut self, ctx: &egui::Context, act: Act) {
        match act {
            Act::Open(e) if e.kind == Kind::Folder => {
                self.library.section = Section::Browse;
                self.library.search.clear();
                self.library.current = Some(e.path);
                self.library.rescan();
            }
            Act::Open(e) => {
                self.open_path(ctx, &e.path);
                self.workspace = Workspace::Design;
            }
            Act::Insert(e) => self.library_insert(ctx, &e),
            Act::Rename(e) => self.library.dialog = Some(Dialog::Rename(e.path.clone(), e.name())),
            Act::Move(e, to) => {
                let target = to.join(e.path.file_name().unwrap_or_default());
                if target == e.path || target.starts_with(&e.path) {
                    return;
                }
                if target.exists() {
                    return self.notify(
                        ctx,
                        "Ya existe un archivo con ese nombre en la carpeta de destino.",
                    );
                }
                match std::fs::rename(&e.path, &target) {
                    Ok(()) => self.library.forget(&e.path),
                    Err(err) => self.notify(ctx, format!("No se pudo mover: {err}")),
                }
            }
            Act::Trash(e) => match trash::delete(&e.path) {
                Ok(()) => {
                    self.library.forget(&e.path);
                    self.notify(ctx, format!("«{}» enviado a la Papelera.", e.name()));
                }
                Err(err) => self.notify(ctx, format!("No se pudo enviar a la Papelera: {err}")),
            },
            Act::Reveal(e) => {
                let target = if e.kind == Kind::Folder {
                    e.path.clone()
                } else {
                    e.path.parent().map(Path::to_path_buf).unwrap_or_default()
                };
                let _ = if cfg!(target_os = "macos") {
                    std::process::Command::new("open")
                        .arg("-R")
                        .arg(&e.path)
                        .spawn()
                } else if cfg!(target_os = "windows") {
                    std::process::Command::new("explorer").arg(&target).spawn()
                } else {
                    std::process::Command::new("xdg-open").arg(&target).spawn()
                };
            }
        }
    }
}

fn draw_thumb(ui: &mut Ui, rect: egui::Rect, thumb: &Thumb) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::WHITE);
    match thumb {
        Thumb::Vector {
            shapes,
            bounds,
            images,
        } => {
            let vp = Viewport::fit(*bounds, rect, 6.0);
            for (i, s) in shapes.iter().enumerate() {
                if let Some(tex) = images.get(&i) {
                    crate::canvas::render::textured_quad(
                        &painter,
                        &vp,
                        tex.id(),
                        s.rect,
                        s.rotation,
                        Color32::WHITE,
                    );
                } else if !s.is_image() {
                    crate::canvas::render::draw_shape(&painter, &vp, s);
                }
            }
        }
        Thumb::Raster(tex) => {
            let ts = tex.size_vec2();
            let area = rect.shrink(6.0);
            let k = (area.width() / ts.x).min(area.height() / ts.y);
            let r = egui::Rect::from_center_size(area.center(), ts * k);
            painter.image(
                tex.id(),
                r,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
}

/// Menú contextual de una tarjeta o fila.
fn context_menu(
    resp: &egui::Response,
    e: &Entry,
    folders: &[PathBuf],
    root: &Path,
    act: &mut Option<Act>,
) {
    resp.context_menu(|ui| {
        let is_folder = e.kind == Kind::Folder;
        if ui
            .button(if is_folder {
                "Abrir carpeta"
            } else {
                "Abrir en pestaña nueva"
            })
            .clicked()
        {
            *act = Some(Act::Open(e.clone()));
            ui.close();
        }
        if !is_folder && ui.button("Añadir al documento actual").clicked() {
            *act = Some(Act::Insert(e.clone()));
            ui.close();
        }
        ui.separator();
        if ui.button("Renombrar…").clicked() {
            *act = Some(Act::Rename(e.clone()));
            ui.close();
        }
        ui.menu_button("Mover a", |ui| {
            let parent = e.path.parent().map(Path::to_path_buf);
            let mut targets = vec![root.to_path_buf()];
            targets.extend(folders.iter().cloned());
            for t in targets {
                if Some(&t) == parent.as_ref() || t == e.path {
                    continue;
                }
                let label = if t == root {
                    "Biblioteca (raíz)".to_string()
                } else {
                    let rel = t.strip_prefix(root).unwrap_or(&t);
                    let parts: Vec<_> = rel.iter().map(|c| c.to_string_lossy()).collect();
                    parts.join(" / ")
                };
                if ui
                    .button(format!("{}  {label}", <&str>::from(mi::ICON_FOLDER)))
                    .clicked()
                {
                    *act = Some(Act::Move(e.clone(), t));
                    ui.close();
                }
            }
        });
        if ui
            .button(if cfg!(target_os = "macos") {
                "Mostrar en Finder"
            } else {
                "Mostrar en la carpeta"
            })
            .clicked()
        {
            *act = Some(Act::Reveal(e.clone()));
            ui.close();
        }
        ui.separator();
        if ui
            .button(
                RichText::new("Enviar a la Papelera").color(Color32::from_rgb(0xff, 0x8a, 0x80)),
            )
            .clicked()
        {
            *act = Some(Act::Trash(e.clone()));
            ui.close();
        }
    });
}

fn dialog_window(app: &mut OpenCraftCutterApp, ctx: &egui::Context, current: &Path) {
    let Some(d) = app.library.dialog.clone() else {
        return;
    };
    let (title, mut text) = match &d {
        Dialog::NewFolder(t) => ("Nueva carpeta", t.clone()),
        Dialog::Rename(_, t) => ("Renombrar", t.clone()),
    };
    let mut ok = false;
    let mut cancel = false;
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(280.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ok = true;
            } else if !r.has_focus() {
                r.request_focus();
            }
            ui.horizontal(|ui| {
                if ui.button("Cancelar").clicked() {
                    cancel = true;
                }
                if ui.button("Aceptar").clicked() {
                    ok = true;
                }
            });
        });
    if cancel || ui_escape(ctx) {
        app.library.dialog = None;
        return;
    }
    let name: String = text
        .trim()
        .chars()
        .filter(|c| !"/\\:*?\"<>|".contains(*c))
        .collect();
    app.library.dialog = Some(match &d {
        Dialog::NewFolder(_) => Dialog::NewFolder(text.clone()),
        Dialog::Rename(p, _) => Dialog::Rename(p.clone(), text.clone()),
    });
    if !ok || name.is_empty() {
        return;
    }
    let result = apply_dialog(&d, &name, current);
    match result {
        Ok(_) => {
            if let Dialog::Rename(p, _) = &d {
                app.library.forget(p);
            }
            app.library.rescan();
            app.library.dialog = None;
        }
        Err(e) => app.notify(ctx, format!("No se pudo completar: {e}")),
    }
}

/// Crea la carpeta o renombra el archivo del diálogo (el nombre ya viene limpio). Al renombrar
/// se conserva la extensión y no se sobrescribe nada.
fn apply_dialog(d: &Dialog, name: &str, current: &Path) -> std::io::Result<PathBuf> {
    match d {
        Dialog::NewFolder(_) => std::fs::create_dir(current.join(name)).map(|_| current.join(name)),
        Dialog::Rename(p, _) => {
            let ext = if p.is_dir() {
                String::new()
            } else {
                p.extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default()
            };
            let to = p.with_file_name(format!("{name}{ext}"));
            if to.exists() && &to != p {
                Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "ya existe",
                ))
            } else {
                std::fs::rename(p, &to).map(|_| to)
            }
        }
    }
}

fn ui_escape(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.key_pressed(egui::Key::Escape))
}

pub fn library_workspace(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let recent = app.prefs.recent_files.clone();
    app.library.scan(&recent);
    let root = app.library.root();
    let current = app.library.current_dir();
    let folders: Vec<PathBuf> = {
        let mut f: Vec<PathBuf> = walk(&root).into_iter().filter(|p| p.is_dir()).collect();
        f.sort_by_key(|p| p.to_string_lossy().to_lowercase());
        f
    };
    let section = app.library.section;
    let searching = !app.library.search.trim().is_empty();
    let mut act: Option<Act> = None;

    egui::Panel::left("library_side")
        .exact_size(230.0)
        .resizable(false)
        .frame(
            Frame::NONE
                .fill(theme::CHROME_900)
                .inner_margin(egui::Margin::same(12)),
        )
        .show(ui, |ui| {
            ui.label(
                RichText::new("BIBLIOTECA")
                    .small()
                    .strong()
                    .color(theme::CHROME_300),
            );
            ui.add_space(6.0);
            if ui
                .selectable_label(
                    section == Section::Recent,
                    format!("{} Recientes", <&str>::from(mi::ICON_SCHEDULE)),
                )
                .clicked()
            {
                app.library.section = Section::Recent;
                app.library.search.clear();
                app.library.rescan();
            }
            if ui
                .selectable_label(
                    section == Section::Browse && current == root && !searching,
                    format!("{} Todos los diseños", <&str>::from(mi::ICON_FOLDER)),
                )
                .clicked()
            {
                app.library.section = Section::Browse;
                app.library.search.clear();
                app.library.current = None;
                app.library.rescan();
            }
            for f in &folders {
                let depth = f
                    .strip_prefix(&root)
                    .map(|r| r.components().count())
                    .unwrap_or(1);
                let name = f
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.add_space(12.0 * depth as f32);
                    let r = ui.selectable_label(
                        section == Section::Browse && &current == f && !searching,
                        format!("{} {name}", <&str>::from(mi::ICON_FOLDER)),
                    );
                    if r.clicked() {
                        act = Some(Act::Open(Entry {
                            path: f.clone(),
                            kind: Kind::Folder,
                            modified: None,
                            size: 0,
                        }));
                    }
                });
            }
            ui.add_space(12.0);
            if ui
                .button(format!(
                    "{} Nueva carpeta",
                    <&str>::from(mi::ICON_CREATE_NEW_FOLDER)
                ))
                .clicked()
            {
                app.library.dialog = Some(Dialog::NewFolder("Nueva carpeta".into()));
            }
            if ui
                .button(format!(
                    "{} Importar archivos…",
                    <&str>::from(mi::ICON_DOWNLOAD)
                ))
                .clicked()
            {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter(
                        "Diseños",
                        &[
                            "opencraftcutter",
                            "studio3",
                            "svg",
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
                    .pick_files()
                {
                    for f in files {
                        if let Some(name) = f.file_name() {
                            let _ = std::fs::copy(&f, current.join(name));
                        }
                    }
                    app.library.rescan();
                }
            }
            if ui
                .button(format!(
                    "{} Abrir carpeta",
                    <&str>::from(mi::ICON_FOLDER_OPEN)
                ))
                .clicked()
            {
                let _ = std::process::Command::new(if cfg!(target_os = "macos") {
                    "open"
                } else {
                    "xdg-open"
                })
                .arg(&current)
                .spawn();
            }
            if ui
                .button(format!("{} Actualizar", <&str>::from(mi::ICON_REFRESH)))
                .clicked()
            {
                app.library.thumbs.clear();
                app.library.rescan();
            }
            ui.add_space(12.0);
            ui.label(
                RichText::new(root.display().to_string())
                    .small()
                    .color(theme::CHROME_400),
            );
        });

    egui::CentralPanel::default()
        .frame(
            Frame::NONE
                .fill(theme::CHROME_800)
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ui, |ui| {
            // Barra superior: título o ruta, búsqueda, orden y vista.
            ui.horizontal(|ui| {
                let can_go_back = section == Section::Browse && current != root && !searching;
                if can_go_back
                    && ui
                        .button(mi::ICON_ARROW_BACK.rich_text())
                        .on_hover_text("Volver")
                        .clicked()
                {
                    app.library.current = current
                        .parent()
                        .map(Path::to_path_buf)
                        .filter(|p| p.starts_with(&root) && p != &root);
                    app.library.rescan();
                }
                let title = match section {
                    Section::Recent => "Recientes".to_string(),
                    _ if searching => "Resultados de la búsqueda".to_string(),
                    _ if current == root => "Todos los diseños".to_string(),
                    _ => current
                        .strip_prefix(&root)
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                };
                ui.label(RichText::new(title).heading().color(theme::CHROME_100));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let list = app.library.list_view;
                    if ui
                        .add(egui::Button::selectable(
                            list,
                            mi::ICON_VIEW_LIST.rich_text().size(18.0),
                        ))
                        .on_hover_text("Lista")
                        .clicked()
                    {
                        app.library.list_view = true;
                    }
                    if ui
                        .add(egui::Button::selectable(
                            !list,
                            mi::ICON_GRID_VIEW.rich_text().size(18.0),
                        ))
                        .on_hover_text("Cuadrícula")
                        .clicked()
                    {
                        app.library.list_view = false;
                    }
                    let before = app.library.sort;
                    egui::ComboBox::from_id_salt("library_sort")
                        .width(110.0)
                        .selected_text(match app.library.sort {
                            SortBy::Name => "Nombre",
                            SortBy::Date => "Fecha",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.library.sort, SortBy::Name, "Nombre");
                            ui.selectable_value(
                                &mut app.library.sort,
                                SortBy::Date,
                                "Fecha (recientes primero)",
                            );
                        });
                    if app.library.sort != before {
                        app.library.rescan();
                    }
                    ui.label(RichText::new("Ordenar").color(theme::CHROME_300));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut app.library.search)
                            .hint_text("Buscar en la biblioteca…")
                            .desired_width(220.0),
                    );
                    if r.changed() {
                        if !app.library.search.trim().is_empty() {
                            app.library.section = Section::Browse;
                        }
                        app.library.rescan();
                    }
                });
            });
            ui.add_space(8.0);

            let entries = app.library.entries.clone();
            if entries.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.3);
                    ui.label(
                        mi::ICON_LIBRARY_BOOKS
                            .rich_text()
                            .size(40.0)
                            .color(theme::CHROME_400),
                    );
                    let (t, d) = if searching {
                        (
                            "Sin resultados",
                            "Ningún diseño de la biblioteca coincide con la búsqueda.",
                        )
                    } else if section == Section::Recent {
                        (
                            "Sin archivos recientes",
                            "Los archivos que abras o guardes aparecerán aquí.",
                        )
                    } else {
                        ("Esta carpeta está vacía", EMPTY_HINT)
                    };
                    ui.label(RichText::new(t).heading());
                    ui.label(RichText::new(d).color(theme::CHROME_300));
                });
                return;
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                if app.library.list_view {
                    egui::Grid::new("library_list")
                        .num_columns(4)
                        .striped(true)
                        .spacing([18.0, 6.0])
                        .show(ui, |ui| {
                            for h in ["Nombre", "Tipo", "Modificado", "Tamaño"] {
                                ui.label(
                                    RichText::new(h).small().strong().color(theme::CHROME_300),
                                );
                            }
                            ui.end_row();
                            for e in &entries {
                                let icon = match e.kind {
                                    Kind::Folder => mi::ICON_FOLDER,
                                    Kind::Document => mi::ICON_DESCRIPTION,
                                    Kind::Svg | Kind::Studio3 => mi::ICON_SHAPE_LINE,
                                    Kind::Image => mi::ICON_IMAGE,
                                };
                                let r = ui.add(
                                    egui::Label::new(format!(
                                        "{}  {}",
                                        <&str>::from(icon),
                                        e.name()
                                    ))
                                    .sense(egui::Sense::click()),
                                );
                                if r.double_clicked() || (e.kind == Kind::Folder && r.clicked()) {
                                    act = Some(Act::Open(e.clone()));
                                }
                                context_menu(&r, e, &folders, &root, &mut act);
                                ui.label(RichText::new(e.kind.label()).color(theme::CHROME_300));
                                ui.label(
                                    RichText::new(format_date(e.modified)).color(theme::CHROME_300),
                                );
                                ui.label(
                                    RichText::new(if e.kind == Kind::Folder {
                                        String::new()
                                    } else {
                                        format_size(e.size)
                                    })
                                    .color(theme::CHROME_300),
                                );
                                ui.end_row();
                            }
                        });
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
                        for e in &entries {
                            let (card, resp) = ui.allocate_exact_size(
                                egui::vec2(150.0, 170.0),
                                egui::Sense::click(),
                            );
                            let hovered = resp.hovered();
                            ui.painter().rect(
                                card,
                                8.0,
                                if hovered {
                                    theme::CHROME_600
                                } else {
                                    theme::CHROME_700
                                },
                                egui::Stroke::new(1.0, theme::CHROME_600),
                                egui::StrokeKind::Inside,
                            );
                            let preview = egui::Rect::from_min_size(
                                card.min + egui::vec2(8.0, 8.0),
                                egui::vec2(134.0, 120.0),
                            );
                            if e.kind == Kind::Folder {
                                ui.painter().text(
                                    preview.center(),
                                    egui::Align2::CENTER_CENTER,
                                    String::from(mi::ICON_FOLDER),
                                    egui::FontId::new(48.0, mi::ICON_FOLDER.font_family()),
                                    theme::CHROME_300,
                                );
                            } else {
                                match app.library.thumb(&ctx, e) {
                                    Some(t) => draw_thumb(ui, preview, t),
                                    None => {
                                        ui.painter().rect_filled(preview, 4.0, theme::CHROME_800);
                                        ui.painter().text(
                                            preview.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "Sin vista previa",
                                            egui::FontId::proportional(11.0),
                                            theme::CHROME_400,
                                        );
                                    }
                                }
                            }
                            let name = e.name();
                            let short: String = if name.chars().count() > 20 {
                                name.chars().take(19).chain(std::iter::once('…')).collect()
                            } else {
                                name.clone()
                            };
                            ui.painter().text(
                                egui::pos2(card.min.x + 8.0, card.max.y - 30.0),
                                egui::Align2::LEFT_TOP,
                                short,
                                egui::FontId::proportional(12.0),
                                theme::CHROME_100,
                            );
                            ui.painter().text(
                                egui::pos2(card.min.x + 8.0, card.max.y - 15.0),
                                egui::Align2::LEFT_TOP,
                                e.kind.label(),
                                egui::FontId::proportional(10.0),
                                theme::CHROME_300,
                            );
                            if resp.double_clicked() || (e.kind == Kind::Folder && resp.clicked()) {
                                act = Some(Act::Open(e.clone()));
                            }
                            let resp = resp.on_hover_text(match e.kind {
                                Kind::Folder => format!("{name}\n{FOLDER_TIP}"),
                                _ => format!("{name}\n{FILE_TIP}"),
                            });
                            context_menu(&resp, e, &folders, &root, &mut act);
                        }
                    });
                }
            });
        });
    dialog_window(app, &ctx, &current);
    if let Some(a) = act {
        app.library_act(&ctx, a);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn civil_date() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_793_277_600);
        assert_eq!(format_date(Some(t)).len(), "29/10/2026 13:00".len());
        assert_eq!(format_date(None), "");
    }

    /// Biblioteca de prueba en una carpeta temporal propia.
    fn library(tag: &str) -> (crate::app::OpenCraftCutterApp, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "opencraftcutter-biblio-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Animales")).unwrap();
        let mut doc = oc_core::Document::default();
        doc.add_shape(crate::flow_tests::rect(10.0, crate::flow_tests::RED));
        std::fs::write(root.join("gato.opencraftcutter"), doc.to_json()).unwrap();
        std::fs::write(
            root.join("Animales").join("perro.opencraftcutter"),
            doc.to_json(),
        )
        .unwrap();
        std::fs::write(root.join(".oculto.opencraftcutter"), doc.to_json()).unwrap();
        std::fs::write(root.join("notas.txt"), "no es un diseño").unwrap();
        let mut app = crate::flow_tests::app();
        app.library.set_root(Some(root.to_str().unwrap()));
        (app, root)
    }

    fn names(app: &mut crate::app::OpenCraftCutterApp) -> Vec<String> {
        app.library.scan(&[]);
        app.library.entries.iter().map(|e| e.name()).collect()
    }

    #[test]
    fn lists_folders_first_and_hides_unsupported() {
        let (mut app, root) = library("lista");
        let n = names(&mut app);
        assert_eq!(n.first().map(String::as_str), Some("Animales"), "{n:?}");
        assert!(n.iter().any(|x| x.starts_with("gato")));
        assert!(
            !n.iter()
                .any(|x| x.contains("oculto") || x.contains("notas")),
            "{n:?}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn search_looks_in_every_folder() {
        let (mut app, root) = library("busca");
        app.library.search = "perro".into();
        let n = names(&mut app);
        assert_eq!(n.len(), 1, "{n:?}");
        assert!(n[0].starts_with("perro"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn open_folder_insert_and_move() {
        let ctx = egui::Context::default();
        let (mut app, root) = library("acciones");
        names(&mut app);
        let folder = app
            .library
            .entries
            .iter()
            .find(|e| e.kind == Kind::Folder)
            .unwrap()
            .clone();
        let gato = app
            .library
            .entries
            .iter()
            .find(|e| e.name().starts_with("gato"))
            .unwrap()
            .clone();
        // Insertar añade las formas al documento actual.
        app.library_act(&ctx, Act::Insert(gato.clone()));
        assert_eq!(app.doc.shapes.len(), 1);
        // Mover a la carpeta.
        app.library_act(&ctx, Act::Move(gato.clone(), folder.path.clone()));
        assert!(root.join("Animales").join("gato.opencraftcutter").exists());
        assert!(!root.join("gato.opencraftcutter").exists());
        // Abrir la carpeta entra en ella.
        app.library_act(&ctx, Act::Open(folder));
        let n = names(&mut app);
        assert_eq!(n.len(), 2, "{n:?}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn add_files_never_overwrites() {
        let (mut app, root) = library("copiar");
        let src = std::env::temp_dir().join(format!("gato-{}.opencraftcutter", std::process::id()));
        std::fs::copy(root.join("gato.opencraftcutter"), &src).unwrap();
        let renamed = root.join(src.file_name().unwrap());
        assert_eq!(app.library.add_files(&[src.clone(), src.clone()]), 2);
        assert!(renamed.exists());
        let stem = src.file_stem().unwrap().to_string_lossy().into_owned();
        assert!(
            root.join(format!("{stem} 2.opencraftcutter")).exists(),
            "la segunda copia lleva « 2»"
        );
        let _ = std::fs::remove_file(src);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rename_keeps_extension_and_new_folder() {
        let (_, root) = library("renombrar");
        let gato = root.join("gato.opencraftcutter");
        let to =
            apply_dialog(&Dialog::Rename(gato.clone(), String::new()), "michi", &root).unwrap();
        assert_eq!(to, root.join("michi.opencraftcutter"));
        assert!(to.exists() && !gato.exists());
        // No se sobrescribe un archivo existente.
        std::fs::write(root.join("otro.opencraftcutter"), "{}").unwrap();
        let err = apply_dialog(&Dialog::Rename(to.clone(), String::new()), "otro", &root);
        assert!(err.is_err() && to.exists());
        let f = apply_dialog(&Dialog::NewFolder(String::new()), "Nueva", &root).unwrap();
        assert!(f.is_dir());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2 KB");
        assert_eq!(format_size(3 * 1_048_576), "3.0 MB");
    }
}
