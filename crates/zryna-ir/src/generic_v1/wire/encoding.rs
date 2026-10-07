//! Canonical successor field ordering and closed physical tags.

use super::{Failure, HEADER, MAX_BYTES, VERSION, raw, reserve};

struct Writer {
    bytes: Vec<u8>,
    children: usize,
}

impl Writer {
    fn bytes(&mut self, bytes: &[u8]) -> Result<(), Failure> {
        if self.bytes.len().checked_add(bytes.len()).is_none_or(|length| length > MAX_BYTES) {
            return Err(crate::generic_v1::budget("successor wire byte ceiling exceeded"));
        }
        self.bytes.try_reserve(bytes.len()).map_err(|_| Failure::AllocationFailure)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn byte(&mut self, byte: u8) -> Result<(), Failure> {
        self.bytes(&[byte])
    }
    fn u32(&mut self, value: u32) -> Result<(), Failure> {
        self.bytes(&value.to_le_bytes())
    }
    fn count(&mut self, count: usize) -> Result<(), Failure> {
        self.children =
            self.children.checked_add(count).filter(|sum| *sum <= super::MAX_CHILDREN).ok_or_else(
                || crate::generic_v1::budget("successor wire child ceiling exceeded"),
            )?;
        self.u32(u32::try_from(count).map_err(|_| Failure::InternalFailure)?)
    }
    fn blob(&mut self, bytes: &[u8]) -> Result<(), Failure> {
        self.u32(u32::try_from(bytes.len()).map_err(|_| Failure::InternalFailure)?)?;
        self.bytes(bytes)
    }
    fn span(&mut self, span: zryna_source::UntrustedSpan) -> Result<(), Failure> {
        self.u32(span.file)?;
        self.u32(span.start)?;
        self.u32(span.end)
    }
    fn ty(&mut self, ty: raw::Type) -> Result<(), Failure> {
        match ty {
            raw::Type::Stored(id) => {
                self.byte(0)?;
                self.u32(id)
            }
            raw::Type::Unit => self.byte(1),
            raw::Type::Borrow { referent, exclusive } => {
                self.byte(2)?;
                self.u32(referent)?;
                self.byte(u8::from(exclusive))
            }
        }
    }
    fn definition(&mut self, value: &raw::Definition) -> Result<(), Failure> {
        self.u32(value.id)?;
        self.ty(value.ty)
    }
    fn ids(&mut self, ids: &[u32]) -> Result<(), Failure> {
        self.count(ids.len())?;
        for id in ids {
            self.u32(*id)?;
        }
        Ok(())
    }
    fn optional_id(&mut self, value: Option<u32>) -> Result<(), Failure> {
        self.byte(u8::from(value.is_some()))?;
        if let Some(value) = value {
            self.u32(value)?;
        }
        Ok(())
    }
    fn edge(&mut self, edge: &raw::Edge) -> Result<(), Failure> {
        self.u32(edge.target)?;
        self.ids(&edge.arguments)
    }
    fn operation(&mut self, operation: &raw::Operation) -> Result<(), Failure> {
        match operation {
            raw::Operation::BoolLiteral(value) => {
                self.byte(1)?;
                self.byte(u8::from(*value))
            }
            raw::Operation::I32Literal(value) => {
                self.byte(2)?;
                self.bytes(&value.to_le_bytes())
            }
            raw::Operation::Unit => self.byte(3),
            raw::Operation::Copy { value } => {
                self.byte(4)?;
                self.u32(*value)
            }
            raw::Operation::I32Add { left, right } => {
                self.byte(5)?;
                self.u32(*left)?;
                self.u32(*right)
            }
            raw::Operation::ClosedGenericCall { instance, arguments } => {
                self.byte(6)?;
                self.u32(*instance)?;
                self.ids(arguments)
            }
            raw::Operation::SourceCall { module, function, arguments } => {
                self.byte(7)?;
                self.u32(*module)?;
                self.u32(*function)?;
                self.ids(arguments)
            }
            raw::Operation::ClosedEnumConstruct { ty, ordinal, payload } => {
                self.byte(8)?;
                self.u32(*ty)?;
                self.u32(*ordinal)?;
                self.optional_id(*payload)
            }
        }
    }
    fn terminator(&mut self, terminator: &raw::Terminator) -> Result<(), Failure> {
        match terminator {
            raw::Terminator::Return(value) => {
                self.byte(1)?;
                self.u32(*value)
            }
            raw::Terminator::Jump(edge) => {
                self.byte(2)?;
                self.edge(edge)
            }
            raw::Terminator::Branch { condition, yes, no } => {
                self.byte(3)?;
                self.u32(*condition)?;
                self.edge(yes)?;
                self.edge(no)
            }
            raw::Terminator::ClosedEnumMatch { ty, scrutinee, mode, arms } => {
                self.byte(4)?;
                self.u32(*ty)?;
                self.u32(*scrutinee)?;
                self.byte(match mode {
                    raw::MatchMode::Value => 0,
                    raw::MatchMode::SharedBorrow => 1,
                    raw::MatchMode::ExclusiveBorrow => 2,
                })?;
                self.count(arms.len())?;
                for arm in arms {
                    self.u32(arm.ordinal)?;
                    self.byte(u8::from(arm.binding.is_some()))?;
                    if let Some(binding) = &arm.binding {
                        self.definition(binding)?;
                    }
                    self.edge(&arm.edge)?;
                }
                Ok(())
            }
        }
    }
}

pub(super) fn encode(program: &raw::Program) -> Result<Vec<u8>, Failure> {
    let mut writer = Writer { bytes: reserve(HEADER.len() + 4)?, children: 0 };
    writer.bytes(HEADER)?;
    writer.u32(VERSION)?;
    writer.count(program.modules.len())?;
    for module in &program.modules {
        writer.u32(module.id)?;
        writer.u32(module.functions)?;
    }
    writer.count(program.declarations.len())?;
    for declaration in &program.declarations {
        writer.u32(declaration.module)?;
        writer.u32(declaration.function)?;
        writer.u32(declaration.parameters)?;
        writer.span(declaration.span)?;
    }
    writer.count(program.type_keys.len())?;
    for key in &program.type_keys {
        writer.blob(key)?;
    }
    writer.bytes(&program.universe)?;
    writer.bytes(&program.linear32)?;
    writer.bytes(&program.linux_x86_64)?;
    writer.count(program.functions.len())?;
    for function in &program.functions {
        writer.blob(&function.key)?;
        writer.span(function.span)?;
        writer.byte(u8::from(function.public_export.is_some()))?;
        if let Some(name) = &function.public_export {
            writer.blob(name.as_bytes())?;
        }
        writer.count(function.parameters.len())?;
        for parameter in &function.parameters {
            writer.ty(*parameter)?;
        }
        writer.ty(function.result)?;
        writer.count(function.blocks.len())?;
        for block in &function.blocks {
            writer.u32(block.id)?;
            writer.count(block.parameters.len())?;
            for parameter in &block.parameters {
                writer.definition(parameter)?;
            }
            writer.count(block.instructions.len())?;
            for instruction in &block.instructions {
                writer.definition(&instruction.result)?;
                writer.span(instruction.span)?;
                writer.operation(&instruction.operation)?;
            }
            writer.span(block.span)?;
            writer.terminator(&block.terminator)?;
        }
    }
    Ok(writer.bytes)
}
