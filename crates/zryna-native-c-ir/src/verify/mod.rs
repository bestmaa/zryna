//! Ordered, bounded admission. Producer code is never called from this module.

use crate::{IrError, VerifiedNativeCProgram, raw, require};
use zryna_ir::data_ownership_v1 as limits;
use zryna_semantics::native_c_v0::body::VerifiedPrivateBoundaries;
use zryna_source::SourceMap;
use zryna_syntax::native_c_source_v0 as syntax;

mod calls;
mod exits;
mod source;
mod storage;
mod values;

/// Independently admits complete untrusted IR against the real original issuer.
/// # Errors
/// Rejects bounded shape, identity/target/ABI/resource, source/value, storage or exit claims
/// atomically. One error is returned; C4108 remains reserved for a terminal diagnostic report.
pub fn verify(
    program: raw::Program,
    sources: &SourceMap,
    authority: &VerifiedPrivateBoundaries,
) -> Result<VerifiedNativeCProgram, IrError> {
    preflight(&program)?;
    calls::declarations(&program, authority)?;
    source::check(&program, sources, authority)?;
    for (claim, original) in program.functions.iter().zip(authority.body_authority().functions()) {
        values::check(claim, original).map_err(|e| e.at(claim.span))?;
        calls::function(claim, original, &program.declarations).map_err(|e| e.at(claim.span))?;
    }
    storage::check(&program, authority)?;
    for (claim, original) in program.functions.iter().zip(authority.functions()) {
        exits::check(claim, original).map_err(|e| e.at(claim.span))?;
    }
    Ok(VerifiedNativeCProgram { program, authority: authority.clone() })
}

pub(super) fn count(
    current: usize,
    added: usize,
    maximum: usize,
    metric: &'static str,
) -> Result<usize, IrError> {
    current
        .checked_add(added)
        .filter(|v| *v <= maximum)
        .ok_or_else(|| IrError::new("ZRYNA-C4107", metric))
}

fn preflight(program: &raw::Program) -> Result<(), IrError> {
    let d = &program.declarations;
    count(0, d.sources.len(), 256, "ir-source-budget")?;
    count(0, d.libraries.len(), 16, "ir-library-budget")?;
    count(0, d.operations.len(), 256, "ir-operation-budget")?;
    count(0, d.sites.len(), 4096, "ir-site-budget")?;
    count(0, program.functions.len(), syntax::MAX_PROJECT_FUNCTIONS, "ir-function-budget")?;
    declaration_payloads(d)?;
    // Runtime reservations remain conditional across frames. This is not a static 64-owner cap.
    let mut values = 0;
    let mut parameters = 0;
    let mut statements = 0;
    for f in &program.functions {
        source::preflight_names(f)?;
        count(0, f.parameters.len(), limits::MAX_PARAMETERS_PER_FUNCTION, "ir-parameter-budget")?;
        parameters = count(
            parameters,
            f.parameters.len(),
            limits::MAX_PARAMETERS_PER_PROGRAM,
            "ir-program-parameter-budget",
        )?;
        count(
            0,
            f.parameter_layouts.len(),
            limits::MAX_PARAMETERS_PER_FUNCTION,
            "ir-layout-parameter-budget",
        )?;
        count(0, f.values.len(), limits::MAX_VALUES_PER_FUNCTION, "ir-value-budget")?;
        values = count(
            values,
            f.values.len(),
            limits::MAX_VALUES_PER_PROGRAM,
            "ir-program-value-budget",
        )?;
        count(0, f.statements.len(), syntax::MAX_STATEMENTS, "ir-statement-budget")?;
        statements = count(
            statements,
            f.statements.len(),
            syntax::MAX_PROJECT_STATEMENTS,
            "ir-program-statement-budget",
        )?;
        count(
            0,
            f.effects.len(),
            limits::MAX_OWNERSHIP_TRANSITIONS_PER_FUNCTION,
            "ir-effect-budget",
        )?;
        count(0, f.private_owners.len(), limits::MAX_PLACES_PER_FUNCTION, "ir-owner-budget")?;
        for value in &f.values {
            match &value.kind {
                raw::ValueKind::Key(key) => {
                    count(0, key.len(), 257, "ir-value-key-budget")?;
                }
                raw::ValueKind::Primitive(_, args) => {
                    count(0, args.len(), 17, "ir-argument-budget")?;
                }
                _ => {}
            }
        }
        let mut plans = 0;
        let mut drops = 0;
        for effect in &f.effects {
            calls::preflight_effect(&effect.operation)?;
            storage::preflight_preparation(effect.preparation.as_ref())?;
            plans = count(
                plans,
                effect.exits.len(),
                limits::MAX_CLEANUP_PLANS_PER_FUNCTION,
                "ir-exit-budget",
            )?;
            count(
                0,
                effect.completed.len(),
                limits::MAX_PLACES_PER_FUNCTION,
                "ir-completion-budget",
            )?;
            for exit in &effect.exits {
                drops = count(
                    drops,
                    exit.cleanup.len(),
                    limits::MAX_DROP_ACTIONS_PER_FUNCTION,
                    "ir-drop-budget",
                )?;
                count(
                    0,
                    exit.end_loans.len(),
                    limits::MAX_ACTIVE_BORROWS_PER_FUNCTION,
                    "ir-loan-budget",
                )?;
                count(
                    0,
                    exit.unresolved.len(),
                    limits::MAX_PLACES_PER_FUNCTION,
                    "ir-unresolved-budget",
                )?;
            }
        }
    }
    require(program.storage.runtime.len() <= 128, "ZRYNA-C4107", "ir-runtime-string-budget")
}

