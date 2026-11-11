//! Diseños más grandes que el tapete: «Dividir en partes» y navegación entre partes.

use crate::app::OpenCraftCutterApp;
use oc_core::ops::BoolOp;
use oc_core::shape::{FillRule, Shape, ShapeKind};

/// Las formas de una parte llevan este prefijo en el nombre: «Parte 2/4 · …».
const PREFIX: &str = "Parte ";

/// (parte, total) de una forma dividida.
pub fn part_of(s: &Shape) -> Option<(usize, usize)> {
    let rest = s.name.strip_prefix(PREFIX)?;
    let (frac, _) = rest.split_once(" · ")?;
    let (k, n) = frac.split_once('/')?;
    Some((k.parse().ok()?, n.parse().ok()?))
}

impl OpenCraftCutterApp {
    /// Tamaño útil del tapete (sin el margen de corte), o de la página si no hay tapete.
    fn part_size(&self) -> (f64, f64) {
        let page = &self.doc.page;
        let mat = oc_core::machines::resolve_mat(page.machine, page.mat);
        let m = page.cut_margin_mm.max(0.0);
        let (w, h) = if mat.width_mm > 0.0 {
            (mat.width_mm, mat.height_mm)
        } else {
            (page.width_mm, page.height_mm)
        };
        (w - 2.0 * m, h - 2.0 * m)
    }

    /// Reparte el diseño visible en partes del tamaño del tapete con `overlap` de solape. Cada
    /// parte es un grupo en la esquina del área de corte; solo se ve la primera.
    pub fn split_into_parts(&mut self, overlap: f64) -> String {
        let test_ids = oc_core::send::test_shape_ids(&self.doc);
        let shapes: Vec<Shape> = self
            .doc
            .shapes
            .iter()
            .filter(|s| s.visible && !test_ids.contains(&s.id) && part_of(s).is_none())
            .cloned()
            .collect();
        let images = shapes.iter().filter(|s| s.is_image()).count();
        let lines = crate::text::paths_for_ops(
            &shapes
                .iter()
                .filter(|s| !s.is_image())
                .cloned()
                .collect::<Vec<_>>(),
        );
        let Some(design) = oc_core::edit::selection_bounds(lines.iter()) else {
            return "No hay líneas que dividir.".into();
        };
        let (w, h) = self.part_size();
        let tiles = oc_core::tiles::tile_grid(design, w, h, overlap);
        if tiles.len() <= 1 {
            return "El diseño ya cabe en el tapete: no hace falta dividirlo.".into();
        }
        let n = tiles.len();
        let m = self.doc.page.cut_margin_mm.max(0.0);
        self.commit();
        let old: Vec<u64> = shapes
            .iter()
            .filter(|s| !s.is_image())
            .map(|s| s.id)
            .collect();
        self.doc.remove_shapes(&old);
        for (k, tile) in tiles.iter().enumerate() {
            let shift = kurbo::Point::new(m, m) - tile.origin();
            let window = Shape::new(0, "", *tile, ShapeKind::Rect { corner_radius: 0.0 });
            let g = self.doc.next_id();
            let mut pieces: Vec<Shape> = Vec::new();
            for s in &lines {
                if !oc_core::ops::closed_contours(s).is_empty() {
                    if let Some(v) =
                        oc_core::ops::boolean(&[s.clone(), window.clone()], BoolOp::Intersect)
                    {
                        pieces.extend(v);
                    }
                }
                let open: Vec<(Vec<kurbo::Point>, bool)> = oc_core::ops::open_paths(s)
                    .iter()
                    .flat_map(|p| {
                        let pts: Vec<kurbo::Point> =
                            p.iter().map(|q| kurbo::Point::new(q[0], q[1])).collect();
                        oc_core::tiles::clip_polyline(&pts, *tile)
                    })
                    .map(|p| (p, false))
                    .collect();
                if !open.is_empty() {
                    let (rect, subpaths) = Shape::normalize_subpaths(&open);
                    let mut o = s.clone();
                    o.id = 0;
                    o.rect = rect;
                    o.rotation = 0.0;
                    o.kind = ShapeKind::Path {
                        subpaths,
                        fill_rule: FillRule::NonZero,
                    };
                    o.fill = None;
                    pieces.push(o);
                }
            }
            for mut p in pieces {
                p.id = 0;
                p.rect = p.rect + shift;
                p.group = Some(g);
                p.inner_groups.clear();
                let base = p.name.clone();
                p.name = format!("{PREFIX}{}/{n} · {base}", k + 1);
                p.visible = k == 0;
                self.doc.add_shape(p);
            }
        }
        self.doc.selected.clear();
        let cols = tiles.iter().filter(|t| t.y0 == tiles[0].y0).count();
        let mut msg = format!(
            "Diseño dividido en {n} partes ({cols} × {}). Corta la parte 1 y luego pasa a la siguiente.",
            n / cols
        );
        if images > 0 {
            msg += " Las imágenes no se dividen: se quedan como estaban.";
        }
        msg
    }

