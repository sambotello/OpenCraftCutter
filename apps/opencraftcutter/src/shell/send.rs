//! Sección Enviar, con la estructura de la referencia: Guía, Estándar, por color de línea y por
//! color de relleno; materiales con ajustes por portaherramientas y acción; opciones generales;
//! forma de prueba, con sitio elegido en el tapete y registro de intentos.

use crate::app::OpenCraftCutterApp;
use crate::theme;
use egui::{Color32, Frame, RichText, Ui};
use egui_material_icons::icons as mi;
use oc_core::Color;
use oc_core::send::{
    self, Action, ActionSettings, ColorRow, CutOrder, Family, FeedOption, Holder, HolderSetup, Job,
    Material, PunchOrder, SendMode, SendOptions, ShowBlade, Tool,
};

/// Estado de la sección Enviar (no se guarda con el documento).
pub struct SendSettings {
    pub mode: SendMode,
    /// Pestaña de portaherramientas del modo Estándar.
    pub holder_tab: Holder,
    pub setups: [HolderSetup; 2],
    pub line_rows: Vec<(Color, ColorRow)>,
    pub fill_rows: Vec<(Color, ColorRow)>,
    pub selected_row: Option<usize>,
    /// «Guardar como…» desde los ajustes propios de una fila: (por relleno, fila, ajustes).
    pub row_save: Option<(bool, usize, Holder, Action, ActionSettings)>,
    pub guide_step: usize,
    /// El usuario eligió «Probar o modificar» en el resumen del asistente.
    pub guide_modify: bool,
    /// «Configuración de materiales» abierta.
    pub editor: Option<Editor>,
    pub show_help: bool,
    pub hide_banner: bool,
    /// Diálogo de nombre (material nuevo o «Guardar como…»).
    pub name_dialog: Option<NameDialog>,
    /// Material tal como estaba antes de cambiar sus ajustes desde el panel (para «Deshacer»).
    pub inline_backup: Option<Material>,
    /// Ya se actualizó la lista de materiales guardada.
    migrated: bool,
    /// Esperando un clic en el tapete para colocar la forma de prueba.
    pub placing_test: bool,
    /// Cortes de prueba enviados en esta sesión, del primero al último.
    pub test_attempts: Vec<send::TestAttempt>,
    /// Color de la fila con la que se corta la prueba en los modos por color.
    pub test_color: Option<Color>,
    /// «Guardar como material» pedido desde el intento con este índice.
    pub test_save: Option<usize>,
    /// Reflejar el diseño al cortar: `None` = automático (vinil textil).
    pub mirror: Option<bool>,
    /// Simulación del corte en curso sobre la vista previa.
    pub simulation: Option<Simulation>,
    /// Ventana «Historial de cortes y cuchillas» abierta.
    pub show_history: bool,
    /// Detalles demasiado finos de la última vista previa (huella de las líneas, mínimo).
    thin_cache: std::cell::RefCell<Option<(u64, std::sync::Arc<oc_core::ops::Polygons>)>>,
}

/// Simulación del recorrido de la cuchilla.
pub struct Simulation {
    pub moves: std::sync::Arc<Vec<send::ToolMove>>,
    pub total_s: f64,
    /// Momento (reloj de egui) que corresponde al segundo 0 del corte.
    pub started: f64,
    /// Veces más rápido que el corte real.
    pub speed: f64,
}

impl Simulation {
    /// Segundo del corte que se está mostrando.
    pub fn at(&self, now: f64) -> f64 {
        ((now - self.started) * self.speed).clamp(0.0, self.total_s)
    }
}

/// Editor de materiales: material, portaherramientas y acción a la vista, y copia para «Deshacer».
pub struct Editor {
    pub material: usize,
    pub holder: Holder,
    pub action: Action,
    pub backup: Material,
}

pub struct NameDialog {
    pub name: String,
    pub family: Family,
    /// `Some(i)`: «Guardar como…» a partir del material `i`; `None`: material nuevo.
    pub from: Option<usize>,
}

impl Default for SendSettings {
    fn default() -> Self {
        let first = send::DEFAULT_MATERIAL.to_string();
        SendSettings {
            mode: SendMode::Standard,
            holder_tab: Holder::One,
            setups: [
                HolderSetup {
                    material: first.clone(),
                    action: Action::Cut,
                    tool: Tool::AutoBlade,
                },
                HolderSetup {
                    material: first,
                    action: Action::Auto,
                    tool: Tool::SketchPen,
                },
            ],
            line_rows: Vec::new(),
            fill_rows: Vec::new(),
            selected_row: None,
            row_save: None,
            guide_step: 0,
            guide_modify: false,
            editor: None,
            show_help: false,
            hide_banner: false,
            name_dialog: None,
            inline_backup: None,
            migrated: false,
            placing_test: false,
            test_attempts: Vec::new(),
            test_color: None,
            test_save: None,
            mirror: None,
            simulation: None,
            show_history: false,
            thin_cache: Default::default(),
        }
    }
}

/// Rojo del portaherramientas 1 y azul del 2 (como las líneas de corte de la vista previa).
pub const HOLDER1: Color32 = Color32::from_rgb(0xe3, 0x34, 0x3f);
pub const HOLDER2: Color32 = Color32::from_rgb(0x2f, 0x5b, 0xe8);

pub fn holder_color(h: Holder) -> Color32 {
    match h {
        Holder::One => HOLDER1,
        Holder::Two => HOLDER2,
    }
}

impl OpenCraftCutterApp {
    /// Materiales (los incluidos si el usuario aún no tiene lista propia).
    pub fn materials_mut(&mut self) -> &mut Vec<Material> {
        if self.prefs.materials.is_empty()
            || self.prefs.materials_version < send::BUILTIN_MATERIALS_VERSION
        {
            let custom: Vec<Material> = self
                .prefs
                .materials
                .iter()
                .filter(|m| m.custom)
                .cloned()
                .collect();
            self.prefs.materials = send::builtin_materials();
            self.prefs.materials.extend(custom);
            self.prefs.materials_version = send::BUILTIN_MATERIALS_VERSION;
        }
        // Listas guardadas con versiones anteriores: los incluidos ganan esbozar y punteado.
        if !self.canvas.send.migrated {
            self.canvas.send.migrated = true;
            for m in self.prefs.materials.iter_mut().filter(|m| !m.custom) {
                if m.settings(Holder::One, Action::Sketch).is_none() {
                    m.add_action(Holder::One, Action::Sketch);
                    m.add_action(Holder::One, Action::Stipple);
                }
            }
        }
        &mut self.prefs.materials
    }

    /// Colores de línea o de relleno presentes en las formas que se envían.
    fn send_colors(&self, by_fill: bool) -> Vec<Color> {
        let mut v: Vec<Color> = Vec::new();
        for s in self
            .doc
            .shapes
            .iter()
            .filter(|s| s.visible && !s.is_image())
        {
            let c = if by_fill {
                s.fill.unwrap_or(send::NO_FILL)
            } else {
                s.stroke.color
            };
            if !v.contains(&c) {
                v.push(c);
            }
        }
        v
    }

    /// Mantiene una fila por color presente (conserva los ajustes de los que siguen) y que cada
    /// herramienta sea compatible con su portaherramientas y acción.
    pub(crate) fn sync_rows(&mut self) {
        let mats = &self.prefs.materials;
        let fix = |holder: Holder, material: &str, action: Action, tool: &mut Tool| {
            let m = mats.iter().find(|m| m.name == material);
            let action = send::pick_action(m, holder, action, *tool);
            if !tool.compatible(holder, action) {
                if let Some(t) = Tool::for_action(holder, action).first() {
                    *tool = *t;
                }
            }
        };
        for (i, h) in Holder::ALL.into_iter().enumerate() {
            let st = &mut self.canvas.send.setups[i];
            fix(h, &st.material, st.action, &mut st.tool);
        }
        for (_, r) in self
            .canvas
            .send
            .line_rows
            .iter_mut()
            .chain(self.canvas.send.fill_rows.iter_mut())
        {
            fix(r.holder, &r.material, r.action, &mut r.tool);
        }
        let base = self.canvas.send.setups[0].clone();
        for by_fill in [false, true] {
            let colors = self.send_colors(by_fill);
            let rows = if by_fill {
                &mut self.canvas.send.fill_rows
            } else {
                &mut self.canvas.send.line_rows
            };
            rows.retain(|(c, _)| colors.contains(c));
            for c in colors {
                if !rows.iter().any(|(rc, _)| *rc == c) {
                    rows.push((
                        c,
                        ColorRow {
                            settings: None,
                            holder: Holder::One,
                            enabled: true,
                            edge_only: false,
                            material: base.material.clone(),
                            action: Action::Auto,
                            tool: base.tool,
                            pause_after: false,
                        },
                    ));
                }
            }
        }
    }

