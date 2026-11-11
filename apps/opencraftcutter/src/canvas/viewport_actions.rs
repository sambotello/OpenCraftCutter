use super::Viewport;
use crate::app::OpenCraftCutterApp;
use kurbo::Rect;
use oc_core::edit::selection_bounds;

/// Rectángulo (mm) que engloba página y tapete para "ajustar a página".
pub fn page_frame(app: &OpenCraftCutterApp) -> Rect {
    let p = &app.doc.page;
    Rect::new(-14.0, -32.0, p.width_mm + 14.0, p.height_mm + 14.0)
}

pub fn fit_to_page(app: &mut OpenCraftCutterApp) {
    let frame = page_frame(app);
    app.canvas.viewport =
        Viewport::fit_rotated(frame, app.canvas.stage, 24.0, app.doc.page.preview_rotation);
}

pub fn zoom_to_selection(app: &mut OpenCraftCutterApp) {
    if let Some(b) = selection_bounds(app.doc.selected_shapes()) {
        app.canvas.viewport =
            Viewport::fit_rotated(b, app.canvas.stage, 80.0, app.doc.page.preview_rotation);
    }
}

pub fn zoom_by(app: &mut OpenCraftCutterApp, factor: f64) {
    let c = app.canvas.stage.center();
    app.canvas.viewport.zoom_at(factor, c);
}

pub fn zoom_to(app: &mut OpenCraftCutterApp, scale: f64) {
    let f = scale / app.canvas.viewport.scale;
    zoom_by(app, f);
}
