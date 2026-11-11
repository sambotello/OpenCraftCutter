//! Atajos de teclado globales. Se ignoran mientras un campo de texto tiene el foco.

use crate::app::{OpenCraftCutterApp, Tool};
use crate::canvas::viewport_actions;
use egui::{Key, Modifiers};

impl OpenCraftCutterApp {
    pub fn delete_selected(&mut self) {
        let ids: Vec<_> = self
            .doc
            .selected_shapes()
            .iter()
            .filter(|s| !s.locked)
            .map(|s| s.id)
            .collect();
        if ids.is_empty() {
            return;
        }
        self.commit();
        self.doc.remove_shapes(&ids);
    }

    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.canvas.editing_text.is_some() {
            return;
        }
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let consume = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));

        // Portapapeles: en macOS egui entrega ⌘C/⌘X/⌘V como eventos, no como teclas.
        let clip: Vec<egui::Event> = ctx.input(|i| {
            i.events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)
                    )
                })
                .cloned()
                .collect()
        });
        for e in clip {
            match e {
                egui::Event::Copy => self.copy_selection(ctx),
                egui::Event::Cut => self.cut_selection(ctx),
                egui::Event::Paste(text) => self.paste_text(ctx, &text),
                _ => {}
            }
        }
        if consume(cmd, Key::K) {
            self.show_prefs = !self.show_prefs;
            return;
        }

        if consume(cmd, Key::Comma) {
            self.show_prefs = !self.show_prefs;
            return;
        }
        // Archivo
        // Menú Archivo (atajos de la referencia). Con Mayús primero.
        if consume(cmd_shift, Key::N) {
            return self.new_with_wizard();
        }
        if consume(cmd_shift, Key::O) {
            return self.merge_document(ctx);
        }
        if consume(
            Modifiers::COMMAND | Modifiers::ALT | Modifiers::CTRL,
            Key::R,
        ) {
            return self.save_to_library(ctx, false);
        }
        if consume(cmd, Key::N) {
            return self.new_document(ctx);
        }
        if consume(cmd, Key::O) {
            return self.open_document(ctx);
        }
        if consume(cmd_shift, Key::S) {
            self.save_document(ctx, true);
            return;
        }
        if consume(cmd, Key::S) {
            self.save_document(ctx, false);
            return;
        }
        if consume(cmd_shift, Key::I) {
            return self.import_svg(ctx);
        }
        // Menú Ver (atajos de la referencia).
        let cmd_alt = Modifiers::COMMAND | Modifiers::ALT;
        if consume(cmd_alt, Key::D) {
            self.workspace = crate::app::Workspace::Design;
            return;
        }
        if consume(cmd_alt, Key::L) {
            self.workspace = crate::app::Workspace::Library;
            return;
        }
        if consume(cmd_shift, Key::P) {
            self.doc.page.show_print_margins = !self.doc.page.show_print_margins;
            self.dirty = true;
            return;
        }
        if consume(cmd_shift, Key::L) {
            self.doc.page.show_cut_area = !self.doc.page.show_cut_area;
            self.dirty = true;
            return;
        }
        if consume(cmd, Key::Num1) {
            return crate::canvas::viewport_actions::fit_to_page(self);
        }
        // Exportar SVG pasa a ⌥⌘E: ⇧⌘E es «Soltar trazado compuesto», como la referencia.
        if consume(Modifiers::COMMAND | Modifiers::ALT, Key::E) {
            return self.export_svg(ctx);
        }
        // Menú Objeto (atajos de la referencia).
        {
            use crate::object_menu::Cmd;
            let pairs = [
                // Primero los que llevan Mayús (egui ignora Mayús si el atajo no la incluye).
                (cmd_shift, Key::G, Cmd::Ungroup),
                (cmd_shift, Key::E, Cmd::ReleaseCompound),
                (cmd_shift, Key::W, Cmd::Bool(oc_core::ops::BoolOp::Weld)),
                (cmd_shift, Key::F, Cmd::FillPage),
                (cmd, Key::G, Cmd::Group),
                (cmd, Key::E, Cmd::MakeCompound),
                (cmd, Key::J, Cmd::Simplify),
            ];
            for (m, k, c) in pairs {
                if consume(m, k) {
                    return self.object_run(c);
                }
            }
        }
        if consume(cmd_shift, Key::M) {
            return self.import_image(ctx);
        }
        // Sin Mayús, después de ⇧⌘W y ⇧⌘P.
        if consume(cmd, Key::F) {
            return self.paste(true);
        }
        if consume(cmd, Key::W) {
            return self.close_active_tab();
        }
        if consume(cmd, Key::P) {
            return self.print_page(ctx);
        }
        // Edición
        if consume(cmd_shift, Key::Z) || consume(cmd, Key::Y) {
            return self.redo();
        }
        if consume(cmd, Key::Z) {
            return self.undo();
        }
        if consume(cmd_shift, Key::A) {
            self.doc.selected.clear();
            return;
        }
        if consume(cmd, Key::A) {
            self.doc.select_all();
            return;
        }
        if consume(cmd, Key::D) {
            if !self.doc.selected.is_empty() {
                self.commit();
                self.doc.duplicate_selected();
            }
            return;
        }
        if consume(Modifiers::NONE, Key::Delete) || consume(Modifiers::NONE, Key::Backspace) {
            // En Editar puntos, Supr solo borra nodos (nunca la forma entera).
            if self.tool == Tool::EditPoints && self.canvas.point_edit.is_some() {
                self.delete_selected_vertex();
                return;
            }
            return self.delete_selected();
        }
        // Orden
        if consume(cmd_shift, Key::CloseBracket) {
            self.commit();
            self.doc.reorder_selected(i32::MAX);
            return;
        }
        if consume(cmd_shift, Key::OpenBracket) {
            self.commit();
            self.doc.reorder_selected(i32::MIN);
            return;
        }
        if consume(cmd, Key::CloseBracket) {
            self.commit();
            self.doc.reorder_selected(1);
            return;
        }
        if consume(cmd, Key::OpenBracket) {
            self.commit();
            self.doc.reorder_selected(-1);
            return;
        }
        // Vista
        if consume(cmd, Key::Plus) || consume(cmd, Key::Equals) {
            return viewport_actions::zoom_by(self, 1.25);
        }
        if consume(cmd, Key::Minus) {
            return viewport_actions::zoom_by(self, 1.0 / 1.25);
        }
        if consume(cmd, Key::Num0) {
            return viewport_actions::fit_to_page(self);
        }
        if consume(cmd, Key::Num2) {
            return viewport_actions::zoom_to_selection(self);
        }
        // Flechas: mover selección 1 mm (10 con Shift)
        let (shift, arrows) = ctx.input(|i| {
            let a = [
                Key::ArrowLeft,
                Key::ArrowRight,
                Key::ArrowUp,
                Key::ArrowDown,
            ]
            .map(|k| i.key_pressed(k));
            (i.modifiers.shift, a)
        });
        if arrows.iter().any(|a| *a) && !self.doc.selected.is_empty() {
            let step = if shift { 10.0 } else { 1.0 };
            let dx = if arrows[0] {
                -step
            } else if arrows[1] {
                step
            } else {
                0.0
            };
            let dy = if arrows[2] {
                -step
            } else if arrows[3] {
                step
            } else {
                0.0
            };
            self.commit();
            let ids = self.doc.selected.clone();
            for id in ids {
                if let Some(s) = self.doc.shape_mut(id) {
                    if !s.locked {
                        s.rect = s.rect + kurbo::Vec2::new(dx, dy);
                    }
                }
            }
            return;
        }
        if consume(Modifiers::NONE, Key::Escape) {
            if self.canvas.punch_draw.is_some() {
                self.canvas.punch_draw = None;
            } else if self.canvas.polygon.is_some() {
                self.canvas.polygon = None;
            } else if self.canvas.point_edit.is_some() {
                self.canvas.point_edit = None;
            } else {
                self.doc.selected.clear();
            }
            return;
        }
        // Herramientas
        let tools = [
            (Key::V, Tool::Select),
            (Key::A, Tool::EditPoints),
            (Key::L, Tool::Line),
            (Key::R, Tool::Rect),
            (Key::E, Tool::Ellipse),
            (Key::G, Tool::Polygon),
            (Key::P, Tool::Freehand),
            (Key::T, Tool::Text),
            (Key::H, Tool::Pan),
            (Key::Z, Tool::ZoomDrag),
        ];
        for (k, t) in tools {
            if consume(Modifiers::NONE, k) {
                self.tool = t;
                return;
            }
        }
    }
}