    /// Documento tal como se corta: los textos pasan a trazado (el contorno de sus letras)
    /// conservando cómo se asignan al corte, su grupo y sus colores.
    pub(crate) fn cut_doc(&self) -> std::borrow::Cow<'_, oc_core::Document> {
        let mirror = self.mirror_on_cut();
        if !mirror && !self.doc.shapes.iter().any(|s| s.is_text()) {
            return std::borrow::Cow::Borrowed(&self.doc);
        }
        let mut doc = self.doc.clone();
        for s in doc.shapes.iter_mut().filter(|s| s.is_text()) {
            if let Some(mut p) = crate::text::text_to_path(s) {
                p.id = s.id;
                p.name = s.name.clone();
                p.cut = s.cut;
                p.group = s.group;
                p.inner_groups = s.inner_groups.clone();
                *s = p;
            }
        }
        // Espejo respecto al centro de la página; la forma de prueba se queda donde está.
        if mirror {
            let test_ids = send::test_shape_ids(&doc);
            let axis = doc.page.width_mm / 2.0;
            for s in doc.shapes.iter_mut().filter(|s| !test_ids.contains(&s.id)) {
                *s = oc_core::edit::mirror_shape(s, true, axis);
            }
        }
        std::borrow::Cow::Owned(doc)
    }

    /// Materiales elegidos en los portaherramientas o en las filas de color activas.
    fn materials_in_use(&self) -> Vec<&Material> {
        let s = &self.canvas.send;
        let names: Vec<&String> = match s.mode {
            SendMode::Guide | SendMode::Standard => s.setups.iter().map(|h| &h.material).collect(),
            SendMode::ByLine => s
                .line_rows
                .iter()
                .filter(|(_, r)| r.enabled)
                .map(|(_, r)| &r.material)
                .collect(),
            SendMode::ByFill => s
                .fill_rows
                .iter()
                .filter(|(_, r)| r.enabled)
                .map(|(_, r)| &r.material)
                .collect(),
        };
        names
            .into_iter()
            .filter_map(|n| self.prefs.materials.iter().find(|m| &m.name == n))
            .collect()
    }

    /// Herramientas elegidas en los portaherramientas o en las filas de color activas.
    fn tools_in_use(&self) -> Vec<Tool> {
        let s = &self.canvas.send;
        match s.mode {
            SendMode::Guide | SendMode::Standard => s.setups.iter().map(|h| h.tool).collect(),
            SendMode::ByLine => s
                .line_rows
                .iter()
                .filter(|(_, r)| r.enabled)
                .map(|(_, r)| r.tool)
                .collect(),
            SendMode::ByFill => s
                .fill_rows
                .iter()
                .filter(|(_, r)| r.enabled)
                .map(|(_, r)| r.tool)
                .collect(),
        }
    }

    /// Aviso si alguna cuchilla en uso pasa del recordatorio de cambio.
    pub fn blade_warning(&self) -> Option<String> {
        let tools = self.tools_in_use();
        let worn: Vec<String> = oc_core::usage::worn(&self.prefs.blades, self.prefs.blade_life_m)
            .into_iter()
            .filter(|b| tools.contains(&b.tool))
            .map(|b| {
                format!(
                    "La {} lleva {} m cortados",
                    b.tool.name().to_lowercase(),
                    format!("{:.1}", b.meters).replace('.', ",")
                )
            })
            .collect();
        (!worn.is_empty()).then(|| {
            format!(
                "{}. Si los cortes empiezan a fallar, cámbiala y márcalo en «Historial de cortes y cuchillas».",
                worn.join("; ")
            )
        })
    }

    /// Primer material en uso que se corta en espejo (vinil textil), si lo hay.
    pub fn mirrored_material(&self) -> Option<String> {
        self.materials_in_use()
            .into_iter()
            .find(|m| m.cut_mirrored())
            .map(|m| m.name.clone())
    }

    /// Anchura mínima de los detalles con los materiales en uso (la más exigente) y el
    /// material que la pide.
    pub fn min_detail(&self) -> (f64, String) {
        self.materials_in_use()
            .into_iter()
            .map(|m| (m.family.min_detail_mm(), m.name.clone()))
            .fold((0.0, String::new()), |a, b| if b.0 > a.0 { b } else { a })
    }

    /// Zonas del diseño más finas que el mínimo del material (se recalcula solo si cambian las
    /// líneas que se cortan).
    pub fn thin_details(&self) -> std::sync::Arc<oc_core::ops::Polygons> {
        use std::hash::{Hash, Hasher};
        let (min, _) = self.min_detail();
        // Forma por forma (lo que se corta de cada una): las letras o piezas que se tocan no
        // se mezclan.
        let test_ids = send::test_shape_ids(&self.doc);
        let doc = self.cut_doc();
        let mut one = (*doc).clone();
        let per_shape: Vec<Vec<Vec<[f64; 2]>>> = doc
            .shapes
            .iter()
            .filter(|s| !test_ids.contains(&s.id))
            .map(|s| {
                one.shapes = vec![s.clone()];
                self.preview_of(&one)
                    .iter()
                    .flat_map(|(_, paths)| paths.iter())
                    .filter(|(p, closed)| *closed && p.len() >= 3)
                    .map(|(p, _)| p.iter().map(|q| [q.x, q.y]).collect())
                    .collect()
            })
            .collect();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        min.to_bits().hash(&mut h);
        for contours in &per_shape {
            contours.len().hash(&mut h);
            for c in contours {
                c.len().hash(&mut h);
                for q in c {
                    q[0].to_bits().hash(&mut h);
                    q[1].to_bits().hash(&mut h);
                }
            }
        }
        let key = h.finish();
        let cache = &self.canvas.send.thin_cache;
        if let Some((k, v)) = cache.borrow().as_ref() {
            if *k == key {
                return v.clone();
            }
        }
        let v: std::sync::Arc<oc_core::ops::Polygons> = std::sync::Arc::new(
            per_shape
                .iter()
                .flat_map(|c| oc_core::details::thin_parts(c, min))
                .collect(),
        );
        *cache.borrow_mut() = Some((key, v.clone()));
        v
    }

    /// Aviso de detalles demasiado finos para el material.
    pub fn thin_warning(&self) -> Option<String> {
        let n = self.thin_details().len();
        if n == 0 {
            return None;
        }
        let (min, mat) = self.min_detail();
        let mm = format!("{min:.1}").replace('.', ",");
        let what = if n == 1 {
            "1 detalle es más fino".to_string()
        } else {
            format!("{n} detalles son más finos")
        };
        Some(format!(
            "{what} que {mm} mm, el mínimo para «{mat}»: pueden romperse al cortar o al pelar. Están marcados en rojo en la vista previa."
        ))
    }

    /// ¿Se refleja el diseño al cortar? Automático con vinil textil, salvo que el usuario diga
    /// otra cosa.
    pub fn mirror_on_cut(&self) -> bool {
        self.canvas
            .send
            .mirror
            .unwrap_or_else(|| self.mirrored_material().is_some())
    }

    /// Motivo por el que ENVIAR no cortaría nada (sin contar la forma de prueba), o `None`.
    pub fn nothing_to_cut(&self) -> Option<&'static str> {
        let test_ids = send::test_shape_ids(&self.doc);
        let design: Vec<&oc_core::Shape> = self
            .doc
            .shapes
            .iter()
            .filter(|s| s.visible && !test_ids.contains(&s.id))
            .collect();
        if design.is_empty() {
            return Some(
                "No hay nada que cortar: el documento está vacío. Dibuja o abre un diseño.",
            );
        }
        if design.iter().all(|s| s.is_image()) {
            return Some(
                "Las imágenes no se cortan: trázalas primero en el panel Trazar para crear líneas de corte.",
            );
        }
        let any = self
            .design_preview()
            .iter()
            .any(|(_, paths)| paths.iter().any(|(p, _)| p.len() >= 2));
        (!any).then_some(
            "No hay nada que cortar: todas las líneas están omitidas o sus colores en Off. Revisa la asignación de corte.",
        )
    }

    /// Líneas que se cortarían, sin la forma de prueba.
    fn design_preview(&self) -> Vec<send::HolderPaths> {
        let test_ids = send::test_shape_ids(&self.doc);
        let mut doc = self.cut_doc().into_owned();
        doc.shapes.retain(|s| !test_ids.contains(&s.id));
        self.preview_of(&doc)
    }

    fn preview_of(&self, doc: &oc_core::Document) -> Vec<send::HolderPaths> {
        let rows = &self.canvas.send;
        match rows.mode {
            SendMode::Guide | SendMode::Standard => send::standard_preview(doc),
            SendMode::ByLine => send::color_preview(doc, false, &rows.line_rows),
            SendMode::ByFill => send::color_preview(doc, true, &rows.fill_rows),
        }
    }

    /// Imprimir y cortar: aviso si hay líneas fuera del marco entre marcas o sobre ellas.
    pub fn marks_warning(&self) -> Option<String> {
        let page = &self.doc.page;
        if !page.reg_marks.enabled {
            return None;
        }
        let preview = self.design_preview();
        let c = page.reg_marks.conflicts(
            page.width_mm,
            page.height_mm,
            preview
                .iter()
                .flat_map(|(_, paths)| paths.iter().map(|(p, _)| p.as_slice())),
        );
        let lines = |n: usize| {
            if n == 1 {
                "1 línea".to_string()
            } else {
                format!("{n} líneas")
            }
        };
        let mut parts = Vec::new();
        if c.outside > 0 {
            parts.push(format!(
                "{} {} fuera del área entre las marcas de registro: la máquina no las cortará.",
                lines(c.outside),
                if c.outside == 1 { "queda" } else { "quedan" }
            ));
        }
        if c.near_marks > 0 {
            parts.push(format!(
                "{} {} por la zona de las marcas: pueden estorbar la lectura.",
                lines(c.near_marks),
                if c.near_marks == 1 { "pasa" } else { "pasan" }
            ));
        }
        (!parts.is_empty()).then(|| {
            parts.push("Mueve el diseño dentro del marco, fuera de las zonas rayadas.".into());
            parts.join(" ")
        })
    }

    /// Trabajos que se enviarían con los ajustes actuales.
    pub fn send_jobs(&mut self) -> Vec<Job> {
        let options = self.prefs.send_options;
        self.materials_mut();
        let doc = self.cut_doc();
        let s = &self.canvas.send;
        let mats = &self.prefs.materials;
        match s.mode {
            SendMode::Guide | SendMode::Standard => {
                send::standard_jobs(&doc, &s.setups, mats, &options)
            }
            SendMode::ByLine => send::color_jobs(&doc, false, &s.line_rows, mats, &options),
            SendMode::ByFill => send::color_jobs(&doc, true, &s.fill_rows, mats, &options),
        }
    }

    /// Líneas de la vista previa por portaherramientas.
    pub fn send_preview(&self) -> Vec<send::HolderPaths> {
        let doc = self.cut_doc();
        let s = &self.canvas.send;
        match s.mode {
            SendMode::Guide | SendMode::Standard => send::standard_preview(&doc),
            SendMode::ByLine => send::color_preview(&doc, false, &s.line_rows),
            SendMode::ByFill => send::color_preview(&doc, true, &s.fill_rows),
        }
    }

    fn toggle_test_shape(&mut self, ctx: &egui::Context) {
        let ids = send::test_shape_ids(&self.doc);
        if ids.is_empty() {
            // En la esquina del área de corte, o junto al último intento.
            let area = send::test_area(&self.doc);
            let origin = match self.canvas.send.test_attempts.last() {
                Some(a) => send::next_test_origin(a.origin, area),
                None => kurbo::Point::new(area.x0 + 5.0, area.y0 + 5.0),
            };
            self.place_test_shape(origin);
            self.notify(
                ctx,
                "Forma de prueba añadida. Con «Elegir sitio» puedes moverla a un trozo libre del material.",
            );
        } else {
            self.commit();
            self.doc.remove_shapes(&ids);
            self.canvas.send.placing_test = false;
        }
    }

    /// Esquina de la forma de prueba del documento.
    pub fn test_shape_origin(&self) -> Option<kurbo::Point> {
        let ids = send::test_shape_ids(&self.doc);
        self.doc
            .shapes
            .iter()
            .find(|s| ids.contains(&s.id))
            .map(|s| s.rect.origin())
    }

    /// Pone la forma de prueba con su esquina en `origin` (sustituye a la que hubiera).
    pub fn place_test_shape(&mut self, origin: kurbo::Point) {
        self.commit();
        let ids = send::test_shape_ids(&self.doc);
        self.doc.remove_shapes(&ids);
        for s in send::test_shapes(origin) {
            self.doc.add_shape(s);
        }
        self.apply_test_color();
    }

    /// Clic en el tapete con «Elegir sitio»: la prueba se centra en el punto, dentro del área.
    pub fn place_test_shape_at(&mut self, p: kurbo::Point) {
        let origin = send::test_origin_at(p, send::test_area(&self.doc));
        self.place_test_shape(origin);
        self.canvas.send.placing_test = false;
    }

    /// En los modos por color, la prueba toma el color de la fila elegida para que se corte con
    /// sus ajustes; en el resto vuelve a su rojo sin relleno.
    pub fn apply_test_color(&mut self) {
        let mode = self.canvas.send.mode;
        let color = self.canvas.send.test_color;
        let ids = send::test_shape_ids(&self.doc);
        for s in self.doc.shapes.iter_mut().filter(|s| ids.contains(&s.id)) {
            let red = send::test_shapes(kurbo::Point::ZERO)[0].stroke.color;
            match (mode, color) {
                (SendMode::ByLine, Some(c)) => {
                    s.stroke.color = c;
                    s.fill = None;
                }
                (SendMode::ByFill, Some(c)) => {
                    s.stroke.color = red;
                    s.fill = (c != send::NO_FILL).then_some(c);
                }
                _ => {
                    s.stroke.color = red;
                    s.fill = None;
                }
            }
        }
    }

    /// Al terminar un corte de prueba: se apunta el intento y la forma pasa al sitio siguiente,
    /// para que la próxima prueba no corte encima.
    pub fn finish_test_attempt(&mut self, attempt: send::TestAttempt) {
        let next = send::next_test_origin(attempt.origin, send::test_area(&self.doc));
        self.canvas.send.test_attempts.push(attempt);
        if self.test_shape_origin().is_some() {
            self.place_test_shape(next);
        }
    }

    /// «Guardar como material» desde un intento: abre el diálogo de nombre partiendo de su
    /// material.
    pub fn save_test_attempt(&mut self, i: usize) {
        let Some(a) = self.canvas.send.test_attempts.get(i).cloned() else {
            return;
        };
        let from = self
            .materials_mut()
            .iter()
            .position(|m| m.name == a.material);
        let family = from
            .map(|j| self.prefs.materials[j].family)
            .unwrap_or(Family::Vinyl);
        self.canvas.send.test_save = Some(i);
        self.canvas.send.name_dialog = Some(NameDialog {
            name: format!("{} (prueba {})", a.material, a.number),
            family,
            from,
        });
    }
}

/// Alto común de los botones de la barra de modos.
const MODE_H: f32 = 30.0;

fn mode_button(ui: &mut Ui, selected: bool, text: RichText, tip: &str) -> bool {
    ui.add(egui::Button::selectable(selected, text).min_size(egui::vec2(MODE_H, MODE_H)))
        .on_hover_text(tip)
        .clicked()
}

fn mode_bar(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let s = &mut app.canvas.send;
        let modes = [
            (
                SendMode::Guide,
                RichText::new("Guía"),
                "Asistente paso a paso",
            ),
            (
                SendMode::Standard,
                RichText::new("Estándar"),
                "Ajustes por portaherramientas",
            ),
            (
                SendMode::ByLine,
                mi::ICON_LINE_STYLE.rich_text().size(18.0),
                "Cortar por color de línea",
            ),
            (
                SendMode::ByFill,
                mi::ICON_FORMAT_COLOR_FILL.rich_text().size(18.0),
                "Cortar por color de relleno",
            ),
        ];
        for (mode, text, tip) in modes {
            if mode_button(ui, s.mode == mode, text, tip) {
                s.mode = mode;
                s.selected_row = None;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let gear = ui
                .add(
                    egui::Button::new(mi::ICON_SETTINGS.rich_text().size(18.0))
                        .min_size(egui::vec2(MODE_H, MODE_H)),
                )
                .on_hover_text("Opciones de envío");
            egui::Popup::menu(&gear).show(|ui| options_menu(app, ui));
        });
    });
}

fn radio<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, v: T, label: &str) {
    if ui.radio(*value == v, label).clicked() {
        *value = v;
    }
}

fn options_menu(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    ui.set_min_width(280.0);
    let o: &mut SendOptions = &mut app.prefs.send_options;
    let head = |ui: &mut Ui, t: &str| {
        ui.label(RichText::new(t).small().color(theme::CHROME_300));
    };
    head(ui, "Opciones de alimentación");
    radio(
        ui,
        &mut o.feed,
        FeedOption::ReturnToOrigin,
        "Regresar al origen",
    );
    radio(
        ui,
        &mut o.feed,
        FeedOption::FeedOut,
        "Hacer avanzar el material",
    );
    ui.separator();
    head(ui, "Cortadores en la lista");
    radio(ui, &mut o.all_cutters, false, "Cortadores conocidos");
    radio(ui, &mut o.all_cutters, true, "Todos los cortadores");
    ui.separator();
    head(ui, "Mostrar ajustes de la cuchilla");
    radio(ui, &mut o.show_blade, ShowBlade::Auto, "Auto");
    radio(ui, &mut o.show_blade, ShowBlade::Always, "Siempre");
    radio(ui, &mut o.show_blade, ShowBlade::Never, "Nunca");
    ui.separator();
    head(ui, "Orden de los trabajos de corte");
    radio(ui, &mut o.order, CutOrder::NoOptimize, "No optimizar");
    radio(ui, &mut o.order, CutOrder::MaxSpeed, "Maximizar velocidad");
    radio(
        ui,
        &mut o.order,
        CutOrder::MinRollerMovement,
        "Minimizar movimiento de rodillo",
    );
    ui.checkbox(&mut o.inner_first, "Ordenar por contornos interiores");
    ui.separator();
    head(ui, "Orden del punteado");
    radio(
        ui,
        &mut o.punch_order,
        PunchOrder::Default,
        "Predeterminado",
    );
    radio(
        ui,
        &mut o.punch_order,
        PunchOrder::EdgeToCenter,
        "Del borde al centro",
    );
    radio(
        ui,
        &mut o.punch_order,
        PunchOrder::CenterToEdge,
        "Del centro al borde",
    );
    ui.separator();
    if ui
        .button(RichText::new("Restaurar materiales predeterminados").color(Color32::from_rgb(0xff, 0x9a, 0x7a)))
        .on_hover_text("Recupera los valores de los materiales incluidos. Los materiales propios se conservan.")
        .clicked()
    {
        let custom: Vec<Material> = app.prefs.materials.iter().filter(|m| m.custom).cloned().collect();
        app.prefs.materials = send::builtin_materials();
        app.prefs.materials.extend(custom);
        app.canvas.send.editor = None;
        ui.close();
    }
}

/// Dibujo sencillo de un cortador (propio, no la ilustración de la referencia).
fn machine_glyph(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(84.0, 34.0), egui::Sense::hover());
    let p = ui.painter();
    let body = egui::Rect::from_min_max(
        rect.min + egui::vec2(2.0, 8.0),
        rect.max - egui::vec2(2.0, 4.0),
    );
    p.rect(
        body,
        6.0,
        theme::CHROME_200,
        egui::Stroke::new(1.0, theme::CHROME_400),
        egui::StrokeKind::Inside,
    );
    let slot = egui::Rect::from_min_size(
        body.left_center() + egui::vec2(8.0, 3.0),
        egui::vec2(body.width() - 16.0, 3.0),
    );
    p.rect_filled(slot, 1.0, theme::CHROME_500);
    let lid = egui::Rect::from_min_max(
        body.min + egui::vec2(4.0, -5.0),
        egui::pos2(body.max.x - 4.0, body.min.y + 2.0),
    );
    p.rect_filled(lid, 4.0, theme::CHROME_100);
}

