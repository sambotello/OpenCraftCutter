//! Máquinas, tapetes, tipos de alimentación y tamaños de medios, con las mismas listas
//! (y en el mismo orden) que muestra el panel Configuración de página de la referencia.

use crate::units::MM_PER_IN;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------
// Máquinas
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MachineId {
    /// Detectar la máquina conectada (sin conexión: la predeterminada).
    Auto,
    #[default]
    Cameo5,
    Cameo5Plus,
    CameoProMark2,
    Portrait4,
    Curio2,
    Cameo4,
    CameoPlus,
    CameoPro,
    Portrait3,
    /// Heredadas.
    Cameo,
    Portrait,
    Curio,
    /// Otros tipos.
    Alta,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineGroup {
    Current,
    Legacy,
    Other,
}

impl MachineGroup {
    /// Título del grupo en el desplegable (`None` para el grupo principal).
    pub fn title(self) -> Option<&'static str> {
        match self {
            MachineGroup::Current => None,
            MachineGroup::Legacy => Some("Máquinas heredadas"),
            MachineGroup::Other => Some("Otros tipos de máquina"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Machine {
    pub id: MachineId,
    pub name: &'static str,
    pub group: MachineGroup,
    /// Ancho máximo de corte en mm (0 = no aplica).
    pub max_width_mm: f64,
    /// Tapete que usa "Auto (…)".
    pub default_mat: MatId,
    /// Nombre corto del tapete automático ("CAMEO", "Portrait", "CURIO").
    pub auto_mat_label: &'static str,
}

const IN12: f64 = 12.0 * MM_PER_IN;
const IN15: f64 = 15.0 * MM_PER_IN;
const IN8: f64 = 8.0 * MM_PER_IN;

/// Máquina que asume "Auto" mientras no haya una conectada.
pub const AUTO_DETECTED: MachineId = MachineId::Cameo5;

pub const MACHINES: [Machine; 15] = [
    Machine {
        id: MachineId::Auto,
        name: "Auto",
        group: MachineGroup::Current,
        max_width_mm: IN12,
        default_mat: MatId::Cameo12x12,
        auto_mat_label: "CAMEO",
    },
    Machine {
        id: MachineId::Cameo5,
        name: "CAMEO 5",
        group: MachineGroup::Current,
        max_width_mm: IN12,
        default_mat: MatId::Cameo12x12,
        auto_mat_label: "CAMEO",
    },
    Machine {
        id: MachineId::Cameo5Plus,
        name: "CAMEO 5 Plus",
        group: MachineGroup::Current,
        max_width_mm: IN15,
        default_mat: MatId::Cameo15x15,
        auto_mat_label: "CAMEO",
    },
    Machine {
        id: MachineId::CameoProMark2,
        name: "CAMEO Pro Mark 2",
        group: MachineGroup::Current,
        max_width_mm: 24.0 * MM_PER_IN,
        default_mat: MatId::CameoPro24x24,
        auto_mat_label: "CAMEO Pro",
    },
    Machine {
        id: MachineId::Portrait4,
        name: "Portrait 4",
        group: MachineGroup::Current,
        max_width_mm: IN8,
        default_mat: MatId::Portrait8x12,
        auto_mat_label: "Portrait",
    },
    Machine {
        id: MachineId::Curio2,
        name: "CURIO 2",
        group: MachineGroup::Current,
        max_width_mm: IN12,
        default_mat: MatId::Curio2Emboss,
        auto_mat_label: "CURIO",
    },
    Machine {
        id: MachineId::Cameo4,
        name: "CAMEO 4",
        group: MachineGroup::Current,
        max_width_mm: IN12,
        default_mat: MatId::Cameo12x12,
        auto_mat_label: "CAMEO",
    },
    Machine {
        id: MachineId::CameoPlus,
        name: "CAMEO Plus",
        group: MachineGroup::Current,
        max_width_mm: IN15,
        default_mat: MatId::Cameo15x15,
        auto_mat_label: "CAMEO",
    },
    Machine {
        id: MachineId::CameoPro,
        name: "CAMEO Pro",
        group: MachineGroup::Current,
        max_width_mm: 24.0 * MM_PER_IN,
        default_mat: MatId::CameoPro24x24,
        auto_mat_label: "CAMEO Pro",
    },
    Machine {
        id: MachineId::Portrait3,
        name: "Portrait 3",
        group: MachineGroup::Current,
        max_width_mm: IN8,
        default_mat: MatId::Portrait8x12,
        auto_mat_label: "Portrait",
    },
    Machine {
        id: MachineId::Cameo,
        name: "CAMEO",
        group: MachineGroup::Legacy,
        max_width_mm: IN12,
        default_mat: MatId::Cameo12x12,
        auto_mat_label: "CAMEO",
    },
    Machine {
        id: MachineId::Portrait,
        name: "Portrait",
        group: MachineGroup::Legacy,
        max_width_mm: IN8,
        default_mat: MatId::Portrait8x12,
        auto_mat_label: "Portrait",
    },
    Machine {
        id: MachineId::Curio,
        name: "CURIO",
        group: MachineGroup::Legacy,
        max_width_mm: IN8,
        default_mat: MatId::Curio2Emboss,
        auto_mat_label: "CURIO",
    },
    Machine {
        id: MachineId::Alta,
        name: "Alta",
        group: MachineGroup::Other,
        max_width_mm: 0.0,
        default_mat: MatId::None,
        auto_mat_label: "Ninguna",
    },
    Machine {
        id: MachineId::None,
        name: "Ninguno",
        group: MachineGroup::Other,
        max_width_mm: 0.0,
        default_mat: MatId::None,
        auto_mat_label: "Ninguna",
    },
];

pub fn machine(id: MachineId) -> &'static Machine {
    MACHINES.iter().find(|m| m.id == id).unwrap_or(&MACHINES[1])
}

/// Máquina efectiva (resuelve "Auto").
pub fn effective_machine(id: MachineId) -> &'static Machine {
    machine(if id == MachineId::Auto {
        AUTO_DETECTED
    } else {
        id
    })
}

/// Etiqueta para mostrar: "Auto (CAMEO 5)" o el nombre.
pub fn machine_label(id: MachineId) -> String {
    if id == MachineId::Auto {
        format!("Auto ({})", machine(AUTO_DETECTED).name)
    } else {
        machine(id).name.to_string()
    }
}

// ---------------------------------------------------------------------------------------------
// Tapetes
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MatId {
    #[default]
    Auto,
    None,
    /// Alfombrilla electrostática de 12 in.
    Electrostatic12,
    Cameo12x12,
    Cameo12x24,
    Portrait8x12,
    Stamp,
    PixScanCameo,
    Curio2Emboss,
    /// Tapete de 15 × 15 in de los modelos Plus.
    Cameo15x15,
    /// Tapete de 24 × 24 in de los modelos Pro.
    CameoPro24x24,
    /// Tapete del Portrait original (8 × 12 in), en la lista de los modelos Pro.
    PortraitLegacy8x12,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat {
    pub id: MatId,
    pub name: &'static str,
    pub width_mm: f64,
    pub height_mm: f64,
}

/// Tapetes en el orden del desplegable (tras "Auto").
pub const MATS: [Mat; 11] = [
    Mat {
        id: MatId::None,
        name: "Ninguna",
        width_mm: 0.0,
        height_mm: 0.0,
    },
    Mat {
        id: MatId::CameoPro24x24,
        name: "CAMEO Pro",
        width_mm: 609.6,
        height_mm: 609.6,
    },
    Mat {
        id: MatId::Electrostatic12,
        name: "Alfombrilla electrostática, 12 pulgadas",
        width_mm: 305.0,
        height_mm: 305.0,
    },
    Mat {
        id: MatId::Cameo12x12,
        name: "CAMEO",
        width_mm: 305.0,
        height_mm: 305.0,
    },
    Mat {
        id: MatId::Cameo12x24,
        name: "CAMEO",
        width_mm: 305.0,
        height_mm: 610.0,
    },
    Mat {
        id: MatId::Cameo15x15,
        name: "CAMEO Plus",
        width_mm: 381.0,
        height_mm: 381.0,
    },
    Mat {
        id: MatId::PortraitLegacy8x12,
        name: "Portrait",
        width_mm: 203.2,
        height_mm: 304.8,
    },
    Mat {
        id: MatId::Portrait8x12,
        name: "Portrait 4",
        width_mm: 216.0,
        height_mm: 305.0,
    },
    Mat {
        id: MatId::Stamp,
        name: "Sello",
        width_mm: 152.0,
        height_mm: 203.0,
    },
    Mat {
        id: MatId::PixScanCameo,
        name: "PixScan CAMEO",
        width_mm: 297.0,
        height_mm: 216.0,
    },
    Mat {
        id: MatId::Curio2Emboss,
        name: "CURIO 2 (relieve)",
        width_mm: 305.0,
        height_mm: 305.0,
    },
];

pub fn mat(id: MatId) -> Option<&'static Mat> {
    MATS.iter().find(|m| m.id == id)
}

/// Tapetes ofrecidos para una máquina, como la referencia: el de 15 × 15 solo en los modelos
/// Plus y Pro; los Pro, además, con el de 24 × 24 y sin la alfombrilla electrostática (la Pro
/// original tampoco ofrece los de Portrait 4 ni CURIO 2).
pub fn mats_for(machine_id: MachineId) -> Vec<&'static Mat> {
    let id = effective_machine(machine_id).id;
    let pro = matches!(id, MachineId::CameoPro | MachineId::CameoProMark2);
    let plus = matches!(id, MachineId::Cameo5Plus | MachineId::CameoPlus);
    MATS.iter()
        .filter(|m| match m.id {
            MatId::CameoPro24x24 | MatId::PortraitLegacy8x12 => pro,
            MatId::Cameo15x15 => plus || pro,
            MatId::Electrostatic12 => !pro,
            MatId::Portrait8x12 | MatId::Curio2Emboss => id != MachineId::CameoPro,
            _ => true,
        })
        .collect()
}

/// Resuelve `Auto` al tapete por defecto de la máquina.
pub fn resolve_mat(machine_id: MachineId, mat_id: MatId) -> &'static Mat {
    let id = if mat_id == MatId::Auto {
        effective_machine(machine_id).default_mat
    } else {
        mat_id
    };
    mat(id).unwrap_or(&MATS[0])
}

