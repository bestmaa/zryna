//! Independently packed fixed bytes and hostile decoder boundaries.

use crate::generic_v1::{
    raw,
    wire::{self, decode, encode},
};

fn fixed() -> raw::Program {
    let span = zryna_source::UntrustedSpan { file: 0, start: 0, end: 1 };
    raw::Program {
        modules: vec![raw::Module { id: 0, functions: 1 }],
        declarations: vec![raw::Declaration { module: 0, function: 0, parameters: 0, span }],
        type_keys: vec![vec![0], vec![1], vec![2]],
        universe: [11; 32],
        linear32: [22; 32],
        linux_x86_64: [33; 32],
        functions: vec![raw::Function {
            key: vec![0x41, 0, 0, 0, 0, 0, 0, 0, 0],
            span,
            public_export: None,
            parameters: vec![],
            result: raw::Type::Stored(1),
            blocks: vec![raw::Block {
                id: 0,
                parameters: vec![],
                instructions: vec![raw::Instruction {
                    result: raw::Definition { id: 0, ty: raw::Type::Stored(1) },
                    span,
                    operation: raw::Operation::I32Literal(7),
                }],
                span,
                terminator: raw::Terminator::Return(0),
            }],
        }],
    }
}

#[test]
fn separately_frozen_wire_bytes_are_not_a_matching_producer_oracle() {
    let bytes = include_bytes!("../../../../../../tests/m7-generic-copy-fixtures/wire-v1.zir");
    assert_eq!(bytes.len(), 277);
    assert_eq!(encode(&fixed()).expect("genuine fixture invariant"), bytes);
    assert_eq!(decode(bytes).expect("genuine fixture invariant").claims(), &fixed());
}

#[test]
fn every_truncated_prefix_wrong_domain_version_or_trailing_byte_rejects() {
    let bytes = encode(&fixed()).expect("genuine fixture invariant");
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end]).is_err(), "prefix {end}");
    }
    for index in [0, wire::HEADER.len()] {
        let mut claim = bytes.clone();
        claim[index] ^= 1;
        assert!(decode(&claim).is_err());
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(decode(&extra).is_err());
    assert_eq!(decode(&bytes).expect("genuine fixture invariant").claims(), &fixed());
}

#[test]
fn forged_counts_unknown_opcode_and_noncanonical_boolean_reject_before_authority() {
    let bytes = encode(&fixed()).expect("genuine fixture invariant");
    let mut count = bytes.clone();
    count[wire::HEADER.len() + 4..wire::HEADER.len() + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode(&count).is_err());
    // Independent frozen-vector offsets: optional-export flag 208; literal tag 255.
    let mut opcode = bytes.clone();
    opcode[255] = 0xff;
    assert!(decode(&opcode).is_err());
    let mut boolean = bytes.clone();
    boolean[208] = 2;
    assert!(decode(&boolean).is_err());
}

#[test]
fn wire_byte_budget_first_extra_is_rejected_without_reserving_claims() {
    let extra = vec![0; wire::MAX_BYTES + 1];
    assert!(matches!(decode(&extra), Err(crate::generic_v1::Failure::Diagnostics(_))));
}

#[test]
fn synthetic_wire_byte_ceiling_is_exact_and_first_extra_is_atomic() {
    let mut raw = fixed();
    raw.modules.clear();
    raw.declarations.clear();
    raw.functions.clear();
    // 136 fixed bytes plus 65,536 length-prefixed blobs, counted independently of the encoder.
    raw.type_keys =
        (0..65536).map(|index| vec![0; if index < 65400 { 508 } else { 507 }]).collect();
    let bytes = encode(&raw).expect("exact independently counted 32 MiB transport claim");
    assert_eq!(bytes.len(), wire::MAX_BYTES);
    assert_eq!(
        decode(&bytes).expect("exact raw transport boundary").claims().type_keys.len(),
        65536
    );
    raw.type_keys[0].push(0);
    assert!(encode(&raw).is_err());
    let mut extra = bytes;
    extra.push(0);
    assert!(decode(&extra).is_err());
}

#[test]
fn synthetic_vector_children_are_exact_then_first_extra_rejects() {
    let mut raw = fixed();
    raw.functions[0].parameters = vec![raw::Type::Stored(1); 9];
    raw.functions[0].blocks[0].instructions = (0..4080)
        .map(|id| raw::Instruction {
            result: raw::Definition { id, ty: raw::Type::Stored(1) },
            span: raw.functions[0].span,
            operation: raw::Operation::ClosedGenericCall { instance: 0, arguments: vec![0; 256] },
        })
        .collect();
    // 1 module + 1 declaration + 3 types + 1 function + 9 parameters + 1 block +
    // 4,080 instructions + 4,080 * 256 argument IDs = exactly 1,048,576 children.
    let bytes = encode(&raw).expect("exact independently counted vector children");
    assert_eq!(decode(&bytes).expect("exact raw child transport boundary").claims(), &raw);
    raw.functions[0].parameters.push(raw::Type::Stored(1));
    assert!(encode(&raw).is_err());
    // Change the parameter count and insert its five-byte record independently in the wire.
    let mut extra = bytes;
    extra[209..213].copy_from_slice(&10u32.to_le_bytes());
    extra.splice(213..213, [0, 1, 0, 0, 0]);
    assert!(decode(&extra).is_err());
}
