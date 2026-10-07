//! Bounded exact decoder; counts are checked before reserving typed vectors.

use super::{Failure, HEADER, MAX_CHILDREN, VERSION, invalid, raw, reserve};

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
    children: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], Failure> {
        let end = self.cursor.checked_add(length).ok_or_else(invalid)?;
        let bytes = self.bytes.get(self.cursor..end).ok_or_else(invalid)?;
        self.cursor = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, Failure> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, Failure> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| invalid())?))
    }
    fn boolean(&mut self) -> Result<bool, Failure> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid()),
        }
    }
    fn array(&mut self) -> Result<[u8; 32], Failure> {
        self.take(32)?.try_into().map_err(|_| invalid())
    }
    fn blob(&mut self, limit: usize) -> Result<Vec<u8>, Failure> {
        let length = usize::try_from(self.u32()?).map_err(|_| invalid())?;
        if length > limit {
            return Err(crate::generic_v1::budget("successor wire field byte ceiling exceeded"));
        }
        let bytes = self.take(length)?;
        let mut value = reserve(length)?;
        value.extend_from_slice(bytes);
        Ok(value)
    }
    fn vector<T>(
        &mut self,
        limit: usize,
        minimum: usize,
        mut read: impl FnMut(&mut Self) -> Result<T, Failure>,
    ) -> Result<Vec<T>, Failure> {
        let count = usize::try_from(self.u32()?).map_err(|_| invalid())?;
        if count > limit || self.children.checked_add(count).is_none_or(|sum| sum > MAX_CHILDREN) {
            return Err(crate::generic_v1::budget("successor wire child/count ceiling exceeded"));
        }
        if count.checked_mul(minimum).is_none_or(|bytes| bytes > self.bytes.len() - self.cursor) {
            return Err(invalid());
        }
        self.children += count;
        let mut values = reserve(count)?;
        for _ in 0..count {
            values.push(read(self)?);
        }
        Ok(values)
    }
    fn span(&mut self) -> Result<zryna_source::UntrustedSpan, Failure> {
        Ok(zryna_source::UntrustedSpan { file: self.u32()?, start: self.u32()?, end: self.u32()? })
    }
    fn ty(&mut self) -> Result<raw::Type, Failure> {
        match self.byte()? {
            0 => Ok(raw::Type::Stored(self.u32()?)),
            1 => Ok(raw::Type::Unit),
            2 => Ok(raw::Type::Borrow { referent: self.u32()?, exclusive: self.boolean()? }),
            _ => Err(invalid()),
        }
    }
    fn definition(&mut self) -> Result<raw::Definition, Failure> {
        Ok(raw::Definition { id: self.u32()?, ty: self.ty()? })
    }
    fn ids(&mut self) -> Result<Vec<u32>, Failure> {
        self.vector(256, 4, Self::u32)
    }
    fn optional_id(&mut self) -> Result<Option<u32>, Failure> {
        if self.boolean()? { Ok(Some(self.u32()?)) } else { Ok(None) }
    }
    fn edge(&mut self) -> Result<raw::Edge, Failure> {
        Ok(raw::Edge { target: self.u32()?, arguments: self.ids()? })
    }
    fn operation(&mut self) -> Result<raw::Operation, Failure> {
        match self.byte()? {
            1 => Ok(raw::Operation::BoolLiteral(self.boolean()?)),
            2 => Ok(raw::Operation::I32Literal(i32::from_le_bytes(
                self.take(4)?.try_into().map_err(|_| invalid())?,
            ))),
            3 => Ok(raw::Operation::Unit),
            4 => Ok(raw::Operation::Copy { value: self.u32()? }),
            5 => Ok(raw::Operation::I32Add { left: self.u32()?, right: self.u32()? }),
            6 => Ok(raw::Operation::ClosedGenericCall {
                instance: self.u32()?,
                arguments: self.ids()?,
            }),
            7 => Ok(raw::Operation::SourceCall {
                module: self.u32()?,
                function: self.u32()?,
                arguments: self.ids()?,
            }),
            8 => Ok(raw::Operation::ClosedEnumConstruct {
                ty: self.u32()?,
                ordinal: self.u32()?,
                payload: self.optional_id()?,
            }),
            _ => Err(invalid()),
        }
    }
    fn terminator(&mut self) -> Result<raw::Terminator, Failure> {
        match self.byte()? {
            1 => Ok(raw::Terminator::Return(self.u32()?)),
            2 => Ok(raw::Terminator::Jump(self.edge()?)),
            3 => Ok(raw::Terminator::Branch {
                condition: self.u32()?,
                yes: self.edge()?,
                no: self.edge()?,
            }),
            4 => {
                let ty = self.u32()?;
                let scrutinee = self.u32()?;
                let mode = match self.byte()? {
                    0 => raw::MatchMode::Value,
                    1 => raw::MatchMode::SharedBorrow,
                    2 => raw::MatchMode::ExclusiveBorrow,
                    _ => return Err(invalid()),
                };
                let arms = self.vector(1024, 10, |reader| {
                    let ordinal = reader.u32()?;
                    let binding = if reader.boolean()? { Some(reader.definition()?) } else { None };
                    Ok(raw::Arm { ordinal, binding, edge: reader.edge()? })
                })?;
                Ok(raw::Terminator::ClosedEnumMatch { ty, scrutinee, mode, arms })
            }
            _ => Err(invalid()),
        }
    }
    fn function(&mut self) -> Result<raw::Function, Failure> {
        let key = self.blob(4096)?;
        let span = self.span()?;
        let public_export = if self.boolean()? {
            Some(String::from_utf8(self.blob(256)?).map_err(|_| invalid())?)
        } else {
            None
        };
        let parameters = self.vector(256, 1, Self::ty)?;
        let result = self.ty()?;
        let blocks = self.vector(4096, 25, |reader| {
            let id = reader.u32()?;
            let parameters = reader.vector(256, 5, Self::definition)?;
            let instructions = reader.vector(16384, 18, |reader| {
                Ok(raw::Instruction {
                    result: reader.definition()?,
                    span: reader.span()?,
                    operation: reader.operation()?,
                })
            })?;
            Ok(raw::Block {
                id,
                parameters,
                instructions,
                span: reader.span()?,
                terminator: reader.terminator()?,
            })
        })?;
        Ok(raw::Function { key, span, public_export, parameters, result, blocks })
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<raw::Program, Failure> {
    let mut reader = Reader { bytes, cursor: 0, children: 0 };
    if reader.take(HEADER.len())? != HEADER || reader.u32()? != VERSION {
        return Err(invalid());
    }
    let modules = reader.vector(4096, 8, |reader| {
        Ok(raw::Module { id: reader.u32()?, functions: reader.u32()? })
    })?;
    let declarations = reader.vector(16384, 24, |reader| {
        Ok(raw::Declaration {
            module: reader.u32()?,
            function: reader.u32()?,
            parameters: reader.u32()?,
            span: reader.span()?,
        })
    })?;
    let type_keys = reader.vector(65536, 5, |reader| reader.blob(4096))?;
    let universe = reader.array()?;
    let linear32 = reader.array()?;
    let linux_x86_64 = reader.array()?;
    let functions = reader.vector(16384, 26, Reader::function)?;
    if reader.cursor != bytes.len() {
        return Err(invalid());
    }
    Ok(raw::Program {
        modules,
        declarations,
        type_keys,
        universe,
        linear32,
        linux_x86_64,
        functions,
    })
}
