mod batch;

use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::{Component, Path},
};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{ambient_authority, fs::Dir};
use same_file::Handle;
use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;
use zryna_source::{MAX_SOURCE_FILE_BYTES, NormalizedSourcePath};

pub(super) const MAX_DIRECTORY_ENTRIES: usize = 65_536;

#[derive(Clone)]
pub(super) struct StableSource {
    pub(super) text: String,
    pub(super) sha256: [u8; 32],
}

struct RetainedDirectory {
    dir: Dir,
    parent: Option<String>,
    name: Option<String>,
    identity: Handle,
    metadata: fs::Metadata,
    entries: DirectoryIndex,
}

struct RetainedSource {
    parent: String,
    name: String,
    identity: Handle,
    metadata: fs::Metadata,
    stable: StableSource,
}

#[derive(Clone)]
enum EntrySpelling {
    Unique(String),
    Collision,
}

#[derive(Clone)]
struct DirectoryIndex {
    entries: BTreeMap<String, EntrySpelling>,
}

/// Retained authority for source reads below one validated workspace root.
pub struct WorkspaceSourceRoot {
    root: Dir,
    parent: Option<Dir>,
    name: Option<OsString>,
    _anchors: Vec<Dir>,
}

impl WorkspaceSourceRoot {
    /// Captures one absolute real directory by walking from an ambient filesystem anchor.
    ///
    /// # Errors
    ///
    /// Returns a stable diagnostic when the platform root form is unsupported or any named
    /// component is linked, reparsed, replaced during capture, or not a real directory.
    pub fn capture(path: &Path) -> Result<Self, Diagnostic> {
        if !path.is_absolute() {
            return Err(root_error(
                "workspace source root must be absolute",
                "validate and resolve one real workspace root before source discovery",
            ));
        }
        let (anchors, root) = capture_absolute_root(path).map_err(|_| {
            root_error(
                "workspace source root could not be captured component by component",
                "use a supported local absolute path made only of real directories",
            )
        })?;
        let name = path
            .components()
            .filter_map(|component| match component {
                Component::Normal(name) => Some(name.to_os_string()),
                _ => None,
            })
            .next_back();
        let parent = if name.is_some() {
            anchors
                .get(anchors.len().checked_sub(2).ok_or_else(unsafe_root)?)
                .ok_or_else(unsafe_root)?
                .try_clone()
                .map(Some)
                .map_err(|_| unsafe_root())?
        } else {
            None
        };
        Ok(Self { root, parent, name, _anchors: anchors })
    }

    pub(super) fn begin_discovery(&self) -> Result<WorkspaceSourceSession<'_>, Diagnostic> {
        WorkspaceSourceSession::new(self)
    }
}

pub(super) struct WorkspaceSourceSession<'root> {
    root: &'root WorkspaceSourceRoot,
    directories: BTreeMap<String, RetainedDirectory>,
    sources: BTreeMap<NormalizedSourcePath, RetainedSource>,
}

