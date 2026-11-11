//! Escritura de documentos `.studio3`.
//!
//! El formato no está documentado. Se parte de un documento real como plantilla
//! (`assets/template.studio3`): se conserva todo lo que no se entiende y se sustituye la lista
//! de objetos del documento por la nuestra. Cada objeto reutiliza los registros de un objeto de
//! la plantilla y reescribe geometría, colores, caja y nombre.
//!
//! Estructura conocida:
//! - Cabecera fija de 1000 bytes + 24 y, en el byte 16, la tabla de tamaños de las 14 secciones
//!   (u64). La sección 0 es el documento; la 2, la miniatura PNG.
//! - El documento son registros anidados `01 <u32 L>` (L cuenta sus propios 4 bytes). Al
//!   cambiar un registro hay que ajustar la longitud de todos los que lo contienen y el tamaño
//!   de la sección 0.
//! - Objeto: tipo + propiedades (colores de línea y relleno), lista de subtrazados, matriz 3×3,
//!   caja orientada, banderas, caché de nodos en coordenadas del mundo, nombre e identificador.

use kurbo::{Affine, Point, Rect};
use oc_core::nodes::{Node, NodePath};
use oc_core::shape::ShapeKind;
use oc_core::{Color, Document, Shape};

const TEMPLATE: &[u8] = include_bytes!("../assets/template.studio3");
/// Registro del documento (sección 0) y de su lista de objetos.
const DOC: usize = 0x40d;
const ROOT: usize = 0x7c5;
/// Objeto de la plantilla que sirve de molde: el registro del elemento, el del objeto y el de
/// su lista de subtrazados.
const ITEM: usize = 0xe1a;
const OBJ: usize = 0xe20;
const PATHS: usize = 0xfe7;
/// Relleno del molde, para localizarlo en sus propiedades.
/// Registro de estilo dentro de las propiedades del molde (contiene los colores).
const STYLE: usize = 6;
const MOLD_FILL: [u8; 16] = [
    0xff, 0xfd, 0x7d, 0x3f, 0xbb, 0xba, 0x3a, 0x3f, 0xde, 0xe0, 0xe0, 0x3d, 0x00, 0x00, 0x80, 0x3f,
];
const MOLD_NAME: &[u8] = "Polígono".as_bytes();
/// Grupo de la plantilla (el interior del grupo anidado): elemento, objeto y lista de hijos.
const GROUP_ITEM: usize = 0xc44;
const GROUP_OBJ: usize = 0xc4a;
const GROUP_LIST: usize = 0xe11;
const GROUP_NAME: &[u8] = b"Grupo";
/// Bloque de relleno de imagen tomado de un documento real: `05 <u32 32> <clave>` y sus
/// matrices de textura (identidad: el contorno va en píxeles). La clave se sustituye.
const IMAGE_FILL: &[u8] = include_bytes!("../assets/image_fill.bin");
/// Registro que sigue al bloque de relleno en las propiedades.
const FILL_END: &[u8] = b"\x01\x08\x00\x00\x00\xff\xff\xff\xff";
/// Tamaño de los medios: ancho y alto (f32, mm) en el registro de página del documento y en
/// una segunda copia tras la lista de objetos, y el identificador `constant:<medio>`.
const PAGE_REC: usize = 0x44f;
const MEDIA_WH: usize = 0x47d;
const MEDIA_WH_COPY: usize = 0x44e2;
const MEDIA_ID: usize = 0x51b;
/// Constante de las curvas Bézier que aproximan un cuarto de círculo.
const KAPPA: f64 = 0.552_284_749_8;

fn u32_at(d: &[u8], p: usize) -> usize {
    u32::from_le_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]]) as usize
}

fn rec_end(d: &[u8], p: usize) -> usize {
    p + 1 + u32_at(d, p + 1)
}

/// Registro `01 <u32 L> payload`.
pub(crate) fn rec(payload: &[u8]) -> Vec<u8> {
    let mut r = Vec::with_capacity(payload.len() + 5);
    r.push(1);
    r.extend_from_slice(&(payload.len() as u32 + 4).to_le_bytes());
    r.extend_from_slice(payload);
    r
}

pub(crate) fn f32s(out: &mut Vec<u8>, v: &[f64]) {
    for x in v {
        out.extend_from_slice(&(*x as f32).to_le_bytes());
    }
}

fn rgba(c: Color) -> [f64; 4] {
    [
        c.r as f64 / 255.0,
        c.g as f64 / 255.0,
        c.b as f64 / 255.0,
        c.a as f64 / 255.0,
    ]
}

fn replace(buf: &mut Vec<u8>, needle: &[u8], with: &[u8]) {
    if let Some(i) = buf.windows(needle.len()).position(|w| w == needle) {
        buf.splice(i..i + needle.len(), with.iter().copied());
    }
}

/// Manejador opcional relativo al ancla: `01 dx dy` o `00`.
fn handle(out: &mut Vec<u8>, anchor: Point, h: Option<Point>) {
    match h {
        Some(h) => {
            out.push(1);
            f32s(out, &[h.x - anchor.x, h.y - anchor.y]);
        }
        None => out.push(0),
    }
}

fn subpath_record(np: &NodePath) -> Vec<u8> {
    let mut body = vec![u8::from(np.closed)];
    body.extend_from_slice(&(np.nodes.len() as u32 - 1).to_le_bytes());
    for n in &np.nodes {
        let mut p = Vec::new();
        f32s(&mut p, &[n.p.x, n.p.y]);
        handle(&mut p, n.p, n.h_in);
        handle(&mut p, n.p, n.h_out);
        p.extend_from_slice(&[0, u8::from(n.smooth), 0]);
        body.extend(rec(&p));
    }
    rec(&body)
}

