//! Test-only separately compiled reviewed C; captured data never grants production execution.

use super::*;
use std::{fmt::Write as _, os::unix::process::ExitStatusExt as _};
mod boolean;
mod bytes;
mod harness;
use harness::Fixture;

fn library_source() -> String {
    format!(
        "{}\n{}",
        std::str::from_utf8(capture::HEADER).expect("independent fixture prerequisite"),
        include_str!("library.c")
    )
}

#[test]
fn independently_instrumented_compiler_object_rejects_then_explicit_profile_recovers() {
    use object::{Object as _, ObjectSection as _};
    let fixture = Fixture::new();
    let requirements = linked_requirements(&capture::reference(), "imported");
    let instrumented = fixture.compile_variant(&library_source(), &["-fcf-protection=full"]);
    let file = object::File::parse(instrumented.as_slice()).expect("independent compiler ELF");
    assert!(file.sections().any(|section| section.name().ok() == Some(".note.gnu.property")));
    reject(&requirements, &instrumented, &["free", "malloc"]);
    fixture.empty();
    let explicit = fixture.compile(&library_source());
    let file = object::File::parse(explicit.as_slice()).expect("explicit compiler ELF");
    assert!(!file.sections().any(|section| section.name().ok() == Some(".note.gnu.property")));
    accept(&requirements, &explicit, &["free", "malloc"]);
}
fn entry(requirements: &HandleLinkRequirements, name: &str) -> String {
    requirements
        .object()
        .program()
        .functions()
        .find(|f| f.name() == name)
        .expect("independent fixture prerequisite")
        .entry()
        .symbol
        .clone()
}

#[test]
fn separately_compiled_foreign_scalar_uses_snapshot_after_path_replacement() {
    let fixture = Fixture::new();
    let requirements = linked_requirements(&capture::reference(), "imported");
    let bytes = fixture.compile(&library_source());
    let path = fixture.acquired_path();
    std::fs::write(&path, &bytes).expect("independent fixture prerequisite");
    let acquired = std::fs::read(&path).expect("independent fixture prerequisite");
    let snapshot = accept(&requirements, &acquired, &["free", "malloc"]);
    std::fs::write(&path, b"replacement must never reach linker")
        .expect("independent fixture prerequisite");
    drop(acquired);
    let entry = entry(&requirements, "imported");
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs = {{.count=2, .values={{20,22}}}};
  struct zryna_c_v0_outcome outcome;
  assert({entry}(&context,&inputs,&outcome)==0);
  assert(outcome.tag==0 && outcome.value==42 && outcome.unresolved==0);
  inputs.values[0]=INT32_MAX; inputs.values[1]=1;
  assert({entry}(&context,&inputs,&outcome)==0);
  assert(outcome.tag==0 && outcome.value==UINT32_C(2147483648) && outcome.unresolved==0);
  assert(context.busy==0 && context.live==0 && context.poisoned==0);
  return 0;
}}
"
    );
    assert!(fixture.run(&requirements, &snapshot, &client, false, true).success());
}

