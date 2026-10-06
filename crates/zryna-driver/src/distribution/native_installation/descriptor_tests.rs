//! Independent parser cases; a mock Binding grants no running-image admission authority.

use serde_json::{Value, json};

use super::{Descriptor, cli_path};
use crate::distribution::native_installation::identity::Binding;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const TREE: &str = "89abcdef0123456789abcdef0123456789abcdef";

fn binding() -> Binding {
    Binding { descriptor_sha256: "0", commit: COMMIT, tree: TREE }
}

fn positive() -> Value {
    let target = if cfg!(windows) { "x86_64-pc-windows-msvc" } else { "x86_64-unknown-linux-gnu" };
    let value = json!({
        "format": "zryna.native-installation-internal.v1", "version": env!("CARGO_PKG_VERSION"),
        "source": { "repository": "https://github.com/zryna/zryna", "commit": COMMIT, "tree": TREE },
        "target": target, "cli": cli_path(), "protocols": [2, 3, 4],
        "license_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    });
    assert!(
        Descriptor::parse(&bytes(&value), &binding()).is_ok(),
        "valid parser baseline before mutation"
    );
    value
}

fn bytes(value: &Value) -> Vec<u8> {
    let mut result = serde_json::to_vec(value).expect("independent canonical Value fixture");
    result.push(b'\n');
    result
}

fn rejected(value: Value) {
    assert!(Descriptor::parse(&bytes(&value), &binding()).is_err());
}

fn noncanonical_rejected(raw: &[u8]) {
    assert_eq!(serde_json::from_slice::<Value>(raw).expect("same valid JSON value"), positive());
    assert!(Descriptor::parse(raw, &binding()).is_err());
}

#[test]
fn independent_canonical_current_host_positive() {
    assert!(Descriptor::parse(&bytes(&positive()), &binding()).is_ok());
}

#[test]
fn independent_duplicate_key_rejected() {
    let mut raw = bytes(&positive());
    raw.splice(1..1, b"\"protocols\":[2,3,4],".iter().copied());
    noncanonical_rejected(&raw);
}

#[test]
fn independent_unknown_top_field_rejected() {
    let mut value = positive();
    value["runtime"] = json!("node");
    rejected(value);
}

#[test]
fn independent_unknown_source_field_rejected() {
    let mut value = positive();
    value["source"]["authenticated"] = json!(true);
    rejected(value);
}

#[test]
fn independent_missing_field_rejected() {
    let mut value = positive();
    value.as_object_mut().expect("object").remove("license_sha256");
    rejected(value);
}

#[test]
fn independent_json_whitespace_rejected() {
    let mut raw = bytes(&positive());
    raw.insert(1, b' ');
    noncanonical_rejected(&raw);
}

#[test]
fn independent_json_key_order_rejected() {
    let raw = bytes(&positive());
    let end = raw.len() - 2;
    let prefix = b"\"protocols\":[2,3,4],";
    let start = raw.windows(prefix.len()).position(|x| x == prefix).expect("protocol member");
    let mut changed = raw[..start].to_vec();
    changed.extend_from_slice(&raw[start + prefix.len()..end]);
    changed.extend_from_slice(b",\"protocols\":[2,3,4]}\n");
    noncanonical_rejected(&changed);
}

#[test]
fn independent_missing_final_lf_rejected() {
    let mut raw = bytes(&positive());
    raw.pop();
    noncanonical_rejected(&raw);
}

#[test]
fn independent_extra_final_lf_rejected() {
    let mut raw = bytes(&positive());
    raw.push(b'\n');
    noncanonical_rejected(&raw);
}

#[test]
fn independent_utf8_bom_rejected() {
    let mut raw = b"\xef\xbb\xbf".to_vec();
    raw.extend_from_slice(&bytes(&positive()));
    assert!(Descriptor::parse(&raw, &binding()).is_err());
}

#[test]
fn independent_invalid_utf8_rejected() {
    let _ = positive();
    assert!(Descriptor::parse(b"\xff", &binding()).is_err());
}

#[test]
fn independent_trailing_document_rejected() {
    let mut raw = bytes(&positive());
    raw.extend_from_slice(b"{}\n");
    assert!(Descriptor::parse(&raw, &binding()).is_err());
}

#[test]
fn independent_escaped_equivalent_string_rejected() {
    let raw = String::from_utf8(bytes(&positive())).expect("UTF8");
    let changed = raw.replacen("zryna.native-installation", "\\u007aryna.native-installation", 1);
    noncanonical_rejected(changed.as_bytes());
}

#[test]
fn independent_json_depth_13_rejected() {
    let _ = positive();
    let raw = format!("{}0{}\n", "[".repeat(13), "]".repeat(13));
    let error = Descriptor::parse(raw.as_bytes(), &binding()).err().expect("depth rejection");
    assert!(error.message.contains("nesting exceeds 12"));
}

#[test]
fn independent_descriptor_byte_4097_rejected() {
    let _ = positive();
    let error =
        Descriptor::parse(&vec![b' '; 4097], &binding()).err().expect("byte bound rejection");
    assert!(error.message.contains("exceeds 4096"));
}

macro_rules! top_case {
    ($name:ident, $key:literal, $value:expr) => {
        #[test]
        fn $name() {
            let mut value = positive();
            value[$key] = json!($value);
            rejected(value);
        }
    };
}

top_case!(independent_foreign_format_rejected, "format", "zryna.distribution.v1");
top_case!(independent_foreign_version_rejected, "version", "999.0.0");
top_case!(independent_wrong_cli_name_rejected, "cli", "bin/zryna");
top_case!(independent_traversing_cli_path_rejected, "cli", "bin/../native-installation-proof");
top_case!(independent_protocol_missing_rejected, "protocols", [2, 3]);
top_case!(independent_protocol_duplicate_rejected, "protocols", [2, 3, 3, 4]);
top_case!(independent_protocol_reordered_rejected, "protocols", [4, 3, 2]);
top_case!(independent_protocol_unknown_rejected, "protocols", [2, 3, 4, 5]);
top_case!(independent_protocol_bool_rejected, "protocols", [true, false]);
top_case!(independent_protocol_float_rejected, "protocols", [2.0, 3.0, 4.0]);
top_case!(
    independent_license_digest_uppercase_rejected,
    "license_sha256",
    "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCD"
);
top_case!(independent_license_digest_length_rejected, "license_sha256", "abc");

#[test]
fn independent_foreign_host_target_rejected() {
    let mut value = positive();
    value["target"] =
        json!(if cfg!(windows) { "x86_64-unknown-linux-gnu" } else { "x86_64-pc-windows-msvc" });
    rejected(value);
}

#[test]
fn independent_foreign_repository_rejected() {
    let mut value = positive();
    value["source"]["repository"] = json!("https://example.invalid/zryna");
    rejected(value);
}

#[test]
fn independent_wrong_commit_rejected() {
    let mut value = positive();
    value["source"]["commit"] = json!(TREE);
    rejected(value);
}

#[test]
fn independent_wrong_tree_rejected() {
    let mut value = positive();
    value["source"]["tree"] = json!(COMMIT);
    rejected(value);
}
