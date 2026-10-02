mod bounds;
mod rejection;

use zryna_diagnostics::{Diagnostic, PrimaryLocation, Severity};

use super::{CommandRequest, admit};

const VALID: &str = r#"{"schema":"zryna.wasi-command-request.v1","world":"zryna:capability-profiles/command@0.1.0","grant":{"capability":"environment","key":"MODE"},"input":{"present":true,"value":"on"}}"#;

fn request(key: &str, input: &str) -> String {
    let key = serde_json::to_string(key).expect("encode test key");
    format!(
        r#"{{"schema":"zryna.wasi-command-request.v1","world":"zryna:capability-profiles/command@0.1.0","grant":{{"capability":"environment","key":{key}}},"input":{input}}}"#
    )
}

fn present_request(key: &str, value: &str) -> String {
    let value = serde_json::to_string(value).expect("encode test value");
    request(key, &format!(r#"{{"present":true,"value":{value}}}"#))
}

fn accepted(bytes: &[u8], key: &str) -> CommandRequest {
    admit(bytes, Some(key)).expect("admit valid bounded input")
}

fn rejected(bytes: &[u8], key: Option<&str>) -> Diagnostic {
    let result = admit(bytes, key);
    assert!(result.is_err(), "malformed request was accepted");
    let diagnostic = result.err().expect("rejected input diagnostic");
    assert_eq!(diagnostic.code(), "ZRYNA-D4101");
    assert_eq!(diagnostic.severity(), Severity::Error);
    assert!(matches!(diagnostic.primary(), PrimaryLocation::Global));
    assert!(diagnostic.path().is_none());
    assert!(diagnostic.primary_span().is_none());
    diagnostic
}

#[test]
fn present_missing_and_empty_are_distinct_owned_inputs() {
    let found = accepted(VALID.as_bytes(), "MODE");
    assert_eq!(found.key(), "MODE");
    assert!(matches!(found.value(), Some("on")));
    assert_eq!(found.value_byte_count(), 2);

    let missing = accepted(request("MODE", r#"{"present":false}"#).as_bytes(), "MODE");
    let empty = accepted(present_request("MODE", "").as_bytes(), "MODE");
    assert!(missing.value().is_none());
    assert!(matches!(empty.value(), Some("")));
    assert_eq!(missing.value_byte_count(), 0);
    assert_eq!(empty.value_byte_count(), 0);
    assert!(missing != empty);
    assert!(found != empty);
}

#[test]
fn field_permutations_and_json_trivia_preserve_semantics() {
    let expected = accepted(VALID.as_bytes(), "MODE");
    for grant in [
        r#""grant":{"capability":"environment","key":"MODE"}"#,
        r#""grant":{"key":"MODE","capability":"environment"}"#,
    ] {
        for input in
            [r#""input":{"present":true,"value":"on"}"#, r#""input":{"value":"on","present":true}"#]
        {
            let fields = [
                r#""schema":"zryna.wasi-command-request.v1""#,
                r#""world":"zryna:capability-profiles/command@0.1.0""#,
                grant,
                input,
            ];
            for first in 0..4 {
                for second in 0..4 {
                    for third in 0..4 {
                        if first == second || first == third || second == third {
                            continue;
                        }
                        let fourth = (0..4)
                            .find(|index| ![first, second, third].contains(index))
                            .expect("remaining field");
                        let ordered = [first, second, third, fourth].map(|index| fields[index]);
                        let bytes = format!(" \n{{\n {} \n}}\t\r\n", ordered.join(" , \n"));
                        assert!(accepted(bytes.as_bytes(), "MODE") == expected);
                    }
                }
            }
        }
    }
}

#[test]
fn escaped_fields_and_unicode_spellings_preserve_semantics() {
    let escaped = br#"{"\u0073chema":"zryna.wasi-command-request.v1","world":"zryna:capability-profiles/command@0.1.0","grant":{"capability":"environment","\u006bey":"M\u004fDE"},"input":{"\u0070resent":true,"value":"\u006fn"}}"#;
    assert!(accepted(escaped, "MODE") == accepted(VALID.as_bytes(), "MODE"));

    let unicode = request("é😀", r#"{"present":true,"value":"é😀\u0000\n\"\\"}"#);
    let surrogate = request("é😀", r#"{"value":"\u00e9\ud83d\ude00\u0000\n\"\\","present":true}"#);
    let expected = accepted(unicode.as_bytes(), "é😀");
    assert!(expected.value().eq(&Some("é😀\0\n\"\\")));
    assert_eq!(expected.value_byte_count(), 10);
    assert!(accepted(surrogate.as_bytes(), "é😀") == expected);
}

#[test]
fn admission_retains_owned_bytes_after_the_capture_buffer_changes() {
    let mut captured = VALID.as_bytes().to_vec();
    let admitted = accepted(&captured, "MODE");
    captured.fill(b' ');
    assert!(matches!(admitted.value(), Some("on")));
    assert_eq!(admitted.key(), "MODE");
    assert!(admitted == accepted(VALID.as_bytes(), "MODE"));
}

#[test]
fn equality_binds_exact_key_presence_and_value_bytes() {
    let baseline = accepted(VALID.as_bytes(), "MODE");
    for (key, value) in [("MODE", "off"), ("OTHER", "on"), ("MODE", "ON")] {
        assert!(accepted(present_request(key, value).as_bytes(), key) != baseline);
    }
    let composed = accepted(present_request("MODE", "é").as_bytes(), "MODE");
    let decomposed = accepted(present_request("MODE", "e\u{301}").as_bytes(), "MODE");
    assert!(composed != decomposed);
}

#[test]
fn pure_stale_or_mismatched_source_requirements_reject() {
    for key in [None, Some("OTHER"), Some("mode"), Some(""), Some("MODE\0")] {
        rejected(VALID.as_bytes(), key);
    }
    let unicode = present_request("é", "on");
    rejected(unicode.as_bytes(), Some("e\u{301}"));
    let missing = request("MODE", r#"{"present":false}"#);
    rejected(missing.as_bytes(), None);
    assert!(matches!(accepted(VALID.as_bytes(), "MODE").value(), Some("on")));
}
