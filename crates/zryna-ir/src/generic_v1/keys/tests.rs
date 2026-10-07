//! Independent byte fixtures and hostile boundaries, without a semantic key producer.

use super::{Domain, Kind, decode};
use crate::generic_v1::Failure;

fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).expect("hex"), 16).expect("byte"))
        .collect()
}

fn option(mut bytes: Vec<u8>) -> Vec<u8> {
    let mut result = vec![0x14, 1, 0, 0, 0];
    result.extend_from_slice(&u32::try_from(bytes.len()).expect("fixture length").to_le_bytes());
    result.append(&mut bytes);
    result
}

fn code(result: Result<super::DecodedKey<'_>, Failure>) -> String {
    let Failure::Diagnostics(errors) = result.expect_err("hostile key rejected") else {
        panic!("stable diagnostic expected");
    };
    assert_eq!(errors.len(), 1);
    errors[0].code().to_owned()
}

#[test]
fn independent_fixed_type_function_and_root_bytes_decode_without_identity_authority() {
    for (text, domain, kind, count, depth) in [
        ("14010000000100000001", Domain::Type, Kind::Option, 1, 1),
        ("150200000001000000010100000000", Domain::Type, Kind::Result, 2, 1),
        (
            "400000000000000000010000000100000001",
            Domain::FunctionInstance,
            Kind::FunctionInstance,
            1,
            0,
        ),
        ("410000000000000000", Domain::SourceRoot, Kind::SourceRoot, 0, 0),
        (
            "1200000000000000000100000012000000120000000000000000010000000100000001",
            Domain::Type,
            Kind::GenericStruct,
            1,
            2,
        ),
    ] {
        let bytes = hex(text);
        let decoded = decode(&bytes, domain).expect("fixed complete fixture");
        assert_eq!(decoded.kind(), kind);
        assert_eq!(decoded.arguments().len(), count);
        assert_eq!(decoded.application_depth(), depth);
        if matches!(kind, Kind::FunctionInstance | Kind::SourceRoot | Kind::GenericStruct) {
            assert_eq!(decoded.declaration(), Some((0, 0)));
        } else {
            assert_eq!(decoded.declaration(), None);
        }
    }
}

#[test]
fn every_truncation_and_suffix_of_fixed_nested_key_rejects_then_replays() {
    let bytes = hex("1200000000000000000100000012000000120000000000000000010000000100000001");
    for end in 0..bytes.len() {
        assert_eq!(code(decode(&bytes[..end], Domain::Type)), "ZRYNA-I7001");
    }
    for suffix in 0..=255u8 {
        let mut changed = bytes.clone();
        changed.push(suffix);
        assert_eq!(code(decode(&changed, Domain::Type)), "ZRYNA-I7001");
    }
    assert!(decode(&bytes, Domain::Type).is_ok());
}

#[test]
fn malformed_count_child_lengths_namespaces_and_hidden_function_keys_reject() {
    for text in [
        "14020000000100000001",
        "15010000000100000001",
        "140100000000000000",
        "1401000000ffffffff01",
        "14010000000200000001",
        "14010000000100000040",
        "140100000009000000410000000000000000",
        "12000000000000000000000000",
        "12000000000000000003000000",
        "40000000000000000003000000",
    ] {
        let bytes = hex(text);
        let domain = if bytes[0] == 0x40 { Domain::FunctionInstance } else { Domain::Type };
        assert_eq!(code(decode(&bytes, domain)), "ZRYNA-I7001");
    }
    let function = hex("400000000000000000010000000100000001");
    assert_eq!(code(decode(&function, Domain::Type)), "ZRYNA-I7001");
    assert_eq!(code(decode(&[1], Domain::SourceRoot)), "ZRYNA-I7001");
    for tag in 0..=255u8 {
        assert_eq!(decode(&[tag], Domain::Type).is_ok(), tag <= 2);
    }
}

#[test]
fn exact_type_depth_and_first_extra_are_independent_of_function_root_level() {
    let mut bytes = vec![1];
    for _ in 0..64 {
        bytes = option(bytes);
    }
    assert_eq!(decode(&bytes, Domain::Type).expect("exact depth").application_depth(), 64);
    let mut function = hex("40000000000000000001000000");
    function.extend_from_slice(&u32::try_from(bytes.len()).expect("length").to_le_bytes());
    function.extend_from_slice(&bytes);
    assert_eq!(
        decode(&function, Domain::FunctionInstance)
            .expect("function adds no application")
            .application_depth(),
        64
    );
    let extra = option(bytes);
    assert_eq!(code(decode(&extra, Domain::Type)), "ZRYNA-I3201");
}

#[test]
fn hostile_max_lanes_and_key_byte_first_extra_never_panic() {
    let mut bytes = vec![0x20];
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.push(1);
    assert_eq!(code(decode(&bytes, Domain::Type)), "ZRYNA-I7001");
    assert_eq!(code(decode(&vec![0; 4097], Domain::Type)), "ZRYNA-I3201");
    assert!(decode(&[1], Domain::Type).is_ok());
}

#[test]
fn valid_complete_type_and_function_keys_reach_exact_4096_bytes() {
    fn tree(leaves: usize) -> Vec<u8> {
        if leaves == 1 {
            return vec![1];
        }
        let left = tree(leaves / 2);
        let right = tree(leaves - leaves / 2);
        let mut result = vec![0x15, 2, 0, 0, 0];
        for child in [left, right] {
            result.extend_from_slice(&u32::try_from(child.len()).expect("length").to_le_bytes());
            result.extend(child);
        }
        result
    }
    let inner = tree(291);
    assert_eq!(inner.len(), 4061);
    let mut nominal = hex("12000000000000000001000000");
    nominal.extend_from_slice(&u32::try_from(inner.len()).expect("length").to_le_bytes());
    nominal.extend_from_slice(&inner);
    let exact = option(option(nominal));
    assert_eq!(exact.len(), 4096);
    decode(&exact, Domain::Type).expect("genuine exact complete type key");
    let mut function = hex("40000000000000000001000000");
    let argument = option(option(inner));
    function.extend_from_slice(&u32::try_from(argument.len()).expect("length").to_le_bytes());
    function.extend(argument);
    assert_eq!(function.len(), 4096);
    decode(&function, Domain::FunctionInstance).expect("exact complete function key");
    assert_eq!(code(decode(&option(exact), Domain::Type)), "ZRYNA-I3201");
}