/// Etiqueta del tapete automático: "Auto (CAMEO)".
pub fn auto_mat_label(machine_id: MachineId) -> String {
    format!("Auto ({})", effective_machine(machine_id).auto_mat_label)
}

// ---------------------------------------------------------------------------------------------
// Tipo de alimentación
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FeedType {
    #[default]
    Auto,
    Manual,
    /// Alimentador automático de hojas A4.
    #[serde(alias = "tray", alias = "roll")]
    A4SheetFeeder,
}

pub const FEED_TYPES_ALL: [FeedType; 3] =
    [FeedType::Auto, FeedType::Manual, FeedType::A4SheetFeeder];

impl FeedType {
    pub const ALL: [FeedType; 3] = FEED_TYPES_ALL;

    pub fn name(self) -> &'static str {
        match self {
            FeedType::Auto => "Auto (Manual)",
            FeedType::Manual => "Manual",
            FeedType::A4SheetFeeder => "A4 Auto Sheet Feeder",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Tamaños de medios
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaSize {
    pub id: &'static str,
    pub name: &'static str,
    /// 0 × 0 para "Auto" (tamaño del tapete) y "A medida".
    pub width_mm: f64,
    pub height_mm: f64,
}

pub const MEDIA_AUTO: &str = "auto";
pub const MEDIA_CUSTOM: &str = "custom";

/// En el orden del desplegable.
pub const MEDIA_SIZES: [MediaSize; 12] = [
    MediaSize {
        id: MEDIA_AUTO,
        name: "Auto",
        width_mm: 0.0,
        height_mm: 0.0,
    },
    MediaSize {
        id: MEDIA_CUSTOM,
        name: "A medida",
        width_mm: 0.0,
        height_mm: 0.0,
    },
    MediaSize {
        id: "24x24",
        name: "24x24",
        width_mm: 609.6,
        height_mm: 609.6,
    },
    MediaSize {
        id: "12x24",
        name: "12x24",
        width_mm: 304.8,
        height_mm: 609.6,
    },
    MediaSize {
        id: "12x12",
        name: "12x12",
        width_mm: 304.8,
        height_mm: 304.8,
    },
    MediaSize {
        id: "letter",
        name: "Carta",
        width_mm: 215.9,
        height_mm: 279.4,
    },
    MediaSize {
        id: "a4",
        name: "A4",
        width_mm: 210.0,
        height_mm: 297.0,
    },
    MediaSize {
        id: "a3",
        name: "A3",
        width_mm: 297.0,
        height_mm: 420.0,
    },
    MediaSize {
        id: "a5",
        name: "A5",
        width_mm: 148.0,
        height_mm: 210.0,
    },
    MediaSize {
        id: "a6",
        name: "A6",
        width_mm: 105.0,
        height_mm: 148.0,
    },
    MediaSize {
        id: "5x7",
        name: "5x7",
        width_mm: 127.0,
        height_mm: 177.8,
    },
    MediaSize {
        id: "printer",
        name: "Printer",
        width_mm: 209.9,
        height_mm: 297.04,
    },
];

/// Tamaños de medios ofrecidos para una máquina: 24x24, 12x24 y A3 solo en los modelos Pro.
pub fn media_sizes_for(machine_id: MachineId) -> Vec<&'static MediaSize> {
    let pro = matches!(
        effective_machine(machine_id).id,
        MachineId::CameoPro | MachineId::CameoProMark2
    );
    MEDIA_SIZES
        .iter()
        .filter(|m| pro || !matches!(m.id, "24x24" | "12x24" | "a3"))
        .collect()
}

pub fn media_size(id: &str) -> Option<&'static MediaSize> {
    MEDIA_SIZES.iter().find(|m| m.id == id)
}

