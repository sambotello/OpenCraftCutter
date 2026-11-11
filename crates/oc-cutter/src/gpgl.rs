//! Traducción de los trabajos de Enviar a GPGL (Graphtec Plotter Graphics Language), el lenguaje
//! de las cortadoras Silhouette. Sin E/S: devuelve los bytes que hay que enviar.
//!
//! Coordenadas: unidades de la máquina (SU) = 1/20 mm. La máquina usa (y, x): la primera
//! coordenada es la de avance del material (hacia abajo en el tapete) y la segunda, la del carro.
//! Cada comando termina en ETX (0x03).

use kurbo::Point;
use oc_core::send::{Action, ActionSettings, FeedOption, Job, Tool, stipple_points};

pub const ETX: u8 = 0x03;
pub const ESC: u8 = 0x1b;

/// Consultas de solo lectura.
pub const QUERY_STATUS: &[u8] = &[ESC, 0x05];
pub const QUERY_TOOLS: &[u8] = &[ESC, 0x15];
pub const QUERY_FIRMWARE: &[u8] = b"FG\x03";

/// mm → unidades de la máquina.
pub fn su(mm: f64) -> i64 {
    (mm * 20.0).round() as i64
}

/// Estado que devuelve `ESC ENQ`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ready,
    Moving,
    Unloaded,
    Other(u8),
}

impl Status {
    pub fn parse(resp: &[u8]) -> Option<Status> {
        let b = *resp.iter().find(|b| b.is_ascii_digit())?;
        Some(match b {
            b'0' => Status::Ready,
            b'1' => Status::Moving,
            b'2' => Status::Unloaded,
            o => Status::Other(o),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Status::Ready => "Lista",
            Status::Moving => "Trabajando",
            Status::Unloaded => "Sin material cargado",
            Status::Other(_) => "Estado desconocido",
        }
    }
}

/// Herramienta detectada en un portaherramientas (`ESC NAK`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedTool {
    Empty,
    Known(Tool),
    Unknown(u32),
}

impl DetectedTool {
    pub fn from_code(c: u32) -> DetectedTool {
        match c {
            0 => DetectedTool::Empty,
            1 => DetectedTool::Known(Tool::RatchetBlade),
            2 => DetectedTool::Known(Tool::AutoBlade),
            3 => DetectedTool::Known(Tool::DeepCutBlade),
            4 => DetectedTool::Known(Tool::KraftBlade),
            5 => DetectedTool::Known(Tool::RotaryBlade),
            7 => DetectedTool::Known(Tool::SketchPen),
            o => DetectedTool::Unknown(o),
        }
    }

    pub fn name(self) -> String {
        match self {
            DetectedTool::Empty => "Vacío".into(),
            DetectedTool::Known(t) => t.name().into(),
            DetectedTool::Unknown(c) => format!("Herramienta {c}"),
        }
    }
}

/// Respuesta de `ESC NAK`: " 2, 0" → herramienta de cada portaherramientas.
pub fn parse_tools(resp: &[u8]) -> Vec<DetectedTool> {
    String::from_utf8_lossy(resp)
        .trim_matches(|c: char| c == '\u{3}' || c.is_whitespace())
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .map(DetectedTool::from_code)
        .collect()
}

/// Tapete para `TG`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatCode {
    None,
    Cameo12x12,
    Cameo12x24,
    Plus15x15,
    Pro24x24,
}

impl MatCode {
    fn code(self) -> u8 {
        match self {
            MatCode::None => 0,
            MatCode::Cameo12x12 => 1,
            MatCode::Cameo12x24 => 2,
            MatCode::Plus15x15 => 8,
            MatCode::Pro24x24 => 9,
        }
    }

    pub fn from_mat(mat: &oc_core::machines::Mat) -> MatCode {
        let w = mat.width_mm.round() as i64;
        let h = mat.height_mm.round() as i64;
        match (w, h) {
            (0, _) => MatCode::None,
            (305, 610) | (305, 609) => MatCode::Cameo12x24,
            (381, 381) => MatCode::Plus15x15,
            (610, 610) | (609, 609) => MatCode::Pro24x24,
            _ => MatCode::Cameo12x12,
        }
    }
}

