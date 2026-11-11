//! Fuente de un solo trazo (Hershey Sans) para dibujar con rotulador: cada letra son líneas
//! abiertas que la herramienta recorre una vez, en vez del contorno de una letra rellena.
//! Los datos y su reconocimiento van en `assets/fonts/HersheySans1.svg`.

use kurbo::{BezPath, Point};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Nombre de la fuente en el panel de texto.
pub const FAMILY: &str = "Hershey Sans (un trazo)";

const SVG: &str = include_str!("../assets/fonts/HersheySans1.svg");
const UNITS_PER_EM: f64 = 1000.0;
const ASCENT: f64 = 800.0;

struct Glyph {
    advance: f64,
    strokes: Vec<Vec<Point>>,
}

struct Font {
    glyphs: HashMap<char, Glyph>,
    default_advance: f64,
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let i = tag.find(&key)? + key.len();
    let j = tag[i..].find('"')? + i;
    Some(&tag[i..j])
}

/// Carácter de un atributo `unicode` (con las entidades XML que usa el archivo).
fn unescape(s: &str) -> Option<char> {
    let t = match s {
        "&amp;" => "&",
        "&apos;" => "'",
        "&quot;" => "\"",
        "&lt;" => "<",
        "&gt;" => ">",
        other => other,
    };
    if let Some(hex) = t.strip_prefix("&#x").and_then(|h| h.strip_suffix(';')) {
        return char::from_u32(u32::from_str_radix(hex, 16).ok()?);
    }
    let mut it = t.chars();
    let c = it.next()?;
    it.next().is_none().then_some(c)
}

fn font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        let default_advance = SVG
            .find("<font ")
            .and_then(|i| attr(&SVG[i..], "horiz-adv-x"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(378.0);
        let mut glyphs = HashMap::new();
        for chunk in SVG.split("<glyph").skip(1) {
            let tag = &chunk[..chunk.find("/>").unwrap_or(chunk.len())];
            let tag = format!(" {}", tag.replace(['\n', '\t'], " "));
            let Some(c) = attr(&tag, "unicode").and_then(unescape) else {
                continue;
            };
            let advance = attr(&tag, "horiz-adv-x")
                .and_then(|v| v.parse().ok())
                .unwrap_or(default_advance);
            let mut strokes = Vec::new();
            if let Some(d) = attr(&tag, "d") {
                if let Ok(path) = BezPath::from_svg(d) {
                    let mut cur: Vec<Point> = Vec::new();
                    kurbo::flatten(path, 2.0, |el| match el {
                        kurbo::PathEl::MoveTo(p) => {
                            if cur.len() >= 2 {
                                strokes.push(std::mem::take(&mut cur));
                            }
                            cur = vec![p];
                        }
                        kurbo::PathEl::LineTo(p) => cur.push(p),
                        _ => {}
                    });
                    if cur.len() >= 2 {
                        strokes.push(cur);
                    }
                }
            }
            glyphs.insert(c, Glyph { advance, strokes });
        }
        Font {
            glyphs,
            default_advance,
        }
    })
}

/// ¿Tiene la fuente esta letra?
pub fn has_glyph(c: char) -> bool {
    font().glyphs.contains_key(&c)
}

/// Líneas (abiertas) de un texto de `size_mm` de cuerpo, con la parte superior del cuerpo en
/// y = 0 y cada línea de texto `line_height` cuerpos más abajo. Las letras que no existen
/// ocupan un espacio.
pub fn text_strokes(text: &str, size_mm: f64, line_height: f64) -> Vec<(Vec<Point>, bool)> {
    let f = font();
    let k = size_mm / UNITS_PER_EM;
    let mut out = Vec::new();
    for (row, line) in text.split('\n').enumerate() {
        let base = ASCENT * k + row as f64 * line_height * size_mm;
        let mut x = 0.0;
        for c in line.chars() {
            match f.glyphs.get(&c) {
                Some(g) => {
                    for s in &g.strokes {
                        out.push((
                            s.iter()
                                .map(|p| Point::new(x + p.x * k, base - p.y * k))
                                .collect(),
                            false,
                        ));
                    }
                    x += g.advance * k;
                }
                None => x += f.default_advance * k,
            }
        }
    }
    out
}

