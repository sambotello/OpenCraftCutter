//! Enviar: modelo de los ajustes de corte (materiales, acciones, herramientas, portaherramientas)
//! y preparación de los trabajos que recibirá el controlador de la máquina. Sin E/S.

use crate::color::Color;
use crate::document::Document;
use crate::ops::{self, Polygons};
use crate::shape::{Shape, ShapeId};
use i_overlay::core::fill_rule::FillRule as IoFillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use kurbo::{Point, Rect};
use serde::{Deserialize, Serialize};

/// Polilínea en mm con su indicador de cierre.
pub type Polyline = (Vec<Point>, bool);
/// Líneas de un portaherramientas.
pub type HolderPaths = (Holder, Vec<Polyline>);
/// Ajustes de un material incluido: (portaherramientas, acción, presión, velocidad, pasadas,
/// aceleración, profundidad).
type PresetEntry = (Holder, Action, u32, u32, u32, u32, u32);
/// Material incluido: nombre, familia y sus ajustes.
type Preset = (&'static str, Family, &'static [PresetEntry]);

/// Portaherramientas de la máquina (la Cameo 5 tiene dos carros).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub enum Holder {
    #[default]
    One,
    Two,
}

impl Holder {
    pub const ALL: [Holder; 2] = [Holder::One, Holder::Two];

    pub fn name(self) -> &'static str {
        match self {
            Holder::One => "Portaherramientas 1",
            Holder::Two => "Portaherramientas 2",
        }
    }

    pub fn number(self) -> u8 {
        match self {
            Holder::One => 1,
            Holder::Two => 2,
        }
    }
}

/// Qué líneas de un objeto usa el portaherramientas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CutStyle {
    /// Todas las líneas del objeto.
    #[default]
    All,
    /// Solo el contorno exterior de la selección.
    Edge,
    /// Ninguna (no se envía).
    Skip,
}

/// Asignación de corte de un objeto (botones bajo la vista previa de Enviar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct CutAssign {
    pub holder: Holder,
    pub style: CutStyle,
}

/// Acción que realiza una herramienta sobre las líneas.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub enum Action {
    /// Sin acción elegida: se usa la primera compatible con la herramienta.
    #[default]
    Auto,
    Cut,
    Sketch,
    /// Puntos sueltos a lo largo de la línea (antes «Perforate»).
    #[serde(alias = "Perforate")]
    Stipple,
    /// Marcado para doblar.
    Score,
    /// Corte que deja pequeños puentes para desprender la pieza.
    PopOut,
    /// Encuadernado: marca de pliegue sobre un soporte grueso.
    Binding,
    KraftCut,
    RotaryCut,
    /// Perforación con punzón (puntos de diámetro fijo).
    Punch,
    Emboss,
}

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Action::Auto => "Auto (ninguna acción)",
            Action::Cut => "Cortar",
            Action::Sketch => "Esbozar",
            Action::Stipple => "Punteado",
            Action::Score => "Marcar",
            Action::PopOut => "Corte desplegable",
            Action::Binding => "Encuadernar",
            Action::KraftCut => "Corte Kraft",
            Action::RotaryCut => "Corte giratorio",
            Action::Punch => "Perforar",
            Action::Emboss => "Relieve",
        }
    }

    /// Acciones que admite un portaherramientas, en el orden de la referencia.
    pub fn for_holder(holder: Holder) -> &'static [Action] {
        match holder {
            Holder::One => &[
                Action::Cut,
                Action::Sketch,
                Action::Stipple,
                Action::KraftCut,
                Action::Score,
                Action::PopOut,
                Action::Binding,
            ],
            Holder::Two => &[
                Action::KraftCut,
                Action::Sketch,
                Action::Punch,
                Action::RotaryCut,
                Action::Binding,
                Action::Emboss,
            ],
        }
    }

    /// Lleva sobrecorte de segmento de línea.
    pub fn has_overcut(self) -> bool {
        matches!(
            self,
            Action::Cut | Action::KraftCut | Action::PopOut | Action::RotaryCut
        )
    }

    /// Admite Intelligent Path (cortar primero lo interior).
    pub fn has_smart_path(self) -> bool {
        matches!(
            self,
            Action::Cut | Action::KraftCut | Action::PopOut | Action::RotaryCut
        )
    }

    /// Usa profundidad de cuchilla.
    pub fn has_depth(self) -> bool {
        matches!(
            self,
            Action::Cut | Action::Stipple | Action::KraftCut | Action::PopOut
        )
    }
}

/// Herramientas que se pueden montar en la máquina.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Tool {
    #[default]
    AutoBlade,
    RatchetBlade,
    DeepCutBlade,
    KraftBlade,
    RotaryBlade,
    SketchPen,
    PenAdapter,
    PerforationBlade,
    EmbossingTool,
    EngravingTool,
    HeatPen,
    LeatherPunch,
    LeatherCarver,
}

impl Tool {
    pub const ALL: [Tool; 13] = [
        Tool::AutoBlade,
        Tool::RatchetBlade,
        Tool::DeepCutBlade,
        Tool::KraftBlade,
        Tool::RotaryBlade,
        Tool::SketchPen,
        Tool::PenAdapter,
        Tool::PerforationBlade,
        Tool::EmbossingTool,
        Tool::EngravingTool,
        Tool::HeatPen,
        Tool::LeatherPunch,
        Tool::LeatherCarver,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tool::AutoBlade => "Cuchilla automática",
            Tool::RatchetBlade => "Cuchilla de trinquete",
            Tool::DeepCutBlade => "Cuchilla de corte profundo",
            Tool::KraftBlade => "Cuchilla kraft (2 mm)",
            Tool::RotaryBlade => "Cuchilla rotatoria",
            Tool::SketchPen => "Rotulador de esbozo",
            Tool::PenAdapter => "Adaptador de bolígrafo",
            Tool::PerforationBlade => "Cuchilla de perforado",
            Tool::EmbossingTool => "Herramienta de relieve",
            Tool::EngravingTool => "Herramienta de grabado",
            Tool::HeatPen => "Lápiz térmico",
            Tool::LeatherPunch => "Punzón de cuero",
            Tool::LeatherCarver => "Tallador de cuero",
        }
    }

    /// Portaherramientas donde se puede montar.
    pub fn holders(self) -> &'static [Holder] {
        match self {
            Tool::AutoBlade | Tool::RatchetBlade | Tool::DeepCutBlade | Tool::PerforationBlade => {
                &[Holder::One]
            }
            Tool::KraftBlade | Tool::RotaryBlade | Tool::LeatherPunch | Tool::LeatherCarver => {
                &[Holder::Two]
            }
            _ => &[Holder::One, Holder::Two],
        }
    }

    /// Acciones que puede realizar.
    pub fn actions(self) -> &'static [Action] {
        match self {
            Tool::AutoBlade => &[
                Action::Cut,
                Action::Stipple,
                Action::Score,
                Action::PopOut,
                Action::Binding,
            ],
            Tool::RatchetBlade | Tool::DeepCutBlade => {
                &[Action::Cut, Action::PopOut, Action::Binding]
            }
            Tool::PerforationBlade => &[Action::Cut, Action::Stipple],
            Tool::KraftBlade => &[Action::KraftCut, Action::Binding],
            Tool::RotaryBlade => &[Action::RotaryCut],
            Tool::SketchPen | Tool::PenAdapter => &[Action::Sketch, Action::Stipple],
            Tool::EmbossingTool => &[Action::Emboss, Action::Score],
            Tool::EngravingTool => &[Action::Emboss, Action::Sketch],
            Tool::HeatPen => &[Action::Sketch],
            Tool::LeatherPunch => &[Action::Punch],
            Tool::LeatherCarver => &[Action::Emboss],
        }
    }

    /// La profundidad se ajusta a mano (en la propia cuchilla) en vez de automáticamente.
    pub fn manual_depth(self) -> bool {
        matches!(
            self,
            Tool::RatchetBlade | Tool::DeepCutBlade | Tool::KraftBlade
        )
    }

    /// Tiene profundidad de cuchilla (automática o manual).
    pub fn has_depth(self) -> bool {
        matches!(
            self,
            Tool::AutoBlade | Tool::RatchetBlade | Tool::DeepCutBlade | Tool::KraftBlade
        )
    }

    pub fn compatible(self, holder: Holder, action: Action) -> bool {
        self.holders().contains(&holder)
            && (action == Action::Auto || self.actions().contains(&action))
    }

    /// Herramientas que admite un portaherramientas.
    pub fn for_holder(holder: Holder) -> Vec<Tool> {
        Tool::ALL
            .into_iter()
            .filter(|t| t.holders().contains(&holder))
            .collect()
    }

    /// Herramientas de un portaherramientas que pueden hacer la acción (`Auto`: todas).
    pub fn for_action(holder: Holder, action: Action) -> Vec<Tool> {
        Tool::ALL
            .into_iter()
            .filter(|t| t.compatible(holder, action))
            .collect()
    }
}

/// Acción efectiva: `Auto` se resuelve a la primera que admite la herramienta.
pub fn resolve_action(action: Action, tool: Tool) -> Action {
    if action == Action::Auto {
        tool.actions()[0]
    } else {
        action
    }
}

/// Familia de material (sirve para filtrar la lista y para valores de partida).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Family {
    #[default]
    Card,
    Paper,
    Vinyl,
    HeatTransfer,
    Sticker,
    Fabric,
    Adhesive,
    Other,
}

impl Family {
    pub const ALL: [Family; 8] = [
        Family::Sticker,
        Family::HeatTransfer,
        Family::Vinyl,
        Family::Card,
        Family::Paper,
        Family::Fabric,
        Family::Adhesive,
        Family::Other,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Family::Card => "Tarjeta",
            Family::Paper => "Papel",
            Family::Vinyl => "Vinilo",
            Family::HeatTransfer => "Transferencia térmica",
            Family::Sticker => "Pegatina",
            Family::Fabric => "Tela",
            Family::Adhesive => "Adhesivo",
            Family::Other => "Otro",
        }
    }

    /// Anchura mínima de un detalle (mm) para que no se rompa al cortar o al pelar.
    pub fn min_detail_mm(self) -> f64 {
        match self {
            Family::Paper | Family::Card => 0.8,
            Family::Fabric => 2.0,
            Family::HeatTransfer => 1.2,
            Family::Vinyl | Family::Sticker | Family::Adhesive | Family::Other => 1.0,
        }
    }
}

