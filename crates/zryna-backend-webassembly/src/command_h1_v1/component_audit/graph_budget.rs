//! Count bounds precede parser allocation for the H1 executable graph.

use super::invalid;
use wasmparser::{
    BinaryReader, CanonicalOption, ComponentExternName, ComponentExternalKind, Export,
    InstantiationArg,
};
use zryna_diagnostics::Diagnostic;

pub(super) fn instances(bytes: &[u8], offset: u64) -> Result<(), Diagnostic> {
    let mut reader = BinaryReader::new(bytes, offset);
    let count = reader.read_var_u32().map_err(|_| invalid())?;
    if count > 3 {
        return Err(invalid());
    }
    for _ in 0..count {
        match reader.read_u8().map_err(|_| invalid())? {
            0 => {
                reader.read_var_u32().map_err(|_| invalid())?;
                let count = reader.read_var_u32().map_err(|_| invalid())?;
                if count > 2 {
                    return Err(invalid());
                }
                for _ in 0..count {
                    let arg: InstantiationArg<'_> = reader.read().map_err(|_| invalid())?;
                    if arg.name.len() > 256 {
                        return Err(invalid());
                    }
                }
            }
            1 => {
                if reader.read_var_u32().map_err(|_| invalid())? != 1 {
                    return Err(invalid());
                }
                let export: Export<'_> = reader.read().map_err(|_| invalid())?;
                if export.name.len() > 256 {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
    }
    end(&reader)
}

pub(super) fn canonical(bytes: &[u8], offset: u64) -> Result<(), Diagnostic> {
    let mut reader = BinaryReader::new(bytes, offset);
    if reader.read_var_u32().map_err(|_| invalid())? != 1 {
        return Err(invalid());
    }
    let tag = reader.read_u8().map_err(|_| invalid())?;
    if tag > 1 || reader.read_u8().map_err(|_| invalid())? != 0 {
        return Err(invalid());
    }
    reader.read_var_u32().map_err(|_| invalid())?;
    let count = reader.read_var_u32().map_err(|_| invalid())?;
    if count > 3 {
        return Err(invalid());
    }
    for _ in 0..count {
        reader.read::<CanonicalOption>().map_err(|_| invalid())?;
    }
    if tag == 0 {
        reader.read_var_u32().map_err(|_| invalid())?;
    }
    end(&reader)
}

pub(super) fn component_instance(bytes: &[u8], offset: u64) -> Result<(), Diagnostic> {
    let mut reader = BinaryReader::new(bytes, offset);
    if reader.read_var_u32().map_err(|_| invalid())? != 1
        || reader.read_u8().map_err(|_| invalid())? != 1
        || reader.read_var_u32().map_err(|_| invalid())? != 1
    {
        return Err(invalid());
    }
    let name: ComponentExternName<'_> = reader.read().map_err(|_| invalid())?;
    crate::component_command::type_budget::external_name(name)?;
    reader.read::<ComponentExternalKind>().map_err(|_| invalid())?;
    reader.read_var_u32().map_err(|_| invalid())?;
    end(&reader)
}

fn end(reader: &BinaryReader<'_>) -> Result<(), Diagnostic> {
    if reader.eof() { Ok(()) } else { Err(invalid()) }
}