fn machine_card(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    Frame::NONE
        .fill(theme::CHROME_800)
        .stroke(egui::Stroke::new(1.0, theme::CHROME_600))
        .corner_radius(6.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(mi::ICON_BLUETOOTH.rich_text().size(20.0).color(
                    if app.machine.connected.is_some() {
                        theme::ACCENT
                    } else {
                        theme::CHROME_300
                    },
                ))
                .on_hover_text("Bluetooth");
                ui.vertical(|ui| {
                    let label = oc_core::machines::machine_label(app.doc.page.machine);
                    let all = app.prefs.send_options.all_cutters;
                    egui::ComboBox::from_id_salt("send_machine")
                        .width(170.0)
                        .selected_text(RichText::new(label).color(theme::CHROME_100))
                        .show_ui(ui, |ui| {
                            if !all {
                                ui.label(
                                    RichText::new("No se ha detectado ninguna máquina.")
                                        .small()
                                        .color(theme::CHROME_300),
                                );
                            }
                            let current = app.doc.page.machine;
                            for m in oc_core::machines::MACHINES
                                .iter()
                                .filter(|m| all || m.id == current)
                            {
                                if m.id == oc_core::machines::MachineId::None {
                                    continue;
                                }
                                ui.selectable_value(&mut app.doc.page.machine, m.id, m.name);
                            }
                        });
                    let m = &app.machine;
                    let (text, color) = match (&m.connected, m.status) {
                        (Some(_), Some(st)) => (
                            st.name().to_string(),
                            match st {
                                oc_cutter::gpgl::Status::Ready => Color32::from_rgb(0x5c, 0xd6, 0x7a),
                                oc_cutter::gpgl::Status::Moving => theme::ACCENT,
                                _ => Color32::from_rgb(0xff, 0xc1, 0x5e),
                            },
                        ),
                        (Some(_), None) => ("Conectada".into(), Color32::from_rgb(0x5c, 0xd6, 0x7a)),
                        (None, _) if m.searching => ("Buscando por Bluetooth…".into(), theme::CHROME_300),
                        (None, _) => ("Sin conexión".into(), Color32::from_rgb(0xff, 0x6b, 0x6b)),
                    };
                    ui.label(RichText::new(text).small().color(color));
                    if let Some((name, fw)) = &m.connected {
                        ui.label(
                            RichText::new(format!("{name} · {fw}"))
                                .small()
                                .color(theme::CHROME_300),
                        );
                    }
                    let ctx = ui.ctx().clone();
                    let connected = app.machine.connected.is_some();
                    let searching = app.machine.searching;
                    ui.horizontal(|ui| {
                        if connected {
                            if ui.small_button("Desconectar").clicked() {
                                app.machine_disconnect(&ctx);
                            }
                        } else if ui
                            .add_enabled(!searching, egui::Button::new("Conectar").small())
                            .on_hover_text("Busca la máquina por Bluetooth (enciéndela y cierra Silhouette Studio)")
                            .clicked()
                        {
                            app.machine_connect(&ctx);
                        }
                    });
                });
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    machine_glyph,
                );
            });
        });
}

/// Alto de la lista de materiales del desplegable.
const MATERIAL_LIST_H: f32 = 320.0;

/// Selector de material con categorías y búsqueda. Devuelve `true` al cambiar. Los materiales
/// sin ajustes para `holder` llevan un aviso.
fn material_picker(
    ui: &mut Ui,
    id_salt: &str,
    materials: &[Material],
    holder: Holder,
    value: &mut String,
    width: f32,
    open_new: &mut bool,
) -> bool {
    #[derive(Clone, Default)]
    struct Pick {
        search: String,
        /// `None` = todos; `Some(None)` = a medida; `Some(Some(f))` = familia.
        filter: Option<Option<Family>>,
        /// Alto de la lista mientras el desplegable está abierto y último frame dibujado.
        list_h: f32,
        frame: u64,
    }
    let id = egui::Id::unique(("material_picker", id_salt));
    let text = if value.is_empty() {
        "Elige un material".to_string()
    } else {
        value.clone()
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(("material_picker", id_salt))
        .width(width)
        .height(640.0)
        .truncate()
        .selected_text(text)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            ui.set_min_width(300.0);
            let mut st: Pick = ui.data(|d| d.get_temp(id)).unwrap_or_default();
            let now = ui.ctx().cumulative_frame_nr();
            if st.frame + 1 != now {
                // Recién abierto: se parte del alto completo.
                st.list_h = MATERIAL_LIST_H;
            }
            st.frame = now;
            let top = ui.cursor().top();
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_label(st.filter == Some(None), "A medida")
                    .clicked()
                {
                    st.filter = if st.filter == Some(None) {
                        None
                    } else {
                        Some(None)
                    };
                }
                for f in Family::ALL {
                    if ui
                        .selectable_label(st.filter == Some(Some(f)), f.name())
                        .clicked()
                    {
                        // Un segundo clic en la categoría vuelve a «Todos los materiales».
                        st.filter = if st.filter == Some(Some(f)) {
                            None
                        } else {
                            Some(Some(f))
                        };
                    }
                }
            });
            let search = ui.add(
                egui::TextEdit::singleline(&mut st.search)
                    .hint_text(format!("{}  Buscar", <&str>::from(mi::ICON_SEARCH)))
                    .desired_width(f32::INFINITY),
            );
            // Al abrir, el teclado va directo a la búsqueda.
            if ui.memory(|m| m.focused().is_none()) {
                search.request_focus();
            }
            let q = st.search.trim().to_lowercase();
            // Al buscar se busca en todos los materiales, sin importar la categoría elegida.
            let filter = if q.is_empty() { st.filter } else { None };
            let title = match filter {
                _ if !q.is_empty() => "Resultados en todos los materiales".to_string(),
                None => "Todos los materiales".to_string(),
                Some(None) => "A medida".to_string(),
                Some(Some(f)) => f.name().to_string(),
            };
            ui.label(RichText::new(title).strong().color(theme::CHROME_100));
            // Altura fija: el desplegable no cambia de tamaño al cambiar de categoría.
            let list_h = st.list_h;
            egui::ScrollArea::vertical()
                .id_salt("material_list")
                .max_height(list_h)
                .min_scrolled_height(list_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut any = false;
                    for m in materials.iter().filter(|m| match filter {
                        None => true,
                        Some(None) => m.custom,
                        Some(Some(f)) => m.family == f,
                    }) {
                        if !q.is_empty() && !m.name.to_lowercase().contains(&q) {
                            continue;
                        }
                        any = true;
                        ui.horizontal(|ui| {
                            let r = ui.selectable_label(*value == m.name, &m.name);
                            if r.clicked() {
                                *value = m.name.clone();
                                changed = true;
                                ui.close();
                            }
                            if m.actions_for(holder).is_empty() {
                                ui.label(
                                    mi::ICON_WARNING
                                        .rich_text()
                                        .color(Color32::from_rgb(0xff, 0xc1, 0x5e)),
                                )
                                .on_hover_text(format!(
                                    "Sin ajustes para el {}",
                                    holder.name().to_lowercase()
                                ));
                            }
                        });
                    }
                    if !any {
                        ui.label(
                            RichText::new("Ningún material coincide.")
                                .small()
                                .color(theme::CHROME_300),
                        );
                    }
                });
            ui.separator();
            if ui
                .add(
                    egui::Button::new("Añadir nuevo tipo de material…")
                        .min_size(egui::vec2(ui.available_width(), 26.0)),
                )
                .clicked()
            {
                *open_new = true;
                ui.close();
            }
            // Si no cabe bajo el botón, se acorta la lista para que solo ella se desplace y las
            // categorías, la búsqueda y el botón queden siempre a la vista.
            let overflow = (ui.cursor().top() - top) - ui.clip_rect().height();
            if overflow > 0.5 {
                let shrunk = (st.list_h - overflow).max(120.0);
                // Solo se vuelve a pintar si la lista cambia: con el mínimo alcanzado no hay
                // nada más que hacer (si no, se redibujaría sin parar).
                if shrunk < st.list_h - 0.5 {
                    st.list_h = shrunk;
                    ui.ctx().request_repaint();
                }
            }
            ui.data_mut(|d| d.insert_temp(id, st));
        });
    changed
}

/// Acción: «Auto» más las acciones que el material tiene para el portaherramientas.
fn action_combo(ui: &mut Ui, id_salt: &str, available: &[Action], action: &mut Action, width: f32) {
    egui::ComboBox::from_id_salt(("send_action", id_salt))
        .width(width)
        .selected_text(action.name())
        .show_ui(ui, |ui| {
            ui.selectable_value(action, Action::Auto, Action::Auto.name());
            for a in available {
                ui.selectable_value(action, *a, a.name());
            }
        });
    if *action != Action::Auto && !available.contains(action) {
        *action = Action::Auto;
    }
}

/// Herramienta: sin máquina conectada no se puede detectar, así que se elige a mano entre las
/// que admiten la acción en ese portaherramientas.
fn tool_combo(
    ui: &mut Ui,
    id_salt: &str,
    holder: Holder,
    detected: Option<oc_cutter::gpgl::DetectedTool>,
    action: Action,
    tool: &mut Tool,
    width: f32,
) {
    let tools = Tool::for_action(holder, action);
    // Con la máquina conectada se usa la herramienta que detecta (si sirve para la acción).
    if let Some(oc_cutter::gpgl::DetectedTool::Known(t)) = detected {
        if tools.contains(&t) {
            *tool = t;
        }
    }
    if !tools.contains(tool) {
        if let Some(t) = tools.first() {
            *tool = *t;
        }
    }
    if let Some(d) = detected {
        let ok = matches!(d, oc_cutter::gpgl::DetectedTool::Known(t) if t == *tool);
        let (icon, color, text) = match d {
            oc_cutter::gpgl::DetectedTool::Empty => (
                mi::ICON_WARNING,
                Color32::from_rgb(0xff, 0xa0, 0x3c),
                format!(
                    "Portaherramientas vacío: monta {}",
                    tool.name().to_lowercase()
                ),
            ),
            _ if ok => (
                mi::ICON_CHECK_CIRCLE,
                Color32::from_rgb(0x5c, 0xd6, 0x7a),
                format!("{} (detectada)", tool.name()),
            ),
            other => (
                mi::ICON_WARNING,
                Color32::from_rgb(0xff, 0xa0, 0x3c),
                format!("Detectada: {}; no sirve para esta acción", other.name()),
            ),
        };
        ui.add_sized(
            [width, 24.0],
            egui::Label::new(RichText::new(format!("{}  {text}", <&str>::from(icon))).color(color)),
        );
        return;
    }
    let warn = Color32::from_rgb(0xff, 0xa0, 0x3c);
    let text = if tools.is_empty() {
        RichText::new(format!(
            "{}  Ninguna herramienta compatible",
            <&str>::from(mi::ICON_WARNING)
        ))
        .color(warn)
    } else {
        RichText::new(format!(
            "{}  {}",
            <&str>::from(mi::ICON_WARNING),
            tool.name()
        ))
        .color(warn)
    };
    egui::ComboBox::from_id_salt(("send_tool", id_salt))
        .width(width)
        .selected_text(text)
        .show_ui(ui, |ui| {
            ui.label(
                RichText::new("Esperando la conexión: elige la herramienta montada.")
                    .small()
                    .color(theme::CHROME_300),
            );
            for t in tools {
                ui.selectable_value(tool, t, t.name());
            }
        })
        .response
        .on_hover_text("Esperando la conexión…");
}

fn show_depth(mode: ShowBlade, tool: Tool, action: Action) -> bool {
    match mode {
        ShowBlade::Always => action.has_depth(),
        ShowBlade::Never => false,
        ShowBlade::Auto => action.has_depth() && tool.has_depth(),
    }
}

/// «Modificar la configuración» del panel: Presión, Velocidad, Pasadas, Profundidad y
/// Sobrecorte, editables en el material, con Deshacer y Guardar como. Devuelve (más, crear).
fn inline_settings(
    app: &mut OpenCraftCutterApp,
    ui: &mut Ui,
    material: &str,
    holder: Holder,
    action: Action,
    tool: Tool,
) -> (bool, bool) {
    let action = send::pick_action(
        app.prefs.materials.iter().find(|m| m.name == material),
        holder,
        action,
        tool,
    );
    let show_blade = app.prefs.send_options.show_blade;
    let mut more = false;
    let mut create = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Modificar la configuración").color(theme::CHROME_100));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            more = ui
                .add(
                    egui::Button::new(RichText::new("Más").color(Color32::WHITE))
                        .fill(theme::ACCENT_STRONG),
                )
                .on_hover_text("Configuración de materiales")
                .clicked();
        });
    });
    let Some(i) = app.prefs.materials.iter().position(|m| m.name == material) else {
        return (more, create);
    };
    let Some(original) = app.prefs.materials[i].settings(holder, action).copied() else {
        ui.horizontal(|ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "No se ha encontrado la acción «{}» compatible con el {} para este \
                         material.\n\nHaz clic en «Más» para establecer nuevos ajustes.",
                        action.name(),
                        holder.name().to_lowercase()
                    ))
                    .small()
                    .color(theme::CHROME_300),
                )
                .wrap(),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            create = ui
                .button("Crear nuevo…")
                .on_hover_text("Material nuevo")
                .clicked();
        });
        return (more, create);
    };
    let mut st = original;
    slider_u32(ui, "Presión", &mut st.force, ActionSettings::FORCE);
    slider_u32(ui, "Velocidad", &mut st.speed, ActionSettings::SPEED);
    slider_u32(ui, "Pasadas", &mut st.passes, ActionSettings::PASSES);
    if show_depth(show_blade, tool, action) {
        depth_dial(ui, &mut st.depth, ActionSettings::depth_range(action));
    }
    if action.has_overcut() {
        ui.checkbox(&mut st.overcut, "Sobrecorte de segmento de línea");
    }
    if tool.manual_depth() && action.has_depth() {
        ui.label(
            RichText::new(format!("Ajusta la cuchilla a mano en {}.", st.depth))
                .small()
                .color(Color32::from_rgb(0xff, 0xc1, 0x5e)),
        );
    }
    let st = st.clamped();
    let s = &mut app.canvas.send;
    if st != original {
        if s.inline_backup.as_ref().map(|m| m.name.as_str()) != Some(material) {
            s.inline_backup = Some(app.prefs.materials[i].clone());
        }
        *app.prefs.materials[i].settings_mut(holder, action) = st;
    }
    let edited = s
        .inline_backup
        .as_ref()
        .is_some_and(|b| b.name == material && *b != app.prefs.materials[i]);
    let mut undo = false;
    let mut save_as = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            save_as = ui.button("Guardar como…").clicked();
            undo = ui
                .add_enabled(edited, egui::Button::new("Deshacer"))
                .clicked();
        });
    });
    if undo {
        if let Some(b) = s.inline_backup.take() {
            app.prefs.materials[i] = b;
        }
    }
    if save_as {
        let base = &app.prefs.materials[i];
        s.name_dialog = Some(NameDialog {
            name: format!("{} (copia)", base.name),
            family: base.family,
            from: Some(i),
        });
    }
    (more, create)
}

