//! Pruebas de la capa de la app: menús Objeto y Editar, atajos de teclado, archivos
//! (guardar, abrir, combinar, recuperación) y Editar puntos con el ratón.

use crate::flow_tests::{BLUE, RED, app, app_harness, drag_mm, rect, text};
use crate::object_menu::Cmd;
use kurbo::Rect;
use oc_core::arrange::{Align, Axis};
use oc_core::ops::BoolOp;
use oc_core::{Color, Shape, ShapeKind};

/// Caja de una forma en el mundo (con su rotación).
fn bounds(s: &Shape) -> Rect {
    oc_core::edit::selection_bounds(std::iter::once(s)).expect("tiene caja")
}

fn select_all(a: &mut crate::app::OpenCraftCutterApp) {
    a.doc.selected = a.doc.shapes.iter().map(|s| s.id).collect();
}

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("opencraftcutter-pruebas-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

// ---------------------------------------------------------------- Menú Objeto

#[test]
fn group_and_ungroup() {
    let mut a = app();
    a.doc.add_shape(rect(10.0, RED));
    a.doc.add_shape(rect(60.0, BLUE));
    select_all(&mut a);
    assert!(!a.object_enabled(Cmd::Ungroup));
    a.object_run(Cmd::Group);
    let g = a.doc.shapes[0].group;
    assert!(g.is_some() && a.doc.shapes.iter().all(|s| s.group == g));
    assert!(a.object_enabled(Cmd::Ungroup));
    a.object_run(Cmd::Ungroup);
    assert!(a.doc.shapes.iter().all(|s| s.group.is_none()));
}

#[test]
fn align_and_distribute() {
    let mut a = app();
    for x in [10.0, 30.0, 90.0] {
        let mut s = rect(x, RED);
        s.rect = Rect::new(x, x, x + 10.0, x + 10.0);
        a.doc.add_shape(s);
    }
    select_all(&mut a);
    a.object_run(Cmd::Align(Align::Left));
    assert!(a.doc.shapes.iter().all(|s| (s.rect.x0 - 10.0).abs() < 1e-6));
    a.object_run(Cmd::Distribute(Axis::Vertical));
    let mut ys: Vec<f64> = a.doc.shapes.iter().map(|s| s.rect.y0).collect();
    ys.sort_by(f64::total_cmp);
    assert!(((ys[1] - ys[0]) - (ys[2] - ys[1])).abs() < 1e-6, "{ys:?}");
}

#[test]
fn flip_and_rotate() {
    let mut a = app();
    a.doc.add_shape(rect(10.0, RED));
    select_all(&mut a);
    let r0 = a.doc.shapes[0].rect;
    a.object_run(Cmd::RotateBy(90.0));
    assert!((a.doc.shapes[0].rotation - 90.0).abs() < 1e-6);
    a.object_run(Cmd::RotateTo(0.0));
    assert!(a.doc.shapes[0].rotation.abs() < 1e-6);
    a.object_run(Cmd::FlipH);
    let r1 = bounds(&a.doc.shapes[0]);
    assert!(
        (r1.width() - r0.width()).abs() < 1e-6,
        "voltear no cambia el tamaño"
    );
}

#[test]
fn boolean_weld_from_menu_and_undo() {
    let mut a = app();
    let mut s1 = rect(10.0, RED);
    s1.rect = Rect::new(10.0, 10.0, 40.0, 40.0);
    let mut s2 = rect(10.0, RED);
    s2.rect = Rect::new(30.0, 10.0, 60.0, 40.0);
    a.doc.add_shape(s1);
    a.doc.add_shape(s2);
    select_all(&mut a);
    a.object_run(Cmd::Bool(BoolOp::Weld));
    assert_eq!(a.doc.shapes.len(), 1, "fusionar deja una forma");
    let b = bounds(&a.doc.shapes[0]);
    assert!((b.width() - 50.0).abs() < 1e-3, "{b:?}");
    a.undo();
    assert_eq!(a.doc.shapes.len(), 2, "deshacer devuelve las dos formas");
}

#[test]
fn weld_a_single_text_from_menu() {
    let mut a = app();
    a.doc.add_shape(text("mm"));
    select_all(&mut a);
    assert!(
        a.object_enabled(Cmd::Bool(BoolOp::Weld)),
        "Fusionar admite un solo texto"
    );
    a.object_run(Cmd::Bool(BoolOp::Weld));
    assert_eq!(a.doc.shapes.len(), 1);
    assert!(!a.doc.shapes[0].is_text(), "el texto pasa a trazado");
}

#[test]
fn compound_paths_and_to_path() {
    let mut a = app();
    let mut outer = rect(10.0, RED);
    outer.rect = Rect::new(10.0, 10.0, 60.0, 60.0);
    let mut inner = rect(10.0, RED);
    inner.rect = Rect::new(20.0, 20.0, 30.0, 30.0);
    a.doc.add_shape(outer);
    a.doc.add_shape(inner);
    select_all(&mut a);
    a.object_run(Cmd::MakeCompound);
    assert_eq!(a.doc.shapes.len(), 1);
    assert_eq!(a.doc.shapes[0].world_subpaths().len(), 2);
    select_all(&mut a);
    a.object_run(Cmd::ReleaseCompound);
    assert_eq!(a.doc.shapes.len(), 2);
    select_all(&mut a);
    a.object_run(Cmd::ToPath);
    assert!(
        a.doc
            .shapes
            .iter()
            .all(|s| matches!(s.kind, ShapeKind::Path { .. }))
    );
}

#[test]
fn replicate_fill_page_and_rows() {
    let mut a = app();
    a.doc.add_shape(rect(10.0, RED));
    select_all(&mut a);
    a.object_run(Cmd::Row(oc_core::replicate::Side::Right, 3));
    assert!(a.doc.shapes.len() >= 3, "fila de tres");
    let mut b = app();
    b.doc.add_shape(rect(10.0, RED));
    select_all(&mut b);
    b.object_run(Cmd::FillPage);
    assert!(
        b.doc.shapes.len() > 10,
        "llenar página: {}",
        b.doc.shapes.len()
    );
    let page = Rect::new(0.0, 0.0, b.doc.page.width_mm, b.doc.page.height_mm);
    assert!(
        b.doc.shapes.iter().all(|s| page.contains(s.rect.origin())),
        "todas las copias dentro de la página"
    );
}

#[test]
fn reorder_brings_to_front() {
    let mut a = app();
    let first = a.doc.add_shape(rect(10.0, RED));
    a.doc.add_shape(rect(60.0, BLUE));
    a.doc.selected = vec![first];
    a.object_run(Cmd::Reorder(i32::MAX));
    assert_eq!(a.doc.shapes.last().map(|s| s.id), Some(first));
}

// ---------------------------------------------------------------- Menú Editar

#[test]
fn copy_paste_and_paste_in_place() {
    let ctx = egui::Context::default();
    let mut a = app();
    let mut g1 = rect(10.0, RED);
    g1.group = Some(5);
    let mut g2 = rect(60.0, BLUE);
    g2.group = Some(5);
    a.doc.add_shape(g1);
    a.doc.add_shape(g2);
    select_all(&mut a);
    a.copy_selection(&ctx);
    a.paste(false);
    assert_eq!(a.doc.shapes.len(), 4);
    let pasted: Vec<&Shape> = a.doc.selected_shapes();
    assert!(pasted[0].rect.x0 > 10.0, "pegar desplaza la copia");
    assert!(
        pasted
            .iter()
            .all(|s| s.group.is_some() && s.group != Some(5)),
        "las copias forman un grupo nuevo"
    );
    a.paste(true);
    let in_place = a.doc.selected_shapes();
    assert_eq!(in_place.len(), 2);
    a.cut_selection(&ctx);
    assert_eq!(a.doc.shapes.len(), 4, "cortar quita la selección");
}

#[test]
fn select_by_color_picks_matching_shapes() {
    let mut a = app();
    a.doc.add_shape(rect(10.0, RED));
    a.doc.add_shape(rect(60.0, BLUE));
    a.doc.add_shape(rect(110.0, RED));
    let mut locked = rect(160.0, RED);
    locked.locked = true;
    a.doc.add_shape(locked);
    a.select_by_color(RED, false);
    assert_eq!(a.doc.selected.len(), 2, "los rojos sin bloquear");
    let (lines, _) = a.document_colors();
    assert!(lines.contains(&RED) && lines.contains(&BLUE));
}

// ---------------------------------------------------------------- Atajos de teclado

#[test]
fn keyboard_shortcuts() {
    use egui::{Key, Modifiers};
    let mut h = app_harness();
    {
        let a = h.state_mut();
        a.doc.add_shape(rect(20.0, RED));
        a.doc.add_shape(rect(70.0, BLUE));
    }
    h.run_steps(1);
    let cmd = Modifiers::COMMAND;
    // Herramientas.
    for (k, t) in [
        (Key::R, crate::app::Tool::Rect),
        (Key::E, crate::app::Tool::Ellipse),
        (Key::T, crate::app::Tool::Text),
        (Key::V, crate::app::Tool::Select),
    ] {
        h.key_press(k);
        h.run_steps(1);
        assert_eq!(h.state().tool, t, "tecla {k:?}");
    }
    // ⌘A selecciona todo; ⇧⌘A lo deselecciona.
    h.key_press_modifiers(cmd, Key::A);
    h.run_steps(1);
    assert_eq!(h.state().doc.selected.len(), 2);
    // ⌘G agrupa; ⇧⌘G desagrupa.
    h.key_press_modifiers(cmd, Key::G);
    h.run_steps(1);
    assert!(
        h.state().doc.shapes.iter().all(|s| s.group.is_some()),
        "⌘G agrupa"
    );
    h.key_press_modifiers(cmd | Modifiers::SHIFT, Key::G);
    h.run_steps(1);
    assert!(
        h.state().doc.shapes.iter().all(|s| s.group.is_none()),
        "⇧⌘G desagrupa"
    );
    // ⌘D duplica.
    h.key_press_modifiers(cmd, Key::D);
    h.run_steps(1);
    assert_eq!(h.state().doc.shapes.len(), 4, "⌘D duplica");
    // ⌘Z deshace; ⇧⌘Z rehace.
    h.key_press_modifiers(cmd, Key::Z);
    h.run_steps(1);
    assert_eq!(h.state().doc.shapes.len(), 2, "⌘Z deshace");
    h.key_press_modifiers(cmd | Modifiers::SHIFT, Key::Z);
    h.run_steps(1);
    assert_eq!(h.state().doc.shapes.len(), 4, "⇧⌘Z rehace");
    // Flechas: 1 mm; con Mayús, 10 mm.
    h.key_press_modifiers(cmd, Key::A);
    h.run_steps(1);
    let x0 = h.state().doc.shapes[0].rect.x0;
    h.key_press(Key::ArrowRight);
    h.run_steps(1);
    assert!(
        (h.state().doc.shapes[0].rect.x0 - x0 - 1.0).abs() < 1e-6,
        "→ mueve 1 mm"
    );
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
    h.run_steps(1);
    assert!(
        (h.state().doc.shapes[0].rect.x0 - x0 - 11.0).abs() < 1e-6,
        "⇧→ mueve 10 mm"
    );
    // ⇧⌘A deselecciona; Supr sin selección no borra nada.
    h.key_press_modifiers(cmd | Modifiers::SHIFT, Key::A);
    h.run_steps(1);
    assert!(h.state().doc.selected.is_empty());
    h.key_press(Key::Backspace);
    h.run_steps(1);
    assert_eq!(h.state().doc.shapes.len(), 4);
    // ⌥⌘L abre la Biblioteca; ⌥⌘D vuelve a Diseño.
    h.key_press_modifiers(cmd | Modifiers::ALT, Key::L);
    h.run_steps(2);
    assert_eq!(h.state().workspace, crate::app::Workspace::Library);
    h.key_press_modifiers(cmd | Modifiers::ALT, Key::D);
    h.run_steps(2);
    assert_eq!(h.state().workspace, crate::app::Workspace::Design);
}

#[test]
fn shortcuts_are_ignored_while_editing_text() {
    let mut h = app_harness();
    {
        let a = h.state_mut();
        let id = a.doc.add_shape(text("Hola"));
        a.doc.selected = vec![id];
        a.canvas.editing_text = Some(id);
    }
    h.run_steps(1);
    h.key_press(egui::Key::R);
    h.run_steps(1);
    assert_ne!(
        h.state().tool,
        crate::app::Tool::Rect,
        "la R se escribe, no cambia de herramienta"
    );
    assert_eq!(
        h.state().doc.shapes.len(),
        1,
        "Supr/teclas no borran el texto en edición"
    );
}

// ---------------------------------------------------------------- Archivos

#[test]
fn save_and_open_keep_the_document() {
    let ctx = egui::Context::default();
    let mut a = app();
    let mut r = rect(10.0, RED);
    r.rotation = 30.0;
    r.fill = Some(Color::rgb(1, 2, 3));
    r.group = Some(4);
    a.doc.add_shape(r);
    a.doc.add_shape(text("Hola"));
    a.doc.page.media_size = "letter".into();
    a.dirty = true;
    let path = temp("documento.opencraftcutter");
    assert!(a.save_to(&ctx, &path));
    assert!(!a.dirty, "guardar limpia los cambios pendientes");
    assert_eq!(a.file_path.as_deref(), Some(path.as_path()));
    let mut b = app();
    b.open_path(&ctx, &path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(b.doc.shapes.len(), 2);
    assert_eq!(b.doc.page.media_size, "letter");
    let s = &b.doc.shapes[0];
    assert!((s.rotation - 30.0).abs() < 1e-9);
    assert_eq!(s.fill, Some(Color::rgb(1, 2, 3)));
    assert!(s.group.is_some());
    assert!(b.doc.shapes[1].is_text());
}

#[test]
fn merge_adds_without_replacing() {
    let ctx = egui::Context::default();
    let mut other = app();
    other.doc.add_shape(rect(10.0, BLUE));
    other.doc.add_shape(rect(60.0, BLUE));
    let path = temp("combinar.opencraftcutter");
    assert!(other.save_to(&ctx, &path));
    let mut a = app();
    a.doc.add_shape(rect(10.0, RED));
    a.merge_path(&ctx, &path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        a.doc.shapes.len(),
        3,
        "se añaden las dos formas al documento actual"
    );
    assert_eq!(a.doc.selected.len(), 2, "y quedan seleccionadas");
    let ids: std::collections::HashSet<u64> = a.doc.shapes.iter().map(|s| s.id).collect();
    assert_eq!(ids.len(), 3, "identificadores únicos");
}

#[test]
fn recovery_copy_only_with_unsaved_changes() {
    let dir = temp("recuperacion");
    let _ = std::fs::remove_dir_all(&dir);
    let mut a = app();
    a.doc.name = "Diseño/raro".into();
    a.autosave_into(&dir);
    assert!(
        !dir.exists() || std::fs::read_dir(&dir).unwrap().next().is_none(),
        "vacío: nada que guardar"
    );
    a.doc.add_shape(rect(10.0, RED));
    a.dirty = true;
    a.autosave_into(&dir);
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    assert_eq!(files.len(), 1);
    let back =
        oc_core::Document::from_json(&std::fs::read_to_string(files[0].path()).unwrap()).unwrap();
    assert_eq!(back.shapes.len(), 1, "la copia se puede abrir");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opening_svg_and_image_files() {
    let ctx = egui::Context::default();
    let svg = temp("forma.svg");
    std::fs::write(
        &svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="50mm" height="50mm" viewBox="0 0 50 50"><rect x="5" y="5" width="20" height="10" fill="red"/></svg>"#,
    )
    .unwrap();
    let mut a = app();
    a.open_path(&ctx, &svg);
    assert_eq!(a.doc.shapes.len(), 1, "SVG importado");
    let png = temp("imagen.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]))
        .save(&png)
        .unwrap();
    a.import_image_path(&ctx, &png);
    assert!(
        a.doc.shapes.iter().any(|s| s.is_image()),
        "imagen importada"
    );
    let _ = std::fs::remove_file(svg);
    let _ = std::fs::remove_file(png);
}

// ---------------------------------------------------------------- Editar puntos con el ratón

#[test]
fn edit_points_drag_a_node() {
    let mut h = app_harness();
    {
        let a = h.state_mut();
        let mut s = rect(40.0, RED);
        s.rect = Rect::new(40.0, 40.0, 80.0, 80.0);
        let id = a.doc.add_shape(s);
        a.doc.selected = vec![id];
        a.object_run(Cmd::ToPath);
        a.tool = crate::app::Tool::EditPoints;
    }
    h.run_steps(2);
    let id = h.state().doc.shapes[0].id;
    crate::canvas::point_edit::begin(h.state_mut(), id);
    h.run_steps(1);
    // Arrastra la esquina (80, 80) a (100, 90).
    drag_mm(&mut h, (80.0, 80.0), (100.0, 90.0), egui::Modifiers::NONE);
    let b = bounds(&h.state().doc.shapes[0]);
    assert!(
        (b.x1 - 100.0).abs() < 1.0 && (b.y1 - 90.0).abs() < 1.0,
        "{b:?}"
    );
    assert!((b.x0 - 40.0).abs() < 1e-6, "los demás nodos no se mueven");
}

#[test]
fn edit_points_delete_and_join() {
    let mut a = app();
    let mut s = rect(40.0, RED);
    s.rect = Rect::new(40.0, 40.0, 80.0, 80.0);
    let id = a.doc.add_shape(s);
    a.doc.selected = vec![id];
    a.object_run(Cmd::ToPath);
    let id = a.doc.shapes[0].id;
    crate::canvas::point_edit::begin(&mut a, id);
    a.point_select(0, 1, false);
    assert!(a.delete_selected_vertex());
    assert_eq!(a.doc.shapes[0].world_node_paths()[0].nodes.len(), 3);
    a.point_select(0, 0, false);
    a.point_break();
    assert!(
        !a.doc.shapes[0].world_node_paths()[0].closed,
        "romper abre el trazado"
    );
    a.point_select(0, 0, false);
    a.point_join();
    let np = &a.doc.shapes[0].world_node_paths()[0];
    assert!(np.closed && np.nodes.len() == 3, "unir lo vuelve a cerrar");
}

// ---------------------------------------------------------------- Paneles (clics simulados)

use egui_kittest::kittest::Queryable;

/// App completa con un panel abierto y la selección dada.
fn panel(
    p: crate::app::Panel,
    shapes: Vec<Shape>,
    select: &[usize],
) -> egui_kittest::Harness<'static, crate::app::OpenCraftCutterApp> {
    let mut h = app_harness();
    {
        let a = h.state_mut();
        let ids: Vec<u64> = shapes.into_iter().map(|s| a.doc.add_shape(s)).collect();
        a.doc.selected = select.iter().map(|i| ids[*i]).collect();
        a.panel = Some(p);
    }
    h.run_steps(3);
    h
}

/// Pulsa el primer control cuyo nombre contiene `label`; antes se desplaza hasta él si queda
/// fuera de la vista (como haría el usuario en un panel largo).
fn click(h: &mut egui_kittest::Harness<'static, crate::app::OpenCraftCutterApp>, label: &str) {
    // Si el texto aparece también en una ayuda, se pulsa el control, no la etiqueta.
    fn find<'a>(
        h: &'a egui_kittest::Harness<'static, crate::app::OpenCraftCutterApp>,
        label: &'a str,
    ) -> egui_kittest::Node<'a> {
        use egui_kittest::kittest::NodeT;
        let all: Vec<_> = h.get_all_by_label_contains(label).collect();
        let pos = all
            .iter()
            .position(|n| n.accesskit_node().role() != egui::accesskit::Role::Label)
            .unwrap_or(0);
        all.into_iter()
            .nth(pos)
            .unwrap_or_else(|| panic!("no se encuentra «{label}»"))
    }
    find(h, label).scroll_to_me();
    h.run_steps(3);
    find(h, label).click();
    h.run_steps(3);
}

fn square(x0: f64, y0: f64, side: f64) -> Shape {
    let mut s = rect(0.0, RED);
    s.rect = Rect::new(x0, y0, x0 + side, y0 + side);
    s
}

#[test]
fn transform_panel_aligns_left() {
    let mut h = panel(
        crate::app::Panel::Transform,
        vec![
            square(10.0, 10.0, 10.0),
            square(40.0, 30.0, 10.0),
            square(80.0, 60.0, 10.0),
        ],
        &[0, 1, 2],
    );
    click(&mut h, "Alinear a la izquierda");
    assert!(
        h.state()
            .doc
            .shapes
            .iter()
            .all(|s| (s.rect.x0 - 10.0).abs() < 1e-6)
    );
}

#[test]
fn modify_panel_welds() {
    let mut h = panel(
        crate::app::Panel::Modify,
        vec![square(10.0, 10.0, 30.0), square(30.0, 10.0, 30.0)],
        &[0, 1],
    );
    click(&mut h, "Fusionar");
    assert_eq!(h.state().doc.shapes.len(), 1);
}

#[test]
fn offset_panel_adds_an_outline() {
    let mut h = panel(
        crate::app::Panel::Offset,
        vec![square(40.0, 40.0, 30.0)],
        &[0],
    );
    // Primero se elige el tipo (el título del panel también dice «Desplazamiento»).
    use egui_kittest::kittest::NodeT;
    h.get_all_by_role(egui::accesskit::Role::Button)
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.ends_with("Desplazamiento"))
        })
        .expect("botón Desplazamiento")
        .click();
    h.run_steps(3);
    click(&mut h, "Aplicar");
    let shapes = &h.state().doc.shapes;
    assert_eq!(shapes.len(), 2, "se añade el contorno");
    assert!(shapes[1].rect.width() > shapes[0].rect.width(), "por fuera");
}

