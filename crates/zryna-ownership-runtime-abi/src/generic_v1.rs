//! Additive runtime declaration issuer for the successor closed generic layout universe.
//!
//! This proves declarations, not an allocator or executable runtime implementation. Its
//! source-bound identities cannot be supplied to the older ownership-runtime issuer.

use sha2::{Digest, Sha256};
use zryna_layout::StorageTarget;
use zryna_layout::generic_v1::{TypeId, VerifiedLayouts};

use super::{RuntimeAbiViolation, RuntimeAbiViolationKind, Violations, raw};

mod metadata;
#[cfg(test)]
mod tests;
mod views;

pub use views::{ControlLayout, ElementLayout, OperationId, OperationView};

/// Separate declaration domain; it never changes the frozen older ABI identifier.
pub const IDENTIFIER: &str = "zryna-generic-ownership-runtime-v1";

/// Atomic failure without a partially issued authority.
#[derive(Debug)]
pub enum Failure {
    /// Bounded exact-declaration or dual-layout rejection.
    Violations(Vec<RuntimeAbiViolation>),
    /// Fallible temporary storage could not be reserved.
    AllocationFailure,
}

/// Exact compilation brand and stable declaration fingerprint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Identity {
    brand: TypeId,
    fingerprint: [u8; 32],
}

impl Identity {
    /// Stable exact declaration fingerprint; the separate compilation brand is also required.
    #[must_use]
    pub const fn fingerprint(self) -> [u8; 32] {
        self.fingerprint
    }
}

/// Runtime declaration authority retaining both complete generic layout issuers.
///
/// ```compile_fail
/// fn older(new: zryna_ownership_runtime_abi::generic_v1::VerifiedOwnershipRuntimeAbi<'_>) {
///     let _: zryna_ownership_runtime_abi::VerifiedOwnershipRuntimeAbi = new;
/// }
/// ```
#[derive(Debug)]
pub struct VerifiedOwnershipRuntimeAbi<'a> {
    identity: Identity,
    linear: &'a VerifiedLayouts,
    linux: &'a VerifiedLayouts,
    contract: raw::Contract,
    elements: Vec<ElementLayout>,
    controls: Vec<ControlLayout>,
}

impl<'a> VerifiedOwnershipRuntimeAbi<'a> {
    /// Separately branded identity, including the exact source compilation.
    #[must_use]
    pub const fn identity(&self) -> Identity {
        self.identity
    }
    /// Exact retained layout authority for a selected target.
    #[must_use]
    pub const fn layouts(&self, target: StorageTarget) -> &'a VerifiedLayouts {
        match target {
            StorageTarget::Linear32V1 => self.linear,
            StorageTarget::LinuxX8664V1 => self.linux,
        }
    }
    /// Returns whether all brands and stable records match supplied authorities.
    #[must_use]
    pub fn is_bound_to(&self, linear: &VerifiedLayouts, linux: &VerifiedLayouts) -> bool {
        self.linear.source_map_identity() == linear.source_map_identity()
            && self.linux.source_map_identity() == linux.source_map_identity()
            && self.linear.universe_identity() == linear.universe_identity()
            && self.linux.universe_identity() == linux.universe_identity()
            && self.linear.fingerprint() == linear.fingerprint()
            && self.linux.fingerprint() == linux.fingerprint()
            && linear.target() == StorageTarget::Linear32V1
            && linux.target() == StorageTarget::LinuxX8664V1
    }
    /// Source-branded operation declarations in the frozen logical order.
    #[must_use]
    pub fn operations(&self) -> impl ExactSizeIterator<Item = OperationView<'_>> {
        self.contract.operations.iter().enumerate().map(|(index, declaration)| OperationView {
            id: OperationId { owner: self.identity, operation: super::OPERATIONS[index] },
            declaration,
        })
    }
    /// Looks up only operation IDs issued by this exact compilation and contract.
    #[must_use]
    pub fn operation(&self, id: OperationId) -> Option<OperationView<'_>> {
        if id.owner != self.identity {
            return None;
        }
        self.operations().find(|operation| operation.id() == id)
    }
    /// Exact target-specific Vec element stride records.
    #[must_use]
    pub fn element_layouts(&self) -> &[ElementLayout] {
        &self.elements
    }
    /// Exact target-specific shared/weak payload control block records.
    #[must_use]
    pub fn control_layouts(&self) -> &[ControlLayout] {
        &self.controls
    }
    /// Verified target/helper/header inventory, exposed immutably.
    #[must_use]
    pub const fn declarations(&self) -> &raw::Contract {
        &self.contract
    }
}

