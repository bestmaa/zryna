//! Closed semantic discovery from checked original bodies, separate from executable IR.
//!
//! Keys and inventories retain the exact original authority. This phase grants no layout,
//! ownership, function-instance ID, backend or public-profile authority.

use zryna_diagnostics::Diagnostic;
use zryna_source::UntrustedSpan;

use super::body_types::{BodyTypeContext, TypeShape, TypeView};
use super::{DeclarationIdentity, DeclarationKind};

mod discovery;
mod keys;
mod model;
#[cfg(test)]
mod resource_tests;
#[cfg(test)]
mod tests;
mod types;
mod value_bounds;

use model::{Builder, Function, Node};

/// Maximum distinct closed generic function instances.
pub const MAX_FUNCTION_INSTANCES: usize = 4_096;
/// Maximum distinct closed generic data instances, including standard enum families.
pub const MAX_DATA_INSTANCES: usize = 4_096;
/// Maximum distinct ordered instantiation dependency edges.
pub const MAX_EDGES: usize = 65_536;
/// Maximum complete canonical type application depth.
pub const MAX_DEPTH: u32 = 64;
/// Maximum complete canonical instance key length.
pub const MAX_KEY_BYTES: usize = 4_096;

/// Atomic discovery failure: no incomplete inventory or executable authority escapes.
#[derive(Debug)]
pub enum InstantiationFailure {
    /// Authenticated source rejection.
    Diagnostics(Vec<Diagnostic>),
    /// Fallible temporary storage could not be reserved.
    AllocationFailure,
    /// An invariant of the retained symbolic authority was violated.
    InternalFailure,
}

/// Complete closed semantic inventory, retaining its exact original body-check authority.
///
/// It deliberately cannot be consumed by existing layout or backend entrypoints.
/// ```compile_fail
/// fn emit(context: &zryna_semantics::bounded_generics_v1::instantiation::InstanceContext<'_, '_, '_>) {
///     let _: &zryna_ir::data_ownership_v1::VerifiedProgram = context;
/// }
/// ```
#[derive(Debug)]
pub struct InstanceContext<'b, 'c, 's> {
    bodies: &'b BodyTypeContext<'c, 's>,
    types: Vec<Node>,
    type_order: Vec<usize>,
    functions: Vec<Function>,
    function_order: Vec<usize>,
    edges: Vec<(Vec<u8>, Vec<u8>)>,
}

impl<'b, 'c, 's> InstanceContext<'b, 'c, 's> {
    /// The exact original source and symbolic body authority.
    #[must_use]
    pub const fn bodies(&self) -> &'b BodyTypeContext<'c, 's> {
        self.bodies
    }

    /// Complete stored type keys in unsigned bytewise order, without layout IDs.
    #[must_use]
    pub fn type_keys(&self) -> impl ExactSizeIterator<Item = &[u8]> {
        self.type_order.iter().map(|id| self.types[*id].key.as_slice())
    }

    /// Generic function keys in unsigned bytewise order, without executable instance IDs.
    #[must_use]
    pub fn function_keys(&self) -> impl ExactSizeIterator<Item = &[u8]> {
        self.function_order.iter().map(|id| self.functions[*id].key.as_slice())
    }

    /// Deduplicated ordered dependency pairs in canonical order.
    #[must_use]
    pub fn edges(&self) -> impl ExactSizeIterator<Item = (&[u8], &[u8])> {
        self.edges.iter().map(|(from, to)| (from.as_slice(), to.as_slice()))
    }
}

/// Discovers complete closed instances only after every original body has been checked.
///
/// # Errors
/// Rejects recursion, expanding generated instances and resource excess with no partial result.
pub fn discover<'b, 'c, 's>(
    bodies: &'b BodyTypeContext<'c, 's>,
) -> Result<InstanceContext<'b, 'c, 's>, InstantiationFailure> {
    let mut builder = Builder::new(bodies)?;
    discovery::source_calls(&builder)?;
    discovery::roots(&mut builder)?;
    discovery::pending(&mut builder)?;
    discovery::value_arguments(&mut builder)?;
    builder.finish()
}

fn reserve<T>(count: usize) -> Result<Vec<T>, InstantiationFailure> {
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| InstantiationFailure::AllocationFailure)?;
    Ok(values)
}

fn push<T>(values: &mut Vec<T>, value: T) -> Result<(), InstantiationFailure> {
    values.try_reserve(1).map_err(|_| InstantiationFailure::AllocationFailure)?;
    values.push(value);
    Ok(())
}

fn copy_bytes(bytes: &[u8]) -> Result<Vec<u8>, InstantiationFailure> {
    let mut result = reserve(bytes.len())?;
    result.extend_from_slice(bytes);
    Ok(result)
}

fn checked_count(mut counts: impl Iterator<Item = usize>) -> Result<usize, InstantiationFailure> {
    counts.try_fold(0usize, |total, count| {
        total.checked_add(count).ok_or(InstantiationFailure::InternalFailure)
    })
}

fn failure(
    bodies: &BodyTypeContext<'_, '_>,
    code: &str,
    at: Option<UntrustedSpan>,
    message: String,
) -> InstantiationFailure {
    let span = at.and_then(|at| bodies.declarations().sources().verify_span(at).ok());
    let guidance = "use the admitted bounded closed-instantiation graph";
    let diagnostic = match span {
        Some(span) => Diagnostic::error_at(code, span, message, guidance),
        None => Diagnostic::error(code, None, message, guidance),
    };
    InstantiationFailure::Diagnostics(vec![diagnostic])
}

fn budget(
    bodies: &BodyTypeContext<'_, '_>,
    metric: &str,
    limit: usize,
    actual: usize,
    at: Option<UntrustedSpan>,
) -> InstantiationFailure {
    failure(bodies, "ZRYNA-M7201", at, format!("{metric} limit {limit}; rejected count {actual}"))
}
