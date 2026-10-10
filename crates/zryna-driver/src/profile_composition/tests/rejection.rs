use super::*;

#[test]
fn independent_hostile_shapes_reject_without_summary_authority() {
    let valid = input(&["A", "B", "C"], &[("A", "B"), ("B", "C")]);
    let claim = pure_claim(&valid);
    let mut cases = Vec::new();
    let mut bad = valid.clone();
    bad.edges.push(("C".to_owned(), "A".to_owned()));
    cases.push(bad);
    let mut bad = valid.clone();
    bad.edges.push(("A".to_owned(), "missing".to_owned()));
    cases.push(bad);
    let mut bad = valid.clone();
    bad.edges.push(bad.edges[0].clone());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.instances.push(bad.instances[0].clone());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.instances.push(node("orphan"));
    cases.push(bad);
    let mut bad = valid.clone();
    bad.version.push('2');
    cases.push(bad);
    let mut bad = valid.clone();
    bad.selections.push(bad.selections[0].clone());
    cases.push(bad);
    for bad in cases {
        rejected(&bad, &claim, INVALID);
    }
    let mut encoded = serde_json::to_value(&valid).expect("input JSON");
    encoded["effectGrant"] = true.into();
    assert!(serde_json::from_value::<Input>(encoded).is_err());
    let mut encoded = serde_json::to_value(&valid).expect("input JSON");
    encoded["language"] = "FutureProfile".into();
    assert!(serde_json::from_value::<Input>(encoded).is_err());
}

#[test]
fn grandchild_denial_and_pure_parent_retain_exact_witness_and_policy() {
    let (mut input, claim) = diamond();
    input.instances[1].restrictions.clear();
    let diagnostics = rejected(&input, &claim, FORBIDDEN);
    assert!(diagnostics[0].message().contains("rejecting instance B"));
    assert!(diagnostics[0].message().contains("A -> B -> D"));
    assert!(diagnostics[0].message().contains(model::POLICY));
    input.instances[1].restrictions.insert(Capability::Clock);
    input.selections[0].approved.clear();
    rejected(&input, &claim, FORBIDDEN);
    input.selections = vec![selection(Row::UniversalJavaScript)];
    rejected(&input, &claim, FORBIDDEN);
}

#[test]
fn profile_world_and_interface_mismatches_precede_capability_checks() {
    let (mut input, claim) = diamond();
    input.language = Language::DataOwnershipV1;
    rejected(&input, &claim, PROFILE);
    input.selections[0].world = Some("zryna:capability-profiles/command@0.2.0".to_owned());
    rejected(&input, &claim, UNSUPPORTED);
    input.selections[0] = selection(Row::WitServer);
    input.instances[3].requirements = BTreeSet::from([Requirement {
        capability: Capability::Network,
        interface: "wasi:sockets/tcp@0.2.12".to_owned(),
    }]);
    rejected(&input, &claim, UNSUPPORTED);
    input.instances[3].requirements = BTreeSet::from([Requirement {
        capability: Capability::Clock,
        interface: "wasi:clocks/monotonic-clock@0.2.13".to_owned(),
    }]);
    rejected(&input, &claim, UNSUPPORTED);
}

#[test]
fn forged_summaries_quotas_witnesses_and_binding_reject_independently() {
    let (input, claim) = diamond();
    let mut cases = Vec::new();
    let mut bad = claim.clone();
    bad.binding[0] ^= 1;
    cases.push(bad);
    let mut bad = claim.clone();
    bad.summaries.remove("C");
    cases.push(bad);
    let mut bad = claim.clone();
    bad.summaries.insert("orphan".to_owned(), Summary::default());
    cases.push(bad);
    let mut bad = claim.clone();
    bad.summaries.get_mut("A").expect("root").requirements.clear();
    cases.push(bad);
    let mut bad = claim.clone();
    bad.summaries.get_mut("B").expect("B").quota[0] = 0;
    cases.push(bad);
    let mut bad = claim.clone();
    bad.summaries.get_mut("A").expect("root").quota[0] = 2;
    cases.push(bad);
    let mut bad = claim.clone();
    bad.witnesses.clear();
    cases.push(bad);
    let mut bad = claim.clone();
    bad.witnesses.insert(clock(), vec!["A".to_owned(), "C".to_owned(), "D".to_owned()]);
    cases.push(bad);
    let mut bad = claim.clone();
    bad.witnesses.insert(clock(), vec!["A".to_owned(), "D".to_owned()]);
    cases.push(bad);
    for bad in cases {
        rejected(&input, &bad, INVALID);
    }
}

