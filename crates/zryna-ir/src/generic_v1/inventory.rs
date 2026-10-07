//! Complete raw inventory, exact successor authorities and inherited amplification preflight.

use super::{Failure, budget, keys, raw, reject, reserve};
use zryna_layout::{StorageTarget, generic_v1::VerifiedLayouts};
use zryna_source::SourceMap;

pub(super) struct Inventory {
    pub generic_count: usize,
    pub declarations: Vec<usize>,
}

pub(super) fn check(
    program: &raw::Program,
    sources: &SourceMap,
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
) -> Result<Inventory, Failure> {
    preflight(program)?;
    if linear.source_map_identity() != sources.identity()
        || linux.source_map_identity() != sources.identity()
        || linear.target() != StorageTarget::Linear32V1
        || linux.target() != StorageTarget::LinuxX8664V1
        || linear.universe_identity() != linux.universe_identity()
        || program.universe != *linear.universe_identity()
        || program.linear32 != *linear.fingerprint()
        || program.linux_x86_64 != *linux.fingerprint()
    {
        return Err(reject("foreign source/universe, wrong target or successor fingerprint"));
    }
    if program.type_keys.len() != linear.types().len()
        || program.type_keys.len() != linux.types().len()
    {
        return Err(reject("omitted or extra complete stored type inventory"));
    }
    for ((key, left), right) in program.type_keys.iter().zip(linear.types()).zip(linux.types()) {
        if key != left.key() || key != right.key() {
            return Err(reject("claimed type key/ID order differs from both successor layouts"));
        }
        keys::decode(key, keys::Domain::Type)?;
    }
    originals(program, sources)?;
    let mut declarations = reserve(program.functions.len())?;
    let mut represented = reserve(program.declarations.len())?;
    represented.resize(program.declarations.len(), false);
    let mut generic_count = 0usize;
    for (index, function) in program.functions.iter().enumerate() {
        if index > 0 && program.functions[index - 1].key >= function.key {
            return Err(reject(
                "function keys/IDs are duplicate or not in unsigned canonical order",
            ));
        }
        let generic = function.key.first() == Some(&0x40);
        let domain =
            if generic { keys::Domain::FunctionInstance } else { keys::Domain::SourceRoot };
        let key = keys::decode(&function.key, domain)?;
        let owner = key.declaration().ok_or(Failure::InternalFailure)?;
        let declaration = program
            .declarations
            .binary_search_by_key(&owner, |row| (row.module, row.function))
            .map_err(|_| reject("closed function refers to an unknown original declaration"))?;
        let original = &program.declarations[declaration];
        if function.span != original.span
            || key.arguments().len() != original.parameters as usize
            || generic != (original.parameters != 0)
        {
            return Err(reject("function kind, arity or original declaration range differs"));
        }
        if generic && function.public_export.is_some() {
            return Err(reject(
                "a generic instance cannot enter the scalar executable export inventory",
            ));
        }
        for argument in key.arguments() {
            if program
                .type_keys
                .binary_search_by(|candidate| candidate.as_slice().cmp(argument))
                .is_err()
            {
                return Err(reject(
                    "function argument is absent from the complete stored universe",
                ));
            }
        }
        generic_count =
            generic_count.checked_add(usize::from(generic)).ok_or(Failure::InternalFailure)?;
        if generic_count > 4096 {
            return Err(budget("closed generic function instance limit 4096; first extra 4097"));
        }
        represented[declaration] = true;
        declarations.push(declaration);
    }
    for (declaration, represented) in program.declarations.iter().zip(represented) {
        if declaration.parameters == 0 && !represented {
            return Err(reject("nongeneric original function root is missing"));
        }
    }
    Ok(Inventory { generic_count, declarations })
}

fn originals(program: &raw::Program, sources: &SourceMap) -> Result<(), Failure> {
    if program.modules.len() != sources.len() {
        return Err(reject("original module inventory differs from the exact source map"));
    }
    let mut next = 0usize;
    for (module, row) in program.modules.iter().enumerate() {
        if usize::try_from(row.id).ok() != Some(module) {
            return Err(reject("original module IDs are not dense"));
        }
        for function in 0..row.functions {
            let declaration = program
                .declarations
                .get(next)
                .ok_or_else(|| reject("original function declaration inventory is incomplete"))?;
            if declaration.module != row.id
                || declaration.function != function
                || declaration.parameters > 2
                || declaration.span.file != row.id
                || sources.verify_span(declaration.span).is_err()
            {
                return Err(reject("original declaration/module/arity/source range differs"));
            }
            next = next.checked_add(1).ok_or(Failure::InternalFailure)?;
        }
    }
    if next != program.declarations.len() {
        return Err(reject("extra original declaration claims"));
    }
    Ok(())
}