/// Letras de una línea de texto para colocarlas sobre un trazado (`textpath::place_on_path`):
/// base en y = 0, lo de arriba con y negativa, cada letra ya en su posición x de la línea. Las
/// letras que no existen ocupan un espacio sin trazos.
pub fn path_glyphs(text: &str, size_mm: f64) -> Vec<crate::textpath::Glyph> {
    let f = font();
    let k = size_mm / UNITS_PER_EM;
    let mut x = 0.0;
    text.chars()
        .map(|c| {
            let (advance, outline) = match f.glyphs.get(&c) {
                Some(g) => (
                    g.advance * k,
                    g.strokes
                        .iter()
                        .map(|s| {
                            let pts = s.iter().map(|p| Point::new(x + p.x * k, -p.y * k));
                            (pts.collect(), false)
                        })
                        .collect(),
                ),
                None => (f.default_advance * k, Vec::new()),
            };
            let g = crate::textpath::Glyph {
                x_mm: x,
                advance_mm: advance,
                outline,
            };
            x += advance;
            g
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_glyphs_match_the_strokes_on_the_baseline() {
        // Mismos trazos que `text_strokes`, solo desplazados para que la base quede en y = 0.
        let flat = text_strokes("AB", 10.0, 1.2);
        let glyphs = path_glyphs("AB", 10.0);
        assert_eq!(glyphs.len(), 2);
        let placed: Vec<&(Vec<Point>, bool)> = glyphs.iter().flat_map(|g| &g.outline).collect();
        assert_eq!(placed.len(), flat.len());
        for ((a, ca), (b, cb)) in placed.iter().map(|p| (&p.0, p.1)).zip(&flat) {
            assert_eq!(ca, *cb);
            for (p, q) in a.iter().zip(b) {
                assert!((p.x - q.x).abs() < 1e-9 && (p.y - (q.y - 8.0)).abs() < 1e-9);
            }
        }
        // La segunda letra empieza donde acaba la primera.
        assert!((glyphs[1].x_mm - glyphs[0].advance_mm).abs() < 1e-9);
        // Una letra que no existe deja su hueco, sin trazos.
        let gap = path_glyphs("A\u{1F600}A", 10.0);
        assert!(gap[1].outline.is_empty() && gap[1].advance_mm > 0.0);
    }

    #[test]
    fn font_has_ascii_and_spanish_letters() {
        for c in "AZaz09¿¡ñÑáéíóú".chars() {
            assert!(has_glyph(c), "{c}");
        }
        // El texto del reconocimiento va con los datos.
        assert!(SVG.contains("Dr.") && SVG.contains("Hershey"));
    }

    #[test]
    fn letters_are_open_strokes_in_place() {
        // «A» de 10 mm: tres trazos abiertos, apoyados en la base (y = 8 mm).
        let a = text_strokes("A", 10.0, 1.2);
        assert_eq!(a.len(), 3);
        assert!(a.iter().all(|(_, closed)| !closed));
        let ys: Vec<f64> = a.iter().flat_map(|(p, _)| p.iter().map(|q| q.y)).collect();
        let max = ys.iter().cloned().fold(f64::MIN, f64::max);
        assert!((max - 8.0).abs() < 1e-9, "base en 8 mm: {max}");
        // Dos líneas: la segunda 1,2 cuerpos más abajo.
        let two = text_strokes("A\nA", 10.0, 1.2);
        let ymax = two
            .iter()
            .flat_map(|(p, _)| p.iter().map(|q| q.y))
            .fold(f64::MIN, f64::max);
        assert!((ymax - 20.0).abs() < 1e-9);
        // «AA» avanza una letra.
        let aa = text_strokes("AA", 10.0, 1.2);
        let x0 = aa[0].0[0].x;
        let x1 = aa[3].0[0].x;
        assert!((x1 - x0 - 5.67).abs() < 1e-9);
    }
}
