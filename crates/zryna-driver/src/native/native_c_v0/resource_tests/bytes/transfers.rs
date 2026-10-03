//! Original private moves and repeated synchronous loans, with exact physical release ordering.

use super::*;

#[test]
fn private_string_result_moves_only_after_loan_end_and_successful_cleanup() {
    let extra = r#"
function retained(text: String): String {
  const loan: FfiBytes = Ffi.borrowUtf8(text);
  const out: FfiI32Out = Ffi.outI32();
  const status: i32 = Ffi.rawCall("fixture-c-v0@0/sum_bytes",loan,Ffi.byteLength(loan),out);
  if (status !== 0) { return Ffi.foreignError("fixture-c-v0@0/sum_bytes",status); }
  return text;
}
"#;
    let artifact = emit_bytes(
        &capture::edited(&format!("{}{extra}", capture::BUFFER), capture::HANDLE, capture::SCALAR),
        "retained",
    );
    let entry = entry(&artifact, "retained");
    let client = format!(
        r#"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  zryna_rt_o1_handle input={{0}};
  assert(zryna_rt_o1_string_from_utf8_copy((const uint8_t *)"abc",3,&input) == 0);
  memcpy(&inputs.owned[0],&input,sizeof(input)); fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 0);
  assert(outcome.owned.pointer == input.pointer && outcome.owned.length == 3 && outcome.owned.capacity == 3);
  assert(outcome.unresolved == 0 && context.private_unresolved == 0 && allocation_head != NULL);
  zryna_rt_o1_handle returned; memcpy(&returned,&outcome.owned,sizeof(returned));
  assert(zryna_rt_o1_string_release(&returned) == 0 && allocation_head == NULL);
  return 0;
}}
"#
    );
    run(&artifact, &client, true);
}

#[test]
fn repeated_borrow_calls_keep_source_and_packing_live_then_release_in_reverse_order() {
    let extra = r#"
function twice(bytes: Vec<i32>): i32 {
  const moved: Vec<i32> = bytes;
  const loan: FfiBytes = Ffi.borrowBytes(moved);
  const a: FfiI32Out = Ffi.outI32();
  const s: i32 = Ffi.rawCall("fixture-c-v0@0/sum_bytes",loan,Ffi.byteLength(loan),a);
  if (s !== 0) { return Ffi.foreignError("fixture-c-v0@0/sum_bytes",s); }
  const b: FfiI32Out = Ffi.outI32();
  const t: i32 = Ffi.rawCall("fixture-c-v0@0/sum_bytes",loan,Ffi.byteLength(loan),b);
  if (t !== 0) { return Ffi.foreignError("fixture-c-v0@0/sum_bytes",t); }
  return Ffi.readI32(a) + Ffi.readI32(b);
}
"#;
    let artifact = emit_bytes(
        &capture::edited(&format!("{}{extra}", capture::BUFFER), capture::HANDLE, capture::SCALAR),
        "twice",
    );
    let entry = entry(&artifact, "twice");
    let replacement = "int32_t sum_bytes(const uint8_t *bytes,size_t count,int32_t *out) { assert(allocation_head != NULL && allocation_head->next != NULL && allocation_head->next->next == NULL); assert((uintptr_t)bytes == allocation_head->data); return reviewed_sum_bytes(bytes,count,out); }\nuint32_t zryna_rt_o1_release(uintptr_t pointer,uint64_t size,uint32_t alignment) { assert(release_step++ == 0); return reviewed_rt_release(pointer,size,alignment); }\nuint32_t zryna_rt_o1_vec_release_storage(uint32_t element,const zryna_rt_o1_handle *value) { assert(release_step++ == 1); return reviewed_rt_vec_release_storage(element,value); }";
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  zryna_rt_o1_handle input={{0}};
  assert(zryna_rt_o1_vec_allocate(1,3,&input) == 0); input.length=3;
  int32_t *data=(int32_t *)input.pointer; data[0]=1; data[1]=2; data[2]=3;
  memcpy(&inputs.owned[0],&input,sizeof(input)); fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 0 && outcome.value == 12);
  assert(release_step == 2 && allocation_head == NULL && outcome.unresolved == 0 && fixture_test_observe().calls == 2);
  return 0;
}}
"
    );
    let _ = run_fault(&artifact, &client, "static uint32_t release_step;", replacement, true);
}

#[test]
fn private_result_protects_its_exact_origin_while_other_owned_parameters_are_released() {
    let extra = "\nfunction second(first: String, second: String): String { return second; }\n";
    let artifact = emit_bytes(
        &capture::edited(&format!("{}{extra}", capture::BUFFER), capture::HANDLE, capture::SCALAR),
        "second",
    );
    let entry = entry(&artifact, "second");
    let client = format!(
        r#"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=2}};
  struct zryna_c_v0_outcome outcome;
  zryna_rt_o1_handle first={{0}}, second={{0}};
  assert(zryna_rt_o1_string_from_utf8_copy((const uint8_t *)"first",5,&first) == 0);
  assert(zryna_rt_o1_string_from_utf8_copy((const uint8_t *)"second",6,&second) == 0);
  memcpy(&inputs.owned[0],&first,sizeof(first));
  memcpy(&inputs.owned[1],&second,sizeof(second));
  assert({entry}(&context,&inputs,&outcome) == 0);
  assert(outcome.owned.pointer == second.pointer && outcome.owned.length == 6);
  assert(header_for(first.pointer) == NULL && allocation_head != NULL && allocation_head->next == NULL);
  assert(outcome.unresolved == 0 && context.private_unresolved == 0);
  zryna_rt_o1_handle returned; memcpy(&returned,&outcome.owned,sizeof(returned));
  assert(zryna_rt_o1_string_release(&returned) == 0 && allocation_head == NULL);
  return 0;
}}
"#
    );
    run(&artifact, &client, true);
}
