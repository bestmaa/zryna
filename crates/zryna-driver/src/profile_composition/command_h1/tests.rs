mod rejection;

use super::*;
use zryna_source::SourceFileInput;
use zryna_syntax::{command_h1_v1, v4};

struct Candidate {
    program: VerifiedProgram,
    artifact: ValidatedCommandH1Artifact,
    sources: SourceMap,
}

impl Candidate {
    fn new(name: &str) -> Self {
        Self::with_key(name, "MODE")
    }

    fn with_key(name: &str, key: &str) -> Self {
        assert_eq!(key.len(), 4, "fixture replacement preserves authenticated byte spans");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/wasi-command-source-fixtures");
        let text = std::fs::read_to_string(root.join(format!("{name}.zry")))
            .expect("source fixture")
            .replace("MODE", key);
        let bytes = std::fs::read_to_string(root.join(format!("{name}.json")))
            .expect("provider fixture")
            .replace("MODE", key);
        let sources =
            SourceMap::build(vec![SourceFileInput { path: "src/main.zry".to_owned(), text }])
                .expect("source authority");
        let syntax = v4::verify_snapshot(
            v4::decode_snapshot(bytes.as_bytes()).expect("provider decode"),
            &sources,
        )
        .expect("source-bound v4 syntax");
        let source = command_h1_v1::admit(&syntax, &sources).expect("complete source admission");
        let program = zryna_semantics::command_h1_v1::lower(&source, &sources)
            .expect("actual semantic lowering and mandatory verification");
        let artifact = ValidatedCommandH1Artifact::emit(
            program.verified_ir(),
            program.runtime_abi(),
            &sources,
            &pinned_wit_sources(),
        )
        .expect("actual independently audited factory");
        Self { program, artifact, sources }
    }

    fn key(&self) -> Option<&str> {
        self.program
            .verified_ir()
            .source()
            .environment()
            .map(command_h1_v1::EnvironmentRequirement::key)
    }

    fn admit(&self) -> CommandH1Composition {
        CommandH1Composition::admit(&self.program, &self.artifact, &self.sources, self.key())
            .expect("sealed command composition")
    }

    fn reject_input(&self, edit: impl FnOnce(&mut Input)) {
        let result = self.admit();
        let mut input = result.input.clone();
        edit(&mut input);
        let claim = claim(&input, &result.authorities, &result.composition);
        let first = verify(&input, &result.authorities, &claim).expect_err("mutant rejected");
        assert_eq!(first[0].code(), INVALID);
        assert_eq!(first, verify(&input, &result.authorities, &claim).expect_err("repeat"));
        result
            .revalidate(&self.program, &self.artifact, &self.sources, self.key())
            .expect("next valid composition recovers");
    }
}

fn claim(input: &Input, authorities: &Authorities, result: &ValidatedComposition) -> Claim {
    let graph = graph::validate(input).expect("well-formed graph mutant");
    Claim {
        binding: graph
            .binding(&authorities.binding(&graph.ids()).expect("real authorities"))
            .expect("current content binding"),
        summaries: result.summaries.clone(),
        witnesses: result.witnesses.clone(),
    }
}

#[test]
fn real_pure_and_environment_semantics_have_distinct_static_reservations() {
    for (name, key, count, bytes) in [
        ("pure-entry", None, 0, 0),
        ("environment-match", Some("MODE"), 1, 1088),
        ("environment-helper", Some("MODE"), 1, 1088),
    ] {
        let candidate = Candidate::new(name);
        let result = candidate.admit();
        assert_eq!(result.required_key(), key);
        assert_eq!((result.quota()[2], result.quota()[3]), (count, bytes));
        assert!(result.input.instances[0].reservation.environment.is_empty());
        assert_eq!(result.composition.requirements().len(), usize::from(key.is_some()));
        result
            .revalidate(&candidate.program, &candidate.artifact, &candidate.sources, key)
            .expect("retained semantic issuer revalidates");
    }
}

