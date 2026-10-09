use super::Artifact;
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::{
    command_h1_v1::{self, CommandSyntax},
    v4,
};

fn source(name: &str) -> (SourceMap, CommandSyntax) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/wasi-command-source-fixtures");
    let text = std::fs::read_to_string(root.join(format!("{name}.zry"))).expect("source");
    let provider = std::fs::read(root.join(format!("{name}.json"))).expect("provider");
    let sources = SourceMap::build(vec![SourceFileInput { path: "src/main.zry".into(), text }])
        .expect("source map");
    let syntax = v4::verify_snapshot(v4::decode_snapshot(&provider).expect("decode"), &sources)
        .expect("syntax");
    let source = command_h1_v1::admit(&syntax, &sources).expect("whole source");
    (sources, source)
}

#[test]
fn immutable_artifact_binding_is_deterministic_across_fresh_issuers_and_wit_input_order() {
    let (sources, source) = source("environment-match");
    let first =
        zryna_semantics::command_h1_v1::lower(&source, &sources).expect("first verified body");
    let second =
        zryna_semantics::command_h1_v1::lower(&source, &sources).expect("fresh verified body");
    assert_ne!(first.verified_ir().identity(), second.verified_ir().identity());
    let mut wit = crate::pinned_wit_sources();
    let first = Artifact::emit(first.verified_ir(), first.runtime_abi(), &sources, &wit)
        .expect("first complete artifact");
    wit.reverse();
    let second = Artifact::emit(second.verified_ir(), second.runtime_abi(), &sources, &wit)
        .expect("fresh complete artifact");
    assert_ne!(first.issuer(), second.issuer());
    assert_eq!(first.bytes(), second.bytes());
    assert_eq!(first.program_binding(), second.program_binding());
    assert_eq!(first.world_digest(), second.world_digest());
    assert_eq!(first.wit_closure_digest(), second.wit_closure_digest());
}

#[test]
fn complete_artifact_rejects_stale_source_and_foreign_runtime_then_recovers() {
    let (sources, source) = source("environment-match");
    let command = zryna_semantics::command_h1_v1::lower(&source, &sources).expect("whole body");
    let (equal_bytes, _) = self::source("environment-match");
    let (pure_sources, pure) = self::source("pure-entry");
    let pure = zryna_semantics::command_h1_v1::lower(&pure, &pure_sources).expect("pure body");
    let wit = crate::pinned_wit_sources();
    assert!(
        Artifact::emit(command.verified_ir(), command.runtime_abi(), &equal_bytes, &wit).is_err()
    );
    assert!(Artifact::emit(command.verified_ir(), pure.runtime_abi(), &sources, &wit).is_err());
    let mut invalid_wit = wit.clone();
    invalid_wit.pop();
    assert!(
        Artifact::emit(command.verified_ir(), command.runtime_abi(), &sources, &invalid_wit)
            .is_err()
    );
    Artifact::emit(command.verified_ir(), command.runtime_abi(), &sources, &wit)
        .expect("next valid artifact");
}

#[test]
fn revalidation_requires_the_exact_retained_issuer_and_all_content_observations() {
    let (sources, source) = source("environment-match");
    let command = zryna_semantics::command_h1_v1::lower(&source, &sources).expect("command");
    let fresh = zryna_semantics::command_h1_v1::lower(&source, &sources).expect("fresh issuer");
    let wit = crate::pinned_wit_sources();
    let mut artifact = Artifact::emit(command.verified_ir(), command.runtime_abi(), &sources, &wit)
        .expect("opaque factory");
    artifact
        .revalidate(command.verified_ir(), command.runtime_abi(), &sources, &wit)
        .expect("retained issuer");
    assert!(artifact.revalidate(fresh.verified_ir(), fresh.runtime_abi(), &sources, &wit).is_err());
    artifact.source_digest[0] ^= 1;
    assert!(
        artifact.revalidate(command.verified_ir(), command.runtime_abi(), &sources, &wit).is_err()
    );
    artifact.source_digest[0] ^= 1;
    artifact.bytes.push(0);
    assert!(
        artifact.revalidate(command.verified_ir(), command.runtime_abi(), &sources, &wit).is_err()
    );
    artifact.bytes.pop();
    artifact
        .revalidate(command.verified_ir(), command.runtime_abi(), &sources, &wit)
        .expect("valid factory observation recovery");
}
