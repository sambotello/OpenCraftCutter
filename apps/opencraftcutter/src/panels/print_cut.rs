//! Panel Imprimir y cortar, con las opciones y el orden de la referencia: activar marcas de
//! registro, longitud, grosor e inserción; más ajustes (estilo e inserción por lado);
//! restaurar, imprimir y sangrado de impresión. Además, «Hoja de pegatinas»: contorno con
//! margen alrededor de cada imagen, copias acomodadas en la hoja y marcas activadas.

use super::section;
use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{Color32, RichText, Sense, Stroke as EStroke, StrokeKind, Ui, Vec2 as EVec2};
use egui_material_icons::icons as mi;
use oc_core::regmarks::RegMarks;
use oc_core::shape::Shape;

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(theme::CHROME_300));
}

/// Deslizador con campo en la unidad del documento. Devuelve si cambió.
fn length_field(
    ui: &mut Ui,
    unit: oc_core::units::Unit,
    label: &str,
    mm: &mut f64,
    range_mm: std::ops::RangeInclusive<f64>,
) -> bool {
    ui.label(RichText::new(label).color(theme::CHROME_200));
    let mut v = unit.from_mm(*mm);
    let range = unit.from_mm(*range_mm.start())..=unit.from_mm(*range_mm.end());
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 110.0).max(60.0);
        changed |= ui
            .add(egui::Slider::new(&mut v, range.clone()).show_value(false))
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut v)
                    .range(range)
                    .speed(unit.step() / 2.0)
                    .fixed_decimals(unit.decimals() + 1)
                    .suffix(format!(" {}", unit.label())),
            )
            .changed();
    });
    if changed {
        *mm = unit.to_mm(v);
    }
    changed
}

