use std::collections::BTreeMap;

use zryna_ir::data_ownership_v1::{
    VerifiedBackendInstruction as B, VerifiedBackendTerminator as T, VerifiedBorrowAccess,
    VerifiedCallArgument, VerifiedDropActionKind, VerifiedFunction, VerifiedInstructionKind as K,
    VerifiedPlaceKind,
};
use zryna_layout::{TypeCategory, VerifiedLayouts};
use zryna_ownership_runtime_abi::{LogicalOperation, VerifiedOwnershipRuntimeAbi};

use super::{VerifiedMirModule, raw, verify};
mod operands;
use operands::lower_operands;

/// Lowers sealed target-neutral ownership IR and independently verifies every native claim.
///
/// # Errors
/// Returns stable bounded diagnostics for a mismatched authority or internal lowering failure.
pub fn lower(
    program: &zryna_ir::data_ownership_v1::VerifiedProgram,
    runtime: &VerifiedOwnershipRuntimeAbi,
) -> Result<VerifiedMirModule, Vec<zryna_diagnostics::Diagnostic>> {
    let raw = lower_unverified(program, runtime)?;
    verify(raw, program, runtime)
}

/// Produces untrusted native MIR claims for verifier and hostile-input testing.
/// The returned value grants no code-generation authority until [`verify`] succeeds.
///
/// # Errors
/// Returns stable diagnostics if a sealed operand or runtime symbol cannot be represented.
pub fn lower_unverified(
    program: &zryna_ir::data_ownership_v1::VerifiedProgram,
    runtime: &VerifiedOwnershipRuntimeAbi,
) -> Result<raw::Program, Vec<zryna_diagnostics::Diagnostic>> {
    let layouts = program.linux_x86_64_layouts();
    let symbols = runtime
        .operations()
        .zip(runtime.native_linux_x86_64_functions())
        .map(|(operation, function)| (operation.operation(), function.symbol().to_owned()))
        .collect::<BTreeMap<_, _>>();
    Ok(raw::Program {
        authority: raw::Authority {
            type_universe: program.type_universe_identity().as_bytes(),
            linux_layout: *layouts.fingerprint(),
            runtime_identifier: runtime.identifier().to_owned(),
        },
        types: layouts.types().map(lower_type).collect(),
        functions: program
            .modules()
            .flat_map(zryna_ir::data_ownership_v1::VerifiedModule::functions)
            .map(|function| lower_function(function, layouts, &symbols))
            .collect::<Result<_, _>>()?,
        runtime_symbols: runtime
            .native_linux_x86_64_functions()
            .map(|function| function.symbol().to_owned())
            .collect(),
    })
}

fn lower_type(ty: zryna_layout::VerifiedType<'_>) -> raw::Type {
    raw::Type {
        id: ty.id().index(),
        category: match ty.category() {
            TypeCategory::Bool => raw::TypeCategory::Bool,
            TypeCategory::I32 => raw::TypeCategory::I32,
            TypeCategory::Struct => raw::TypeCategory::Struct,
            TypeCategory::Enum => raw::TypeCategory::Enum,
            TypeCategory::FixedArray => raw::TypeCategory::FixedArray,
            TypeCategory::String => raw::TypeCategory::String,
            TypeCategory::Vec => raw::TypeCategory::Vec,
            TypeCategory::Shared => raw::TypeCategory::Shared,
            TypeCategory::Weak => raw::TypeCategory::Weak,
        },
        size: ty.size(),
        alignment: ty.alignment(),
        drop_kind: ty.drop_kind(),
        runtime_kind: ty.runtime_kind(),
        fields: ty
            .fields()
            .iter()
            .map(|field| raw::Field {
                ordinal: field.ordinal(),
                ty: field.ty().index(),
                offset: field.offset(),
            })
            .collect(),
        variants: ty
            .variants()
            .iter()
            .map(|variant| raw::Variant {
                ordinal: variant.ordinal(),
                payload: variant.payload().map(zryna_layout::TypeId::index),
            })
            .collect(),
        array_stride: ty.array_stride(),
        array_length: ty.array_length(),
        enum_payload: ty.enum_payload_layout(),
        referenced_type: ty.referenced_type().map(zryna_layout::TypeId::index),
    }
}

