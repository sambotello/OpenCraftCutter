//! Pruebas de flujo completo: del documento a lo que se envía a la máquina, y de la interfaz
//! con clics simulados (egui_kittest). Cubren lo que las pruebas de cada módulo no ven: cómo se
//! conectan las piezas (texto → corte, panel → clics, ajustes de un color → trabajo).

use crate::app::OpenCraftCutterApp;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use kurbo::Rect;
use oc_core::send::{SendMode, TEST_SHAPE_NAME};
use oc_core::{Color, Shape, ShapeKind, TextAlign};

/// App de prueba sin ventana ni preferencias guardadas.
pub(crate) fn app() -> OpenCraftCutterApp {
    let ctx = egui::Context::default();
    let mut app = OpenCraftCutterApp::new(&eframe::CreationContext::_new_kittest(ctx));
    app.doc = oc_core::Document::default();
    app
}

pub(crate) fn rect(x: f64, color: Color) -> Shape {
    let mut s = Shape::new(
        0,
        "Rectángulo",
        Rect::new(x, 20.0, x + 30.0, 50.0),
        ShapeKind::Rect { corner_radius: 0.0 },
    );
    s.stroke.color = color;
    s
}

pub(crate) fn text(t: &str) -> Shape {
    let kind = ShapeKind::Text {
        text: t.into(),
        font_family: crate::text::DEFAULT_FONT_FAMILY.into(),
        font_size_pt: 36.0,
        align: TextAlign::Left,
        bold: false,
        italic: false,
        line_height: 1.2,
        fx: Default::default(),
    };
    let (w, h) = crate::text::TextStyle::from_kind(&kind)
        .map(|st| crate::text::measure(&st))
        .expect("se mide");
    Shape::new(0, "Texto", Rect::new(20.0, 80.0, 20.0 + w, 80.0 + h), kind)
}

pub(crate) fn image() -> Shape {
    let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).unwrap();
    Shape::new(
        0,
        "Imagen",
        Rect::new(10.0, 10.0, 40.0, 40.0),
        ShapeKind::Image {
            png: oc_core::shape::ImageData(std::sync::Arc::new(png.into_inner())),
            px_width: 8,
            px_height: 8,
        },
    )
}

pub(crate) const RED: Color = Color::rgb(0xe3, 0x34, 0x3f);
pub(crate) const BLUE: Color = Color::rgb(0x20, 0x30, 0xdf);

// ---------------------------------------------------------------- Flujo de envío

#[test]
fn texts_are_cut_in_every_mode() {
    let mut a = app();
    a.doc.add_shape(text("Hola"));
    for mode in [SendMode::Standard, SendMode::ByLine, SendMode::ByFill] {
        a.canvas.send.mode = mode;
        a.sync_rows();
        let jobs = a.send_jobs();
        assert!(
            jobs.iter().any(|j| j.paths.len() >= 4),
            "{mode:?}: el texto da al menos un contorno por letra"
        );
        assert!(a.nothing_to_cut().is_none(), "{mode:?}");
    }
}

#[test]
fn nothing_to_cut_explains_why() {
    let mut a = app();
    let empty = a.nothing_to_cut().expect("documento vacío");
    assert!(empty.contains("vacío"), "{empty}");

    a.doc.add_shape(image());
    let only_images = a.nothing_to_cut().expect("solo imágenes");
    assert!(only_images.contains("imágenes"), "{only_images}");

    a.doc.add_shape(rect(60.0, RED));
    assert!(a.nothing_to_cut().is_none());

    // Todas las líneas en Off.
    a.canvas.send.mode = SendMode::ByLine;
    a.sync_rows();
    for (_, row) in &mut a.canvas.send.line_rows {
        row.enabled = false;
    }
    let off = a.nothing_to_cut().expect("todo en Off");
    assert!(off.contains("Off"), "{off}");
}