#[test]
fn replicate_panel_duplicates() {
    let mut h = panel(
        crate::app::Panel::Replicate,
        vec![square(20.0, 20.0, 20.0)],
        &[0],
    );
    click(&mut h, "Duplicar a la derecha");
    assert_eq!(h.state().doc.shapes.len(), 2);
    let s = &h.state().doc.shapes;
    assert!(
        s[1].rect.x0 > s[0].rect.x1 - 1e-6,
        "la copia va a la derecha"
    );
}

#[test]
fn rhinestone_and_sketch_panels_generate() {
    let mut h = panel(
        crate::app::Panel::Rhinestone,
        vec![square(20.0, 20.0, 60.0)],
        &[0],
    );
    click(&mut h, "Generar pedrería");
    let stones: usize = h
        .state()
        .doc
        .shapes
        .iter()
        .map(|s| s.world_subpaths().len())
        .sum();
    assert!(stones > 10, "muchas piedras: {stones}");

    let mut h = panel(
        crate::app::Panel::Sketch,
        vec![square(20.0, 20.0, 60.0)],
        &[0],
    );
    click(&mut h, "Generar sombreado");
    let lines: usize = h
        .state()
        .doc
        .shapes
        .iter()
        .map(|s| s.world_subpaths().len())
        .sum();
    assert!(lines > 5, "líneas de sombreado: {lines}");
}

