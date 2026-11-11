//! Genera versiones del logo con fondo transparente (tinta negra y blanca) para el README.
//! Uso: cargo run -p oc-trace --example logo_variants -- logo.png salida_dir ancho
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let img = image::open(&a[1]).unwrap().to_luma8();
    let w: u32 = a.get(3).and_then(|v| v.parse().ok()).unwrap_or(900);
    let h = (img.height() as f64 * w as f64 / img.width() as f64).round() as u32;
    let small = image::imageops::resize(&img, w, h, image::imageops::FilterType::Lanczos3);
    for (name, ink) in [("logo-light.png", 20u8), ("logo-dark.png", 240u8)] {
        let mut out = image::RgbaImage::new(w, h);
        for (x, y, p) in small.enumerate_pixels() {
            let alpha = 255 - p.0[0];
            out.put_pixel(x, y, image::Rgba([ink, ink, ink, alpha]));
        }
        out.save(std::path::Path::new(&a[2]).join(name)).unwrap();
    }
}
