use super::{
    BlockType, CANONICAL_START, Function, Instruction as I, LANGUAGE_START, MEMORY_END, ValType,
    emit, reject_if, require_live_store,
};

pub(super) fn allocate() -> Function {
    let mut body = Function::new([(2, ValType::I32)]);
    require_live_store(&mut body);
    emit(&mut body, [I::GlobalGet(1), I::If(BlockType::Empty), I::I32Const(0), I::Return, I::End]);
    emit(&mut body, [I::LocalGet(0), I::I32Const(i32::MAX), I::I32GtU]);
    language_failure(&mut body, 3);
    emit(
        &mut body,
        [I::GlobalGet(0), I::LocalTee(1), I::LocalGet(0), I::I32Add, I::I32Const(7), I::I32Add],
    );
    emit(&mut body, [I::I32Const(-8), I::I32And, I::LocalTee(2), I::LocalGet(1), I::I32LtU]);
    language_failure(&mut body, 3);
    emit(&mut body, [I::LocalGet(2), I::I32Const(CANONICAL_START), I::I32GtU]);
    language_failure(&mut body, 2);
    emit(&mut body, [I::LocalGet(2), I::GlobalSet(0), I::LocalGet(1), I::End]);
    body
}

fn language_failure(body: &mut Function, reason: i32) {
    emit(
        body,
        [
            I::If(BlockType::Empty),
            I::I32Const(reason),
            I::GlobalSet(1),
            I::I32Const(0),
            I::Return,
            I::End,
        ],
    );
}

pub(super) fn copy() -> Function {
    let mut body = Function::new([(1, ValType::I32)]);
    require_live_store(&mut body);
    emit(&mut body, [I::LocalGet(2), I::I32Eqz, I::If(BlockType::Empty), I::Return, I::End]);
    for pointer in [0, 1] {
        check_range(&mut body, pointer);
    }
    emit(
        &mut body,
        [I::I32Const(0), I::LocalSet(3), I::Block(BlockType::Empty), I::Loop(BlockType::Empty)],
    );
    emit(&mut body, [I::LocalGet(3), I::LocalGet(2), I::I32GeU, I::BrIf(1)]);
    emit(
        &mut body,
        [I::LocalGet(0), I::LocalGet(3), I::I32Add, I::LocalGet(1), I::LocalGet(3), I::I32Add],
    );
    let byte = wasm_encoder::MemArg { offset: 0, align: 0, memory_index: 0 };
    body.instruction(&I::I32Load8U(byte));
    body.instruction(&I::I32Store8(byte));
    emit(
        &mut body,
        [
            I::LocalGet(3),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(3),
            I::Br(0),
            I::End,
            I::End,
            I::End,
        ],
    );
    body
}

fn check_range(body: &mut Function, pointer: u32) {
    emit(body, [I::LocalGet(pointer), I::I32Const(LANGUAGE_START), I::I32LtU]);
    reject_if(body, 1);
    emit(body, [I::LocalGet(pointer), I::LocalGet(2), I::I32Add, I::LocalGet(pointer), I::I32LtU]);
    reject_if(body, 1);
    emit(
        body,
        [I::LocalGet(pointer), I::LocalGet(2), I::I32Add, I::I32Const(MEMORY_END), I::I32GtU],
    );
    reject_if(body, 1);
    emit(body, [I::LocalGet(pointer), I::I32Const(CANONICAL_START), I::I32LtU]);
    emit(
        body,
        [
            I::LocalGet(pointer),
            I::LocalGet(2),
            I::I32Add,
            I::I32Const(CANONICAL_START),
            I::I32GtU,
            I::I32And,
        ],
    );
    reject_if(body, 1);
}
