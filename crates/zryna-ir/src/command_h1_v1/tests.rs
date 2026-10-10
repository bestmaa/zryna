mod fixture;
mod ownership;
mod source_binding;

use super::raw;
use fixture::Fixture;

fn reject(fixture: &Fixture, program: raw::Program, code: &str) {
    let errors = fixture.check(program.clone()).expect_err("independent malformed command");
    assert!(errors.iter().any(|error| error.code() == code), "expected {code}: {errors:?}");
    assert_eq!(fixture.check(program).expect_err("deterministic rejection"), errors);
    fixture.check(fixture.seed()).expect("valid recovery");
}

#[test]
fn exact_environment_match_seals_dynamic_variant_and_payload_cleanup() {
    let fixture = Fixture::new();
    let verified = fixture.check(fixture.seed()).expect("independent complete command IR");
    assert_eq!(verified.source().environment().expect("effect").key(), "MODE");
    let function = verified.modules().next().expect("module").functions().next().expect("function");
    let blocks = function.blocks().collect::<Vec<_>>();
    assert_eq!(blocks.len(), 3);
    assert_eq!(
        blocks[0].instructions().next().expect("host operation").kind(),
        crate::data_ownership_v1::VerifiedInstructionKind::EnvironmentLookup
    );
    let dropped = blocks[1]
        .instructions()
        .flat_map(crate::data_ownership_v1::VerifiedInstruction::derived_drop_actions)
        .find(|action| action.root().index() == 0)
        .expect("partially consumed Found root");
    assert_eq!(dropped.active_variant(), Some(0));
    assert_eq!(dropped.moved_projections().next().expect("String payload move").index(), 1);
}

#[test]
fn ordinary_m3_admission_rejects_exact_command_effect() {
    let fixture = Fixture::new();
    let mut program = fixture.seed();
    program.authorities.runtime =
        crate::data_ownership_v1::RuntimeContractIdentity::OwnershipRuntimeV1;
    let errors = crate::data_ownership_v1::verify(
        program,
        &fixture.sources,
        fixture.sources.verify_file_id(0).expect("file"),
        fixture.linear.clone(),
        fixture.linux.clone(),
    )
    .expect_err("ordinary M3 cannot admit host authority");
    assert!(errors.iter().any(|error| error.code() == "ZRYNA-I4100"));
}

#[test]
fn command_runtime_identity_cannot_be_replaced_by_ordinary_ownership_identity() {
    let fixture = Fixture::new();
    let mut program = fixture.seed();
    program.authorities.runtime =
        crate::data_ownership_v1::RuntimeContractIdentity::OwnershipRuntimeV1;
    reject(&fixture, program, "ZRYNA-I3003");
    let errors = crate::data_ownership_v1::verify(
        fixture.seed(),
        &fixture.sources,
        fixture.sources.verify_file_id(0).expect("file"),
        fixture.linear.clone(),
        fixture.linux.clone(),
    )
    .expect_err("ordinary admission cannot accept the command runtime identity");
    assert!(errors.iter().any(|error| error.code() == "ZRYNA-I3003"));
}

#[test]
fn missing_duplicate_substituted_and_same_span_wrong_key_effects_reject() {
    let fixture = Fixture::new();
    let mut missing = fixture.seed();
    missing.modules[0].functions[0].blocks[0].instructions.clear();
    reject(&fixture, missing, "ZRYNA-I4100");
    let mut duplicate = fixture.seed();
    let instruction = duplicate.modules[0].functions[0].blocks[0].instructions[0].clone();
    duplicate.modules[0].functions[0].blocks[0].instructions.push(instruction);
    reject(&fixture, duplicate, "ZRYNA-I4100");
    let mut wrong_key = fixture.seed();
    let raw::InstructionKind::EnvironmentLookup { key, .. } =
        &mut wrong_key.modules[0].functions[0].blocks[0].instructions[0].kind
    else {
        unreachable!()
    };
    *key = "OTHER".into();
    reject(&fixture, wrong_key, "ZRYNA-I4100");
    let mut constructed = fixture.seed();
    constructed.modules[0].functions[0].blocks[0].instructions[0].kind =
        raw::InstructionKind::EnumConstruct { variant: 1, payload: None, cleanup: None };
    reject(&fixture, constructed, "ZRYNA-I4100");
}

#[test]
fn entry_module_declaration_and_dual_layout_substitution_reject() {
    let fixture = Fixture::new();
    let mut entry = fixture.seed();
    entry.modules[0].functions[0].entry_export = Some("other".into());
    reject(&fixture, entry, "ZRYNA-I4100");
    let mut declarations = fixture.seed();
    declarations.modules[0].data_declarations = 0;
    reject(&fixture, declarations, "ZRYNA-I3003");
    let mut functions = fixture.seed();
    let duplicate = functions.modules[0].functions[0].clone();
    functions.modules[0].functions.push(duplicate);
    reject(&fixture, functions, "ZRYNA-I4100");
    let mut graph = fixture.graph.clone();
    let zryna_layout::raw::TypeKind::Enum { variants, .. } = &mut graph.types[3].kind else {
        unreachable!()
    };
    variants[0].payload = Some(zryna_layout::raw::NodeId(1));
    let (linear, linux) = Fixture::layouts(&graph, &fixture.sources);
    let mut program = fixture.seed();
    program.authorities.type_universe = linear.universe_identity().as_bytes();
    program.authorities.linear32_fingerprint = *linear.fingerprint();
    program.authorities.linux_x86_64_fingerprint = *linux.fingerprint();
    let errors = super::verify(program, &fixture.sources, &fixture.source, linear, linux)
        .expect_err("self-consistent hostile outcome layouts");
    assert!(errors.iter().any(|error| error.code() == "ZRYNA-I4100"), "{errors:?}");
}
