//! Bound syntax, reference depth and prospective identities before recursive decoding.

use std::{collections::BTreeMap, sync::Arc};

use wasmparser::{
    BinaryReader, ComponentAlias, ComponentExternName, ComponentTypeRef, ComponentValType,
    TypeBounds,
};
use zryna_diagnostics::Diagnostic;

use super::type_indices::{Interface, Shape, TypeIndices};

#[derive(Default)]
pub(crate) struct TypeBudget {
    indices: TypeIndices,
    entries: usize,
}

impl TypeBudget {
    pub(super) fn identities(&self) -> usize {
        self.indices.identities()
    }

    pub(crate) fn section(&mut self, bytes: &[u8], offset: u64) -> Result<(), Diagnostic> {
        let mut reader = BinaryReader::new(bytes, offset);
        let count = self.count(&mut reader)?;
        for _ in 0..count {
            let shape = self.ty(&mut reader, None)?;
            self.indices.outer.push(shape);
        }
        if !reader.eof() {
            return Err(invalid("command type section has trailing bytes"));
        }
        Ok(())
    }

    pub(crate) fn alias(&mut self, alias: &ComponentAlias<'_>) -> Result<(), Diagnostic> {
        self.indices.charge(1)?;
        let shape = self.indices.alias(None, alias)?;
        self.indices.outer.push(shape);
        Ok(())
    }

    pub(crate) fn interface_use(&mut self, index: u32, import: bool) -> Result<(), Diagnostic> {
        self.indices.interface_use(index, import)
    }

    fn count(&mut self, reader: &mut BinaryReader<'_>) -> Result<u32, Diagnostic> {
        let count = reader.read_var_u32().map_err(malformed)?;
        if count > 4096 || count as usize > 16384 - self.entries {
            return Err(invalid("command type declarations exceed their envelope"));
        }
        self.entries += count as usize;
        Ok(count)
    }

    fn ty(
        &mut self,
        reader: &mut BinaryReader<'_>,
        scope: Option<&[Shape]>,
    ) -> Result<Shape, Diagnostic> {
        self.indices.charge(1)?;
        let tag = reader.read_u8().map_err(malformed)?;
        if tag == 0x42 && scope.is_none() {
            return self.instance(reader);
        }
        let mut depth = 0;
        match tag {
            0x40 => {
                for _ in 0..self.count(reader)? {
                    name(reader)?;
                    self.value(reader, scope)?;
                }
                match reader.read_u8().map_err(malformed)? {
                    0 => {
                        self.value(reader, scope)?;
                    }
                    1 if reader.read_u8().map_err(malformed)? == 0 => {}
                    _ => return Err(invalid("command function has unsupported results")),
                }
                return Ok(Shape::Function);
            }
            0x72 | 0x71 | 0x6f => {
                for _ in 0..self.count(reader)? {
                    if tag != 0x6f {
                        name(reader)?;
                    }
                    let child = if tag == 0x71 {
                        self.optional(reader, scope)?
                    } else {
                        self.value(reader, scope)?
                    };
                    depth = depth.max(child);
                    if tag == 0x71 && reader.read_u8().map_err(malformed)? != 0 {
                        return Err(invalid("command variant has an unsupported refinement"));
                    }
                }
            }
            0x6e | 0x6d => {
                for _ in 0..self.count(reader)? {
                    name(reader)?;
                }
            }
            0x70 | 0x6b => depth = self.value(reader, scope)?,
            0x6a => depth = self.optional(reader, scope)?.max(self.optional(reader, scope)?),
            0x69 | 0x68 => {
                let index = reader.read_var_u32().map_err(malformed)?;
                depth = self.indices.value(scope, ComponentValType::Type(index))?;
            }
            0x73..=0x7f => return TypeIndices::defined(0),
            _ => {
                return Err(invalid(
                    "command type has unsupported nesting or a non-profile definition",
                ));
            }
        }
        TypeIndices::defined(depth + 1)
    }

    fn instance(&mut self, reader: &mut BinaryReader<'_>) -> Result<Shape, Diagnostic> {
        let mut local = Vec::new();
        let mut exports = BTreeMap::new();
        for _ in 0..self.count(reader)? {
            match reader.read_u8().map_err(malformed)? {
                0x01 => {
                    let shape = self.ty(reader, Some(&local))?;
                    local.push(shape);
                }
                0x02 => {
                    self.indices.charge(1)?;
                    let alias = reader.read().map_err(malformed)?;
                    let shape = self.indices.alias(Some(&local), &alias)?;
                    local.push(shape);
                }
                0x04 => {
                    let name: ComponentExternName<'_> = reader.read().map_err(malformed)?;
                    external_name(name)?;
                    match reader.read::<ComponentTypeRef>().map_err(malformed)? {
                        ComponentTypeRef::Func(index)
                            if matches!(TypeIndices::get(&local, index)?, Shape::Function) => {}
                        ComponentTypeRef::Type(bound) => {
                            self.indices.charge(1)?;
                            let shape = match bound {
                                TypeBounds::Eq(index) => {
                                    TypeIndices::aliased(TypeIndices::get(&local, index)?)?
                                }
                                TypeBounds::SubResource => TypeIndices::defined(0)?,
                            };
                            if exports.insert(name.name.to_owned(), shape.clone()).is_some() {
                                return Err(invalid("command instance has duplicate type exports"));
                            }
                            local.push(shape);
                        }
                        _ => {
                            return Err(invalid(
                                "command instance type exports an unsupported kind",
                            ));
                        }
                    }
                }
                _ => return Err(invalid("command instance type has an unsupported declaration")),
            }
        }
        Ok(Shape::Instance(Arc::new(Interface { exports, identities: local.len() })))
    }

    fn value(
        &self,
        reader: &mut BinaryReader<'_>,
        scope: Option<&[Shape]>,
    ) -> Result<usize, Diagnostic> {
        self.indices.value(scope, reader.read().map_err(malformed)?)
    }

    fn optional(
        &self,
        reader: &mut BinaryReader<'_>,
        scope: Option<&[Shape]>,
    ) -> Result<usize, Diagnostic> {
        match reader.read_u8().map_err(malformed)? {
            0 => Ok(0),
            1 => self.value(reader, scope),
            _ => Err(invalid("command optional type is malformed")),
        }
    }
}

pub(crate) fn external_name(name: ComponentExternName<'_>) -> Result<(), Diagnostic> {
    if name.implements.is_some() || name.version_suffix.is_some() || name.external_id.is_some() {
        return Err(invalid("command names contain unreviewed identity options"));
    }
    bounded_name(name.name)
}

fn name(reader: &mut BinaryReader<'_>) -> Result<(), Diagnostic> {
    bounded_name(reader.read_string().map_err(malformed)?)
}

fn bounded_name(name: &str) -> Result<(), Diagnostic> {
    if name.len() > 256 {
        return Err(invalid("command type name exceeds 256 bytes"));
    }
    Ok(())
}

fn malformed(_: wasmparser::BinaryReaderError) -> Diagnostic {
    invalid("command type syntax is malformed")
}

pub(super) fn invalid(message: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4013",
        None,
        message,
        "use the bounded authenticated command interface types",
    )
}
