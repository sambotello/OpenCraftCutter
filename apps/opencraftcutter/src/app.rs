//! Estado de la aplicación y composición de la ventana.

use crate::canvas::CanvasState;
use crate::canvas::Viewport;
use crate::prefs::Prefs;
use crate::theme;
use oc_core::shape::Stroke;
use oc_core::{Color, Document, History};
use std::path::PathBuf;

/// Documento abierto en segundo plano (pestaña inactiva).
pub struct DocSlot {
    pub doc: Document,
    pub history: History,
    pub file_path: Option<PathBuf>,
    pub dirty: bool,
    pub viewport: Viewport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workspace {
    Design,
    Library,
    Send,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    EditPoints,
    Line,
    Rect,
    Ellipse,
    Polygon,
    Freehand,
    Text,
    Note,
    Eraser,
    Knife,
    Pan,
    ZoomDrag,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Seleccionar (V)",
            Tool::EditPoints => "Editar puntos (A)",
            Tool::Line => "Línea (L)",
            Tool::Rect => "Rectángulo (R)",
            Tool::Ellipse => "Elipse (E)",
            Tool::Polygon => "Polígono (G): clic por vértice, doble clic cierra",
            Tool::Freehand => "Mano alzada (P)",
            Tool::Text => "Texto (T)",
            Tool::Note => "Nota",
            Tool::Eraser => "Borrador",
            Tool::Knife => "Cuchilla",
            Tool::Pan => "Desplazar (H, o mantén Espacio)",
            Tool::ZoomDrag => "Zoom de arrastre (Z)",
        }
    }

    pub fn in_development(self) -> bool {
        matches!(self, Tool::Note)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    PageSetup,
    Transform,
    Fill,
    Line,
    Text,
    PrintCut,
    Layers,
    Modify,
    Offset,
    Trace,
    Replicate,
    PixScan,
    SpecialTools,
    EditPoints,
    Rhinestone,
    Sketch,
    Warp,
    Emboss,
    Weeding,
}

impl Panel {
    pub const ALL: [Panel; 18] = [
        Panel::PageSetup,
        Panel::Fill,
        Panel::Line,
        Panel::Trace,
        Panel::Transform,
        Panel::Replicate,
        Panel::Text,
        Panel::PrintCut,
        Panel::Modify,
        Panel::Layers,
        Panel::Offset,
        Panel::Weeding,
        Panel::SpecialTools,
        Panel::PixScan,
        Panel::Rhinestone,
        Panel::Sketch,
        Panel::Warp,
        Panel::Emboss,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Panel::PageSetup => "Configuración de página",
            Panel::Transform => "Transformar",
            Panel::Fill => "Relleno",
            Panel::Line => "Estilo de trazo",
            Panel::Text => "Estilo de texto",
            Panel::PrintCut => "Imprimir y cortar",
            Panel::Layers => "Capas",
            Panel::Modify => "Modificar",
            Panel::Offset => "Desplazamiento",
            Panel::Trace => "Trazar",
            Panel::Replicate => "Replicar",
            Panel::PixScan => "PixScan",
            Panel::SpecialTools => "Herramientas especiales",
            Panel::EditPoints => "Editar puntos",
            Panel::Rhinestone => "Pedrería",
            Panel::Sketch => "Boceto",
            Panel::Warp => "Deformar",
            Panel::Emboss => "Relieve",
            Panel::Weeding => "Desbarbado",
        }
    }

    /// Descripción para los paneles aún no implementados.
    pub fn description(self) -> Option<&'static str> {
        match self {
            Panel::Modify => Some("Soldar, intersecar, restar, dividir y combinar formas."),
            Panel::Offset => Some("Contornos exteriores e interiores a una distancia dada."),
            Panel::Trace => Some("Convertir imágenes PNG/JPG en líneas de corte."),
            Panel::Replicate => Some("Duplicar en fila, columna, matriz o alrededor de un punto."),
            Panel::PixScan => Some("Cortar a partir de una foto del tapete PixScan."),
            Panel::Rhinestone => Some("Rellenar formas con plantillas de pedrería."),
            Panel::Sketch => Some("Rellenos de sombreado para plumas de dibujo."),
            Panel::Warp => Some("Deformar texto y formas con envolventes."),
            Panel::Emboss => Some("Patrones de relieve y repujado."),
            _ => None,
        }
    }
}

