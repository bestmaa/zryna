//! Allocation, status and attempted-release faults retain their exact domains and obligations.

use super::*;

const INPUT: &str = "zryna_rt_o1_handle input={0}; assert(zryna_rt_o1_vec_allocate(1,3,&input) == 0); int32_t *data=(int32_t *)input.pointer; data[0]=1; data[1]=2; data[2]=3; input.length=3; memcpy(&inputs.owned[0],&input,sizeof(input));";

#[test]
fn private_allocation_traps_keep_branded_identity_and_clean_the_foreign_prefix() {
    let artifact = emit_bytes(&capture::reference(), "copied");
    let entry = entry(&artifact, "copied");
    for (symbol, signature, arguments, returned, trap, foreign_calls) in [
        (
            "allocate",
            "uint64_t size, uint32_t alignment, uintptr_t *out",
            "size,alignment,out",
            1,
            4,
            0,
        ),
        (
            "allocate",
            "uint64_t size, uint32_t alignment, uintptr_t *out",
            "size,alignment,out",
            2,
            5,
            0,
        ),
        (
            "vec_allocate",
            "uint32_t element, uint64_t size, zryna_rt_o1_handle *out",
            "element,size,out",
            1,
            4,
            2,
        ),
        (
            "vec_allocate",
            "uint32_t element, uint64_t size, zryna_rt_o1_handle *out",
            "element,size,out",
            2,
            5,
            2,
        ),
    ] {
        let replacement = format!(
            "uint32_t zryna_rt_o1_{symbol}({signature}) {{ if (armed) return {returned}; return reviewed_rt_{symbol}({arguments}); }}"
        );
        let client = format!(
            r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  {INPUT}
  armed=1; fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 2);
  assert(outcome.trap == {trap} && outcome.status == {returned} && outcome.value == 0 && outcome.owned.pointer == 0);
  assert(outcome.unresolved == 0 && allocation_head == NULL && context.poisoned == 0);
  struct fixture_test_observation observed=fixture_test_observe();
  assert(observed.calls == {foreign_calls} && observed.live == 0 && observed.releases == {releases});
  return 0;
}}
",
            releases = usize::from(foreign_calls != 0)
        );
        let _ = run_fault(&artifact, &client, "static uint32_t armed;", &replacement, true);
    }
}

#[test]
fn foreign_allocation_failure_cleans_private_storage_and_unknown_status_retains_credit() {
    let artifact = emit_bytes(&capture::reference(), "copied");
    let entry = entry(&artifact, "copied");
    for (body, tag, status, reserved) in [
        ("return reviewed_fixture_copy_bytes(bytes,length,out,count);", 1, 1, 0),
        ("(void)bytes; (void)length; (void)out; (void)count; return 7;", 3, 7, 1),
    ] {
        let replacement = format!(
            "int32_t fixture_copy_bytes(const uint8_t *bytes, size_t length, uint8_t **out, size_t *count) {{ {body} }}"
        );
        let client = format!(
            r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  {INPUT}
  fixture_test_reset(); fixture_test_fail_allocation(1);
  assert({entry}(&context,&inputs,&outcome) == {tag});
  assert(outcome.status == {status} && outcome.unresolved == {reserved} && outcome.reserved == {reserved});
  assert(outcome.owned.pointer == 0 && allocation_head == NULL && fixture_test_observe().live == 0);
  return 0;
}}
"
        );
        let _ = run_fault(&artifact, &client, "", &replacement, true);
    }
}

