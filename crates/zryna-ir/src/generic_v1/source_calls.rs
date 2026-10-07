//! Closed call targets are rederived from exact source expressions and original type arguments.

use super::{
    Failure, keys, raw, reject, reserve,
    source::{Originals, Target},
    source_types::{Closed, Resolver},
};
use zryna_syntax::v5::{RawExpressionKind, RawExpressionSyntax};

pub(super) fn check(program: &raw::Program, originals: &Originals<'_>) -> Result<(), Failure> {
    for function in &program.functions {
        let domain = if function.key[0] == 0x40 {
            keys::Domain::FunctionInstance
        } else {
            keys::Domain::SourceRoot
        };
        let key = keys::decode(&function.key, domain)?;
        let (module, index) = key.declaration().ok_or(Failure::InternalFailure)?;
        let original = &originals.units[module as usize].functions[index as usize];
        let mut arguments = reserve(key.arguments().len())?;
        arguments.extend(key.arguments());
        let resolver = Resolver {
            originals,
            module,
            parameters: original.type_parameters.as_ref(),
            arguments,
        };
        let mut expressions = reserve(original.body.expressions.len())?;
        expressions.extend(
            original
                .body
                .expressions
                .iter()
                .filter(|expression| matches!(expression.kind, RawExpressionKind::Call { .. })),
        );
        expressions.sort_unstable_by_key(|expression| (expression.span.start, expression.span.end));
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            if !matches!(
                instruction.operation,
                raw::Operation::ClosedGenericCall { .. } | raw::Operation::SourceCall { .. }
            ) {
                continue;
            }
            let index = expressions
                .binary_search_by_key(
                    &(instruction.span.start, instruction.span.end),
                    |expression| (expression.span.start, expression.span.end),
                )
                .map_err(|_| {
                    reject("closed call range does not identify an exact original call expression")
                })?;
            check_call(program, &resolver, instruction, expressions[index])?;
        }
    }
    Ok(())
}

fn check_call(
    program: &raw::Program,
    resolver: &Resolver<'_, '_>,
    instruction: &raw::Instruction,
    expression: &RawExpressionSyntax,
) -> Result<(), Failure> {
    let RawExpressionKind::Call { callee, type_arguments, arguments, .. } = &expression.kind else {
        return Err(Failure::InternalFailure);
    };
    let Target::Function(module, function) =
        resolver.originals.resolve(resolver.module, &callee.text)?
    else {
        return Err(reject("closed call source target is not an original function"));
    };
    match &instruction.operation {
        raw::Operation::ClosedGenericCall { instance, arguments: operands } => {
            let target = program
                .functions
                .get(*instance as usize)
                .ok_or_else(|| reject("unknown closed call instance"))?;
            let key = keys::decode(&target.key, keys::Domain::FunctionInstance)?;
            let list = type_arguments
                .as_ref()
                .ok_or_else(|| reject("closed generic call lacks explicit source arguments"))?;
            if key.declaration() != Some((module, function))
                || key.arguments().len() != list.arguments.len()
                || operands.len() != arguments.len()
            {
                return Err(reject(
                    "closed call target/arity differs from exact original source call",
                ));
            }
            for (key, occurrence) in key.arguments().zip(&list.arguments) {
                let Closed::Stored(expected) = resolver.resolve(*occurrence)? else {
                    return Err(reject("source generic call argument is not a stored ZrynaValue"));
                };
                if key != expected {
                    return Err(reject(
                        "closed call arguments are not original caller substitution",
                    ));
                }
            }
        }
        raw::Operation::SourceCall {
            module: claim_module,
            function: claim_function,
            arguments: operands,
        } => {
            if (module, function) != (*claim_module, *claim_function)
                || type_arguments.is_some()
                || operands.len() != arguments.len()
            {
                return Err(reject(
                    "nongeneric call target/arity differs from exact original source call",
                ));
            }
        }
        _ => return Err(Failure::InternalFailure),
    }
    Ok(())
}