impl<'root> WorkspaceSourceSession<'root> {
    fn new(root: &'root WorkspaceSourceRoot) -> Result<Self, Diagnostic> {
        let dir = root.root.try_clone().map_err(|_| unsafe_root())?;
        let (identity, metadata) =
            directory_identity_and_metadata(&dir).map_err(|_| unsafe_root())?;
        if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
            return Err(unsafe_root());
        }
        let entries = scan_entries(&dir).map_err(root_child_error)?;
        let directories = BTreeMap::from([(
            String::new(),
            RetainedDirectory { dir, parent: None, name: None, identity, metadata, entries },
        )]);
        Ok(Self { root, directories, sources: BTreeMap::new() })
    }

    pub(super) fn read_source(
        &mut self,
        path: &NormalizedSourcePath,
    ) -> Result<StableSource, Diagnostic> {
        if let Some(source) = self.sources.get(path) {
            return Ok(source.stable.clone());
        }
        self.read_source_impl(path, || {})
    }

    #[cfg(all(test, unix))]
    pub(super) fn read_source_with_after_read(
        &mut self,
        path: &NormalizedSourcePath,
        after_read: impl FnOnce(),
    ) -> Result<StableSource, Diagnostic> {
        self.read_source_impl(path, after_read)
    }

    fn read_source_impl(
        &mut self,
        path: &NormalizedSourcePath,
        after_read: impl FnOnce(),
    ) -> Result<StableSource, Diagnostic> {
        let mut components = path.as_str().split('/').peekable();
        let mut parent_key = String::new();
        let mut final_name = None;
        while let Some(component) = components.next() {
            if components.peek().is_none() {
                final_name = Some(component.to_owned());
                break;
            }
            let child_key = if parent_key.is_empty() {
                component.to_owned()
            } else {
                format!("{parent_key}/{component}")
            };
            if !self.directories.contains_key(&child_key) {
                let parent = self.directories.get(&parent_key).ok_or_else(|| unsafe_path(path))?;
                parent.entries.require_exact(component).map_err(|kind| child_error(path, kind))?;
                let child =
                    parent.dir.open_dir_nofollow(component).map_err(|_| unsafe_path(path))?;
                let (identity, metadata) =
                    directory_identity_and_metadata(&child).map_err(|_| unsafe_path(path))?;
                if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
                    return Err(unsafe_path(path));
                }
                let entries = scan_entries(&child).map_err(|kind| child_error(path, kind))?;
                self.directories.insert(
                    child_key.clone(),
                    RetainedDirectory {
                        dir: child,
                        parent: Some(parent_key.clone()),
                        name: Some(component.to_owned()),
                        identity,
                        metadata,
                        entries,
                    },
                );
            }
            parent_key = child_key;
        }
        let final_name = final_name.ok_or_else(|| unsafe_path(path))?;
        let parent = self.directories.get(&parent_key).ok_or_else(|| unsafe_path(path))?;
        parent.entries.require_exact(&final_name).map_err(|kind| child_error(path, kind))?;
        let parent_index = parent.entries.clone();
        let mut opened =
            open_regular_nofollow(&parent.dir, &final_name).map_err(|_| unreadable(path))?;
        let metadata = opened.as_file().metadata().map_err(|_| unreadable(path))?;
        if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
            return Err(unreadable(path));
        }
        if self.sources.values().any(|source| source.identity == opened) {
            return Err(unsafe_path(path));
        }
        let bytes = bounded_read(&mut opened).map_err(|kind| read_error(path, kind))?;
        after_read();
        let stable = StableSource {
            sha256: Sha256::digest(&bytes).into(),
            text: String::from_utf8(bytes).map_err(|_| invalid_utf8(path))?,
        };
        self.sources.insert(
            path.clone(),
            RetainedSource {
                parent: parent_key,
                name: final_name,
                identity: opened,
                metadata,
                stable: stable.clone(),
            },
        );
        self.revalidate_source_with_index(path, &parent_index)?;
        Ok(stable)
    }

    fn revalidate_root(&self) -> Result<(), Diagnostic> {
        let expected = self.directories.get("").ok_or_else(unsafe_root)?;
        let held_metadata = expected.identity.as_file().metadata().map_err(|_| changed_root())?;
        if !same_file_state(&expected.metadata, &held_metadata)
            || !held_metadata.is_dir()
            || metadata_is_link_or_reparse(&held_metadata)
        {
            return Err(changed_root());
        }
        if let (Some(parent), Some(name)) = (&self.root.parent, &self.root.name) {
            let current = parent.open_dir_nofollow(name).map_err(|_| changed_root())?;
            let (identity, metadata) =
                directory_identity_and_metadata(&current).map_err(|_| changed_root())?;
            if identity != expected.identity
                || !same_file_state(&metadata, &expected.metadata)
                || !metadata.is_dir()
                || metadata_is_link_or_reparse(&metadata)
            {
                return Err(changed_root());
            }
        }
        Ok(())
    }

    fn revalidate_source_with_indexes(
        &mut self,
        path: &NormalizedSourcePath,
        indexes: &BTreeMap<String, DirectoryIndex>,
    ) -> Result<(), Diagnostic> {
        let source = self.sources.get(path).ok_or_else(|| changed(path))?;
        let index = indexes.get(&source.parent).ok_or_else(|| changed(path))?;
        self.revalidate_source_with_index(path, index)
    }

    fn revalidate_source_with_index(
        &mut self,
        path: &NormalizedSourcePath,
        index: &DirectoryIndex,
    ) -> Result<(), Diagnostic> {
        let source = self.sources.get_mut(path).ok_or_else(|| changed(path))?;
        index.require_exact(&source.name).map_err(|_| changed(path))?;
        let parent = self.directories.get(&source.parent).ok_or_else(|| changed(path))?;
        crate::source_identity::validate(
            &parent.dir,
            &source.name,
            &source.identity,
            &source.metadata,
        )
        .map_err(|_| changed(path))?;
        let digest: [u8; 32] = Sha256::digest(source.stable.text.as_bytes()).into();
        if digest != source.stable.sha256 {
            return Err(changed(path));
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum ChildLookupError {
    Missing,
    CaseMismatch,
    Collision,
    EntryLimit,
    Io,
}

impl DirectoryIndex {
    fn require_exact(&self, requested: &str) -> Result<(), ChildLookupError> {
        match self.entries.get(&requested.to_ascii_lowercase()) {
            Some(EntrySpelling::Unique(actual)) if actual == requested => Ok(()),
            Some(EntrySpelling::Unique(_)) => Err(ChildLookupError::CaseMismatch),
            Some(EntrySpelling::Collision) => Err(ChildLookupError::Collision),
            None => Err(ChildLookupError::Missing),
        }
    }
}

fn scan_entries(directory: &Dir) -> Result<DirectoryIndex, ChildLookupError> {
    let entries = directory.entries().map_err(|_| ChildLookupError::Io)?;
    index_entry_names(
        entries.map(|entry| entry.map(|entry| entry.file_name()).map_err(|_| ChildLookupError::Io)),
    )
}

fn index_entry_names(
    names: impl IntoIterator<Item = Result<OsString, ChildLookupError>>,
) -> Result<DirectoryIndex, ChildLookupError> {
    let mut index = BTreeMap::new();
    let mut count = 0_usize;
    for name in names {
        count = count.checked_add(1).ok_or(ChildLookupError::EntryLimit)?;
        if count > MAX_DIRECTORY_ENTRIES {
            return Err(ChildLookupError::EntryLimit);
        }
        let name = name?;
        let Some(name) = name.to_str().filter(|name| name.is_ascii()) else {
            continue;
        };
        let folded = name.to_ascii_lowercase();
        index
            .entry(folded)
            .and_modify(|spelling| *spelling = EntrySpelling::Collision)
            .or_insert_with(|| EntrySpelling::Unique(name.to_owned()));
    }
    Ok(DirectoryIndex { entries: index })
}

#[cfg(test)]
mod bounded_index_tests {
    use std::ffi::OsString;

    use super::{ChildLookupError, MAX_DIRECTORY_ENTRIES, index_entry_names};

    fn names(count: usize) -> impl Iterator<Item = Result<OsString, ChildLookupError>> {
        (0..count).map(|index| Ok(OsString::from(format!("entry-{index:05}"))))
    }

    #[test]
    fn directory_index_accepts_exact_limit_and_rejects_first_extra_entry() {
        let Ok(exact) = index_entry_names(names(MAX_DIRECTORY_ENTRIES)) else {
            panic!("exact directory-entry limit must succeed");
        };
        assert_eq!(exact.entries.len(), MAX_DIRECTORY_ENTRIES);
        assert!(matches!(
            index_entry_names(names(MAX_DIRECTORY_ENTRIES + 1)),
            Err(ChildLookupError::EntryLimit)
        ));
    }
}

fn capture_child(parent: &Dir, name: &OsStr) -> io::Result<Dir> {
    let child = parent.open_dir_nofollow(name)?;
    let metadata = child.try_clone().map(Dir::into_std_file)?.metadata()?;
    if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
        return Err(invalid_root());
    }
    let reopened = parent.open_dir_nofollow(name)?;
    if directory_identity(&child)? != directory_identity(&reopened)? {
        return Err(io::Error::other("root component changed"));
    }
    Ok(child)
}

#[cfg(unix)]
fn capture_absolute_root(path: &Path) -> io::Result<(Vec<Dir>, Dir)> {
    let anchor = Dir::open_ambient_dir(Path::new("/"), ambient_authority())?;
    let mut anchors = vec![anchor];
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => {
                let child = capture_child(anchors.last().ok_or_else(invalid_root)?, name)?;
                anchors.push(child);
            }
            Component::ParentDir | Component::Prefix(_) => return Err(invalid_root()),
        }
    }
    let root = anchors.last().ok_or_else(invalid_root)?.try_clone()?;
    Ok((anchors, root))
}

