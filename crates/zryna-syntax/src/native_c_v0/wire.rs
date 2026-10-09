use serde_json::Value;

use super::{DecodeError, MAX_STRING_BYTES, MAX_WIRE_BYTES, MAX_WIRE_DEPTH, error, require};

pub(super) fn decode(bytes: &[u8]) -> Result<Value, DecodeError> {
    require(bytes.len() <= MAX_WIRE_BYTES, "ZRYNA-C4107", "wire-bytes")?;
    let text = std::str::from_utf8(bytes).map_err(|_| error("ZRYNA-C4100", "utf8"))?;
    check_depth(bytes)?;
    let document: Value = serde_json::from_str(text).map_err(|_| error("ZRYNA-C4100", "json"))?;
    let mut encoded = String::new();
    canonical(&document, &mut encoded)?;
    encoded.push('\n');
    require(encoded.as_bytes() == bytes, "ZRYNA-C4100", "canonical-wire-or-duplicate-key")?;
    Ok(document)
}

pub(super) fn check_string_budget(document: &Value) -> Result<(), DecodeError> {
    let mut pending = vec![document];
    let mut string_bytes = 0;
    while let Some(value) = pending.pop() {
        match value {
            Value::String(value) => {
                string_bytes += value.len();
                require(string_bytes <= MAX_STRING_BYTES, "ZRYNA-C4107", "string-value-bytes")?;
            }
            Value::Array(values) => pending.extend(values),
            Value::Object(values) => pending.extend(values.values()),
            _ => {}
        }
    }
    Ok(())
}

fn check_depth(bytes: &[u8]) -> Result<(), DecodeError> {
    let mut depth: usize = 0;
    let mut quoted = false;
    let mut escaped = false;
    for byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    depth += 1;
                    require(depth <= MAX_WIRE_DEPTH, "ZRYNA-C4107", "wire-depth")?;
                }
                b'}' | b']' => {
                    depth = depth.checked_sub(1).ok_or_else(|| error("ZRYNA-C4100", "json"))?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn canonical(value: &Value, output: &mut String) -> Result<(), DecodeError> {
    match value {
        Value::Object(values) => {
            output.push('{');
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            for (index, key) in keys.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key).map_err(|_| error("ZRYNA-C4100", "json"))?,
                );
                output.push(':');
                canonical(&values[*key], output)?;
            }
            output.push('}');
        }
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                canonical(value, output)?;
            }
            output.push(']');
        }
        Value::Number(number) => {
            require(
                number.as_u64().is_some_and(|value| value <= 9_007_199_254_740_991),
                "ZRYNA-C4100",
                "integer-encoding",
            )?;
            output.push_str(&number.to_string());
        }
        _ => output
            .push_str(&serde_json::to_string(value).map_err(|_| error("ZRYNA-C4100", "json"))?),
    }
    Ok(())
}