#[test]
fn replay_binds_sources_profiles_edges_approval_and_host_narrowing() {
    let (input, claim) = diamond();
    let result = verify(&input, &claim).expect("initial validation");
    let mut cases = Vec::new();
    let mut bad = input.clone();
    bad.language = Language::ControlFlowV1;
    cases.push(bad);
    let mut bad = input.clone();
    bad.selections[0] = selection(Row::WitServer);
    bad.selections[0].approved.insert(clock());
    cases.push(bad);
    let mut bad = input.clone();
    bad.language = Language::DataOwnershipV1;
    cases.push(bad);
    let mut bad = input.clone();
    bad.edges.pop();
    cases.push(bad);
    let mut bad = input.clone();
    bad.selections[0].approved.clear();
    cases.push(bad);
    let mut bad = input.clone();
    bad.selections[0].ceilings[0] = 0;
    cases.push(bad);
    let mut bad = input.clone();
    bad.selections[0].policy_version.push('2');
    cases.push(bad);
    let mut bad = input.clone();
    bad.instances[0].restrictions.clear();
    cases.push(bad);
    let mut bad = input.clone();
    bad.instances[3].reservation.timers = 1;
    cases.push(bad);
    for bad in cases {
        assert!(result.revalidate(&bad, &authorities(&bad)).is_err());
        assert!(verify(&bad, &claim).is_err());
        assert!(result.revalidate(&input, &authorities(&input)).is_ok());
    }
}

#[test]
fn sealed_program_source_and_world_authorities_cannot_be_forged_or_omitted() {
    let input = input(&["A"], &[]);
    let claim = pure_claim(&input);
    let valid = authorities(&input);
    let result = super::super::verify(&input, &valid, &claim).expect("sealed authority");

    let mut replaced = authorities(&input);
    replaced.instances.get_mut("A").expect("instance").programs =
        vec![verified_language("replacement")];
    assert_eq!(
        super::super::verify(&input, &replaced, &claim).expect_err("replaced program")[0].code(),
        INVALID
    );
    assert!(result.revalidate(&input, &replaced).is_err());

    let VerifiedLanguage::I32V1 { program, .. } = verified_language("first") else {
        panic!("legacy scalar fixture");
    };
    let VerifiedLanguage::I32V1 { sources, .. } = verified_language("second") else {
        panic!("legacy scalar fixture");
    };
    let mut mismatched = authorities(&input);
    mismatched.instances.get_mut("A").expect("instance").programs =
        vec![VerifiedLanguage::I32V1 { program, sources }];
    assert_eq!(
        super::super::verify(&input, &mismatched, &claim).expect_err("program/source mismatch")[0]
            .code(),
        INVALID
    );

    let (wit_input, wit_claim) = diamond();
    let mut missing_wit = authorities(&wit_input);
    missing_wit.wit = None;
    assert_eq!(
        super::super::verify(&wit_input, &missing_wit, &wit_claim).expect_err("missing WIT audit")
            [0]
        .code(),
        UNSUPPORTED
    );
}

#[test]
fn sealed_program_cardinality_rejects_before_program_fingerprinting() {
    let input = input(&["A"], &[]);
    let expected = BTreeSet::from(["A".to_owned()]);
    let mut authorities = authorities(&input);
    authorities
        .instances
        .get_mut("A")
        .expect("instance")
        .programs
        .extend([verified_language("second"), verified_language("third")]);
    assert_eq!(
        authorities.binding(&expected).expect_err("duplicate exact-bound authorities")[0].message(),
        "instance requires one distinct sealed authority per language"
    );

    let VerifiedLanguage::I32V1 { program, .. } = verified_language("mismatched program") else {
        panic!("legacy scalar fixture");
    };
    let VerifiedLanguage::I32V1 { sources, .. } = verified_language("mismatched source") else {
        panic!("legacy scalar fixture");
    };
    authorities
        .instances
        .get_mut("A")
        .expect("instance")
        .programs
        .push(VerifiedLanguage::I32V1 { program, sources });
    let diagnostics = authorities.binding(&expected).expect_err("four sealed authorities");
    assert_eq!(diagnostics[0].code(), INVALID);
    assert_eq!(
        diagnostics[0].message(),
        "instance sealed program authority bound (1..=3) exceeded"
    );
}
