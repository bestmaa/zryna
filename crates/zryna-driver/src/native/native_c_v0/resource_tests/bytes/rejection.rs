//! Actual private dispatcher rejection must neither expose stale results nor move caller storage.

use super::*;
use zryna_backend_native::native_c_v0::resources::ValidatedHandleEntries;

#[test]
fn byte_dispatcher_invalid_ordinal_clears_stale_results_and_reports_private_obligations() {
    let artifact = emit_bytes(&capture::reference(), "copied");
    observe_rejection(&artifact, u32::MAX);
}

#[test]
fn byte_dispatcher_unselected_ordinal_clears_stale_results_and_reports_private_obligations() {
    let artifact = emit_bytes(&capture::reference(), "copied");
    let ordinal = artifact
        .program()
        .functions()
        .enumerate()
        .find(|(index, _)| !artifact.entries().contains(index))
        .map(|(index, _)| u32::try_from(index).expect("bounded original ordinal"))
        .expect("retained unselected body");
    observe_rejection(&artifact, ordinal);
}

fn observe_rejection(artifact: &ValidatedHandleEntries, ordinal: u32) {
    let client = REJECTION_CLIENT
        .replace("SELECTED_ENTRY", &entry(artifact, "copied"))
        .replace("REJECTED_ORDINAL", &ordinal.to_string());
    run(artifact, &client, true);
    run_sanitized(artifact, &client);
}

const REJECTION_CLIENT: &str = r"
int main(void) {
  for (unsigned path=0; path<2; ++path) {
    for (unsigned test=0; test<3; ++test) {
      struct zryna_c_v0_context context;
      zryna_c_v0_context_initialize(&context);
      context.private_unresolved=test ? 3U : 0U;
      context.poisoned=test ? 1U : 0U;
      context.live=test==2 ? 2U : 0U;
      context.reserved=test==2 ? 1U : 0U;
      struct zryna_c_v0_context before=context;
      struct zryna_c_v0_inputs inputs={.count=path ? 1U : 0U};
      zryna_rt_o1_handle input={0}, previous={0};
      assert(zryna_rt_o1_vec_allocate(1,3,&input)==0);
      input.length=3;
      assert(zryna_rt_o1_vec_allocate(1,1,&previous)==0);
      previous.length=1;
      memcpy(&inputs.owned[0],&input,sizeof(input));
      struct zryna_c_v0_inputs input_before=inputs;
      struct zryna_c_v0_outcome outcome;
      memset(&outcome,0xa5,sizeof(outcome));
      memcpy(&outcome.owned,&previous,sizeof(previous));
      fixture_test_reset();
      uint32_t tag=path
        ? zryna_c_v0_i_dispatch(&context,&inputs,&outcome,REJECTED_ORDINAL)
        : SELECTED_ENTRY(&context,&inputs,&outcome);
      assert(tag==3 && outcome.tag==3 && outcome.operation==UINT32_MAX);
      assert(outcome.status==0 && outcome.trap==0 && outcome.value==0 && outcome.padding==0);
      assert(outcome.unresolved==before.live+before.reserved+before.private_unresolved);
      assert(outcome.reserved==before.reserved);
      assert(outcome.owned.pointer==0 && outcome.owned.length==0 && outcome.owned.capacity==0);
      assert(memcmp(&context,&before,sizeof(context))==0);
      assert(memcmp(&inputs,&input_before,sizeof(inputs))==0);
      assert(header_for(input.pointer)!=NULL && header_for(previous.pointer)!=NULL);
      assert(allocation_head!=NULL && allocation_head->next!=NULL && allocation_head->next->next==NULL);
      struct fixture_test_observation observed=fixture_test_observe();
      assert(observed.calls==0 && observed.allocations==0 && observed.releases==0);
      /* Explicit caller disposal is not generated cleanup or recovery evidence. */
      assert(zryna_rt_o1_vec_release_storage(1,&input)==0);
      assert(zryna_rt_o1_vec_release_storage(1,&previous)==0 && allocation_head==NULL);
    }
  }
  return 0;
}
";

#[test]
fn byte_rejection_validates_channel_pointers_alignment_and_magic_before_extended_access() {
    let artifact = emit_bytes(&capture::reference(), "copied");
    let ordinal = u32::try_from(artifact.entries()[0]).expect("selected original ordinal");
    let client = GUARD_CLIENT
        .replace("SELECTED_ENTRY", &entry(&artifact, "copied"))
        .replace("SELECTED_ORDINAL", &ordinal.to_string());
    run(&artifact, &client, true);
    run_sanitized(&artifact, &client);
}

#[test]
fn byte_channel_alignment_checks_preserve_the_handle_only_four_byte_channel() {
    let artifact = emit(&capture::reference(), "imported");
    assert!(!artifact.uses_storage_channel());
    let client = HANDLE_ALIGNMENT_CLIENT.replace("SELECTED_ENTRY", &entry(&artifact, "imported"));
    run(&artifact, &client, true);
    run_sanitized(&artifact, &client);
}