#[test]
fn fill_and_stroke_panels_change_colors() {
    let mut s = square(20.0, 20.0, 20.0);
    s.fill = Some(Color::rgb(10, 20, 30));
    let mut h = panel(crate::app::Panel::Fill, vec![s], &[0]);
    click(&mut h, "Sin relleno");
    assert_eq!(h.state().doc.shapes[0].fill, None);
    h.state_mut().apply_fill(Some(BLUE));
    assert_eq!(h.state().doc.shapes[0].fill, Some(BLUE));
    h.state_mut().apply_stroke(|st| st.width_pt = 2.0);
    assert!((h.state().doc.shapes[0].stroke.width_pt - 2.0).abs() < 1e-9);
    h.state_mut().undo();
    assert!(
        h.state().doc.shapes[0].stroke.width_pt.abs() < 1e-9,
        "deshacer el grosor"
    );
}

#[test]
fn text_panel_toggles_bold() {
    let mut h = panel(crate::app::Panel::Text, vec![text("Hola")], &[0]);
    click(&mut h, "Negrita");
    let ShapeKind::Text { bold, .. } = &h.state().doc.shapes[0].kind else {
        panic!("sigue siendo texto")
    };
    assert!(*bold);
}

#[test]
fn page_setup_landscape_swaps_size() {
    let mut h = panel(crate::app::Panel::PageSetup, vec![], &[]);
    let (w, hh) = (h.state().doc.page.width_mm, h.state().doc.page.height_mm);
    click(&mut h, "Apaisado");
    let p = &h.state().doc.page;
    assert!(
        (p.width_mm - hh).abs() < 1e-6 && (p.height_mm - w).abs() < 1e-6,
        "{w}×{hh} → {}×{}",
        p.width_mm,
        p.height_mm
    );
}

#[test]
fn print_and_cut_enables_registration_marks() {
    let mut h = panel(crate::app::Panel::PrintCut, vec![], &[]);
    assert!(!h.state().doc.page.reg_marks.enabled);
    click(&mut h, "Activar marcas de registro");
    assert!(h.state().doc.page.reg_marks.enabled);

    // Al enviar, la máquina busca las marcas: el origen es la esquina del cuadrado impreso y
    // el área, el marco entre marcas.
    let a = h.state();
    let page = &a.doc.page;
    let r = a.registration().expect("se leen las marcas");
    let square = page.reg_marks.mark_rects(page.width_mm, page.height_mm)[0];
    assert_eq!(r.origin, square.origin());
    let frame = page.reg_marks.cut_area(page.width_mm, page.height_mm);
    assert_eq!((r.width_mm, r.height_mm), (frame.width(), frame.height()));
    assert!(a.registration_problem().is_none());

    h.state_mut().doc.page.reg_marks.inverted = true;
    assert!(
        h.state().registration_problem().is_some(),
        "invertidas: todavía no"
    );
    h.state_mut().doc.page.reg_marks.enabled = false;
    assert!(h.state().registration().is_none());
    assert!(h.state().registration_problem().is_none());
}

