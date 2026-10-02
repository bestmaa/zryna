//! Independent mutations keep the actual source/material/body/private issuer fixed.
#[allow(dead_code)]
mod capture;

use zryna_native_c_ir::{lower, lower_unverified, raw, verify};
use zryna_semantics::native_c_v0::body::{
    BoundaryExitKind, FailureRoute, FlowStep, PrivatePreparation,
};
use zryna_syntax::native_c_v0::raw::{AbiType, Safety};

fn reject(change: impl FnOnce(&mut raw::Program), code: &str) {
    let input = capture::reference();
    let mut candidate =
        lower_unverified(&input.sources, &input.authority).expect("untrusted candidate");
    change(&mut candidate);
    let error = verify(candidate, &input.sources, &input.authority)
        .expect_err("hostile independent candidate must reject without a seal");
    assert_eq!(error.code(), code, "{error}");
    if let Some(span) = error.span() {
        assert!(input.sources.resolve(span).is_ok());
    }
    assert!(
        lower(&input.sources, &input.authority).is_ok(),
        "rejection cannot poison the genuine issuer"
    );
}

#[test]
fn rebuilt_source_and_other_real_issuer_cannot_replace_original_authority() {
    let input = capture::reference();
    let other = capture::reference();
    let candidate = lower_unverified(&input.sources, &input.authority).expect("claims");
    let error = verify(candidate, &other.sources, &input.authority)
        .expect_err("equal bytes are another map");
    assert_eq!(error.code(), "ZRYNA-C4106");
    assert!(error.span().is_none());
    let candidate =
        lower_unverified(&other.sources, &other.authority).expect("other genuine claims");
    assert_eq!(
        verify(candidate, &input.sources, &input.authority).expect_err("another issuer").code(),
        "ZRYNA-C4102"
    );
}

#[test]
fn missing_duplicate_and_reordered_original_occupants_fail_closed() {
    reject(
        |p| {
            p.functions.pop();
        },
        "ZRYNA-C4106",
    );
    reject(
        |p| {
            p.functions[1] = p.functions[0].clone();
        },
        "ZRYNA-C4106",
    );
    reject(
        |p| {
            p.functions[0].values[1].id = 0;
        },
        "ZRYNA-C4106",
    );
    reject(
        |p| {
            p.functions[0].statements.swap(0, 1);
        },
        "ZRYNA-C4106",
    );
}

#[test]
fn changed_add_operand_binding_and_literal_do_not_match_source() {
    reject(
        |p| {
            let value = p
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.values)
                .find(|v| matches!(v.kind, raw::ValueKind::WrappingAdd(_, _)))
                .expect("wrapping add");
            if let raw::ValueKind::WrappingAdd(left, right) = &mut value.kind {
                *right = *left;
            }
        },
        "ZRYNA-C4106",
    );
    reject(
        |p| {
            let value = p.functions[0]
                .values
                .iter_mut()
                .find(|v| matches!(v.kind, raw::ValueKind::Local(_)))
                .expect("local");
            value.kind = raw::ValueKind::Local(999);
        },
        "ZRYNA-C4106",
    );
    let input = capture::edited(
        capture::BUFFER,
        capture::HANDLE,
        &format!("{}\nfunction constant(): i32 {{ return 7; }}\n", capture::SCALAR),
    );
    let mut p = lower_unverified(&input.sources, &input.authority).expect("genuine literal");
    p.functions.iter_mut().find(|f| f.name == "constant").expect("source function").values[0]
        .kind = raw::ValueKind::I32(8);
    assert_eq!(
        verify(p, &input.sources, &input.authority).expect_err("changed source literal").code(),
        "ZRYNA-C4106"
    );
}

