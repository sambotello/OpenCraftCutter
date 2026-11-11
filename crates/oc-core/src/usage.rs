//! Historial de cortes y desgaste de cuchillas: qué se envió, con qué ajustes, cómo acabó y
//! cuántos metros lleva cada cuchilla desde que se cambió.

use crate::send::{Action, ActionSettings, Holder, Job, Tool};
use serde::{Deserialize, Serialize};

/// Recordatorio por defecto para cambiar una cuchilla (metros cortados). Depende mucho del
/// material: es solo un aviso que el usuario ajusta a su experiencia.
pub const DEFAULT_BLADE_LIFE_M: f64 = 100.0;
/// Entradas que se guardan en el historial (las más antiguas se descartan).
pub const HISTORY_MAX: usize = 200;

/// ¿Se desgasta con el uso? (los rotuladores y herramientas de marcar no cuentan).
pub fn is_blade(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::AutoBlade
            | Tool::RatchetBlade
            | Tool::DeepCutBlade
            | Tool::KraftBlade
            | Tool::RotaryBlade
            | Tool::PerforationBlade
    )
}

/// Metros que recorre cortando cada herramienta en estos trabajos (con pasadas).
pub fn meters_by_tool(jobs: &[Job]) -> Vec<(Tool, f64)> {
    let mut out: Vec<(Tool, f64)> = Vec::new();
    for j in jobs {
        let mm: f64 = j
            .paths
            .iter()
            .map(|(p, c)| crate::replicate::polyline_length(p, *c))
            .sum::<f64>()
            * j.settings.passes.max(1) as f64;
        match out.iter_mut().find(|(t, _)| *t == j.tool) {
            Some(e) => e.1 += mm / 1000.0,
            None => out.push((j.tool, mm / 1000.0)),
        }
    }
    out
}

/// Uso de una cuchilla desde que se cambió.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BladeLog {
    pub tool: Tool,
    pub meters: f64,
    /// Cuándo se puso (segundos Unix).
    pub since: i64,
}

/// Suma lo cortado a cada cuchilla (crea su registro si no existe).
pub fn add_wear(blades: &mut Vec<BladeLog>, used: &[(Tool, f64)], now: i64) {
    for &(tool, m) in used.iter().filter(|(t, _)| is_blade(*t)) {
        match blades.iter_mut().find(|b| b.tool == tool) {
            Some(b) => b.meters += m,
            None => blades.push(BladeLog {
                tool,
                meters: m,
                since: now,
            }),
        }
    }
}

/// Cuchillas que pasan del recordatorio.
pub fn worn(blades: &[BladeLog], life_m: f64) -> Vec<&BladeLog> {
    blades.iter().filter(|b| b.meters >= life_m).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobResult {
    Done,
    Cancelled,
    Failed(String),
}

impl JobResult {
    pub fn name(&self) -> &str {
        match self {
            JobResult::Done => "Terminado",
            JobResult::Cancelled => "Cancelado",
            JobResult::Failed(_) => "Error",
        }
    }
}

/// Ajustes de un trabajo enviado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryJob {
    pub holder: Holder,
    pub tool: Tool,
    pub action: Action,
    pub material: String,
    pub settings: ActionSettings,
}

/// Un envío a la máquina.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Segundos Unix.
    pub when: i64,
    pub document: String,
    pub test: bool,
    pub jobs: Vec<HistoryJob>,
    /// Metros cortados (todo el trabajo).
    pub meters: f64,
    pub estimate_s: f64,
    pub result: Option<JobResult>,
}

impl HistoryEntry {
    pub fn new(when: i64, document: &str, test: bool, jobs: &[Job]) -> HistoryEntry {
        HistoryEntry {
            when,
            document: document.to_string(),
            test,
            jobs: jobs
                .iter()
                .map(|j| HistoryJob {
                    holder: j.holder,
                    tool: j.tool,
                    action: j.action,
                    material: j.material.clone(),
                    settings: j.settings,
                })
                .collect(),
            meters: meters_by_tool(jobs).iter().map(|(_, m)| m).sum(),
            estimate_s: crate::send::estimate_seconds(jobs),
            result: None,
        }
    }
}

/// Añade al principio y recorta a [`HISTORY_MAX`].
pub fn push_history(history: &mut Vec<HistoryEntry>, e: HistoryEntry) {
    history.insert(0, e);
    history.truncate(HISTORY_MAX);
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Point;

    fn job(tool: Tool, len_mm: f64, passes: u32) -> Job {
        Job {
            holder: Holder::One,
            tool,
            action: Action::Cut,
            material: "Vinilo".into(),
            settings: ActionSettings {
                passes,
                ..Default::default()
            },
            paths: vec![(vec![Point::ZERO, Point::new(len_mm, 0.0)], false)],
            pause_after: false,
            missing_settings: false,
        }
    }

    #[test]
    fn wear_adds_up_per_blade_and_ignores_pens() {
        let jobs = [
            job(Tool::AutoBlade, 500.0, 2),
            job(Tool::SketchPen, 3000.0, 1),
            job(Tool::AutoBlade, 1000.0, 1),
        ];
        let used = meters_by_tool(&jobs);
        assert_eq!(used, vec![(Tool::AutoBlade, 2.0), (Tool::SketchPen, 3.0)]);
        let mut blades = Vec::new();
        add_wear(&mut blades, &used, 10);
        add_wear(&mut blades, &used, 20);
        assert_eq!(
            blades,
            vec![BladeLog {
                tool: Tool::AutoBlade,
                meters: 4.0,
                since: 10
            }]
        );
        assert!(worn(&blades, 5.0).is_empty());
        assert_eq!(worn(&blades, 4.0).len(), 1);
    }

    #[test]
    fn history_keeps_newest_first_and_is_capped() {
        let mut h = Vec::new();
        for i in 0..(HISTORY_MAX + 5) {
            push_history(
                &mut h,
                HistoryEntry::new(i as i64, "doc", false, &[job(Tool::AutoBlade, 100.0, 1)]),
            );
        }
        assert_eq!(h.len(), HISTORY_MAX);
        assert_eq!(h[0].when, (HISTORY_MAX + 4) as i64);
        assert!((h[0].meters - 0.1).abs() < 1e-9);
        assert_eq!(h[0].jobs[0].material, "Vinilo");
        // Se guarda y se vuelve a leer igual.
        let back: Vec<HistoryEntry> =
            serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
        assert_eq!(back, h);
    }
}
