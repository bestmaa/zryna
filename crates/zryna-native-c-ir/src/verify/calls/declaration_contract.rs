//! Ordered target, ABI, resource and complete declaration comparisons.

use crate::{IrError, require};
use zryna_syntax::native_c_v0::raw as declaration;

pub(super) fn contract(
    candidate: &declaration::DeclarationSet,
    original: &declaration::DeclarationSet,
) -> Result<(), IrError> {
    require(candidate.target == original.target, "ZRYNA-C4103", "ir-native-target")?;
    require(
        candidate.abi == original.abi
            && candidate.convention == original.convention
            && candidate.carriers == original.carriers
            && candidate.runtime_contract == original.runtime_contract,
        "ZRYNA-C4104",
        "ir-abi-tuple",
    )?;
    for (claim, sealed) in candidate.operations.iter().zip(&original.operations) {
        require(
            claim.direction == sealed.direction
                && claim.parameters.len() == sealed.parameters.len()
                && claim
                    .parameters
                    .iter()
                    .zip(&sealed.parameters)
                    .all(|(a, b)| a.abi == b.abi && a.name == b.name)
                && claim.result == sealed.result
                && claim.effects == sealed.effects
                && claim.mode == sealed.mode
                && claim.execution == sealed.execution,
            "ZRYNA-C4104",
            "ir-operation-signature",
        )?;
    }
    for (claim, sealed) in candidate.operations.iter().zip(&original.operations) {
        require(
            claim.resources == sealed.resources
                && claim.statuses == sealed.statuses
                && claim
                    .parameters
                    .iter()
                    .zip(&sealed.parameters)
                    .all(|(a, b)| a.resource == b.resource),
            "ZRYNA-C4105",
            "ir-operation-resources",
        )?;
    }
    require(
        candidate.libraries == original.libraries && candidate.ownership == original.ownership,
        "ZRYNA-C4105",
        "ir-library-resource-policy",
    )?;
    require(candidate.sites == original.sites, "ZRYNA-C4106", "ir-exact-source-sites")?;
    // Equality covers every schema field after category-specific admission, including unused records.
    require(candidate == original, "ZRYNA-C4102", "ir-complete-declaration-contract")
}
