//! Edición en línea de textos: un `TextEdit` superpuesto sobre la forma, con su fuente y tamaño.

use crate::app::OpenCraftCutterApp;
use crate::text::{TextStyle, measure};
use egui::{Color32, FontId, Key, Ui};
use oc_core::shape::ShapeKind;
use oc_core::units::pt_to_mm;

pub fn show_text_editor(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let Some(id) = app.canvas.editing_text else {
        return;
    };
    let vp = app.canvas.viewport;
    let Some(shape) = app.doc.shape(id).cloned() else {
        app.canvas.editing_text = None;
        return;
    };
    let ShapeKind::Text {
        text, font_size_pt, ..
    } = &shape.kind
    else {
        app.canvas.editing_text = None;
        return;
    };
    let size_px = (pt_to_mm(*font_size_pt) * vp.scale) as f32;
    let screen = vp.rect_to_screen(shape.rect);
    let width = (screen.width() + size_px).max(40.0);
    let mut buffer = text.clone();
    let ctx = ui.ctx().clone();
    let edit_id = egui::Id::unique(("text_editor", id));
    let color = shape
        .fill
        .map(|c| Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a))
        .unwrap_or(Color32::BLACK);

    let response = egui::Area::new(edit_id)
        .fixed_pos(screen.min)
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            ui.set_width(width);
            ui.add(
                egui::TextEdit::multiline(&mut buffer)
                    .id(edit_id.with("edit"))
                    .font(FontId::proportional(size_px.max(4.0)))
                    .text_color(color)
                    .frame(egui::Frame::NONE)
                    .desired_width(width)
                    .desired_rows(1)
                    .margin(egui::Margin::ZERO),
            )
        })
        .inner;

    if !response.has_focus() && !response.lost_focus() {
        response.request_focus();
    }

    if buffer != *text {
        if let Some(s) = app.doc.shape_mut(id) {
            if let ShapeKind::Text { text, .. } = &mut s.kind {
                *text = buffer.clone();
            }
            if let Some(st) = TextStyle::from_kind(&s.kind) {
                let (w, h) = measure(&st);
                s.rect = kurbo::Rect::new(s.rect.x0, s.rect.y0, s.rect.x0 + w, s.rect.y0 + h);
            }
        }
        app.dirty = true;
    }

    let escape = ctx.input(|i| i.key_pressed(Key::Escape));
    let clicked_outside = ctx.input(|i| i.pointer.any_pressed()) && !response.hovered();
    if escape || response.lost_focus() || clicked_outside {
        if app.doc.shape(id).is_some_and(
            |s| matches!(&s.kind, ShapeKind::Text { text, .. } if text.trim().is_empty()),
        ) {
            app.doc.remove_shapes(&[id]);
        }
        app.canvas.editing_text = None;
    }
}
