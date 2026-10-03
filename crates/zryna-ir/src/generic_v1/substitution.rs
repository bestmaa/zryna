//! Original signatures and source-ordered nominal members, independently substituted.

use super::{
    Failure, keys, raw, reject, reserve,
    source::Originals,
    source_types::{Resolver, type_id},
};
use zryna_layout::generic_v1::VerifiedLayouts;
use zryna_syntax::v5::RawDataDeclarationKind;

pub(super) fn check(
    program: &raw::Program,
    layouts: &VerifiedLayouts,
    originals: &Originals<'_>,
) -> Result<(), Failure> {
    original_layout_inventory(layouts, originals)?;
    for function in &program.functions {
        let domain = if function.key[0] == 0x40 {
            keys::Domain::FunctionInstance
        } else {
            keys::Domain::SourceRoot
        };
        let key = keys::decode(&function.key, domain)?;
        let (module, index) = key.declaration().ok_or(Failure::InternalFailure)?;
        let original = &originals.units[module as usize].functions[index as usize];
        let resolver = Resolver {
            originals,
            module,
            parameters: original.type_parameters.as_ref(),
            arguments: arguments(&key)?,
        };
        if function.parameters.len() != original.parameters.len() {
            return Err(reject(
                "closed function parameter count differs from its original signature",
            ));
        }
        for (claim, parameter) in function.parameters.iter().zip(&original.parameters) {
            if *claim != type_id(program, resolver.resolve(parameter.type_syntax)?)? {
                return Err(reject("closed function parameter is not exact original substitution"));
            }
        }
        if function.result != type_id(program, resolver.resolve(original.result_type)?)? {
            return Err(reject("closed function result is not exact original substitution"));
        }
        let export = if original.export_span.is_some() && original.type_parameters.is_none() {
            Some(original.name.text.as_str())
        } else {
            None
        };
        if function.public_export.as_deref() != export {
            return Err(reject("scalar export claim differs from exact original export/name"));
        }
    }
    nominal_members(program, layouts, originals)
}

fn original_layout_inventory(
    layouts: &VerifiedLayouts,
    originals: &Originals<'_>,
) -> Result<(), Failure> {
    if layouts.original_modules().len() != originals.units.len() {
        return Err(reject(
            "sealed layout original module inventory differs from authenticated syntax",
        ));
    }
    let mut next = 0;
    for (module, unit) in layouts.original_modules().iter().zip(originals.units) {
        if module.id.0 != unit.id
            || module.source_file.index() != unit.id
            || module.data_declarations as usize != unit.data_declarations.len()
        {
            return Err(reject(
                "sealed layout original data inventory differs from authenticated syntax",
            ));
        }
        for (index, data) in unit.data_declarations.iter().enumerate() {
            let claim = layouts
                .original_declarations()
                .get(next)
                .ok_or_else(|| reject("sealed layout omits an original data declaration"))?;
            let (kind, members) = match &data.kind {
                RawDataDeclarationKind::Struct { fields, .. } => {
                    (zryna_layout::generic_v1::raw::NominalKind::Struct, fields.len())
                }
                RawDataDeclarationKind::Enum { variants, .. } => {
                    (zryna_layout::generic_v1::raw::NominalKind::Enum, variants.len())
                }
            };
            if claim.module.0 != unit.id
                || claim.index as usize != index
                || claim.kind != kind
                || claim.parameters as usize
                    != data.type_parameters.as_ref().map_or(0, |list| list.parameters.len())
                || claim.members as usize != members
                || claim.span.file().index() != data.span.file
                || claim.span.start() != data.span.start
                || claim.span.end() != data.span.end
            {
                return Err(reject(
                    "sealed original data metadata is not authenticated source metadata",
                ));
            }
            next += 1;
        }
    }
    if next != layouts.original_declarations().len() {
        return Err(reject("sealed layout carries extra original data declarations"));
    }
    Ok(())
}

fn nominal_members(
    program: &raw::Program,
    layouts: &VerifiedLayouts,
    originals: &Originals<'_>,
) -> Result<(), Failure> {
    for ty in layouts.types() {
        let key = keys::decode(ty.key(), keys::Domain::Type)?;
        let Some((module, index)) = key.declaration() else {
            continue;
        };
        let original = originals
            .units
            .get(module as usize)
            .and_then(|unit| unit.data_declarations.get(index as usize))
            .ok_or_else(|| {
                reject("stored nominal key refers to an unknown original data declaration")
            })?;
        let count = original.type_parameters.as_ref().map_or(0, |list| list.parameters.len());
        let kind = match (&original.kind, count) {
            (RawDataDeclarationKind::Struct { .. }, 0) => keys::Kind::Struct,
            (RawDataDeclarationKind::Struct { .. }, _) => keys::Kind::GenericStruct,
            (RawDataDeclarationKind::Enum { .. }, 0) => keys::Kind::Enum,
            (RawDataDeclarationKind::Enum { .. }, _) => keys::Kind::GenericEnum,
        };
        if key.kind() != kind || key.arguments().len() != count {
            return Err(reject("nominal key kind/arity differs from its authenticated original"));
        }
        let resolver = Resolver {
            originals,
            module,
            parameters: original.type_parameters.as_ref(),
            arguments: arguments(&key)?,
        };
        match &original.kind {
            RawDataDeclarationKind::Struct { fields, .. } => {
                if ty.fields().len() != fields.len() || ty.variants().len() != 0 {
                    return Err(reject(
                        "sealed struct member inventory differs from original source",
                    ));
                }
                for ((ordinal, claim, _), field) in ty.fields().zip(fields) {
                    let expected = type_id(program, resolver.resolve(field.type_syntax)?)?;
                    if expected != raw::Type::Stored(claim.index())
                        || fields.get(ordinal as usize) != Some(field)
                    {
                        return Err(reject(
                            "sealed struct field is not exact source-ordered substitution",
                        ));
                    }
                }
            }
            RawDataDeclarationKind::Enum { variants, .. } => {
                if ty.variants().len() != variants.len() || ty.fields().len() != 0 {
                    return Err(reject(
                        "sealed enum variant inventory differs from original source",
                    ));
                }
                for ((ordinal, claim), variant) in ty.variants().zip(variants) {
                    let expected = variant
                        .payload_type
                        .map(|occurrence| {
                            resolver.resolve(occurrence).and_then(|closed| type_id(program, closed))
                        })
                        .transpose()?;
                    if expected != claim.map(|id| raw::Type::Stored(id.index()))
                        || variants.get(ordinal as usize) != Some(variant)
                    {
                        return Err(reject(
                            "sealed active payload is not exact source-ordered substitution",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn arguments<'a>(key: &keys::DecodedKey<'a>) -> Result<Vec<&'a [u8]>, Failure> {
    let mut arguments = reserve(key.arguments().len())?;
    arguments.extend(key.arguments());
    Ok(arguments)
}