#[test]
fn source_key_equal_sign_and_multibyte_bytes_do_not_enter_legacy_environment_values() {
    for key in ["M=DE", "é=E"] {
        let candidate = Candidate::with_key("environment-match", key);
        let result = candidate.admit();
        assert_eq!(result.required_key(), Some(key));
        assert_eq!(result.quota()[3], 1088);
        assert!(result.input.instances[0].reservation.environment.is_empty());
        assert!(
            super::super::quota::validate_reservation(&Reservation {
                environment: BTreeMap::from([(key.to_owned(), String::new())]),
                ..Reservation::default()
            })
            .is_err()
        );
    }
}

#[test]
fn equal_content_new_semantic_issuer_cannot_revalidate_retained_composition() {
    let candidate = Candidate::new("environment-match");
    let result = candidate.admit();
    let source = candidate.program.verified_ir().source();
    let fresh = zryna_semantics::command_h1_v1::lower(source, &candidate.sources)
        .expect("fresh real semantics on the same authenticated map");
    let artifact = ValidatedCommandH1Artifact::emit(
        fresh.verified_ir(),
        fresh.runtime_abi(),
        &candidate.sources,
        &pinned_wit_sources(),
    )
    .expect("fresh factory");
    assert_eq!(artifact.program_binding(), candidate.artifact.program_binding());
    assert_ne!(artifact.issuer(), candidate.artifact.issuer());
    assert!(result.revalidate(&fresh, &artifact, &candidate.sources, candidate.key()).is_err());
    assert_eq!(result.identity(), Candidate::new("environment-match").admit().identity());
    result
        .revalidate(&candidate.program, &candidate.artifact, &candidate.sources, candidate.key())
        .expect("original issuer recovers");
}

#[test]
fn legacy_i32_binding_debug_and_digest_representation_remain_exact() {
    let text = "export function value(): i32 { return 1; }";
    let sources =
        SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: text.into() }])
            .expect("legacy source map");
    let span = sources
        .span(
            sources.verify_file_id(0).expect("file"),
            0,
            u32::try_from(text.len()).expect("source extent"),
        )
        .expect("bound span");
    let program = zryna_ir::verify(
        zryna_ir::Program {
            functions: vec![zryna_ir::Function {
                name: "value".into(),
                parameters: Vec::new(),
                return_type: zryna_ir::Type::I32,
                expressions: vec![zryna_ir::Expr {
                    ty: zryna_ir::Type::I32,
                    span,
                    kind: zryna_ir::ExprKind::I32Literal(1),
                }],
                body: zryna_ir::ExprId(0),
            }],
        },
        &sources,
    )
    .expect("real legacy verifier");
    let mut fingerprint = Vec::new();
    for function in program.functions() {
        fingerprint.extend_from_slice(function.export_name().as_str().as_bytes());
        fingerprint.push(0);
        fingerprint.extend_from_slice(format!("{:?}", function.parameters()).as_bytes());
        fingerprint.extend_from_slice(format!("{:?}", function.return_type()).as_bytes());
        fingerprint.extend_from_slice(&serde_json::to_vec(function.expressions()).expect("IR"));
        fingerprint.extend_from_slice(format!("{:?}", function.body()).as_bytes());
    }
    let program_digest: [u8; 32] = Sha256::digest(fingerprint).into();
    let source_digest: [u8; 32] =
        Sha256::digest([b"main.zry\0".as_slice(), text.as_bytes(), &[0xff]].concat()).into();
    let mut input = input(&BTreeSet::new(), [0; 10]);
    input.language = Language::I32V1;
    let authorities = Authorities {
        instances: BTreeMap::from([(
            ROOT.to_owned(),
            InstanceAuthority { programs: vec![VerifiedLanguage::I32V1 { program, sources }] },
        )]),
        wit: None,
    };
    let graph = graph::validate(&input).expect("legacy graph");
    let binding = authorities.binding(&graph.ids()).expect("legacy binding");
    let old_instances =
        vec![(ROOT.to_owned(), vec![(Language::I32V1, source_digest, program_digest)])];
    let old_debug =
        format!("Binding {{ instances: {old_instances:?}, wit: {:?} }}", authorities.wit);
    assert_eq!(format!("{binding:?}"), old_debug);
    let mut bytes = serde_json::to_vec(&graph.input).expect("historical input encoding");
    bytes.extend_from_slice(old_debug.as_bytes());
    let old_digest: [u8; 32] = Sha256::digest(bytes).into();
    assert_eq!(graph.binding(&binding).expect("legacy digest"), old_digest);
}
