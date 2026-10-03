//! Exact closed operations and edge bindings; no ownership plan or program authority is issued.

use super::{Failure, cfg, inventory::Inventory, raw, reject, reserve};
use zryna_layout::generic_v1::{TypeView, VerifiedLayouts};
use zryna_source::SourceMap;

pub(super) fn check(
    program: &raw::Program,
    sources: &SourceMap,
    layouts: &VerifiedLayouts,
    inventory: &Inventory,
) -> Result<(), Failure> {
    let mut views = reserve(layouts.types().len())?;
    views.extend(layouts.types());
    let mut calls = reserve(0)?;
    for (function_index, function) in program.functions.iter().enumerate() {
        for parameter in &function.parameters {
            cfg::ty(*parameter, program.type_keys.len())?;
            if *parameter == raw::Type::Unit {
                return Err(reject("unit is not a value parameter"));
            }
        }
        cfg::ty(function.result, program.type_keys.len())?;
        if matches!(function.result, raw::Type::Borrow { .. }) {
            return Err(reject("a function result cannot carry an escaping loan"));
        }
        let graph = cfg::build(function, sources, program.type_keys.len())?;
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (position, instruction) in block.instructions.iter().enumerate() {
                let operand = |id| graph.operand(id, block_index, position);
                let expected = operation(
                    program,
                    &views,
                    inventory.generic_count,
                    &instruction.operation,
                    &operand,
                )?;
                if instruction.result.ty != expected {
                    return Err(reject(
                        "instruction result differs from its exact closed operation type",
                    ));
                }
                if let Some(target) =
                    call_target(program, inventory.generic_count, &instruction.operation)?
                {
                    calls.try_reserve(1).map_err(|_| Failure::AllocationFailure)?;
                    calls.push((
                        inventory.declarations[function_index],
                        inventory.declarations[target],
                    ));
                }
            }
            let operand = |id| graph.operand(id, block_index, block.instructions.len());
            terminator(&views, function, &block.terminator, &operand)?;
        }
    }
    super::calls::check(program.declarations.len(), &calls)
}

