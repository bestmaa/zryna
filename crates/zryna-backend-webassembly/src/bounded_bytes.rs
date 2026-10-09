use super::{control_flow_budget_error, control_flow_index_error};
use zryna_diagnostics::Diagnostic;

pub(super) struct BoundedBytes {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedBytes {
    pub(super) const fn new(limit: usize) -> Self {
        Self { bytes: Vec::new(), limit }
    }

    pub(super) fn finish(self) -> Vec<u8> {
        self.bytes
    }

    pub(super) fn byte(&mut self, byte: u8) -> Result<(), Diagnostic> {
        self.extend(&[byte])
    }

    pub(super) fn instruction(&mut self, opcode: u8) -> Result<(), Diagnostic> {
        self.byte(opcode)
    }

    pub(super) fn extend(&mut self, bytes: &[u8]) -> Result<(), Diagnostic> {
        let length = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| control_flow_budget_error(self.limit))?;
        if length > self.limit {
            return Err(control_flow_budget_error(self.limit));
        }
        self.bytes.try_reserve(bytes.len()).map_err(|_| control_flow_budget_error(self.limit))?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    pub(super) fn section(&mut self, id: u8, payload: &[u8]) -> Result<(), Diagnostic> {
        self.byte(id)?;
        self.u32(u32::try_from(payload.len()).map_err(|_| control_flow_index_error())?)?;
        self.extend(payload)
    }

    pub(super) fn name(&mut self, name: &str) -> Result<(), Diagnostic> {
        self.u32(u32::try_from(name.len()).map_err(|_| control_flow_index_error())?)?;
        self.extend(name.as_bytes())
    }

    pub(super) fn local_get(&mut self, index: u32) -> Result<(), Diagnostic> {
        self.instruction(0x20)?;
        self.u32(index)
    }

    pub(super) fn local_set(&mut self, index: u32) -> Result<(), Diagnostic> {
        self.instruction(0x21)?;
        self.u32(index)
    }

    pub(super) fn branch(&mut self, opcode: u8, depth: u32) -> Result<(), Diagnostic> {
        self.instruction(opcode)?;
        self.u32(depth)
    }

    pub(super) fn u32(&mut self, mut value: u32) -> Result<(), Diagnostic> {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            self.byte(byte)?;
            if value == 0 {
                return Ok(());
            }
        }
    }

    pub(super) fn i32_const(&mut self, value: i32) -> Result<(), Diagnostic> {
        self.instruction(0x41)?;
        self.i32(value)
    }

    fn i32(&mut self, mut value: i32) -> Result<(), Diagnostic> {
        loop {
            let mut byte = value.to_le_bytes()[0] & 0x7f;
            value >>= 7;
            let sign = byte & 0x40 != 0;
            let done = (value == 0 && !sign) || (value == -1 && sign);
            if !done {
                byte |= 0x80;
            }
            self.byte(byte)?;
            if done {
                return Ok(());
            }
        }
    }
}
