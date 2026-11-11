//! Documento: página, formas y selección. Serializable como `.opencraftcutter` (JSON).

use crate::machines::{FeedType, MachineId, MatId};
use crate::shape::{Shape, ShapeId};
use crate::units::Unit;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const DOCUMENT_FORMAT: &str = "opencraftcutter";
pub const DOCUMENT_VERSION: u32 = 2;
pub const DOCUMENT_EXTENSION: &str = "opencraftcutter";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GridStyle {
    #[default]
    Square,
    Isometric,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageSetup {
    pub machine: MachineId,
    pub feed: FeedType,
    pub mat: MatId,
    pub media_size: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub orientation: Orientation,
    pub restrict_to_mat: bool,
    pub show_grid: bool,
    pub grid_spacing_mm: f64,
    pub grid_divisions: u32,
    pub snap_to_grid: bool,
    pub unit: Unit,
    pub cut_margin_mm: f64,
    pub show_cut_area: bool,
    /// Color de la página (fondo de la hoja).
    pub page_color: crate::Color,
    /// Transparencia de la página, 0..1 (0 = opaca).
    pub page_transparency: f64,
    /// Rotación de la previsualización en grados: 0, 90, 180 o 270.
    pub preview_rotation: u16,
    /// Mostrar los bordes de impresión (margen de impresora).
    pub show_print_margins: bool,
    /// Ajuste inteligente: alinear al mover con bordes y centros de página y objetos.
    pub smart_snap: bool,
    /// Ajuste inteligente solo con la página (no con otros objetos).
    pub smart_snap_within_media: bool,
    /// Tolerancia del ajuste inteligente en mm.
    pub snap_tolerance_mm: f64,
    pub grid_style: GridStyle,
    pub grid_primary_color: crate::Color,
    pub grid_secondary_color: crate::Color,
    /// Marcas de registro de Imprimir y cortar.
    pub reg_marks: crate::regmarks::RegMarks,
}

impl Default for PageSetup {
    fn default() -> Self {
        PageSetup {
            machine: MachineId::Cameo5,
            feed: FeedType::Auto,
            mat: MatId::Auto,
            media_size: "a4".into(),
            width_mm: 210.0,
            height_mm: 297.0,
            orientation: Orientation::Portrait,
            restrict_to_mat: false,
            show_grid: true,
            grid_spacing_mm: 10.0,
            grid_divisions: 2,
            snap_to_grid: false,
            unit: Unit::Cm,
            cut_margin_mm: 3.0,
            show_cut_area: true,
            page_color: crate::Color::WHITE,
            page_transparency: 0.0,
            preview_rotation: 0,
            show_print_margins: true,
            smart_snap: true,
            smart_snap_within_media: false,
            snap_tolerance_mm: 1.5,
            grid_style: GridStyle::Square,
            grid_primary_color: crate::Color::rgb(0xd8, 0xd8, 0xd8),
            grid_secondary_color: crate::Color::rgb(0xec, 0xec, 0xec),
            reg_marks: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    pub page: PageSetup,
    pub shapes: Vec<Shape>,
    #[serde(default, skip_serializing)]
    pub selected: Vec<ShapeId>,
    #[serde(default)]
    next_id: ShapeId,
}

impl Default for Document {
    fn default() -> Self {
        Document {
            name: "Sin título-1".into(),
            page: PageSetup::default(),
            shapes: Vec::new(),
            selected: Vec::new(),
            next_id: 1,
        }
    }
}

/// Envoltorio del archivo en disco.
#[derive(Debug, Serialize, Deserialize)]
struct DocumentFile {
    format: String,
    version: u32,
    #[serde(flatten)]
    document: Document,
}

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("El archivo no es un documento de OpenCraftCutter válido: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("El archivo no es un documento de OpenCraftCutter.")]
    WrongFormat,
    #[error("Versión de documento no compatible ({0}).")]
    Version(u32),
}

impl Document {
    pub fn next_id(&mut self) -> ShapeId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn shape(&self, id: ShapeId) -> Option<&Shape> {
        self.shapes.iter().find(|s| s.id == id)
    }

    pub fn shape_mut(&mut self, id: ShapeId) -> Option<&mut Shape> {
        self.shapes.iter_mut().find(|s| s.id == id)
    }

    pub fn is_selected(&self, id: ShapeId) -> bool {
        self.selected.contains(&id)
    }

    /// Formas seleccionadas en orden de pila.
    pub fn selected_shapes(&self) -> Vec<&Shape> {
        let set: HashSet<ShapeId> = self.selected.iter().copied().collect();
        self.shapes.iter().filter(|s| set.contains(&s.id)).collect()
    }

    pub fn select_all(&mut self) {
        self.selected = self
            .shapes
            .iter()
            .filter(|s| s.visible)
            .map(|s| s.id)
            .collect();
    }

    pub fn toggle_selected(&mut self, id: ShapeId) {
        if let Some(i) = self.selected.iter().position(|x| *x == id) {
            self.selected.remove(i);
        } else {
            self.selected.push(id);
        }
    }

    /// Añade una forma (asignándole id si es 0) y la devuelve.
    pub fn add_shape(&mut self, mut shape: Shape) -> ShapeId {
        if shape.id == 0 {
            shape.id = self.next_id();
        } else {
            self.next_id = self.next_id.max(shape.id + 1);
        }
        let id = shape.id;
        self.shapes.push(shape);
        id
    }

    pub fn remove_shapes(&mut self, ids: &[ShapeId]) {
        let set: HashSet<ShapeId> = ids.iter().copied().collect();
        self.shapes.retain(|s| !set.contains(&s.id));
        self.selected.retain(|id| !set.contains(id));
    }

    /// Duplica la selección desplazada 5 mm y selecciona las copias.
    pub fn duplicate_selected(&mut self) {
        let copies: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        // Las copias forman grupos nuevos (con su misma jerarquía).
        let mut ids = std::collections::HashMap::new();
        let copies: Vec<Shape> = copies
            .into_iter()
            .map(|mut c| {
                c.id = 0;
                c.rect = c.rect + kurbo::Vec2::new(5.0, 5.0);
                c.remap_groups(|g| *ids.entry(g).or_insert_with(|| self.next_id()));
                c
            })
            .collect();
        self.selected = copies.into_iter().map(|c| self.add_shape(c)).collect();
    }

    /// Reordena la selección en la pila: +1 adelante, -1 atrás, `i32::MAX` al frente, `i32::MIN` al fondo.
    pub fn reorder_selected(&mut self, delta: i32) {
        let set: HashSet<ShapeId> = self.selected.iter().copied().collect();
        if set.is_empty() {
            return;
        }
        if delta == i32::MAX || delta == i32::MIN {
            let (sel, rest): (Vec<Shape>, Vec<Shape>) =
                self.shapes.drain(..).partition(|s| set.contains(&s.id));
            self.shapes = if delta == i32::MAX {
                [rest, sel].concat()
            } else {
                [sel, rest].concat()
            };
            return;
        }
        let n = self.shapes.len();
        let indices: Vec<usize> = (0..n)
            .filter(|i| set.contains(&self.shapes[*i].id))
            .collect();
        let order: Vec<usize> = if delta > 0 {
            indices.into_iter().rev().collect()
        } else {
            indices
        };
        for i in order {
            let j = i as i64 + delta as i64;
            if j < 0 || j >= n as i64 || set.contains(&self.shapes[j as usize].id) {
                continue;
            }
            self.shapes.swap(i, j as usize);
        }
    }

    /// Agrupa la selección (2 objetos o más). Los grupos anteriores se funden en el nuevo.
    pub fn group_selected(&mut self) -> bool {
        if self.selected.len() < 2 {
            return false;
        }
        let gid = self.next_id();
        let set: HashSet<ShapeId> = self.selected.iter().copied().collect();
        for s in self.shapes.iter_mut().filter(|s| set.contains(&s.id)) {
            // El grupo que ya tenía queda dentro del nuevo.
            if let Some(g) = s.group.replace(gid) {
                s.inner_groups.push(g);
            }
        }
        true
    }

    /// Desagrupa la selección.
    pub fn ungroup_selected(&mut self) -> bool {
        let set: HashSet<ShapeId> = self.selected.iter().copied().collect();
        let mut any = false;
        for s in self.shapes.iter_mut().filter(|s| set.contains(&s.id)) {
            // Se quita un nivel: el grupo interior inmediato pasa a ser el exterior.
            any |= s.group.is_some();
            s.group = s.inner_groups.pop();
        }
        any
    }

    /// ¿Hay algún grupo en la selección?
    pub fn selection_has_group(&self) -> bool {
        self.selected_shapes().iter().any(|s| s.group.is_some())
    }

    /// Ids de `id` y de todo su grupo (si lo tiene).
    pub fn with_group(&self, id: ShapeId) -> Vec<ShapeId> {
        match self.shape(id).and_then(|s| s.group) {
            Some(g) => self
                .shapes
                .iter()
                .filter(|s| s.group == Some(g))
                .map(|s| s.id)
                .collect(),
            None => vec![id],
        }
    }

    pub fn to_json(&self) -> String {
        let file = DocumentFile {
            format: DOCUMENT_FORMAT.into(),
            version: DOCUMENT_VERSION,
            document: self.clone(),
        };
        serde_json::to_string_pretty(&file).expect("el documento siempre es serializable")
    }

    pub fn from_json(text: &str) -> Result<Document, DocumentError> {
        let file: DocumentFile = serde_json::from_str(text)?;
        if file.format != DOCUMENT_FORMAT {
            return Err(DocumentError::WrongFormat);
        }
        if file.version != DOCUMENT_VERSION {
            return Err(DocumentError::Version(file.version));
        }
        let mut doc = file.document;
        doc.selected.clear();
        doc.next_id = doc
            .shapes
            .iter()
            .map(|s| s.id + 1)
            .max()
            .unwrap_or(1)
            .max(doc.next_id);
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn group_and_ungroup() {
        let mut d = Document::default();
        let r = |x: f64| {
            crate::Shape::new(
                0,
                "r",
                kurbo::Rect::new(x, 0.0, x + 5.0, 5.0),
                crate::ShapeKind::Rect { corner_radius: 0.0 },
            )
        };
        let a = d.add_shape(r(0.0));
        let b = d.add_shape(r(10.0));
        let c = d.add_shape(r(20.0));
        d.selected = vec![a, b];
        assert!(d.group_selected());
        let mut g = d.with_group(a);
        g.sort();
        assert_eq!(g, vec![a, b]);
        assert_eq!(d.with_group(c), vec![c]);
        assert!(d.selection_has_group());
        assert!(d.ungroup_selected());
        assert_eq!(d.with_group(a), vec![a]);
    }

    #[test]
    fn nested_groups_ungroup_one_level() {
        let mut d = Document::default();
        let r = |x: f64| {
            crate::Shape::new(
                0,
                "r",
                kurbo::Rect::new(x, 0.0, x + 5.0, 5.0),
                crate::ShapeKind::Rect { corner_radius: 0.0 },
            )
        };
        let a = d.add_shape(r(0.0));
        let b = d.add_shape(r(10.0));
        let c = d.add_shape(r(20.0));
        d.selected = vec![a, b];
        d.group_selected();
        d.selected = vec![a, b, c];
        d.group_selected();
        assert_eq!(d.with_group(c).len(), 3);
        // Desagrupar el exterior deja el grupo interior (a, b) intacto.
        d.selected = vec![a, b, c];
        d.ungroup_selected();
        let mut g = d.with_group(a);
        g.sort();
        assert_eq!(g, vec![a, b]);
        assert_eq!(d.with_group(c), vec![c]);
        // Duplicar crea grupos nuevos, no se une al original.
        d.selected = vec![a, b];
        d.duplicate_selected();
        assert_eq!(d.with_group(a).len(), 2);
    }

    use super::*;
    use crate::shape::ShapeKind;
    use kurbo::Rect;

    fn rect(doc: &mut Document, name: &str) -> ShapeId {
        doc.add_shape(Shape::new(
            0,
            name,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            ShapeKind::Rect { corner_radius: 0.0 },
        ))
    }

    #[test]
    fn json_round_trip_keeps_shapes_and_ids() {
        let mut doc = Document::default();
        let a = rect(&mut doc, "A");
        let b = rect(&mut doc, "B");
        doc.selected = vec![a];
        let text = doc.to_json();
        let back = Document::from_json(&text).unwrap();
        assert_eq!(back.shapes.len(), 2);
        assert!(back.selected.is_empty());
        let mut back = back;
        assert!(back.next_id() > b);
    }

    #[test]
    fn reorder_moves_selection_within_stack() {
        let mut doc = Document::default();
        let a = rect(&mut doc, "A");
        let b = rect(&mut doc, "B");
        let c = rect(&mut doc, "C");
        doc.selected = vec![a];
        doc.reorder_selected(1);
        assert_eq!(
            doc.shapes.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![b, a, c]
        );
        doc.reorder_selected(i32::MAX);
        assert_eq!(doc.shapes.last().unwrap().id, a);
        doc.reorder_selected(i32::MIN);
        assert_eq!(doc.shapes.first().unwrap().id, a);
    }

    #[test]
    fn rejects_foreign_json() {
        assert!(Document::from_json("{\"format\":\"otro\",\"version\":2}").is_err());
        assert!(Document::from_json("no json").is_err());
    }
}
