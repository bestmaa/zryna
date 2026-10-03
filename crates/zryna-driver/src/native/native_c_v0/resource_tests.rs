//! Generated handle bodies linked to the independently authored C observation fixture.

#[path = "../../../../zryna-native-c-ir/tests/capture.rs"]
mod capture;
mod faults;
mod identity;
mod owner_faults;
mod support;

use std::fmt::Write as _;
use support::*;

#[test]
fn generated_handle_body_executes_success_and_declared_failures_with_exact_cleanup() {
    let capture = capture::edited(capture::BUFFER, capture::HANDLE, capture::SCALAR);
    let artifact = emit(&capture, "readSeed");
    let entry = entry(&artifact, "readSeed");
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs = {{.count=1, .values={{42}}}};
  struct zryna_c_v0_outcome outcome;
  fixture_test_reset();
  assert({entry}(&context, &inputs, &outcome) == 0);
  assert(outcome.tag == 0 && outcome.value == 42 && outcome.unresolved == 0);
  struct fixture_test_observation observed = fixture_test_observe();
  assert(observed.calls == 3 && observed.allocations == 1 && observed.releases == 1 && observed.live == 0);
  assert(fixture_test_release_at(0).acquisition == 1);
  fixture_test_reset();
  inputs.values[0] = UINT32_MAX;
  assert({entry}(&context, &inputs, &outcome) == 1);
  assert(outcome.tag == 1 && outcome.status == 1 && outcome.value == 0 && outcome.unresolved == 0);
  observed = fixture_test_observe();
  assert(observed.calls == 1 && observed.allocations == 0 && observed.releases == 0);
  fixture_test_reset();
  fixture_test_fail_allocation(1);
  inputs.values[0] = 42;
  assert({entry}(&context, &inputs, &outcome) == 1);
  assert(outcome.tag == 1 && outcome.status == 2 && outcome.unresolved == 0);
  observed = fixture_test_observe();
  assert(observed.calls == 1 && observed.allocation_attempts == 1 && observed.live == 0 && observed.releases == 0);
  assert(context.busy == 0 && context.live == 0 && context.reserved == 0 && context.poisoned == 0);
  return 0;
}}
"
    );
    run(&artifact, &client, true);
    run_sanitized(&artifact, &client);
}

#[test]
fn generated_implicit_cleanup_is_reverse_order_and_capacity_precedes_foreign_call() {
    for count in [2, 64, 65] {
        let capture = capture::compact_handle(&acquisitions("acquire", count));
        let artifact = emit(&capture, "acquire");
        let entry = entry(&artifact, "acquire");
        let tag = if count == 65 { 2 } else { 0 };
        let live = count.min(64);
        let client = format!(
            r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs = {{0}};
  struct zryna_c_v0_outcome outcome;
  fixture_test_reset();
  assert({entry}(&context, &inputs, &outcome) == {tag});
  assert(outcome.tag == {tag} && outcome.unresolved == 0 && outcome.reserved == 0);
  assert(outcome.trap == {trap});
  struct fixture_test_observation observed = fixture_test_observe();
  assert(observed.allocations == {live} && observed.releases == {live} && observed.live == 0);
  assert(observed.calls == {calls});
  for (size_t index=0; index<{live}; ++index)
    assert(fixture_test_release_at(index).acquisition == {live}-index);
  return 0;
}}
",
            trap = i32::from(tag == 2),
            calls = live * 2
        );
        run(&artifact, &client, true);
        if count == 64 {
            run_sanitized(&artifact, &client);
        }
    }
}

fn acquisitions(name: &str, count: usize) -> String {
    let mut body = format!("\nfunction {name}(): i32 {{\n");
    for index in 0..count {
        write!(body,
            "const o{index}: FfiHandleOut = Ffi.outHandle(\"fixture-c-v0@0/fixture_handle\");\nconst s{index}: i32 = Ffi.rawCall(\"fixture-c-v0@0/fixture_open\", {index}, o{index});\nif (s{index} !== 0) {{ return Ffi.foreignError(\"fixture-c-v0@0/fixture_open\", s{index}); }}\nconst h{index}: FfiHandle = Ffi.takeHandle(o{index});\n"
        ).expect("bounded source fixture");
    }
    body.push_str("return 42;\n}\n");
    body
}
