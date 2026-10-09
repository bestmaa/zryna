//! Candidate production is separate from every independent verifier replay.

use crate::{IrError, VerifiedNativeCProgram, raw, require};
use std::collections::BTreeMap;
use zryna_semantics::native_c_v0::body::VerifiedPrivateBoundaries;
use zryna_source::SourceMap;
use zryna_syntax::{native_c_source_v0::raw as syntax, native_c_v0};

/// Produces explicit IR claims and always invokes the independent verifier.
/// # Errors
/// Rejects source identity, bounded candidate production or any independent admission failure.
pub fn lower(
    sources: &SourceMap,
    authority: &VerifiedPrivateBoundaries,
) -> Result<VerifiedNativeCProgram, IrError> {
    crate::verify(lower_unverified(sources, authority)?, sources, authority)
}

/// Produces untrusted claims only; callers may mutate them for independent rejection tests.
/// # Errors
/// Rejects a rebuilt map, source resolution failure or incomplete original inventory.
pub fn lower_unverified(
    sources: &SourceMap,
    authority: &VerifiedPrivateBoundaries,
) -> Result<raw::Program, IrError> {
    require(authority.belongs_to(sources), "ZRYNA-C4106", "ir-original-source-map")?;
    let declarations = native_c_v0::decode(
        authority.body_authority().declaration_authority().declaration_bytes(),
        native_c_v0::TARGET,
    )
    .map_err(|e| IrError::new(e.code(), e.detail()))?;
    let mut functions = Vec::new();
    for (source, boundary) in
        authority.body_authority().functions().iter().zip(authority.functions())
    {
        functions.push(lower_function(sources, source, boundary)?);
    }
    Ok(raw::Program {
        source_map: sources.identity(),
        declarations,
        storage: raw::Storage {
            universe: authority.native_layouts().universe_identity().as_bytes(),
            linear: *authority.linear_layouts().fingerprint(),
            native: *authority.native_layouts().fingerprint(),
            runtime: authority.runtime_abi().identifier().into(),
        },
        functions,
    })
}

fn lower_function(
    sources: &SourceMap,
    source: &zryna_semantics::native_c_v0::body::TypedFunction,
    boundary: &zryna_semantics::native_c_v0::body::FunctionBoundary,
) -> Result<raw::Function, IrError> {
    let range = source.declaration_range();
    let span = sources
        .span(source.file_id(), range.start, range.end)
        .map_err(|_| IrError::new("ZRYNA-C4106", "ir-function-span"))?;
    let mut bindings = BTreeMap::new();
    for (index, parameter) in source.parameters().iter().enumerate() {
        bindings.insert(parameter.name.as_str(), index);
    }
    let mut values = Vec::new();
    let mut next_binding = source.parameters().len();
    for statement in source.statements() {
        let root = match &statement.kind {
            syntax::StatementKind::Const(_, root)
            | syntax::StatementKind::Return(root)
            | syntax::StatementKind::Guard(_, root)
            | syntax::StatementKind::Expression(root) => *root,
        };
        while values.len() <= root {
            let id = values.len();
            let expression = source
                .expressions()
                .get(id)
                .ok_or_else(|| IrError::new("ZRYNA-C4106", "ir-source-arena"))?;
            let kind = match expression.source_kind() {
                syntax::ExpressionKind::I32(v) => raw::ValueKind::I32(*v),
                syntax::ExpressionKind::Bool(v) => raw::ValueKind::Bool(*v),
                syntax::ExpressionKind::Key(v) => raw::ValueKind::Key(v.clone()),
                syntax::ExpressionKind::Local(name) => raw::ValueKind::Local(
                    *bindings
                        .get(name.as_str())
                        .ok_or_else(|| IrError::new("ZRYNA-C4106", "ir-local-binding"))?,
                ),
                syntax::ExpressionKind::Add(a, b) => raw::ValueKind::WrappingAdd(*a, *b),
                syntax::ExpressionKind::Intrinsic(p, args) => {
                    raw::ValueKind::Primitive(*p, args.clone())
                }
            };
            let range = expression.range();
            values.push(raw::Value {
                id,
                span: sources
                    .span(source.file_id(), range.start, range.end)
                    .map_err(|_| IrError::new("ZRYNA-C4106", "ir-value-span"))?,
                ty: expression.value_type(),
                kind,
                token: expression.token_id(),
                status_call: expression.status_call(),
                origin: *boundary
                    .expression_origins
                    .get(id)
                    .ok_or_else(|| IrError::new("ZRYNA-C4106", "ir-private-origin"))?,
            });
        }
        if let syntax::StatementKind::Const(binding, _) = &statement.kind {
            bindings.insert(binding.name.as_str(), next_binding);
            next_binding += 1;
        }
    }
    require(source.steps().len() == boundary.steps.len(), "ZRYNA-C4106", "ir-complete-effects")?;
    let effects = source
        .steps()
        .iter()
        .zip(&boundary.steps)
        .enumerate()
        .map(|(id, (operation, plan))| raw::Effect {
            id,
            operation: operation.clone(),
            preparation: plan.preparation.clone(),
            exits: plan.exits.clone(),
            completed: plan.completed.clone(),
        })
        .collect();
    Ok(raw::Function {
        file: source.file_id(),
        ordinal: source.source_function_index(),
        span,
        name: source.name().into(),
        parameters: source.parameters().to_vec(),
        parameter_layouts: boundary.parameter_types.clone(),
        result: source.result_type(),
        result_layout: boundary.result_type,
        export: source.export_operation_index(),
        statements: source.statements().to_vec(),
        values,
        effects,
        private_owners: boundary.private_owners.clone(),
    })
}
