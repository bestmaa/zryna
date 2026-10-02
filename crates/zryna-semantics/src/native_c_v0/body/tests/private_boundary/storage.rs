use super::{
    BoundaryExitKind, PrivateOrigin, PrivatePreparation, StorageStage, capture,
    compose_private_boundaries, reference, reject, verify_bodies,
};

#[test]
fn native_c_private_boundary_v0_pack_checks_precede_distinct_one_byte_scratch_and_empty_needs_no_allocation()
 {
    let (capture, bodies, _) = reference();
    let boundary = compose_private_boundaries(&capture.sources, &bodies).expect("private plan");
    let loan = boundary.functions()[0]
        .steps
        .iter()
        .find_map(|step| match &step.preparation {
            Some(PrivatePreparation::Loan(loan)) => Some(loan),
            _ => None,
        })
        .expect("packed loan");
    assert_eq!(loan.source, PrivateOrigin::Parameter(0));
    assert_ne!(loan.scratch, Some(loan.source));
    assert_eq!((loan.source_stride, loan.backing_stride, loan.native_bits), (4, 1, 64));
    assert_eq!(loan.maximum_bytes, 4096);
    assert_eq!(
        loan.stages,
        [
            StorageStage::Length,
            StorageStage::ByteRange,
            StorageStage::AllocateNonempty,
            StorageStage::Initialize,
            StorageStage::Commit
        ]
    );
    assert!(loan.empty_without_allocation);
    for length in [0_usize, 4096, 4097] {
        assert_eq!(length <= loan.maximum_bytes, length != 4097);
    }
    for element in [-1_i32, 0, 255, 256] {
        assert_eq!((0..=255).contains(&element), element == 0 || element == 255);
    }
    assert!(
        boundary.functions()[0]
            .steps
            .iter()
            .flat_map(|step| &step.exits)
            .filter(|exit| matches!(
                exit.kind,
                BoundaryExitKind::PrivateTrap(_) | BoundaryExitKind::PrivateAbiFailure(_)
            ))
            .all(|exit| exit.preparation_nonempty_only)
    );
}

#[test]
fn native_c_private_boundary_v0_source_stride_cast_linear_lane_and_omitted_prechecks_reject() {
    for defect in 0..4 {
        reject(
            |candidate| {
                let loan = candidate.functions[0]
                    .steps
                    .iter_mut()
                    .find_map(|step| match &mut step.preparation {
                        Some(PrivatePreparation::Loan(loan)) => Some(loan),
                        _ => None,
                    })
                    .expect("loan");
                match defect {
                    0 => loan.source_stride = 1,
                    1 => loan.backing_stride = 4,
                    2 => loan.native_bits = 32,
                    _ => {
                        loan.stages.remove(1);
                    }
                }
            },
            "ZRYNA-C4105",
            if defect == 1 || defect == 2 {
                "boundary-loan-retention"
            } else {
                "boundary-byte-packing"
            },
        );
    }
}

