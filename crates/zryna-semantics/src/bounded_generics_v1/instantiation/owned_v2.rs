//! Raw owned successor production after original body typing and complete discovery.
use super::{BodyTypeContext, InstanceContext};
#[cfg(test)]
mod branches;
#[cfg(test)]
mod clone_attacks;
#[cfg(test)]
mod clones;
#[cfg(test)]
mod loops;
#[cfg(test)]
mod mixed_attacks;
#[cfg(test)]
mod mixed_clones;
#[cfg(test)]
mod mutable;
#[cfg(test)]
mod negative;
#[cfg(test)]
mod nested_attacks;
#[cfg(test)]
mod nested_clones;
#[cfg(test)]
mod nested_resources;
#[cfg(test)]
mod resources;
#[cfg(test)]
mod scalar_phi;
#[cfg(test)]
mod scalar_phi_frozen;
#[cfg(test)]
mod tests;
/// Complete discovery issued only after original ownership checking, before closed layout work.
/// ```compile_fail
/// fn bypass(old: zryna_semantics::bounded_generics_v1::instantiation::InstanceContext<'_, '_, '_>) {
///     let _: zryna_semantics::bounded_generics_v1::instantiation::owned_v2::OwnedInstanceContext<'_, '_, '_> = old;
/// }
/// ```
#[derive(Debug)]
pub struct OwnedInstanceContext<'b, 'c, 's> {
    instances: InstanceContext<'b, 'c, 's>,
}
impl OwnedInstanceContext<'_, '_, '_> {
    /// Exact closed inventory for independently issuing both target layouts.
    #[must_use]
    pub const fn instances(&self) -> &InstanceContext<'_, '_, '_> {
        &self.instances
    }
}
/// Checks every original's ownership, including unused templates, then discovers instances.
/// # Errors
/// Rejects source typing, unsupported ownership forms, loans, moves and discovery budgets.
pub fn discover<'b, 'c, 's>(
    bodies: &'b BodyTypeContext<'c, 's>,
) -> Result<OwnedInstanceContext<'b, 'c, 's>, zryna_ir::generic_v1::Failure> {
    let declarations = bodies.declarations();
    zryna_ir::generic_v1::owned_v2::check_original_bodies(
        declarations.syntax(),
        declarations.sources(),
    )
    .map_err(semantic_failure)?;
    super::discover(bodies).map(|instances| OwnedInstanceContext { instances }).map_err(|failure| {
        match failure {
            super::InstantiationFailure::Diagnostics(errors) => {
                zryna_ir::generic_v1::Failure::Diagnostics(errors)
            }
            super::InstantiationFailure::AllocationFailure => {
                zryna_ir::generic_v1::Failure::AllocationFailure
            }
            super::InstantiationFailure::InternalFailure => {
                zryna_ir::generic_v1::Failure::InternalFailure
            }
        }
    })
}
/// Produces untrusted source-owned claims; separate owned decoding and IR sealing remain mandatory.
/// # Errors
/// Rejects source ownership, unsupported forms, foreign layouts and checked amplification.
pub fn produce_claim(
    owned: &OwnedInstanceContext<'_, '_, '_>,
    linear: &zryna_layout::generic_v1::VerifiedLayouts,
    linux: &zryna_layout::generic_v1::VerifiedLayouts,
) -> Result<zryna_ir::generic_v1::owned_v2::raw::Program, zryna_ir::generic_v1::Failure> {
    let instances = owned.instances();
    if instances.type_keys().len() != linear.types().len()
        || instances.type_keys().zip(linear.types()).any(|(a, b)| a != b.key())
    {
        return Err(zryna_ir::generic_v1::Failure::InternalFailure);
    }
    let keys = instances.function_keys().collect::<Vec<_>>();
    let original = instances.bodies.declarations();
    zryna_ir::generic_v1::owned_v2::produce_claim(
        original.syntax(),
        original.sources(),
        linear,
        linux,
        &keys,
    )
    .map_err(semantic_failure)
}
fn semantic_failure(failure: zryna_ir::generic_v1::Failure) -> zryna_ir::generic_v1::Failure {
    match failure {
        zryna_ir::generic_v1::Failure::Diagnostics(mut errors) => {
            for e in &mut errors {
                if e.code == "ZRYNA-I7001" {
                    "ZRYNA-M3008".clone_into(&mut e.code);
                } else if e.code == "ZRYNA-I3201" {
                    "ZRYNA-M7201".clone_into(&mut e.code);
                }
            }
            zryna_ir::generic_v1::Failure::Diagnostics(errors)
        }
        other => other,
    }
}
