//! Genera una «foto» sintética de un tapete en perspectiva con un dibujo, para probar PixScan.
//! Uso: cargo run -p oc-trace --example pixscan_photo -- salida.png
use image::{Rgba, RgbaImage};
use kurbo::Point;
use oc_trace::pixscan::{apply_h, homography};

fn main() {
    let out = std::env::args().nth(1).expect("ruta de salida");
    let (w, h) = (3200u32, 2400u32);
    let (mw, mh) = (297.0, 216.0);
    let mat = [
        Point::new(0.0, 0.0),
        Point::new(mw, 0.0),
        Point::new(mw, mh),
        Point::new(0.0, mh),
    ];
    let photo = [
        Point::new(420.0, 380.0),
        Point::new(2850.0, 260.0),
        Point::new(2980.0, 2150.0),
        Point::new(300.0, 2050.0),
    ];
    let inv = homography(&photo, &mat).unwrap();
    let mut img = RgbaImage::from_pixel(w, h, Rgba([96, 92, 86, 255]));
    for (x, y, p) in img.enumerate_pixels_mut() {
        let m = apply_h(&inv, Point::new(x as f64 + 0.5, y as f64 + 0.5));
        if m.x < -8.0 || m.y < -8.0 || m.x > mw + 8.0 || m.y > mh + 8.0 {
            continue;
        }
        let mut c = [236u8, 236, 230];
        // Cuadrícula cada 25,4 mm.
        let gx = (m.x / 25.4).fract().abs();
        let gy = (m.y / 25.4).fract().abs();
        if gx < 0.012 || gy < 0.012 {
            c = [190, 190, 190];
        }
        // Corazón rojo centrado de unos 100 mm.
        let (u, v) = ((m.x - mw / 2.0) / 50.0, -(m.y - mh / 2.0) / 50.0 + 0.2);
        let heart = (u * u + v * v - 1.0).powi(3) - u * u * v * v * v;
        if heart <= 0.0 {
            c = [210, 30, 40];
        }
        // Estrella azul arriba a la izquierda.
        if (m.x - 40.0).hypot(m.y - 40.0) < 18.0 {
            c = [30, 80, 200];
        }
        *p = Rgba([c[0], c[1], c[2], 255]);
    }
    img.save(&out).unwrap();
}
