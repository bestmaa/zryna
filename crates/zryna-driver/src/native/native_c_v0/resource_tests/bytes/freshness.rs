//! Same-pointer fresh claims cannot create two accepted byte obligations or two releases.

use super::*;

#[test]
fn aliased_byte_creation_retains_credit_and_releases_only_the_accepted_prefix() {
    let extra = r#"
function alias(bytes: Vec<i32>): i32 {
  const loan: FfiBytes = Ffi.borrowBytes(bytes);
  const first: FfiBytesOut = Ffi.outBytes();
  const n: FfiCountOut = Ffi.outCount();
  const a: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_copy_bytes",loan,Ffi.byteLength(loan),first,n);
  if (a !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_copy_bytes",a); }
  const owned: FfiOwnedBytes = Ffi.takeBytes(first,n);
  const second: FfiBytesOut = Ffi.outBytes();
  const m: FfiCountOut = Ffi.outCount();
  const b: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_copy_bytes",loan,Ffi.byteLength(loan),second,m);
  if (b !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_copy_bytes",b); }
  const again: FfiOwnedBytes = Ffi.takeBytes(second,m);
  return 42;
}
"#;
    let artifact = emit_bytes(
        &capture::edited(&format!("{}{extra}", capture::BUFFER), capture::HANDLE, capture::SCALAR),
        "alias",
    );
    let entry = entry(&artifact, "alias");
    let replacement = "int32_t fixture_copy_bytes(const uint8_t *bytes,size_t length,uint8_t **out,size_t *count) { if (first_bytes) { *out=first_bytes;*count=length;return 0; } int32_t status=reviewed_fixture_copy_bytes(bytes,length,out,count);if(status==0)first_bytes=*out;return status; }";
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  zryna_rt_o1_handle input={{0}};
  assert(zryna_rt_o1_vec_allocate(1,3,&input) == 0);input.length=3;memset((void *)input.pointer,0,12);
  memcpy(&inputs.owned[0],&input,sizeof(input));fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(outcome.unresolved == 1 && outcome.reserved == 1 && context.live == 0 && context.poisoned == 1);
  assert(fixture_test_observe().allocations == 1 && fixture_test_observe().releases == 1 && fixture_test_observe().live == 0 && allocation_head == NULL);
  return 0;
}}
"
    );
    let _ = run_fault(&artifact, &client, "static uint8_t *first_bytes;", replacement, true);
}