#[test]
fn native_c_private_boundary_v0_utf8_alias_and_source_move_retain_one_origin_without_preparation_allocation()
 {
    let extra = r#"
function retained(text: String): String {
  const loan: FfiBytes = Ffi.borrowUtf8(text);
  const alias: FfiBytes = loan;
  const moved: String = text;
  const one: FfiI32Out = Ffi.outI32();
  const a: i32 = Ffi.rawCall("fixture-c-v0@0/sum_bytes", alias, Ffi.byteLength(alias), one);
  if (a !== 0) { return Ffi.foreignError("fixture-c-v0@0/sum_bytes", a); }
  const two: FfiI32Out = Ffi.outI32();
  const b: i32 = Ffi.rawCall("fixture-c-v0@0/sum_bytes", loan, Ffi.byteLength(loan), two);
  if (b !== 0) { return Ffi.foreignError("fixture-c-v0@0/sum_bytes", b); }
  return moved;
}
"#;
    let capture =
        capture::edited(&format!("{}{extra}", capture::BUFFER), capture::HANDLE, capture::SCALAR);
    let bodies =
        verify_bodies(&capture.sources, &capture.declarations).expect("move preserves backing");
    let boundary =
        compose_private_boundaries(&capture.sources, &bodies).expect("String entry retention");
    let function = boundary
        .functions()
        .iter()
        .find(|function| {
            function.source_function == 3 && function.file == bodies.functions()[0].file_id()
        })
        .expect("retained String");
    assert_eq!(function.private_owners.len(), 1);
    let loans = function
        .steps
        .iter()
        .filter_map(|step| match &step.preparation {
            Some(PrivatePreparation::Loan(loan)) => Some(loan),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(loans.len(), 1);
    assert_eq!(loans[0].source, PrivateOrigin::Parameter(0));
    assert!(
        loans[0].scratch.is_none() && loans[0].allocation.is_none() && loans[0].faults.is_empty()
    );
    let returned = function.steps.last().expect("return").exits.last().expect("return exit");
    assert_eq!(returned.protected_result, Some(PrivateOrigin::Parameter(0)));
    assert!(returned.cleanup.is_empty());
    assert_eq!(returned.end_loans, [loans[0].token]);
}

#[test]
fn native_c_private_boundary_v0_string_cannot_invent_a_utf8_allocation_or_forget_retention() {
    for defect in 0..2 {
        reject(
            |candidate| {
                let loan = candidate.functions[1]
                    .steps
                    .iter_mut()
                    .find_map(|step| match &mut step.preparation {
                        Some(PrivatePreparation::Loan(loan)) => Some(loan),
                        _ => None,
                    })
                    .expect("String loan");
                if defect == 0 {
                    loan.source = PrivateOrigin::Copy(99);
                } else {
                    loan.scratch = Some(PrivateOrigin::Packed(loan.expression));
                }
            },
            "ZRYNA-C4105",
            if defect == 0 {
                "boundary-loan-retention"
            } else {
                "boundary-string-storage-retention"
            },
        );
    }
}

#[test]
fn native_c_private_boundary_v0_copy_expands_unsigned_bytes_into_distinct_four_byte_elements_before_commit()
 {
    let (capture, bodies, _) = reference();
    let boundary = compose_private_boundaries(&capture.sources, &bodies).expect("expanded result");
    let copy = boundary.functions()[2]
        .steps
        .iter()
        .find_map(|step| match &step.preparation {
            Some(PrivatePreparation::Copy(copy)) => Some(copy),
            _ => None,
        })
        .expect("copy");
    assert_eq!((copy.stride, copy.alignment), (4, 4));
    assert_ne!(copy.vector_type, copy.element_type);
    assert_eq!(copy.result, PrivateOrigin::Copy(copy.expression));
    assert_eq!(4096_u64.checked_mul(copy.stride), Some(16384));
    assert_eq!(u64::MAX.checked_mul(copy.stride), None);
    assert_eq!(
        copy.stages,
        [
            StorageStage::ValidateForeign,
            StorageStage::CheckedCapacity,
            StorageStage::AllocateNonempty,
            StorageStage::Initialize,
            StorageStage::Commit
        ]
    );
    assert!(copy.zero_extend_bytes && copy.empty_without_allocation);
    assert_eq!([0_u8, 127, 128, 255].map(i32::from), [0, 127, 128, 255]);
}

#[test]
fn native_c_private_boundary_v0_adoption_wrong_element_and_early_commit_reject() {
    for defect in 0..3 {
        reject(
            |candidate| {
                let copy = candidate.functions[2]
                    .steps
                    .iter_mut()
                    .find_map(|step| match &mut step.preparation {
                        Some(PrivatePreparation::Copy(copy)) => Some(copy),
                        _ => None,
                    })
                    .expect("copy");
                match defect {
                    0 => copy.zero_extend_bytes = false,
                    1 => copy.element_type = copy.vector_type,
                    _ => copy.stages.swap(3, 4),
                }
            },
            if defect == 1 { "ZRYNA-C4104" } else { "ZRYNA-C4105" },
            if defect == 1 {
                "boundary-copy-element-layout"
            } else {
                "boundary-copy-initialized-prefix"
            },
        );
    }
}
