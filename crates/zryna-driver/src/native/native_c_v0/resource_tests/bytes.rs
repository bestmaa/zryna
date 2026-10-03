//! Independent C observations of actual generated packing, String loans and owned byte copies.

use super::*;
mod bounds;
mod failures;
mod freshness;
mod malformed;
mod transfers;

#[test]
fn generated_packed_vector_and_string_loans_execute_with_private_cleanup() {
    for (name, prepare, expected) in [
        (
            "sum",
            "assert(zryna_rt_o1_vec_allocate(1,3,&input) == 0); int32_t *data=(int32_t *)input.pointer; data[0]=1; data[1]=2; data[2]=255; input.length=3;",
            258,
        ),
        (
            "utf8Sum",
            "assert(zryna_rt_o1_string_from_utf8_copy((const uint8_t *)\"A\\xc3\\xa9\",3,&input) == 0);",
            429,
        ),
    ] {
        let artifact = emit_bytes(&capture::reference(), name);
        let entry = entry(&artifact, name);
        let client = format!(
            r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs = {{.count=1}};
  struct zryna_c_v0_outcome outcome;
  zryna_rt_o1_handle input = {{0}};
  {prepare}
  uintptr_t original = input.pointer;
  memcpy(&inputs.owned[0],&input,sizeof(input));
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 0);
  assert(outcome.value == {expected} && outcome.unresolved == 0 && context.poisoned == 0);
  assert(header_for(original) == NULL && allocation_head == NULL);
  assert(fixture_test_observe().calls == 1 && fixture_test_observe().allocations == 0);
  memset(&inputs.owned[0],0,sizeof(inputs.owned[0]));
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 0 && outcome.value == 0);
  assert(allocation_head == NULL && fixture_test_observe().calls == 1);
  return 0;
}}
"
        );
        run(&artifact, &client, true);
        run_sanitized(&artifact, &client);
    }
}

#[test]
fn generated_foreign_bytes_expand_to_distinct_private_i32_storage_before_transfer() {
    let artifact = emit_bytes(&capture::reference(), "copied");
    let entry = entry(&artifact, "copied");
    let requirements = super::super::resource_identity::handle_link_requirements(&artifact)
        .expect("exact byte requirements");
    assert!(requirements.private_runtime_source().is_some());
    assert!(requirements.private_runtime_source_sha256().is_some());
    assert!(requirements.private_runtime_header_sha256().is_some());
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs = {{.count=1}};
  struct zryna_c_v0_outcome outcome;
  zryna_rt_o1_handle input = {{0}};
  assert(zryna_rt_o1_vec_allocate(1,3,&input) == 0);
  int32_t *data=(int32_t *)input.pointer;
  data[0]=0; data[1]=128; data[2]=255; input.length=3;
  uintptr_t original=input.pointer;
  memcpy(&inputs.owned[0],&input,sizeof(input));
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 0);
  assert(outcome.owned.pointer != 0 && outcome.owned.length == 3 && outcome.owned.capacity == 3);
  assert(outcome.unresolved == 0 && outcome.value == 0 && context.live == 0 && context.reserved == 0);
  assert(header_for(original) == NULL);
  assert(allocation_head != NULL && allocation_head->next == NULL);
  assert(allocation_head->data == outcome.owned.pointer && allocation_head->size == 12 && allocation_head->alignment == 4);
  int32_t *result=(int32_t *)outcome.owned.pointer;
  assert(result[0] == 0 && result[1] == 128 && result[2] == 255);
  struct fixture_test_observation observed=fixture_test_observe();
  assert(observed.allocations == 1 && observed.releases == 1 && observed.live == 0 && observed.calls == 2);
  zryna_rt_o1_handle returned;
  memcpy(&returned,&outcome.owned,sizeof(returned));
  assert(zryna_rt_o1_vec_release_storage(1,&returned) == 0 && allocation_head == NULL);
  memset(&inputs.owned[0],0,sizeof(inputs.owned[0]));
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 0);
  assert(outcome.owned.pointer == 0 && outcome.owned.length == 0 && outcome.owned.capacity == 0);
  assert(outcome.unresolved == 0 && allocation_head == NULL);
  observed=fixture_test_observe();
  assert(observed.calls == 1 && observed.allocations == 0 && observed.releases == 0 && observed.live == 0);
  return 0;
}}
"
    );
    run(&artifact, &client, true);
    run_sanitized(&artifact, &client);
}
