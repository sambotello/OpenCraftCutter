use crate::app::{OpenCraftCutterApp, Panel};
use crate::shell::icons;
use crate::theme;
use egui::Frame;

pub fn panel_rail(app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    egui::Panel::right("panel_rail")
        .exact_size(40.0)
        .resizable(false)
        .frame(
            Frame::NONE
                .fill(theme::CHROME_900)
                .inner_margin(egui::Margin::symmetric(4, 6)),
        )
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for p in Panel::ALL {
                    if icons::rail_button(ui, icons::panel_icon(p), p.title(), app.panel == Some(p))
                    {
                        app.toggle_panel(p);
                    }
                }
            });
        });
}
