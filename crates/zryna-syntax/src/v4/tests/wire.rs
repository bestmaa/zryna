use super::*;
use crate::v4::decode::reject_duplicate_json_keys;

#[test]
fn golden_decodes_verifies_and_remains_source_bound() {
    let raw = raw();
    let bytes = serde_json::to_vec(&raw).unwrap();
    let decoded = decode_snapshot(&bytes).unwrap();
    let authority = sources(SOURCE);
    let verified = verify_snapshot(decoded, &authority).unwrap();
    assert_eq!(verified.schema_version(), 4);
    assert!(verified.is_bound_to(&authority));
    assert!(!verified.is_bound_to(&sources(SOURCE)));
}

#[test]
fn adapter_shorthand_fixture_decodes_and_verifies_end_to_end() {
    let source = include_str!("../../../../../tests/m3-fixtures/syntax-v4-shorthand.zry");
    let bytes = include_bytes!("../../../../../tests/m3-fixtures/syntax-v4-shorthand.json");
    let decoded = decode_snapshot(bytes).expect("adapter fixture must satisfy the closed DTO");
    let authority = sources(source);
    let verified = verify_snapshot(decoded, &authority)
        .expect("adapter shorthand value edge must satisfy Rust arena ownership");
    let fields = match &verified.files()[0].functions()[0].body.expressions[1].kind {
        RawExpressionKind::StructConstruction { fields, .. } => fields,
        other => panic!("expected struct construction, got {other:?}"),
    };
    assert!(matches!(fields[0].kind, RawFieldInitializerKind::Shorthand { value: 0, .. }));
}

#[test]
fn decoder_is_exact_closed_and_bounded() {
    let mut value = serde_json::to_value(raw()).unwrap();
    value.as_object_mut().unwrap().insert("unknown".into(), true.into());
    assert_eq!(
        decode_snapshot(&serde_json::to_vec(&value).unwrap()),
        Err(SyntaxDecodeError::InvalidSnapshot)
    );
    assert_eq!(
        decode_snapshot(br#"{"schema_version":4,"schema_version":4,"files":[],"diagnostics":[]}"#),
        Err(SyntaxDecodeError::InvalidSnapshot)
    );
    assert_eq!(
        reject_duplicate_json_keys(br#"{"outer":{"key":1,"key":2}}"#),
        Err(SyntaxDecodeError::InvalidSnapshot)
    );
    assert_eq!(
        decode_snapshot(&vec![b' '; MAX_RESPONSE_BYTES]),
        Err(SyntaxDecodeError::InvalidSnapshot)
    );
    assert!(matches!(
        decode_snapshot(&vec![b' '; MAX_RESPONSE_BYTES + 1]),
        Err(SyntaxDecodeError::ResponseTooLarge { .. })
    ));
}
