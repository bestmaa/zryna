use serde_json::Value;

use super::{TARGET, decode, raw, validation, wire};

const FIXTURE: &[u8] = include_bytes!("../../../../tests/native-c-abi-v0/declarations.ffi.json");

fn document() -> Value {
    serde_json::from_slice(FIXTURE).expect("independent committed reference document")
}

fn encode(mut value: Value) -> Vec<u8> {
    value.sort_all_objects();
    let mut bytes = serde_json::to_vec(&value).expect("independent JSON encoding");
    bytes.push(b'\n');
    bytes
}

fn rejected(bytes: &[u8], code: &str, detail: &str) {
    let failure = decode(bytes, TARGET).expect_err("input must not produce even a raw set");
    assert_eq!(failure.code(), code);
    assert_eq!(failure.detail(), detail);
}

#[test]
fn native_c_v0_decodes_reference_into_untrusted_distinct_carriers() {
    let declarations = decode(FIXTURE, TARGET).expect("bounded closed reference wire");
    assert_eq!(declarations.operations.len(), 8);
    assert_eq!(declarations.sites.len(), 31);
    assert_eq!(declarations.operations[0].parameters[0].abi, raw::AbiType::CI32);
    assert_ne!(raw::AbiType::CI32, raw::AbiType::CInt);
    assert_ne!(raw::AbiType::Bool32, raw::AbiType::CI32);
    assert_eq!(declarations.operations[0].parameters[0].resource, None);
    assert_eq!(declarations.operations[0].execution, raw::Execution::Synchronous);
    let open = declarations
        .operations
        .iter()
        .find(|operation| operation.symbol == "fixture_open")
        .expect("fixture open declaration");
    assert_eq!(open.statuses.iter().map(|status| status.code).collect::<Vec<_>>(), vec![0, 1, 2]);
}

#[test]
fn native_c_v0_rejects_independent_duplicate_keys_and_noncanonical_bytes() {
    let source = std::str::from_utf8(FIXTURE).expect("reference UTF-8");
    for bytes in [
        source.replace("\"version\":0", "\"version\":0,\"version\":0"),
        source.replace("\"version\":0", "\"version\":1,\"version\":0"),
        source.replace("\"resource\":null", "\"resource\":0,\"resource\":null"),
        source.replace("\"version\":0", "\"version\":0e0"),
        format!(" {source}"),
        source.trim_end().to_owned(),
    ] {
        let failure =
            decode(bytes.as_bytes(), TARGET).expect_err("canonical wire must reject mutation");
        assert_eq!(failure.code(), "ZRYNA-C4100");
    }
    rejected(&[0xc0, 0xaf], "ZRYNA-C4100", "utf8");
    rejected(b"{\n", "ZRYNA-C4100", "json");
    assert!(decode(FIXTURE, TARGET).is_ok(), "rejection retains no state");
}

#[test]
fn native_c_v0_required_nullable_fields_cannot_be_omitted() {
    for location in ["parameter", "resource-max", "resource-count", "site-operation"] {
        let mut value = document();
        match location {
            "parameter" => {
                value["operations"][0]["parameters"][0]
                    .as_object_mut()
                    .expect("parameter object")
                    .remove("resource");
            }
            "resource-max" => {
                value["operations"][1]["resources"][0]
                    .as_object_mut()
                    .expect("resource object")
                    .remove("maxBytes");
            }
            "resource-count" => {
                value["operations"][1]["resources"][0]
                    .as_object_mut()
                    .expect("resource object")
                    .remove("expectedLengthSlot");
            }
            _ => {
                value["sites"][0].as_object_mut().expect("site object").remove("operation");
            }
        }
        rejected(&encode(value), "ZRYNA-C4101", "closed-shape");
    }
}

#[test]
fn native_c_v0_closed_tags_unknown_fields_and_wrong_carriers_reject() {
    for tag in ["varargs", "_Bool", "void-pointer", "bytes-out", "struct-by-value"] {
        let mut value = document();
        value["operations"][0]["parameters"][0]["abi"] = tag.into();
        rejected(&encode(value), "ZRYNA-C4101", "closed-shape");
    }
    let mut unknown = document();
    unknown["operations"][0]["linkerPath"] = "/tmp/unchecked.so".into();
    rejected(&encode(unknown), "ZRYNA-C4101", "closed-shape");
    let mut unit = document();
    unit["operations"][0]["parameters"][0]["abi"] = "unit".into();
    rejected(&encode(unit), "ZRYNA-C4101", "parameter-carrier-or-resource");
    let mut pointer_result = document();
    pointer_result["operations"][0]["result"] = "handle-in".into();
    rejected(&encode(pointer_result), "ZRYNA-C4101", "result-carrier");
}

#[test]
fn native_c_v0_wire_tags_are_strings_not_externally_tagged_enum_objects() {
    let mutations = [
        ("parameter", serde_json::json!({"c-i32": null})),
        ("direction", serde_json::json!({"import": null})),
        ("owner", serde_json::json!({"caller": null})),
        ("primitive", serde_json::json!({"borrowBytes": null})),
    ];
    for (location, tag) in mutations {
        let mut value = document();
        match location {
            "parameter" => value["operations"][0]["parameters"][0]["abi"] = tag,
            "direction" => value["operations"][0]["direction"] = tag,
            "owner" => value["operations"][1]["resources"][0]["ownerBefore"] = tag,
            _ => value["sites"][0]["primitive"] = tag,
        }
        rejected(&encode(value), "ZRYNA-C4101", "closed-shape");
    }
}

