//! Panel PixScan, con las opciones, el orden y los avisos de la referencia:
//! vista previa · pestañas Cámara / Escáner · Abrir imagen PixScan (importar desde archivo,
//! tarjeta de prueba de calibración con su tamaño, lista de calibraciones con + y −) ·
//! Eliminar imagen PixScan (Todo, Superior, Inferior).
//!
//! Flujo de importación como la referencia: resolución → perfil de calibración → tapete. Como
//! la detección automática de las marcas del tapete no está disponible, las esquinas del
//! tapete se marcan sobre la foto en una ventana y la imagen se rectifica con ellas.

use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{Color32, Pos2, RichText, Sense, Stroke as EStroke, StrokeKind, Ui, Vec2 as EVec2};
use egui_material_icons::icons as mi;
use kurbo::Point;
use oc_trace::pixscan::{self, CalibrationProfile, Resolution};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Camera,
    Scanner,
}

/// Calibración elegida en la lista.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum CalChoice {
    None,
    #[default]
    Auto,
    Profile(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Cancel,
    ContinueLowRes,
    ContinueNoCal,
    Ok,
}

#[derive(Clone)]
struct Notice {
    text: String,
    buttons: Vec<(&'static str, Action)>,
}

/// Foto en curso de importación.
pub struct Pending {
    name: String,
    img: Arc<image::DynamicImage>,
    camera: Option<String>,
    calibration: Option<CalibrationProfile>,
    corners: [Point; 4],
}

#[derive(Default)]
pub struct PixScanState {
    pub tab: Tab,
    card_size: usize,
    calibration: CalChoice,
    pending: Option<Pending>,
    notice: Option<Notice>,
    status: Option<String>,
    /// Ventana para marcar las esquinas del tapete abierta.
    marking: bool,
    preview: Option<(String, egui::TextureHandle)>,
}

const MSG_TOO_SMALL: &str = "Resolución de imagen demasiado baja. Por favor, pruebe de nuevo con una imagen de mayor resolución. La resolución mínima soportada es 640x480. La resolución mínima recomendada es 1600x900.";
const MSG_LOW_RES: &str = "La resolución de esta imagen es baja y podría causar imprecisiones en la línea de corte. Para resultados óptimos, utilice una imagen con una resolución de 1600x900 o superior.";
const MSG_NO_PROFILE: &str = "No se ha podido encontrar ningún perfil de calibración que se ajuste a la fotografía que se está importando. En los casos en los que no se usa un perfil de calibración, la precisión de la línea de corte podría verse afectada negativamente.";
const MSG_PROFILE_MISMATCH: &str = "El perfil de calibración elegido no corresponde a la cámara o a la resolución de esta fotografía. La precisión de la línea de corte podría verse afectada negativamente.";
const MSG_CAL_FAILED: &str = "No se pudo importar la imagen de calibración PixScan. Por favor, tome de nuevo la fotografía asegurando que haya buena iluminación. Asegúrese de que la parrilla de puntos está aproximadamente alineada con la imagen.";
const MSG_CAL_QUALITY: &str = "La calidad de la calibración es insuficiente. Tome de nuevo la fotografía de la tarjeta de prueba con buena iluminación, enfocada y sin sombras.";

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(theme::CHROME_300));
}

fn wide(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(text).min_size(EVec2::new(ui.available_width(), 30.0)),
    )
}

impl OpenCraftCutterApp {
    fn pixscan_mat(&self) -> (f64, f64) {
        let page = &self.doc.page;
        let m = oc_core::machines::resolve_mat(page.machine, page.mat);
        if m.id == oc_core::machines::MatId::PixScanCameo {
            (m.width_mm, m.height_mm)
        } else {
            let p = oc_core::machines::resolve_mat(
                page.machine,
                oc_core::machines::MatId::PixScanCameo,
            );
            (p.width_mm, p.height_mm)
        }
    }

