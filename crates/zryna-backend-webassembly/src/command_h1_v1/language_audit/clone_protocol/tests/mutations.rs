use super::{Candidate, has_acquisition, replace};
use wasm_encoder::{
    ConstExpr, ExportKind, ExportSection, GlobalSection, GlobalType, Instruction as I, ValType,
};
use wasmparser::{Operator, Parser, Payload};

#[test]
fn independently_valid_private_global_declarations_reject_nonzero_missing_and_extra() {
    let candidate = Candidate::new("clone-aggregate");
    for (count, nonzero) in
        [(4, Some(0)), (4, Some(1)), (4, Some(2)), (4, Some(3)), (3, None), (5, None)]
    {
        let mut globals = GlobalSection::new();
        for index in 0..count {
            globals.global(
                GlobalType { val_type: ValType::I32, mutable: true, shared: false },
                &ConstExpr::i32_const(i32::from(nonzero == Some(index))),
            );
        }
        let mut bytes = candidate.section(6, &globals);
        if count == 3 {
            // Keep the missing-global mutant valid by redirecting pending accesses to
            // another existing i32 global; declaration admission still rejects it first.
            let mut indices = Vec::new();
            for payload in Parser::new(0).parse_all(&bytes) {
                if let Payload::CodeSectionEntry(body) = payload.expect("mutant syntax") {
                    let mut reader = body.get_operators_reader().expect("operators");
                    while !reader.eof() {
                        let (operator, at) = reader.read_with_offset().expect("operator");
                        if matches!(
                            operator,
                            Operator::GlobalGet { global_index: 9 }
                                | Operator::GlobalSet { global_index: 9 }
                        ) {
                            indices.push(usize::try_from(at + 1).expect("bounded one-byte index"));
                        }
                    }
                }
            }
            for at in indices {
                assert_eq!(bytes[at], 9);
                bytes[at] = 8;
            }
        }
        candidate.reject(&bytes);
    }
}

#[test]
fn exporting_any_private_label_or_pending_rejects_valid_module() {
    let candidate = Candidate::new("clone-aggregate");
    for private in 6..=9 {
        let mut exports = ExportSection::new();
        for payload in Parser::new(0).parse_all(&candidate.bytes) {
            if let Payload::ExportSection(original) = payload.expect("original exports") {
                for export in original {
                    let export = export.expect("export");
                    exports.export(export.name, ExportKind::Func, export.index);
                }
            }
        }
        exports.export("private-clone-state", ExportKind::Global, private);
        candidate.reject(&candidate.section(7, &exports));
    }
}

#[test]
fn wrong_acquisition_phase_and_duplicate_consumption_reject_valid_module() {
    let candidate = Candidate::new("clone-aggregate");
    for value in [0, 2] {
        let bytes = candidate.mutate(has_acquisition, |body, operations| {
            let index = operations
                .windows(2)
                .position(|pair| {
                    matches!(pair[0].operator, Operator::GlobalGet { global_index: 9 })
                        && matches!(pair[1].operator, Operator::I32Const { value: 1 })
                })
                .expect("manual acquired guard")
                + 1;
            replace(body, &operations[index], &I::I32Const(value));
        });
        candidate.reject(&bytes);
    }
    let bytes = candidate.mutate(has_acquisition, |body, operations| {
        let start = operations
            .windows(2)
            .position(|pair| {
                matches!(pair[0].operator, Operator::GlobalGet { global_index: 9 })
                    && matches!(pair[1].operator, Operator::I32Const { value: 2 })
            })
            .expect("failure label guard");
        let first = operations[start].start;
        let last = operations[start + 12].end;
        let repeat = body[first..last].to_vec();
        body.splice(last..last, repeat);
    });
    candidate.reject(&bytes);
}

#[test]
fn acquisition_before_allocation_and_program_private_read_reject_valid_module() {
    let candidate = Candidate::new("clone-aggregate");
    let bytes = candidate.mutate(has_acquisition, |body, operations| {
        let start = operations
            .windows(2)
            .position(|pair| {
                matches!(pair[0].operator, Operator::GlobalGet { global_index: 9 })
                    && matches!(pair[1].operator, Operator::I32Const { value: 1 })
            })
            .expect("acquired guard");
        let first = operations[start].start;
        let last = operations[start + 6].end;
        let acquisition = body[first..last].to_vec();
        body.drain(first..last);
        body.splice(operations[0].start..operations[0].start, acquisition);
    });
    candidate.reject(&bytes);
    let candidate = Candidate::new("clone-aggregate");
    let bytes = candidate.mutate(super::bindings::has_binding, |body, operations| {
        let mut added = Vec::new();
        wasm_encoder::Encode::encode(&I::GlobalGet(6), &mut added);
        wasm_encoder::Encode::encode(&I::Drop, &mut added);
        body.splice(operations[0].start..operations[0].start, added);
    });
    candidate.reject(&bytes);
}