/// Parámetros de la hoja de trabajo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Media {
    pub mat: MatCode,
    /// Área de corte (mm).
    pub width_mm: f64,
    pub height_mm: f64,
    pub feed: FeedOption,
    /// Imprimir y cortar: la máquina busca las marcas antes de cortar.
    pub registration: Option<Registration>,
}

/// Marcas de registro impresas (mm, coordenadas del documento).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Registration {
    /// Esquina exterior del cuadrado (arriba a la izquierda del marco de marcas). Tras la
    /// lectura, la máquina toma este punto como origen.
    pub origin: Point,
    /// Distancia entre marcas: ancho (carro) y alto (avance del material).
    pub width_mm: f64,
    pub height_mm: f64,
    /// Longitud de los brazos de las escuadras y grosor de las líneas.
    pub length_mm: f64,
    pub thickness_mm: f64,
}

/// Margen de búsqueda alrededor de la primera marca (mm).
pub const REGISTRATION_SEARCH_MM: f64 = 10.0;

/// Búsqueda automática de las marcas (tipo Cameo: cuadrado + dos escuadras). La máquina
/// responde cuando termina: «0» si las encontró.
///
/// Velocidad 10 para leer las marcas (la del trabajo anterior podría ser demasiado alta para el
/// sensor), `TB51` antes de `TB52` y `TB54,0,0`, cuyo significado no se conoce pero se envía siempre.
pub fn registration(r: &Registration) -> Vec<u8> {
    let mut o = Out(Vec::new());
    o.cmd("!10,0");
    o.cmd("TB99");
    o.cmd(format!("TB51,{}", su(r.length_mm)));
    o.cmd("TB52,2");
    o.cmd(format!("TB53,{}", su(r.thickness_mm)));
    o.cmd("TB54,0,0");
    o.cmd("TB55,1");
    o.cmd(format!(
        "TB123,{},{},{},{}",
        su(r.height_mm),
        su(r.width_mm),
        su((r.origin.y - REGISTRATION_SEARCH_MM).max(0.0)),
        su((r.origin.x - REGISTRATION_SEARCH_MM).max(0.0)),
    ));
    o.0
}

/// ¿Encontró la máquina las marcas? (respuesta «    0» + ETX).
pub fn registration_found(resp: &[u8]) -> bool {
    resp.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'0')
}

struct Out(Vec<u8>);

impl Out {
    fn cmd(&mut self, c: impl AsRef<str>) {
        self.0.extend_from_slice(c.as_ref().as_bytes());
        self.0.push(ETX);
    }
}

/// Cabecera: tapete, orientación y área de corte.
pub fn header(media: &Media) -> Vec<u8> {
    let mut o = setup(media);
    o.0.extend(area(media));
    o.0
}

fn setup(media: &Media) -> Out {
    let mut o = Out(Vec::new());
    o.cmd(format!("TG{}", media.mat.code()));
    o.cmd("FN0");
    o.cmd("TB50,0");
    o
}

/// Área de corte: con marcas, el marco entre ellas.
fn area(media: &Media) -> Vec<u8> {
    let (w, h) = match &media.registration {
        Some(r) => (r.width_mm, r.height_mm),
        None => (media.width_mm, media.height_mm),
    };
    let mut o = Out(Vec::new());
    o.cmd("FM1");
    o.cmd("\\0,0");
    o.cmd(format!("Z{},{}", su(h), su(w)));
    o.0
}

/// Desplazamiento de la punta para `FC` (SU): distancia entre el eje de giro y el filo. Valores por
/// herramienta: 18 para las cuchillas normales, 68 para la kraft (falta confirmar si la de la
/// Cameo 5 necesita 88) y 40 para la rotatoria. Las herramientas que no giran (rotuladores,
/// repujado, grabado, punta de calor, piel) no llevan desplazamiento.
fn tip_offset(job: &Job) -> u32 {
    if job.action == Action::Sketch {
        return 0;
    }
    match job.tool {
        Tool::AutoBlade | Tool::RatchetBlade | Tool::DeepCutBlade | Tool::PerforationBlade => 18,
        Tool::KraftBlade => 68,
        Tool::RotaryBlade => 40,
        Tool::SketchPen
        | Tool::PenAdapter
        | Tool::EmbossingTool
        | Tool::EngravingTool
        | Tool::HeatPen
        | Tool::LeatherPunch
        | Tool::LeatherCarver => 0,
    }
}