#[test]
fn layers_panel_lists_every_shape() {
    let h = panel(
        crate::app::Panel::Layers,
        vec![
            square(10.0, 10.0, 10.0),
            text("Capa"),
            square(40.0, 10.0, 10.0),
        ],
        &[],
    );
    assert!(
        h.query_by_label_contains("Capa").is_some(),
        "el texto aparece en la lista"
    );
}

// ---------------------------------------------------------------- Soltar archivos

#[derive(Debug)]
struct Dropped(std::path::PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn drop_files(
    h: &mut egui_kittest::Harness<'static, crate::app::OpenCraftCutterApp>,
    files: &[&std::path::Path],
) {
    for f in files {
        h.input_mut()
            .dropped_files
            .push(std::sync::Arc::new(Dropped(f.to_path_buf())));
    }
    h.run_steps(3);
}

#[test]
fn dropping_files_on_the_design_adds_them() {
    let svg = temp("soltar.svg");
    std::fs::write(
        &svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="40mm" viewBox="0 0 40 40"><circle cx="20" cy="20" r="10"/></svg>"#,
    )
    .unwrap();
    let txt = temp("soltar.txt");
    std::fs::write(&txt, "no").unwrap();
    let mut h = app_harness();
    drop_files(&mut h, &[&svg, &txt]);
    assert_eq!(
        h.state().doc.shapes.len(),
        1,
        "el SVG se añade; el .txt se rechaza"
    );
    let _ = std::fs::remove_file(svg);
    let _ = std::fs::remove_file(txt);
}

#[test]
fn dropping_files_on_the_library_copies_them() {
    let root = temp("biblio-soltar");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let svg = temp("para-biblioteca.svg");
    std::fs::write(
        &svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#,
    )
    .unwrap();
    let mut h = app_harness();
    h.state_mut().library.set_root(Some(root.to_str().unwrap()));
    h.state_mut().workspace = crate::app::Workspace::Library;
    h.run_steps(2);
    drop_files(&mut h, &[&svg]);
    assert!(
        root.join("para-biblioteca.svg").exists(),
        "copiado a la biblioteca"
    );
    assert!(h.state().doc.shapes.is_empty(), "no se añade al diseño");
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_file(svg);
}

// ---------------------------------------------------------------- Dibujo (capturas de referencia)

/// Documento fijo para las capturas: formas con relleno, sin relleno, girada, agrupada, texto.
fn snapshot_doc(a: &mut crate::app::OpenCraftCutterApp) {
    let mut filled = square(20.0, 20.0, 40.0);
    filled.fill = Some(Color::rgb(240, 180, 30));
    a.doc.add_shape(filled);
    let mut ring = Shape::new(
        0,
        "Elipse",
        Rect::new(80.0, 20.0, 150.0, 70.0),
        ShapeKind::Ellipse,
    );
    ring.stroke.color = BLUE;
    ring.stroke.width_pt = 2.0;
    a.doc.add_shape(ring);
    let mut turned = square(40.0, 90.0, 30.0);
    turned.rotation = 30.0;
    turned.fill = Some(Color::rgb(60, 120, 230));
    a.doc.add_shape(turned);
    let mut t = text("OpenCraftCutter");
    t.fill = Some(Color::rgb(20, 20, 20));
    a.doc.add_shape(t);
}

fn snapshot_harness() -> egui_kittest::Harness<'static, crate::app::OpenCraftCutterApp> {
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_context_setup(|ctx| {
            egui_material_icons::initialize(ctx);
            crate::theme::apply(ctx);
        })
        .allow_missing_glyphs()
        .with_accessibility_check(false)
        .wgpu()
        .build_eframe(|cc| {
            let mut a = crate::app::OpenCraftCutterApp::new(cc);
            a.doc = oc_core::Document::default();
            a.panel = None;
            snapshot_doc(&mut a);
            a
        });
    h.run_steps(4);
    h
}

/// Tolerancia de las capturas: cada píxel admite la diferencia de color por defecto (0,6 YIQ)
/// y hasta 0,05 % de píxeles distintos (≈ 480 de 1200×800), lo que absorbe el suavizado de
/// bordes y texto entre versiones de GPU/macOS; mover una forma 1 mm cambia miles.
fn snapshot_options() -> egui_kittest::SnapshotOptions {
    egui_kittest::SnapshotOptions::new().max_failed_pixels(480)
}

/// Si cambia el aspecto del lienzo, la prueba falla y deja la imagen nueva y la diferencia en
/// `tests/snapshots`. Si el cambio es intencionado: `UPDATE_SNAPSHOTS=1 cargo test`.
#[test]
fn snapshot_design_canvas() {
    let mut h = snapshot_harness();
    h.snapshot_options("lienzo_diseno", &snapshot_options());
}

/// La tolerancia no esconde cambios reales: con una forma desplazada 2 mm la captura falla.
#[test]
fn snapshot_tolerance_still_catches_a_moved_shape() {
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        return; // no sobrescribir la referencia con la versión alterada
    }
    let mut h = snapshot_harness();
    {
        let a = h.state_mut();
        let s = &mut a.doc.shapes[0];
        s.rect = s.rect + kurbo::Vec2::new(2.0, 0.0);
    }
    h.run_steps(4);
    assert!(
        h.try_snapshot_options("lienzo_diseno", &snapshot_options())
            .is_err()
    );
}

#[test]
fn snapshot_send_preview_by_color() {
    let mut h = snapshot_harness();
    {
        let a = h.state_mut();
        a.workspace = crate::app::Workspace::Send;
        a.canvas.send.mode = oc_core::send::SendMode::ByLine;
    }
    h.run_steps(4);
    h.snapshot_options("enviar_por_color", &snapshot_options());
}

// ---------------------------------------------------------------- Trazar y herramientas especiales

/// Imagen PNG de 200×200 con un círculo negro sobre fondo transparente.
fn circle_image() -> Shape {
    let img = image::RgbaImage::from_fn(200, 200, |x, y| {
        let d = ((x as f64 - 100.0).powi(2) + (y as f64 - 100.0).powi(2)).sqrt();
        if d < 60.0 {
            image::Rgba([0, 0, 0, 255])
        } else {
            image::Rgba([255, 255, 255, 0])
        }
    });
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).unwrap();
    Shape::new(
        0,
        "Círculo",
        Rect::new(20.0, 20.0, 70.0, 70.0),
        ShapeKind::Image {
            png: oc_core::shape::ImageData(std::sync::Arc::new(png.into_inner())),
            px_width: 200,
            px_height: 200,
        },
    )
}

#[test]
fn trace_panel_traces_an_image() {
    let mut h = panel(crate::app::Panel::Trace, vec![circle_image()], &[0]);
    click(&mut h, "Trazar contorno externo");
    let paths: Vec<&Shape> = h
        .state()
        .doc
        .shapes
        .iter()
        .filter(|s| !s.is_image())
        .collect();
    assert_eq!(paths.len(), 1, "un contorno para el círculo");
    let b = paths[0].rect;
    // El círculo ocupa 120 de 200 px de una imagen de 50 mm: unos 30 mm de diámetro.
    assert!(
        (b.width() - 30.0).abs() < 2.0 && (b.height() - 30.0).abs() < 2.0,
        "{b:?}"
    );
}

#[test]
fn trace_and_detach_clips_the_image() {
    let mut h = panel(crate::app::Panel::Trace, vec![circle_image()], &[0]);
    let before = h.state().doc.shapes.len();
    click(&mut h, "Trazar y separar");
    let shapes = &h.state().doc.shapes;
    assert!(shapes.len() > before, "se añade la línea de corte");
    assert!(shapes.iter().any(|s| s.is_image()), "la imagen se conserva");
}

#[test]
fn special_tools_set_a_fill_and_punch_points() {
    let mut h = panel(
        crate::app::Panel::SpecialTools,
        vec![square(20.0, 20.0, 40.0)],
        &[0],
    );
    click(&mut h, "Lápiz térmico");
    assert!(
        h.query_by_label_contains("Rellenos para lápices térmicos")
            .is_some(),
        "cambia de pestaña"
    );
    click(&mut h, "Punzón de cuero");
    click(&mut h, "Añadir puntos de perforación");
    assert!(
        h.state().doc.shapes.iter().any(|s| s.punch_mm.is_some()),
        "se añaden puntos de perforación"
    );
    click(&mut h, "Quitar puntos de perforación");
    assert!(h.state().doc.shapes.iter().all(|s| s.punch_mm.is_none()));
}

// ---------------------------------------------------------------- PixScan

