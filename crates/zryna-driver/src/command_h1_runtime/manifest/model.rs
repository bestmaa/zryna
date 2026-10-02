use serde::{Deserialize, Serialize};
use zryna_abi::ScalarType;

use super::wire::{Optional, object, objects, optional_object, optional_string_enum, string_enum};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Document {
    pub schema: String,
    #[serde(deserialize_with = "object")]
    pub source: Source,
    #[serde(deserialize_with = "object")]
    pub composition: Composition,
    #[serde(deserialize_with = "object")]
    pub component: Component,
    #[serde(deserialize_with = "object")]
    pub grants: Grants,
    #[serde(deserialize_with = "object")]
    pub input: Input,
    #[serde(deserialize_with = "object")]
    pub limits: Limits,
    #[serde(deserialize_with = "object")]
    pub execution: Execution,
    #[serde(deserialize_with = "string_enum")]
    pub teardown: Teardown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Source {
    pub path: String,
    pub sha256: String,
    pub profile: String,
    pub verifier_revision: u16,
    pub program_binding: String,
    #[serde(deserialize_with = "objects")]
    pub requirements: Vec<Grant>,
    #[serde(deserialize_with = "object")]
    pub scalar_entry: ScalarEntry,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct ScalarEntry {
    pub name: String,
    pub abi_version: u16,
    pub abi_index: u32,
    pub parameters: Vec<ScalarType>,
    #[serde(deserialize_with = "string_enum")]
    pub result: ScalarType,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Composition {
    pub binding: String,
    pub root: String,
    pub language: String,
    pub row: String,
    pub world: String,
    pub policy_version: String,
    #[serde(deserialize_with = "objects")]
    pub approved: Vec<Grant>,
    pub static_quota: [u64; 10],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Component {
    pub path: String,
    pub kind: String,
    pub sha256: String,
    pub language_sha256: String,
    pub storage_sha256: String,
    pub linear32_sha256: String,
    pub linux_x86_64_sha256: String,
    pub world_sha256: String,
    pub wit_closure_digest: String,
    pub wit_file_count: u32,
    pub world: String,
    pub wasi_version: String,
    pub packages: Vec<String>,
    pub explicit_imports: Vec<String>,
    pub resolved_imports: Vec<String>,
    pub exports: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Grant {
    pub capability: String,
    pub interface: String,
    pub key: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Grants {
    #[serde(deserialize_with = "objects")]
    pub requested: Vec<Grant>,
    #[serde(deserialize_with = "objects")]
    pub effective: Vec<Grant>,
    pub registry_ceilings: [u64; 10],
    pub static_quota: [u64; 10],
    pub host_policy: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Input {
    #[serde(deserialize_with = "string_enum")]
    pub kind: InputKind,
    pub utf8_byte_count: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum InputKind {
    None,
    Missing,
    Present,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Limits {
    pub fuel: u64,
    pub deadline_millis: u64,
    pub max_wasm_stack_bytes: u32,
    pub backtrace_max_frames: u32,
    pub instances: u32,
    pub memories: u32,
    pub tables: u32,
    pub memory_pages: u32,
    pub memory_bytes: u32,
    pub memory_growth: bool,
    pub shared_memory: bool,
    pub memory64: bool,
    pub static_start: u32,
    pub static_end: u32,
    pub language_start: u32,
    pub language_end: u32,
    pub canonical_start: u32,
    pub canonical_end: u32,
    pub max_live_transfer_entries: u32,
    pub max_transfer_allocations: u32,
    pub max_transfer_allocation_bytes: u32,
    pub max_transfer_bytes: u32,
    pub max_request_bytes: u32,
    pub max_key_bytes: u32,
    pub max_value_bytes: u32,
    pub max_component_bytes: u32,
    pub max_manifest_bytes: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Execution {
    #[serde(deserialize_with = "string_enum")]
    pub kind: OutcomeKind,
    #[serde(deserialize_with = "string_enum")]
    pub run_return: RunReturn,
    #[serde(
        default,
        skip_serializing_if = "Optional::absent",
        deserialize_with = "optional_string_enum"
    )]
    pub trap_category: Optional<TrapCategory>,
    #[serde(default, skip_serializing_if = "Optional::absent")]
    pub trap_identity: Optional<String>,
    #[serde(
        default,
        skip_serializing_if = "Optional::absent",
        deserialize_with = "optional_object"
    )]
    pub denial: Optional<Denial>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Denial {
    pub interface: String,
    pub operation: String,
    pub reason: String,
    pub policy_revision: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum OutcomeKind {
    RunReturned,
    HostDenial,
    RuntimeTrap,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum RunReturn {
    Ok,
    Err,
    Absent,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum TrapCategory {
    ControlledLanguage,
    InterfaceViolation,
    HostProcessFailure,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Teardown {
    Confirmed,
    Unconfirmed,
}
