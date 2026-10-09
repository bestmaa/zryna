use super::*;

#[test]
fn direct_requirement_cannot_be_omitted_added_or_retyped_before_derivation() {
    let candidate = Candidate::new("environment-match");
    candidate.reject_input(|input| input.instances[0].requirements.clear());
    candidate.reject_input(|input| {
        input.instances[0].requirements.insert(Requirement {
            capability: Capability::Clock,
            interface: "wasi:clocks/monotonic-clock@0.2.12".into(),
        });
    });
    candidate.reject_input(|input| {
        input.instances[0].requirements = BTreeSet::from([Requirement {
            capability: Capability::Environment,
            interface: "wasi:cli/environment@0.2.13".into(),
        }]);
    });
    candidate.reject_input(|input| {
        input.instances[0].requirements = BTreeSet::from([Requirement {
            capability: Capability::Clock,
            interface: ENVIRONMENT.into(),
        }]);
    });
    Candidate::new("pure-entry").reject_input(|input| {
        input.instances[0].requirements.insert(environment());
    });
}

#[test]
fn current_root_key_approval_is_separate_from_environment_interface_approval() {
    let candidate = Candidate::new("environment-match");
    for key in [None, Some("MORE"), Some("")] {
        let error = CommandH1Composition::admit(
            &candidate.program,
            &candidate.artifact,
            &candidate.sources,
            key,
        )
        .err()
        .expect("wrong current root key rejects");
        assert_eq!(error[0].code(), INVALID);
        assert!(
            candidate
                .admit()
                .revalidate(&candidate.program, &candidate.artifact, &candidate.sources, key)
                .is_err()
        );
    }
    candidate.reject_input(|input| input.selections[0].approved.clear());
    candidate.reject_input(|input| input.instances[0].restrictions.clear());
    let pure = Candidate::new("pure-entry");
    assert!(
        CommandH1Composition::admit(&pure.program, &pure.artifact, &pure.sources, Some("MODE"))
            .is_err()
    );
    candidate.admit();
}

#[test]
fn stale_source_and_foreign_factory_or_semantic_issuer_reject() {
    let candidate = Candidate::new("environment-match");
    let foreign = Candidate::new("environment-match");
    assert!(
        CommandH1Composition::admit(
            &candidate.program,
            &candidate.artifact,
            &foreign.sources,
            candidate.key()
        )
        .is_err()
    );
    assert!(
        CommandH1Composition::admit(
            &candidate.program,
            &foreign.artifact,
            &candidate.sources,
            candidate.key()
        )
        .is_err()
    );
    assert!(
        CommandH1Composition::admit(
            &foreign.program,
            &candidate.artifact,
            &foreign.sources,
            candidate.key()
        )
        .is_err()
    );
    let changed = Candidate::with_key("environment-match", "MORE");
    assert!(
        candidate
            .admit()
            .revalidate(&changed.program, &changed.artifact, &changed.sources, changed.key())
            .is_err()
    );
    candidate.admit();
}

#[test]
fn different_row_world_policy_or_language_reject() {
    let candidate = Candidate::new("environment-match");
    candidate.reject_input(|input| input.selections[0].row = Row::WitServer);
    candidate.reject_input(|input| {
        input.instances[0].rows.insert(Row::WitServer);
    });
    candidate.reject_input(|input| {
        input.selections[0].world = Some("zryna:capability-profiles/server@0.1.0".into());
    });
    candidate.reject_input(|input| input.selections[0].policy_version.push_str(".changed"));
    candidate.reject_input(|input| input.language = Language::DataOwnershipV1);
}

#[test]
fn static_reservation_cannot_be_replaced_by_value_entries_or_expanded_ceilings() {
    let candidate = Candidate::new("environment-match");
    candidate.reject_input(|input| {
        input.instances[0].reservation.environment.insert("MODE".into(), String::new());
    });
    for (metric, amount) in [(2, 0), (2, 2), (3, 1087), (3, 1089), (0, 1)] {
        candidate.reject_input(|input| input.selections[0].ceilings[metric] = amount);
    }
    let result = candidate.admit();
    let mut forged = claim(&result.input, &result.authorities, &result.composition);
    for (metric, amount) in [(2, 0), (2, 2), (3, 1087), (3, 1089)] {
        forged.summaries.get_mut(ROOT).expect("summary").quota[metric] = amount;
        assert!(verify(&result.input, &result.authorities, &forged).is_err());
        forged = claim(&result.input, &result.authorities, &result.composition);
    }
    candidate.admit();
}

#[test]
fn absent_wit_and_duplicate_semantic_authorities_reject() {
    let candidate = Candidate::new("environment-match");
    let result = candidate.admit();
    let mut authorities = result.authorities.clone();
    authorities.wit = None;
    let claim = claim(&result.input, &authorities, &result.composition);
    let error = verify(&result.input, &authorities, &claim).expect_err("missing WIT rejects");
    assert_eq!(error[0].code(), super::super::super::UNSUPPORTED);
    let mut authorities = result.authorities.clone();
    let duplicate = authorities.instances[ROOT].programs[0].clone();
    authorities.instances.get_mut(ROOT).expect("root").programs.push(duplicate);
    assert!(authorities.binding(&BTreeSet::from([ROOT.to_owned()])).is_err());
    candidate.admit();
}

#[test]
fn changed_retained_source_or_key_metadata_cannot_replace_whole_semantic_source() {
    let candidate = Candidate::new("environment-match");
    let foreign = Candidate::new("environment-match");
    let result = candidate.admit();
    for mutation in 0..3 {
        let mut authorities = result.authorities.clone();
        let VerifiedLanguage::CommandH1V1(command) =
            &mut authorities.instances.get_mut(ROOT).expect("root").programs[0]
        else {
            panic!("command authority")
        };
        match mutation {
            0 => command.sources = foreign.sources.clone(),
            1 => {
                command.binding.key = Some("MORE".into());
                command.binding.approved_key = Some("MORE".into());
            }
            _ => command.binding.issuer = foreign.program.verified_ir().identity(),
        }
        let error = authorities
            .binding(&BTreeSet::from([ROOT.to_owned()]))
            .expect_err("immutable whole semantic source rejects metadata replacement");
        assert_eq!(error, invalid());
    }
    result
        .revalidate(&candidate.program, &candidate.artifact, &candidate.sources, candidate.key())
        .expect("retained unmodified authority recovers");
}
