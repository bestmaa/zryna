//! Protocol-only reference checks complement whole verified candidate mutations.

use super::super::{Binding, Callsite, Step, audit_calls};
use super::{Candidate, replace};
use wasm_encoder::{Encode as _, Instruction as I};
use wasmparser::{FunctionBody, Operator};

fn binding() -> Callsite {
    Callsite {
        helper: 9,
        binding: Some(Binding { module: 0x2000_0000, declaration: 3, root: 5, helper: 9 }),
    }
}

pub(super) fn has_binding(body: &FunctionBody<'_>) -> bool {
    body.get_operators_reader().expect("candidate operators").into_iter().any(|operator| {
        matches!(operator.expect("instruction"), Operator::GlobalSet { global_index: 6 })
    })
}

#[test]
fn real_source_prefix_callsites_reject_wrong_labels_missing_arm_disarm_and_second_call() {
    for name in ["clone-aggregate", "clone-vector", "clone-generic"] {
        let candidate = Candidate::new(name);
        let shape = super::super::super::declarations::Shape::derive(&candidate.program)
            .expect("whole verified declaration shape");
        let protocol = super::super::Protocol::derive(&candidate.program, &shape)
            .expect("source-derived clone protocol");
        assert!(
            protocol.calls.values().flatten().any(|call| call.binding.is_some()),
            "fixture must actually derive a sealed prefix callsite"
        );
        for global in [6, 7, 8, 9] {
            let bytes = candidate.mutate(has_binding, |body, operations| {
                let index = operations.iter().position(|operation| matches!(operation.operator, Operator::GlobalSet { global_index } if global_index == global)).expect("sealed label or arm write");
                let Operator::I32Const { value } = operations[index - 1].operator else { panic!("literal protocol label"); };
                replace(body, &operations[index - 1], &I::I32Const(value + 1));
            });
            candidate.reject(&bytes);
        }
        for removal in [6..8, 9..11] {
            let bytes = candidate.mutate(has_binding, |body, operations| {
                let start = operations
                    .iter()
                    .position(|operation| {
                        matches!(operation.operator, Operator::GlobalSet { global_index: 6 })
                    })
                    .expect("bound callsite")
                    - 1;
                body.drain(
                    operations[start + removal.start].start
                        ..operations[start + removal.end - 1].end,
                );
            });
            candidate.reject(&bytes);
        }
        let bytes = candidate.mutate(has_binding, |body, operations| {
            let start = operations
                .iter()
                .position(|operation| {
                    matches!(operation.operator, Operator::GlobalSet { global_index: 6 })
                })
                .expect("bound callsite")
                - 1;
            let Operator::Call { function_index } = operations[start + 8].operator else {
                panic!("one helper call");
            };
            let mut extra = Vec::new();
            I::Call(function_index).encode(&mut extra);
            body.splice(operations[start + 8].end..operations[start + 8].end, extra);
        });
        candidate.reject(&bytes);
    }
}

fn reference() -> Vec<Step> {
    vec![
        Step::Constant(0x2000_0000),
        Step::Set(6),
        Step::Constant(3),
        Step::Set(7),
        Step::Constant(5),
        Step::Set(8),
        Step::Constant(1),
        Step::Set(9),
        Step::Call(9),
        Step::Constant(0),
        Step::Set(9),
        Step::Get(1),
        Step::If,
        Step::Branch(1),
        Step::End,
    ]
}

#[test]
fn manual_callsite_reference_binds_exact_labels_arm_disarm_and_one_helper() {
    let original = reference();
    audit_calls(&original, &mut vec![false; original.len()], &[binding()], 8)
        .expect("manually authored exact reference");
    for (index, changed) in [
        (0, Step::Constant(0x2000_0001)),
        (2, Step::Constant(4)),
        (4, Step::Constant(6)),
        (6, Step::Constant(2)),
        (8, Step::Call(10)),
        (9, Step::Constant(1)),
        (13, Step::Branch(0)),
    ] {
        let mut mutant = original.clone();
        mutant[index] = changed;
        assert!(audit_calls(&mutant, &mut vec![false; mutant.len()], &[binding()], 8).is_err());
    }
    for index in [6..8, 9..11] {
        let mut mutant = original.clone();
        mutant.drain(index);
        assert!(audit_calls(&mutant, &mut vec![false; mutant.len()], &[binding()], 8).is_err());
    }
    let mut extra = original.clone();
    extra.extend(reference());
    assert!(audit_calls(&extra, &mut vec![false; extra.len()], &[binding()], 8).is_err());
    audit_calls(&original, &mut vec![false; original.len()], &[binding()], 8)
        .expect("manual reference recovers");
}