#[allow(clippy::too_many_lines)]
fn lower_function(
    function: VerifiedFunction<'_>,
    layouts: &VerifiedLayouts,
    symbols: &BTreeMap<LogicalOperation, String>,
) -> Result<raw::Function, Vec<zryna_diagnostics::Diagnostic>> {
    let mut cleanup_kinds = BTreeMap::new();
    for instruction in
        function.blocks().flat_map(zryna_ir::data_ownership_v1::VerifiedBlock::instructions)
    {
        if let Some(plan) = instruction.cleanup() {
            for action in instruction.derived_drop_actions() {
                cleanup_kinds
                    .insert((plan.index(), action.root().index()), drop_kind(action.kind()));
            }
        }
    }
    for terminator in function.blocks().map(zryna_ir::data_ownership_v1::VerifiedBlock::terminator)
    {
        if let Some(plan) = terminator.cleanup() {
            for action in terminator.derived_drop_actions() {
                cleanup_kinds
                    .insert((plan.index(), action.root().index()), drop_kind(action.kind()));
            }
        }
    }
    let mut lowered = raw::Function {
        module: function.id().module(),
        declaration: function.id().declaration(),
        symbol: format!("zryna_m3_m{}_f{}", function.id().module(), function.id().declaration()),
        parameters: function
            .parameters()
            .map(|value| raw::Value { id: value.id().index(), ty: value.ty().index() })
            .collect(),
        borrow_parameters: function
            .borrow_parameters()
            .map(|borrow| raw::BorrowParameter {
                id: borrow.id().index(),
                referent: borrow.referent().index(),
                access: match borrow.access() {
                    VerifiedBorrowAccess::Shared => raw::BorrowAccess::Shared,
                    VerifiedBorrowAccess::Exclusive => raw::BorrowAccess::Exclusive,
                },
            })
            .collect(),
        result_type: function.result_type().index(),
        places: function
            .places()
            .map(|place| lower_place(function, place, layouts))
            .collect::<Result<_, _>>()?,
        blocks: function
            .blocks()
            .map(|block| {
                Ok(raw::Block {
                    id: block.id().index(),
                    parameters: block
                        .parameters()
                        .map(|value| raw::Value { id: value.id().index(), ty: value.ty().index() })
                        .collect(),
                    operations: block
                        .instructions()
                        .map(|instruction| lower_operation(function, instruction, symbols))
                        .collect::<Result<_, _>>()?,
                    terminator: lower_terminator(block.terminator(), symbols)?,
                    cleanup: block
                        .terminator()
                        .cleanup()
                        .map(zryna_ir::data_ownership_v1::CleanupPlanIdentity::index),
                })
            })
            .collect::<Result<_, Vec<zryna_diagnostics::Diagnostic>>>()?,
        cleanup_plans: function
            .cleanup_plans()
            .map(|plan| raw::CleanupPlan {
                id: plan.id().index(),
                actions: plan
                    .actions()
                    .map(|place| raw::DropAction {
                        place: place.index(),
                        kind: cleanup_kinds
                            .get(&(plan.id().index(), place.index()))
                            .copied()
                            .unwrap_or(raw::DropKind::Place),
                    })
                    .collect(),
            })
            .collect(),
    };
    for (source_block, block) in function.blocks().zip(&mut lowered.blocks) {
        for (source, operation) in source_block.instructions().zip(&mut block.operations) {
            if !verify::source_binding::needs_storage_plan(source, layouts) {
                continue;
            }
            let actions = source
                .allocation_failure_drop_actions()
                .map(|action| raw::DropAction {
                    place: action.root().index(),
                    kind: drop_kind(action.kind()),
                })
                .collect::<Vec<_>>();
            let plan = if let Some(plan) =
                lowered.cleanup_plans.iter().find(|plan| plan.actions == actions)
            {
                plan.id
            } else {
                let id = u32::try_from(lowered.cleanup_plans.len()).map_err(|_| {
                    vec![super::error(
                        "ZRYNA-N3107",
                        "constructor cleanup inventory exceeds native MIR limits",
                    )]
                })?;
                lowered.cleanup_plans.push(raw::CleanupPlan { id, actions });
                id
            };
            operation.cleanup = Some(plan);
        }
    }
    Ok(lowered)
}