/// Caché de nodos en coordenadas del mundo: por subtrazado, `<u32 L> <cerrado> <u32 n-1>` y
/// los nodos sin registro propio.
pub(crate) fn cache_record(paths: &[NodePath], m: Affine) -> Vec<u8> {
    let mut body = (paths.len() as u32 - 1).to_le_bytes().to_vec();
    for np in paths {
        let mut sub = vec![u8::from(np.closed)];
        sub.extend_from_slice(&(np.nodes.len() as u32 - 1).to_le_bytes());
        for n in &np.nodes {
            let p = m * n.p;
            f32s(&mut sub, &[p.x, p.y]);
            handle(&mut sub, p, n.h_in.map(|h| m * h));
            handle(&mut sub, p, n.h_out.map(|h| m * h));
        }
        body.extend_from_slice(&(sub.len() as u32 + 4).to_le_bytes());
        body.extend(sub);
    }
    rec(&body)
}

/// Nodos de una forma en mm, sin rotación (la rotación va en la matriz del objeto).
fn node_paths(s: &Shape) -> Vec<NodePath> {
    let r = s.rect;
    match &s.kind {
        ShapeKind::Path { .. } => s.world_node_paths(),
        ShapeKind::Rect { corner_radius } => vec![rect_nodes(r, *corner_radius)],
        ShapeKind::Ellipse => vec![ellipse_nodes(r)],
        _ => Vec::new(),
    }
    .into_iter()
    .filter(|np| !np.nodes.is_empty())
    .collect()
}

fn closed(nodes: Vec<Node>) -> NodePath {
    NodePath {
        nodes,
        closed: true,
    }
}

fn ellipse_nodes(r: Rect) -> NodePath {
    let c = r.center();
    let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let node = |p: Point, d: kurbo::Vec2| Node {
        p,
        h_in: Some(p - d),
        h_out: Some(p + d),
        smooth: true,
    };
    closed(vec![
        node(Point::new(c.x + rx, c.y), kurbo::Vec2::new(0.0, ky)),
        node(Point::new(c.x, c.y + ry), kurbo::Vec2::new(-kx, 0.0)),
        node(Point::new(c.x - rx, c.y), kurbo::Vec2::new(0.0, -ky)),
        node(Point::new(c.x, c.y - ry), kurbo::Vec2::new(kx, 0.0)),
    ])
}

fn rect_nodes(r: Rect, radius: f64) -> NodePath {
    let rad = radius.clamp(0.0, r.width().min(r.height()) / 2.0);
    if rad <= 0.0 {
        return closed(
            [(r.x0, r.y0), (r.x1, r.y0), (r.x1, r.y1), (r.x0, r.y1)]
                .iter()
                .map(|&(x, y)| Node::line(Point::new(x, y)))
                .collect(),
        );
    }
    let k = rad * KAPPA;
    let n = |x: f64, y: f64, hi: Option<(f64, f64)>, ho: Option<(f64, f64)>| Node {
        p: Point::new(x, y),
        h_in: hi.map(|(dx, dy)| Point::new(x + dx, y + dy)),
        h_out: ho.map(|(dx, dy)| Point::new(x + dx, y + dy)),
        smooth: false,
    };
    closed(vec![
        n(r.x0 + rad, r.y0, Some((-k, 0.0)), None),
        n(r.x1 - rad, r.y0, None, Some((k, 0.0))),
        n(r.x1, r.y0 + rad, Some((0.0, -k)), None),
        n(r.x1, r.y1 - rad, None, Some((0.0, k))),
        n(r.x1 - rad, r.y1, Some((k, 0.0)), None),
        n(r.x0 + rad, r.y1, None, Some((-k, 0.0))),
        n(r.x0, r.y1 - rad, Some((0.0, k)), None),
        n(r.x0, r.y0 + rad, None, Some((0.0, -k))),
    ])
}

/// Identificador de 32 cifras hexadecimales, distinto por objeto.
pub(crate) fn object_id(seed: u64) -> [u8; 32] {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut x = t ^ seed.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let mut out = [0u8; 32];
    for b in out.iter_mut() {
        // xorshift64*: basta con que no se repitan dentro del documento.
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        let v = (x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 60) as u8;
        *b = b"0123456789abcdef"[v as usize];
    }
    out
}

/// Elemento completo de la lista de objetos para una forma, o `None` si no tiene trazados.
/// Imagen guardada en la sección de imágenes: clave, PNG RGBA y tamaño en píxeles.
struct StoredImage {
    key: [u8; 32],
    png: Vec<u8>,
    w: u32,
    h: u32,
    /// PNG original, para reutilizar la entrada si la misma imagen aparece varias veces.
    src: std::sync::Arc<Vec<u8>>,
}

impl StoredImage {
    fn png_src_eq(&self, other: &std::sync::Arc<Vec<u8>>) -> bool {
        std::sync::Arc::ptr_eq(&self.src, other) || *self.src == **other
    }
}

/// Contorno local, matriz al mundo y bloque de relleno de imagen (si la forma es una imagen).
type Geometry = (Vec<NodePath>, Affine, Option<Vec<u8>>);

fn geometry(s: &Shape, seed: u64, images: &mut Vec<StoredImage>) -> Option<Geometry> {
    let c = s.rect.center();
    let rot = Affine::translate(c.to_vec2())
        * Affine::rotate(s.rotation.to_radians())
        * Affine::translate(-c.to_vec2());
    if let ShapeKind::Image { png, .. } = &s.kind {
        // La referencia guarda el contorno en píxeles del PNG y la matriz lo lleva a mm.
        let rgba = image::load_from_memory(&png.0).ok()?.to_rgba8();
        let (w, h) = rgba.dimensions();
        let key = match images.iter().find(|i| i.png_src_eq(&png.0)) {
            Some(i) => i.key,
            None => {
                let mut out = std::io::Cursor::new(Vec::new());
                rgba.write_to(&mut out, image::ImageFormat::Png).ok()?;
                let key = object_id(seed ^ 0x1a6e);
                images.push(StoredImage {
                    key,
                    png: out.into_inner(),
                    w,
                    h,
                    src: png.0.clone(),
                });
                key
            }
        };
        let r = s.rect;
        let m = rot
            * Affine::translate((r.x0, r.y0))
            * Affine::scale_non_uniform(r.width() / w as f64, r.height() / h as f64);
        let outline = rect_nodes(Rect::new(0.0, 0.0, w as f64, h as f64), 0.0);
        let mut fill = IMAGE_FILL.to_vec();
        fill[5..37].copy_from_slice(&key);
        return Some((vec![outline], m, Some(fill)));
    }
    let paths = node_paths(s);
    (!paths.is_empty()).then_some((paths, rot, None))
}

