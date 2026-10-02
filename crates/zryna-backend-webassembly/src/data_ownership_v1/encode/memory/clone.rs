use super::{
    Context, Function, Instruction, TypeCategory, ValType, VerifiedType, WORD, address, drop_child,
    index_error, indexed, load_value, raw_indexed, store_value,
};

pub(in super::super) fn clone_helper(
    ty: VerifiedType<'_>,
    context: &Context<'_>,
) -> Result<Function, zryna_diagnostics::Diagnostic> {
    let mut body = Function::new([(5, ValType::I32)]);
    if let Some(kind) = super::super::observation::value_kind(ty.category()) {
        super::super::observation::probe(if kind < 4 { 2 } else { 4 }, context, &mut body);
        super::super::failure::helper_check(context, &mut body);
    }
    match ty.category() {
        TypeCategory::Bool | TypeCategory::I32 => {
            body.instruction(&Instruction::LocalGet(0));
        }
        TypeCategory::String => clone_bytes(4, context, &mut body),
        TypeCategory::Struct => {
            clone_fixed(ty, context, &mut body)?;
            for (index, field) in ty.fields().iter().enumerate() {
                clone_child(
                    field.ty(),
                    field.offset(),
                    true,
                    &ty.fields()[..index]
                        .iter()
                        .map(|field| (field.ty(), field.offset()))
                        .collect::<Vec<_>>(),
                    context,
                    &mut body,
                )?;
            }
            body.instruction(&Instruction::LocalGet(2));
        }
        TypeCategory::FixedArray => {
            clone_fixed(ty, context, &mut body)?;
            let child_id = ty.referenced_type().ok_or_else(index_error)?;
            let child = context.layouts.type_by_id(child_id).ok_or_else(index_error)?;
            if child.drop_kind() != 0 {
                let stride = i32::try_from(ty.array_stride().ok_or_else(index_error)?)
                    .map_err(|_| index_error())?;
                let length = i32::try_from(ty.array_length().ok_or_else(index_error)?)
                    .map_err(|_| index_error())?;
                clone_elements(child_id, child, stride, length, context, &mut body);
            }
            body.instruction(&Instruction::LocalGet(2));
        }
        TypeCategory::Enum => {
            clone_fixed(ty, context, &mut body)?;
            let payload_offset = ty.enum_payload_layout().ok_or_else(index_error)?.0;
            for variant in ty.variants() {
                let Some(payload) = variant.payload() else { continue };
                body.instruction(&Instruction::LocalGet(0));
                body.instruction(&Instruction::I32Load(WORD));
                body.instruction(&Instruction::I32Const(
                    i32::try_from(variant.ordinal()).map_err(|_| index_error())?,
                ));
                body.instruction(&Instruction::I32Eq);
                body.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
                clone_child(payload, payload_offset, false, &[], context, &mut body)?;
                body.instruction(&Instruction::End);
            }
            body.instruction(&Instruction::LocalGet(2));
        }
        TypeCategory::Vec => clone_vec(ty, context, &mut body)?,
        TypeCategory::Shared => clone_count(0, context, &mut body),
        TypeCategory::Weak => clone_count(4, context, &mut body),
    }
    body.instruction(&Instruction::End);
    Ok(body)
}

fn clone_bytes(length_offset: u64, context: &Context<'_>, body: &mut Function) {
    body.instruction(&Instruction::LocalGet(0));
    body.instruction(&Instruction::I32Const(i32::try_from(length_offset).unwrap_or(4)));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::I32Load(WORD));
    body.instruction(&Instruction::LocalSet(1));
    body.instruction(&Instruction::LocalGet(1));
    body.instruction(&Instruction::I32Const(12));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::Call(0));
    super::super::failure::helper_check(context, body);
    body.instruction(&Instruction::LocalTee(2));
    body.instruction(&Instruction::LocalGet(0));
    body.instruction(&Instruction::LocalGet(1));
    body.instruction(&Instruction::I32Const(12));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::Call(1));
    body.instruction(&Instruction::LocalGet(2));
    body.instruction(&Instruction::LocalGet(2));
    body.instruction(&Instruction::I32Const(12));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::I32Store(WORD));
    body.instruction(&Instruction::LocalGet(2));
}

