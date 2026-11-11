//! Muestra el canal alfa de una imagen: transparente en magenta, opaco tal cual.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let img = image::open(&a[1]).unwrap().to_rgba8();
    let mut out = img.clone();
    for p in out.pixels_mut() {
        let al = p.0[3] as u32;
        let c = |v: u8, m: u32| ((v as u32 * al + m * (255 - al)) / 255) as u8;
        p.0 = [c(p.0[0], 255), c(p.0[1], 0), c(p.0[2], 255), 255];
    }
    let big = image::imageops::resize(
        &out,
        out.width() * 4,
        out.height() * 4,
        image::imageops::FilterType::Nearest,
    );
    big.save(&a[2]).unwrap();
}