/// Foto sintética (gris con un marco oscuro) de `w`×`h` px.
fn pixscan_photo(name: &str, w: u32, h: u32) -> std::path::PathBuf {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        if x < 10 || y < 10 || x > w - 10 || y > h - 10 {
            image::Rgb([20, 20, 20])
        } else {
            image::Rgb([200, 200, 200])
        }
    });
    let p = temp(name);
    img.save(&p).unwrap();
    p
}

fn pixscan_import(
    w: u32,
    h: u32,
) -> egui_kittest::Harness<'static, crate::app::OpenCraftCutterApp> {
    let mut h_ = panel(crate::app::Panel::PixScan, vec![], &[]);
    let path = pixscan_photo(&format!("pixscan-{w}x{h}.png"), w, h);
    let ctx = h_.ctx.clone();
    {
        let a = h_.state_mut();
        a.prefs.pixscan_calibrations.clear();
        a.pixscan_import_path(&ctx, &path);
    }
    h_.run_steps(3);
    h_
}

#[test]
fn pixscan_rejects_a_too_small_photo() {
    let mut h = pixscan_import(600, 400);
    assert!(
        h.query_by_label_contains("La resolución mínima soportada")
            .is_some()
    );
    click(&mut h, "Cancelar");
    assert!(h.query_by_label_contains("Error de importación").is_some());
    assert!(
        h.query_by_label_contains("Esquinas del tapete PixScan")
            .is_none()
    );
}

#[test]
fn pixscan_low_res_then_no_calibration_then_place_on_mat() {
    let mut h = pixscan_import(800, 600);
    assert!(
        h.query_by_label_contains("La resolución de esta imagen es baja")
            .is_some()
    );
    click(&mut h, "Continuar igualmente");
    // Sin perfiles guardados, «Automática» avisa de que no hay calibración.
    assert!(
        h.query_by_label_contains("ningún perfil de calibración")
            .is_some()
    );
    click(&mut h, "Continuar sin calibración");
    assert!(
        h.query_by_label_contains("Esquinas del tapete PixScan")
            .is_some()
    );
    click(&mut h, "Colocar en el tapete");

    let a = h.state();
    assert_eq!(a.doc.shapes.len(), 1);
    let s = &a.doc.shapes[0];
    assert!(s.pixscan && s.locked && s.is_image());
    assert_eq!(a.doc.page.mat, oc_core::machines::MatId::PixScanCameo);
    // La foto ocupa el tapete entero y la página toma su tamaño.
    assert!((s.rect.width() - a.doc.page.width_mm).abs() < 1e-6);
    assert!((s.rect.height() - a.doc.page.height_mm).abs() < 1e-6);
    assert!(
        h.query_by_label_contains("Esquinas del tapete PixScan")
            .is_none()
    );
}

#[test]
fn pixscan_good_photo_goes_straight_to_calibration_notice() {
    let mut h = pixscan_import(1600, 1200);
    assert!(
        h.query_by_label_contains("La resolución de esta imagen es baja")
            .is_none()
    );
    assert!(
        h.query_by_label_contains("ningún perfil de calibración")
            .is_some()
    );
    click(&mut h, "Cancelar");
    assert!(
        h.query_by_label_contains("Esquinas del tapete PixScan")
            .is_none()
    );
    assert!(h.state().doc.shapes.is_empty());
}

// ---------------------------------------------------------------- Corte de prueba

fn send_harness_with(
    shapes: Vec<Shape>,
) -> egui_kittest::Harness<'static, crate::app::OpenCraftCutterApp> {
    let mut h = app_harness();
    {
        let a = h.state_mut();
        for s in shapes {
            a.doc.add_shape(s);
        }
        a.workspace = crate::app::Workspace::Send;
    }
    h.run_steps(3);
    h
}

/// Clic corto en un punto del lienzo (mm).
fn click_mm(h: &mut egui_kittest::Harness<'static, crate::app::OpenCraftCutterApp>, p: (f64, f64)) {
    let pos = h
        .state()
        .canvas
        .viewport
        .to_screen(kurbo::Point::new(p.0, p.1));
    h.hover_at(pos);
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(3);
}

/// Lo que haría la máquina al terminar «Enviar prueba», sin máquina.
fn simulate_test_cut(h: &mut egui_kittest::Harness<'static, crate::app::OpenCraftCutterApp>) {
    {
        let a = h.state_mut();
        let jobs = a.jobs_to_send(true);
        let n = a.canvas.send.test_attempts.len() + 1;
        let origin = a.test_shape_origin().expect("hay forma de prueba");
        let at = oc_core::send::TestAttempt::from_jobs(n, origin, &jobs).expect("la prueba corta");
        a.finish_test_attempt(at);
    }
    h.run_steps(3);
}

#[test]
fn test_cut_is_placed_where_clicked() {
    let mut h = send_harness_with(vec![rect(10.0, BLUE)]);
    click(&mut h, "Elegir sitio en el tapete");
    assert!(h.state().canvas.send.placing_test);
    click_mm(&mut h, (120.0, 150.0));
    let a = h.state();
    assert!(!a.canvas.send.placing_test, "un clic basta");
    let o = a.test_shape_origin().expect("se crea la forma de prueba");
    // Centrada en el clic (lado 10 mm).
    assert!(
        (o.x - 115.0).abs() < 0.5 && (o.y - 145.0).abs() < 0.5,
        "{o:?}"
    );
    // El diseño no se ha tocado.
    assert_eq!(
        a.doc
            .shapes
            .iter()
            .filter(|s| s.name == "Rectángulo")
            .count(),
        1
    );
}

#[test]
fn each_test_cut_goes_next_to_the_previous_one() {
    let mut h = send_harness_with(vec![]);
    click(&mut h, "Generar forma de prueba");
    let first = h.state().test_shape_origin().unwrap();
    simulate_test_cut(&mut h);
    let second = h.state().test_shape_origin().unwrap();
    assert_eq!(second.y, first.y);
    assert!(
        (second.x - first.x - 15.0).abs() < 1e-6,
        "{first:?} → {second:?}"
    );
    assert_eq!(oc_core::send::test_shape_ids(&h.state().doc).len(), 1);
    // Quitar la forma y volver a generarla sigue la fila de intentos.
    click(&mut h, "Eliminar forma de prueba");
    click(&mut h, "Generar forma de prueba");
    assert_eq!(h.state().test_shape_origin().unwrap(), second);
}

#[test]
fn a_good_test_cut_becomes_a_material() {
    let mut h = send_harness_with(vec![]);
    let material = h.state().canvas.send.setups[0].material.clone();
    click(&mut h, "Generar forma de prueba");
    simulate_test_cut(&mut h);
    let sent = h.state().canvas.send.test_attempts[0].settings;
    assert!(h.query_by_label_contains("Intentos de prueba").is_some());
    click(&mut h, "#1 Bien");
    assert_eq!(
        h.state().canvas.send.test_attempts[0].result,
        Some(oc_core::send::TestResult::Good)
    );
    click(&mut h, "Guardar como material");
    let name = format!("{material} (prueba 1)");
    h.get_by_label("Guardar").click();
    h.run_steps(3);
    let a = h.state();
    let m = a
        .prefs
        .materials
        .iter()
        .find(|m| m.name == name)
        .expect("material nuevo");
    assert!(m.custom);
    let at = &a.canvas.send.test_attempts[0];
    assert_eq!(*m.settings(at.holder, at.action).unwrap(), sent);
    assert_eq!(a.canvas.send.setups[0].material, name, "pasa a usarse");
}

#[test]
fn in_color_mode_the_test_cuts_with_the_chosen_row() {
    let mut h = send_harness_with(vec![rect(10.0, RED), rect(60.0, BLUE)]);
    {
        let a = h.state_mut();
        a.canvas.send.mode = oc_core::send::SendMode::ByLine;
    }
    h.run_steps(3);
    click(&mut h, "Generar forma de prueba");
    {
        let a = h.state_mut();
        let blue = a
            .canvas
            .send
            .line_rows
            .iter_mut()
            .find(|(c, _)| *c == BLUE)
            .unwrap();
        blue.1.settings = Some(oc_core::send::ActionSettings {
            depth: 7,
            force: 30,
            ..Default::default()
        });
        a.canvas.send.test_color = Some(BLUE);
        a.apply_test_color();
    }
    h.run_steps(3);
    let jobs = h.state_mut().jobs_to_send(true);
    assert_eq!(jobs.len(), 1);
    assert_eq!((jobs[0].settings.depth, jobs[0].settings.force), (7, 30));
    let ids = oc_core::send::test_shape_ids(&h.state().doc);
    assert_eq!(h.state().doc.shape(ids[0]).unwrap().stroke.color, BLUE);
}