fn clone_fixed(
    ty: VerifiedType<'_>,
    context: &Context<'_>,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    body.instruction(&Instruction::I32Const(
        i32::try_from(ty.size().max(1)).map_err(|_| index_error())?,
    ));
    body.instruction(&Instruction::Call(0));
    super::super::failure::helper_check(context, body);
    super::super::clone_frontier::acquired(context, body);
    body.instruction(&Instruction::LocalTee(2));
    body.instruction(&Instruction::LocalGet(0));
    body.instruction(&Instruction::I32Const(i32::try_from(ty.size()).map_err(|_| index_error())?));
    body.instruction(&Instruction::Call(1));
    Ok(())
}

fn clone_child(
    child: zryna_layout::TypeId,
    offset: u64,
    sequence: bool,
    prefix: &[(zryna_layout::TypeId, u64)],
    context: &Context<'_>,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    let ty = context.layouts.type_by_id(child).ok_or_else(index_error)?;
    if ty.drop_kind() == 0 {
        return Ok(());
    }
    address(2, offset, body)?;
    address(0, offset, body)?;
    load_value(ty, body);
    body.instruction(&Instruction::Call(context.clone_index(child)));
    body.instruction(&Instruction::GlobalGet(1));
    body.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
    super::super::clone_frontier::failed(context, body);
    body.instruction(&Instruction::LocalGet(2));
    body.instruction(&Instruction::LocalSet(0));
    if sequence {
        super::super::observation::record(0x1000_0002, context, body);
    } else if context.clone_frontier {
        super::super::observation::record(0x1000_0003, context, body);
    }
    for (ty, offset) in prefix.iter().rev() {
        drop_child(*ty, *offset, context, body)?;
    }
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::Return);
    body.instruction(&Instruction::End);
    store_value(ty, body);
    Ok(())
}

fn clone_count(offset: u64, context: &Context<'_>, body: &mut Function) {
    address(0, offset, body).expect("constant offset");
    body.instruction(&Instruction::I32Load(WORD));
    body.instruction(&Instruction::LocalTee(1));
    body.instruction(&Instruction::I32Const(-1));
    body.instruction(&Instruction::I32Eq);
    body.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
    super::super::clone_frontier::failed(context, body);
    super::super::failure::helper_trap(4, body);
    body.instruction(&Instruction::End);
    address(0, offset, body).expect("constant offset");
    body.instruction(&Instruction::LocalGet(1));
    body.instruction(&Instruction::I32Const(1));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::I32Store(WORD));
    body.instruction(&Instruction::LocalGet(0));
}

