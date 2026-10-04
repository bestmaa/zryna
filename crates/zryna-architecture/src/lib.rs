//! Canonical fail-closed Zryna repository architecture engine.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use same_file::Handle;
use serde::{Deserialize, Serialize};
use zryna_diagnostics::Diagnostic;

const CONTRACT_FILE: &str = "zryna.workspace.json";
const CONTRACT_VERSION: u32 = 1;
const CONTRACT_PROFILE: &str = "zryna-compiler-workspace-v1";
const MAX_CONTRACT_BYTES: u64 = 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_SCANNED_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SCAN_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SCAN_ENTRIES: usize = 50_000;
const MAX_SCAN_DEPTH: usize = 32;
const MAX_SCAN_DIAGNOSTICS: usize = 256;
const MAX_REGISTERED_COMPONENTS: usize = 256;
const MAX_CARGO_METADATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_CARGO_STDERR_BYTES: usize = 64 * 1024;
const MAX_CARGO_PACKAGES: usize = 4096;
const MAX_CARGO_EDGES: usize = 65_536;
const MAX_CARGO_METADATA_DURATION: Duration = Duration::from_secs(30);

const REQUIRED_ROOT_FILES: &[&str] =
    &["Cargo.lock", "Cargo.toml", "rust-toolchain.toml", CONTRACT_FILE];
const REQUIRED_ROOT_DIRECTORIES: &[&str] = &["adapters", "apps", "crates"];

const ALLOWED_ROOT_ENTRIES: &[&str] = &[
    ".cargo",
    ".git",
    ".github",
    ".gitattributes",
    ".gitignore",
    ".zryna",
    "CODE_OF_CONDUCT.md",
    "CONTRIBUTING.md",
    "Cargo.lock",
    "Cargo.toml",
    "LICENSE",
    "NOTICE",
    "README.md",
    "SECURITY.md",
    "adapters",
    "apps",
    "crates",
    "docs",
    "editors",
    "examples",
    "node_modules",
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "runtime",
    "rust-toolchain.toml",
    "rustfmt.toml",
    "schemas",
    "scripts",
    "spec",
    "target",
    "tests",
    "toolchains",
    "zryna.workspace.json",
];

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

#[derive(Clone, Debug, Deserialize)]
struct CargoMetadataDocument {
    version: u32,
    packages: Vec<CargoMetadataPackage>,
    workspace_members: Vec<String>,
    workspace_root: String,
    resolve: Option<CargoMetadataResolve>,
}

#[derive(Clone, Debug, Deserialize)]
struct CargoMetadataPackage {
    id: String,
    name: String,
    manifest_path: String,
    source: Option<String>,
    #[serde(default)]
    dependencies: Vec<CargoMetadataDependency>,
}

#[derive(Clone, Debug, Deserialize)]
struct CargoMetadataDependency {
    name: String,
    rename: Option<String>,
    kind: Option<String>,
    target: Option<String>,
    path: Option<String>,
    optional: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct CargoMetadataResolve {
    nodes: Vec<CargoMetadataNode>,
}

#[derive(Clone, Debug, Deserialize)]
struct CargoMetadataNode {
    id: String,
    #[serde(default)]
    deps: Vec<CargoMetadataNodeDependency>,
}

#[derive(Clone, Debug, Deserialize)]
struct CargoMetadataNodeDependency {
    name: String,
    pkg: String,
    #[serde(default)]
    dep_kinds: Vec<CargoMetadataDependencyKind>,
}

#[derive(Clone, Debug, Deserialize)]
struct CargoMetadataDependencyKind {
    kind: Option<String>,
    target: Option<String>,
}

struct CargoInputSnapshot {
    relative_path: PathBuf,
    source: Option<String>,
    max_bytes: u64,
}

type InternalDependencyGraph = BTreeMap<String, BTreeSet<String>>;

/// Complete result of one architecture check.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationReport {
    /// Stable diagnostics in deterministic order.
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// Whether the architecture is valid.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

#[derive(Default)]
struct ValidationDiagnostics {
    values: Vec<Diagnostic>,
    halted: bool,
}

impl ValidationDiagnostics {
    fn push(&mut self, diagnostic: Diagnostic) {
        if self.halted {
            return;
        }
        if self.values.len().saturating_add(1) >= MAX_SCAN_DIAGNOSTICS {
            self.halt(architecture_error(
                "ZRYNA-A1204",
                None,
                "architecture validation exceeded its deterministic diagnostic budget",
                "reduce invalid controlled input; incomplete validation never passes",
            ));
            return;
        }
        self.values.push(diagnostic);
    }

    fn halt(&mut self, diagnostic: Diagnostic) {
        if self.halted {
            return;
        }
        if self.values.len() >= MAX_SCAN_DIAGNOSTICS {
            self.values.truncate(MAX_SCAN_DIAGNOSTICS.saturating_sub(1));
        }
        self.values.push(diagnostic);
        self.halted = true;
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    const fn is_halted(&self) -> bool {
        self.halted
    }

    fn into_vec(self) -> Vec<Diagnostic> {
        self.values
    }
}

/// Validates a complete Zryna workspace and fails closed on incomplete inspection.
#[must_use]
pub fn validate_workspace(root: &Path) -> ValidationReport {
    let mut diagnostics = ValidationDiagnostics::default();
    let canonical_root = match canonical_workspace_root(root) {
        Ok(value) => value,
        Err(diagnostic) => return ValidationReport { diagnostics: vec![diagnostic] },
    };
    let (contract, contract_source) = match load_contract(&canonical_root) {
        Ok(value) => value,
        Err(diagnostic) => return ValidationReport { diagnostics: vec![diagnostic] },
    };

    validate_contract_identity(&contract, &mut diagnostics);
    if !diagnostics.is_empty() {
        return validation_report(diagnostics);
    }
    validate_contract_paths(&contract, &mut diagnostics);
    if !diagnostics.is_empty() {
        return validation_report(diagnostics);
    }

    let scan_started_with = diagnostics.len();
    let scan_completed =
        validate_bounded_filesystem(&canonical_root, &contract, &contract_source, &mut diagnostics);
    if !scan_completed || diagnostics.len() != scan_started_with {
        return validation_report(diagnostics);
    }

    validate_root_entries(&canonical_root, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_required_root_shapes(&canonical_root, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_component_containers(&canonical_root, &contract, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_paths(&canonical_root, &contract, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_component_entries(&canonical_root, &contract, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    let mut cargo_inputs = validate_members(&canonical_root, &contract, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_adapters(&canonical_root, &contract, &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    let mut resolved_graph = None;
    if diagnostics.is_empty() && cargo_inputs.len() == contract.members.len().saturating_add(1) {
        let lock_snapshot = read_required_cargo_input_snapshot(
            &canonical_root,
            "Cargo.lock",
            MAX_SCANNED_FILE_BYTES,
            &mut diagnostics,
        );
        let toolchain_snapshot = read_required_cargo_input_snapshot(
            &canonical_root,
            "rust-toolchain.toml",
            MAX_MANIFEST_BYTES,
            &mut diagnostics,
        );
        if let (Some(lock_snapshot), Some(toolchain_snapshot)) = (lock_snapshot, toolchain_snapshot)
        {
            cargo_inputs.extend([lock_snapshot, toolchain_snapshot]);
            read_optional_cargo_input_snapshots(
                &canonical_root,
                &mut cargo_inputs,
                &mut diagnostics,
            );
            if diagnostics.is_empty()
                && let Some(metadata) = load_cargo_metadata(&canonical_root, true, &mut diagnostics)
            {
                resolved_graph = Some(validate_resolved_cargo_graph(
                    &canonical_root,
                    &contract,
                    &metadata,
                    &mut diagnostics,
                ));
            }
        }
        if !diagnostics.is_halted() {
            validate_cargo_inputs_unchanged(&canonical_root, &cargo_inputs, &mut diagnostics);
        }
    }
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_dependency_graph(&contract, resolved_graph.as_ref(), &mut diagnostics);
    if diagnostics.is_halted() {
        return validation_report(diagnostics);
    }
    validate_contract_unchanged(&canonical_root, &contract_source, &mut diagnostics);

    validation_report(diagnostics)
}

fn validation_report(diagnostics: ValidationDiagnostics) -> ValidationReport {
    let mut values = diagnostics.into_vec();
    values.sort_by(|left, right| {
        (left.code(), left.path(), left.message()).cmp(&(
            right.code(),
            right.path(),
            right.message(),
        ))
    });
    ValidationReport { diagnostics: values }
}

fn canonical_workspace_root(root: &Path) -> Result<PathBuf, Diagnostic> {
    let metadata = fs::symlink_metadata(root).map_err(|error| {
        architecture_error(
            "ZRYNA-A1001",
            Some(root),
            format!("workspace root is unavailable: {error}"),
            "select an existing regular directory containing zryna.workspace.json",
        )
    })?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(architecture_error(
            "ZRYNA-A1201",
            Some(root),
            "workspace root must be a real, non-symlink directory",
            "open the canonical project directory directly",
        ));
    }
    fs::canonicalize(root).map_err(|error| {
        architecture_error(
            "ZRYNA-A1001",
            Some(root),
            format!("workspace root cannot be canonicalized: {error}"),
            "fix workspace permissions and path components",
        )
    })
}

fn read_bounded_utf8(
    path: &Path,
    diagnostic_path: Option<&Path>,
    max_bytes: u64,
    unavailable_code: &str,
    unavailable_guidance: &str,
) -> Result<(String, u64), Diagnostic> {
    let policy = ControlledReadPolicy {
        diagnostic_path,
        max_bytes,
        unavailable_code,
        unavailable_guidance,
        expected_size: None,
    };
    read_bounded_utf8_with_hooks(path, policy, || {}, || {})
}

fn read_bounded_utf8_with_expected_size(
    path: &Path,
    diagnostic_path: Option<&Path>,
    max_bytes: u64,
    unavailable_code: &str,
    unavailable_guidance: &str,
    expected_size: u64,
) -> Result<(String, u64), Diagnostic> {
    let policy = ControlledReadPolicy {
        diagnostic_path,
        max_bytes,
        unavailable_code,
        unavailable_guidance,
        expected_size: Some(expected_size),
    };
    read_bounded_utf8_with_hooks(path, policy, || {}, || {})
}

#[derive(Clone, Copy)]
struct ControlledReadPolicy<'a> {
    diagnostic_path: Option<&'a Path>,
    max_bytes: u64,
    unavailable_code: &'a str,
    unavailable_guidance: &'a str,
    expected_size: Option<u64>,
}

fn read_bounded_utf8_with_hooks<BeforeOpen, AfterRead>(
    path: &Path,
    policy: ControlledReadPolicy<'_>,
    before_open: BeforeOpen,
    after_read: AfterRead,
) -> Result<(String, u64), Diagnostic>
where
    BeforeOpen: FnOnce(),
    AfterRead: FnOnce(),
{
    let (mut handle, opened) = open_controlled_file(
        path,
        policy.diagnostic_path,
        policy.max_bytes,
        policy.unavailable_code,
        policy.unavailable_guidance,
    )?;
    if policy.expected_size.is_some_and(|expected| expected != opened.len()) {
        return Err(architecture_error(
            "ZRYNA-A1203",
            policy.diagnostic_path,
            "controlled file size changed before its bounded read",
            "stop concurrent replacement and retry architecture validation",
        ));
    }
    before_open();
    validate_current_controlled_path(path, policy.diagnostic_path, &handle, &opened)?;

    let mut bytes = Vec::new();
    handle.as_file_mut().take(policy.max_bytes.saturating_add(1)).read_to_end(&mut bytes).map_err(
        |error| {
            architecture_error(
                "ZRYNA-A1203",
                policy.diagnostic_path,
                format!("controlled file could not be read completely: {error}"),
                "restore stable read access and retry",
            )
        },
    )?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > policy.max_bytes) {
        return Err(architecture_error(
            "ZRYNA-A1204",
            policy.diagnostic_path,
            format!("controlled file exceeds its {}-byte safety limit", policy.max_bytes),
            "reduce the file before architecture validation",
        ));
    }

    after_read();
    revalidate_controlled_file(path, policy.diagnostic_path, &handle, &opened)?;
    let length = u64::try_from(bytes.len()).map_err(|_| {
        architecture_error(
            "ZRYNA-A1204",
            policy.diagnostic_path,
            "controlled file length cannot be represented safely",
            "reduce the file before architecture validation",
        )
    })?;
    String::from_utf8(bytes).map(|source| (source, length)).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            policy.diagnostic_path,
            format!("controlled file is not valid UTF-8: {error}"),
            "save controlled source and manifest files as UTF-8",
        )
    })
}

fn open_controlled_file(
    path: &Path,
    diagnostic_path: Option<&Path>,
    max_bytes: u64,
    unavailable_code: &str,
    unavailable_guidance: &str,
) -> Result<(Handle, fs::Metadata), Diagnostic> {
    let link_metadata = fs::symlink_metadata(path).map_err(|error| {
        architecture_error(
            unavailable_code,
            diagnostic_path,
            format!("controlled file is unavailable: {error}"),
            unavailable_guidance,
        )
    })?;
    if metadata_is_link_or_reparse(&link_metadata) || !link_metadata.is_file() {
        return Err(architecture_error(
            "ZRYNA-A1201",
            diagnostic_path,
            "controlled input must be a regular, non-symlink file",
            "replace it with a regular in-workspace UTF-8 file",
        ));
    }
    let file = open_regular_no_follow(path).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file could not be opened safely: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    let handle = Handle::from_file(file).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file identity is unavailable: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    let opened = handle.as_file().metadata().map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("opened file metadata is unavailable: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    if metadata_is_link_or_reparse(&opened) || !opened.is_file() {
        return Err(architecture_error(
            "ZRYNA-A1201",
            diagnostic_path,
            "opened controlled input is not a regular file",
            "replace it with a regular in-workspace UTF-8 file",
        ));
    }
    if opened.len() > max_bytes {
        return Err(architecture_error(
            "ZRYNA-A1204",
            diagnostic_path,
            format!("controlled file exceeds its {max_bytes}-byte safety limit"),
            "reduce the file before architecture validation",
        ));
    }
    validate_current_controlled_path(path, diagnostic_path, &handle, &opened)?;
    Ok((handle, opened))
}

fn validate_current_controlled_path(
    path: &Path,
    diagnostic_path: Option<&Path>,
    reference: &Handle,
    opened: &fs::Metadata,
) -> Result<(), Diagnostic> {
    let path_metadata = fs::symlink_metadata(path).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file path changed during inspection: {error}"),
            "stop concurrent replacement and retry architecture validation",
        )
    })?;
    if metadata_is_link_or_reparse(&path_metadata) || !path_metadata.is_file() {
        return Err(architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            "controlled file path became a link or non-file during inspection",
            "stop concurrent replacement and retry architecture validation",
        ));
    }
    let file = open_regular_no_follow(path).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file could not be opened safely: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    let current = Handle::from_file(file).map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file identity is unavailable: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    let current_metadata = current.as_file().metadata().map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("current file metadata is unavailable: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    if reference != &current || !same_file_state(opened, &current_metadata) {
        return Err(architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            "controlled file identity or state changed during inspection",
            "stop concurrent replacement and retry architecture validation",
        ));
    }
    Ok(())
}