/// Ajustes de una acción para un material y portaherramientas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActionSettings {
    /// Presión (1–33).
    pub force: u32,
    /// Velocidad (1–30).
    pub speed: u32,
    /// Pasadas (1–10).
    pub passes: u32,
    /// Aceleración (1–9).
    pub accel: u32,
    /// Profundidad de la cuchilla (nivel 1–10).
    pub depth: u32,
    /// Sobrecorte de segmento de línea: prolonga cada tramo recto en las esquinas.
    pub overcut: bool,
    pub overcut_start_mm: f64,
    pub overcut_end_mm: f64,
    /// Ordena los trazos para recorrer menos distancia en vacío.
    pub path_optimization: bool,
    /// Intelligent Path: corta primero lo que menos se sujeta al tapete.
    pub smart_path: bool,
    /// Ordena los esbozos (de dentro afuera y por proximidad).
    pub sort_sketches: bool,
    /// Mejora de arrastre: pasa el material adelante y atrás antes de cortar.
    pub track_enhancing: bool,
    /// Punteado: distancia entre puntos (mm).
    pub stipple_spacing_mm: f64,
    /// Corte Kraft: fuerza y altura de la cuchilla en los bucles de las esquinas.
    pub loop_force: u32,
    pub loop_height: u32,
    /// Corte Kraft: sobrecorte y radio del lazo (mm).
    pub loop_overcut_mm: f64,
    pub loop_radius_mm: f64,
    /// Corte Kraft: sin el corte inteligente de esquinas.
    pub disable_smart_cutting: bool,
    /// Encuadernar: espesor del soporte (mm), si se indica.
    pub backing_thickness_mm: Option<f64>,
}

impl Default for ActionSettings {
    fn default() -> Self {
        ActionSettings {
            force: 15,
            speed: 5,
            passes: 1,
            accel: 1,
            depth: 2,
            overcut: false,
            overcut_start_mm: 0.1,
            overcut_end_mm: 0.1,
            path_optimization: false,
            smart_path: false,
            sort_sketches: true,
            track_enhancing: false,
            stipple_spacing_mm: 2.0,
            loop_force: 1,
            loop_height: 0,
            loop_overcut_mm: 2.5,
            loop_radius_mm: 5.0,
            disable_smart_cutting: false,
            backing_thickness_mm: None,
        }
    }
}

impl ActionSettings {
    pub const FORCE: std::ops::RangeInclusive<u32> = 1..=33;
    pub const SPEED: std::ops::RangeInclusive<u32> = 1..=30;
    pub const PASSES: std::ops::RangeInclusive<u32> = 1..=10;
    pub const ACCEL: std::ops::RangeInclusive<u32> = 1..=9;
    /// Niveles de profundidad de la cuchilla, como los del anillo de la propia cuchilla.
    pub const DEPTH: std::ops::RangeInclusive<u32> = 1..=10;
    /// La cuchilla Kraft tiene más niveles que la automática.
    pub const DEPTH_KRAFT: std::ops::RangeInclusive<u32> = 1..=20;
    pub const LOOP_HEIGHT: std::ops::RangeInclusive<u32> = 0..=10;

    fn cut(force: u32, speed: u32, passes: u32, depth: u32) -> Self {
        ActionSettings {
            force,
            speed,
            passes,
            depth,
            ..Default::default()
        }
    }

    /// Valores de partida de una acción para una familia.
    pub fn for_action(action: Action, family: Family) -> Self {
        let (force, speed, passes, depth) = family_defaults(family);
        match action {
            Action::Sketch => ActionSettings {
                force: 7,
                speed: 5,
                accel: 3,
                ..Default::default()
            },
            Action::Stipple => ActionSettings::cut(33, 5, 1, depth),
            Action::Score | Action::Emboss => ActionSettings {
                force: 20,
                speed: 3,
                ..Default::default()
            },
            Action::Punch => ActionSettings {
                force: 33,
                speed: 1,
                ..Default::default()
            },
            _ => ActionSettings::cut(force, speed, passes, depth),
        }
    }

    /// Niveles de profundidad válidos para una acción.
    pub fn depth_range(action: Action) -> std::ops::RangeInclusive<u32> {
        if action == Action::KraftCut {
            Self::DEPTH_KRAFT
        } else {
            Self::DEPTH
        }
    }

    /// Recorta los valores a sus rangos.
    pub fn clamped(mut self) -> Self {
        let c = |v: u32, r: std::ops::RangeInclusive<u32>| v.clamp(*r.start(), *r.end());
        self.force = c(self.force, Self::FORCE);
        self.speed = c(self.speed, Self::SPEED);
        self.passes = c(self.passes, Self::PASSES);
        self.accel = c(self.accel, Self::ACCEL);
        self.depth = c(self.depth, Self::DEPTH_KRAFT);
        self.loop_force = c(self.loop_force, Self::FORCE);
        self.loop_height = c(self.loop_height, Self::LOOP_HEIGHT);
        self.overcut_start_mm = self.overcut_start_mm.clamp(0.0, 1.0);
        self.overcut_end_mm = self.overcut_end_mm.clamp(0.0, 1.0);
        self.stipple_spacing_mm = self.stipple_spacing_mm.clamp(0.2, 50.0);
        self.loop_overcut_mm = self.loop_overcut_mm.clamp(0.0, 2.5);
        self.loop_radius_mm = self.loop_radius_mm.clamp(0.0, 10.0);
        self.backing_thickness_mm = self.backing_thickness_mm.map(|t| t.clamp(0.0, 10.0));
        self
    }
}

/// Ajustes guardados de un material para (portaherramientas, acción).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialEntry {
    pub holder: Holder,
    pub action: Action,
    pub settings: ActionSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub family: Family,
    /// Creado por el usuario (aparece también en «A medida» y se puede borrar).
    #[serde(default)]
    pub custom: bool,
    pub entries: Vec<MaterialEntry>,
}

impl Material {
    /// Vinil textil (de planchar): se corta por el revés, así que el diseño va en espejo. Los
    /// imprimibles no, porque se cortan con la impresión hacia arriba.
    pub fn cut_mirrored(&self) -> bool {
        self.family == Family::HeatTransfer && !self.name.to_lowercase().contains("imprimible")
    }

    /// Material nuevo: Cortar en el portaherramientas 1 y Corte Kraft en el 2.
    pub fn new(name: impl Into<String>, family: Family, custom: bool) -> Material {
        let mut m = Material {
            name: name.into(),
            family,
            custom,
            entries: Vec::new(),
        };
        m.add_action(Holder::One, Action::Cut);
        m.add_action(Holder::Two, Action::KraftCut);
        m
    }

    pub fn settings(&self, holder: Holder, action: Action) -> Option<&ActionSettings> {
        self.entries
            .iter()
            .find(|e| e.holder == holder && e.action == action)
            .map(|e| &e.settings)
    }

    /// Ajustes de la acción; si no existen se crean a partir de los de la familia.
    pub fn settings_mut(&mut self, holder: Holder, action: Action) -> &mut ActionSettings {
        if self.settings(holder, action).is_none() {
            self.add_action(holder, action);
        }
        let i = self
            .entries
            .iter()
            .position(|e| e.holder == holder && e.action == action)
            .expect("recién añadida");
        &mut self.entries[i].settings
    }

    /// Añade una acción con valores de partida (no hace nada si ya existe).
    pub fn add_action(&mut self, holder: Holder, action: Action) {
        if action == Action::Auto || self.settings(holder, action).is_some() {
            return;
        }
        self.entries.push(MaterialEntry {
            holder,
            action,
            settings: ActionSettings::for_action(action, self.family),
        });
    }

    pub fn remove_action(&mut self, holder: Holder, action: Action) {
        self.entries
            .retain(|e| !(e.holder == holder && e.action == action));
    }

    /// Acciones con ajustes guardados para un portaherramientas, en el orden de la referencia.
    pub fn actions_for(&self, holder: Holder) -> Vec<Action> {
        Action::for_holder(holder)
            .iter()
            .copied()
            .filter(|a| self.settings(holder, *a).is_some())
            .collect()
    }

    /// Acciones que aún se pueden añadir a un portaherramientas.
    pub fn addable_actions(&self, holder: Holder) -> Vec<Action> {
        Action::for_holder(holder)
            .iter()
            .copied()
            .filter(|a| self.settings(holder, *a).is_none())
            .collect()
    }
}

/// Presión, velocidad, pasadas y profundidad de partida de cada familia.
fn family_defaults(f: Family) -> (u32, u32, u32, u32) {
    match f {
        Family::Card => (20, 5, 1, 4),
        Family::Paper => (10, 6, 1, 2),
        Family::Vinyl => (8, 8, 1, 1),
        Family::HeatTransfer => (10, 8, 1, 2),
        Family::Sticker => (12, 6, 1, 2),
        Family::Fabric => (18, 3, 2, 5),
        Family::Adhesive => (12, 5, 1, 3),
        Family::Other => (15, 5, 1, 3),
    }
}

/// Material elegido por omisión en los portaherramientas.
pub const DEFAULT_MATERIAL: &str = "Cartulina, lisa";

/// Versión de la lista de materiales incluidos: al subirla, las listas guardadas recuperan los
/// incluidos nuevos (los materiales propios se conservan).
pub const BUILTIN_MATERIALS_VERSION: u32 = 4;

/// Materiales incluidos con «Sobrecorte de segmento de línea» activado en la referencia.
const OVERCUT: &[&str] = &[
    "Madera prensada",
    "Papel corrugado",
    "Papel de portada, sólido (105 lb-122 lb)",
    "Goma Eva",
];
/// Materiales incluidos con «Optimización de ruta» activada en la referencia.
const PATH_OPTIMIZATION: &[&str] = &["Hojas de acetato (finas)"];