fn lower_place(
    function: VerifiedFunction<'_>,
    place: zryna_ir::data_ownership_v1::VerifiedPlace<'_>,
    layouts: &VerifiedLayouts,
) -> Result<raw::Place, Vec<zryna_diagnostics::Diagnostic>> {
    let kind = match place.kind() {
        VerifiedPlaceKind::Parameter(index) => raw::PlaceKind::Parameter(index),
        VerifiedPlaceKind::Local(index) => raw::PlaceKind::Local(index),
        VerifiedPlaceKind::Temporary(value) => raw::PlaceKind::Temporary(value.index()),
        VerifiedPlaceKind::StructField { base, ordinal } => {
            let base_ty = function
                .places()
                .find(|candidate| candidate.id() == base)
                .and_then(|candidate| layouts.type_by_id(candidate.ty()));
            let offset = base_ty
                .and_then(|ty| ty.fields().iter().find(|field| field.ordinal() == ordinal))
                .map(|field| field.offset())
                .ok_or_else(lowering_error)?;
            raw::PlaceKind::Field { base: base.index(), offset }
        }
        VerifiedPlaceKind::EnumPayload { base, variant } => {
            let base_ty = function
                .places()
                .find(|candidate| candidate.id() == base)
                .and_then(|candidate| layouts.type_by_id(candidate.ty()));
            let offset = base_ty
                .and_then(zryna_layout::VerifiedType::enum_payload_layout)
                .map(|layout| layout.0)
                .ok_or_else(lowering_error)?;
            raw::PlaceKind::EnumPayload { base: base.index(), offset, variant }
        }
        VerifiedPlaceKind::FixedArrayConstant { base, index } => {
            let base_ty = function
                .places()
                .find(|candidate| candidate.id() == base)
                .and_then(|candidate| layouts.type_by_id(candidate.ty()));
            let offset = base_ty
                .and_then(zryna_layout::VerifiedType::array_stride)
                .and_then(|stride| stride.checked_mul(u64::from(index)))
                .ok_or_else(lowering_error)?;
            raw::PlaceKind::ArrayElement { base: base.index(), offset }
        }
    };
    Ok(raw::Place { id: place.id().index(), ty: place.ty().index(), kind })
}

fn lower_operation(
    function: VerifiedFunction<'_>,
    instruction: zryna_ir::data_ownership_v1::VerifiedInstruction<'_>,
    symbols: &BTreeMap<LogicalOperation, String>,
) -> Result<raw::Operation, Vec<zryna_diagnostics::Diagnostic>> {
    let kind = instruction.kind();
    let opcode = match kind {
        K::EnvironmentLookup => return Err(lowering_error()),
        K::BoolLiteral => raw::Opcode::BoolLiteral,
        K::I32Literal => raw::Opcode::I32Literal,
        K::I32Add => raw::Opcode::I32Add,
        K::I32Sub => raw::Opcode::I32Sub,
        K::I32Mul => raw::Opcode::I32Mul,
        K::I32Neg => raw::Opcode::I32Neg,
        K::Eq => raw::Opcode::Eq,
        K::Ne => raw::Opcode::Ne,
        K::I32LtS => raw::Opcode::I32LtS,
        K::I32LeS => raw::Opcode::I32LeS,
        K::I32GtS => raw::Opcode::I32GtS,
        K::I32GeS => raw::Opcode::I32GeS,
        K::DirectCall => raw::Opcode::Call,
        K::StructConstruct | K::EnumConstruct | K::FixedArrayConstruct => raw::Opcode::Construct,
        K::CopyFromPlace => raw::Opcode::Copy,
        K::MoveFromPlace | K::GenericMoveFromPlace => raw::Opcode::Move,
        K::ClonePlace
        | K::GenericClonePlace
        | K::GenericCloneBorrow
        | K::HandleAwareClonePlace
        | K::HandleAwareCloneBorrow => raw::Opcode::Clone,
        K::StringClone => raw::Opcode::StringClone,
        K::VecClone => raw::Opcode::VecClone,
        K::InitializePlace => raw::Opcode::Initialize,
        K::ReplacePlace | K::GenericReplacePlace => raw::Opcode::Replace,
        K::DropPlace => raw::Opcode::Drop,
        K::EnumDiscriminant => raw::Opcode::Discriminant,
        K::FixedArrayIndexCopy | K::VecIndexCopy => raw::Opcode::Index,
        K::StringFromUtf8 => raw::Opcode::String,
        K::StringConcat => raw::Opcode::StringConcat,
        K::VecConstruct => raw::Opcode::VecConstruct,
        K::VecPush => raw::Opcode::VecPush,
        K::SharedConstruct => raw::Opcode::SharedConstruct,
        K::SharedClone => raw::Opcode::SharedClone,
        K::WeakDowngrade => raw::Opcode::WeakDowngrade,
        K::WeakClone => raw::Opcode::WeakClone,
        K::BeginBorrow => raw::Opcode::BeginBorrow,
        K::BeginIndexedBorrow | K::BeginIndexedAccess => raw::Opcode::BeginIndexedBorrow,
        K::ProjectIndexedBorrow => raw::Opcode::ProjectBorrow,
        K::BindIndexedBorrow => raw::Opcode::BindBorrow,
        K::BorrowReplace => raw::Opcode::BorrowReplace,
        K::BorrowRead => raw::Opcode::BorrowRead,
        K::BorrowWrite => raw::Opcode::BorrowWrite,
        K::EndBorrow => raw::Opcode::EndBorrow,
    };
    let runtime = runtime_operation(kind)
        .map(|operation| symbols.get(&operation).cloned().ok_or_else(lowering_error))
        .transpose()?;
    let backend = instruction.backend_instruction();
    let (values, places, borrows, callee, call_arguments) = lower_operands(backend.clone())?;
    let immediate = match backend {
        B::BoolLiteral(value) => raw::Immediate::Bool(value),
        B::I32Literal(value) => raw::Immediate::I32(value),
        B::String(bytes) => raw::Immediate::Utf8(bytes.to_vec()),
        B::Construct { variant: Some(variant), .. } => raw::Immediate::Variant(variant),
        _ => raw::Immediate::None,
    };
    Ok(raw::Operation {
        opcode,
        result: instruction.result().map(|id| raw::Value {
            id: id.index(),
            ty: instruction.result_type().map_or(0, zryna_layout::TypeId::index),
        }),
        values,
        places,
        borrows,
        callee,
        runtime_symbol: runtime,
        cleanup: instruction.cleanup().map(zryna_ir::data_ownership_v1::CleanupPlanIdentity::index),
        immediate,
        call_arguments,
        borrow_type: instruction
            .borrow()
            .and_then(|borrow| function.backend_borrow_type(borrow.index()))
            .map(zryna_layout::TypeId::index),
        borrow_access: instruction.borrow_access().map(|access| match access {
            VerifiedBorrowAccess::Shared => raw::BorrowAccess::Shared,
            VerifiedBorrowAccess::Exclusive => raw::BorrowAccess::Exclusive,
        }),
    })
}

