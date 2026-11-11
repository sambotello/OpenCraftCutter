//! Lectura de documentos `.studio3` de Silhouette Studio (solo lectura).
//!
//! El formato no está documentado; esta lectura se basa en comparar archivos guardados por la
//! propia aplicación. Lo que se reconoce:
//!
//! - Firma `silhouette05;` al principio.
//! - Registros `01 <u32 L> <L−4 bytes>` (la longitud cuenta sus propios 4 bytes).
//! - Un trazado es una cadena de subtrazados seguidos. Cada subtrazado es un registro que
//!   empieza con `01 <u32 n>` (número de nodos − 1) y contiene `n + 1` registros de nodo.
//! - Un nodo es `x, y` (f32, mm) y, opcionalmente, sus manejadores de entrada y salida
//!   (`01 dx dy`, relativos al ancla), seguidos de 3 bytes de estado.
//! - Tras la cadena va la matriz afín 3×3 (f32, por filas) que lleva los nodos al tapete.
//! - El estilo (color de línea y relleno) está en el registro de estilo previo al trazado:
//!   RGBA de la línea, nombres de unión y remate, y un indicador de relleno con su RGBA.
//!
//! - Una imagen es una forma cuyo relleno es `05` + identificador (32 hex) de su PNG, seguido
//!   de una matriz de textura que lleva los píxeles al espacio local de la forma (a veces
//!   [−1, 1]², a veces los propios píxeles). Tapete = matriz de la forma × matriz de textura.
//!   El contorno de la forma recorta la imagen. Los PNG van al final, tras su identificador.
//!
//! - Un texto es un objeto con un hijo por carácter. Cada carácter guarda la fuente
//!   (`<u32 n><nombre> + 7 bytes + em (mm) + tamaño (pt)`), su código (`u32`, seguido del em)
//!   y su contorno ya colocado en el tapete: `01 L <u32> [<u32 L2> 01 <u32 n−1> nodos]…`, con
//!   nodos `x, y` y manejadores opcionales (`01 dx dy`). El objeto padre va después de sus
//!   caracteres y lleva `create_text`.

pub mod text_write;
pub mod write;

use kurbo::{Affine, Point, Vec2};
use oc_core::Color;
use oc_core::nodes::{Node, NodePath};

#[derive(Debug, thiserror::Error)]
pub enum Studio3Error {
    #[error("no es un documento de Silhouette Studio (.studio3)")]
    NotStudio3,
    #[error("el documento no contiene formas que se puedan importar")]
    Empty,
}

/// Texto editable: contenido, fuente y caja en el tapete (mm) que ocupan sus letras.
#[derive(Debug, Clone, PartialEq)]
pub struct Studio3Text {
    pub text: String,
    pub font_family: String,
    pub font_size_pt: f64,
    /// Caja sin girar (mm, tapete) y giro en grados sobre su centro.
    pub bounds: kurbo::Rect,
    pub rotation_deg: f64,
    /// Grupos que lo contienen, del más externo al más interno.
    pub groups: Vec<usize>,
    /// Posición del primer carácter y del objeto texto en el archivo.
    at: usize,
    end: usize,
    /// Anclas de las letras en el tapete (para recalcular la caja sin girar).
    points: Vec<Point>,
    pub line: Option<Color>,
    pub fill: Option<Color>,
}

/// Una forma del documento: trazados ya colocados en el tapete (mm) y su estilo.
#[derive(Debug, Clone, PartialEq)]
pub struct Studio3Shape {
    pub paths: Vec<NodePath>,
    pub line: Option<Color>,
    pub fill: Option<Color>,
    /// Si es una imagen: el PNG y dónde va.
    pub image: Option<Studio3Image>,
    /// Grupos del documento que la contienen (índices), del más externo al más interno.
    pub groups: Vec<usize>,
}

/// Imagen colocada en el tapete.
#[derive(Debug, Clone, PartialEq)]
pub struct Studio3Image {
    pub png: std::sync::Arc<Vec<u8>>,
    pub px_width: u32,
    pub px_height: u32,
    /// Centro, ancho y alto (mm) y giro (grados) en el tapete.
    pub center: Point,
    pub width_mm: f64,
    pub height_mm: f64,
    pub rotation_deg: f64,
    /// La matriz invierte la imagen (espejo); se coloca sin invertir.
    pub mirrored: bool,
}