/// Materiales incluidos, con los ajustes de la referencia para la Cameo 5 (cuchilla automática en
/// el portaherramientas 1; Kraft, giratoria o esbozo donde la referencia no corta en el 1).
pub fn builtin_materials() -> Vec<Material> {
    use Action::{Cut, KraftCut, RotaryCut, Sketch, Stipple};
    use Holder::{One as H1, Two as H2};
    let list: &[Preset] = &[
        (
            "Hojas de acetato (finas)",
            Family::Other,
            &[(H1, Cut, 33, 3, 2, 1, 1), (H1, KraftCut, 4, 3, 3, 1, 3)],
        ),
        (
            "Madera de balsa",
            Family::Other,
            &[(H2, KraftCut, 8, 3, 2, 1, 11)],
        ),
        (
            "Hoja adhesiva metálica cepillada",
            Family::Sticker,
            &[(H1, Cut, 25, 3, 1, 1, 3)],
        ),
        (
            "Cartulina, adhesiva",
            Family::Adhesive,
            &[(H1, Cut, 29, 3, 1, 3, 4)],
        ),
        (
            "Cartulina, pizarra",
            Family::Card,
            &[(H1, Cut, 29, 3, 1, 3, 4)],
        ),
        (
            "Cartulina, purpurina",
            Family::Card,
            &[(H1, Cut, 33, 3, 2, 3, 6)],
        ),
        (
            "Cartulina, lisa",
            Family::Card,
            &[(H1, Cut, 20, 4, 1, 3, 3)],
        ),
        (
            "Cartulina, imprimible y adhesivo",
            Family::Adhesive,
            &[(H1, Cut, 29, 3, 1, 3, 4)],
        ),
        (
            "Cartulina, con relieve - gruesa (80 lb)",
            Family::Card,
            &[(H1, Cut, 30, 4, 1, 3, 3)],
        ),
        (
            "Cartulina fina",
            Family::Card,
            &[(H1, Cut, 15, 10, 1, 3, 3)],
        ),
        (
            "Papel de copia, medio",
            Family::Paper,
            &[(H1, Cut, 15, 10, 1, 3, 3)],
        ),
        (
            "Madera prensada",
            Family::Other,
            &[(H1, Cut, 33, 1, 2, 3, 4)],
        ),
        (
            "Cartón prensado (grueso)",
            Family::Other,
            &[(H1, KraftCut, 12, 3, 1, 1, 9)],
        ),
        (
            "Láminas de corcho",
            Family::Other,
            &[(H1, Cut, 20, 1, 1, 3, 7)],
        ),
        (
            "Papel corrugado",
            Family::Paper,
            &[(H1, Cut, 33, 3, 1, 3, 8)],
        ),
        (
            "Lienzo de algodón, imprimible",
            Family::Fabric,
            &[(H1, Cut, 33, 3, 2, 3, 5)],
        ),
        (
            "Tela de algodón, imprimible",
            Family::Fabric,
            &[(H1, Cut, 33, 5, 1, 3, 3)],
        ),
        (
            "Papel de portada, sólido (105 lb-122 lb)",
            Family::Card,
            &[(H1, Cut, 33, 1, 2, 3, 7)],
        ),
        ("Goma Eva", Family::Other, &[(H1, Cut, 4, 4, 2, 3, 10)]),
        (
            "Papel de crepe",
            Family::Paper,
            &[(H1, KraftCut, 12, 5, 2, 1, 7)],
        ),
        (
            "Adhesivo de doble cara",
            Family::Adhesive,
            &[(H1, Cut, 33, 3, 2, 3, 4)],
        ),
        (
            "Cinta de embalar, imprimible",
            Family::Adhesive,
            &[(H1, Cut, 20, 4, 1, 3, 3)],
        ),
        (
            "Tela, grueso (lienzo)",
            Family::Fabric,
            &[(H1, Cut, 33, 5, 1, 3, 5)],
        ),
        (
            "Tela, fino (impresión algodón)",
            Family::Fabric,
            &[(H1, Cut, 33, 5, 1, 3, 3)],
        ),
        (
            "Tejido, lino",
            Family::Fabric,
            &[(H2, RotaryCut, 23, 5, 1, 1, 1)],
        ),
        (
            "Tejido, lana",
            Family::Fabric,
            &[(H2, RotaryCut, 20, 5, 1, 1, 1)],
        ),
        (
            "Papel de piel sintética",
            Family::Paper,
            &[(H2, Sketch, 7, 5, 1, 3, 1)],
        ),
        (
            "Fieltro, acrílico",
            Family::Fabric,
            &[(H2, RotaryCut, 22, 5, 1, 1, 1)],
        ),
        (
            "Fieltro, poliéster",
            Family::Fabric,
            &[(H2, RotaryCut, 19, 5, 1, 1, 1)],
        ),
        (
            "Papel de aluminio, imprimible",
            Family::Adhesive,
            &[(H1, Cut, 33, 5, 1, 3, 5)],
        ),
        (
            "Transferencia de calor, tela",
            Family::HeatTransfer,
            &[(H1, Cut, 10, 3, 1, 1, 2)],
        ),
        (
            "Transferencia de calor, terciopelo",
            Family::HeatTransfer,
            &[(H1, Cut, 33, 5, 1, 3, 3)],
        ),
        (
            "Transferencia de calor, purpurina",
            Family::HeatTransfer,
            &[(H1, Cut, 33, 5, 1, 3, 3)],
        ),
        (
            "Transferencia de calor, fosforescente",
            Family::HeatTransfer,
            &[(H1, Cut, 10, 6, 1, 1, 2)],
        ),
        (
            "Transferencia de calor, iridiscente",
            Family::HeatTransfer,
            &[(H1, Cut, 10, 6, 1, 3, 2)],
        ),
        (
            "Transferencia de calor, metálico",
            Family::HeatTransfer,
            &[(H1, Cut, 33, 5, 1, 3, 3)],
        ),
        (
            "Transferencia de calor – imprimible (oscuro)",
            Family::HeatTransfer,
            &[(H1, Cut, 8, 3, 1, 3, 1)],
        ),
        (
            "Transferencia de calor, tela imprimible",
            Family::HeatTransfer,
            &[(H1, Cut, 29, 5, 2, 1, 3)],
        ),
        (
            "Transferencia de calor – imprimible (claro)",
            Family::HeatTransfer,
            &[(H1, Cut, 2, 3, 1, 3, 1)],
        ),
        (
            "Transferencia de calor, reflectante",
            Family::HeatTransfer,
            &[(H1, Cut, 10, 6, 1, 3, 1)],
        ),
        (
            "Transferencia de calor, suave",
            Family::HeatTransfer,
            &[(H1, Cut, 4, 8, 1, 3, 2)],
        ),
        (
            "Papel de transferencia de imagen",
            Family::Paper,
            &[(H1, Cut, 30, 3, 1, 3, 4)],
        ),
        (
            "Hoja de pegatinas holográficas, patrón de puntos",
            Family::Sticker,
            &[(H1, Cut, 33, 4, 2, 1, 5)],
        ),
        (
            "Papel Kraft, adhesivo",
            Family::Sticker,
            &[(H1, Cut, 20, 3, 1, 3, 3)],
        ),
        (
            "Cuero sintético",
            Family::Fabric,
            &[(H1, Cut, 33, 3, 2, 3, 6)],
        ),
        (
            "Cuero, textil",
            Family::Fabric,
            &[(H2, KraftCut, 10, 4, 3, 1, 17)],
        ),
        (
            "Cuero, ante",
            Family::Fabric,
            &[(H2, KraftCut, 10, 5, 2, 1, 20)],
        ),
        (
            "Papel imantado, adhesivo",
            Family::Adhesive,
            &[(H1, Cut, 33, 1, 2, 3, 6)],
        ),
        (
            "Papel imantado, imprimible",
            Family::Sticker,
            &[(H1, Cut, 30, 1, 1, 3, 4)],
        ),
        (
            "Láminas metálicas, punteado",
            Family::Other,
            &[(H1, Stipple, 33, 8, 1, 3, 1)],
        ),
        (
            "Papel fotográfico",
            Family::Paper,
            &[(H1, Cut, 15, 10, 1, 3, 3)],
        ),
        (
            "Material de plantilla de pedrería Silhouette",
            Family::Other,
            &[(H1, Cut, 33, 5, 2, 3, 6)],
        ),
        (
            "Papel de marcado y relieve",
            Family::Paper,
            &[(H1, Cut, 20, 5, 1, 3, 2)],
        ),
        (
            "Hoja de pegatina de rascar",
            Family::Sticker,
            &[(H1, Cut, 14, 7, 1, 3, 2)],
        ),
        (
            "Plástico retráctil",
            Family::Other,
            &[(H1, Cut, 33, 1, 2, 3, 6)],
        ),
        (
            "Material Estampado",
            Family::Other,
            &[(H1, Cut, 18, 1, 1, 3, 9)],
        ),
        (
            "Material de plantilla",
            Family::Other,
            &[(H1, Cut, 33, 2, 1, 3, 3)],
        ),
        (
            "Papel de pegatina, claro",
            Family::Sticker,
            &[(H1, Cut, 25, 4, 2, 3, 2)],
        ),
        (
            "Papel adhesivo, brillantina",
            Family::Sticker,
            &[(H1, Cut, 25, 2, 1, 3, 4)],
        ),
        (
            "Papel de pegatina, brilla en la oscuridad",
            Family::Sticker,
            &[(H1, Cut, 33, 4, 2, 3, 3)],
        ),
        (
            "Hoja de pegatinas holográficas",
            Family::Sticker,
            &[(H1, Cut, 33, 4, 2, 1, 5)],
        ),
        (
            "Papel de pegatina, iridiscente",
            Family::Sticker,
            &[(H1, Cut, 29, 4, 2, 1, 4)],
        ),
        (
            "Papel de pegatina, blanco",
            Family::Sticker,
            &[(H1, Cut, 14, 8, 1, 3, 2)],
        ),
        (
            "Papel de tatuaje temporal, metálico",
            Family::Paper,
            &[(H1, Cut, 25, 3, 1, 3, 2)],
        ),
        (
            "Papel de tatuajes temporales, imprimible",
            Family::Paper,
            &[(H1, Cut, 25, 3, 1, 3, 4)],
        ),
        (
            "Vinilo, fibra de carbono",
            Family::Vinyl,
            &[(H1, Cut, 10, 5, 1, 3, 1)],
        ),
        (
            "Vinilo, esmerilado",
            Family::Vinyl,
            &[(H1, Cut, 10, 5, 1, 3, 1)],
        ),
        (
            "Vinilo, brillante",
            Family::Vinyl,
            &[(H1, Cut, 10, 5, 1, 3, 1)],
        ),
        (
            "Vinilo, brillo",
            Family::Vinyl,
            &[(H1, Cut, 10, 5, 1, 1, 4)],
        ),
        (
            "Vinilo, brilla en la oscuridad",
            Family::Vinyl,
            &[(H1, Cut, 10, 7, 1, 3, 2)],
        ),
        ("Vinilo, mate", Family::Vinyl, &[(H1, Cut, 10, 5, 1, 3, 1)]),
        (
            "Vinilo, metálico",
            Family::Vinyl,
            &[(H1, Cut, 10, 5, 1, 3, 1)],
        ),
        (
            "Vinilo, imprimible",
            Family::Vinyl,
            &[(H1, Cut, 20, 5, 1, 3, 1)],
        ),
        (
            "Vinilo, reflectante",
            Family::Vinyl,
            &[(H1, Cut, 10, 5, 1, 3, 1)],
        ),
        (
            "Vinilo, Silhouette Oracal 651",
            Family::Vinyl,
            &[(H1, Cut, 10, 5, 1, 3, 1)],
        ),
        (
            "Vinilo, texturado traslúcido",
            Family::Vinyl,
            &[(H1, Cut, 25, 5, 1, 3, 4)],
        ),
        (
            "Vinilo, Pizarra blanca",
            Family::Vinyl,
            &[(H1, Cut, 33, 5, 1, 3, 2)],
        ),
        (
            "Vinilo, madera",
            Family::Vinyl,
            &[(H1, Cut, 10, 5, 1, 3, 1)],
        ),
        (
            "Papel Washi",
            Family::Adhesive,
            &[(H1, Cut, 17, 7, 1, 3, 1)],
        ),
        (
            "Adherente a ventana, imprimible",
            Family::Adhesive,
            &[(H1, Cut, 14, 8, 1, 3, 2)],
        ),
        (
            "Papel madera",
            Family::Adhesive,
            &[(H1, Cut, 32, 4, 1, 3, 6)],
        ),
    ];
    list.iter()
        .map(|&(name, family, entries)| {
            let mut m = Material {
                name: name.to_string(),
                family,
                custom: false,
                entries: entries
                    .iter()
                    .map(
                        |&(holder, action, force, speed, passes, accel, depth)| MaterialEntry {
                            holder,
                            action,
                            settings: ActionSettings {
                                accel,
                                ..ActionSettings::cut(force, speed, passes, depth)
                            }
                            .clamped(),
                        },
                    )
                    .collect(),
            };
            // Casillas marcadas en la referencia para el corte del portaherramientas 1.
            if let Some(st) = m
                .entries
                .iter_mut()
                .find(|e| e.holder == H1 && e.action == Cut)
                .map(|e| &mut e.settings)
            {
                st.overcut = OVERCUT.contains(&name);
                st.path_optimization = PATH_OPTIMIZATION.contains(&name);
            }
            // Como antes: esbozar y punteado en el 1 (si corta ahí) y esbozar en el 2.
            if m.settings(H1, Cut).is_some() {
                m.add_action(H1, Sketch);
                m.add_action(H1, Stipple);
            }
            m.add_action(H2, Sketch);
            m
        })
        .collect()
}

