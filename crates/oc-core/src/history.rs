//! Historial de deshacer/rehacer por instantáneas del documento.
//!
//! El documento es pequeño (decenas o cientos de formas), así que clonar es barato y
//! mucho más simple y robusto que comandos invertibles.

use crate::document::Document;

const LIMIT: usize = 100;

#[derive(Debug, Default)]
pub struct History {
    past: Vec<Document>,
    future: Vec<Document>,
}

impl History {
    /// Guarda el estado actual antes de una mutación. Invalida el futuro.
    pub fn push(&mut self, current: &Document) {
        if self.past.len() >= LIMIT {
            self.past.remove(0);
        }
        self.past.push(current.clone());
        self.future.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn undo(&mut self, current: &mut Document) -> bool {
        let Some(prev) = self.past.pop() else {
            return false;
        };
        self.future.push(std::mem::replace(current, prev));
        Self::prune_selection(current);
        true
    }

    pub fn redo(&mut self, current: &mut Document) -> bool {
        let Some(next) = self.future.pop() else {
            return false;
        };
        self.past.push(std::mem::replace(current, next));
        Self::prune_selection(current);
        true
    }

    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
    }

    fn prune_selection(doc: &mut Document) {
        let ids: Vec<_> = doc.shapes.iter().map(|s| s.id).collect();
        doc.selected.retain(|id| ids.contains(id));
    }
}

impl History {
    /// Descarta el futuro (tras un deshacer interno que no debe poder rehacerse).
    pub fn clear_future(&mut self) {
        self.future.clear();
    }
}
