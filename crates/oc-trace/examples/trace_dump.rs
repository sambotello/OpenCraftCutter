//! Traza una imagen y dibuja el resultado sobre ella (para comparar calidad).
//! Uso: cargo run -p oc-trace --example trace_dump -- entrada.png salida.png
use kurbo::Rect;
use oc_core::shape::Stroke;
use oc_trace::{TraceSettings, trace};

fn line(img: &mut image::RgbaImage, a: (f64, f64), b: (f64, f64), c: [u8; 4]) {
    let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) * 2.0)
        .ceil()
        .max(1.0) as i32;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let (px, py) = (x as i64 + dx, y as i64 + dy);
            if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                img.put_pixel(px as u32, py as u32, image::Rgba(c));
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let png = std::fs::read(&args[1]).unwrap();
    let src = image::load_from_memory(&png).unwrap().to_rgba8();
    let (w, h) = src.dimensions();
    let k: f64 = std::env::var("K")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2.0);
    let mut out = image::imageops::resize(
        &src,
        w * k as u32,
        h * k as u32,
        image::imageops::FilterType::Nearest,
    );
    for p in out.pixels_mut() {
        // Imagen original atenuada de fondo.
        for c in 0..3 {
            p.0[c] = 255 - (255 - p.0[c]) / 4;
        }
        p.0[3] = 255;
    }
    // 1 px = 1 mm para leer coordenadas directamente.
    let target = Rect::new(0.0, 0.0, w as f64, h as f64);
    let env = |k: &str, d: f64| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let st = TraceSettings {
        filter_speckle: 2,
        scale: env("SCALE", 1.0) as u32,
        quality: env("QUALITY", 3.0) as u8,
        smooth: env("SMOOTH", 1.0) > 0.0,
        corner_threshold: env("CORNER", 60.0) as i32,
        threshold: (env("THRESHOLD", 50.0) * 2.55).round() as u8,
        area: std::env::var("AREA").ok().map(|v| {
            let n: Vec<f64> = v.split(',').map(|x| x.trim().parse().unwrap()).collect();
            [n[0], n[1], n[2], n[3]]
        }),
        ..Default::default()
    };
    let shapes = trace(&png, target, &st, Stroke::default()).unwrap();
    let mut total = 0;
    for s in &shapes {
        for (pts, closed) in s.world_subpaths() {
            total += pts.len();
            println!("subtrazo: {} puntos, cerrado={closed}", pts.len());
            let m = |p: kurbo::Point| (p.x * k, p.y * k);
            for win in pts.windows(2) {
                line(&mut out, m(win[0]), m(win[1]), [220, 30, 30, 255]);
            }
            if closed && pts.len() > 1 {
                line(
                    &mut out,
                    m(*pts.last().unwrap()),
                    m(pts[0]),
                    [220, 30, 30, 255],
                );
            }
        }
    }
    println!("formas: {}, puntos totales: {total}", shapes.len());
    out.save(&args[2]).unwrap();
}
