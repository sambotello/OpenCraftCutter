//! Preferencias persistentes (se guardan con el almacenamiento de eframe).

use oc_core::machines::MachineId;
use oc_core::units::Unit;
use serde::{Deserialize, Serialize};

/// Paleta guardada por el usuario.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NamedPalette {
    pub name: String,
    pub colors: Vec<oc_core::Color>,
}

/// Paleta de degradados guardada por el usuario.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NamedGradients {
    pub name: String,
    pub gradients: Vec<oc_core::shape::Gradient>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Prefs {
    pub unit: Unit,
    pub machine: MachineId,
    pub library_root: Option<String>,
    pub show_grid: bool,
    pub grid_spacing_mm: f64,
    pub cut_margin_mm: f64,
    pub eraser_width_mm: f64,
    /// Últimos colores de relleno usados (el más reciente primero).
    pub recent_fill_colors: Vec<oc_core::Color>,
    pub fill_palettes: Vec<NamedPalette>,
    pub gradient_palettes: Vec<NamedGradients>,
    pub recent_line_colors: Vec<oc_core::Color>,
    pub line_palettes: Vec<NamedPalette>,
    /// Perfiles de calibración de cámara para PixScan.
    pub pixscan_calibrations: Vec<oc_trace::pixscan::CalibrationProfile>,
    /// Archivos abiertos o guardados recientemente (el más reciente primero).
    pub recent_files: Vec<String>,
    /// Materiales de corte (incluidos y del usuario). Vacío = los incluidos.
    pub materials: Vec<oc_core::send::Material>,
    /// Versión de los materiales incluidos guardados en `materials`.
    pub materials_version: u32,
    /// Opciones generales de Enviar.
    pub send_options: oc_core::send::SendOptions,
    /// Metros de cada cuchilla desde que se cambió.
    pub blades: Vec<oc_core::usage::BladeLog>,
    /// Recordatorio para cambiar una cuchilla (metros).
    pub blade_life_m: f64,
    /// Envíos a la máquina (el más reciente primero).
    pub history: Vec<oc_core::usage::HistoryEntry>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            unit: Unit::Cm,
            machine: MachineId::Cameo5,
            library_root: None,
            show_grid: true,
            grid_spacing_mm: 10.0,
            cut_margin_mm: 3.0,
            eraser_width_mm: 3.0,
            recent_fill_colors: Vec::new(),
            fill_palettes: Vec::new(),
            gradient_palettes: Vec::new(),
            recent_line_colors: Vec::new(),
            line_palettes: Vec::new(),
            pixscan_calibrations: Vec::new(),
            recent_files: Vec::new(),
            materials: Vec::new(),
            materials_version: 0,
            send_options: Default::default(),
            blades: Vec::new(),
            blade_life_m: oc_core::usage::DEFAULT_BLADE_LIFE_M,
            history: Vec::new(),
        }
    }
}

pub const STORAGE_KEY: &str = "opencraftcutter.prefs";

impl Prefs {
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Prefs {
        storage
            .and_then(|s| s.get_string(STORAGE_KEY))
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        if let Ok(t) = serde_json::to_string(self) {
            storage.set_string(STORAGE_KEY, t);
        }
    }

    /// Página inicial de un documento nuevo según las preferencias.
    pub fn page_setup(&self) -> oc_core::PageSetup {
        oc_core::PageSetup {
            unit: self.unit,
            machine: self.machine,
            show_grid: self.show_grid,
            grid_spacing_mm: self.grid_spacing_mm,
            cut_margin_mm: self.cut_margin_mm,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MemStorage(std::collections::HashMap<String, String>);

    impl eframe::Storage for MemStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn save_and_load_round_trip() {
        let p = Prefs {
            unit: Unit::Mm,
            grid_spacing_mm: 7.5,
            recent_files: vec!["/tmp/a.opencraftcutter".into()],
            materials: oc_core::send::builtin_materials(),
            materials_version: 4,
            ..Prefs::default()
        };
        let mut st = MemStorage::default();
        p.save(&mut st);
        let back = Prefs::load(Some(&st));
        assert_eq!(back.unit, Unit::Mm);
        assert!((back.grid_spacing_mm - 7.5).abs() < 1e-9);
        assert_eq!(back.recent_files, p.recent_files);
        assert_eq!(back.materials.len(), p.materials.len());
        assert_eq!(back.materials_version, 4);
    }

    #[test]
    fn missing_or_broken_prefs_fall_back_to_defaults() {
        assert_eq!(Prefs::load(None).unit, Prefs::default().unit);
        let mut st = MemStorage::default();
        eframe::Storage::set_string(&mut st, STORAGE_KEY, "{roto".into());
        assert_eq!(Prefs::load(Some(&st)).unit, Prefs::default().unit);
        // Preferencias antiguas sin los campos nuevos: se completan con los valores por defecto.
        eframe::Storage::set_string(&mut st, STORAGE_KEY, r#"{"show_grid":false}"#.into());
        let p = Prefs::load(Some(&st));
        assert!(!p.show_grid);
        assert!(p.materials.is_empty() && p.materials_version == 0);
    }

    #[test]
    fn new_documents_use_the_preferences() {
        let p = Prefs {
            cut_margin_mm: 5.0,
            show_grid: false,
            ..Prefs::default()
        };
        let page = p.page_setup();
        assert!((page.cut_margin_mm - 5.0).abs() < 1e-9);
        assert!(!page.show_grid);
    }
}
