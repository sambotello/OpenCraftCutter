//! «Fotografías» de lo que se envía a la máquina. Si un cambio altera los comandos GPGL, la
//! prueba falla y muestra la diferencia; si el cambio es intencionado, se actualizan con
//! `OPENCRAFTCUTTER_UPDATE_GOLDEN=1 cargo test -p oc-cutter --test golden_gpgl` y se revisa el diff.

use kurbo::{Point, Rect};
use oc_core::send::*;
use oc_core::{Color, Document, Shape, ShapeKind};
use oc_cutter::gpgl::{MatCode, Media, encode};

fn golden(name: &str, actual: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var("OPENCRAFTCUTTER_UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "falta {}: créala con OPENCRAFTCUTTER_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    if expected != actual {
        let diff: Vec<String> = expected
            .lines()
            .zip(actual.lines())
            .enumerate()
            .filter(|(_, (e, a))| e != a)
            .take(10)
            .map(|(i, (e, a))| format!("línea {}:\n  antes: {e}\n  ahora: {a}", i + 1))
            .collect();
        panic!(
            "{name} cambió ({} → {} líneas). Si es intencionado, actualiza con \
             OPENCRAFTCUTTER_UPDATE_GOLDEN=1.\n{}",
            expected.lines().count(),
            actual.lines().count(),
            diff.join("\n")
        );
    }
}

/// Segmentos como texto: un comando por línea (separados por ETX en el flujo real).
fn as_text(segments: &[Vec<u8>]) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        out.push_str(&format!("# segmento {i}\n"));
        for c in seg.split(|b| *b == 0x03).filter(|c| !c.is_empty()) {
            out.push_str(&String::from_utf8_lossy(c));
            out.push('\n');
        }
    }
    out
}

fn media() -> Media {
    Media {
        mat: MatCode::Cameo12x12,
        width_mm: 304.8,
        height_mm: 304.8,
        feed: FeedOption::ReturnToOrigin,
        registration: None,
    }
}

fn setups() -> [HolderSetup; 2] {
    [
        HolderSetup {
            material: DEFAULT_MATERIAL.into(),
            action: Action::Cut,
            tool: Tool::AutoBlade,
        },
        HolderSetup {
            material: DEFAULT_MATERIAL.into(),
            action: Action::Auto,
            tool: Tool::SketchPen,
        },
    ]
}

#[test]
fn test_shape_with_default_material() {
    let mut doc = Document::default();
    for s in test_shapes(Point::new(8.0, 8.0)) {
        doc.add_shape(s);
    }
    let mats = builtin_materials();
    let jobs = standard_jobs(&doc, &setups(), &mats, &SendOptions::default());
    golden("forma_de_prueba.gpgl", &as_text(&encode(&media(), &jobs)));
}

#[test]
fn two_colors_with_own_settings_and_pause() {
    let mut doc = Document::default();
    let mut red = Shape::new(
        0,
        "Interior",
        Rect::new(20.0, 20.0, 50.0, 40.0),
        ShapeKind::Ellipse,
    );
    red.stroke.color = Color::rgb(255, 0, 0);
    let mut blue = Shape::new(
        0,
        "Contorno",
        Rect::new(15.0, 15.0, 55.0, 45.0),
        ShapeKind::Rect { corner_radius: 4.0 },
    );
    blue.stroke.color = Color::rgb(0, 0, 255);
    doc.add_shape(red);
    doc.add_shape(blue);
    let mats = builtin_materials();
    let vinyl = mats
        .iter()
        .find(|m| m.name == "Vinilo, imprimible")
        .unwrap();
    let base = *vinyl.settings(Holder::One, Action::Cut).unwrap();
    let row = |depth, pause_after| ColorRow {
        holder: Holder::One,
        enabled: true,
        edge_only: false,
        material: vinyl.name.clone(),
        action: Action::Cut,
        tool: Tool::AutoBlade,
        pause_after,
        settings: Some(ActionSettings { depth, ..base }),
    };
    let rows = [
        (Color::rgb(255, 0, 0), row(2, true)),
        (Color::rgb(0, 0, 255), row(4, false)),
    ];
    let jobs = color_jobs(&doc, false, &rows, &mats, &SendOptions::default());
    let mut m = media();
    m.feed = FeedOption::FeedOut;
    golden("dos_colores.gpgl", &as_text(&encode(&m, &jobs)));
}

#[test]
fn print_and_cut_reads_the_marks_first() {
    let mut doc = Document::default();
    doc.add_shape(Shape::new(
        0,
        "Pegatina",
        Rect::new(30.0, 40.0, 60.0, 70.0),
        ShapeKind::Rect { corner_radius: 0.0 },
    ));
    let mats = builtin_materials();
    let jobs = standard_jobs(&doc, &setups(), &mats, &SendOptions::default());
    let mut m = media();
    m.registration = Some(oc_cutter::gpgl::Registration {
        origin: Point::new(10.0, 10.0),
        width_mm: 195.9,
        height_mm: 259.4,
        length_mm: 20.0,
        thickness_mm: 0.5,
    });
    let segs = encode(&m, &jobs);
    assert_eq!(oc_cutter::gpgl::registration_segment(&m), Some(0));
    assert_eq!(oc_cutter::gpgl::header_segments(&m), 2);
    assert_eq!(segs.len(), 2 + jobs.len() + 1);
    golden("imprimir_y_cortar.gpgl", &as_text(&segs));
}
