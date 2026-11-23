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
    Some((pe.shape, s.world_edit_paths()))
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
    // Manejadores de los nodos seleccionados primero (están encima).
    for (si, ni) in app.selected_nodes() {
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

/// Tolerancia del ajuste en píxeles de pantalla.
pub const SNAP_PX: f64 = 7.0;
/// A partir de este número de nodos, una forma ajena solo aporta su caja como objetivo.
const SNAP_MAX_NODES: usize = 400;

/// Puntos a los que se ajusta un nodo que se arrastra: los demás nodos del trazo en edición
/// (salvo los que se mueven con él), los nodos y cajas de las otras formas visibles y los
/// bordes y el centro de la página.
pub fn snap_targets(app: &OpenCraftCutterApp, moving: &[(usize, usize)]) -> Vec<Point> {
    let mut out = Vec::new();
    let Some((id, paths)) = edited_paths(app) else {
        return out;
    };
    for (si, np) in paths.iter().enumerate() {
        for (ni, n) in np.nodes.iter().enumerate() {
            if !moving.contains(&(si, ni)) {
                out.push(n.p);
            }
        }
    }
    let page = &app.doc.page;
    for x in [0.0, page.width_mm / 2.0, page.width_mm] {
        for y in [0.0, page.height_mm / 2.0, page.height_mm] {
            out.push(Point::new(x, y));
        }
    }
    if page.smart_snap_within_media {
        return out;
    }
    for s in app.doc.shapes.iter().filter(|s| s.visible && s.id != id) {
        let b = oc_core::edit::shape_bounds(s);
        for x in [b.x0, b.center().x, b.x1] {
            for y in [b.y0, b.center().y, b.y1] {
                out.push(Point::new(x, y));
            }
        }
        if matches!(s.kind, ShapeKind::Path { .. }) {
            let rot = oc_core::edit::rotation_about(s.center(), s.rotation);
            let nodes = s.world_node_paths();
            if nodes.iter().map(|np| np.nodes.len()).sum::<usize>() <= SNAP_MAX_NODES {
                out.extend(
                    nodes
                        .iter()
                        .flat_map(|np| np.nodes.iter().map(|n| rot * n.p)),
                );
            }
        }
    }
    out
}

/// Nodos de una forma no-trazo (rectángulo, elipse, texto), ya en mm del mundo.
fn as_node_paths(s: &Shape) -> Vec<NodePath> {
    match &s.kind {
        ShapeKind::Path { .. } => s.world_edit_paths(),
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
        let mut paths = s.world_edit_paths();
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

    /// Descarta de la selección de nodos lo que ya no existe en el documento (tras deshacer o
    /// rehacer puede haber menos nodos, o no estar la forma en edición).
    pub fn fix_point_selection(&mut self) {
        let Some(pe) = self.canvas.point_edit else {
            self.canvas.point_multi.clear();
            return;
        };
        let Some(paths) = self.doc.shape(pe.shape).map(|s| s.world_edit_paths()) else {
            self.canvas.point_edit = None;
            self.canvas.point_multi.clear();
            return;
        };
        let exists = |(si, ni): (usize, usize)| paths.get(si).is_some_and(|p| ni < p.nodes.len());
        self.canvas.point_multi.retain(|n| exists(*n));
        let selected = pe
            .selected
            .filter(|n| exists(*n))
            .or_else(|| self.canvas.point_multi.iter().next().copied());
        self.canvas.point_edit = Some(PointEdit {
            shape: pe.shape,
            selected,
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
        let pair = self.join_pair();
        self.edit_nodes(true, |p, sel| {
            *sel = match pair {
                Some((a, b)) => nodes::join_ends(p, a, b),
                None => nodes::join_paths(p, si, ni),
            };
        });
        self.canvas.point_multi.clear();
    }

    /// Si hay exactamente dos nodos seleccionados y son extremos libres que se pueden unir,
    /// esos dos (Unir trazados une esos y no el más cercano).
    pub fn join_pair(&self) -> Option<((usize, usize), (usize, usize))> {
        let sel = self.selected_nodes();
        let (_, paths) = edited_paths(self)?;
        let [a, b] = sel.as_slice() else {
            return None;
        };
        let ok = nodes::is_free_end(&paths, a.0, a.1)
            && nodes::is_free_end(&paths, b.0, b.1)
            && (a.0 != b.0 || paths[a.0].nodes.len() >= 3);
        ok.then_some((*a, *b))
    }

    /// ¿Se puede unir la selección? Dos extremos elegidos, o uno con otro extremo libre.
    pub fn can_join(&self) -> bool {
        if self.join_pair().is_some() {
            return true;
        }
        let sel = self.selected_nodes();
        let [(si, ni)] = sel.as_slice() else {
            return false;
        };
        edited_paths(self).is_some_and(|(_, p)| nodes::join_target(&p, *si, *ni).is_some())
    }

    /// Deja de alinear los manejadores del nodo (pasa a vértice), para arrastrar uno solo con
    /// Alt. Devuelve si cambió algo (y por tanto ya hay una entrada de historial).
    pub fn point_edit_break_handles(&mut self, si: usize, ni: usize) -> bool {
        let smooth = edited_paths(self).is_some_and(|(_, p)| {
            p.get(si)
                .and_then(|np| np.nodes.get(ni))
                .is_some_and(|n| n.smooth)
        });
        if !smooth {
            return false;
        }
        self.edit_nodes(true, |p, _| {
            if let Some(np) = p.get_mut(si) {
                np.make_corner(ni);
            }
        });
        true
    }

    /// Nodo simétrico: suave y con los dos manejadores iguales.
    pub fn point_set_symmetric(&mut self) {
        let sel = self.selected_nodes();
        if sel.is_empty() {
            return;
        }
        self.edit_nodes(true, |p, _| {
            for (si, ni) in &sel {
                if let Some(np) = p.get_mut(*si) {
                    np.make_symmetric(*ni);
                }
            }
        });
    }

    /// Alinea entre sí los nodos seleccionados (dos o más).
    pub fn point_align(&mut self, mode: oc_core::arrange::Align) {
        let sel = self.selected_nodes();
        if sel.len() < 2 {
            return;
        }
        self.edit_nodes(true, |p, _| nodes::align_nodes(p, &sel, mode));
    }

    /// Reparte los nodos seleccionados (tres o más) a intervalos iguales.
    pub fn point_distribute(&mut self, axis: oc_core::arrange::Axis) {
        let sel = self.selected_nodes();
        if sel.len() < 3 {
            return;
        }
        self.edit_nodes(true, |p, _| nodes::distribute_nodes(p, &sel, axis));
    }

    /// Mueve los nodos seleccionados `d` mm (flechas del teclado). Devuelve si había alguno.
    pub fn point_nudge(&mut self, d: kurbo::Vec2) -> bool {
        let sel = self.selected_nodes();
        if sel.is_empty() {
            return false;
        }
        self.edit_nodes(true, |p, _| {
            for (si, ni) in &sel {
                if let Some(q) = p.get(*si).and_then(|np| np.nodes.get(*ni)).map(|n| n.p) {
                    p[*si].move_node(*ni, q + d);
                }
            }
        });
        true
    }

    /// Lleva el nodo principal a `to` (campos X e Y del panel); los demás seleccionados se
    /// mueven lo mismo. `commit` = nueva entrada de historial.
    pub fn point_set_position(&mut self, to: Point, commit: bool) {
        let Some((si, ni)) = self.canvas.point_edit.and_then(|pe| pe.selected) else {
            return;
        };
        if commit {
            self.commit();
        }
        self.point_move_node(si, ni, to);
    }

    /// Selecciona todos los nodos del trazo en edición.
    pub fn point_select_all(&mut self) {
        let Some((_, paths)) = edited_paths(self) else {
            return;
        };
        let all: std::collections::BTreeSet<(usize, usize)> = paths
            .iter()
            .enumerate()
            .flat_map(|(si, np)| (0..np.nodes.len()).map(move |ni| (si, ni)))
            .collect();
        if let Some(pe) = self.canvas.point_edit.as_mut() {
            if pe.selected.is_none() {
                pe.selected = all.iter().next().copied();
            }
        }
        self.canvas.point_multi = all;
    }

    /// Quita la selección de nodos. Devuelve si había alguno seleccionado.
    pub fn point_deselect(&mut self) -> bool {
        let had = !self.selected_nodes().is_empty();
        self.canvas.point_multi.clear();
        if let Some(pe) = self.canvas.point_edit.as_mut() {
            pe.selected = None;
        }
        had
    }

    /// Trazados (subtrazos) con algún nodo seleccionado.
    fn selected_subpaths(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.selected_nodes().iter().map(|s| s.0).collect();
        v.dedup();
        v
    }

    /// Invierte el sentido de los trazados con nodos seleccionados (cambia por dónde empieza
    /// el corte y hacia dónde va).
    pub fn point_reverse(&mut self) {
        let subs = self.selected_subpaths();
        if subs.is_empty() {
            return;
        }
        self.edit_nodes(true, |p, sel| {
            for si in &subs {
                if let Some(np) = p.get_mut(*si) {
                    np.reverse();
                }
            }
            *sel = None;
        });
        self.canvas.point_multi.clear();
    }

    /// ¿Hay algún trazado abierto (de tres nodos o más) con nodos seleccionados?
    pub fn can_close(&self) -> bool {
        let subs = self.selected_subpaths();
        edited_paths(self).is_some_and(|(_, p)| {
            subs.iter().any(|si| {
                p.get(*si)
                    .is_some_and(|np| !np.closed && np.nodes.len() >= 3)
            })
        })
    }

    /// Cierra con un tramo recto los trazados abiertos con nodos seleccionados.
    pub fn point_close(&mut self) {
        let subs = self.selected_subpaths();
        if !self.can_close() {
            return;
        }
        self.edit_nodes(true, |p, _| {
            for si in &subs {
                if let Some(np) = p.get_mut(*si) {
                    np.close();
                }
            }
        });
    }

    /// Tramos donde «Añadir punto medio» pondría un nodo: los que tienen sus dos nodos
    /// seleccionados o, con un solo nodo, el tramo resaltado.
    pub fn midpoint_segments(&self) -> Vec<(usize, usize)> {
        let Some((_, paths)) = edited_paths(self) else {
            return Vec::new();
        };
        let sel = self.selected_nodes();
        if let [(si, ni)] = sel.as_slice() {
            return paths
                .get(*si)
                .and_then(|np| highlighted_segment(np, *ni))
                .map(|seg| vec![(*si, seg)])
                .unwrap_or_default();
        }
        nodes::selected_segments(&paths, &sel)
    }

    /// Añade un nodo en el punto medio de los tramos seleccionados; los nuevos quedan
    /// seleccionados.
    pub fn point_midpoints(&mut self) {
        let segs = self.midpoint_segments();
        if segs.is_empty() {
            return;
        }
        let mut new = Vec::new();
        self.edit_nodes(true, |p, sel| {
            new = nodes::insert_midpoints(p, &segs);
            *sel = new.first().copied();
        });
        self.canvas.point_multi = new.into_iter().collect();
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

    /// Convertir en recta (`false`) o curva (`true`) los tramos seleccionados: los que tienen
    /// sus dos nodos seleccionados o, con un solo nodo, el tramo resaltado.
    pub fn point_segment_kind(&mut self, curve: bool) {
        let segs = self.midpoint_segments();
        if segs.is_empty() {
            return;
        }
        self.edit_nodes(true, |p, _| {
            for (si, seg) in &segs {
                let Some(np) = p.get_mut(*si) else {
                    continue;
                };
                if curve {
                    np.segment_to_curve(*seg);
                } else {
                    np.segment_to_line(*seg);
                }
            }
        });
    }

    /// Esquinas (nodos que se pueden redondear) entre los nodos seleccionados.
    fn selected_corners(&self) -> Vec<(usize, usize)> {
        let sel = self.selected_nodes();
        let Some((_, paths)) = edited_paths(self) else {
            return Vec::new();
        };
        sel.into_iter()
            .filter(|(si, ni)| paths.get(*si).is_some_and(|np| nodes::is_corner(np, *ni)))
            .collect()
    }

    /// Radio de redondeo (mm) de las esquinas seleccionadas: el del nodo principal si es una
    /// esquina; si no, el mayor de la selección. 0 si no hay ninguna redondeada.
    pub fn fillet_radius(&self) -> f64 {
        let Some((_, paths)) = edited_paths(self) else {
            return 0.0;
        };
        let radius = |(si, ni): (usize, usize)| {
            paths
                .get(si)
                .and_then(|np| np.nodes.get(ni))
                .map_or(0.0, |n| n.radius)
        };
        let corners = self.selected_corners();
        let primary = self
            .canvas
            .point_edit
            .and_then(|pe| pe.selected)
            .filter(|n| corners.contains(n));
        match primary {
            Some(n) => radius(n),
            None => corners.into_iter().map(radius).fold(0.0, f64::max),
        }
    }

    /// ¿Hay alguna esquina entre los nodos seleccionados?
    pub fn can_fillet(&self) -> bool {
        !self.selected_corners().is_empty()
    }

    /// Pone el radio de redondeo (mm) de las esquinas seleccionadas. El radio queda guardado
    /// en el nodo, que sigue en el vértice: se puede cambiar cuando se quiera, y 0 devuelve la
    /// esquina viva. `commit` = nueva entrada de historial (falso mientras se arrastra el valor).
    pub fn point_fillet(&mut self, radius: f64, commit: bool) {
        let corners = self.selected_corners();
        if corners.is_empty() {
            return;
        }
        let radius = radius.max(0.0);
        self.edit_nodes(commit, |p, _| {
            for (si, ni) in &corners {
                if let Some(n) = p.get_mut(*si).and_then(|np| np.nodes.get_mut(*ni)) {
                    n.radius = radius;
                }
            }
        });
    }

    /// ¿Hay alguna esquina redondeada entre los nodos seleccionados?
    pub fn can_expand_fillet(&self) -> bool {
        self.fillet_radius() > 0.0
    }

    /// Convierte el redondeo de las esquinas seleccionadas en nodos normales (los dos extremos
    /// del arco), para retocar la curva a mano. Los nodos nuevos quedan seleccionados.
    pub fn point_expand_fillet(&mut self) {
        let corners = self.selected_corners();
        if !self.can_expand_fillet() {
            return;
        }
        let mut selected: Vec<(usize, usize)> = Vec::new();
        self.edit_nodes(true, |p, sel| {
            for (si, np) in p.iter_mut().enumerate() {
                let each: Vec<(usize, f64)> = corners
                    .iter()
                    .filter(|c| c.0 == si)
                    .filter_map(|c| np.nodes.get(c.1).map(|n| (c.1, n.radius)))
                    .filter(|c| c.1 > 0.0)
                    .collect();
                if each.is_empty() {
                    continue;
                }
                let (out, new) = nodes::fillet_corners_each(np, &each);
                *np = out;
                selected.extend(new.into_iter().map(|ni| (si, ni)));
            }
            *sel = selected.first().copied();
        });
        self.canvas.point_multi = selected.into_iter().collect();
    }

    /// Convierte un tramo concreto en recta (`false`) o curva (`true`).
    pub fn point_segment_set(&mut self, si: usize, seg: usize, curve: bool) {
        self.edit_nodes(true, |p, _| {
            let Some(np) = p.get_mut(si).filter(|np| seg < np.segment_count()) else {
                return;
            };
            if curve {
                np.segment_to_curve(seg);
            } else {
                np.segment_to_line(seg);
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
