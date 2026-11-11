//! Núcleo de OpenCraftCutter: modelo del documento, geometría de edición, unidades y máquinas.
//!
//! Este crate no sabe nada de la interfaz: es puro dato y funciones, testeable sin ventana.
//! Todas las longitudes del documento se expresan en **milímetros**.

pub mod arrange;
pub mod color;
pub mod details;
pub mod document;
pub mod edit;
pub mod history;
pub mod machines;
pub mod nodes;
pub mod ops;
pub mod qr;
pub mod regmarks;
pub mod replicate;
pub mod send;
pub mod shape;
pub mod singleline;
pub mod special;
pub mod templates;
pub mod textpath;
pub mod tiles;
pub mod units;
pub mod usage;
pub mod weeding;

pub use color::Color;
pub use document::{Document, PageSetup};
pub use history::History;
pub use kurbo::{Affine, Point, Rect, Vec2};
pub use shape::{FillRule, LineStyle, Shape, ShapeId, ShapeKind, Stroke, Subpath, TextAlign};
