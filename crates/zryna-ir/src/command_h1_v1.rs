//! Distinct command admission; an ordinary ownership authority cannot admit a host effect.

use zryna_diagnostics::Diagnostic;
use zryna_layout::{TypeCategory, VerifiedLayouts};
use zryna_source::SourceMap;
use zryna_syntax::command_h1_v1::CommandSyntax;

use crate::data_ownership_v1::{self as owned, raw};

mod consumption;
mod identity;
mod source_body;
pub use identity::{MemoryPartition, ProgramIdentity};

#[cfg(test)]
mod tests;

/// Distinct language authority, without an ordinary M3 program accessor.
///
/// ```compile_fail
/// fn substitute(command: &zryna_ir::command_h1_v1::VerifiedProgram) {
///     let _: &zryna_ir::data_ownership_v1::VerifiedProgram = command;
/// }
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedProgram {
    body: owned::VerifiedProgram,
    source: CommandSyntax,
    identity: ProgramIdentity,
}

impl VerifiedProgram {
    /// Returns the issuing identity of this exact command body.
    #[must_use]
    pub const fn identity(&self) -> ProgramIdentity {
        self.identity
    }

    /// Returns its distinct runtime contract, including the memory partition.
    #[must_use]
    pub const fn runtime_contract(&self) -> owned::RuntimeContractIdentity {
        owned::RuntimeContractIdentity::CommandH1V1
    }

    /// Returns its closed command memory partition witness.
    #[must_use]
    pub const fn memory_partition(&self) -> MemoryPartition {
        MemoryPartition::H1
    }

    /// Returns source admission retained by this exact command program.
    #[must_use]
    pub const fn source(&self) -> &CommandSyntax {
        &self.source
    }

    /// Returns the independently verified linear storage layouts.
    #[must_use]
    pub const fn linear32_layouts(&self) -> &VerifiedLayouts {
        self.body.linear32_layouts()
    }

    /// Returns the paired native layout witness for shared ownership ABI binding.
    #[must_use]
    pub const fn linux_x86_64_layouts(&self) -> &VerifiedLayouts {
        self.body.linux_x86_64_layouts()
    }

    /// Returns the exact verified type universe, without exposing an ordinary M3 program.
    #[must_use]
    pub const fn type_universe_identity(&self) -> zryna_layout::TypeUniverseIdentity {
        self.body.type_universe_identity()
    }

    /// Returns the paired sealed scalar entry ABI without exposing an ordinary M3 program.
    #[must_use]
    pub const fn scalar_abi(&self) -> &zryna_abi::VerifiedScalarAbiModule {
        self.body.scalar_abi()
    }

    /// Returns immutable, owner-branded language-body views for command lowering.
    #[must_use]
    pub fn modules(&self) -> impl ExactSizeIterator<Item = owned::VerifiedModule<'_>> {
        self.body.modules()
    }
}

/// Seals one command's exact source, entry, environment operation, ownership and cleanup.
///
/// # Errors
/// Rejects stale/omitted/substituted source and effects, an invalid entry or outcome layout,
/// or any failed ordinary language-body verifier obligation. This approves no host grant.
pub fn verify(
    program: raw::Program,
    sources: &SourceMap,
    source: &CommandSyntax,
    linear32: VerifiedLayouts,
    linux_x86_64: VerifiedLayouts,
) -> Result<VerifiedProgram, Vec<Diagnostic>> {
    let entry = source
        .syntax()
        .files()
        .first()
        .ok_or_else(|| vec![error("command source is absent")])?
        .id();
    let body = owned::verify_owned(program, sources, entry, linear32, linux_x86_64, Some(source))?;
    consumption::verify(&body, source)?;
    let identity = ProgramIdentity::issue(sources.identity())
        .ok_or_else(|| vec![error("command issuing identity space is exhausted")])?;
    Ok(VerifiedProgram { body, source: source.clone(), identity })
}

pub(crate) fn admit_body(
    program: &raw::Program,
    sources: &SourceMap,
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
    command: Option<&CommandSyntax>,
) -> Result<(), Vec<Diagnostic>> {
    let contains_effect = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .any(|instruction| {
            matches!(instruction.kind, raw::InstructionKind::EnvironmentLookup { .. })
        });
    match command {
        None if contains_effect => {
            Err(vec![error("environment operations cannot enter DataOwnershipV1")])
        }
        None => Ok(()),
        Some(source) => source_body::verify(program, sources, source, linear, linux),
    }
}

pub(crate) fn outcome_shape(layouts: &VerifiedLayouts, ty: raw::TypeId) -> Option<(u32, u32)> {
    let record = layouts.types().find(|record| record.id().index() == ty.0)?;
    let [found, missing] = record.variants() else {
        return None;
    };
    let string = layouts.type_by_id(found.payload()?)?;
    (record.category() == TypeCategory::Enum
        && record.drop_kind() != 0
        && found.ordinal() == 0
        && missing.ordinal() == 1
        && missing.payload().is_none()
        && string.category() == TypeCategory::String)
        .then(|| record.nominal_identity())
        .flatten()
}

fn error(message: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-I4100",
        None,
        message,
        "derive command source, entry, exact environment effect and owned cleanup before admission",
    )
}
