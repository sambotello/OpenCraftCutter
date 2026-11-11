//! OpenCraftCutter: software libre de diseño y corte para cortadoras de manualidades.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
#[cfg(test)]
mod app_tests;
mod canvas;
mod edit_menu;
mod file_drop;
mod file_menu;
#[cfg(test)]
mod flow_tests;
mod insert;
mod machine;
mod object_menu;
mod panels;
mod parts;
mod prefs;
mod shell;
mod text;
mod theme;

const ICON_PNG: &[u8] = include_bytes!("../assets/logo-icon.png");

/// Icono con el estilo de macOS: «squircle» (superelipse) sobre la rejilla de Apple
/// (1024 px con 100 px de margen), fondo claro con degradado suave, sombra ligera y el logo
/// (negro sobre blanco) centrado.
fn app_icon_rgba(logo_png: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    const SIZE: u32 = 1024;
    const MARGIN: f32 = 100.0;
    let logo = image::load_from_memory(logo_png).ok()?.to_luma8();
    let body = SIZE as f32 - 2.0 * MARGIN;
    let half = body / 2.0;
    let c = SIZE as f32 / 2.0;
    // Superelipse de Apple (n ≈ 5): |x|^n + |y|^n <= 1.
    let inside = |x: f32, y: f32, grow: f32| {
        let (u, v) = (
            ((x - c) / (half + grow)).abs(),
            ((y - c) / (half + grow)).abs(),
        );
        u.powf(5.0) + v.powf(5.0) <= 1.0
    };
    // Logo a ~64 % del cuerpo, muestreado por luminancia (tinta = oscuro).
    let logo_side = body * 0.64;
    let (lw, lh) = (logo.width() as f32, logo.height() as f32);
    let k = logo_side / lw.max(lh);
    let (ox, oy) = (c - lw * k / 2.0, c - lh * k / 2.0);
    let ink_at = |x: f32, y: f32| -> f32 {
        let (u, v) = ((x - ox) / k, (y - oy) / k);
        if u < 0.0 || v < 0.0 || u >= lw - 1.0 || v >= lh - 1.0 {
            return 0.0;
        }
        let (x0, y0) = (u.floor() as u32, v.floor() as u32);
        let (fx, fy) = (u - x0 as f32, v - y0 as f32);
        let p = |a, b| logo.get_pixel(a, b).0[0] as f32;
        let top = p(x0, y0) * (1.0 - fx) + p(x0 + 1, y0) * fx;
        let bot = p(x0, y0 + 1) * (1.0 - fx) + p(x0 + 1, y0 + 1) * fx;
        1.0 - (top * (1.0 - fy) + bot * fy) / 255.0
    };
    let mut out = vec![0u8; (SIZE * SIZE * 4) as usize];
    let samples = [0.25f32, 0.75];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (mut cover, mut shadow) = (0.0f32, 0.0f32);
            for sy in samples {
                for sx in samples {
                    let (px, py) = (x as f32 + sx, y as f32 + sy);
                    if inside(px, py, 0.0) {
                        cover += 0.25;
                    }
                    // Sombra: la misma forma desplazada hacia abajo y algo mayor.
                    if inside(px, py - 12.0, 6.0) {
                        shadow += 0.25;
                    }
                }
            }
            let i = ((y * SIZE + x) * 4) as usize;
            // Fondo: blanco arriba a gris muy claro abajo.
            let t = ((y as f32 - MARGIN) / body).clamp(0.0, 1.0);
            let bg = 255.0 - 22.0 * t;
            let ink = ink_at(x as f32 + 0.5, y as f32 + 0.5);
            let ink_col = 28.0;
            let col = bg * (1.0 - ink) + ink_col * ink;
            let sh_a = (shadow - cover).max(0.0) * 0.28;
            let a = cover + sh_a * (1.0 - cover);
            if a <= 0.0 {
                continue;
            }
            let rgb = (col * cover) / a; // la sombra es negra
            out[i] = rgb.round() as u8;
            out[i + 1] = rgb.round() as u8;
            out[i + 2] = rgb.round() as u8;
            out[i + 3] = (a * 255.0).round() as u8;
        }
    }
    Some((out, SIZE, SIZE))
}

fn window_icon() -> Option<egui::IconData> {
    let (rgba, width, height) = app_icon_rgba(ICON_PNG)?;
    Some(egui::IconData {
        rgba,
        width,
        height,
    })
}

fn main() -> eframe::Result {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    // Empaquetado: `opencraftcutter --export-icon icono.png` guarda el icono de la app (1024 px).
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, path] = args.as_slice() {
        if flag == "--export-icon" {
            let (rgba, w, h) = app_icon_rgba(ICON_PNG).expect("icono");
            image::RgbaImage::from_raw(w, h, rgba)
                .expect("tamaño")
                .save(path)
                .expect("guardar el icono");
            return Ok(());
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OpenCraftCutter")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1024.0, 700.0])
            .with_app_id("com.sambotello.opencraftcutter")
            .with_drag_and_drop(true)
            .with_icon(window_icon().unwrap_or_default()),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "OpenCraftCutter",
        options,
        Box::new(|cc| Ok(Box::new(app::OpenCraftCutterApp::new(cc)))),
    )
}

#[cfg(test)]
mod icon_tests {
    #[test]
    fn icon_is_rounded_with_transparent_corners() {
        let (rgba, w, h) = super::app_icon_rgba(super::ICON_PNG).unwrap();
        assert_eq!((w, h), (1024, 1024));
        let a = |x: u32, y: u32| rgba[((y * w + x) * 4 + 3) as usize];
        // Esquinas y margen transparentes; centro opaco.
        assert_eq!(a(0, 0), 0);
        assert_eq!(a(110, 110), 0);
        assert_eq!(a(512, 512), 255);
        if let Ok(p) = std::env::var("OC_ICON_DUMP") {
            image::save_buffer(p, &rgba, w, h, image::ColorType::Rgba8).unwrap();
        }
    }
}