fn item_record(s: &Shape, seed: u64, images: &mut Vec<StoredImage>) -> Option<(Vec<u8>, Rect)> {
    let (paths, m, image_fill) = geometry(s, seed, images)?;
    let t = TEMPLATE;
    let obj_end = rec_end(t, OBJ);
    let item_end = rec_end(t, ITEM);
    let cache_at = obj_end + 36 + 24 + 5;
    let cache_end = rec_end(t, cache_at);

    // Propiedades con los colores de la forma (sin color de línea, transparente).
    let mut props = t[OBJ + 5..PATHS].to_vec();
    let before = props.len();
    // Color de línea: `01` + RGBA + 8 bytes justo antes del registro `cut`.
    if let Some(k) = props
        .windows(crate::CUT_RECORD.len())
        .position(|w| w == crate::CUT_RECORD)
        && k >= 25
        && props[k - 25] == 1
    {
        if image_fill.is_some() {
            // Las imágenes no llevan línea (en la referencia, `00` en vez de `01` + RGBA): si
            // no, se dibuja y se corta el rectángulo de la imagen.
            props.splice(k - 25..k - 8, [0u8]);
        } else {
            let mut line = Vec::new();
            f32s(&mut line, &rgba(s.stroke.color));
            props[k - 24..k - 8].copy_from_slice(&line);
        }
    }
    // Relleno: `01` + RGBA + transparencia, o `00` sin relleno.
    let mut mold_fill = vec![1u8];
    mold_fill.extend_from_slice(&MOLD_FILL);
    match image_fill {
        // Imagen: se sustituye todo el bloque de relleno (hasta el registro que lo sigue).
        Some(block) => {
            if let Some(i) = props
                .windows(mold_fill.len())
                .position(|w| *w == mold_fill[..])
                && let Some(j) = props[i..]
                    .windows(FILL_END.len())
                    .position(|w| w == FILL_END)
            {
                props.splice(i..i + j, block);
            }
        }
        None => {
            match s.fill {
                Some(c) => {
                    // Opacidad en el alfa del color y transparencia (1 - opacidad) en el f32
                    // que lo sigue, como guarda la referencia.
                    let opacity = s.fill_opacity.clamp(0.0, 1.0) * c.a as f64 / 255.0;
                    let mut f = vec![1u8];
                    f32s(&mut f, &rgba(Color { a: 255, ..c }));
                    f.truncate(f.len() - 4);
                    f32s(&mut f, &[opacity, 1.0 - opacity]);
                    let mut mold = mold_fill.clone();
                    mold.extend_from_slice(&[0; 4]);
                    replace(&mut props, &mold, &f);
                }
                None => replace(&mut props, &mold_fill, &[0u8]),
            }
        }
    }
    // Sin relleno el bloque encoge: ajustar las dos longitudes que lo encierran (la del bloque,
    // tras el tipo, y la del registro de estilo).
    let delta = props.len() as i64 - before as i64;
    let total = (props.len() - 1) as u32;
    props[1..5].copy_from_slice(&total.to_le_bytes());
    let style = (u32_at(&props, STYLE + 1) as i64 + delta) as u32;
    props[STYLE + 1..STYLE + 5].copy_from_slice(&style.to_le_bytes());

    let mut list = (paths.len() as u32 - 1).to_le_bytes().to_vec();
    for np in &paths {
        list.extend(subpath_record(np));
    }
    let mut obj = props;
    obj.extend(rec(&list));
    let obj = rec(&obj);

    let k = m.as_coeffs();
    let mut tail = Vec::new();
    f32s(
        &mut tail,
        &[k[0], k[2], k[4], k[1], k[3], k[5], 0.0, 0.0, 1.0],
    );
    // Caja orientada: centro y semiejes (en coordenadas del mundo).
    let r = s.rect;
    let c = r.center();
    let rot = Affine::translate(c.to_vec2())
        * Affine::rotate(s.rotation.to_radians())
        * Affine::translate(-c.to_vec2());
    let ax = rot * Point::new(r.x1, c.y) - rot * c;
    let ay = rot * Point::new(c.x, r.y1) - rot * c;
    let wc = rot * c;
    f32s(&mut tail, &[wc.x, wc.y, ax.x, ax.y, ay.x, ay.y]);
    tail.extend_from_slice(&t[obj_end + 60..cache_at]);
    tail.extend(cache_record(&paths, m));

    let rest = named(&t[cache_end..item_end], &s.name, "Forma", seed);

    let mut item = vec![0u8];
    item.extend(obj);
    item.extend(tail);
    item.extend(rest);
    // Caja en el mundo (anclas y manejadores: basta como envolvente).
    let pts = paths.iter().flat_map(|np| {
        np.nodes
            .iter()
            .flat_map(|n| [Some(n.p), n.h_in, n.h_out])
            .flatten()
            .map(|p| m * p)
    });
    Some((rec(&item), bounds_of(pts)?))
}

fn bounds_of(mut pts: impl Iterator<Item = Point>) -> Option<Rect> {
    let first = pts.next()?;
    Some(pts.fold(Rect::from_points(first, first), |r, p| r.union_pt(p)))
}