fn revalidate_controlled_file(
    path: &Path,
    diagnostic_path: Option<&Path>,
    handle: &Handle,
    opened: &fs::Metadata,
) -> Result<(), Diagnostic> {
    let after = handle.as_file().metadata().map_err(|error| {
        architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            format!("controlled file could not be revalidated: {error}"),
            "restore a stable readable regular file and retry",
        )
    })?;
    if !same_file_state(opened, &after) {
        return Err(architecture_error(
            "ZRYNA-A1203",
            diagnostic_path,
            "controlled file changed during inspection",
            "stop concurrent modification and retry architecture validation",
        ));
    }
    validate_current_controlled_path(path, diagnostic_path, handle, opened)
}

#[cfg(unix)]
fn open_regular_no_follow(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = fs::OpenOptions::new();
    options.read(true);
    options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK);
    options.open(path)
}

#[cfg(windows)]
fn open_regular_no_follow(path: &Path) -> std::io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    let mut options = fs::OpenOptions::new();
    options.read(true).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).share_mode(FILE_SHARE_READ);
    options.open(path)
}

#[cfg(not(any(unix, windows)))]
fn open_regular_no_follow(_path: &Path) -> std::io::Result<fs::File> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "no fail-closed no-follow file strategy exists for this platform",
    ))
}

#[cfg(windows)]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(unix)]
fn same_file_state(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    left.size() == right.size()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

#[cfg(windows)]
fn same_file_state(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    left.file_attributes() == right.file_attributes()
        && left.creation_time() == right.creation_time()
        && left.last_write_time() == right.last_write_time()
        && left.file_size() == right.file_size()
}

#[cfg(not(any(unix, windows)))]
fn same_file_state(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

fn load_contract(root: &Path) -> Result<(WorkspaceContract, String), Diagnostic> {
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

fn validate_contract_unchanged(
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

fn validate_contract_identity(
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

fn validate_contract_paths(contract: &WorkspaceContract, diagnostics: &mut ValidationDiagnostics) {
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

fn validate_root_entries(root: &Path, diagnostics: &mut ValidationDiagnostics) {
    let allowed: BTreeSet<&str> = ALLOWED_ROOT_ENTRIES.iter().copied().collect();
    let portable_allowed: BTreeMap<String, &str> =
        ALLOWED_ROOT_ENTRIES.iter().map(|value| (value.to_ascii_lowercase(), *value)).collect();
    let Some(entries) = sorted_directory_entries(root, root, diagnostics) else {
        return;
    };
    for entry in entries {
        if diagnostics.is_halted() {
            return;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            diagnostics.push(architecture_error(
                "ZRYNA-A1203",
                Some(&entry.path()),
                "workspace entry name is not valid UTF-8",
                "rename the entry with a portable UTF-8 name",
            ));
            continue;
        };
        if !allowed.contains(name) {
            if let Some(expected) = portable_allowed.get(&name.to_ascii_lowercase()) {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1003",
                    Some(Path::new(name)),
                    format!("root entry '{name}' has noncanonical spelling; expected '{expected}'"),
                    "rename the entry to its exact portable spelling",
                ));
                continue;
            }
            diagnostics.push(architecture_error(
                "ZRYNA-A1004",
                Some(Path::new(name)),
                format!("root entry '{name}' is not part of the Zryna architecture"),
                "move the content into a registered component or redefine the contract deliberately",
            ));
        }
    }
}

fn validate_required_root_shapes(root: &Path, diagnostics: &mut ValidationDiagnostics) {
    for relative in REQUIRED_ROOT_FILES {
        if diagnostics.is_halted() {
            return;
        }
        let path = root.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(relative)),
                format!("required root entry '{relative}' is not a regular file"),
                "restore the canonical root file shape",
            )),
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(relative)),
                format!("required root file '{relative}' is unavailable: {error}"),
                "restore the canonical root file",
            )),
        }
    }
    for relative in REQUIRED_ROOT_DIRECTORIES {
        if diagnostics.is_halted() {
            return;
        }
        let path = root.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(relative)),
                format!("required root entry '{relative}' is not a directory"),
                "restore the canonical root directory shape",
            )),
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(relative)),
                format!("required root directory '{relative}' is unavailable: {error}"),
                "restore the canonical root directory",
            )),
        }
    }
}

fn validate_component_containers(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) {
    let applications: BTreeSet<String> = contract
        .members
        .iter()
        .filter(|member| member.kind == MemberKind::Application)
        .map(|member| member.id.clone())
        .collect();
    let libraries: BTreeSet<String> = contract
        .members
        .iter()
        .filter(|member| member.kind != MemberKind::Application)
        .map(|member| member.id.clone())
        .collect();
    let adapters: BTreeSet<String> =
        contract.adapters.iter().map(|adapter| adapter.id.clone()).collect();
    for (container, expected) in
        [("apps", applications), ("crates", libraries), ("adapters", adapters)]
    {
        if diagnostics.is_halted() {
            return;
        }
        let container_path = root.join(container);
        let Some(entries) =
            sorted_directory_entries(&container_path, Path::new(container), diagnostics)
        else {
            continue;
        };
        let portable_expected: BTreeMap<String, &str> =
            expected.iter().map(|value| (value.to_ascii_lowercase(), value.as_str())).collect();
        let mut actual = BTreeSet::new();
        for entry in entries {
            if diagnostics.is_halted() {
                return;
            }
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1203",
                    entry.path().strip_prefix(root).ok(),
                    "component directory name is not valid UTF-8",
                    "rename the directory with its registered printable ASCII id",
                ));
                continue;
            };
            let relative = Path::new(container).join(&name);
            if expected.contains(name.as_str()) {
                match entry.file_type() {
                    Ok(file_type) if file_type.is_dir() => {
                        actual.insert(name);
                    }
                    Ok(_) => diagnostics.push(architecture_error(
                        "ZRYNA-A1005",
                        Some(&relative),
                        "registered component root is not a directory",
                        "restore the registered component directory",
                    )),
                    Err(error) => diagnostics.push(architecture_error(
                        "ZRYNA-A1205",
                        Some(&relative),
                        format!("component root type could not be inspected: {error}"),
                        "restore directory consistency and retry",
                    )),
                }
                continue;
            }
            if let Some(canonical) = portable_expected.get(&name.to_ascii_lowercase()) {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1003",
                    Some(&relative),
                    format!(
                        "component root '{container}/{name}' has noncanonical spelling; expected '{container}/{canonical}'"
                    ),
                    "rename the directory to its exact registered portable spelling",
                ));
            } else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1005",
                    Some(&relative),
                    format!("unregistered component root '{container}/{name}'"),
                    "register the component deliberately or remove it from the controlled container",
                ));
            }
        }
        for missing in expected.difference(&actual) {
            if diagnostics.is_halted() {
                return;
            }
            diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(&Path::new(container).join(missing)),
                format!("registered component root '{container}/{missing}' is missing"),
                "restore the exact registered component directory",
            ));
        }
    }
}

fn sorted_directory_entries(
    path: &Path,
    diagnostic_path: &Path,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<Vec<fs::DirEntry>> {
    let entries = match fs::read_dir(path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(diagnostic_path),
                format!("directory scan failed: {error}"),
                "restore read access; incomplete scans never pass",
            ));
            return None;
        }
    };
    let mut sorted = Vec::new();
    for entry in entries {
        if diagnostics.is_halted() {
            return None;
        }
        if sorted.len() >= MAX_SCAN_ENTRIES {
            diagnostics.halt(architecture_error(
                "ZRYNA-A1204",
                Some(diagnostic_path),
                "directory validation exceeded its deterministic entry budget",
                "reduce controlled entries; incomplete validation never passes",
            ));
            return None;
        }
        match entry {
            Ok(value) => sorted.push(value),
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(diagnostic_path),
                format!("directory entry could not be inspected: {error}"),
                "restore directory consistency and retry",
            )),
        }
    }
    sorted.sort_by_key(fs::DirEntry::file_name);
    Some(sorted)
}

fn validate_paths(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) {
    for member_root in contract
        .members
        .iter()
        .map(|member| &member.root)
        .chain(contract.adapters.iter().map(|adapter| &adapter.root))
    {
        if diagnostics.is_halted() {
            return;
        }
        if exact_relative_entry(root, Path::new(member_root)).is_none() {
            diagnostics.push(architecture_error(
                "ZRYNA-A1003",
                Some(Path::new(member_root)),
                "component root spelling differs from its registered portable identity",
                "restore every path segment with its exact registered spelling",
            ));
            continue;
        }
        let joined = root.join(member_root);
        match fs::canonicalize(&joined) {
            Ok(canonical) if canonical.starts_with(root) => {}
            Ok(_) => diagnostics.push(architecture_error(
                "ZRYNA-A1202",
                Some(Path::new(member_root)),
                "component root escapes the canonical workspace",
                "place the component inside the workspace without symlink indirection",
            )),
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(member_root)),
                format!("registered component root is unavailable: {error}"),
                "create it with the canonical project planner",
            )),
        }
    }
}

fn exact_relative_entry(root: &Path, relative: &Path) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(expected) = component else {
            return None;
        };
        let entries = fs::read_dir(&current).ok()?;
        let mut matched = None;
        for entry in entries {
            let entry = entry.ok()?;
            if entry.file_name() == expected {
                matched = Some(entry.path());
                break;
            }
        }
        current = matched?;
    }
    Some(current)
}

fn exact_regular_file(root: &Path, relative: &Path) -> bool {
    exact_relative_entry(root, relative).is_some_and(|path| {
        fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
    })
}

fn validate_component_entries(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) {
    for (component_root, allowed_entries, allows_node_modules) in
        contract.members.iter().map(|member| (&member.root, &member.allowed_entries, false)).chain(
            contract.adapters.iter().map(|adapter| (&adapter.root, &adapter.allowed_entries, true)),
        )
    {
        if diagnostics.is_halted() {
            return;
        }
        let mut allowed = BTreeSet::new();
        let mut portable_identities = BTreeSet::new();
        for entry in allowed_entries {
            if diagnostics.is_halted() {
                return;
            }
            if !valid_component_entry(entry)
                || !allowed.insert(entry.as_str())
                || !portable_identities.insert(entry.to_ascii_lowercase())
            {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1003",
                    Some(Path::new(component_root)),
                    format!("allowed component entry '{entry}' is invalid or duplicated"),
                    "use each portable immediate file or directory name exactly once",
                ));
            }
        }
        let path = root.join(component_root);
        let Some(entries) = sorted_directory_entries(&path, Path::new(component_root), diagnostics)
        else {
            continue;
        };
        for entry in entries {
            if diagnostics.is_halted() {
                return;
            }
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1203",
                    entry.path().strip_prefix(root).ok(),
                    "component entry name is not valid UTF-8",
                    "rename the entry with a portable UTF-8 name",
                ));
                continue;
            };
            if allows_node_modules && name == "node_modules" {
                match entry.file_type() {
                    Ok(file_type) if file_type.is_dir() => continue,
                    Ok(_) => {}
                    Err(error) => {
                        diagnostics.push(architecture_error(
                            "ZRYNA-A1205",
                            entry.path().strip_prefix(root).ok(),
                            format!("component entry type could not be inspected: {error}"),
                            "restore directory consistency and retry",
                        ));
                        continue;
                    }
                }
            }
            if !allowed.contains(name.as_str()) {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1007",
                    entry.path().strip_prefix(root).ok(),
                    format!("'{name}' is outside the registered component layout"),
                    "move it into an allowed entry or update zryna.workspace.json deliberately",
                ));
            }
        }
    }
}

