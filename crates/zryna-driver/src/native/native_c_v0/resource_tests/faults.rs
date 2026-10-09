//! Independent contract-violation observations; unresolved and process faults remain distinct.

use super::*;

#[test]
fn generated_unknown_status_and_null_success_are_host_failures_and_block_reuse() {
    let artifact = emit(&capture::reference(), "readSeed");
    let entry = entry(&artifact, "readSeed");
    for (open, reserved) in
        [("(void)seed; (void)out; return 7;", 1), ("(void)seed; *out = NULL; return 0;", 0)]
    {
        let replacement = format!(
            "int32_t fixture_open(int32_t seed, struct fixture_handle **out) {{ {open} }}\nvoid fixture_close(struct fixture_handle *handle) {{ reviewed_close(handle); }}"
        );
        let client = format!(
            r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs = {{.count=1,.values={{42}}}};
  struct zryna_c_v0_outcome outcome;
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(outcome.tag == 3 && outcome.value == 0 && outcome.reserved == {reserved} && outcome.unresolved == {reserved});
  assert(context.poisoned == 1 && context.busy == 0);
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(outcome.tag == 3 && outcome.unresolved == {reserved});
  assert(fixture_test_observe().allocations == 0 && fixture_test_observe().releases == 0);
  return 0;
}}
"
        );
        let _ = run_fault(&artifact, &client, "", &replacement, true);
    }
}

#[test]
fn generated_release_fault_is_a_process_failure_without_retry_or_success_frame() {
    let artifact = emit(&capture::reference(), "readSeed");
    let entry = entry(&artifact, "readSeed");
    let replacement = "int32_t fixture_open(int32_t seed, struct fixture_handle **out) { return reviewed_open(seed,out); }\nvoid fixture_close(struct fixture_handle *handle) { (void)handle; assert(tested_context->live == 1 && tested_context->owners[0].state == 2); _Exit(86); }";
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  tested_context = &context;
  struct zryna_c_v0_inputs inputs = {{.count=1,.values={{42}}}};
  struct zryna_c_v0_outcome outcome;
  fixture_test_reset();
  (void){entry}(&context,&inputs,&outcome);
  abort();
}}
"
    );
    let status = run_fault(
        &artifact,
        &client,
        "static struct zryna_c_v0_context *tested_context;",
        replacement,
        false,
    );
    assert_eq!(status.code(), Some(86));
}

#[test]
fn generated_earlier_prefix_is_cleaned_on_later_recoverable_allocation_failure() {
    let artifact = emit(&capture::compact_handle(&acquisitions("acquire", 3)), "acquire");
    let entry = entry(&artifact, "acquire");
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs = {{0}};
  struct zryna_c_v0_outcome outcome;
  fixture_test_reset();
  fixture_test_fail_allocation(3);
  assert({entry}(&context,&inputs,&outcome) == 1);
  assert(outcome.status == 2 && outcome.unresolved == 0 && outcome.value == 0);
  struct fixture_test_observation observed = fixture_test_observe();
  assert(observed.calls == 5 && observed.allocations == 2 && observed.releases == 2 && observed.live == 0);
  assert(fixture_test_release_at(0).acquisition == 2 && fixture_test_release_at(1).acquisition == 1);
  return 0;
}}
"
    );
    run(&artifact, &client, true);
}