#[test]
fn test_shape_alone_is_not_a_design() {
    let mut a = app();
    for s in oc_core::send::test_shapes(kurbo::Point::new(8.0, 8.0)) {
        a.doc.add_shape(s);
    }
    assert!(a.doc.shapes.iter().all(|s| s.name == TEST_SHAPE_NAME));
    assert!(
        a.nothing_to_cut().is_some(),
        "la forma de prueba no cuenta como diseño"
    );
}

#[test]
fn each_color_uses_its_own_settings() {
    let mut a = app();
    a.doc.add_shape(rect(10.0, RED));
    a.doc.add_shape(rect(60.0, BLUE));
    a.canvas.send.mode = SendMode::ByLine;
    a.sync_rows();
    let rows = &mut a.canvas.send.line_rows;
    assert_eq!(rows.len(), 2);
    let material = rows[0].1.material.clone();
    let base = a
        .prefs
        .materials
        .iter()
        .chain(oc_core::send::builtin_materials().iter())
        .find(|m| m.name == material)
        .and_then(|m| {
            m.settings(oc_core::send::Holder::One, oc_core::send::Action::Cut)
                .copied()
        })
        .expect("material con corte");
    let blue = a
        .canvas
        .send
        .line_rows
        .iter_mut()
        .find(|(c, _)| *c == BLUE)
        .expect("fila azul");
    blue.1.settings = Some(oc_core::send::ActionSettings {
        depth: base.depth + 1,
        force: base.force + 5,
        ..base
    });
    let jobs = a.send_jobs();
    assert_eq!(jobs.len(), 2);
    let depths: Vec<u32> = jobs.iter().map(|j| j.settings.depth).collect();
    assert!(
        depths.contains(&base.depth) && depths.contains(&(base.depth + 1)),
        "{depths:?}"
    );
    // El material no cambia.
    let after = a
        .prefs
        .materials
        .iter()
        .find(|m| m.name == material)
        .and_then(|m| {
            m.settings(oc_core::send::Holder::One, oc_core::send::Action::Cut)
                .copied()
        });
    assert!(after.is_none_or(|s| s == base));
}

#[test]
fn skipped_lines_are_not_sent() {
    let mut a = app();
    let mut s = rect(10.0, RED);
    s.cut.style = oc_core::send::CutStyle::Skip;
    a.doc.add_shape(s);
    a.canvas.send.mode = SendMode::Standard;
    assert!(a.send_jobs().is_empty());
    assert!(a.nothing_to_cut().is_some());
}

// ---------------------------------------------------------------- Interfaz

/// Panel Enviar en un arnés de prueba.
pub(crate) fn send_harness(a: OpenCraftCutterApp) -> Harness<'static, OpenCraftCutterApp> {
    Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        // Iconos y tema como en la app; los símbolos sin glifo (p. ej., flechas en textos de
        // ayuda) no hacen fallar la prueba.
        .with_context_setup(|ctx| {
            egui_material_icons::initialize(ctx);
            crate::theme::apply(ctx);
        })
        .allow_missing_glyphs()
        // Varios botones de icono aún no tienen nombre accesible (pendiente aparte).
        .with_accessibility_check(false)
        .build_ui_state(
            |ui, app: &mut OpenCraftCutterApp| crate::shell::send_panel(app, ui),
            a,
        )
}

#[test]
fn color_row_material_dropdown_opens() {
    let mut a = app();
    a.doc.add_shape(rect(10.0, RED));
    a.doc.add_shape(rect(60.0, BLUE));
    a.canvas.send.mode = SendMode::ByLine;
    let mut h = send_harness(a);
    h.run();
    let material = h.state().canvas.send.line_rows[0].1.material.clone();
    // El desplegable no tiene nombre accesible: se busca por su valor (el material elegido).
    let combo = h
        .get_all_by_role(egui::accesskit::Role::ComboBox)
        .find(|n| n.accesskit_node().value().as_deref() == Some(material.as_str()))
        .expect("desplegable de material de la primera fila");
    combo.click();
    // Al abrirse, la búsqueda recibe el foco y su cursor parpadea (la interfaz no se queda
    // quieta): se avanzan unos fotogramas en lugar de esperar a que se asiente.
    h.run_steps(4);
    assert!(
        h.query_by_label("Todos los materiales").is_some(),
        "el desplegable de material se abre al hacer clic"
    );
}

