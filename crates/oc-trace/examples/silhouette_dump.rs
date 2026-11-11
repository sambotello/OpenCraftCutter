//! Desplaza la silueta de una imagen y la dibuja encima (para comparar con la referencia).
//! Uso: cargo run -p oc-trace --example silhouette_dump -- entrada.png salida.png distancia_mm
use kurbo::{Point, Rect};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let png = std::fs::read(&a[1]).unwrap();
    let d: f64 = a.get(3).and_then(|v| v.parse().ok()).unwrap_or(1.7);
    let img = image::load_from_memory(&png).unwrap().to_rgba8();
    let (w, h) = img.dimensions();
    // 1 px de salida = 0,1 mm; la imagen mide 75,67 mm de ancho.
    let mm_w: f64 = std::env::var("MMW")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(75.67);
    let mm_h = mm_w * h as f64 / w as f64;
    let target = Rect::new(5.0, 5.0, 5.0 + mm_w, 5.0 + mm_h);
    let loops = oc_trace::image_silhouette(&png, target).unwrap();
    let world: Vec<(Vec<Point>, bool)> = loops.into_iter().map(|l| (l, true)).collect();
    let (rect, subpaths) = oc_core::Shape::normalize_subpaths(&world);
    let s = oc_core::Shape::new(
        0,
        "s",
        rect,
        oc_core::ShapeKind::Path {
            subpaths,
            fill_rule: oc_core::FillRule::EvenOdd,
        },
    );
    let off = oc_core::ops::offset(&s, d, oc_core::ops::Join::Round).unwrap();
    let k = 10.0;
    let (ow, oh) = (((mm_w + 10.0) * k) as u32, ((mm_h + 10.0) * k) as u32);
    let mut out = image::RgbaImage::from_pixel(ow, oh, image::Rgba([255, 255, 255, 255]));
    let small = image::imageops::resize(
        &img,
        (mm_w * k) as u32,
        (mm_h * k) as u32,
        image::imageops::FilterType::Triangle,
    );
    image::imageops::overlay(&mut out, &small, (5.0 * k) as i64, (5.0 * k) as i64);
    for (pts, _) in off.world_subpaths() {
        let n = pts.len();
        for i in 0..n {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            let steps = ((q - p).hypot() * k * 2.0).ceil().max(1.0) as i32;
            for j in 0..=steps {
                let t = j as f64 / steps as f64;
                let (x, y) = ((p.x + (q.x - p.x) * t) * k, (p.y + (q.y - p.y) * t) * k);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (xx, yy) = (x as i64 + dx, y as i64 + dy);
                    if xx >= 0 && yy >= 0 && (xx as u32) < ow && (yy as u32) < oh {
                        out.put_pixel(xx as u32, yy as u32, image::Rgba([220, 30, 30, 255]));
                    }
                }
            }
        }
    }
    println!("contornos: {}", off.world_subpaths().len());
    out.save(&a[2]).unwrap();
}
