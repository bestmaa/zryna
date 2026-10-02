use wasm_encoder::{Function, Instruction};
use zryna_ir::data_ownership_v1::{
    VerifiedBackendInstruction as B, VerifiedCallArgument, VerifiedFunction, VerifiedInstruction,
    VerifiedInstructionKind as K, VerifiedPlaceKind,
};
use zryna_layout::{TypeCategory, VerifiedLayouts};

use super::{Context, Locals, index_error, memory};

pub(super) use super::places::{
    load, place_address, place_storage_address, place_type, place_value, store, store_place_value,
};

#[allow(clippy::match_same_arms, clippy::too_many_lines)]
pub(super) fn instruction(
    function: VerifiedFunction<'_>,
    instruction: VerifiedInstruction<'_>,
    locals: Locals,
    context: &Context<'_>,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    let kind = instruction.kind();
    let probes: &[i32] = match kind {
        K::StringFromUtf8 => &[5, 2],
        K::StringConcat | K::VecPush => &[3, 2],
        K::StructConstruct
        | K::FixedArrayConstruct
        | K::EnumConstruct
        | K::VecConstruct
        | K::SharedConstruct => &[2],
        _ => &[],
    };
    for code in probes {
        super::observation::probe(*code, context, body);
        super::failure::operation_check(body);
    }
    match (kind, instruction.backend_instruction()) {
        (K::BoolLiteral, B::BoolLiteral(value)) => {
            body.instruction(&Instruction::I32Const(i32::from(value)));
        }
        (K::I32Literal, B::I32Literal(value)) => {
            body.instruction(&Instruction::I32Const(value));
        }
        (
            K::I32Add
            | K::I32Sub
            | K::I32Mul
            | K::Eq
            | K::Ne
            | K::I32LtS
            | K::I32LeS
            | K::I32GtS
            | K::I32GeS,
            B::Binary(left, right),
        ) => {
            body.instruction(&Instruction::LocalGet(left.index()));
            body.instruction(&Instruction::LocalGet(right.index()));
            body.instruction(&match kind {
                K::I32Add => Instruction::I32Add,
                K::I32Sub => Instruction::I32Sub,
                K::I32Mul => Instruction::I32Mul,
                K::Eq => Instruction::I32Eq,
                K::Ne => Instruction::I32Ne,
                K::I32LtS => Instruction::I32LtS,
                K::I32LeS => Instruction::I32LeS,
                K::I32GtS => Instruction::I32GtS,
                _ => Instruction::I32GeS,
            });
        }
        (K::I32Neg, B::Unary(value)) => {
            body.instruction(&Instruction::I32Const(0));
            body.instruction(&Instruction::LocalGet(value.index()));
            body.instruction(&Instruction::I32Sub);
        }
        (K::DirectCall, B::DirectCall { callee, arguments }) => {
            for argument in arguments {
                match argument {
                    VerifiedCallArgument::Value(value) => {
                        body.instruction(&Instruction::LocalGet(value.index()))
                    }
                    VerifiedCallArgument::Borrow(borrow) => {
                        body.instruction(&Instruction::LocalGet(locals.borrows + borrow.index()))
                    }
                };
            }
            super::failure::operation_call(context.function_index(callee)?, body);
        }
        (
            K::StructConstruct | K::FixedArrayConstruct | K::EnumConstruct,
            B::Construct { operands, variant },
        ) => {
            let result_type = instruction.result_type().ok_or_else(index_error)?;
            let layout = context.layouts.type_by_id(result_type).ok_or_else(index_error)?;
            body.instruction(&Instruction::I32Const(
                i32::try_from(layout.size().max(1)).map_err(|_| index_error())?,
            ));
            super::failure::operation_call(0, body);
            let result = instruction.result().ok_or_else(index_error)?;
            body.instruction(&Instruction::LocalSet(result.index()));
            if layout.category() == TypeCategory::Enum {
                body.instruction(&Instruction::LocalGet(result.index()));
                body.instruction(&Instruction::I32Const(
                    i32::try_from(variant.ok_or_else(index_error)?).map_err(|_| index_error())?,
                ));
                store(body);
                if let Some(value) = operands.first() {
                    body.instruction(&Instruction::LocalGet(result.index()));
                    body.instruction(&Instruction::I32Const(
                        i32::try_from(layout.enum_payload_layout().ok_or_else(index_error)?.0)
                            .map_err(|_| index_error())?,
                    ));
                    body.instruction(&Instruction::I32Add);
                    body.instruction(&Instruction::LocalGet(value.index()));
                    let variant = layout
                        .variants()
                        .iter()
                        .find(|candidate| candidate.ordinal() == variant.unwrap_or_default())
                        .and_then(|candidate| candidate.payload())
                        .and_then(|ty| context.layouts.type_by_id(ty))
                        .ok_or_else(index_error)?;
                    memory::store_value(variant, body);
                }
                return sync_result(function, instruction, locals, context.layouts, body);
            }
            for (index, value) in operands.iter().enumerate() {
                body.instruction(&Instruction::LocalGet(result.index()));
                let offset = if layout.category() == TypeCategory::Struct {
                    layout.fields()[index].offset()
                } else {
                    layout
                        .array_stride()
                        .ok_or_else(index_error)?
                        .checked_mul(u64::try_from(index).map_err(|_| index_error())?)
                        .ok_or_else(index_error)?
                };
                body.instruction(&Instruction::I32Const(
                    i32::try_from(offset).map_err(|_| index_error())?,
                ));
                body.instruction(&Instruction::I32Add);
                body.instruction(&Instruction::LocalGet(value.index()));
                let element = if layout.category() == TypeCategory::Struct {
                    context.layouts.type_by_id(layout.fields()[index].ty())
                } else {
                    layout.referenced_type().and_then(|ty| context.layouts.type_by_id(ty))
                }
                .ok_or_else(index_error)?;
                memory::store_value(element, body);
            }
            return sync_result(function, instruction, locals, context.layouts, body);
        }
        (K::CopyFromPlace | K::MoveFromPlace | K::GenericMoveFromPlace, B::Place(place)) => {
            place_value(function, place.index(), locals, context.layouts, body)?;
        }
        (
            K::ClonePlace
            | K::GenericClonePlace
            | K::HandleAwareClonePlace
            | K::StringClone
            | K::VecClone,
            B::Place(place),
        ) => {
            place_value(function, place.index(), locals, context.layouts, body)?;
            super::clone_frontier::call(
                function,
                instruction,
                context.clone_index(instruction.result_type().ok_or_else(index_error)?),
                context.clone_frontier,
                body,
            )?;
        }
        (K::EnumDiscriminant, B::Place(place)) => {
            place_value(function, place.index(), locals, context.layouts, body)?;
            load(body);
        }
        (
            K::InitializePlace | K::ReplacePlace | K::GenericReplacePlace,
            B::PlaceValue { place, value },
        ) => {
            if kind != K::InitializePlace {
                emit_instruction_drops(function, instruction, locals, context, body)?;
            }
            let target = place_type(function, place.index(), context.layouts)?;
            place_address(function, place.index(), locals, context.layouts, body)?;
            body.instruction(&Instruction::LocalGet(value.index()));
            store_place_value(function, place.index(), target, body)?;
            return Ok(());
        }
        (K::DropPlace, B::Place(_)) => {
            emit_instruction_drops(function, instruction, locals, context, body)?;
            return Ok(());
        }
        (K::EndBorrow, B::BorrowUse(_)) => return Ok(()),
        (K::FixedArrayIndexCopy | K::VecIndexCopy, B::IndexedPlace { place, index }) => {
            super::values::indexed_value(
                function,
                place.index(),
                index.index(),
                locals,
                context.layouts,
                body,
            )?;
        }
        (K::StringFromUtf8, B::String(bytes)) => {
            super::values::string_value(bytes, locals.scratch, body)?;
        }
        (K::EnvironmentLookup, B::EnvironmentLookup(key)) => {
            let command = context
                .command
                .as_ref()
                .filter(|command| command.key == key)
                .ok_or_else(index_error)?;
            super::failure::operation_call(command.environment, body);
        }
        (K::StringConcat, B::StringConcat { left, right }) => super::values::string_concat(
            function,
            left.index(),
            right.index(),
            locals,
            context.layouts,
            body,
        )?,
        (K::VecConstruct, B::VecConstruct(values)) => super::values::vec_value(
            &values.iter().map(|value| value.index()).collect::<Vec<_>>(),
            instruction.result_type().ok_or_else(index_error)?,
            locals.scratch,
            context,
            body,
        )?,
        (K::VecPush, B::VecPush { vector, value }) => {
            super::values::vec_push(
                function,
                vector.index(),
                value.index(),
                locals,
                context,
                body,
            )?;
            return Ok(());
        }
        (K::SharedConstruct, B::Unary(value)) => super::values::shared_value(
            value.index(),
            instruction.result_type().ok_or_else(index_error)?,
            locals.scratch,
            context,
            body,
        )?,
        (K::SharedClone | K::WeakClone | K::WeakDowngrade, B::Place(place)) => {
            place_value(function, place.index(), locals, context.layouts, body)?;
            super::failure::operation_call(
                context.clone_index(instruction.result_type().ok_or_else(index_error)?),
                body,
            );
        }
        (K::BeginBorrow, B::BeginBorrow(definition)) => {
            place_storage_address(
                function,
                definition.place().index(),
                locals,
                context.layouts,
                body,
            )?;
            body.instruction(&Instruction::LocalSet(locals.borrows + definition.id().index()));
            return Ok(());
        }
        (K::BeginIndexedBorrow | K::BeginIndexedAccess, B::IndexedBorrow { definition, index }) => {
            super::values::indexed_address(
                function,
                definition.place().index(),
                index.index(),
                locals,
                context.layouts,
                body,
            )?;
            body.instruction(&Instruction::LocalSet(locals.borrows + definition.id().index()));
            return Ok(());
        }
        (K::BindIndexedBorrow, B::BindIndexedBorrow { parent, borrow }) => {
            body.instruction(&Instruction::LocalGet(locals.borrows + parent.index()));
            body.instruction(&Instruction::LocalSet(locals.borrows + borrow.index()));
            return Ok(());
        }
        (K::ProjectIndexedBorrow, B::ProjectIndexedBorrow { parent, borrow, index }) => {
            super::values::projected_address(
                function,
                parent.index(),
                index.index(),
                locals,
                context,
                body,
            )?;
            body.instruction(&Instruction::LocalSet(locals.borrows + borrow.index()));
            return Ok(());
        }
        (
            K::BorrowRead | K::GenericCloneBorrow | K::HandleAwareCloneBorrow,
            B::BorrowUse(borrow),
        ) => {
            body.instruction(&Instruction::LocalGet(locals.borrows + borrow.index()));
            let ty = instruction.result_type().ok_or_else(index_error)?;
            memory::load_value(context.layouts.type_by_id(ty).ok_or_else(index_error)?, body);
            if kind != K::BorrowRead {
                super::clone_frontier::call(
                    function,
                    instruction,
                    context.clone_index(ty),
                    context.clone_frontier,
                    body,
                )?;
            }
        }
        (K::BorrowWrite | K::BorrowReplace, B::BorrowValue { borrow, value }) => {
            let ty = function.backend_borrow_type(borrow.index()).ok_or_else(index_error)?;
            let layout = context.layouts.type_by_id(ty).ok_or_else(index_error)?;
            if kind == K::BorrowReplace && layout.drop_kind() != 0 {
                body.instruction(&Instruction::LocalGet(locals.borrows + borrow.index()));
                memory::load_value(layout, body);
                body.instruction(&Instruction::Call(context.drop_index(ty)));
            }
            body.instruction(&Instruction::LocalGet(locals.borrows + borrow.index()));
            body.instruction(&Instruction::LocalGet(value.index()));
            memory::store_value(layout, body);
            return Ok(());
        }
        _ => return Err(index_error()),
    }
    if let Some(result) = instruction.result() {
        body.instruction(&Instruction::LocalSet(result.index()));
        sync_result(function, instruction, locals, context.layouts, body)?;
    }
    Ok(())
}

fn sync_result(
    function: VerifiedFunction<'_>,
    instruction: VerifiedInstruction<'_>,
    locals: Locals,
    layouts: &VerifiedLayouts,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    let Some(result) = instruction.result() else { return Ok(()) };
    if let Some(place) =
        function.places().find(|place| place.kind() == VerifiedPlaceKind::Temporary(result))
    {
        place_address(function, place.id().index(), locals, layouts, body)?;
        body.instruction(&Instruction::LocalGet(result.index()));
        store(body);
    }
    Ok(())
}

fn emit_instruction_drops(
    function: VerifiedFunction<'_>,
    instruction: VerifiedInstruction<'_>,
    locals: Locals,
    context: &Context<'_>,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    for action in instruction.derived_drop_actions() {
        super::cleanup::action(function, &action, locals, context, body)?;
    }
    Ok(())
}

pub(super) fn emit_terminator_drops(
    function: VerifiedFunction<'_>,
    terminator: zryna_ir::data_ownership_v1::VerifiedTerminator<'_>,
    locals: Locals,
    context: &Context<'_>,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    for action in terminator.derived_drop_actions() {
        super::cleanup::action(function, &action, locals, context, body)?;
    }
    Ok(())
}
