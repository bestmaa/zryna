//! Conditional malformed, untaken and unknown-status cleanup obligations.

use super::super::super::{BoundaryDrop, FailureRoute};
use super::{
    BoundaryExitKind, BoundaryOwner, PrivateOrigin, capture, compose_private_boundaries,
    verify_bodies,
};

#[test]
fn native_c_private_boundary_v0_malformed_without_release_promise_preserves_private_cleanup_and_unresolved_foreign_owner()
 {
    let mut document = capture::document();
    let creator = document["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["symbol"] == "fixture_copy_bytes")
        .expect("creator");
    let resource = creator["resources"]
        .as_array_mut()
        .expect("resources")
        .iter_mut()
        .find(|resource| resource["access"] == "create")
        .expect("created owner");
    resource["releasableOnMalformed"] = false.into();
    let capture = capture::captured(
        &[("buffer", capture::BUFFER), ("handle", capture::HANDLE), ("scalar", capture::SCALAR)],
        false,
        document,
    );
    let bodies =
        verify_bodies(&capture.sources, &capture.declarations).expect("captured weaker promise");
    let boundary = compose_private_boundaries(&capture.sources, &bodies).expect("malformed route");
    let function = super::named(&boundary, "copied");
    let malformed = function
        .steps
        .iter()
        .zip(bodies.functions()[2].steps())
        .find_map(|(step, original)| {
            if matches!(original, super::super::super::FlowStep::Take { .. }) {
                step.exits.first()
            } else {
                None
            }
        })
        .expect("take metadata failure");
    assert_eq!(malformed.unresolved, [BoundaryOwner::Foreign(0)]);
    assert_eq!(malformed.cleanup.len(), 2);
    assert!(malformed.cleanup.iter().all(|drop| matches!(drop, BoundaryDrop::Private(_))));
}

#[test]
fn native_c_private_boundary_v0_untaken_outputs_and_unknown_status_keep_exact_conditional_obligations()
 {
    let capture = capture::append_handle(
        r#"
function untaken(bytes: Vec<i32>): i32 {
  const one: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const a: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_open", 1, one);
  const two: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
  const b: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_open", 2, two);
  if (b !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_open", b); }
  if (a !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_open", a); }
  return 0;
}
"#,
    );
    let bodies = verify_bodies(&capture.sources, &capture.declarations).expect("untaken body");
    let boundary = compose_private_boundaries(&capture.sources, &bodies).expect("combined cleanup");
    let function = super::named(&boundary, "untaken");
    let returned = &function.steps.last().expect("return").exits[0];
    assert_eq!(
        returned.cleanup.iter().map(BoundaryDrop::owner).collect::<Vec<_>>(),
        [
            BoundaryOwner::Foreign(1),
            BoundaryOwner::Foreign(0),
            BoundaryOwner::Private(PrivateOrigin::Parameter(0))
        ]
    );
    assert!(returned.cleanup.iter().all(|drop| match drop {
        BoundaryDrop::Foreign(entry) => entry.validation_required(),
        BoundaryDrop::Private(_) => true,
    }));
    let unknown = function
        .steps
        .iter()
        .flat_map(|step| &step.exits)
        .filter(|exit| exit.kind == BoundaryExitKind::ForeignFailure(FailureRoute::HostAbiFailure))
        .collect::<Vec<_>>();
    assert_eq!(unknown[1].unresolved, [BoundaryOwner::Foreign(1)]);
    assert_eq!(
        unknown[1].cleanup.iter().map(BoundaryDrop::owner).collect::<Vec<_>>(),
        [BoundaryOwner::Foreign(0), BoundaryOwner::Private(PrivateOrigin::Parameter(0))]
    );
    let declared = function
        .steps
        .iter()
        .flat_map(|step| &step.exits)
        .filter(|exit| {
            exit.kind == BoundaryExitKind::ForeignFailure(FailureRoute::DeclaredForeignError)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        declared[0].cleanup.iter().map(BoundaryDrop::owner).collect::<Vec<_>>(),
        [BoundaryOwner::Foreign(0), BoundaryOwner::Private(PrivateOrigin::Parameter(0))]
    );
}
