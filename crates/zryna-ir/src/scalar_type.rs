//! Exact original scalar vocabulary, re-exported at its established crate path.

use serde::{Deserialize, Serialize};

/// Exact scalar types represented by the initial Zryna IR.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Type {
    /// No value, reserved until every universal backend implements it.
    Unit,
    /// Boolean value, reserved until the scalar ABI enables it for every backend.
    Bool,
    /// Signed 32-bit integer.
    I32,
}