    /// Parte visible ahora y número de partes.
    pub fn current_part(&self) -> Option<(usize, usize)> {
        let mut total = None;
        let mut shown = None;
        for s in &self.doc.shapes {
            if let Some((k, n)) = part_of(s) {
                total = Some(n);
                if s.visible {
                    shown = Some(k);
                }
            }
        }
        Some((shown.unwrap_or(0), total?))
    }

    /// Muestra solo la parte `k` (1…n).
    pub fn show_part(&mut self, k: usize) {
        for s in self.doc.shapes.iter_mut() {
            if let Some((p, _)) = part_of(s) {
                s.visible = p == k;
            }
        }
        self.doc.selected.clear();
    }
}

/// Sección «Diseños grandes» del panel Configuración de página.
pub fn section(app: &mut OpenCraftCutterApp, ui: &mut egui::Ui) {
    use crate::theme;
    use egui::RichText;
    use egui_material_icons::icons as mi;
    ui.separator();
    ui.label(
        RichText::new("Diseños grandes")
            .strong()
            .color(theme::CHROME_100),
    );
    if let Some((k, n)) = app.current_part() {
        ui.horizontal(|ui| {
            let r = ui
                .add_enabled(k > 1, egui::Button::new(mi::ICON_CHEVRON_LEFT.rich_text()))
                .on_hover_text("Parte anterior");
            theme::name_widget(&r, "Parte anterior");
            if r.clicked() {
                app.show_part(k - 1);
            }
            ui.label(format!("Parte {k} de {n}"));
            let r = ui
                .add_enabled(k < n, egui::Button::new(mi::ICON_CHEVRON_RIGHT.rich_text()))
                .on_hover_text("Parte siguiente");
            theme::name_widget(&r, "Parte siguiente");
            if r.clicked() {
                app.show_part(k + 1);
            }
        });
        ui.label(
            RichText::new("Solo se ve (y se corta) la parte elegida. Corta una, cambia el material y pasa a la siguiente.")
                .small()
                .color(theme::CHROME_300),
        );
        return;
    }
    ui.label(
        RichText::new(
            "Si el diseño es más grande que el tapete, divídelo en partes que se solapan para \
             unirlas al aplicarlo.",
        )
        .small()
        .color(theme::CHROME_300),
    );
    let id = egui::Id::unique("parts_overlap");
    let mut overlap: f64 = ui.data(|d| d.get_temp(id)).unwrap_or(10.0);
    let unit = app.doc.page.unit;
    ui.horizontal(|ui| {
        ui.label("Solape");
        let mut v = unit.from_mm(overlap);
        if ui
            .add(
                egui::DragValue::new(&mut v)
                    .range(0.0..=unit.from_mm(50.0))
                    .speed(unit.step())
                    .fixed_decimals(unit.decimals())
                    .suffix(format!(" {}", unit.label())),
            )
            .changed()
        {
            overlap = unit.to_mm(v);
        }
    });
    if ui
        .add(
            egui::Button::new(format!(
                "{}  Dividir en partes",
                <&str>::from(mi::ICON_GRID_VIEW)
            ))
            .min_size(egui::vec2(ui.available_width(), 30.0)),
        )
        .clicked()
    {
        let msg = app.split_into_parts(overlap);
        let ctx = ui.ctx().clone();
        app.notify(&ctx, msg);
    }
    ui.data_mut(|d| d.insert_temp(id, overlap));
}
