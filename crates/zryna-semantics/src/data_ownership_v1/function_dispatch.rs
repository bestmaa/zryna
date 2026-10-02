use zryna_syntax::v4::RawTypeSyntaxKind;
mod source_features;
use source_features::{has_root_borrow_syntax, uses_owned_function};

use super::copy_enum_match::lower_enum_match_function;
use super::copy_function_lowering::lower_copy_function;
use super::owned_aggregate_lowering::{
    is_private_mixed_constructor_candidate, is_private_owned_aggregate_candidate,
    lower_private_owned_aggregate_function, requires_generic_function, requires_structured_cfg,
};
use super::owned_control_flow_shape::is_terminal_owned_phi_candidate;
use super::owned_enum_payload_move::{
    is_private_owned_enum_payload_move_candidate, lower_private_owned_enum_payload_move_function,
};
use super::owned_root_borrow_planning::{
    is_direct_owned_root_borrow_candidate, plan_private_owned_root_borrow_syntax,
};
use super::owned_root_borrow_postprocessing::postprocess_private_owned_root_borrow_function;
use super::owned_string_lowering::lower_private_string_function;
use super::owned_vec_lowering::lower_private_vec_function;
use super::root_borrow_function_lowering::lower_private_root_borrow_function;
use super::{
    Decl, Errors, FunctionCatalog, RawExpressionKind, RawStatementKind, SemanticInput, SourceMap,
    Ty, TypeCategory, layout, raw, raw_layout, semantic_type, syntax,
};
use crate::data_ownership_v1::diagnostics::span;
use crate::data_ownership_v1::type_model::require_current_type_only_boundary;

