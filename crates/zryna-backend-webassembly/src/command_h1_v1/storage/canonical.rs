use super::{
    ATTEMPTS, BlockType, CANONICAL_START, CURSOR, Function, Instruction as I, LEDGER_END,
    LEDGER_START, LIVE, MEMORY_END, ValType, emit, memory, reject_if, require_live_store,
};

pub(super) fn find() -> Function {
    let mut body = Function::new([(1, ValType::I32)]);
    require_live_store(&mut body);
    emit(
        &mut body,
        [
            I::I32Const(LEDGER_START),
            I::LocalSet(2),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
        ],
    );
    emit(&mut body, [I::LocalGet(2), I::I32Const(LEDGER_END), I::I32GeU, I::BrIf(1)]);
    emit(&mut body, [I::LocalGet(2), I::I32Load(memory(0)), I::LocalGet(0), I::I32Eq]);
    emit(&mut body, [I::LocalGet(2), I::I32Load(memory(4)), I::LocalGet(1), I::I32Eq, I::I32And]);
    emit(&mut body, [I::LocalGet(0), I::I32Eqz, I::I32Eqz, I::I32And, I::If(BlockType::Empty)]);
    emit(&mut body, [I::LocalGet(2), I::Return, I::End]);
    emit(
        &mut body,
        [I::LocalGet(2), I::I32Const(12), I::I32Add, I::LocalSet(2), I::Br(0), I::End, I::End],
    );
    emit(&mut body, [I::I32Const(0), I::End]);
    body
}

pub(super) fn allocate() -> Function {
    let mut body = Function::new([(3, ValType::I32)]);
    require_live_store(&mut body);
    emit(&mut body, [I::LocalGet(0), I::I32Eqz]);
    reject_if(&mut body, 1);
    emit(&mut body, [I::LocalGet(0), I::I32Const(4096), I::I32GtU]);
    reject_if(&mut body, 2);
    emit(
        &mut body,
        [
            I::LocalGet(1),
            I::I32Const(1),
            I::I32Ne,
            I::LocalGet(1),
            I::I32Const(4),
            I::I32Ne,
            I::I32And,
        ],
    );
    reject_if(&mut body, 1);
    emit(&mut body, [I::GlobalGet(LIVE), I::I32Const(16), I::I32GeU]);
    reject_if(&mut body, 2);
    emit(&mut body, [I::GlobalGet(ATTEMPTS), I::I32Const(4096), I::I32GeU]);
    reject_if(&mut body, 2);
    emit(&mut body, [I::GlobalGet(CURSOR), I::LocalGet(1), I::I32Const(1), I::I32Sub, I::I32Add]);
    emit(&mut body, [I::I32Const(0), I::LocalGet(1), I::I32Sub, I::I32And, I::LocalTee(3)]);
    emit(&mut body, [I::LocalGet(0), I::I32Add, I::LocalTee(4), I::LocalGet(3), I::I32LtU]);
    reject_if(&mut body, 2);
    emit(&mut body, [I::LocalGet(4), I::I32Const(MEMORY_END), I::I32GtU]);
    reject_if(&mut body, 2);
    emit(
        &mut body,
        [
            I::I32Const(LEDGER_START),
            I::LocalSet(2),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
        ],
    );
    emit(&mut body, [I::LocalGet(2), I::I32Load(memory(0)), I::I32Eqz, I::BrIf(1)]);
    emit(
        &mut body,
        [I::LocalGet(2), I::I32Const(12), I::I32Add, I::LocalSet(2), I::Br(0), I::End, I::End],
    );
    emit(&mut body, [I::LocalGet(2), I::LocalGet(3), I::I32Store(memory(0))]);
    emit(&mut body, [I::LocalGet(2), I::LocalGet(0), I::I32Store(memory(4))]);
    emit(&mut body, [I::LocalGet(2), I::LocalGet(1), I::I32Store(memory(8))]);
    emit(&mut body, [I::LocalGet(4), I::GlobalSet(CURSOR)]);
    for global in [LIVE, ATTEMPTS] {
        emit(&mut body, [I::GlobalGet(global), I::I32Const(1), I::I32Add, I::GlobalSet(global)]);
    }
    emit(&mut body, [I::LocalGet(3), I::End]);
    body
}

pub(super) fn free() -> Function {
    let mut body = Function::new([]);
    require_live_store(&mut body);
    clear_slot(&mut body, 0);
    emit(&mut body, [I::GlobalGet(LIVE), I::I32Const(1), I::I32Sub, I::GlobalSet(LIVE)]);
    emit(&mut body, [I::GlobalGet(LIVE), I::I32Eqz, I::If(BlockType::Empty)]);
    emit(&mut body, [I::I32Const(CANONICAL_START), I::GlobalSet(CURSOR), I::End, I::End]);
    body
}

pub(super) fn drain() -> Function {
    let mut body = Function::new([(1, ValType::I32)]);
    require_live_store(&mut body);
    emit(
        &mut body,
        [
            I::I32Const(LEDGER_START),
            I::LocalSet(0),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
        ],
    );
    emit(&mut body, [I::LocalGet(0), I::I32Const(LEDGER_END), I::I32GeU, I::BrIf(1)]);
    clear_slot(&mut body, 0);
    emit(
        &mut body,
        [I::LocalGet(0), I::I32Const(12), I::I32Add, I::LocalSet(0), I::Br(0), I::End, I::End],
    );
    emit(
        &mut body,
        [
            I::I32Const(0),
            I::GlobalSet(LIVE),
            I::I32Const(CANONICAL_START),
            I::GlobalSet(CURSOR),
            I::End,
        ],
    );
    body
}

fn clear_slot(body: &mut Function, local: u32) {
    for offset in [0, 4, 8] {
        emit(body, [I::LocalGet(local), I::I32Const(0), I::I32Store(memory(offset))]);
    }
}
