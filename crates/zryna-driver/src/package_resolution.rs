//! Retained filesystem capabilities for source-only package resolution.

mod authenticated;
mod filesystem;
mod project_capture;
pub(crate) use project_capture::CapturedProject;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use cap_fs_ext::DirExt as _;
use cap_std::fs::Dir;
use sha2::{Digest as _, Sha256};
use zryna_package::{
    LockMode, PackageFile, PackageMaterial, PackageSource, PackageSourceKind,
    PackageSourceProvider, ResolveError, ResolvedGraph,
};

use authenticated::authenticated_sources;
use filesystem::{CapturedRoot, RetainedFile, ensure_same_directory, publish_lock, read_retained};

pub use authenticated::{AuthenticatedPackageFile, AuthenticatedPackageSources};

const MANIFEST_NAME: &str = "zryna.package.json";
const LOCK_NAME: &str = "zryna.lock.json";
const MAX_MANIFEST_BYTES: usize = 65_536;
const MAX_SOURCE_BYTES: usize = 1_024;
const MAX_SOURCE_FILES: usize = 16;
const MAX_SOURCE_ENTRIES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Lock behavior for a driver-owned package resolution request.
pub enum PackageLockMode {
    /// Verify an existing exact lock without mutation.
    Frozen,
    /// Atomically publish the newly authenticated lock.
    Update,
}

#[derive(Clone, Debug)]
/// Driver request for one source-only package graph.
pub struct PackageResolutionRequest {
    /// Absolute declared reproduction/source root.
    pub source_root: PathBuf,
    /// Portable root-package locator relative to `source_root`.
    pub package: String,
    /// Optional absolute prepopulated exact-commit Git material cache.
    pub git_cache: Option<PathBuf>,
    /// Frozen verification or update publication behavior.
    pub mode: PackageLockMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Complete authenticated graph and lock publication observation.
pub struct PackageResolutionSuccess {
    graph: ResolvedGraph,
    sources: Vec<AuthenticatedPackageSources>,
    lock_path: PathBuf,
    published: bool,
}

impl PackageResolutionSuccess {
    #[must_use]
    /// Returns the authenticated graph.
    pub fn graph(&self) -> &ResolvedGraph {
        &self.graph
    }

    #[must_use]
    /// Returns immutable source inventories in canonical package identity order.
    pub fn sources(&self) -> &[AuthenticatedPackageSources] {
        &self.sources
    }