#[test]
fn print_and_cut_warns_about_lines_outside_or_on_the_marks() {
    let mut h = send_harness_with(vec![square(80.0, 100.0, 30.0)]);
    h.state_mut().doc.page.reg_marks.enabled = true;
    h.run_steps(3);
    assert!(
        h.state().marks_warning().is_none(),
        "dentro del marco no avisa"
    );

    // Fuera del marco (a la izquierda) y junto al cuadrado de arriba a la izquierda.
    {
        let a = h.state_mut();
        a.doc.add_shape(square(2.0, 100.0, 20.0));
        a.doc.add_shape(square(20.0, 20.0, 8.0));
    }
    h.run_steps(3);
    let w = h.state().marks_warning().expect("avisa");
    assert!(w.contains("1 línea queda fuera"), "{w}");
    assert!(w.contains("1 línea pasa por la zona"), "{w}");
    assert!(
        h.query_by_label_contains("fuera del área entre las marcas")
            .is_some()
    );
    // Es un aviso: se puede enviar lo que está dentro.
    assert!(h.state().nothing_to_cut().is_none());

    // La forma de prueba se coloca dentro del marco, lejos de las marcas.
    click(&mut h, "Generar forma de prueba");
    let o = h.state().test_shape_origin().unwrap();
    let page = &h.state().doc.page;
    let zones = page.reg_marks.keep_out_zones(page.width_mm, page.height_mm);
    let r = kurbo::Rect::new(o.x, o.y, o.x + 10.0, o.y + 10.0);
    assert!(
        page.reg_marks
            .cut_area(page.width_mm, page.height_mm)
            .contains(r.origin())
    );
    assert!(zones.iter().all(|z| z.intersect(r).area() == 0.0), "{r:?}");
}

// ---------------------------------------------------------------- Vinil textil

fn job_x_range(jobs: &[oc_core::send::Job]) -> (f64, f64) {
    let xs: Vec<f64> = jobs
        .iter()
        .flat_map(|j| j.paths.iter())
        .flat_map(|(p, _)| p.iter().map(|q| q.x))
        .collect();
    (
        xs.iter().cloned().fold(f64::MAX, f64::min),
        xs.iter().cloned().fold(f64::MIN, f64::max),
    )
}

#[test]
fn heat_transfer_vinyl_is_cut_mirrored() {
    let mut h = send_harness_with(vec![square(10.0, 20.0, 30.0)]);
    let w = h.state().doc.page.width_mm;
    assert!(h.query_by_label_contains("Reflejar el diseño").is_none());
    assert_eq!(
        job_x_range(&h.state_mut().jobs_to_send(false)),
        (10.0, 40.0)
    );

    h.state_mut().canvas.send.setups[0].material = "Transferencia de calor, tela".into();
    h.run_steps(3);
    assert!(h.state().mirror_on_cut());
    let (x0, x1) = job_x_range(&h.state_mut().jobs_to_send(false));
    assert!((x0 - (w - 40.0)).abs() < 1e-6 && (x1 - (w - 10.0)).abs() < 1e-6);
    // La forma de prueba no se refleja: se corta donde se colocó.
    click(&mut h, "Generar forma de prueba");
    let o = h.state().test_shape_origin().unwrap();
    let (t0, _) = job_x_range(&h.state_mut().jobs_to_send(true));
    assert!((t0 - o.x).abs() < 1e-6);

    // Desmarcar la casilla corta sin espejo (y se avisa).
    click(&mut h, "Reflejar el diseño al cortar");
    assert_eq!(h.state().canvas.send.mirror, Some(false));
    assert_eq!(
        job_x_range(&h.state_mut().jobs_to_send(false)),
        (10.0, 40.0)
    );
    assert!(h.query_by_label_contains("quedará al revés").is_some());
    // Volver a marcarla vuelve a lo automático.
    click(&mut h, "Reflejar el diseño al cortar");
    assert_eq!(h.state().canvas.send.mirror, None);

    // Vinil textil imprimible: sin espejo.
    h.state_mut().canvas.send.setups[0].material = "Transferencia de calor, tela imprimible".into();
    h.run_steps(3);
    assert!(!h.state().mirror_on_cut());
}

// ---------------------------------------------------------------- Detalles finos

#[test]
fn thin_details_are_warned_and_marked() {
    let bar = {
        let mut s = square(60.0, 20.0, 10.0);
        s.rect = Rect::new(60.0, 20.0, 60.5, 35.0);
        s
    };
    let mut h = send_harness_with(vec![square(10.0, 20.0, 30.0)]);
    assert!(
        h.state().thin_warning().is_none(),
        "un cuadrado grande no avisa"
    );

    h.state_mut().doc.add_shape(bar);
    h.run_steps(3);
    let a = h.state();
    assert_eq!(a.thin_details().len(), 1);
    let w = a.thin_warning().unwrap();
    assert!(w.contains("1 detalle es más fino que 0,8 mm"), "{w}");
    assert!(h.query_by_label_contains("marcados en rojo").is_some());
    assert!(h.state().nothing_to_cut().is_none(), "es solo un aviso");

    // Una barra de 1,5 mm vale para cartulina, pero la tela pide 2 mm.
    {
        let a = h.state_mut();
        a.doc.shapes.last_mut().unwrap().rect = Rect::new(60.0, 20.0, 61.5, 35.0);
    }
    assert!(h.state().thin_warning().is_none());
    let fabric = h
        .state()
        .prefs
        .materials
        .iter()
        .find(|m| m.family == oc_core::send::Family::Fabric)
        .map(|m| m.name.clone())
        .expect("hay algún material de tela");
    h.state_mut().canvas.send.setups[0].material = fabric;
    assert!(h.state().thin_warning().unwrap().contains("2,0 mm"));
}

// ---------------------------------------------------------------- Desbarbado

#[test]
fn weeding_box_and_lines_go_around_and_between_pieces() {
    let mut h = panel(
        crate::app::Panel::Weeding,
        vec![square(10.0, 10.0, 10.0), square(30.0, 10.0, 10.0)],
        &[0, 1],
    );
    click(&mut h, "Añadir desbarbado");
    let a = h.state();
    let frame = a
        .doc
        .shapes
        .iter()
        .find(|s| s.name == crate::panels::weeding::BOX_NAME)
        .expect("caja");
    assert_eq!(
        frame.rect,
        Rect::new(5.0, 5.0, 45.0, 25.0),
        "margen de 5 mm"
    );
    assert_eq!(frame.stroke.color, RED, "mismo color que el diseño");
    let lines = a
        .doc
        .shapes
        .iter()
        .find(|s| s.name == crate::panels::weeding::LINES_NAME)
        .expect("líneas");
    let open = oc_core::ops::open_paths(lines);
    assert_eq!(open.len(), 1);
    assert!(
        open[0].iter().all(|p| (p[0] - 25.0).abs() < 1e-6),
        "vertical en el hueco"
    );
    assert_eq!(a.doc.selected.len(), 2, "queda seleccionado lo nuevo");

    click(&mut h, "Quitar desbarbado");
    assert_eq!(h.state().doc.shapes.len(), 2, "solo quedan las piezas");
}

#[test]
fn alignment_marks_are_repeated_in_every_color() {
    let mut h = panel(
        crate::app::Panel::Weeding,
        vec![rect(10.0, RED), rect(30.0, BLUE)],
        &[],
    );
    click(&mut h, "Añadir marcas de alineación");
    let marks: Vec<Shape> = h
        .state()
        .doc
        .shapes
        .iter()
        .filter(|s| s.name == crate::panels::weeding::MARKS_NAME)
        .cloned()
        .collect();
    assert_eq!(marks.len(), 2, "una por color");
    let colors: Vec<_> = marks.iter().map(|s| s.stroke.color).collect();
    assert!(colors.contains(&RED) && colors.contains(&BLUE));
    assert_eq!(
        marks[0].rect, marks[1].rect,
        "en el mismo sitio en cada capa"
    );
    assert_eq!(marks[0].world_subpaths().len(), 2, "dos cruces");
    // Repetir no las duplica.
    click(&mut h, "Añadir marcas de alineación");
    let n = h
        .state()
        .doc
        .shapes
        .iter()
        .filter(|s| s.name == crate::panels::weeding::MARKS_NAME)
        .count();
    assert_eq!(n, 2);
    click(&mut h, "Quitar desbarbado y marcas");
    assert_eq!(h.state().doc.shapes.len(), 2);
}