/// Etiqueta de "Auto": "Auto (12x12)" según el tapete en uso.
pub fn auto_media_label(machine_id: MachineId, mat_id: MatId) -> String {
    let m = resolve_mat(machine_id, mat_id);
    if m.width_mm <= 0.0 {
        return "Auto".into();
    }
    let w = (m.width_mm / MM_PER_IN).round();
    let h = (m.height_mm / MM_PER_IN).round();
    format!("Auto ({w}x{h})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pro_machines_use_24x24() {
        for id in [MachineId::CameoPro, MachineId::CameoProMark2] {
            let m = resolve_mat(id, MatId::Auto);
            assert_eq!(m.id, MatId::CameoPro24x24);
            assert_eq!(auto_mat_label(id), "Auto (CAMEO Pro)");
            assert_eq!(auto_media_label(id, MatId::Auto), "Auto (24x24)");
            let mats: Vec<_> = mats_for(id).iter().map(|m| m.id).collect();
            assert!(mats.contains(&MatId::CameoPro24x24) && mats.contains(&MatId::Cameo15x15));
            assert!(!mats.contains(&MatId::Electrostatic12));
            assert!(media_sizes_for(id).iter().any(|m| m.id == "24x24"));
        }
        assert!(
            !mats_for(MachineId::CameoPro)
                .iter()
                .any(|m| m.id == MatId::Curio2Emboss)
        );
        assert!(
            mats_for(MachineId::CameoProMark2)
                .iter()
                .any(|m| m.id == MatId::Curio2Emboss)
        );
        assert!(
            !mats_for(MachineId::Cameo5)
                .iter()
                .any(|m| m.id == MatId::CameoPro24x24)
        );
        assert!(
            !media_sizes_for(MachineId::Cameo5)
                .iter()
                .any(|m| m.id == "24x24")
        );
    }

    #[test]
    fn auto_labels_resolve_to_cameo5() {
        assert_eq!(machine_label(MachineId::Auto), "Auto (CAMEO 5)");
        assert_eq!(auto_mat_label(MachineId::Cameo5), "Auto (CAMEO)");
        assert_eq!(
            auto_media_label(MachineId::Cameo5, MatId::Auto),
            "Auto (12x12)"
        );
    }

    #[test]
    fn plus_mat_only_for_plus_models() {
        assert!(
            !mats_for(MachineId::Cameo5)
                .iter()
                .any(|m| m.id == MatId::Cameo15x15)
        );
        assert!(
            mats_for(MachineId::Cameo5Plus)
                .iter()
                .any(|m| m.id == MatId::Cameo15x15)
        );
    }

    #[test]
    fn old_feed_names_still_deserialize() {
        let f: FeedType = serde_json::from_str("\"tray\"").unwrap();
        assert_eq!(f, FeedType::A4SheetFeeder);
    }
}