fn open_editor(
    app: &mut OpenCraftCutterApp,
    material: &str,
    holder: Holder,
    action: Action,
    tool: Tool,
) {
    let mats = app.materials_mut();
    let i = mats.iter().position(|m| m.name == material).unwrap_or(0);
    let mut action = send::pick_action(Some(&mats[i]), holder, action, tool);
    // Si el material no tiene esa acción, se abre en la primera que tenga.
    if mats[i].settings(holder, action).is_none() {
        if let Some(a) = mats[i].actions_for(holder).first() {
            action = *a;
        }
    }
    let backup = mats[i].clone();
    app.canvas.send.editor = Some(Editor {
        material: i,
        holder,
        action,
        backup,
    });
}

fn holder_setup(app: &mut OpenCraftCutterApp, ui: &mut Ui, holder: Holder, id: &str) {
    let w = ui.available_width();
    let mut open_new = false;
    app.materials_mut();
    let idx = holder.number() as usize - 1;
    let detected = app.machine.tool(holder);
    {
        let mats = &app.prefs.materials;
        let setup = &mut app.canvas.send.setups[idx];
        material_picker(ui, id, mats, holder, &mut setup.material, w, &mut open_new);
        let available = mats
            .iter()
            .find(|m| m.name == setup.material)
            .map(|m| m.actions_for(holder))
            .unwrap_or_default();
        let mut tool = setup.tool;
        let mut action = setup.action;
        action_combo(ui, id, &available, &mut action, w);
        let effective = send::pick_action(
            mats.iter().find(|m| m.name == setup.material),
            holder,
            action,
            tool,
        );
        tool_combo(ui, id, holder, detected, effective, &mut tool, w);
        setup.tool = tool;
        setup.action = action;
    }
    ui.add_space(6.0);
    let setup = app.canvas.send.setups[idx].clone();
    let (more, create) =
        inline_settings(app, ui, &setup.material, holder, setup.action, setup.tool);
    if more {
        open_editor(app, &setup.material, holder, setup.action, setup.tool);
    }
    if create || open_new {
        app.canvas.send.name_dialog = Some(NameDialog {
            name: "Material nuevo".into(),
            family: Family::Card,
            from: None,
        });
    }
}

/// Botón de portaherramientas: relleno con su color cuando está elegido.
fn holder_chip(ui: &mut Ui, selected: bool, h: Holder, text: &str) -> egui::Response {
    let c = holder_color(h);
    let mut b = egui::Button::new(RichText::new(text).color(if selected {
        Color32::WHITE
    } else {
        theme::CHROME_300
    }))
    .stroke(egui::Stroke::new(
        1.0,
        c.gamma_multiply(if selected { 1.0 } else { 0.5 }),
    ));
    if selected {
        b = b.fill(c.gamma_multiply(0.8));
    }
    ui.add(b)
}

fn standard_mode(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    ui.label(RichText::new("Ajustes de corte").size(15.0));
    ui.horizontal(|ui| {
        for h in Holder::ALL {
            let sel = app.canvas.send.holder_tab == h;
            if holder_chip(ui, sel, h, h.name()).clicked() {
                app.canvas.send.holder_tab = h;
            }
        }
    });
    let h = app.canvas.send.holder_tab;
    ui.label(
        RichText::new(format!("{}: material, acción y herramienta", h.name()))
            .color(theme::CHROME_300),
    );
    holder_setup(app, ui, h, if h == Holder::One { "std1" } else { "std2" });
    ui.add_space(8.0);
    ui.label(
        RichText::new(
            "Elige qué portaherramientas usa cada objeto con los botones bajo la vista previa.",
        )
        .small()
        .color(theme::CHROME_300),
    );
}

fn color_swatch(ui: &mut Ui, c: Color, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    let p = ui.painter();
    if c == send::NO_FILL {
        p.rect_filled(rect, 3.0, Color32::from_gray(0xe0));
        p.line_segment(
            [rect.left_bottom(), rect.right_top()],
            egui::Stroke::new(1.5, Color32::RED),
        );
    } else {
        p.rect_filled(
            rect,
            3.0,
            Color32::from_rgba_unmultiplied(c.r, c.g, c.b, 255),
        );
    }
    p.rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected {
                theme::ACCENT
            } else {
                theme::CHROME_500
            },
        ),
        egui::StrokeKind::Inside,
    );
    resp
}

fn color_name(c: Color) -> String {
    if c == send::NO_FILL {
        "Sin color".into()
    } else {
        c.to_hex()
    }
}

fn color_mode(app: &mut OpenCraftCutterApp, ui: &mut Ui, by_fill: bool) {
    ui.label(
        RichText::new(if by_fill {
            "Cortar por color de relleno"
        } else {
            "Cortar por color de línea"
        })
        .size(15.0),
    );
    ui.add_space(4.0);
    app.materials_mut();
    let count = if by_fill {
        app.canvas.send.fill_rows.len()
    } else {
        app.canvas.send.line_rows.len()
    };
    if count == 0 {
        ui.label(
            RichText::new("No hay objetos que enviar.")
                .small()
                .color(theme::CHROME_300),
        );
        return;
    }
    let mut open_new = false;
    let width = ui.available_width();
    let s = &mut app.canvas.send;
    let mats = &app.prefs.materials;
    let rows = if by_fill {
        &mut s.fill_rows
    } else {
        &mut s.line_rows
    };
    let mut cmd: Option<(usize, RowCmd)> = None;
    for (i, (c, row)) in rows.iter_mut().enumerate() {
        let selected = s.selected_row == Some(i);
        let frame = Frame::NONE
            .fill(if selected {
                theme::ACCENT_SOFT
            } else {
                theme::CHROME_800
            })
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(6, 4));
        // Fondo de la tarjeta para el clic derecho. Se registra antes que su contenido (con el
        // rectángulo del frame anterior) para que los desplegables y botones queden por encima
        // y reciban sus clics.
        let bg_id = ui.scope_id().with(("color_row", by_fill, i));
        let bg_rect: Option<egui::Rect> = ui.data(|d| d.get_temp(bg_id));
        let bg = bg_rect.map(|rect| ui.interact(rect, bg_id, egui::Sense::click()));
        let r = frame.show(ui, |ui| {
            ui.set_width(width - 12.0);
            ui.horizontal(|ui| {
                let sw =
                    color_swatch(ui, *c, selected).on_hover_text("Ver los ajustes de este color");
                if sw.clicked() {
                    s.selected_row = if selected { None } else { Some(i) };
                }
                ui.label(
                    RichText::new(color_name(*c))
                        .monospace()
                        .color(theme::CHROME_200),
                );
                if row.settings.is_some() {
                    ui.label(
                        mi::ICON_TUNE
                            .rich_text()
                            .color(Color32::from_rgb(0xff, 0xc1, 0x5e)),
                    )
                    .on_hover_text("Ajustes propios de este color");
                }
                if row.pause_after {
                    ui.label(
                        mi::ICON_PAUSE
                            .rich_text()
                            .color(Color32::from_rgb(0xff, 0xc1, 0x5e)),
                    )
                    .on_hover_text("La máquina se detiene al terminar este color");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (icon, tip) = if row.edge_only {
                        (
                            mi::ICON_BORDER_OUTER,
                            "Solo el contorno exterior (clic para usar todas las líneas)",
                        )
                    } else {
                        (
                            mi::ICON_FILTER_NONE,
                            "Todas las líneas (clic para usar solo el contorno)",
                        )
                    };
                    if ui
                        .add(egui::Button::new(icon.rich_text().size(16.0)).frame(false))
                        .on_hover_text(tip)
                        .clicked()
                    {
                        row.edge_only = !row.edge_only;
                    }
                    ui.checkbox(&mut row.enabled, "")
                        .on_hover_text("On/Off: enviar las líneas de este color");
                    for h in [Holder::Two, Holder::One] {
                        if holder_chip(ui, row.holder == h, h, &h.number().to_string())
                            .on_hover_text(h.name())
                            .clicked()
                        {
                            row.holder = h;
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                let half = (ui.available_width() - 8.0) / 2.0;
                let mut mat = row.material.clone();
                if material_picker(
                    ui,
                    &format!("row{by_fill}{i}"),
                    mats,
                    row.holder,
                    &mut mat,
                    half,
                    &mut open_new,
                ) {
                    row.material = mat;
                }
                let available = mats
                    .iter()
                    .find(|m| m.name == row.material)
                    .map(|m| m.actions_for(row.holder))
                    .unwrap_or_default();
                action_combo(
                    ui,
                    &format!("row{by_fill}{i}"),
                    &available,
                    &mut row.action,
                    half,
                );
            });
        });
        ui.data_mut(|d| d.insert_temp(bg_id, r.response.rect));
        if let Some(bg) = &bg {
            if bg.clicked() {
                s.selected_row = if selected { None } else { Some(i) };
            }
        }
        if let Some(bg) = bg {
            bg.context_menu(|ui| {
                if ui.checkbox(&mut row.enabled, "On/Off").clicked() {
                    ui.close();
                }
                if ui.button("Activar/Desactivar todo").clicked() {
                    cmd = Some((i, RowCmd::ToggleAll));
                    ui.close();
                }
                if ui
                    .add_enabled(
                        s.selected_row.is_some_and(|j| j != i),
                        egui::Button::new("Fusionar con el color seleccionado"),
                    )
                    .on_hover_text("El color seleccionado toma los ajustes de este")
                    .clicked()
                {
                    cmd = Some((i, RowCmd::Merge));
                    ui.close();
                }
                if ui
                    .button("Unir todo")
                    .on_hover_text("Todos los colores toman los ajustes de este")
                    .clicked()
                {
                    cmd = Some((i, RowCmd::JoinAll));
                    ui.close();
                }
                if ui
                    .button("Herramienta 2 todo")
                    .on_hover_text(
                        "Todos los colores pasan al portaherramientas 2 (o vuelven al 1)",
                    )
                    .clicked()
                {
                    cmd = Some((i, RowCmd::AllToTwo));
                    ui.close();
                }
                ui.separator();
                if ui
                    .checkbox(&mut row.pause_after, "Añadir pausa después")
                    .clicked()
                {
                    ui.close();
                }
            });
        }
        ui.add_space(3.0);
    }
    if let Some((i, c)) = cmd {
        let model = rows[i].1.clone();
        match c {
            RowCmd::ToggleAll => {
                let on = !rows.iter().all(|(_, r)| r.enabled);
                for (_, r) in rows.iter_mut() {
                    r.enabled = on;
                }
            }
            RowCmd::Merge => {
                if let Some(j) = s.selected_row.filter(|j| *j < rows.len()) {
                    let r = &mut rows[j].1;
                    r.holder = model.holder;
                    r.material = model.material.clone();
                    r.action = model.action;
                    r.tool = model.tool;
                }
            }
            RowCmd::JoinAll => {
                for (_, r) in rows.iter_mut() {
                    r.holder = model.holder;
                    r.material = model.material.clone();
                    r.action = model.action;
                    r.tool = model.tool;
                }
            }
            RowCmd::AllToTwo => {
                let to = if rows.iter().all(|(_, r)| r.holder == Holder::Two) {
                    Holder::One
                } else {
                    Holder::Two
                };
                for (_, r) in rows.iter_mut() {
                    r.holder = to;
                }
            }
        }
    }
    ui.label(
        RichText::new(
            "Clic en un color para ver sus ajustes; clic derecho para más opciones y pausas.",
        )
        .small()
        .color(theme::CHROME_300),
    );
    if open_new {
        app.canvas.send.name_dialog = Some(NameDialog {
            name: "Material nuevo".into(),
            family: Family::Card,
            from: None,
        });
    }
    // Detalle de la fila elegida: los mismos controles que el modo Estándar.
    let Some(i) = app.canvas.send.selected_row.filter(|i| *i < count) else {
        return;
    };
    ui.add_space(8.0);
    ui.separator();
    let (c, row) = {
        let rows = if by_fill {
            &app.canvas.send.fill_rows
        } else {
            &app.canvas.send.line_rows
        };
        rows[i].clone()
    };
    ui.horizontal(|ui| {
        color_swatch(ui, c, false);
        ui.label(RichText::new(color_name(c)).color(theme::CHROME_100));
        ui.label(RichText::new(row.holder.name()).color(holder_color(row.holder)));
    });
    let w = ui.available_width();
    let mut row2 = row.clone();
    let mut open_new = false;
    material_picker(
        ui,
        &format!("detail{by_fill}"),
        &app.prefs.materials,
        row2.holder,
        &mut row2.material,
        w,
        &mut open_new,
    );
    let available = app
        .prefs
        .materials
        .iter()
        .find(|m| m.name == row2.material)
        .map(|m| m.actions_for(row2.holder))
        .unwrap_or_default();
    action_combo(
        ui,
        &format!("detail{by_fill}"),
        &available,
        &mut row2.action,
        w,
    );
    let effective = send::pick_action(
        app.prefs.materials.iter().find(|m| m.name == row2.material),
        row2.holder,
        row2.action,
        row2.tool,
    );
    tool_combo(
        ui,
        &format!("detail{by_fill}"),
        row2.holder,
        app.machine.tool(row2.holder),
        effective,
        &mut row2.tool,
        w,
    );
    ui.checkbox(&mut row2.pause_after, "Pausa al terminar este color");
    if row2 != row {
        let rows = if by_fill {
            &mut app.canvas.send.fill_rows
        } else {
            &mut app.canvas.send.line_rows
        };
        rows[i].1 = row2.clone();
    }
    ui.add_space(4.0);
    let (more, create) = row_settings(app, ui, by_fill, i, &row2);
    if more {
        open_editor(app, &row2.material, row2.holder, row2.action, row2.tool);
    }
    if create || open_new {
        app.canvas.send.name_dialog = Some(NameDialog {
            name: "Material nuevo".into(),
            family: Family::Card,
            from: None,
        });
    }
}

/// Ajustes de un color: parten de los del material y, al cambiarlos, pasan a ser propios del
/// color (el material no cambia). «Usar los del material» los descarta; «Guardar como…» crea
/// un material con ellos.
fn row_settings(
    app: &mut OpenCraftCutterApp,
    ui: &mut Ui,
    by_fill: bool,
    i: usize,
    row: &send::ColorRow,
) -> (bool, bool) {
    let mat = app.prefs.materials.iter().find(|m| m.name == row.material);
    let action = send::pick_action(mat, row.holder, row.action, row.tool);
    let from_material = mat.and_then(|m| m.settings(row.holder, action).copied());
    let mut more = false;
    let mut create = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Modificar la configuración").color(theme::CHROME_100));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            more = ui
                .add(
                    egui::Button::new(RichText::new("Más").color(Color32::WHITE))
                        .fill(theme::ACCENT_STRONG),
                )
                .on_hover_text("Configuración de materiales")
                .clicked();
        });
    });
    let Some(base) = row.settings.or(from_material) else {
        ui.label(
            RichText::new(format!(
                "El material no tiene la acción «{}» en el {}. Haz clic en «Más» para añadirla.",
                action.name(),
                row.holder.name().to_lowercase()
            ))
            .small()
            .color(theme::CHROME_300),
        );
        create = ui.button("Crear nuevo…").clicked();
        return (more, create);
    };
    if row.settings.is_some() {
        ui.label(
            RichText::new("Ajustes propios de este color (el material no cambia).")
                .small()
                .color(Color32::from_rgb(0xff, 0xc1, 0x5e)),
        );
    }
    let show_blade = app.prefs.send_options.show_blade;
    let mut st = base;
    slider_u32(ui, "Presión", &mut st.force, ActionSettings::FORCE);
    slider_u32(ui, "Velocidad", &mut st.speed, ActionSettings::SPEED);
    slider_u32(ui, "Pasadas", &mut st.passes, ActionSettings::PASSES);
    if show_depth(show_blade, row.tool, action) {
        depth_dial(ui, &mut st.depth, ActionSettings::depth_range(action));
    }
    if action.has_overcut() {
        ui.checkbox(&mut st.overcut, "Sobrecorte de segmento de línea");
    }
    if row.tool.manual_depth() && action.has_depth() {
        ui.label(
            RichText::new(format!("Ajusta la cuchilla a mano en {}.", st.depth))
                .small()
                .color(Color32::from_rgb(0xff, 0xc1, 0x5e)),
        );
    }
    let st = st.clamped();
    let mut own = row.settings;
    if st != base {
        own = Some(st);
    }
    let mut reset = false;
    let mut save_as = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            save_as = ui
                .button("Guardar como…")
                .on_hover_text("Crea un material nuevo con estos ajustes")
                .clicked();
            reset = ui
                .add_enabled(own.is_some(), egui::Button::new("Usar los del material"))
                .on_hover_text("Descarta los ajustes propios de este color")
                .clicked();
        });
    });
    if reset {
        own = None;
    }
    let s = &mut app.canvas.send;
    let rows = if by_fill {
        &mut s.fill_rows
    } else {
        &mut s.line_rows
    };
    if let Some(r) = rows.get_mut(i) {
        r.1.settings = own;
    }
    if save_as {
        if let Some(j) = app
            .prefs
            .materials
            .iter()
            .position(|m| m.name == row.material)
        {
            let base = &app.prefs.materials[j];
            app.canvas.send.row_save = Some((by_fill, i, row.holder, action, st));
            app.canvas.send.name_dialog = Some(NameDialog {
                name: format!("{} (copia)", base.name),
                family: base.family,
                from: Some(j),
            });
        }
    }
    (more, create)
}

