use super::{
    BTreeMap, INVALID, Language, ValidatedNativeComposition, VerifiedLanguage, fixture,
    rejects_emit, verify,
};

#[test]
fn h1_without_wit_rejects_before_native_proof_and_on_revalidation() {
    let (mut input, mut pure, native) = fixture();
    let seal = verify(&input, &pure, &native).expect("unchanged I32 native composition");
    let command = crate::profile_composition::command_h1::tests::pure_command_authorities();
    assert!(command.wit.is_some(), "genuine command issuer retains its audited world");
    let instance = command.instances.into_values().next().expect("sole genuine command");
    pure.instances.values_mut().for_each(|authority| *authority = instance.clone());
    pure.wit = None;
    input.language = Language::CommandH1V1;
    // Every pure instance has a genuine, source-bound H1 authority; the native graph,
    // C issuer, rows and empty effects remain exactly the valid native fixture.
    pure.binding(&pure.instances.keys().cloned().collect()).expect("genuine H1 bindings");
    let errors = verify(&input, &pure, &native).expect_err("H1 cannot construct native proof");
    assert_eq!(errors[0].code(), INVALID);
    assert_eq!(
        seal.revalidate(&input, &pure, &native).expect_err("same rejection before proof replay"),
        errors,
    );
    rejects_emit(&seal, &input, &pure, &native, INVALID);
}

#[test]
fn i32_source_identities_and_native_composition_remain_exact() {
    let (input, pure, native) = fixture();
    let expected: BTreeMap<_, _> = pure
        .instances
        .iter()
        .map(|(id, authority)| {
            let VerifiedLanguage::I32V1 { sources, .. } = &authority.programs[0] else {
                panic!("fixture contains genuine I32 authority")
            };
            (id.clone(), vec![sources.identity()])
        })
        .collect();
    assert_eq!(pure.source_identities().expect("I32 identities"), expected);
    assert_eq!(pure.clone().source_identities().expect("retained clone identities"), expected);
    let seal: ValidatedNativeComposition =
        verify(&input, &pure, &native).expect("I32 native proof");
    assert_eq!(seal.pure_sources, expected);
    seal.revalidate(&input, &pure, &native).expect("unchanged I32 replay");
}