/// Ajustes de herramienta de un trabajo (como en la referencia para la Cameo 4/5).
fn tool_setup(o: &mut Out, job: &Job) {
    let h = job.holder.number();
    let s: &ActionSettings = &job.settings;
    let pen =
        job.action == Action::Sketch || matches!(job.tool, Tool::SketchPen | Tool::PenAdapter);
    o.cmd(format!("J{h}"));
    o.cmd(format!("FX{},{h}", s.force.clamp(1, 33)));
    o.cmd("TJ0");
    o.cmd(format!("!{},{h}", s.speed.clamp(1, 30)));
    o.cmd(format!("FC0,1,{h}"));
    o.cmd(format!("FE0,{h}"));
    // Sobrecorte de esquinas en décimas de mm (0 = sin sobrecorte o rotulador).
    let (a, b) = if pen || !s.overcut {
        (0, 0)
    } else {
        (
            ((s.overcut_start_mm + 0.05) * 10.0) as i64,
            ((s.overcut_end_mm + 0.05) * 10.0) as i64,
        )
    };
    o.cmd(format!("FF{a},0,{h}"));
    o.cmd(format!("FF{a},{b},{h}"));
    o.cmd(format!("FX{},{h}", s.force.clamp(1, 33)));
    o.cmd(format!("TJ{}", s.accel.clamp(1, 9)));
    o.cmd(format!("FC{},1,{h}", tip_offset(job)));
    if job.tool == Tool::AutoBlade && h == 1 && job.action != Action::Sketch {
        o.cmd(format!("TF{},1", s.depth.min(10)));
    }
    if s.track_enhancing {
        o.cmd("FY0");
    }
}

fn mv(o: &mut Out, p: Point) {
    o.cmd(format!("M{},{}", su(p.y), su(p.x)));
}

fn dr(o: &mut Out, p: Point) {
    o.cmd(format!("D{},{}", su(p.y), su(p.x)));
}

/// Trazos de un trabajo, con pasadas y punteado. El sobrecorte de esquinas lo hace la máquina (FF).
fn job_paths(o: &mut Out, job: &Job) {
    for _ in 0..job.settings.passes.max(1) {
        for (pts, closed) in &job.paths {
            match job.action {
                Action::Stipple => {
                    for p in stipple_points(pts, *closed, job.settings.stipple_spacing_mm) {
                        mv(o, p);
                        dr(o, p);
                    }
                }
                _ => {
                    let Some(first) = pts.first() else { continue };
                    mv(o, *first);
                    for p in &pts[1..] {
                        dr(o, *p);
                    }
                    if *closed && pts.len() > 2 {
                        dr(o, *first);
                    }
                }
            }
        }
    }
}

/// Bytes de un trabajo (ajustes de herramienta + trazos).
pub fn encode_job(job: &Job) -> Vec<u8> {
    let mut o = Out(Vec::new());
    tool_setup(&mut o, job);
    job_paths(&mut o, job);
    o.0
}

/// Final: volver al origen o hacer avanzar el material hasta debajo del diseño.
pub fn trailer(media: &Media, jobs: &[Job]) -> Vec<u8> {
    let mut o = Out(Vec::new());
    match media.feed {
        FeedOption::ReturnToOrigin => {
            o.cmd("L0");
            o.cmd("\\0,0");
            o.cmd("M0,0");
            o.cmd("J0");
            o.cmd("FN0");
            o.cmd("TB50,0");
        }
        FeedOption::FeedOut => {
            let bottom = jobs
                .iter()
                .flat_map(|j| j.paths.iter())
                .flat_map(|(p, _)| p.iter().map(|q| q.y))
                .fold(0.0f64, f64::max);
            o.cmd(format!("M{},0", su(bottom + 10.0)));
            o.cmd("SO0");
            o.cmd("FN0");
        }
    }
    o.0
}

