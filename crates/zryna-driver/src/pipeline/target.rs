//! Closed artifact-directory identities shared by the atomic transaction.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ManifestTarget {
    JavaScript,
    WebAssembly,
    Native,
    Component,
    #[serde(rename = "wasi-command")]
    WasiCommand,
}

impl ManifestTarget {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::WebAssembly => "webassembly",
            Self::Native => "native",
            Self::Component => "component",
            Self::WasiCommand => "wasi-command",
        }
    }
}