/// Cómo se asignan las líneas a portaherramientas y materiales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SendMode {
    /// Asistente paso a paso.
    Guide,
    /// Un material y acción por portaherramientas; cada objeto elige portaherramientas.
    #[default]
    Standard,
    /// Ajustes por color de línea.
    ByLine,
    /// Ajustes por color de relleno.
    ByFill,
}

impl SendMode {
    pub fn name(self) -> &'static str {
        match self {
            SendMode::Guide => "Guía",
            SendMode::Standard => "Estándar",
            SendMode::ByLine => "Cortar por color de línea",
            SendMode::ByFill => "Cortar por color de relleno",
        }
    }
}

/// Qué hace la máquina al terminar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FeedOption {
    /// El carro vuelve al origen y el material queda cargado.
    #[default]
    ReturnToOrigin,
    /// El material avanza hasta salir por detrás del área de corte.
    FeedOut,
}

/// Orden de los trabajos de corte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CutOrder {
    /// En el orden del documento.
    NoOptimize,
    /// Vecino más cercano: menos recorrido en vacío.
    MaxSpeed,
    /// Por franjas en la dirección de alimentación: el material va y viene lo menos posible.
    #[default]
    MinRollerMovement,
}

/// Orden de los puntos de perforación (punzón).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PunchOrder {
    #[default]
    Default,
    EdgeToCenter,
    CenterToEdge,
}

/// Cuándo mostrar los ajustes de profundidad de la cuchilla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ShowBlade {
    /// Solo cuando la herramienta se ajusta a mano.
    #[default]
    Auto,
    Always,
    Never,
}

/// Opciones generales de Enviar (menú del engranaje).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SendOptions {
    pub feed: FeedOption,
    /// Mostrar todos los modelos de cortador, no solo los conocidos.
    pub all_cutters: bool,
    pub show_blade: ShowBlade,
    pub order: CutOrder,
    /// Cortar los contornos interiores antes que los exteriores que los contienen.
    pub inner_first: bool,
    pub punch_order: PunchOrder,
}

impl Default for SendOptions {
    fn default() -> Self {
        SendOptions {
            feed: FeedOption::ReturnToOrigin,
            all_cutters: false,
            show_blade: ShowBlade::Auto,
            order: CutOrder::MinRollerMovement,
            inner_first: true,
            punch_order: PunchOrder::Default,
        }
    }
}

/// Material, acción y herramienta elegidos para un portaherramientas (modo Estándar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HolderSetup {
    pub material: String,
    pub action: Action,
    pub tool: Tool,
}

/// Clave de la fila de objetos sin relleno en «por color de relleno».
pub const NO_FILL: Color = Color::rgba(0, 0, 0, 0);

/// Fila de las tablas «por color de línea / de relleno».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorRow {
    pub holder: Holder,
    pub enabled: bool,
    /// Solo el contorno exterior de los objetos de este color.
    pub edge_only: bool,
    pub material: String,
    pub action: Action,
    pub tool: Tool,
    /// La máquina se detiene al terminar este grupo (p. ej., para cambiar de rotulador).
    pub pause_after: bool,
    /// Ajustes propios de este color: sustituyen a los del material sin cambiarlo (p. ej., el
    /// mismo vinilo con medio corte en un color y corte completo en otro).
    #[serde(default)]
    pub settings: Option<ActionSettings>,
}

/// Un grupo de trazos con los mismos ajustes, listo para el controlador.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub holder: Holder,
    pub tool: Tool,
    pub action: Action,
    pub material: String,
    pub settings: ActionSettings,
    /// Polilíneas en mm (coordenadas del documento) con su indicador de cierre.
    pub paths: Vec<Polyline>,
    pub pause_after: bool,
    /// El material no tiene ajustes para esta acción (se usan valores de partida).
    pub missing_settings: bool,
}

/// Acción efectiva de una fila o portaherramientas: con «Auto», la primera acción que tiene el
/// material en ese portaherramientas y que la herramienta puede hacer.
pub fn pick_action(
    material: Option<&Material>,
    holder: Holder,
    action: Action,
    tool: Tool,
) -> Action {
    if action != Action::Auto {
        return action;
    }
    let available = material.map(|m| m.actions_for(holder)).unwrap_or_default();
    available
        .iter()
        .copied()
        .find(|a| tool.actions().contains(a))
        .or_else(|| available.first().copied())
        .unwrap_or_else(|| resolve_action(action, tool))
}

/// Trazos de un objeto en mm del mundo (con rotación), cerrados y abiertos.
pub fn shape_paths(s: &Shape) -> Vec<Polyline> {
    let to_pts = |c: Vec<[f64; 2]>| {
        c.into_iter()
            .map(|p| Point::new(p[0], p[1]))
            .collect::<Vec<_>>()
    };
    let mut v: Vec<Polyline> = ops::closed_contours(s)
        .into_iter()
        .map(|c| (to_pts(c), true))
        .collect();
    v.extend(ops::open_paths(s).into_iter().map(|c| (to_pts(c), false)));
    v.retain(|(p, _)| p.len() >= 2);
    v
}

/// Contorno exterior de un conjunto de objetos (unión, sin agujeros ni líneas interiores).
pub fn outline_paths<'a>(shapes: impl IntoIterator<Item = &'a Shape>) -> Vec<Polyline> {
    let contours: Vec<Vec<[f64; 2]>> = shapes.into_iter().flat_map(ops::closed_contours).collect();
    if contours.is_empty() {
        return Vec::new();
    }
    let empty: Vec<Vec<[f64; 2]>> = Vec::new();
    let polys: Polygons = contours.overlay(&empty, OverlayRule::Subject, IoFillRule::NonZero);
    polys
        .into_iter()
        .filter_map(|poly| poly.into_iter().next())
        .filter(|c| c.len() >= 3)
        .map(|c| {
            (
                c.into_iter().map(|p| Point::new(p[0], p[1])).collect(),
                true,
            )
        })
        .collect()
}

type KeyedPaths<K> = (K, Vec<Polyline>);

