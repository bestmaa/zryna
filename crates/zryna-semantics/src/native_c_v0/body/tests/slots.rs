use super::{capture, verify_bodies};

#[test]
fn native_c_body_v0_scoped_loan_and_slot_aliases_keep_the_same_origin_without_cloning_storage() {
    let buffer = capture::BUFFER.replacen(
        "  const status: i32 = Ffi.rawCall(\"fixture-c-v0@0/sum_bytes\", loan, Ffi.byteLength(loan), out);",
        "  const sameLoan: FfiBytes = loan;\n  const sameOut: FfiI32Out = out;\n  const status: i32 = Ffi.rawCall(\"fixture-c-v0@0/sum_bytes\", loan, Ffi.byteLength(sameLoan), sameOut);", 1);
    let capture = capture::edited(&buffer, capture::HANDLE, capture::SCALAR);
    let bodies = verify_bodies(&capture.sources, &capture.declarations)
        .expect("scoped aliases preserve one physical origin");
    let sum =
        bodies.functions().iter().find(|function| function.name() == "sum").expect("sum wrapper");
    let loans: Vec<_> = sum
        .expressions()
        .iter()
        .filter(|expression| expression.value_type() == super::ValueType::Bytes)
        .filter_map(super::super::types::TypedExpression::token_id)
        .collect();
    assert!(loans.iter().all(|token| *token == loans[0]));
}

#[test]
fn native_c_body_v0_a_later_call_invalidates_an_earlier_calls_scalar_output_initialization() {
    let extra = r#"
function stale(bytes: Vec<i32>): i32 {
  const loan: FfiBytes = Ffi.borrowBytes(bytes); const out: FfiI32Out = Ffi.outI32();
  const a: i32 = Ffi.rawCall("fixture-c-v0@0/sum_bytes", loan, Ffi.byteLength(loan), out);
  if (a !== 0) { return Ffi.foreignError("fixture-c-v0@0/sum_bytes", a); }
  const b: i32 = Ffi.rawCall("fixture-c-v0@0/sum_bytes", loan, Ffi.byteLength(loan), out);
  const wrong: i32 = Ffi.readI32(out);
  if (b !== 0) { return Ffi.foreignError("fixture-c-v0@0/sum_bytes", b); }
  return wrong;
}
"#;
    let capture = capture::append_handle(extra);
    assert_eq!(
        verify_bodies(&capture.sources, &capture.declarations)
            .expect_err("stale dominance from earlier call")
            .detail(),
        "output-before-exact-status-zero"
    );
}

#[test]
fn native_c_body_v0_distinct_names_never_make_one_output_token_two_nonaliasing_c_slots() {
    let mut document = capture::document();
    let mut operation = document["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .find(|operation| operation["symbol"] == "fixture_read")
        .expect("status template")
        .clone();
    operation["key"] = "fixture-c-v0@0/pair_out".into();
    operation["logicalName"] = "pair_out".into();
    operation["symbol"] = "pair_out".into();
    operation["parameters"] = serde_json::json!([
        {"abi":"i32-out","name":"arg0","resource":null}, {"abi":"i32-out","name":"arg1","resource":null}
    ]);
    operation["resources"] = serde_json::json!([]);
    operation["statuses"][0]["initialized"] = serde_json::json!([0, 1]);
    document["operations"].as_array_mut().expect("operations").push(operation);
    let extra = r#"
function alias(): i32 {
  const out: FfiI32Out = Ffi.outI32(); const alias: FfiI32Out = out;
  const status: i32 = Ffi.rawCall("fixture-c-v0@0/pair_out", out, alias);
  if (status !== 0) { return Ffi.foreignError("fixture-c-v0@0/pair_out", status); }
  return Ffi.readI32(out);
}
"#;
    let handle = format!("{}{extra}", capture::HANDLE);
    let header =
        [capture::HEADER, b"\nint32_t pair_out(int32_t *first, int32_t *second);\n"].concat();
    let capture = capture::captured_headers(
        &[("buffer", capture::BUFFER), ("handle", &handle), ("scalar", capture::SCALAR)],
        false,
        document,
        &[("fixture-c-v0@0", &header)],
    );
    assert_eq!(
        verify_bodies(&capture.sources, &capture.declarations)
            .expect_err("aliased actual output identities")
            .detail(),
        "aliased-output-slots"
    );
}
