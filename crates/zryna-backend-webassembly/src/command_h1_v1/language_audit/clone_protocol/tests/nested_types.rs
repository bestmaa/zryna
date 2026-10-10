use super::{Candidate, Operation, has_acquisition, replace};
use wasm_encoder::{Encode as _, Instruction as I, MemArg};
use wasmparser::{FunctionBody, Operator};
use zryna_layout::TypeCategory;

struct Indices {
    boolean: u32,
    string: u32,
    string_drop: u32,
    recorder: u32,
}

fn indices(candidate: &Candidate) -> Indices {
    let layouts = candidate.program.linear32_layouts();
    let boolean = layouts
        .types()
        .find(|ty| ty.category() == TypeCategory::Bool)
        .expect("Bool result layout")
        .id()
        .index();
    let string = layouts
        .types()
        .find(|ty| ty.category() == TypeCategory::String)
        .expect("owned String child layout")
        .id()
        .index();
    let shape =
        super::super::super::declarations::Shape::derive(&candidate.program).expect("source shape");
    Indices {
        boolean: 6 + boolean,
        string: 6 + string,
        string_drop: 6 + shape.type_count + string,
        recorder: shape.run + 2,
    }
}

fn has_call(body: &FunctionBody<'_>, callee: u32) -> bool {
    body.get_operators_reader().expect("operators").into_iter().any(|operator| matches!(operator.expect("instruction"), Operator::Call { function_index } if function_index == callee))
}

fn child_call(operations: &[Operation<'_>], callee: u32) -> usize {
    operations.iter().position(|operation| matches!(operation.operator, Operator::Call { function_index } if function_index == callee)).expect("actual child operation")
}

#[test]
fn genuine_nested_clones_reject_same_signature_wrong_type_and_clone_to_drop_substitution() {
    for name in ["clone-aggregate", "clone-vector", "clone-generic"] {
        let candidate = Candidate::new(name);
        let index = indices(&candidate);
        let bytes = candidate.mutate(
            |body| has_acquisition(body) && has_call(body, index.string),
            |body, operations| {
                let at = child_call(operations, index.string);
                replace(body, &operations[at], &I::Call(index.boolean));
            },
        );
        candidate.reject(&bytes);
        let bytes = candidate.mutate(
            |body| has_acquisition(body) && has_call(body, index.string),
            |body, operations| {
                let at = child_call(operations, index.string);
                let mut substitution = Vec::new();
                I::Call(index.string_drop).encode(&mut substitution);
                I::I32Const(0).encode(&mut substitution);
                body.splice(operations[at].start..operations[at].end, substitution);
            },
        );
        candidate.reject(&bytes);
    }
}

#[test]
fn same_child_type_cannot_use_wrong_source_destination_or_memory_load() {
    for name in ["clone-aggregate", "clone-vector", "clone-generic"] {
        let candidate = Candidate::new(name);
        let index = indices(&candidate);
        for (original, changed) in [(0, 1), (2, 0)] {
            let bytes = candidate.mutate(|body| has_acquisition(body) && has_call(body, index.string), |body, operations| {
                let call = child_call(operations, index.string);
                let at = operations[..call].iter().rposition(|operation| matches!(operation.operator, Operator::LocalGet { local_index } if local_index == original)).expect("sealed source or destination address");
                replace(body, &operations[at], &I::LocalGet(changed));
            });
            candidate.reject(&bytes);
        }
        let bytes = candidate.mutate(
            |body| has_acquisition(body) && has_call(body, index.string),
            |body, operations| {
                let call = child_call(operations, index.string);
                assert!(matches!(operations[call - 1].operator, Operator::I32Load { .. }));
                replace(
                    body,
                    &operations[call - 1],
                    &I::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }),
                );
            },
        );
        candidate.reject(&bytes);
    }
}

#[test]
fn helper_call_before_matched_suffix_cannot_bypass_exact_child_inventory() {
    let candidate = Candidate::new("clone-aggregate");
    let index = indices(&candidate);
    let bytes = candidate.mutate(
        |body| has_acquisition(body) && has_call(body, index.string),
        |body, operations| {
            let mut added = Vec::new();
            I::LocalGet(0).encode(&mut added);
            I::Call(index.string).encode(&mut added);
            I::Drop.encode(&mut added);
            body.splice(operations[0].start..operations[0].start, added);
        },
    );
    candidate.reject(&bytes);
}

#[test]
fn genuine_drop_helper_rejects_clone_substitution_and_wrong_child_address() {
    let candidate = Candidate::new("clone-aggregate");
    let index = indices(&candidate);
    let select = |body: &FunctionBody<'_>| {
        let mut reader = body.get_operators_reader().expect("drop helper");
        matches!(reader.read().expect("category record"), Operator::I32Const { value: 0x1000_0002 })
            && matches!(reader.read().expect("record call"), Operator::Call { function_index } if function_index == index.recorder)
            && has_call(body, index.string_drop)
    };
    let bytes = candidate.mutate(select, |body, operations| {
        let at = child_call(operations, index.string_drop);
        let mut substitution = Vec::new();
        I::Call(index.boolean).encode(&mut substitution);
        I::Drop.encode(&mut substitution);
        body.splice(operations[at].start..operations[at].end, substitution);
    });
    candidate.reject(&bytes);
    let bytes = candidate.mutate(select, |body, operations| {
        let call = child_call(operations, index.string_drop);
        let at = operations[..call]
            .iter()
            .rposition(|operation| {
                matches!(operation.operator, Operator::LocalGet { local_index: 0 })
            })
            .expect("exact root address");
        replace(body, &operations[at], &I::LocalGet(1));
    });
    candidate.reject(&bytes);
}
