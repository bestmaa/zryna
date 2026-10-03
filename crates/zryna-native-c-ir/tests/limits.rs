//! Genuine exact-bound source plus hostile first-extra IR and runtime-reservation plans.
use std::fmt::Write as _;
#[allow(dead_code)]
mod capture;

use zryna_native_c_ir::{lower, lower_unverified, verify};
use zryna_semantics::native_c_v0::body::{FlowStep, TrapRequirement};

#[test]
fn genuine_exact_value_arena_is_admitted_and_first_extra_raw_definition_rejects() {
    let mut extra = String::from("\nfunction arena(): i32 {\n");
    for index in 0..2340 {
        writeln!(extra, "const v{index}: i32 = 1 + 2 + 3 + 4;")
            .expect("bounded fixture formatting");
    }
    extra.push_str("1;\nreturn v0 + 1;\n}\n");
    let input =
        capture::edited(capture::BUFFER, capture::HANDLE, &format!("{}{extra}", capture::SCALAR));
    let program = lower(&input.sources, &input.authority).expect("genuine 16384-definition arena");
    assert_eq!(
        program.functions().find(|f| f.name() == "arena").expect("arena").values().len(),
        16384
    );
    let mut raw = lower_unverified(&input.sources, &input.authority).expect("exact untrusted IR");
    let function = raw.functions.iter_mut().find(|f| f.name == "arena").expect("arena");
    function.values.push(function.values[0].clone());
    let error = verify(raw, &input.sources, &input.authority)
        .expect_err("first extra before inventory replay");
    assert_eq!(error.code(), "ZRYNA-C4107");
    assert_eq!(error.detail(), "ir-value-budget");
    assert!(error.span().is_none());
}

#[test]
fn exact_declaration_count_reaches_identity_stage_and_first_extra_stops_at_budget() {
    let input = capture::reference();
    for count in [256, 257] {
        let mut raw = lower_unverified(&input.sources, &input.authority).expect("claims");
        let mut duplicate = raw.declarations.operations[0].clone();
        duplicate.key = "k".into();
        duplicate.library = "l@0".into();
        duplicate.logical_name = "a".into();
        duplicate.symbol = "a".into();
        duplicate.parameters.clear();
        duplicate.source_binding.path = "a.zry".into();
        raw.declarations.operations.resize(count, duplicate);
        let error = verify(raw, &input.sources, &input.authority)
            .expect_err("duplicate identities or budget");
        if count == 257 {
            assert_eq!((error.code(), error.detail()), ("ZRYNA-C4107", "ir-operation-budget"));
        } else {
            assert_eq!(error.code(), "ZRYNA-C4102");
        }
    }
}

fn handles(count: usize, release: bool) -> String {
    let mut text = String::from("\nfunction many(seed: i32): i32 {\n");
    for index in 0..count {
        write!(text, "const o{index}: FfiHandleOut = Ffi.outHandle(\"fixture-c-v0@0/fixture_handle\");\nconst s{index}: i32 = Ffi.rawCall(\"fixture-c-v0@0/fixture_open\", seed, o{index});\nif (s{index} !== 0) {{ return Ffi.foreignError(\"fixture-c-v0@0/fixture_open\", s{index}); }}\nconst h{index}: FfiHandle = Ffi.takeHandle(o{index});\n").expect("bounded fixture formatting");
        if release {
            writeln!(text, "Ffi.release(\"fixture-c-v0@0/fixture_close\", h{index});")
                .expect("bounded fixture formatting");
        }
    }
    text.push_str("return 0;\n}\n");
    text
}

#[test]
fn sixty_five_potential_live_owners_keep_dynamic_shared_limit_and_pre_call_trap() {
    let input = capture::compact_handle(&handles(65, false));
    let program = lower(&input.sources, &input.authority)
        .expect("conditional 65th reservation is not a static ban");
    let function = program.functions().find(|f| f.name() == "many").expect("many");
    let reservations = function
        .effects()
        .filter_map(|e| match e.operation() {
            FlowStep::Reserve { maximum_new_owners, live_limit, trap, .. } => {
                Some((*maximum_new_owners, *live_limit, *trap))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(reservations.len(), 65);
    assert!(
        reservations.iter().all(|entry| *entry == (1, 64, TrapRequirement::ForeignResourceLimit))
    );
    let mut raw = lower_unverified(&input.sources, &input.authority).expect("claims");
    let reserve = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "many")
        .expect("many")
        .effects
        .iter_mut()
        .find(|e| matches!(e.operation, FlowStep::Reserve { .. }))
        .expect("reserve");
    if let FlowStep::Reserve { live_limit, .. } = &mut reserve.operation {
        *live_limit = 65;
    }
    assert_eq!(
        verify(raw, &input.sources, &input.authority).expect_err("widened runtime bound").code(),
        "ZRYNA-C4105"
    );
}

#[test]
fn sequential_acquisitions_above_sixty_four_do_not_reset_or_become_static_lifetime_count() {
    let input = capture::compact_handle(&handles(65, true));
    let program =
        lower(&input.sources, &input.authority).expect("sequential conditional resource plans");
    let function = program.functions().find(|f| f.name() == "many").expect("many");
    assert_eq!(
        function
            .effects()
            .filter(|e| matches!(e.operation(), FlowStep::ConfirmRelease { .. }))
            .count(),
        65
    );
    assert!(function.effects().all(|e| match e.operation() {
        FlowStep::Reserve { live_limit, .. } => *live_limit == 64,
        _ => true,
    }));
}

#[test]
fn oversized_nested_storage_plan_rejects_before_replay_and_fresh_input_recovers() {
    let input = capture::reference();
    let mut raw = lower_unverified(&input.sources, &input.authority).expect("claims");
    let preparation =
        raw.functions[0].effects.iter_mut().find_map(|e| e.preparation.as_mut()).expect("loan");
    if let zryna_semantics::native_c_v0::body::PrivatePreparation::Loan(loan) = preparation {
        loan.stages.push(loan.stages[0]);
    }
    let error = verify(raw, &input.sources, &input.authority)
        .expect_err("bounded storage stage vocabulary");
    assert_eq!((error.code(), error.detail()), ("ZRYNA-C4107", "ir-storage-stage-budget"));
    let fresh = capture::reference();
    assert!(lower(&fresh.sources, &fresh.authority).is_ok());
}