/// Trabajo completo dividido en tramos: cabecera, un tramo por trabajo y final. Entre tramos
/// la aplicación puede esperar (pausas) o informar del progreso.
///
/// Con marcas de registro hay dos tramos de cabecera: el primero termina con la búsqueda de
/// marcas (hay que esperar la respuesta, ver [`registration_segment`]) y el segundo fija el
/// área; los trazos pasan a medirse desde la primera marca.
pub fn encode(media: &Media, jobs: &[Job]) -> Vec<Vec<u8>> {
    let Some(r) = &media.registration else {
        let mut out = vec![header(media)];
        out.extend(jobs.iter().map(encode_job));
        out.push(trailer(media, jobs));
        return out;
    };
    let shift = -r.origin.to_vec2();
    let jobs: Vec<Job> = jobs
        .iter()
        .map(|j| {
            let mut j = j.clone();
            for (pts, _) in &mut j.paths {
                for p in pts.iter_mut() {
                    *p += shift;
                }
            }
            j
        })
        .collect();
    let mut first = setup(media);
    first.0.extend(registration(r));
    let mut out = vec![first.0, area(media)];
    out.extend(jobs.iter().map(encode_job));
    out.push(trailer(media, &jobs));
    out
}

/// Tramo tras el que hay que esperar a que la máquina lea las marcas.
pub fn registration_segment(media: &Media) -> Option<usize> {
    media.registration.map(|_| 0)
}

