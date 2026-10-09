use super::index_error;
use wasm_encoder::{BlockType, Function, Instruction as I, MemArg, ValType};
use zryna_layout::VerifiedType;

mod utf8;

const WORD: MemArg = MemArg { offset: 0, align: 2, memory_index: 0 };
const BYTE: MemArg = MemArg { offset: 0, align: 0, memory_index: 0 };

pub(super) fn helper(
    key: &str,
    outcome: VerifiedType<'_>,
) -> Result<Function, zryna_diagnostics::Diagnostic> {
    let size = i32::try_from(outcome.size().max(1)).map_err(|_| index_error())?;
    let payload = outcome.enum_payload_layout().ok_or_else(index_error)?.0;
    let payload = i32::try_from(payload).map_err(|_| index_error())?;
    let mut body = Function::new([(12, ValType::I32)]);
    receive(&mut body, size);
    validate_tuple(&mut body, key)?;
    copy_found(&mut body, size, payload);
    Ok(body)
}

fn receive(body: &mut Function, size: i32) {
    emit(body, [I::I32Const(1), I::Call(5)]);
    reject_if(body);
    for offset in [16, 20] {
        emit(body, [I::I32Const(offset), I::I32Const(0), I::I32Store(WORD)]);
    }
    emit(body, [I::I32Const(16), I::Call(2)]);
    emit(body, [I::I32Const(16), I::I32Load(WORD), I::LocalSet(0)]);
    emit(body, [I::I32Const(20), I::I32Load(WORD), I::LocalTee(6), I::I32Const(1), I::I32GtU]);
    reject_if(body);
    emit(body, [I::LocalGet(6), I::I32Eqz, I::If(BlockType::Empty)]);
    emit(body, [I::LocalGet(0)]);
    reject_if(body);
    emit(body, [I::I32Const(1), I::Call(5)]);
    reject_if(body);
    allocate(body, size, 4);
    emit(
        body,
        [
            I::LocalGet(4),
            I::I32Const(1),
            I::I32Store(WORD),
            I::Call(4),
            I::LocalGet(4),
            I::Return,
            I::End,
        ],
    );
}

fn validate_tuple(body: &mut Function, key: &str) -> Result<(), zryna_diagnostics::Diagnostic> {
    emit(
        body,
        [I::LocalGet(0), I::I32Const(16), I::I32Const(4), I::Call(3), I::I32Const(16), I::I32Ne],
    );
    reject_if(body);
    load_field(body, 0, 0, 1);
    emit(
        body,
        [
            I::LocalGet(0),
            I::I32Load(MemArg { offset: 4, ..WORD }),
            I::I32Const(i32::try_from(key.len()).map_err(|_| index_error())?),
            I::I32Ne,
        ],
    );
    reject_if(body);
    emit(
        body,
        [
            I::LocalGet(1),
            I::I32Const(i32::try_from(key.len()).map_err(|_| index_error())?),
            I::I32Const(1),
            I::Call(3),
            I::Drop,
        ],
    );
    load_field(body, 0, 8, 2);
    load_field(body, 0, 12, 3);
    emit(body, [I::LocalGet(3), I::I32Const(1024), I::I32GtU]);
    reject_if(body);
    emit(body, [I::LocalGet(3), I::I32Eqz, I::LocalGet(2), I::I32Eqz, I::I32Ne]);
    reject_if(body);
    emit(body, [I::LocalGet(2), I::LocalGet(3), I::I32Const(1), I::Call(3), I::Drop]);
    for (left, right) in [(0, 1), (0, 2), (1, 2)] {
        emit(body, [I::LocalGet(left), I::LocalGet(right), I::I32Eq]);
        reject_if(body);
    }
    emit(
        body,
        [
            I::I32Const(1),
            I::Call(5),
            I::I32Const(2),
            I::LocalGet(2),
            I::I32Eqz,
            I::I32Eqz,
            I::I32Add,
            I::I32Ne,
        ],
    );
    reject_if(body);
    for (offset, byte) in key.bytes().enumerate() {
        emit(
            body,
            [
                I::LocalGet(1),
                I::I32Load8U(MemArg {
                    offset: u64::try_from(offset).map_err(|_| index_error())?,
                    ..BYTE
                }),
                I::I32Const(i32::from(byte)),
                I::I32Ne,
            ],
        );
        reject_if(body);
    }
    utf8::validate(body);
    Ok(())
}

fn copy_found(body: &mut Function, size: i32, payload: i32) {
    allocate(body, size, 4);
    emit(body, [I::LocalGet(3), I::I32Const(12), I::I32Add, I::Call(0), I::LocalSet(5)]);
    language_check(body);
    emit(body, [I::LocalGet(5), I::LocalGet(5), I::I32Const(12), I::I32Add, I::I32Store(WORD)]);
    for offset in [4, 8] {
        emit(body, [I::LocalGet(5), I::LocalGet(3), I::I32Store(MemArg { offset, ..WORD })]);
    }
    emit(
        body,
        [I::LocalGet(5), I::I32Const(12), I::I32Add, I::LocalGet(2), I::LocalGet(3), I::Call(1)],
    );
    emit(body, [I::LocalGet(4), I::I32Const(0), I::I32Store(WORD)]);
    emit(
        body,
        [I::LocalGet(4), I::I32Const(payload), I::I32Add, I::LocalGet(5), I::I32Store(WORD)],
    );
    emit(body, [I::Call(4), I::LocalGet(4), I::End]);
}

fn allocate(body: &mut Function, size: i32, local: u32) {
    emit(body, [I::I32Const(size), I::Call(0), I::LocalSet(local)]);
    language_check(body);
}

fn language_check(body: &mut Function) {
    emit(
        body,
        [I::GlobalGet(1), I::If(BlockType::Empty), I::Call(4), I::I32Const(0), I::Return, I::End],
    );
}

fn load_field(body: &mut Function, pointer: u32, offset: u64, local: u32) {
    emit(body, [I::LocalGet(pointer), I::I32Load(MemArg { offset, ..WORD }), I::LocalSet(local)]);
}

fn emit<'a>(body: &mut Function, instructions: impl IntoIterator<Item = I<'a>>) {
    for instruction in instructions {
        body.instruction(&instruction);
    }
}

fn reject_if(body: &mut Function) {
    body.instruction(&I::If(BlockType::Empty));
    emit(
        body,
        [
            I::I32Const(1),
            I::I32Const(0),
            I::I32Const(1),
            I::Call(3),
            I::Drop,
            I::Unreachable,
            I::End,
        ],
    );
}
