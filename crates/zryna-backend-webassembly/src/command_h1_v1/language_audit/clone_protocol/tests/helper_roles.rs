use super::{Candidate, has_acquisition, replace};
use wasm_encoder::{Encode as _, Instruction as I};
use wasmparser::Operator;

#[test]
fn failure_consumption_requires_acquired_guard_and_cannot_be_omitted() {
    let candidate = Candidate::new("clone-aggregate");
    for value in [0, 1] {
        let bytes = candidate.mutate(has_acquisition, |body, operations| {
            let start = operations
                .windows(2)
                .position(|pair| {
                    matches!(pair[0].operator, Operator::GlobalGet { global_index: 9 })
                        && matches!(pair[1].operator, Operator::I32Const { value: 2 })
                })
                .expect("manually selected failure consumption");
            replace(body, &operations[start + 1], &I::I32Const(value));
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
            .expect("failure consumption is a complete zero-stack block");
        body.drain(operations[start].start..operations[start + 12].end);
    });
    candidate.reject(&bytes);
}

#[test]
fn helper_cannot_reenter_program_or_run_even_with_valid_stack_types() {
    let candidate = Candidate::new("clone-aggregate");
    let shape = super::super::super::declarations::Shape::derive(&candidate.program)
        .expect("sealed entry and wrapper shape");
    for target in [shape.main, shape.run] {
        let bytes = candidate.mutate(has_acquisition, |body, operations| {
            let mut added = Vec::new();
            I::Call(target).encode(&mut added);
            I::Drop.encode(&mut added);
            body.splice(operations[0].start..operations[0].start, added);
        });
        candidate.reject(&bytes);
    }
}

#[test]
fn acquisition_requires_successful_allocation_and_immediate_destination_retention() {
    let candidate = Candidate::new("clone-aggregate");
    let bytes = candidate.mutate(has_acquisition, |body, operations| {
        let start = operations
            .windows(2)
            .position(|pair| {
                matches!(pair[0].operator, Operator::GlobalGet { global_index: 9 })
                    && matches!(pair[1].operator, Operator::I32Const { value: 1 })
            })
            .expect("manual acquisition block");
        let mut added = Vec::new();
        I::Nop.encode(&mut added);
        body.splice(operations[start + 6].end..operations[start + 6].end, added);
    });
    candidate.reject(&bytes);
}
