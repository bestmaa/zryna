use std::collections::{BTreeMap, BTreeSet};

use zryna_ir::data_ownership_v1::{self as ir, raw};
use zryna_layout::{self as layout, TypeCategory, raw as raw_layout};
use zryna_syntax::v4::{self as syntax, RawStatementKind};

use super::super::SemanticInput;
use super::super::diagnostics::Errors;
use super::super::layout_graph::{Decl, semantic_type};
use super::super::owner_state::OwnerState;
use super::super::type_model::{Binding, Ty};
use super::{
    PrivateOwnedAggregateLowerer, StatementOutcome, aggregate_graph_is_supported,
    owned_enum_graph_is_supported,
};
use crate::data_ownership_v1::diagnostics::span;

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn lower_owned_aggregate_function_impl<'a>(
    input: SemanticInput<'a>,
    module: usize,
    declaration: usize,
    function: &syntax::RawFunctionSyntax,
    declarations: &'a [Decl],
    graph: &'a raw_layout::Graph,
    node_types: &'a [Option<Ty>],
    layouts: &'a layout::VerifiedLayouts,
    catalog: &'a super::super::function_catalog::FunctionCatalog,
    result: Ty,
    errors: &mut Errors<'a>,
) -> Option<raw::Function> {
    let file = &input.syntax().files()[module];
    let signature = catalog.modules.get(module)?.get(declaration)?.as_ref()?;
    let generic_function = super::requires_generic_function(
        function,
        (signature, catalog),
        file,
        declarations,
        node_types,
        layouts,
    );
    let structured_function = signature.private && super::requires_structured_cfg(function);
    let generic_surface = input.command.is_some() || generic_function || structured_function;
    if !if generic_surface {
        super::mixed_shape::supported(result, layouts)
    } else if super::mixed_shape::requires_summary(result, layouts) {
        true
    } else if result.category == TypeCategory::Enum {
        owned_enum_graph_is_supported(result, layouts)
    } else {
        aggregate_graph_is_supported(result, layouts, &mut BTreeSet::new())
    } {
        errors.at(
            "ZRYNA-M3016",
            span(input.sources(), function.span),
            "owned aggregate graph contains an unsupported nested enum, Vec, handle, borrow, or cycle",
            "use an acyclic Struct/Enum/FixedArray graph with only bool, i32, and String leaves",
        );
        return None;
    }
    assert_eq!(signature.name, function.name.text, "aggregate catalog declaration name");
    assert_eq!(signature.result, result, "aggregate catalog result type");
    assert_eq!(signature.id.module.0 as usize, module, "aggregate catalog module");
    assert_eq!(signature.id.declaration as usize, declaration, "aggregate catalog declaration");
    let root = usize::try_from(function.body.root_block)
        .ok()
        .and_then(|index| function.body.blocks.get(index))?;
    if !generic_function && result.category == TypeCategory::Enum && !function.parameters.is_empty()
    {
        errors.at(
            "ZRYNA-M3016",
            span(input.sources(), function.parameters[0].span),
            "private owned enum functions do not admit parameters",
            "construct the enum from literals and explicitly typed initialized locals",
        );
        return None;
    }
    let mut lowerer = PrivateOwnedAggregateLowerer {
        input,
        file,
        function,
        module,
        declarations,
        graph,
        node_types,
        layouts,
        catalog,
        mixed_function: generic_surface
            || (signature.private
                && !result.is_copy()
                && super::mixed_shape::requires_summary(result, layouts)),
        errors,
        bindings: BTreeMap::new(),
        projections: BTreeMap::new(),
        moved_projections: BTreeSet::new(),
        partial_roots: BTreeSet::new(),
        places: Vec::new(),
        instructions: Vec::new(),
        constructor_types: super::ConstructorValueTypes::default(),
        constructor_storage: super::constructor_resources::ConstructorStorage::default(),
        preparation_facts: super::preparation_plan::PreparationFacts::default(),
        cleanup_plans: Vec::new(),
        cleanup_actions: 0,
        aggregate_operands: 0,
        aggregate_subobject_moves: 0,
        projected_aggregate_clones: 0,
        projected_aggregate_assignments: 0,
        reserved_transitions: 0,
        owners: OwnerState::default(),
        next_value: 0,
        next_local: 0,
    };
    let mut parameters = Vec::with_capacity(signature.parameters.len());
    let mut borrow_parameters = Vec::with_capacity(signature.borrow_parameters.len());
    if signature.borrow_parameters.len() > ir::MAX_ACTIVE_BORROWS_PER_FUNCTION {
        lowerer.errors.at(
            "ZRYNA-M3201",
            span(input.sources(), function.span),
            "formal borrow parameters exceed the active authority limit",
            "reduce borrow parameters",
        );
        return None;
    }
    for (parameter, order) in function.parameters.iter().zip(&signature.parameter_order) {
        if lowerer
            .bindings
            .keys()
            .chain(lowerer.preparation_facts.aliases.keys())
            .any(|name| name.eq_ignore_ascii_case(&parameter.name.text))
        {
            lowerer.errors.at(
                "ZRYNA-M3002",
                span(input.sources(), parameter.name.span),
                "parameter collides with an existing value or borrow binding",
                "use portable distinct parameter names",
            );
            return None;
        }
        if let super::super::function_catalog::FunctionParameterOrder::Borrow(index) = *order {
            let descriptor = *signature.borrow_parameters.get(index as usize)?;
            if !generic_surface || !super::mixed_shape::supported(descriptor.referent, layouts) {
                lowerer.errors.at(
                    "ZRYNA-M3017",
                    descriptor.span,
                    "borrow parameter is outside the exact non-handle signature",
                    "use a supported exact non-escaping referent",
                );
                return None;
            }
            let id = raw::BorrowId(index);
            borrow_parameters.push(raw::BorrowParameter {
                id,
                referent: descriptor.referent.ir,
                access: descriptor.access,
                span: descriptor.span,
            });
            lowerer.preparation_facts.parameter_borrows.insert(id);
            lowerer.preparation_facts.next_borrow = index.checked_add(1)?;
            lowerer.preparation_facts.aliases.insert(
                parameter.name.text.clone(),
                super::preparation_plan::LexicalAlias {
                    borrow: id,
                    ty: descriptor.referent,
                    access: descriptor.access,
                },
            );
            continue;
        }
        let index = parameters.len();
        let ty = semantic_type(
            file,
            parameter.type_syntax,
            module,
            declarations,
            graph,
            node_types,
            lowerer.errors,
        )?;
        let admitted = if generic_surface {
            super::mixed_shape::supported(ty, layouts)
        } else {
            ty.is_copy() && matches!(ty.category, TypeCategory::Bool | TypeCategory::I32)
        };
        if !admitted {
            lowerer.errors.at(
                "ZRYNA-M3016",
                span(input.sources(), parameter.span),
                "owned aggregate functions do not admit owned or aggregate parameters",
                "use only optional bool/i32 parameters in this private checkpoint",
            );
            return None;
        }
        if lowerer.bindings.keys().any(|name| name.eq_ignore_ascii_case(&parameter.name.text)) {
            lowerer.errors.at(
                "ZRYNA-M3002",
                span(input.sources(), parameter.name.span),
                format!(
                    "parameter '{}' collides under portable ASCII case folding",
                    parameter.name.text
                ),
                "give every binding one portable case-insensitive unique name",
            );
            return None;
        }
        if lowerer.budget_values() >= ir::MAX_VALUES_PER_FUNCTION
            || lowerer.budget_places() >= ir::MAX_PLACES_PER_FUNCTION
        {
            lowerer.errors.at(
                "ZRYNA-M3201",
                span(input.sources(), parameter.span),
                "derived aggregate parameter storage exceeds an M3 resource limit",
                "reduce private aggregate parameters",
            );
            return None;
        }
        let value = raw::ValueId(lowerer.next_value);
        lowerer.next_value += 1;
        parameters.push(raw::ValueDefinition {
            id: value,
            ty: ty.ir,
            span: span(input.sources(), parameter.span),
        });
        lowerer
            .constructor_types
            .record_parameter(parameters.last().expect("new parameter"))
            .expect("aggregate parameters have dense emitted identities");
        let place = raw::PlaceId(u32::try_from(lowerer.places.len()).ok()?);
        lowerer.places.push(raw::Place {
            id: place,
            ty: ty.ir,
            span: span(input.sources(), parameter.span),
            kind: raw::PlaceKind::Parameter(u32::try_from(index).ok()?),
        });
        lowerer.bindings.insert(parameter.name.text.clone(), Binding { ty, place, mutable: false });
        if !ty.is_copy() {
            lowerer.owners.register_parameter(place)?;
        }
    }
    if structured_function {
        let blocks = lowerer.lower_structured_cfg(&parameters, result)?;
        assert!(lowerer.constructor_storage_is_clear(), "structured constructor credits released");
        assert_eq!(lowerer.reserved_transitions, 0, "structured transition credits released");
        return Some(raw::Function {
            id: raw::FunctionId {
                module: raw::ModuleId(u32::try_from(module).ok()?),
                declaration: u32::try_from(declaration).ok()?,
            },
            entry_export: None,
            span: span(input.sources(), function.span),
            parameters,
            borrow_parameters,
            result: result.ir,
            places: lowerer.places,
            blocks,
            cleanup_plans: lowerer.cleanup_plans,
        });
    }
    let mut returned = None;
    let final_statement = root.statements.last().copied();
    let return_count = root
        .statements
        .iter()
        .filter(|statement_id| {
            usize::try_from(**statement_id)
                .ok()
                .and_then(|index| function.body.statements.get(index))
                .is_some_and(|statement| matches!(statement.kind, RawStatementKind::Return { .. }))
        })
        .count();
    for statement_id in &root.statements {
        let statement = usize::try_from(*statement_id)
            .ok()
            .and_then(|index| function.body.statements.get(index))?;
        if let StatementOutcome::Return(value, at) = lowerer.lower_statement(
            *statement_id,
            statement,
            result,
            final_statement,
            return_count,
        )? {
            returned = Some((value, at));
        }
    }
    let (returned, return_span) = returned?;
    assert!(lowerer.constructor_storage_is_clear(), "constructor storage credits released");
    assert_eq!(lowerer.reserved_transitions, 0, "aggregate transition credits released");
    let return_owner = lowerer.owners.owner(returned);
    let cleanup = lowerer.push_cleanup(return_span, return_owner)?;
    Some(raw::Function {
        id: raw::FunctionId {
            module: raw::ModuleId(u32::try_from(module).ok()?),
            declaration: u32::try_from(declaration).ok()?,
        },
        entry_export: None,
        span: span(input.sources(), function.span),
        parameters,
        borrow_parameters,
        result: result.ir,
        places: lowerer.places,
        blocks: vec![raw::Block {
            id: raw::BlockId(0),
            parameters: Vec::new(),
            instructions: lowerer.instructions,
            terminators: vec![raw::SpannedTerminator {
                span: return_span,
                kind: raw::Terminator::Return { value: returned, cleanup },
            }],
        }],
        cleanup_plans: lowerer.cleanup_plans,
    })
}