/// Órdenes del menú contextual de una fila de color.
#[derive(Clone, Copy)]
enum RowCmd {
    ToggleAll,
    Merge,
    JoinAll,
    AllToTwo,
}

/// Pasos del asistente (modo Guía), en el orden de la referencia. Se construyen según el
/// diseño: portaherramientas usados, herramientas que se ajustan a mano, lápiz térmico,
/// tapete electrostático, tapete o rollo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuideStep {
    Connect,
    Material,
    Action(Holder),
    Tool(Holder),
    AdjustBlade(Holder),
    Preheat,
    Electrostatic,
    PlaceMedia,
    LoadMat,
    LoadRoll,
    Summary,
    Modify,
    MakeIt,
}

impl GuideStep {
    fn title(self) -> String {
        match self {
            GuideStep::Connect => "Conectarse a la máquina".into(),
            GuideStep::Material => "Elige el material".into(),
            GuideStep::Action(h) => format!("Elige la acción · {}", h.name()),
            GuideStep::Tool(h) => format!("Monta la herramienta · {}", h.name()),
            GuideStep::AdjustBlade(h) => format!("Ajusta la cuchilla · {}", h.name()),
            GuideStep::Preheat => "Precalienta el lápiz térmico".into(),
            GuideStep::Electrostatic => "Conecta el tapete electrostático".into(),
            GuideStep::PlaceMedia => "Coloca el material en el tapete".into(),
            GuideStep::LoadMat => "Carga el tapete".into(),
            GuideStep::LoadRoll => "Carga el rollo".into(),
            GuideStep::Summary => "Resumen de los ajustes".into(),
            GuideStep::Modify => "Modificar los ajustes".into(),
            GuideStep::MakeIt => "¡Hazlo!".into(),
        }
    }
}

impl OpenCraftCutterApp {
    /// Portaherramientas que usa el diseño (al menos el 1).
    fn used_holders(&self) -> Vec<Holder> {
        let mut v: Vec<Holder> = Holder::ALL
            .into_iter()
            .filter(|h| {
                self.doc.shapes.iter().any(|s| {
                    s.visible
                        && !s.is_image()
                        && s.cut.holder == *h
                        && s.cut.style != send::CutStyle::Skip
                })
            })
            .collect();
        if v.is_empty() {
            v.push(Holder::One);
        }
        v
    }

    fn guide_steps(&self) -> Vec<GuideStep> {
        let s = &self.canvas.send;
        let mut v = vec![GuideStep::Connect, GuideStep::Material];
        let mut heat = false;
        for h in self.used_holders() {
            let st = &s.setups[h.number() as usize - 1];
            v.push(GuideStep::Action(h));
            v.push(GuideStep::Tool(h));
            let m = self.prefs.materials.iter().find(|m| m.name == st.material);
            let action = send::pick_action(m, h, st.action, st.tool);
            if st.tool.manual_depth() && action.has_depth() {
                v.push(GuideStep::AdjustBlade(h));
            }
            heat |= st.tool == Tool::HeatPen;
        }
        if heat {
            v.push(GuideStep::Preheat);
        }
        let mat = self.doc.page.mat;
        if mat == oc_core::machines::MatId::Electrostatic12 {
            v.push(GuideStep::Electrostatic);
        }
        if mat == oc_core::machines::MatId::None {
            v.push(GuideStep::LoadRoll);
        } else {
            v.push(GuideStep::PlaceMedia);
            v.push(GuideStep::LoadMat);
        }
        v.push(GuideStep::Summary);
        if s.guide_modify {
            v.push(GuideStep::Modify);
        }
        v.push(GuideStep::MakeIt);
        v
    }
}

fn guide_text(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).color(theme::CHROME_200)).wrap());
}

fn guide_note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).small().color(theme::CHROME_300)).wrap());
}

/// Puntos de progreso del asistente.
fn guide_dots(ui: &mut Ui, count: usize, current: usize) -> Option<usize> {
    let mut go = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for i in 0..count {
            let (rect, r) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::click());
            let c = if i == current {
                theme::ACCENT
            } else if i < current {
                theme::CHROME_300
            } else {
                theme::CHROME_600
            };
            ui.painter().circle_filled(rect.center(), 4.0, c);
            if r.clicked() && i < current {
                go = Some(i);
            }
        }
    });
    go
}

fn guide_mode(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let steps = app.guide_steps();
    let idx = app.canvas.send.guide_step.min(steps.len() - 1);
    app.canvas.send.guide_step = idx;
    let step = steps[idx];
    ui.label(
        RichText::new(step.title())
            .size(16.0)
            .color(theme::CHROME_100),
    );
    if let Some(i) = guide_dots(ui, steps.len(), idx) {
        app.canvas.send.guide_step = i;
    }
    ui.separator();
    let w = ui.available_width();
    // Qué hacen los botones del pie en este paso.
    let mut next_label = "Continuar";
    let mut can_next = true;
    let mut goto: Option<GuideStep> = None;
    match step {
        GuideStep::Connect => {
            guide_text(ui, "Las conexiones disponibles aparecen a continuación.");
            guide_note(
                ui,
                "Si tu máquina no aparece, pulsa «Volver a buscar dispositivos» o apaga y vuelve a \
                 encender el Bluetooth de la máquina.",
            );
            let connected = app.machine.connected.clone();
            let searching = app.machine.searching;
            Frame::NONE
                .fill(theme::CHROME_800)
                .stroke(egui::Stroke::new(1.0, theme::CHROME_600))
                .corner_radius(4.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.set_min_height(120.0);
                    ui.set_width(ui.available_width());
                    match &connected {
                        Some((name, fw)) => {
                            ui.label(
                                RichText::new(format!(
                                    "{}  {name}",
                                    <&str>::from(mi::ICON_BLUETOOTH)
                                ))
                                .color(theme::ACCENT),
                            );
                            guide_note(ui, &format!("Conectada · {fw}"));
                        }
                        None if searching => {
                            ui.label(
                                RichText::new("Buscando por Bluetooth…").color(theme::CHROME_300),
                            );
                        }
                        None => {
                            ui.label(
                                RichText::new("No se ha encontrado ninguna máquina.")
                                    .color(theme::CHROME_300),
                            );
                            guide_note(
                                ui,
                                "Enciende la máquina y cierra Silhouette Studio (solo una \
                                 aplicación puede estar conectada).",
                            );
                        }
                    }
                });
            let ctx = ui.ctx().clone();
            ui.vertical_centered(|ui| {
                if connected.is_none()
                    && !searching
                    && ui.link("Volver a buscar dispositivos").clicked()
                {
                    app.machine_connect(&ctx);
                }
            });
            if connected.is_none()
                && ui
                    .add_enabled(
                        !searching,
                        egui::Button::new("Conectar").min_size(egui::vec2(w, 30.0)),
                    )
                    .clicked()
            {
                app.machine_connect(&ctx);
            }
            if connected.is_some() {
                next_label = "Continuar";
            } else {
                next_label = "Continuar sin conexión";
            }
        }
        GuideStep::Material => {
            guide_text(ui, "¿Qué material vas a usar?");
            app.materials_mut();
            let mut open_new = false;
            let mut mat = app.canvas.send.setups[0].material.clone();
            if material_picker(
                ui,
                "guide_mat",
                &app.prefs.materials,
                Holder::One,
                &mut mat,
                w,
                &mut open_new,
            ) {
                for st in app.canvas.send.setups.iter_mut() {
                    st.material = mat.clone();
                }
            }
            if open_new {
                app.canvas.send.name_dialog = Some(NameDialog {
                    name: "Material nuevo".into(),
                    family: Family::Card,
                    from: None,
                });
            }
            guide_note(
                ui,
                "Los ajustes de corte se cargan del material. Podrás revisarlos antes de enviar.",
            );
        }
        GuideStep::Action(h) => {
            let i = h.number() as usize - 1;
            let st = app.canvas.send.setups[i].clone();
            let available = app
                .prefs
                .materials
                .iter()
                .find(|m| m.name == st.material)
                .map(|m| m.actions_for(h))
                .unwrap_or_default();
            guide_text(
                ui,
                &format!(
                    "¿Qué hará el {} con «{}»?",
                    h.name().to_lowercase(),
                    st.material
                ),
            );
            if available.is_empty() {
                guide_note(
                    ui,
                    "Este material no tiene acciones para este portaherramientas. Añádelas en \
                     «Configuración de materiales» (modo Estándar › Más).",
                );
            }
            for a in available {
                let sel = st.action == a;
                let b = egui::Button::selectable(
                    sel,
                    RichText::new(format!("{}   {}", <&str>::from(action_icon(a)), a.name()))
                        .size(15.0),
                )
                .min_size(egui::vec2(w, 34.0));
                if ui.add(b).clicked() {
                    app.canvas.send.setups[i].action = a;
                }
            }
            can_next = app.canvas.send.setups[i].action != Action::Auto;
        }
        GuideStep::Tool(h) => {
            let i = h.number() as usize - 1;
            let st = app.canvas.send.setups[i].clone();
            let side = if h == Holder::One {
                "izquierdo"
            } else {
                "derecho"
            };
            guide_text(
                ui,
                &format!(
                    "Coloca la herramienta en el portaherramientas {} ({side}) y cierra el seguro.",
                    h.number()
                ),
            );
            let m = app.prefs.materials.iter().find(|m| m.name == st.material);
            let action = send::pick_action(m, h, st.action, st.tool);
            let mut tool = st.tool;
            tool_combo(
                ui,
                &format!("guide_tool{}", h.number()),
                h,
                app.machine.tool(h),
                action,
                &mut tool,
                w,
            );
            app.canvas.send.setups[i].tool = tool;
            guide_note(
                ui,
                "Con la máquina conectada, la herramienta se detecta sola. Sin conexión, elige la \
                 que vas a montar.",
            );
        }
        GuideStep::AdjustBlade(h) => {
            let i = h.number() as usize - 1;
            let st = app.canvas.send.setups[i].clone();
            let m = app.prefs.materials.iter().find(|m| m.name == st.material);
            let action = send::pick_action(m, h, st.action, st.tool);
            let depth = m
                .and_then(|m| m.settings(h, action))
                .map(|s| s.depth)
                .unwrap_or(0);
            guide_text(
                ui,
                &format!(
                    "La {} se ajusta a mano. Gira la cuchilla hasta la profundidad {depth}.",
                    st.tool.name().to_lowercase()
                ),
            );
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(depth.to_string())
                        .size(48.0)
                        .color(holder_color(h)),
                );
            });
        }
        GuideStep::Preheat => {
            guide_text(
                ui,
                "El lápiz térmico necesita calentarse antes de empezar. Pulsa «Precalentar» y espera \
                 a que esté listo.",
            );
            ui.add_enabled(
                false,
                egui::Button::new("Precalentar").min_size(egui::vec2(w, 30.0)),
            )
            .on_disabled_hover_text("Necesita la máquina conectada");
        }
        GuideStep::Electrostatic => {
            guide_text(
                ui,
                "Conecta el tapete electrostático a la máquina y pulsa su botón para activar la \
                 sujeción.",
            );
            guide_note(
                ui,
                "Comprueba que el material queda bien pegado antes de cargar.",
            );
        }
        GuideStep::PlaceMedia => {
            let mat = app.canvas.send.setups[0].material.clone();
            guide_text(
                ui,
                &format!(
                    "Coloca «{mat}» en el tapete, alineado con la esquina superior izquierda de la \
                     cuadrícula, tal como se ve en la vista previa."
                ),
            );
            guide_note(
                ui,
                "Alisa el material para que no queden burbujas ni bordes levantados.",
            );
        }
        GuideStep::LoadMat => {
            guide_text(
                ui,
                "Introduce el tapete alineado con la guía izquierda, apoyado en los rodillos, y pulsa \
                 el botón de carga de la máquina.",
            );
        }
        GuideStep::LoadRoll => {
            guide_text(
                ui,
                "Coloca el rollo en el alimentador, pasa el borde bajo los rodillos alineado con la \
                 guía y pulsa el botón de carga de la máquina.",
            );
        }
        GuideStep::Summary => {
            for h in app.used_holders() {
                let st = &app.canvas.send.setups[h.number() as usize - 1];
                let m = app.prefs.materials.iter().find(|m| m.name == st.material);
                let action = send::pick_action(m, h, st.action, st.tool);
                ui.label(RichText::new(h.name()).color(holder_color(h)));
                guide_text(
                    ui,
                    &format!("{} · {} · {}", st.material, action.name(), st.tool.name()),
                );
                if let Some(cs) = m.and_then(|m| m.settings(h, action)) {
                    guide_note(
                        ui,
                        &format!(
                            "Presión {} · Velocidad {} · Pasadas {} · Profundidad {}",
                            cs.force, cs.speed, cs.passes, cs.depth
                        ),
                    );
                }
                ui.add_space(4.0);
            }
            ui.separator();
            let jobs = app.send_jobs();
            job_summary(ui, &jobs);
            ui.add_space(6.0);
            if ui
                .add(egui::Button::new("Probar o modificar").min_size(egui::vec2(w, 30.0)))
                .clicked()
            {
                app.canvas.send.guide_modify = true;
                goto = Some(GuideStep::Modify);
            }
            next_label = "Usar estos ajustes";
        }
        GuideStep::Modify => {
            for h in app.used_holders() {
                let st = app.canvas.send.setups[h.number() as usize - 1].clone();
                ui.label(RichText::new(h.name()).color(holder_color(h)));
                let (more, create) = inline_settings(app, ui, &st.material, h, st.action, st.tool);
                if more {
                    open_editor(app, &st.material, h, st.action, st.tool);
                }
                if create {
                    app.canvas.send.name_dialog = Some(NameDialog {
                        name: "Material nuevo".into(),
                        family: Family::Card,
                        from: None,
                    });
                }
                ui.add_space(6.0);
            }
            guide_note(
                ui,
                "Para probar, genera la forma de prueba y pulsa «Enviar prueba» (abajo). Si el \
                 resultado es bueno, continúa.",
            );
        }
        GuideStep::MakeIt => {
            guide_text(ui, "Todo listo. Pulsa ENVIAR para empezar.");
            let jobs = app.send_jobs();
            job_summary(ui, &jobs);
            can_next = false;
        }
    }
    ui.add_space(10.0);
    let mut back = false;
    let mut next = false;
    ui.horizontal(|ui| {
        back = ui
            .add_enabled(idx > 0, egui::Button::new("Atrás"))
            .clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if step != GuideStep::MakeIt {
                next = ui
                    .add_enabled(
                        can_next,
                        egui::Button::new(RichText::new(next_label).color(Color32::WHITE))
                            .fill(theme::ACCENT_STRONG),
                    )
                    .clicked();
            }
        });
    });
    let s = &mut app.canvas.send;
    if let Some(target) = goto {
        let steps = app.guide_steps();
        if let Some(i) = steps.iter().position(|x| *x == target) {
            app.canvas.send.guide_step = i;
        }
    } else if next {
        if step == GuideStep::Summary {
            // «Usar estos ajustes» va directo al final.
            s.guide_modify = false;
            let steps = app.guide_steps();
            app.canvas.send.guide_step = steps.len() - 1;
        } else {
            s.guide_step = idx + 1;
        }
    } else if back {
        if step == GuideStep::MakeIt || step == GuideStep::Modify {
            // Vuelve al resumen.
            let steps = app.guide_steps();
            if let Some(i) = steps.iter().position(|x| *x == GuideStep::Summary) {
                app.canvas.send.guide_step = i;
            }
            app.canvas.send.guide_modify = false;
        } else {
            s.guide_step = idx - 1;
        }
    }
}