fn validate_members(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) -> Vec<CargoInputSnapshot> {
    let mut snapshots = Vec::new();
    let root_manifest_path = root.join("Cargo.toml");
    let root_manifest = read_toml_with_source(&root_manifest_path, diagnostics);
    if let Some((manifest, source)) = root_manifest {
        snapshots.push(CargoInputSnapshot {
            relative_path: PathBuf::from("Cargo.toml"),
            source: Some(source),
            max_bytes: MAX_MANIFEST_BYTES,
        });
        let cargo_members: BTreeSet<String> = manifest
            .get("workspace")
            .and_then(|workspace| workspace.get("members"))
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str)
            .map(ToOwned::to_owned)
            .collect();
        let contract_members: BTreeSet<String> =
            contract.members.iter().map(|member| member.root.clone()).collect();
        if cargo_members != contract_members {
            diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new("Cargo.toml")),
                "Cargo workspace members differ from zryna.workspace.json",
                "register every Rust member exactly once in both authoritative manifests",
            ));
        }
    }
    if diagnostics.is_halted() {
        return snapshots;
    }

    for member in &contract.members {
        if diagnostics.is_halted() {
            return snapshots;
        }
        let member_root = root.join(&member.root);
        let manifest_path = member_root.join("Cargo.toml");
        let manifest_relative = Path::new(&member.root).join("Cargo.toml");
        let readme_relative = Path::new(&member.root).join("README.md");
        if !exact_regular_file(root, &readme_relative) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1006",
                Some(Path::new(&member.root)),
                "registered member is missing README.md",
                "document the component authority and dependency boundary",
            ));
        }
        let expected_entry = if member.kind == MemberKind::Application {
            Path::new(&member.root).join("src/main.rs")
        } else {
            Path::new(&member.root).join("src/lib.rs")
        };
        if !exact_regular_file(root, &expected_entry) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1006",
                Some(Path::new(&member.root)),
                "registered member has the wrong Rust entrypoint",
                "applications require src/main.rs; all other members require src/lib.rs",
            ));
        }
        if diagnostics.is_halted() {
            return snapshots;
        }
        if !exact_regular_file(root, &manifest_relative) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1006",
                Some(&manifest_relative),
                "registered member Cargo.toml is missing or has noncanonical spelling",
                "restore the exact regular Cargo.toml file",
            ));
            continue;
        }
        let Some((manifest, source)) = read_toml_with_source(&manifest_path, diagnostics) else {
            continue;
        };
        snapshots.push(CargoInputSnapshot {
            relative_path: PathBuf::from(&member.root).join("Cargo.toml"),
            source: Some(source),
            max_bytes: MAX_MANIFEST_BYTES,
        });
        let package_name = manifest
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str);
        if package_name != Some(member.id.as_str()) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1006",
                Some(&manifest_path),
                format!("Cargo package name does not match registered id '{}'", member.id),
                "make the directory, member id, and Cargo package name identical",
            ));
        }
    }
    snapshots
}

fn validate_adapters(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) {
    for adapter in &contract.adapters {
        if diagnostics.is_halted() {
            return;
        }
        let adapter_root = root.join(&adapter.root);
        let readme_path = Path::new(&adapter.root).join("README.md");
        let worker_path = Path::new(&adapter.root).join("src/worker.mjs");
        let package_path = adapter_root.join("package.json");
        let package_relative = Path::new(&adapter.root).join("package.json");
        if !exact_regular_file(root, &readme_path)
            || !exact_regular_file(root, &worker_path)
            || !exact_regular_file(root, &package_relative)
        {
            diagnostics.push(architecture_error(
                "ZRYNA-A1010",
                Some(Path::new(&adapter.root)),
                "registered frontend adapter is missing README.md or src/worker.mjs",
                "restore the documented newline-JSON worker boundary",
            ));
        }
        if diagnostics.is_halted() {
            return;
        }
        let Some(package) = read_json(&package_path, diagnostics) else {
            continue;
        };
        let expected_name = format!("@zryna/adapter-{}", adapter.id);
        if package.get("name").and_then(serde_json::Value::as_str) != Some(expected_name.as_str()) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1010",
                Some(&package_path),
                format!("adapter package name must be '{expected_name}'"),
                "make the package identity derive from the registered adapter id",
            ));
        }
        let metadata = package.get("zryna");
        let metadata_id =
            metadata.and_then(|value| value.get("adapterId")).and_then(serde_json::Value::as_str);
        let metadata_protocol = metadata
            .and_then(|value| value.get("protocolVersion"))
            .and_then(serde_json::Value::as_u64);
        let metadata_worker =
            metadata.and_then(|value| value.get("worker")).and_then(serde_json::Value::as_str);
        if metadata_id != Some(adapter.id.as_str())
            || metadata_protocol != Some(u64::from(adapter.protocol_version))
            || metadata_worker != Some("src/worker.mjs")
        {
            diagnostics.push(architecture_error(
                "ZRYNA-A1010",
                Some(&package_path),
                "adapter metadata differs from zryna.workspace.json",
                "set zryna.adapterId, zryna.protocolVersion, and zryna.worker to the registered values",
            ));
        }
        let Some((toolchain_name, toolchain_version)) = adapter.toolchain.rsplit_once('@') else {
            diagnostics.push(architecture_error(
                "ZRYNA-A1011",
                Some(Path::new(CONTRACT_FILE)),
                format!(
                    "adapter toolchain '{}' is not an exact package@version",
                    adapter.toolchain
                ),
                "pin one exact frontend package version in the architecture contract",
            ));
            continue;
        };
        let dependency_version = package
            .get("dependencies")
            .and_then(|value| value.get(toolchain_name))
            .and_then(serde_json::Value::as_str);
        if toolchain_name.is_empty()
            || toolchain_version.is_empty()
            || dependency_version != Some(toolchain_version)
        {
            diagnostics.push(architecture_error(
                "ZRYNA-A1011",
                Some(&package_path),
                format!("adapter must pin toolchain '{}' exactly", adapter.toolchain),
                "make package.json dependencies match the registered package and version",
            ));
        }
    }
}

fn read_required_cargo_input_snapshot(
    root: &Path,
    relative: &str,
    max_bytes: u64,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<CargoInputSnapshot> {
    let relative_path = PathBuf::from(relative);
    let path = root.join(&relative_path);
    match read_bounded_utf8(
        &path,
        Some(&relative_path),
        max_bytes,
        "ZRYNA-A1005",
        "restore the required Cargo graph input",
    ) {
        Ok((source, _)) => {
            Some(CargoInputSnapshot { relative_path, source: Some(source), max_bytes })
        }
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            None
        }
    }
}

fn read_optional_cargo_input_snapshots(
    root: &Path,
    snapshots: &mut Vec<CargoInputSnapshot>,
    diagnostics: &mut ValidationDiagnostics,
) {
    for relative in [".cargo/config.toml", ".cargo/config"] {
        if diagnostics.is_halted() {
            return;
        }
        let relative_path = PathBuf::from(relative);
        let path = root.join(&relative_path);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(noncanonical) = find_noncanonical_case_entry(root, &relative_path) {
                    diagnostics.push(architecture_error(
                        "ZRYNA-A1003",
                        Some(&noncanonical),
                        "Cargo configuration input has noncanonical filesystem spelling",
                        "use the exact portable .cargo/config.toml or .cargo/config spelling",
                    ));
                    continue;
                }
                snapshots.push(CargoInputSnapshot {
                    relative_path,
                    source: None,
                    max_bytes: MAX_MANIFEST_BYTES,
                });
                continue;
            }
            Err(error) => {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1205",
                    Some(&relative_path),
                    format!("Cargo configuration input could not be inspected: {error}"),
                    "restore stable repository-local Cargo configuration",
                ));
                continue;
            }
        };
        if exact_relative_entry(root, &relative_path).is_none() {
            diagnostics.push(architecture_error(
                "ZRYNA-A1003",
                Some(&relative_path),
                "Cargo configuration input has noncanonical filesystem spelling",
                "use the exact portable .cargo/config.toml or .cargo/config spelling",
            ));
            continue;
        }
        if !metadata.file_type().is_file() {
            diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(&relative_path),
                "Cargo configuration input is not a regular file",
                "restore the canonical repository-local Cargo configuration file",
            ));
            continue;
        }
        match read_bounded_utf8(
            &path,
            Some(&relative_path),
            MAX_MANIFEST_BYTES,
            "ZRYNA-A1203",
            "restore stable repository-local Cargo configuration",
        ) {
            Ok((source, _)) => snapshots.push(CargoInputSnapshot {
                relative_path,
                source: Some(source),
                max_bytes: MAX_MANIFEST_BYTES,
            }),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
}

fn find_noncanonical_case_entry(root: &Path, relative: &Path) -> Option<PathBuf> {
    let parent = relative.parent()?;
    let expected = relative.file_name()?.to_str()?;
    let parent_path = exact_relative_entry(root, parent)?;
    let mut names: Vec<OsString> = fs::read_dir(parent_path)
        .ok()?
        .filter_map(|entry| entry.ok().map(|value| value.file_name()))
        .collect();
    names.sort();
    names.into_iter().find_map(|name| {
        let value = name.to_str()?;
        (value != expected && value.eq_ignore_ascii_case(expected)).then(|| parent.join(value))
    })
}

fn validate_cargo_inputs_unchanged(
    root: &Path,
    snapshots: &[CargoInputSnapshot],
    diagnostics: &mut ValidationDiagnostics,
) {
    for snapshot in snapshots {
        if diagnostics.is_halted() {
            return;
        }
        let path = root.join(&snapshot.relative_path);
        if let Some(expected_source) = &snapshot.source {
            match read_bounded_utf8(
                &path,
                Some(&snapshot.relative_path),
                snapshot.max_bytes,
                "ZRYNA-A1203",
                "stop concurrent Cargo input mutation and retry architecture validation",
            ) {
                Ok((source, _)) if source == *expected_source => {}
                Ok(_) => diagnostics.push(architecture_error(
                    "ZRYNA-A1203",
                    Some(&snapshot.relative_path),
                    "Cargo graph input changed during architecture validation",
                    "stop concurrent Cargo input mutation and retry architecture validation",
                )),
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
            continue;
        }
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if find_noncanonical_case_entry(root, &snapshot.relative_path).is_some() {
                    diagnostics.push(architecture_error(
                        "ZRYNA-A1203",
                        Some(&snapshot.relative_path),
                        "noncanonical Cargo graph input was created during validation",
                        "stop concurrent Cargo input mutation and retry architecture validation",
                    ));
                }
            }
            Ok(_) => diagnostics.push(architecture_error(
                "ZRYNA-A1203",
                Some(&snapshot.relative_path),
                "Cargo graph input was created during architecture validation",
                "stop concurrent Cargo input mutation and retry architecture validation",
            )),
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(&snapshot.relative_path),
                format!("Cargo graph input state could not be inspected: {error}"),
                "restore stable Cargo input paths and retry architecture validation",
            )),
        }
    }
}

fn load_cargo_metadata(
    root: &Path,
    frozen: bool,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<CargoMetadataDocument> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    load_cargo_metadata_with_executable(
        root,
        frozen,
        cargo,
        MAX_CARGO_METADATA_DURATION,
        diagnostics,
    )
}

fn load_cargo_metadata_with_executable(
    root: &Path,
    frozen: bool,
    cargo: OsString,
    duration: Duration,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<CargoMetadataDocument> {
    let mut command = Command::new(cargo);
    command
        .current_dir(root)
        .arg("metadata")
        .arg("--format-version=1")
        .arg("--all-features")
        .arg("--color=never")
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .env("CARGO_TERM_COLOR", "never")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if frozen {
        command.arg("--frozen");
    }
    let mut child = match zryna_process::spawn(|| command.spawn()) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata could not be started: {error}"),
                "install the pinned Cargo toolchain and restore the locked dependency graph",
            ));
            return None;
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
        let _ = child.kill();
        let _ = child.wait();
        diagnostics.push(architecture_error(
            "ZRYNA-A1205",
            Some(Path::new("Cargo.toml")),
            "Cargo metadata process streams could not be inspected",
            "restore the pinned Cargo toolchain; incomplete graph inspection never passes",
        ));
        return None;
    };
    let stdout_reader = spawn_process_reader(stdout, MAX_CARGO_METADATA_BYTES);
    let stderr_reader = spawn_process_reader(stderr, MAX_CARGO_STDERR_BYTES);
    let deadline = Instant::now() + duration;
    let wait_result = wait_for_cargo_metadata(&mut child, deadline);
    let (status, timed_out) = match wait_result {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata did not complete: {error}"),
                "restore the pinned Cargo toolchain; incomplete graph inspection never passes",
            ));
            return None;
        }
    };
    if timed_out {
        diagnostics.halt(architecture_error(
            "ZRYNA-A1204",
            Some(Path::new("Cargo.toml")),
            "Cargo metadata exceeded its deterministic execution budget",
            "reduce the locked dependency graph or restore the local Cargo cache",
        ));
        return None;
    }
    let stdout = receive_process_reader(&stdout_reader, "stdout", deadline, diagnostics);
    let stderr = receive_process_reader(&stderr_reader, "stderr", deadline, diagnostics);
    let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
        return None;
    };
    validate_cargo_process_output(status, &stdout, &stderr, diagnostics)
}

fn wait_for_cargo_metadata(
    child: &mut std::process::Child,
    deadline: Instant,
) -> std::io::Result<(std::process::ExitStatus, bool)> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok((status, false)),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                return child.wait().map(|status| (status, true));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    }
}

fn spawn_process_reader(
    stream: impl Read + Send + 'static,
    limit: usize,
) -> Receiver<std::io::Result<BoundedProcessStream>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = sender.send(read_process_stream(stream, limit));
    });
    receiver
}

fn receive_process_reader(
    receiver: &Receiver<std::io::Result<BoundedProcessStream>>,
    stream_name: &str,
    deadline: Instant,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<BoundedProcessStream> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    match receiver.recv_timeout(remaining) {
        Ok(Ok(value)) => Some(value),
        Ok(Err(error)) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata {stream_name} could not be read: {error}"),
                "restore stable process I/O and retry architecture validation",
            ));
            None
        }
        Err(RecvTimeoutError::Disconnected) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata {stream_name} reader failed"),
                "restore stable process I/O and retry architecture validation",
            ));
            None
        }
        Err(RecvTimeoutError::Timeout) => {
            diagnostics.halt(architecture_error(
                "ZRYNA-A1204",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata {stream_name} exceeded its execution budget"),
                "stop descendant processes retaining Cargo output streams and retry",
            ));
            None
        }
    }
}

fn validate_cargo_process_output(
    status: std::process::ExitStatus,
    stdout: &BoundedProcessStream,
    stderr: &BoundedProcessStream,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<CargoMetadataDocument> {
    if stdout.exceeded || stderr.exceeded {
        diagnostics.halt(architecture_error(
            "ZRYNA-A1204",
            Some(Path::new("Cargo.toml")),
            "Cargo metadata exceeded its deterministic process-output budget",
            "reduce the locked dependency graph; incomplete metadata never passes",
        ));
        return None;
    }
    if !status.success() {
        diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            "Cargo metadata rejected the locked workspace",
            "repair Cargo manifests and Cargo.lock with the pinned toolchain",
        ));
        return None;
    }
    let metadata: CargoMetadataDocument = match serde_json::from_slice(&stdout.bytes) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata output is invalid: {error}"),
                "use the pinned Cargo toolchain and metadata format version 1",
            ));
            return None;
        }
    };
    if metadata.version != 1 {
        diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            format!("Cargo metadata format version {} is unsupported", metadata.version),
            "use Cargo metadata format version 1",
        ));
        return None;
    }
    if !validate_cargo_metadata_limits(&metadata, diagnostics) {
        return None;
    }
    Some(metadata)
}

