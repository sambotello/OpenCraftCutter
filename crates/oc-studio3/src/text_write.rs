//! Textos editables para `.studio3`.
//!
//! Molde: un texto «Abc» guardado por la referencia (`assets/text_mold.bin`). Es un contenedor
//! (tipo 07: estilo, alineación y posición de la línea) con una lista de letras: una vacía al
//! principio y una por carácter. Cada letra lleva la fuente, su código, una matriz con el em y
//! la posición del lápiz, su caja (avance × ascendente+descendente) y su contorno en mm.

use crate::write::{cache_record, f32s, object_id, rec, set_style, splice_fix};
use kurbo::{Affine, Point};
use oc_core::Color;
use oc_core::nodes::NodePath;

const MOLD: &[u8] = include_bytes!("../assets/text_mold.bin");
/// Partes del molde (desplazamientos dentro de `MOLD`).
const HEAD: (usize, usize) = (0x10, 0x1db);
/// Lista de letras (solo se comprueba en las pruebas).
#[cfg(test)]
const LIST: usize = 0x1db;
const NULL_GLYPH: (usize, usize) = (0x1e4, 0x478);
const GLYPH: (usize, usize) = (0x478, 0x7c0);
const TAIL77: (usize, usize) = (0x10a3, 0x10f0);
const ITEM_TAIL: (usize, usize) = (0x10f0, 0x11a1);
const FONT: &[u8] = b"\x05\x00\x00\x00Arial";
/// Posición de la línea dentro de la cabecera (tras el estilo) y del final del texto.
const HEAD_XY: usize = 0x1cb;

/// Letra lista para escribir: código, posición del lápiz (mm, mundo sin girar), avance y
/// contorno en mm (mundo sin girar).
pub struct TextGlyph {
    pub ch: char,
    pub x_mm: f64,
    pub advance_mm: f64,
    pub outline: Vec<NodePath>,
}

/// Texto de una línea con sus letras ya colocadas.
pub struct TextRun {
    pub family: String,
    pub size_pt: f64,
    /// Inicio del lápiz sobre la línea base (mm, mundo sin girar).
    pub origin: Point,
    pub ascent_mm: f64,
    pub descent_mm: f64,
    pub glyphs: Vec<TextGlyph>,
}

/// Colores de línea y relleno (con opacidad) de un texto.
type Style = (Color, Option<(Color, f64)>);

fn glyph(
    mold: &[u8],
    run: &TextRun,
    gl: &TextGlyph,
    style: Style,
    serial: u64,
    seed: u64,
) -> Vec<u8> {
    let (ch, x, adv, outline) = (gl.ch, gl.x_mm, gl.advance_mm, &gl.outline);
    let (line, fill) = style;
    let mut g = mold.to_vec();
    let em = oc_core::units::pt_to_mm(run.size_pt);
    let y = run.origin.y;
    // Cola: código y identificador (tamaño fijo, al final).
    let tail = g.len() - 69;
    g[tail + 4..tail + 8].copy_from_slice(&(ch as u32).to_le_bytes());
    g[tail + 13..tail + 45].copy_from_slice(&object_id(seed));
    // Propiedades: registro en 11; el código va en sus 4 últimos bytes.
    let props_end = |g: &[u8]| 11 + 1 + u32::from_le_bytes(g[12..16].try_into().unwrap()) as usize;
    let pe = props_end(&g);
    g[pe - 4..pe].copy_from_slice(&(ch as u32).to_le_bytes());
    // Matriz (em y lápiz) y caja de la letra justo tras las propiedades.
    let mut b = Vec::new();
    let h = run.ascent_mm + run.descent_mm;
    f32s(&mut b, &[em, 0.0, x, 0.0, em, y, 0.0, 0.0, 1.0]);
    f32s(
        &mut b,
        &[
            x + adv / 2.0,
            y - run.ascent_mm + h / 2.0,
            adv / 2.0,
            0.0,
            0.0,
            h / 2.0,
        ],
    );
    g[pe..pe + 60].copy_from_slice(&b);
    // Contorno (registro tras la matriz y la caja + 5 bytes de banderas) y número de serie.
    let o = pe + 65;
    let o_end = o + 1 + u32::from_le_bytes(g[o + 1..o + 5].try_into().unwrap()) as usize;
    g.splice(
        o..o_end,
        if outline.is_empty() {
            vec![1, 8, 0, 0, 0, 0xff, 0xff, 0xff, 0xff]
        } else {
            cache_record(outline, Affine::IDENTITY)
        },
    );
    let s = o + if outline.is_empty() {
        9
    } else {
        cache_record(outline, Affine::IDENTITY).len()
    };
    g[s + 1..s + 9].copy_from_slice(&serial.to_le_bytes());
    // Los cambios de tamaño variable, al final: ajustan las longitudes que los contienen.
    font(&mut g, run);
    set_style(&mut g, line, fill);
    fix_outer(&mut g);
    g
}

