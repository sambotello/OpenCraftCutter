//! Editar puntos con nodos Bézier, como la referencia: seleccionar y mover nodos y
//! manejadores, insertar un nodo con un clic en la línea, y las acciones del panel
//! (borrar punto, romper trazado, vértice, suavizar, convertir en recta/curva, simplificar,
//! convertir en trazado).

use super::PointEdit;
use crate::app::OpenCraftCutterApp;
use kurbo::Point;
use oc_core::nodes::{self, NodePath};
use oc_core::shape::{Shape, ShapeKind};

/// Qué hay bajo el puntero en la edición de puntos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Node(usize, usize),
    /// Manejador del nodo (subtrazo, nodo, de salida).
    Handle(usize, usize, bool),
    /// Tramo (subtrazo, tramo, t).
    Segment(usize, usize, f64),
}

/// Nodos (mm, mundo) del trazo en edición.
pub fn edited_paths(app: &OpenCraftCutterApp) -> Option<(u64, Vec<NodePath>)> {
    let pe = app.canvas.point_edit?;
    let s = app.doc.shape(pe.shape)?;
    Some((pe.shape, s.world_node_paths()))
}

/// Tramo resaltado: el que termina en el nodo seleccionado (como la referencia).
pub fn highlighted_segment(np: &NodePath, idx: usize) -> Option<usize> {
    let n = np.nodes.len();
    if idx > 0 {
        Some(idx - 1)
    } else if np.closed && n > 1 {
        Some(n - 1)
    } else {
        None
    }
}

/// Elemento bajo el puntero `pos` (px de pantalla).
pub fn hit(app: &OpenCraftCutterApp, pos: egui::Pos2) -> Option<Hit> {
    let (_, paths) = edited_paths(app)?;
    let vp = app.canvas.viewport;
    let r = super::overlay::VERTEX_PX + 2.0;
    // Manejadores del nodo seleccionado primero (están encima).
    if let Some((si, ni)) = app.canvas.point_edit.and_then(|pe| pe.selected) {
        if let Some(n) = paths.get(si).and_then(|p| p.nodes.get(ni)) {
            for (h, out) in [(n.h_out, true), (n.h_in, false)] {
                if let Some(h) = h {
                    if vp.to_screen(h).distance(pos) <= r {
                        return Some(Hit::Handle(si, ni, out));
                    }
                }
            }
        }
    }
    for (si, np) in paths.iter().enumerate() {
        for (ni, n) in np.nodes.iter().enumerate() {
            if vp.to_screen(n.p).distance(pos) <= r {
                return Some(Hit::Node(si, ni));
            }
        }
    }
    let p = vp.to_world(pos);
    let tol_mm = 6.0 / vp.scale;
    paths
        .iter()
        .enumerate()
        .filter_map(|(si, np)| np.nearest_segment(p).map(|(i, t, d)| (si, i, t, d)))
        .filter(|(.., d)| *d <= tol_mm)
        .min_by(|a, b| a.3.total_cmp(&b.3))
        .map(|(si, i, t, _)| Hit::Segment(si, i, t))
}

/// Nodos de una forma no-trazo (rectángulo, elipse, texto), ya en mm del mundo.
fn as_node_paths(s: &Shape) -> Vec<NodePath> {
    match &s.kind {
        ShapeKind::Path { .. } => s.world_node_paths(),
        ShapeKind::Ellipse => vec![nodes::ellipse(s.rect)],
        ShapeKind::Text { .. } => crate::text::text_to_path(s)
            .map(|p| p.world_node_paths())
            .unwrap_or_default(),
        _ => oc_core::ops::closed_contours(&Shape {
            rotation: 0.0,
            ..s.clone()
        })
        .iter()
        .map(|c| {
            NodePath::from_points(
                &c.iter().map(|p| Point::new(p[0], p[1])).collect::<Vec<_>>(),
                true,
            )
        })
        .collect(),
    }
}