fn sendable(s: &Shape) -> bool {
    s.visible && !s.is_image()
}

/// Objetos agrupados por la clave de `key`, en orden de aparición, con sus trazos.
fn group_paths<K: PartialEq + Copy>(
    doc: &Document,
    key: impl Fn(&Shape) -> Option<(K, bool)>,
) -> Vec<KeyedPaths<K>> {
    let mut groups: Vec<(K, bool, Vec<&Shape>)> = Vec::new();
    for s in doc.shapes.iter().filter(|s| sendable(s)) {
        let Some((k, edge)) = key(s) else { continue };
        match groups
            .iter_mut()
            .find(|(gk, ge, _)| *gk == k && *ge == edge)
        {
            Some(g) => g.2.push(s),
            None => groups.push((k, edge, vec![s])),
        }
    }
    let mut out: Vec<KeyedPaths<K>> = Vec::new();
    for (k, edge, shapes) in groups {
        let paths = if edge {
            outline_paths(shapes.iter().copied())
        } else {
            shapes.iter().flat_map(|s| shape_paths(s)).collect()
        };
        match out.iter_mut().find(|(ok, _)| *ok == k) {
            Some(o) => o.1.extend(paths),
            None => out.push((k, paths)),
        }
    }
    out
}

/// Líneas que se enviarán, por portaherramientas, para la vista previa del modo Estándar.
pub fn standard_preview(doc: &Document) -> Vec<HolderPaths> {
    group_paths(doc, |s| match s.cut.style {
        CutStyle::Skip => None,
        style => Some((s.cut.holder, style == CutStyle::Edge)),
    })
}

/// Vista previa por color: portaherramientas de cada fila activa con sus líneas.
pub fn color_preview(
    doc: &Document,
    by_fill: bool,
    rows: &[(Color, ColorRow)],
) -> Vec<HolderPaths> {
    group_paths(doc, |s| {
        let color = if by_fill {
            s.fill.unwrap_or(NO_FILL)
        } else {
            s.stroke.color
        };
        let row = &rows.iter().find(|(c, _)| *c == color)?.1;
        row.enabled.then_some((row.holder, row.edge_only))
    })
}

/// Prepara los trabajos del modo Estándar: cada objeto va a su portaherramientas.
pub fn standard_jobs(
    doc: &Document,
    setups: &[HolderSetup; 2],
    materials: &[Material],
    options: &SendOptions,
) -> Vec<Job> {
    let groups = group_paths(doc, |s| match s.cut.style {
        CutStyle::Skip => None,
        style => Some((s.cut.holder, style == CutStyle::Edge)),
    });
    groups
        .into_iter()
        .filter_map(|(holder, paths)| {
            let setup = &setups[holder.number() as usize - 1];
            make_job(
                holder,
                setup.tool,
                setup.action,
                &setup.material,
                materials,
                paths,
                false,
                options,
                None,
            )
        })
        .collect()
}

/// Prepara los trabajos por color (de línea o de relleno), en el orden de las filas.
pub fn color_jobs(
    doc: &Document,
    by_fill: bool,
    rows: &[(Color, ColorRow)],
    materials: &[Material],
    options: &SendOptions,
) -> Vec<Job> {
    let row_of = |c: Option<Color>| c.and_then(|c| rows.iter().position(|(rc, _)| *rc == c));
    let groups = group_paths(doc, |s| {
        let color = if by_fill {
            Some(s.fill.unwrap_or(NO_FILL))
        } else {
            Some(s.stroke.color)
        };
        let i = row_of(color)?;
        let row = &rows[i].1;
        row.enabled.then_some((i, row.edge_only))
    });
    let mut jobs: Vec<(usize, Job)> = groups
        .into_iter()
        .filter_map(|(i, paths)| {
            let row = &rows[i].1;
            make_job(
                row.holder,
                row.tool,
                row.action,
                &row.material,
                materials,
                paths,
                row.pause_after,
                options,
                row.settings,
            )
            .map(|j| (i, j))
        })
        .collect();
    jobs.sort_by_key(|(i, _)| *i);
    jobs.into_iter().map(|(_, j)| j).collect()
}

#[allow(clippy::too_many_arguments)]
fn make_job(
    holder: Holder,
    tool: Tool,
    action: Action,
    material: &str,
    materials: &[Material],
    paths: Vec<Polyline>,
    pause_after: bool,
    options: &SendOptions,
    own: Option<ActionSettings>,
) -> Option<Job> {
    if paths.is_empty() || !tool.holders().contains(&holder) {
        return None;
    }
    let mat = materials.iter().find(|m| m.name == material);
    let action = pick_action(mat, holder, action, tool);
    if !tool.actions().contains(&action) {
        return None;
    }
    let found = own.or_else(|| mat.and_then(|m| m.settings(holder, action).copied()));
    let missing_settings = found.is_none();
    let family = mat.map(|m| m.family).unwrap_or_default();
    let settings = found
        .unwrap_or_else(|| ActionSettings::for_action(action, family))
        .clamped();
    let paths = order_paths(paths, options, &settings, action);
    Some(Job {
        holder,
        tool,
        action,
        material: material.to_string(),
        settings,
        paths,
        pause_after,
        missing_settings,
    })
}

fn bounds(p: &[Point]) -> Rect {
    crate::edit::bounds_of(p.iter().copied()).unwrap_or(Rect::ZERO)
}

/// Ordena los trazos según las opciones: interiores primero y recorrido.
pub fn order_paths(
    mut paths: Vec<Polyline>,
    options: &SendOptions,
    settings: &ActionSettings,
    action: Action,
) -> Vec<Polyline> {
    let optimize =
        settings.path_optimization || (action == Action::Sketch && settings.sort_sketches);
    let inner = options.inner_first || settings.smart_path;
    match options.order {
        CutOrder::NoOptimize if !optimize => {}
        CutOrder::MaxSpeed | CutOrder::NoOptimize => return nearest_neighbour(paths, inner),
        CutOrder::MinRollerMovement => {
            // Franjas de 20 mm en la dirección de alimentación (y); dentro de cada franja, de
            // izquierda a derecha.
            const BAND_MM: f64 = 20.0;
            paths.sort_by(|a, b| {
                let (ba, bb) = (bounds(&a.0), bounds(&b.0));
                let (fa, fb) = ((ba.y0 / BAND_MM).floor(), (bb.y0 / BAND_MM).floor());
                fa.total_cmp(&fb).then(ba.x0.total_cmp(&bb.x0))
            });
            if inner {
                paths = inner_first(paths);
            }
            return with_nearest_entries(paths);
        }
    }
    if inner {
        paths = inner_first(paths);
    }
    paths
}

/// Nodos por los que puede empezar a cortarse un trazado: en los cerrados, uno de cada
/// `max(1, ⌈(n+1)/10⌉)` (unos diez repartidos); en
/// los abiertos, solo el primero.
fn entry_candidates((pts, closed): &Polyline) -> impl Iterator<Item = usize> {
    let n = if *closed { pts.len() } else { 1 };
    let step = n.div_ceil(10).max(1);
    (0..n).step_by(step)
}