    fn pixscan_import_file(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Imágenes",
                &["jpg", "jpeg", "png", "tif", "tiff", "bmp", "webp"],
            )
            .pick_file()
        else {
            return;
        };
        self.pixscan_import_path(ctx, &path);
    }

    /// Importa una foto PixScan desde una ruta (también lo usa la demo de depuración).
    pub fn pixscan_import_path(&mut self, ctx: &egui::Context, path: &std::path::Path) {
        let st = &mut self.pixscan;
        st.notice = None;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(bytes) = std::fs::read(path) else {
            st.status = Some("No se pudo cargar imagen".into());
            return;
        };
        let Ok(img) = image::load_from_memory(&bytes) else {
            st.status = Some("No se pudo cargar imagen".into());
            return;
        };
        let (w, h) = (img.width(), img.height());
        // Miniatura para la vista previa del panel.
        let thumb = img.thumbnail(512, 512).to_rgba8();
        let tex = ctx.load_texture(
            "pixscan-preview",
            egui::ColorImage::from_rgba_unmultiplied(
                [thumb.width() as usize, thumb.height() as usize],
                thumb.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        st.preview = Some((name.clone(), tex));
        let (fw, fh) = (w as f64, h as f64);
        st.pending = Some(Pending {
            name,
            camera: pixscan::camera_model(&bytes),
            img: Arc::new(img),
            calibration: None,
            corners: [
                Point::new(fw * 0.08, fh * 0.08),
                Point::new(fw * 0.92, fh * 0.08),
                Point::new(fw * 0.92, fh * 0.92),
                Point::new(fw * 0.08, fh * 0.92),
            ],
        });
        match pixscan::check_resolution(w, h) {
            Resolution::TooSmall => {
                st.status = Some("No se pudo cargar imagen".into());
                st.notice = Some(Notice {
                    text: MSG_TOO_SMALL.into(),
                    buttons: vec![("Cancelar", Action::Cancel)],
                });
            }
            Resolution::Low => {
                st.notice = Some(Notice {
                    text: MSG_LOW_RES.into(),
                    buttons: vec![
                        ("Cancelar", Action::Cancel),
                        ("Continuar igualmente", Action::ContinueLowRes),
                    ],
                });
            }
            Resolution::Ok => self.pixscan_step_calibration(),
        }
    }

    fn pixscan_step_calibration(&mut self) {
        let profiles = self.prefs.pixscan_calibrations.clone();
        let st = &mut self.pixscan;
        let Some(p) = st.pending.as_mut() else { return };
        let (w, h) = (p.img.width(), p.img.height());
        match st.calibration {
            CalChoice::None => {
                p.calibration = None;
                st.status = Some("Calibración: Ninguna".into());
                st.marking = true;
            }
            CalChoice::Auto => match profiles
                .iter()
                .find(|c| c.matches(p.camera.as_deref(), w, h))
            {
                Some(c) => {
                    st.status = Some(format!("Calibración: {}", c.name));
                    p.calibration = Some(c.clone());
                    st.marking = true;
                }
                None => {
                    st.status = Some("Calibración: Ninguna".into());
                    st.notice = Some(Notice {
                        text: MSG_NO_PROFILE.into(),
                        buttons: vec![
                            ("Cancelar", Action::Cancel),
                            ("Continuar sin calibración", Action::ContinueNoCal),
                        ],
                    });
                }
            },
            CalChoice::Profile(i) => match profiles.get(i) {
                Some(c) => {
                    st.status = Some(format!("Calibración: {}", c.name));
                    p.calibration = Some(c.clone());
                    if c.matches(p.camera.as_deref(), w, h) {
                        st.marking = true;
                    } else {
                        st.notice = Some(Notice {
                            text: MSG_PROFILE_MISMATCH.into(),
                            buttons: vec![
                                ("Cancelar", Action::Cancel),
                                ("Continuar", Action::ContinueNoCal),
                            ],
                        });
                    }
                }
                None => st.marking = true,
            },
        }
    }

    fn pixscan_action(&mut self, a: Action) {
        let st = &mut self.pixscan;
        st.notice = None;
        match a {
            Action::Cancel => {
                st.pending = None;
                st.marking = false;
                st.status = Some("Error de importación".into());
            }
            Action::ContinueLowRes => self.pixscan_step_calibration(),
            // Con perfil que no encaja se conserva; sin perfil se continúa sin él.
            Action::ContinueNoCal => st.marking = true,
            Action::Ok => {}
        }
    }

    /// Rectifica la foto con las esquinas marcadas y la coloca sobre el tapete.
    fn pixscan_apply(&mut self, ctx: &egui::Context) {
        let (mw, mh) = self.pixscan_mat();
        let Some(p) = self.pixscan.pending.take() else {
            return;
        };
        self.pixscan.marking = false;
        // Resolución de salida: la de la foto, limitada a 4000 px en el lado mayor.
        let src_long = p.img.width().max(p.img.height()) as f64;
        let px_mm = (src_long / mw.max(mh)).min(4000.0 / mw.max(mh)).max(2.0);
        let (ow, oh) = ((mw * px_mm).round() as u32, (mh * px_mm).round() as u32);
        let Some(out) = pixscan::rectify(&p.img, p.corners, ow, oh, p.calibration.as_ref()) else {
            self.pixscan.status = Some("Error de importación".into());
            return;
        };
        let mut png = Vec::new();
        if image::DynamicImage::ImageRgba8(out)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .is_err()
        {
            self.pixscan.status = Some("Error de importación".into());
            return;
        }
        let kind = oc_core::shape::ShapeKind::Image {
            png: oc_core::shape::ImageData(Arc::new(png)),
            px_width: ow,
            px_height: oh,
        };
        let mut s = oc_core::Shape::new(
            0,
            format!("PixScan ({})", p.name),
            kurbo::Rect::new(0.0, 0.0, mw, mh),
            kind,
        );
        s.pixscan = true;
        s.locked = true;
        s.stroke.width_pt = 0.0;
        self.commit();
        // Como la referencia: tapete PixScan y página del tamaño del tapete.
        let page = &mut self.doc.page;
        page.mat = oc_core::machines::MatId::PixScanCameo;
        page.media_size = "custom".into();
        page.width_mm = mw;
        page.height_mm = mh;
        page.orientation = if mw > mh {
            oc_core::document::Orientation::Landscape
        } else {
            oc_core::document::Orientation::Portrait
        };
        let id = self.doc.add_shape(s);
        // Al fondo de la pila, bajo el diseño.
        if let Some(i) = self.doc.shapes.iter().position(|s| s.id == id) {
            let s = self.doc.shapes.remove(i);
            self.doc.shapes.insert(0, s);
        }
        self.pixscan.status = Some("Imagen PixScan colocada en el tapete".into());
        self.notify(
            ctx,
            "Imagen PixScan colocada en el tapete. Ya puedes trazarla o dibujar encima.",
        );
    }

    fn pixscan_add_calibration(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Foto de la tarjeta de prueba de calibración")
            .add_filter("Imágenes", &["jpg", "jpeg", "png", "tif", "tiff"])
            .pick_file()
        else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let stem = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = pixscan::camera_model(&bytes).unwrap_or(stem);
        match pixscan::calibrate(&bytes, &name) {
            Ok(prof) => {
                self.prefs.pixscan_calibrations.push(prof);
                self.pixscan.calibration =
                    CalChoice::Profile(self.prefs.pixscan_calibrations.len() - 1);
                self.pixscan.status = Some(format!("Calibración añadida: {name}"));
            }
            Err(e) => {
                let text = match e {
                    pixscan::CalibrationError::NotEnoughDots => MSG_CAL_FAILED,
                    pixscan::CalibrationError::Quality => MSG_CAL_QUALITY,
                };
                self.pixscan.notice = Some(Notice {
                    text: text.into(),
                    buttons: vec![("OK", Action::Ok)],
                });
            }
        }
    }

    /// Abre la tarjeta de prueba de calibración en una pestaña nueva.
    fn pixscan_show_card(&mut self) {
        let (label, w, h) =
            pixscan::CARD_SIZES[self.pixscan.card_size.min(pixscan::CARD_SIZES.len() - 1)];
        let mut doc = oc_core::Document::default();
        doc.name = format!("Tarjeta de calibración PixScan ({label})");
        doc.page = self.prefs.page_setup();
        doc.page.width_mm = w;
        doc.page.height_mm = h;
        doc.page.media_size = "custom".into();
        let r = pixscan::CARD_DOT_MM / 2.0;
        let circles: Vec<(Vec<Point>, bool)> = pixscan::card_dots(w, h)
            .into_iter()
            .map(|c| {
                let pts = (0..16)
                    .map(|i| {
                        let a = std::f64::consts::TAU * i as f64 / 16.0;
                        Point::new(c.x + r * a.cos(), c.y + r * a.sin())
                    })
                    .collect();
                (pts, true)
            })
            .collect();
        let (rect, subpaths) = oc_core::Shape::normalize_subpaths(&circles);
        let mut s = oc_core::Shape::new(
            0,
            "Tarjeta de calibración",
            rect,
            oc_core::ShapeKind::Path {
                subpaths,
                fill_rule: oc_core::FillRule::NonZero,
            },
        );
        s.fill = Some(oc_core::Color::rgb(0, 0, 0));
        s.locked = true;
        doc.add_shape(s);
        self.open_in_new_tab(doc, None);
    }

    fn pixscan_remove(&mut self, which: i32) {
        let ids: Vec<u64> = self
            .doc
            .shapes
            .iter()
            .filter(|s| s.pixscan)
            .map(|s| s.id)
            .collect();
        let target: Vec<u64> = match which {
            0 => ids,
            1 => ids.last().copied().into_iter().collect(),
            _ => ids.first().copied().into_iter().collect(),
        };
        if !target.is_empty() {
            self.commit();
            self.doc.remove_shapes(&target);
        }
    }
}

/// Vista previa: la foto en curso, o un dibujo del tapete en perspectiva.
fn preview(app: &OpenCraftCutterApp, ui: &mut Ui) {
    let size = EVec2::new(ui.available_width(), 190.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, theme::CHROME_900);
    p.rect_stroke(
        rect,
        4.0,
        EStroke::new(1.0, theme::CHROME_600),
        StrokeKind::Inside,
    );
    let title = app
        .pixscan
        .preview
        .as_ref()
        .map(|(n, _)| n.as_str())
        .unwrap_or("PixScan");
    p.text(
        rect.center_top() + EVec2::new(0.0, 12.0),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(11.0),
        theme::ACCENT,
    );
    if let Some((_, tex)) = &app.pixscan.preview {
        let area = rect
            .shrink2(EVec2::new(12.0, 26.0))
            .translate(EVec2::new(0.0, 8.0));
        let ts = tex.size_vec2();
        let k = (area.width() / ts.x).min(area.height() / ts.y);
        let r = egui::Rect::from_center_size(area.center(), ts * k);
        p.image(
            tex.id(),
            r,
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        return;
    }
    // Dibujo propio: tapete inclinado con esquinas y marcas.
    let c = rect.center() + EVec2::new(0.0, 8.0);
    let quad = [
        c + EVec2::new(-95.0, 10.0),
        c + EVec2::new(25.0, -55.0),
        c + EVec2::new(100.0, -5.0),
        c + EVec2::new(-25.0, 60.0),
    ];
    p.add(egui::Shape::convex_polygon(
        quad.to_vec(),
        Color32::from_gray(0xf2),
        EStroke::new(1.0, Color32::from_gray(0xb0)),
    ));
    let lerp = |a: Pos2, b: Pos2, t: f32| a + (b - a) * t;
    let mark = EStroke::new(1.2, Color32::from_gray(0x80));
    for (i, &corner) in quad.iter().enumerate() {
        let (prev, next) = (quad[(i + 3) % 4], quad[(i + 1) % 4]);
        let a = lerp(corner, lerp(corner, next, 0.12), 1.0);
        let b = lerp(corner, prev, 0.12);
        let inner = lerp(corner, lerp(prev, next, 0.5), 0.12);
        p.line_segment([lerp(inner, a, 0.0), lerp(a, inner, 0.0)], mark);
        p.line_segment([inner, lerp(inner, a, 0.6)], mark);
        p.line_segment([inner, lerp(inner, b, 0.6)], mark);
    }
    for t in [0.33f32, 0.66] {
        p.circle_filled(
            lerp(quad[0], quad[1], t) + (quad[3] - quad[0]) * 0.05,
            2.0,
            Color32::from_gray(0x80),
        );
        p.circle_filled(
            lerp(quad[3], quad[2], t) - (quad[3] - quad[0]) * 0.05,
            2.0,
            Color32::from_gray(0x80),
        );
    }
}

pub fn show(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    preview(app, ui);
    if let Some(s) = app.pixscan.status.clone() {
        ui.label(RichText::new(s).small().color(theme::CHROME_200));
    }
    if let Some(n) = app.pixscan.notice.clone() {
        egui::Frame::new()
            .fill(theme::CHROME_900)
            .stroke(EStroke::new(1.5, Color32::WHITE))
            .corner_radius(4.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&n.text).small().color(theme::CHROME_100));
            });
        let mut clicked = None;
        for (label, a) in &n.buttons {
            if wide(ui, label, true).clicked() {
                clicked = Some(*a);
            }
        }
        if let Some(a) = clicked {
            app.pixscan_action(a);
        }
    }
    ui.horizontal(|ui| {
        for (tab, icon, tip) in [
            (Tab::Camera, mi::ICON_PHOTO_CAMERA, "Cámara / archivo"),
            (Tab::Scanner, mi::ICON_SCANNER, "Escáner"),
        ] {
            if ui
                .selectable_label(app.pixscan.tab == tab, icon.rich_text().size(18.0))
                .on_hover_text(tip)
                .clicked()
            {
                app.pixscan.tab = tab;
            }
        }
    });
    ui.separator();
    super::section(ui, "Abrir imagen PixScan");
    let busy = app.pixscan.notice.is_some() || app.pixscan.marking;
    match app.pixscan.tab {
        Tab::Camera => {
            if wide(ui, "Importar imagen PixScan desde archivo", !busy).clicked() {
                let ctx = ui.ctx().clone();
                app.pixscan_import_file(&ctx);
            }
            ui.horizontal(|ui| {
                let w = ui.available_width() - 70.0;
                if ui
                    .add_sized(
                        [w, 34.0],
                        egui::Button::new(
                            RichText::new("Mostrar tarjeta de prueba de calibración").small(),
                        )
                        .wrap(),
                    )
                    .on_hover_text("Abre la tarjeta de puntos para imprimirla y fotografiarla")
                    .clicked()
                {
                    app.pixscan_show_card();
                }
                egui::ComboBox::from_id_salt("pixscan_card_size")
                    .width(48.0)
                    .selected_text(pixscan::CARD_SIZES[app.pixscan.card_size].0)
                    .show_ui(ui, |ui| {
                        for (i, (name, _, _)) in pixscan::CARD_SIZES.iter().enumerate() {
                            ui.selectable_value(&mut app.pixscan.card_size, i, *name);
                        }
                    });
            });
            // Lista de calibraciones.
            let mut remove = None;
            egui::Frame::new()
                .fill(theme::CHROME_900)
                .stroke(EStroke::new(1.0, theme::CHROME_600))
                .corner_radius(4.0)
                .inner_margin(2.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width() - 6.0);
                    ui.set_min_height(96.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    // Filas a todo el ancho, texto a la izquierda.
                    let row = |ui: &mut Ui, sel: bool, text: &str| {
                        let (rect, resp) = ui.allocate_exact_size(
                            EVec2::new(ui.available_width(), 24.0),
                            Sense::click(),
                        );
                        if sel {
                            ui.painter().rect_filled(rect, 0.0, theme::CHROME_600);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, 0.0, theme::CHROME_700);
                        }
                        ui.painter().text(
                            rect.left_center() + EVec2::new(8.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            text,
                            egui::FontId::proportional(13.0),
                            theme::CHROME_100,
                        );
                        resp.clicked()
                    };
                    if row(ui, app.pixscan.calibration == CalChoice::None, "Ninguna") {
                        app.pixscan.calibration = CalChoice::None;
                    }
                    if row(ui, app.pixscan.calibration == CalChoice::Auto, "Auto") {
                        app.pixscan.calibration = CalChoice::Auto;
                    }
                    for (i, c) in app.prefs.pixscan_calibrations.iter().enumerate() {
                        let label = format!("{}  ·  {}×{}", c.name, c.width, c.height);
                        if row(ui, app.pixscan.calibration == CalChoice::Profile(i), &label) {
                            app.pixscan.calibration = CalChoice::Profile(i);
                        }
                    }
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 56.0).max(0.0));
                        let is_profile = matches!(app.pixscan.calibration, CalChoice::Profile(_));
                        if ui
                            .add(egui::Button::new(mi::ICON_ADD.rich_text()).frame(false))
                            .on_hover_text(
                                "Añadir calibración desde la foto de la tarjeta de prueba",
                            )
                            .clicked()
                        {
                            app.pixscan_add_calibration();
                        }
                        if ui
                            .add_enabled(
                                is_profile,
                                egui::Button::new(mi::ICON_REMOVE.rich_text()).frame(false),
                            )
                            .on_hover_text("Quitar la calibración")
                            .clicked()
                        {
                            remove = Some(());
                        }
                    });
                });
            if remove.is_some() {
                if let CalChoice::Profile(i) = app.pixscan.calibration {
                    if i < app.prefs.pixscan_calibrations.len() {
                        app.prefs.pixscan_calibrations.remove(i);
                    }
                    app.pixscan.calibration = CalChoice::Auto;
                }
            }
        }
        Tab::Scanner => {
            hint(
                ui,
                "Conecte y encienda su escáner para que se muestre en la lista a continuación.",
            );
            egui::Frame::new()
                .fill(theme::CHROME_900)
                .stroke(EStroke::new(1.0, theme::CHROME_600))
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(140.0);
                });
            wide(ui, "Importar imagen PixScan desde escáner", false).on_disabled_hover_text(
                "La captura directa desde escáner aún no está disponible: escanea a un archivo e impórtalo desde la pestaña Cámara.",
            );
        }
    }
    ui.add_space(2.0);
    ui.separator();
    super::section(ui, "Eliminar imagen PixScan");
    let any = app.doc.shapes.iter().any(|s| s.pixscan);
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0;
        for (i, (icon, label)) in [
            (mi::ICON_DELETE_SWEEP, "Todo"),
            (mi::ICON_VERTICAL_ALIGN_TOP, "Superior"),
            (mi::ICON_VERTICAL_ALIGN_BOTTOM, "Inferior"),
        ]
        .into_iter()
        .enumerate()
        {
            if ui
                .add_enabled(
                    any,
                    egui::Button::new(format!("{}  {label}", <&str>::from(icon)))
                        .min_size(EVec2::new(w, 28.0)),
                )
                .clicked()
            {
                app.pixscan_remove(i as i32);
            }
        }
    });
}