#[cfg(windows)]
fn capture_absolute_root(path: &Path) -> io::Result<(Vec<Dir>, Dir)> {
    use std::path::{PathBuf, Prefix};

    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            _ => return Err(invalid_root()),
        },
        _ => return Err(invalid_root()),
    };
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(invalid_root());
    }
    let anchor_path = PathBuf::from(format!("{}:\\", char::from(drive)));
    let anchor = Dir::open_ambient_dir(&anchor_path, ambient_authority())?;
    let anchor_metadata = anchor.try_clone().map(Dir::into_std_file)?.metadata()?;
    if !anchor_metadata.is_dir() || metadata_is_link_or_reparse(&anchor_metadata) {
        return Err(invalid_root());
    }
    let mut anchors = vec![anchor];
    for component in components {
        match component {
            Component::Normal(name) => {
                let child = capture_child(anchors.last().ok_or_else(invalid_root)?, name)?;
                anchors.push(child);
            }
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(invalid_root());
            }
        }
    }
    let root = anchors.last().ok_or_else(invalid_root)?.try_clone()?;
    Ok((anchors, root))
}

#[cfg(not(any(unix, windows)))]
fn capture_absolute_root(_path: &Path) -> io::Result<(Vec<Dir>, Dir)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no retained root acquisition exists for this platform",
    ))
}