/// Valores por defecto para formas nuevas.
#[derive(Debug, Clone, Copy)]
pub struct Defaults {
    pub fill: Option<Color>,
    pub stroke: Stroke,
}

pub struct OpenCraftCutterApp {
    pub doc: Document,
    pub history: History,
    pub file_path: Option<PathBuf>,
    pub dirty: bool,
    pub workspace: Workspace,
    pub tool: Tool,
    pub panel: Option<Panel>,
    pub defaults: Defaults,
    pub canvas: CanvasState,
    /// Mensaje transitorio de la barra de estado (texto, instante de expiración).
    pub toast: Option<(String, f64)>,
    /// Aviso pedido desde donde no hay contexto de egui; se muestra en el siguiente fotograma.
    pub pending_notice: Option<String>,
    /// Diálogo de «Insertar» abierto (QR, plantillas).
    pub insert_dialog: Option<crate::insert::InsertDialog>,
    fonts: std::sync::OnceLock<std::sync::Arc<Vec<String>>>,
    pub library: crate::shell::LibraryState,
    pub machine: crate::machine::MachineState,
    /// Pestañas inactivas; la activa vive en los campos `doc`, `history`, `file_path` y `dirty`.
    inactive: Vec<DocSlot>,
    /// Posición de la pestaña activa en el orden visible.
    active: usize,
    pub prefs: Prefs,
    pub show_prefs: bool,
    pub show_page_guide: bool,
    pub pixscan: crate::panels::PixScanState,
    pixscan_debug_done: bool,
    /// Herramienta del fotograma anterior y panel a restaurar al salir de Editar puntos.
    last_tool: Tool,
    /// Último guardado de recuperación (s desde el arranque).
    pub last_autosave: f64,
    /// Portapapeles interno (formas copiadas).
    pub clipboard: Vec<oc_core::Shape>,
    /// Texto que dejamos en el portapapeles del sistema al copiar.
    pub clipboard_text: String,
    panel_before_edit: Option<Option<Panel>>,
}