/// Convierte una forma en trazo de nodos con la rotación aplicada (para editarla en el mundo).
pub fn to_editable_path(s: &Shape) -> Option<Shape> {
    if s.is_image() {
        return None;
    }
    let rot = oc_core::edit::rotation_about(s.center(), s.rotation);
    let paths: Vec<NodePath> = as_node_paths(s)
        .iter()
        .map(|p| p.map(|q| rot * q))
        .collect();
    if paths.is_empty() {
        return None;
    }
    let mut out = s.clone();
    if !matches!(s.kind, ShapeKind::Path { .. }) {
        out.kind = ShapeKind::Path {
            subpaths: Vec::new(),
            fill_rule: oc_core::FillRule::EvenOdd,
        };
        out.name = format!("{} (trazado)", s.name);
    }
    out.rotation = 0.0;
    out.set_world_node_paths(&paths);
    Some(out)
}

/// Empieza a editar los puntos de una forma (convirtiéndola en trazo si hace falta).
pub fn begin(app: &mut OpenCraftCutterApp, id: u64) {
    let Some(s) = app.doc.shape(id).cloned() else {
        return;
    };
    let ready = matches!(s.kind, ShapeKind::Path { .. }) && s.rotation == 0.0;
    let target = if ready {
        id
    } else {
        let Some(mut p) = to_editable_path(&s) else {
            return;
        };
        p.id = 0;
        app.replace_shapes(&[id], vec![p]);
        app.doc.selected.first().copied().unwrap_or(id)
    };
    app.doc.selected = vec![target];
    app.canvas.point_multi.clear();
    app.canvas.point_edit = Some(PointEdit {
        shape: target,
        selected: None,
    });
}

impl OpenCraftCutterApp {
    /// Aplica `f` a los nodos del trazo en edición (con historial) y guarda el resultado.
    fn edit_nodes(
        &mut self,
        commit: bool,
        f: impl FnOnce(&mut Vec<NodePath>, &mut Option<(usize, usize)>),
    ) {
        let Some(pe) = self.canvas.point_edit else {
            return;
        };
        let Some(s) = self.doc.shape(pe.shape) else {
            return;
        };
        let mut paths = s.world_node_paths();
        let mut sel = pe.selected;
        f(&mut paths, &mut sel);
        paths.retain(|p| p.nodes.len() >= 2);
        if commit {
            self.commit();
        }
        if let Some(s) = self.doc.shape_mut(pe.shape) {
            s.set_world_node_paths(&paths);
        }
        let sel = sel.filter(|(si, ni)| paths.get(*si).is_some_and(|p| *ni < p.nodes.len()));
        if sel.is_none() {
            self.canvas.point_multi.clear();
        }
        self.canvas
            .point_multi
            .retain(|(si, ni)| paths.get(*si).is_some_and(|p| *ni < p.nodes.len()));
        self.canvas.point_edit = Some(PointEdit {
            shape: pe.shape,
            selected: sel,
        });
    }

    /// Nodos seleccionados (el principal incluido), sin repetir.
    pub fn selected_nodes(&self) -> Vec<(usize, usize)> {
        let mut v: std::collections::BTreeSet<(usize, usize)> = self.canvas.point_multi.clone();
        if let Some(p) = self.canvas.point_edit.and_then(|pe| pe.selected) {
            v.insert(p);
        }
        v.into_iter().collect()
    }