/// Cabecera de «Trabajos» con el tiempo estimado y «Simular corte».
fn simulation_controls(app: &mut OpenCraftCutterApp, ui: &mut Ui, jobs: &[Job]) {
    let now = ui.ctx().input(|i| i.time);
    if jobs.is_empty() {
        app.canvas.send.simulation = None;
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("Trabajos").color(theme::CHROME_100));
        if jobs.is_empty() {
            return;
        }
        ui.label(
            RichText::new(format!(
                "· Tiempo estimado: {}",
                send::format_duration(send::estimate_seconds(jobs))
            ))
            .color(theme::CHROME_300),
        )
        .on_hover_text("Aproximado: depende de la máquina, del material y de las curvas.");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.canvas.send.simulation.is_some() {
                if ui
                    .small_button(format!("{}  Parar", <&str>::from(mi::ICON_STOP)))
                    .clicked()
                {
                    app.canvas.send.simulation = None;
                }
            } else if ui
                .small_button(format!("{}  Simular corte", <&str>::from(mi::ICON_PLAY_ARROW)))
                .on_hover_text(
                    "Muestra en la vista previa el recorrido de la cuchilla en el orden en que se cortará",
                )
                .clicked()
            {
                let moves = send::toolpath(jobs);
                let total_s = moves.last().map(|m| m.start_s + m.secs).unwrap_or(0.0);
                app.canvas.send.simulation = Some(Simulation {
                    moves: std::sync::Arc::new(moves),
                    total_s,
                    started: now,
                    speed: 5.0,
                });
            }
        });
    });
    let Some(sim) = app.canvas.send.simulation.as_mut() else {
        return;
    };
    ui.horizontal(|ui| {
        for v in [1.0, 5.0, 20.0] {
            if ui
                .selectable_label(sim.speed == v, format!("×{v:.0}"))
                .clicked()
                && sim.speed != v
            {
                // Se sigue desde el mismo punto con la nueva velocidad.
                let t = sim.at(now);
                sim.speed = v;
                sim.started = now - t / v;
            }
        }
        let t = sim.at(now);
        ui.add(
            egui::ProgressBar::new((t / sim.total_s.max(1e-9)) as f32)
                .text(format!(
                    "{} de {}",
                    send::format_duration(t).trim_start_matches('~'),
                    send::format_duration(sim.total_s).trim_start_matches('~')
                ))
                .desired_width(ui.available_width()),
        );
    });
}

fn job_summary(ui: &mut Ui, jobs: &[Job]) {
    if jobs.is_empty() {
        ui.label(
            RichText::new("No hay nada que enviar con estos ajustes.")
                .small()
                .color(theme::CHROME_300),
        );
        return;
    }
    for (i, j) in jobs.iter().enumerate() {
        let len: f64 = j
            .paths
            .iter()
            .map(|(p, c)| oc_core::replicate::polyline_length(p, *c))
            .sum();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}.", i + 1)).color(theme::CHROME_300));
            ui.label(
                RichText::new(format!("P{}", j.holder.number())).color(holder_color(j.holder)),
            );
            ui.add(
                egui::Label::new(format!(
                    "{} · {} · {} trazo{} · {:.0} cm{}",
                    j.action.name(),
                    j.tool.name(),
                    j.paths.len(),
                    if j.paths.len() == 1 { "" } else { "s" },
                    len * j.settings.passes as f64 / 10.0,
                    if j.pause_after { " · pausa" } else { "" }
                ))
                .wrap(),
            );
            if j.missing_settings {
                ui.label(
                    mi::ICON_WARNING
                        .rich_text()
                        .color(Color32::from_rgb(0xff, 0xc1, 0x5e)),
                )
                .on_hover_text(format!(
                    "«{}» no tiene ajustes de «{}» para el {}: se usarán valores de partida.",
                    j.material,
                    j.action.name(),
                    j.holder.name().to_lowercase()
                ));
            }
        });
    }
}

fn editor_view(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let options = app.prefs.send_options;
    app.materials_mut();
    let Some(ed) = app.canvas.send.editor.as_mut() else {
        return;
    };
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Configuración de materiales").size(15.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = ui
                .button(mi::ICON_CLOSE.rich_text())
                .on_hover_text("Cerrar")
                .clicked();
        });
    });
    let mats = &mut app.prefs.materials;
    ed.material = ed.material.min(mats.len().saturating_sub(1));
    let mut select: Option<usize> = None;
    let mut add = false;
    let mut remove = false;
    Frame::NONE
        .fill(theme::CHROME_800)
        .corner_radius(4.0)
        .inner_margin(4.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("mat_list")
                .max_height(260.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (i, m) in mats.iter().enumerate() {
                        let label = if m.custom {
                            format!("{}  ·  propio", m.name)
                        } else {
                            m.name.clone()
                        };
                        let r = ui.selectable_label(ed.material == i, label);
                        if r.clicked() {
                            select = Some(i);
                        }
                        if ed.material == i && r.gained_focus() {
                            r.scroll_to_me(None);
                        }
                    }
                });
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    remove = ui
                        .add_enabled(
                            mats[ed.material].custom,
                            egui::Button::new(mi::ICON_REMOVE.rich_text()),
                        )
                        .on_hover_text("Borrar el material (solo los propios)")
                        .clicked();
                    add = ui
                        .button(mi::ICON_ADD.rich_text())
                        .on_hover_text("Material nuevo")
                        .clicked();
                });
            });
        });
    if let Some(i) = select {
        ed.material = i;
        ed.backup = mats[i].clone();
    }
    let edited = mats[ed.material] != ed.backup;
    ui.add_space(6.0);
    // Título «material : acción - portaherramientas» y familia a la derecha.
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(
                RichText::new(format!(
                    "{} : {} - {}{}",
                    mats[ed.material].name,
                    ed.action.name(),
                    ed.holder.name(),
                    if edited { " (editado)" } else { "" }
                ))
                .color(theme::CHROME_100),
            )
            .truncate(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let m = &mut mats[ed.material];
            egui::ComboBox::from_id_salt("editor_family")
                .width(110.0)
                .selected_text(m.family.name())
                .show_ui(ui, |ui| {
                    for f in Family::ALL {
                        ui.selectable_value(&mut m.family, f, f.name());
                    }
                });
        });
    });
    // Portaherramientas y «+» para añadir acciones.
    let mut add_action: Option<Action> = None;
    ui.horizontal(|ui| {
        for h in Holder::ALL {
            if holder_chip(ui, ed.holder == h, h, h.name()).clicked() && ed.holder != h {
                ed.holder = h;
                ed.action = mats[ed.material]
                    .actions_for(h)
                    .first()
                    .copied()
                    .unwrap_or(Action::Auto);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let addable = mats[ed.material].addable_actions(ed.holder);
            ui.add_enabled_ui(!addable.is_empty(), |ui| {
                ui.menu_button(mi::ICON_ADD.rich_text().size(18.0), |ui| {
                    for a in &addable {
                        if ui.button(format!("Añadir acción «{}»", a.name())).clicked() {
                            add_action = Some(*a);
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("Añadir una acción a este portaherramientas");
            });
        });
    });
    if let Some(a) = add_action {
        mats[ed.material].add_action(ed.holder, a);
        ed.action = a;
    }
    // Pestañas de las acciones que tiene el material en este portaherramientas.
    let actions = mats[ed.material].actions_for(ed.holder);
    if !actions.contains(&ed.action) {
        ed.action = actions.first().copied().unwrap_or(Action::Auto);
    }
    ui.horizontal_wrapped(|ui| {
        for a in &actions {
            if ui
                .selectable_label(
                    ed.action == *a,
                    format!("{}  {}", <&str>::from(action_icon(*a)), a.name()),
                )
                .clicked()
            {
                ed.action = *a;
            }
        }
    });
    ui.add_space(4.0);
    let (holder, action) = (ed.holder, ed.action);
    let mut remove_action = false;
    match mats[ed.material].settings(holder, action).copied() {
        Some(original) => {
            let mut st = original;
            settings_editor(ui, &mut st, action, options.show_blade != ShowBlade::Never);
            let st = st.clamped();
            if st != original {
                *mats[ed.material].settings_mut(holder, action) = st;
            }
        }
        None => {
            ui.label(
                RichText::new(
                    "Este material no tiene acciones en este portaherramientas. Añade una con «+».",
                )
                .small()
                .color(theme::CHROME_300),
            );
        }
    }
    ui.add_space(8.0);
    let mut undo = false;
    let mut save_as = false;
    let mut save = false;
    let custom = mats[ed.material].custom;
    ui.horizontal(|ui| {
        remove_action = ui
            .add_enabled(action != Action::Auto, egui::Button::new("Remover acción"))
            .clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if custom {
                save = ui
                    .add_enabled(
                        edited,
                        egui::Button::new(RichText::new("Guardar").color(Color32::WHITE))
                            .fill(theme::ACCENT_STRONG),
                    )
                    .clicked();
            }
            save_as = ui.button("Guardar como…").clicked();
            undo = ui
                .add_enabled(edited, egui::Button::new("Deshacer"))
                .clicked();
        });
    });
    if remove_action {
        mats[ed.material].remove_action(holder, action);
    }
    if undo {
        mats[ed.material] = ed.backup.clone();
    }
    if save {
        ed.backup = mats[ed.material].clone();
    }
    let i = ed.material;
    if remove && mats[i].custom {
        mats.remove(i);
        ed.material = i.saturating_sub(1);
        ed.backup = mats[ed.material].clone();
    }
    if save_as {
        let base = &mats[i];
        app.canvas.send.name_dialog = Some(NameDialog {
            name: format!("{} (copia)", base.name),
            family: base.family,
            from: Some(i),
        });
    }
    if add {
        app.canvas.send.name_dialog = Some(NameDialog {
            name: "Material nuevo".into(),
            family: Family::Card,
            from: None,
        });
    }
    if close {
        app.canvas.send.editor = None;
    }
}

fn action_icon(a: Action) -> egui_material_icons::MaterialIcon {
    match a {
        Action::Auto => mi::ICON_AUTO_MODE,
        Action::Cut | Action::KraftCut => mi::ICON_CONTENT_CUT,
        Action::Sketch => mi::ICON_DRAW,
        Action::Stipple => mi::ICON_GRAIN,
        Action::Score => mi::ICON_HORIZONTAL_RULE,
        Action::PopOut => mi::ICON_FILTER_FRAMES,
        Action::Binding => mi::ICON_BOOK,
        Action::RotaryCut => mi::ICON_ROTATE_RIGHT,
        Action::Punch => mi::ICON_RADIO_BUTTON_CHECKED,
        Action::Emboss => mi::ICON_TEXTURE,
    }
}

/// Profundidad de la cuchilla como la referencia: una rueda numerada que imita el anillo de la
/// cuchilla (cilindro sombreado con el nivel actual resaltado). Se cambia con las flechas, la
/// rueda del ratón o arrastrando. A diferencia de la referencia no da la vuelta (de 10 a 1 o de
/// 1 a 10), para que un gesto de más no cambie de golpe a la profundidad máxima.
fn depth_dial(ui: &mut Ui, v: &mut u32, range: std::ops::RangeInclusive<u32>) {
    let (lo, hi) = (*range.start(), *range.end());
    *v = (*v).clamp(lo, hi);
    ui.horizontal(|ui| {
        ui.add_sized(
            [110.0, 20.0],
            egui::Label::new(RichText::new("Profundidad").color(theme::CHROME_200)),
        );
        let arrow =
            |ui: &mut Ui, icon: egui_material_icons::MaterialIcon, tip: &str, enabled: bool| {
                ui.add_enabled(
                    enabled,
                    egui::Button::new(icon.rich_text().size(16.0)).frame(false),
                )
                .on_hover_text(tip)
                .clicked()
            };
        if arrow(ui, mi::ICON_CHEVRON_LEFT, "Menos profundidad", *v > lo) {
            *v -= 1;
        }
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(150.0, 26.0), egui::Sense::click_and_drag());
        // Rueda del ratón y arrastre horizontal: un nivel por paso.
        if resp.hovered() {
            let dy = ui.input(|i| i.smooth_scroll_delta.y + i.smooth_scroll_delta.x);
            let id = resp.id.with("acc");
            let mut acc: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0) + dy;
            while acc >= 24.0 {
                acc -= 24.0;
                *v = (*v + 1).min(hi);
            }
            while acc <= -24.0 {
                acc += 24.0;
                *v = v.saturating_sub(1).max(lo);
            }
            ui.data_mut(|d| d.insert_temp(id, acc));
        }
        if resp.dragged() {
            let id = resp.id.with("drag");
            let mut acc: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0) - resp.drag_delta().x;
            while acc >= 18.0 {
                acc -= 18.0;
                *v = (*v + 1).min(hi);
            }
            while acc <= -18.0 {
                acc += 18.0;
                *v = v.saturating_sub(1).max(lo);
            }
            ui.data_mut(|d| d.insert_temp(id, acc));
        }
        let p = ui.painter_at(rect);
        // Cilindro: degradado vertical en bandas, más claro en el centro.
        let bands = 10;
        for i in 0..bands {
            let t = i as f32 / (bands - 1) as f32;
            let shade = 1.0 - (t - 0.45).abs() * 1.4;
            let g = (22.0 + 62.0 * shade.clamp(0.0, 1.0)) as u8;
            let y0 = rect.top() + rect.height() * i as f32 / bands as f32;
            let y1 = rect.top() + rect.height() * (i + 1) as f32 / bands as f32;
            p.rect_filled(
                egui::Rect::from_min_max(egui::pos2(rect.left(), y0), egui::pos2(rect.right(), y1)),
                0.0,
                Color32::from_gray(g),
            );
        }
        // Niveles alrededor del actual; el actual, centrado y resaltado.
        let step = rect.width() / 5.0;
        for k in -2i32..=2 {
            let n = *v as i32 + k;
            if n < lo as i32 || n > hi as i32 {
                continue;
            }
            let x = rect.center().x + k as f32 * step;
            let far = k.unsigned_abs() as f32;
            let (size, color) = if k == 0 {
                (19.0, Color32::WHITE)
            } else {
                (
                    15.0 - far * 1.5,
                    Color32::from_gray((215.0 - far * 45.0) as u8),
                )
            };
            p.text(
                egui::pos2(x, rect.center().y),
                egui::Align2::CENTER_CENTER,
                n.to_string(),
                egui::FontId::proportional(size),
                color,
            );
        }
        // Marco del nivel actual.
        let cur = egui::Rect::from_center_size(rect.center(), egui::vec2(step, rect.height()));
        p.rect_stroke(
            cur,
            3.0,
            egui::Stroke::new(1.5, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
        p.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0, theme::CHROME_500),
            egui::StrokeKind::Inside,
        );
        resp.on_hover_text("Nivel de profundidad de la cuchilla (1–10)");
        if arrow(ui, mi::ICON_CHEVRON_RIGHT, "Más profundidad", *v < hi) {
            *v += 1;
        }
        ui.label(RichText::new(format!("Nivel {v}")).color(theme::CHROME_100));
    });
}

