//! Closed token roles and independent token-size/boundary checks for source coverage.

use super::super::identifiers;
use serde_json::Value;

pub(super) fn validate(
    source: &str,
    range: (usize, usize),
    role: &str,
) -> Result<(), &'static str> {
    let (start, end) = range;
    let token = source.get(start..end).ok_or("invalid source token range")?;
    let word = || {
        let bytes = token.as_bytes();
        !bytes.is_empty()
            && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
            && bytes.iter().all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            && !source[..start].chars().next_back().is_some_and(identifiers::identifier_part)
            && !source[end..].chars().next().is_some_and(identifiers::identifier_part)
    };
    let valid = match role {
        "identifier" => word(),
        "bool-literal" => matches!(token, "true" | "false") && word(),
        "i32-literal" | "length_span" => {
            let digits = if role == "i32-literal" {
                token.strip_prefix('-').unwrap_or(token)
            } else {
                token
            };
            !digits.is_empty()
                && digits.bytes().all(|byte| byte.is_ascii_digit())
                && (digits == "0" || !digits.starts_with('0'))
                && token != "-0"
                && !source[..start].chars().next_back().is_some_and(identifiers::identifier_part)
                && !source[end..].chars().next().is_some_and(identifiers::identifier_part)
        }
        "string-literal" => {
            let bytes = token.as_bytes();
            bytes.len() >= 2
                && matches!(bytes[0], b'\'' | b'"')
                && bytes.last() == Some(&bytes[0])
                && !bytes[1..bytes.len() - 1]
                    .iter()
                    .any(|byte| matches!(byte, b'\\' | b'\n' | b'\r') || *byte == bytes[0])
        }
        "export_span" => token == "export" && word(),
        "function_span" => token == "function" && word(),
        "interface_span" => token == "interface" && word(),
        "extends_span" => token == "extends" && word(),
        "marker_span" => matches!(token, "ZrynaStruct" | "ZrynaEnum") && word(),
        "none_span" => token == "ZrynaNone" && word(),
        "else_span" => token == "=>" || (token == "else" && word()),
        "as_span" | "arrow_span" => token == "=>",
        "keyword_span" => {
            matches!(
                token,
                "String"
                    | "Vec"
                    | "FixedArray"
                    | "Shared"
                    | "Weak"
                    | "Borrow"
                    | "BorrowMut"
                    | "const"
                    | "let"
                    | "return"
                    | "if"
                    | "else"
                    | "while"
                    | "break"
                    | "continue"
                    | "clone"
                    | "shared"
                    | "downgrade"
                    | "borrow"
                    | "borrowMut"
                    | "push"
                    | "match"
                    | "upgradeWeak"
            ) && word()
        }
        "open_brace_span" => token == "{",
        "close_brace_span" => token == "}",
        "open_paren_span" => token == "(",
        "close_paren_span" => token == ")",
        "open_bracket_span" => token == "[",
        "close_bracket_span" => token == "]",
        "less_than_span" => token == "<",
        "greater_than_span" => token == ">",
        "comma_span" => token == ",",
        "colon_span" => token == ":",
        "dot_span" => token == ".",
        "semicolon_span" => token == ";",
        "equals_span" => token == "=" && !matches!(source.as_bytes().get(end), Some(b'=' | b'>')),
        "operator_span" => {
            matches!(token, "+" | "-" | "*" | "===" | "!==" | "<" | "<=" | ">" | ">=")
                && !(matches!(token, "+" | "-" | "*")
                    && source.as_bytes().get(end) == token.as_bytes().first())
                && !matches!(source.as_bytes().get(end), Some(b'='))
        }
        _ => false,
    };
    if valid { Ok(()) } else { Err("command coverage leaf is not an exact known source token") }
}

pub(super) fn range(value: &Value) -> Result<(usize, usize), &'static str> {
    let index = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("invalid coverage span")
    };
    Ok((index("start")?, index("end")?))
}

#[cfg(test)]
mod tests {
    #[test]
    fn coverage_leaf_validation_rejects_wide_or_partial_tokens() {
        for (source, end, role) in [
            ("\"a\"; hidden(); \"b\"", 18, "string-literal"),
            ("123abc", 3, "i32-literal"),
            ("environmentLookupSuffix", 17, "identifier"),
            ("==", 1, "equals_span"),
            ("--1", 1, "operator_span"),
            ("===", 2, "operator_span"),
            ("!==", 2, "operator_span"),
            ("====", 3, "operator_span"),
        ] {
            assert!(super::validate(source, (0, end), role).is_err());
        }
        for (source, role) in [
            ("\"é界\"", "string-literal"),
            ("-123", "i32-literal"),
            ("false", "bool-literal"),
            ("===", "operator_span"),
            ("!==", "operator_span"),
        ] {
            super::validate(source, (0, source.len()), role)
                .expect("one exact accepted source leaf");
        }
    }
}
