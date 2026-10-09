//! Independent paired-output defects and reviewed conditional-release policy.

use super::*;
use sha2::{Digest, Sha256};

fn without_malformed_release() -> capture::Capture {
    let original = capture::reference();
    let mut document: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../../tests/native-c-abi-v0/declarations.ffi.json"
    ))
    .expect("original declaration bytes");
    let operations = document["operations"].as_array_mut().expect("operations");
    let creator = operations
        .iter_mut()
        .find(|operation| operation["logicalName"] == "fixture_copy_bytes")
        .expect("exact byte creator");
    creator["resources"][1]["releasableOnMalformed"] = false.into();
    let policy_operations = operations
        .iter()
        .filter(|operation| operation["library"] == "fixture-c-v0@0")
        .map(|operation| {
            let mut policy = operation.clone();
            policy.as_object_mut().expect("operation object").remove("sourceBinding");
            policy
        })
        .collect::<Vec<_>>();
    let mut policy = serde_json::json!({"allocators":document["libraries"][0]["allocators"],"kinds":document["libraries"][0]["kinds"],"operations":policy_operations});
    policy.sort_all_objects();
    let mut policy = serde_json::to_vec(&policy).expect("canonical policy");
    policy.push(b'\n');
    document["libraries"][0]["policySha256"] = format!("{:x}", Sha256::digest(&policy)).into();
    document.sort_all_objects();
    let mut bytes = serde_json::to_vec(&document).expect("canonical declarations");
    bytes.push(b'\n');
    let syntax = zryna_syntax::native_c_source_v0::authenticate_sources(&original.sources)
        .expect("original source issuer");
    let declarations = zryna_semantics::native_c_v0::verify(
        &bytes,
        &original.sources,
        &syntax,
        &[zryna_semantics::native_c_v0::LibraryMaterial {
            library_id: "fixture-c-v0@0",
            header_bytes: capture::HEADER,
            policy_bytes: &policy,
        }],
        zryna_syntax::native_c_v0::TARGET,
    )
    .expect("independently reviewed policy recapture");
    let body = zryna_semantics::native_c_v0::body::verify_bodies(&original.sources, &declarations)
        .expect("original body recapture");
    let authority =
        zryna_semantics::native_c_v0::body::compose_private_boundaries(&original.sources, &body)
            .expect("original private issuer");
    capture::Capture { sources: original.sources, authority }
}

#[test]
fn malformed_byte_lengths_are_released_only_with_the_captured_live_allocation_guarantee() {
    for allowed in [true, false] {
        let capture = if allowed { capture::reference() } else { without_malformed_release() };
        let artifact = emit_bytes(&capture, "copied");
        let entry = entry(&artifact, "copied");
        for count in ["0", "2", "4097"] {
            let replacement = format!(
                "int32_t fixture_copy_bytes(const uint8_t *bytes,size_t length,uint8_t **out,size_t *count) {{ int32_t status=reviewed_fixture_copy_bytes(bytes,length,out,count); if (status == 0) *count={count}; return status; }}"
            );
            let client = format!(
                r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  zryna_rt_o1_handle input={{0}};
  assert(zryna_rt_o1_vec_allocate(1,3,&input) == 0); input.length=3;
  int32_t *data=(int32_t *)input.pointer; data[0]=1;data[1]=2;data[2]=3;
  memcpy(&inputs.owned[0],&input,sizeof(input));fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(outcome.owned.pointer == 0 && outcome.value == 0 && outcome.unresolved == {unresolved} && context.poisoned == 1);
  assert(allocation_head == NULL && fixture_test_observe().releases == {releases} && fixture_test_observe().live == {unresolved});
  if ({unresolved}) {{
    /* Independent test disposal of the retained foreign obligation, not wrapper recovery. */
    for (size_t index=0;index<TEST_CAPACITY;++index) if (live[index].pointer) fixture_release_bytes(live[index].pointer);
  }}
  return 0;
}}
",
                unresolved = usize::from(!allowed),
                releases = usize::from(allowed)
            );
            let _ = run_fault(&artifact, &client, "", &replacement, true);
        }
    }
}

#[test]
fn null_positive_count_is_host_failure_without_a_guessed_byte_read_or_release() {
    let artifact = emit_bytes(&capture::reference(), "copied");
    let entry = entry(&artifact, "copied");
    let replacement = "int32_t fixture_copy_bytes(const uint8_t *bytes,size_t length,uint8_t **out,size_t *count) { (void)bytes;(void)length;*out=NULL;*count=3;return 0; }";
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  zryna_rt_o1_handle input={{0}};
  assert(zryna_rt_o1_vec_allocate(1,3,&input) == 0); input.length=3; memset((void *)input.pointer,0,12);
  memcpy(&inputs.owned[0],&input,sizeof(input));fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(outcome.unresolved == 0 && allocation_head == NULL && outcome.owned.pointer == 0);
  assert(fixture_test_observe().allocations == 0 && fixture_test_observe().releases == 0);
  return 0;
}}
"
    );
    let _ = run_fault(&artifact, &client, "", replacement, true);
}

#[test]
fn untaken_bytes_validate_metadata_before_cleanup_and_override_pending_return() {
    let extra = r#"
function untaken(bytes: Vec<i32>): i32 {
  const loan: FfiBytes = Ffi.borrowBytes(bytes);
  const out: FfiBytesOut = Ffi.outBytes();
  const count: FfiCountOut = Ffi.outCount();
  const status: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_copy_bytes",loan,Ffi.byteLength(loan),out,count);
  if (status !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_copy_bytes",status); }
  return 42;
}
"#;
    let artifact = emit_bytes(
        &capture::edited(&format!("{}{extra}", capture::BUFFER), capture::HANDLE, capture::SCALAR),
        "untaken",
    );
    let entry = entry(&artifact, "untaken");
    let replacement = "int32_t fixture_copy_bytes(const uint8_t *bytes,size_t length,uint8_t **out,size_t *count) { int32_t status=reviewed_fixture_copy_bytes(bytes,length,out,count);if(status==0)*count=4097;return status; }";
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
  assert(outcome.value == 0 && outcome.unresolved == 0 && allocation_head == NULL && fixture_test_observe().releases == 1 && fixture_test_observe().live == 0);
  return 0;
}}
"
    );
    let _ = run_fault(&artifact, &client, "", replacement, true);
}
