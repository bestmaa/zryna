use super::{
    ALLOCATE, BlockType, COPY, FIND, FREE, Function, Instruction as I, LEDGER_END, LEDGER_START,
    ValType, emit, memory, reject_if, require_live_store,
};

pub(super) fn realloc() -> Function {
    let mut body = Function::new([(2, ValType::I32)]);
    require_live_store(&mut body);
    check_alignment(&mut body, 2);
    emit(&mut body, [I::LocalGet(0), I::I32Eqz, I::If(BlockType::Empty)]);
    emit(&mut body, [I::LocalGet(1)]);
    reject_if(&mut body, 1);
    emit(
        &mut body,
        [I::LocalGet(3), I::I32Eqz, I::If(BlockType::Empty), I::I32Const(0), I::Return, I::End],
    );
    emit(&mut body, [I::LocalGet(3), I::LocalGet(2), I::Call(ALLOCATE), I::Return, I::End]);
    emit(&mut body, [I::LocalGet(0), I::LocalGet(1), I::Call(FIND), I::LocalTee(4), I::I32Eqz]);
    reject_if(&mut body, 1);
    emit(&mut body, [I::LocalGet(4), I::I32Load(memory(8)), I::LocalGet(2), I::I32Ne]);
    reject_if(&mut body, 1);
    emit(&mut body, [I::LocalGet(3), I::I32Eqz, I::If(BlockType::Empty)]);
    emit(&mut body, [I::LocalGet(4), I::Call(FREE), I::I32Const(0), I::Return, I::End]);
    emit(&mut body, [I::LocalGet(3), I::LocalGet(2), I::Call(ALLOCATE), I::LocalSet(5)]);
    emit(&mut body, [I::LocalGet(5), I::LocalGet(0), I::LocalGet(1), I::LocalGet(3), I::I32LtU]);
    emit(
        &mut body,
        [
            I::If(BlockType::Result(ValType::I32)),
            I::LocalGet(1),
            I::Else,
            I::LocalGet(3),
            I::End,
            I::Call(COPY),
        ],
    );
    emit(&mut body, [I::LocalGet(4), I::Call(FREE), I::LocalGet(5), I::End]);
    body
}

pub(super) fn validate() -> Function {
    let mut body = Function::new([(1, ValType::I32)]);
    require_live_store(&mut body);
    check_alignment(&mut body, 2);
    emit(&mut body, [I::LocalGet(0), I::I32Eqz, I::If(BlockType::Empty)]);
    emit(&mut body, [I::LocalGet(1)]);
    reject_if(&mut body, 1);
    emit(&mut body, [I::I32Const(0), I::Return, I::End]);
    emit(
        &mut body,
        [
            I::I32Const(LEDGER_START),
            I::LocalSet(3),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
        ],
    );
    emit(&mut body, [I::LocalGet(3), I::I32Const(LEDGER_END), I::I32GeU, I::BrIf(1)]);
    emit(
        &mut body,
        [I::LocalGet(3), I::I32Load(memory(0)), I::LocalGet(0), I::I32Eq, I::If(BlockType::Empty)],
    );
    emit(&mut body, [I::LocalGet(3), I::I32Load(memory(8)), I::LocalGet(2), I::I32Ne]);
    reject_if(&mut body, 1);
    emit(&mut body, [I::LocalGet(1), I::LocalGet(3), I::I32Load(memory(4)), I::I32GtU]);
    reject_if(&mut body, 1);
    emit(&mut body, [I::LocalGet(3), I::I32Load(memory(4)), I::Return, I::End]);
    emit(
        &mut body,
        [I::LocalGet(3), I::I32Const(12), I::I32Add, I::LocalSet(3), I::Br(0), I::End, I::End],
    );
    super::fatal(&mut body, 1);
    body.instruction(&I::End);
    body
}

fn check_alignment(body: &mut Function, local: u32) {
    emit(
        body,
        [
            I::LocalGet(local),
            I::I32Const(1),
            I::I32Ne,
            I::LocalGet(local),
            I::I32Const(4),
            I::I32Ne,
            I::I32And,
        ],
    );
    reject_if(body, 1);
}
