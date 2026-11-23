//! Red de seguridad para los cálculos de geometría: una forma con coordenadas no válidas (o un
//! caso que la biblioteca de polígonos no resuelve) no debe cerrar la app. El cálculo se
//! abandona, se devuelve un resultado neutro y se anota el fallo para avisar al usuario.

use std::cell::Cell;
use std::sync::Mutex;

thread_local! {
    /// Cálculos protegidos en curso en este hilo.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// ¿Se está ejecutando un cálculo protegido en este hilo? Un pánico ahora no cierra la app (lo
/// usa el registro de errores para distinguirlo de un cierre).
pub fn is_guarding() -> bool {
    DEPTH.with(|d| d.get() > 0)
}

/// Último cálculo que falló y aún no se ha mostrado.
static LAST_FAILURE: Mutex<Option<&'static str>> = Mutex::new(None);

/// Ejecuta `f`; si entra en pánico, devuelve `fallback()` y anota `what` (el nombre del cálculo,
/// para el aviso).
pub fn guarded<T>(what: &'static str, fallback: impl FnOnce() -> T, f: impl FnOnce() -> T) -> T {
    DEPTH.with(|d| d.set(d.get() + 1));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    match result {
        Ok(v) => v,
        Err(_) => {
            if let Ok(mut last) = LAST_FAILURE.lock() {
                *last = Some(what);
            }
            fallback()
        }
    }
}

/// Nombre del último cálculo que falló desde la llamada anterior, si lo hubo.
pub fn take_failure() -> Option<&'static str> {
    LAST_FAILURE.lock().ok().and_then(|mut last| last.take())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_calculation_returns_the_fallback_and_is_reported() {
        let _ = take_failure();
        assert_eq!(guarded("suma", || 0, || 2 + 2), 4);
        assert_eq!(take_failure(), None);
        let v = guarded("contorno", Vec::<u8>::new, || {
            panic!("coordenadas no válidas")
        });
        assert!(v.is_empty());
        assert_eq!(take_failure(), Some("contorno"));
        assert_eq!(take_failure(), None, "se informa una sola vez");
        assert!(!is_guarding());
        assert!(guarded("anidado", || false, is_guarding));
    }
}
