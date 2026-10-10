use super::{BTreeMap, Candidate, ROOT, VerifiedLanguage, invalid};

#[test]
fn collector_retains_revalidated_h1_source_identity() {
    for name in ["pure-entry", "environment-match"] {
        let candidate = Candidate::new(name);
        let composition = candidate.admit();
        assert_eq!(
            composition.authorities.source_identities().expect("revalidated H1 source"),
            BTreeMap::from([(ROOT.to_owned(), vec![candidate.sources.identity()])]),
        );
    }
}

#[test]
fn collector_propagates_h1_source_validation_errors_without_partial_results() {
    let candidate = Candidate::new("environment-match");
    let foreign = Candidate::new("environment-match");
    let composition = candidate.admit();
    for mutation in 0..3 {
        let mut authorities = composition.authorities.clone();
        authorities
            .instances
            .insert("before-invalid".to_owned(), composition.authorities.instances[ROOT].clone());
        let VerifiedLanguage::CommandH1V1(command) =
            &mut authorities.instances.get_mut(ROOT).expect("root").programs[0]
        else {
            panic!("genuine H1 authority")
        };
        match mutation {
            0 => command.sources = foreign.sources.clone(),
            1 => {
                command.binding.key = Some("MORE".into());
                command.binding.approved_key = Some("MORE".into());
            }
            _ => command.binding.issuer = foreign.program.verified_ir().identity(),
        }
        let expected = command.sources().expect_err("original H1 validation rejects");
        assert_eq!(expected, invalid());
        assert_eq!(authorities.source_identities().expect_err("collector propagates"), expected);
    }
    composition.authorities.source_identities().expect("original retained authority recovers");
}