#[allow(clippy::too_many_arguments)]
fn lower_private_owned_root_borrow_function<'a>(
    input: SemanticInput<'a>,
    module: usize,
    declaration: usize,
    function: &'a syntax::RawFunctionSyntax,
    declarations: &'a [Decl],
    graph: &'a raw_layout::Graph,
    node_types: &'a [Option<Ty>],
    layouts: &'a layout::VerifiedLayouts,
    catalog: &'a FunctionCatalog,
    result: Ty,
    errors: &mut Errors<'a>,
) -> Option<raw::Function> {
    let plan = plan_private_owned_root_borrow_syntax(
        input,
        module,
        function,
        declarations,
        graph,
        node_types,
        result,
        errors,
    )?;
    let lowered = lower_function_impl(
        input,
        module,
        declaration,
        &plan.synthetic,
        declarations,
        graph,
        node_types,
        layouts,
        catalog,
        errors,
    )?;
    postprocess_private_owned_root_borrow_function(
        span(input.sources(), function.span),
        &plan,
        lowered,
        errors,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_function<'a>(
    input: SemanticInput<'a>,
    module: usize,
    declaration: usize,
    function: &'a syntax::RawFunctionSyntax,
    declarations: &'a [Decl],
    graph: &'a raw_layout::Graph,
    node_types: &'a [Option<Ty>],
    layouts: &'a layout::VerifiedLayouts,
    catalog: &'a FunctionCatalog,
    errors: &mut Errors<'a>,
) -> Option<raw::Function> {
    let file = &input.syntax().files()[module];
    let result =
        semantic_type(file, function.result_type, module, declarations, graph, node_types, errors)?;
    if uses_owned_function(input, function) {
        return lower_private_owned_aggregate_function(
            input,
            module,
            declaration,
            function,
            declarations,
            graph,
            node_types,
            layouts,
            catalog,
            result,
            errors,
        );
    }
    if super::owned_aggregate_lowering::has_indexed_borrow(function)
        || super::owned_aggregate_lowering::has_nonindexed_owned_borrow(
            function,
            file,
            module,
            declarations,
            node_types,
        )
    {
        if let Some(at) = function.export_span {
            errors.at(
                "ZRYNA-M3017",
                span(input.sources(), at),
                "indexed borrowing remains an internal private source route",
                "keep indexed borrowing functions private until profile integration",
            );
            return None;
        }
        verify_single_final_return(function, input.sources(), errors)?;
        return lower_private_owned_aggregate_function(
            input,
            module,
            declaration,
            function,
            declarations,
            graph,
            node_types,
            layouts,
            catalog,
            result,
            errors,
        );
    }
    if has_root_borrow_syntax(file, function)
        && (result.is_copy() || !is_direct_owned_root_borrow_candidate(file, function))
    {
        return lower_private_root_borrow_function(
            input,
            module,
            declaration,
            function,
            declarations,
            graph,
            node_types,
            layouts,
            catalog,
            result,
            errors,
        );
    }
    if has_root_borrow_syntax(file, function) {
        return lower_private_owned_root_borrow_function(
            input,
            module,
            declaration,
            function,
            declarations,
            graph,
            node_types,
            layouts,
            catalog,
            result,
            errors,
        );
    }
    lower_function_impl(
        input,
        module,
        declaration,
        function,
        declarations,
        graph,
        node_types,
        layouts,
        catalog,
        errors,
    )
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn lower_function_impl<'a>(
    input: SemanticInput<'a>,
    module: usize,
    declaration: usize,
    function: &syntax::RawFunctionSyntax,
    declarations: &'a [Decl],
    graph: &'a raw_layout::Graph,
    node_types: &'a [Option<Ty>],
    layouts: &'a layout::VerifiedLayouts,
    catalog: &'a FunctionCatalog,
    errors: &mut Errors<'a>,
) -> Option<raw::Function> {
    let file = &input.syntax().files()[module];
    let result =
        semantic_type(file, function.result_type, module, declarations, graph, node_types, errors)?;
    let has_vec_operation = function.body.expressions.iter().any(|expression| {
        matches!(
            expression.kind,
            RawExpressionKind::VecConstruction { .. } | RawExpressionKind::VecPush { .. }
        )
    });
    let has_vec_signature = catalog
        .modules
        .get(module)
        .and_then(|signatures| signatures.get(declaration))
        .and_then(Option::as_ref)
        .is_some_and(|signature| {
            signature.result.category == TypeCategory::Vec
                || signature
                    .parameters
                    .iter()
                    .any(|parameter| parameter.category == TypeCategory::Vec)
        });
    let has_vec_local = function.body.statements.iter().any(|statement| {
        let RawStatementKind::LocalDeclaration { type_syntax, .. } = statement.kind else {
            return false;
        };
        usize::try_from(type_syntax)
            .ok()
            .and_then(|index| file.type_syntax().get(index))
            .is_some_and(|ty| matches!(ty.kind, RawTypeSyntaxKind::Vec { .. }))
    });
    let terminal_owned_phi_candidate =
        is_terminal_owned_phi_candidate(function, result.category, has_vec_operation);
    let generic_function = catalog
        .modules
        .get(module)
        .and_then(|signatures| signatures.get(declaration))
        .and_then(Option::as_ref)
        .is_some_and(|signature| {
            requires_generic_function(
                function,
                (signature, catalog),
                file,
                declarations,
                node_types,
                layouts,
            )
        });
    let structured_owned_function = function.export_span.is_none()
        && requires_structured_cfg(function)
        && (generic_function
            || !result.is_copy()
            || has_vec_signature
            || has_vec_local
            || has_vec_operation);
    let existing_payload_move = function.export_span.is_none()
        && matches!(result.category, TypeCategory::Struct | TypeCategory::FixedArray)
        && !function.body.expressions.iter().any(|expression| matches!(&expression.kind, RawExpressionKind::Match { arms, .. } if arms.len() > 1))
        && is_private_owned_enum_payload_move_candidate(function);
    if (generic_function || structured_owned_function) && !existing_payload_move {
        if !requires_structured_cfg(function) {
            verify_single_final_return(function, input.sources(), errors)?;
        }
        return lower_private_owned_aggregate_function(
            input,
            module,
            declaration,
            function,
            declarations,
            graph,
            node_types,
            layouts,
            catalog,
            result,
            errors,
        );
    }
    if !terminal_owned_phi_candidate {
        verify_single_final_return(function, input.sources(), errors)?;
    }
    if result.category == TypeCategory::String
        && function.export_span.is_none()
        && !has_vec_operation
    {
        let diagnostics_before = errors.len();
        let lowered = lower_private_string_function(
            input,
            module,
            declaration,
            function,
            declarations,
            graph,
            node_types,
            result,
            catalog,
            errors,
        );
        if lowered.is_none() && errors.len() == diagnostics_before {
            errors.at(
                "ZRYNA-M3012",
                span(input.sources(), function.span),
                "private String lowering rejected a source function without a specific diagnostic",
                "use only the exact supported private String forms",
            );
        }
        return lowered;
    }
    if function.export_span.is_none()
        && matches!(result.category, TypeCategory::Struct | TypeCategory::FixedArray)
        && is_private_owned_enum_payload_move_candidate(function)
    {
        return lower_private_owned_enum_payload_move_function(
            input,
            module,
            declaration,
            function,
            declarations,
            graph,
            node_types,
            layouts,
            result,
            errors,
        );
    }
    if is_private_owned_aggregate_candidate(function, result)
        || is_private_mixed_constructor_candidate(function, result, layouts)
    {
        return lower_private_owned_aggregate_function(
            input,
            module,
            declaration,
            function,
            declarations,
            graph,
            node_types,
            layouts,
            catalog,
            result,
            errors,
        );
    }
    if function.export_span.is_none() && (has_vec_signature || has_vec_local || has_vec_operation) {
        let diagnostics_before = errors.len();
        let lowered = lower_private_vec_function(
            input,
            module,
            declaration,
            function,
            declarations,
            graph,
            node_types,
            layouts,
            result,
            catalog,
            errors,
        );
        if lowered.is_none() && errors.len() == diagnostics_before {
            errors.at(
                "ZRYNA-M3013",
                span(input.sources(), function.span),
                "private Vec lowering rejected a source function without a specific diagnostic",
                "use only the exact supported private Vec forms",
            );
        }
        return lowered;
    }
    require_current_type_only_boundary(
        result,
        span(input.sources(), function.span),
        function.export_span.is_some(),
        errors,
    )?;
    let tail_match = usize::try_from(function.body.root_block)
        .ok()
        .and_then(|index| function.body.blocks.get(index))
        .and_then(|block| <&[u32; 1]>::try_from(block.statements.as_slice()).ok())
        .and_then(|[statement]| usize::try_from(*statement).ok())
        .and_then(|index| function.body.statements.get(index))
        .and_then(|statement| match statement.kind {
            RawStatementKind::Return { value, .. } => Some(value),
            _ => None,
        })
        .and_then(|value| usize::try_from(value).ok())
        .and_then(|index| function.body.expressions.get(index))
        .is_some_and(|expression| matches!(expression.kind, RawExpressionKind::Match { .. }));
    if tail_match {
        return lower_enum_match_function(
            input,
            module,
            declaration,
            function,
            declarations,
            graph,
            node_types,
            layouts,
            catalog,
            result,
            errors,
        );
    }
    lower_copy_function(
        input,
        file,
        module,
        declaration,
        function,
        declarations,
        graph,
        node_types,
        layouts,
        catalog,
        result,
        errors,
    )
}

fn verify_single_final_return(
    function: &syntax::RawFunctionSyntax,
    sources: &SourceMap,
    errors: &mut Errors<'_>,
) -> Option<()> {
    let root = usize::try_from(function.body.root_block)
        .ok()
        .and_then(|index| function.body.blocks.get(index))?;
    let mut first_return = None;
    for (position, statement_id) in root.statements.iter().enumerate() {
        let Some(statement) = usize::try_from(*statement_id)
            .ok()
            .and_then(|index| function.body.statements.get(index))
        else {
            continue;
        };
        if matches!(statement.kind, RawStatementKind::Return { .. }) {
            if first_return.replace(position).is_some() {
                errors.at(
                    "ZRYNA-M3010",
                    span(sources, statement.span),
                    "function contains more than one return",
                    "use exactly one return as the final root-block statement",
                );
                return None;
            }
        } else if first_return.is_some() {
            errors.at(
                "ZRYNA-M3010",
                span(sources, statement.span),
                "statement appears after the function return",
                "make the single return the final root-block statement",
            );
            return None;
        }
    }
    if first_return == root.statements.len().checked_sub(1) {
        Some(())
    } else {
        errors.at(
            "ZRYNA-M3010",
            span(sources, function.body.span),
            "function must end with exactly one value return",
            "make one return the final root-block statement",
        );
        None
    }
}
