//! Authenticated native Copy MIR shape evidence.
use zryna_ir::generic_v1::{copy_v1, wire};
use zryna_layout::StorageTarget;
use zryna_ownership_runtime_abi::generic_v1 as runtime;
use zryna_semantics::bounded_generics_v1::{
    SemanticInput,
    body_types::check_body_types,
    instantiation::{copy_v1::produce_claim, discover, layouts::verify_layouts},
    resolve_declarations,
};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v5::{decode_snapshot, verify_snapshot};

pub(super) fn with_program(cross: bool, test: impl FnOnce(&copy_v1::VerifiedCopyProgram<'_>)) {
    let mut files = vec![SourceFileInput {
        path: "main.zry".into(),
        text: if cross {
            include_str!("../../../../tests/m7-generic-copy-fixtures/cross-main.zry")
        } else {
            include_str!("../../../../tests/m7-generic-copy-fixtures/main.zry")
        }
        .into(),
    }];
    if cross {
        files.push(SourceFileInput {
            path: "values.zry".into(),
            text: include_str!("../../../../tests/m7-generic-copy-fixtures/cross-values.zry")
                .into(),
        });
    }
    let snapshot: &[u8] = if cross {
        include_bytes!("../../../../tests/m7-generic-copy-fixtures/cross-reference.json")
    } else {
        include_bytes!("../../../../tests/m7-generic-copy-fixtures/reference.json")
    };
    let sources = SourceMap::build(files).expect("genuine source map");
    let syntax = verify_snapshot(decode_snapshot(snapshot).expect("frozen DTO"), &sources)
        .expect("exact full source authentication");
    let entry = sources.verify_file_id(0).expect("entry");
    let declarations = resolve_declarations(
        SemanticInput::try_new(&syntax, &sources, entry).expect("exact semantic input"),
    )
    .expect("declarations");
    let bodies = check_body_types(&declarations).expect("opaque symbolic original bodies");
    let instances = discover(&bodies).expect("bounded complete demand");
    assert_eq!(instances.function_keys().len(), 5);
    let linear = verify_layouts(&instances, StorageTarget::Linear32V1).expect("linear layouts");
    let linux = verify_layouts(&instances, StorageTarget::LinuxX8664V1).expect("linux layouts");
    let runtime = runtime::verify_v1(
        runtime::raw_v1(&linear, &linux).expect("raw contract"),
        &linear,
        &linux,
    )
    .expect("separate runtime issuer");
    let raw = produce_claim(&instances, &linear, &linux).expect("untrusted semantic claim");
    let encoded = wire::encode(&raw).expect("bounded wire");
    let program = copy_v1::verify(
        wire::decode(&encoded).expect("wire decode"),
        &syntax,
        &sources,
        entry,
        &linear,
        &linux,
        &runtime,
    )
    .expect("independent source/body/demand/Copy seal");
    assert_eq!(program.ownership_effects(), (0, 0));
    test(&program);
}

#[test]
fn single_and_cross_module_programs_retain_the_exact_copy_authority() {
    for cross in [false, true] {
        with_program(cross, |program| {
            let mir = super::lower(program).expect("bounded native lane");
            assert!(std::ptr::eq(mir.program(), program));
            assert_eq!(mir.functions().len(), 11);
            assert_eq!(program.scalar_abi().exports().len(), 6);
            assert_eq!(program.ownership_effects(), (0, 0));
            for (i, function) in program.functions().iter().enumerate() {
                assert_eq!(mir.functions()[i].block_order().len(), function.blocks.len());
                assert_eq!(mir.functions()[i].symbol(), format!("zryna_gcopy_v1_{i}"));
            }
        });
    }
}