fn declaration_payloads(d: &zryna_syntax::native_c_v0::raw::DeclarationSet) -> Result<(), IrError> {
    let mut strings = 0;
    let mut string = |s: &str, maximum, metric| {
        count(0, s.len(), maximum, metric)?;
        strings = count(strings, s.len(), 65_536, "ir-declaration-string-budget")?;
        Ok::<(), IrError>(())
    };
    for s in [
        &d.format,
        &d.target,
        &d.abi,
        &d.convention,
        &d.carriers,
        &d.ownership,
        &d.runtime_contract,
    ] {
        string(s, 128, "ir-identity-string-budget")?;
    }
    for record in &d.sources {
        string(&record.path, 256, "ir-path-budget")?;
        string(&record.sha256, 64, "ir-digest-budget")?;
    }
    for library in &d.libraries {
        string(&library.id, 128, "ir-name-budget")?;
        string(&library.version, 128, "ir-name-budget")?;
        string(&library.header_sha256, 64, "ir-digest-budget")?;
        string(&library.policy_sha256, 64, "ir-digest-budget")?;
        count(0, library.kinds.len(), 16, "ir-kind-budget")?;
        count(0, library.allocators.len(), 16, "ir-allocator-budget")?;
        for kind in &library.kinds {
            string(kind, 257, "ir-key-budget")?;
        }
        for a in &library.allocators {
            for s in [&a.id, &a.kind, &a.create, &a.release] {
                string(s, 257, "ir-key-budget")?;
            }
        }
    }
    for operation in &d.operations {
        count(0, operation.parameters.len(), 16, "ir-carrier-budget")?;
        count(0, operation.resources.len(), 8, "ir-resource-budget")?;
        count(0, operation.statuses.len(), 16, "ir-status-budget")?;
        for s in [&operation.key, &operation.library] {
            string(s, 257, "ir-key-budget")?;
        }
        string(&operation.symbol, 128, "ir-symbol-budget")?;
        string(&operation.logical_name, 128, "ir-name-budget")?;
        string(&operation.source_binding.path, 256, "ir-path-budget")?;
        string(&operation.source_binding.sha256, 64, "ir-digest-budget")?;
        for parameter in &operation.parameters {
            string(&parameter.name, 128, "ir-name-budget")?;
        }
        for resource in &operation.resources {
            count(0, resource.slots.len(), 16, "ir-resource-slot-budget")?;
            for s in [&resource.kind, &resource.allocator, &resource.release] {
                string(s, 257, "ir-key-budget")?;
            }
        }
        for status in &operation.statuses {
            count(0, status.initialized.len(), 16, "ir-output-budget")?;
            count(0, status.new_owners.len(), 8, "ir-acquisition-budget")?;
        }
    }
    for site in &d.sites {
        string(&site.path, 256, "ir-path-budget")?;
        string(&site.source_sha256, 64, "ir-digest-budget")?;
        string(&site.spelling, 4096, "ir-spelling-budget")?;
        if let Some(key) = &site.operation {
            string(key, 257, "ir-key-budget")?;
        }
    }
    Ok(())
}
