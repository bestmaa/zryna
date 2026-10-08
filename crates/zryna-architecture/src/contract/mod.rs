use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use crate::filesystem::read::read_bounded_utf8;
use crate::filesystem::safe_relative_path;
use crate::filesystem::valid_id;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;
use zryna_diagnostics::Diagnostic;

const MAX_REGISTERED_COMPONENTS: usize = 256;
pub(super) const MAX_CONTRACT_BYTES: u64 = 1024 * 1024;
pub(super) const CONTRACT_PROFILE: &str = "zryna-compiler-workspace-v1";
pub(super) const CONTRACT_VERSION: u32 = 1;
pub(super) const CONTRACT_FILE: &str = "zryna.workspace.json";

/// Authoritative workspace contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceContract {
    /// Editor schema reference.
    #[serde(rename = "$schema")]
    pub schema: String,
    /// Contract version.
    pub version: u32,
    /// Exact architecture profile.
    pub profile: String,
    /// Registered Rust components.
    pub members: Vec<MemberContract>,
    /// Registered replaceable frontend adapters.
    pub adapters: Vec<AdapterContract>,
    /// Exclusive generated output roots.
    pub outputs: Vec<String>,
}

/// One registered Rust component.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MemberContract {
    /// Cargo package and architecture identifier.
    pub id: String,
    /// Workspace-relative root.
    pub root: String,
    /// Architecture layer.
    pub kind: MemberKind,
    /// Exact allowed internal direct dependencies.
    pub dependencies: Vec<String>,
    /// Allowed immediate files and directories, excluding generated vendor/output directories.
    pub allowed_entries: Vec<String>,
}

/// Architecture layer classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemberKind {
    /// Base contracts and validation.
    Foundation,
    /// Replaceable source acquisition.
    Frontend,
    /// Target-neutral compiler logic.
    Compiler,
    /// Target-specific output generation.
    Backend,
    /// Pipeline orchestration.
    Orchestrator,
    /// User-facing entrypoint.
    Application,
}

/// External frontend adapter declaration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AdapterContract {
    /// Adapter identity.
    pub id: String,
    /// Workspace-relative root.
    pub root: String,
    /// ZRYNA-owned protocol version.
    pub protocol_version: u32,
    /// Exact external toolchain package.
    pub toolchain: String,
    /// Allowed immediate files and directories, excluding generated vendor/output directories.
    pub allowed_entries: Vec<String>,
}

pub(super) fn load_contract(root: &Path) -> Result<(WorkspaceContract, String), Diagnostic> {
    let path = root.join(CONTRACT_FILE);
    let (source, _) = read_bounded_utf8(
        &path,
        Some(Path::new(CONTRACT_FILE)),
        MAX_CONTRACT_BYTES,
        "ZRYNA-A1001",
        "restore the canonical zryna.workspace.json file",
    )?;
    let contract = serde_json::from_str(&source).map_err(|error| {
        architecture_error(
            "ZRYNA-A1001",
            Some(&path),
            format!("workspace contract is invalid: {error}"),
            "match schemas/zryna-workspace-v1.schema.json exactly; unknown fields are forbidden",
        )
    })?;
    Ok((contract, source))
}

pub(super) fn validate_contract_unchanged(
    root: &Path,
    expected_source: &str,
    diagnostics: &mut ValidationDiagnostics,
) {
    let path = root.join(CONTRACT_FILE);
    match read_bounded_utf8(
        &path,
        Some(Path::new(CONTRACT_FILE)),
        MAX_CONTRACT_BYTES,
        "ZRYNA-A1001",
        "restore the canonical zryna.workspace.json file",
    ) {
        Ok((source, _)) if source == expected_source => {}
        Ok(_) => diagnostics.push(architecture_error(
            "ZRYNA-A1203",
            Some(Path::new(CONTRACT_FILE)),
            "workspace contract changed during architecture validation",
            "stop concurrent mutation and retry architecture validation",
        )),
        Err(diagnostic) => diagnostics.push(diagnostic),
    }
}