#[test]
fn changed_value_category_and_status_call_provenance_reject() {
    reject(
        |p| {
            p.functions[0].values[0].ty = zryna_semantics::native_c_v0::body::ValueType::Bool;
        },
        "ZRYNA-C4104",
    );
    reject(
        |p| {
            let value = p.functions[0]
                .values
                .iter_mut()
                .find(|v| v.status_call.is_some())
                .expect("status result");
            value.status_call = Some(99);
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            let value = p.functions[0]
                .values
                .iter_mut()
                .find(|v| v.origin.is_some())
                .expect("retained private origin");
            value.origin = Some(zryna_semantics::native_c_v0::body::PrivateOrigin::Copy(99));
        },
        "ZRYNA-C4105",
    );
}

#[test]
fn identity_precedes_target_and_signature_resource_source_categories_remain_distinct() {
    reject(
        |p| {
            let parameter = p
                .declarations
                .operations
                .iter_mut()
                .flat_map(|op| &mut op.parameters)
                .find(|parameter| parameter.resource.is_some())
                .expect("resource role");
            parameter.resource = None;
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            p.declarations.target = "x86_64-pc-windows-msvc".into();
        },
        "ZRYNA-C4103",
    );
    reject(
        |p| {
            p.declarations.target = "x86_64-pc-windows-msvc".into();
            p.declarations.libraries[0].header_sha256 = "0".repeat(64);
        },
        "ZRYNA-C4102",
    );
    reject(
        |p| {
            p.declarations.operations[0].parameters[0].abi = AbiType::CInt;
        },
        "ZRYNA-C4104",
    );
    reject(
        |p| {
            p.declarations.operations[0].symbol = "renamed".into();
        },
        "ZRYNA-C4102",
    );
    reject(
        |p| {
            p.declarations
                .operations
                .iter_mut()
                .find(|o| !o.resources.is_empty())
                .expect("resource")
                .resources[0]
                .release = "wrong@0/free".into();
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            p.declarations.sites[0].safety = Safety::UnsafeRaw;
        },
        "ZRYNA-C4106",
    );
}

#[test]
fn no_unused_declaration_or_library_may_disappear_from_complete_accounting() {
    reject(
        |p| {
            p.declarations.operations.pop();
        },
        "ZRYNA-C4102",
    );
    reject(
        |p| {
            p.declarations.libraries.clear();
        },
        "ZRYNA-C4102",
    );
}

#[test]
fn raw_safe_marker_and_exact_call_carriers_cannot_be_changed() {
    reject(
        |p| {
            if let FlowStep::Call { safety, .. } = &mut p.functions[0]
                .effects
                .iter_mut()
                .find(|e| matches!(e.operation, FlowStep::Call { .. }))
                .expect("call")
                .operation
            {
                *safety = Safety::Safe;
            }
        },
        "ZRYNA-C4106",
    );
    reject(
        |p| {
            if let FlowStep::Call { carriers, .. } = &mut p.functions[0]
                .effects
                .iter_mut()
                .find(|e| matches!(e.operation, FlowStep::Call { .. }))
                .expect("call")
                .operation
            {
                carriers[0] = AbiType::Bool32;
            }
        },
        "ZRYNA-C4104",
    );
}

#[test]
fn reservation_status_guard_initialized_read_and_confirmed_release_are_exact() {
    for defect in 0..4 {
        reject(
            |p| {
                for effect in p.functions.iter_mut().flat_map(|f| &mut f.effects) {
                    match (&mut effect.operation, defect) {
                        (FlowStep::Reserve { maximum_new_owners, .. }, 0) => {
                            *maximum_new_owners += 1;
                            break;
                        }
                        (FlowStep::StatusGuard { call, .. }, 1)
                        | (FlowStep::ReadOutput { call, .. }, 2) => {
                            *call = 99;
                            break;
                        }
                        (FlowStep::ConfirmRelease { owner, .. }, 3) => {
                            *owner = 99;
                            break;
                        }
                        _ => {}
                    }
                }
            },
            "ZRYNA-C4105",
        );
    }
}