#[test]
fn alignment_marks_follow_fill_colors_when_layers_differ_by_fill() {
    // Vinil por capas: mismo trazo negro, relleno rojo y azul.
    let layer = |x: f64, fill: oc_core::Color| {
        let mut s = rect(x, oc_core::Color::BLACK);
        s.fill = Some(fill);
        s
    };
    let mut h = panel(
        crate::app::Panel::Weeding,
        vec![layer(10.0, RED), layer(50.0, BLUE)],
        &[],
    );
    click(&mut h, "Añadir marcas de alineación");
    let marks: Vec<Shape> = h
        .state()
        .doc
        .shapes
        .iter()
        .filter(|s| s.name == crate::panels::weeding::MARKS_NAME)
        .cloned()
        .collect();
    assert_eq!(marks.len(), 2, "una por capa de relleno");
    let fills: Vec<_> = marks.iter().map(|s| s.fill).collect();
    assert!(fills.contains(&Some(RED)) && fills.contains(&Some(BLUE)));
    assert!(
        marks
            .iter()
            .all(|s| s.stroke.color == oc_core::Color::BLACK),
        "conservan el trazo de su capa"
    );
}

// ---------------------------------------------------------------- Hoja de pegatinas y acomodo

/// Cajas de cada grupo (pieza) del documento.
fn unit_boxes(doc: &oc_core::Document) -> Vec<Rect> {
    let mut groups: std::collections::BTreeMap<u64, Rect> = Default::default();
    for s in &doc.shapes {
        let b = oc_core::edit::shape_bounds(s);
        let g = s.group.unwrap_or(s.id);
        groups.entry(g).and_modify(|r| *r = r.union(b)).or_insert(b);
    }
    groups.into_values().collect()
}

fn assert_packed(doc: &oc_core::Document, boxes: &[Rect]) {
    let area = oc_core::send::safe_area(doc).inflate(1e-6, 1e-6);
    for (i, a) in boxes.iter().enumerate() {
        assert!(
            area.contains(a.origin()) && area.contains(kurbo::Point::new(a.x1, a.y1)),
            "{a:?} fuera de {area:?}"
        );
        for b in &boxes[i + 1..] {
            assert!(a.intersect(*b).area() < 1e-9, "se solapan {a:?} {b:?}");
        }
    }
}

#[test]
fn sticker_sheet_fills_the_page_inside_the_marks() {
    let mut h = panel(crate::app::Panel::PrintCut, vec![circle_image()], &[]);
    click(&mut h, "Crear hoja de pegatinas");
    let a = h.state();
    assert!(a.doc.page.reg_marks.enabled, "activa las marcas");
    let images = a.doc.shapes.iter().filter(|s| s.is_image()).count();
    let contours: Vec<&Shape> = a
        .doc
        .shapes
        .iter()
        .filter(|s| s.name == crate::panels::print_cut::STICKER_CONTOUR)
        .collect();
    // Imagen de 50 mm (círculo de 30 mm + 2 mm de margen) en una carta: caben varias.
    assert!(images >= 6, "{images}");
    assert_eq!(contours.len(), images, "cada imagen con su contorno");
    // Contorno ≈ círculo de 30 mm + 2 mm de margen por lado.
    let c = oc_core::edit::shape_bounds(contours[0]);
    assert!((c.width() - 34.0).abs() < 1.0, "{c:?}");
    // Cada pegatina (imagen + contorno) es un grupo, y no se solapan.
    let boxes = unit_boxes(&a.doc);
    assert_eq!(boxes.len(), images);
    assert_packed(&a.doc, &boxes);
    // Los contornos se cortan; las imágenes no.
    assert!(a.nothing_to_cut().is_none());
}

#[test]
fn sticker_sheet_with_a_fixed_number_of_copies() {
    let mut a = app();
    a.doc.add_shape(circle_image());
    let msg = a.make_sticker_sheet(2.0, Some(3));
    assert!(msg.starts_with("3 pegatinas"), "{msg}");
    assert_eq!(a.doc.shapes.iter().filter(|s| s.is_image()).count(), 3);
    // Sin imágenes, se explica qué hacer.
    let mut b = app();
    assert!(b.make_sticker_sheet(2.0, None).contains("imagen"));
}

#[test]
fn pack_moves_pieces_into_the_page_without_overlap() {
    let mut a = app();
    for (x, y) in [(400.0, 10.0), (-90.0, 300.0), (20.0, 20.0)] {
        a.doc.add_shape(square(x, y, 40.0));
    }
    select_all(&mut a);
    a.object_run(Cmd::Pack);
    let boxes: Vec<Rect> = a
        .doc
        .shapes
        .iter()
        .map(oc_core::edit::shape_bounds)
        .collect();
    assert_packed(&a.doc, &boxes);
    // Deshacer lo devuelve todo.
    a.undo();
    assert_eq!(a.doc.shapes[0].rect.x0, 400.0);
}

// ---------------------------------------------------------------- Simulación del corte

#[test]
fn cut_simulation_shows_time_and_plays() {
    let mut h = send_harness_with(vec![square(10.0, 20.0, 30.0)]);
    assert!(h.query_by_label_contains("Tiempo estimado: ~").is_some());
    click(&mut h, "Simular corte");
    let (total, moves) = {
        let sim = h
            .state()
            .canvas
            .send
            .simulation
            .as_ref()
            .expect("simulando");
        (sim.total_s, sim.moves.len())
    };
    assert!(total > 0.0 && moves >= 5);
    h.run_steps(10);
    let t0 = {
        let sim = h.state().canvas.send.simulation.as_ref().unwrap();
        sim.at(h.ctx.input(|i| i.time))
    };
    assert!(t0 > 0.0, "avanza");
    click(&mut h, "×20");
    let sim = h.state().canvas.send.simulation.as_ref().unwrap();
    assert_eq!(sim.speed, 20.0);
    assert!(
        sim.at(h.ctx.input(|i| i.time)) >= t0,
        "sigue desde donde iba"
    );
    click(&mut h, "Parar");
    assert!(h.state().canvas.send.simulation.is_none());
}

// ---------------------------------------------------------------- Historial y cuchillas

#[test]
fn history_and_blade_wear_are_recorded() {
    let mut h = send_harness_with(vec![square(10.0, 20.0, 50.0)]);
    let cut = |h: &mut egui_kittest::Harness<'static, crate::app::OpenCraftCutterApp>,
               r: oc_core::usage::JobResult,
               part: f64| {
        let a = h.state_mut();
        let jobs = a.jobs_to_send(false);
        a.start_history(&jobs, false);
        a.finish_history(r, part);
    };
    cut(&mut h, oc_core::usage::JobResult::Done, 1.0);
    cut(&mut h, oc_core::usage::JobResult::Cancelled, 0.5);
    let a = h.state();
    assert_eq!(a.prefs.history.len(), 2);
    assert_eq!(
        a.prefs.history[0].result,
        Some(oc_core::usage::JobResult::Cancelled)
    );
    // Cuadrado de 50 mm = 0,2 m; el cancelado a medias suma 0,1 m.
    assert!((a.prefs.history[1].meters - 0.2).abs() < 1e-9);
    let blade = &a.prefs.blades[0];
    assert_eq!(blade.tool, oc_core::send::Tool::AutoBlade);
    assert!((blade.meters - 0.3).abs() < 1e-9);
    assert!(a.blade_warning().is_none());

    // Pasado el recordatorio, Enviar avisa.
    h.state_mut().prefs.blade_life_m = 0.25;
    h.run_steps(3);
    assert!(
        h.state()
            .blade_warning()
            .unwrap()
            .contains("cuchilla automática lleva 0,3 m")
    );
    assert!(
        h.query_by_label_contains("Si los cortes empiezan a fallar")
            .is_some()
    );

    // En la ventana se ve el historial y se marca el cambio de cuchilla.
    click(&mut h, "Historial de cortes y cuchillas");
    assert!(h.query_by_label_contains("Cancelado").is_some());
    click(&mut h, "La he cambiado");
    assert_eq!(h.state().prefs.blades[0].meters, 0.0);
    assert!(h.state().blade_warning().is_none());
}

// ---------------------------------------------------------------- Insertar

#[test]
fn qr_dialog_inserts_a_centered_code() {
    let mut h = app_harness();
    h.state_mut().insert_dialog = Some(crate::insert::InsertDialog::Qr {
        text: String::new(),
        size_mm: 30.0,
    });
    h.run_steps(3);
    h.get_by_role(egui::accesskit::Role::MultilineTextInput)
        .focus();
    h.run_steps(2);
    h.get_by_role(egui::accesskit::Role::MultilineTextInput)
        .type_text("https://opencraftcutter.example");
    h.run_steps(3);
    h.get_by_label("Insertar").click();
    h.run_steps(3);
    let a = h.state();
    assert!(a.insert_dialog.is_none(), "se cierra al insertar");
    let s = &a.doc.shapes[0];
    assert!(s.name.starts_with("Código QR"));
    let c = s.rect.center();
    assert!((c.x - a.doc.page.width_mm / 2.0).abs() < 1e-6);
    assert!((s.rect.width() - 30.0).abs() < 1e-6);
    assert_eq!(a.doc.selected, vec![s.id]);
}