pub(in crate::data_ownership_v1) fn is_private_owned_aggregate_candidate(
    function: &syntax::RawFunctionSyntax,
    result: Ty,
) -> bool {
    function.export_span.is_none()
        && !result.is_copy()
        && matches!(
            result.category,
            TypeCategory::Struct | TypeCategory::Enum | TypeCategory::FixedArray
        )
}

pub(in crate::data_ownership_v1) fn is_private_mixed_constructor_candidate(
    function: &syntax::RawFunctionSyntax,
    result: Ty,
    layouts: &layout::VerifiedLayouts,
) -> bool {
    function.export_span.is_none()
        && !result.is_copy()
        && super::mixed_shape::requires_summary(result, layouts)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::data_ownership_v1) fn lower_private_owned_aggregate_function<'a>(
    input: SemanticInput<'a>,
    module: usize,
    declaration: usize,
    function: &syntax::RawFunctionSyntax,
    declarations: &'a [Decl],
    graph: &'a raw_layout::Graph,
    node_types: &'a [Option<Ty>],
    layouts: &'a layout::VerifiedLayouts,
    catalog: &'a super::super::function_catalog::FunctionCatalog,
    result: Ty,
    errors: &mut Errors<'a>,
) -> Option<raw::Function> {
    let diagnostics_before = errors.len();
    let lowered = lower_owned_aggregate_function_impl(
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
    if lowered.is_none() && errors.len() == diagnostics_before {
        errors.at(
            "ZRYNA-M3016",
            span(input.sources(), function.span),
            "private owned aggregate lowering rejected a source function without a specific diagnostic",
            "use only the exact straight-line owned Struct/Enum/FixedArray forms",
        );
    }
    lowered
}