#[test]
fn private_release_failure_overrides_result_and_stops_before_suffix_or_transfer() {
    let artifact = emit_bytes(&capture::reference(), "copied");
    let entry = entry(&artifact, "copied");
    let replacement = "uint32_t zryna_rt_o1_release(uintptr_t pointer, uint64_t size, uint32_t alignment) { (void)pointer; (void)size; (void)alignment; ++raw_attempts; return 255; }\nuint32_t zryna_rt_o1_vec_release_storage(uint32_t element, const zryna_rt_o1_handle *storage) { ++vec_attempts; return reviewed_rt_vec_release_storage(element,storage); }";
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  {INPUT}
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(raw_attempts == 1 && vec_attempts == 0);
  assert(outcome.unresolved == 3 && context.private_unresolved == 3 && context.poisoned == 1);
  assert(outcome.owned.pointer == 0 && outcome.value == 0 && fixture_test_observe().releases == 1 && fixture_test_observe().live == 0);
  assert({entry}(&context,&inputs,&outcome) == 3 && outcome.unresolved == 3 && raw_attempts == 1);
  size_t retained=0; while (allocation_head != NULL) {{ ++retained; assert(release_known(allocation_head) == 0); }}
  assert(retained == 3); /* Explicit test disposal; the generated path did not release these. */
  return 0;
}}
"
    );
    let _ = run_fault(
        &artifact,
        &client,
        "static uint32_t raw_attempts, vec_attempts;",
        replacement,
        true,
    );
}

#[test]
fn foreign_release_process_fault_never_exposes_a_prepared_private_result() {
    let artifact = emit_bytes(&capture::reference(), "copied");
    let entry = entry(&artifact, "copied");
    let replacement = "void fixture_release_bytes(uint8_t *bytes) { (void)bytes; assert(tested_context->live == 1 && tested_context->owners[0].state == 2); assert(tested_outcome->owned.pointer == 0); size_t count=0; for (allocation_header *p=allocation_head;p;p=p->next) ++count; assert(count == 3); _Exit(86); }";
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  tested_context=&context; tested_outcome=&outcome;
  {INPUT}
  fixture_test_reset(); (void){entry}(&context,&inputs,&outcome); abort();
}}
"
    );
    let status = run_fault(
        &artifact,
        &client,
        "static struct zryna_c_v0_context *tested_context; static struct zryna_c_v0_outcome *tested_outcome;",
        replacement,
        false,
    );
    assert_eq!(status.code(), Some(86));
}

#[test]
fn private_unknown_status_and_failure_atomicity_defects_are_host_failures() {
    let artifact = emit_bytes(&capture::reference(), "copied");
    let entry = entry(&artifact, "copied");
    for body in [
        "(void)size;(void)alignment;(void)out;return 7;",
        "assert(reviewed_rt_allocate(size,alignment,out)==0);return 1;",
    ] {
        let replacement = format!(
            "uint32_t zryna_rt_o1_allocate(uint64_t size,uint32_t alignment,uintptr_t *out) {{ {body} }}"
        );
        let client = format!(
            r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  {INPUT}
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(outcome.trap == 0 && outcome.unresolved == 2 && context.private_unresolved == 2 && context.poisoned == 1);
  assert(outcome.owned.pointer == 0 && fixture_test_observe().calls == 0);
  assert({entry}(&context,&inputs,&outcome) == 3 && outcome.unresolved == 2);
  while (allocation_head != NULL) assert(release_known(allocation_head)==0);
  return 0;
}}
"
        );
        let _ = run_fault(&artifact, &client, "", &replacement, true);
    }
}

#[test]
fn foreign_error_that_writes_outputs_is_host_failure_and_never_a_guessed_release() {
    let artifact = emit_bytes(&capture::reference(), "copied");
    let entry = entry(&artifact, "copied");
    for body in [
        "(void)bytes;(void)length;(void)out;*count=3;return 1;",
        "assert(reviewed_fixture_copy_bytes(bytes,length,out,count)==0);return 1;",
    ] {
        let replacement = format!(
            "int32_t fixture_copy_bytes(const uint8_t *bytes,size_t length,uint8_t **out,size_t *count) {{ {body} }}"
        );
        let client = format!(
            r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  {INPUT}
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(outcome.trap == 0 && outcome.unresolved == 1 && outcome.reserved == 1 && context.poisoned == 1);
  assert(outcome.owned.pointer == 0 && allocation_head == NULL && fixture_test_observe().releases == 0);
  /* Independent oracle disposal only; the wrapper never guessed that failed-status output owned memory. */
  for (size_t index=0;index<TEST_CAPACITY;++index) if (live[index].pointer) fixture_release_bytes(live[index].pointer);
  return 0;
}}
"
        );
        let _ = run_fault(&artifact, &client, "", &replacement, true);
    }
}
