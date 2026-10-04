use super::{discover, produce_claim};
use crate::bounded_generics_v1::{
    SemanticInput, body_types::check_body_types, instantiation::layouts::verify_layouts,
    resolve_declarations, tests::body_fixtures::project,
};
use zryna_ir::generic_v1::{Failure, owned_v2};
use zryna_layout::StorageTarget;
const VALUES: &str = include_str!("../../../../../../tests/m7-generic-owned-fixtures/values.zry");
const MAIN: &str = include_str!("../../../../../../tests/m7-generic-owned-fixtures/main.zry");
pub(super) fn claim(files: &[(&str, &str)]) -> Result<owned_v2::raw::Program, Failure> {
    let p = project(files);
    let entry = p.sources.verify_file_id(0).expect("entry");
    let d =
        resolve_declarations(SemanticInput::try_new(&p.syntax, &p.sources, entry).expect("input"))
            .expect("declarations");
    let b = check_body_types(&d).expect("well-typed source before ownership");
    let i = discover(&b)?;
    let linear = verify_layouts(i.instances(), StorageTarget::Linear32V1).expect("linear");
    let linux = verify_layouts(i.instances(), StorageTarget::LinuxX8664V1).expect("linux");
    produce_claim(&i, &linear, &linux)
}
fn with_claim(
    files: &[(&str, &str)],
    test: impl FnOnce(
        owned_v2::raw::Program,
        &zryna_syntax::v5::VerifiedProjectSyntaxV5,
        &zryna_source::SourceMap,
        &zryna_layout::generic_v1::VerifiedLayouts,
        &zryna_layout::generic_v1::VerifiedLayouts,
    ),
) {
    if let Some(dir) = std::env::var_os("ZRYNA_GENERIC_OWNED_FIXTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::write(
            dir.join(format!("{}-reference.json", files.len())),
            serde_json::to_vec_pretty(&crate::bounded_generics_v1::tests::body_fixtures::snapshot(
                files,
            ))
            .expect("fixture DTO"),
        )
        .expect("explicit fixture output");
    }
    let p = project(files);
    let entry = p.sources.verify_file_id(0).expect("entry");
    let d =
        resolve_declarations(SemanticInput::try_new(&p.syntax, &p.sources, entry).expect("input"))
            .expect("declarations");
    let b = check_body_types(&d).expect("opaque bodies");
    let i = discover(&b).expect("original ownership then complete demand");
    let linear = verify_layouts(i.instances(), StorageTarget::Linear32V1).expect("linear");
    let linux = verify_layouts(i.instances(), StorageTarget::LinuxX8664V1).expect("linux");
    let claim = produce_claim(&i, &linear, &linux).expect("source-owned claims");
    test(claim, &p.syntax, &p.sources, &linear, &linux);
}
#[test]
fn genuine_owned_program_seals_shared_exclusive_payload_loans_and_complete_cleanup() {
    for cross in [false, true] {
        let joined = format!("{VALUES}\n{MAIN}");
        let imported = format!(
            "import {{identity,keep,make,discard,inspect,inspectMut}} from \"./values.zry\";\n{MAIN}"
        );
        let files = if cross {
            vec![("main.zry", imported.as_str()), ("values.zry", VALUES)]
        } else {
            vec![("main.zry", joined.as_str())]
        };
        with_claim(&files, |claim, syntax, sources, linear, linux| {
            let contract = zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux)
                .expect("declarations");
            let runtime =
                zryna_ownership_runtime_abi::generic_v1::verify_v1(contract, linear, linux)
                    .expect("runtime seal");
            let bytes = owned_v2::wire::encode(&claim).expect("v2 wire");
            let decoded = owned_v2::wire::decode(&bytes).expect("v2 decode");
            assert_eq!(decoded.claims(), &claim);
            let verified = owned_v2::verify(
                decoded,
                syntax,
                sources,
                sources.verify_file_id(0).expect("entry"),
                linear,
                linux,
                &runtime,
            )
            .expect("owned seal");
            assert_eq!(verified.scalar_abi().exports().len(), 9);
            assert!(
                claim
                    .plans
                    .iter()
                    .flat_map(|p| &p.steps)
                    .any(|s| s.failure && !s.cleanup.is_empty())
            );
            for mutation in [0, 1, 2] {
                let mut hostile = claim.clone();
                let step = hostile
                    .plans
                    .iter_mut()
                    .flat_map(|p| &mut p.steps)
                    .find(|s| !s.cleanup.is_empty())
                    .expect("owned cleanup");
                match mutation {
                    0 => {
                        step.cleanup.pop();
                    }
                    1 => step.cleanup.push(step.cleanup[0]),
                    _ => step.end_loans.push(u32::MAX),
                }
                let decoded = owned_v2::wire::decode(
                    &owned_v2::wire::encode(&hostile).expect("raw encoding"),
                )
                .expect("untrusted decode");
                assert!(matches!(
                    owned_v2::verify(
                        decoded,
                        syntax,
                        sources,
                        sources.verify_file_id(0).expect("entry"),
                        linear,
                        linux,
                        &runtime
                    ),
                    Err(Failure::Diagnostics(_))
                ));
            }
            operation_attacks(&claim, syntax, sources, linear, linux, &runtime);
            let verified = owned_v2::verify(
                owned_v2::wire::decode(&bytes).expect("pristine wire"),
                syntax,
                sources,
                sources.verify_file_id(0).expect("entry"),
                linear,
                linux,
                &runtime,
            )
            .expect("pristine replay after every raw attack");
            assert_eq!(verified.scalar_abi().exports().len(), 9);
        });
    }
}

fn operation_attacks(
    claim: &owned_v2::raw::Program,
    syntax: &zryna_syntax::v5::VerifiedProjectSyntaxV5,
    sources: &zryna_source::SourceMap,
    linear: &zryna_layout::generic_v1::VerifiedLayouts,
    linux: &zryna_layout::generic_v1::VerifiedLayouts,
    runtime: &zryna_ownership_runtime_abi::generic_v1::VerifiedOwnershipRuntimeAbi<'_>,
) {
    for mutation in [0, 1, 2, 3] {
        let mut hostile = claim.clone();
        let operation = hostile
            .extensions
            .iter_mut()
            .flatten()
            .find(|e| match mutation {
                0 => matches!(e.operation, owned_v2::raw::Operation::StringLiteral(_)),
                1 => matches!(e.operation, owned_v2::raw::Operation::Move(_)),
                2 => matches!(e.operation, owned_v2::raw::Operation::Borrow { .. }),
                _ => matches!(e.operation, owned_v2::raw::Operation::EndLoan(_)),
            })
            .expect("real owned operation");
        match &mut operation.operation {
            owned_v2::raw::Operation::StringLiteral(bytes) => bytes.push(b'x'),
            owned_v2::raw::Operation::Move(id) | owned_v2::raw::Operation::EndLoan(id) => {
                *id = u32::MAX;
            }
            owned_v2::raw::Operation::Borrow { exclusive, .. } => *exclusive = !*exclusive,
            _ => panic!("selected raw mutation"),
        }
        let decoded =
            owned_v2::wire::decode(&owned_v2::wire::encode(&hostile).expect("untrusted wire"))
                .expect("structural decoding");
        assert!(
            owned_v2::verify(
                decoded,
                syntax,
                sources,
                sources.verify_file_id(0).expect("entry"),
                linear,
                linux,
                runtime
            )
            .is_err()
        );
    }
}