// ---------------------------------------------------------------- Texto sobre trazado

#[test]
fn text_is_placed_around_a_circle() {
    let circle = Shape::new(
        0,
        "Círculo",
        Rect::new(50.0, 50.0, 110.0, 110.0),
        ShapeKind::Ellipse,
    );
    let mut h = panel(
        crate::app::Panel::Text,
        vec![text("OPENCRAFTCUTTER"), circle],
        &[0, 1],
    );
    click(&mut h, "Colocar sobre el trazado");
    let a = h.state();
    assert!(
        a.doc.shapes.iter().all(|s| !s.is_text()),
        "el texto pasa a trazado"
    );
    let placed = a
        .doc
        .shapes
        .iter()
        .find(|s| s.name.ends_with("(sobre trazado)"))
        .expect("texto sobre el trazado");
    let original = text("OPENCRAFTCUTTER");
    assert_eq!(
        (placed.fill, placed.stroke),
        (original.fill, original.stroke),
        "conserva el estilo"
    );
    // Por fuera del círculo (radio 30), a 1 mm de la línea: ningún punto dentro.
    let c = kurbo::Point::new(80.0, 80.0);
    let pts: Vec<kurbo::Point> = placed
        .world_subpaths()
        .into_iter()
        .flat_map(|(p, _)| p)
        .collect();
    assert!(!pts.is_empty());
    assert!(pts.iter().all(|p| p.distance(c) > 30.5), "por fuera");
    assert!(
        // Radio 30 + 1 mm de separación + altura de las mayúsculas (36 pt ≈ 9 mm).
        pts.iter().all(|p| p.distance(c) < 30.0 + 1.0 + 10.0),
        "pegado a la línea"
    );
    // Deshacer devuelve el texto editable.
    h.state_mut().undo();
    assert!(h.state().doc.shapes.iter().any(|s| s.is_text()));
}

#[test]
fn single_line_font_text_is_placed_on_a_path() {
    // La fuente de un trazo no es del sistema: antes fallaba con «No se pudo leer la fuente».
    let mut t = text("CAMEO");
    if let ShapeKind::Text { font_family, .. } = &mut t.kind {
        *font_family = oc_core::singleline::FAMILY.into();
    }
    let circle = Shape::new(
        0,
        "Círculo",
        Rect::new(50.0, 50.0, 110.0, 110.0),
        ShapeKind::Ellipse,
    );
    let mut a = app();
    let tid = a.doc.add_shape(t);
    let cid = a.doc.add_shape(circle);
    a.text_on_path(tid, cid, 0.75, 1.0, false)
        .expect("se coloca con la fuente de un trazo");
    let placed = a
        .doc
        .shapes
        .iter()
        .find(|s| s.name.ends_with("(sobre trazado)"))
        .expect("texto sobre el trazado");
    let paths = placed.world_subpaths();
    assert!(!paths.is_empty());
    assert!(
        paths.iter().all(|(_, closed)| !closed),
        "líneas abiertas para rotulador"
    );
    // Por fuera del círculo (radio 30), pegado a la línea.
    let c = kurbo::Point::new(80.0, 80.0);
    let pts: Vec<kurbo::Point> = paths.into_iter().flat_map(|(p, _)| p).collect();
    assert!(
        pts.iter()
            .all(|p| p.distance(c) > 30.5 && p.distance(c) < 41.0)
    );
}

// ---------------------------------------------------------------- Diseños grandes

#[test]
fn big_design_is_split_into_mat_sized_parts() {
    let mut wide = square(20.0, 20.0, 50.0);
    wide.rect = Rect::new(20.0, 20.0, 520.0, 70.0);
    let line = {
        let pts = vec![(
            vec![
                kurbo::Point::new(20.0, 100.0),
                kurbo::Point::new(520.0, 100.0),
            ],
            false,
        )];
        let (rect, subpaths) = Shape::normalize_subpaths(&pts);
        Shape::new(
            0,
            "Línea",
            rect,
            ShapeKind::Path {
                subpaths,
                fill_rule: oc_core::FillRule::NonZero,
            },
        )
    };
    let mut h = panel(crate::app::Panel::PageSetup, vec![wide, line], &[]);
    click(&mut h, "Dividir en partes");
    let a = h.state();
    assert_eq!(
        a.current_part(),
        Some((1, 2)),
        "500 mm en un tapete de 12″: 2 partes"
    );
    let m = a.doc.page.cut_margin_mm;
    for s in &a.doc.shapes {
        let (k, _) = crate::parts::part_of(s).expect("todo pertenece a una parte");
        assert_eq!(s.visible, k == 1, "solo se ve la parte 1");
        let b = oc_core::edit::shape_bounds(s);
        assert!(
            b.x0 >= m - 1e-6 && b.x1 <= m + 299.0 + 1e-6,
            "{b:?} cabe en el tapete"
        );
    }
    // Rectángulo y línea en cada parte.
    let in_part = |k: usize| {
        a.doc
            .shapes
            .iter()
            .filter(|s| crate::parts::part_of(s).map(|p| p.0) == Some(k))
            .count()
    };
    assert_eq!((in_part(1), in_part(2)), (2, 2));
    // La línea abierta se recorta: la parte 1 llega justo al borde de la parte.
    let line1 = a
        .doc
        .shapes
        .iter()
        .find(|s| s.name.ends_with("Línea") && crate::parts::part_of(s) == Some((1, 2)))
        .unwrap();
    assert!((oc_core::edit::shape_bounds(line1).width() - 299.0).abs() < 1e-6);

    h.get_by_label("Parte siguiente").click();
    h.run_steps(3);
    assert_eq!(h.state().current_part(), Some((2, 2)));
    // Deshacer devuelve el diseño entero.
    h.state_mut().undo();
    assert_eq!(h.state().current_part(), None);
}

#[test]
fn template_dialog_inserts_cut_and_fold_lines() {
    let mut h = app_harness();
    h.state_mut().insert_dialog = Some(crate::insert::InsertDialog::Template {
        kind: 0,
        dims: crate::insert::template_defaults(0),
    });
    h.run_steps(3);
    h.get_by_label("Insertar").click();
    h.run_steps(3);
    let a = h.state();
    assert!(a.insert_dialog.is_none());
    assert_eq!(a.doc.shapes.len(), 2, "corte y dobleces");
    let (cut, fold) = (&a.doc.shapes[0], &a.doc.shapes[1]);
    assert_eq!(cut.group, fold.group);
    assert!(cut.group.is_some());
    assert_ne!(
        cut.stroke.color, fold.stroke.color,
        "colores distintos para asignar acciones"
    );
    assert_eq!(fold.stroke.style, oc_core::shape::LineStyle::Dashed);
    // Caja de 60×40×25: abierta mide 110×90, en la esquina del área útil.
    let area = oc_core::send::safe_area(&a.doc);
    let b = oc_core::edit::shape_bounds(cut);
    assert!(
        (b.width() - 110.0).abs() < 1e-6 && (b.height() - 90.0).abs() < 1e-6,
        "{b:?}"
    );
    assert_eq!(b.origin(), area.origin());

    // Las demás plantillas también se insertan (la etiqueta, sin dobleces).
    let mut b = app();
    for kind in 1..4 {
        let folds = b.insert_template(kind, crate::insert::template_defaults(kind));
        assert_eq!(folds, kind != 3, "plantilla {kind}");
    }
}

// ---------------------------------------------------------------- Fuente de un trazo

#[test]
fn single_line_font_sends_open_strokes() {
    let mut t = text("Año");
    if let ShapeKind::Text { font_family, .. } = &mut t.kind {
        *font_family = oc_core::singleline::FAMILY.into();
    }
    // La caja se mide con la fuente nueva.
    let (w, hh) = crate::text::TextStyle::from_kind(&t.kind)
        .map(|st| crate::text::measure(&st))
        .unwrap();
    t.rect = Rect::new(20.0, 80.0, 20.0 + w, 80.0 + hh);
    let mut h = panel(crate::app::Panel::Text, vec![t], &[0]);
    assert!(
        h.get_all_by_label_contains(oc_core::singleline::FAMILY)
            .next()
            .is_some(),
        "aparece en la lista de fuentes"
    );
    h.state_mut().workspace = crate::app::Workspace::Send;
    h.run_steps(3);
    let jobs = h.state_mut().jobs_to_send(false);
    let paths: Vec<_> = jobs.iter().flat_map(|j| j.paths.iter()).collect();
    assert!(paths.len() >= 5, "A, ñ (con su tilde) y o: {}", paths.len());
    assert!(
        paths.iter().all(|(_, closed)| !closed),
        "solo trazos abiertos"
    );
}
