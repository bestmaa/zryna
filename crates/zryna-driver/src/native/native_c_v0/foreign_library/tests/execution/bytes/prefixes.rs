//! Actual mixed foreign prefix cleanup, before a later real-library proof.

use super::*;

#[test]
fn separate_handle_then_bytes_allocation_failures_release_the_reverse_foreign_prefix() {
    let extra = r#"
function after_handle(bytes: Vec<i32>): Vec<i32> {
  const handleOut: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const opened: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_open",7,handleOut);
  if (opened !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_open",opened); }
  const handle: FfiHandle = Ffi.takeHandle(handleOut);
  const loan: FfiBytes = Ffi.borrowBytes(bytes);
  const out: FfiBytesOut = Ffi.outBytes();
  const count: FfiCountOut = Ffi.outCount();
  const status: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_copy_bytes",loan,Ffi.byteLength(loan),out,count);
  if (status !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_copy_bytes",status); }
  const foreign: FfiOwnedBytes = Ffi.takeBytes(out,count);
  const copied: Vec<i32> = Ffi.copyBytes(foreign);
  Ffi.release("fixture-c-v0@0/fixture_release_bytes",foreign);
  Ffi.release("fixture-c-v0@0/fixture_close",handle);
  return copied;
}
"#;
    let captured =
        capture::edited(&format!("{}{extra}", capture::BUFFER), capture::HANDLE, capture::SCALAR);
    let client = r"
int main(void) {
  for (size_t failure=0;failure<5;++failure) {
    reset_observation();
    struct zryna_c_v0_context context; zryna_c_v0_context_initialize(&context);
    struct zryna_c_v0_inputs inputs={.count=1}; struct zryna_c_v0_outcome outcome;
    input_bytes(&inputs,3); attempts=0; fail_at=failure;
    uint32_t tag=ENTRY(&context,&inputs,&outcome);
    assert(tag==(!failure?0U:failure==1 || failure==3?1U:2U));
    assert(outcome.unresolved==0 && context.live==0 && context.reserved==0 && context.poisoned==0);
    if (!failure) { assert(outcome.owned.length==3); release_result(&outcome); }
    else {
      assert(outcome.owned.pointer==0 && outcome.value==0);
      if (failure==1 || failure==3) assert(outcome.status==(failure==1?2:1) && outcome.trap==0);
      else assert(outcome.trap==4 && outcome.status==1);
    }
    assert(live_count()==0 && releases==owner_count);
    assert(foreign_count(4,0)==(!failure || failure>=2));
    assert(foreign_count(3,0)==(!failure || failure==4));
    if (!failure || failure==4) foreign_releases_precede(3,4);
  }
  return 0;
}
";
    for sanitized in [false, true] {
        assert!(
            observe_issued(&captured, "after_handle", &library_source(), client, sanitized)
                .status
                .success()
        );
    }
}
