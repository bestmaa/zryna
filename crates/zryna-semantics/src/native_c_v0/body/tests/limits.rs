use super::{FlowStep, TrapRequirement, capture, verify_bodies};
use std::fmt::Write;

fn acquisitions(count: usize, sequential: bool) -> String {
    let mut source = String::from("function limit(): i32 {\n");
    for index in 0..count {
        write!(source,
            "const o{index}: FfiHandleOut = Ffi.outHandle(\"fixture-c-v0@0/fixture_handle\");\n\
             const s{index}: i32 = Ffi.rawCall(\"fixture-c-v0@0/fixture_open\", 0, o{index});\n\
             if (s{index} !== 0) {{ return Ffi.foreignError(\"fixture-c-v0@0/fixture_open\", s{index}); }}\n"
        ).expect("writing an owned source String");
        if sequential {
            write!(
                source,
                "const h{index}: FfiHandle = Ffi.takeHandle(o{index});\n\
                 Ffi.release(\"fixture-c-v0@0/fixture_close\", h{index});\n"
            )
            .expect("writing an owned source String");
        }
    }
    source.push_str("return 0; }");
    source
}

#[test]
fn native_c_body_v0_execution_instance_reserves_64_or_65_live_before_c_without_source_count_rejection()
 {
    for count in [64, 65] {
        let capture = capture::compact(&acquisitions(count, false));
        let bodies = verify_bodies(&capture.sources, &capture.declarations)
            .expect("source produces a bounded runtime reservation plan");
        let function = bodies
            .functions()
            .iter()
            .find(|function| function.name() == "limit")
            .expect("limit wrapper");
        assert_eq!(function.owner_origins().len(), count);
        let reservations: Vec<_> = function
            .steps()
            .iter()
            .filter_map(|step| match step {
                FlowStep::Reserve {
                    call, maximum_new_owners, live_limit, trap, cleanup, ..
                } => Some((*call, *maximum_new_owners, *live_limit, *trap, cleanup)),
                _ => None,
            })
            .collect();
        assert_eq!(reservations.len(), count);
        for (index, (call, acquisitions, limit, trap, cleanup)) in reservations.iter().enumerate() {
            assert_eq!(
                (*call, *acquisitions, *limit, *trap),
                (index, 1, 64, TrapRequirement::ForeignResourceLimit)
            );
            assert_eq!(cleanup.len(), index, "reservation precedes acquisition of its own owner");
            assert!(cleanup.iter().all(|entry| entry.creating_call() < *call));
        }
        // These are exact runtime obligations, not an executed counter or a 65-live leak claim.
    }
}

#[test]
fn native_c_body_v0_more_than_64_released_lifetime_acquisitions_do_not_exhaust_the_live_budget() {
    let capture = capture::compact(&acquisitions(65, true));
    let bodies = verify_bodies(&capture.sources, &capture.declarations)
        .expect("sequential released lifetime acquisitions");
    let function = bodies
        .functions()
        .iter()
        .find(|function| function.name() == "limit")
        .expect("sequential wrapper");
    assert_eq!(function.owner_origins().len(), 65);
    assert!(
        function
            .steps()
            .iter()
            .filter_map(|step| match step {
                FlowStep::Reserve { maximum_new_owners: 1, cleanup, .. } => Some(cleanup),
                _ => None,
            })
            .all(Vec::is_empty),
        "only actual still-live obligations enter preflight cleanup"
    );
    assert!(
        matches!(function.steps().last(), Some(FlowStep::Return { cleanup, .. }) if cleanup.is_empty())
    );
}