fn validate_cargo_metadata_limits(
    metadata: &CargoMetadataDocument,
    diagnostics: &mut ValidationDiagnostics,
) -> bool {
    let edge_count = metadata
        .packages
        .iter()
        .map(|package| package.dependencies.len())
        .chain(
            metadata
                .resolve
                .iter()
                .flat_map(|resolve| resolve.nodes.iter().map(|node| node.deps.len())),
        )
        .fold(0_usize, usize::saturating_add);
    if metadata.packages.len() > MAX_CARGO_PACKAGES || edge_count > MAX_CARGO_EDGES {
        diagnostics.halt(architecture_error(
            "ZRYNA-A1204",
            Some(Path::new("Cargo.toml")),
            format!(
                "Cargo metadata contains {} packages and {edge_count} dependency edges",
                metadata.packages.len()
            ),
            format!(
                "keep the graph within {MAX_CARGO_PACKAGES} packages and {MAX_CARGO_EDGES} edges"
            ),
        ));
        return false;
    }
    true
}

struct BoundedProcessStream {
    bytes: Vec<u8>,
    exceeded: bool,
}

fn read_process_stream(
    mut stream: impl Read,
    limit: usize,
) -> std::io::Result<BoundedProcessStream> {
    let mut bytes = Vec::with_capacity(limit.min(8192));
    let mut exceeded = false;
    let mut chunk = [0_u8; 8192];
    loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let stored = remaining.min(count);
        bytes.extend_from_slice(&chunk[..stored]);
        if stored != count {
            exceeded = true;
        }
    }
    Ok(BoundedProcessStream { bytes, exceeded })
}

fn validate_resolved_cargo_graph(
    root: &Path,
    contract: &WorkspaceContract,
    metadata: &CargoMetadataDocument,
    diagnostics: &mut ValidationDiagnostics,
) -> InternalDependencyGraph {
    validate_metadata_workspace_root(root, metadata, diagnostics);
    let registered_roots = registered_cargo_roots(root, contract, diagnostics);
    let internal_packages =
        map_internal_cargo_packages(root, metadata, &registered_roots, diagnostics);
    let expected_workspace: BTreeSet<String> =
        contract.members.iter().map(|member| member.id.clone()).collect();
    validate_metadata_workspace_members(
        metadata,
        &internal_packages,
        &expected_workspace,
        diagnostics,
    );
    let mut graph: InternalDependencyGraph =
        contract.members.iter().map(|member| (member.id.clone(), BTreeSet::new())).collect();
    collect_declared_cargo_edges(
        root,
        metadata,
        &registered_roots,
        &internal_packages,
        &mut graph,
        diagnostics,
    );
    collect_resolved_cargo_edges(
        metadata,
        &internal_packages,
        &expected_workspace,
        &mut graph,
        diagnostics,
    );
    compare_internal_cargo_graph(contract, &graph, diagnostics);
    graph
}

fn validate_metadata_workspace_root(
    root: &Path,
    metadata: &CargoMetadataDocument,
    diagnostics: &mut ValidationDiagnostics,
) {
    let controlled_root = match fs::canonicalize(root) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(Path::new("Cargo.toml")),
                format!("controlled workspace root cannot be identified: {error}"),
                "restore the canonical controlled workspace root",
            ));
            return;
        }
    };
    match fs::canonicalize(&metadata.workspace_root) {
        Ok(workspace_root) if workspace_root == controlled_root => {}
        Ok(workspace_root) => diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            format!(
                "Cargo metadata resolved workspace root '{}' instead of the controlled root",
                workspace_root.display()
            ),
            "run metadata only for the canonical controlled workspace",
        )),
        Err(error) => diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            format!("Cargo metadata workspace root cannot be identified: {error}"),
            "restore the canonical controlled workspace root",
        )),
    }
}

fn registered_cargo_roots(
    root: &Path,
    contract: &WorkspaceContract,
    diagnostics: &mut ValidationDiagnostics,
) -> BTreeMap<PathBuf, String> {
    let mut registered_roots = BTreeMap::new();
    for member in &contract.members {
        if diagnostics.is_halted() {
            return registered_roots;
        }
        match fs::canonicalize(root.join(&member.root)) {
            Ok(path) => {
                registered_roots.insert(path, member.id.clone());
            }
            Err(error) => diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new(&member.root)),
                format!("registered Cargo member root cannot be identified: {error}"),
                "restore the registered member directory",
            )),
        }
    }
    registered_roots
}

fn map_internal_cargo_packages(
    root: &Path,
    metadata: &CargoMetadataDocument,
    registered_roots: &BTreeMap<PathBuf, String>,
    diagnostics: &mut ValidationDiagnostics,
) -> BTreeMap<String, String> {
    let mut internal_packages = BTreeMap::new();
    let mut package_roots = BTreeMap::new();
    for package in sorted_metadata_packages(metadata) {
        if diagnostics.is_halted() {
            return internal_packages;
        }
        if package.source.is_some() {
            continue;
        }
        let manifest = PathBuf::from(&package.manifest_path);
        let canonical_manifest = match fs::canonicalize(&manifest) {
            Ok(value) => value,
            Err(error) => {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(Path::new("Cargo.toml")),
                    format!("resolved local Cargo manifest cannot be identified: {error}"),
                    "restore every resolved local package inside a registered member root",
                ));
                continue;
            }
        };
        let Some(package_root) = canonical_manifest.parent() else {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(Path::new("Cargo.toml")),
                "resolved local Cargo manifest has no package root",
                "restore every resolved local package inside a registered member root",
            ));
            continue;
        };
        let Some(member_id) = registered_roots.get(package_root) else {
            let diagnostic_path = canonical_manifest
                .strip_prefix(root)
                .map_or_else(|_| PathBuf::from("Cargo.toml"), Path::to_path_buf);
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(&diagnostic_path),
                format!("Cargo resolves unregistered local package '{}'", package.name),
                "register the package as one canonical member or remove the local dependency",
            ));
            continue;
        };
        let diagnostic_path = cargo_manifest_diagnostic_path(root, package);
        if canonical_manifest != package_root.join("Cargo.toml") {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(&diagnostic_path),
                "Cargo metadata manifest path is not the exact registered Cargo.toml",
                "bind the package id to the exact snapshotted member manifest",
            ));
        }
        if package.name != *member_id {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(&diagnostic_path),
                format!(
                    "resolved package '{}' does not match registered member '{member_id}'",
                    package.name
                ),
                "make the package name, component id, and canonical root identical",
            ));
        }
        if internal_packages.insert(package.id.clone(), member_id.clone()).is_some()
            || package_roots.insert(package_root.to_path_buf(), package.id.clone()).is_some()
        {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(&diagnostic_path),
                "Cargo metadata contains a duplicate local package identity",
                "keep exactly one package id for each registered component root",
            ));
        }
    }
    internal_packages
}

fn validate_metadata_workspace_members(
    metadata: &CargoMetadataDocument,
    internal_packages: &BTreeMap<String, String>,
    expected_workspace: &BTreeSet<String>,
    diagnostics: &mut ValidationDiagnostics,
) {
    let mut actual_workspace = BTreeSet::new();
    for package_id in &metadata.workspace_members {
        if let Some(member_id) = internal_packages.get(package_id) {
            actual_workspace.insert(member_id.clone());
        } else {
            diagnostics.push(architecture_error(
                "ZRYNA-A1005",
                Some(Path::new("Cargo.toml")),
                format!("Cargo workspace contains unregistered package id '{package_id}'"),
                "make Cargo workspace_members and zryna.workspace.json identical",
            ));
        }
    }
    if &actual_workspace != expected_workspace
        || metadata.workspace_members.len() != expected_workspace.len()
    {
        diagnostics.push(architecture_error(
            "ZRYNA-A1005",
            Some(Path::new("Cargo.toml")),
            "resolved Cargo workspace members differ from zryna.workspace.json",
            "register every resolved workspace package exactly once",
        ));
    }
}

fn collect_declared_cargo_edges(
    root: &Path,
    metadata: &CargoMetadataDocument,
    registered_roots: &BTreeMap<PathBuf, String>,
    internal_packages: &BTreeMap<String, String>,
    graph: &mut InternalDependencyGraph,
    diagnostics: &mut ValidationDiagnostics,
) {
    for package in sorted_metadata_packages(metadata) {
        let Some(source_id) = internal_packages.get(&package.id) else {
            continue;
        };
        let diagnostic_path = cargo_manifest_diagnostic_path(root, package);
        let mut dependencies: Vec<&CargoMetadataDependency> = package.dependencies.iter().collect();
        dependencies.sort_by(|left, right| {
            (&left.name, &left.rename, &left.kind, &left.target, &left.path, left.optional).cmp(&(
                &right.name,
                &right.rename,
                &right.kind,
                &right.target,
                &right.path,
                right.optional,
            ))
        });
        for dependency in dependencies {
            if diagnostics.is_halted() {
                return;
            }
            if !valid_cargo_dependency_kind(dependency.kind.as_deref())
                || dependency.rename.as_deref().is_some_and(str::is_empty)
                || dependency.target.as_deref().is_some_and(str::is_empty)
            {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(&diagnostic_path),
                    format!("Cargo dependency '{}' has unsupported metadata", dependency.name),
                    "use normal, dev, or build dependency kinds with a valid target expression",
                ));
            }
            let Some(path) = &dependency.path else {
                continue;
            };
            let canonical_dependency = match fs::canonicalize(path) {
                Ok(value) => value,
                Err(error) => {
                    diagnostics.push(architecture_error(
                        "ZRYNA-A1101",
                        Some(&diagnostic_path),
                        format!(
                            "local dependency '{}' cannot be identified: {error}",
                            dependency.name
                        ),
                        "restore the dependency inside a registered member root",
                    ));
                    continue;
                }
            };
            let Some(target_id) = registered_roots.get(&canonical_dependency) else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(&diagnostic_path),
                    format!(
                        "'{}' declares local dependency '{}' outside registered member roots",
                        source_id, dependency.name
                    ),
                    "register the local package or remove the dependency",
                ));
                continue;
            };
            if dependency.name != *target_id {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(&diagnostic_path),
                    format!(
                        "dependency metadata names '{}' but resolves registered member '{target_id}'",
                        dependency.name
                    ),
                    "bind every local dependency to its registered package identity",
                ));
            }
            graph.entry(source_id.clone()).or_default().insert(target_id.clone());
        }
    }
}

fn collect_resolved_cargo_edges(
    metadata: &CargoMetadataDocument,
    internal_packages: &BTreeMap<String, String>,
    expected_workspace: &BTreeSet<String>,
    graph: &mut InternalDependencyGraph,
    diagnostics: &mut ValidationDiagnostics,
) {
    let Some(resolve) = &metadata.resolve else {
        diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            "Cargo metadata omitted the resolved dependency graph",
            "run full metadata format version 1 without --no-deps",
        ));
        return;
    };
    let mut resolved_members = BTreeSet::new();
    let mut nodes: Vec<&CargoMetadataNode> = resolve.nodes.iter().collect();
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    for node in nodes {
        let Some(source_id) = internal_packages.get(&node.id) else {
            continue;
        };
        resolved_members.insert(source_id.clone());
        let mut dependencies: Vec<&CargoMetadataNodeDependency> = node.deps.iter().collect();
        dependencies.sort_by(|left, right| (&left.name, &left.pkg).cmp(&(&right.name, &right.pkg)));
        for dependency in dependencies {
            if dependency.name.is_empty()
                || dependency.dep_kinds.iter().any(|kind| {
                    !valid_cargo_dependency_kind(kind.kind.as_deref())
                        || kind.target.as_deref().is_some_and(str::is_empty)
                })
            {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(Path::new("Cargo.toml")),
                    format!("resolved dependency '{}' has unsupported metadata", dependency.name),
                    "use normal, dev, or build dependency kinds with a valid target expression",
                ));
            }
            if let Some(target_id) = internal_packages.get(&dependency.pkg) {
                graph.entry(source_id.clone()).or_default().insert(target_id.clone());
            }
        }
    }
    if &resolved_members != expected_workspace {
        diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            "resolved Cargo graph omits one or more registered members",
            "restore a complete full-workspace Cargo resolve graph",
        ));
    }
}

fn compare_internal_cargo_graph(
    contract: &WorkspaceContract,
    graph: &InternalDependencyGraph,
    diagnostics: &mut ValidationDiagnostics,
) {
    for member in &contract.members {
        let expected: BTreeSet<String> = member.dependencies.iter().cloned().collect();
        let actual = graph.get(&member.id).cloned().unwrap_or_default();
        if actual != expected {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(Path::new(&member.root).join("Cargo.toml").as_path()),
                format!(
                    "resolved internal dependencies for '{}' are {actual:?}, but the architecture contract declares {expected:?}",
                    member.id
                ),
                "declare the exact resolved normal, dev, build, target-specific, aliased, and patched internal graph",
            ));
        }
    }
}

fn sorted_metadata_packages(metadata: &CargoMetadataDocument) -> Vec<&CargoMetadataPackage> {
    let mut packages: Vec<&CargoMetadataPackage> = metadata.packages.iter().collect();
    packages.sort_by(|left, right| left.id.cmp(&right.id));
    packages
}

fn cargo_manifest_diagnostic_path(root: &Path, package: &CargoMetadataPackage) -> PathBuf {
    fs::canonicalize(&package.manifest_path)
        .ok()
        .and_then(|path| path.strip_prefix(root).map(Path::to_path_buf).ok())
        .unwrap_or_else(|| PathBuf::from("Cargo.toml"))
}

fn valid_cargo_dependency_kind(kind: Option<&str>) -> bool {
    kind.is_none_or(|value| matches!(value, "dev" | "build"))
}

