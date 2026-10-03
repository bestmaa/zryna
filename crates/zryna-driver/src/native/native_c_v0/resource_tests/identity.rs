//! Exact recipe integration requirements and hostile private caller storage.

use super::*;
use crate::native::native_c_v0::resource_identity::handle_link_requirements;

#[test]
fn generated_direct_scalar_import_keeps_full_width_return_and_zero_resource_state() {
    let artifact = emit(&capture::reference(), "imported");
    let entry = entry(&artifact, "imported");
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs = {{.count=2,.values={{INT32_MAX,1}}}};
  struct zryna_c_v0_outcome outcome;
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 0);
  assert(outcome.value == INT32_MIN && outcome.unresolved == 0 && outcome.reserved == 0);
  assert(fixture_test_observe().calls == 1 && fixture_test_observe().allocations == 0);
  return 0;
}}
"
    );
    run(&artifact, &client, true);
}

#[test]
fn link_requirements_retain_exact_selected_imports_and_source_bound_material() {
    let first = emit(&capture::reference(), "readSeed");
    let requirements = handle_link_requirements(&first).expect("complete original material");
    assert_eq!(requirements.object().bytes(), first.bytes());
    assert_eq!(
        requirements
            .object()
            .imported_operations()
            .map(|operation| operation.declaration().symbol.as_str())
            .collect::<Vec<_>>(),
        ["fixture_close", "fixture_open", "fixture_read"]
    );
    assert_eq!(requirements.libraries().len(), 1);
    assert_eq!(requirements.libraries()[0].id(), "fixture-c-v0@0");
    let changed = capture::HANDLE.replace("return result;", "return result + 1;");
    let second = emit(&capture::edited(capture::BUFFER, &changed, capture::SCALAR), "readSeed");
    let second =
        handle_link_requirements(&second).expect("independently recaptured changed source");
    assert_ne!(requirements.object_sha256(), second.object_sha256());
    assert_ne!(requirements.declaration_sha256(), second.declaration_sha256());
    assert_eq!(requirements.private_header_sha256(), second.private_header_sha256());
    assert_eq!(requirements.libraries(), second.libraries());
}

#[test]
fn private_context_and_dispatch_rejections_precede_every_foreign_effect() {
    let artifact = emit(&capture::reference(), "readSeed");
    let entry = entry(&artifact, "readSeed");
    let client = format!(
        r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs = {{.count=1,.values={{42}}}};
  struct zryna_c_v0_outcome outcome;
  fixture_test_reset();
  inputs.count = 0;
  assert({entry}(&context,&inputs,&outcome) == 3);
  inputs.count = 1;
  context.busy = 1;
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(context.busy == 1);
  context.busy = 0;
  context.owners[0].release = 99;
  assert({entry}(&context,&inputs,&outcome) == 3);
  zryna_c_v0_context_initialize(&context);
  assert(zryna_c_v0_i_dispatch(&context,&inputs,&outcome,UINT32_MAX) == 3);
  assert(outcome.tag == 3 && context.busy == 0);
  assert(fixture_test_observe().calls == 0);
  return 0;
}}
"
    );
    run(&artifact, &client, true);
}