fn lower_terminator(
    terminator: zryna_ir::data_ownership_v1::VerifiedTerminator<'_>,
    symbols: &BTreeMap<LogicalOperation, String>,
) -> Result<raw::Terminator, Vec<zryna_diagnostics::Diagnostic>> {
    Ok(match terminator.backend_terminator() {
        T::Return(value) => raw::Terminator::Return(value.index()),
        T::Jump(edge) => raw::Terminator::Jump(lower_edge(&edge)),
        T::Branch { condition, when_true, when_false } => raw::Terminator::Branch {
            condition: condition.index(),
            when_true: lower_edge(&when_true),
            when_false: lower_edge(&when_false),
        },
        T::EnumMatch { place, arms } => raw::Terminator::EnumMatch {
            place: place.index(),
            arms: arms.into_iter().map(|arm| (arm.variant(), lower_edge(arm.edge()))).collect(),
        },
        T::WeakUpgrade { weak, success, expired } => raw::Terminator::WeakUpgrade {
            weak: weak.index(),
            success: lower_edge(&success),
            expired: lower_edge(&expired),
            runtime_symbol: symbols
                .get(&LogicalOperation::WeakUpgrade)
                .cloned()
                .ok_or_else(lowering_error)?,
        },
        T::Trap(identity) => raw::Terminator::Trap(identity as u8),
    })
}

fn lower_edge(edge: &zryna_ir::data_ownership_v1::VerifiedEdge) -> raw::Edge {
    raw::Edge {
        target: edge.target().index(),
        arguments: edge
            .arguments()
            .map(zryna_ir::data_ownership_v1::ValueIdentity::index)
            .collect(),
    }
}
fn drop_kind(kind: VerifiedDropActionKind) -> raw::DropKind {
    match kind {
        VerifiedDropActionKind::Place => raw::DropKind::Place,
        VerifiedDropActionKind::VecInitializedPrefix => raw::DropKind::VecPrefix,
        VerifiedDropActionKind::AggregateInitializedPrefix => raw::DropKind::AggregatePrefix,
        VerifiedDropActionKind::GenericCloneInitializedPrefix => raw::DropKind::GenericPrefix,
    }
}
fn runtime_operation(kind: K) -> Option<LogicalOperation> {
    match kind {
        K::StringFromUtf8 => Some(LogicalOperation::StringFromUtf8Copy),
        K::StringClone => Some(LogicalOperation::StringClone),
        K::StringConcat => Some(LogicalOperation::StringConcat),
        K::VecConstruct => Some(LogicalOperation::VecAllocate),
        K::VecPush => Some(LogicalOperation::VecReserve),
        K::SharedClone => Some(LogicalOperation::StrongClone),
        K::WeakDowngrade => Some(LogicalOperation::WeakDowngrade),
        K::WeakClone => Some(LogicalOperation::WeakClone),
        _ => None,
    }
}
fn lowering_error() -> Vec<zryna_diagnostics::Diagnostic> {
    vec![super::error("ZRYNA-N3101", "sealed DataOwnershipV1 view could not be lowered exactly")]
}
