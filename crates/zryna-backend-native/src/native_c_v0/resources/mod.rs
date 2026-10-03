//! Native handle entries from the retained machine seal, separate from public C exports.
//!
//! The context is compiler/driver private. Its ledger is not an OS isolation or native-recipe
//! authorization mechanism. Foreign libraries must independently satisfy their captured promises.

mod admit;
mod audit;
mod cleanup;
mod emit;
mod header;
mod ledger;
mod lower;
mod release_emit;
mod state;
mod storage;

use crate::LinuxX8664ObjectTarget;
use zryna_diagnostics::Diagnostic;
use zryna_native_mir::native_c_v0::VerifiedMirProgram;

/// Audited private-entry object retaining the complete original machine/source authority.
///
/// ```compile_fail
/// let _ = zryna_backend_native::native_c_v0::resources::ValidatedHandleEntries {
///     bytes: vec![], header: String::new(), entries: vec![], program: todo!()
/// };
/// ```
#[derive(Clone, Debug)]
pub struct ValidatedHandleEntries {
    bytes: Vec<u8>,
    header: String,
    entries: Vec<usize>,
    program: VerifiedMirProgram,
}
impl ValidatedHandleEntries {
    /// Exact audited relocatable bytes, including closed imported call relocations.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Fixed compiler-private context/input/outcome ABI with exact selected entry prototypes.
    #[must_use]
    pub fn header(&self) -> &str {
        &self.header
    }
    /// Original complete immutable authority; a link identity does not replace this issuer.
    #[must_use]
    pub const fn program(&self) -> &VerifiedMirProgram {
        &self.program
    }
    /// Exact imported operations required by the selected machine bodies and terminal drops.
    /// This immutable requirement inventory grants no library acquisition or execution authority.
    pub fn imported_operations(
        &self,
    ) -> impl Iterator<Item = zryna_native_mir::native_c_v0::VerifiedOperation<'_>> {
        let required = admit::imports(&self.program, &self.entries);
        self.program.operations().filter(move |operation| required.contains(&operation.index()))
    }
    /// Whether this artifact requires the distinct generated private storage channel.
    #[must_use]
    pub fn uses_storage_channel(&self) -> bool {
        storage::enabled(&self.program, &self.entries)
    }
    /// Exact private runtime imports from the retained branded issuer, without execution permission.
    pub fn imported_runtime_operations(
        &self,
    ) -> impl Iterator<Item = zryna_ownership_runtime_abi::VerifiedNativeFunction<'_>> {
        let names = storage::runtime_imports(&self.program, &self.entries);
        self.program
            .source()
            .runtime_abi()
            .native_linux_x86_64_functions()
            .filter(move |operation| names.contains(operation.symbol()))
    }
    /// Complete-program ordinals emitted by this distinct artifact, in original order.
    #[must_use]
    pub fn entries(&self) -> &[usize] {
        &self.entries
    }
}

/// Emits the exact selected private entries with scalar inputs/results and foreign handles.
///
/// Selections are exact compiler-private symbols from the retained seal. This increment does not
/// admit private String/Vec storage or foreign byte copies. Every selected body is checked before
/// emission, and unselected bodies remain retained without acquiring executable authority.
/// # Errors
/// Rejects missing/duplicate entries, unsupported storage/effects, code generation or object audit.
pub fn emit_handle_entries(
    program: &VerifiedMirProgram,
    entries: &[&str],
    target: LinuxX8664ObjectTarget,
) -> Result<ValidatedHandleEntries, Diagnostic> {
    let selected = admit::entries(program, entries)?;
    let bytes = emit::object(program, &selected, target)?;
    audit::check(&bytes, program, &selected)?;
    let header = header::generate(program, &selected)?;
    Ok(ValidatedHandleEntries { bytes, header, entries: selected, program: program.clone() })
}

#[cfg(test)]
mod tests;

/// Emits selected source-bound byte loans, foreign byte copies and private owned results.
///
/// The distinct generated private storage channel is retained by this artifact. The existing
/// handle-only entry point keeps its original admission and channel. Runtime declarations grant
/// no runtime object, foreign recipe, host execution permission or public C aggregate ABI.
/// # Errors
/// Rejects unsupported effects, missing genuine runtime operations, emission or independent audit.
pub fn emit_byte_entries(
    program: &VerifiedMirProgram,
    entries: &[&str],
    target: LinuxX8664ObjectTarget,
) -> Result<ValidatedHandleEntries, Diagnostic> {
    let selected = admit::storage_entries(program, entries)?;
    let bytes = emit::object(program, &selected, target)?;
    audit::check(&bytes, program, &selected)?;
    let header = header::generate(program, &selected)?;
    Ok(ValidatedHandleEntries { bytes, header, entries: selected, program: program.clone() })
}