fn invalid_root() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "unsupported workspace root")
}

fn directory_identity(directory: &Dir) -> io::Result<Handle> {
    directory.try_clone().map(Dir::into_std_file).and_then(Handle::from_file)
}

fn directory_identity_and_metadata(directory: &Dir) -> io::Result<(Handle, fs::Metadata)> {
    let identity = directory_identity(directory)?;
    let metadata = identity.as_file().metadata()?;
    Ok((identity, metadata))
}

fn open_regular_nofollow(directory: &Dir, name: &str) -> io::Result<Handle> {
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true);
    options.follow(FollowSymlinks::No);
    configure_final_open(&mut options);
    directory
        .open_with(OsStr::new(name), &options)
        .map(cap_std::fs::File::into_std)
        .and_then(Handle::from_file)
}

#[cfg(unix)]
fn configure_final_open(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.custom_flags(libc::O_NONBLOCK);
}

#[cfg(windows)]
fn configure_final_open(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    options.share_mode(FILE_SHARE_READ);
}

#[cfg(not(any(unix, windows)))]
fn configure_final_open(_options: &mut cap_std::fs::OpenOptions) {}

#[derive(Clone, Copy)]
enum ReadError {
    Io,
    Limit,
}

fn bounded_read(handle: &mut Handle) -> Result<Vec<u8>, ReadError> {
    let metadata = handle.as_file().metadata().map_err(|_| ReadError::Io)?;
    if metadata.len() > u64::try_from(MAX_SOURCE_FILE_BYTES).unwrap_or(u64::MAX) {
        return Err(ReadError::Limit);
    }
    handle.as_file_mut().seek(SeekFrom::Start(0)).map_err(|_| ReadError::Io)?;
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len()).unwrap_or(MAX_SOURCE_FILE_BYTES).min(MAX_SOURCE_FILE_BYTES),
    );
    handle
        .as_file_mut()
        .take(u64::try_from(MAX_SOURCE_FILE_BYTES).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| ReadError::Io)?;
    if bytes.len() > MAX_SOURCE_FILE_BYTES { Err(ReadError::Limit) } else { Ok(bytes) }
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