/// Candidato de entrada más cercano a `at`: (índice del nodo, distancia²).
fn nearest_entry(path: &Polyline, at: Point) -> (usize, f64) {
    entry_candidates(path)
        .map(|i| (i, path.0[i].distance_squared(at)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("al menos un punto")
}

/// Trazado cerrado que empieza en el nodo `i`. Si el último punto repite el primero, se quita
/// antes de rotar: el cierre lo dibuja el codificador.
fn start_at((mut pts, closed): Polyline, i: usize) -> Polyline {
    if closed && i > 0 {
        if pts.len() > 2 && pts.first() == pts.last() {
            pts.pop();
        }
        let n = pts.len();
        pts.rotate_left(i % n);
    }
    (pts, closed)
}

/// Último punto que dibuja un trazado (en los cerrados, el cierre vuelve al primero).
fn end_point((pts, closed): &Polyline) -> Point {
    if *closed && pts.len() > 2 {
        pts[0]
    } else {
        *pts.last().expect("al menos dos puntos")
    }
}

/// Mantiene el orden y solo elige por qué nodo empieza cada trazado cerrado: el candidato más
/// cercano a donde terminó el anterior.
fn with_nearest_entries(paths: Vec<Polyline>) -> Vec<Polyline> {
    let mut at = Point::ZERO;
    paths
        .into_iter()
        .map(|p| {
            let (i, _) = nearest_entry(&p, at);
            let p = start_at(p, i);
            at = end_point(&p);
            p
        })
        .collect()
}

/// Vecino más cercano, eligiendo a la vez el siguiente trazado y su punto de entrada. Con
/// `inner`, un trazado solo está disponible cuando ya se cortó todo lo que tiene dentro, para
/// que las piezas sigan sujetas al material hasta el final (primero los contornos interiores).
fn nearest_neighbour(paths: Vec<Polyline>, inner: bool) -> Vec<Polyline> {
    let inside = if inner {
        containment(&paths)
    } else {
        vec![Vec::new(); paths.len()]
    };
    let mut rest: Vec<Option<Polyline>> = paths.into_iter().map(Some).collect();
    let mut out = Vec::with_capacity(rest.len());
    let mut at = Point::ZERO;
    while out.len() < rest.len() {
        let closest = |only_free: bool| {
            rest.iter()
                .enumerate()
                .filter_map(|(k, p)| p.as_ref().map(|p| (k, p)))
                .filter(|(k, _)| !only_free || inside[*k].iter().all(|j| rest[*j].is_none()))
                .map(|(k, p)| {
                    let (i, d) = nearest_entry(p, at);
                    (k, i, d)
                })
                .min_by(|a, b| a.2.total_cmp(&b.2))
        };
        // Dos contornos idénticos pueden quedar uno «dentro» del otro: si ninguno está libre,
        // se elige entre todos los que quedan.
        let (k, entry, _) = closest(true)
            .or_else(|| closest(false))
            .expect("quedan trazados por cortar");
        let next = start_at(rest[k].take().expect("disponible"), entry);
        at = end_point(&next);
        out.push(next);
    }
    out
}

/// Para cada trazado, los trazados que quedan dentro de él (solo los cerrados contienen).
fn containment(paths: &[Polyline]) -> Vec<Vec<usize>> {
    paths
        .iter()
        .map(|(q, closed)| {
            let b = bounds(q);
            paths
                .iter()
                .enumerate()
                .filter(|(_, (p, _))| {
                    *closed
                        && !std::ptr::eq(q, p)
                        && b.contains(p[0])
                        && crate::edit::point_in_polygon(p[0], q)
                })
                .map(|(j, _)| j)
                .collect()
        })
        .collect()
}

/// Profundidad de anidamiento de cada trazo cerrado; los más profundos se cortan antes, para que
/// las piezas sigan sujetas al material hasta el final.
fn inner_first(paths: Vec<Polyline>) -> Vec<Polyline> {
    let inside = containment(&paths);
    let depth: Vec<usize> = (0..paths.len())
        .map(|j| inside.iter().filter(|c| c.contains(&j)).count())
        .collect();
    let mut idx: Vec<usize> = (0..paths.len()).collect();
    // Orden estable: más profundos primero, respetando el orden previo entre iguales.
    idx.sort_by(|a, b| depth[*b].cmp(&depth[*a]));
    let mut paths: Vec<Option<Polyline>> = paths.into_iter().map(Some).collect();
    idx.into_iter().filter_map(|i| paths[i].take()).collect()
}

/// Ordena puntos de perforación según la opción del punzón.
pub fn order_punches(mut points: Vec<Point>, order: PunchOrder) -> Vec<Point> {
    let Some(b) = crate::edit::bounds_of(points.iter().copied()) else {
        return points;
    };
    let c = b.center();
    match order {
        PunchOrder::Default => {}
        PunchOrder::EdgeToCenter => points.sort_by(|a, p| p.distance(c).total_cmp(&a.distance(c))),
        PunchOrder::CenterToEdge => points.sort_by(|a, p| a.distance(c).total_cmp(&p.distance(c))),
    }
    points
}

/// Prolonga los extremos de cada tramo recto en las esquinas vivas (sobrecorte).
pub fn apply_overcut(path: &[Point], closed: bool, start_mm: f64, end_mm: f64) -> Vec<Vec<Point>> {
    if path.len() < 2 || (start_mm <= 0.0 && end_mm <= 0.0) {
        return vec![path.to_vec()];
    }
    let n = path.len();
    let segs = if closed { n } else { n - 1 };
    (0..segs)
        .filter_map(|i| {
            let a = path[i];
            let b = path[(i + 1) % n];
            let d = b - a;
            let len = d.hypot();
            (len > 1e-9).then(|| {
                let u = d / len;
                vec![a - u * start_mm, b + u * end_mm]
            })
        })
        .collect()
}

/// Puntos de un trazo cada `spacing_mm` (punteado), empezando en su primer punto.
pub fn stipple_points(path: &[Point], closed: bool, spacing_mm: f64) -> Vec<Point> {
    let spacing = spacing_mm.max(0.05);
    let mut pts = path.to_vec();
    if closed && pts.len() > 1 {
        pts.push(pts[0]);
    }
    let mut out = Vec::new();
    let mut next = 0.0;
    let mut walked = 0.0;
    for w in pts.windows(2) {
        let len = w[0].distance(w[1]);
        while next <= walked + len + 1e-9 {
            let t = if len > 0.0 {
                (next - walked) / len
            } else {
                0.0
            };
            out.push(w[0].lerp(w[1], t.clamp(0.0, 1.0)));
            next += spacing;
        }
        walked += len;
    }
    // En un trazo cerrado el último punto coincide con el primero.
    if closed && out.len() > 1 && out[0].distance(*out.last().unwrap()) < spacing * 0.5 {
        out.pop();
    }
    out
}

/// Forma de prueba: un cuadrado de 10 mm con un triángulo dentro, en `origin` (esquina).
pub fn test_shapes(origin: Point) -> Vec<Shape> {
    use crate::shape::{FillRule, ShapeKind};
    let sq = 10.0;
    let square = vec![
        Point::new(0.0, 0.0),
        Point::new(sq, 0.0),
        Point::new(sq, sq),
        Point::new(0.0, sq),
    ];
    let tri = vec![
        Point::new(sq / 2.0, 2.0),
        Point::new(sq - 2.2, sq - 2.5),
        Point::new(2.2, sq - 2.5),
    ];
    let world: Vec<Polyline> = [square, tri]
        .into_iter()
        .map(|p| (p.into_iter().map(|q| q + origin.to_vec2()).collect(), true))
        .collect();
    let (rect, subpaths) = Shape::normalize_subpaths(&world);
    let mut s = Shape::new(
        0,
        TEST_SHAPE_NAME,
        rect,
        ShapeKind::Path {
            subpaths,
            fill_rule: FillRule::EvenOdd,
        },
    );
    s.stroke.color = Color::rgb(0xe3, 0x34, 0x3f);
    vec![s]
}

/// Nombre de la forma de prueba (para reconocerla y quitarla).
pub const TEST_SHAPE_NAME: &str = "Forma de prueba";

/// Ids de las formas de prueba del documento.
pub fn test_shape_ids(doc: &Document) -> Vec<ShapeId> {
    doc.shapes
        .iter()
        .filter(|s| s.name == TEST_SHAPE_NAME)
        .map(|s| s.id)
        .collect()
}

/// Velocidad de corte aproximada (mm/s) para un nivel de velocidad (1–30).
pub fn cut_speed_mm_s(level: u32) -> f64 {
    level.clamp(1, 30) as f64 * 10.0
}

/// Velocidad aproximada en vacío (mm/s) y tiempo de subir y bajar la herramienta (s).
pub const TRAVEL_MM_S: f64 = 250.0;
pub const LIFT_S: f64 = 0.15;

/// Un tramo del recorrido de la herramienta, con su momento de inicio y duración (s).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolMove {
    pub from: Point,
    pub to: Point,
    /// Con la herramienta bajada.
    pub cutting: bool,
    pub start_s: f64,
    pub secs: f64,
    /// Trabajo al que pertenece.
    pub job: usize,
}

/// Recorrido completo de la herramienta en el orden de envío, desde el origen y de vuelta.
pub fn toolpath(jobs: &[Job]) -> Vec<ToolMove> {
    let mut out: Vec<ToolMove> = Vec::new();
    let mut at = Point::ZERO;
    let mut t = 0.0;
    let mut push = |out: &mut Vec<ToolMove>,
                    at: &mut Point,
                    to: Point,
                    cutting: bool,
                    v: f64,
                    extra: f64,
                    job: usize| {
        let secs = at.distance(to) / v + extra;
        out.push(ToolMove {
            from: *at,
            to,
            cutting,
            start_s: t,
            secs,
            job,
        });
        t += secs;
        *at = to;
    };
    for (k, j) in jobs.iter().enumerate() {
        let v = cut_speed_mm_s(j.settings.speed);
        for _ in 0..j.settings.passes.max(1) {
            for (pts, closed) in &j.paths {
                if j.action == Action::Stipple {
                    for p in stipple_points(pts, *closed, j.settings.stipple_spacing_mm) {
                        push(&mut out, &mut at, p, false, TRAVEL_MM_S, 0.0, k);
                        push(&mut out, &mut at, p, true, v, LIFT_S, k);
                    }
                    continue;
                }
                let Some(first) = pts.first() else { continue };
                push(&mut out, &mut at, *first, false, TRAVEL_MM_S, LIFT_S, k);
                for p in &pts[1..] {
                    push(&mut out, &mut at, *p, true, v, 0.0, k);
                }
                if *closed && pts.len() > 2 {
                    push(&mut out, &mut at, *first, true, v, 0.0, k);
                }
            }
        }
    }
    if !out.is_empty() {
        let job = out.last().map(|m| m.job).unwrap_or(0);
        push(
            &mut out,
            &mut at,
            Point::ZERO,
            false,
            TRAVEL_MM_S,
            LIFT_S,
            job,
        );
    }
    out
}

/// Tiempo aproximado (s) de todos los trabajos.
pub fn estimate_seconds(jobs: &[Job]) -> f64 {
    toolpath(jobs)
        .last()
        .map(|m| m.start_s + m.secs)
        .unwrap_or(0.0)
}

/// «~2 min 30 s», «~45 s», «~1 h 5 min».
pub fn format_duration(secs: f64) -> String {
    let s = secs.round().max(1.0) as u64;
    match s {
        0..60 => format!("~{s} s"),
        60..3600 => {
            let (m, r) = (s / 60, s % 60);
            if r == 0 {
                format!("~{m} min")
            } else {
                format!("~{m} min {r} s")
            }
        }
        _ => format!("~{} h {} min", s / 3600, (s % 3600) / 60),
    }
}

/// Lado de la forma de prueba y separación entre intentos seguidos (mm).
pub const TEST_SIZE_MM: f64 = 10.0;
pub const TEST_GAP_MM: f64 = 5.0;

/// Área donde caben las pruebas (y donde se acomodan las piezas): la página menos el margen
/// de corte; con marcas de registro, el marco entre marcas sin las zonas de las marcas.
pub fn test_area(doc: &Document) -> Rect {
    safe_area(doc)
}

/// Área útil de la página para cortar (ver [`test_area`]).
pub fn safe_area(doc: &Document) -> Rect {
    let page = &doc.page;
    let r = &page.reg_marks;
    if r.enabled {
        let f = r.cut_area(page.width_mm, page.height_mm);
        let m = r.length_mm.max(crate::regmarks::SQUARE_MM) + 2.0;
        return f.inset(-m);
    }
    let m = page.cut_margin_mm.max(0.0);
    Rect::new(m, m, page.width_mm - m, page.height_mm - m)
}

/// Esquina de una prueba centrada en `p`, sin salirse del área.
pub fn test_origin_at(p: Point, area: Rect) -> Point {
    let h = TEST_SIZE_MM / 2.0;
    Point::new(
        (p.x - h).clamp(area.x0, (area.x1 - TEST_SIZE_MM).max(area.x0)),
        (p.y - h).clamp(area.y0, (area.y1 - TEST_SIZE_MM).max(area.y0)),
    )
}

/// Dónde va la prueba siguiente: a la derecha de la anterior; si no cabe, al principio de la
/// fila de abajo; si tampoco, vuelve a la esquina del área.
pub fn next_test_origin(prev: Point, area: Rect) -> Point {
    let step = TEST_SIZE_MM + TEST_GAP_MM;
    if prev.x + step + TEST_SIZE_MM <= area.x1 {
        Point::new(prev.x + step, prev.y)
    } else if prev.y + step + TEST_SIZE_MM <= area.y1 {
        Point::new(area.x0, prev.y + step)
    } else {
        Point::new(area.x0, area.y0)
    }
}

/// Cómo salió un corte de prueba (lo marca el usuario).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestResult {
    Good,
    Partial,
    Bad,
}

