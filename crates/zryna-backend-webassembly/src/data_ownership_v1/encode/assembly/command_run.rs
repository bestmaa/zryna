use wasm_encoder::{Function, Instruction as I, ValType};

pub(super) fn wrapper(target: u32, clone_frontier: bool) -> Function {
    let mut body = Function::new([(1, ValType::I32)]);
    body.instruction(&I::Call(4));
    for global in [1, 2, 5] {
        body.instruction(&I::I32Const(0));
        body.instruction(&I::GlobalSet(global));
    }
    super::super::clone_frontier::clear(clone_frontier, &mut body);
    body.instruction(&I::I32Const(65_536));
    body.instruction(&I::GlobalSet(0));
    body.instruction(&I::Call(target));
    body.instruction(&I::I32Eqz);
    body.instruction(&I::LocalSet(0));
    super::super::clone_frontier::clear(clone_frontier, &mut body);
    body.instruction(&I::Call(4));
    body.instruction(&I::I32Const(65_536));
    body.instruction(&I::GlobalSet(0));
    // These are the shared M3 core's private observation codes, not the
    // ownership-runtime ABI's separate numeric status representation.
    // Each controlled failure has its own audited instruction address.
    for status in 1..=5 {
        body.instruction(&I::GlobalGet(1));
        body.instruction(&I::I32Const(status));
        body.instruction(&I::I32Eq);
        body.instruction(&I::If(wasm_encoder::BlockType::Empty));
        body.instruction(&I::Unreachable);
        body.instruction(&I::End);
    }
    body.instruction(&I::GlobalGet(1));
    body.instruction(&I::If(wasm_encoder::BlockType::Empty));
    body.instruction(&I::Unreachable);
    body.instruction(&I::End);
    body.instruction(&I::LocalGet(0));
    body.instruction(&I::End);
    body
}