/// Fuente y tamaño: `<u32 n><nombre> 00×7 <em f32> <pt f32>`.
fn font(g: &mut Vec<u8>, run: &TextRun) {
    let Some(f) = g.windows(FONT.len()).position(|w| w == FONT) else {
        return;
    };
    let mut new = (run.family.len() as u32).to_le_bytes().to_vec();
    new.extend_from_slice(run.family.as_bytes());
    new.extend_from_slice(&[0; 7]);
    f32s(
        &mut new,
        &[oc_core::units::pt_to_mm(run.size_pt), run.size_pt],
    );
    splice_fix(g, f, FONT.len() + 7 + 8, &new);
}

/// Recalcula la longitud del registro exterior de la letra (`01 L 02 L2 …`).
fn fix_outer(g: &mut [u8]) {
    let n = g.len() as u32;
    g[1..5].copy_from_slice(&(n - 1).to_le_bytes());
    g[6..10].copy_from_slice(&(n - 6).to_le_bytes());
}

/// Elemento de la lista de objetos para un texto (con su envoltorio).
pub fn text_item(
    run: &TextRun,
    rotation_deg: f64,
    line: Color,
    fill: Option<(Color, f64)>,
    name: &str,
    seed: u64,
) -> Option<(Vec<u8>, kurbo::Rect)> {
    let last = run.glyphs.last()?;
    let width = last.x_mm + last.advance_mm - run.origin.x;
    let (x0, y) = (run.origin.x, run.origin.y);
    // Cabecera: estilo y posición de la línea.
    let mut head = MOLD[HEAD.0..HEAD.1].to_vec();
    let mut xy = Vec::new();
    f32s(&mut xy, &[x0, y]);
    let k = head.len() - (HEAD.1 - HEAD_XY);
    head[k..k + 8].copy_from_slice(&xy);
    set_style(&mut head, line, fill);
    let hl = head.len() as u32 - 1;
    head[1..5].copy_from_slice(&hl.to_le_bytes());
    // Lista de letras: la vacía y una por carácter.
    let mut list = (run.glyphs.len() as u32).to_le_bytes().to_vec();
    let mut serial = 0x0002_2869u64;
    let empty = TextGlyph {
        ch: '\0',
        x_mm: x0,
        advance_mm: 0.0,
        outline: Vec::new(),
    };
    list.extend(glyph(
        &MOLD[NULL_GLYPH.0..NULL_GLYPH.1],
        run,
        &empty,
        (line, fill),
        serial,
        seed,
    ));
    for (i, g) in run.glyphs.iter().enumerate() {
        serial += 1;
        list.extend(glyph(
            &MOLD[GLYPH.0..GLYPH.1],
            run,
            g,
            (line, fill),
            serial,
            seed.wrapping_mul(31).wrapping_add(i as u64 + 1),
        ));
    }
    let mut t77 = MOLD[TAIL77.0..TAIL77.1].to_vec();
    t77[0..8].copy_from_slice(&xy);
    let n = (run.glyphs.len() as u32).to_le_bytes();
    t77[8..12].copy_from_slice(&n);
    t77[12..16].copy_from_slice(&n);
    let mut obj = head;
    obj.extend(rec(&list));
    obj.extend(t77);
    let obj = rec(&obj);
    // Cola del elemento: matriz (giro sobre el centro de la caja), caja y el resto del molde.
    let r = kurbo::Rect::new(x0, y - run.ascent_mm, x0 + width, y + run.descent_mm);
    let c = r.center();
    let m = Affine::translate(c.to_vec2())
        * Affine::rotate(rotation_deg.to_radians())
        * Affine::translate(-c.to_vec2());
    let mc = m.as_coeffs();
    let mut tail = Vec::new();
    f32s(
        &mut tail,
        &[mc[0], mc[2], mc[4], mc[1], mc[3], mc[5], 0.0, 0.0, 1.0],
    );
    let ax = m * Point::new(r.x1, c.y) - c;
    let ay = m * Point::new(c.x, r.y1) - c;
    f32s(&mut tail, &[c.x, c.y, ax.x, ax.y, ay.x, ay.y]);
    let mold_tail = &MOLD[ITEM_TAIL.0..ITEM_TAIL.1];
    tail.extend_from_slice(&mold_tail[60..]);
    let tail = crate::write::named(&tail, name, "Texto", seed);
    let mut item = vec![0u8];
    item.extend(obj);
    item.extend(tail);
    let pts = [
        r.origin(),
        Point::new(r.x1, r.y0),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y1),
    ]
    .map(|p| m * p);
    let bounds = pts
        .iter()
        .skip(1)
        .fold(kurbo::Rect::from_points(pts[0], pts[0]), |b, p| {
            b.union_pt(*p)
        });
    Some((rec(&rec(&item)), bounds))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, w: f64) -> NodePath {
        NodePath::from_points(
            &[
                Point::new(x, y - w),
                Point::new(x + w, y - w),
                Point::new(x + w, y),
                Point::new(x, y),
            ],
            true,
        )
    }

    pub(crate) fn run(text: &str) -> TextRun {
        let origin = Point::new(30.0, 60.0);
        TextRun {
            family: "Helvetica Neue".into(),
            size_pt: 36.0,
            origin,
            ascent_mm: 11.0,
            descent_mm: 3.0,
            glyphs: text
                .chars()
                .enumerate()
                .map(|(i, ch)| TextGlyph {
                    ch,
                    x_mm: origin.x + 9.0 * i as f64,
                    advance_mm: 9.0,
                    outline: vec![square(origin.x + 9.0 * i as f64 + 1.0, origin.y, 7.0)],
                })
                .collect(),
        }
    }

    #[test]
    fn text_round_trip() {
        let mut doc = oc_core::Document::default();
        let mut s = oc_core::Shape::new(
            0,
            "Hola",
            kurbo::Rect::new(0.0, 0.0, 1.0, 1.0),
            oc_core::ShapeKind::Text {
                text: "Hola".into(),
                font_family: "Helvetica Neue".into(),
                font_size_pt: 36.0,
                align: oc_core::TextAlign::Left,
                bold: false,
                italic: false,
                line_height: 1.2,
                fx: Default::default(),
            },
        );
        s.fill = Some(Color::rgb(200, 30, 60));
        let id = doc.add_shape(s);
        let mut runs = crate::write::TextRuns::new();
        runs.insert(id, run("Hola"));
        let bytes = crate::write::write_with_texts(&doc, &runs);
        let sizes: u64 = (0..14)
            .map(|i| u64::from_le_bytes(bytes[16 + 8 * i..24 + 8 * i].try_into().unwrap()))
            .sum();
        assert_eq!(sizes as usize + 0x400, bytes.len());
        let back = crate::parse(&bytes).expect("se lee");
        assert_eq!(
            back.texts.len(),
            1,
            "{:?}",
            back.texts.iter().map(|t| &t.text).collect::<Vec<_>>()
        );
        let t = &back.texts[0];
        assert_eq!(t.text, "Hola");
        assert_eq!(t.font_family, "Helvetica Neue");
        assert!((t.font_size_pt - 36.0).abs() < 1e-3);
        assert!(
            (t.bounds.x0 - 31.0).abs() < 1e-2 && (t.bounds.y1 - 60.0).abs() < 1e-2,
            "{:?}",
            t.bounds
        );
        assert_eq!(t.fill.map(|c| (c.r, c.g, c.b)), Some((200, 30, 60)));
        if let Ok(out) = std::env::var("OPENCRAFTCUTTER_STUDIO3_TEXT") {
            std::fs::write(out, &bytes).unwrap();
        }
    }

    #[test]
    fn mold_layout() {
        assert_eq!(MOLD[0], 1);
        assert_eq!(MOLD[HEAD.0], 7);
        assert_eq!(MOLD[LIST], 1);
        assert_eq!(&MOLD[LIST + 5..LIST + 9], &3u32.to_le_bytes());
        for (a, b) in [NULL_GLYPH, GLYPH] {
            assert_eq!(MOLD[a], 1);
            assert_eq!(
                a + 1 + u32::from_le_bytes(MOLD[a + 1..a + 5].try_into().unwrap()) as usize,
                b
            );
            assert!(MOLD[a..b].windows(FONT.len()).any(|w| w == FONT));
        }
        let x = f32::from_le_bytes(MOLD[HEAD_XY..HEAD_XY + 4].try_into().unwrap());
        assert!((x - 120.999).abs() < 1e-3, "{x}");
        assert!(
            MOLD[ITEM_TAIL.0..ITEM_TAIL.1]
                .windows(5)
                .any(|w| w == b"Texto")
        );
    }
}