fn child_error(path: &NormalizedSourcePath, kind: ChildLookupError) -> Diagnostic {
    match kind {
        ChildLookupError::CaseMismatch | ChildLookupError::Collision => source_diagnostic(
            "ZRYNA-D3005",
            Some(path),
            "source path is not unique with exact portable ASCII casing",
            "use one exact spelling that remains unique when ASCII case is ignored",
        ),
        ChildLookupError::EntryLimit => source_diagnostic(
            "ZRYNA-D3201",
            Some(path),
            "source directory exceeds the bounded entry-enumeration budget",
            "reduce the containing directory before module discovery",
        ),
        ChildLookupError::Missing | ChildLookupError::Io => unreadable(path),
    }
}

fn root_child_error(kind: ChildLookupError) -> Diagnostic {
    match kind {
        ChildLookupError::EntryLimit => source_diagnostic(
            "ZRYNA-D3201",
            None,
            "workspace source root exceeds the bounded entry-enumeration budget",
            "reduce the workspace root directory before module discovery",
        ),
        _ => unsafe_root(),
    }
}

fn unsafe_root() -> Diagnostic {
    root_error(
        "workspace source root capability could not be revalidated",
        "keep the retained real workspace root stable and retry",
    )
}

fn root_error(message: &'static str, guidance: &'static str) -> Diagnostic {
    source_diagnostic("ZRYNA-D3002", None, message, guidance)
}

fn unsafe_path(path: &NormalizedSourcePath) -> Diagnostic {
    source_diagnostic(
        "ZRYNA-D3002",
        Some(path),
        "source path could not be traversed through retained no-follow directory capabilities",
        "use only real directories without symbolic links, junctions, or reparse points",
    )
}

fn unreadable(path: &NormalizedSourcePath) -> Diagnostic {
    source_diagnostic(
        "ZRYNA-D3003",
        Some(path),
        "source file is missing, unreadable, or not a real regular file",
        "provide the exact named .zry file as a stable real regular file",
    )
}

fn invalid_utf8(path: &NormalizedSourcePath) -> Diagnostic {
    source_diagnostic(
        "ZRYNA-D3003",
        Some(path),
        "source file is not valid UTF-8",
        "encode every .zry source as valid UTF-8",
    )
}

fn changed(path: &NormalizedSourcePath) -> Diagnostic {
    source_diagnostic(
        "ZRYNA-D3004",
        Some(path),
        "source path identity, state, or content changed during module discovery",
        "stop concurrent workspace mutation and retry",
    )
}

fn changed_path(path: &str) -> Diagnostic {
    source_diagnostic(
        "ZRYNA-D3004",
        None,
        format!("{path}: source directory identity or state changed during module discovery"),
        "stop concurrent workspace mutation and retry",
    )
}

fn changed_root() -> Diagnostic {
    source_diagnostic(
        "ZRYNA-D3004",
        None,
        "workspace source root identity or state changed during module discovery",
        "retry from one unchanged real workspace source root",
    )
}

