//! Prerequisite byte cleanup proof, before separate reviewed real-library admission.

use super::*;
mod harness;
mod malformed;
mod prefixes;
mod provenance;
mod reference;

fn requirements(captured: &capture::Capture, name: &str) -> HandleLinkRequirements {
    let ir = zryna_native_c_ir::lower(&captured.sources, &captured.authority)
        .expect("independent byte fixture prerequisite");
    let mir =
        zryna_native_mir::native_c_v0::lower(&ir).expect("independent byte fixture prerequisite");
    let symbol = mir
        .functions()
        .find(|f| f.name() == name)
        .expect("independent byte fixture prerequisite")
        .entry()
        .symbol
        .clone();
    let object = zryna_backend_native::native_c_v0::resources::emit_byte_entries(
        &mir,
        &[&symbol],
        zryna_backend_native::select_object_target(zryna_backend_native::NATIVE_OBJECT_TARGET)
            .expect("independent byte fixture prerequisite"),
    )
    .expect("independent byte fixture prerequisite");
    super::super::super::super::resource_identity::handle_link_requirements(&object)
        .expect("independent byte fixture prerequisite")
}

fn observe(
    name: &str,
    library: &str,
    client: &str,
    sanitized: bool,
) -> crate::native::BoundedProcessOutput {
    observe_issued(&capture::reference(), name, library, client, sanitized)
}

fn observe_issued(
    capture: &capture::Capture,
    name: &str,
    library: &str,
    client: &str,
    sanitized: bool,
) -> crate::native::BoundedProcessOutput {
    let fixture = Fixture::new();
    let requirements = requirements(capture, name);
    let bytes = fixture.compile(library);
    let snapshot = accept(&requirements, &bytes, &["free", "malloc"]);
    let client = client.replace("ENTRY", &entry(&requirements, name));
    harness::observe(&fixture, &requirements, &snapshot, library, &client, sanitized)
}

#[test]
fn separate_foreign_bytes_copy_into_distinct_private_storage_then_release_once() {
    let client = r"
int main(void) {
  const size_t lengths[] = {0,3,4096};
  for (size_t test=0;test<3;++test) {
    reset_observation();
    struct zryna_c_v0_context context; zryna_c_v0_context_initialize(&context);
    struct zryna_c_v0_inputs inputs={.count=1}; struct zryna_c_v0_outcome outcome;
    size_t length=lengths[test]; input_bytes(&inputs,length);
    uintptr_t input=inputs.owned[0].pointer;
    assert(ENTRY(&context,&inputs,&outcome)==0 && outcome.tag==0);
    assert(outcome.owned.length==length && outcome.owned.capacity==length);
    assert(outcome.unresolved==0 && context.live==0 && context.reserved==0 && context.poisoned==0);
    if (length) {
      assert(outcome.owned.pointer!=input && outcome.owned.pointer!=0);
      private_pointer_is_distinct_from_foreign(outcome.owned.pointer,length);
      int32_t *result=(int32_t *)outcome.owned.pointer;
      for (size_t i=0;i<length;++i) assert(result[i]==(int32_t)(i%256));
      assert(foreign_count(length,0)==1 && foreign_count(length,1)==0);
      assert(live_count()==1); release_result(&outcome);
    } else { assert(outcome.owned.pointer==0 && live_count()==0); }
    assert(live_count()==0 && releases==owner_count);
  }
  return 0;
}
";
    for sanitized in [false, true] {
        assert!(observe("copied", &library_source(), client, sanitized).status.success());
    }
}

#[test]
fn separate_byte_allocation_failures_preserve_domains_and_release_every_accepted_owner() {
    let client = r"
int main(void) {
  for (size_t failure=0;failure<4;++failure) {
    reset_observation();
    struct zryna_c_v0_context context; zryna_c_v0_context_initialize(&context);
    struct zryna_c_v0_inputs inputs={.count=1}; struct zryna_c_v0_outcome outcome;
    input_bytes(&inputs,3); attempts=0; fail_at=failure;
    uint32_t tag=ENTRY(&context,&inputs,&outcome);
    assert(tag==(failure==0?0U:failure==2?1U:2U));
    assert(outcome.unresolved==0 && context.live==0 && context.reserved==0 && context.poisoned==0);
    if (!failure) { assert(outcome.owned.length==3); release_result(&outcome); }
    else {
      assert(outcome.owned.pointer==0 && outcome.value==0);
      if (failure==2) assert(outcome.status==1 && outcome.trap==0);
      else assert(outcome.trap==4 && outcome.status==1);
    }
    assert(live_count()==0 && releases==owner_count);
    assert(foreign_count(3,1)==0 && foreign_count(3,0)==(failure==0 || failure==3));
  }
  return 0;
}
";
    for sanitized in [false, true] {
        assert!(observe("copied", &library_source(), client, sanitized).status.success());
    }
}

#[test]
fn separate_byte_oracle_detects_actual_missing_release_then_pristine_recovers() {
    let client = r"
int main(void) {
  reset_observation();
  struct zryna_c_v0_context context; zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={.count=1}; struct zryna_c_v0_outcome outcome;
  input_bytes(&inputs,3);
  assert(ENTRY(&context,&inputs,&outcome)==0);
  assert(foreign_count(3,1)==0 && foreign_count(3,0)==1);
  release_result(&outcome); assert(live_count()==0);
  return 0;
}
";
    let mutant = library_source().replace(
        "void fixture_release_bytes(uint8_t *bytes) {free(bytes);}",
        "void fixture_release_bytes(uint8_t *bytes) {(void)bytes;}",
    );
    assert_ne!(mutant, library_source());
    let failure = observe("copied", &mutant, client, false);
    assert_eq!(failure.status.signal(), Some(libc::SIGABRT));
    assert!(String::from_utf8_lossy(&failure.stderr).contains("foreign_count(3,1)==0"));
    for sanitized in [false, true] {
        assert!(observe("copied", &library_source(), client, sanitized).status.success());
    }
}
