//! Positive inventory and issuer checks on genuinely captured source programs.
#[allow(dead_code)]
mod capture;

use zryna_native_c_ir::{lower, raw::ValueKind};
use zryna_semantics::native_c_v0::body::{
    BoundaryExitKind, BoundaryOwner, FailureRoute, FlowStep, PrivateOrigin, PrivatePreparation,
    StorageStage,
};
use zryna_syntax::native_c_v0::raw::{Direction, Primitive};

#[test]
fn complete_functions_declarations_and_all_fourteen_primitives_are_sealed() {
    let input = capture::reference();
    let program = lower(&input.sources, &input.authority).expect("verified extension IR");
    assert_eq!(program.functions().len(), 6);
    assert_eq!(
        program.operations().len(),
        input.authority.body_authority().declaration_authority().operation_count()
    );
    let mut primitives = Vec::new();
    for (function, original) in
        program.functions().zip(input.authority.body_authority().functions())
    {
        assert_eq!(function.identity(), (original.file_id(), original.source_function_index()));
        assert_eq!(function.values().len(), original.expressions().len());
        assert_eq!(function.effects().len(), original.steps().len());
        for value in function.values() {
            if let ValueKind::Primitive(p, _) = &value.definition().kind
                && !primitives.contains(p)
            {
                primitives.push(*p);
            }
        }
    }
    assert_eq!(primitives.len(), 14);
    for p in [
        Primitive::RawCall,
        Primitive::BorrowBytes,
        Primitive::BorrowUtf8,
        Primitive::ByteLength,
        Primitive::OutI32,
        Primitive::OutHandle,
        Primitive::OutBytes,
        Primitive::OutCount,
        Primitive::ReadI32,
        Primitive::TakeHandle,
        Primitive::TakeBytes,
        Primitive::CopyBytes,
        Primitive::Release,
        Primitive::ForeignError,
    ] {
        assert!(primitives.contains(&p));
    }
}

#[test]
fn total_scalar_export_retains_wrapping_body_and_exact_public_symbol() {
    let input = capture::reference();
    let program = lower(&input.sources, &input.authority).expect("IR");
    let operation = program
        .operations()
        .find(|op| op.declaration().direction == Direction::Export)
        .expect("scalar export");
    assert_eq!(operation.declaration().symbol, "zryna_c_v0_e_add");
    let function = program
        .functions()
        .find(|f| f.export() == Some(operation.index()))
        .expect("exact source export");
    assert!(function.values().any(|v| matches!(v.definition().kind, ValueKind::WrappingAdd(_, _))));
    assert!(function.private_owners().is_empty());
    assert!(
        function
            .effects()
            .flat_map(zryna_native_c_ir::VerifiedEffect::exits)
            .all(|exit| exit.kind == BoundaryExitKind::Return)
    );
}

#[test]
fn packing_copy_faults_and_dual_layout_runtime_issuers_remain_distinct() {
    let input = capture::reference();
    let program = lower(&input.sources, &input.authority).expect("IR");
    assert_eq!(
        program.native_layouts().universe_identity(),
        program.linear_layouts().universe_identity()
    );
    assert_eq!(
        program.runtime_abi().type_universe_identity(),
        program.native_layouts().universe_identity()
    );
    let mut packed = false;
    let mut copied = false;
    let mut utf8 = false;
    for effect in program.functions().flat_map(zryna_native_c_ir::VerifiedFunction::effects) {
        match effect.preparation() {
            Some(PrivatePreparation::Loan(loan)) if loan.scratch.is_some() => {
                packed = true;
                assert_eq!(
                    (
                        loan.source_stride,
                        loan.backing_stride,
                        loan.backing_alignment,
                        loan.native_bits
                    ),
                    (4, 1, 1, 64)
                );
                assert_eq!(loan.stages.last(), Some(&StorageStage::Commit));
                assert!(loan.empty_without_allocation);
                assert!(
                    loan.faults
                        .iter()
                        .all(|fault| fault.runtime == program.runtime_abi().identity()
                            && fault.declaration.trap_identity().is_some())
                );
            }
            Some(PrivatePreparation::Loan(loan)) => {
                utf8 = true;
                assert!(loan.allocation.is_none() && loan.faults.is_empty());
            }
            Some(PrivatePreparation::Copy(copy)) => {
                copied = true;
                assert_eq!((copy.stride, copy.alignment), (4, 4));
                assert!(copy.zero_extend_bytes && copy.empty_without_allocation);
            }
            None => {}
        }
    }
    assert!(packed && copied && utf8);
}

#[test]
fn mixed_private_foreign_cleanup_and_process_failure_domains_are_complete() {
    let input = capture::reference();
    let program = lower(&input.sources, &input.authority).expect("IR");
    let copied = program.functions().find(|f| f.name() == "copied").expect("copy wrapper");
    let preparation = copied
        .effects()
        .find(|e| matches!(e.operation(), FlowStep::Copy { .. }))
        .expect("copy effect");
    let fault = preparation
        .exits()
        .iter()
        .find(|e| matches!(e.kind, BoundaryExitKind::PrivateTrap(_)))
        .expect("private fault");
    assert!(fault.cleanup.iter().any(|drop| matches!(drop.owner(), BoundaryOwner::Foreign(_))));
    assert!(fault.cleanup.iter().any(|drop| matches!(drop.owner(), BoundaryOwner::Private(_))));
    for exit in program
        .functions()
        .flat_map(zryna_native_c_ir::VerifiedFunction::effects)
        .flat_map(zryna_native_c_ir::VerifiedEffect::exits)
    {
        assert_eq!(exit.release_failure_route, FailureRoute::ReleaseFailureOverridesUnresolved);
        if matches!(
            exit.kind,
            BoundaryExitKind::ForeignFailure(FailureRoute::ProcessFailureNoCleanupGuarantee)
        ) {
            assert!(!exit.cleanup_required && exit.cleanup.is_empty() && exit.end_loans.is_empty());
        }
    }
}

#[test]
fn original_private_move_and_string_alias_retain_one_protected_origin() {
    let extra = r#"
function retained(text: String): String {
  const loan: FfiBytes = Ffi.borrowUtf8(text);
  const alias: FfiBytes = loan;
  const moved: String = text;
  const out: FfiI32Out = Ffi.outI32();
  const status: i32 = Ffi.rawCall("fixture-c-v0@0/sum_bytes", alias, Ffi.byteLength(alias), out);
  if (status !== 0) { return Ffi.foreignError("fixture-c-v0@0/sum_bytes", status); }
  return moved;
}
"#;
    let input =
        capture::edited(&format!("{}{extra}", capture::BUFFER), capture::HANDLE, capture::SCALAR);
    let program = lower(&input.sources, &input.authority).expect("independent moved origin replay");
    let function = program.functions().find(|f| f.name() == "retained").expect("original function");
    assert_eq!(function.private_owners().len(), 1);
    let exit = function
        .effects()
        .flat_map(zryna_native_c_ir::VerifiedEffect::exits)
        .find(|exit| exit.kind == BoundaryExitKind::Return)
        .expect("return");
    assert_eq!(exit.protected_result, Some(PrivateOrigin::Parameter(0)));
    assert!(exit.cleanup.is_empty());
    assert_eq!(exit.end_loans.len(), 1);
}
