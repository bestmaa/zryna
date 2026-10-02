//! Genuine capture positives and an internal hostile-claim seam; no authority is fabricated.

use super::super::private_boundary::{Candidate, build_candidate, verify_candidate};
use super::super::{
    BoundaryExitKind, BoundaryOwner, FunctionBoundary, PrivateOrigin, PrivatePreparation,
    StorageStage, VerifiedForeignBodies, VerifiedPrivateBoundaries, compose_private_boundaries,
    verify_bodies,
};
use super::capture;

mod cleanup;
mod hostile;
mod storage;

fn reference() -> (capture::Capture, VerifiedForeignBodies, Candidate) {
    let capture = capture::reference();
    let bodies =
        verify_bodies(&capture.sources, &capture.declarations).expect("genuine body authority");
    let candidate =
        build_candidate(&capture.sources, &bodies).expect("untrusted private candidate");
    (capture, bodies, candidate)
}

fn function(candidate: &mut Candidate, ordinal: usize) -> &mut FunctionBoundary {
    &mut candidate.functions[ordinal]
}

fn named<'a>(boundary: &'a VerifiedPrivateBoundaries, name: &str) -> &'a FunctionBoundary {
    let index = boundary
        .body_authority()
        .functions()
        .iter()
        .position(|function| function.name() == name)
        .expect("original function name");
    &boundary.functions()[index]
}

fn reject(change: impl FnOnce(&mut Candidate), code: &str, detail: &str) {
    let (capture, bodies, mut candidate) = reference();
    change(&mut candidate);
    let error = verify_candidate(&capture.sources, &bodies, candidate)
        .expect_err("independent hostile claim");
    assert_eq!(error.code(), code);
    assert_eq!(error.detail(), detail);
    let fresh = capture::reference();
    let bodies = verify_bodies(&fresh.sources, &fresh.declarations).expect("fresh body recovery");
    assert!(compose_private_boundaries(&fresh.sources, &bodies).is_ok());
}

#[test]
fn native_c_private_boundary_v0_retains_three_fixture_issuers_and_complete_source_inventory() {
    let (capture, bodies, _) = reference();
    let boundary =
        compose_private_boundaries(&capture.sources, &bodies).expect("complete boundary");
    assert_eq!(boundary.functions().len(), 6);
    assert!(boundary.belongs_to(&capture.sources));
    assert_eq!(boundary.body_authority().functions().len(), bodies.functions().len());
    assert_eq!(
        boundary.runtime_abi().type_universe_identity(),
        boundary.native_layouts().universe_identity()
    );
    assert_eq!(
        boundary.native_layouts().universe_identity(),
        boundary.linear_layouts().universe_identity()
    );
    assert_eq!(boundary.runtime_abi().native_linux_x86_64_functions().len(), 17);
    for (function, original) in boundary.functions().iter().zip(bodies.functions()) {
        assert_eq!(function.file, original.file_id());
        assert_eq!(function.source_function, original.source_function_index());
        assert_eq!(function.expression_origins.len(), original.expressions().len());
        assert_eq!(function.steps.len(), original.steps().len());
        assert_eq!(
            function.steps.iter().map(|step| step.source_step).collect::<Vec<_>>(),
            (0..original.steps().len()).collect::<Vec<_>>()
        );
    }
}

#[test]
fn native_c_private_boundary_v0_rebuilt_map_and_genuine_other_map_layouts_do_not_supply_authority()
{
    let (capture, bodies, mut candidate) = reference();
    let (other, _, replacement) = reference();
    assert_eq!(
        compose_private_boundaries(&other.sources, &bodies)
            .expect_err("another original map")
            .detail(),
        "boundary-original-source-map"
    );
    candidate.layouts = replacement.layouts;
    assert_eq!(
        verify_candidate(&capture.sources, &bodies, candidate)
            .expect_err("other layout source")
            .detail(),
        "boundary-layout-issuer"
    );
}

#[test]
fn native_c_private_boundary_v0_all_fourteen_primitives_and_wrapping_scalar_export_are_retained() {
    use zryna_syntax::{native_c_source_v0::raw::ExpressionKind, native_c_v0::raw::Primitive};
    let (capture, bodies, _) = reference();
    let boundary = compose_private_boundaries(&capture.sources, &bodies).expect("boundary");
    let mut used = Vec::new();
    for function in boundary.body_authority().functions() {
        for expression in function.expressions() {
            if let ExpressionKind::Intrinsic(primitive, _) = expression.source_kind()
                && !used.contains(primitive)
            {
                used.push(*primitive);
            }
        }
    }
    for primitive in [
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
        assert!(used.contains(&primitive), "missing {primitive:?}");
    }
    let ordinal =
        bodies.functions().iter().position(|function| function.name() == "add").expect("export");
    let scalar = &boundary.functions()[ordinal];
    assert!(scalar.private_owners.is_empty());
    assert!(
        scalar.steps.iter().all(|step| step.preparation.is_none() && step.completed.is_empty())
    );
    assert!(
        bodies.functions()[ordinal]
            .expressions()
            .iter()
            .any(|expression| matches!(expression.source_kind(), ExpressionKind::Add(_, _)))
    );
    assert_eq!(i32::MAX.wrapping_add(1), i32::MIN);
}
