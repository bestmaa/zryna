//! Syntax owns only Y failures; declaration failures enter after the project-wide barrier.

use super::{DeclarationError, arena};

#[derive(Default)]
pub(super) struct Errors {
    items: Vec<DeclarationError>,
    terminal: bool,
}

fn key(error: &DeclarationError) -> (bool, u32, u32, u32, &'static str) {
    error.span.map_or((true, 0, 0, 0, error.code), |span| {
        (false, span.file().index(), span.start(), span.end(), error.code)
    })
}

impl Errors {
    pub(super) fn push(&mut self, error: DeclarationError) {
        if self.terminal || self.items.iter().any(|existing| key(existing) == key(&error)) {
            return;
        }
        if error.code == "ZRYNA-Y5201" {
            self.terminal = true;
            self.items.push(error);
        } else if self.items.len() == crate::v4::MAX_VALIDATION_ERRORS - 1 {
            self.terminal = true;
            self.items.push(arena::budget());
        } else {
            self.items.push(error);
        }
    }

    pub(super) fn terminal(&self) -> bool {
        self.terminal
    }

    pub(super) fn finish(mut self) -> Vec<DeclarationError> {
        self.items.sort_by_key(key);
        self.items
    }
}
