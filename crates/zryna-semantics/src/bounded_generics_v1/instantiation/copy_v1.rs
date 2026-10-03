//! Copy-lane source producer from complete closed semantic discovery.

use super::InstanceContext;

/// Produces raw successor Copy-lane claims from the retained original symbolic-body context.
///
/// This grants no executable authority: the new wire decoder and independent IR issuer remain
/// mandatory. It neither changes the current M3 lowering path nor accepts peer runtime ABI seals.
///
/// # Errors
/// Rejects incomplete discovery/layout equality, unsupported original bodies or allocation.
pub fn produce_claim(
    instances: &InstanceContext<'_, '_, '_>,
    linear: &zryna_layout::generic_v1::VerifiedLayouts,
    linux: &zryna_layout::generic_v1::VerifiedLayouts,
) -> Result<zryna_ir::generic_v1::raw::Program, zryna_ir::generic_v1::Failure> {
    let mut keys = Vec::new();
    keys.try_reserve_exact(instances.function_keys().len())
        .map_err(|_| zryna_ir::generic_v1::Failure::AllocationFailure)?;
    keys.extend(instances.function_keys());
    if instances.type_keys().len() != linear.types().len()
        || instances.type_keys().zip(linear.types()).any(|(key, ty)| key != ty.key())
    {
        return Err(zryna_ir::generic_v1::Failure::InternalFailure);
    }
    let original = instances.bodies.declarations();
    zryna_ir::generic_v1::copy_v1::produce_claim(
        original.syntax(),
        original.sources(),
        linear,
        linux,
        &keys,
    )
}
