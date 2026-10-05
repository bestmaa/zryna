//! Existing authenticated Bool32 import executed against a separately compiled C shim.

use super::*;
use sha2::{Digest, Sha256};

#[test]
fn separately_compiled_boolean_import_rejects_invalid_results_before_exposure() {
    let ir = super::super::super::super::tests::boolean_import_fixture();
    let mir = zryna_native_mir::native_c_v0::lower(&ir).expect("existing Boolean import seal");
    let symbol = mir
        .functions()
        .find(|function| function.name() == "booleanBridge")
        .expect("existing Boolean bridge")
        .entry()
        .symbol
        .clone();
    let object = zryna_backend_native::native_c_v0::resources::emit_handle_entries(
        &mir,
        &[&symbol],
        zryna_backend_native::select_object_target(zryna_backend_native::NATIVE_OBJECT_TARGET)
            .expect("exact admitted target"),
    )
    .expect("source-to-IR-to-MIR-to-object Boolean entry");
    let requirements =
        super::super::super::super::resource_identity::handle_link_requirements(&object)
            .expect("original Boolean issuer and ABI requirements");
    let authority = requirements
        .object()
        .program()
        .source()
        .private_authority()
        .body_authority()
        .declaration_authority();
    let header = authority.header_bytes("fixture-c-v0@0").expect("existing Boolean header");
    for result in [0_u32, 1, 2, u32::MAX] {
        let library = format!(
            "{}\n{}\nuint32_t fixture_boolean_shim(uint32_t flag) {{ (void)flag; return UINT32_C({result}); }}\n",
            std::str::from_utf8(header).expect("retained exact C prototype"),
            include_str!("../library.c"),
        );
        let fixture = Fixture::new();
        let snapshot = accept(&requirements, &fixture.compile(&library), &["free", "malloc"]);
        let tag = if result <= 1 { 0 } else { 3 };
        let value = if result <= 1 { result } else { 0 };
        let client = format!(
            r"
int main(void) {{
  for (uint32_t flag=0;flag<=1;++flag) {{
    struct zryna_c_v0_context context; zryna_c_v0_context_initialize(&context);
    struct zryna_c_v0_inputs inputs={{.count=1,.values={{flag}}}};
    struct zryna_c_v0_outcome outcome={{.tag=99,.value=4242,.trap=42,.unresolved=43}};
    assert({symbol}(&context,&inputs,&outcome)=={tag} && outcome.tag=={tag});
    assert(outcome.value==UINT32_C({value}) && outcome.trap==0 && outcome.unresolved==0);
    assert(context.busy==0 && context.live==0 && context.reserved==0);
  }}
  return 0;
}}
"
        );
        assert!(fixture.run(&requirements, &snapshot, &client, false, true).success());
        eprintln!(
            "417-boolean-evidence {}",
            serde_json::json!({"raw_result":result,"entry_tag":tag,"exposed_value":value,
                "scalar_input_vectors":[0,1],"sanitized":false,
                "library_source_sha256":format!("{:x}",Sha256::digest(library.as_bytes())),
                "foreign_object_sha256":format!("{:x}",Sha256::digest(snapshot.bytes())),
                "client_fragment_sha256":format!("{:x}",Sha256::digest(client.as_bytes())),
                "program_object_sha256":requirements.object_sha256(),
                "declaration_sha256":requirements.declaration_sha256(),
                "library_header_sha256":requirements.libraries()[0].header_sha256(),
                "library_policy_sha256":requirements.libraries()[0].policy_sha256()})
        );
    }
}