impl OpenCraftCutterApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_material_icons::initialize(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        add_symbol_fallback(&cc.egui_ctx);
        let prefs = Prefs::load(cc.storage);
        let mut doc = Document::default();
        doc.page = prefs.page_setup();
        let mut canvas = CanvasState::default();
        canvas.eraser_width_mm = prefs.eraser_width_mm;
        let mut library = crate::shell::LibraryState::default();
        library.set_root(prefs.library_root.as_deref());
        // Depuración: OPENCRAFTCUTTER_DEBUG_LIBRARY=carpeta abre la Biblioteca sobre esa carpeta.
        let debug_library = std::env::var("OPENCRAFTCUTTER_DEBUG_LIBRARY")
            .ok()
            .filter(|_| cfg!(debug_assertions));
        if let Some(dir) = &debug_library {
            library.set_root(Some(dir));
        }
        // Depuración: OPENCRAFTCUTTER_DEBUG_DEMO=fill carga formas de ejemplo y abre el panel Relleno.
        let demo = cfg!(debug_assertions)
            && std::env::var("OPENCRAFTCUTTER_DEBUG_DEMO").as_deref() == Ok("fill");
        let mut panel = Some(Panel::PageSetup);
        if cfg!(debug_assertions)
            && std::env::var("OPENCRAFTCUTTER_DEBUG_DEMO").as_deref() == Ok("trace")
        {
            // OPENCRAFTCUTTER_DEBUG_IMAGE=ruta.png usa otra imagen en lugar del logo con texto.
            let bytes = std::env::var("OPENCRAFTCUTTER_DEBUG_IMAGE")
                .ok()
                .and_then(|p| std::fs::read(p).ok())
                .unwrap_or_else(|| include_bytes!("../assets/logo-with-text.png").to_vec());
            if let Ok((png, w, h)) = oc_trace::to_png(&bytes) {
                // Ancho de 180 mm conservando la proporción.
                let height = 180.0 * h as f64 / w.max(1) as f64;
                let kind = oc_core::shape::ShapeKind::Image {
                    png: oc_core::shape::ImageData(std::sync::Arc::new(png)),
                    px_width: w,
                    px_height: h,
                };
                let img = oc_core::Shape::new(
                    0,
                    "logo",
                    kurbo::Rect::new(15.0, 30.0, 195.0, 30.0 + height),
                    kind,
                );
                let id = doc.add_shape(img);
                doc.selected = vec![id];
                panel = Some(Panel::Trace);
            }
        }
        if cfg!(debug_assertions)
            && std::env::var("OPENCRAFTCUTTER_DEBUG_DEMO").as_deref() == Ok("line")
        {
            use oc_core::shape::{LineCap, LineJoin, LineStyle, ShapeKind, StrokeAlign};
            let mut ids = Vec::new();
            for (i, (join, cap, style, align)) in [
                (
                    LineJoin::Miter,
                    LineCap::Butt,
                    LineStyle::Solid,
                    StrokeAlign::Center,
                ),
                (
                    LineJoin::Round,
                    LineCap::Round,
                    LineStyle::Solid,
                    StrokeAlign::Inside,
                ),
                (
                    LineJoin::Bevel,
                    LineCap::Square,
                    LineStyle::Dashed,
                    StrokeAlign::Center,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let x = 15.0 + i as f64 * 62.0;
                let mut tri = oc_core::Shape::new(
                    0,
                    "Triángulo",
                    kurbo::Rect::ZERO,
                    ShapeKind::Path {
                        subpaths: vec![],
                        fill_rule: Default::default(),
                    },
                );
                tri.set_world_subpaths(&[(
                    vec![
                        kurbo::Point::new(x, 70.0),
                        kurbo::Point::new(x + 25.0, 20.0),
                        kurbo::Point::new(x + 50.0, 70.0),
                    ],
                    true,
                )]);
                tri.fill = Some(Color::rgb(0xff, 0xd2, 0x3f));
                tri.stroke.width_pt = 14.0;
                tri.stroke.join = join;
                tri.stroke.align = align;
                tri.stroke.style = style;
                tri.stroke.color = Color::rgb(0x1f, 0x5f, 0xbf);
                ids.push(doc.add_shape(tri));
                let mut line = oc_core::Shape::new(
                    0,
                    "Línea",
                    kurbo::Rect::ZERO,
                    ShapeKind::Path {
                        subpaths: vec![],
                        fill_rule: Default::default(),
                    },
                );
                line.set_world_subpaths(&[(
                    vec![
                        kurbo::Point::new(x + 5.0, 95.0),
                        kurbo::Point::new(x + 45.0, 95.0),
                    ],
                    false,
                )]);
                line.stroke.width_pt = 18.0;
                line.stroke.cap = cap;
                line.stroke.opacity = 0.7;
                doc.add_shape(line);
            }
            doc.selected = vec![ids[1]];
            panel = Some(Panel::Line);
        }
        if cfg!(debug_assertions)
            && std::env::var("OPENCRAFTCUTTER_DEBUG_DEMO").as_deref() == Ok("special")
        {
            // Un rectángulo seleccionado con Herramientas especiales abierto.
            let r = oc_core::Shape::new(
                0,
                "Rectángulo",
                kurbo::Rect::new(30.0, 30.0, 90.5, 80.9),
                oc_core::shape::ShapeKind::Rect { corner_radius: 0.0 },
            );
            let id = doc.add_shape(r);
            doc.selected = vec![id];
            panel = Some(Panel::SpecialTools);
        }
        if cfg!(debug_assertions)
            && std::env::var("OPENCRAFTCUTTER_DEBUG_DEMO").as_deref() == Ok("printcut")
        {
            // Una forma rellena con Imprimir y cortar abierto.
            let mut e = oc_core::Shape::new(
                0,
                "Elipse",
                kurbo::Rect::new(60.0, 90.0, 150.0, 170.0),
                oc_core::shape::ShapeKind::Ellipse,
            );
            e.fill = Some(Color::rgb(0x22, 0xb8, 0xd8));
            doc.add_shape(e);
            panel = Some(Panel::PrintCut);
        }
        if cfg!(debug_assertions)
            && std::env::var("OPENCRAFTCUTTER_DEBUG_DEMO").as_deref() == Ok("text")
        {
            // Un texto de dos líneas seleccionado, con Estilo de texto abierto.
            let kind = oc_core::shape::ShapeKind::Text {
                text: "Hola AVATAR\nOpenCraftCutter".into(),
                font_family: crate::text::DEFAULT_FONT_FAMILY.into(),
                font_size_pt: 48.0,
                align: oc_core::shape::TextAlign::Left,
                bold: false,
                italic: false,
                line_height: 1.2,
                fx: Default::default(),
            };
            let (w, h) = crate::text::TextStyle::from_kind(&kind)
                .map(|st| crate::text::measure(&st))
                .unwrap_or((50.0, 20.0));
            let t = oc_core::Shape::new(
                0,
                "Texto",
                kurbo::Rect::new(25.0, 40.0, 25.0 + w, 40.0 + h),
                kind,
            );
            let id = doc.add_shape(t);
            doc.selected = vec![id];
            panel = Some(Panel::Text);
        }
        if cfg!(debug_assertions)
            && std::env::var("OPENCRAFTCUTTER_DEBUG_DEMO").as_deref() == Ok("replicate")
        {
            // Un rectángulo alargado (seleccionado) y una elipse grande como guía.
            use oc_core::shape::ShapeKind;
            let r = oc_core::Shape::new(
                0,
                "Rectángulo",
                kurbo::Rect::new(90.0, 120.0, 110.0, 128.0),
                ShapeKind::Rect { corner_radius: 0.0 },
            );
            let e = oc_core::Shape::new(
                0,
                "Elipse",
                kurbo::Rect::new(40.0, 60.0, 170.0, 190.0),
                ShapeKind::Ellipse,
            );
            let rid = doc.add_shape(r);
            doc.add_shape(e);
            doc.selected = vec![rid];
            panel = Some(Panel::Replicate);
        }
        if cfg!(debug_assertions)
            && std::env::var("OPENCRAFTCUTTER_DEBUG_DEMO").as_deref() == Ok("transform")
        {
            // Tres rectángulos de tamaños distintos, seleccionados, con Transformar abierto.
            let mut ids = Vec::new();
            for (x0, y0, x1, y1) in [
                (25.0, 25.0, 66.0, 58.0),
                (88.0, 87.0, 108.0, 136.0),
                (32.0, 167.0, 89.0, 187.0),
            ] {
                let r = oc_core::Shape::new(
                    0,
                    "Rectángulo",
                    kurbo::Rect::new(x0, y0, x1, y1),
                    oc_core::shape::ShapeKind::Rect { corner_radius: 0.0 },
                );
                ids.push(doc.add_shape(r));
            }
            doc.selected = ids;
            panel = Some(Panel::Transform);
        }
        if demo {
            use oc_core::shape::{Gradient, GradientStop, ShapeKind};
            let mut a = oc_core::Shape::new(
                0,
                "Degradado",
                kurbo::Rect::new(20.0, 20.0, 120.0, 80.0),
                ShapeKind::Rect { corner_radius: 0.0 },
            );
            a.fill = Some(Color::rgb(0xe3, 0x34, 0x3f));
            a.fill_gradient = Some(Gradient {
                stops: vec![
                    GradientStop {
                        pos: 0.0,
                        color: Color::rgb(0xe3, 0x34, 0x3f),
                    },
                    GradientStop {
                        pos: 0.5,
                        color: Color::rgb(0xff, 0xd2, 0x3f),
                    },
                    GradientStop {
                        pos: 1.0,
                        color: Color::rgb(0x22, 0xb8, 0xd8),
                    },
                ],
                angle_deg: 30.0,
            });
            let mut b = oc_core::Shape::new(
                0,
                "Transparente",
                kurbo::Rect::new(70.0, 60.0, 150.0, 130.0),
                ShapeKind::Ellipse,
            );
            b.fill = Some(Color::rgb(0x3c, 0xb4, 0x4b));
            b.fill_opacity = 0.5;
            let id = doc.add_shape(a);
            doc.add_shape(b);
            doc.selected = vec![id];
            panel = Some(Panel::Fill);
        }
        OpenCraftCutterApp {
            doc,
            history: History::default(),
            file_path: None,
            dirty: false,
            workspace: if debug_library.is_some() {
                Workspace::Library
            } else {
                Workspace::Design
            },
            tool: Tool::Select,
            panel,
            defaults: Defaults {
                fill: None,
                stroke: Stroke::default(),
            },
            canvas,
            toast: None,
            pending_notice: None,
            insert_dialog: None,
            fonts: std::sync::OnceLock::new(),
            library,
            machine: Default::default(),
            inactive: Vec::new(),
            active: 0,
            prefs,
            show_prefs: false,
            show_page_guide: false,
            pixscan: Default::default(),
            pixscan_debug_done: false,
            last_tool: Tool::Select,
            last_autosave: 0.0,
            clipboard: Vec::new(),
            clipboard_text: String::new(),
            panel_before_edit: None,
        }
    }

    // ----- Pestañas -----

    pub fn active_tab(&self) -> usize {
        self.active
    }

    /// (nombre, sucio) de cada pestaña en orden visible.
    pub fn tab_titles(&self) -> Vec<(String, bool)> {
        let mut v: Vec<(String, bool)> = self
            .inactive
            .iter()
            .map(|t| (t.doc.name.clone(), t.dirty))
            .collect();
        v.insert(
            self.active.min(v.len()),
            (self.doc.name.clone(), self.dirty),
        );
        v
    }

    fn tab_count(&self) -> usize {
        self.inactive.len() + 1
    }

    fn take_active(&mut self) -> DocSlot {
        DocSlot {
            doc: std::mem::take(&mut self.doc),
            history: std::mem::take(&mut self.history),
            file_path: self.file_path.take(),
            dirty: std::mem::take(&mut self.dirty),
            viewport: self.canvas.viewport,
        }
    }

    fn install(&mut self, slot: DocSlot) {
        self.doc = slot.doc;
        self.history = slot.history;
        self.file_path = slot.file_path;
        self.dirty = slot.dirty;
        self.canvas.viewport = slot.viewport;
        self.canvas.point_edit = None;
        self.canvas.editing_text = None;
        self.canvas.drag = None;
        self.canvas.polygon = None;
    }

    pub fn switch_tab(&mut self, i: usize) {
        if i == self.active || i >= self.tab_count() {
            return;
        }
        let current = self.take_active();
        // Índice en `inactive` de la pestaña destino (la activa no está en la lista).
        let idx = if i < self.active { i } else { i - 1 };
        let target = self.inactive.remove(idx);
        self.inactive
            .insert(self.active.min(self.inactive.len()), current);
        // Tras reinsertar, la activa pasa a ser `i`.
        self.active = i;
        self.install(target);
    }

    /// Abre un documento en una pestaña nueva y la activa.
    pub fn open_in_new_tab(&mut self, doc: Document, file_path: Option<PathBuf>) {
        let current = self.take_active();
        self.inactive
            .insert(self.active.min(self.inactive.len()), current);
        self.active += 1;
        self.install(DocSlot {
            doc,
            history: History::default(),
            file_path,
            dirty: false,
            viewport: Viewport::default(),
        });
        self.canvas.request_fit();
    }

    pub fn new_tab(&mut self) {
        let n = self.tab_count() + 1;
        let mut doc = Document::default();
        doc.name = format!("Sin título-{n}");
        doc.page = self.prefs.page_setup();
        self.open_in_new_tab(doc, None);
    }

    /// Cierra la pestaña `i` (pide confirmación si tiene cambios). Si es la última, deja un documento vacío.
    pub fn close_tab(&mut self, i: usize) {
        if i >= self.tab_count() {
            return;
        }
        let dirty = if i == self.active {
            self.dirty
        } else {
            self.inactive[if i < self.active { i } else { i - 1 }].dirty
        };
        if dirty {
            let ok = rfd::MessageDialog::new()
                .set_title("Cambios sin guardar")
                .set_description(
                    "Este documento tiene cambios sin guardar. ¿Cerrarlo de todas formas?",
                )
                .set_buttons(rfd::MessageButtons::OkCancel)
                .show()
                == rfd::MessageDialogResult::Ok;
            if !ok {
                return;
            }
        }
        if i != self.active {
            let idx = if i < self.active { i } else { i - 1 };
            self.inactive.remove(idx);
            if i < self.active {
                self.active -= 1;
            }
            return;
        }
        if self.inactive.is_empty() {
            let mut doc = Document::default();
            doc.page = self.prefs.page_setup();
            self.doc = doc;
            self.history = History::default();
            self.file_path = None;
            self.dirty = false;
            self.canvas.request_fit();
            return;
        }
        let idx = self.active.min(self.inactive.len() - 1);
        let next = self.inactive.remove(idx);
        self.active = idx;
        self.install(next);
    }

    /// Familias de fuentes del sistema (se calcula una vez).
    pub fn font_families(&self) -> std::sync::Arc<Vec<String>> {
        self.fonts
            .get_or_init(|| {
                // La fuente de un trazo (incluida) va primero: es la de los rotuladores.
                let mut v = vec![oc_core::singleline::FAMILY.to_string()];
                v.extend(oc_svg::system_font_families());
                std::sync::Arc::new(v)
            })
            .clone()
    }

    /// Guarda el estado en el historial antes de una mutación.
    pub fn commit(&mut self) {
        self.history.push(&self.doc);
        self.dirty = true;
    }

    pub fn undo(&mut self) {
        if self.history.undo(&mut self.doc) {
            self.dirty = true;
        }
    }

    pub fn redo(&mut self) {
        if self.history.redo(&mut self.doc) {
            self.dirty = true;
        }
    }

    pub fn toggle_panel(&mut self, p: Panel) {
        self.panel = if self.panel == Some(p) { None } else { Some(p) };
    }

    pub fn notify(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        self.toast = Some((msg.into(), ctx.input(|i| i.time) + 4.0));
    }

    fn window_title(&self) -> String {
        format!(
            "{}{} — OpenCraftCutter",
            if self.dirty { "• " } else { "" },
            self.doc.name
        )
    }
}

