use super::{BYTE, BlockType, Function, I, emit, reject_if};

pub(super) fn validate(body: &mut Function) {
    emit(
        body,
        [I::I32Const(0), I::LocalSet(7), I::Block(BlockType::Empty), I::Loop(BlockType::Empty)],
    );
    emit(body, [I::LocalGet(7), I::LocalGet(3), I::I32GeU, I::BrIf(1)]);
    read_byte(body);
    advance(body);
    emit(body, [I::LocalGet(8), I::I32Const(127), I::I32LeU, I::BrIf(0)]);
    emit(body, [I::I32Const(128), I::LocalSet(10), I::I32Const(191), I::LocalSet(11)]);
    range(body, 194, 223);
    emit(body, [I::If(BlockType::Empty), I::I32Const(1), I::LocalSet(9), I::Else]);
    range(body, 224, 239);
    emit(body, [I::If(BlockType::Empty), I::I32Const(2), I::LocalSet(9)]);
    first_bound(body, 224, 160, 10);
    first_bound(body, 237, 159, 11);
    body.instruction(&I::Else);
    range(body, 240, 244);
    emit(body, [I::I32Eqz]);
    reject_if(body);
    emit(body, [I::I32Const(3), I::LocalSet(9)]);
    first_bound(body, 240, 144, 10);
    first_bound(body, 244, 143, 11);
    emit(body, [I::End, I::End]);
    emit(body, [I::LocalGet(7), I::LocalGet(9), I::I32Add, I::LocalGet(3), I::I32GtU]);
    reject_if(body);
    emit(
        body,
        [
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(9),
            I::I32Eqz,
            I::BrIf(1),
        ],
    );
    read_byte(body);
    emit(
        body,
        [
            I::LocalGet(8),
            I::LocalGet(10),
            I::I32LtU,
            I::LocalGet(8),
            I::LocalGet(11),
            I::I32GtU,
            I::I32Or,
        ],
    );
    reject_if(body);
    advance(body);
    emit(body, [I::LocalGet(9), I::I32Const(1), I::I32Sub, I::LocalSet(9)]);
    emit(
        body,
        [
            I::I32Const(128),
            I::LocalSet(10),
            I::I32Const(191),
            I::LocalSet(11),
            I::Br(0),
            I::End,
            I::End,
            I::Br(0),
            I::End,
            I::End,
        ],
    );
}

fn read_byte(body: &mut Function) {
    emit(body, [I::LocalGet(2), I::LocalGet(7), I::I32Add, I::I32Load8U(BYTE), I::LocalSet(8)]);
}

fn advance(body: &mut Function) {
    emit(body, [I::LocalGet(7), I::I32Const(1), I::I32Add, I::LocalSet(7)]);
}

fn range(body: &mut Function, low: i32, high: i32) {
    emit(
        body,
        [
            I::LocalGet(8),
            I::I32Const(low),
            I::I32GeU,
            I::LocalGet(8),
            I::I32Const(high),
            I::I32LeU,
            I::I32And,
        ],
    );
}

fn first_bound(body: &mut Function, lead: i32, bound: i32, local: u32) {
    emit(
        body,
        [
            I::LocalGet(8),
            I::I32Const(lead),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::I32Const(bound),
            I::LocalSet(local),
            I::End,
        ],
    );
}
