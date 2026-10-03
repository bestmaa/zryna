use super::*;

#[test]
fn input_exact_byte_and_revision_boundaries() {
    assert!(validate_input(1, &"x".repeat(MAX_BROWSER_SOURCE_BYTES)).is_ok());
    assert!(validate_input(MAX_REVISION, "").is_ok());
    assert!(validate_input(0, "").is_err());
    assert!(validate_input(MAX_REVISION + 1, "").is_err());
    assert!(validate_input(1, &"x".repeat(MAX_BROWSER_SOURCE_BYTES + 1)).is_err());
    assert!(validate_input(1, &"é".repeat(MAX_BROWSER_SOURCE_BYTES / 2)).is_ok());
    assert!(validate_input(1, &"é".repeat(MAX_BROWSER_SOURCE_BYTES / 2 + 1)).is_err());
}

#[test]
fn rejected_frame_contains_no_executable_payload_and_preserves_source_bytes() {
    let response = BrowserCompilation::empty(3, "é\r\n");
    let mut bytes = Vec::new();
    response.write_frame(&mut bytes).expect("bounded rejected frame");
    let length =
        u32::from_le_bytes(bytes[..4].try_into().expect("four-byte frame header")) as usize;
    assert_eq!(bytes.len(), length + 4);
    let metadata: serde_json::Value =
        serde_json::from_slice(&bytes[4..]).expect("valid frame metadata");
    assert_eq!(metadata["sourceBytes"], 4);
    assert_eq!(metadata["sourceSha256"], digest("é\r\n".as_bytes()));
    assert_eq!(metadata["revision"], 3);
    assert_eq!(metadata["artifacts"], serde_json::json!([]));
}

#[test]
fn unavailable_failure_exact_text_budget_and_first_extra() {
    assert!(CompilerFailure::new("ZRYNA-F1001", "x".repeat(4096)).is_ok());
    assert!(CompilerFailure::new("ZRYNA-F1001", "x".repeat(4097)).is_err());
    assert!(CompilerFailure::new(&"x".repeat(129), String::new()).is_err());
}