/// Ventana para marcar las cuatro esquinas del tapete en la foto.
pub fn corners_window(app: &mut OpenCraftCutterApp, ctx: &egui::Context) {
    if !app.pixscan.marking || app.pixscan.pending.is_none() {
        return;
    }
    let tex_id = egui::Id::unique("pixscan_full_tex");
    let mut apply = false;
    let mut cancel = false;
    egui::Window::new("Esquinas del tapete PixScan")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, EVec2::ZERO)
        .show(ctx, |ui| {
            let Some(p) = app.pixscan.pending.as_mut() else { return };
            ui.label(RichText::new("No se han detectado las marcas PixScan automáticamente. Arrastra los cuatro puntos hasta las esquinas de la cuadrícula del tapete (1 arriba a la izquierda, en el sentido de las agujas del reloj).").small().color(theme::CHROME_200));
            // Textura de la foto (reducida a 1600 px para la pantalla).
            let tex: egui::TextureHandle = match ui.data(|d| d.get_temp::<(String, egui::TextureHandle)>(tex_id)) {
                Some((n, t)) if n == p.name => t,
                _ => {
                    let small = p.img.thumbnail(1600, 1600).to_rgba8();
                    let t = ctx.load_texture(
                        "pixscan-full",
                        egui::ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw()),
                        egui::TextureOptions::LINEAR,
                    );
                    ui.data_mut(|d| d.insert_temp(tex_id, (p.name.clone(), t.clone())));
                    t
                }
            };
            let (iw, ih) = (p.img.width() as f32, p.img.height() as f32);
            let avail = ctx.content_rect().size() * EVec2::new(0.7, 0.62);
            let k = (avail.x / iw).min(avail.y / ih);
            let (rect, _) = ui.allocate_exact_size(EVec2::new(iw * k, ih * k), Sense::hover());
            ui.painter().image(tex.id(), rect, egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            let to_screen = |q: Point| rect.min + EVec2::new(q.x as f32 * k, q.y as f32 * k);
            let pts: Vec<Pos2> = p.corners.iter().map(|q| to_screen(*q)).collect();
            ui.painter().add(egui::Shape::closed_line(pts.clone(), EStroke::new(2.0, theme::ACCENT)));
            for (i, &pt) in pts.iter().enumerate() {
                let id = egui::Id::unique("pixscan_corner").with(i);
                let hit = egui::Rect::from_center_size(pt, EVec2::splat(22.0));
                let r = ui.interact(hit, id, Sense::drag());
                if r.dragged() {
                    let d = r.drag_delta() / k;
                    let c = &mut p.corners[i];
                    c.x = (c.x + d.x as f64).clamp(0.0, iw as f64);
                    c.y = (c.y + d.y as f64).clamp(0.0, ih as f64);
                }
                let hot = r.hovered() || r.dragged();
                ui.painter().circle(pt, if hot { 9.0 } else { 7.0 }, Color32::WHITE, EStroke::new(2.0, theme::ACCENT));
                ui.painter().text(pt, egui::Align2::CENTER_CENTER, format!("{}", i + 1), egui::FontId::proportional(10.0), Color32::BLACK);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Cancelar").clicked() {
                    cancel = true;
                }
                if ui.button(RichText::new("Colocar en el tapete").strong()).clicked() {
                    apply = true;
                }
            });
        });
    if cancel {
        app.pixscan.marking = false;
        app.pixscan.pending = None;
        app.pixscan.status = Some("Error de importación".into());
    }
    if apply {
        app.pixscan_apply(ctx);
    }
}