/// Builds an untrusted successor declaration claim from a coherent dual layout universe.
///
/// # Errors
/// Rejects mismatched targets, compilation brands, universes or unrepresentable metadata.
pub fn raw_v1(linear: &VerifiedLayouts, linux: &VerifiedLayouts) -> Result<raw::Contract, Failure> {
    coherent(linear, linux)?;
    let (_, controls) = metadata::derive(linear, linux)?;
    canonical(linear, linux, &controls)
}

/// Independently verifies exact declarations before issuing a separate successor authority.
///
/// # Errors
/// Rejects foreign dual layouts, older domain claims, any changed declaration and all inherited
/// declaration budgets. No partial runtime authority is returned.
pub fn verify_v1<'a>(
    contract: raw::Contract,
    linear: &'a VerifiedLayouts,
    linux: &'a VerifiedLayouts,
) -> Result<VerifiedOwnershipRuntimeAbi<'a>, Failure> {
    if !super::input_within_limits(&contract) {
        return Err(reject(RuntimeAbiViolationKind::Budget, "runtime declaration budget exceeded"));
    }
    coherent(linear, linux)?;
    let (elements, controls) = metadata::derive(linear, linux)?;
    let expected = canonical(linear, linux, &controls)?;
    let mut errors = Violations::default();
    super::compare_contract(&contract, &expected, &mut errors);
    if !errors.0.is_empty() {
        return Err(Failure::Violations(errors.finish()));
    }
    let brand = linear
        .types()
        .next()
        .ok_or_else(|| {
            reject(RuntimeAbiViolationKind::Layout, "missing canonical primitive compilation brand")
        })?
        .id();
    let mut hash = Sha256::new();
    hash.update(b"ZRYNA-GENERIC-OWNERSHIP-RUNTIME-V1\0");
    hash.update(linear.universe_identity());
    hash.update(linear.fingerprint());
    hash.update(linux.fingerprint());
    super::hash_contract(&mut hash, &contract);
    metadata::hash(&mut hash, &elements, &controls);
    let identity = Identity { brand, fingerprint: hash.finalize().into() };
    Ok(VerifiedOwnershipRuntimeAbi { identity, linear, linux, contract, elements, controls })
}

fn coherent(linear: &VerifiedLayouts, linux: &VerifiedLayouts) -> Result<(), Failure> {
    if linear.target() != StorageTarget::Linear32V1
        || linux.target() != StorageTarget::LinuxX8664V1
        || linear.source_map_identity() != linux.source_map_identity()
        || linear.universe_identity() != linux.universe_identity()
    {
        return Err(reject(
            RuntimeAbiViolationKind::Layout,
            "generic dual layouts are not one exact compilation",
        ));
    }
    Ok(())
}

fn canonical(
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
    controls: &[ControlLayout],
) -> Result<raw::Contract, Failure> {
    let claims = vec![
        raw::LayoutClaim {
            target: raw::LayoutTarget::Linear32V1,
            universe: *linear.universe_identity(),
            fingerprint: *linear.fingerprint(),
        },
        raw::LayoutClaim {
            target: raw::LayoutTarget::LinuxX8664V1,
            universe: *linux.universe_identity(),
            fingerprint: *linux.fingerprint(),
        },
    ];
    let mut contract =
        super::declarations::fixed_contract(claims, metadata::records(linear, linux, controls)?);
    IDENTIFIER.clone_into(&mut contract.identifier);
    if !super::input_within_limits(&contract) {
        return Err(reject(
            RuntimeAbiViolationKind::Budget,
            "derived runtime declaration budget exceeded",
        ));
    }
    Ok(contract)
}

fn reserve<T>(count: usize) -> Result<Vec<T>, Failure> {
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| Failure::AllocationFailure)?;
    Ok(values)
}

fn reject(kind: RuntimeAbiViolationKind, message: &str) -> Failure {
    Failure::Violations(vec![RuntimeAbiViolation {
        kind,
        declaration_index: None,
        message: message.into(),
    }])
}
