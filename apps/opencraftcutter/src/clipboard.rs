//! Lectura del portapapeles del sistema más allá del texto: SVG de programas vectoriales,
//! imágenes (por ejemplo, «Copiar imagen» en un navegador) y archivos copiados en el Finder.

use std::path::PathBuf;

/// Contenido del portapapeles que se puede pegar en el lienzo.
#[derive(Debug, Clone, PartialEq)]
pub enum Pasted {
    /// Documento SVG (vectores).
    Svg(String),
    /// Imagen codificada (PNG, TIFF, JPEG…).
    Image(Vec<u8>),
    /// Archivos copiados en el Finder.
    Files(Vec<PathBuf>),
    /// Vectores en PDF (lo que copian por defecto muchos programas vectoriales). `image` es la
    /// imagen que venía junto al PDF, si la había: se usa si el PDF no trae trazados.
    Pdf {
        pdf: Vec<u8>,
        image: Option<Vec<u8>>,
    },
}

/// Tipos de portapapeles con SVG, por orden de preferencia.
#[cfg(target_os = "macos")]
const SVG_TYPES: [&str; 3] = ["public.svg-image", "image/svg+xml", "com.inkscape.svg"];
/// Tipos de imagen: primero sin pérdida.
#[cfg(target_os = "macos")]
const IMAGE_TYPES: [&str; 6] = [
    "public.png",
    "public.tiff",
    "public.jpeg",
    "org.webmproject.webp",
    "com.compuserve.gif",
    "com.microsoft.bmp",
];
#[cfg(target_os = "macos")]
const PDF_TYPES: [&str; 2] = ["com.adobe.pdf", "public.pdf"];

/// ¿Parece SVG este texto?
pub fn looks_like_svg(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("<svg") || (t.starts_with("<?xml") && t.contains("<svg")) || t.contains("<svg ")
}

/// Ruta local de una URL `file://` (con sus `%xx` decodificados).
pub fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.trim().strip_prefix("file://")?;
    // Tras el esquema puede venir un host («localhost») antes de la ruta.
    let path = &rest[rest.find('/')?..];
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%')
            .then(|| path.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(b) => {
                out.push(b);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok().map(PathBuf::from)
}

/// Lo que hay en el portapapeles del sistema que se puede pegar, o `None`. Preferencia: SVG
/// antes que archivos, estos antes que PDF, y los vectores antes que un mapa de bits.
#[cfg(target_os = "macos")]
pub fn read_system() -> Option<Pasted> {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;
    let pb = NSPasteboard::generalPasteboard();
    let data = |ty: &str| {
        pb.dataForType(&NSString::from_str(ty))
            .map(|d| d.to_vec())
            .filter(|d| !d.is_empty())
    };
    for ty in SVG_TYPES {
        if let Some(text) = data(ty).and_then(|d| String::from_utf8(d).ok()) {
            if looks_like_svg(&text) {
                return Some(Pasted::Svg(text));
            }
        }
    }
    let file_url = NSString::from_str("public.file-url");
    let files: Vec<PathBuf> = pb
        .pasteboardItems()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.stringForType(&file_url))
                .filter_map(|url| file_url_to_path(&url.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if !files.is_empty() {
        return Some(Pasted::Files(files));
    }
    let image = IMAGE_TYPES.iter().find_map(|ty| data(ty));
    if let Some(pdf) = PDF_TYPES.iter().find_map(|ty| data(ty)) {
        return Some(Pasted::Pdf { pdf, image });
    }
    image.map(Pasted::Image)
}

#[cfg(not(target_os = "macos"))]
pub fn read_system() -> Option<Pasted> {
    None
}

/// ¿Hay en el portapapeles del sistema algo de lo que lee [`read_system`]? Solo mira los
/// tipos, sin copiar los datos (para habilitar «Pegar» en los menús).
#[cfg(target_os = "macos")]
pub fn has_system_content() -> bool {
    use objc2_app_kit::NSPasteboard;
    let Some(types) = NSPasteboard::generalPasteboard().types() else {
        return false;
    };
    types.iter().any(|t| {
        let t = t.to_string();
        t == "public.file-url"
            || SVG_TYPES.contains(&t.as_str())
            || IMAGE_TYPES.contains(&t.as_str())
            || PDF_TYPES.contains(&t.as_str())
    })
}

#[cfg(not(target_os = "macos"))]
pub fn has_system_content() -> bool {
    false
}

/// Como [`read_system`]. En las pruebas no se toca el portapapeles real del equipo.
pub fn read() -> Option<Pasted> {
    if cfg!(test) { None } else { read_system() }
}

/// Como [`has_system_content`]. En las pruebas no se toca el portapapeles real del equipo.
pub fn has_content() -> bool {
    !cfg!(test) && has_system_content()
}

/// Texto plano del portapapeles del sistema.
#[cfg(target_os = "macos")]
pub fn read_text() -> Option<String> {
    if cfg!(test) {
        return None;
    }
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;
    NSPasteboard::generalPasteboard()
        .stringForType(&NSString::from_str("public.utf8-plain-text"))
        .map(|s| s.to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn read_text() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_svg_text() {
        assert!(looks_like_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"
        ));
        assert!(looks_like_svg(
            "  <?xml version=\"1.0\"?>\n<!-- x -->\n<svg width=\"1\"></svg>"
        ));
        assert!(!looks_like_svg("https://example.com/foto.png"));
        assert!(!looks_like_svg("hola"));
    }

    #[test]
    fn decodes_file_urls() {
        assert_eq!(
            file_url_to_path("file:///Users/yo/Mis%20dise%C3%B1os/caja.svg"),
            Some(PathBuf::from("/Users/yo/Mis diseños/caja.svg"))
        );
        assert_eq!(
            file_url_to_path("file://localhost/tmp/a.png"),
            Some(PathBuf::from("/tmp/a.png"))
        );
        assert_eq!(file_url_to_path("https://example.com/a.png"), None);
    }
}
