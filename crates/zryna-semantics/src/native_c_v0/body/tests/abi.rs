use super::super::BoundaryCheck;
use super::{FlowStep, capture, verify_bodies};
use zryna_syntax::native_c_v0::raw::AbiType;

#[test]
fn native_c_body_v0_c_int_bridge_remains_distinct_from_c_i32_in_the_typed_call() {
    let mut document = capture::document();
    let operation = document["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["symbol"] == "add")
        .expect("scalar import");
    operation["parameters"][0]["abi"] = "c-int".into();
    let header = std::str::from_utf8(capture::HEADER).expect("C header").replace(
        "int32_t add(int32_t left, int32_t right);",
        "int32_t add(int left, int32_t right);",
    );
    let capture = capture::captured_headers(
        &[("buffer", capture::BUFFER), ("handle", capture::HANDLE), ("scalar", capture::SCALAR)],
        false,
        document,
        &[("fixture-c-v0@0", header.as_bytes())],
    );
    let bodies =
        verify_bodies(&capture.sources, &capture.declarations).expect("explicit C-int bridge");
    let imported = bodies
        .functions()
        .iter()
        .find(|function| function.name() == "imported")
        .expect("direct import");
    assert!(
        imported.steps().iter().any(|step| matches!(step,
            FlowStep::Call { carriers, .. } if carriers.as_slice() == [AbiType::CInt, AbiType::CI32]
        )),
        "source i32 compatibility does not erase declared C spelling"
    );
}

#[test]
fn native_c_body_v0_boolean_shim_requires_complete_carrier_checks_before_value_exposure() {
    let mut document = capture::document();
    let mut operation = document["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .find(|operation| operation["symbol"] == "add")
        .expect("direct import")
        .clone();
    operation["key"] = "fixture-c-v0@0/fixture_boolean_shim".into();
    operation["logicalName"] = "fixture_boolean_shim".into();
    operation["symbol"] = "fixture_boolean_shim".into();
    operation["parameters"] = serde_json::json!([{"abi":"bool32","name":"arg0","resource":null}]);
    operation["result"] = "bool32".into();
    document["operations"].as_array_mut().expect("operations").push(operation);
    let scalar = format!(
        "{}\nfunction booleanBridge(flag: bool): bool {{ return Ffi.rawCall(\"fixture-c-v0@0/fixture_boolean_shim\", flag); }}",
        capture::SCALAR
    );
    let header = [capture::HEADER, b"\nuint32_t fixture_boolean_shim(uint32_t flag);\n"].concat();
    let capture = capture::captured_headers(
        &[("buffer", capture::BUFFER), ("handle", capture::HANDLE), ("scalar", &scalar)],
        false,
        document,
        &[("fixture-c-v0@0", &header)],
    );
    let bodies =
        verify_bodies(&capture.sources, &capture.declarations).expect("declared Boolean shim");
    let bridge = bodies
        .functions()
        .iter()
        .find(|function| function.name() == "booleanBridge")
        .expect("Boolean wrapper");
    assert!(bridge.steps().iter().any(|step| matches!(step,
        FlowStep::Call { carriers, boundary_checks, .. }
            if carriers.as_slice() == [AbiType::Bool32]
                && boundary_checks.iter().any(|check| matches!(check, BoundaryCheck::BooleanCarrier { .. }))
                && boundary_checks.contains(&BoundaryCheck::BooleanResult)
    )));
}