/// Botón de estilo con un dibujo de las marcas (cuadrado + escuadras).
fn style_button(ui: &mut Ui, w: f32, selected: bool, inverted: bool, label: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(EVec2::new(w, 34.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 4.0, theme::ACCENT_STRONG);
    } else if resp.hovered() {
        p.rect_filled(rect, 4.0, theme::CHROME_700);
    }
    p.rect_stroke(
        rect,
        4.0,
        EStroke::new(1.0, theme::CHROME_500),
        StrokeKind::Inside,
    );
    let color = if selected {
        Color32::WHITE
    } else {
        theme::CHROME_200
    };
    // Miniatura 18×18: cuadrado en una esquina y dos escuadras.
    let box_ = egui::Rect::from_min_size(
        rect.left_center() + EVec2::new(8.0, -9.0),
        EVec2::splat(18.0),
    );
    let (top, bottom) = (box_.top(), box_.bottom());
    let (sq_y, l1_y, l2_y) = if inverted {
        (bottom - 4.0, top, bottom)
    } else {
        (top, top, bottom)
    };
    p.rect_filled(
        egui::Rect::from_min_size(egui::pos2(box_.left(), sq_y), EVec2::splat(4.0)),
        0.0,
        color,
    );
    let st = EStroke::new(1.5, color);
    let arm = 6.0;
    let dir = |y: f32, down: bool| if down { y + arm } else { y - arm };
    // Escuadra a la derecha (arriba o abajo) y a la izquierda en el extremo opuesto.
    let (ry, rdown) = if inverted {
        (l2_y, false)
    } else {
        (l1_y, true)
    };
    p.line_segment(
        [
            egui::pos2(box_.right() - arm, ry),
            egui::pos2(box_.right(), ry),
        ],
        st,
    );
    p.line_segment(
        [
            egui::pos2(box_.right(), ry),
            egui::pos2(box_.right(), dir(ry, rdown)),
        ],
        st,
    );
    let (ly, ldown) = if inverted {
        (l1_y, true)
    } else {
        (l2_y, false)
    };
    p.line_segment(
        [
            egui::pos2(box_.left(), ly),
            egui::pos2(box_.left() + arm, ly),
        ],
        st,
    );
    p.line_segment(
        [
            egui::pos2(box_.left(), ly),
            egui::pos2(box_.left(), dir(ly, ldown)),
        ],
        st,
    );
    p.text(
        rect.center() + EVec2::new(10.0, 0.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(12.5),
        color,
    );
    resp.clicked()
}

/// Nombre de los contornos de corte de la hoja de pegatinas.
pub const STICKER_CONTOUR: &str = "Contorno de pegatina";
/// Máximo de pegatinas al rellenar la hoja.
const MAX_STICKERS: usize = 300;

#[derive(Clone, Copy, PartialEq)]
struct StickerSettings {
    margin_mm: f64,
    /// `None`: tantas como quepan.
    copies: Option<usize>,
}

impl Default for StickerSettings {
    fn default() -> Self {
        StickerSettings {
            margin_mm: 2.0,
            copies: None,
        }
    }
}

impl OpenCraftCutterApp {
    /// Imágenes para la hoja: las seleccionadas o, sin selección, todas (salvo PixScan).
    fn sticker_images(&self) -> Vec<Shape> {
        let ok = |s: &&Shape| s.is_image() && !s.pixscan && s.visible;
        let sel: Vec<Shape> = self
            .doc
            .selected_shapes()
            .into_iter()
            .filter(ok)
            .cloned()
            .collect();
        if !sel.is_empty() {
            return sel;
        }
        self.doc.shapes.iter().filter(ok).cloned().collect()
    }

    /// Hoja de pegatinas: cada imagen con su contorno (silueta + margen), agrupados; las
    /// copias pedidas (o tantas como quepan) acomodadas en el área útil; marcas activadas.
    /// Devuelve un mensaje para el usuario.
    pub fn make_sticker_sheet(&mut self, margin_mm: f64, copies: Option<usize>) -> String {
        let images = self.sticker_images();
        if images.is_empty() {
            return "Añade o selecciona una imagen para hacer pegatinas.".into();
        }
        // Pieza por imagen: la imagen y su contorno de corte.
        let mut kinds: Vec<(Shape, Shape)> = Vec::new();
        for img in &images {
            let Some(sil) = super::offset::image_silhouette(self, img) else {
                continue;
            };
            let Some(mut c) = oc_core::ops::offset(&sil, margin_mm, oc_core::ops::Join::Round)
            else {
                continue;
            };
            c.name = STICKER_CONTOUR.into();
            c.fill = None;
            c.stroke.color = oc_core::Color::rgb(0xe3, 0x34, 0x3f);
            kinds.push((img.clone(), c));
        }
        if kinds.is_empty() {
            return "No se encontró el contorno de las imágenes (¿son totalmente opacas o transparentes?).".into();
        }
        self.commit();
        self.doc.page.reg_marks.enabled = true;
        let area = oc_core::send::safe_area(&self.doc);
        let gap = crate::object_menu::PACK_GAP_MM;
        let bounds: Vec<kurbo::Rect> = kinds
            .iter()
            .map(|(i, c)| oc_core::edit::shape_bounds(i).union(oc_core::edit::shape_bounds(c)))
            .collect();
        // Lista de piezas (índice de imagen) en reparto alterno.
        let fits = |list: &[usize]| {
            let sizes: Vec<(f64, f64)> = list
                .iter()
                .map(|&k| (bounds[k].width(), bounds[k].height()))
                .collect();
            let at = oc_core::arrange::pack(&sizes, area, gap);
            at.iter().all(|p| p.is_some()).then_some(at)
        };
        let mut list: Vec<usize> = Vec::new();
        match copies {
            Some(n) => {
                for _ in 0..n.max(1) {
                    list.extend(0..kinds.len());
                }
            }
            None => {
                let mut k = 0;
                while list.len() < MAX_STICKERS {
                    list.push(k % kinds.len());
                    if fits(&list).is_none() {
                        list.pop();
                        break;
                    }
                    k += 1;
                }
            }
        }
        let sizes: Vec<(f64, f64)> = list
            .iter()
            .map(|&k| (bounds[k].width(), bounds[k].height()))
            .collect();
        let at = oc_core::arrange::pack(&sizes, area, gap);
        // Las imágenes originales se sustituyen por las pegatinas.
        let old: Vec<u64> = images.iter().map(|s| s.id).collect();
        self.doc.remove_shapes(&old);
        let mut new_ids = Vec::new();
        let mut placed = 0;
        for (&k, p) in list.iter().zip(at) {
            let Some(p) = p else { continue };
            let d = p - bounds[k].origin();
            let g = self.doc.next_id();
            for src in [&kinds[k].0, &kinds[k].1] {
                let mut s = src.clone();
                s.id = 0;
                s.rect = s.rect + d;
                s.group = Some(g);
                s.inner_groups.clear();
                new_ids.push(self.doc.add_shape(s));
            }
            placed += 1;
        }
        self.doc.selected = new_ids;
        let missing = list.len() - placed;
        let mut msg = if placed == 1 {
            "1 pegatina en la hoja".to_string()
        } else {
            format!("{placed} pegatinas en la hoja")
        };
        if missing > 0 {
            msg += &format!("; {missing} no caben");
        }
        msg + ". Imprime la hoja y envíala: la máquina leerá las marcas antes de cortar."
    }
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let unit = app.doc.page.unit;
    let before = app.doc.page.reg_marks;
    let mut m: RegMarks = before;
    let more_id = egui::Id::unique("print_cut_more");
    let mut more: bool = ui.data(|d| d.get_temp(more_id)).unwrap_or(false);

    ui.checkbox(&mut m.enabled, "Activar marcas de registro");
    hint(
        ui,
        "Las marcas de registro son marcas guía que ayudan a la máquina a cortar tus diseños \
         impresos con precisión.",
    );
    ui.separator();
    ui.add_enabled_ui(m.enabled, |ui| {
        section(ui, "Configuración avanzada");
        hint(
            ui,
            "Para obtener mejores resultados, se recomienda la configuración predeterminada.",
        );
        length_field(ui, unit, "Longitud", &mut m.length_mm, 5.0..=20.0);
        length_field(ui, unit, "Grosor", &mut m.thickness_mm, 0.3..=1.5);
        let mut inset = m.uniform_inset().unwrap_or(m.inset_left_mm);
        if length_field(ui, unit, "Inserción", &mut inset, 5.0..=60.0) {
            m.set_inset(inset);
        }
        let icon = if more {
            mi::ICON_ARROW_DROP_DOWN
        } else {
            mi::ICON_ARROW_RIGHT
        };
        if ui
            .add(
                egui::Button::new(
                    RichText::new(format!("{}  Más ajustes", <&str>::from(icon)))
                        .strong()
                        .color(theme::CHROME_100),
                )
                .frame(false),
            )
            .clicked()
        {
            more = !more;
        }
        if more {
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: 8,
                    right: 0,
                    top: 0,
                    bottom: 0,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                        if style_button(ui, w, !m.inverted, false, "Por defecto") {
                            m.inverted = false;
                        }
                        if style_button(ui, w, m.inverted, true, "Invertido") {
                            m.inverted = true;
                        }
                    });
                    length_field(
                        ui,
                        unit,
                        "Inserción izquierda",
                        &mut m.inset_left_mm,
                        5.0..=60.0,
                    );
                    length_field(
                        ui,
                        unit,
                        "Inserción superior",
                        &mut m.inset_top_mm,
                        5.0..=60.0,
                    );
                    length_field(
                        ui,
                        unit,
                        "Inserción derecha",
                        &mut m.inset_right_mm,
                        5.0..=60.0,
                    );
                    length_field(
                        ui,
                        unit,
                        "Inserción inferior",
                        &mut m.inset_bottom_mm,
                        5.0..=60.0,
                    );
                });
        }
        ui.add_space(4.0);
        if ui
            .add(
                egui::Button::new(format!(
                    "{}   Restaurar marcas por defecto",
                    <&str>::from(mi::ICON_UNDO)
                ))
                .min_size(EVec2::new(ui.available_width(), 30.0)),
            )
            .clicked()
        {
            m.reset_marks();
        }
    });
    if ui
        .add(
            egui::Button::new(format!("{}   Imprimir", <&str>::from(mi::ICON_PRINT)))
                .min_size(EVec2::new(ui.available_width(), 30.0)),
        )
        .on_hover_text(
            "Abre la página lista para imprimir (diseño y marcas, sin las líneas de corte)",
        )
        .clicked()
    {
        let ctx = ui.ctx().clone();
        app.print_page(&ctx);
    }
    ui.add_space(4.0);
    ui.separator();
    ui.checkbox(&mut m.bleed, "Activar sangrado de impresión");
    ui.add_enabled_ui(m.bleed, |ui| {
        length_field(ui, unit, "Radio de sangrado", &mut m.bleed_mm, 0.0..=5.0);
    });
    hint(
        ui,
        "El sangrado imprime los rellenos un poco más grandes que el corte para evitar bordes blancos.",
    );

    ui.add_space(4.0);
    ui.separator();
    section(ui, "Hoja de pegatinas");
    hint(
        ui,
        "Contorno de corte alrededor de cada imagen, copias acomodadas en la hoja y marcas de \
         registro activadas, listo para imprimir y cortar. Sin selección se usan todas las imágenes.",
    );
    let sid = egui::Id::unique("sticker_settings");
    let mut stk: StickerSettings = ui.data(|d| d.get_temp(sid)).unwrap_or_default();
    length_field(ui, unit, "Margen blanco", &mut stk.margin_mm, 0.0..=10.0);
    ui.horizontal(|ui| {
        let mut fill = stk.copies.is_none();
        if ui.checkbox(&mut fill, "Rellenar la hoja").changed() {
            stk.copies = if fill { None } else { Some(1) };
        }
        if let Some(n) = stk.copies.as_mut() {
            ui.add(
                egui::DragValue::new(n)
                    .range(1..=MAX_STICKERS)
                    .suffix(" copias"),
            );
        }
    });
    let images = app.sticker_images().len();
    let make = ui
        .add_enabled(
            images > 0,
            egui::Button::new(format!(
                "{}   Crear hoja de pegatinas",
                <&str>::from(mi::ICON_AUTO_AWESOME_MOSAIC)
            ))
            .min_size(EVec2::new(ui.available_width(), 32.0)),
        )
        .on_disabled_hover_text("Añade o selecciona una imagen")
        .clicked();
    ui.data_mut(|d| d.insert_temp(sid, stk));

    ui.data_mut(|d| d.insert_temp(more_id, more));
    if m != before {
        let dragging = ui.ctx().input(|i| i.pointer.any_down());
        let edit_id = egui::Id::unique("print_cut_editing");
        let editing: bool = ui.data(|d| d.get_temp(edit_id)).unwrap_or(false);
        if !editing {
            app.commit();
        }
        ui.data_mut(|d| d.insert_temp(edit_id, dragging));
        app.doc.page.reg_marks = m;
    } else if !ui.ctx().input(|i| i.pointer.any_down()) {
        ui.data_mut(|d| d.insert_temp(egui::Id::unique("print_cut_editing"), false));
    }
    if make {
        let ctx = ui.ctx().clone();
        let msg = app.make_sticker_sheet(stk.margin_mm, stk.copies);
        app.notify(&ctx, msg);
    }
}