#[test]
fn clicking_a_color_card_shows_its_settings() {
    let mut a = app();
    a.doc.add_shape(rect(10.0, RED));
    a.canvas.send.mode = SendMode::ByLine;
    let mut h = send_harness(a);
    h.run();
    assert!(h.query_by_label("Modificar la configuración").is_none());
    h.get_by_label("#e3343f").click();
    h.run();
    assert_eq!(h.state().canvas.send.selected_row, Some(0));
    assert!(h.query_by_label("Modificar la configuración").is_some());
}

#[test]
fn send_is_disabled_with_a_reason_when_nothing_to_cut() {
    let a = app();
    let mut h = send_harness(a);
    h.run();
    assert!(
        h.query_by_label_contains("No hay nada que cortar")
            .is_some(),
        "aviso visible con el documento vacío"
    );
    let send = h.get_by_label("ENVIAR");
    assert!(send.accesskit_node().is_disabled(), "ENVIAR desactivado");
}

// ---------------------------------------------------------------- Lienzo (app completa)

/// App completa (barras, paneles y lienzo) en un arnés, sobre un documento vacío.
pub(crate) fn app_harness() -> Harness<'static, OpenCraftCutterApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .with_context_setup(|ctx| {
            egui_material_icons::initialize(ctx);
            crate::theme::apply(ctx);
        })
        .allow_missing_glyphs()
        .with_accessibility_check(false)
        .build_eframe(|cc| {
            let mut a = OpenCraftCutterApp::new(cc);
            a.doc = oc_core::Document::default();
            // Sin rejilla ni ajuste inteligente: las medidas no se redondean.
            a.doc.page.snap_to_grid = false;
            a.doc.page.smart_snap = false;
            a
        });
    h.run_steps(3);
    h
}

/// Arrastra en el lienzo entre dos puntos del documento (mm), con modificadores opcionales.
pub(crate) fn drag_mm(
    h: &mut Harness<'static, OpenCraftCutterApp>,
    from: (f64, f64),
    to: (f64, f64),
    mods: egui::Modifiers,
) {
    let vp = h.state().canvas.viewport;
    let a = vp.to_screen(kurbo::Point::new(from.0, from.1));
    let b = vp.to_screen(kurbo::Point::new(to.0, to.1));
    h.event(egui::Event::ModifiersChanged(mods));
    h.hover_at(a);
    h.step();
    h.event(egui::Event::PointerButton {
        pos: a,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: mods,
    });
    h.step();
    for k in 1..=8 {
        let t = k as f32 / 8.0;
        h.hover_at(a + (b - a) * t);
        h.step();
    }
    h.event(egui::Event::PointerButton {
        pos: b,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: mods,
    });
    h.step();
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run_steps(2);
}

#[test]
fn rectangle_tool_draws_the_dragged_box() {
    let mut h = app_harness();
    h.state_mut().tool = crate::app::Tool::Rect;
    h.run_steps(1);
    drag_mm(&mut h, (40.0, 40.0), (100.0, 80.0), egui::Modifiers::NONE);
    let shapes = &h.state().doc.shapes;
    assert_eq!(shapes.len(), 1, "se crea un rectángulo");
    let r = shapes[0].rect;
    assert!(
        (r.width() - 60.0).abs() < 1.0 && (r.height() - 40.0).abs() < 1.0,
        "{r:?}"
    );
    assert!(
        (r.x0 - 40.0).abs() < 1.0 && (r.y0 - 40.0).abs() < 1.0,
        "{r:?}"
    );
}

#[test]
fn shift_draws_a_square() {
    let mut h = app_harness();
    h.state_mut().tool = crate::app::Tool::Rect;
    h.run_steps(1);
    drag_mm(&mut h, (40.0, 40.0), (100.0, 70.0), egui::Modifiers::SHIFT);
    let r = h.state().doc.shapes[0].rect;
    assert!(
        (r.width() - r.height()).abs() < 1.0,
        "con Mayús es un cuadrado: {r:?}"
    );
}