#[test]
fn native_c_v0_unsupported_target_is_independently_selected() {
    for target in [
        "all",
        "universal",
        "javascript",
        "webassembly",
        "component",
        "aarch64-unknown-linux-gnu",
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-linux-gnux32",
    ] {
        let failure = decode(FIXTURE, target).expect_err("selection must fail closed");
        assert_eq!(failure.code(), "ZRYNA-C4103");
    }
    let mut value = document();
    value["target"] = "x86_64-pc-windows-msvc".into();
    rejected(&encode(value), "ZRYNA-C4103", "target");
}

#[test]
fn native_c_v0_wire_depth_utf8_string_and_predecode_byte_bounds_are_exact() {
    let nested = |depth| format!("{}null{}\n", "[".repeat(depth), "]".repeat(depth));
    assert!(wire::decode(nested(16).as_bytes()).is_ok());
    let depth = wire::decode(nested(17).as_bytes()).expect_err("first extra container");
    assert_eq!((depth.code(), depth.detail()), ("ZRYNA-C4107", "wire-depth"));
    let exact = encode(Value::String("é".repeat(32768)));
    let exact = wire::decode(&exact).expect("canonical string wire");
    assert!(wire::check_string_budget(&exact).is_ok());
    let first_extra = encode(Value::String(format!("{}a", "é".repeat(32768))));
    let first_extra = wire::decode(&first_extra).expect("canonical string wire");
    let strings =
        wire::check_string_budget(&first_extra).expect_err("UTF-8 bytes, not character count");
    assert_eq!((strings.code(), strings.detail()), ("ZRYNA-C4107", "string-value-bytes"));
    let bytes = vec![b' '; super::MAX_WIRE_BYTES];
    assert_eq!(wire::decode(&bytes).expect_err("not JSON, within wire cap").code(), "ZRYNA-C4100");
    let bytes = vec![b' '; super::MAX_WIRE_BYTES + 1];
    let wire_size = wire::decode(&bytes).expect_err("reject size before decoding");
    assert_eq!((wire_size.code(), wire_size.detail()), ("ZRYNA-C4107", "wire-bytes"));
}

#[test]
fn native_c_v0_raw_collection_limits_do_not_depend_on_identity_or_policy_acceptance() {
    let declarations = decode(FIXTURE, TARGET).expect("reference raw set");
    for (metric, maximum) in [
        ("sources", 256),
        ("libraries", 16),
        ("operations", 256),
        ("sites", 4096),
        ("parameters", 16),
        ("resources", 8),
        ("statuses", 16),
        ("kinds", 16),
        ("allocators", 16),
    ] {
        let mut exact = declarations.clone();
        match metric {
            "sources" => exact.sources = vec![exact.sources[0].clone(); maximum],
            "libraries" => exact.libraries = vec![exact.libraries[0].clone(); maximum],
            "operations" => exact.operations = vec![exact.operations[0].clone(); maximum],
            "sites" => exact.sites = vec![exact.sites[0].clone(); maximum],
            "parameters" => {
                exact.operations[0].parameters =
                    vec![exact.operations[0].parameters[0].clone(); maximum];
            }
            "resources" => {
                exact.operations[1].resources =
                    vec![exact.operations[1].resources[0].clone(); maximum];
            }
            "statuses" => {
                let open = exact
                    .operations
                    .iter_mut()
                    .find(|operation| operation.symbol == "fixture_open")
                    .expect("fixture open declaration");
                let status = open.statuses.first().expect("fixture open status").clone();
                open.statuses = vec![status; maximum];
            }
            "kinds" => {
                exact.libraries[0].kinds =
                    (0..maximum).map(|index| format!("fixture-c-v0@0/k{index}")).collect();
            }
            _ => {
                exact.libraries[0].allocators =
                    vec![exact.libraries[0].allocators[0].clone(); maximum];
            }
        }
        validation::check(&exact).expect("exact raw shape budget; identities remain unverified");
        match metric {
            "sources" => exact.sources.push(exact.sources[0].clone()),
            "libraries" => exact.libraries.push(exact.libraries[0].clone()),
            "operations" => exact.operations.push(exact.operations[0].clone()),
            "sites" => exact.sites.push(exact.sites[0].clone()),
            "parameters" => {
                let extra = exact.operations[0].parameters[0].clone();
                exact.operations[0].parameters.push(extra);
            }
            "resources" => {
                let extra = exact.operations[1].resources[0].clone();
                exact.operations[1].resources.push(extra);
            }
            "statuses" => {
                let open = exact
                    .operations
                    .iter_mut()
                    .find(|operation| operation.symbol == "fixture_open")
                    .expect("fixture open declaration");
                let extra = open.statuses.first().expect("fixture open status").clone();
                open.statuses.push(extra);
            }
            "kinds" => exact.libraries[0].kinds.push("fixture-c-v0@0/extra".into()),
            _ => {
                let extra = exact.libraries[0].allocators[0].clone();
                exact.libraries[0].allocators.push(extra);
            }
        }
        let failure = validation::check(&exact).expect_err("first extra raw shape budget");
        assert_eq!((failure.code(), failure.detail()), ("ZRYNA-C4107", metric));
    }
}

#[test]
fn native_c_v0_decoding_never_authenticates_policy_or_source_claims() {
    let mut value = document();
    value["libraries"][0]["headerSha256"] = "0".repeat(64).into();
    value["libraries"][0]["policySha256"] = "0".repeat(64).into();
    value["operations"][0]["sourceBinding"]["start"] = 0.into();
    let decoded = decode(&encode(value), TARGET).expect("well-shaped raw claims, still untrusted");
    assert_eq!(decoded.libraries[0].header_sha256, "0".repeat(64));
    assert_eq!(decoded.operations[0].source_binding.start, 0);
}
