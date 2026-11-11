//! Iconos (Material Symbols) y botón de riel.

use crate::app::Panel;
use crate::theme;
use egui::{Color32, Ui};
use egui_material_icons::{MaterialIcon, icons as mi};

pub const NEW: MaterialIcon = mi::ICON_NOTE_ADD;
pub const OPEN: MaterialIcon = mi::ICON_FOLDER_OPEN;
pub const SAVE: MaterialIcon = mi::ICON_SAVE;
pub const IMPORT: MaterialIcon = mi::ICON_DOWNLOAD;
pub const EXPORT: MaterialIcon = mi::ICON_UPLOAD;
pub const DUPLICATE: MaterialIcon = mi::ICON_CONTENT_COPY;
pub const DELETE: MaterialIcon = mi::ICON_DELETE;
pub const UNDO: MaterialIcon = mi::ICON_UNDO;
pub const REDO: MaterialIcon = mi::ICON_REDO;
pub const ZOOM_IN: MaterialIcon = mi::ICON_ZOOM_IN;
pub const ZOOM_OUT: MaterialIcon = mi::ICON_ZOOM_OUT;
pub const ZOOM_SELECTION: MaterialIcon = mi::ICON_CENTER_FOCUS_STRONG;
pub const ZOOM_DRAG: MaterialIcon = mi::ICON_CROP_FREE;
pub const PAN: MaterialIcon = mi::ICON_PAN_TOOL;
pub const FIT: MaterialIcon = mi::ICON_FIT_SCREEN;

pub const SELECT: MaterialIcon = mi::ICON_ARROW_SELECTOR_TOOL;
pub const EDIT_POINTS: MaterialIcon = mi::ICON_POLYLINE;
pub const LINE: MaterialIcon = mi::ICON_HORIZONTAL_RULE;
pub const RECT: MaterialIcon = mi::ICON_CROP_SQUARE;
pub const ELLIPSE: MaterialIcon = mi::ICON_RADIO_BUTTON_UNCHECKED;
pub const POLYGON: MaterialIcon = mi::ICON_PENTAGON;
pub const FREEHAND: MaterialIcon = mi::ICON_DRAW;
pub const TEXT: MaterialIcon = mi::ICON_TITLE;
pub const NOTE: MaterialIcon = mi::ICON_STICKY_NOTE_2;
pub const ERASER: MaterialIcon = mi::ICON_INK_ERASER;
pub const KNIFE: MaterialIcon = mi::ICON_CONTENT_CUT;

pub fn panel_icon(p: Panel) -> MaterialIcon {
    match p {
        Panel::PageSetup => mi::ICON_DESCRIPTION,
        Panel::Transform => mi::ICON_ALIGN_HORIZONTAL_CENTER,
        Panel::Fill => mi::ICON_FORMAT_COLOR_FILL,
        Panel::Line => mi::ICON_LINE_WEIGHT,
        Panel::Text => mi::ICON_TEXT_FIELDS,
        Panel::PrintCut => mi::ICON_CROP_FREE,
        Panel::Layers => mi::ICON_LAYERS,
        Panel::Modify => mi::ICON_JOIN_FULL,
        Panel::Offset => mi::ICON_ADJUST,
        Panel::Trace => mi::ICON_AUTO_FIX_HIGH,
        Panel::Replicate => mi::ICON_GRID_VIEW,
        Panel::PixScan => mi::ICON_PHOTO_CAMERA,
        Panel::SpecialTools => mi::ICON_HANDYMAN,
        Panel::EditPoints => mi::ICON_POLYLINE,
        Panel::Rhinestone => mi::ICON_DIAMOND,
        Panel::Sketch => mi::ICON_GESTURE,
        Panel::Warp => mi::ICON_WAVES,
        Panel::Emboss => mi::ICON_TEXTURE,
        Panel::Weeding => mi::ICON_CROP_SQUARE,
    }
}

/// Botón cuadrado de icono para los rieles laterales.
pub fn rail_button(ui: &mut Ui, icon: MaterialIcon, tooltip: &str, active: bool) -> bool {
    let text = icon.rich_text().size(18.0).color(if active {
        theme::CHROME_950
    } else {
        theme::CHROME_200
    });
    let btn = egui::Button::new(text)
        .min_size(egui::vec2(32.0, 32.0))
        .fill(if active {
            theme::ACCENT
        } else {
            Color32::TRANSPARENT
        })
        .stroke(egui::Stroke::NONE);
    ui.add(btn).on_hover_text(tooltip).clicked()
}

/// Botón de icono para la barra superior.
pub fn bar_button(
    ui: &mut Ui,
    icon: MaterialIcon,
    tooltip: &str,
    enabled: bool,
    active: bool,
) -> bool {
    let text = icon.rich_text().size(17.0).color(if active {
        theme::CHROME_950
    } else {
        theme::CHROME_200
    });
    let btn = egui::Button::new(text)
        .min_size(egui::vec2(30.0, 28.0))
        .fill(if active {
            theme::ACCENT
        } else {
            Color32::TRANSPARENT
        })
        .stroke(egui::Stroke::NONE);
    ui.add_enabled(enabled, btn)
        .on_hover_text(tooltip)
        .clicked()
}
