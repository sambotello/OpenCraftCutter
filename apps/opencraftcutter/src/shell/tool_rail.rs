use crate::app::{OpenCraftCutterApp, Tool};
use crate::shell::icons;
use crate::theme;
use egui::Frame;

pub const TOOLS: [(Tool, egui_material_icons::MaterialIcon); 11] = [
    (Tool::Select, icons::SELECT),
    (Tool::EditPoints, icons::EDIT_POINTS),
    (Tool::Line, icons::LINE),
    (Tool::Rect, icons::RECT),
    (Tool::Ellipse, icons::ELLIPSE),
    (Tool::Polygon, icons::POLYGON),
    (Tool::Freehand, icons::FREEHAND),
    (Tool::Text, icons::TEXT),
    (Tool::Note, icons::NOTE),
    (Tool::Eraser, icons::ERASER),
    (Tool::Knife, icons::KNIFE),
];

pub fn tool_rail(app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    egui::Panel::left("tool_rail")
        .exact_size(40.0)
        .resizable(false)
        .frame(
            Frame::NONE
                .fill(theme::CHROME_900)
                .inner_margin(egui::Margin::symmetric(4, 6)),
        )
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (tool, icon) in TOOLS {
                if icons::rail_button(ui, icon, tool.label(), app.tool == tool) {
                    app.tool = tool;
                }
            }
        });
}