impl TestResult {
    pub const ALL: [TestResult; 3] = [TestResult::Good, TestResult::Partial, TestResult::Bad];

    pub fn name(self) -> &'static str {
        match self {
            TestResult::Good => "Bien",
            TestResult::Partial => "Solo en partes",
            TestResult::Bad => "No cortó",
        }
    }
}

/// Corte de prueba enviado: dónde y con qué ajustes exactos.
#[derive(Clone, Debug, PartialEq)]
pub struct TestAttempt {
    pub number: usize,
    pub origin: Point,
    pub material: String,
    pub holder: Holder,
    pub action: Action,
    pub settings: ActionSettings,
    pub result: Option<TestResult>,
}

impl TestAttempt {
    /// Intento a partir de los trabajos que se envían (el primero que corta la forma).
    pub fn from_jobs(number: usize, origin: Point, jobs: &[Job]) -> Option<TestAttempt> {
        let j = jobs.iter().find(|j| !j.paths.is_empty())?;
        Some(TestAttempt {
            number,
            origin,
            material: j.material.clone(),
            holder: j.holder,
            action: j.action,
            settings: j.settings,
            result: None,
        })
    }

    /// «prof. 4 · presión 20 · vel. 5 · 1 pasada».
    pub fn summary(&self) -> String {
        let st = &self.settings;
        let passes = if st.passes == 1 {
            "1 pasada".to_string()
        } else {
            format!("{} pasadas", st.passes)
        };
        format!(
            "prof. {} · presión {} · vel. {} · {passes}",
            st.depth, st.force, st.speed
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::ShapeKind;

    fn rect(id: u64, x0: f64, y0: f64, x1: f64, y1: f64) -> Shape {
        Shape::new(
            id,
            "r",
            Rect::new(x0, y0, x1, y1),
            ShapeKind::Rect { corner_radius: 0.0 },
        )
    }

    fn doc(shapes: Vec<Shape>) -> Document {
        let mut d = Document::default();
        d.shapes = shapes;
        d
    }

    #[test]
    fn auto_uses_material_action() {
        let mats = builtin_materials();
        let lisa = mats
            .iter()
            .position(|m| m.name == DEFAULT_MATERIAL)
            .unwrap();
        // El portaherramientas 2 de un material incluido solo tiene «Esbozar».
        let a = pick_action(
            Some(&mats[lisa]),
            Holder::Two,
            Action::Auto,
            Tool::KraftBlade,
        );
        assert_eq!(a, Action::Sketch);
        let a = pick_action(
            Some(&mats[lisa]),
            Holder::One,
            Action::Auto,
            Tool::AutoBlade,
        );
        assert_eq!(a, Action::Cut);
    }

    #[test]
    fn old_prefs_still_load() {
        let m: MaterialEntry = serde_json::from_str(
            r#"{"holder":"One","action":"Perforate","settings":{"perf_cut_mm":2.0}}"#,
        )
        .unwrap();
        assert_eq!(m.action, Action::Stipple);
    }

    #[test]
    fn tools_and_holders() {
        assert!(Tool::AutoBlade.compatible(Holder::One, Action::Cut));
        assert!(!Tool::AutoBlade.compatible(Holder::Two, Action::Cut));
        assert!(!Tool::SketchPen.compatible(Holder::One, Action::Cut));
        assert!(Tool::KraftBlade.compatible(Holder::Two, Action::Auto));
        assert_eq!(
            resolve_action(Action::Auto, Tool::SketchPen),
            Action::Sketch
        );
        assert!(Tool::for_holder(Holder::Two).contains(&Tool::RotaryBlade));
    }

    #[test]
    fn standard_groups_by_holder_and_skips() {
        let mut a = rect(1, 0.0, 0.0, 10.0, 10.0);
        let mut b = rect(2, 20.0, 0.0, 30.0, 10.0);
        let mut c = rect(3, 40.0, 0.0, 50.0, 10.0);
        b.cut.holder = Holder::Two;
        c.cut.style = CutStyle::Skip;
        a.cut.style = CutStyle::All;
        let d = doc(vec![a, b, c]);
        let mats = builtin_materials();
        let lisa = mats
            .iter()
            .position(|m| m.name == DEFAULT_MATERIAL)
            .unwrap();
        let setups = [
            HolderSetup {
                material: mats[lisa].name.clone(),
                action: Action::Cut,
                tool: Tool::AutoBlade,
            },
            HolderSetup {
                material: mats[lisa].name.clone(),
                action: Action::Auto,
                tool: Tool::SketchPen,
            },
        ];
        let jobs = standard_jobs(&d, &setups, &mats, &SendOptions::default());
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].holder, Holder::One);
        assert_eq!(jobs[0].paths.len(), 1);
        assert_eq!(jobs[1].action, Action::Sketch);
    }

    #[test]
    fn edge_keeps_only_outline() {
        let mut outer = rect(1, 0.0, 0.0, 30.0, 30.0);
        let mut inner = rect(2, 10.0, 10.0, 20.0, 20.0);
        outer.cut.style = CutStyle::Edge;
        inner.cut.style = CutStyle::Edge;
        let d = doc(vec![outer, inner]);
        let setups = [
            HolderSetup {
                material: String::new(),
                action: Action::Cut,
                tool: Tool::AutoBlade,
            },
            HolderSetup {
                material: String::new(),
                action: Action::Sketch,
                tool: Tool::SketchPen,
            },
        ];
        let jobs = standard_jobs(&d, &setups, &[], &SendOptions::default());
        assert_eq!(jobs[0].paths.len(), 1);
    }

    #[test]
    fn inner_contours_first() {
        let outer = rect(1, 0.0, 0.0, 30.0, 30.0);
        let inner = rect(2, 10.0, 10.0, 20.0, 20.0);
        let d = doc(vec![outer, inner]);
        let setups = [
            HolderSetup {
                material: String::new(),
                action: Action::Cut,
                tool: Tool::AutoBlade,
            },
            HolderSetup {
                material: String::new(),
                action: Action::Sketch,
                tool: Tool::SketchPen,
            },
        ];
        let jobs = standard_jobs(&d, &setups, &[], &SendOptions::default());
        let first = bounds(&jobs[0].paths[0].0);
        assert!(first.width() < 15.0, "el interior va primero: {first:?}");
    }

