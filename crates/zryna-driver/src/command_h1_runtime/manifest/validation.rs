use zryna_abi::ScalarType;
use zryna_diagnostics::Diagnostic;
use zryna_source::NormalizedSourcePath;

use super::super::{
    CommandH1ExecutionRecord, CommandH1Outcome, CommandH1RunReturn, CommandH1Teardown,
    CommandH1TrapCategory,
};
use super::{
    COMMAND_H1_MANIFEST_SCHEMA, ENVIRONMENT, HOST_POLICY, INTERFACE_FAILURE, POLICY, PROFILE,
    WORLD, invalid, limits,
    model::{Document, Execution, InputKind, OutcomeKind, RunReturn, Teardown, TrapCategory},
    registry_ceilings,
    wire::Optional,
};

mod fingerprint;

pub(super) fn validate(document: &Document) -> Result<CommandH1ExecutionRecord, Diagnostic> {
    let source = &document.source;
    let composition = &document.composition;
    let component = &document.component;
    let grants = &document.grants;
    let entry = &source.scalar_entry;
    if document.schema != COMMAND_H1_MANIFEST_SCHEMA
        || source.profile != PROFILE
        || source.verifier_revision != 1
        || entry.name != "main"
        || entry.abi_version != 1
        || entry.abi_index != 0
        || !entry.parameters.is_empty()
        || entry.result != ScalarType::Bool
        || NormalizedSourcePath::new(source.path.clone()).is_err()
        || composition.root != "command-source"
        || composition.language != "CommandH1V1"
        || composition.row != "WitCommand"
        || composition.world != WORLD
        || composition.policy_version != POLICY
        || component.kind != "wasi-command-component-v1"
        || component.world != WORLD
        || component.wasi_version != "0.2.12"
        || component.wit_file_count != 34
        || grants.host_policy != HOST_POLICY
        || grants.registry_ceilings != registry_ceilings()?
        || document.limits != limits()
    {
        return Err(invalid());
    }
    let stem = component
        .path
        .strip_prefix("wasi-command/")
        .and_then(|path| path.strip_suffix(".wasm"))
        .ok_or_else(invalid)?;
    crate::javascript::validate_artifact_stem(stem).map_err(|_| invalid())?;
    let requirements = &source.requirements;
    if requirements.len() > 1
        || requirements.iter().any(|grant| {
            grant.capability != "environment"
                || grant.interface != ENVIRONMENT
                || grant.key.is_empty()
                || grant.key.len() > 64
                || grant.key.contains(['\\', '\n', '\r', '\0'])
        })
        || composition.approved != *requirements
        || grants.requested != *requirements
        || grants.effective != *requirements
    {
        return Err(invalid());
    }
    let mut quota = [0; 10];
    if !requirements.is_empty() {
        quota[2] = 1;
        quota[3] = 1088;
    }
    if grants.static_quota != quota
        || composition.static_quota != quota
        || document.input.utf8_byte_count > 1024
        || (document.input.kind != InputKind::Present && document.input.utf8_byte_count != 0)
        || (requirements.is_empty() != (document.input.kind == InputKind::None))
    {
        return Err(invalid());
    }
    for digest in [
        &source.sha256,
        &source.program_binding,
        &composition.binding,
        &component.sha256,
        &component.language_sha256,
        &component.storage_sha256,
        &component.linear32_sha256,
        &component.linux_x86_64_sha256,
        &component.world_sha256,
        &component.wit_closure_digest,
    ] {
        fingerprint::digest(digest)?;
    }
    fingerprint::validate(document)?;
    let outcome = outcome(&document.execution, !requirements.is_empty())?;
    let teardown = match document.teardown {
        Teardown::Confirmed => CommandH1Teardown::Confirmed,
        Teardown::Unconfirmed => CommandH1Teardown::Unconfirmed,
    };
    Ok(CommandH1ExecutionRecord::new(outcome, teardown))
}

fn outcome(execution: &Execution, has_environment: bool) -> Result<CommandH1Outcome, Diagnostic> {
    match execution.kind {
        OutcomeKind::RunReturned
            if execution.trap_category.absent()
                && execution.trap_identity.absent()
                && execution.denial.absent() =>
        {
            let result = match execution.run_return {
                RunReturn::Ok => CommandH1RunReturn::Ok,
                RunReturn::Err => CommandH1RunReturn::Err,
                RunReturn::Absent => return Err(invalid()),
            };
            Ok(CommandH1Outcome::RunReturned { result })
        }
        OutcomeKind::HostDenial
            if has_environment
                && execution.run_return == RunReturn::Absent
                && execution.trap_category.absent()
                && execution.trap_identity.absent() =>
        {
            let Optional::Present(denial) = &execution.denial else { return Err(invalid()) };
            if denial.interface != ENVIRONMENT
                || denial.operation != "get-environment"
                || denial.reason != "permission-denied"
                || denial.policy_revision != HOST_POLICY
            {
                return Err(invalid());
            }
            Ok(CommandH1Outcome::HostDenial {
                interface: denial.interface.clone(),
                operation: denial.operation.clone(),
            })
        }
        OutcomeKind::RuntimeTrap
            if execution.run_return == RunReturn::Absent && execution.denial.absent() =>
        {
            let Optional::Present(category) = &execution.trap_category else {
                return Err(invalid());
            };
            let category = match category {
                TrapCategory::ControlledLanguage => {
                    let Optional::Present(identity) = &execution.trap_identity else {
                        return Err(invalid());
                    };
                    if ![
                        "zryna.trap.bounds-v1",
                        "zryna.trap.allocation-v1",
                        "zryna.trap.capacity-v1",
                        "zryna.trap.refcount-v1",
                        "zryna.trap.utf8-v1",
                    ]
                    .contains(&identity.as_str())
                    {
                        return Err(invalid());
                    }
                    CommandH1TrapCategory::ControlledLanguage
                }
                TrapCategory::InterfaceViolation => {
                    if execution.trap_identity != Optional::Present(INTERFACE_FAILURE.into()) {
                        return Err(invalid());
                    }
                    CommandH1TrapCategory::InterfaceViolation
                }
                TrapCategory::HostProcessFailure => {
                    if !execution.trap_identity.absent() {
                        return Err(invalid());
                    }
                    CommandH1TrapCategory::HostProcessFailure
                }
            };
            Ok(CommandH1Outcome::RuntimeTrap {
                category,
                identity: match &execution.trap_identity {
                    Optional::Absent => None,
                    Optional::Present(identity) => Some(identity.clone()),
                },
            })
        }
        _ => Err(invalid()),
    }
}
