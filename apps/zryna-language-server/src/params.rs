use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;

use crate::coordinates::Position;

pub(super) fn decode_params<T: for<'de> Deserialize<'de>>(params: Option<Value>) -> Option<T> {
    serde_json::from_value(params?).ok()
}

pub(super) fn normalize_root_uri(uri: &str) -> Option<String> {
    if !uri.starts_with("file://")
        || uri.contains(['?', '#'])
        || !uri.is_ascii()
        || !valid_percent_encoding(uri.as_bytes())
    {
        return None;
    }
    let value = uri.trim_end_matches('/');
    (!value.is_empty() && value.is_ascii()).then(|| value.to_owned())
}

pub(super) fn path_below_root(root: &str, uri: &str) -> Option<String> {
    let remainder = uri.strip_prefix(root)?.strip_prefix('/')?;
    if remainder.is_empty() || uri.contains(['?', '#']) {
        return None;
    }
    percent_decode(remainder)
}

pub(super) fn root_path_from_uri(uri: &str) -> Option<PathBuf> {
    let uri = normalize_root_uri(uri)?;
    let encoded = uri.strip_prefix("file://")?;
    if !encoded.starts_with('/') {
        return None;
    }
    let decoded = percent_decode(encoded)?;
    #[cfg(windows)]
    let decoded = {
        let drive = decoded.strip_prefix('/')?;
        if !drive.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            || drive.as_bytes().get(1) != Some(&b':')
            || drive.as_bytes().get(2) != Some(&b'/')
        {
            return None;
        }
        drive.to_owned()
    };
    let path = PathBuf::from(decoded);
    path.is_absolute().then_some(path)
}

pub(super) fn same_trusted_root(configured: &std::path::Path, uri_path: &std::path::Path) -> bool {
    #[cfg(windows)]
    {
        let configured = configured.to_string_lossy().replace('\\', "/");
        let uri_path = uri_path.to_string_lossy().replace('\\', "/");
        match (configured.get(..1), uri_path.get(..1), configured.get(1..), uri_path.get(1..)) {
            (Some(left_drive), Some(right_drive), Some(left_rest), Some(right_rest)) => {
                left_drive.eq_ignore_ascii_case(right_drive) && left_rest == right_rest
            }
            _ => false,
        }
    }
    #[cfg(not(windows))]
    {
        configured == uri_path
    }
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0_usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex(*bytes.get(index + 1)?)?;
            let low = hex(*bytes.get(index + 2)?)?;
            let decoded = (high << 4) | low;
            if matches!(decoded, b'/' | b'\\') {
                return None;
            }
            output.push(decoded);
            index += 3;
        } else {
            if !bytes[index].is_ascii() {
                return None;
            }
            output.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(output).ok()?;
    (!decoded.contains(['\0', '\\'])).then_some(decoded)
}

fn valid_percent_encoding(bytes: &[u8]) -> bool {
    let mut index = 0_usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if bytes.get(index + 1).copied().and_then(hex).is_none()
                || bytes.get(index + 2).copied().and_then(hex).is_none()
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

const fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub(super) struct InitializeParams {
    pub(super) root_uri: String,
    pub(super) capabilities: ClientCapabilities,
    #[serde(default)]
    process_id: Option<Value>,
    #[serde(default)]
    client_info: Option<Value>,
    #[serde(default)]
    locale: Option<Value>,
    #[serde(default)]
    root_path: Option<Value>,
    #[serde(default)]
    pub(super) initialization_options: Option<Value>,
    #[serde(default)]
    trace: Option<Value>,
    #[serde(default)]
    workspace_folders: Option<Value>,
}

#[derive(Deserialize)]
pub(super) struct ClientCapabilities {
    #[serde(default)]
    pub(super) general: Option<GeneralCapabilities>,
    #[serde(flatten)]
    _others: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeneralCapabilities {
    #[serde(default)]
    pub(super) position_encodings: Option<Vec<String>>,
    #[serde(flatten)]
    _others: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DidOpenParams {
    pub(super) text_document: TextDocumentItem,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TextDocumentItem {
    pub(super) uri: String,
    pub(super) language_id: String,
    pub(super) version: i64,
    pub(super) text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DidChangeParams {
    pub(super) text_document: VersionedDocument,
    pub(super) content_changes: Vec<FullTextChange>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VersionedDocument {
    pub(super) uri: String,
    pub(super) version: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FullTextChange {
    pub(super) text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DidCloseParams {
    pub(super) text_document: TextDocument,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TextDocument {
    pub(super) uri: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DefinitionParams {
    pub(super) text_document: TextDocument,
    pub(super) position: Position,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CancelParams {
    pub(super) id: Value,
}
