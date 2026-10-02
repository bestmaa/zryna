//! Original map resolution and complete source inventory, without provider or producer callbacks.

use crate::{IrError, raw, require};
use zryna_semantics::native_c_v0::body::VerifiedPrivateBoundaries;
use zryna_source::SourceMap;

pub(super) fn preflight_names(function: &raw::Function) -> Result<(), IrError> {
    use zryna_syntax::native_c_source_v0::{MAX_SOURCE_BYTES, raw::StatementKind};
    let mut total = function.name.len();
    for parameter in &function.parameters {
        total =
            super::count(total, parameter.name.len(), MAX_SOURCE_BYTES, "ir-private-name-budget")?;
    }
    for statement in &function.statements {
        let name = match &statement.kind {
            StatementKind::Const(binding, _) => Some(&binding.name),
            StatementKind::Guard(name, _) => Some(name),
            _ => None,
        };
        if let Some(name) = name {
            total = super::count(total, name.len(), MAX_SOURCE_BYTES, "ir-private-name-budget")?;
        }
    }
    super::count(0, total, MAX_SOURCE_BYTES, "ir-private-name-budget")?;
    Ok(())
}

pub(super) fn check(
    program: &raw::Program,
    sources: &SourceMap,
    authority: &VerifiedPrivateBoundaries,
) -> Result<(), IrError> {
    require(authority.belongs_to(sources), "ZRYNA-C4106", "ir-original-source-map")?;
    require(program.source_map == sources.identity(), "ZRYNA-C4102", "ir-source-issuer")?;
    let functions = authority.body_authority().functions();
    require(
        program.functions.len() == functions.len()
            && functions.len() == authority.functions().len(),
        "ZRYNA-C4106",
        "ir-complete-functions",
    )?;
    for (claim, original) in program.functions.iter().zip(functions) {
        let range = original.declaration_range();
        let span = sources
            .span(original.file_id(), range.start, range.end)
            .map_err(|_| IrError::new("ZRYNA-C4106", "ir-original-function-span"))?;
        require(
            claim.file == original.file_id()
                && claim.ordinal == original.source_function_index()
                && claim.span == span
                && sources.resolve(claim.span).is_ok()
                && claim.name == original.name(),
            "ZRYNA-C4106",
            "ir-function-source",
        )
        .map_err(|e| e.at(span))?;
        require(
            claim.parameters == original.parameters()
                && claim.statements == original.statements()
                && claim.result == original.result_type()
                && claim.export == original.export_operation_index(),
            "ZRYNA-C4106",
            "ir-complete-source-occupants",
        )
        .map_err(|e| e.at(span))?;
        require(
            claim.values.len() == original.expressions().len()
                && claim.effects.len() == original.steps().len(),
            "ZRYNA-C4106",
            "ir-complete-values-effects",
        )
        .map_err(|e| e.at(span))?;
        for (index, (value, expression)) in
            claim.values.iter().zip(original.expressions()).enumerate()
        {
            let range = expression.range();
            let expected = sources
                .span(original.file_id(), range.start, range.end)
                .map_err(|_| IrError::new("ZRYNA-C4106", "ir-original-expression-span"))?;
            require(
                value.id == index && value.span == expected && sources.resolve(value.span).is_ok(),
                "ZRYNA-C4106",
                "ir-value-source",
            )
            .map_err(|e| e.at(expected))?;
        }
        for (index, effect) in claim.effects.iter().enumerate() {
            require(effect.id == index, "ZRYNA-C4106", "ir-effect-source")
                .map_err(|e| e.at(span))?;
        }
    }
    Ok(())
}
