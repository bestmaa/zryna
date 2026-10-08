//! Private stage for the exact authenticated source-ref-selected provider files.

use std::{
    collections::BTreeMap,
    io::Write as _,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use cap_fs_ext::{DirExt as _, FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::fs::Dir;
use zryna_diagnostics::Diagnostic;

use super::{
    InstalledCompiler, admission_error,
    filesystem::{InstallationTree, capture_absolute_directory},
};

#[cfg(test)]
mod tests;

static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

pub(super) struct ProviderStage {
    path: PathBuf,
    name: String,
    parent: Dir,
    directories: BTreeMap<String, Dir>,
    files: Vec<(String, String)>,
    tree: Option<InstallationTree>,
}

impl ProviderStage {
    pub(super) fn create(compiler: &InstalledCompiler) -> Result<Self, Diagnostic> {
        compiler.revalidate()?;
        let temporary = std::env::temp_dir();
        let (_anchors, parent) =
            capture_absolute_directory(&temporary).map_err(|_| stage_error())?;
        let mut selected = None;
        for _ in 0..64 {
            let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let name = format!("zryna-provider-{}-{sequence}", std::process::id());
            let mut builder = cap_std::fs::DirBuilder::new();
            configure_directory(&mut builder);
            match parent.create_dir_with(&name, &builder) {
                Ok(()) => {
                    selected = Some(name);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(stage_error()),
            }
        }
        let name = selected.ok_or_else(stage_error)?;
        let root = parent.open_dir_nofollow(&name).map_err(|_| stage_error())?;
        let mut stage = Self {
            path: temporary.join(&name),
            name,
            parent,
            directories: BTreeMap::from([(String::new(), root)]),
            files: Vec::new(),
            tree: None,
        };
        for &path in compiler.distribution.provider_paths()? {
            let relative = path.strip_prefix("lib/zryna/bootstrap/").ok_or_else(stage_error)?;
            let data = compiler.tree.bytes(path)?;
            stage.write(relative, data)?;
        }
        let mut tree = InstallationTree::capture(&stage.path)?;
        for &path in compiler.distribution.provider_paths()? {
            let relative = path.strip_prefix("lib/zryna/bootstrap/").ok_or_else(stage_error)?;
            let record = compiler
                .distribution
                .files
                .iter()
                .find(|record| record.path == path)
                .ok_or_else(stage_error)?;
            tree.capture_file(relative, record.size, false)?;
            let mut expected = record.clone();
            relative.clone_into(&mut expected.path);
            expected.mode = 0o600;
            tree.matches(&expected)?;
        }
        stage.tree = Some(tree);
        stage.revalidate()?;
        compiler.revalidate()?;
        Ok(stage)
    }

    pub(super) fn working_directory(&self) -> Result<PathBuf, Diagnostic> {
        self.revalidate()?;
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd as _;
            let root = self.directories.get("").ok_or_else(stage_error)?;
            Ok(PathBuf::from(format!("/proc/{}/fd/{}", std::process::id(), root.as_raw_fd())))
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(self.path.clone())
        }
    }

    pub(super) fn worker(&self, name: &str) -> Result<PathBuf, Diagnostic> {
        if !["worker.mjs", "worker-v3.mjs", "worker-v4.mjs"].contains(&name) {
            return Err(stage_error());
        }
        Ok(self.working_directory()?.join(name))
    }

    pub(super) fn revalidate(&self) -> Result<(), Diagnostic> {
        self.tree.as_ref().ok_or_else(stage_error)?.revalidate()
    }

    fn write(&mut self, relative: &str, data: &[u8]) -> Result<(), Diagnostic> {
        let path = Path::new(relative);
        let mut parent = String::new();
        let parts = relative.split('/').collect::<Vec<_>>();
        for part in &parts[..parts.len() - 1] {
            let key =
                if parent.is_empty() { (*part).to_owned() } else { format!("{parent}/{part}") };
            if !self.directories.contains_key(&key) {
                let directory = self.directories.get(&parent).ok_or_else(stage_error)?;
                directory.create_dir(part).map_err(|_| stage_error())?;
                let child = directory.open_dir_nofollow(part).map_err(|_| stage_error())?;
                self.directories.insert(key.clone(), child);
            }
            parent = key;
        }
        let name = path.file_name().and_then(|name| name.to_str()).ok_or_else(stage_error)?;
        let directory = self.directories.get(&parent).ok_or_else(stage_error)?;
        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true).follow(FollowSymlinks::No);
        configure_file(&mut options);
        let mut file = directory.open_with(name, &options).map_err(|_| stage_error())?;
        self.files.push((parent, name.to_owned()));
        file.write_all(data)
            .and_then(|()| file.flush())
            .and_then(|()| file.sync_all())
            .map_err(|_| stage_error())
    }
}

impl Drop for ProviderStage {
    fn drop(&mut self) {
        // An incomplete or changed stage is retained instead of deleting unverified entries.
        // Construction/revalidation has already reported failure to its caller in that case.
        if self.tree.as_ref().is_none_or(|tree| tree.revalidate().is_err()) {
            return;
        }
        // Every removal is one known name below a retained directory, never recursive traversal.
        self.tree.take();
        for (parent, name) in self.files.iter().rev() {
            if let Some(directory) = self.directories.get(parent) {
                let _ = directory.remove_file(name);
            }
        }
        let mut keys =
            self.directories.keys().filter(|key| !key.is_empty()).cloned().collect::<Vec<_>>();
        keys.sort_by_key(|key| std::cmp::Reverse(key.len()));
        for key in keys {
            self.directories.remove(&key);
            let (parent, name) = key.rsplit_once('/').unwrap_or(("", &key));
            if let Some(directory) = self.directories.get(parent) {
                let _ = directory.remove_dir(name);
            }
        }
        self.directories.clear();
        let _ = self.parent.remove_dir(&self.name);
    }
}

#[cfg(unix)]
fn configure_directory(builder: &mut cap_std::fs::DirBuilder) {
    use cap_std::fs::DirBuilderExt as _;
    builder.mode(0o700);
}

#[cfg(windows)]
fn configure_directory(_builder: &mut cap_std::fs::DirBuilder) {}

#[cfg(unix)]
fn configure_file(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt as _;
    options.mode(0o600);
}

#[cfg(windows)]
fn configure_file(_options: &mut cap_std::fs::OpenOptions) {}

fn stage_error() -> Diagnostic {
    admission_error("private provider stage could not be captured or revalidated")
}
