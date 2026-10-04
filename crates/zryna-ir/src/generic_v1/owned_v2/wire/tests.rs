//! Independent raw wire attacks cannot construct a backend authority.
use super::*;
fn claim() -> raw::Program {
    raw::Program {
        graph: crate::generic_v1::raw::Program {
            modules: vec![],
            declarations: vec![],
            type_keys: vec![],
            universe: [0; 32],
            linear32: [0; 32],
            linux_x86_64: [0; 32],
            functions: vec![],
        },
        extensions: vec![vec![raw::Extension {
            result: 0,
            operation: raw::Operation::Borrow { value: 0, exclusive: false },
        }]],
        plans: vec![],
    }
}
fn core_end(bytes: &[u8]) -> usize {
    let at = HEADER.len() + 4;
    at + 4 + u32::from_le_bytes(bytes[at..at + 4].try_into().expect("core length")) as usize
}
#[test]
fn every_truncation_unknown_domain_version_opcode_bool_and_trailing_byte_rejects() {
    let c = claim();
    let bytes = encode(&c).expect("untrusted wire");
    assert_eq!(decode(&bytes).expect("raw decoding").claims(), &c);
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end]).is_err(), "accepted truncation {end}");
    }
    let ext = core_end(&bytes);
    let opcode = ext + 4 + 4 + 4;
    for (offset, value) in [(0, b'X'), (HEADER.len(), 3), (opcode, 0), (opcode, 7), (opcode + 5, 2)]
    {
        let mut attack = bytes.clone();
        attack[offset] = value;
        assert!(decode(&attack).is_err(), "accepted attack at {offset}");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode(&trailing).is_err());
    assert!(decode(&crate::generic_v1::wire::encode(&c.graph).expect("frozen v1")).is_err());
    assert_eq!(decode(&bytes).expect("pristine replay").claims(), &c);
}
#[test]
fn hostile_length_before_allocation_and_exact_literal_byte_ceiling() {
    let mut c = claim();
    c.extensions[0][0].operation = raw::Operation::StringLiteral(vec![b'x'; 65_536]);
    let bytes = encode(&c).expect("exact separate String byte ceiling");
    assert_eq!(decode(&bytes).expect("exact ceiling").claims(), &c);
    let ext = core_end(&bytes);
    for offset in [ext, ext + 4, ext + 13] {
        let mut attack = bytes.clone();
        attack[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&attack).is_err());
    }
    let mut invalid_utf8 = bytes.clone();
    invalid_utf8[ext + 17] = 255;
    assert!(decode(&invalid_utf8).is_err());
    c.extensions[0][0].operation = raw::Operation::StringLiteral(vec![b'x'; 65_537]);
    assert!(matches!(encode(&c),Err(Failure::Diagnostics(v)) if v[0].code=="ZRYNA-I3201"));
    assert!(
        matches!(decode(&vec![0;MAX_BYTES+1]),Err(Failure::Diagnostics(v)) if v[0].code=="ZRYNA-I3201")
    );
}
#[test]
fn complete_child_credit_ceiling_is_checked_before_reservation() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    let mut reader = Reader { bytes: &bytes, position: 0, children: MAX_CHILDREN };
    assert!(
        reader.vector::<u32>(1, 4, Reader::u32).expect("zero children at exact ceiling").is_empty()
    );
    bytes[0..4].copy_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    let mut reader = Reader { bytes: &bytes, position: 0, children: MAX_CHILDREN };
    assert!(
        matches!(reader.vector(1,4,Reader::u32),Err(Failure::Diagnostics(v)) if v[0].code=="ZRYNA-I3201")
    );
}