fn validate_dependency_graph(
    contract: &WorkspaceContract,
    resolved_graph: Option<&InternalDependencyGraph>,
    diagnostics: &mut ValidationDiagnostics,
) {
    let members: BTreeMap<&str, &MemberContract> =
        contract.members.iter().map(|member| (member.id.as_str(), member)).collect();
    for member in &contract.members {
        if diagnostics.is_halted() {
            return;
        }
        let mut unique = BTreeSet::new();
        for dependency in &member.dependencies {
            if diagnostics.is_halted() {
                return;
            }
            if !unique.insert(dependency.as_str()) {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(Path::new(&member.root)),
                    format!("'{}' declares duplicate dependency '{dependency}'", member.id),
                    "declare each internal component edge exactly once",
                ));
            }
            let Some(target) = members.get(dependency.as_str()) else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(Path::new(&member.root)),
                    format!("'{}' depends on unknown member '{dependency}'", member.id),
                    "register the dependency or remove the edge",
                ));
                continue;
            };
            let _ = target;
        }
    }
    let effective_graph = resolved_graph.cloned().unwrap_or_else(|| {
        contract
            .members
            .iter()
            .map(|member| (member.id.clone(), member.dependencies.iter().cloned().collect()))
            .collect()
    });
    for member in &contract.members {
        if diagnostics.is_halted() {
            return;
        }
        let dependencies = effective_graph.get(&member.id).cloned().unwrap_or_default();
        for dependency in dependencies {
            let Some(target) = members.get(dependency.as_str()) else {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1101",
                    Some(Path::new(&member.root)),
                    format!("resolved graph contains unknown member '{dependency}'"),
                    "register the dependency or remove the edge",
                ));
                continue;
            };
            if !allowed_layer_edge(member.kind, target.kind) {
                diagnostics.push(architecture_error(
                    "ZRYNA-A1102",
                    Some(Path::new(&member.root)),
                    format!("forbidden dependency direction: '{}' -> '{dependency}'", member.id),
                    "depend only toward lower-level contracts or route orchestration through zryna-driver",
                ));
            }
        }
    }
    for member in &contract.members {
        if diagnostics.is_halted() {
            return;
        }
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        if has_graph_cycle(member.id.as_str(), &effective_graph, &mut visiting, &mut visited) {
            diagnostics.push(architecture_error(
                "ZRYNA-A1103",
                Some(Path::new(&member.root)),
                format!("dependency cycle reaches '{}'", member.id),
                "break the cycle by moving shared contracts into a lower foundation member",
            ));
        }
    }
}

#[derive(Clone, Copy)]
struct ScanLimits {
    entries: usize,
    depth: usize,
    file_bytes: u64,
    total_bytes: u64,
    diagnostics: usize,
}

const PRODUCTION_SCAN_LIMITS: ScanLimits = ScanLimits {
    entries: MAX_SCAN_ENTRIES,
    depth: MAX_SCAN_DEPTH,
    file_bytes: MAX_SCANNED_FILE_BYTES,
    total_bytes: MAX_SCAN_TOTAL_BYTES,
    diagnostics: MAX_SCAN_DIAGNOSTICS,
};

struct ScanState {
    limits: ScanLimits,
    entries_seen: usize,
    bytes_seen: u64,
    diagnostics_seen: usize,
    halted: bool,
}

struct ScanPolicy<'a> {
    exclusions: BTreeMap<PathBuf, bool>,
    contract_source: &'a str,
}

impl<'a> ScanPolicy<'a> {
    fn new(contract: &WorkspaceContract, contract_source: &'a str) -> Self {
        let mut exclusions =
            BTreeMap::from([(PathBuf::from(".git"), false), (PathBuf::from("node_modules"), true)]);
        for output in &contract.outputs {
            exclusions.insert(PathBuf::from(output), true);
        }
        for adapter in &contract.adapters {
            exclusions.insert(Path::new(&adapter.root).join("node_modules"), true);
        }
        Self { exclusions, contract_source }
    }

    fn excluded_shape(&self, relative: &Path) -> Option<bool> {
        self.exclusions.get(relative).copied()
    }
}

impl ScanState {
    const fn new(limits: ScanLimits) -> Self {
        Self { limits, entries_seen: 0, bytes_seen: 0, diagnostics_seen: 0, halted: false }
    }
}

fn validate_bounded_filesystem(
    root: &Path,
    contract: &WorkspaceContract,
    contract_source: &str,
    diagnostics: &mut ValidationDiagnostics,
) -> bool {
    let policy = ScanPolicy::new(contract, contract_source);
    let mut state = ScanState::new(PRODUCTION_SCAN_LIMITS);
    scan_path(root, root, &policy, 0, &mut state, diagnostics);
    !state.halted
}

fn scan_path(
    root: &Path,
    path: &Path,
    policy: &ScanPolicy<'_>,
    depth: usize,
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
) {
    if state.halted {
        return;
    }
    if depth > state.limits.depth {
        halt_scan(
            state,
            diagnostics,
            path.strip_prefix(root).ok(),
            "architecture scan exceeded its deterministic depth budget",
        );
        return;
    }
    if state.entries_seen >= state.limits.entries {
        halt_scan(
            state,
            diagnostics,
            path.strip_prefix(root).ok(),
            "architecture scan exceeded its deterministic entry budget",
        );
        return;
    }
    state.entries_seen += 1;

    let relative = path.strip_prefix(root).unwrap_or(path);
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) => {
            push_scan_diagnostic(
                state,
                diagnostics,
                architecture_error(
                    "ZRYNA-A1205",
                    Some(relative),
                    format!("filesystem entry could not be inspected: {error}"),
                    "restore a stable readable entry and retry",
                ),
            );
            return;
        }
    };
    if metadata_is_link_or_reparse(&metadata) {
        push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1201",
                Some(relative),
                "symlinks are forbidden inside the controlled workspace",
                "replace the link with a real in-workspace file or directory",
            ),
        );
        return;
    }

    if let Some(expected_directory) = policy.excluded_shape(relative) {
        validate_excluded_entry(relative, &metadata, expected_directory, state, diagnostics);
        return;
    }

    if metadata.is_file() {
        scan_regular_file(path, relative, &metadata, policy, state, diagnostics);
        return;
    }
    if !metadata.is_dir() {
        push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1201",
                Some(relative),
                "non-regular filesystem entries are forbidden",
                "remove sockets, devices, and FIFOs from controlled source roots",
            ),
        );
        return;
    }

    scan_directory(root, path, relative, policy, depth, state, diagnostics);
}

fn validate_excluded_entry(
    relative: &Path,
    metadata: &fs::Metadata,
    expected_directory: bool,
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
) {
    if expected_directory && !metadata.is_dir() {
        push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1201",
                Some(relative),
                "declared generated output must be a real directory",
                "replace it with the declared non-symlink output directory",
            ),
        );
    } else if !expected_directory && !metadata.is_dir() && !metadata.is_file() {
        push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1201",
                Some(relative),
                "Git metadata must be a real file or directory",
                "replace it with regular Git metadata",
            ),
        );
    }
}

fn scan_regular_file(
    path: &Path,
    relative: &Path,
    metadata: &fs::Metadata,
    policy: &ScanPolicy<'_>,
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
) {
    let Some(advertised_total) = state.bytes_seen.checked_add(metadata.len()) else {
        halt_scan(
            state,
            diagnostics,
            Some(relative),
            "architecture scan byte accounting overflowed",
        );
        return;
    };
    if advertised_total > state.limits.total_bytes {
        halt_scan(
            state,
            diagnostics,
            Some(relative),
            "architecture scan exceeded its deterministic aggregate byte budget",
        );
        return;
    }
    state.bytes_seen = advertised_total;
    match read_bounded_utf8_with_expected_size(
        path,
        Some(relative),
        state.limits.file_bytes,
        "ZRYNA-A1203",
        "restore the controlled UTF-8 source file",
        metadata.len(),
    ) {
        Ok((source, _)) => {
            if relative == Path::new(CONTRACT_FILE) && source != policy.contract_source {
                halt_scan_with_diagnostic(
                    state,
                    diagnostics,
                    architecture_error(
                        "ZRYNA-A1203",
                        Some(relative),
                        "workspace contract changed after it was parsed",
                        "stop concurrent mutation and retry architecture validation",
                    ),
                );
            }
        }
        Err(diagnostic) if diagnostic.code == "ZRYNA-A1204" => {
            halt_scan_with_diagnostic(state, diagnostics, diagnostic);
        }
        Err(diagnostic) => push_scan_diagnostic(state, diagnostics, diagnostic),
    }
}

fn scan_directory(
    root: &Path,
    path: &Path,
    relative: &Path,
    policy: &ScanPolicy<'_>,
    depth: usize,
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
) {
    let children = match fs::read_dir(path) {
        Ok(value) => value,
        Err(error) => {
            push_scan_diagnostic(
                state,
                diagnostics,
                architecture_error(
                    "ZRYNA-A1205",
                    Some(relative),
                    format!("directory scan failed: {error}"),
                    "restore read access; incomplete scans never pass",
                ),
            );
            return;
        }
    };
    let remaining_entries = state.limits.entries.saturating_sub(state.entries_seen);
    let mut child_entries = Vec::new();
    let mut read_failed = false;
    for child in children {
        match child {
            Ok(entry) => {
                if child_entries.len() >= remaining_entries {
                    halt_scan(
                        state,
                        diagnostics,
                        Some(relative),
                        "architecture scan exceeded its deterministic entry budget",
                    );
                    return;
                }
                child_entries.push(entry);
            }
            Err(_) => read_failed = true,
        }
    }
    if read_failed {
        push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1205",
                Some(relative),
                "one or more directory entries could not be read",
                "restore directory consistency and retry",
            ),
        );
        return;
    }
    child_entries.sort_by_key(fs::DirEntry::file_name);
    let mut child_paths = Vec::with_capacity(child_entries.len());
    let mut portable_identities = BTreeMap::new();
    for entry in child_entries {
        validate_portable_directory_entry(
            root,
            &entry,
            &mut portable_identities,
            state,
            diagnostics,
        );
        child_paths.push(entry.path());
    }
    child_paths.sort();
    for child_path in child_paths {
        if state.halted {
            break;
        }
        scan_path(root, &child_path, policy, depth + 1, state, diagnostics);
    }
}

fn validate_portable_directory_entry(
    root: &Path,
    entry: &fs::DirEntry,
    portable_identities: &mut BTreeMap<String, String>,
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
) {
    let name = entry.file_name();
    match name.to_str() {
        Some(value) if portable_path_segment(value) => {
            let identity = value.to_ascii_lowercase();
            if let Some(previous) = portable_identities.insert(identity, value.to_owned()) {
                push_scan_diagnostic(
                    state,
                    diagnostics,
                    architecture_error(
                        "ZRYNA-A1003",
                        entry.path().strip_prefix(root).ok(),
                        format!(
                            "filesystem entries '{previous}' and '{value}' collide under the portable path identity"
                        ),
                        "keep one printable ASCII spelling for every controlled entry",
                    ),
                );
            }
        }
        Some(value) => push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1003",
                entry.path().strip_prefix(root).ok(),
                format!("filesystem entry name '{value}' is not portable"),
                "use printable ASCII without reserved names, characters, or trailing dots and spaces",
            ),
        ),
        None => push_scan_diagnostic(
            state,
            diagnostics,
            architecture_error(
                "ZRYNA-A1203",
                entry.path().strip_prefix(root).ok(),
                "filesystem entry name is not valid UTF-8",
                "rename the entry with a portable printable ASCII name",
            ),
        ),
    }
}

fn push_scan_diagnostic(
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
    diagnostic: Diagnostic,
) {
    if state.halted {
        return;
    }
    if state.diagnostics_seen.saturating_add(1) >= state.limits.diagnostics {
        halt_scan(
            state,
            diagnostics,
            None,
            "architecture scan exceeded its deterministic diagnostic budget",
        );
        return;
    }
    diagnostics.push(diagnostic);
    state.diagnostics_seen += 1;
    if diagnostics.is_halted() {
        state.halted = true;
    }
}

fn halt_scan(
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
    path: Option<&Path>,
    message: &str,
) {
    if state.halted {
        return;
    }
    halt_scan_with_diagnostic(
        state,
        diagnostics,
        architecture_error(
            "ZRYNA-A1204",
            path,
            message,
            "reduce controlled input size or depth; incomplete scans never pass",
        ),
    );
}

fn halt_scan_with_diagnostic(
    state: &mut ScanState,
    diagnostics: &mut ValidationDiagnostics,
    diagnostic: Diagnostic,
) {
    if state.halted {
        return;
    }
    if state.diagnostics_seen < state.limits.diagnostics {
        diagnostics.push(diagnostic);
        state.diagnostics_seen += 1;
    }
    state.halted = true;
}

fn read_toml_with_source(
    path: &Path,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<(toml::Value, String)> {
    let (source, _) = match read_bounded_utf8(
        path,
        Some(path),
        MAX_MANIFEST_BYTES,
        "ZRYNA-A1005",
        "restore the canonical component Cargo.toml",
    ) {
        Ok(value) => value,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            return None;
        }
    };
    match toml::from_str(&source) {
        Ok(value) => Some((value, source)),
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1006",
                Some(path),
                format!("Cargo manifest is invalid: {error}"),
                "repair the manifest before architecture validation",
            ));
            None
        }
    }
}

fn read_json(path: &Path, diagnostics: &mut ValidationDiagnostics) -> Option<serde_json::Value> {
    let (source, _) = match read_bounded_utf8(
        path,
        Some(path),
        MAX_MANIFEST_BYTES,
        "ZRYNA-A1010",
        "restore the registered adapter package.json",
    ) {
        Ok(value) => value,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            return None;
        }
    };
    match serde_json::from_str(&source) {
        Ok(value) => Some(value),
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1010",
                Some(path),
                format!("adapter package manifest is invalid JSON: {error}"),
                "repair package.json before architecture validation",
            ));
            None
        }
    }
}

const fn allowed_layer_edge(source: MemberKind, target: MemberKind) -> bool {
    use MemberKind::{Application, Backend, Compiler, Foundation, Frontend, Orchestrator};
    match source {
        Foundation | Frontend => matches!(target, Foundation),
        Compiler => matches!(target, Foundation | Compiler),
        Backend => matches!(target, Foundation | Compiler),
        Orchestrator => !matches!(target, Application | Orchestrator),
        Application => matches!(target, Foundation | Orchestrator),
    }
}

