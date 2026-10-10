use super::{VALID, accepted, present_request, rejected, request};

#[test]
fn exact_file_limit_accepts_and_first_extra_byte_rejects() {
    let mut exact = VALID.as_bytes().to_vec();
    exact.resize(4096, b' ');
    assert_eq!(exact.len(), 4096);
    assert!(accepted(&exact, "MODE") == accepted(VALID.as_bytes(), "MODE"));
    exact.push(b' ');
    assert_eq!(exact.len(), 4097);
    rejected(&exact, Some("MODE"));
}

#[test]
fn key_limit_counts_utf8_bytes_and_rejects_first_extra() {
    for key in ["x".to_owned(), "x".repeat(64), "é".repeat(32), "😀".repeat(16)] {
        let bytes = present_request(&key, "on");
        assert_eq!(accepted(bytes.as_bytes(), &key).key(), key);
    }
    for key in [
        String::new(),
        "x".repeat(65),
        format!("{}x", "é".repeat(32)),
        format!("{}x", "😀".repeat(16)),
    ] {
        rejected(present_request(&key, "on").as_bytes(), Some(&key));
        rejected(request(&key, r#"{"present":false}"#).as_bytes(), Some(&key));
    }
}

#[test]
fn present_value_limit_counts_utf8_bytes_and_rejects_first_extra() {
    for value in [String::new(), "x".repeat(1024), "é".repeat(512), "😀".repeat(256)] {
        let bytes = present_request("MODE", &value);
        let admitted = accepted(bytes.as_bytes(), "MODE");
        assert!(admitted.value().eq(&Some(value.as_str())));
        assert_eq!(admitted.value_byte_count(), value.len());
    }
    for value in
        ["x".repeat(1025), format!("{}x", "é".repeat(512)), format!("{}x", "😀".repeat(256))]
    {
        let bytes = present_request("MODE", &value);
        assert!(bytes.len() < 4096);
        rejected(bytes.as_bytes(), Some("MODE"));
    }
}

#[test]
fn escaped_spelling_has_the_same_decoded_byte_limits() {
    let key = "x".repeat(64);
    let escaped_key = format!("{}\\u0078", "x".repeat(63));
    let bytes = present_request(&key, "on").replace(&key, &escaped_key);
    assert_eq!(accepted(bytes.as_bytes(), &key).key(), key);
    let extra_key = format!("{key}\\u0078");
    let bytes = present_request(&key, "on").replace(&key, &extra_key);
    rejected(bytes.as_bytes(), Some(&format!("{key}x")));

    let value = "é".repeat(512);
    let escaped_value = format!("{}\\u00e9", "é".repeat(511));
    let bytes = present_request("MODE", &value).replace(&value, &escaped_value);
    assert_eq!(accepted(bytes.as_bytes(), "MODE").value_byte_count(), 1024);
    let bytes = present_request("MODE", &value).replace(&value, &format!("{value}\\u0078"));
    rejected(bytes.as_bytes(), Some("MODE"));
}