impl eframe::App for OpenCraftCutterApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.prefs.save(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
        // Cierre de la ventana con cambios sin guardar en cualquier pestaña.
        if ctx.input(|i| i.viewport().close_requested()) {
            let any_dirty = self.dirty || self.inactive.iter().any(|t| t.dirty);
            if any_dirty && !self.confirm_discard() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        self.handle_shortcuts(ctx);

        crate::shell::top_bar(self, ui);
        crate::shell::status_bar(self, ui);
        crate::shell::preferences_window(self, ui);
        // Como la referencia: al entrar en Editar puntos se abre su panel.
        if self.tool != self.last_tool {
            if self.tool == Tool::EditPoints {
                self.panel_before_edit = Some(self.panel);
                self.panel = Some(Panel::EditPoints);
                if let Some(id) = self.doc.selected.first().copied() {
                    if self.doc.selected.len() == 1 {
                        crate::canvas::point_edit::begin(self, id);
                    }
                }
            } else if self.last_tool == Tool::EditPoints {
                if self.panel == Some(Panel::EditPoints) {
                    self.panel = self.panel_before_edit.take().flatten();
                }
                self.canvas.point_edit = None;
                self.canvas.point_multi.clear();
            }
            self.last_tool = self.tool;
        }
        let now = ctx.input(|i| i.time);
        self.autosave_tick(now);
        crate::panels::page_guide_window(self, ctx);
        // Depuración: OPENCRAFTCUTTER_DEBUG_OPEN_PATH=archivo abre ese archivo al arrancar.
        if cfg!(debug_assertions) && !self.pixscan_debug_done {
            if let Some(path) = std::env::var_os("OPENCRAFTCUTTER_DEBUG_OPEN_PATH") {
                self.pixscan_debug_done = true;
                self.open_path(ctx, std::path::Path::new(&path));
            }
        }
        // Depuración: OPENCRAFTCUTTER_DEBUG_PIXSCAN=foto.png importa esa foto al arrancar.
        if cfg!(debug_assertions) {
            if let Some(path) = std::env::var_os("OPENCRAFTCUTTER_DEBUG_PIXSCAN") {
                if !self.pixscan_debug_done {
                    self.pixscan_debug_done = true;
                    self.panel = Some(Panel::PixScan);
                    self.pixscan_import_path(ctx, std::path::Path::new(&path));
                }
            }
        }
        crate::panels::pixscan_corners_window(self, ctx);
        crate::insert::window(self, ctx);
        self.handle_file_drop(ctx);
        self.machine_poll(ctx);
        if let Some(m) = self.pending_notice.take() {
            self.notify(ctx, m);
        }
        match self.workspace {
            Workspace::Design => {
                crate::shell::style_bar(self, ui);
                crate::shell::document_tabs(self, ui);
                crate::shell::tool_rail(self, ui);
                crate::shell::panel_rail(self, ui);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(theme::CHROME_700))
                    .show(ui, |ui| {
                        crate::canvas::show(self, ui);
                        crate::panels::show_floating(self, ui);
                    });
            }
            Workspace::Send => {
                crate::shell::send_panel(self, ui);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(theme::CHROME_700))
                    .show(ui, |ui| crate::canvas::show(self, ui));
            }
            Workspace::Library => crate::shell::library_workspace(self, ui),
        }
    }
}

/// Fuente del sistema como respaldo para símbolos de atajos (⌘ ⇧ ⌥) que la fuente de la
/// interfaz no incluye.
fn add_symbol_fallback(ctx: &egui::Context) {
    let candidates = [
        "/System/Library/Fonts/Apple Symbols.ttf",
        "/System/Library/Fonts/LucidaGrande.ttc",
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];
    let Some(bytes) = candidates.iter().find_map(|p| std::fs::read(p).ok()) else {
        return;
    };
    let families = [egui::FontFamily::Proportional, egui::FontFamily::Monospace]
        .into_iter()
        .map(|family| egui::epaint::text::InsertFontFamily {
            family,
            priority: egui::epaint::text::FontPriority::Lowest,
        })
        .collect();
    ctx.add_font(egui::epaint::text::FontInsert::new(
        "symbols-fallback",
        egui::FontData::from_owned(bytes),
        families,
    ));
}