    /// Clic en un nodo: con Mayús lo añade o quita; sin Mayús lo selecciona (si ya formaba
    /// parte de la selección múltiple, la conserva para poder arrastrar el grupo).
    pub fn point_select(&mut self, si: usize, ni: usize, additive: bool) {
        let in_sel = self.selected_nodes().contains(&(si, ni));
        let Some(pe) = self.canvas.point_edit.as_mut() else {
            return;
        };
        if additive {
            if in_sel {
                self.canvas.point_multi.remove(&(si, ni));
                if pe.selected == Some((si, ni)) {
                    pe.selected = self.canvas.point_multi.iter().next().copied();
                }
            } else {
                if let Some(prev) = pe.selected {
                    self.canvas.point_multi.insert(prev);
                }
                pe.selected = Some((si, ni));
            }
        } else {
            if !in_sel {
                self.canvas.point_multi.clear();
            } else if let Some(prev) = pe.selected {
                self.canvas.point_multi.insert(prev);
            }
            pe.selected = Some((si, ni));
        }
    }

    /// Selecciona los nodos dentro de `r` (mm, mundo).
    pub fn point_select_rect(&mut self, r: kurbo::Rect, additive: bool) {
        let Some((_, paths)) = edited_paths(self) else {
            return;
        };
        if !additive {
            self.canvas.point_multi.clear();
            if let Some(pe) = self.canvas.point_edit.as_mut() {
                pe.selected = None;
            }
        }
        let hits: Vec<(usize, usize)> = paths
            .iter()
            .enumerate()
            .flat_map(|(si, np)| {
                np.nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| r.contains(n.p))
                    .map(move |(ni, _)| (si, ni))
            })
            .collect();
        for h in hits {
            self.point_select(h.0, h.1, true);
        }
    }

    /// Mueve el nodo arrastrado a `to`; los demás nodos seleccionados se mueven lo mismo.
    pub fn point_move_node(&mut self, si: usize, ni: usize, to: Point) {
        let sel = self.selected_nodes();
        self.edit_nodes(false, |p, _| {
            let Some(from) = p.get(si).and_then(|np| np.nodes.get(ni)).map(|n| n.p) else {
                return;
            };
            let d = to - from;
            if sel.contains(&(si, ni)) {
                for (a, b) in &sel {
                    if let Some(np) = p.get_mut(*a) {
                        if let Some(q) = np.nodes.get(*b).map(|n| n.p) {
                            np.move_node(*b, q + d);
                        }
                    }
                }
            } else if let Some(np) = p.get_mut(si) {
                np.move_node(ni, to);
            }
        });
    }

    pub fn point_move_handle(&mut self, si: usize, ni: usize, out: bool, to: Point) {
        self.edit_nodes(false, |p, _| {
            if let Some(np) = p.get_mut(si) {
                np.move_handle(ni, out, to);
            }
        });
    }

    /// Inserta un nodo en el tramo y lo selecciona.
    pub fn point_insert(&mut self, si: usize, seg: usize, t: f64) {
        self.edit_nodes(true, |p, sel| {
            if let Some(np) = p.get_mut(si) {
                let k = np.insert_at(seg, t);
                *sel = Some((si, k));
            }
        });
    }

    /// Borrar punto: borra todos los nodos seleccionados. Devuelve si había alguno.
    pub fn delete_selected_vertex(&mut self) -> bool {
        let mut sel = self.selected_nodes();
        if sel.is_empty() {
            return false;
        }
        // De atrás hacia delante para que los índices sigan siendo válidos.
        sel.sort_by(|a, b| b.cmp(a));
        self.edit_nodes(true, |p, s| {
            for (si, ni) in &sel {
                if let Some(np) = p.get_mut(*si) {
                    np.delete(*ni);
                }
            }
            *s = None;
        });
        self.canvas.point_multi.clear();
        true
    }

    pub fn point_break(&mut self) {
        let Some((si, ni)) = self.canvas.point_edit.and_then(|pe| pe.selected) else {
            return;
        };
        self.edit_nodes(true, |p, sel| {
            if si < p.len() {
                let parts = p[si].break_at(ni);
                p.splice(si..=si, parts);
            }
            *sel = None;
        });
    }

    /// Funde el extremo seleccionado con el extremo libre más cercano (de otro trazado o del
    /// mismo, que queda cerrado) en un solo nodo. El nodo de la unión queda seleccionado.
    pub fn point_join(&mut self) {
        let Some((si, ni)) = self.canvas.point_edit.and_then(|pe| pe.selected) else {
            return;
        };
        self.edit_nodes(true, |p, sel| {
            *sel = oc_core::nodes::join_paths(p, si, ni);
        });
    }

    pub fn point_set_smooth(&mut self, smooth: bool) {
        let sel = self.selected_nodes();
        if sel.is_empty() {
            return;
        }
        self.edit_nodes(true, |p, _| {
            for (si, ni) in &sel {
                if let Some(np) = p.get_mut(*si) {
                    if smooth {
                        np.make_smooth(*ni);
                    } else {
                        np.make_corner(*ni);
                    }
                }
            }
        });
    }

    /// Convertir el tramo resaltado en recta (`false`) o curva (`true`).
    pub fn point_segment_kind(&mut self, curve: bool) {
        let Some((si, ni)) = self.canvas.point_edit.and_then(|pe| pe.selected) else {
            return;
        };
        self.edit_nodes(true, |p, _| {
            if let Some(np) = p.get_mut(si) {
                if let Some(seg) = highlighted_segment(np, ni) {
                    if curve {
                        np.segment_to_curve(seg);
                    } else {
                        np.segment_to_line(seg);
                    }
                }
            }
        });
    }

    /// Simplificar: reajusta cada trazo con curvas de pocos nodos.
    pub fn point_simplify(&mut self) {
        let ids: Vec<u64> = match self.canvas.point_edit {
            Some(pe) => vec![pe.shape],
            None => self.doc.selected.clone(),
        };
        let mut changed = false;
        for id in &ids {
            let Some(s) = self.doc.shape(*id) else {
                continue;
            };
            if !matches!(s.kind, ShapeKind::Path { .. }) {
                continue;
            }
            let paths: Vec<NodePath> = s
                .world_node_paths()
                .iter()
                .flat_map(|np| {
                    let pts = np.flatten(nodes::FLATTEN_TOL_MM);
                    nodes::from_bez(&oc_trace::simplify_polyline(&pts, np.closed, 0.15))
                })
                .collect();
            if !changed {
                self.commit();
                changed = true;
            }
            if let Some(s) = self.doc.shape_mut(*id) {
                s.set_world_node_paths(&paths);
            }
        }
        if let Some(pe) = self.canvas.point_edit.as_mut() {
            pe.selected = None;
        }
    }

    /// Convertir en trazado: los rectángulos, elipses y textos seleccionados pasan a trazos.
    pub fn point_convert_to_path(&mut self) {
        let shapes: Vec<Shape> = self
            .doc
            .selected_shapes()
            .into_iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    ShapeKind::Rect { .. } | ShapeKind::Ellipse | ShapeKind::Text { .. }
                )
            })
            .cloned()
            .collect();
        if shapes.is_empty() {
            return;
        }
        let ids: Vec<u64> = shapes.iter().map(|s| s.id).collect();
        let out: Vec<Shape> = shapes
            .iter()
            .filter_map(to_editable_path)
            .map(|mut p| {
                p.id = 0;
                p
            })
            .collect();
        self.replace_shapes(&ids, out);
    }
}

/// Recuento de nodos de la selección: (formas, contornos, nodos, lineales, Bézier).
pub fn node_counts(app: &OpenCraftCutterApp) -> (usize, usize, usize, usize, usize) {
    let shapes = app.doc.selected_shapes();
    let mut c = (shapes.len(), 0, 0, 0, 0);
    for s in shapes {
        let paths = if matches!(s.kind, ShapeKind::Path { .. }) {
            s.world_node_paths()
        } else {
            as_node_paths(s)
        };
        for np in paths {
            c.1 += 1;
            for n in &np.nodes {
                c.2 += 1;
                if n.is_bezier() {
                    c.4 += 1;
                } else {
                    c.3 += 1;
                }
            }
        }
    }
    c
}