    #[must_use]
    /// Returns the root package lock path.
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    #[must_use]
    /// Reports whether update mode atomically published the lock.
    pub fn published(&self) -> bool {
        self.published
    }
}

/// Resolves one package graph through retained local and optional Git-cache capabilities.
///
/// # Errors
///
/// Returns a stable package rejection without publishing a partial or unauthenticated lock.
pub fn resolve_package(
    request: &PackageResolutionRequest,
) -> Result<PackageResolutionSuccess, ResolveError> {
    resolve_package_internal(request, None).map(|(success, _)| success)
}

pub(crate) fn resolve_project_package(
    request: &PackageResolutionRequest,
) -> Result<(PackageResolutionSuccess, Vec<PackageFile>), ResolveError> {
    capture_project_package(request)
        .map(|(success, captured)| (success, captured.into_root_files()))
}

pub(crate) fn capture_project_package(
    request: &PackageResolutionRequest,
) -> Result<(PackageResolutionSuccess, CapturedProject), ResolveError> {
    resolve_package_internal(request, Some(&request.package))
}

pub(crate) fn capture_package(
    request: &PackageResolutionRequest,
) -> Result<(PackageResolutionSuccess, CapturedProject), ResolveError> {
    resolve_package_internal(request, None)
}

fn resolve_package_internal(
    request: &PackageResolutionRequest,
    local_scope: Option<&str>,
) -> Result<(PackageResolutionSuccess, CapturedProject), ResolveError> {
    let source_root = CapturedRoot::capture(&request.source_root)?;
    let git_cache = request.git_cache.as_deref().map(CapturedRoot::capture).transpose()?;
    let root_source = PackageSource {
        kind: PackageSourceKind::Local,
        locator: request.package.clone(),
        revision: String::new(),
    };
    zryna_package::validate_source(&root_source)?;
    let mut provider = FilesystemProvider {
        source_root,
        git_cache,
        root_source: root_source.clone(),
        local_scope: local_scope.map(ToOwned::to_owned),
        loaded_files: BTreeMap::new(),
        retained_files: Vec::new(),
        retained_packages: Vec::new(),
    };
    let package_dir = provider.open_source_dir(&root_source)?;
    let existing_lock = match request.mode {
        PackageLockMode::Frozen => {
            let (bytes, retained) = read_retained(&package_dir, LOCK_NAME, MAX_MANIFEST_BYTES)?;
            provider.push_retained(retained)?;
            Some(bytes)
        }
        PackageLockMode::Update => None,
    };
    let mode = existing_lock.as_deref().map_or(LockMode::Update, LockMode::Frozen);
    let graph = zryna_package::resolve(&mut provider, root_source, mode)?;
    let root_files = provider
        .loaded_files
        .get(&provider.root_source)
        .cloned()
        .ok_or_else(|| ResolveError::source("root package source inventory is unavailable"))?;
    provider.revalidate()?;
    let sources = authenticated_sources(&graph, &provider.loaded_files)?;
    let current_package_dir = provider.open_source_dir(&PackageSource {
        kind: PackageSourceKind::Local,
        locator: request.package.clone(),
        revision: String::new(),
    })?;
    ensure_same_directory(&package_dir, &current_package_dir)?;
    let published = request.mode == PackageLockMode::Update;
    if published {
        publish_lock(&package_dir, LOCK_NAME, graph.lock_bytes())?;
    }
    Ok((
        PackageResolutionSuccess {
            graph,
            sources,
            lock_path: request.source_root.join(&request.package).join(LOCK_NAME),
            published,
        },
        CapturedProject::new(provider, root_files),
    ))
}

struct FilesystemProvider {
    source_root: CapturedRoot,
    git_cache: Option<CapturedRoot>,
    root_source: PackageSource,
    local_scope: Option<String>,
    loaded_files: BTreeMap<PackageSource, Vec<PackageFile>>,
    retained_files: Vec<RetainedFile>,
    retained_packages: Vec<RetainedPackage>,
}

impl PackageSourceProvider for FilesystemProvider {
    fn load(&mut self, source: &PackageSource) -> Result<PackageMaterial, ResolveError> {
        let directory = self.open_source_dir(source)?;
        let mut retained_package = RetainedPackage::new(source.clone(), &directory)?;
        let (manifest, retained) = read_retained(&directory, MANIFEST_NAME, MAX_MANIFEST_BYTES)?;
        self.push_retained(retained)?;
        let mut files = Vec::new();
        let mut entries_seen = 0;
        self.collect_files(
            &directory,
            source == &self.root_source,
            &mut entries_seen,
            &mut files,
            &mut retained_package,
        )?;
        files.sort_by(|left, right| left.path.cmp(&right.path));
        self.loaded_files.insert(source.clone(), files.clone());
        self.retained_packages.push(retained_package);
        Ok(PackageMaterial { manifest, files })
    }
}

impl FilesystemProvider {
    fn open_source_dir(&self, source: &PackageSource) -> Result<Dir, ResolveError> {
        match source.kind {
            PackageSourceKind::Local => {
                if let Some(scope) = &self.local_scope {
                    let prefix = format!("{scope}/");
                    if source.locator != *scope && !source.locator.starts_with(&prefix) {
                        return Err(ResolveError::source(
                            "local package dependency escapes the explicit project tree",
                        ));
                    }
                }
                self.source_root.open_relative(&source.locator)
            }
            PackageSourceKind::Git => {
                let cache = self.git_cache.as_ref().ok_or_else(|| {
                    ResolveError::source("exact Git source is absent from the configured cache")
                })?;
                cache.open_relative(&git_cache_key(source))
            }
        }
    }

    fn collect_files(
        &mut self,
        root: &Dir,
        is_root_package: bool,
        entries_seen: &mut usize,
        files: &mut Vec<PackageFile>,
        retained_package: &mut RetainedPackage,
    ) -> Result<(), ResolveError> {
        let root = root
            .try_clone()
            .map_err(|_| ResolveError::source("package directory capability cannot be retained"))?;
        let mut pending = vec![(root, String::new())];
        while let Some((directory, prefix)) = pending.pop() {
            let entries = enumerate_directory(&directory, entries_seen)?;
            retained_package.retain_inventory(
                &directory,
                &prefix,
                &entries,
                is_root_package && prefix.is_empty(),
            )?;
            let mut children = Vec::new();
            for entry in entries {
                let name = entry.name;
                if prefix.is_empty() && matches!(name.as_str(), MANIFEST_NAME | LOCK_NAME) {
                    continue;
                }
                if is_root_package && prefix.is_empty() && name == ".zryna" {
                    if entry.kind != PackageEntryKind::Directory {
                        return Err(ResolveError::source(
                            "reserved project state path is not a real directory",
                        ));
                    }
                    continue;
                }
                let path =
                    if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
                if entry.kind == PackageEntryKind::Directory {
                    let child = directory.open_dir_nofollow(&name).map_err(|_| {
                        ResolveError::source("package directory cannot be retained")
                    })?;
                    children.push((child, path));
                } else {
                    if files.len() == MAX_SOURCE_FILES {
                        return Err(ResolveError::source("package source file count exceeds 16"));
                    }
                    let (bytes, retained) = read_retained(&directory, &name, MAX_SOURCE_BYTES)?;
                    self.push_retained(retained)?;
                    files.push(PackageFile { path, bytes });
                }
            }
            pending.extend(children.into_iter().rev());
        }
        Ok(())
    }

    fn revalidate(&mut self) -> Result<(), ResolveError> {
        self.source_root.revalidate()?;
        if let Some(cache) = &self.git_cache {
            cache.revalidate()?;
        }
        for package in &self.retained_packages {
            let reopened = self.open_source_dir(&package.source)?;
            package.revalidate(&reopened)?;
        }
        for retained in &mut self.retained_files {
            retained.revalidate()?;
        }
        Ok(())
    }

