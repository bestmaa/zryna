//! Remaining normative tiny-fixture cases with independent object and physical owner observations.

use super::*;
use sha2::{Digest, Sha256};

#[test]
fn separate_raw_sum_null_zero_and_over_limit_preserve_outputs_and_inputs() {
    let client = r"
int32_t sum_bytes(const uint8_t *,size_t,int32_t *);
int32_t fixture_copy_bytes(const uint8_t *,size_t,uint8_t **,size_t *);
void fixture_release_bytes(uint8_t *);
int main(void) {
  reset_observation();
  int32_t out=4242;
  assert(sum_bytes(NULL,0,&out)==0 && out==0);
  const uint8_t small[]={1,2,3}; out=4242;
  assert(sum_bytes(small,3,&out)==0 && out==6);
  assert(small[0]==1 && small[1]==2 && small[2]==3);
  uint8_t large[4097],before[4097]; memset(large,1,sizeof(large));
  memcpy(before,large,sizeof(large)); out=4242;
  assert(sum_bytes(large,sizeof(large),&out)==1 && out==4242);
  assert(memcmp(large,before,sizeof(large))==0 && owner_count==0 && releases==0);
  uint8_t *owned=(uint8_t *)(uintptr_t)1; size_t count=99;
  assert(fixture_copy_bytes(large,sizeof(large),&owned,&count)==1);
  assert(owned==(uint8_t *)(uintptr_t)1 && count==99 && owner_count==0);
  assert(memcmp(large,before,sizeof(large))==0);
  assert(fixture_copy_bytes(NULL,0,&owned,&count)==0 && owned==NULL && count==0);
  assert(owner_count==0 && releases==0);
  assert(fixture_copy_bytes(small,sizeof(small),&owned,&count)==0 && count==3);
  assert(owned!=small && memcmp(owned,small,3)==0 && foreign_count(3,1)==1);
  fixture_release_bytes(owned);
  assert(foreign_count(3,0)==1 && live_count()==0 && releases==1);
  reset_observation();
  struct zryna_c_v0_context context; zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={.count=1}; struct zryna_c_v0_outcome outcome;
  input_bytes(&inputs,3); int32_t *data=(int32_t *)inputs.owned[0].pointer;
  data[0]=1;data[1]=2;data[2]=3;
  assert(ENTRY(&context,&inputs,&outcome)==0 && outcome.value==6);
  assert(outcome.unresolved==0 && context.poisoned==0 && owner_count==2);
  assert(live_count()==0 && releases==owner_count);
  return 0;
}
";
    for sanitized in [false, true] {
        assert!(observe("sum", &library_source(), client, sanitized).status.success());
    }
}

#[test]
fn separate_reference_utf8_string_loan_releases_only_the_private_input() {
    let client = r"
int main(void) {
  const uint8_t utf8[]={65,0xc3,0xa9};
  for (size_t length=0;length<=3;length+=3) {
    reset_observation();
    struct zryna_c_v0_context context; zryna_c_v0_context_initialize(&context);
    struct zryna_c_v0_inputs inputs={.count=1}; struct zryna_c_v0_outcome outcome;
    zryna_rt_o1_handle input={0};
    assert(zryna_rt_o1_string_from_utf8_copy(length?utf8:NULL,length,&input)==0);
    memcpy(&inputs.owned[0],&input,sizeof(input));
    assert(ENTRY(&context,&inputs,&outcome)==0 && outcome.value==(length?429U:0U));
    assert(outcome.unresolved==0 && context.private_unresolved==0 && context.poisoned==0);
    assert(owner_count==(length?1U:0U) && releases==owner_count && live_count()==0);
  }
  return 0;
}
";
    for sanitized in [false, true] {
        assert!(observe("utf8Sum", &library_source(), client, sanitized).status.success());
    }
}

#[test]
fn separate_malformed_metadata_without_release_permission_retains_the_foreign_obligation() {
    let captured = without_malformed_release();
    for count in ["0", "2", "4097"] {
        let library = library_source()
            .replace("*out=copy; *count=length;", &format!("*out=copy; *count={count};"));
        let client = r"
int main(void) {
  reset_observation();
  struct zryna_c_v0_context context; zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={.count=1}; struct zryna_c_v0_outcome outcome;
  input_bytes(&inputs,3);
  assert(ENTRY(&context,&inputs,&outcome)==3 && outcome.tag==3);
  assert(outcome.owned.pointer==0 && outcome.owned.length==0 && outcome.owned.capacity==0);
  assert(outcome.value==0 && outcome.trap==0 && outcome.unresolved==1);
  assert(context.live==1 && context.reserved==0 && context.poisoned==1);
  assert(owner_count==3 && attempts==3 && releases==2 && live_count()==1);
  assert(foreign_count(3,1)==1 && foreign_count(3,0)==0);
  /* Oracle-owned disposal is not a generated release or leak-free recovery claim. */
  for (size_t i=0;i<owner_count;++i) if (!owner_releases[i]) __wrap_free(owners[i]);
  assert(live_count()==0 && releases==owner_count);
  return 0;
}
";
        for sanitized in [false, true] {
            assert!(
                observe_issued(&captured, "copied", &library, client, sanitized).status.success()
            );
        }
    }
}

#[test]
fn separate_foreign_byte_timeout_is_a_process_diagnostic_without_recovery_claim() {
    let fixture = Fixture::new();
    let requirements = requirements(&capture::reference(), "sum");
    let library = library_source().replace(
        "int32_t result=0;",
        "volatile int running=1; while(running) {} int32_t result=0;",
    );
    let snapshot = accept(&requirements, &fixture.compile(&library), &["free", "malloc"]);
    let client = r"
int main(void) {
  reset_observation();
  struct zryna_c_v0_context context; zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={.count=1}; struct zryna_c_v0_outcome outcome;
  input_bytes(&inputs,3);
  return (int)ENTRY(&context,&inputs,&outcome);
}
"
    .replace("ENTRY", &entry(&requirements, "sum"));
    let error = harness::observe_with_timeout(
        &fixture,
        &requirements,
        &snapshot,
        &library,
        &client,
        false,
        std::time::Duration::from_millis(100),
    )
    .expect_err("a genuine foreign call must time out, not produce a language outcome");
    assert_eq!(error.code(), "ZRYNA-N4007");
    fixture.empty();
    // A separate pristine invocation proves staging recovery, not crashed C owner cleanup.
    assert!(observe("sum", &library_source(), &client, false).status.success());
}

fn without_malformed_release() -> capture::Capture {
    let original = capture::reference();
    let mut document: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../../../../tests/native-c-abi-v0/declarations.ffi.json"
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