fn read_error(path: &NormalizedSourcePath, kind: ReadError) -> Diagnostic {
    match kind {
        ReadError::Io => unreadable(path),
        ReadError::Limit => source_diagnostic(
            "ZRYNA-D3201",
            Some(path),
            "source file exceeds the fixed per-file byte budget",
            "split or reduce the source file before module discovery",
        ),
    }
}

fn source_diagnostic(
    code: &'static str,
    path: Option<&NormalizedSourcePath>,
    message: impl Into<String>,
    guidance: &'static str,
) -> Diagnostic {
    let message = message.into();
    let message = path.map_or(message.clone(), |path| format!("{}: {message}", path.as_str()));
    Diagnostic::error(code, None, message, guidance)
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use nix::{sys::stat::Mode, unistd::mkfifo};
    use zryna_source::{MAX_SOURCE_FILE_BYTES, NormalizedSourcePath};

    use super::WorkspaceSourceRoot;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct TemporaryRoot {
        path: PathBuf,
    }

    impl TemporaryRoot {
        fn new(label: &str) -> Self {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("zryna-workspace-source-{}-{label}-{sequence}", std::process::id()));
            fs::create_dir(&path).expect("unique source root must be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn source_path(value: &str) -> NormalizedSourcePath {
            NormalizedSourcePath::new(value).expect("test source path must be normalized")
        }
    }

    impl Drop for TemporaryRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn capture_rejects_a_linked_workspace_root() {
        use std::os::unix::fs::symlink;

        let parent = TemporaryRoot::new("linked-root");
        let real = parent.path().join("real");
        fs::create_dir(&real).expect("real root must be created");
        let linked = parent.path().join("linked");
        symlink(&real, &linked).expect("root link must be created");

        let diagnostic =
            WorkspaceSourceRoot::capture(&linked).err().expect("linked root must be rejected");
        assert_eq!(diagnostic.code(), "ZRYNA-D3002");
    }

    #[test]
    fn retained_root_rejects_its_binding_and_top_level_entry_replacement() {
        let top_level = TemporaryRoot::new("top-level-swap");
        let source = top_level.path().join("main.zry");
        let displaced = top_level.path().join("old.zry");
        fs::write(&source, "original\n").expect("source must be written");
        let root = WorkspaceSourceRoot::capture(top_level.path()).expect("root capture");
        let mut session = root.begin_discovery().expect("discovery session");
        fs::rename(&source, &displaced).expect("source binding must move");
        fs::write(&source, "replaced\n").expect("replacement source must be written");
        session
            .read_source(&TemporaryRoot::source_path("main.zry"))
            .expect("replacement remains readable before root-state authentication");
        let diagnostic = session
            .revalidate_all()
            .expect_err("top-level replacement must change the retained root state");
        assert_eq!(diagnostic.code(), "ZRYNA-D3004");

        let parent = TemporaryRoot::new("root-binding-swap");
        let workspace = parent.path().join("workspace");
        let old_workspace = parent.path().join("old-workspace");
        fs::create_dir(&workspace).expect("workspace must be created");
        fs::write(workspace.join("main.zry"), "original\n")
            .expect("workspace source must be written");
        let root = WorkspaceSourceRoot::capture(&workspace).expect("root capture");
        let mut session = root.begin_discovery().expect("discovery session");
        fs::rename(&workspace, &old_workspace).expect("workspace binding must move");
        fs::create_dir(&workspace).expect("replacement workspace must be created");
        let diagnostic = session
            .revalidate_all()
            .expect_err("workspace root replacement must fail retained binding authentication");
        assert_eq!(diagnostic.code(), "ZRYNA-D3004");
    }

    #[test]
    fn retained_chain_detects_parent_and_final_replacement() {
        let parent_swap = TemporaryRoot::new("parent-swap");
        let source_dir = parent_swap.path().join("source");
        fs::create_dir(&source_dir).expect("source directory must be created");
        fs::write(source_dir.join("main.zry"), "original\n").expect("source must be written");
        let root = WorkspaceSourceRoot::capture(parent_swap.path()).expect("root capture");
        let mut session = root.begin_discovery().expect("discovery session");
        let path = TemporaryRoot::source_path("source/main.zry");
        let displaced = parent_swap.path().join("displaced");
        let replacement = parent_swap.path().join("source");
        session
            .read_source_with_after_read(&path, || {
                fs::rename(&source_dir, &displaced).expect("source directory must move");
                fs::create_dir(&replacement).expect("replacement directory must be created");
                fs::write(replacement.join("main.zry"), "replacement\n")
                    .expect("replacement source must be written");
            })
            .expect("retained handle read remains bounded");
        let diagnostic =
            session.revalidate_all().expect_err("replaced parent binding must fail revalidation");
        assert_eq!(diagnostic.code(), "ZRYNA-D3004");

        let final_swap = TemporaryRoot::new("final-swap");
        fs::write(final_swap.path().join("main.zry"), "original\n")
            .expect("source must be written");
        fs::write(final_swap.path().join("replacement.zry"), "replaced\n")
            .expect("replacement must be written");
        let root = WorkspaceSourceRoot::capture(final_swap.path()).expect("root capture");
        let mut session = root.begin_discovery().expect("discovery session");
        let path = TemporaryRoot::source_path("main.zry");
        let source = final_swap.path().join("main.zry");
        let replacement = final_swap.path().join("replacement.zry");
        let diagnostic = session
            .read_source_with_after_read(&path, || {
                fs::rename(&replacement, &source).expect("replacement must be installed");
            })
            .err()
            .expect("replaced final binding must fail post-read revalidation");
        assert_eq!(diagnostic.code(), "ZRYNA-D3004");
    }

    #[test]
    fn fifo_and_per_file_limit_fail_without_unbounded_reads() {
        let fifo_root = TemporaryRoot::new("fifo");
        mkfifo(&fifo_root.path().join("pipe.zry"), Mode::S_IRUSR | Mode::S_IWUSR)
            .expect("test fifo must be created");
        let root = WorkspaceSourceRoot::capture(fifo_root.path()).expect("root capture");
        let mut session = root.begin_discovery().expect("discovery session");
        let diagnostic = session
            .read_source(&TemporaryRoot::source_path("pipe.zry"))
            .err()
            .expect("fifo must be rejected");
        assert_eq!(diagnostic.code(), "ZRYNA-D3003");

        let large_root = TemporaryRoot::new("large");
        fs::write(large_root.path().join("main.zry"), vec![b'a'; MAX_SOURCE_FILE_BYTES + 1])
            .expect("oversized source must be written");
        let root = WorkspaceSourceRoot::capture(large_root.path()).expect("root capture");
        let mut session = root.begin_discovery().expect("discovery session");
        let diagnostic = session
            .read_source(&TemporaryRoot::source_path("main.zry"))
            .err()
            .expect("oversized source must be rejected");
        assert_eq!(diagnostic.code(), "ZRYNA-D3201");
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use zryna_source::NormalizedSourcePath;

    use super::WorkspaceSourceRoot;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct TemporaryRoot {
        path: PathBuf,
    }

    impl TemporaryRoot {
        fn new(label: &str) -> Self {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "zryna-workspace-source-windows-{}-{label}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("unique Windows source root must be created");
            Self { path }
        }

        fn source_path(value: &str) -> NormalizedSourcePath {
            NormalizedSourcePath::new(value).expect("test source path must be normalized")
        }
    }

    impl Drop for TemporaryRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn create_junction(link: &Path, target: &Path) {
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .status()
            .expect("junction command must start");
        assert!(status.success(), "junction fixture must be created");
    }

    #[test]
    fn captures_plain_and_canonical_local_drive_roots_and_rejects_unsupported_forms() {
        let local = TemporaryRoot::new("local-root");
        WorkspaceSourceRoot::capture(&local.path).expect("local drive workspace must be supported");
        let canonical = local.path.canonicalize().expect("local drive root must canonicalize");
        WorkspaceSourceRoot::capture(&canonical)
            .expect("canonical verbatim local drive workspace must be supported");
        for unsupported in [
            Path::new(r"C:relative"),
            Path::new(r"\\localhost\C$\workspace"),
            Path::new(r"\\.\C:\workspace"),
        ] {
            let diagnostic = WorkspaceSourceRoot::capture(unsupported)
                .err()
                .expect("unsupported Windows root form must fail closed");
            assert_eq!(diagnostic.code(), "ZRYNA-D3002");
        }
    }

    #[test]
    fn rejects_windows_junction_roots_and_intermediate_components() {
        let parent = TemporaryRoot::new("junction-root");
        let real_root = parent.path.join("real-root");
        let linked_root = parent.path.join("linked-root");
        fs::create_dir(&real_root).expect("real root must be created");
        create_junction(&linked_root, &real_root);
        let diagnostic = WorkspaceSourceRoot::capture(&linked_root)
            .err()
            .expect("junction workspace root must be rejected");
        assert_eq!(diagnostic.code(), "ZRYNA-D3002");
        fs::remove_dir(&linked_root).expect("root junction must be removed");

        let workspace = TemporaryRoot::new("junction-component");
        let real = workspace.path.join("real");
        let linked = workspace.path.join("linked");
        fs::create_dir(&real).expect("real dependency directory must be created");
        fs::write(real.join("dep.zry"), "value\n").expect("dependency must be written");
        create_junction(&linked, &real);
        let root = WorkspaceSourceRoot::capture(&workspace.path).expect("real root capture");
        let mut session = root.begin_discovery().expect("discovery session");
        let diagnostic = session
            .read_source(&TemporaryRoot::source_path("linked/dep.zry"))
            .err()
            .expect("junction component must be rejected");
        assert_eq!(diagnostic.code(), "ZRYNA-D3002");
        drop(session);
        drop(root);
        fs::remove_dir(&linked).expect("component junction must be removed");
    }

    #[test]
    fn retained_windows_handles_block_source_and_ancestor_replacement() {
        let workspace = TemporaryRoot::new("retained-locks");
        let source_dir = workspace.path.join("source");
        let source = source_dir.join("main.zry");
        fs::create_dir(&source_dir).expect("source directory must be created");
        fs::write(&source, "original\n").expect("source must be written");
        let root = WorkspaceSourceRoot::capture(&workspace.path).expect("root capture");
        let mut session = root.begin_discovery().expect("discovery session");
        session
            .read_source(&TemporaryRoot::source_path("source/main.zry"))
            .expect("source must be retained");

        assert!(fs::write(&source, "modified\n").is_err(), "retained source must deny writes");
        assert!(
            fs::rename(&source, source_dir.join("moved.zry")).is_err(),
            "retained source must deny replacement"
        );
        assert!(
            fs::rename(&source_dir, workspace.path.join("moved-source")).is_err(),
            "retained ancestor must deny replacement"
        );
        assert!(
            fs::rename(&workspace.path, workspace.path.with_extension("moved")).is_err(),
            "retained workspace root must deny replacement"
        );
        session.revalidate_all().expect("blocked replacement attempts preserve the source graph");
    }

    #[test]
    fn windows_source_lookup_requires_exact_ascii_case() {
        let workspace = TemporaryRoot::new("wrong-case");
        fs::write(workspace.path.join("Dep.zry"), "value\n")
            .expect("mixed-case dependency must be written");
        let root = WorkspaceSourceRoot::capture(&workspace.path).expect("root capture");
        let mut session = root.begin_discovery().expect("discovery session");
        let diagnostic = session
            .read_source(&TemporaryRoot::source_path("dep.zry"))
            .err()
            .expect("wrong-case lookup must fail");
        assert_eq!(diagnostic.code(), "ZRYNA-D3005");
    }
}