    fn push_retained(&mut self, retained: RetainedFile) -> Result<(), ResolveError> {
        if self.retained_files.iter().any(|existing| existing.handle == retained.handle) {
            return Err(ResolveError::source("package inputs contain a hard-link alias"));
        }
        self.retained_files.push(retained);
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackageEntry {
    identity: String,
    name: String,
    kind: PackageEntryKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PackageEntryKind {
    Directory,
    File,
}

fn enumerate_directory(
    directory: &Dir,
    entries_seen: &mut usize,
) -> Result<Vec<PackageEntry>, ResolveError> {
    let entries = directory
        .entries()
        .map_err(|_| ResolveError::source("package directory cannot be enumerated"))?;
    let mut identities = BTreeSet::new();
    let mut result = Vec::new();
    for entry in entries {
        if *entries_seen >= MAX_SOURCE_ENTRIES {
            return Err(ResolveError::source("package directory entry count exceeds 256"));
        }
        *entries_seen += 1;
        let entry =
            entry.map_err(|_| ResolveError::source("package directory entry cannot be read"))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| ResolveError::source("package entry name is not portable UTF-8"))?;
        let identity = name.to_ascii_lowercase();
        if !identities.insert(identity.clone()) {
            return Err(ResolveError::source("package entries collide under ASCII case folding"));
        }
        let metadata = directory
            .symlink_metadata(&name)
            .map_err(|_| ResolveError::source("package entry cannot be inspected"))?;
        if metadata.is_symlink() {
            return Err(ResolveError::source("package source contains a link or reparse point"));
        }
        let kind = if metadata.is_dir() {
            PackageEntryKind::Directory
        } else if metadata.is_file() {
            PackageEntryKind::File
        } else {
            return Err(ResolveError::source("package source contains a special file"));
        };
        result.push(PackageEntry { identity, name, kind });
    }
    result.sort_by(|left, right| {
        left.identity.cmp(&right.identity).then_with(|| left.name.cmp(&right.name))
    });
    Ok(result)
}

struct RetainedPackage {
    source: PackageSource,
    root: Dir,
    inventories: Vec<RetainedInventory>,
}

impl RetainedPackage {
    fn new(source: PackageSource, root: &Dir) -> Result<Self, ResolveError> {
        let root = root
            .try_clone()
            .map_err(|_| ResolveError::source("package directory capability cannot be retained"))?;
        Ok(Self { source, root, inventories: Vec::new() })
    }

    fn retain_inventory(
        &mut self,
        directory: &Dir,
        path: &str,
        entries: &[PackageEntry],
        project_state: bool,
    ) -> Result<(), ResolveError> {
        self.inventories.push(RetainedInventory {
            directory: directory.try_clone().map_err(|_| {
                ResolveError::source("package directory capability cannot be retained")
            })?,
            path: path.to_owned(),
            entries: source_inventory(entries.to_vec(), project_state)?,
            project_state,
        });
        Ok(())
    }

    fn revalidate(&self, reopened: &Dir) -> Result<(), ResolveError> {
        ensure_same_directory(&self.root, reopened)?;
        let mut entries_seen = 0;
        for inventory in &self.inventories {
            let current = open_descendant(reopened, &inventory.path)?;
            ensure_same_directory(&inventory.directory, &current)?;
            let entries = enumerate_directory(&current, &mut entries_seen)?;
            if source_inventory(entries, inventory.project_state)? != inventory.entries {
                return Err(ResolveError::source("package directory changed during resolution"));
            }
        }
        Ok(())
    }
}

fn open_descendant(root: &Dir, path: &str) -> Result<Dir, ResolveError> {
    let mut directory = root
        .try_clone()
        .map_err(|_| ResolveError::source("package directory cannot be revalidated"))?;
    if path.is_empty() {
        return Ok(directory);
    }
    for component in path.split('/') {
        directory = directory
            .open_dir_nofollow(component)
            .map_err(|_| ResolveError::source("package directory changed during resolution"))?;
    }
    Ok(directory)
}

struct RetainedInventory {
    directory: Dir,
    path: String,
    entries: Vec<PackageEntry>,
    project_state: bool,
}

fn source_inventory(
    mut entries: Vec<PackageEntry>,
    project_state: bool,
) -> Result<Vec<PackageEntry>, ResolveError> {
    if project_state {
        if entries
            .iter()
            .any(|entry| entry.name == ".zryna" && entry.kind != PackageEntryKind::Directory)
        {
            return Err(ResolveError::source("reserved project state is not a real directory"));
        }
        entries.retain(|entry| entry.name != ".zryna");
    }
    Ok(entries)
}

fn git_cache_key(source: &PackageSource) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ZRYNA-PACKAGE-GIT-CACHE-V1\0");
    hasher.update(source.locator.as_bytes());
    hasher.update(b"\0");
    hasher.update(source.revision.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
#[path = "package_resolution_tests.rs"]
mod tests;
