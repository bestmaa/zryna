//! Hostile nominal and freshness claims cannot authorize a guessed or repeated foreign release.

use super::*;

#[test]
fn aliased_fresh_handle_leaves_a_credit_unresolved_and_releases_only_accepted_prefix() {
    let artifact = emit(&capture::compact_handle(&acquisitions("acquire", 2)), "acquire");
    let entry = entry(&artifact, "acquire");
    let replacement = "static struct fixture_handle *first;\nint32_t fixture_open(int32_t seed, struct fixture_handle **out) { if (seed == 1) { *out = first; return 0; } int32_t status = reviewed_open(seed,out); if (status == 0) first = *out; return status; }\nvoid fixture_close(struct fixture_handle *handle) { reviewed_close(handle); }";
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs = {{0}};
  struct zryna_c_v0_outcome outcome;
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(outcome.value == 0 && outcome.unresolved == 1 && outcome.reserved == 1);
  assert(context.poisoned == 1 && context.live == 0);
  struct fixture_test_observation observed = fixture_test_observe();
  assert(observed.allocations == 1 && observed.releases == 1 && observed.live == 0);
  return 0;
}}
"
    );
    let _ = run_fault(&artifact, &client, "", replacement, true);
}

#[test]
fn corrupt_release_identity_blocks_c_close_and_reports_the_live_obligation() {
    let artifact = emit(&capture::reference(), "readSeed");
    let entry = entry(&artifact, "readSeed");
    let replacement = "int32_t fixture_open(int32_t seed, struct fixture_handle **out) { return reviewed_open(seed,out); }\nvoid fixture_close(struct fixture_handle *handle) { reviewed_close(handle); }\nint32_t fixture_read(struct fixture_handle *handle, int32_t *out) { int32_t status = reviewed_read(handle,out); tested_context->owners[0].release ^= 1; return status; }";
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  tested_context = &context;
  struct zryna_c_v0_inputs inputs = {{.count=1,.values={{42}}}};
  struct zryna_c_v0_outcome outcome;
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(outcome.value == 0 && outcome.unresolved == 1 && context.live == 1 && context.poisoned == 1);
  struct fixture_test_observation observed = fixture_test_observe();
  assert(observed.allocations == 1 && observed.releases == 0 && observed.live == 1 && observed.calls == 2);
  return 0;
}}
"
    );
    // Deliberately retained in-process allocation: this is refusal evidence, not leak-free proof.
    let _ = run_fault(
        &artifact,
        &client,
        "static struct zryna_c_v0_context *tested_context;",
        replacement,
        true,
    );
}
