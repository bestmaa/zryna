//! Exact reservation and checked resource boundaries retain genuine source issuers.

use super::{function, reference, verify_candidate};
use zryna_ir::data_ownership_v1 as limits;

#[test]
fn native_c_private_boundary_v0_global_64_slots_survive_65_live_and_sequential_source_plans() {
    use super::super::super::FlowStep;
    use std::fmt::Write;
    for (count, sequential) in [(64, false), (65, false), (65, true)] {
        let mut source = String::from("function privateLimit(): i32 {\n");
        for index in 0..count {
            writeln!(source,
                "const o{index}: FfiHandleOut = Ffi.outHandle(\"fixture-c-v0@0/fixture_handle\");\n\
                 const s{index}: i32 = Ffi.rawCall(\"fixture-c-v0@0/fixture_open\", 0, o{index});\n\
                 if (s{index} !== 0) {{ return Ffi.foreignError(\"fixture-c-v0@0/fixture_open\", s{index}); }}"
            ).expect("independent bounded source");
            if sequential {
                writeln!(
                    source,
                    "const h{index}: FfiHandle = Ffi.takeHandle(o{index});\n\
                    Ffi.release(\"fixture-c-v0@0/fixture_close\", h{index});"
                )
                .expect("independent confirmed releases");
            }
        }
        source.push_str("return 0; }");
        let capture = super::capture::compact(&source);
        let bodies = super::verify_bodies(&capture.sources, &capture.declarations)
            .expect("global reservation plans");
        let boundary = super::compose_private_boundaries(&capture.sources, &bodies)
            .expect("bounded private composition");
        let original = bodies
            .functions()
            .iter()
            .find(|function| function.name() == "privateLimit")
            .expect("source function");
        let function = super::named(&boundary, "privateLimit");
        assert!(function.private_owners.is_empty());
        assert_eq!(original.owner_origins().len(), count);
        let mut acquisitions = 0;
        for step in original.steps() {
            if let FlowStep::Reserve { live_limit, maximum_new_owners, cleanup, .. } = step {
                assert_eq!(*live_limit, 64);
                assert!(*maximum_new_owners <= 1);
                if *maximum_new_owners == 1 {
                    acquisitions += 1;
                    if sequential {
                        assert!(cleanup.is_empty());
                    }
                }
            }
        }
        assert_eq!(acquisitions, count);
        assert_eq!(
            function.steps.last().expect("return").exits[0].cleanup.len(),
            if sequential { 0 } else { count }
        );
    }
}

#[test]
fn native_c_private_boundary_v0_checked_budget_boundaries_and_overflow_precede_candidate_sealing() {
    use super::super::super::private_boundary::checked_count;
    for maximum in [
        limits::MAX_VALUES_PER_FUNCTION,
        limits::MAX_VALUES_PER_PROGRAM,
        limits::MAX_PLACES_PER_FUNCTION,
        limits::MAX_OWNERSHIP_TRANSITIONS_PER_FUNCTION,
        limits::MAX_ACTIVE_BORROWS_PER_FUNCTION,
        limits::MAX_DROP_ACTIONS_PER_FUNCTION,
        limits::MAX_CLEANUP_PLANS_PER_FUNCTION,
    ] {
        assert_eq!(checked_count(maximum - 1, 1, maximum).expect("exact maximum"), maximum);
        assert_eq!(
            checked_count(maximum, 1, maximum).expect_err("first excess").code(),
            "ZRYNA-C4107"
        );
    }
    assert_eq!(
        checked_count(usize::MAX, 1, usize::MAX).expect_err("overflow").code(),
        "ZRYNA-C4107"
    );
    let (capture, bodies, mut candidate) = reference();
    function(&mut candidate, 0)
        .expression_origins
        .resize(limits::MAX_VALUES_PER_FUNCTION + 1, None);
    assert_eq!(
        verify_candidate(&capture.sources, &bodies, candidate)
            .expect_err("candidate excess")
            .code(),
        "ZRYNA-C4107"
    );
}
