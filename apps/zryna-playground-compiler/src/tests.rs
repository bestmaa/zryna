use super::*;

fn decode(bytes: &[u8]) -> Result<SourceRequest, &'static str> {
    read_request(&mut &*bytes)
}

#[test]
fn malformed_closed_request_rejects_before_any_compiler_is_required() {
    for bytes in [
        b"".as_slice(),
        b"null",
        b"[]",
        b"{}",
        b"\xff",
        br#"{"version":1,"revision":1,"source":"","target":"native"}"#,
        br#"{"version":1,"revision":1,"source":"","source":"other"}"#,
        br#"{"version":1,"revision":1,"source":""} {}"#,
        br#"{"version":1,"revision":1,"source":{}}"#,
        br#"{"version":1,"revision":-1,"source":""}"#,
        br#"{"version":1,"revision":1.0,"source":""}"#,
        br#"{"version":1,"revision":1e0,"source":""}"#,
    ] {
        assert!(decode(bytes).is_err(), "{bytes:?}");
    }
    assert_eq!(
        decode(br#"{"version":2,"revision":1,"source":""}"#).expect_err("request policy rejection"),
        "PLAYGROUND-REQUEST-VERSION"
    );
}

#[test]
fn structure_rejects_first_extra_before_closed_deserialization() {
    assert!(structure::validate(format!("{}0{}", "[".repeat(8), "]".repeat(8)).as_bytes()).is_ok());
    assert_eq!(
        structure::validate(format!("{}0{}", "[".repeat(9), "]".repeat(9)).as_bytes()),
        Err("PLAYGROUND-REQUEST-STRUCTURE")
    );
    assert!(structure::validate(format!("[{}0]", "0,".repeat(511)).as_bytes()).is_ok());
    assert!(structure::validate(format!("[{}0]", "0,".repeat(512)).as_bytes()).is_err());
}

#[test]
fn source_and_wire_byte_boundaries_are_independent_and_recover() {
    let request = serde_json::json!({"version": 1, "revision": 1, "source": "é".repeat(2048)});
    let mut bytes = serde_json::to_vec(&request).expect("bounded request fixture");
    assert_eq!(decode(&bytes).expect("bounded request fixture").source().len(), 4096);
    bytes.resize(MAX_REQUEST_BYTES, b' ');
    assert!(decode(&bytes).is_ok());
    bytes.push(b' ');
    assert_eq!(decode(&bytes).expect_err("request policy rejection"), "PLAYGROUND-REQUEST-LIMIT");
    let oversized = serde_json::json!({"version": 1, "revision": 1, "source": "x".repeat(4097)});
    assert_eq!(
        decode(&serde_json::to_vec(&oversized).expect("bounded request fixture"))
            .expect_err("request policy rejection"),
        "PLAYGROUND-REQUEST-LIMIT"
    );
    let recovered =
        decode(br#"{"version":1,"revision":2,"source":"a\r\n"}"#).expect("bounded request fixture");
    assert_eq!(recovered.source(), "a\r\n");
    assert_eq!(recovered.revision(), 2);
    let escaped = format!(
        "{{\"version\":1,\"revision\":9007199254740991,\"source\":\"{}\"}}",
        "\\u0000".repeat(4096)
    );
    assert!(escaped.len() < MAX_REQUEST_BYTES);
    assert_eq!(
        decode(escaped.as_bytes()).expect("bounded request fixture").source().as_bytes(),
        vec![0; 4096]
    );
    for revision in [0_u64, 9_007_199_254_740_992] {
        let request = serde_json::json!({"version": 1, "revision": revision, "source": ""});
        assert!(decode(&serde_json::to_vec(&request).expect("bounded request fixture")).is_err());
    }
}