#[test]
fn dragging_a_selected_shape_moves_it_and_undo_restores() {
    let mut h = app_harness();
    {
        let a = h.state_mut();
        a.doc.add_shape(rect(40.0, RED));
        a.tool = crate::app::Tool::Select;
    }
    h.run_steps(2);
    let before = h.state().doc.shapes[0].rect;
    let c = before.center();
    drag_mm(
        &mut h,
        (c.x, c.y),
        (c.x + 25.0, c.y + 10.0),
        egui::Modifiers::NONE,
    );
    let after = h.state().doc.shapes[0].rect;
    assert!(
        (after.x0 - before.x0 - 25.0).abs() < 1.0,
        "{before:?} → {after:?}"
    );
    assert!(
        (after.y0 - before.y0 - 10.0).abs() < 1.0,
        "{before:?} → {after:?}"
    );
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(2);
    let undone = h.state().doc.shapes[0].rect;
    assert!(
        (undone.x0 - before.x0).abs() < 1e-6,
        "⌘Z vuelve a la posición: {undone:?}"
    );
}

#[test]
fn delete_removes_the_selection() {
    let mut h = app_harness();
    {
        let a = h.state_mut();
        let id = a.doc.add_shape(rect(40.0, RED));
        a.doc.add_shape(rect(90.0, BLUE));
        a.doc.selected = vec![id];
    }
    h.run_steps(1);
    h.key_press(egui::Key::Backspace);
    h.run_steps(2);
    let shapes = &h.state().doc.shapes;
    assert_eq!(shapes.len(), 1);
    assert_eq!(shapes[0].stroke.color, BLUE);
}

#[test]
fn new_text_has_no_fill() {
    let mut h = app_harness();
    h.state_mut().tool = crate::app::Tool::Text;
    h.run_steps(1);
    let p = h
        .state()
        .canvas
        .viewport
        .to_screen(kurbo::Point::new(50.0, 50.0));
    h.hover_at(p);
    h.step();
    h.drag_at(p);
    h.step();
    h.drop_at(p);
    h.run_steps(3);
    let texts: Vec<&Shape> = h
        .state()
        .doc
        .shapes
        .iter()
        .filter(|s| s.is_text())
        .collect();
    assert_eq!(
        texts.len(),
        1,
        "un clic con la herramienta Texto crea un texto"
    );
    assert!(texts[0].fill.is_none(), "como la referencia, sin relleno");
}

// ---------------------------------------------------------------- .studio3

#[test]
fn opening_a_studio3_keeps_shapes_groups_texts_and_media() {
    let path = std::env::temp_dir().join(format!(
        "opencraftcutter-prueba-{}.studio3",
        std::process::id()
    ));
    std::fs::write(
        &path,
        include_bytes!("../../../crates/oc-studio3/assets/template.studio3"),
    )
    .unwrap();
    let mut a = app();
    a.open_path(&egui::Context::default(), &path);
    let _ = std::fs::remove_file(&path);
    let doc = &a.doc;
    assert_eq!(doc.page.media_size, "a4");
    let texts: Vec<&Shape> = doc.shapes.iter().filter(|s| s.is_text()).collect();
    assert_eq!(texts.len(), 1);
    if let ShapeKind::Text { text, .. } = &texts[0].kind {
        assert_eq!(text, "Hola");
    }
    let paths = doc.shapes.iter().filter(|s| !s.is_text()).count();
    assert_eq!(paths, 2);
    // Un grupo exterior que lo contiene todo y uno interior con una forma y el texto.
    let groups: std::collections::HashSet<u64> =
        doc.shapes.iter().filter_map(|s| s.group).collect();
    assert_eq!(groups.len(), 1, "un solo grupo exterior");
    let nested = doc
        .shapes
        .iter()
        .filter(|s| !s.inner_groups.is_empty())
        .count();
    assert_eq!(nested, 2, "dos objetos en el grupo interior");
}

