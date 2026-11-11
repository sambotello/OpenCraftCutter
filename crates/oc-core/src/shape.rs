//! Formas del documento. La caja (`rect`) está en mm sin rotar; `rotation` gira sobre su centro.

use crate::color::Color;
use kurbo::{Point, Rect};
use serde::{Deserialize, Serialize};

pub type ShapeId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LineStyle {
    #[default]
    Solid,
    /// Guiones cortos y juntos.
    DashFine,
    Dashed,
    /// Guiones largos con huecos largos.
    DashLong,
    /// Guiones largos con huecos cortos.
    DashLongTight,
    Dotted,
}

impl LineStyle {
    pub const ALL: [LineStyle; 6] = [
        LineStyle::Solid,
        LineStyle::DashFine,
        LineStyle::Dashed,
        LineStyle::DashLong,
        LineStyle::DashLongTight,
        LineStyle::Dotted,
    ];

    /// Patrón (trazo, hueco) en múltiplos del grosor; `None` = continua.
    pub fn pattern(self) -> Option<(f64, f64)> {
        match self {
            LineStyle::Solid => None,
            LineStyle::DashFine => Some((2.0, 1.5)),
            LineStyle::Dashed => Some((4.0, 3.0)),
            LineStyle::DashLong => Some((8.0, 7.0)),
            LineStyle::DashLongTight => Some((6.0, 1.5)),
            LineStyle::Dotted => Some((1.0, 2.5)),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LineStyle::Solid => "Continua",
            LineStyle::DashFine => "Guiones finos",
            LineStyle::Dashed => "Guiones",
            LineStyle::DashLong => "Guiones largos",
            LineStyle::DashLongTight => "Guiones largos juntos",
            LineStyle::Dotted => "Puntos",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stroke {
    pub color: Color,
    /// Grosor en puntos tipográficos. 0 = línea de corte fina (siempre visible, sin grosor).
    pub width_pt: f64,
    pub style: LineStyle,
    /// Unión de segmentos en los vértices.
    pub join: LineJoin,
    /// Remate de los extremos de líneas abiertas.
    pub cap: LineCap,
    /// Posición del trazo respecto al borde de la forma.
    pub align: StrokeAlign,
    /// Opacidad de la línea, 0..1.
    pub opacity: f64,
    /// Imprimir y exportar las líneas de corte de 0 pt como líneas de 0,1 pt.
    pub print_hairline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

impl LineJoin {
    pub const ALL: [LineJoin; 3] = [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel];
    pub fn name(self) -> &'static str {
        match self {
            LineJoin::Miter => "En ángulo",
            LineJoin::Round => "Redondeado",
            LineJoin::Bevel => "Biselado",
        }
    }
    pub fn svg(self) -> &'static str {
        match self {
            LineJoin::Miter => "miter",
            LineJoin::Round => "round",
            LineJoin::Bevel => "bevel",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

impl LineCap {
    pub const ALL: [LineCap; 3] = [LineCap::Butt, LineCap::Round, LineCap::Square];
    pub fn name(self) -> &'static str {
        match self {
            LineCap::Butt => "Plano",
            LineCap::Round => "Redondeado",
            LineCap::Square => "Cuadrado",
        }
    }
    pub fn svg(self) -> &'static str {
        match self {
            LineCap::Butt => "butt",
            LineCap::Round => "round",
            LineCap::Square => "square",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StrokeAlign {
    /// Centrado sobre el borde.
    #[default]
    Center,
    /// Hacia dentro de la forma.
    Inside,
}

impl Default for Stroke {
    fn default() -> Self {
        Stroke {
            color: Color::CUT_RED,
            width_pt: 0.0,
            style: LineStyle::Solid,
            join: LineJoin::Miter,
            cap: LineCap::Butt,
            align: StrokeAlign::Center,
            opacity: 1.0,
            print_hairline: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    /// Justificado: cada línea (salvo la última) ocupa todo el ancho del bloque.
    Justify,
}

/// Opciones tipográficas extra del panel Estilo de texto.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextFx {
    pub underline: bool,
    /// Espaciado de caracteres en % (100 = normal).
    pub char_spacing: f64,
    /// Interletraje de la fuente (pares como «AV» más juntos).
    pub kerning: bool,
    /// Texto vertical (de arriba abajo).
    pub vertical: bool,
}

impl Default for TextFx {
    fn default() -> Self {
        TextFx {
            underline: false,
            char_spacing: 100.0,
            kerning: true,
            vertical: false,
        }
    }
}

fn one() -> f64 {
    1.0
}

/// Parada de un degradado: posición 0..1 y color.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub pos: f64,
    pub color: Color,
}

/// Degradado lineal. El ángulo (grados) se mide sobre la caja de la forma: 0 = de izquierda a derecha.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    pub stops: Vec<GradientStop>,
    pub angle_deg: f64,
}

impl Gradient {
    pub fn two(a: Color, b: Color) -> Gradient {
        Gradient {
            stops: vec![
                GradientStop { pos: 0.0, color: a },
                GradientStop { pos: 1.0, color: b },
            ],
            angle_deg: 0.0,
        }
    }

    /// Paradas ordenadas por posición.
    pub fn sorted(&self) -> Vec<GradientStop> {
        let mut v = self.stops.clone();
        v.sort_by(|a, b| {
            a.pos
                .partial_cmp(&b.pos)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        v
    }

    /// Color en la posición `t` (0..1), interpolando en RGBA.
    pub fn sample(&self, t: f64) -> Color {
        let stops = self.sorted();
        let Some(first) = stops.first() else {
            return Color::BLACK;
        };
        let t = t.clamp(0.0, 1.0);
        if t <= first.pos {
            return first.color;
        }
        for w in stops.windows(2) {
            let (a, b) = (w[0], w[1]);
            if t <= b.pos {
                let span = (b.pos - a.pos).max(1e-9);
                let k = (t - a.pos) / span;
                let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * k).round() as u8;
                return Color::rgba(
                    mix(a.color.r, b.color.r),
                    mix(a.color.g, b.color.g),
                    mix(a.color.b, b.color.b),
                    mix(a.color.a, b.color.a),
                );
            }
        }
        stops.last().map(|s| s.color).unwrap_or(Color::BLACK)
    }
}

/// Polilínea con puntos normalizados (0..1) respecto a la caja de la forma.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subpath {
    pub points: Vec<Point>,
    pub closed: bool,
    /// Manejadores Bézier por nodo (normalizados como `points`); `None` = todo recto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handles: Option<Vec<crate::nodes::Handles>>,
}

impl Subpath {
    fn node_path(&self, f: impl Fn(Point) -> Point + Copy) -> crate::nodes::NodePath {
        let nodes = self
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let h = self
                    .handles
                    .as_ref()
                    .and_then(|h| h.get(i))
                    .copied()
                    .unwrap_or_default();
                crate::nodes::Node {
                    p: f(*p),
                    h_in: h.h_in.map(f),
                    h_out: h.h_out.map(f),
                    smooth: h.smooth,
                }
            })
            .collect();
        crate::nodes::NodePath {
            nodes,
            closed: self.closed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ShapeKind {
    Rect {
        corner_radius: f64,
    },
    Ellipse,
    Path {
        subpaths: Vec<Subpath>,
        fill_rule: FillRule,
    },
    Text {
        text: String,
        font_family: String,
        font_size_pt: f64,
        align: TextAlign,
        bold: bool,
        italic: bool,
        line_height: f64,
        #[serde(default)]
        fx: TextFx,
    },
    /// Imagen de mapa de bits (PNG codificado) colocada en la página.
    Image {
        png: ImageData,
        px_width: u32,
        px_height: u32,
    },
}

/// Bytes PNG serializados como base64 para que el documento JSON siga siendo texto.
#[derive(Clone, PartialEq, Eq)]
pub struct ImageData(pub std::sync::Arc<Vec<u8>>);

impl std::fmt::Debug for ImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ImageData({} bytes)", self.0.len())
    }
}

impl Serialize for ImageData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use base64::Engine;
        serializer
            .serialize_str(&base64::engine::general_purpose::STANDARD.encode(self.0.as_slice()))
    }
}

impl<'de> Deserialize<'de> for ImageData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use base64::Engine;
        let text = String::deserialize(deserializer)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(serde::de::Error::custom)?;
        Ok(ImageData(std::sync::Arc::new(bytes)))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub id: ShapeId,
    pub name: String,
    /// Caja envolvente sin rotar, en mm.
    pub rect: Rect,
    /// Rotación en grados sobre el centro de la caja.
    pub rotation: f64,
    /// Color de relleno. Con degradado guarda el primer color (para pruebas de impacto y exportación simple).
    pub fill: Option<Color>,
    /// Degradado lineal del relleno; si existe, sustituye al color sólido al dibujar.
    #[serde(default)]
    pub fill_gradient: Option<Gradient>,
    /// Opacidad del relleno, 0..1 (la UI la muestra como transparencia).
    #[serde(default = "one")]
    pub fill_opacity: f64,
    pub stroke: Stroke,
    pub locked: bool,
    pub visible: bool,
    /// Grupo exterior al que pertenece (los objetos de un grupo se seleccionan y mueven juntos).
    #[serde(default)]
    pub group: Option<u64>,
    /// Grupos interiores, del más externo al más interno (sin contar `group`). Desagrupar quita
    /// el nivel exterior y el último de esta pila pasa a ser `group`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inner_groups: Vec<u64>,
    /// Imagen PixScan (foto del tapete colocada en su sitio).
    #[serde(default)]
    pub pixscan: bool,
    /// Relleno de herramienta especial (relieve, grabador, lápiz térmico, tallador).
    #[serde(default)]
    pub tool_fill: Option<crate::special::ToolFill>,
    /// Puntos de perforación del punzón de cuero (diámetro en mm).
    #[serde(default)]
    pub punch_mm: Option<f64>,
    /// Portaherramientas y líneas que se envían a la máquina.
    #[serde(default)]
    pub cut: crate::send::CutAssign,
    pub kind: ShapeKind,
}

impl Shape {
    pub fn new(id: ShapeId, name: impl Into<String>, rect: Rect, kind: ShapeKind) -> Shape {
        Shape {
            id,
            name: name.into(),
            rect,
            rotation: 0.0,
            fill: None,
            fill_gradient: None,
            fill_opacity: 1.0,
            stroke: Stroke::default(),
            locked: false,
            visible: true,
            group: None,
            inner_groups: Vec::new(),
            pixscan: false,
            tool_fill: None,
            punch_mm: None,
            cut: Default::default(),
            kind,
        }
    }