fn add(total: &mut usize, amount: usize, limit: usize, label: &str) -> Result<(), Failure> {
    *total = total.checked_add(amount).ok_or(Failure::InternalFailure)?;
    if *total > limit {
        return Err(budget(label));
    }
    Ok(())
}

pub(super) fn preflight(program: &raw::Program) -> Result<(), Failure> {
    use crate::data_ownership_v1 as limits;
    if program.modules.len() > limits::MAX_MODULES
        || program
            .modules
            .iter()
            .any(|module| module.functions as usize > limits::MAX_FUNCTIONS_PER_MODULE)
        || program.declarations.len() > limits::MAX_FUNCTIONS_PER_PROGRAM
        || program.functions.len() > limits::MAX_FUNCTIONS_PER_PROGRAM
        || program.type_keys.len() > limits::MAX_INSTANTIATED_TYPES
    {
        return Err(budget("inherited module/function/type inventory limit exceeded"));
    }
    let (mut parameters, mut blocks, mut values, mut edges, mut calls, mut operands) =
        (0, 0, 0, 0, 0, 0);
    for function in &program.functions {
        if function.parameters.len() > limits::MAX_PARAMETERS_PER_FUNCTION
            || function.blocks.len() > limits::MAX_BLOCKS_PER_FUNCTION
        {
            return Err(budget("inherited function parameter/block limit exceeded"));
        }
        add(
            &mut parameters,
            function.parameters.len(),
            limits::MAX_PARAMETERS_PER_PROGRAM,
            "program parameter limit exceeded",
        )?;
        add(
            &mut blocks,
            function.blocks.len(),
            limits::MAX_BLOCKS_PER_PROGRAM,
            "program block limit exceeded",
        )?;
        let (mut local_values, mut local_edges) = (0, 0);
        for block in &function.blocks {
            if block.parameters.len() > limits::MAX_BLOCK_PARAMETERS {
                return Err(budget("block parameter limit exceeded"));
            }
            add(
                &mut local_values,
                block.parameters.len(),
                limits::MAX_VALUES_PER_FUNCTION,
                "function value limit exceeded",
            )?;
            add(
                &mut local_values,
                block.instructions.len(),
                limits::MAX_VALUES_PER_FUNCTION,
                "function value limit exceeded",
            )?;
            let count = match &block.terminator {
                raw::Terminator::Return(_) => 0,
                raw::Terminator::Jump(_) => 1,
                raw::Terminator::Branch { .. } => 2,
                raw::Terminator::ClosedEnumMatch { arms, .. } => arms.len(),
            };
            add(
                &mut local_edges,
                count,
                limits::MAX_CFG_EDGES_PER_FUNCTION,
                "function CFG edge limit exceeded",
            )?;
            instruction_limits(block, &mut calls, &mut operands)?;
        }
        add(
            &mut values,
            local_values,
            limits::MAX_VALUES_PER_PROGRAM,
            "program value limit exceeded",
        )?;
        add(
            &mut edges,
            local_edges,
            limits::MAX_CFG_EDGES_PER_PROGRAM,
            "program CFG edge limit exceeded",
        )?;
    }
    Ok(())
}

fn instruction_limits(
    block: &raw::Block,
    calls: &mut usize,
    operands: &mut usize,
) -> Result<(), Failure> {
    use crate::data_ownership_v1 as limits;
    for instruction in &block.instructions {
        let count = match &instruction.operation {
            raw::Operation::ClosedGenericCall { arguments, .. }
            | raw::Operation::SourceCall { arguments, .. } => {
                add(calls, 1, limits::MAX_CALL_EDGES, "program call edge limit exceeded")?;
                if arguments.len() > limits::MAX_PARAMETERS_PER_FUNCTION {
                    return Err(reject(
                        "call argument count exceeds the inherited exact signature bound",
                    ));
                }
                0
            }
            raw::Operation::ClosedEnumConstruct { payload, .. } => usize::from(payload.is_some()),
            _ => 0,
        };
        add(
            operands,
            count,
            limits::MAX_AGGREGATE_OPERANDS,
            "program aggregate operand limit exceeded",
        )?;
    }
    Ok(())
}