#[test]
fn studio3_export_then_import_round_trip() {
    let mut a = app();
    let mut grouped = rect(10.0, RED);
    grouped.group = Some(1);
    let mut grouped2 = rect(60.0, BLUE);
    grouped2.group = Some(1);
    grouped2.fill = Some(Color::rgb(0, 200, 0));
    grouped2.fill_opacity = 0.5;
    a.doc.add_shape(grouped);
    a.doc.add_shape(grouped2);
    a.doc.add_shape(text("Hola"));
    let (bytes, skipped) = crate::actions::studio3_export(&a.doc);
    assert_eq!(skipped, 0);
    let path = std::env::temp_dir().join(format!(
        "opencraftcutter-ida-vuelta-{}.studio3",
        std::process::id()
    ));
    std::fs::write(&path, bytes).unwrap();
    let mut b = app();
    b.open_path(&egui::Context::default(), &path);
    let _ = std::fs::remove_file(&path);
    let shapes = &b.doc.shapes;
    assert_eq!(
        shapes.iter().filter(|s| s.is_text()).count(),
        1,
        "texto editable"
    );
    let filled: Vec<&Shape> = shapes
        .iter()
        .filter(|s| s.fill.is_some() && !s.is_text())
        .collect();
    assert_eq!(filled.len(), 1);
    assert!(
        (filled[0].fill_opacity - 0.5).abs() < 0.01,
        "transparencia del relleno"
    );
    let groups: std::collections::HashSet<u64> = shapes.iter().filter_map(|s| s.group).collect();
    assert_eq!(groups.len(), 1, "los dos rectángulos siguen agrupados");
}

// ---------------------------------------------------------------- Humo

/// Abre cada panel, cada herramienta y cada espacio de trabajo con un documento variado
/// (formas, texto, imagen, grupo) y con y sin selección: nada debe fallar.
#[test]
fn every_panel_tool_and_workspace_renders() {
    use crate::app::{Panel, Tool, Workspace};
    let mut h = app_harness();
    {
        let a = h.state_mut();
        let r = a.doc.add_shape(rect(20.0, RED));
        let mut g = rect(70.0, BLUE);
        g.group = Some(9);
        a.doc.add_shape(g);
        a.doc.add_shape(text("Prueba"));
        a.doc.add_shape(image());
        a.doc.selected = vec![r];
    }
    let panels = [
        Panel::PageSetup,
        Panel::Transform,
        Panel::Fill,
        Panel::Line,
        Panel::Text,
        Panel::PrintCut,
        Panel::Layers,
        Panel::Modify,
        Panel::Offset,
        Panel::Trace,
        Panel::Replicate,
        Panel::PixScan,
        Panel::SpecialTools,
        Panel::EditPoints,
        Panel::Rhinestone,
        Panel::Sketch,
        Panel::Warp,
        Panel::Emboss,
        Panel::Weeding,
    ];
    let all: Vec<u64> = h.state().doc.shapes.iter().map(|s| s.id).collect();
    for selection in [vec![], vec![all[0]], all.clone()] {
        for p in panels {
            h.state_mut().doc.selected = selection.clone();
            h.state_mut().panel = Some(p);
            h.run_steps(2);
        }
    }
    for t in [
        Tool::Select,
        Tool::EditPoints,
        Tool::Line,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Polygon,
        Tool::Freehand,
        Tool::Text,
        Tool::Note,
        Tool::Eraser,
        Tool::Knife,
        Tool::Pan,
        Tool::ZoomDrag,
    ] {
        h.state_mut().tool = t;
        h.run_steps(2);
    }
    h.state_mut().tool = Tool::Select;
    for w in [Workspace::Library, Workspace::Send, Workspace::Design] {
        h.state_mut().workspace = w;
        h.run_steps(3);
    }
    for mode in [
        SendMode::Guide,
        SendMode::Standard,
        SendMode::ByLine,
        SendMode::ByFill,
    ] {
        h.state_mut().workspace = Workspace::Send;
        h.state_mut().canvas.send.mode = mode;
        h.run_steps(3);
    }
    assert_eq!(
        h.state().doc.shapes.len(),
        4,
        "abrir paneles no cambia el documento"
    );
}