    #[test]
    fn color_rows_order_and_disable() {
        let mut a = rect(1, 0.0, 0.0, 10.0, 10.0);
        let mut b = rect(2, 20.0, 0.0, 30.0, 10.0);
        a.stroke.color = Color::rgb(255, 0, 0);
        b.stroke.color = Color::rgb(0, 0, 255);
        let d = doc(vec![a, b]);
        let row = |holder, tool, enabled| ColorRow {
            settings: None,
            holder,
            enabled,
            edge_only: false,
            material: String::new(),
            action: Action::Auto,
            tool,
            pause_after: false,
        };
        let rows = vec![
            (
                Color::rgb(0, 0, 255),
                row(Holder::Two, Tool::SketchPen, true),
            ),
            (
                Color::rgb(255, 0, 0),
                row(Holder::One, Tool::AutoBlade, false),
            ),
        ];
        let jobs = color_jobs(&d, false, &rows, &[], &SendOptions::default());
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].holder, Holder::Two);
    }

    #[test]
    fn overcut_and_perforation() {
        let sq = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let segs = apply_overcut(&sq, true, 0.5, 0.5);
        assert_eq!(segs.len(), 4);
        assert!((segs[0][0].x + 0.5).abs() < 1e-9 && (segs[0][1].x - 10.5).abs() < 1e-9);
        let dots = stipple_points(&sq[..2], false, 2.5);
        assert_eq!(dots.len(), 5); // 0, 2.5, 5, 7.5, 10
        let ring = stipple_points(&sq, true, 10.0);
        assert_eq!(ring.len(), 4);
    }

    #[test]
    fn punch_order() {
        let pts = vec![
            Point::new(0.0, 0.0),
            Point::new(5.0, 5.0),
            Point::new(10.0, 10.0),
        ];
        let c2e = order_punches(pts.clone(), PunchOrder::CenterToEdge);
        assert_eq!(c2e[0], Point::new(5.0, 5.0));
        let e2c = order_punches(pts, PunchOrder::EdgeToCenter);
        assert_ne!(e2c[0], Point::new(5.0, 5.0));
    }

    #[test]
    fn heat_transfer_is_cut_mirrored_except_printable() {
        let mats = builtin_materials();
        let get = |n: &str| mats.iter().find(|m| m.name == n).unwrap().cut_mirrored();
        assert!(get("Transferencia de calor, tela"));
        assert!(get("Transferencia de calor, purpurina"));
        assert!(!get("Transferencia de calor, tela imprimible"));
        assert!(!get("Vinilo, imprimible"));
        assert!(!get(DEFAULT_MATERIAL));
    }

    #[test]
    fn toolpath_and_time_estimate() {
        let st = ActionSettings {
            speed: 10,
            passes: 2,
            ..Default::default()
        };
        let job = Job {
            holder: Holder::One,
            tool: Tool::AutoBlade,
            action: Action::Cut,
            material: "m".into(),
            settings: st,
            // Cuadrado de 10 mm cerrado en (100, 0).
            paths: vec![(
                vec![
                    Point::new(100.0, 0.0),
                    Point::new(110.0, 0.0),
                    Point::new(110.0, 10.0),
                    Point::new(100.0, 10.0),
                ],
                true,
            )],
            pause_after: false,
            missing_settings: false,
        };
        let moves = toolpath(std::slice::from_ref(&job));
        // Por pasada: ir al inicio + 4 lados; al final, vuelta al origen.
        assert_eq!(moves.len(), 2 * 5 + 1);
        let cut: f64 = moves
            .iter()
            .filter(|m| m.cutting)
            .map(|m| m.from.distance(m.to))
            .sum();
        assert!((cut - 80.0).abs() < 1e-9, "2 pasadas × 40 mm");
        assert_eq!(moves[0].from, Point::ZERO);
        assert_eq!(moves.last().unwrap().to, Point::ZERO);
        // Tramos seguidos en el tiempo.
        for w in moves.windows(2) {
            assert!((w[0].start_s + w[0].secs - w[1].start_s).abs() < 1e-9);
        }
        // 80 mm a 100 mm/s + 200 mm en vacío + 3 subidas.
        let t = estimate_seconds(&[job]);
        let expected = 0.8 + 200.0 / TRAVEL_MM_S + 3.0 * LIFT_S;
        assert!((t - expected).abs() < 1e-6, "{t} vs {expected}");
        assert_eq!(format_duration(42.4), "~42 s");
        assert_eq!(format_duration(150.0), "~2 min 30 s");
        assert_eq!(format_duration(3900.0), "~1 h 5 min");
    }

    #[test]
    fn test_attempts_line_up_and_wrap() {
        let area = Rect::new(3.0, 3.0, 63.0, 43.0);
        let a = test_origin_at(Point::new(0.0, 0.0), area);
        assert_eq!(
            a,
            Point::new(3.0, 3.0),
            "no se sale por arriba a la izquierda"
        );
        assert_eq!(
            test_origin_at(Point::new(100.0, 100.0), area),
            Point::new(53.0, 33.0)
        );
        let b = next_test_origin(a, area);
        assert_eq!(b, Point::new(18.0, 3.0));
        // 3, 18, 33, 48 caben; la siguiente va a la fila de abajo.
        let c = next_test_origin(Point::new(48.0, 3.0), area);
        assert_eq!(c, Point::new(3.0, 18.0));
        // Sin sitio abajo, vuelve a la esquina.
        assert_eq!(next_test_origin(Point::new(48.0, 33.0), area), a);
    }

    #[test]
    fn test_attempt_keeps_the_sent_settings() {
        let st = ActionSettings {
            depth: 5,
            force: 25,
            speed: 3,
            passes: 2,
            ..Default::default()
        };
        let job = Job {
            holder: Holder::One,
            tool: Tool::AutoBlade,
            action: Action::Cut,
            material: "Vinilo, imprimible".into(),
            settings: st,
            paths: vec![(vec![Point::ZERO, Point::new(1.0, 0.0)], false)],
            pause_after: false,
            missing_settings: false,
        };
        let a = TestAttempt::from_jobs(3, Point::new(1.0, 2.0), &[job]).unwrap();
        assert_eq!(a.material, "Vinilo, imprimible");
        assert_eq!(a.summary(), "prof. 5 · presión 25 · vel. 3 · 2 pasadas");
        assert!(TestAttempt::from_jobs(1, Point::ZERO, &[]).is_none());
    }

    #[test]
    fn test_shape_is_10mm() {
        let s = &test_shapes(Point::new(5.0, 5.0))[0];
        assert!((s.rect.width() - 10.0).abs() < 1e-9);
        assert_eq!(s.name, TEST_SHAPE_NAME);
    }

    #[test]
    fn material_settings_created_on_demand() {
        let mut m = Material::new("x", Family::Vinyl, true);
        assert!(m.settings(Holder::Two, Action::Sketch).is_none());
        m.settings_mut(Holder::Two, Action::Sketch).force = 20;
        assert_eq!(m.settings(Holder::Two, Action::Sketch).unwrap().force, 20);
        assert_eq!(
            m.actions_for(Holder::Two),
            vec![Action::KraftCut, Action::Sketch]
        );
        assert!(!m.addable_actions(Holder::Two).contains(&Action::Sketch));
        m.remove_action(Holder::Two, Action::Sketch);
        assert_eq!(m.actions_for(Holder::Two), vec![Action::KraftCut]);
    }

    #[test]
    fn color_row_settings_override_material() {
        let mats = builtin_materials();
        let mut doc = doc(vec![
            rect(1, 0.0, 0.0, 10.0, 10.0),
            rect(2, 20.0, 0.0, 30.0, 10.0),
        ]);
        doc.shapes[1].stroke.color = Color::rgb(0, 0, 255);
        let red = doc.shapes[0].stroke.color;
        let row = |settings| ColorRow {
            holder: Holder::One,
            enabled: true,
            edge_only: false,
            material: DEFAULT_MATERIAL.into(),
            action: Action::Cut,
            tool: Tool::AutoBlade,
            pause_after: false,
            settings,
        };
        let deep = ActionSettings {
            depth: 3,
            ..mats[0].settings(Holder::One, Action::Cut).copied().unwrap()
        };
        let rows = [(red, row(None)), (Color::rgb(0, 0, 255), row(Some(deep)))];
        let jobs = color_jobs(&doc, false, &rows, &mats, &SendOptions::default());
        assert_eq!(jobs.len(), 2);
        let lisa = mats.iter().find(|m| m.name == DEFAULT_MATERIAL).unwrap();
        assert_eq!(
            jobs[0].settings.depth,
            lisa.settings(Holder::One, Action::Cut).unwrap().depth
        );
        assert_eq!(jobs[1].settings.depth, 3);
    }
}

#[cfg(test)]
mod order_tests {
    use super::*;

    /// Cuadrado cerrado como polilínea (sin repetir el primer punto).
    fn square(x: f64, y: f64, s: f64) -> Polyline {
        let p = |a, b| Point::new(a, b);
        (
            vec![p(x + s, y + s), p(x, y + s), p(x, y), p(x + s, y)],
            true,
        )
    }

    fn order(paths: Vec<Polyline>, order: CutOrder, inner_first: bool) -> Vec<Polyline> {
        let options = SendOptions {
            order,
            inner_first,
            ..SendOptions::default()
        };
        order_paths(paths, &options, &ActionSettings::default(), Action::Cut)
    }

    /// Mismos puntos en el mismo sentido, empezando en otro nodo.
    fn same_loop(a: &[Point], b: &[Point]) -> bool {
        a.len() == b.len()
            && (0..a.len()).any(|k| a.iter().cycle().skip(k).take(a.len()).eq(b.iter()))
    }

    #[test]
    fn candidates_are_about_ten_spread_entries() {
        let many: Polyline = ((0..95).map(|i| Point::new(i as f64, 0.0)).collect(), true);
        assert_eq!(
            entry_candidates(&many).collect::<Vec<_>>(),
            (0..95).step_by(10).collect::<Vec<_>>()
        );
        let few = square(0.0, 0.0, 10.0);
        assert_eq!(entry_candidates(&few).count(), 4);
        let open: Polyline = (vec![Point::new(5.0, 5.0), Point::new(0.0, 0.0)], false);
        assert_eq!(
            entry_candidates(&open).collect::<Vec<_>>(),
            vec![0],
            "las abiertas no se invierten"
        );
    }

    #[test]
    fn closed_contour_starts_at_the_nearest_corner() {
        for mode in [CutOrder::MaxSpeed, CutOrder::MinRollerMovement] {
            let sq = square(10.0, 10.0, 20.0);
            let out = order(vec![sq.clone()], mode, false);
            assert_eq!(
                out[0].0[0],
                Point::new(10.0, 10.0),
                "{mode:?}: esquina más cercana al origen"
            );
            assert!(same_loop(&sq.0, &out[0].0), "{mode:?}: la forma no cambia");
        }
    }

    #[test]
    fn document_order_is_left_untouched() {
        let sq = square(10.0, 10.0, 20.0);
        assert_eq!(
            order(vec![sq.clone()], CutOrder::NoOptimize, false),
            vec![sq]
        );
    }

    #[test]
    fn repeated_closing_point_is_dropped_before_rotating() {
        let mut sq = square(10.0, 10.0, 20.0);
        sq.0.push(sq.0[0]);
        let out = order(vec![sq], CutOrder::MaxSpeed, false);
        assert_eq!(out[0].0.len(), 4);
        assert_eq!(out[0].0[0], Point::new(10.0, 10.0));
    }

    #[test]
    fn each_piece_is_finished_before_jumping_to_the_next() {
        // Dos piezas con agujero, lejos una de otra. Antes se cortaban los dos agujeros y luego
        // los dos contornos (A_in, B_in, A_out, B_out); ahora cada pieza se termina antes de
        // pasar a la otra y cada agujero va antes que su contorno.
        let a_out = square(0.0, 0.0, 30.0);
        let a_in = square(10.0, 10.0, 10.0);
        let b_out = square(200.0, 0.0, 30.0);
        let b_in = square(210.0, 10.0, 10.0);
        let out = order(vec![b_out, a_out, b_in, a_in], CutOrder::MaxSpeed, true);
        let left: Vec<f64> = out.iter().map(|(p, _)| bounds(p).x0).collect();
        assert_eq!(left, vec![10.0, 0.0, 210.0, 200.0]);
    }

    #[test]
    fn identical_overlapping_contours_do_not_block_the_order() {
        let out = order(
            vec![square(0.0, 0.0, 10.0), square(0.0, 0.0, 10.0)],
            CutOrder::MaxSpeed,
            true,
        );
        assert_eq!(out.len(), 2);
    }
}
