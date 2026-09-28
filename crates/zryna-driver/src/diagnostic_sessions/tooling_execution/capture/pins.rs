use std::fmt::Write as _;

use serde_json::Value;
use zryna_diagnostics::Diagnostic;

use super::{super::execution_error, CapturedFile};

pub(super) fn validate_graph(
    wrapper: &[u8],
    loader: &[u8],
    typescript: &[u8],
) -> Result<(), Diagnostic> {
    let wrapper: Value = serde_json::from_slice(wrapper)
        .map_err(|_| execution_error("TypeScript compatibility manifest is invalid"))?;
    let typescript: Value = serde_json::from_slice(typescript)
        .map_err(|_| execution_error("TypeScript implementation manifest is invalid"))?;
    let dependencies = wrapper.get("dependencies").and_then(Value::as_object);
    if wrapper.get("name").and_then(Value::as_str) != Some("@typescript/typescript6")
        || wrapper.get("version").and_then(Value::as_str) != Some("6.0.2")
        || wrapper.get("main").and_then(Value::as_str) != Some("./lib/typescript.js")
        || wrapper.get("exports").is_some()
        || dependencies.is_none_or(|values| {
            values.len() != 1
                || values.get("@typescript/old").and_then(Value::as_str)
                    != Some("npm:typescript@^6")
        })
        || loader != b"module.exports = require(\"@typescript/old\");\n"
        || typescript.get("name").and_then(Value::as_str) != Some("typescript")
        || typescript.get("version").and_then(Value::as_str) != Some("6.0.3")
        || typescript.get("main").and_then(Value::as_str) != Some("./lib/typescript.js")
        || typescript.get("exports").is_some()
        || typescript.get("dependencies").is_some()
    {
        return Err(execution_error("TypeScript package names or dependency mapping changed"));
    }
    Ok(())
}

pub(super) fn require_digest(
    file: &CapturedFile,
    expected: &str,
    label: &str,
) -> Result<(), Diagnostic> {
    let mut actual = String::with_capacity(64);
    for byte in file.sha256 {
        let _ = write!(actual, "{byte:02x}");
    }
    if actual == expected {
        Ok(())
    } else {
        Err(execution_error(format!("{label} does not match the pinned executable bytes")))
    }
}