fn clone_vec(
    ty: VerifiedType<'_>,
    context: &Context<'_>,
    body: &mut Function,
) -> Result<(), zryna_diagnostics::Diagnostic> {
    let child_id = ty.referenced_type().ok_or_else(index_error)?;
    let child = context.layouts.type_by_id(child_id).ok_or_else(index_error)?;
    let stride = i32::try_from(child.size().max(1)).map_err(|_| index_error())?;
    body.instruction(&Instruction::LocalGet(0));
    body.instruction(&Instruction::I32Const(4));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::I32Load(WORD));
    body.instruction(&Instruction::LocalSet(1));
    body.instruction(&Instruction::LocalGet(1));
    body.instruction(&Instruction::I32Const(stride));
    body.instruction(&Instruction::I32Mul);
    body.instruction(&Instruction::I32Const(12));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::Call(0));
    super::super::failure::helper_check(context, body);
    super::super::clone_frontier::acquired(context, body);
    body.instruction(&Instruction::LocalTee(2));
    body.instruction(&Instruction::LocalGet(0));
    body.instruction(&Instruction::LocalGet(1));
    body.instruction(&Instruction::I32Const(stride));
    body.instruction(&Instruction::I32Mul);
    body.instruction(&Instruction::I32Const(12));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::Call(1));
    body.instruction(&Instruction::LocalGet(2));
    body.instruction(&Instruction::LocalGet(2));
    body.instruction(&Instruction::I32Const(12));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::I32Store(WORD));
    if child.drop_kind() != 0 {
        body.instruction(&Instruction::I32Const(0));
        body.instruction(&Instruction::LocalSet(3));
        body.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        body.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        body.instruction(&Instruction::LocalGet(3));
        body.instruction(&Instruction::LocalGet(1));
        body.instruction(&Instruction::I32GeU);
        body.instruction(&Instruction::BrIf(1));
        indexed(2, 3, stride, body);
        indexed(0, 3, stride, body);
        load_value(child, body);
        body.instruction(&Instruction::Call(context.clone_index(child_id)));
        cleanup_sequence(child_id, child, stride, true, context, body);
        store_value(child, body);
        body.instruction(&Instruction::LocalGet(3));
        body.instruction(&Instruction::I32Const(1));
        body.instruction(&Instruction::I32Add);
        body.instruction(&Instruction::LocalSet(3));
        body.instruction(&Instruction::Br(0));
        body.instruction(&Instruction::End);
        body.instruction(&Instruction::End);
    }
    body.instruction(&Instruction::LocalGet(2));
    Ok(())
}

fn clone_elements(
    child_id: zryna_layout::TypeId,
    child: VerifiedType<'_>,
    stride: i32,
    length: i32,
    context: &Context<'_>,
    body: &mut Function,
) {
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::LocalSet(3));
    body.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
    body.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
    body.instruction(&Instruction::LocalGet(3));
    body.instruction(&Instruction::I32Const(length));
    body.instruction(&Instruction::I32GeU);
    body.instruction(&Instruction::BrIf(1));
    raw_indexed(2, 3, stride, body);
    raw_indexed(0, 3, stride, body);
    load_value(child, body);
    body.instruction(&Instruction::Call(context.clone_index(child_id)));
    cleanup_sequence(child_id, child, stride, false, context, body);
    store_value(child, body);
    body.instruction(&Instruction::LocalGet(3));
    body.instruction(&Instruction::I32Const(1));
    body.instruction(&Instruction::I32Add);
    body.instruction(&Instruction::LocalSet(3));
    body.instruction(&Instruction::Br(0));
    body.instruction(&Instruction::End);
    body.instruction(&Instruction::End);
}

fn cleanup_sequence(
    child_id: zryna_layout::TypeId,
    child: VerifiedType<'_>,
    stride: i32,
    header: bool,
    context: &Context<'_>,
    body: &mut Function,
) {
    body.instruction(&Instruction::GlobalGet(1));
    body.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
    super::super::clone_frontier::failed(context, body);
    super::super::observation::record(0x1000_0002, context, body);
    body.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
    body.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
    body.instruction(&Instruction::LocalGet(3));
    body.instruction(&Instruction::I32Eqz);
    body.instruction(&Instruction::BrIf(1));
    body.instruction(&Instruction::LocalGet(3));
    body.instruction(&Instruction::I32Const(1));
    body.instruction(&Instruction::I32Sub);
    body.instruction(&Instruction::LocalSet(3));
    if header {
        indexed(2, 3, stride, body);
    } else {
        raw_indexed(2, 3, stride, body);
    }
    load_value(child, body);
    body.instruction(&Instruction::Call(context.drop_index(child_id)));
    body.instruction(&Instruction::Br(0));
    body.instruction(&Instruction::End);
    body.instruction(&Instruction::End);
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::Return);
    body.instruction(&Instruction::End);
}