const HANDLE_ALIGNMENT_CLIENT: &str = r#"
extern uint32_t test_handle_words(uintptr_t,uintptr_t,uintptr_t) __asm__("SELECTED_ENTRY");
int main(void) {
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_context before=context;
  _Alignas(8) uint8_t input[sizeof(struct zryna_c_v0_inputs)+8]={0};
  const uint32_t words[]={2,0,20,22};
  memcpy(input+4,words,sizeof(words));
  uint8_t input_before[sizeof(input)];
  memcpy(input_before,input,sizeof(input));
  _Alignas(8) uint8_t output[sizeof(struct zryna_c_v0_outcome)+8];
  memset(output,0xa5,sizeof(output));
  fixture_test_reset();
  assert(test_handle_words((uintptr_t)&context,(uintptr_t)(input+4),(uintptr_t)(output+4))==0);
  struct zryna_c_v0_outcome outcome;
  memcpy(&outcome,output+4,sizeof(outcome));
  assert(outcome.tag==0 && outcome.value==42 && outcome.unresolved==0 && outcome.reserved==0);
  for (size_t index=0; index<4; ++index) {
    assert(output[index]==0xa5 && output[index+4+sizeof(outcome)]==0xa5);
  }
  assert(memcmp(&context,&before,sizeof(context))==0);
  assert(memcmp(input,input_before,sizeof(input))==0);
  assert(fixture_test_observe().calls==1 && fixture_test_observe().allocations==0);
  return 0;
}
"#;

// Raw channel words preserve the generated SysV I64/I32 signature while avoiding C lvalues
// for deliberately null or misaligned private-channel addresses.
const GUARD_CLIENT: &str = r#"
extern uint32_t test_dispatch_words(uintptr_t,uintptr_t,uintptr_t,uint32_t)
  __asm__("zryna_c_v0_i_dispatch");
extern uint32_t test_entry_words(uintptr_t,uintptr_t,uintptr_t) __asm__("SELECTED_ENTRY");
static uint32_t call_words(unsigned dispatch,uintptr_t context,uintptr_t inputs,uintptr_t output) {
  return dispatch ? test_dispatch_words(context,inputs,output,SELECTED_ORDINAL)
                  : test_entry_words(context,inputs,output);
}
int main(void) {
  for (unsigned dispatch=0; dispatch<2; ++dispatch) {
    for (unsigned test=0; test<3; ++test) {
      struct zryna_c_v0_context context;
      zryna_c_v0_context_initialize(&context);
      context.private_unresolved=3;
      context.poisoned=1;
      if (test==2) context.magic=UINT64_C(0x5a4348414e444c30);
      struct zryna_c_v0_context before=context;
      struct zryna_c_v0_inputs inputs={.count=1};
      struct zryna_c_v0_outcome outcome;
      memset(&outcome,0xa5,sizeof(outcome));
      struct zryna_c_v0_storage owned=outcome.owned;
      uintptr_t pointer=test==0 ? 0 : (test==1 ? 1 : (uintptr_t)&context);
      fixture_test_reset();
      assert(call_words(dispatch,pointer,(uintptr_t)&inputs,(uintptr_t)&outcome)==3);
      assert(outcome.tag==3 && outcome.unresolved==0);
      assert(memcmp(&outcome.owned,&owned,sizeof(owned))==0);
      assert(memcmp(&context,&before,sizeof(context))==0);
      assert(fixture_test_observe().calls==0 && allocation_head==NULL);
    }
    struct zryna_c_v0_context context;
    zryna_c_v0_context_initialize(&context);
    struct zryna_c_v0_context before=context;
    struct zryna_c_v0_inputs inputs={.count=1};
    fixture_test_reset();
    assert(call_words(dispatch,(uintptr_t)&context,(uintptr_t)&inputs,0)==3);
    assert(memcmp(&context,&before,sizeof(context))==0);
    _Alignas(8) uint8_t output[sizeof(struct zryna_c_v0_outcome)+8];
    uint8_t output_before[sizeof(output)];
    memset(output,0xa5,sizeof(output));
    memcpy(output_before,output,sizeof(output));
    assert(call_words(dispatch,(uintptr_t)&context,(uintptr_t)&inputs,(uintptr_t)(output+4))==3);
    assert(memcmp(output,output_before,sizeof(output))==0);
    _Alignas(8) uint8_t input[sizeof(struct zryna_c_v0_inputs)+8]={0};
    uint32_t count=1;
    memcpy(input+4,&count,sizeof(count));
    uint8_t input_before[sizeof(input)];
    memcpy(input_before,input,sizeof(input));
    struct zryna_c_v0_outcome outcome;
    memset(&outcome,0xa5,sizeof(outcome));
    assert(call_words(dispatch,(uintptr_t)&context,(uintptr_t)(input+4),(uintptr_t)&outcome)==3);
    assert(outcome.tag==3 && outcome.unresolved==0 && outcome.reserved==0);
    assert(outcome.owned.pointer==0 && outcome.owned.length==0 && outcome.owned.capacity==0);
    assert(memcmp(input,input_before,sizeof(input))==0);
    assert(memcmp(&context,&before,sizeof(context))==0);
    assert(fixture_test_observe().calls==0 && allocation_head==NULL);
  }
  return 0;
}
"#;