/// Recorta un PNG con un contorno en píxeles de la imagen (par-impar): lo de fuera
/// queda transparente. Devuelve el PNG original si no hace falta o si falla.
fn clip_png(png: &[u8], w: u32, h: u32, outline: &[Vec<Point>]) -> Option<Vec<u8>> {
    // Contorno = marco completo de la imagen: no hay nada que recortar.
    let (fw, fh) = (w as f64, h as f64);
    let full = outline.len() == 1
        && outline[0].len() <= 5
        && outline[0].iter().all(|p| {
            (p.x.abs() < 0.5 || (p.x - fw).abs() < 0.5)
                && (p.y.abs() < 0.5 || (p.y - fh).abs() < 0.5)
        });
    if full || outline.is_empty() {
        return None;
    }
    let mut img = image::load_from_memory(png).ok()?.to_rgba8();
    let (iw, ih) = img.dimensions();
    if (iw, ih) != (w, h) {
        return None;
    }
    let polys: Vec<Vec<(f64, f64)>> = outline
        .iter()
        .map(|c| c.iter().map(|p| (p.x, p.y)).collect())
        .collect();
    let mut xs = Vec::new();
    for y in 0..h {
        let fy = y as f64 + 0.5;
        xs.clear();
        for poly in &polys {
            let n = poly.len();
            for i in 0..n {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                if (a.1 <= fy) != (b.1 <= fy) {
                    xs.push(a.0 + (fy - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        let mut inside = vec![false; w as usize];
        for pair in xs.chunks(2) {
            if let [x0, x1] = pair {
                let a = x0.round().clamp(0.0, w as f64) as usize;
                let b = x1.round().clamp(0.0, w as f64) as usize;
                inside[a..b].iter_mut().for_each(|v| *v = true);
            }
        }
        for (x, ins) in inside.iter().enumerate() {
            if !ins {
                img.get_pixel_mut(x as u32, y).0[3] = 0;
            }
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Studio3Doc {
    pub shapes: Vec<Studio3Shape>,
    /// Imágenes incrustadas (PNG) que no se importan.
    pub images_skipped: usize,
    /// Textos editables que no se importan.
    pub texts_skipped: usize,
    /// Tamaño de los medios del documento (`letter`, `a4`…), si lo fija.
    pub media: Option<String>,
    /// Marcas de registro de Imprimir y cortar, si están activadas.
    pub reg_marks: Option<RegMarksInfo>,
    pub texts: Vec<Studio3Text>,
}

/// Marcas de registro del documento (mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegMarksInfo {
    /// Márgenes izquierdo, superior, derecho e inferior.
    pub insets: [f64; 4],
    pub length: f64,
    pub thickness: f64,
}

const SIGNATURE: &[u8] = b"silhouette";
/// Longitudes de registro de nodo: sin manejadores, con uno o con los dos.
const NODE_LENS: [u32; 3] = [0x11, 0x19, 0x21];

fn u32_at(d: &[u8], p: usize) -> Option<u32> {
    d.get(p..p + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn f32_at(d: &[u8], p: usize) -> Option<f64> {
    d.get(p..p + 4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64)
}

fn point_at(d: &[u8], p: usize) -> Option<Point> {
    Some(Point::new(f32_at(d, p)?, f32_at(d, p + 4)?))
}

fn is_node(d: &[u8], p: usize) -> bool {
    d.get(p) == Some(&1) && u32_at(d, p + 1).is_some_and(|l| NODE_LENS.contains(&l))
}

/// Nodos locales de un subtrazado en `s`: (fin del registro, nodos), o `None`.
/// Nodo local: ancla y manejadores de entrada y salida (relativos al ancla).
type LocalNode = (Point, Option<Vec2>, Option<Vec2>);

fn subpath_at(d: &[u8], s: usize) -> Option<(usize, Vec<LocalNode>)> {
    if d.get(s) != Some(&1) || d.get(s + 5) != Some(&1) || !is_node(d, s + 10) {
        return None;
    }
    let end = s + u32_at(d, s + 1)? as usize + 1;
    let count = u32_at(d, s + 6)? as usize + 1;
    if end > d.len() || count > 1_000_000 {
        return None;
    }
    let mut r = s + 10;
    let mut nodes = Vec::with_capacity(count.min(4096));
    while r < end && is_node(d, r) {
        let len = u32_at(d, r + 1)? as usize;
        let pay = r + 5;
        let p = point_at(d, pay)?;
        let mut k = pay + 8;
        let handle = |k: &mut usize| -> Option<Option<Vec2>> {
            if *d.get(*k)? == 1 {
                let v = point_at(d, *k + 1)?.to_vec2();
                *k += 9;
                Some(Some(v))
            } else {
                *k += 1;
                Some(None)
            }
        };
        let h_in = handle(&mut k)?;
        let h_out = handle(&mut k)?;
        nodes.push((p, h_in, h_out));
        r += len + 1;
    }
    (r == end && nodes.len() == count).then_some((end, nodes))
}

/// Contorno local aplanado (los manejadores se ignoran: basta para recortar).
fn local_outline(subs: &[Vec<LocalNode>]) -> Vec<Vec<Point>> {
    subs.iter()
        .map(|nodes| {
            let n = nodes.len();
            let mut pts = Vec::new();
            for i in 0..n {
                let (p0, _, out0) = nodes[i];
                let (p1, in1, _) = nodes[(i + 1) % n];
                pts.push(p0);
                if out0.is_some() || in1.is_some() {
                    let c1 = p0 + out0.unwrap_or_default();
                    let c2 = p1 + in1.unwrap_or_default();
                    let cb = kurbo::CubicBez::new(p0, c1, c2, p1);
                    for k in 1..12 {
                        pts.push(kurbo::ParamCurve::eval(&cb, k as f64 / 12.0));
                    }
                }
            }
            pts
        })
        .collect()
}

/// Matriz afín guardada tras el trazado (identidad si lo que hay no lo es).
fn matrix_at(d: &[u8], p: usize) -> Affine {
    let m: Option<Vec<f64>> = (0..9).map(|i| f32_at(d, p + 4 * i)).collect();
    match m {
        Some(m)
            if m.iter().all(|v| v.is_finite())
                && m[6].abs() < 1e-3
                && m[7].abs() < 1e-3
                && (m[8] - 1.0).abs() < 1e-3 =>
        {
            Affine::new([m[0], m[3], m[1], m[4], m[2], m[5]])
        }
        _ => Affine::IDENTITY,
    }
}

fn rgba_at(d: &[u8], p: usize) -> Option<Color> {
    let c: Option<Vec<f64>> = (0..4).map(|i| f32_at(d, p + 4 * i)).collect();
    let c = c?;
    if c.iter().any(|v| !(0.0..=1.0001).contains(v)) {
        return None;
    }
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(Color::rgba(b(c[0]), b(c[1]), b(c[2]), b(c[3])))
}

/// Cadena de longitud prefijada `<u32 n><n letras minúsculas>`; devuelve su fin.
fn word_at(d: &[u8], p: usize) -> Option<usize> {
    let n = u32_at(d, p)? as usize;
    if !(3..=12).contains(&n) {
        return None;
    }
    let w = d.get(p + 4..p + 4 + n)?;
    w.iter().all(u8::is_ascii_lowercase).then_some(p + 4 + n)
}

/// Registro `cut` del estilo; el color de línea va justo antes: `01` + RGBA + 8 bytes.
pub(crate) const CUT_RECORD: &[u8] = b"\x01\x29\x00\x00\x00\x03\x00\x00\x00cut";

/// Color de línea del estilo cuya unión empieza en `join` (el RGBA que precede a la unión
/// pertenece al registro `cut`, no a la línea).
fn line_color_before(d: &[u8], join: usize) -> Option<Color> {
    let lo = join.saturating_sub(200);
    let k = d[lo..join]
        .windows(CUT_RECORD.len())
        .rposition(|w| w == CUT_RECORD)
        .map(|i| lo + i)?;
    let q = k.checked_sub(24)?;
    (d.get(q - 1) == Some(&1)).then(|| rgba_at(d, q)).flatten()
}

/// Estilo de la forma que empieza en `at`: busca hacia atrás «unión, 01, remate» y lee el
/// RGBA de la línea (justo antes) y el relleno (justo después).
fn style_before(d: &[u8], at: usize) -> (Option<Color>, Option<Color>, Option<(String, Affine)>) {
    let lo = at.saturating_sub(4000);
    let mut found = None;
    for p in lo..at {
        if let Some(e1) = word_at(d, p) {
            if d.get(e1) == Some(&1) {
                if let Some(e2) = word_at(d, e1 + 1) {
                    if e2 <= at {
                        found = Some((p, e2));
                    }
                }
            }
        }
    }
    let Some((a, b)) = found else {
        return (None, None, None);
    };
    let line = line_color_before(d, a);
    let fill = (d.get(b) == Some(&1)).then(|| rgba_at(d, b + 1)).flatten();
    let image = (d.get(b) == Some(&5))
        .then(|| key_at(d, b + 1, 4))
        .flatten()
        .map(|k| {
            let after = b + 5 + k.len();
            (k, texture_matrix(d, after))
        });
    (line, fill, image)
}

/// Matriz de textura tras el identificador de la imagen: lleva los píxeles del PNG al espacio
/// local de la forma. Es la segunda matriz `01 + 3×3` que sigue al identificador.
fn texture_matrix(d: &[u8], from: usize) -> Affine {
    let mut seen = 0;
    for k in from..(from + 200).min(d.len()) {
        if d[k] != 1 {
            continue;
        }
        let m = matrix_at(d, k + 1);
        let looks_affine = (0..9).all(|i| f32_at(d, k + 1 + 4 * i).is_some_and(f64::is_finite))
            && f32_at(d, k + 1 + 32) == Some(1.0);
        if looks_affine && m.determinant().abs() > 1e-12 {
            seen += 1;
            if seen == 2 {
                return m;
            }
        }
    }
    Affine::IDENTITY
}

/// Identificador de imagen `<u32 n><n caracteres imprimibles>` (un hash o un nombre).
fn key_at(d: &[u8], p: usize, skip: usize) -> Option<String> {
    let n = u32_at(d, p)? as usize;
    if !(1..=160).contains(&n) {
        return None;
    }
    let k = d.get(p + skip..p + skip + n)?;
    k.iter()
        .all(|c| (0x21..0x7f).contains(c))
        .then(|| String::from_utf8_lossy(k).into_owned())
}

const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";

type ImageTable = std::collections::HashMap<String, (std::sync::Arc<Vec<u8>>, u32, u32)>;

/// PNG incrustados por identificador (el identificador va poco antes de cada PNG).
fn image_table(d: &[u8]) -> ImageTable {
    let mut t = ImageTable::new();
    let mut p = 0;
    while let Some(i) = find(d, PNG_SIG, p) {
        let Some(e) = find(d, b"IEND", i).map(|e| (e + 8).min(d.len())) else {
            break;
        };
        p = e;
        let be = |q: usize| {
            d.get(q..q + 4)
                .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        };
        let (Some(w), Some(h)) = (be(i + 16), be(i + 20)) else {
            continue;
        };
        // Antes del PNG van la clave y el nombre de archivo, con longitudes de 64 bits.
        let lo = i.saturating_sub(200);
        let mut strings: Vec<(usize, String)> = Vec::new();
        let mut q = lo;
        while q < i {
            match (u32_at(d, q + 4) == Some(0))
                .then(|| key_at(d, q, 8))
                .flatten()
            {
                Some(k) => {
                    let n = k.len();
                    strings.push((q, k));
                    q += 8 + n;
                }
                None => q += 1,
            }
        }
        // La clave va primero; el nombre de archivo, si lo hay, después.
        if let Some((_, key)) = strings.first() {
            let key = key.clone();
            t.entry(key)
                .or_insert_with(|| (std::sync::Arc::new(d[i..e].to_vec()), w, h));
        }
    }
    t
}

fn find(d: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    d.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

/// Coloca una imagen: `a` lleva sus píxeles (0..w, 0..h) al tapete.
fn place_image(a: Affine, png: std::sync::Arc<Vec<u8>>, w: u32, h: u32) -> Studio3Image {
    let c = a.as_coeffs();
    let ux = Vec2::new(c[0], c[1]) * w as f64;
    let uy = Vec2::new(c[2], c[3]) * h as f64;
    Studio3Image {
        png,
        px_width: w,
        px_height: h,
        center: a * Point::new(w as f64 / 2.0, h as f64 / 2.0),
        width_mm: ux.hypot(),
        height_mm: uy.hypot(),
        rotation_deg: ux.y.atan2(ux.x).to_degrees(),
        mirrored: ux.cross(uy) < 0.0,
    }
}

fn count(d: &[u8], needle: &[u8]) -> usize {
    d.windows(needle.len()).filter(|w| *w == needle).count()
}

/// Grupos de una forma: el exterior en `group` y el resto, en orden, en `inner_groups`
/// (identificadores = índice + 1; el importador los cambia por los del documento).
fn set_groups(s: &mut oc_core::Shape, groups: &[usize]) {
    let mut ids = groups.iter().map(|&g| g as u64 + 1);
    s.group = ids.next();
    s.inner_groups = ids.collect();
}

impl Studio3Doc {
    /// Formas de OpenCraftCutter (trazados en mm, con su línea y relleno), en el orden de apilado
    /// del documento. `group` lleva el índice del grupo + 1; quien las añade al documento debe
    /// sustituirlo por identificadores propios.
    pub fn to_shapes(&self, default_stroke: oc_core::Stroke) -> Vec<oc_core::Shape> {
        let mut out = self.shapes_in_file_order(default_stroke);
        out.extend(self.texts.iter().map(|t| {
            let mut s = oc_core::Shape::new(
                0,
                t.text.lines().next().unwrap_or("Texto").to_string(),
                t.bounds,
                oc_core::ShapeKind::Text {
                    text: t.text.clone(),
                    font_family: t.font_family.clone(),
                    font_size_pt: t.font_size_pt,
                    align: oc_core::TextAlign::Left,
                    bold: false,
                    italic: false,
                    line_height: 1.0,
                    fx: Default::default(),
                },
            );
            s.rotation = t.rotation_deg;
            set_groups(&mut s, &t.groups);
            s.stroke = default_stroke;
            if let Some(c) = t.line.filter(|c| c.a > 0) {
                s.stroke.color = Color::rgb(c.r, c.g, c.b);
            }
            s.fill = t
                .fill
                .filter(|c| c.a > 0)
                .map(|c| Color::rgb(c.r, c.g, c.b));
            s
        }));
        out
    }

    fn shapes_in_file_order(&self, default_stroke: oc_core::Stroke) -> Vec<oc_core::Shape> {
        use oc_core::shape::{FillRule, ShapeKind};
        self.shapes
            .iter()
            .enumerate()
            .flat_map(|(i, f)| {
                let mut out = Vec::new();
                if let Some(img) = &f.image {
                    let half = Vec2::new(img.width_mm / 2.0, img.height_mm / 2.0);
                    let mut s = oc_core::Shape::new(
                        0,
                        format!("Imagen {}", i + 1),
                        kurbo::Rect::from_points(img.center - half, img.center + half),
                        ShapeKind::Image {
                            png: oc_core::shape::ImageData(img.png.clone()),
                            px_width: img.px_width,
                            px_height: img.px_height,
                        },
                    );
                    s.rotation = img.rotation_deg;
                    set_groups(&mut s, &f.groups);
                    out.push(s);
                }
                let mut s = oc_core::Shape::new(
                    0,
                    format!("Forma {}", i + 1),
                    kurbo::Rect::ZERO,
                    ShapeKind::Path {
                        subpaths: Vec::new(),
                        fill_rule: FillRule::EvenOdd,
                    },
                );
                s.set_world_node_paths(&f.paths);
                set_groups(&mut s, &f.groups);
                s.stroke = default_stroke;
                if let Some(c) = f.line {
                    s.stroke.color = Color::rgb(c.r, c.g, c.b);
                }
                s.fill = f
                    .fill
                    .filter(|c| c.a > 0)
                    .map(|c| Color::rgb(c.r, c.g, c.b));
                // El alfa del relleno es su opacidad (la referencia guarda además 1 - alfa).
                if let Some(c) = f.fill.filter(|c| c.a > 0) {
                    s.fill_opacity = c.a as f64 / 255.0;
                }
                // En una imagen, el contorno solo la recorta: forma parte del objeto imagen, como
                // en la referencia (si se borra la imagen, desaparece con ella).
                if f.image.is_none() {
                    out.push(s);
                }
                out
            })
            .collect()
    }

    /// Avisos para el usuario sobre lo que no se importó.
    pub fn warnings(&self) -> Vec<String> {
        let mut w = Vec::new();
        if self.texts_skipped > 0 {
            w.push(format!(
                "{} texto(s) no se pudieron leer.",
                self.texts_skipped
            ));
        }
        if self.images_skipped > 0 {
            w.push(format!(
                "{} imagen(es) no se encontraron en el archivo.",
                self.images_skipped
            ));
        }
        let mirrored = self
            .shapes
            .iter()
            .filter(|s| s.image.as_ref().is_some_and(|i| i.mirrored))
            .count();
        if mirrored > 0 {
            w.push(format!(
                "{mirrored} imagen(es) estaban reflejadas y se colocaron sin reflejar."
            ));
        }
        w
    }
}

/// Carácter de un texto: posición del registro, carácter, fuente, tamaño y caja de su
/// contorno (si tiene; los espacios no).
struct Glyph {
    at: usize,
    ch: char,
    family: String,
    size_pt: f64,
    points: Vec<Point>,
}

/// Registro de fuente `<u32 n><nombre> 00×7 <em f32> <pt f32>`: (fin, nombre, em, pt).
fn font_at(d: &[u8], p: usize) -> Option<(usize, String, f64, f64)> {
    let n = u32_at(d, p)? as usize;
    if !(2..=64).contains(&n) {
        return None;
    }
    let name = d.get(p + 4..p + 4 + n)?;
    if !name.iter().all(|c| (0x20..0x7f).contains(c)) || !name[0].is_ascii_alphabetic() {
        return None;
    }
    let e = p + 4 + n;
    if d.get(e..e + 7)? != [0; 7] {
        return None;
    }
    let em = f32_at(d, e + 7)?;
    let pt = f32_at(d, e + 11)?;
    ((1.0..=500.0).contains(&em) && (1.0..=2000.0).contains(&pt))
        .then(|| (e, String::from_utf8_lossy(name).into_owned(), em, pt))
}

/// Anclas de un bloque de contornos `01 L <u32> [<u32 L2> 01 <u32 n−1> nodos]…` (tapete).
fn outline_points_at(d: &[u8], q: usize) -> Option<Vec<Point>> {
    if d.get(q) != Some(&1) || d.get(q + 13) != Some(&1) {
        return None;
    }
    let end = q + u32_at(d, q + 1)? as usize + 1;
    let mut p = q + 9;
    let mut pts = Vec::new();
    while p + 9 < end {
        let l2 = u32_at(d, p)? as usize;
        if d.get(p + 4) != Some(&1) || l2 < 9 {
            return None;
        }
        let n = u32_at(d, p + 5)? as usize + 1;
        let mut r = p + 9;
        for _ in 0..n {
            let pt = point_at(d, r)?;
            r += 8;
            for _ in 0..2 {
                if *d.get(r)? == 1 {
                    r += 9;
                } else {
                    r += 1;
                }
            }
            pts.push(pt);
        }
        if r != p + l2 {
            return None;
        }
        p += l2;
    }
    (p == end && !pts.is_empty()).then_some(pts)
}

fn glyphs(d: &[u8]) -> Vec<Glyph> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 20 < d.len() {
        let Some((e, family, em, pt)) = font_at(d, p) else {
            p += 1;
            continue;
        };
        p = e;
        // Código del carácter: u32 seguido del mismo em.
        let ch = (e..(e + 200).min(d.len())).find_map(|k| {
            let cp = u32_at(d, k)?;
            let c = char::from_u32(cp).filter(|c| !c.is_control() || *c == '\n')?;
            (f32_at(d, k + 4)? == em).then_some((k, c))
        });
        let Some((k, ch)) = ch else {
            continue;
        };
        let points = (k..(k + 200).min(d.len()))
            .find_map(|q| outline_points_at(d, q))
            .unwrap_or_default();
        out.push(Glyph {
            at: k,
            ch,
            family,
            size_pt: pt,
            points,
        });
    }
    out
}

/// Textos: los caracteres entre un objeto `create_text` y el siguiente forman un texto.
fn texts(d: &[u8]) -> Vec<Studio3Text> {
    let all = glyphs(d);
    let mut out = Vec::new();
    let mut from = 0;
    let mut start = 0;
    while let Some(t) = find(d, b"create_text", from) {
        from = t + 1;
        let gs: Vec<&Glyph> = all.iter().filter(|g| g.at >= start && g.at < t).collect();
        start = t;
        let Some(first) = gs.first() else { continue };
        let points: Vec<Point> = gs.iter().flat_map(|g| g.points.iter().copied()).collect();
        let Some(bounds) = crate_bounds(&points) else {
            continue;
        };
        let text: String = gs.iter().map(|g| g.ch).collect();
        let (line, fill, _) = style_before(d, first.at);
        out.push(Studio3Text {
            text,
            font_family: first.family.clone(),
            font_size_pt: first.size_pt,
            bounds,
            rotation_deg: 0.0,
            groups: Vec::new(),
            at: first.at,
            end: t,
            points,
            line,
            fill,
        });
    }
    out
}

fn crate_bounds(points: &[Point]) -> Option<kurbo::Rect> {
    let mut it = points.iter();
    let first = *it.next()?;
    Some(it.fold(kurbo::Rect::from_points(first, first), |r, p| {
        r.union_pt(*p)
    }))
}

/// Coloca un texto con las matrices de su objeto y de los grupos que lo contienen: el
/// contorno de las letras está en el espacio del texto.
fn place_text(d: &[u8], t: &mut Studio3Text, groups: &[GroupTransform]) {
    let mut containing: Vec<usize> = (0..groups.len())
        .filter(|&i| groups[i].start <= t.at && t.at < groups[i].matrix_at)
        .collect();
    containing.sort_by_key(|&i| groups[i].start);
    let g = containing
        .iter()
        .fold(Affine::IDENTITY, |acc, &i| acc * groups[i].m);
    // Matriz del propio objeto texto: 75 bytes antes de su registro de nombre.
    let own = (t.end.saturating_sub(400)..t.end)
        .rev()
        .find(|&i| object_name_at(d, i).is_some_and(|e| e + 24 >= t.end))
        .and_then(|i| i.checked_sub(75))
        .map(|k| matrix_at(d, k))
        .unwrap_or(Affine::IDENTITY);
    // Las letras ya vienen giradas en el tapete: se deshace solo el giro para hallar la caja
    // recta del texto y se vuelve a girar su centro.
    let c = (g * own).as_coeffs();
    let angle = c[1].atan2(c[0]);
    if angle.abs() > 1e-6 {
        let pivot = t.bounds.center();
        let local: Vec<Point> = t
            .points
            .iter()
            .map(|p| Affine::rotate_about(-angle, pivot) * *p)
            .collect();
        if let Some(b) = crate_bounds(&local) {
            let center = Affine::rotate_about(angle, pivot) * b.center();
            t.bounds = kurbo::Rect::from_center_size(center, b.size());
        }
    }
    t.rotation_deg = angle.to_degrees();
    t.groups = containing;
}

/// Grupo: su matriz se aplica a los objetos guardados entre el grupo anterior y ella.
struct GroupTransform {
    start: usize,
    matrix_at: usize,
    m: Affine,
}

/// Registro de nombre de objeto: `01 0c000000 <8 bytes> <u32 n> <nombre> 00000000 20000000
/// <32 hex>`. Devuelve el fin del registro.
fn object_name_at(d: &[u8], p: usize) -> Option<usize> {
    if d.get(p..p + 5)? != b"\x01\x0c\x00\x00\x00" {
        return None;
    }
    let n = u32_at(d, p + 13)? as usize;
    if !(1..=200).contains(&n) {
        return None;
    }
    let after = p + 17 + n;
    (u32_at(d, after)? == 0 && hash_like(d, after + 4)).then_some(after + 40)
}

/// ¿El objeto cuyo nombre termina en `end` es un grupo? Los grupos no tienen «creador»
/// (`create_rectangle`, `create_textshape`…): tras 12 bytes y `ff ff ff ff`, longitud 0.
fn is_group_record(d: &[u8], end: usize) -> bool {
    d.get(end + 12..end + 16) == Some(&[0xff; 4]) && u32_at(d, end + 20) == Some(0)
}

/// Las capas tienen la misma forma que los grupos, pero su registro va precedido de
/// `ff ff ff ff` (13 bytes antes del nombre).
fn is_layer_record(d: &[u8], name_at: usize) -> bool {
    name_at
        .checked_sub(13)
        .and_then(|k| d.get(k..k + 4))
        .is_some_and(|b| b == [0xff; 4])
}

fn hash_like(d: &[u8], p: usize) -> bool {
    u32_at(d, p) == Some(32)
        && d.get(p + 4..p + 36)
            .is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit))
}

/// Inicio del registro `01 L` que termina exactamente en `end` (buscando hacia atrás desde
/// `before`).
fn object_start(d: &[u8], end: usize, before: usize) -> Option<usize> {
    (0..before)
        .rev()
        .find(|&h| d[h] == 1 && u32_at(d, h + 1).is_some_and(|l| h + l as usize + 1 == end))
}

/// Matrices de los objetos que no son trazados (grupos): la matriz 3×3 que precede al
/// registro de nombre y no es la de una cadena de subtrazados.
fn group_transforms(
    d: &[u8],
    path_matrices: &std::collections::HashSet<usize>,
) -> Vec<GroupTransform> {
    let mut out = Vec::new();
    let mut prev_end = 0;
    let mut p = 0;
    while let Some(i) = find(d, b"\x01\x0c\x00\x00\x00", p) {
        p = i + 1;
        let Some(end) = object_name_at(d, i) else {
            continue;
        };
        // En los grupos, la matriz 3×3 va justo 75 bytes antes del registro de nombre.
        let found = i.checked_sub(75).filter(|&k| {
            let v: Option<Vec<f64>> = (0..9).map(|j| f32_at(d, k + 4 * j)).collect();
            v.is_some_and(|v| {
                v.iter().all(|x| x.is_finite() && x.abs() < 1e5)
                    && v[6] == 0.0
                    && v[7] == 0.0
                    && v[8] == 1.0
                    && (v[0] * v[4] - v[1] * v[3]).abs() > 1e-6
            })
        });
        match found {
            Some(k)
                if !path_matrices.contains(&k)
                    && is_group_record(d, end)
                    && !is_layer_record(d, i) =>
            {
                // El objeto grupo es un registro `01 L` que encierra a sus hijos y termina 24
                // bytes después del nombre. Si no se encuentra, sus hijos son los objetos
                // desde el grupo anterior.
                let start = object_start(d, end + 24, i).unwrap_or(prev_end);
                out.push(GroupTransform {
                    start,
                    matrix_at: k,
                    m: matrix_at(d, k),
                });
                prev_end = end;
            }
            _ => {}
        }
    }
    out
}

/// Marcas de registro: en la cabecera, tras `02 00 00 00 00 00 00`, van el indicador de
/// activación, los cuatro márgenes (f32), 8 bytes, la longitud y el grosor.
fn reg_marks_at(d: &[u8]) -> Option<RegMarksInfo> {
    let head = &d[..d.len().min(4000)];
    let mut from = 0;
    while let Some(i) = find(head, b"\x02\x00\x00\x00\x00\x00\x00", from) {
        from = i + 1;
        let f = i + 7;
        let flag = *head.get(f)?;
        let insets: Option<Vec<f64>> = (0..4).map(|k| f32_at(head, f + 1 + 4 * k)).collect();
        let (Some(insets), Some(length), Some(thickness)) =
            (insets, f32_at(head, f + 25), f32_at(head, f + 29))
        else {
            continue;
        };
        let plausible = flag <= 1
            && insets.iter().all(|v| (0.0..=200.0).contains(v))
            && (1.0..=100.0).contains(&length)
            && (0.05..=5.0).contains(&thickness);
        if plausible {
            return (flag == 1).then_some(RegMarksInfo {
                insets: [insets[0], insets[1], insets[2], insets[3]],
                length,
                thickness,
            });
        }
    }
    None
}

/// Tamaño de los medios (`constant:<id>`), si el documento lo fija.
fn media_at(d: &[u8]) -> Option<String> {
    let i = find(&d[..d.len().min(8000)], b"constant:", 0)?;
    let n = u32_at(d, i.checked_sub(4)?)? as usize;
    let s = d.get(i + 9..i + n)?;
    s.iter()
        .all(|c| c.is_ascii_alphanumeric())
        .then(|| String::from_utf8_lossy(s).into_owned())
}

pub fn parse(d: &[u8]) -> Result<Studio3Doc, Studio3Error> {
    if !d.starts_with(SIGNATURE) {
        return Err(Studio3Error::NotStudio3);
    }
    let mut doc = Studio3Doc {
        texts: texts(d),
        ..Default::default()
    };
    doc.texts_skipped = count(d, b"create_text").saturating_sub(doc.texts.len());
    let images = image_table(d);
    doc.media = media_at(d);
    doc.reg_marks = reg_marks_at(d);
    // Cadenas de subtrazados: (inicio, fin = posición de su matriz, nodos).
    let mut chains = Vec::new();
    let mut p = 0;
    while p + 15 < d.len() {
        let Some(s) = subpath_at(d, p).map(|_| p) else {
            p += 1;
            continue;
        };
        let mut subs = Vec::new();
        let mut q = s;
        while let Some((end, nodes)) = subpath_at(d, q) {
            subs.push(nodes);
            q = end;
        }
        chains.push((s, q, subs));
        p = q;
    }
    let path_matrices: std::collections::HashSet<usize> = chains.iter().map(|c| c.1).collect();
    let groups = group_transforms(d, &path_matrices);
    for t in &mut doc.texts {
        place_text(d, t, &groups);
    }
    for (s, q, subs) in chains {
        // Transformación de grupo: la del primer grupo que cierra después de la forma.
        // Grupos que contienen la forma, del más externo al más interno.
        let mut containing: Vec<usize> = (0..groups.len())
            .filter(|&i| groups[i].start <= s && s < groups[i].matrix_at)
            .collect();
        containing.sort_by_key(|&i| groups[i].start);
        let g = containing
            .iter()
            .fold(Affine::IDENTITY, |acc, &i| acc * groups[i].m);
        let subs_local = subs.clone();
        let m = g * matrix_at(d, q);
        let lin = Affine::new({
            let c = m.as_coeffs();
            [c[0], c[1], c[2], c[3], 0.0, 0.0]
        });
        let paths = subs
            .into_iter()
            .filter(|n| !n.is_empty())
            .map(|nodes| NodePath {
                nodes: nodes
                    .into_iter()
                    .map(|(pt, hi, ho)| {
                        let w = m * pt;
                        Node {
                            p: w,
                            h_in: hi.map(|v| w + (lin * v.to_point()).to_vec2()),
                            h_out: ho.map(|v| w + (lin * v.to_point()).to_vec2()),
                            smooth: false,
                        }
                    })
                    .collect(),
                // Los trazados de corte se guardan cerrados; el último nodo une con el primero.
                closed: true,
            })
            .collect::<Vec<_>>();
        if !paths.is_empty() {
            let (line, fill, image) = style_before(d, s);
            let image = match image {
                Some((hash, tex)) => match images.get(&hash) {
                    Some((png, w, h)) => {
                        // Contorno en píxeles de la imagen para recortarla.
                        let to_px = tex.inverse();
                        let outline: Vec<Vec<Point>> = local_outline(&subs_local)
                            .into_iter()
                            .map(|c| c.into_iter().map(|p| to_px * p).collect())
                            .collect();
                        let png = clip_png(png, *w, *h, &outline)
                            .map(std::sync::Arc::new)
                            .unwrap_or_else(|| png.clone());
                        Some(place_image(m * tex, png, *w, *h))
                    }
                    // Relleno de superficie sin PNG en el archivo (p. ej., un patrón).
                    None => None,
                },
                None => None,
            };
            doc.shapes.push(Studio3Shape {
                paths,
                line,
                fill,
                image,
                groups: containing,
            });
        }
    }
    if doc.shapes.is_empty() && doc.texts.is_empty() {
        return Err(Studio3Error::Empty);
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(payload: &[u8]) -> Vec<u8> {
        let mut v = vec![1];
        v.extend_from_slice(&(payload.len() as u32 + 4).to_le_bytes());
        v.extend_from_slice(payload);
        v
    }

    fn node(x: f32, y: f32, h: Option<(f32, f32, f32, f32)>) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&x.to_le_bytes());
        p.extend_from_slice(&y.to_le_bytes());
        match h {
            Some((a, b, c, e)) => {
                p.push(1);
                p.extend_from_slice(&a.to_le_bytes());
                p.extend_from_slice(&b.to_le_bytes());
                p.push(1);
                p.extend_from_slice(&c.to_le_bytes());
                p.extend_from_slice(&e.to_le_bytes());
            }
            None => p.extend_from_slice(&[0, 0]),
        }
        p.extend_from_slice(&[0, 0, 0]);
        rec(&p)
    }

    fn subpath(nodes: &[Vec<u8>]) -> Vec<u8> {
        let mut p = vec![1];
        p.extend_from_slice(&(nodes.len() as u32 - 1).to_le_bytes());
        for n in nodes {
            p.extend_from_slice(n);
        }
        rec(&p)
    }

    fn matrix(m: [f32; 9]) -> Vec<u8> {
        m.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(
            parse(b"PK\x03\x04"),
            Err(Studio3Error::NotStudio3)
        ));
    }

    #[test]
    fn rectangle_with_rotation_matrix() {
        let mut d = b"silhouette05;\0\0\0".to_vec();
        d.extend(subpath(&[
            node(0.0, 0.0, None),
            node(10.0, 0.0, None),
            node(10.0, 5.0, None),
            node(0.0, 5.0, None),
        ]));
        // Giro de 90° y traslación (20, 30).
        d.extend(matrix([0.0, -1.0, 20.0, 1.0, 0.0, 30.0, 0.0, 0.0, 1.0]));
        let doc = parse(&d).unwrap();
        assert_eq!(doc.shapes.len(), 1);
        let n = &doc.shapes[0].paths[0].nodes;
        assert_eq!(n.len(), 4);
        assert!((n[1].p - Point::new(20.0, 40.0)).hypot() < 1e-4);
        assert!(doc.shapes[0].paths[0].closed);
    }

    #[test]
    fn bezier_handles_are_relative_and_transformed() {
        let mut d = b"silhouette05;\0\0\0".to_vec();
        d.extend(subpath(&[
            node(10.0, 0.0, Some((0.0, -2.0, 0.0, 2.0))),
            node(0.0, 10.0, None),
        ]));
        d.extend(matrix([1.0, 0.0, 5.0, 0.0, 1.0, 5.0, 0.0, 0.0, 1.0]));
        let doc = parse(&d).unwrap();
        let n0 = doc.shapes[0].paths[0].nodes[0];
        assert!((n0.p - Point::new(15.0, 5.0)).hypot() < 1e-4);
        assert!((n0.h_out.unwrap() - Point::new(15.0, 7.0)).hypot() < 1e-4);
        assert!((n0.h_in.unwrap() - Point::new(15.0, 3.0)).hypot() < 1e-4);
    }

    #[test]
    fn compound_paths_and_style() {
        let mut d = b"silhouette05;\0\0\0".to_vec();
        // Estilo: 01 + RGBA de línea, 8 bytes, registro «cut» con su RGBA, «rounded», 01,
        // «rounded», relleno.
        d.push(1);
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&[0; 8]);
        d.extend_from_slice(CUT_RECORD);
        d.extend_from_slice(b"\x03\x00\x00\x00Cut\x01\x00\x00\x000");
        for v in [1.0f32, 0.6, 0.6, 1.0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&[0, 0, 0]);
        for (i, w) in ["rounded", "rounded"].iter().enumerate() {
            if i == 1 {
                d.push(1);
            }
            d.extend_from_slice(&(w.len() as u32).to_le_bytes());
            d.extend_from_slice(w.as_bytes());
        }
        d.push(1);
        for v in [1.0f32, 1.0, 0.0, 1.0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&[0; 8]);
        let sq = |o: f32, s: f32| {
            subpath(&[
                node(o, o, None),
                node(o + s, o, None),
                node(o + s, o + s, None),
                node(o, o + s, None),
            ])
        };
        d.extend(sq(0.0, 10.0));
        d.extend(sq(3.0, 4.0));
        d.extend(matrix([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]));
        let doc = parse(&d).unwrap();
        assert_eq!(doc.shapes.len(), 1);
        assert_eq!(doc.shapes[0].paths.len(), 2);
        assert_eq!(doc.shapes[0].line, Some(Color::rgba(0, 0, 255, 255)));
        assert_eq!(doc.shapes[0].fill, Some(Color::rgba(255, 255, 0, 255)));
    }

    #[test]
    fn image_object_is_placed_with_its_matrix() {
        let mut d = b"silhouette05;\0\0\0".to_vec();
        // Estilo con relleno de imagen (05 + identificador).
        d.extend_from_slice(&[0; 19]);
        for (i, w) in ["rounded", "rounded"].iter().enumerate() {
            if i == 1 {
                d.push(1);
            }
            d.extend_from_slice(&(w.len() as u32).to_le_bytes());
            d.extend_from_slice(w.as_bytes());
        }
        let hash = "0123456789abcdef0123456789abcdef";
        d.push(5);
        d.extend_from_slice(&32u32.to_le_bytes());
        d.extend_from_slice(hash.as_bytes());
        // Matrices tras el identificador: una identidad y la de textura (píxeles 4×2 → [−1, 1]²).
        d.push(1);
        d.extend(matrix([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]));
        d.push(1);
        d.extend(matrix([0.5, 0.0, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0]));
        d.extend(subpath(&[
            node(-1.0, -1.0, None),
            node(1.0, -1.0, None),
            node(1.0, 1.0, None),
            node(-1.0, 1.0, None),
        ]));
        // Local x → 20 mm hacia abajo (giro 90°), local y → 10 mm; centro (50, 60).
        d.extend(matrix([0.0, -5.0, 50.0, 10.0, 0.0, 60.0, 0.0, 0.0, 1.0]));
        // Tabla de imágenes: identificador (longitud de 64 bits) y PNG mínimo (IHDR 4×2).
        d.extend_from_slice(&32u64.to_le_bytes());
        d.extend_from_slice(hash.as_bytes());
        d.extend_from_slice(&72u64.to_le_bytes());
        d.extend_from_slice(&7u64.to_le_bytes());
        d.extend_from_slice(b"img.png");
        d.extend_from_slice(&[0; 20]);
        d.extend_from_slice(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
        d.extend_from_slice(&4u32.to_be_bytes());
        d.extend_from_slice(&2u32.to_be_bytes());
        d.extend_from_slice(b"\0\0\0\0IEND\xaeB`\x82");
        let doc = parse(&d).unwrap();
        let img = doc.shapes[0].image.as_ref().expect("imagen");
        assert_eq!((img.px_width, img.px_height), (4, 2));
        assert!((img.width_mm - 20.0).abs() < 1e-6 && (img.height_mm - 10.0).abs() < 1e-6);
        assert!((img.rotation_deg - 90.0).abs() < 1e-6);
        assert_eq!(img.center, Point::new(50.0, 60.0));
        assert!(!img.mirrored);
        let shapes = doc.to_shapes(Default::default());
        assert!(shapes[0].is_image());
        // El contorno de la imagen la recorta; no es una línea de corte aparte.
        assert_eq!(shapes.len(), 1);
    }

    #[test]
    fn text_from_glyphs() {
        let mut d = b"silhouette05;\0\0\0".to_vec();
        for (ch, x) in [('H', 10.0f32), ('i', 30.0)] {
            // Registro de fuente.
            d.extend_from_slice(&5u32.to_le_bytes());
            d.extend_from_slice(b"Arial");
            d.extend_from_slice(&[0; 7]);
            d.extend_from_slice(&25.4f32.to_le_bytes());
            d.extend_from_slice(&72.0f32.to_le_bytes());
            d.extend_from_slice(&[0; 20]);
            // Carácter y em.
            d.extend_from_slice(&(ch as u32).to_le_bytes());
            d.extend_from_slice(&25.4f32.to_le_bytes());
            d.extend_from_slice(&[0; 8]);
            // Contorno: un rectángulo de 10×20 en (x, 50).
            let mut pts = vec![1];
            pts.extend_from_slice(&3u32.to_le_bytes());
            for (px, py) in [(x, 50.0f32), (x + 10.0, 50.0), (x + 10.0, 70.0), (x, 70.0)] {
                pts.extend_from_slice(&px.to_le_bytes());
                pts.extend_from_slice(&py.to_le_bytes());
                pts.extend_from_slice(&[0, 0]);
            }
            let mut inner = ((pts.len() + 4) as u32).to_le_bytes().to_vec();
            inner.extend(pts);
            let mut blk = vec![0u8; 4];
            blk.extend(inner);
            d.extend(rec(&blk));
            d.extend_from_slice(&[0; 8]);
        }
        d.extend_from_slice(b"create_text");
        let doc = parse(&d).unwrap();
        assert_eq!(doc.texts.len(), 1);
        let t = &doc.texts[0];
        assert_eq!(t.text, "Hi");
        assert_eq!(t.font_family, "Arial");
        assert_eq!(t.font_size_pt, 72.0);
        assert_eq!(t.bounds, kurbo::Rect::new(10.0, 50.0, 40.0, 70.0));
        assert!(doc.to_shapes(Default::default())[0].is_text());
    }

    #[test]
    fn dump_file_if_requested() {
        let Ok(f) = std::env::var("OPENCRAFTCUTTER_STUDIO3_DUMP") else {
            return;
        };
        let doc = parse(&std::fs::read(f).unwrap()).unwrap();
        for s in doc.to_shapes(Default::default()) {
            eprintln!("{} {:?} {:?} {:?}", s.name, s.group, s.inner_groups, s.rect);
        }
    }

    #[test]
    fn reads_real_files_if_present() {
        // Prueba con documentos reales cuando están disponibles (OPENCRAFTCUTTER_STUDIO3_DIR).
        let Ok(dir) = std::env::var("OPENCRAFTCUTTER_STUDIO3_DIR") else {
            return;
        };
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "studio3") {
                let d = std::fs::read(&p).unwrap();
                let doc = parse(&d);
                eprintln!(
                    "{}: {:?}",
                    p.display(),
                    doc.as_ref().map(|d| (
                        d.shapes.len(),
                        d.shapes.iter().filter(|s| s.image.is_some()).count(),
                        d.images_skipped,
                        d.shapes
                            .iter()
                            .flat_map(|s| s.groups.iter().copied())
                            .collect::<std::collections::BTreeSet<_>>()
                            .len(),
                        d.shapes.iter().filter(|s| s.groups.is_empty()).count(),
                        d.shapes.iter().map(|s| s.groups.len()).max().unwrap_or(0)
                    ))
                );
            }
        }
    }
}
