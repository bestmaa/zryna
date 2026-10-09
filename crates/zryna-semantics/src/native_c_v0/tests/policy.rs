//! Independent malformed declaration records and exact C/policy distinctions.

use super::{
    DECLARATIONS, POLICY, capture_changed_policy, document, material, sources, verify, wire,
};
use serde_json::Value;
use zryna_syntax::{native_c_source_v0::authenticate_sources, native_c_v0::raw::AbiType};

#[test]
fn native_c_v0_declaration_authority_rejects_all_independent_malformed_records() {
    let sources = sources();
    let syntax = authenticate_sources(&sources).expect("independent grammar");
    let rows: Vec<Value> = serde_json::from_slice(include_bytes!(
        "../../../../../tests/native-c-abi-v0/malformed-declarations.json"
    ))
    .expect("independent hostile rows");
    assert_eq!(rows.len(), 38);
    for row in rows {
        let mut document = document();
        let target = row["target"].as_str().expect("target");
        let mut selected = match target {
            "root" => &mut document,
            "library" => &mut document["libraries"][0],
            "raw-site" => document["sites"]
                .as_array_mut()
                .expect("sites")
                .iter_mut()
                .find(|site| site["primitive"] == "rawCall")
                .expect("raw site"),
            _ => document["operations"]
                .as_array_mut()
                .expect("operations")
                .iter_mut()
                .find(|operation| operation["symbol"] == row["symbol"])
                .expect("operation"),
        };
        let fields = row["field"].as_array().expect("field path");
        for field in &fields[..fields.len() - 1] {
            selected = if let Some(key) = field.as_str() {
                &mut selected[key]
            } else {
                &mut selected
                    [usize::try_from(field.as_u64().expect("array index")).expect("bounded index")]
            };
        }
        let last = fields.last().expect("field key");
        if row["delete"] == true {
            selected
                .as_object_mut()
                .expect("delete object")
                .remove(last.as_str().expect("delete key"));
        } else if let Some(key) = last.as_str() {
            selected[key] = row["value"].clone();
        } else {
            selected
                [usize::try_from(last.as_u64().expect("array index")).expect("bounded index")] =
                row["value"].clone();
        }
        let policy = if row["reseal"] == true {
            capture_changed_policy(&mut document)
        } else {
            POLICY.to_vec()
        };
        let expected = if row["id"] == "wrong-target" {
            "ZRYNA-C4103"
        } else {
            row["code"].as_str().expect("fixed code")
        };
        let failure = verify(
            &wire(document),
            &sources,
            &syntax,
            &material(&policy),
            "x86_64-unknown-linux-gnu",
        )
        .expect_err("hostile declaration must fail");
        assert_eq!(failure.code(), expected, "{}: {}", row["id"], failure);
    }
    assert!(
        verify(DECLARATIONS, &sources, &syntax, &material(POLICY), "x86_64-unknown-linux-gnu")
            .is_ok()
    );
}

#[test]
fn native_c_v0_declaration_authority_keeps_c_int_distinct_and_malformed_release_promises_explicit()
{
    let sources = sources();
    let syntax = authenticate_sources(&sources).expect("independent grammar");
    let mut changed = document();
    changed["operations"][0]["parameters"][0]["abi"] = "c-int".into();
    let policy = capture_changed_policy(&mut changed);
    let verified =
        verify(&wire(changed), &sources, &syntax, &material(&policy), "x86_64-unknown-linux-gnu")
            .expect("explicit declared C-int bridge");
    assert_eq!(
        verified
            .operation("fixture-c-v0@0/add")
            .expect("add")
            .parameter_carriers()
            .collect::<Vec<_>>(),
        [AbiType::CInt, AbiType::CI32]
    );
    let mut changed = document();
    changed["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["symbol"] == "fixture_open")
        .expect("open")["resources"][0]["releasableOnMalformed"] = false.into();
    let policy = capture_changed_policy(&mut changed);
    assert!(
        verify(&wire(changed), &sources, &syntax, &material(&policy), "x86_64-unknown-linux-gnu")
            .is_ok(),
        "a false release promise must remain false for later runtime failure replay"
    );
}