#[test]
fn packing_cast_commit_order_and_omitted_checks_reject_independently() {
    for defect in 0..4 {
        reject(
            |p| {
                let loan = p.functions[0]
                    .effects
                    .iter_mut()
                    .find_map(|e| match &mut e.preparation {
                        Some(PrivatePreparation::Loan(l)) => Some(l),
                        _ => None,
                    })
                    .expect("packing");
                match defect {
                    0 => loan.source_stride = 1,
                    1 => loan.backing_stride = 4,
                    2 => loan.stages.swap(3, 4),
                    _ => {
                        loan.stages.remove(1);
                    }
                }
            },
            "ZRYNA-C4105",
        );
    }
}

#[test]
fn private_copy_issuer_fault_domain_and_complete_initialization_cannot_be_forged() {
    for defect in 0..3 {
        reject(
            |p| {
                let wrong_issuer = p.functions[0].private_owners[0].release;
                let copy = p.functions[2]
                    .effects
                    .iter_mut()
                    .find_map(|e| match &mut e.preparation {
                        Some(PrivatePreparation::Copy(c)) => Some(c),
                        _ => None,
                    })
                    .expect("copy");
                match defect {
                    0 => copy.zero_extend_bytes = false,
                    1 => copy.faults[0].operation = wrong_issuer,
                    _ => copy.stages.swap(0, 4),
                }
            },
            "ZRYNA-C4105",
        );
    }
    reject(
        |p| {
            let exit = p
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.effects)
                .flat_map(|e| &mut e.exits)
                .find(|e| matches!(e.kind, BoundaryExitKind::PrivateTrap(_)))
                .expect("private trap");
            exit.kind = BoundaryExitKind::ForeignFailure(FailureRoute::DeclaredForeignError);
        },
        "ZRYNA-C4105",
    );
}

#[test]
fn loan_end_and_mixed_reverse_cleanup_cannot_be_omitted_reordered_or_duplicated() {
    for defect in 0..4 {
        reject(
            |p| {
                let exits =
                    p.functions.iter_mut().flat_map(|f| &mut f.effects).flat_map(|e| &mut e.exits);
                if defect == 0 {
                    exits
                        .into_iter()
                        .find(|e| !e.end_loans.is_empty())
                        .expect("loan end")
                        .end_loans
                        .clear();
                } else {
                    let exit =
                        exits.into_iter().find(|e| e.cleanup.len() > 1).expect("mixed cleanup");
                    match defect {
                        1 => {
                            exit.cleanup.pop();
                        }
                        2 => exit.cleanup.swap(0, 1),
                        _ => exit.cleanup.push(exit.cleanup[0].clone()),
                    }
                }
            },
            "ZRYNA-C4105",
        );
    }
}

#[test]
fn pending_return_release_override_and_process_unresolved_obligations_are_preserved() {
    reject(
        |p| {
            p.functions[2]
                .effects
                .iter_mut()
                .flat_map(|e| &mut e.exits)
                .find(|e| e.kind == BoundaryExitKind::Return)
                .expect("owned return")
                .protected_result = None;
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            p.functions[0]
                .effects
                .iter_mut()
                .flat_map(|e| &mut e.exits)
                .find(|e| !e.cleanup.is_empty())
                .expect("cleanup")
                .release_failure_route = FailureRoute::DeclaredForeignError;
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            p.functions[0]
                .effects
                .iter_mut()
                .flat_map(|e| &mut e.exits)
                .find(|e| !e.cleanup_required)
                .expect("process edge")
                .cleanup_required = true;
        },
        "ZRYNA-C4105",
    );
    reject(
        |p| {
            p.functions[2]
                .effects
                .iter_mut()
                .flat_map(|e| &mut e.exits)
                .find(|e| !e.unresolved.is_empty())
                .expect("unresolved owner")
                .unresolved
                .clear();
        },
        "ZRYNA-C4105",
    );
}

#[test]
fn stale_layout_and_missing_storage_descriptor_cannot_mint_an_ir_seal() {
    reject(
        |p| {
            p.storage.native[0] ^= 1;
        },
        "ZRYNA-C4102",
    );
    reject(
        |p| {
            p.functions[0].parameter_layouts.clear();
        },
        "ZRYNA-C4105",
    );
}
