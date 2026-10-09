use wasm_encoder::{Encode as _, Instruction as I, MemArg};

use super::{Candidate, locate, replacement};

#[test]
fn independent_omitted_tuple_key_and_utf8_checks_reject_valid_wasm() {
    let candidate = Candidate::new("environment-match");
    for (start, end) in [
        (
            vec!["local.get 0", "i32.const 16", "i32.const 4", "call 3"],
            vec!["local.get 0", "i32.load memory=0 align=2 offset=0", "local.set 1"],
        ),
        (
            vec!["local.get 1", "i32.const 4", "i32.const 1", "call 3", "drop"],
            vec!["local.get 0", "i32.load memory=0 align=2 offset=8", "local.set 2"],
        ),
        (
            vec!["local.get 1", "i32.load8_u memory=0 align=0 offset=0", "i32.const 77", "i32.ne"],
            vec!["i32.const 0", "local.set 7", "block", "loop"],
        ),
        (
            vec!["i32.const 0", "local.set 7", "block", "loop"],
            vec!["i32.const 16", "call 0", "local.set 4"],
        ),
    ] {
        candidate.reject(&candidate.mutate(|body, operations| {
            let first = locate(operations, &start, 0);
            let last = locate(operations, &end, first + start.len());
            body.drain(operations[first].start..operations[last].start);
        }));
    }
}

#[test]
fn independent_value_limits_live_counts_and_unicode_bounds_reject_valid_wasm() {
    let candidate = Candidate::new("environment-match");
    for (pattern, offset, value) in [
        (vec!["local.get 3", "i32.const 1024", "i32.gt_u"], 1, 1025),
        (vec!["i32.const 1", "call 5", "i32.const 2", "local.get 2"], 2, 1),
        (vec!["local.get 8", "i32.const 127", "i32.le_u"], 1, 255),
        (vec!["local.get 8", "i32.const 194", "i32.ge_u"], 1, 192),
        (vec!["i32.const 160", "local.set 10"], 0, 128),
        (vec!["i32.const 159", "local.set 11"], 0, 191),
        (vec!["i32.const 144", "local.set 10"], 0, 128),
        (vec!["i32.const 143", "local.set 11"], 0, 191),
        (vec!["local.get 8", "i32.const 244", "i32.le_u"], 1, 255),
    ] {
        candidate.reject(&candidate.mutate(|body, operations| {
            let index = locate(operations, &pattern, 0) + offset;
            replacement(body, &operations[index], &I::I32Const(value));
        }));
    }
}

#[test]
fn independent_key_byte_tag_offset_and_string_representation_reject_valid_wasm() {
    let candidate = Candidate::new("environment-match");
    for (pattern, offset, instruction) in [
        (
            vec!["local.get 1", "i32.load8_u memory=0 align=0 offset=0", "i32.const 77"],
            2,
            I::I32Const(78),
        ),
        (
            vec!["local.get 4", "i32.const 1", "i32.store memory=0 align=2 offset=0"],
            1,
            I::I32Const(0),
        ),
        (
            vec!["local.get 4", "i32.const 0", "i32.store memory=0 align=2 offset=0"],
            1,
            I::I32Const(1),
        ),
        (vec!["local.get 4", "i32.const 4", "i32.add", "local.get 5"], 1, I::I32Const(8)),
        (vec!["local.get 4", "i32.const 4", "i32.add", "local.get 5"], 3, I::LocalGet(2)),
        (vec!["local.get 5", "local.get 5", "i32.const 12", "i32.add"], 2, I::I32Const(8)),
        (
            vec!["local.get 0", "i32.load memory=0 align=2 offset=8", "local.set 2"],
            1,
            I::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }),
        ),
        (
            vec!["local.get 0", "i32.load memory=0 align=2 offset=8", "local.set 2"],
            1,
            I::I32Load(MemArg { offset: 8, align: 1, memory_index: 0 }),
        ),
    ] {
        candidate.reject(&candidate.mutate(|body, operations| {
            let index = locate(operations, &pattern, 0) + offset;
            replacement(body, &operations[index], &instruction);
        }));
    }
}

