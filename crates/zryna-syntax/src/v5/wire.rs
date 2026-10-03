//! Scalar and cross-field grammar checks supplementary to bounded Serde records.

use serde_json::Value;
use zryna_source::NormalizedSourcePath;

pub(super) fn identifier(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        && bytes.iter().all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        && !matches!(text, "__proto__" | "prototype" | "constructor")
}

fn decimal(text: &str, signed: bool, limit: usize) -> bool {
    let digits = if signed { text.strip_prefix('-').unwrap_or(text) } else { text };
    !text.is_empty()
        && text.len() <= limit
        && (text == "0"
            || (!digits.is_empty()
                && digits.as_bytes()[0] != b'0'
                && digits.bytes().all(|byte| byte.is_ascii_digit())))
}

fn string_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 2
        && text.chars().count() <= 8 * 1_024 * 1_024
        && matches!(bytes[0], b'\'' | b'"')
        && bytes[bytes.len() - 1] == bytes[0]
        && !bytes[1..bytes.len() - 1]
            .iter()
            .any(|byte| matches!(*byte, b'\\' | b'\r' | b'\n') || *byte == bytes[0])
}

pub(super) fn valid(value: &Value) -> bool {
    // Wire nesting is bounded by Serde's decoder. Arena depth is a separate source check.
    match value {
        Value::Array(items) => items.iter().all(valid),
        Value::Object(object) => {
            if let Some(text) = object.get("text").and_then(Value::as_str) {
                if object.contains_key("span") && !identifier(text) {
                    return false;
                }
                if object.contains_key("token_span")
                    && (text.is_empty()
                        || text.len() > 1_024
                        || !text.is_ascii()
                        || !(text.starts_with("./") || text.starts_with("../"))
                        || !text.as_bytes().ends_with(b".zry")
                        || text.contains(['\\', '?', '#', '\0'])
                        || text.contains("//"))
                {
                    return false;
                }
            }
            if let Some(path) = object.get("path").and_then(Value::as_str)
                && NormalizedSourcePath::new(path).is_err()
            {
                return false;
            }
            for key in ["bindings", "variants", "blocks"] {
                if object.get(key).and_then(Value::as_array).is_some_and(Vec::is_empty) {
                    return false;
                }
            }
            if object.get("kind").and_then(Value::as_str) == Some("struct")
                && object.get("fields").and_then(Value::as_array).is_some_and(Vec::is_empty)
            {
                return false;
            }
            if object.contains_key("less_than_span") && object.contains_key("span") {
                for key in ["parameters", "arguments"] {
                    if object.get(key).and_then(Value::as_array).is_some_and(Vec::is_empty) {
                        return false;
                    }
                }
            }
            if object.contains_key("payload_type")
                && object.get("payload_type").is_some_and(Value::is_null)
                    == object.get("none_span").is_some_and(Value::is_null)
            {
                return false;
            }
            if let Some(spelling) = object.get("length_spelling").and_then(Value::as_str)
                && (!decimal(spelling, false, 10)
                    || object.get("length").and_then(Value::as_u64).is_none_or(|n| n > 1_048_576))
            {
                return false;
            }
            if let Some(spelling) = object.get("spelling").and_then(Value::as_str) {
                match object.get("kind").and_then(Value::as_str) {
                    Some("i32-literal") if !decimal(spelling, true, 64) => return false,
                    Some("string-literal") if !string_literal(spelling) => return false,
                    _ => {}
                }
            }
            if object.contains_key("code") {
                for (key, limit) in [("code", 1_024), ("message", 4_096), ("guidance", 4_096)] {
                    let Some(text) = object.get(key).and_then(Value::as_str) else { return false };
                    if text.is_empty() || text.chars().count() > limit {
                        return false;
                    }
                    if key == "code" && text.chars().any(char::is_control) {
                        return false;
                    }
                }
            }
            object.values().all(valid)
        }
        _ => true,
    }
}