    pub fn center(&self) -> Point {
        self.rect.center()
    }

    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            ShapeKind::Rect { .. } => "Rectángulo",
            ShapeKind::Ellipse => "Elipse",
            ShapeKind::Path { .. } => "Trazo",
            ShapeKind::Text { .. } => "Texto",
            ShapeKind::Image { .. } => "Imagen",
        }
    }

    /// Sustituye los identificadores de grupo (todos los niveles) con `f`.
    pub fn remap_groups(&mut self, mut f: impl FnMut(u64) -> u64) {
        self.group = self.group.map(&mut f);
        for g in &mut self.inner_groups {
            *g = f(*g);
        }
    }

    pub fn is_text(&self) -> bool {
        matches!(self.kind, ShapeKind::Text { .. })
    }

    pub fn is_image(&self) -> bool {
        matches!(self.kind, ShapeKind::Image { .. })
    }

    /// Subtrazos en coordenadas del mundo (mm), sin aplicar la rotación. Las curvas Bézier
    /// se devuelven aplanadas.
    pub fn world_subpaths(&self) -> Vec<(Vec<Point>, bool)> {
        self.world_node_paths()
            .into_iter()
            .map(|np| (np.flatten(crate::nodes::FLATTEN_TOL_MM), np.closed))
            .collect()
    }

    /// Nodos de los trazados en mm (mundo, sin rotación). Rectángulos y elipses dan sus
    /// nodos equivalentes; textos e imágenes, ninguno.
    pub fn world_node_paths(&self) -> Vec<crate::nodes::NodePath> {
        let r = self.rect;
        match &self.kind {
            ShapeKind::Path { subpaths, .. } => {
                let f = |p: Point| Point::new(r.x0 + p.x * r.width(), r.y0 + p.y * r.height());
                subpaths.iter().map(|sp| sp.node_path(f)).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Sustituye los trazados por nodos en mm (mundo, sin rotación) y recalcula la caja.
    pub fn set_world_node_paths(&mut self, paths: &[crate::nodes::NodePath]) {
        let mut b: Option<Rect> = None;
        for np in paths {
            for p in np.flatten(crate::nodes::FLATTEN_TOL_MM) {
                let r = Rect::from_points(p, p);
                b = Some(b.map_or(r, |o| o.union(r)));
            }
        }
        let b = b.unwrap_or(Rect::ZERO);
        let (w, h) = (b.width(), b.height());
        let norm = |p: Point| {
            Point::new(
                if w > 0.0 { (p.x - b.x0) / w } else { 0.0 },
                if h > 0.0 { (p.y - b.y0) / h } else { 0.0 },
            )
        };
        let subpaths: Vec<Subpath> = paths
            .iter()
            .map(|np| {
                let any = np.nodes.iter().any(|n| n.is_bezier() || n.smooth);
                Subpath {
                    points: np.nodes.iter().map(|n| norm(n.p)).collect(),
                    closed: np.closed,
                    handles: any.then(|| {
                        np.nodes
                            .iter()
                            .map(|n| crate::nodes::Handles {
                                h_in: n.h_in.map(norm),
                                h_out: n.h_out.map(norm),
                                smooth: n.smooth,
                            })
                            .collect()
                    }),
                }
            })
            .collect();
        self.rect = b;
        match &mut self.kind {
            ShapeKind::Path { subpaths: sp, .. } => *sp = subpaths,
            other => {
                *other = ShapeKind::Path {
                    subpaths,
                    fill_rule: FillRule::EvenOdd,
                };
            }
        }
    }

    /// Caja y subtrazos normalizados a partir de subtrazos en mm.
    pub fn normalize_subpaths(world: &[(Vec<Point>, bool)]) -> (Rect, Vec<Subpath>) {
        let mut bounds: Option<Rect> = None;
        for (pts, _) in world {
            for p in pts {
                let r = Rect::from_points(*p, *p);
                bounds = Some(bounds.map_or(r, |b| b.union(r)));
            }
        }
        let b = bounds.unwrap_or(Rect::ZERO);
        let w = b.width();
        let h = b.height();
        let subpaths = world
            .iter()
            .map(|(pts, closed)| Subpath {
                points: pts
                    .iter()
                    .map(|p| {
                        Point::new(
                            if w > 0.0 { (p.x - b.x0) / w } else { 0.0 },
                            if h > 0.0 { (p.y - b.y0) / h } else { 0.0 },
                        )
                    })
                    .collect(),
                closed: *closed,
                handles: None,
            })
            .collect();
        (b, subpaths)
    }

    /// Sustituye los subtrazos (en mm) de un trazo, recalculando su caja.
    pub fn set_world_subpaths(&mut self, world: &[(Vec<Point>, bool)]) {
        let (rect, subpaths) = Shape::normalize_subpaths(world);
        self.rect = rect;
        if let ShapeKind::Path { subpaths: sp, .. } = &mut self.kind {
            *sp = subpaths;
        }
    }
}

#[cfg(test)]
mod gradient_tests {
    use super::*;

    #[test]
    fn samples_between_stops() {
        let g = Gradient {
            stops: vec![
                GradientStop {
                    pos: 1.0,
                    color: Color::WHITE,
                },
                GradientStop {
                    pos: 0.0,
                    color: Color::BLACK,
                },
            ],
            angle_deg: 0.0,
        };
        assert_eq!(g.sample(0.0), Color::BLACK);
        assert_eq!(g.sample(1.0), Color::WHITE);
        let mid = g.sample(0.5);
        assert!((mid.r as i32 - 128).abs() <= 1);
    }

    #[test]
    fn old_documents_get_default_fill_fields() {
        let mut s = Shape::new(
            1,
            "r",
            kurbo::Rect::new(0.0, 0.0, 1.0, 1.0),
            ShapeKind::Ellipse,
        );
        let mut v = serde_json::to_value(&s).unwrap();
        v.as_object_mut().unwrap().remove("fill_gradient");
        v.as_object_mut().unwrap().remove("fill_opacity");
        let back: Shape = serde_json::from_value(v).unwrap();
        s.fill_opacity = 1.0;
        assert_eq!(back, s);
    }
}