fn operation(
    program: &raw::Program,
    layouts: &[TypeView<'_>],
    generic_count: usize,
    operation: &raw::Operation,
    operand: &impl Fn(u32) -> Result<raw::Type, Failure>,
) -> Result<raw::Type, Failure> {
    use raw::Operation as Op;
    Ok(match operation {
        Op::BoolLiteral(_) => raw::Type::Stored(0),
        Op::I32Literal(_) => raw::Type::Stored(1),
        Op::Unit => raw::Type::Unit,
        Op::Copy { value } => {
            let ty = operand(*value)?;
            let raw::Type::Stored(id) = ty else {
                return Err(reject("copy cannot duplicate a loan or unit"));
            };
            let view = layout(layouts, id)?;
            if view.drop_kind() != 0 {
                return Err(reject("owned value requires explicit ownership transfer, not Copy"));
            }
            ty
        }
        Op::I32Add { left, right } => {
            exact(operand(*left)?, raw::Type::Stored(1))?;
            exact(operand(*right)?, raw::Type::Stored(1))?;
            raw::Type::Stored(1)
        }
        Op::ClosedGenericCall { arguments, .. } | Op::SourceCall { arguments, .. } => {
            let index =
                call_target(program, generic_count, operation)?.ok_or(Failure::InternalFailure)?;
            let target = &program.functions[index];
            if arguments.len() != target.parameters.len() {
                return Err(reject("closed call value arity differs from its target"));
            }
            for (argument, expected) in arguments.iter().zip(&target.parameters) {
                exact(operand(*argument)?, *expected)?;
            }
            target.result
        }
        Op::ClosedEnumConstruct { ty, ordinal, payload } => {
            let variants = variants(layouts, *ty)?;
            let expected = variants
                .variants()
                .nth(*ordinal as usize)
                .ok_or_else(|| reject("constructor selected an unknown enum ordinal"))?
                .1
                .map(zryna_layout::generic_v1::TypeId::index);
            match (expected, payload) {
                (None, None) => {}
                (Some(expected), Some(value)) => {
                    exact(operand(*value)?, raw::Type::Stored(expected))?;
                }
                _ => {
                    return Err(reject(
                        "constructor payload presence differs from the selected fixed variant",
                    ));
                }
            }
            raw::Type::Stored(*ty)
        }
    })
}

fn terminator(
    layouts: &[TypeView<'_>],
    function: &raw::Function,
    term: &raw::Terminator,
    operand: &impl Fn(u32) -> Result<raw::Type, Failure>,
) -> Result<(), Failure> {
    match term {
        raw::Terminator::Return(value) => exact(operand(*value)?, function.result),
        raw::Terminator::Jump(next) => edge(function, next, None, operand),
        raw::Terminator::Branch { condition, yes, no } => {
            exact(operand(*condition)?, raw::Type::Stored(0))?;
            edge(function, yes, None, operand)?;
            edge(function, no, None, operand)
        }
        raw::Terminator::ClosedEnumMatch { ty, scrutinee, mode, arms } => {
            let expected = match mode {
                raw::MatchMode::Value => raw::Type::Stored(*ty),
                raw::MatchMode::SharedBorrow => {
                    raw::Type::Borrow { referent: *ty, exclusive: false }
                }
                raw::MatchMode::ExclusiveBorrow => {
                    raw::Type::Borrow { referent: *ty, exclusive: true }
                }
            };
            exact(operand(*scrutinee)?, expected)?;
            let variants = variants(layouts, *ty)?;
            if arms.len() != variants.variants().len() {
                return Err(reject("match successors are missing or extra"));
            }
            for (arm, (ordinal, payload)) in arms.iter().zip(variants.variants()) {
                if arm.ordinal != ordinal {
                    return Err(reject(
                        "match successors are duplicate, forged or not ordinal-ordered",
                    ));
                }
                let expected = payload.map(|payload| match mode {
                    raw::MatchMode::Value => raw::Type::Stored(payload.index()),
                    raw::MatchMode::SharedBorrow => {
                        raw::Type::Borrow { referent: payload.index(), exclusive: false }
                    }
                    raw::MatchMode::ExclusiveBorrow => {
                        raw::Type::Borrow { referent: payload.index(), exclusive: true }
                    }
                });
                match (expected, &arm.binding) {
                    (None, None) => edge(function, &arm.edge, None, operand)?,
                    (Some(expected), Some(binding)) if binding.ty == expected => {
                        edge(function, &arm.edge, Some(binding), operand)?;
                    }
                    _ => {
                        return Err(reject(
                            "match active payload binding/type differs from its fixed variant and loan mode",
                        ));
                    }
                }
            }
            Ok(())
        }
    }
}

fn edge(
    function: &raw::Function,
    edge: &raw::Edge,
    binding: Option<&raw::Definition>,
    operand: &impl Fn(u32) -> Result<raw::Type, Failure>,
) -> Result<(), Failure> {
    let block =
        function.blocks.get(edge.target as usize).ok_or_else(|| reject("unknown edge target"))?;
    let offset = usize::from(binding.is_some());
    if binding.is_some_and(|binding| block.parameters.first() != Some(binding))
        || block.parameters.len()
            != edge.arguments.len().checked_add(offset).ok_or(Failure::InternalFailure)?
    {
        return Err(reject("edge payload binding or exact parameter arity differs"));
    }
    for (argument, parameter) in edge.arguments.iter().zip(block.parameters.iter().skip(offset)) {
        exact(operand(*argument)?, parameter.ty)?;
    }
    Ok(())
}

fn layout<'a>(layouts: &[TypeView<'a>], index: u32) -> Result<TypeView<'a>, Failure> {
    layouts.get(index as usize).copied().ok_or_else(|| reject("unknown closed layout type ID"))
}

fn variants<'a>(layouts: &[TypeView<'a>], index: u32) -> Result<TypeView<'a>, Failure> {
    let view = layout(layouts, index)?;
    if !matches!(view.key().first(), Some(0x11 | 0x13 | 0x14 | 0x15)) {
        return Err(reject("enum operation claimed a scalar, struct or container type"));
    }
    Ok(view)
}

pub(super) fn call_target(
    program: &raw::Program,
    generic_count: usize,
    operation: &raw::Operation,
) -> Result<Option<usize>, Failure> {
    match operation {
        raw::Operation::ClosedGenericCall { instance, .. } => {
            let index = *instance as usize;
            if index >= generic_count {
                return Err(reject("generic call target is unknown or a nongeneric root"));
            }
            Ok(Some(index))
        }
        raw::Operation::SourceCall { module, function, .. } => {
            let mut key = [0u8; 9];
            key[0] = 0x41;
            key[1..5].copy_from_slice(&module.to_le_bytes());
            key[5..].copy_from_slice(&function.to_le_bytes());
            let index = program
                .functions
                .binary_search_by(|candidate| candidate.key.as_slice().cmp(&key))
                .map_err(|_| reject("nongeneric call target is absent or a generic template"))?;
            Ok(Some(index))
        }
        _ => Ok(None),
    }
}

fn exact(actual: raw::Type, expected: raw::Type) -> Result<(), Failure> {
    if actual != expected {
        return Err(reject("closed operand/binding/result types differ"));
    }
    Ok(())
}
