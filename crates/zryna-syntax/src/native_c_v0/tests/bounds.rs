use super::{document, encode, rejected};
use crate::native_c_v0::{TARGET, decode};
use serde_json::Value;

// Independent canonical wire fixtures exercise the actual declaration decoder, never MIR
// symbols or a JavaScript prototype. Positive records remain raw claims, not semantic seals.
fn boundary(pointer: &str, exact: String, extra: String, metric: &str) {
    let mut value = document();
    *value.pointer_mut(pointer).expect("committed declaration field") = Value::String(exact);
    let bytes = encode(value.clone());
    let accepted = decode(&bytes, TARGET).expect("exact declaration field budget");
    assert_eq!(
        serde_json::to_value(accepted).expect("raw declaration").pointer(pointer),
        value.pointer(pointer)
    );
    *value.pointer_mut(pointer).expect("same declaration field") = Value::String(extra);
    rejected(&encode(value), "ZRYNA-C4107", metric);
    assert!(decode(&bytes, TARGET).is_ok(), "first-extra rejection retains no state");
}

#[test]
fn native_c_v0_declaration_logical_name_128_129() {
    boundary("/operations/0/logicalName", "a".repeat(128), "a".repeat(129), "name-bytes");
}

#[test]
fn native_c_v0_declaration_symbol_128_129() {
    boundary("/operations/0/symbol", "a".repeat(128), "a".repeat(129), "name-bytes");
}

#[test]
fn native_c_v0_declaration_parameter_name_128_129() {
    boundary("/operations/0/parameters/0/name", "a".repeat(128), "a".repeat(129), "name-bytes");
}

#[test]
fn native_c_v0_declaration_library_id_128_129() {
    boundary(
        "/libraries/0/id",
        format!("{}@0", "a".repeat(126)),
        format!("{}@0", "a".repeat(127)),
        "library-id-bytes",
    );
}

#[test]
fn native_c_v0_declaration_library_version_128_129() {
    boundary("/libraries/0/version", "1".repeat(128), "1".repeat(129), "library-version-bytes");
}

#[test]
fn native_c_v0_declaration_qualified_key_257_258() {
    boundary(
        "/operations/0/key",
        format!("{}@0/{}", "a".repeat(126), "b".repeat(128)),
        format!("{}@0/{}", "a".repeat(126), "b".repeat(129)),
        "key-bytes",
    );
}

#[test]
fn native_c_v0_declaration_source_path_256_257() {
    for pointer in ["/sources/0/path", "/operations/0/sourceBinding/path", "/sites/0/path"] {
        boundary(
            pointer,
            format!("d/{}.zry", "a".repeat(250)),
            format!("d/{}.zry", "a".repeat(251)),
            "source-path-bytes",
        );
    }
}

#[test]
fn native_c_v0_declaration_primitive_spelling_4096_4097() {
    let prefix = "Ffi.rawCall(";
    boundary(
        "/sites/0/spelling",
        format!("{prefix}{})", " ".repeat(4096 - prefix.len() - 1)),
        format!("{prefix}{})", " ".repeat(4097 - prefix.len() - 1)),
        "primitive-spelling-bytes",
    );
}

#[test]
fn native_c_v0_declaration_export_name_115_116() {
    let mut value = document();
    let export = value["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["direction"] == "export")
        .expect("fixed export");
    export["logicalName"] = "e".repeat(115).into();
    export["symbol"] = format!("zryna_c_v0_e_{}", "e".repeat(115)).into();
    let exact = encode(value.clone());
    assert!(decode(&exact, TARGET).is_ok());
    let export = value["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["direction"] == "export")
        .expect("same export");
    export["logicalName"] = "e".repeat(116).into();
    export["symbol"] = format!("zryna_c_v0_e_{}", "e".repeat(116)).into();
    rejected(&encode(value), "ZRYNA-C4107", "name-bytes");
}
