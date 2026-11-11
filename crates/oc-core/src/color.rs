//! Color RGBA de 8 bits con conversión desde/hacia notación CSS.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    /// Rojo de línea de corte, como en Silhouette Studio.
    pub const CUT_RED: Color = Color::rgb(0xe3, 0x34, 0x3f);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    /// Interpreta `#rgb`, `#rrggbb` o `#rrggbbaa`.
    pub fn from_hex(s: &str) -> Option<Color> {
        let h = s.trim().trim_start_matches('#');
        let v = |i: usize, w: usize| u8::from_str_radix(&h[i..i + w], 16).ok();
        match h.len() {
            3 => {
                let d = |i: usize| v(i, 1).map(|x| x * 17);
                Some(Color::rgb(d(0)?, d(1)?, d(2)?))
            }
            6 => Some(Color::rgb(v(0, 2)?, v(2, 2)?, v(4, 2)?)),
            8 => Some(Color::rgba(v(0, 2)?, v(2, 2)?, v(4, 2)?, v(6, 2)?)),
            _ => None,
        }
    }

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(Color::from_hex("#e3343f"), Some(Color::CUT_RED));
        assert_eq!(Color::from_hex("fff"), Some(Color::WHITE));
        assert_eq!(Color::CUT_RED.to_hex(), "#e3343f");
        assert_eq!(Color::from_hex("#zz"), None);
    }
}

impl Color {
    /// Desde HSL (h en grados, s y l en 0..1).
    pub fn from_hsl(h: f64, s: f64, l: f64) -> Color {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let hp = (h.rem_euclid(360.0)) / 60.0;
        let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
        let (r, g, b) = match hp as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = l - c / 2.0;
        let to = |v: f64| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
        Color::rgb(to(r), to(g), to(b))
    }

    /// A HSL: (h en grados, s, l en 0..1).
    pub fn to_hsl(self) -> (f64, f64, f64) {
        let (r, g, b) = (
            self.r as f64 / 255.0,
            self.g as f64 / 255.0,
            self.b as f64 / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        if (max - min).abs() < 1e-12 {
            return (0.0, 0.0, l);
        }
        let d = max - min;
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == r {
            60.0 * (((g - b) / d) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        (h.rem_euclid(360.0), s, l)
    }
}

#[cfg(test)]
mod hsl_tests {
    use super::*;

    #[test]
    fn hsl_round_trip() {
        for c in [
            Color::CUT_RED,
            Color::rgb(0x22, 0xb8, 0xd8),
            Color::rgb(10, 200, 30),
            Color::WHITE,
        ] {
            let (h, s, l) = c.to_hsl();
            let back = Color::from_hsl(h, s, l);
            assert!(
                (back.r as i32 - c.r as i32).abs() <= 1
                    && (back.g as i32 - c.g as i32).abs() <= 1
                    && (back.b as i32 - c.b as i32).abs() <= 1,
                "{c:?} -> {back:?}"
            );
        }
    }
}