/// Cola de nombre e identificador de un elemento del molde con otro nombre y otro id.
pub(crate) fn named(mold: &[u8], name: &str, fallback: &str, seed: u64) -> Vec<u8> {
    let mut rest = mold.to_vec();
    let name = if name.is_empty() { fallback } else { name };
    let Some((k, len)) = [MOLD_NAME, GROUP_NAME, b"Texto".as_slice()]
        .iter()
        .find_map(|n| {
            rest.windows(n.len())
                .position(|w| w == *n)
                .map(|k| (k, n.len()))
        })
    else {
        return rest;
    };
    let mut bytes = (name.len() as u32).to_le_bytes().to_vec();
    bytes.extend_from_slice(name.as_bytes());
    rest.splice(k - 4..k + len, bytes.iter().copied());
    let after = k + name.len();
    if let Some(h) = rest[after..]
        .windows(4)
        .position(|w| w == [0x20, 0, 0, 0])
        .map(|h| after + h + 4)
        && h + 32 <= rest.len()
    {
        rest[h..h + 32].copy_from_slice(&object_id(seed));
    }
    rest
}

/// Elemento de grupo con sus hijos (ya codificados) y la caja que los envuelve.
fn group_record(children: Vec<(Vec<u8>, Rect)>, seed: u64) -> Option<(Vec<u8>, Rect)> {
    let r = children.iter().map(|c| c.1).reduce(|a, b| a.union(b))?;
    let t = TEMPLATE;
    let obj_end = rec_end(t, GROUP_OBJ);
    let list_end = rec_end(t, GROUP_LIST);
    let mut list = (children.len() as u32 - 1).to_le_bytes().to_vec();
    for c in &children {
        list.extend_from_slice(&c.0);
    }
    let mut obj = t[GROUP_OBJ + 5..GROUP_LIST].to_vec();
    obj.extend(rec(&list));
    obj.extend_from_slice(&t[list_end..obj_end]);
    let mut item = vec![0u8];
    item.extend(rec(&obj));
    // Matriz identidad (las formas ya van en coordenadas del mundo) y caja.
    f32s(&mut item, &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    let c = r.center();
    f32s(
        &mut item,
        &[c.x, c.y, r.width() / 2.0, 0.0, 0.0, r.height() / 2.0],
    );
    item.extend(named(
        &t[obj_end + 60..rec_end(t, GROUP_ITEM)],
        "",
        "Grupo",
        seed,
    ));
    Some((rec(&item), r))
}

/// Árbol de grupos en el orden del documento (cada grupo donde aparece su primer objeto).
enum Tree<'a> {
    Shape(&'a Shape),
    Group(u64, Vec<Tree<'a>>),
}

fn insert<'a>(nodes: &mut Vec<Tree<'a>>, path: &[u64], s: &'a Shape) {
    let Some((&g, rest)) = path.split_first() else {
        nodes.push(Tree::Shape(s));
        return;
    };
    let i = nodes
        .iter()
        .position(|n| matches!(n, Tree::Group(id, _) if *id == g))
        .unwrap_or_else(|| {
            nodes.push(Tree::Group(g, Vec::new()));
            nodes.len() - 1
        });
    if let Tree::Group(_, children) = &mut nodes[i] {
        insert(children, rest, s);
    }
}

/// Textos editables preparados por quien llama (por identificador de forma).
pub type TextRuns = std::collections::HashMap<oc_core::ShapeId, crate::text_write::TextRun>;

fn encode(
    nodes: &[Tree],
    seed: &mut u64,
    images: &mut Vec<StoredImage>,
    runs: &TextRuns,
) -> Vec<(Vec<u8>, Rect)> {
    nodes
        .iter()
        .filter_map(|n| {
            *seed += 1;
            match n {
                Tree::Shape(s) => match runs.get(&s.id) {
                    Some(run) => crate::text_write::text_item(
                        run,
                        s.rotation,
                        s.stroke.color,
                        s.fill.map(|c| (c, s.fill_opacity.clamp(0.0, 1.0))),
                        &s.name,
                        *seed,
                    ),
                    None => item_record(s, *seed, images),
                },
                Tree::Group(_, children) => {
                    let kids = encode(children, seed, images, runs);
                    match kids.len() {
                        0 => None,
                        // Un grupo de un solo objeto no aporta nada en la referencia.
                        1 => kids.into_iter().next(),
                        _ => group_record(kids, *seed),
                    }
                }
            }
        })
        .collect()
}

/// Documento `.studio3` con las formas de trazado, rectángulos y elipses de `doc`.
///
/// Los grupos (también anidados) se conservan. Por ahora no se escriben textos, imágenes,
/// tamaño de página ni miniatura: se omiten (textos e imágenes) o quedan como en la plantilla.
pub fn write(doc: &Document) -> Vec<u8> {
    write_with_texts(doc, &TextRuns::new())
}

/// Como [`write`], con los textos de `runs` como textos editables.
pub fn write_with_texts(doc: &Document, runs: &TextRuns) -> Vec<u8> {
    let mut tree = Vec::new();
    for s in doc.shapes.iter().filter(|s| s.visible) {
        let path: Vec<u64> = s.group.iter().chain(&s.inner_groups).copied().collect();
        insert(&mut tree, &path, s);
    }
    let mut images = Vec::new();
    let items: Vec<Vec<u8>> = encode(&tree, &mut 0, &mut images, runs)
        .into_iter()
        .map(|i| i.0)
        .collect();
    let mut root_body = (items.len().max(1) as u32 - 1).to_le_bytes().to_vec();
    for it in &items {
        root_body.extend_from_slice(it);
    }
    let root = if items.is_empty() {
        // Lista vacía: registro con recuento 0 y ningún elemento.
        rec(&0u32.to_le_bytes())
    } else {
        rec(&root_body)
    };
    let mut d = TEMPLATE.to_vec();
    let old = rec_end(&d, ROOT) - ROOT;
    let delta = root.len() as i64 - old as i64;
    d.splice(ROOT..ROOT + old, root);
    let doc_len = (u32_at(&d, DOC + 1) as i64 + delta) as u32;
    d[DOC + 1..DOC + 5].copy_from_slice(&doc_len.to_le_bytes());
    let mut wh = Vec::new();
    f32s(&mut wh, &[doc.page.width_mm, doc.page.height_mm]);
    d[MEDIA_WH..MEDIA_WH + 8].copy_from_slice(&wh);
    let copy = (MEDIA_WH_COPY as i64 + delta) as usize;
    d[copy..copy + 8].copy_from_slice(&wh);
    // Identificador del medio: `constant:<id>` para los tamaños fijos (los de la referencia
    // coinciden con los nuestros) y vacío a medida.
    let mut media = match oc_core::machines::media_size(&doc.page.media_size)
        .filter(|m| m.width_mm > 0.0)
        // Solo los identificadores vistos en la referencia; los demás van a medida.
        .filter(|m| matches!(m.id, "12x12" | "letter" | "a4" | "a5" | "a6" | "5x7"))
    {
        Some(m) => format!("constant:{}", m.id),
        // A medida la referencia deja el identificador vacío y usa el ancho y alto escritos.
        None => String::new(),
    }
    .into_bytes();
    let mut field = (media.len() as u32).to_le_bytes().to_vec();
    field.append(&mut media);
    let old_len = 4 + u32_at(&d, MEDIA_ID);
    let grow = field.len() as i64 - old_len as i64;
    d.splice(MEDIA_ID..MEDIA_ID + old_len, field);
    for rec_at in [DOC, PAGE_REC] {
        let l = (u32_at(&d, rec_at + 1) as i64 + grow) as u32;
        d[rec_at + 1..rec_at + 5].copy_from_slice(&l.to_le_bytes());
    }
    let s0 = u64::from_le_bytes(d[16..24].try_into().expect("8 bytes"));
    let s0 = (s0 as i64 + delta + grow) as u64;
    d[16..24].copy_from_slice(&s0.to_le_bytes());
    // Sección 2: miniatura (tras la de imágenes, que aún tiene el tamaño de la plantilla).
    if let Some(png) = thumbnail(doc) {
        let s1 = u64::from_le_bytes(d[24..32].try_into().expect("8 bytes")) as usize;
        let s2_at = 0x400 + s0 as usize + s1;
        let s2_old = u64::from_le_bytes(d[32..40].try_into().expect("8 bytes")) as usize;
        d[32..40].copy_from_slice(&(png.len() as u64).to_le_bytes());
        d.splice(s2_at..s2_at + s2_old, png);
    }
    // Sección 1: imágenes.
    let s1_at = 0x400 + s0 as usize;
    let s1_old = u64::from_le_bytes(d[24..32].try_into().expect("8 bytes")) as usize;
    let s1 = image_section(&images);
    d[24..32].copy_from_slice(&(s1.len() as u64).to_le_bytes());
    d.splice(s1_at..s1_at + s1_old, s1);
    d
}

/// Sustituye `old` bytes en `at` y ajusta las longitudes de los registros (`<etiqueta>
/// <u32 L>`, con fin en `h + 1 + L`) que encierran el cambio, del más externo al más interno.
pub(crate) fn splice_fix(blob: &mut Vec<u8>, at: usize, old: usize, new: &[u8]) {
    let mut chain = Vec::new();
    let mut limit = blob.len();
    for h in 0..at {
        if !matches!(blob[h], 1 | 2 | 5 | 6 | 7) || h + 5 > blob.len() {
            continue;
        }
        let l = u32_at(blob, h + 1);
        let e = h + 1 + l;
        if l >= 4 && e > at && e >= at + old && e <= limit {
            chain.push(h);
            limit = e;
        }
    }
    let delta = new.len() as i64 - old as i64;
    blob.splice(at..at + old, new.iter().copied());
    for h in chain {
        let l = (u32_at(blob, h + 1) as i64 + delta) as u32;
        blob[h + 1..h + 5].copy_from_slice(&l.to_le_bytes());
    }
}

/// Colores de un bloque de estilo de la referencia: línea (`01` + RGBA antes del registro
/// `cut…`) y relleno (`01` + RGBA o `00`, tras las palabras de unión y remate; luego la
/// transparencia). `fill` = color y opacidad.
pub(crate) fn set_style(blob: &mut Vec<u8>, line: Color, fill: Option<(Color, f64)>) {
    // Registro `cut`/`cutunion`: `01 <u32> <u32 n> cut…`.
    let Some(r) = (9..blob.len().saturating_sub(3)).find(|&i| {
        &blob[i..i + 3] == b"cut" && matches!(u32_at(blob, i - 4), 3 | 8) && blob[i - 9] == 1
    }) else {
        return;
    };
    let r = r - 9;
    let mut c = vec![1u8];
    f32s(&mut c, &rgba(line));
    if r >= 25 && blob[r - 25] == 1 {
        blob[r - 24..r - 8].copy_from_slice(&c[1..]);
    } else if r >= 9 && blob[r - 9] == 0 {
        splice_fix(blob, r - 9, 1, &c);
    }
    const J: &[u8] = b"\x07\x00\x00\x00rounded\x01\x07\x00\x00\x00rounded";
    let Some(s) = blob
        .windows(J.len())
        .position(|w| w == J)
        .map(|k| k + J.len())
    else {
        return;
    };
    let had = blob[s] == 1;
    match fill {
        Some((c, opacity)) => {
            let mut f = vec![1u8];
            f32s(&mut f, &rgba(Color { a: 255, ..c }));
            f.truncate(f.len() - 4);
            f32s(&mut f, &[opacity]);
            splice_fix(blob, s, if had { 17 } else { 1 }, &f);
            let mut t = Vec::new();
            f32s(&mut t, &[1.0 - opacity]);
            blob[s + 17..s + 21].copy_from_slice(&t);
        }
        None => {
            if had {
                splice_fix(blob, s, 17, &[0]);
            }
            blob[s + 1..s + 5].copy_from_slice(&[0; 4]);
        }
    }
}

/// Lado de la miniatura de la referencia (PNG RGBA con fondo transparente).
const THUMB: u32 = 512;

/// Miniatura del documento: los objetos visibles ajustados y centrados en 512×512.
fn thumbnail(doc: &Document) -> Option<Vec<u8>> {
    use tiny_skia::{
        FillRule, FilterQuality, Paint, PathBuilder, Pattern, Pixmap, SpreadMode, Stroke, Transform,
    };
    let mut items = Vec::new();
    for s in doc.shapes.iter().filter(|s| s.visible) {
        let Some((paths, m, image)) = geometry(s, 0, &mut Vec::new()) else {
            continue;
        };
        items.push((s, paths, m, image.is_some()));
    }
    let bounds = items
        .iter()
        .flat_map(|(_, paths, m, _)| {
            let m = *m;
            paths
                .iter()
                .flat_map(move |np| np.nodes.iter().map(move |n| m * n.p))
        })
        .fold(None, |r: Option<Rect>, p| {
            Some(r.map_or(Rect::from_points(p, p), |r| r.union_pt(p)))
        })?;
    let side = THUMB as f64;
    let margin = side * 0.04;
    let scale = (side - 2.0 * margin) / bounds.width().max(bounds.height()).max(1e-6);
    let fit = Affine::translate((side / 2.0, side / 2.0))
        * Affine::scale(scale)
        * Affine::translate(-bounds.center().to_vec2());
    let to_ts = |a: Affine| {
        let c = a.as_coeffs();
        Transform::from_row(
            c[0] as f32,
            c[1] as f32,
            c[2] as f32,
            c[3] as f32,
            c[4] as f32,
            c[5] as f32,
        )
    };
    let mut pix = Pixmap::new(THUMB, THUMB)?;
    for (s, paths, m, is_image) in items {
        let mut pb = PathBuilder::new();
        for np in &paths {
            let n = np.nodes.len();
            let Some(first) = np.nodes.first() else {
                continue;
            };
            pb.move_to(first.p.x as f32, first.p.y as f32);
            let segs = if np.closed { n } else { n - 1 };
            for i in 0..segs {
                let (a, b) = (&np.nodes[i], &np.nodes[(i + 1) % n]);
                if a.h_out.is_some() || b.h_in.is_some() {
                    let c1 = a.h_out.unwrap_or(a.p);
                    let c2 = b.h_in.unwrap_or(b.p);
                    pb.cubic_to(
                        c1.x as f32,
                        c1.y as f32,
                        c2.x as f32,
                        c2.y as f32,
                        b.p.x as f32,
                        b.p.y as f32,
                    );
                } else {
                    pb.line_to(b.p.x as f32, b.p.y as f32);
                }
            }
            if np.closed {
                pb.close();
            }
        }
        let Some(path) = pb.finish() else { continue };
        let ts = to_ts(fit * m);
        let mut paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        if is_image {
            let ShapeKind::Image { png, .. } = &s.kind else {
                continue;
            };
            let Ok(img) = image::load_from_memory(&png.0) else {
                continue;
            };
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let mut data = rgba.into_raw();
            for px in data.chunks_exact_mut(4) {
                let a = px[3] as u16;
                for c in &mut px[..3] {
                    *c = ((*c as u16 * a + 127) / 255) as u8;
                }
            }
            let Some(src) =
                tiny_skia::IntSize::from_wh(w, h).and_then(|sz| Pixmap::from_vec(data, sz))
            else {
                continue;
            };
            paint.shader = Pattern::new(
                src.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bicubic,
                1.0,
                Transform::identity(),
            );
            pix.fill_path(&path, &paint, FillRule::Winding, ts, None);
            continue;
        }
        if let Some(c) = s.fill {
            paint.set_color_rgba8(
                c.r,
                c.g,
                c.b,
                (s.fill_opacity.clamp(0.0, 1.0) * c.a as f64) as u8,
            );
            pix.fill_path(&path, &paint, FillRule::EvenOdd, ts, None);
        }
        // Contorno fino en el color de la línea (como las líneas de corte en la referencia).
        let c = s.stroke.color;
        paint.set_color_rgba8(c.r, c.g, c.b, c.a);
        // El trazo se dibuja ya transformado para que su grosor sea en píxeles.
        let Some(outline) = path.transform(ts) else {
            continue;
        };
        pix.stroke_path(
            &outline,
            &paint,
            &Stroke {
                width: 1.5,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
    pix.encode_png().ok()
}

/// Sección de imágenes: `01 <u64 longitud-1> <u64 n-1>` y una entrada por imagen.
fn image_section(images: &[StoredImage]) -> Vec<u8> {
    let q = |out: &mut Vec<u8>, v: u64| out.extend_from_slice(&v.to_le_bytes());
    let mut body = Vec::new();
    q(&mut body, (images.len() as u64).wrapping_sub(1));
    for i in images {
        let mut e = Vec::new();
        q(&mut e, 32);
        e.extend_from_slice(&i.key);
        // Campos fijos de la referencia; ancho y alto en píxeles, 72 ppp, 32 bits, RGBA (6).
        for v in [65, 0, i.w as u64, i.h as u64, 72, 32, 6, 0x3e0] {
            q(&mut e, v);
        }
        e.push(0);
        q(&mut e, i.png.len() as u64);
        e.extend_from_slice(&i.png);
        q(&mut e, 0);
        q(&mut body, e.len() as u64 + 8);
        body.extend(e);
    }
    let mut out = vec![1u8];
    q(&mut out, body.len() as u64 + 8);
    out.extend(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::Shape;

    fn bounds(np: &NodePath) -> Rect {
        np.nodes
            .iter()
            .fold(Rect::from_points(np.nodes[0].p, np.nodes[0].p), |r, n| {
                r.union_pt(n.p)
            })
    }

    fn tri() -> Shape {
        let mut s = Shape::new(
            0,
            "Triángulo".to_string(),
            Rect::new(0.0, 0.0, 1.0, 1.0),
            ShapeKind::Rect { corner_radius: 0.0 },
        );
        s.set_world_node_paths(&[NodePath::from_points(
            &[
                Point::new(30.0, 30.0),
                Point::new(90.0, 30.0),
                Point::new(60.0, 90.0),
            ],
            true,
        )]);
        s.fill = Some(Color::rgb(50, 100, 230));
        s
    }

    #[test]
    fn template_layout_is_as_expected() {
        let t = TEMPLATE;
        assert_eq!(&t[MEDIA_ID + 4..MEDIA_ID + 4 + 11], b"constant:a4");
        assert_eq!(u32_at(t, MEDIA_ID), 11);
        assert!(rec_end(t, PAGE_REC) > MEDIA_ID + 15 && rec_end(t, PAGE_REC) < ROOT);
        for at in [MEDIA_WH, MEDIA_WH_COPY] {
            assert_eq!(t[at..at + 8], [0, 0, 0x52, 0x43, 0, 0x80, 0x94, 0x43]);
        }
        assert!(t.starts_with(b"silhouette05;"));
        assert_eq!(rec_end(t, PATHS), rec_end(t, OBJ));
        assert!(rec_end(t, OBJ) < rec_end(t, ITEM));
        assert!(
            t[ITEM..rec_end(t, ITEM)]
                .windows(MOLD_NAME.len())
                .any(|w| w == MOLD_NAME)
        );
    }

    /// PNG de prueba: degradado de `w`×`h` píxeles.
    pub(super) fn test_png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 160, 255])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    pub(super) fn image_shape(r: Rect, rotation: f64) -> Shape {
        let mut s = Shape::new(
            0,
            "Imagen".to_string(),
            r,
            ShapeKind::Image {
                png: oc_core::shape::ImageData(std::sync::Arc::new(test_png(200, 120))),
                px_width: 200,
                px_height: 120,
            },
        );
        s.rotation = rotation;
        s
    }

    #[test]
    fn images_round_trip() {
        let mut doc = Document::default();
        doc.shapes
            .push(image_shape(Rect::new(20.0, 30.0, 120.0, 90.0), 0.0));
        doc.shapes
            .push(image_shape(Rect::new(20.0, 120.0, 120.0, 180.0), 30.0));
        let bytes = write(&doc);
        let sizes: u64 = (0..14)
            .map(|i| u64::from_le_bytes(bytes[16 + 8 * i..24 + 8 * i].try_into().unwrap()))
            .sum();
        assert_eq!(sizes as usize + 0x400, bytes.len());
        let back = crate::parse(&bytes).expect("se lee");
        let imgs: Vec<_> = back
            .shapes
            .iter()
            .filter_map(|s| s.image.as_ref())
            .collect();
        assert_eq!(imgs.len(), 2);
        let a = imgs[0];
        assert_eq!((a.px_width, a.px_height), (200, 120));
        assert!((a.center.x - 70.0).abs() < 1e-2 && (a.center.y - 60.0).abs() < 1e-2);
        assert!((a.width_mm - 100.0).abs() < 1e-2 && (a.height_mm - 60.0).abs() < 1e-2);
        assert!((imgs[1].rotation_deg - 30.0).abs() < 1e-2);
        assert!((imgs[1].center.y - 150.0).abs() < 1e-2);
        let img_shapes: Vec<_> = back.shapes.iter().filter(|s| s.image.is_some()).collect();
        assert!(
            img_shapes.iter().all(|s| s.line.is_none()),
            "las imágenes no llevan línea"
        );
    }

    #[test]
    fn nested_groups_round_trip() {
        let mut doc = Document::default();
        let sq = |x: f64, g: Option<u64>, inner: Vec<u64>| {
            let mut s = Shape::new(
                0,
                "Cuadro".to_string(),
                Rect::new(x, 10.0, x + 10.0, 20.0),
                ShapeKind::Rect { corner_radius: 0.0 },
            );
            s.group = g;
            s.inner_groups = inner;
            s
        };
        // Grupo 1 = { A, grupo 2 = { B, C } }; D suelto.
        doc.shapes.push(sq(0.0, Some(1), vec![]));
        doc.shapes.push(sq(20.0, Some(1), vec![2]));
        doc.shapes.push(sq(40.0, Some(1), vec![2]));
        doc.shapes.push(sq(60.0, None, vec![]));
        let back = crate::parse(&write(&doc)).expect("se lee");
        let groups: Vec<usize> = back.shapes.iter().map(|s| s.groups.len()).collect();
        assert_eq!(groups, vec![1, 2, 2, 0]);
        assert_eq!(back.shapes[0].groups[0], back.shapes[1].groups[0]);
        assert_eq!(back.shapes[1].groups, back.shapes[2].groups);
        let b = bounds(&back.shapes[2].paths[0]);
        assert!(
            (b.x0 - 40.0).abs() < 1e-3,
            "sin desplazar por el grupo: {b:?}"
        );
    }

    #[test]
    fn round_trips_through_reader() {
        let mut doc = Document::default();
        doc.shapes.push(tri());
        let mut r = Shape::new(
            0,
            "Elipse".to_string(),
            Rect::new(120.0, 30.0, 180.0, 90.0),
            ShapeKind::Ellipse,
        );
        r.stroke.color = Color::rgb(0, 150, 0);
        r.fill = None;
        doc.shapes.push(r);
        let bytes = write(&doc);
        let back = crate::parse(&bytes).expect("se lee");
        assert_eq!(back.shapes.len(), 2);
        let b0 = bounds(&back.shapes[0].paths[0]);
        assert!((b0.x0 - 30.0).abs() < 1e-3 && (b0.y1 - 90.0).abs() < 1e-3);
        let b1 = bounds(&back.shapes[1].paths[0]);
        assert!((b1.x0 - 120.0).abs() < 1e-3 && (b1.x1 - 180.0).abs() < 1e-3);
        assert_eq!(
            back.shapes[0].fill.map(|c| (c.r, c.g, c.b)),
            Some((50, 100, 230))
        );
        assert_eq!(
            back.shapes[1].line.map(|c| (c.r, c.g, c.b)),
            Some((0, 150, 0))
        );
        // Tamaño de la sección 0 coherente con el archivo.
        let sizes: u64 = (0..14)
            .map(|i| u64::from_le_bytes(bytes[16 + 8 * i..24 + 8 * i].try_into().unwrap()))
            .sum();
        assert_eq!(sizes as usize + 0x400, bytes.len());
    }
}

#[cfg(test)]
mod sample {
    use super::*;

    /// OPENCRAFTCUTTER_STUDIO3_WRITE=salida.studio3: escribe un documento de muestra para abrirlo en
    /// la referencia.
    #[test]
    fn write_sample_if_requested() {
        let Ok(out) = std::env::var("OPENCRAFTCUTTER_STUDIO3_WRITE") else {
            return;
        };
        let mut doc = Document::default();
        if let Ok(m) = std::env::var("OPENCRAFTCUTTER_STUDIO3_MEDIA") {
            // «letter» o «ancho x alto» a medida.
            match m.split_once('x') {
                Some((w, h)) => {
                    doc.page.media_size = "custom".into();
                    doc.page.width_mm = w.parse().unwrap();
                    doc.page.height_mm = h.parse().unwrap();
                }
                None => {
                    let ms = oc_core::machines::media_size(&m).unwrap();
                    doc.page.media_size = m.clone();
                    doc.page.width_mm = ms.width_mm;
                    doc.page.height_mm = ms.height_mm;
                }
            }
        }
        let mut add = |name: &str, r: Rect, kind: ShapeKind, f: &dyn Fn(&mut Shape)| {
            let mut s = Shape::new(0, name.to_string(), r, kind);
            f(&mut s);
            doc.shapes.push(s);
        };
        add(
            "Rectángulo",
            Rect::new(20.0, 20.0, 70.0, 50.0),
            ShapeKind::Rect { corner_radius: 0.0 },
            &|s| {
                s.fill = Some(Color::rgb(240, 180, 30));
            },
        );
        add(
            "Redondeado",
            Rect::new(100.0, 20.0, 160.0, 60.0),
            ShapeKind::Rect { corner_radius: 8.0 },
            &|s| {
                s.rotation = 20.0;
                s.fill = Some(Color::rgb(60, 120, 230));
                s.stroke.color = Color::rgb(0, 0, 0);
            },
        );
        add(
            "Elipse",
            Rect::new(20.0, 80.0, 90.0, 120.0),
            ShapeKind::Ellipse,
            &|s| {
                s.fill = None;
                s.stroke.color = Color::rgb(20, 90, 220);
            },
        );
        add(
            "Semitransparente",
            Rect::new(110.0, 90.0, 160.0, 140.0),
            ShapeKind::Ellipse,
            &|s| {
                s.fill = Some(Color::rgb(230, 40, 120));
                s.fill_opacity = 0.5;
            },
        );
        // Grupo con otro grupo dentro: { cuadro, { dos círculos } }.
        for (i, (x, inner)) in [(170.0, vec![]), (200.0, vec![2]), (225.0, vec![2])]
            .into_iter()
            .enumerate()
        {
            let kind = if i == 0 {
                ShapeKind::Rect { corner_radius: 0.0 }
            } else {
                ShapeKind::Ellipse
            };
            add(
                "En grupo",
                Rect::new(x - 60.0, 160.0, x - 40.0, 180.0),
                kind,
                &|s| {
                    s.group = Some(1);
                    s.inner_groups = inner.clone();
                    s.fill = Some(Color::rgb(120, 60, 200));
                },
            );
        }
        add(
            "Con agujero",
            Rect::new(0.0, 0.0, 1.0, 1.0),
            ShapeKind::Rect { corner_radius: 0.0 },
            &|s| {
                let outer = NodePath {
                    nodes: vec![
                        Node::line(Point::new(20.0, 160.0)),
                        Node {
                            p: Point::new(90.0, 160.0),
                            h_in: None,
                            h_out: Some(Point::new(110.0, 180.0)),
                            smooth: false,
                        },
                        Node::line(Point::new(90.0, 220.0)),
                        Node::line(Point::new(20.0, 220.0)),
                    ],
                    closed: true,
                };
                let hole = NodePath::from_points(
                    &[
                        Point::new(40.0, 180.0),
                        Point::new(60.0, 180.0),
                        Point::new(60.0, 200.0),
                        Point::new(40.0, 200.0),
                    ],
                    true,
                );
                s.set_world_node_paths(&[outer, hole]);
                s.fill = Some(Color::rgb(40, 170, 90));
            },
        );
        doc.shapes.push(super::tests::image_shape(
            Rect::new(110.0, 190.0, 190.0, 238.0),
            0.0,
        ));
        doc.shapes.push(super::tests::image_shape(
            Rect::new(120.0, 245.0, 180.0, 281.0),
            -15.0,
        ));
        std::fs::write(&out, write(&doc)).unwrap();
        let back = crate::parse(&std::fs::read(&out).unwrap()).unwrap();
        for s in &back.shapes {
            eprintln!(
                "{} subtrazados, línea {:?}, relleno {:?}",
                s.paths.len(),
                s.line,
                s.fill
            );
        }
    }
}

#[cfg(test)]
mod rewrite {
    /// OPENCRAFTCUTTER_STUDIO3_REWRITE=entrada.studio3:salida.studio3: lee un documento y lo vuelve
    /// a escribir (para comprobar la exportación con archivos reales).
    #[test]
    fn rewrite_if_requested() {
        let Ok(arg) = std::env::var("OPENCRAFTCUTTER_STUDIO3_REWRITE") else {
            return;
        };
        let (inp, out) = arg.split_once(':').expect("entrada:salida");
        let parsed = crate::parse(&std::fs::read(inp).unwrap()).unwrap();
        let mut doc = oc_core::Document::default();
        for s in parsed.to_shapes(Default::default()) {
            doc.add_shape(s);
        }
        std::fs::write(out, super::write(&doc)).unwrap();
    }
}
