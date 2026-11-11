//! «Fotografía» de los materiales incluidos (valores copiados de la referencia). Si cambia un
//! valor sin querer, la prueba falla. Para actualizarla a propósito:
//! `OPENCRAFTCUTTER_UPDATE_GOLDEN=1 cargo test -p oc-core --test golden_materials`.

use oc_core::send::builtin_materials;

#[test]
fn builtin_materials_are_unchanged() {
    let mut out = String::new();
    for m in builtin_materials() {
        out.push_str(&format!("{} [{:?}]\n", m.name, m.family));
        for e in &m.entries {
            let s = &e.settings;
            out.push_str(&format!(
                "  {:?} {:?}: presión {} · velocidad {} · pasadas {} · aceleración {} · \
                 profundidad {} · sobrecorte {} · optimización {}\n",
                e.holder,
                e.action,
                s.force,
                s.speed,
                s.passes,
                s.accel,
                s.depth,
                s.overcut,
                s.path_optimization
            ));
        }
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/materiales.txt");
    if std::env::var("OPENCRAFTCUTTER_UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, &out).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).expect("falta materiales.txt");
    if expected != out {
        let diff: Vec<String> = expected
            .lines()
            .zip(out.lines())
            .filter(|(e, a)| e != a)
            .take(10)
            .map(|(e, a)| format!("  antes: {e}\n  ahora: {a}"))
            .collect();
        panic!("los materiales incluidos cambiaron:\n{}", diff.join("\n"));
    }
}