#[test]
fn separately_compiled_handles_release_every_acquired_prefix_in_reverse_order() {
    let fixture = Fixture::new();
    let mut body = capture::HANDLE.to_owned();
    body.push_str("\nfunction acquire(): i32 {\n");
    for index in 0..2 {
        write!(body,"const o{index}: FfiHandleOut = Ffi.outHandle(\"fixture-c-v0@0/fixture_handle\");\nconst s{index}: i32 = Ffi.rawCall(\"fixture-c-v0@0/fixture_open\", 7, o{index});\nif (s{index} !== 0) {{ return Ffi.foreignError(\"fixture-c-v0@0/fixture_open\", s{index}); }}\nconst h{index}: FfiHandle = Ffi.takeHandle(o{index});\n").expect("bounded two-handle source");
    }
    body.push_str("return 42;\n}\n");
    let captured = capture::edited(capture::BUFFER, &body, capture::SCALAR);
    let requirements = linked_requirements(&captured, "acquire");
    let bytes = fixture.compile(&library_source());
    let snapshot = accept(&requirements, &bytes, &["free", "malloc"]);
    let entry = entry(&requirements, "acquire");
    let client = format!(
        r"
static size_t attempts, allocated, released, fail_at;
static void *owners[2];
void *__real_malloc(size_t);
void __real_free(void *);
void *__wrap_malloc(size_t count) {{
  ++attempts;
  if(attempts==fail_at) return NULL;
  void *owner=__real_malloc(count);
  assert(owner!=NULL && allocated<2);
  owners[allocated++]=owner;
  return owner;
}}
void __wrap_free(void *owner) {{
  assert(released<allocated && owner==owners[allocated-released-1]);
  ++released; __real_free(owner);
}}
int main(void) {{
  for(fail_at=0;fail_at<3;++fail_at) {{
    attempts=allocated=released=0;
    struct zryna_c_v0_context context;
    zryna_c_v0_context_initialize(&context);
    struct zryna_c_v0_inputs inputs={{0}};
    struct zryna_c_v0_outcome outcome;
    assert({entry}(&context,&inputs,&outcome)==(fail_at==0 ? 0 : 1));
    assert(outcome.tag==(fail_at==0 ? 0U : 1U) && outcome.unresolved==0);
    assert(outcome.value==(fail_at==0 ? 42U : 0U));
    if(fail_at!=0) assert(outcome.status==2);
    assert(attempts==(fail_at==1 ? 1U : 2U));
    assert(allocated==(fail_at==0 ? 2U : fail_at-1));
    assert(released==allocated);
    assert(context.live==0 && context.busy==0 && context.poisoned==0);
  }}
  return 0;
}}
"
    );
    assert!(fixture.run(&requirements, &snapshot, &client, true, true).success());
}

#[test]
fn foreign_elf_capture_does_not_certify_body_safety_or_process_recovery() {
    let fixture = Fixture::new();
    let requirements = linked_requirements(&capture::reference(), "imported");
    let source = library_source().replace("return (int32_t)value;", "(void)value; abort();");
    let bytes = fixture.compile(&source);
    let snapshot = accept(&requirements, &bytes, &["abort", "free", "malloc"]);
    let entry = entry(&requirements, "imported");
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=2,.values={{20,22}}}};
  struct zryna_c_v0_outcome outcome;
  return (int){entry}(&context,&inputs,&outcome);
}}
"
    );
    assert_eq!(
        fixture.run(&requirements, &snapshot, &client, false, true).signal(),
        Some(libc::SIGABRT)
    );
}

#[test]
fn actual_compiler_missing_required_definition_rejects_before_linking() {
    let fixture = Fixture::new();
    let requirements = linked_requirements(&capture::reference(), "readSeed");
    let bad = fixture
        .compile(&library_source().replace("int32_t fixture_read(", "int32_t renamed_read("));
    reject(&requirements, &bad, &["free", "malloc"]);
    fixture.empty();
    let valid = fixture.compile(&library_source());
    accept(&requirements, &valid, &["free", "malloc"]);
    fixture.empty();
}

#[test]
fn actual_unresolved_external_link_failure_cleans_owned_stage() {
    let fixture = Fixture::new();
    let requirements = linked_requirements(&capture::reference(), "imported");
    let source = format!(
        "{}\nint32_t missing(void);\n{}",
        std::str::from_utf8(capture::HEADER).expect("independent fixture prerequisite"),
        include_str!("library.c")
            .replace("return (int32_t)value;", "(void)value; return missing();")
    );
    let bytes = fixture.compile(&source);
    let snapshot = accept(&requirements, &bytes, &["free", "malloc", "missing"]);
    let client = "int main(void) { return 0; }";
    assert!(!fixture.run(&requirements, &snapshot, client, false, false).success());
    fixture.empty();
    let valid = fixture.compile(&library_source());
    let snapshot = accept(&requirements, &valid, &["free", "malloc"]);
    assert!(fixture.run(&requirements, &snapshot, client, false, true).success());
}
