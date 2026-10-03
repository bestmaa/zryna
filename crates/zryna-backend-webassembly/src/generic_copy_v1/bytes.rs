//! Fallible incremental core binary encoding, bounded before every append.

use super::{MAX_BYTES, budget};
use wasm_encoder::{Encode, Instruction};
use zryna_diagnostics::Diagnostic;

pub(super) struct Bytes {
    pub(super) bytes: Vec<u8>,
}

impl Bytes {
    pub(super) const fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    pub(super) fn extend(&mut self, bytes: &[u8]) -> Result<(), Diagnostic> {
        if self.bytes.len().checked_add(bytes.len()).is_none_or(|n| n > MAX_BYTES) {
            return Err(budget());
        }
        self.bytes.try_reserve(bytes.len()).map_err(|_| budget())?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub(super) fn u32(&mut self, value: u32) -> Result<(), Diagnostic> {
        let mut encoded = Vec::new();
        encoded.try_reserve(5).map_err(|_| budget())?;
        value.encode(&mut encoded);
        self.extend(&encoded)
    }
    pub(super) fn count(&mut self, value: usize) -> Result<(), Diagnostic> {
        self.u32(u32::try_from(value).map_err(|_| budget())?)
    }
    pub(super) fn op(&mut self, instruction: &Instruction<'_>) -> Result<(), Diagnostic> {
        // Only fixed-size instructions from the closed encoder are passed here.
        let mut encoded = Vec::new();
        encoded.try_reserve(12).map_err(|_| budget())?;
        instruction.encode(&mut encoded);
        self.extend(&encoded)
    }
    pub(super) fn section(&mut self, id: u8, payload: &Self) -> Result<(), Diagnostic> {
        self.extend(&[id])?;
        self.count(payload.bytes.len())?;
        self.extend(&payload.bytes)
    }
    pub(super) fn name(&mut self, name: &str) -> Result<(), Diagnostic> {
        self.count(name.len())?;
        self.extend(name.as_bytes())
    }
}
