use super::{VALID, accepted, rejected, request};

#[test]
fn unknown_fields_at_every_record_depth_reject() {
    for candidate in [
        VALID.replacen('{', r#"{"extra":null,"#, 1),
        VALID.replace(r#""grant":{"#, r#""grant":{"extra":null,"#),
        VALID.replace(r#""input":{"#, r#""input":{"extra":null,"#),
        VALID.replace(r#""grant":{"#, r#""grant":{"grants":[],"#),
        VALID.replace(r#""input":{"#, r#""input":{"inputs":[],"#),
        request("MODE", r#"{"present":false,"extra":null}"#),
    ] {
        rejected(candidate.as_bytes(), Some("MODE"));
    }
}

#[test]
fn duplicate_root_fields_reject_equal_conflicting_and_escaped_names() {
    for extra in [
        r#""schema":"zryna.wasi-command-request.v1","#,
        r#""schema":"other","#,
        r#""\u0073chema":"zryna.wasi-command-request.v1","#,
        r#""world":"zryna:capability-profiles/command@0.1.0","#,
        r#""world":"other","#,
        r#""grant":{"capability":"environment","key":"MODE"},"#,
        r#""grant":{"capability":"environment","key":"OTHER"},"#,
        r#""input":{"present":true,"value":"on"},"#,
        r#""input":{"present":false},"#,
    ] {
        let candidate = VALID.replacen('{', &format!("{{{extra}"), 1);
        rejected(candidate.as_bytes(), Some("MODE"));
    }
}

#[test]
fn duplicate_grant_and_input_fields_reject_including_false_presence() {
    for extra in [
        r#""capability":"environment","#,
        r#""capability":"filesystem","#,
        r#""key":"MODE","#,
        r#""key":"OTHER","#,
        r#""\u006bey":"MODE","#,
    ] {
        let candidate = VALID.replace(r#""grant":{"#, &format!(r#""grant":{{{extra}"#));
        rejected(candidate.as_bytes(), Some("MODE"));
    }
    for input in [
        r#"{"present":true,"present":true,"value":"on"}"#,
        r#"{"present":true,"present":false,"value":"on"}"#,
        r#"{"present":false,"present":false}"#,
        r#"{"present":false,"present":true,"value":"on"}"#,
        r#"{"present":true,"value":"on","value":"on"}"#,
        r#"{"value":"on","value":"off","present":true}"#,
        r#"{"present":true,"\u0070resent":true,"value":"on"}"#,
        r#"{"present":true,"value":"on","\u0076alue":"on"}"#,
    ] {
        rejected(request("MODE", input).as_bytes(), Some("MODE"));
    }
}

#[test]
fn missing_required_fields_reject_at_every_record_depth() {
    for field in [
        r#""schema":"zryna.wasi-command-request.v1","#,
        r#""world":"zryna:capability-profiles/command@0.1.0","#,
        r#""grant":{"capability":"environment","key":"MODE"},"#,
        r#", "input":{"present":true,"value":"on"}"#,
        r#""capability":"environment","#,
        r#", "key":"MODE""#,
    ] {
        let field = field.replace(", ", ",");
        let candidate = VALID.replace(&field, "");
        assert!(candidate != VALID);
        rejected(candidate.as_bytes(), Some("MODE"));
    }
    for input in ["{}", r#"{"value":"on"}"#, r#"{"present":true}"#] {
        rejected(request("MODE", input).as_bytes(), Some("MODE"));
    }
}

#[test]
fn every_non_string_type_for_labels_and_key_rejects() {
    for field in [
        r#""schema":"zryna.wasi-command-request.v1""#,
        r#""world":"zryna:capability-profiles/command@0.1.0""#,
        r#""capability":"environment""#,
        r#""key":"MODE""#,
    ] {
        let (name, _) = field.split_once(':').expect("test field");
        for value in ["null", "true", "false", "0", "1.0", "[]", "{}"] {
            let candidate = VALID.replace(field, &format!("{name}:{value}"));
            rejected(candidate.as_bytes(), Some("MODE"));
        }
    }
}

#[test]
fn every_wrong_present_or_value_type_rejects() {
    for value in ["null", "0", "1", "1.0", r#""true""#, "[]", "{}"] {
        let input = format!(r#"{{"present":{value},"value":"on"}}"#);
        rejected(request("MODE", &input).as_bytes(), Some("MODE"));
    }
    for value in ["null", "true", "false", "0", "1.0", "[]", "{}"] {
        let input = format!(r#"{{"present":true,"value":{value}}}"#);
        rejected(request("MODE", &input).as_bytes(), Some("MODE"));
    }
}

#[test]
fn missing_input_forbids_any_value_field_in_both_orders() {
    for value in [r#""""#, r#""on""#, "null", "false", "0", "[]", "{}"] {
        for input in [
            format!(r#"{{"present":false,"value":{value}}}"#),
            format!(r#"{{"value":{value},"present":false}}"#),
        ] {
            rejected(request("MODE", &input).as_bytes(), Some("MODE"));
        }
    }
}

#[test]
fn arrays_and_other_non_object_records_never_replace_objects() {
    for candidate in [
        "null",
        "true",
        "0",
        r#""request""#,
        "[]",
        "{}",
        r#"["zryna.wasi-command-request.v1","zryna:capability-profiles/command@0.1.0",{"capability":"environment","key":"MODE"},{"present":true,"value":"on"}]"#,
    ] {
        rejected(candidate.as_bytes(), Some("MODE"));
    }
    for value in ["null", "false", "0", r#""record""#, "[]", r#"["environment","MODE"]"#] {
        let candidate = VALID.replace(r#"{"capability":"environment","key":"MODE"}"#, value);
        rejected(candidate.as_bytes(), Some("MODE"));
        rejected(request("MODE", value).as_bytes(), Some("MODE"));
    }
    rejected(request("MODE", r#"[true,"on"]"#).as_bytes(), Some("MODE"));
    let two = format!("[{VALID},{VALID}]");
    rejected(two.as_bytes(), Some("MODE"));
    let grant_array = VALID.replace(
        r#"{"capability":"environment","key":"MODE"}"#,
        r#"[{"capability":"environment","key":"MODE"},{"capability":"environment","key":"OTHER"}]"#,
    );
    rejected(grant_array.as_bytes(), Some("MODE"));
}

#[test]
fn fixed_identities_reject_unknown_case_version_and_unit_objects() {
    for (old, new) in [
        ("zryna.wasi-command-request.v1", "zryna.wasi-command-request.v2"),
        ("zryna.wasi-command-request.v1", "Zryna.wasi-command-request.v1"),
        ("zryna:capability-profiles/command@0.1.0", "zryna:capability-profiles/browser@0.1.0"),
        ("zryna:capability-profiles/command@0.1.0", "zryna:capability-profiles/command@0.2.0"),
        ("environment", "Environment"),
        ("environment", "filesystem"),
        ("environment", ""),
    ] {
        rejected(VALID.replace(old, new).as_bytes(), Some("MODE"));
    }
    for label in
        ["zryna.wasi-command-request.v1", "zryna:capability-profiles/command@0.1.0", "environment"]
    {
        let candidate = VALID.replace(&format!(r#""{label}""#), &format!(r#"{{"{label}":null}}"#));
        rejected(candidate.as_bytes(), Some("MODE"));
    }
}

#[test]
fn malformed_json_trailing_data_and_multiple_records_reject() {
    for candidate in [
        String::new(),
        " \t\r\n".to_owned(),
        "{".to_owned(),
        VALID.replace("true", "True"),
        VALID.replace("true", "1e999"),
        VALID.replace("true", "NaN"),
        VALID.replace(r#""MODE""#, "'MODE'"),
        format!("{VALID}\0"),
        format!("{VALID} garbage"),
        format!("{VALID}{VALID}"),
        format!("{VALID}\n{VALID}"),
        format!("{VALID} null"),
        format!("{VALID} []"),
        format!("{VALID} // comment"),
        format!("/* comment */ {VALID}"),
        format!("\u{feff}{VALID}"),
        format!("{VALID}\u{a0}"),
        VALID.replacen('{', "{,", 1),
        VALID.replace(r#""value":"on"}"#, r#""value":"on",}"#),
    ] {
        rejected(candidate.as_bytes(), Some("MODE"));
    }
}

#[test]
fn malformed_unicode_and_utf8_reject_without_parser_details() {
    for value in [
        r#""\ud800""#,
        r#""\udfff""#,
        r#""\ud800x""#,
        r#""\ud800\ud800""#,
        r#""\udfff\ud800""#,
        r#""\u000g""#,
        r#""\x00""#,
        "\"on\n\"",
        "\"on\0\"",
    ] {
        let input = format!(r#"{{"present":true,"value":{value}}}"#);
        rejected(request("MODE", &input).as_bytes(), Some("MODE"));
        rejected(VALID.replace(r#""MODE""#, value).as_bytes(), Some("MODE"));
        rejected(VALID.replace(r#""schema""#, value).as_bytes(), Some("MODE"));
    }
    for invalid in [vec![0xff], vec![0xc0, 0xaf], vec![0xed, 0xa0, 0x80], vec![0xf0, 0x9f, 0x98]] {
        let mut candidate = VALID.as_bytes().to_vec();
        let start = VALID.find("on\"").expect("test value position");
        candidate.splice(start..start + 2, invalid);
        rejected(&candidate, Some("MODE"));
    }
}

#[test]
fn all_failures_are_generic_deterministic_and_valid_input_recovers() {
    let baseline = rejected(b"", Some("MODE"));
    for candidate in [
        request("OTHER", r#"{"present":true,"value":"private-marker"}"#),
        request("MODE", r#"{"present":true,"value":null}"#),
        format!("{VALID}{VALID}"),
        "x".repeat(4097),
    ] {
        let first = rejected(candidate.as_bytes(), Some("MODE"));
        let second = rejected(candidate.as_bytes(), Some("MODE"));
        assert!(first == baseline && second == baseline);
        assert!(!first.message.contains("private-marker"));
        assert!(!first.guidance.contains("private-marker"));
        assert!(matches!(accepted(VALID.as_bytes(), "MODE").value(), Some("on")));
    }
    assert_eq!(rejected(VALID.as_bytes(), None), baseline);
    assert_eq!(rejected(VALID.as_bytes(), Some("OTHER")), baseline);
}
