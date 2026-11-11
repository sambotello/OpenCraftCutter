//! Lienzo de diseño: vista (paneo/zoom), dibujo de la escena y herramientas de edición.

mod overlay;
pub mod point_edit;
pub mod render;
mod send_overlay;
mod text_edit;
mod tools;
mod viewport;
pub mod viewport_actions;

use crate::app::OpenCraftCutterApp;
use crate::shell::SendSettings;
use egui::{Rect as ERect, Ui};
use kurbo::{Point, Rect};
use oc_core::edit::Handle;
use oc_core::shape::{Shape, ShapeId};
pub use viewport::Viewport;

pub const RULER_PX: f32 = 18.0;

/// Gesto en curso con el puntero.
#[derive(Debug, Clone)]
pub enum Drag {
    Pan,
    Move {
        start: Point,
        originals: Vec<Shape>,
        committed: bool,
    },
    Marquee {
        start: Point,
        additive: bool,
    },
    Scale {
        handle: Handle,
        /// Distancia (mm) del puntero al punto real del asa sobre el objeto al empezar.
        grab: kurbo::Vec2,
        originals: Vec<Shape>,
        group: Option<Rect>,
        committed: bool,
    },
    Rotate {
        originals: Vec<Shape>,
        center: Point,
        start_angle: f64,
        committed: bool,
    },
    Create {
        id: ShapeId,
        start: Point,
        line: bool,
    },
    Freehand {
        id: ShapeId,
        points: Vec<Point>,
    },
    ZoomBox {
        start: Point,
    },
    /// Cuchilla: línea recta de corte.
    Knife {
        start: Point,
        end: Point,
    },
    /// Borrador: recorrido de la pincelada.
    Erase {
        path: Vec<Point>,
    },
    /// Seleccionar el área de trazado sobre una imagen.
    TraceArea {
        image: ShapeId,
        start: Point,
    },
    /// Recuadro para seleccionar nodos en Editar puntos.
    NodeMarquee {
        start: Point,
        additive: bool,
    },
    /// Mover un manejador Bézier de un nodo.
    MoveHandle {
        shape: ShapeId,
        sub: usize,
        idx: usize,
        out: bool,
        committed: bool,
    },
    /// Mover un vértice de un trazo.
    MovePoint {
        shape: ShapeId,
        sub: usize,
        idx: usize,
        committed: bool,
    },
}

/// A qué se aplica el color que toma el cuentagotas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EyedropperTarget {
    Fill,
    Line,
}

/// Trazo en edición de puntos y vértice seleccionado (subtrazo, índice).
#[derive(Debug, Clone, Copy)]
pub struct PointEdit {
    pub shape: ShapeId,
    pub selected: Option<(usize, usize)>,
}

pub struct CanvasState {
    pub viewport: Viewport,
    pub drag: Option<Drag>,
    pub marquee: Option<Rect>,
    pub polygon: Option<Vec<Point>>,
    pub cursor_mm: Option<Point>,
    pub editing_text: Option<ShapeId>,
    pub send: SendSettings,
    /// Rectángulo en pantalla del área de dibujo (sin reglas) del último frame.
    pub stage: ERect,
    fit_requested: bool,
    pub space_held: bool,
    /// Texturas de las imágenes del documento, por id de forma.
    pub textures: std::collections::HashMap<ShapeId, egui::TextureHandle>,
    pub text_cache: crate::text::TextCache,
    pub point_edit: Option<PointEdit>,
    /// Nodos seleccionados además del principal (selección múltiple de Editar puntos).
    pub point_multi: std::collections::BTreeSet<(usize, usize)>,
    /// Cuentagotas activo: el próximo clic en el lienzo toma el color de la forma bajo el puntero.
    pub eyedropper: Option<EyedropperTarget>,
    /// Esperando a que el usuario arrastre el área de trazado sobre esta imagen.
    pub trace_area_select: Option<ShapeId>,
    /// Área de trazado elegida (imagen, rectángulo en mm del mundo).
    pub trace_area: Option<(ShapeId, Rect)>,
    /// Vista previa del umbral: (huella de ajustes, imagen, textura).
    pub trace_preview: Option<(u64, ShapeId, egui::TextureHandle)>,
    /// Vista previa del desplazamiento (formas aún no añadidas al documento).
    pub offset_preview: Vec<oc_core::Shape>,
    /// Líneas de los rellenos de herramientas especiales por forma (huella, líneas).
    pub tool_fill_cache: std::cell::RefCell<
        std::collections::HashMap<ShapeId, (u64, std::sync::Arc<oc_core::ops::Lines>)>,
    >,
    /// Modo «Dibujar puntos de perforación» activo (diámetro en mm).
    pub punch_draw: Option<f64>,
    /// Contornos de trazos gruesos ya calculados.
    pub stroke_cache: render::StrokeCache,
    /// Logo del tapete (textura y proporción ancho/alto).
    pub logo: Option<(egui::TextureHandle, f32)>,
    /// Grosor del borrador en mm.
    pub eraser_width_mm: f64,
    /// Contornos de texto del frame actual (mm, sin rotar), por id.
    pub text_outlines: std::collections::HashMap<ShapeId, crate::text::Outlines>,
}

impl Default for CanvasState {
    fn default() -> Self {
        CanvasState {
            viewport: Viewport::default(),
            drag: None,
            marquee: None,
            polygon: None,
            cursor_mm: None,
            editing_text: None,
            send: SendSettings::default(),
            stage: ERect::NOTHING,
            fit_requested: true,
            space_held: false,
            textures: std::collections::HashMap::new(),
            text_cache: crate::text::TextCache::default(),
            point_edit: None,
            point_multi: Default::default(),
            eyedropper: None,
            trace_area_select: None,
            trace_area: None,
            trace_preview: None,
            offset_preview: Vec::new(),
            tool_fill_cache: Default::default(),
            punch_draw: None,
            stroke_cache: Default::default(),
            logo: None,
            eraser_width_mm: 3.0,
            text_outlines: std::collections::HashMap::new(),
        }
    }
}

impl CanvasState {
    pub fn request_fit(&mut self) {
        self.fit_requested = true;
    }
}

/// Dibuja el lienzo completo (reglas + escena) e interpreta la entrada del usuario.
pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let full = ui.available_rect_before_wrap();
    let stage = ERect::from_min_max(full.min + egui::vec2(RULER_PX, RULER_PX), full.max);
    app.canvas.stage = stage;
    if app.canvas.viewport.rotation != app.doc.page.preview_rotation {
        app.canvas.viewport.rotation = app.doc.page.preview_rotation;
        app.canvas.fit_requested = true;
    }

    if app.canvas.fit_requested && stage.width() > 50.0 && stage.height() > 50.0 {
        app.canvas.fit_requested = false;
        viewport_actions::fit_to_page(app);
    }

    let response = ui.allocate_rect(stage, egui::Sense::click_and_drag());
    tools::handle_input(app, ui, &response);

    render::ensure_textures(app, ui.ctx());
    let painter = ui.painter_at(stage);
    render::draw_scene(app, &painter);
    let sending = app.workspace == crate::app::Workspace::Send;
    if sending {
        send_overlay::draw(app, &painter, stage);
    }
    overlay::draw_overlay(app, &painter);
    render::draw_rulers(app, &ui.painter_at(full), full);
    text_edit::show_text_editor(app, ui);
    if sending {
        send_overlay::assign_buttons(app, ui, stage);
    }
}
