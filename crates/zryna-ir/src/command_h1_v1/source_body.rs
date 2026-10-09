use super::{error, outcome_shape, raw};
use zryna_diagnostics::Diagnostic;

mod matches;
use zryna_layout::{TypeCategory, VerifiedLayouts};
use zryna_source::SourceMap;
use zryna_syntax::{
    command_h1_v1::{self as syntax, CommandSyntax},
    v4::RawTypeSyntaxKind,
};

pub(super) fn verify(
    program: &raw::Program,
    sources: &SourceMap,
    source: &CommandSyntax,
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
) -> Result<(), Vec<Diagnostic>> {
    let admitted =
        syntax::admit(source.syntax(), sources).map_err(|diagnostic| vec![diagnostic])?;
    if admitted.environment() != source.environment() || sources.len() != 1 {
        return Err(vec![error("command source authority does not match this final source")]);
    }
    let file = &admitted.syntax().files()[0];
    let [module] = program.modules.as_slice() else {
        return Err(vec![error("command requires one complete module")]);
    };
    if module.functions.len() != file.functions().len() {
        return Err(vec![error("command IR omitted or added source functions")]);
    }
    let builtin = admitted.uses_outcome();
    let declarations = u32::try_from(file.data_declarations().len())
        .map_err(|_| vec![error("source declaration count exceeds its bound")])?;
    if module.data_declarations != declarations + u32::from(builtin) {
        return Err(vec![error("command source and builtin declaration inventories disagree")]);
    }
    let mut entry_count = 0;
    let mut effects = 0;
    for (index, (function, syntax_function)) in
        module.functions.iter().zip(file.functions()).enumerate()
    {
        matches::verify(function, syntax_function, sources, linear, declarations)?;
        if sources.verify_span(syntax_function.span).ok() != Some(function.span) {
            return Err(vec![error("command function identity does not match exact source order")]);
        }
        if syntax_function.export_span.is_some() {
            let result = usize::try_from(syntax_function.result_type)
                .ok()
                .and_then(|id| file.type_syntax().get(id));
            if syntax_function.name.text != "main"
                || !syntax_function.parameters.is_empty()
                || !function.parameters.is_empty()
                || !function.borrow_parameters.is_empty()
                || function.entry_export.as_deref() != Some("main")
                || !matches!(result.map(|ty| &ty.kind), Some(RawTypeSyntaxKind::Named { name }) if name.text == "bool")
                || !linear.types().any(|ty| {
                    ty.id().index() == function.result.0 && ty.category() == TypeCategory::Bool
                })
            {
                return Err(vec![error("command's sole exported entry must be main(): bool")]);
            }
            entry_count += 1;
        } else if function.entry_export.is_some() {
            return Err(vec![error("command IR invented a public source entry")]);
        }
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let raw::InstructionKind::EnvironmentLookup { key, .. } = &instruction.kind {
                effects += 1;
                let required = admitted.environment().ok_or_else(|| {
                    vec![error("pure source cannot acquire an environment effect")]
                })?;
                let result = instruction.result.ok_or_else(|| {
                    vec![error("environment operation is missing its owned result")]
                })?;
                if effects != 1
                    || key != required.key()
                    || index != required.function_index()
                    || instruction.span != required.call_span()
                    || result.span != required.call_span()
                    || outcome_shape(linear, result.ty) != Some((0, declarations))
                    || outcome_shape(linux, result.ty) != Some((0, declarations))
                {
                    return Err(vec![error(
                        "environment effect or closed owned outcome does not match exact source authority",
                    )]);
                }
            }
            if let raw::InstructionKind::EnumConstruct { .. } = instruction.kind
                && instruction.result.is_some_and(|result| {
                    outcome_shape(linear, result.ty) == Some((0, declarations))
                })
            {
                return Err(vec![error(
                    "a command outcome can only originate from the authenticated environment operation",
                )]);
            }
        }
    }
    if entry_count != 1 || effects != usize::from(admitted.environment().is_some()) {
        return Err(vec![error("command entry or complete source effect inventory is missing")]);
    }
    if effects == 1 && !file.functions().iter().any(matches::has_source_match) {
        return Err(vec![error(
            "environment outcome must be consumed by a closed exhaustive source match",
        )]);
    }
    Ok(())
}