/// Fila: etiqueta, deslizador y valor, a lo ancho del panel.
fn slider_u32(ui: &mut Ui, label: &str, v: &mut u32, range: std::ops::RangeInclusive<u32>) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [110.0, 20.0],
            egui::Label::new(RichText::new(label).color(theme::CHROME_200)),
        );
        ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(60.0);
        ui.add(egui::Slider::new(v, range.clone()).show_value(false));
        ui.add(egui::DragValue::new(v).range(range).speed(0.2));
    });
}

fn slider_mm(ui: &mut Ui, label: &str, v: &mut f64, max: f64) {
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.add_sized(
            [86.0, 20.0],
            egui::Label::new(RichText::new(label).color(theme::CHROME_200)),
        );
        ui.spacing_mut().slider_width = (ui.available_width() - 84.0).max(50.0);
        ui.add(egui::Slider::new(v, 0.0..=max).show_value(false));
        ui.add(
            egui::DragValue::new(v)
                .range(0.0..=max)
                .speed(0.01)
                .fixed_decimals(2)
                .suffix(" mm"),
        );
    });
}

fn settings_editor(ui: &mut Ui, st: &mut ActionSettings, action: Action, show_blade: bool) {
    slider_u32(ui, "Presión", &mut st.force, ActionSettings::FORCE);
    slider_u32(ui, "Velocidad", &mut st.speed, ActionSettings::SPEED);
    slider_u32(ui, "Pasadas", &mut st.passes, ActionSettings::PASSES);
    slider_u32(ui, "Aceleración", &mut st.accel, ActionSettings::ACCEL);
    if show_blade && action.has_depth() {
        depth_dial(ui, &mut st.depth, ActionSettings::depth_range(action));
    }
    ui.add_space(4.0);
    if action.has_overcut() {
        ui.checkbox(&mut st.overcut, "Sobrecorte de segmento de línea")
            .on_hover_text("Prolonga cada tramo recto en las esquinas para que queden limpias.");
        if st.overcut {
            slider_mm(ui, "Inicio", &mut st.overcut_start_mm, 1.0);
            slider_mm(ui, "Fin", &mut st.overcut_end_mm, 1.0);
        }
    }
    if action == Action::KraftCut {
        slider_u32(
            ui,
            "Fuerza del bucle",
            &mut st.loop_force,
            ActionSettings::FORCE,
        );
        slider_u32(
            ui,
            "Altura de la cuchilla de bucle",
            &mut st.loop_height,
            ActionSettings::LOOP_HEIGHT,
        );
        slider_mm(ui, "Sobrecorte del lazo", &mut st.loop_overcut_mm, 2.5);
        slider_mm(ui, "Radio del lazo", &mut st.loop_radius_mm, 10.0);
        ui.checkbox(
            &mut st.disable_smart_cutting,
            "Deshabilitar corte inteligente",
        );
    }
    if action == Action::Stipple {
        slider_mm(ui, "Separación", &mut st.stipple_spacing_mm, 20.0);
    }
    ui.checkbox(&mut st.path_optimization, "Optimización de ruta")
        .on_hover_text("Ordena los trazos para recorrer menos distancia sin cortar.");
    if action.has_smart_path() {
        ui.checkbox(&mut st.smart_path, "Intelligent Path Technology™")
            .on_hover_text("Corta primero las piezas interiores para que el material no se mueva.");
    }
    if action == Action::Sketch {
        ui.checkbox(&mut st.sort_sketches, "Ordenar bocetos");
    }
    if action == Action::Binding {
        let mut on = st.backing_thickness_mm.is_some();
        if ui.checkbox(&mut on, "Espesor del soporte").changed() {
            st.backing_thickness_mm = on.then_some(1.0);
        }
        if let Some(t) = st.backing_thickness_mm.as_mut() {
            slider_mm(ui, "Espesor", t, 10.0);
        }
    }
}

fn name_dialog(app: &mut OpenCraftCutterApp, ctx: &egui::Context) {
    let Some(d) = app.canvas.send.name_dialog.as_mut() else {
        return;
    };
    let mut ok = false;
    let mut cancel = false;
    let title = if d.from.is_some() {
        "Guardar material como"
    } else {
        "Material nuevo"
    };
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label("Nombre");
            let r = ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(280.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ok = true;
            } else if !r.has_focus() && !r.lost_focus() {
                r.request_focus();
            }
            ui.horizontal(|ui| {
                ui.label("Familia");
                egui::ComboBox::from_id_salt("new_mat_family")
                    .selected_text(d.family.name())
                    .show_ui(ui, |ui| {
                        for f in Family::ALL {
                            ui.selectable_value(&mut d.family, f, f.name());
                        }
                    });
            });
            ui.label(
                RichText::new("La familia da valores de partida y sirve para filtrar la lista.")
                    .small()
                    .color(theme::CHROME_300),
            );
            ui.horizontal(|ui| {
                cancel = ui.button("Cancelar").clicked();
                ok |= ui.button("Guardar").clicked();
            });
        });
    if cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.canvas.send.name_dialog = None;
        app.canvas.send.test_save = None;
        return;
    }
    if !ok {
        return;
    }
    let name = d.name.trim().to_string();
    let (family, from) = (d.family, d.from);
    if name.is_empty() {
        return;
    }
    if app.materials_mut().iter().any(|m| m.name == name) {
        return app.notify(ctx, format!("Ya existe un material llamado «{name}»."));
    }
    let mut m = match from {
        Some(i) => {
            let mut m = app.prefs.materials[i].clone();
            m.name = name.clone();
            m.family = family;
            m.custom = true;
            m
        }
        None => Material::new(name.clone(), family, true),
    };
    m.custom = true;
    // «Guardar como» deja el original como estaba (también los cambios hechos en el panel).
    if let (Some(i), Some(ed)) = (from, app.canvas.send.editor.as_ref()) {
        if ed.material == i {
            app.prefs.materials[i] = ed.backup.clone();
        }
    }
    if let Some(i) = from {
        if let Some(b) = app.canvas.send.inline_backup.take() {
            if b.name == app.prefs.materials[i].name {
                app.prefs.materials[i] = b;
            }
        }
    }
    // «Guardar como…» desde un color: el material nuevo lleva sus ajustes y el color pasa a
    // usarlo, sin ajustes propios.
    if let Some((by_fill, row, holder, action, st)) = app.canvas.send.row_save.take() {
        *m.settings_mut(holder, action) = st;
        let s = &mut app.canvas.send;
        let rows = if by_fill {
            &mut s.fill_rows
        } else {
            &mut s.line_rows
        };
        if let Some(r) = rows.get_mut(row) {
            r.1.material = name.clone();
            r.1.settings = None;
        }
    }
    // «Guardar como material» desde un corte de prueba: los ajustes del intento, y quien lo
    // cortó (portaherramientas o fila de color) pasa a usar el material nuevo.
    let mut tab = app.canvas.send.holder_tab.number() as usize - 1;
    if let Some(a) = app
        .canvas
        .send
        .test_save
        .take()
        .and_then(|i| app.canvas.send.test_attempts.get(i).cloned())
    {
        *m.settings_mut(a.holder, a.action) = a.settings;
        tab = a.holder.number() as usize - 1;
        let s = &mut app.canvas.send;
        if let Some(c) = s.test_color {
            let rows = match s.mode {
                SendMode::ByFill => &mut s.fill_rows,
                _ => &mut s.line_rows,
            };
            if let Some((_, r)) = rows.iter_mut().find(|(rc, _)| *rc == c) {
                r.material = name.clone();
                r.settings = None;
            }
        }
    }
    app.prefs.materials.push(m.clone());
    let i = app.prefs.materials.len() - 1;
    let (holder, action) = app
        .canvas
        .send
        .editor
        .as_ref()
        .map(|e| (e.holder, e.action))
        .unwrap_or((Holder::One, Action::Cut));
    app.canvas.send.editor = Some(Editor {
        material: i,
        holder,
        action,
        backup: m,
    });
    // El material nuevo queda elegido en el portaherramientas visible (o en el de la prueba).
    app.canvas.send.setups[tab].material = name;
    app.canvas.send.name_dialog = None;
}

/// «Reflejar al cortar»: visible con vinil textil o si el usuario lo activó.
fn mirror_control(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let htv = app.mirrored_material();
    if htv.is_none() && app.canvas.send.mirror != Some(true) {
        return;
    }
    let mut on = app.mirror_on_cut();
    if ui
        .checkbox(&mut on, "Reflejar el diseño al cortar")
        .on_hover_text("El vinil textil se corta por el revés: el diseño tiene que ir en espejo para leerse bien al planchar.")
        .changed()
    {
        // Si coincide con lo automático, se vuelve a automático.
        let auto = htv.is_some();
        app.canvas.send.mirror = (on != auto).then_some(on);
    }
    if let Some(m) = htv {
        let text = if on {
            format!("«{m}» se corta en espejo: la vista previa ya lo muestra reflejado.")
        } else {
            format!("«{m}» se plancha: sin espejo el diseño quedará al revés en la prenda.")
        };
        ui.label(RichText::new(text).small().color(if on {
            theme::CHROME_300
        } else {
            Color32::from_rgb(0xfb, 0xbf, 0x24)
        }));
    }
    ui.add_space(4.0);
}