fn has_graph_cycle<'a>(
    id: &'a str,
    graph: &'a InternalDependencyGraph,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> bool {
    if visiting.contains(id) {
        return true;
    }
    if visited.contains(id) {
        return false;
    }
    visiting.insert(id);
    if let Some(dependencies) = graph.get(id) {
        for dependency in dependencies {
            if has_graph_cycle(dependency, graph, visiting, visited) {
                return true;
            }
        }
    }
    visiting.remove(id);
    visited.insert(id);
    false
}

fn safe_relative_path(value: &str) -> bool {
    if value.is_empty()
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains("//")
        || value.contains('\\')
    {
        return false;
    }
    value.split('/').all(portable_path_segment)
}

fn valid_component_entry(value: &str) -> bool {
    !value.contains(['/', '\\']) && portable_path_segment(value)
}

fn portable_path_segment(value: &str) -> bool {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.len() > 255
        || value.ends_with(['.', ' '])
        || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
        || value.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*'])
    {
        return false;
    }
    let stem = value.split('.').next().unwrap_or(value).to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

fn valid_id(value: &str) -> bool {
    value.bytes().next().is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || (byte == b'-' && index > 0)
        })
        && !value.ends_with('-')
        && !value.contains("--")
}

fn architecture_error(
    code: &str,
    path: Option<&Path>,
    message: impl Into<String>,
    guidance: impl Into<String>,
) -> Diagnostic {
    Diagnostic::error(
        code,
        path.map(|value| value.to_string_lossy().replace('\\', "/")),
        message,
        guidance,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::error::Error;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(windows)]
    use std::process::Command;

    use super::{
        AdapterContract, BoundedProcessStream, CONTRACT_PROFILE, CONTRACT_VERSION,
        CargoMetadataDocument, CargoMetadataNode, CargoMetadataNodeDependency,
        CargoMetadataPackage, CargoMetadataResolve, ControlledReadPolicy, InternalDependencyGraph,
        MAX_CARGO_EDGES, MAX_CARGO_PACKAGES, MAX_CONTRACT_BYTES, MAX_MANIFEST_BYTES,
        MemberContract, MemberKind, ScanLimits, ScanPolicy, ScanState, ValidationDiagnostics,
        WorkspaceContract, allowed_layer_edge, load_cargo_metadata, load_contract,
        portable_path_segment, read_bounded_utf8_with_expected_size, read_bounded_utf8_with_hooks,
        read_optional_cargo_input_snapshots, read_process_stream, read_toml_with_source,
        safe_relative_path, scan_path, valid_id, validate_cargo_inputs_unchanged,
        validate_cargo_metadata_limits, validate_cargo_process_output,
        validate_component_containers, validate_contract_identity, validate_contract_paths,
        validate_contract_unchanged, validate_dependency_graph, validate_paths,
        validate_required_root_shapes, validate_resolved_cargo_graph, validate_workspace,
        validation_report,
    };
    #[cfg(unix)]
    use super::{load_cargo_metadata_with_executable, read_bounded_utf8};

    const FIXTURE_LIMITS: ScanLimits =
        ScanLimits { entries: 128, depth: 16, file_bytes: 128, total_bytes: 1024, diagnostics: 8 };

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct TempFixture {
        root: PathBuf,
    }

    impl TempFixture {
        fn new() -> std::io::Result<Self> {
            let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("zryna-architecture-{}-{sequence}", std::process::id()));
            fs::create_dir_all(&root)?;
            Ok(Self { root })
        }

        fn path(&self, relative: impl AsRef<Path>) -> PathBuf {
            self.root.join(relative)
        }

        fn directory(&self, relative: impl AsRef<Path>) -> std::io::Result<PathBuf> {
            let path = self.path(relative);
            fs::create_dir_all(&path)?;
            Ok(path)
        }

        fn write(&self, relative: impl AsRef<Path>, bytes: &[u8]) -> std::io::Result<PathBuf> {
            let path = self.path(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, bytes)?;
            Ok(path)
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn fixture_contract() -> WorkspaceContract {
        WorkspaceContract {
            schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
            version: CONTRACT_VERSION,
            profile: CONTRACT_PROFILE.to_owned(),
            members: Vec::new(),
            adapters: vec![AdapterContract {
                id: "typescript-6".to_owned(),
                root: "adapters/typescript-6".to_owned(),
                protocol_version: 1,
                toolchain: "@typescript/typescript6@6.0.2".to_owned(),
                allowed_entries: Vec::new(),
            }],
            outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
        }
    }

    fn write_minimal_workspace(fixture: &TempFixture) -> Result<(), Box<dyn Error>> {
        let contract = WorkspaceContract {
            schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
            version: CONTRACT_VERSION,
            profile: CONTRACT_PROFILE.to_owned(),
            members: vec![MemberContract {
                id: "sample".to_owned(),
                root: "crates/sample".to_owned(),
                kind: MemberKind::Foundation,
                dependencies: Vec::new(),
                allowed_entries: vec![
                    "Cargo.toml".to_owned(),
                    "README.md".to_owned(),
                    "src".to_owned(),
                ],
            }],
            adapters: Vec::new(),
            outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
        };
        fixture.write("zryna.workspace.json", &serde_json::to_vec_pretty(&contract)?)?;
        fixture.write(
            "Cargo.toml",
            b"[workspace]\nresolver = \"2\"\nmembers = [\"crates/sample\"]\n",
        )?;
        fixture.write(
            "crates/sample/Cargo.toml",
            b"[package]\nname = \"sample\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        fixture.write("crates/sample/README.md", b"# sample\n")?;
        fixture.write("crates/sample/src/lib.rs", b"//! Sample fixture.\n")?;
        Ok(())
    }

    fn cargo_graph_member(id: &str, dependencies: &[&str]) -> MemberContract {
        MemberContract {
            id: id.to_owned(),
            root: format!("crates/{id}"),
            kind: MemberKind::Foundation,
            dependencies: dependencies.iter().map(ToString::to_string).collect(),
            allowed_entries: vec![
                "Cargo.toml".to_owned(),
                "README.md".to_owned(),
                "src".to_owned(),
            ],
        }
    }

    fn write_cargo_graph_fixture(
        fixture: &TempFixture,
    ) -> Result<WorkspaceContract, Box<dyn Error>> {
        let dependency_ids =
            ["base-normal", "base-dev", "base-build", "base-windows", "base-optional"];
        let mut members: Vec<MemberContract> =
            dependency_ids.iter().map(|id| cargo_graph_member(id, &[])).collect();
        members.push(cargo_graph_member("consumer", &dependency_ids));
        let contract = WorkspaceContract {
            schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
            version: CONTRACT_VERSION,
            profile: CONTRACT_PROFILE.to_owned(),
            members,
            adapters: Vec::new(),
            outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
        };
        let workspace_members = contract
            .members
            .iter()
            .map(|member| format!("\"{}\"", member.root))
            .collect::<Vec<_>>()
            .join(", ");
        fixture.write(
            "Cargo.toml",
            format!("[workspace]\nresolver = \"2\"\nmembers = [{workspace_members}]\n").as_bytes(),
        )?;
        for id in dependency_ids {
            fixture.write(
                format!("crates/{id}/Cargo.toml"),
                format!("[package]\nname = \"{id}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n")
                    .as_bytes(),
            )?;
            fixture.write(format!("crates/{id}/src/lib.rs"), b"//! Graph fixture.\n")?;
        }
        fixture.write(
            "crates/consumer/Cargo.toml",
            br#"[package]
name = "consumer"
version = "0.1.0"
edition = "2024"

[dependencies]
normal_alias = { package = "base-normal", path = "../base-normal" }
optional_alias = { package = "base-optional", path = "../base-optional", optional = true }

[dev-dependencies]
dev_alias = { package = "base-dev", path = "../base-dev" }

[build-dependencies]
build_alias = { package = "base-build", path = "../base-build" }

[target.'cfg(windows)'.dependencies]
windows_alias = { package = "base-windows", path = "../base-windows" }
"#,
        )?;
        fixture.write("crates/consumer/src/lib.rs", b"//! Graph consumer.\n")?;
        Ok(contract)
    }

    fn assert_undeclared_internal_edge_rejected(
        fixture: &TempFixture,
        contract: &WorkspaceContract,
        metadata: &CargoMetadataDocument,
        missing_dependency: &str,
    ) -> Result<(), Box<dyn Error>> {
        let mut incomplete = contract.clone();
        incomplete
            .members
            .iter_mut()
            .find(|member| member.id == "consumer")
            .ok_or("missing consumer contract")?
            .dependencies
            .retain(|dependency| dependency != missing_dependency);
        let mut diagnostics = ValidationDiagnostics::default();
        validate_resolved_cargo_graph(&fixture.root, &incomplete, metadata, &mut diagnostics);
        assert!(
            has_code(&diagnostics.into_vec(), "ZRYNA-A1101"),
            "undeclared {missing_dependency} edge was accepted"
        );
        Ok(())
    }

    fn assert_complete_dependency_forms(consumer: &CargoMetadataPackage) {
        let aliases: BTreeSet<&str> = consumer
            .dependencies
            .iter()
            .filter_map(|dependency| dependency.rename.as_deref())
            .collect();
        assert!(
            ["normal_alias", "dev_alias", "build_alias", "windows_alias", "optional_alias"]
                .into_iter()
                .all(|alias| aliases.contains(alias))
        );
        assert!(
            consumer
                .dependencies
                .iter()
                .any(|dependency| dependency.kind.as_deref() == Some("dev"))
        );
        assert!(
            consumer
                .dependencies
                .iter()
                .any(|dependency| dependency.kind.as_deref() == Some("build"))
        );
        assert!(
            consumer
                .dependencies
                .iter()
                .any(|dependency| dependency.target.as_deref() == Some("cfg(windows)"))
        );
        assert!(consumer.dependencies.iter().any(|dependency| dependency.optional));
    }

    fn scan_fixture(root: &Path, limits: ScanLimits) -> (bool, Vec<zryna_diagnostics::Diagnostic>) {
        let contract = fixture_contract();
        let policy = ScanPolicy::new(&contract, "");
        let mut diagnostics = ValidationDiagnostics::default();
        let mut state = ScanState::new(limits);
        scan_path(root, root, &policy, 0, &mut state, &mut diagnostics);
        (!state.halted, diagnostics.into_vec())
    }

    fn has_code(diagnostics: &[zryna_diagnostics::Diagnostic], code: &str) -> bool {
        diagnostics.iter().any(|diagnostic| diagnostic.code == code)
    }

    #[cfg(unix)]
    fn successful_exit_status() -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;

        std::process::ExitStatus::from_raw(0)
    }

    #[cfg(windows)]
    fn successful_exit_status() -> std::process::ExitStatus {
        use std::os::windows::process::ExitStatusExt;

        std::process::ExitStatus::from_raw(0)
    }

    fn metadata_package(index: usize) -> CargoMetadataPackage {
        CargoMetadataPackage {
            id: format!("package-{index}"),
            name: format!("package-{index}"),
            manifest_path: format!("crates/package-{index}/Cargo.toml"),
            source: Some("registry+fixture".to_owned()),
            dependencies: Vec::new(),
        }
    }

    fn metadata_edge(index: usize) -> CargoMetadataNodeDependency {
        CargoMetadataNodeDependency {
            name: format!("dependency-{index}"),
            pkg: format!("package-{index}"),
            dep_kinds: Vec::new(),
        }
    }

    fn empty_metadata() -> CargoMetadataDocument {
        CargoMetadataDocument {
            version: 1,
            packages: Vec::new(),
            workspace_members: Vec::new(),
            workspace_root: ".".to_owned(),
            resolve: Some(CargoMetadataResolve { nodes: Vec::new() }),
        }
    }

    #[test]
    fn rejects_unsafe_paths() {
        assert!(safe_relative_path("crates/zryna-ir"));
        assert!(!safe_relative_path("../outside"));
        assert!(!safe_relative_path("C:\\outside"));
        assert!(!safe_relative_path("crates\\zryna-ir"));
        for value in [
            "C:/outside",
            "C:relative",
            "//server/share",
            "crates//sample",
            "crates/./sample",
            "crates/sample/",
            "crates/naïve",
        ] {
            assert!(!safe_relative_path(value), "accepted nonportable path {value:?}");
        }
        for value in [
            "CON",
            "con.txt",
            "PRN",
            "AUX.log",
            "NUL",
            "COM1",
            "LPT9.txt",
            "bad:name",
            "trailing.",
            "trailing ",
        ] {
            assert!(!portable_path_segment(value), "accepted reserved segment {value:?}");
        }
        assert!(portable_path_segment("com10"));
    }

    #[test]
    fn validates_canonical_ids() {
        assert!(valid_id("zryna-backend-native"));
        assert!(!valid_id("ZRYNA-native"));
        assert!(!valid_id("7-native"));
        assert!(!valid_id("zryna--native"));
    }

    #[test]
    fn binds_component_kinds_to_portable_roots() {
        let mut contract = fixture_contract();
        contract.adapters[0].root = "crates/typescript-6".to_owned();
        contract.members = vec![
            MemberContract {
                id: "core".to_owned(),
                root: "apps/core".to_owned(),
                kind: MemberKind::Foundation,
                dependencies: Vec::new(),
                allowed_entries: Vec::new(),
            },
            MemberContract {
                id: "cli".to_owned(),
                root: "crates/cli".to_owned(),
                kind: MemberKind::Application,
                dependencies: Vec::new(),
                allowed_entries: Vec::new(),
            },
        ];
        let mut diagnostics = ValidationDiagnostics::default();

        validate_contract_paths(&contract, &mut diagnostics);

        let diagnostics = diagnostics.into_vec();
        assert_eq!(
            diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1003").count(),
            3
        );
    }

    #[test]
    fn proves_required_root_shapes_and_component_inventory() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        fixture.write("Cargo.toml", b"")?;
        fixture.directory("Cargo.lock")?;
        fixture.write("zryna.workspace.json", b"{}")?;
        fixture.directory("apps/zryna")?;
        fixture.directory("crates/core")?;
        fixture.directory("crates/rogue")?;
        fixture.directory("adapters/typescript-6")?;
        let contract = WorkspaceContract {
            schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
            version: CONTRACT_VERSION,
            profile: CONTRACT_PROFILE.to_owned(),
            members: vec![
                MemberContract {
                    id: "zryna".to_owned(),
                    root: "apps/zryna".to_owned(),
                    kind: MemberKind::Application,
                    dependencies: Vec::new(),
                    allowed_entries: Vec::new(),
                },
                cargo_graph_member("core", &[]),
            ],
            adapters: vec![AdapterContract {
                id: "typescript-6".to_owned(),
                root: "adapters/typescript-6".to_owned(),
                protocol_version: 1,
                toolchain: "@typescript/typescript6@6.0.2".to_owned(),
                allowed_entries: Vec::new(),
            }],
            outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
        };
        let mut diagnostics = ValidationDiagnostics::default();

        validate_required_root_shapes(&fixture.root, &mut diagnostics);
        validate_component_containers(&fixture.root, &contract, &mut diagnostics);

        let diagnostics = diagnostics.into_vec();
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "ZRYNA-A1005" && diagnostic.path() == Some("Cargo.lock")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "ZRYNA-A1005" && diagnostic.path() == Some("crates/rogue")
        }));
        Ok(())
    }

    #[test]
    fn detects_noncanonical_component_directory_spelling() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        fixture.directory("crates/Sample")?;
        let contract = WorkspaceContract {
            schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
            version: CONTRACT_VERSION,
            profile: CONTRACT_PROFILE.to_owned(),
            members: vec![cargo_graph_member("sample", &[])],
            adapters: Vec::new(),
            outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
        };
        let canonical_root = fs::canonicalize(&fixture.root)?;
        let mut diagnostics = ValidationDiagnostics::default();

        validate_paths(&canonical_root, &contract, &mut diagnostics);

        assert!(diagnostics.into_vec().iter().any(|diagnostic| {
            diagnostic.code == "ZRYNA-A1003" && diagnostic.path() == Some("crates/sample")
        }));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_case_colliding_filesystem_siblings() -> Result<(), Box<dyn Error>> {
        let first = TempFixture::new()?;
        first.write("Foo.txt", b"first")?;
        first.write("foo.txt", b"second")?;
        let second = TempFixture::new()?;
        second.write("foo.txt", b"second")?;
        second.write("Foo.txt", b"first")?;

        let (_, first_diagnostics) = scan_fixture(&first.root, FIXTURE_LIMITS);
        let (_, second_diagnostics) = scan_fixture(&second.root, FIXTURE_LIMITS);
        let collision = |diagnostics: Vec<zryna_diagnostics::Diagnostic>| {
            diagnostics
                .into_iter()
                .find(|diagnostic| {
                    diagnostic.code == "ZRYNA-A1003"
                        && diagnostic.message.contains("collide under the portable path identity")
                })
                .expect("missing portable collision diagnostic")
        };
        assert_eq!(collision(first_diagnostics), collision(second_diagnostics));
        Ok(())
    }

    #[test]
    fn bounded_process_reader_drains_without_growing_past_limit() -> Result<(), Box<dyn Error>> {
        let input = vec![b'x'; 32 * 1024];

        let output = read_process_stream(std::io::Cursor::new(input), 1024)?;

        assert_eq!(output.bytes.len(), 1024);
        assert!(output.exceeded);
        Ok(())
    }

    #[test]
    fn cargo_metadata_output_limits_and_format_fail_closed() {
        let clean = BoundedProcessStream { bytes: Vec::new(), exceeded: false };
        let oversized = BoundedProcessStream { bytes: Vec::new(), exceeded: true };
        let mut oversized_diagnostics = ValidationDiagnostics::default();
        let oversized_result = validate_cargo_process_output(
            successful_exit_status(),
            &oversized,
            &clean,
            &mut oversized_diagnostics,
        );
        assert!(oversized_result.is_none());
        assert!(has_code(&oversized_diagnostics.into_vec(), "ZRYNA-A1204"));

        let unsupported = BoundedProcessStream {
            bytes: br#"{"version":2,"packages":[],"workspace_members":[],"workspace_root":".","resolve":{"nodes":[]}}"#
                .to_vec(),
            exceeded: false,
        };
        let mut format_diagnostics = ValidationDiagnostics::default();
        let format_result = validate_cargo_process_output(
            successful_exit_status(),
            &unsupported,
            &clean,
            &mut format_diagnostics,
        );
        assert!(format_result.is_none());
        assert!(has_code(&format_diagnostics.into_vec(), "ZRYNA-A1101"));
    }

    #[test]
    fn cargo_metadata_package_and_edge_budgets_fail_closed() {
        let mut package_heavy = empty_metadata();
        package_heavy.packages = (0..=MAX_CARGO_PACKAGES).map(metadata_package).collect();
        let mut package_diagnostics = ValidationDiagnostics::default();
        assert!(!validate_cargo_metadata_limits(&package_heavy, &mut package_diagnostics));
        assert!(has_code(&package_diagnostics.into_vec(), "ZRYNA-A1204"));

        let mut edge_heavy = empty_metadata();
        edge_heavy.resolve = Some(CargoMetadataResolve {
            nodes: vec![CargoMetadataNode {
                id: "source".to_owned(),
                deps: (0..=MAX_CARGO_EDGES).map(metadata_edge).collect(),
            }],
        });
        let mut edge_diagnostics = ValidationDiagnostics::default();
        assert!(!validate_cargo_metadata_limits(&edge_heavy, &mut edge_diagnostics));
        assert!(has_code(&edge_diagnostics.into_vec(), "ZRYNA-A1204"));
    }

    #[test]
    fn detects_optional_cargo_input_creation_and_noncanonical_spelling()
    -> Result<(), Box<dyn Error>> {
        let created_fixture = TempFixture::new()?;
        let mut snapshots = Vec::new();
        let mut snapshot_diagnostics = ValidationDiagnostics::default();
        read_optional_cargo_input_snapshots(
            &created_fixture.root,
            &mut snapshots,
            &mut snapshot_diagnostics,
        );
        assert!(snapshot_diagnostics.is_empty());
        assert_eq!(snapshots.len(), 2);
        created_fixture.write(".cargo/config.toml", b"[net]\noffline = true\n")?;
        let mut changed_diagnostics = ValidationDiagnostics::default();
        validate_cargo_inputs_unchanged(
            &created_fixture.root,
            &snapshots,
            &mut changed_diagnostics,
        );
        assert!(has_code(&changed_diagnostics.into_vec(), "ZRYNA-A1203"));

        let spelling_fixture = TempFixture::new()?;
        spelling_fixture.write(".cargo/Config.toml", b"[net]\noffline = true\n")?;
        let mut spelling_diagnostics = ValidationDiagnostics::default();
        read_optional_cargo_input_snapshots(
            &spelling_fixture.root,
            &mut Vec::new(),
            &mut spelling_diagnostics,
        );
        assert!(has_code(&spelling_diagnostics.into_vec(), "ZRYNA-A1003"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn cargo_metadata_deadline_survives_a_descendant_holding_output_pipes()
    -> Result<(), Box<dyn Error>> {
        use std::os::unix::fs::PermissionsExt;

        let fixture = TempFixture::new()?;
        let fake_cargo = fixture.write("fake-cargo", b"#!/bin/sh\n(sleep 5) &\nexit 0\n")?;
        fs::set_permissions(&fake_cargo, fs::Permissions::from_mode(0o700))?;
        let started = std::time::Instant::now();
        let mut diagnostics = ValidationDiagnostics::default();

        let metadata = load_cargo_metadata_with_executable(
            &fixture.root,
            false,
            fake_cargo.into_os_string(),
            std::time::Duration::from_millis(100),
            &mut diagnostics,
        );

        assert!(metadata.is_none());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert!(has_code(&diagnostics.into_vec(), "ZRYNA-A1204"));
        Ok(())
    }

    #[test]
    fn cargo_metadata_proves_alias_kinds_targets_and_optional_edges() -> Result<(), Box<dyn Error>>
    {
        let fixture = TempFixture::new()?;
        let contract = write_cargo_graph_fixture(&fixture)?;
        let mut acquisition_diagnostics = ValidationDiagnostics::default();
        let metadata = load_cargo_metadata(&fixture.root, false, &mut acquisition_diagnostics)
            .ok_or("Cargo metadata fixture failed")?;
        assert!(acquisition_diagnostics.is_empty());

        let consumer = metadata
            .packages
            .iter()
            .find(|package| package.name == "consumer")
            .ok_or("missing consumer metadata")?;
        assert_complete_dependency_forms(consumer);

        let mut diagnostics = ValidationDiagnostics::default();
        let graph =
            validate_resolved_cargo_graph(&fixture.root, &contract, &metadata, &mut diagnostics);
        assert!(diagnostics.is_empty(), "unexpected diagnostics: {:#?}", diagnostics.values);
        let expected: BTreeSet<String> = contract
            .members
            .iter()
            .find(|member| member.id == "consumer")
            .ok_or("missing consumer contract")?
            .dependencies
            .iter()
            .cloned()
            .collect();
        assert_eq!(graph.get("consumer"), Some(&expected));

        for missing_dependency in
            ["base-normal", "base-dev", "base-build", "base-windows", "base-optional"]
        {
            assert_undeclared_internal_edge_rejected(
                &fixture,
                &contract,
                &metadata,
                missing_dependency,
            )?;
        }

        let mut incomplete = contract.clone();
        incomplete
            .members
            .iter_mut()
            .find(|member| member.id == "consumer")
            .ok_or("missing consumer contract")?
            .dependencies
            .retain(|dependency| dependency != "base-build");
        let mut incomplete_diagnostics = ValidationDiagnostics::default();
        validate_resolved_cargo_graph(
            &fixture.root,
            &incomplete,
            &metadata,
            &mut incomplete_diagnostics,
        );
        let first_report = validation_report(incomplete_diagnostics);
        assert!(has_code(&first_report.diagnostics, "ZRYNA-A1101"));

        let mut shuffled = metadata.clone();
        shuffled.packages.reverse();
        shuffled.workspace_members.reverse();
        if let Some(resolve) = &mut shuffled.resolve {
            resolve.nodes.reverse();
            for node in &mut resolve.nodes {
                node.deps.reverse();
                for dependency in &mut node.deps {
                    dependency.dep_kinds.reverse();
                }
            }
        }
        let mut shuffled_diagnostics = ValidationDiagnostics::default();
        validate_resolved_cargo_graph(
            &fixture.root,
            &incomplete,
            &shuffled,
            &mut shuffled_diagnostics,
        );
        assert_eq!(first_report.diagnostics, validation_report(shuffled_diagnostics).diagnostics);
        Ok(())
    }

    #[test]
    fn rejects_unregistered_resolved_local_packages() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        let mut contract = write_cargo_graph_fixture(&fixture)?;
        let mut acquisition_diagnostics = ValidationDiagnostics::default();
        let metadata = load_cargo_metadata(&fixture.root, false, &mut acquisition_diagnostics)
            .ok_or("Cargo metadata fixture failed")?;
        contract.members.retain(|member| member.id != "base-windows");
        let mut diagnostics = ValidationDiagnostics::default();

        validate_resolved_cargo_graph(&fixture.root, &contract, &metadata, &mut diagnostics);

        let diagnostics = diagnostics.into_vec();
        assert!(has_code(&diagnostics, "ZRYNA-A1005"));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "ZRYNA-A1101"
                && diagnostic.message.contains("unregistered local package 'base-windows'")
        }));
        Ok(())
    }

    #[test]
    fn actual_graph_drives_layer_and_cycle_diagnostics() {
        let foundation = cargo_graph_member("foundation", &[]);
        let backend = MemberContract {
            id: "backend".to_owned(),
            root: "crates/backend".to_owned(),
            kind: MemberKind::Backend,
            dependencies: Vec::new(),
            allowed_entries: Vec::new(),
        };
        let contract = WorkspaceContract {
            schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
            version: CONTRACT_VERSION,
            profile: CONTRACT_PROFILE.to_owned(),
            members: vec![foundation.clone(), backend],
            adapters: Vec::new(),
            outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
        };
        let forbidden = BTreeMap::from([
            ("foundation".to_owned(), BTreeSet::from(["backend".to_owned()])),
            ("backend".to_owned(), BTreeSet::new()),
        ]);
        let mut forbidden_diagnostics = ValidationDiagnostics::default();
        validate_dependency_graph(&contract, Some(&forbidden), &mut forbidden_diagnostics);
        assert!(has_code(&forbidden_diagnostics.into_vec(), "ZRYNA-A1102"));

        let mut peer = foundation;
        peer.id = "peer".to_owned();
        peer.root = "crates/peer".to_owned();
        let cycle_contract = WorkspaceContract {
            members: vec![cargo_graph_member("foundation", &[]), peer],
            ..contract
        };
        let cycle: InternalDependencyGraph = BTreeMap::from([
            ("foundation".to_owned(), BTreeSet::from(["peer".to_owned()])),
            ("peer".to_owned(), BTreeSet::from(["foundation".to_owned()])),
        ]);
        let mut cycle_diagnostics = ValidationDiagnostics::default();
        validate_dependency_graph(&cycle_contract, Some(&cycle), &mut cycle_diagnostics);
        assert!(has_code(&cycle_diagnostics.into_vec(), "ZRYNA-A1103"));
    }

    #[test]
    fn permanent_phase_graph_forbids_compiler_and_backend_provider_edges() {
        let syntax = cargo_graph_member("syntax", &[]);
        let frontend = MemberContract {
            id: "frontend".to_owned(),
            root: "crates/frontend".to_owned(),
            kind: MemberKind::Frontend,
            dependencies: vec!["syntax".to_owned()],
            allowed_entries: Vec::new(),
        };
        let semantics = MemberContract {
            id: "semantics".to_owned(),
            root: "crates/semantics".to_owned(),
            kind: MemberKind::Compiler,
            dependencies: vec!["syntax".to_owned()],
            allowed_entries: Vec::new(),
        };
        let backend = MemberContract {
            id: "backend".to_owned(),
            root: "crates/backend".to_owned(),
            kind: MemberKind::Backend,
            dependencies: Vec::new(),
            allowed_entries: Vec::new(),
        };
        let contract = WorkspaceContract {
            schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
            version: CONTRACT_VERSION,
            profile: CONTRACT_PROFILE.to_owned(),
            members: vec![syntax, frontend, semantics, backend],
            adapters: Vec::new(),
            outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
        };
        let graph = BTreeMap::from([
            ("syntax".to_owned(), BTreeSet::new()),
            ("frontend".to_owned(), BTreeSet::from(["syntax".to_owned()])),
            ("semantics".to_owned(), BTreeSet::from(["frontend".to_owned(), "syntax".to_owned()])),
            ("backend".to_owned(), BTreeSet::from(["frontend".to_owned()])),
        ]);
        let mut diagnostics = ValidationDiagnostics::default();

        validate_dependency_graph(&contract, Some(&graph), &mut diagnostics);

        let diagnostics = diagnostics.into_vec();
        assert_eq!(
            diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1102").count(),
            2
        );
        assert!(allowed_layer_edge(MemberKind::Frontend, MemberKind::Foundation));
        assert!(allowed_layer_edge(MemberKind::Compiler, MemberKind::Foundation));
        assert!(!allowed_layer_edge(MemberKind::Compiler, MemberKind::Frontend));
        assert!(!allowed_layer_edge(MemberKind::Backend, MemberKind::Frontend));
    }

    #[test]
    fn validation_wide_diagnostic_budget_is_deterministic() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        write_minimal_workspace(&fixture)?;
        for index in 0..300 {
            fixture.write(format!("extra-{index:03}.txt"), b"")?;
        }

        let first = validate_workspace(&fixture.root);
        let second = validate_workspace(&fixture.root);
        assert_eq!(first.diagnostics, second.diagnostics);
        assert_eq!(first.diagnostics.len(), 256);
        assert_eq!(
            first.diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1004").count(),
            255
        );
        assert_eq!(
            first.diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1204").count(),
            1
        );
        assert!(
            first
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("diagnostic budget"))
        );
        Ok(())
    }

    #[test]
    fn component_registry_is_bounded_before_scanning() {
        let mut contract = fixture_contract();
        contract.adapters.clear();
        contract.members = (0..257)
            .map(|index| MemberContract {
                id: format!("component-{index}"),
                root: format!("crates/component-{index}"),
                kind: MemberKind::Foundation,
                dependencies: Vec::new(),
                allowed_entries: Vec::new(),
            })
            .collect();
        let mut diagnostics = ValidationDiagnostics::default();

        validate_contract_identity(&contract, &mut diagnostics);

        assert!(diagnostics.into_vec().iter().any(|diagnostic| {
            diagnostic.code == "ZRYNA-A1001" && diagnostic.message.contains("257 components")
        }));
    }

    #[test]
    fn expected_size_and_contract_source_bind_the_scan_snapshot() -> Result<(), Box<dyn Error>> {
        let size_fixture = TempFixture::new()?;
        let controlled = size_fixture.write("controlled.zry", b"first")?;
        let expected_size = fs::metadata(&controlled)?.len();
        fs::write(&controlled, b"larger")?;
        let size_result = read_bounded_utf8_with_expected_size(
            &controlled,
            Some(Path::new("controlled.zry")),
            32,
            "ZRYNA-A1203",
            "restore the file",
            expected_size,
        );
        assert!(matches!(size_result, Err(diagnostic) if diagnostic.code == "ZRYNA-A1203"));

        let contract_fixture = TempFixture::new()?;
        contract_fixture.write("zryna.workspace.json", b"contract-b")?;
        let contract = fixture_contract();
        let policy = ScanPolicy::new(&contract, "contract-a");
        let mut scan_diagnostics = ValidationDiagnostics::default();
        let mut state = ScanState::new(FIXTURE_LIMITS);
        scan_path(
            &contract_fixture.root,
            &contract_fixture.root,
            &policy,
            0,
            &mut state,
            &mut scan_diagnostics,
        );
        assert!(state.halted);
        assert!(has_code(&scan_diagnostics.into_vec(), "ZRYNA-A1203"));

        let mut final_diagnostics = ValidationDiagnostics::default();
        validate_contract_unchanged(&contract_fixture.root, "contract-a", &mut final_diagnostics);
        assert!(has_code(&final_diagnostics.into_vec(), "ZRYNA-A1203"));
        Ok(())
    }

    #[test]
    fn excludes_only_declared_generated_directories() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        fixture.write("target/not-utf8.bin", &[0xff])?;
        fixture.write(".zryna/cache/not-utf8.bin", &[0xff])?;
        fixture.write(".zryna/out/not-utf8.bin", &[0xff])?;
        fixture.write("node_modules/not-utf8.bin", &[0xff])?;
        fixture.write("adapters/typescript-6/node_modules/not-utf8.bin", &[0xff])?;

        let (completed, diagnostics) = scan_fixture(&fixture.root, FIXTURE_LIMITS);
        assert!(completed);
        assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:#?}");
        Ok(())
    }

    #[test]
    fn inspects_nested_generated_names_and_unknown_output_siblings() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        fixture.write("crates/example/src/target/not-utf8.bin", &[0xff])?;
        fixture.write(".zryna/other/not-utf8.bin", &[0xff])?;

        let (_, diagnostics) = scan_fixture(&fixture.root, FIXTURE_LIMITS);
        assert_eq!(
            diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1203").count(),
            2
        );
        Ok(())
    }

    #[test]
    fn rejects_invalid_utf8_and_oversized_regular_files() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        fixture.write("invalid.zry", &[0xff])?;
        fixture.write("large.zry", b"12345")?;
        fixture.write("z-after.zry", &[0xff])?;
        let limits = ScanLimits { file_bytes: 4, ..FIXTURE_LIMITS };

        let (completed, diagnostics) = scan_fixture(&fixture.root, limits);
        assert!(!completed);
        assert_eq!(
            diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1203").count(),
            1
        );
        assert!(has_code(&diagnostics, "ZRYNA-A1204"));
        Ok(())
    }

    #[test]
    fn aggregate_entry_and_depth_budgets_halt_globally() -> Result<(), Box<dyn Error>> {
        let aggregate = TempFixture::new()?;
        aggregate.write("a.zry", b"123")?;
        aggregate.write("b.zry", b"456")?;
        let (_, aggregate_diagnostics) =
            scan_fixture(&aggregate.root, ScanLimits { total_bytes: 5, ..FIXTURE_LIMITS });
        assert_eq!(aggregate_diagnostics.len(), 1);
        assert_eq!(aggregate_diagnostics[0].code, "ZRYNA-A1204");

        let invalid_aggregate = TempFixture::new()?;
        invalid_aggregate.write("a.zry", &[0xff, 0xff, 0xff])?;
        invalid_aggregate.write("b.zry", &[0xff, 0xff, 0xff])?;
        let (invalid_completed, invalid_diagnostics) =
            scan_fixture(&invalid_aggregate.root, ScanLimits { total_bytes: 5, ..FIXTURE_LIMITS });
        assert!(!invalid_completed);
        assert!(has_code(&invalid_diagnostics, "ZRYNA-A1203"));
        assert!(has_code(&invalid_diagnostics, "ZRYNA-A1204"));

        let entries = TempFixture::new()?;
        entries.write("a.zry", b"a")?;
        entries.write("b.zry", b"b")?;
        let (entry_completed, entry_diagnostics) =
            scan_fixture(&entries.root, ScanLimits { entries: 2, ..FIXTURE_LIMITS });
        assert!(!entry_completed);
        assert_eq!(entry_diagnostics.len(), 1);
        assert_eq!(entry_diagnostics[0].code, "ZRYNA-A1204");

        let depth = TempFixture::new()?;
        depth.write("a/b/value.zry", b"value")?;
        let (depth_completed, depth_diagnostics) =
            scan_fixture(&depth.root, ScanLimits { depth: 1, ..FIXTURE_LIMITS });
        assert!(!depth_completed);
        assert_eq!(depth_diagnostics.len(), 1);
        assert_eq!(depth_diagnostics[0].code, "ZRYNA-A1204");
        Ok(())
    }

    #[test]
    fn diagnostic_budget_reserves_one_terminal_diagnostic() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        fixture.write("a.zry", &[0xff])?;
        fixture.write("b.zry", &[0xff])?;
        let (completed, diagnostics) =
            scan_fixture(&fixture.root, ScanLimits { diagnostics: 1, ..FIXTURE_LIMITS });

        assert!(!completed);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "ZRYNA-A1204");
        assert!(diagnostics[0].message.contains("diagnostic budget"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn bounded_reader_detects_same_size_replacement() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        let controlled = fixture.write("controlled.zry", b"first")?;
        let replacement = fixture.write("replacement.zry", b"other")?;
        let result = read_bounded_utf8_with_hooks(
            &controlled,
            ControlledReadPolicy {
                diagnostic_path: Some(Path::new("controlled.zry")),
                max_bytes: 32,
                unavailable_code: "ZRYNA-A1203",
                unavailable_guidance: "restore the file",
                expected_size: None,
            },
            || {
                assert!(fs::remove_file(&controlled).is_ok());
                assert!(fs::rename(&replacement, &controlled).is_ok());
            },
            || {},
        );

        assert!(matches!(result, Err(diagnostic) if diagnostic.code == "ZRYNA-A1203"));
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn bounded_reader_denies_same_size_replacement_on_windows() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        let controlled = fixture.write("controlled.zry", b"first")?;
        let result = read_bounded_utf8_with_hooks(
            &controlled,
            ControlledReadPolicy {
                diagnostic_path: Some(Path::new("controlled.zry")),
                max_bytes: 32,
                unavailable_code: "ZRYNA-A1203",
                unavailable_guidance: "restore the file",
                expected_size: None,
            },
            || {
                assert!(fs::remove_file(&controlled).is_err());
            },
            || {},
        )
        .unwrap_or_else(|diagnostic| panic!("unexpected diagnostic: {diagnostic:?}"));

        assert_eq!(result, ("first".to_string(), 5));
        Ok(())
    }

    #[test]
    fn different_files_never_share_an_identity() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        let left = fixture.write("left.zry", b"same")?;
        let right = fixture.write("right.zry", b"same")?;
        let left_handle = same_file::Handle::from_file(fs::File::open(left)?)?;
        let right_handle = same_file::Handle::from_file(fs::File::open(right)?)?;

        assert_ne!(left_handle, right_handle);
        Ok(())
    }

    #[test]
    fn oversized_contract_and_manifest_fail_before_parsing() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        let contract_size = usize::try_from(MAX_CONTRACT_BYTES + 1)?;
        fixture.write("zryna.workspace.json", &vec![b' '; contract_size])?;
        let contract_result = load_contract(&fixture.root);
        assert!(matches!(contract_result, Err(diagnostic) if diagnostic.code == "ZRYNA-A1204"));

        let manifest_size = usize::try_from(MAX_MANIFEST_BYTES + 1)?;
        let manifest = fixture.write("Cargo.toml", &vec![b' '; manifest_size])?;
        let mut diagnostics = ValidationDiagnostics::default();
        assert!(read_toml_with_source(&manifest, &mut diagnostics).is_none());
        let diagnostics = diagnostics.into_vec();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "ZRYNA-A1204");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_and_socket_at_excluded_paths() -> Result<(), Box<dyn Error>> {
        use std::os::unix::fs::symlink;
        use std::os::unix::net::UnixListener;

        let symlink_fixture = TempFixture::new()?;
        let destination = symlink_fixture.directory("real-target")?;
        symlink(&destination, symlink_fixture.path("target"))?;
        let (_, symlink_diagnostics) = scan_fixture(&symlink_fixture.root, FIXTURE_LIMITS);
        assert!(has_code(&symlink_diagnostics, "ZRYNA-A1201"));

        let socket_fixture = TempFixture::new()?;
        let _listener = UnixListener::bind(socket_fixture.path("target"))?;
        let (_, socket_diagnostics) = scan_fixture(&socket_fixture.root, FIXTURE_LIMITS);
        assert!(has_code(&socket_diagnostics, "ZRYNA-A1201"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_unreadable_and_mid_read_modified_files() -> Result<(), Box<dyn Error>> {
        use std::os::unix::fs::PermissionsExt;

        let fixture = TempFixture::new()?;
        let unreadable = fixture.write("unreadable.zry", b"value")?;
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))?;
        let unreadable_result = read_bounded_utf8(
            &unreadable,
            Some(Path::new("unreadable.zry")),
            32,
            "ZRYNA-A1203",
            "restore the file",
        );
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600))?;
        assert!(matches!(unreadable_result, Err(diagnostic) if diagnostic.code == "ZRYNA-A1203"));

        let modified = fixture.write("modified.zry", b"value")?;
        let modified_result = read_bounded_utf8_with_hooks(
            &modified,
            ControlledReadPolicy {
                diagnostic_path: Some(Path::new("modified.zry")),
                max_bytes: 32,
                unavailable_code: "ZRYNA-A1203",
                unavailable_guidance: "restore the file",
                expected_size: None,
            },
            || {},
            || assert!(fs::write(&modified, b"changed").is_ok()),
        );
        assert!(matches!(modified_result, Err(diagnostic) if diagnostic.code == "ZRYNA-A1203"));
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_directory_reparse_points() -> Result<(), Box<dyn Error>> {
        let fixture = TempFixture::new()?;
        let destination = fixture.directory("real-target")?;
        let junction = fixture.path("target");
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&destination)
            .status()?;
        assert!(status.success(), "failed to create the junction fixture");
        let (_, diagnostics) = scan_fixture(&fixture.root, FIXTURE_LIMITS);
        assert!(has_code(&diagnostics, "ZRYNA-A1201"));
        Ok(())
    }

    #[test]
    fn current_repository_satisfies_the_contract() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = validate_workspace(&root);
        assert!(report.is_valid(), "architecture diagnostics: {:#?}", report.diagnostics);
    }
}
