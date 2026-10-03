//! Source/family enum identities, active payloads and canonical exhaustive match transfers.

use super::{Builder, Value};
use crate::generic_v1::{
    Failure, raw, reject, reserve,
    source::Target,
    source_types::{Closed, Resolver, encode_mode, type_id},
};
use zryna_syntax::{v4::RawMatchArm, v5::RawDataDeclarationKind};

pub(super) struct Description<'a> {
    pub ty: Closed,
    pub variants: Vec<(&'a str, Option<Closed>)>,
}

pub(super) fn describe<'a>(
    resolver: &Resolver<'_, 'a>,
    name: &str,
    arguments: &[Vec<u8>],
    symbolic: bool,
) -> Result<Description<'a>, Failure> {
    let mut variants = reserve(2)?;
    let (tag, lanes) = match name {
        "Option" if arguments.len() == 1 => {
            variants.extend([("none", None), ("some", Some(Closed::Stored(arguments[0].clone())))]);
            (0x14, vec![1])
        }
        "Result" if arguments.len() == 2 => {
            variants.extend([
                ("ok", Some(Closed::Stored(arguments[0].clone()))),
                ("err", Some(Closed::Stored(arguments[1].clone()))),
            ]);
            (0x15, vec![2])
        }
        "Option" | "Result" => {
            return Err(reject("compiler enum family requires exact explicit arity"));
        }
        _ => {
            let Target::Data(module, index) = resolver.originals.resolve(resolver.module, name)?
            else {
                return Err(reject("enum constructor names a function"));
            };
            let original =
                &resolver.originals.units[module as usize].data_declarations[index as usize];
            let RawDataDeclarationKind::Enum { variants: declared, .. } = &original.kind else {
                return Err(reject("enum constructor names a struct"));
            };
            let arity = original.type_parameters.as_ref().map_or(0, |list| list.parameters.len());
            if arity != arguments.len() {
                return Err(reject("nominal enum has wrong generic arity"));
            }
            let mut keys = reserve(arity)?;
            keys.extend(arguments.iter().map(Vec::as_slice));
            let substitution = Resolver {
                originals: resolver.originals,
                module,
                parameters: original.type_parameters.as_ref(),
                arguments: keys,
            };
            variants.try_reserve_exact(declared.len()).map_err(|_| Failure::AllocationFailure)?;
            for variant in declared {
                let payload = variant
                    .payload_type
                    .map(|occurrence| substitution.resolve_symbolic(occurrence))
                    .transpose()?;
                if payload.as_ref().is_some_and(|ty| !matches!(ty, Closed::Stored(_))) {
                    return Err(reject("enum payload is not a stored value"));
                }
                variants.push((&variant.name.text, payload));
            }
            if arity == 0 {
                (0x11, vec![module, index])
            } else {
                (
                    0x13,
                    vec![
                        module,
                        index,
                        u32::try_from(arity).map_err(|_| Failure::InternalFailure)?,
                    ],
                )
            }
        }
    };
    Ok(Description { ty: Closed::Stored(encode_mode(tag, &lanes, arguments, symbolic)?), variants })
}

fn arguments(key: &[u8]) -> Result<Vec<Vec<u8>>, Failure> {
    let (offset, count) = match key.first() {
        Some(0x14) => (5, 1),
        Some(0x15) => (5, 2),
        Some(0x11) => (9, 0),
        Some(0x13) => (
            13,
            u32::from_le_bytes(
                key.get(9..13)
                    .ok_or(Failure::InternalFailure)?
                    .try_into()
                    .map_err(|_| Failure::InternalFailure)?,
            ) as usize,
        ),
        _ => return Err(reject("source match requires an original or compiler-owned enum")),
    };
    let mut result = reserve(count)?;
    let mut cursor = offset;
    for _ in 0..count {
        let length = u32::from_le_bytes(
            key.get(cursor..cursor + 4)
                .ok_or(Failure::InternalFailure)?
                .try_into()
                .map_err(|_| Failure::InternalFailure)?,
        ) as usize;
        cursor += 4;
        let end = cursor.checked_add(length).ok_or(Failure::InternalFailure)?;
        let mut child = reserve(length)?;
        child.extend_from_slice(key.get(cursor..end).ok_or(Failure::InternalFailure)?);
        result.push(child);
        cursor = end;
    }
    if cursor != key.len() {
        return Err(Failure::InternalFailure);
    }
    Ok(result)
}

