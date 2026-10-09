use super::{encode, reject_valid, replace_section};
use wasm_encoder::{
    CodeSection, ConstExpr, Encode, GlobalSection, GlobalType, Instruction as I, ValType,
};
use wasmparser::{Operator, Parser, Payload};

#[test]
fn command_storage_independent_partition_quota_and_ledger_operand_mutations_are_rejected() {
    let bytes = encode();
    for (role, old, occurrence, replacement) in [
        (0, 15_728_640, 0, 16_777_216),
        (6, 4096, 0, 4097),
        (6, 4096, 1, 4097),
        (6, 16, 0, 17),
        (5, 4096, 0, 4092),
        (5, 4288, 0, 4300),
        (5, 12, 0, 16),
        (4, 4096, 0, 4092),
        (4, 4288, 0, 4300),
        (4, 12, 0, 16),
        (6, 16_777_216, 0, 16_777_220),
    ] {
        reject_valid(&mutate(
            &bytes,
            role,
            |op| matches!(op, Operator::I32Const { value } if *value == old),
            occurrence,
            &[I::I32Const(replacement)],
        ));
    }
    let mut globals = GlobalSection::new();
    for value in [65_536, 0, 0, 0, 0, 0, 15_728_636, 0, 0, 0] {
        globals.global(
            GlobalType { val_type: ValType::I32, mutable: true, shared: false },
            &ConstExpr::i32_const(value),
        );
    }
    reject_valid(&replace_section(&bytes, &globals));
}

#[test]
fn command_storage_independent_old_length_alignment_fatal_and_copy_mutations_are_rejected() {
    let bytes = encode();
    reject_valid(&mutate(&bytes, 5, |op| matches!(op, Operator::I32Eq), 1, &[I::I32Ne]));
    reject_valid(&mutate(&bytes, 2, |op| matches!(op, Operator::I32Ne), 2, &[I::I32Eq]));
    reject_valid(&mutate(&bytes, 2, |op| matches!(op, Operator::I32LtU), 0, &[I::I32GtU]));
    reject_valid(&mutate(&bytes, 6, |op| matches!(op, Operator::Unreachable), 1, &[I::Nop]));
    reject_valid(&mutate(
        &bytes,
        5,
        |op| matches!(op, Operator::BrIf { relative_depth: 1 }),
        0,
        &[I::BrIf(0)],
    ));
    reject_valid(&mutate(
        &bytes,
        2,
        |op| matches!(op, Operator::Call { function_index: 1 }),
        0,
        &[I::Drop, I::Drop, I::Drop],
    ));
    reject_valid(&mutate(
        &bytes,
        2,
        |op| matches!(op, Operator::Call { function_index: 7 }),
        1,
        &[I::Drop],
    ));
    reject_valid(&mutate(
        &bytes,
        7,
        |op| matches!(op, Operator::GlobalSet { global_index: 6 }),
        0,
        &[I::GlobalSet(7)],
    ));
}

fn mutate(
    bytes: &[u8],
    role: usize,
    matches: impl Fn(&Operator<'_>) -> bool,
    occurrence: usize,
    replacement: &[I<'_>],
) -> Vec<u8> {
    let mut code = CodeSection::new();
    let mut current = 0;
    let mut changed = false;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("source core") {
            let mut raw = body.as_bytes().to_vec();
            if current == role {
                let base = body.range().start;
                let mut operators = body.get_operators_reader().expect("source operators");
                let mut seen = 0;
                while !operators.eof() {
                    let (operator, start) = operators.read_with_offset().expect("source operator");
                    if matches(&operator) {
                        if seen == occurrence {
                            let end = operators.original_position();
                            let mut encoded = Vec::new();
                            for instruction in replacement {
                                instruction.encode(&mut encoded);
                            }
                            let start =
                                usize::try_from(start - base).expect("bounded helper offset");
                            let end = usize::try_from(end - base).expect("bounded helper end");
                            raw.splice(start..end, encoded);
                            changed = true;
                            break;
                        }
                        seen += 1;
                    }
                }
            }
            code.raw(&raw);
            current += 1;
        }
    }
    assert!(changed, "independent mutation must reach its declared helper operand");
    replace_section(bytes, &code)
}