/// Sitio de la prueba, fila de color con la que se corta y registro de intentos.
fn test_controls(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let placing = app.canvas.send.placing_test;
    let r = ui
        .add(
            egui::Button::selectable(
                placing,
                format!(
                    "{}  Elegir sitio en el tapete",
                    <&str>::from(mi::ICON_ADS_CLICK)
                ),
            )
            .min_size(egui::vec2(ui.available_width(), 26.0)),
        )
        .on_hover_text(
            "Haz clic en un trozo libre del material para cortar ahí la prueba (Esc cancela)",
        );
    if r.clicked() {
        app.canvas.send.placing_test = !placing;
    }
    if placing {
        ui.label(
            RichText::new("Haz clic en el tapete donde quieras la prueba.")
                .small()
                .color(theme::ACCENT),
        );
    }

    // Por color: con qué fila (y por tanto con qué ajustes) se corta la prueba.
    let mode = app.canvas.send.mode;
    if matches!(mode, SendMode::ByLine | SendMode::ByFill) {
        let rows: Vec<(Color, String)> = {
            let s = &app.canvas.send;
            let rows = if mode == SendMode::ByFill {
                &s.fill_rows
            } else {
                &s.line_rows
            };
            rows.iter()
                .filter(|(_, r)| r.enabled)
                .map(|(c, r)| (*c, r.material.clone()))
                .collect()
        };
        let current = app
            .canvas
            .send
            .test_color
            .filter(|c| rows.iter().any(|(rc, _)| rc == c));
        let text = current
            .and_then(|c| rows.iter().find(|(rc, _)| *rc == c))
            .map(|(c, m)| format!("{} · {m}", c.to_hex()))
            .unwrap_or_else(|| "Su propio color".into());
        let mut pick = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Cortar la prueba como").small());
            egui::ComboBox::from_id_salt("test_row")
                .selected_text(text)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(current.is_none(), "Su propio color")
                        .clicked()
                    {
                        pick = Some(None);
                    }
                    for (c, m) in &rows {
                        if ui
                            .selectable_label(current == Some(*c), format!("{} · {m}", c.to_hex()))
                            .clicked()
                        {
                            pick = Some(Some(*c));
                        }
                    }
                });
        });
        if let Some(c) = pick {
            app.canvas.send.test_color = c;
            app.apply_test_color();
        }
    }

    if app.canvas.send.test_attempts.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Intentos de prueba").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Borrar lista").clicked() {
                app.canvas.send.test_attempts.clear();
            }
        });
    });
    let mut save = None;
    let mut mark = None;
    for (i, a) in app.canvas.send.test_attempts.iter().enumerate().rev() {
        Frame::NONE
            .fill(theme::CHROME_900)
            .corner_radius(4.0)
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("#{}  {}", a.number, a.summary())).small());
                ui.label(
                    RichText::new(format!("{} · {}", a.material, a.action.name()))
                        .small()
                        .color(theme::CHROME_300),
                );
                ui.horizontal(|ui| {
                    for (res, icon) in [
                        (send::TestResult::Good, mi::ICON_CHECK_CIRCLE),
                        (send::TestResult::Partial, mi::ICON_CONTRAST),
                        (send::TestResult::Bad, mi::ICON_CANCEL),
                    ] {
                        let r = ui
                            .add(egui::Button::selectable(
                                a.result == Some(res),
                                icon.rich_text().size(16.0),
                            ))
                            .on_hover_text(res.name());
                        theme::name_widget(&r, &format!("#{} {}", a.number, res.name()));
                        if r.clicked() {
                            mark = Some((i, res));
                        }
                    }
                    if let Some(res) = a.result {
                        ui.label(RichText::new(res.name()).small().color(theme::CHROME_200));
                    }
                    if a.result == Some(send::TestResult::Good)
                        && ui.small_button("Guardar como material").clicked()
                    {
                        save = Some(i);
                    }
                });
            });
    }
    if let Some((i, res)) = mark {
        let a = &mut app.canvas.send.test_attempts[i];
        a.result = (a.result != Some(res)).then_some(res);
    }
    if let Some(i) = save {
        app.save_test_attempt(i);
    }
}

/// Fecha local «dd/mm/aaaa hh:mm» de unos segundos Unix.
fn local_date(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%d/%m/%Y %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn history_window(app: &mut OpenCraftCutterApp, ctx: &egui::Context) {
    let mut open = app.canvas.send.show_history;
    if !open {
        return;
    }
    let now = crate::machine::now_unix();
    egui::Window::new("Historial de cortes y cuchillas")
        .open(&mut open)
        .collapsible(false)
        .default_width(420.0)
        .default_height(480.0)
        .show(ctx, |ui| {
            ui.label(RichText::new("Cuchillas").strong());
            if app.prefs.blades.is_empty() {
                ui.label(
                    RichText::new("Todavía no se ha cortado nada con cuchilla.")
                        .small()
                        .color(theme::CHROME_300),
                );
            }
            let life = app.prefs.blade_life_m;
            let mut changed = None;
            for (i, b) in app.prefs.blades.iter().enumerate() {
                ui.horizontal(|ui| {
                    let worn = b.meters >= life;
                    ui.label(RichText::new(format!(
                        "{} · {:.1} m desde el {}",
                        b.tool.name(),
                        b.meters,
                        local_date(b.since).split(' ').next().unwrap_or_default()
                    ))
                    .color(if worn { Color32::from_rgb(0xfb, 0xbf, 0x24) } else { theme::CHROME_100 }));
                    if ui.small_button("La he cambiado").clicked() {
                        changed = Some(i);
                    }
                });
            }
            if let Some(i) = changed {
                let b = &mut app.prefs.blades[i];
                b.meters = 0.0;
                b.since = now;
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Avisar a los").small());
                ui.add(
                    egui::DragValue::new(&mut app.prefs.blade_life_m)
                        .range(5.0..=2000.0)
                        .speed(1.0)
                        .suffix(" m"),
                );
            });
            ui.label(
                RichText::new("Es solo un recordatorio: lo que dura una cuchilla depende mucho del material.")
                    .small()
                    .color(theme::CHROME_300),
            );
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Envíos").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(!app.prefs.history.is_empty(), egui::Button::new("Borrar historial").small())
                        .clicked()
                    {
                        app.prefs.history.clear();
                    }
                });
            });
            if app.prefs.history.is_empty() {
                ui.label(
                    RichText::new("Aquí aparecerá cada envío a la máquina con sus ajustes.")
                        .small()
                        .color(theme::CHROME_300),
                );
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for e in &app.prefs.history {
                    Frame::NONE
                        .fill(theme::CHROME_900)
                        .corner_radius(4.0)
                        .inner_margin(6.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let result = e.result.as_ref().map(|r| r.name()).unwrap_or("En curso");
                            ui.label(
                                RichText::new(format!(
                                    "{} · {}{} · {}",
                                    local_date(e.when),
                                    e.document,
                                    if e.test { " (prueba)" } else { "" },
                                    result
                                ))
                                .small()
                                .strong(),
                            );
                            for j in &e.jobs {
                                let st = &j.settings;
                                ui.label(
                                    RichText::new(format!(
                                        "P{} {} · {} · prof. {} · presión {} · vel. {} · {} pasada{}",
                                        j.holder.number(),
                                        j.material,
                                        j.action.name(),
                                        st.depth,
                                        st.force,
                                        st.speed,
                                        st.passes,
                                        if st.passes == 1 { "" } else { "s" }
                                    ))
                                    .small()
                                    .color(theme::CHROME_200),
                                );
                            }
                            let mut tail = format!(
                                "{:.2} m · {}",
                                e.meters,
                                send::format_duration(e.estimate_s)
                            );
                            if let Some(oc_core::usage::JobResult::Failed(m)) = &e.result {
                                tail += &format!(" · {m}");
                            }
                            ui.label(RichText::new(tail).small().color(theme::CHROME_300));
                        });
                }
            });
        });
    app.canvas.send.show_history = open;
}

fn help_window(app: &mut OpenCraftCutterApp, ctx: &egui::Context) {
    let mut open = app.canvas.send.show_help;
    egui::Window::new("Problemas de conexión")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            for tip in [
                "Comprueba que la máquina está encendida y que el cable USB está conectado directamente al ordenador (sin concentradores).",
                "Si usas Bluetooth, enciéndelo en la máquina y en el ordenador, y acerca la máquina.",
                "Cierra otros programas que puedan estar usando la máquina.",
                "Apaga la máquina, espera unos segundos y vuelve a encenderla.",
                "Prueba con otro cable o puerto USB.",
            ] {
                ui.horizontal_wrapped(|ui| {
                    ui.label(mi::ICON_CHECK.rich_text().color(theme::ACCENT));
                    ui.label(tip);
                });
            }
        });
    app.canvas.send.show_help = open;
}

fn footer(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    if !app.canvas.send.hide_banner {
        Frame::NONE
            .fill(Color32::from_rgb(0x1c, 0x3d, 0x4a))
            .corner_radius(4.0)
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(mi::ICON_INFO.rich_text().color(theme::ACCENT));
                    if ui.link("Haz clic aquí").clicked() {
                        app.canvas.send.show_help = true;
                    }
                    ui.label(RichText::new("si tienes problemas de conexión.").small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(mi::ICON_CLOSE.rich_text().size(14.0))
                                    .frame(false),
                            )
                            .clicked()
                        {
                            app.canvas.send.hide_banner = true;
                        }
                    });
                });
            });
        ui.add_space(6.0);
    }
    let has_test = !send::test_shape_ids(&app.doc).is_empty();
    let w = ui.available_width();
    ui.horizontal(|ui| {
        let label = if has_test {
            "Eliminar forma de prueba"
        } else {
            "Generar forma de prueba"
        };
        if ui
            .add(
                egui::Button::new(format!(
                    "{}  {label}",
                    <&str>::from(mi::ICON_CHANGE_HISTORY)
                ))
                .min_size(egui::vec2(w * 0.58, 28.0)),
            )
            .on_hover_text("Un cuadrado de 10 mm con un triángulo para comprobar los ajustes")
            .clicked()
        {
            app.toggle_test_shape(&ctx);
        }
        let ready = app.machine.ready();
        if ui
            .add_enabled(
                ready && has_test,
                egui::Button::new("Enviar prueba").min_size(egui::vec2(ui.available_width(), 28.0)),
            )
            .on_disabled_hover_text(if !has_test {
                "Genera primero la forma de prueba"
            } else {
                app.machine
                    .not_ready_reason()
                    .unwrap_or("Conecta una máquina para cortar la forma de prueba")
            })
            .clicked()
        {
            app.machine_send(&ctx, true);
        }
    });
    test_controls(app, ui);
    ui.add_space(4.0);
    if let Some(s) = app.machine.sending {
        let frac = if s.total > 0 {
            s.sent as f32 / s.total as f32
        } else {
            0.0
        };
        let label = if s.registering {
            "Buscando marcas de registro…".to_string()
        } else if s.paused {
            "En pausa".to_string()
        } else if s.sent >= s.total && s.total > 0 {
            "Terminando…".to_string()
        } else {
            format!("Enviando… {:.0} %", frac * 100.0)
        };
        ui.add(egui::ProgressBar::new(frac).text(label).desired_width(w));
        ui.horizontal(|ui| {
            if s.paused && ui.button("Continuar").clicked() {
                app.machine_resume(&ctx);
            }
            if ui.button("Cancelar").clicked() {
                app.machine_cancel(&ctx);
            }
        });
        return;
    }
    // Si no hay nada que cortar se dice por qué, en lugar de no hacer nada al pulsar ENVIAR.
    mirror_control(app, ui);
    let nothing = app.nothing_to_cut();
    // Los avisos de marcas y de detalles no impiden enviar.
    let warnings: Vec<String> = match nothing {
        Some(why) => vec![why.to_string()],
        None => [app.marks_warning(), app.thin_warning(), app.blade_warning()]
            .into_iter()
            .flatten()
            .collect(),
    };
    for why in warnings {
        Frame::NONE
            .fill(Color32::from_rgb(0x4a, 0x3a, 0x12))
            .corner_radius(4.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        mi::ICON_WARNING
                            .rich_text()
                            .color(Color32::from_rgb(0xfb, 0xbf, 0x24)),
                    );
                    ui.label(RichText::new(why).color(theme::CHROME_100));
                });
            });
        ui.add_space(6.0);
    }
    let ready = app.machine.ready();
    if ui
        .add_enabled(
            ready && nothing.is_none(),
            egui::Button::new(RichText::new("ENVIAR").color(theme::CHROME_100).size(16.0))
                .fill(theme::ACCENT_STRONG)
                .min_size(egui::vec2(w, 36.0)),
        )
        .on_disabled_hover_text(nothing.unwrap_or_else(|| {
            app.machine
                .not_ready_reason()
                .unwrap_or("Conecta una máquina para enviar el trabajo")
        }))
        .clicked()
    {
        app.machine_send(&ctx, false);
    }
    if let Some(r) = app
        .machine
        .not_ready_reason()
        .filter(|_| app.machine.connected.is_some())
    {
        ui.label(RichText::new(r).small().color(theme::CHROME_300));
    }
}

pub fn send_panel(app: &mut OpenCraftCutterApp, ui: &mut Ui) {
    app.sync_rows();
    let ctx = ui.ctx().clone();
    egui::Panel::right("send_panel")
        .exact_size(460.0)
        .resizable(false)
        .frame(
            Frame::NONE
                .fill(theme::CHROME_900)
                .inner_margin(egui::Margin::same(12)),
        )
        .show(ui, |ui| {
            mode_bar(app, ui);
            ui.add_space(6.0);
            machine_card(app, ui);
            ui.add_space(8.0);
            egui::Panel::bottom("send_footer")
                .frame(
                    Frame::NONE
                        .fill(theme::CHROME_900)
                        .inner_margin(egui::Margin::symmetric(0, 8)),
                )
                .show(ui, |ui| footer(app, ui));
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width());
                    if app.canvas.send.editor.is_some() {
                        editor_view(app, ui);
                        return;
                    }
                    match app.canvas.send.mode {
                        SendMode::Guide => guide_mode(app, ui),
                        SendMode::Standard => standard_mode(app, ui),
                        SendMode::ByLine => color_mode(app, ui, false),
                        SendMode::ByFill => color_mode(app, ui, true),
                    }
                    if app.canvas.send.mode != SendMode::Guide {
                        ui.add_space(10.0);
                        ui.separator();
                        let jobs = app.send_jobs();
                        simulation_controls(app, ui, &jobs);
                        job_summary(ui, &jobs);
                        if ui
                            .small_button(format!(
                                "{}  Historial de cortes y cuchillas",
                                <&str>::from(mi::ICON_HISTORY)
                            ))
                            .clicked()
                        {
                            app.canvas.send.show_history = true;
                        }
                    }
                });
        });
    name_dialog(app, &ctx);
    help_window(app, &ctx);
    history_window(app, &ctx);
}
