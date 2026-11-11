//! Unidades de longitud. El modelo vive en mm; estas funciones convierten para la UI.

use serde::{Deserialize, Serialize};

pub const MM_PER_IN: f64 = 25.4;
/// 1 pt tipográfico = 1/72 in.
pub const MM_PER_PT: f64 = MM_PER_IN / 72.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Mm,
    #[default]
    Cm,
    In,
}

impl Unit {
    pub const ALL: [Unit; 3] = [Unit::Mm, Unit::Cm, Unit::In];

    pub fn factor_mm(self) -> f64 {
        match self {
            Unit::Mm => 1.0,
            Unit::Cm => 10.0,
            Unit::In => MM_PER_IN,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Unit::Mm => "mm",
            Unit::Cm => "cm",
            Unit::In => "in",
        }
    }

    pub fn from_mm(self, mm: f64) -> f64 {
        mm / self.factor_mm()
    }

    pub fn to_mm(self, value: f64) -> f64 {
        value * self.factor_mm()
    }

    /// Decimales razonables para mostrar en esta unidad.
    pub fn decimals(self) -> usize {
        match self {
            Unit::Mm => 1,
            Unit::Cm | Unit::In => 2,
        }
    }

    /// Paso de incremento de los campos numéricos, en la unidad.
    pub fn step(self) -> f64 {
        match self {
            Unit::Mm => 1.0,
            Unit::Cm => 0.1,
            Unit::In => 0.125,
        }
    }

    pub fn format(self, mm: f64) -> String {
        format!("{:.*} {}", self.decimals(), self.from_mm(mm), self.label())
    }
}

pub fn pt_to_mm(pt: f64) -> f64 {
    pt * MM_PER_PT
}

pub fn mm_to_pt(mm: f64) -> f64 {
    mm / MM_PER_PT
}
