//! Marco de la ventana: barra superior, barra de estilo, rieles, barra de estado y secciones.

mod icons;
mod library;
mod panel_rail;
mod prefs_window;
pub mod send;
mod shortcuts;
mod status_bar;
mod style_bar;
mod tabs;
mod tool_rail;
mod top_bar;

pub use icons::panel_icon;
pub use library::{LibraryState, library_workspace};
pub use panel_rail::panel_rail;
pub use prefs_window::preferences_window;
pub use send::SendSettings;
pub use send::send_panel;
pub use status_bar::status_bar;
pub use style_bar::style_bar;
pub use tabs::document_tabs;
pub use tool_rail::tool_rail;
pub use top_bar::top_bar;
