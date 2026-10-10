use super::lower;
mod resources;
use zryna_ir::data_ownership_v1::{VerifiedBlock, VerifiedFunction, VerifiedModule};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::{
    command_h1_v1::{self as source, CommandSyntax},
    v4,
};

fn fixture(name: &str) -> (CommandSyntax, SourceMap) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/wasi-command-source-fixtures");
    let text = std::fs::read_to_string(root.join(format!("{name}.zry"))).expect("source fixture");
    let bytes = std::fs::read(root.join(format!("{name}.json"))).expect("provider fixture");
    let sources = SourceMap::build(vec![SourceFileInput { path: "src/main.zry".into(), text }])
        .expect("source map");
    let syntax = v4::verify_snapshot(v4::decode_snapshot(&bytes).expect("decode"), &sources)
        .expect("authenticated v4");
    (source::admit(&syntax, &sources).expect("complete command source"), sources)
}

#[test]
fn source_environment_call_lowers_to_real_effect_and_closed_owned_match() {
    let (source, sources) = fixture("environment-match");
    let program = lower(&source, &sources).expect("command source to mandatory command IR");
    let ir = program.verified_ir();
    assert_eq!(
        ir.runtime_contract(),
        zryna_ir::data_ownership_v1::RuntimeContractIdentity::CommandH1V1
    );
    let function = ir.modules().next().expect("module").functions().next().expect("main");
    let effects = function
        .blocks()
        .flat_map(VerifiedBlock::instructions)
        .filter(|instruction| {
            instruction.kind()
                == zryna_ir::data_ownership_v1::VerifiedInstructionKind::EnvironmentLookup
        })
        .collect::<Vec<_>>();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].span(), source.environment().expect("source requirement").call_span());
}

#[test]
fn pure_owned_control_flow_and_handle_commands_seal_without_host_effect() {
    for name in ["pure-entry", "owned-aggregates", "control-flow", "weak-upgrade"] {
        let (source, sources) = fixture(name);
        let program =
            lower(&source, &sources).unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
        assert!(program.verified_ir().source().environment().is_none());
        assert!(
            program
                .verified_ir()
                .modules()
                .flat_map(VerifiedModule::functions)
                .flat_map(VerifiedFunction::blocks)
                .flat_map(VerifiedBlock::instructions)
                .all(|instruction| instruction.kind()
                    != zryna_ir::data_ownership_v1::VerifiedInstructionKind::EnvironmentLookup)
        );
    }
}

#[test]
fn trailing_literal_call_and_owned_helper_composition_seal_one_source_effect() {
    for name in ["environment-trailing", "environment-helper"] {
        let (source, sources) = fixture(name);
        let program =
            lower(&source, &sources).unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
        let effects = program
            .verified_ir()
            .modules()
            .flat_map(VerifiedModule::functions)
            .flat_map(VerifiedFunction::blocks)
            .flat_map(VerifiedBlock::instructions)
            .filter(|instruction| {
                instruction.kind()
                    == zryna_ir::data_ownership_v1::VerifiedInstructionKind::EnvironmentLookup
            })
            .count();
        assert_eq!(effects, 1);
    }
}

#[test]
fn environment_preparation_seals_reverse_cleanup_of_existing_string_owners() {
    let (source, sources) = fixture("environment-live-prefix");
    let program = lower(&source, &sources).expect("live owned prefix before environment");
    let function =
        program.verified_ir().modules().next().expect("module").functions().next().expect("main");
    let effect = function
        .blocks()
        .flat_map(VerifiedBlock::instructions)
        .find(|instruction| {
            instruction.kind()
                == zryna_ir::data_ownership_v1::VerifiedInstructionKind::EnvironmentLookup
        })
        .expect("effect");
    let roots =
        effect.derived_drop_actions().map(|action| action.root().index()).collect::<Vec<_>>();
    assert_eq!(roots, [3, 1]);
    assert_eq!(
        effect
            .allocation_failure_drop_actions()
            .map(|action| action.root().index())
            .collect::<Vec<_>>(),
        roots
    );
}

#[test]
fn source_entry_and_outcome_semantic_violations_reject_deterministically() {
    for (name, code) in [
        ("entry-name", "ZRYNA-I4100"),
        ("entry-result", "ZRYNA-I4100"),
        ("entry-parameter", "ZRYNA-I4100"),
        ("entry-extra-export", "ZRYNA-I4100"),
        ("entry-missing-export", "ZRYNA-I4100"),
        ("environment-incomplete-match", "ZRYNA-M3009"),
        ("environment-invalid-binding", "ZRYNA-M3009"),
        ("environment-wrong-type", "ZRYNA-M3016"),
    ] {
        let (source, sources) = fixture(name);
        let errors = lower(&source, &sources).expect_err("invalid command semantics");
        assert!(errors.iter().any(|error| error.code() == code), "{name}: {errors:?}");
        assert_eq!(lower(&source, &sources).expect_err("deterministic rejection"), errors);
    }
    let (source, sources) = fixture("pure-entry");
    lower(&source, &sources).expect("recovery");
}

#[test]
fn environment_source_cannot_be_lowered_through_ordinary_m3_semantics() {
    let (source, sources) = fixture("environment-match");
    let input = crate::data_ownership_v1::SemanticInput::try_new(
        source.syntax(),
        &sources,
        sources.verify_file_id(0).expect("entry"),
    )
    .expect("ordinary authenticated syntax");
    assert!(crate::data_ownership_v1::lower(input).is_err());
    lower(&source, &sources).expect("command recovery");
}