pub(super) fn validate_contract_identity(
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) {
    if contract.version != CONTRACT_VERSION || contract.profile != CONTRACT_PROFILE {
        diagnostics.push(architecture_error(
            "ZRYNA-A1001",
            Some(Path::new(CONTRACT_FILE)),
            "workspace contract version or profile is unsupported",
            format!("use version {CONTRACT_VERSION} and profile {CONTRACT_PROFILE}"),
        ));
    }
    if contract.schema != "./schemas/zryna-workspace-v1.schema.json" {
        diagnostics.push(architecture_error(
            "ZRYNA-A1001",
            Some(Path::new(CONTRACT_FILE)),
            "workspace schema reference is not canonical",
            "set $schema to ./schemas/zryna-workspace-v1.schema.json",
        ));
    }
    if contract.members.is_empty() {
        diagnostics.push(architecture_error(
            "ZRYNA-A1001",
            Some(Path::new(CONTRACT_FILE)),
            "workspace contract must register at least one Rust member",
            "register the compiler components explicitly",
        ));
    }
    let component_count = contract.members.len().saturating_add(contract.adapters.len());
    if component_count > MAX_REGISTERED_COMPONENTS {
        diagnostics.push(architecture_error(
            "ZRYNA-A1001",
            Some(Path::new(CONTRACT_FILE)),
            format!(
                "workspace contract registers {component_count} components; the limit is {MAX_REGISTERED_COMPONENTS}"
            ),
            "split unrelated components into another workspace or reduce the registry",
        ));
    }
    let expected_outputs = BTreeSet::from([".zryna/cache", ".zryna/out", "target"]);
    let actual_outputs: BTreeSet<&str> = contract.outputs.iter().map(String::as_str).collect();
    if actual_outputs != expected_outputs || contract.outputs.len() != expected_outputs.len() {
        diagnostics.push(architecture_error(
            "ZRYNA-A1001",
            Some(Path::new(CONTRACT_FILE)),
            "generated output roots differ from the strict architecture profile",
            "declare target, .zryna/cache, and .zryna/out exactly once",
        ));
    }
}

pub(super) fn validate_contract_paths(
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) {
    let mut identities = BTreeSet::new();
    let mut roots = BTreeSet::new();
    for (id, component_root) in contract
        .members
        .iter()
        .map(|member| (&member.id, &member.root))
        .chain(contract.adapters.iter().map(|adapter| (&adapter.id, &adapter.root)))
    {
        if diagnostics.is_halted() {
            return;
        }
        if !valid_id(id) || !identities.insert(id.to_ascii_lowercase()) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1003",
                Some(Path::new(CONTRACT_FILE)),
                format!("component id '{id}' is invalid or collides case-insensitively"),
                "use one unique lowercase kebab-case identifier",
            ));
        }
        if !safe_relative_path(component_root) || !roots.insert(component_root.to_ascii_lowercase())
        {
            diagnostics.push(architecture_error(
                "ZRYNA-A1003",
                Some(Path::new(component_root)),
                format!("component root '{component_root}' is unsafe or duplicated"),
                "use a unique normalized workspace-relative path without traversal or backslashes",
            ));
        }
    }
    for member in &contract.members {
        if diagnostics.is_halted() {
            return;
        }
        let parent = if member.kind == MemberKind::Application { "apps" } else { "crates" };
        let expected = format!("{parent}/{}", member.id);
        if member.root != expected {
            diagnostics.push(architecture_error(
                "ZRYNA-A1003",
                Some(Path::new(&member.root)),
                format!(
                    "member root '{}' is not the canonical portable root '{expected}'",
                    member.root
                ),
                "place applications under apps/<id> and all library members under crates/<id>",
            ));
        }
    }
    for adapter in &contract.adapters {
        if diagnostics.is_halted() {
            return;
        }
        let expected = format!("adapters/{}", adapter.id);
        if adapter.root != expected {
            diagnostics.push(architecture_error(
                "ZRYNA-A1003",
                Some(Path::new(&adapter.root)),
                format!(
                    "adapter root '{}' is not the canonical portable root '{expected}'",
                    adapter.root
                ),
                "place every frontend adapter under adapters/<id>",
            ));
        }
    }
}