impl<'b> Builder<'_, 'b> {
    pub(super) fn match_expression(
        &mut self,
        scrutinee: u32,
        arms: &'b [RawMatchArm],
        span: zryna_source::UntrustedSpan,
        depth: usize,
    ) -> Result<Value, Failure> {
        let scrutinee = self.expression(scrutinee, depth + 1)?;
        let Closed::Stored(key) = &scrutinee.ty else {
            return Err(reject("Copy match cannot carry a loan"));
        };
        let first = arms.first().ok_or_else(|| reject("source match has no arms"))?;
        let description =
            describe(&self.resolver, &first.type_name.text, &arguments(key)?, self.symbolic)?;
        if description.ty != scrutinee.ty || arms.len() != description.variants.len() {
            return Err(reject("source match does not cover its exact enum universe"));
        }
        let entry = self.block;
        let saved = self.locals.clone();
        let prior_scope = self.scope_start;
        let mut successors = reserve(arms.len())?;
        let mut outputs = reserve(arms.len())?;
        for (ordinal, (name, payload)) in description.variants.iter().enumerate() {
            let mut matches = arms.iter().filter(|arm| arm.variant.text == *name);
            let arm = matches.next().ok_or_else(|| reject("source match omits a fixed ordinal"))?;
            if matches.next().is_some()
                || describe(&self.resolver, &arm.type_name.text, &arguments(key)?, self.symbolic)?
                    .ty
                    != scrutinee.ty
            {
                return Err(reject("source match duplicates an ordinal or names a different enum"));
            }
            self.locals = saved.clone();
            self.scope_start = saved.len();
            self.block = self.new_block(arm.span)?;
            let target = self.block;
            let binding = match (&arm.binding, payload) {
                (Some(name), Some(ty)) => {
                    let value = self.value(ty.clone())?;
                    let definition = self.definition(&value)?;
                    self.blocks[target].parameters.push(definition.clone());
                    self.bind(&name.text, value)?;
                    Some(definition)
                }
                (None, None) => None,
                _ => return Err(reject("source match binding differs from exact active payload")),
            };
            let value = self.expression(arm.value, depth + 1)?;
            outputs.push((self.block, value));
            successors.push(raw::Arm {
                ordinal: u32::try_from(ordinal).map_err(|_| Failure::InternalFailure)?,
                binding,
                edge: raw::Edge {
                    target: u32::try_from(target).map_err(|_| Failure::InternalFailure)?,
                    arguments: Vec::new(),
                },
            });
        }
        let result_type = outputs[0].1.ty.clone();
        if outputs.iter().any(|(_, value)| value.ty != result_type) {
            return Err(reject("source match arms have different symbolic result types"));
        }
        self.block = self.new_block(span)?;
        let result = self.value(result_type)?;
        let definition = self.definition(&result)?;
        self.blocks[self.block].parameters.push(definition);
        let target = u32::try_from(self.block).map_err(|_| Failure::InternalFailure)?;
        for (block, value) in outputs {
            self.blocks[block].terminator =
                raw::Terminator::Jump(raw::Edge { target, arguments: vec![value.id] });
        }
        let ty = if self.symbolic {
            0
        } else {
            let raw::Type::Stored(id) = type_id(self.program, scrutinee.ty)? else {
                return Err(Failure::InternalFailure);
            };
            id
        };
        self.blocks[entry].span = span;
        self.blocks[entry].terminator = raw::Terminator::ClosedEnumMatch {
            ty,
            scrutinee: scrutinee.id,
            mode: raw::MatchMode::Value,
            arms: successors,
        };
        self.locals = saved;
        self.scope_start = prior_scope;
        Ok(result)
    }
}