/// Tramos de cabecera antes del primer trabajo.
pub fn header_segments(media: &Media) -> usize {
    if media.registration.is_some() { 2 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::send::{Holder, Job};

    fn job(action: Action, tool: Tool) -> Job {
        Job {
            holder: Holder::One,
            tool,
            action,
            material: "x".into(),
            settings: ActionSettings {
                force: 12,
                speed: 5,
                passes: 1,
                accel: 3,
                depth: 2,
                ..Default::default()
            },
            paths: vec![(
                vec![
                    Point::new(10.0, 20.0),
                    Point::new(30.0, 20.0),
                    Point::new(30.0, 40.0),
                ],
                true,
            )],
            pause_after: false,
            missing_settings: false,
        }
    }

    fn cmds(b: &[u8]) -> Vec<String> {
        b.split(|c| *c == ETX)
            .filter(|c| !c.is_empty())
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect()
    }

    #[test]
    fn dump_test_shape_commands() {
        // OPENCRAFTCUTTER_GPGL_DUMP=1: imprime los comandos de la forma de prueba con «Cartulina lisa».
        if std::env::var("OPENCRAFTCUTTER_GPGL_DUMP").is_err() {
            return;
        }
        use oc_core::send::*;
        let mats = builtin_materials();
        let lisa = mats
            .iter()
            .position(|m| m.name == DEFAULT_MATERIAL)
            .unwrap();
        let mut doc = oc_core::Document::default();
        for s in test_shapes(Point::new(8.0, 8.0)) {
            doc.add_shape(s);
        }
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
        let jobs = standard_jobs(&doc, &setups, &mats, &SendOptions::default());
        let media = Media {
            mat: MatCode::Cameo12x12,
            width_mm: 304.8,
            height_mm: 304.8,
            feed: FeedOption::ReturnToOrigin,
            registration: None,
        };
        for seg in encode(&media, &jobs) {
            eprintln!("{}", cmds(&seg).join("  "));
        }
        eprintln!("{:?}", jobs[0].settings);
    }

    #[test]
    fn units_and_axes() {
        assert_eq!(su(1.0), 20);
        let c = cmds(&encode_job(&job(Action::Cut, Tool::AutoBlade)));
        // (x=10, y=20) → M400,200; cerrado vuelve al primero.
        assert!(c.contains(&"M400,200".to_string()));
        assert_eq!(c.last().unwrap(), "D400,200");
    }

    #[test]
    fn autoblade_depth_and_blade_offset() {
        let c = cmds(&encode_job(&job(Action::Cut, Tool::AutoBlade)));
        assert!(c.contains(&"J1".to_string()));
        assert!(c.contains(&"FX12,1".to_string()));
        assert!(c.contains(&"!5,1".to_string()));
        assert!(c.contains(&"TF2,1".to_string()));
        assert!(c.contains(&"FC18,1,1".to_string()));
    }

    #[test]
    fn pen_has_no_offset_or_depth() {
        let c = cmds(&encode_job(&job(Action::Sketch, Tool::SketchPen)));
        assert!(!c.iter().any(|s| s.starts_with("TF")));
        assert!(!c.contains(&"FC18,1,1".to_string()));
    }

    /// Desplazamiento de la punta por herramienta.
    #[test]
    fn tip_offset_per_tool() {
        let last_fc = |action, tool| {
            cmds(&encode_job(&job(action, tool)))
                .into_iter()
                .rev()
                .find(|s| s.starts_with("FC"))
                .expect("hay FC")
        };
        for (tool, fc) in [
            (Tool::AutoBlade, "FC18,1,1"),
            (Tool::RatchetBlade, "FC18,1,1"),
            (Tool::DeepCutBlade, "FC18,1,1"),
            (Tool::PerforationBlade, "FC18,1,1"),
            (Tool::KraftBlade, "FC68,1,1"),
            (Tool::RotaryBlade, "FC40,1,1"),
            (Tool::SketchPen, "FC0,1,1"),
            (Tool::PenAdapter, "FC0,1,1"),
            (Tool::EmbossingTool, "FC0,1,1"),
            (Tool::EngravingTool, "FC0,1,1"),
            (Tool::HeatPen, "FC0,1,1"),
            (Tool::LeatherPunch, "FC0,1,1"),
            (Tool::LeatherCarver, "FC0,1,1"),
        ] {
            assert_eq!(last_fc(Action::Cut, tool), fc, "{tool:?}");
        }
        assert_eq!(
            last_fc(Action::Sketch, Tool::AutoBlade),
            "FC0,1,1",
            "esbozar con cuchilla"
        );
    }

    #[test]
    fn passes_repeat_paths() {
        let mut j = job(Action::Cut, Tool::AutoBlade);
        j.settings.passes = 2;
        let n = cmds(&encode_job(&j))
            .iter()
            .filter(|s| s.starts_with('M'))
            .count();
        assert_eq!(n, 2);
    }

    #[test]
    fn status_and_tools_parsing() {
        assert_eq!(Status::parse(b"2\x03"), Some(Status::Unloaded));
        assert_eq!(Status::parse(b"0\x03"), Some(Status::Ready));
        assert_eq!(
            parse_tools(b" 2, 0\x03"),
            vec![DetectedTool::Known(Tool::AutoBlade), DetectedTool::Empty]
        );
    }

    #[test]
    fn header_and_trailer() {
        let media = Media {
            mat: MatCode::Cameo12x12,
            width_mm: 304.8,
            height_mm: 304.8,
            feed: FeedOption::ReturnToOrigin,
            registration: None,
        };
        let h = cmds(&header(&media));
        assert_eq!(h[0], "TG1");
        assert!(h.contains(&"Z6096,6096".to_string()));
        let t = cmds(&trailer(&media, &[]));
        assert_eq!(t, vec!["L0", "\\0,0", "M0,0", "J0", "FN0", "TB50,0"]);
    }

    /// Búsqueda de marcas, incluido el margen de 10 mm antes de la primera marca.
    #[test]
    fn registration_sequence() {
        let r = Registration {
            origin: Point::new(15.0, 12.0),
            width_mm: 180.0,
            height_mm: 250.0,
            length_mm: 20.0,
            thickness_mm: 0.5,
        };
        assert_eq!(
            cmds(&registration(&r)),
            vec![
                "!10,0",
                "TB99",
                "TB51,400",
                "TB52,2",
                "TB53,10",
                "TB54,0,0",
                "TB55,1",
                "TB123,5000,3600,40,100",
            ]
        );
    }
}