#[test]
fn independent_omitted_status_checks_copy_and_drain_reject_valid_wasm() {
    let candidate = Candidate::new("environment-match");
    for (pattern, after) in [
        (vec!["global.get 1", "if", "call 4", "i32.const 0", "return", "end"], 0),
        (vec!["local.get 5", "i32.const 12", "i32.add", "local.get 2", "local.get 3", "call 1"], 0),
        (vec!["call 4", "local.get 4", "end"], 0),
    ] {
        candidate.reject(&candidate.mutate(|body, operations| {
            let start = locate(operations, &pattern, after);
            let count = if pattern[0] == "call 4" { 1 } else { pattern.len() };
            body.drain(operations[start].start..operations[start + count - 1].end);
        }));
    }
}

#[test]
fn independent_bad_branches_repeated_host_calls_and_extra_locals_reject_valid_wasm() {
    let candidate = Candidate::new("environment-match");
    candidate.reject(&candidate.mutate(|body, operations| {
        let index =
            locate(operations, &["local.get 7", "local.get 3", "i32.ge_u", "br_if 1"], 0) + 3;
        replacement(body, &operations[index], &I::BrIf(0));
    }));
    candidate.reject(&candidate.mutate(|body, operations| {
        let index = locate(operations, &["i32.const 16", "call 2"], 0) + 1;
        let mut repeated = Vec::new();
        I::I32Const(16).encode(&mut repeated);
        I::Call(2).encode(&mut repeated);
        body.splice(operations[index].end..operations[index].end, repeated);
    }));
    candidate.reject(&candidate.mutate(|body, _| {
        assert_eq!(body[..3], [1, 12, 0x7f]);
        body[1] = 13;
    }));
}

#[test]
fn independent_omitted_ownership_and_truncation_guards_reject_valid_wasm() {
    let candidate = Candidate::new("environment-match");
    for (start, end) in [
        (
            vec!["local.get 3", "i32.eqz", "local.get 2", "i32.eqz", "i32.ne"],
            vec!["local.get 2", "local.get 3", "i32.const 1", "call 3", "drop"],
        ),
        (
            vec!["local.get 0", "local.get 1", "i32.eq"],
            vec!["i32.const 1", "call 5", "i32.const 2", "local.get 2"],
        ),
        (
            vec!["i32.const 1", "call 5", "i32.const 2", "local.get 2"],
            vec!["local.get 1", "i32.load8_u memory=0 align=0 offset=0", "i32.const 77"],
        ),
        (
            vec!["local.get 7", "local.get 9", "i32.add", "local.get 3", "i32.gt_u"],
            vec!["block", "loop", "local.get 9", "i32.eqz", "br_if 1"],
        ),
    ] {
        candidate.reject(&candidate.mutate(|body, operations| {
            let first = locate(operations, &start, 0);
            let last = locate(operations, &end, first + start.len());
            body.drain(operations[first].start..operations[last].start);
        }));
    }
}

#[test]
fn independent_status_rejection_allocation_and_return_substitutions_reject_valid_wasm() {
    let candidate = Candidate::new("environment-match");
    for (pattern, offset, instruction) in [
        (vec!["global.get 1", "if", "call 4"], 0, I::GlobalGet(0)),
        (vec!["if", "i32.const 1", "i32.const 0", "i32.const 1", "call 3"], 2, I::I32Const(1)),
        (vec!["i32.const 16", "call 0", "local.set 4"], 0, I::I32Const(8)),
        (vec!["call 4", "local.get 4", "end"], 1, I::I32Const(0)),
    ] {
        candidate.reject(&candidate.mutate(|body, operations| {
            let index = locate(operations, &pattern, 0) + offset;
            replacement(body, &operations[index], &instruction);
        }));
    }
}
