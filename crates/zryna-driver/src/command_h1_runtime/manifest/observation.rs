//! Wire observations derived from the retained command run.

use super::{
    CommandH1ExecutionRecord, CommandH1Outcome, CommandH1RunReturn, CommandH1TrapCategory,
    Component, Denial, Diagnostic, ENVIRONMENT, Execution, Grant, HOST_POLICY, Limits, Optional,
    OutcomeKind, RunReturn, TrapCategory, WORLD, envelope, invalid,
};

pub(super) fn component(
    authority: &super::super::Authority,
    stem: &str,
) -> Result<Component, Diagnostic> {
    let artifact = &authority.artifact;
    let program = authority.compiled.verified_ir();
    let world = artifact
        .world()
        .worlds()
        .iter()
        .find(|world| world.identity() == WORLD)
        .ok_or_else(invalid)?;
    Ok(Component {
        path: format!("wasi-command/{stem}.wasm"),
        kind: "wasi-command-component-v1".into(),
        sha256: hex(artifact.component_digest()),
        language_sha256: hex(artifact.language_digest()),
        storage_sha256: hex(artifact.storage_digest()),
        linear32_sha256: hex(program.linear32_layouts().fingerprint()),
        linux_x86_64_sha256: hex(program.linux_x86_64_layouts().fingerprint()),
        world_sha256: hex(artifact.world_digest()),
        wit_closure_digest: hex(artifact.wit_closure_digest()),
        wit_file_count: u32::try_from(authority.wit.len()).map_err(|_| invalid())?,
        world: WORLD.into(),
        wasi_version: "0.2.12".into(),
        packages: artifact.world().packages().to_vec(),
        explicit_imports: world.explicit_imports().to_vec(),
        resolved_imports: world.resolved_imports().to_vec(),
        exports: world.exports().to_vec(),
    })
}

pub(super) fn registry_ceilings() -> Result<[u64; 10], Diagnostic> {
    crate::profile_composition::command_registry_ceilings().map_err(|_| invalid())
}

pub(super) fn grant(key: Option<&str>) -> Vec<Grant> {
    key.map(|key| Grant {
        capability: "environment".into(),
        interface: ENVIRONMENT.into(),
        key: key.into(),
    })
    .into_iter()
    .collect()
}

pub(super) fn limits() -> Limits {
    Limits {
        fuel: envelope::FUEL,
        deadline_millis: u64::try_from(envelope::DEADLINE.as_millis())
            .expect("fixed five-second deadline"),
        max_wasm_stack_bytes: 65_536,
        backtrace_max_frames: 1,
        instances: 2,
        memories: 1,
        tables: 0,
        memory_pages: 256,
        memory_bytes: 16_777_216,
        memory_growth: false,
        shared_memory: false,
        memory64: false,
        static_start: 0,
        static_end: 65_536,
        language_start: 65_536,
        language_end: 15_728_640,
        canonical_start: 15_728_640,
        canonical_end: 16_777_216,
        max_live_transfer_entries: 16,
        max_transfer_allocations: 4096,
        max_transfer_allocation_bytes: 4096,
        max_transfer_bytes: 1_048_576,
        max_request_bytes: 4096,
        max_key_bytes: 64,
        max_value_bytes: 1024,
        max_component_bytes: 1_048_576,
        max_manifest_bytes: 16_384,
    }
}

pub(super) fn execution(record: &CommandH1ExecutionRecord) -> Execution {
    let mut result = Execution {
        kind: OutcomeKind::RunReturned,
        run_return: RunReturn::Absent,
        trap_category: Optional::Absent,
        trap_identity: Optional::Absent,
        denial: Optional::Absent,
    };
    match record.outcome() {
        CommandH1Outcome::RunReturned { result: value } => {
            result.run_return = match value {
                CommandH1RunReturn::Ok => RunReturn::Ok,
                CommandH1RunReturn::Err => RunReturn::Err,
            }
        }
        CommandH1Outcome::HostDenial { interface, operation } => {
            result.kind = OutcomeKind::HostDenial;
            result.denial = Optional::Present(Denial {
                interface: interface.clone(),
                operation: operation.clone(),
                reason: "permission-denied".into(),
                policy_revision: HOST_POLICY.into(),
            });
        }
        CommandH1Outcome::RuntimeTrap { category, identity } => {
            result.kind = OutcomeKind::RuntimeTrap;
            result.trap_category = Optional::Present(match category {
                CommandH1TrapCategory::ControlledLanguage => TrapCategory::ControlledLanguage,
                CommandH1TrapCategory::InterfaceViolation => TrapCategory::InterfaceViolation,
                CommandH1TrapCategory::HostProcessFailure => TrapCategory::HostProcessFailure,
            });
            result.trap_identity = identity
                .as_ref()
                .map_or(Optional::Absent, |identity| Optional::Present(identity.clone()));
        }
    }
    result
}

pub(super) fn hex(bytes: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(64);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("bounded digest formatting");
    }
    text
}
