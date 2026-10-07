//! Supplied cleanup bytes and type-valid semantic drift have separate authorities.
use super::base_fixture_tests::{expected_plan, fixture};
use super::*;
#[test]
fn independent_supplied_cleanup_plan_drift_is_distinct_from_plan_derivation() {
    let (mut p, linear, linux, sources) = fixture();
    p.plans = expected_plan();
    for attack in 0..7 {
        let mut h = p.clone();
        match attack {
            0 => h.plans[0].steps[0].cleanup.clear(),
            1 => h.plans[0].steps[0].cleanup.push(0),
            2 => h.plans[0].steps[0].cleanup[0] = 7, // Copy enum in place of live String root.
            3 => h.plans[0].steps[0].cleanup[0] = u32::MAX,
            4 => h.plans[0].steps[0].end_loans.push(0),
            5 => h.plans[0].steps.swap(0, 1), // Canonical step ordering is exact.
            _ => h.plans[0].steps[0].failure = false,
        }
        let bytes =
            super::super::super::wire::encode(&h).expect("well-formed raw plan drift can encode");
        let decoded =
            super::super::super::wire::decode(&bytes).expect("decode grants no semantic authority");
        typed(decoded.claims(), &linear, &linux, &sources)
            .expect("typing does not authenticate cleanup bytes");
        // This is exactly the inequality consumed by owned_v2::verify's mandatory
        // plan comparison, not a claim that derive examines supplied raw plans.
        assert_ne!(
            derive(&decoded.claims().graph, &decoded.claims().extensions, &linear)
                .expect("independent replay"),
            decoded.claims().plans
        );
        assert_eq!(
            derive(&p.graph, &p.extensions, &linear).expect("pristine plan recovery"),
            p.plans
        );
    }
}

#[test]
fn independent_well_typed_literal_change_is_not_a_typed_source_authentication_failure() {
    let (p, linear, linux, sources) = fixture();
    let mut h = p.clone();
    h.graph.functions[0].blocks[0].instructions[0].operation = graph::Operation::I32Literal(12);
    typed(&h, &linear, &linux, &sources)
        .expect("type-valid literal drift requires source-bound rejection later");
    assert_eq!(
        derive(&h.graph, &h.extensions, &linear).expect("Copy literal keeps owner obligations"),
        expected_plan()
    );
    assert_eq!(observe(&h, false), 41);
    assert_eq!(observe(&p, false), 42);
    typed(&p, &linear, &linux, &sources).expect("pristine literal recovery");
}
